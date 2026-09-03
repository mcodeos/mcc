//! Regression test: re-loading an mcode library file as a NON-system project
//! file (e.g. via load_project/did_open on a file inside ~/.mcode/mcode) must
//! NOT strip its entries from the global system tables. Without the
//! `file_is_system_library` guard in the McCode constructors, the reloaded
//! file is re-registered as a workspace file and its entries are removed from
//! the global tables, which breaks the P5 system lookup for `CAP(...).Cap(_)`
//! and produces E3071.
#![allow(dead_code)]
// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::path::PathBuf;

fn server_data_root() -> PathBuf {
    // Runtime-resolved system root (MCC_SYSTEM_ROOT env or ~/.mcode default).
    mcc::cli::datadir::data_root()
}

fn count_bad() -> Vec<String> {
    mcc::mcc_diagnose_all()
        .into_iter()
        .filter(|d| d.code == 3071 || d.code == 3110 || d.code == 3157 || d.code == 3179)
        .map(|d| format!("{}#{}: {}", d.loc.uri, d.code, d.msg))
        .collect()
}

#[test]
fn def_sysreload__reload_keeps_global_tables() {
    let _lock = common::lock();
    let _guard = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::stderr)
        .without_time()
        .try_init();

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    let sys_root = server_data_root();
    std::env::set_current_dir(&project_root).expect("chdir project root");
    mcc::mcc_set_system_root(sys_root.as_path());
    mcc::mcc_set_project_root(&project_root);

    // Clean baseline: standard init auto-loads mcode from the system root.
    mcc::mcc_init();

    // Probe file exercising the failing construct.
    let probe_uri = "/virtual/probe_cap.mc";
    let cap_src = "module main {\n    CAP(10uF, 10V).Cap(GND, GND)\n}\n";

    // 1) Before degradation: should parse clean (CAP resolves via P5 global).
    mcc::mcc_load_from_string(&probe_uri.into(), cap_src);
    let before = count_bad();
    assert!(before.is_empty(), "probe must be clean on baseline");

    // 2) Load cap.mc as a NON-system project file (the suspect trigger).
    let cap_uri = sys_root.join("mcode/cap.mc").to_string_lossy().into_owned();
    mcc::mcc_load_project(&cap_uri.into());

    // 3) Re-parse the probe: CAP lookup must still succeed (global entries kept).
    mcc::mcc_load_from_string(&probe_uri.into(), cap_src);
    let after_cap = count_bad();

    // 4) Load res.mc as project file too.
    let res_uri = sys_root.join("mcode/res.mc").to_string_lossy().into_owned();
    mcc::mcc_load_project(&res_uri.into());
    mcc::mcc_load_from_string(&probe_uri.into(), cap_src);
    let after_res = count_bad();

    // 5) Repeated reopen loop (did_open style) must stay clean.
    for _round in 0..3 {
        for name in ["cap.mc", "res.mc"] {
            let uri = sys_root
                .join("mcode")
                .join(name)
                .to_string_lossy()
                .into_owned();
            mcc::mcc_load_project(&uri.into());
        }
        mcc::mcc_load_from_string(&probe_uri.into(), cap_src);
    }
    let after_loop = count_bad();

    assert!(
        after_cap.is_empty() && after_res.is_empty() && after_loop.is_empty(),
        "loading mcode files as project files must NOT break the global system tables: \
         after_cap={after_cap:?} after_res={after_res:?} after_loop={after_loop:?}"
    );
}
