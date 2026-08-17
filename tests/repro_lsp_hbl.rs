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
/// LSP sends `sem` with editor content for periph.mc, then reads diagnostics.
/// This exercises `handle_sem`'s content branch (mcb_add_from_string ->
/// mcb_parse_all_modules -> create_lapper), which is the path that emits
/// E3071/E3157 in the extension but is NOT covered by mcc_build.
#[test]
fn repro_sem_content_path() {
    let _lock = REPRO_LOCK.lock().unwrap();
    let project_root = hbl_project_dir();
    let periph_path = project_root.join("src/periph.mc");
    let periph_uri: String = periph_path.to_string_lossy().into_owned();
    let hbl_uri: String = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();
    let sys_root = server_data_root();

    // mcext init order.
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

    // didOpen: parse periph.mc from editor content, then collect diagnostics.
    let content = std::fs::read_to_string(&periph_path).expect("read periph.mc");
    let _ = registry.call(
        "sem",
        Some(serde_json::json!({ "uri": periph_uri, "content": content })),
    );

    let codes = diag_codes();
    eprintln!("SEM-CONTENT E3071/E3110/E3157: {:?}", codes);
    assert!(
        codes.is_empty(),
        "sem content path reproduced E3071/E3110/E3157: {codes:?}"
    );
}
