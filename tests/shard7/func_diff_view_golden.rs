// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Golden baseline for the `diff` functional projection (CIMP §1 U280,
//! sixth slice).
//!
//! The view's serialized payload is an exact-byte snapshot stored at
//! `tests/golden/func_diff_view.expected.json`. Any change that adds,
//! removes, renames or re-orders a shared key — or touches the envelope's
//! stable fields — fails the test on purpose, so the change is a reviewed
//! contract change rather than a silent drift.
//!
//! The three identity fields that move for reasons outside the contract are
//! normalized before comparison: `world_ver` / `top_ver` (a hashing change
//! must not masquerade as a payload change) and `mcc_version` (otherwise
//! every BUILDNr bump would rewrite the golden).
//!
//! One case per §6.1 judgment — `replace`, `diverge`, `defer`, `none` — so
//! all four type words are pinned by bytes.
//!
//! Regenerate (review the diff — a changed verdict means a deliberate
//! contract change): `FUNC_DIFF_VIEW_UPDATE=1 cargo test --test shard7
//! func_diff_view_golden`.

use crate::common;

use mcc::stages::funcdiff::{
    functional_change, functional_view, side_verdicts,
};
use mcc::stages::payload::StageViewData;

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

/// Same shared keys, implementation B delivers 3.4V — inside the window.
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

/// Same shared keys, implementation B delivers 3.6V — outside the window.
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

/// `partno` names nothing: both sides DEFER the same conditional key.
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

/// The B side of the disjoint pair: one driven expectation only.
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

/// One case: a name, and the two worlds compared under it.
struct Case {
    name: &'static str,
    a: &'static str,
    b: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "replace",
        a: A_33,
        b: B_34,
    },
    Case {
        name: "diverge",
        a: A_33,
        b: B_36,
    },
    Case {
        name: "defer",
        a: DEFERRED,
        b: DEFERRED,
    },
    Case {
        name: "none",
        a: NONE_A,
        b: NONE_B,
    },
];

/// Run one side through the acceptance engine.
fn side(src: &str, uri_path: &str) -> mcc::stages::funcdiff::SideVerdicts {
    let _guard = common::lock();
    common::reset();
    let uri: mcc::McURI = uri_path.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2 flat runs");
    let (_, module) = mcc::definition_space()
        .workspace_modules()
        .into_iter()
        .find(|(sn, _)| sn.ident.to_string() == "main")
        .expect("module 'main' registered");
    let report = mcc::check::expectation::run(&table, &module.expects, &module.uri);
    side_verdicts(&module.expects, &report)
}

/// Build one case in fresh worlds and return the normalized payload.
fn build_case(c: &Case) -> StageViewData {
    let a = side(c.a, "/mcc/func-diff-golden-a.mc");
    let b = side(c.b, "/mcc/func-diff-golden-b.mc");
    let change = functional_change(&a, &b);
    let view = functional_view(c.name, c.name, &change);
    let mut payload = StageViewData::from(&view);
    payload.world_ver = None;
    payload.top_ver = None;
    payload.mcc_version = "0.0.0-test".to_string();
    payload
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/func_diff_view.expected.json")
}

#[test]
fn func_diff_view__golden_matches_baseline() {
    // No outer lock here: [`side`] takes [`common::lock`] itself, and this
    // std mutex is not re-entrant — an outer guard would self-deadlock the
    // first `side` call. The steps between the `side` calls
    // (`functional_change` / `functional_view` / the normalization) touch no
    // shared world state, so per-call locking is the whole serialization.
    let update = std::env::var("FUNC_DIFF_VIEW_UPDATE").is_ok();

    let mut actual = serde_json::Map::new();
    for c in CASES {
        let payload = build_case(c);
        let v = serde_json::to_value(&payload).expect("StageViewData serializes");
        actual.insert(c.name.to_string(), v);
    }
    let actual = serde_json::Value::Object(actual);

    let path = golden_path();
    if update {
        let mut s = serde_json::to_string_pretty(&actual).expect("serialize golden");
        s.push('\n');
        std::fs::write(&path, s).expect("write golden");
        return;
    }
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden — run `FUNC_DIFF_VIEW_UPDATE=1 cargo test --test shard7 \
             func_diff_view_golden` to (re)generate: {e}"
        )
    });
    let expected: serde_json::Value = serde_json::from_str(&raw).expect("parse golden");
    assert_eq!(
        actual,
        expected,
        "the `diff` functional payload drifted from its golden baseline; if the \
         change is deliberate, re-run with FUNC_DIFF_VIEW_UPDATE=1 and review the diff"
    );
}
