//! Reproduction + regression for the mcext (RPC) reported E3071/E3110 on the
//! hbl project while CLI `mcc check src/hbl.mc` is clean.
//!
//! Root cause: the mcc RPC server runs handlers on the tokio blocking pool
//! with no serialization, so mcext Phase 2 `init` (mcb_init clears the mcode
//! system tables, then reloads them) can race with did_open's `load_project`
//! (parses files against those tables). periph.mc then resolves `DC`/`CAP`/
//! `RES` against empty tables (E3071/E3110), and the C lexer/parser's
//! process-global state is corrupted (heap corruption, SIGSEGV). The fix is a
//! process-wide dispatch lock in `RpcMethodRegistry::call` (server.info probe
//! exempt), which restores the engine's single-threaded-by-design guarantee.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Mutex;

static REPRO_LOCK: Mutex<()> = Mutex::new(());

fn mcode_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode")
}

/// The system root the mcc server actually uses when mcext does not pass
/// initializationOptions: `~/.mcode` (data_root()).
fn server_data_root() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME set");
    PathBuf::from(home).join(".mcode")
}

fn hbl_project_dir() -> PathBuf {
    PathBuf::from("/Users/dan/work/mo/mcd/projects/hbl")
}

fn diag_codes() -> Vec<(u32, String)> {
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == 3071 || d.code == 3110 || d.code == 3157)
        .map(|d| (d.code, d.msg.clone()))
        .collect()
}

/// Server/RPC order: mcc_init() loads mcode into the default workspace first,
/// then the project root is set and the project loaded afterwards.
#[test]
fn repro_server_order() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let entry_uri: mcc::McURI = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    mcc::mcc_set_system_root(sys_root.as_path());
    mcc::mcc_init(); // server startup: mcode into default workspace
    mcc::mcc_set_project_root(&project_root);
    // RPC lib.load("mcode") — short-circuits because mcode is already loaded.
    mcc::mcb_load_lib("mcode", &sys_root.join("mcode"));
    mcc::mcc_load_project(&entry_uri);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &entry_uri);

    let codes = diag_codes();
    eprintln!("SERVER-ORDER E3071/E3110: {:?}", codes);
    assert!(
        codes.is_empty(),
        "server order reproduces E3071/E3110: {codes:?}"
    );
}

/// CLI order (netdiff.rs pattern): init without lib, then set project root,
/// clear workspace, load mcode explicitly, then load the project.
#[test]
fn repro_cli_order() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let entry_uri: mcc::McURI = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(sys_root.as_path());
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    mcc::mcb_load_lib("mcode", &sys_root.join("mcode"));
    mcc::mcc_load_project(&entry_uri);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &entry_uri);

    let codes = diag_codes();
    eprintln!("CLI-ORDER E3071/E3110: {:?}", codes);
    assert!(
        codes.is_empty(),
        "cli order reproduces E3071/E3110: {codes:?}"
    );
}

/// Control group: the same init + load_project sequence run SEQUENTIALLY
/// (no threads). If this crashes or produces E3071/E3110, the bug is state
/// accumulation across rounds, not concurrency.
#[test]
fn repro_sequential_accumulation() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let periph_uri: String = project_root
        .join("src/periph.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    for round in 0..5 {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(sys_root.as_path());
        mcc::mcc_set_project_root(&project_root);
        mcc::mcc_init();
        mcc::mcc_load_project(&periph_uri);
        let codes = diag_codes();
        eprintln!("seq round {round}: E3071/E3110={codes:?}");
        assert!(
            codes.is_empty(),
            "sequential round {round} hit E3071/E3110: {codes:?}"
        );
    }
}

/// Control group 2: the same RPC handler calls (init + load_project) run
/// SEQUENTIALLY through the registry, no threads. Isolates whether the crash
/// is the handler path itself or the thread model.
#[test]
fn repro_registry_sequential() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let periph_uri: String = project_root
        .join("src/periph.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    let server = mcc::rpc::handlers::register_all(mcc::rpc::RpcServerBuilder::new()).build();
    let registry = server.registry();

    for round in 0..5 {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(sys_root.as_path());
        mcc::mcc_set_project_root(&project_root);
        let _ = registry.call("init", None);
        let _ = registry.call(
            "load_project",
            Some(serde_json::json!({ "entry": periph_uri })),
        );
        let codes = diag_codes();
        eprintln!("reg-seq round {round}: E3071/E3110={codes:?}");
        assert!(
            codes.is_empty(),
            "reg-seq round {round} hit E3071/E3110: {codes:?}"
        );
    }
}

/// Regression: concurrent `init` + `load_project` RPCs must be serialized by
/// the dispatch lock. Dispatches through the same registry the live server
/// uses, from two PERSISTENT worker threads mirroring the tokio blocking pool
/// (fresh threads per round are not a valid model for the C lexer/parser,
/// which keeps process-global state).
#[test]
fn repro_init_load_project_race() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let periph_uri: String = project_root
        .join("src/periph.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    // Same registry the live RPC server dispatches through.
    let server = mcc::rpc::handlers::register_all(mcc::rpc::RpcServerBuilder::new()).build();
    let registry = server.registry();

    // Two persistent worker threads mirroring the blocking pool: the second
    // call blocks on the dispatch lock until the first handler completes.
    let (tx_a, rx_a) = std::sync::mpsc::channel::<()>();
    let (tx_b, rx_b) = std::sync::mpsc::channel::<()>();
    let (done_a, done_arx) = std::sync::mpsc::channel::<()>();
    let (done_b, done_brx) = std::sync::mpsc::channel::<()>();
    let reg = registry.clone();
    let wa = std::thread::spawn(move || {
        while rx_a.recv().is_ok() {
            let _ = reg.call("init", None);
            let _ = done_a.send(());
        }
    });
    let reg = registry.clone();
    let periph = periph_uri.clone();
    let wb = std::thread::spawn(move || {
        while rx_b.recv().is_ok() {
            let _ = reg.call("load_project", Some(serde_json::json!({ "entry": periph })));
            let _ = done_b.send(());
        }
    });

    let rounds = 30;
    let mut bad_rounds = 0;
    for round in 0..rounds {
        // Reset to a pristine state, matching mcc server startup before init.
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(sys_root.as_path());
        mcc::mcc_set_project_root(&project_root);

        // Fire both RPCs back to back: one acquires the lock first and runs to
        // completion, the other waits and then runs against the clean state.
        tx_a.send(()).unwrap();
        tx_b.send(()).unwrap();
        done_arx.recv().unwrap();
        done_brx.recv().unwrap();

        let codes = diag_codes();
        if !codes.is_empty() {
            bad_rounds += 1;
            eprintln!("round {round}: RACE HIT E3071/E3110: {codes:?}");
        }
    }
    drop(tx_a);
    drop(tx_b);
    wa.join().unwrap();
    wb.join().unwrap();
    eprintln!("race rounds with E3071/E3110: {bad_rounds}/{rounds}");
    assert!(
        bad_rounds == 0,
        "init/load_project race reproduced: {bad_rounds}/{rounds} rounds hit E3071/E3110"
    );
}

/// Mirror the mcext didOpen path: after init + lib.load + load_project, the
/// LSP sends `sem` with editor content for each opened .mc file, then reads
/// diagnostics. This exercises `handle_sem`'s content branch (mcb_add_from_string
/// -> mcb_parse_all_modules -> create_lapper), which is the path that emits
/// E3071/E3157 in the extension but is NOT covered by mcc_build.
#[test]
fn repro_sem_content_path() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let hbl_uri: String = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    // mcext init order. The mcc server process runs with cwd = the project
    // root (mccsrv spawns it from there), which can change relative-path
    // resolution in the loader — mirror that.
    let _cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&project_root).expect("chdir project root");
    mcc::mcc_set_system_root(sys_root.as_path());
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);

    let server = mcc::rpc::handlers::register_all(mcc::rpc::RpcServerBuilder::new()).build();
    let registry = server.registry();
    let _ = registry.call("lib.load", Some(serde_json::json!({ "name": "mcode" })));
    let _ = registry.call(
        "load_project",
        Some(serde_json::json!({ "entry": hbl_uri })),
    );

    // didOpen: parse every project file from editor content (any open order),
    // then collect diagnostics per file. E3071/E3157 must stay clean.
    for name in ["hbl.mc", "periph.mc", "power.mc", "us513.mc"] {
        let path = project_root.join("src").join(name);
        let uri: String = path.to_string_lossy().into_owned();
        let content = std::fs::read_to_string(&path).expect("read .mc file");
        // Mirror mcext did_open: load_project(uri) runs for the opened file
        // before/around the sem call (mcext server/mod.rs did_open).
        let _ = registry.call("load_project", Some(serde_json::json!({ "entry": uri })));
        let _ = registry.call(
            "sem",
            Some(serde_json::json!({ "uri": uri, "content": content })),
        );
        let codes: Vec<(u32, String)> = mcc::mcc_diagnose_all()
            .iter()
            .filter(|d| d.loc.uri.as_str().contains(name))
            .filter(|d| d.code == 3071 || d.code == 3110 || d.code == 3157)
            .map(|d| (d.code, d.msg.clone()))
            .collect();
        eprintln!("SEM-CONTENT {name}: E3071/E3110/E3157={codes:?}");
        assert!(
            codes.is_empty(),
            "sem content path reproduced E3071/E3110/E3157 in {name}: {codes:?}"
        );
    }
    // bom.mc is all `define` BOM entries; non-enum DOT attribute values such as
    // `RES.0R_NC` reference local defines, not enum classes. Report separately.
    for name in ["bom.mc"] {
        let path = project_root.join("src").join(name);
        let uri: String = path.to_string_lossy().into_owned();
        let content = std::fs::read_to_string(&path).expect("read .mc file");
        let _ = registry.call(
            "sem",
            Some(serde_json::json!({ "uri": uri, "content": content })),
        );
        let codes: Vec<(u32, String)> = mcc::mcc_diagnose_all()
            .iter()
            .filter(|d| d.loc.uri.as_str().contains(name))
            .filter(|d| d.code == 3071 || d.code == 3110 || d.code == 3157)
            .map(|d| (d.code, d.msg.clone()))
            .collect();
        eprintln!("SEM-CONTENT {name}: E3071/E3110/E3157={codes:?}");
    }
}

/// Full mcext flow with diagnostics trace: init via RPC, lib.load, load_project
/// (entry hbl.mc), project_symbols, then did_open for us513.mc (initial
/// no-content sem -> auto_load, then load_project(uri) + sem(content)).
/// Prints the FULL diagnostic store at each stage so fresh vs stale E3071/E3110/
/// E3157 can be attributed to a specific step.
#[test]
fn repro_mcext_full_trace() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let hbl_uri: String = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    let us513_uri: String = project_root
        .join("src/us513.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    let _cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&project_root).expect("chdir project root");
    mcc::mcc_set_system_root(sys_root.as_path());
    mcc::mcc_set_project_root(&project_root);

    let server = mcc::rpc::handlers::register_all(mcc::rpc::RpcServerBuilder::new()).build();
    let registry = server.registry();

    let dump = |tag: &str| {
        let mut by_file: std::collections::BTreeMap<String, Vec<u32>> =
            std::collections::BTreeMap::new();
        for d in mcc::mcc_diagnose_all() {
            by_file.entry(d.loc.uri.clone()).or_default().push(d.code);
        }
        eprintln!("TRACE[{tag}]:");
        for (uri, codes) in by_file {
            let mut sorted = codes.clone();
            sorted.sort();
            eprintln!("  {uri}: {:?}", sorted);
        }
    };

    let _ = registry.call("init", None);
    dump("after init");
    let _ = registry.call("lib.load", Some(serde_json::json!({ "name": "mcode" })));
    dump("after lib.load mcode");
    let _ = registry.call(
        "load_project",
        Some(serde_json::json!({ "entry": hbl_uri })),
    );
    dump("after load_project hbl.mc");
    let _ = registry.call("project_symbols", None);
    dump("after project_symbols");

    // did_open us513.mc: initial parse_and_publish sem (no content).
    let _ = registry.call("sem", Some(serde_json::json!({ "uri": us513_uri })));
    dump("after sem(us513, no-content)");
    // did_open's explicit load_project(uri).
    let _ = registry.call(
        "load_project",
        Some(serde_json::json!({ "entry": us513_uri })),
    );
    dump("after load_project us513.mc");
    // reparse sem with content.
    let content = std::fs::read_to_string(project_root.join("src/us513.mc")).expect("read");
    let _ = registry.call(
        "sem",
        Some(serde_json::json!({ "uri": us513_uri, "content": content })),
    );
    dump("after sem(us513, content)");
}

/// Regression: a round that parses a project file while the mcode library is
/// NOT loaded emits E3157/E3071 (unresolved class / method). A later clean
/// round (mcode loaded, project reloaded) must leave none of that residue in
/// the diagnostic store — every round re-derives the full diagnostic set, so
/// a broken earlier round must never survive.
#[test]
fn repro_stale_resolution_cleared() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let us513_uri: mcc::McURI = project_root
        .join("src/us513.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    let _cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&project_root).expect("chdir project root");
    mcc::mcc_set_system_root(sys_root.as_path());
    mcc::mcc_set_project_root(&project_root);

    // Broken round: mcode NOT loaded -> PKG/CAP/RES unresolvable.
    mcc::mcc_init_no_lib();
    let content = std::fs::read_to_string(project_root.join("src/us513.mc")).expect("read");
    mcc::mcc_load_from_string(&us513_uri, &content);
    let broken = diag_codes();
    eprintln!("BROKEN round E3071/E3110/E3157: {broken:?}");
    assert!(
        !broken.is_empty(),
        "precondition failed: a no-lib round must emit E3071/E3110/E3157"
    );

    // Clean round: load mcode and reload the project closure.
    mcc::mcc_init();
    mcc::mcc_load_project(&us513_uri);
    let clean = diag_codes();
    eprintln!("CLEAN round E3071/E3110/E3157: {clean:?}");
    assert!(
        clean.is_empty(),
        "stale resolution diagnostics survived a clean round: {clean:?}"
    );
}

/// Regression: repeated load_project / sem(content) rounds must NOT double a
/// file's diagnostics. Every round rebuilds the lapper and re-runs PostParse
/// validators for every workspace file, so per-file counts must stay stable
/// (previously they grew 5 -> 10 -> 15 for files outside the re-parsed use
/// closure).
#[test]
fn repro_no_diagnostic_accumulation() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let hbl_uri: String = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    let us513_uri: String = project_root
        .join("src/us513.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    let _cwd = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(&project_root).expect("chdir project root");
    mcc::mcc_set_system_root(sys_root.as_path());
    mcc::mcc_set_project_root(&project_root);

    let server = mcc::rpc::handlers::register_all(mcc::rpc::RpcServerBuilder::new()).build();
    let registry = server.registry();
    let _ = registry.call("init", None);
    let _ = registry.call("lib.load", Some(serde_json::json!({ "name": "mcode" })));
    let _ = registry.call(
        "load_project",
        Some(serde_json::json!({ "entry": hbl_uri })),
    );

    let count_by_basename = |names: &[&str]| -> Vec<(String, usize)> {
        names
            .iter()
            .map(|n| {
                let c = mcc::mcc_diagnose_all()
                    .iter()
                    .filter(|d| d.loc.uri.as_str().ends_with(&format!("/{n}")))
                    .count();
                ((*n).to_string(), c)
            })
            .collect()
    };

    let names = ["periph.mc", "power.mc", "us513.mc"];
    let base = count_by_basename(&names);

    // Round 2: load_project(us513) — periph.mc is outside its use closure.
    let _ = registry.call(
        "load_project",
        Some(serde_json::json!({ "entry": us513_uri })),
    );
    // Round 3: sem(us513, content) — reparse the edited file from memory.
    let content = std::fs::read_to_string(project_root.join("src/us513.mc")).expect("read");
    let _ = registry.call(
        "sem",
        Some(serde_json::json!({ "uri": us513_uri, "content": content })),
    );

    let after = count_by_basename(&names);
    eprintln!("BASE counts: {base:?}");
    eprintln!("AFTER counts: {after:?}");
    for (b, a) in base.iter().zip(after.iter()) {
        assert_eq!(
            b.1, a.1,
            "diagnostic count for {} grew across rounds ({} -> {})",
            b.0, b.1, a.1
        );
    }
}
