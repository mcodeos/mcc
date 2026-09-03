// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Acceptance tests for the unified class-name resolution policy (§5.4.3) and
//! the V(F) visibility filter (resolve-unification.md §3).
//!
//! These tests exercise `Resolver::resolve_class` and `is_visible` directly
//! against a small loaded workspace.
//!
//! NOTE: This file runs in its own process (integration tests are separate
//! test binaries), so it cannot disturb the in-crate `mc_code::tests` that
//! share mcc's global state in-process. The tests inside this file are still
//! serialized by a mutex because they share mcc's global state with each
//! other.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::db::resolve::{is_visible, Resolver};
use mcc::{McCMIE, McIds, McSpaceName, McURI};

const IFS_SOURCE: &str = r#"
interface DC(volt)
{
    pins = [
        1 = VOUT, "DC power positive"
        2 = GND, "DC power ground"
    ]
}
"#;

/// Load a source string into the workspace, returning its URI.
fn load(uri: &str, source: &str) -> McURI {
    let mc_uri = McURI::from(uri);
    mcc::mcc_load_from_string(&mc_uri, source);
    mc_uri
}

/// Extract the defining URI from a resolved CMIE.
fn def_uri(cmie: &McCMIE) -> String {
    match cmie {
        McCMIE::Component(c) => c.uri.to_string(),
        McCMIE::Module(m) => m.uri.to_string(),
        McCMIE::Interface(i) => i.uri.to_string(),
        McCMIE::Enum(e) => e.uri.to_string(),
    }
}

/// A def defined in a file resolves from that same file (P3).
#[test]
fn svc_resolvepol__own_file_def_resolves_through_p3() {
    let _lock = common::lock();
    common::reset();

    let a = load("/mcc/resolve/ifs.mc", IFS_SOURCE);
    let hit = Resolver::resolve_class(&a, &McIds::from("DC")).expect("DC defined in own file");
    assert_eq!(
        def_uri(&hit),
        "/mcc/resolve/ifs.mc",
        "P3 must resolve the own-file DC"
    );
}

/// A class defined in another file must NOT be reachable by name alone when
/// the referencing file has no `use` (§5.4.5): regression for the case where
/// `net1.basic.mc` resolved `interface DC` to `c3.defs.mc` without a use.
#[test]
fn svc_resolvepol__unused_cross_file_same_name_does_not_resolve() {
    let _lock = common::lock();
    common::reset();

    let a = load("/mcc/resolve/c3.defs.mc", IFS_SOURCE);
    let b = load(
        "/mcc/resolve/net1.basic.mc",
        r#"
module main
{
    in PWR_[VDD2, GND2]::DC(5V)
}
"#,
    );

    // B never `use`s A, so `DC` must fail to resolve from B (no system lib).
    assert!(
        Resolver::resolve_class(&b, &McIds::from("DC")).is_none(),
        "DC defined only in {a} must not resolve from {b} without a use"
    );
    // Sanity: from A itself, DC still resolves (P3).
    assert!(Resolver::resolve_class(&a, &McIds::from("DC")).is_some());
}

/// A `use` statement makes the target file's classes reachable (P4).
#[test]
fn svc_resolvepol__use_chain_resolves_to_target_def() {
    let _lock = common::lock();
    common::reset();

    // Real on-disk files: string-loaded virtual files cannot exercise the
    // use-chain canonicalization (`update_abs_path` re-appends ".mc" only when
    // the target exists on disk, so a virtual `use ./c3.mc` yields uri "c3").
    let dir = std::env::temp_dir().join(format!("mcc-resolve-use-chain-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create fixture directory");
    let a_path = dir.join("c3.mc");
    std::fs::write(&a_path, IFS_SOURCE).expect("write c3.mc");
    let b_path = dir.join("net2.mc");
    std::fs::write(
        &b_path,
        r#"
use ./c3.mc

module main
{
    in PWR_[VDD2, GND2]::DC(5V)
}
"#,
    )
    .expect("write net2.mc");

    mcc::mcc_set_project_root(&dir);
    // Canonicalize (macOS /var -> /private/var) so the test URI equals the
    // canonical workspace key used during loading.
    let b_canon = std::fs::canonicalize(&b_path).expect("canonicalize net2.mc");
    let b = McURI::from(b_canon.to_string_lossy().to_string());
    mcc::mcc_load_project(&b);

    let hit = Resolver::resolve_class(&b, &McIds::from("DC")).expect("DC via use chain");
    let a_canon = std::fs::canonicalize(&a_path).expect("canonicalize c3.mc");
    assert_eq!(
        def_uri(&hit),
        a_canon.to_string_lossy().as_ref(),
        "P4 must resolve to the use'd target's DC"
    );
}

/// `is_visible` implements V(F) = P3 ∪ P4 ∪ P5.
#[test]
fn svc_resolvepol__is_visible_filters_visibility_set() {
    let _lock = common::lock();
    common::reset();

    let a = load("/mcc/resolve/ifs.mc", IFS_SOURCE);
    let b = load(
        "/mcc/resolve/net1.mc",
        r#"
module main
{
    in PWR_[VDD2, GND2]::DC(5V)
}
"#,
    );

    // P3: a file's own def is visible from itself.
    let a_dc = McSpaceName {
        ident: McIds::from("DC"),
        uri: mcc::uri_intern(&a),
    };
    assert!(is_visible(&a, &a_dc), "own-file def must be visible (P3)");

    // Negative: A's def is not visible from B without a use.
    assert!(
        !is_visible(&b, &a_dc),
        "cross-file def without use must be invisible"
    );

    // P5: an mcode symbol (empty global tables here) is not visible either;
    // the positive P5 case is covered by the test that loads the real mcode
    // library in `ref_def_map_entries_carry_ast_def_names` (in-crate).
    let mcode_dc = McSpaceName {
        ident: McIds::from("DC"),
        uri: mcc::uri_intern("mcode/ifs/dc.mc"),
    };
    assert!(!is_visible(&b, &mcode_dc));
}

/// T11 (N3): `use X as A` re-exposes the target's alphabetically-first CMIE
/// under the bare alias and hides that CMIE's original name; the target's
/// remaining CMIEs keep their own names. After consolidation the name_index
/// must mirror the phase-6 `spacenames`, so post-consolidation (map-based)
/// lookups agree with the pre-consolidation visibility that module parsing
/// sees: the alias resolves to the first CMIE's module, the hidden original
/// name no longer resolves from the importer, and the second CMIE still does.
#[test]
fn svc_resolvepol__alias_re_exposes_first_cmie_and_hides_original_name_post_consolidation() {
    let _lock = common::lock();
    common::reset();

    // Real on-disk files (string-loaded virtual files cannot exercise the
    // use-chain canonicalization — see `svc_resolvepol__use_chain_resolves_to_target_def`).
    let dir = std::env::temp_dir().join(format!("mcc-resolve-alias-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create fixture directory");
    // `target` sorts before `zeta`, so the alias `tgt` binds to `target`.
    let tgt_path = dir.join("alias_target.mc");
    std::fs::write(
        &tgt_path,
        r#"
module target {
    func init() {
    }
}

module zeta {
    func init() {
    }
}
"#,
    )
    .expect("write alias_target.mc");
    let user_path = dir.join("alias_user.mc");
    std::fs::write(
        &user_path,
        r#"
use ./alias_target.mc as tgt

module main {
}
"#,
    )
    .expect("write alias_user.mc");

    mcc::mcc_set_project_root(&dir);
    let user_canon = std::fs::canonicalize(&user_path).expect("canonicalize alias_user.mc");
    let user = McURI::from(user_canon.to_string_lossy().to_string());
    let tgt_canon = std::fs::canonicalize(&tgt_path).expect("canonicalize alias_target.mc");
    let tgt_uri = tgt_canon.to_string_lossy().to_string();
    mcc::mcc_load_project(&user);

    // The alias denotes the target's alphabetically-first CMIE (module target).
    let hit = Resolver::resolve_class(&user, &McIds::from("tgt")).expect("alias resolves");
    match &hit {
        McCMIE::Module(m) => {
            assert_eq!(m.name.to_string(), "target", "alias names the first CMIE");
        }
        other => {
            panic!("alias must resolve to a module, got {}", def_uri(other));
        }
    }
    assert_eq!(def_uri(&hit), tgt_uri, "alias def lives in the target file");

    // The first CMIE's original name is hidden from the importer: it must
    // neither resolve nor be visible (no mcode library loaded).
    assert!(
        Resolver::resolve_class(&user, &McIds::from("target")).is_none(),
        "original first-CMIE name must stay hidden behind the alias"
    );
    let hidden = McSpaceName {
        ident: McIds::from("target"),
        uri: mcc::uri_intern(&tgt_uri),
    };
    assert!(
        !is_visible(&user, &hidden),
        "hidden original name must not be visible from the importer"
    );

    // The remaining CMIEs keep their original names.
    let second = Resolver::resolve_class(&user, &McIds::from("zeta")).expect("second CMIE");
    assert_eq!(def_uri(&second), tgt_uri, "second CMIE keeps its own name");
}
