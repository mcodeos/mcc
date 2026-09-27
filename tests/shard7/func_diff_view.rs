// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `diff` functional-alignment locks (CIMP §1 U280, sixth slice): every
//! §6.1 block judgment on real acceptance runs — the same engine the `check`
//! gate judges with, one world per side, so the comparison cannot read a
//! verdict the gate would not have spelled.

use mcc::stages::funcdiff::{
    functional_change, functional_counts, functional_view, render_functional_text, semantic_key,
    side_verdicts,
};
use mcc::Kind;

use crate::common;

/// Same intent, implementation A delivers 3.3V.
const A_33: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    io RAW
    B u1
    RAW -> u1.1
    expects = [
        u1 = B
        RAW = driven
        RAW = [low:3.0V, high:3.5V]
    ]
}
"#;

/// Same intent, same shared keys — implementation B delivers 3.4V instead of
/// 3.3V: a different implementation inside the same declared window.
const B_34: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.4V)
    ]
}
module main {
    io RAW
    B u1
    RAW -> u1.1
    expects = [
        u1 = B
        RAW = driven
        RAW = [low:3.0V, high:3.5V]
    ]
}
"#;

/// Same shared keys, implementation B delivers 3.6V: outside the shared
/// window — the divergence case, FAIL on B only.
const B_36: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.6V)
    ]
}
module main {
    io RAW
    B u1
    RAW -> u1.1
    expects = [
        u1 = B
        RAW = driven
        RAW = [low:3.0V, high:3.5V]
    ]
}
"#;

/// `partno` names nothing, so the conditional row is not statically
/// judgeable: both sides DEFER the same key — the deferred case.
const DEFERRED: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    B u1
    if (partno == "X") {
        expects += [
            u1 = B
        ]
    }
}
"#;

/// The A side of the disjoint pair: one role-match expectation only.
const NONE_A: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    io RAW
    B u1
    RAW -> u1.1
    expects = [
        u1 = B
    ]
}
"#;

/// The B side of the disjoint pair: one driven expectation only — no key
/// overlaps the A side's, so no equivalence judgment is possible.
const NONE_B: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    io RAW
    B u1
    RAW -> u1.1
    expects = [
        RAW = driven
    ]
}
"#;

/// Run the acceptance engine on one source as one world.
fn side(src: &str, uri_path: &str) -> (mcc::Ledger, mcc::check::expectation::ExpectationReport) {
    let _guard = common::lock();
    common::reset();
    let uri: mcc::McURI = uri_path.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2 flat failed");
    let (_, module) = mcc::definition_space()
        .workspace_modules()
        .into_iter()
        .find(|(sn, _)| sn.ident.to_string() == "main")
        .expect("module 'main' not registered");
    let report = mcc::check::expectation::run(&table, &module.expects, &module.uri);
    (module.expects.clone(), report)
}

/// Both sides, compared.
fn compare(
    a_src: &str,
    b_src: &str,
) -> (
    mcc::stages::funcdiff::SideVerdicts,
    mcc::stages::funcdiff::SideVerdicts,
    mcc::stages::funcdiff::FunctionalChange,
) {
    let (a_ledger, a_report) = side(a_src, "/mcc/func-diff-a.mc");
    let (b_ledger, b_report) = side(b_src, "/mcc/func-diff-b.mc");
    let a = side_verdicts(&a_ledger, &a_report);
    let b = side_verdicts(&b_ledger, &b_report);
    let change = functional_change(&a, &b);
    (a, b, change)
}

#[test]
fn same_window_different_implementation_is_boundary_equivalent() {
    let (_a, _b, change) = compare(A_33, B_34);
    assert_eq!(change.change_type, "module-replace");
    assert_eq!(change.kind, "module-replace");
    let delta = change.delta.expect("a shared face carries its delta");
    // Shared keys, in the A side's ledger order: role-match, driven,
    // value-bound. The window rides the key verbatim.
    let keys: Vec<&str> = delta
        .shared_keys
        .iter()
        .map(|s| s.key.as_str())
        .collect();
    assert_eq!(
        keys,
        vec!["role-match:u1", "driven:RAW", "value-bound:RAW[3.0V~3.5V]"]
    );
    assert!(
        delta
            .shared_keys
            .iter()
            .all(|s| s.a == "PASS" && s.b == "PASS"),
        "every shared row passes on both sides: {:?}",
        delta.shared_keys
    );
}

#[test]
fn one_shared_fail_on_one_side_is_verdict_divergence() {
    let (_a, _b, change) = compare(A_33, B_36);
    assert_eq!(change.change_type, "verdict-divergence");
    let delta = change.delta.expect("a shared face carries its delta");
    let failed: Vec<&str> = delta
        .shared_keys
        .iter()
        .filter(|s| s.a == "FAIL" || s.b == "FAIL")
        .map(|s| s.key.as_str())
        .collect();
    assert_eq!(failed, vec!["value-bound:RAW[3.0V~3.5V]"]);
    let row = delta
        .shared_keys
        .iter()
        .find(|s| s.key == "value-bound:RAW[3.0V~3.5V]")
        .expect("the window row is shared");
    assert_eq!(
        (row.a.as_str(), row.b.as_str()),
        ("PASS", "FAIL"),
        "B delivered 3.6V into a [3.0V, 3.5V] window: B fails, A passes"
    );
}

#[test]
fn shared_defer_is_deferred_to_the_dynamic_arm() {
    let (_a, _b, change) = compare(DEFERRED, DEFERRED);
    assert_eq!(change.change_type, "deferred");
    let delta = change.delta.expect("a shared face carries its delta");
    assert!(delta
        .shared_keys
        .iter()
        .all(|s| s.a == "DEFER" && s.b == "DEFER"));
}

#[test]
fn disjoint_expects_judge_none_and_omit_the_delta() {
    let (a, b, change) = compare(NONE_A, NONE_B);
    assert_eq!(change.change_type, "none");
    assert!(change.delta.is_none(), "no shared face, no delta");
    // Both sides' own rows PASS, and each side's key is the other's unshared.
    assert!(a.rows.iter().all(|(_, v)| v == "PASS"));
    assert!(b.rows.iter().all(|(_, v)| v == "PASS"));
    assert_eq!(a.rows[0].0, "role-match:u1");
    assert_eq!(b.rows[0].0, "driven:RAW");
}

#[test]
fn keys_spell_kind_target_and_verbatim_bound() {
    assert_eq!(semantic_key("role-match", "u1", &Kind::Class("B".into())), "role-match:u1");
    assert_eq!(semantic_key("driven", "RAW", &Kind::Driven), "driven:RAW");
    assert_eq!(
        semantic_key(
            "value-bound",
            "RAW",
            &Kind::Window {
                low: Some("3.0V".into()),
                high: Some("3.5V".into()),
            }
        ),
        "value-bound:RAW[3.0V~3.5V]"
    );
    // An unstated side is a `-`, absent rather than normalized away.
    assert_eq!(
        semantic_key(
            "value-bound",
            "RAW",
            &Kind::Window {
                low: None,
                high: Some("3.5V".into()),
            }
        ),
        "value-bound:RAW[-~3.5V]"
    );
}

#[test]
fn text_face_says_what_ruled_the_block_and_accounts_sides() {
    let (a, b, change) = compare(A_33, B_36);
    let view = functional_view("/mcc/a.mc", "/mcc/b.mc", &change);
    assert_eq!(view.view, "diff");
    let text = render_functional_text(&view, &a, &b);
    assert!(
        text.contains("verdict-divergence"),
        "the ruling word is on the page: {text}"
    );
    assert!(
        text.contains("value-bound:RAW[3.0V~3.5V]"),
        "the shared key is on the page: {text}"
    );
    assert!(
        !text.contains("only in"),
        "every key is shared here, so the separate accounting stays empty: {text}"
    );

    let (a, b, change) = compare(NONE_A, NONE_B);
    let view = functional_view("/mcc/a.mc", "/mcc/b.mc", &change);
    let text = render_functional_text(&view, &a, &b);
    assert!(text.contains("no shared expectation"), "{text}");
    assert!(text.contains("only in A"), "{text}");
    assert!(text.contains("role-match:u1"), "{text}");
    assert!(text.contains("only in B"), "{text}");
    assert!(text.contains("driven:RAW"), "{text}");
}

#[test]
fn counts_come_from_the_items_and_name_every_word() {
    let (_a, _b, change) = compare(A_33, B_36);
    let view = functional_view("/mcc/a.mc", "/mcc/b.mc", &change);
    let counts = functional_counts(&view.items);
    assert_eq!(counts["verdict-divergence"].as_u64(), Some(1));
    assert_eq!(counts["module-replace"].as_u64(), Some(0));
    assert_eq!(counts["deferred"].as_u64(), Some(0));
    assert_eq!(counts["none"].as_u64(), Some(0));
}
