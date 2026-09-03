// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! use/import diagnostic coverage locks (reorg doc §8.3/§8.5, use/import batch) —
//! one firing test per reachable code of the `use`-statement import family,
//! loaded through the in-process API and asserted on `mcc_diagnose_all`.
//!
//! Firing codes locked here (each asserts its own code is present; the loader
//! co-fires a few neighbors that are also asserted where they are the point):
//!
//! | code | fixture essence | fires with |
//! |---|---|---|
//! | 2004 USE_SELF_IMPORT | file `use`s itself by `./main.mc` (must load via `mcc_load_project`) | 5642 |
//! | 2006 USE_VERSIONED_TARGET_NOT_FOUND | `use $::nonexistent.lib@1.0` never loaded | 2003/2052/3157/5256 |
//! | 2007 USE_IMPORT_SYMBOL_NOT_FOUND | `use ./tgt.mc : NOPE`, tgt exports only BETA | 2007+2071 |
//! | 2008 USE_REEXPORT_SYMBOL_NOT_FOUND | same but `pub use` | +2007/2071 |
//! | 2009 USE_MIXED_PATH_SEPARATORS | loaded file under a dot-namespaced dir segment | 5642 |
//! | 2071 USE_IMPORTED_NOT_FOUND | load-time parse_nsp miss on the colon import | +2007 |
//!
//! 2061 USE_SYMBOL_CONFLICT needs a temp `MCC_SYSTEM_ROOT` resolved before the
//! process's first mcc init, so it lives in its own single-test binary
//! (tests/use_symbol_conflict.rs) rather than here.
//!
//! Codes recorded as context-gated (defensive arms the C grammar never routes
//! to — parse aborts at 2081 or a bare `use conn` gets an injected `$` prefix
//! → 2003/2052, so these Rust arms are unreachable from real source): 2001
//! USE_PATH_INVALID, 2002 USE_URI_PREFIX_INVALID, 2010 USE_TRAILING_NODE. See
//! the emission sites in src/db/infra/mc_use.rs and reorg doc §8.3.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use std::path::{Path, PathBuf};

/// Sorted + deduped codes in the workspace diagnostic channel.
fn codes() -> Vec<u32> {
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort();
    v.dedup();
    v
}

/// A unique throwaway directory for one test's on-disk files.
fn fresh_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mcc-use-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Write `name` under `dir`, canonicalize it, and load it by absolute path.
fn load_real(dir: &Path, name: &str, content: &str) -> String {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    let canon = p.canonicalize().unwrap();
    let s = canon.to_string_lossy().to_string();
    mcc::mcc_load_from_string(&s, content);
    s
}

/// 2004 USE_SELF_IMPORT (imports.rs K1): the entry file `use`s itself by its
/// own relative path. Loaded via `mcc_load_project`, the recursion-safe loader
/// (a plain `mcc_load_from_string` re-reads the not-yet-inserted self from
/// disk and recurses forever).
#[test]
fn sem_useimp__self_import_reports_2004() {
    let _lock = common::lock();
    common::reset();

    let dir = fresh_dir("2004");
    let src = "use ./main.mc\n\nmodule main {\n    io VDD\n}\n";
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("main.mc");
    std::fs::write(&p, src).unwrap();
    let uri = p.canonicalize().unwrap().to_string_lossy().to_string();
    mcc::mcc_load_project(&uri);
    let c = codes();
    assert!(
        c.contains(&mcc::errcodes::USE_SELF_IMPORT),
        "self-import must report 2004; got codes: {c:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 2006 USE_VERSIONED_TARGET_NOT_FOUND (imports.rs K3): a versioned system-lib
/// use whose target is never loaded. A single virtual load suffices.
#[test]
fn sem_useimp__versioned_target_not_found_reports_2006() {
    let _lock = common::lock();
    common::reset();

    let src = "use $::nonexistent.lib@1.0\n\nmodule main {\n    U1::init()\n}\n";
    let uri = "/mcc/useimp2006.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let c = codes();
    assert!(
        c.contains(&mcc::errcodes::USE_VERSIONED_TARGET_NOT_FOUND),
        "unresolved versioned use must report 2006; got codes: {c:?}"
    );
}

/// Shared two-file colon-import fixture: `tgt.mc` exports only BETA, but the
/// entry imports `NOPE`. Used by 2007/2008/2071.
const SRC_TGT: &str = "component BETA {\n    pins = [\n        1 = 1\n    ]\n}\n";

/// 2007 USE_IMPORT_SYMBOL_NOT_FOUND (imports.rs K4): PostParse check — the
/// colon-imported id is absent from the target's spacenames.
#[test]
fn sem_useimp__import_symbol_not_found_reports_2007() {
    let _lock = common::lock();
    common::reset();

    let dir = fresh_dir("2007");
    let main = "use ./tgt.mc : NOPE\n\nmodule main {\n    io VDD\n}\n";
    let _ = load_real(&dir, "tgt.mc", SRC_TGT);
    let _ = load_real(&dir, "main.mc", main);
    let c = codes();
    assert!(
        c.contains(&mcc::errcodes::USE_IMPORT_SYMBOL_NOT_FOUND),
        "colon-import of an absent symbol must report 2007; got codes: {c:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 2008 USE_REEXPORT_SYMBOL_NOT_FOUND (imports.rs K5): the same miss through a
/// public `pub use` re-export path (co-fires 2007/2071).
#[test]
fn sem_useimp__reexport_symbol_not_found_reports_2008() {
    let _lock = common::lock();
    common::reset();

    let dir = fresh_dir("2008");
    let main = "pub use ./tgt.mc : NOPE\n\nmodule main {\n    io VDD\n}\n";
    let _ = load_real(&dir, "tgt.mc", SRC_TGT);
    let _ = load_real(&dir, "main.mc", main);
    let c = codes();
    assert!(
        c.contains(&mcc::errcodes::USE_REEXPORT_SYMBOL_NOT_FOUND),
        "pub re-export of an absent symbol must report 2008; got codes: {c:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 2071 USE_IMPORTED_NOT_FOUND (mc_code.rs parse_nsp): the load-time parse of
/// the same colon import also reports the id as not exported by the target.
#[test]
fn sem_useimp__imported_not_found_reports_2071() {
    let _lock = common::lock();
    common::reset();

    let dir = fresh_dir("2071");
    let main = "use ./tgt.mc : NOPE\n\nmodule main {\n    io VDD\n}\n";
    let _ = load_real(&dir, "tgt.mc", SRC_TGT);
    let _ = load_real(&dir, "main.mc", main);
    let c = codes();
    assert!(
        c.contains(&mcc::errcodes::USE_IMPORTED_NOT_FOUND),
        "load-time parse_nsp miss must report 2071; got codes: {c:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// 2009 USE_MIXED_PATH_SEPARATORS (body.rs L1): any loaded workspace file
/// whose URI mixes a '/' path with a dot-namespace directory segment. The
/// honest shape is a file inside a directory whose name contains a dot.
#[test]
fn sem_useimp__dot_namespaced_path_reports_2009() {
    let _lock = common::lock();
    common::reset();

    let dir = fresh_dir("2009");
    let dotted = dir.join("parts.one.lib");
    let src = "module main {\n    io VDD\n}\n";
    std::fs::create_dir_all(&dotted).unwrap();
    let p = dotted.join("leaf.mc");
    std::fs::write(&p, src).unwrap();
    let uri = p.canonicalize().unwrap().to_string_lossy().to_string();
    mcc::mcc_load_from_string(&uri, src);
    let c = codes();
    assert!(
        c.contains(&mcc::errcodes::USE_MIXED_PATH_SEPARATORS),
        "dot-namespaced dir segment must report 2009; got codes: {c:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
