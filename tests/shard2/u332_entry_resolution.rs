// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Regression locks for the U332 entry-resolution faces (b4117 replay sweep:
//! log/9.27.u332-repro-sweep.md).
//!
//! `resolve_entry_module` matched the entry only by exact (ident, uri) or by
//! uri-equivalent suffix, so a caller passing a valid module ident with the
//! entry FILE's uri failed with "Target module not found" while the error's
//! own Available-modules list showed the target — the U332 evidence shape.
//! The unique-ident fallback resolves that face; ambiguity (the same ident in
//! two files) must stay an error.

use crate::common;

use mcc::McIds;

const HELPER_SRC: &str = r#"
module helper()
{
    in p
}
"#;

const MAIN_SRC: &str = r#"
module main()
{
}
"#;

/// A module ident whose defining file differs from the passed entry uri
/// resolves when the workspace holds exactly one module of that name.
#[test]
fn svc_u332entry__unique_ident_resolves_across_files() {
    let _lock = common::lock();
    common::reset();

    common::load_string("/mcc/u332/other.mc", HELPER_SRC);
    common::load_string("/mcc/u332/main.mc", MAIN_SRC);

    let built = mcc::mcc_build(&McIds::from("helper"), &"/mcc/u332/main.mc".to_string().into());
    assert!(
        built.is_ok(),
        "unique workspace ident must resolve regardless of the passed uri: {:?}",
        built.err().map(|e| e.to_string())
    );
}

/// The same ident defined in two files stays an error when the passed uri
/// matches neither — the fallback must not silently pick a side.
#[test]
fn svc_u332entry__ambiguous_ident_still_errors() {
    let _lock = common::lock();
    common::reset();

    common::load_string("/mcc/u332/one.mc", HELPER_SRC);
    common::load_string("/mcc/u332/two.mc", HELPER_SRC);
    common::load_string("/mcc/u332/main.mc", MAIN_SRC);

    let err = mcc::mcc_build(&McIds::from("helper"), &"/mcc/u332/main.mc".to_string().into())
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(
        err.contains("Target module not found"),
        "ambiguous ident with a mismatched uri must stay an error, got: {err}"
    );
}
