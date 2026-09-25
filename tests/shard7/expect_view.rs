// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `expectation` projection locks (CIMP U298 batch 4): one fixture per
//! verdict branch, all judged on the real flat world (pass2, not a mock) —
//! the same engine run the `check` gate uses, so the view cannot spell a
//! verdict the gate would not.

use mcc::stages::expectview::{expectation_counts, expectation_view};
use mcc::stages::StageView;

use crate::common;

/// The declared DC fixture: a sink pair carrying 3.3V, wired to a top port.
const GREEN: &str = r#"
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

/// A window the declared value falls outside of: the FAIL row carries its
/// code, the warning level, and the measured side.
const OUT_OF_WINDOW: &str = r#"
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
        RAW = [low:3.4V, high:3.5V]
    ]
}
"#;

/// `partno` names nothing: the branch is not statically judgeable, so its
/// rows land DEFER — a verdict, never a diagnostic (§5.1).
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

/// Parse `src`, build the flat world of `main`, and assemble the ledger view.
fn view_of(src: &str, uri_path: &str) -> StageView {
    // The C parser / workspace tables are process-global — hold the
    // suite-wide parse lock (init.rs), not a private one, or this races
    // other tests.
    let _guard = common::lock();
    let root = mcc::cli::datadir::data_root();
    mcc::mcc_set_system_root(&root);
    mcc::mcc_init();
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
    expectation_view("main", &module.expects, &report, &module.uri)
}

#[test]
fn green_rows_read_as_pass_items_in_ledger_order() {
    let view = view_of(GREEN, "/mcc/expect-view-green.mc");
    assert_eq!(view.view, "expectation");
    assert_eq!(view.items.len(), 3, "items: {:?}", view.items);
    // Ledger order — the row order the author wrote.
    let targets: Vec<&str> = view
        .items
        .iter()
        .map(|i| i["target"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(targets, vec!["u1", "RAW", "RAW"]);
    let verdicts: Vec<&str> = view
        .items
        .iter()
        .map(|i| i["verdict"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(verdicts, vec!["PASS", "PASS", "PASS"]);
    // The kind words are the contract's, not the row grammar's.
    assert_eq!(view.items[0]["kind"].as_str(), Some("role-match"));
    assert_eq!(view.items[1]["kind"].as_str(), Some("driven"));
    assert_eq!(view.items[2]["kind"].as_str(), Some("value-bound"));
    // The window rides verbatim; a PASS row carries no code/level/measured.
    assert_eq!(view.items[2]["bound"]["low"].as_str(), Some("3.0V"));
    assert_eq!(view.items[2]["bound"]["high"].as_str(), Some("3.5V"));
    assert!(view.items[2].get("code").is_none());
    assert!(view.items[2].get("level").is_none());
    assert!(view.items[2].get("measured").is_none());
    // Every item carries its source site.
    assert!(view.items[0]["expect"]["line"].as_u64().unwrap_or(0) > 0);
    // Counts come from the items.
    assert_eq!(expectation_counts(&view.items)["pass"].as_u64(), Some(3));
}

#[test]
fn fail_value_bound_row_carries_code_level_and_measured() {
    let view = view_of(OUT_OF_WINDOW, "/mcc/expect-view-window.mc");
    assert_eq!(view.items.len(), 1);
    let it = &view.items[0];
    assert_eq!(it["verdict"].as_str(), Some("FAIL"));
    assert_eq!(it["code"].as_str(), Some("E9004"));
    assert_eq!(it["level"].as_str(), Some("warning"));
    // The measured side is the declared value outside the window.
    assert_eq!(it["measured"].as_str(), Some("3.3V"));
    let counts = expectation_counts(&view.items);
    assert_eq!(counts["fail"].as_u64(), Some(1));
}

#[test]
fn deferred_rows_read_as_defer_items_without_diagnostics() {
    let view = view_of(DEFERRED, "/mcc/expect-view-defer.mc");
    assert_eq!(view.items.len(), 1);
    let it = &view.items[0];
    assert_eq!(it["verdict"].as_str(), Some("DEFER"));
    assert_eq!(it["target"].as_str(), Some("u1"));
    assert!(it.get("code").is_none() && it.get("level").is_none());
    assert_eq!(expectation_counts(&view.items)["defer"].as_u64(), Some(1));
}

#[test]
fn empty_ledger_reads_as_an_empty_item_set() {
    // A top with no `expects` reads as an empty ledger — not a violation
    // (schema §2.6).
    const NO_EXPECTS: &str = r#"
module main {
    io RAW
}
"#;
    let view = view_of(NO_EXPECTS, "/mcc/expect-view-empty.mc");
    assert!(view.items.is_empty());
    let counts = expectation_counts(&view.items);
    assert_eq!(counts["pass"].as_u64(), Some(0));
    assert_eq!(counts["fail"].as_u64(), Some(0));
    assert_eq!(counts["defer"].as_u64(), Some(0));
}

#[test]
fn items_and_counts_are_row_parallel_with_the_engine() {
    // The invariant the item builder zips on: one outcome per row, one
    // diagnostic per FAIL row. A mixed board exercises both zips at once.
    const MIXED: &str = r#"
component B {
    pins = [
        psnk [1, 2] = [VDD, VSS]::DC(3.3V)
    ]
}
module main {
    B u1
    expects = [
        u1 = NOT_B
        ghost = B
    ]
}
"#;
    let view = view_of(MIXED, "/mcc/expect-view-mixed.mc");
    assert_eq!(view.items.len(), 2);
    let codes: Vec<&str> = view
        .items
        .iter()
        .map(|i| i["code"].as_str().unwrap_or(""))
        .collect();
    // Both rows FAIL, each with its own diagnostic: the class mismatch on
    // the live instance (E9002), the missing target (E9001) — the zips
    // matched each row to its own diagnostic, in ledger order.
    assert_eq!(codes, vec!["E9002", "E9001"]);
    assert_eq!(expectation_counts(&view.items)["fail"].as_u64(), Some(2));
}
