// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! 2061 USE_SYMBOL_CONFLICT (reorg doc §8.3/§8.5, use/import batch) — the only
//! one of the reachable use/import codes that needs a *fresh process*: the
//! system root is resolved once per process from `MCC_SYSTEM_ROOT` (else
//! `~/.mcode`) and `mcc_set_system_root` is a no-op (use-design §19.10 D4).
//! Setting the env var inside a process that already ran an mcc init is a
//! no-op, so this test is its own single-test binary: the env var is set
//! before this process's first mcc call, and no sibling test can race it.
//!
//! Fixture: two system libraries `liba` and `libb` whose entry files both
//! re-export a module named `res` exposing the same component `RES`; the
//! entry `use`s `$::liba.res` and `$::libb.res`, so two resolved libs share a
//! final module name with overlapping export symbol sets → 2061 at parse_nsp
//! (mc_code.rs §14).
//!
//! The other reachable use/import codes (2004/2006/2007/2008/2009/2071) live
//! in tests/use_import_codes.rs; 2001/2002/2010 are context-gated (defensive
//! arms the grammar never routes to), see that file's module doc.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

/// A real third-party library layout: `<root>/<lib>/<lib>.mc` re-exports
/// `<root>/<lib>/res/res.mc`, mirroring the corpus `acme` shape.
#[test]
fn sem_useimp__symbol_conflict_across_libs_reports_2061() {
    let root = std::env::temp_dir().join(format!("mcc-use2061-root-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // Resolve the temp system root BEFORE the first mcc init in this process.
    std::env::set_var("MCC_SYSTEM_ROOT", &root);

    let _lock = common::lock();
    common::reset();

    let res = "component RES {\n    pins = [\n        1 = 1\n    ]\n}\n";
    for lib in ["liba", "libb"] {
        let ldir = root.join(lib);
        std::fs::create_dir_all(ldir.join("res")).unwrap();
        std::fs::write(ldir.join(format!("{lib}.mc")), "pub use ./res/res.mc\n").unwrap();
        std::fs::write(ldir.join("res/res.mc"), res).unwrap();
    }

    let src = "use $::liba.res\nuse $::libb.res\n\nmodule main {\n    io VDD\n}\n";
    let uri = "/mcc/use2061.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort();
    codes.dedup();
    assert!(
        codes.contains(&mcc::errcodes::USE_SYMBOL_CONFLICT),
        "two libs sharing module 'res' with overlapping exports must report 2061; got codes: {codes:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
