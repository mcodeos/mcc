// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Golden baseline for the `core-erc` projection (CIMP §1 U280, second half
//! fourth slice).
//!
//! The view's serialized payload is an exact-byte snapshot stored at
//! `tests/golden/core_erc_view.expected.json`. Any change that adds, removes,
//! renames or re-orders a net or a finding — or touches the envelope's
//! stable fields — fails the test on purpose, so the change is a reviewed
//! contract change rather than a silent drift.
//!
//! The three identity fields that move for reasons outside the contract are
//! normalized before comparison: `world_ver` / `top_ver` (a hashing change
//! must not masquerade as a payload change) and `mcc_version` (otherwise
//! every BUILDNr bump would rewrite the golden).
//!
//! Regenerate (review the diff — a new finding means a deliberate contract
//! change): `CORE_ERC_VIEW_UPDATE=1 cargo test --test shard7 core_erc_view_golden`.

use crate::common;

use mcc::stages::corercview::{self, CoreErcSnapshot, Finding};
use mcc::stages::diagview::{DiagLoc, Level};
use mcc::stages::payload::StageViewData;

struct Case {
    name: &'static str,
    src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "wired",
        // Two resistors in series off a supply pin: the snapshot's `nets`
        // half carries the islands the netlist view names, and no flat check
        // fires — the empty findings array is part of the contract (the
        // counts still name every word).
        src: "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    R r2;\n    func main() {\n        r1.1 -> VDD\n        r1.2 -> r2.1\n        r2.2 -> VDD\n    }\n}",
    },
    Case {
        name: "bare",
        // Instances declared but never wired: the unwired-instance check
        // fires once per instance, and its `net` member names the *instance
        // path* — the parallel-array spelling: the join to `nets` is the
        // consumer's, never the wire's.
        src: "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    R r1;\n    R r2;\n}",
    },
];

/// Build one case in a fresh workspace and return the normalized payload.
/// Entry resolution mirrors `mcc check` (`check_one_world`); the full-fidelity
/// flat run is the one that hands over the net-check results — the pieces
/// `into_parts` would otherwise drop with the `DianLu` that holds them.
fn build_case(c: &Case) -> StageViewData {
    common::reset();
    let uri = format!("/mcc/core-erc-view-{}.mc", c.name);
    mcc::mcc_load_from_string(&uri, c.src);
    let mod_name = mcc::mcb_get_module_name_by_uri(&uri)
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| "main".to_string());
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(mod_name.as_str()),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table, _arena, _store, _diags, net_results) =
        mcc::mcb_pass2_flat_with(&entry, 1, None).expect("flat pass2 runs");
    let view = corercview::core_erc_view(&mod_name, &table, &net_results);

    let mut payload = StageViewData::from(&view);
    payload.world_ver = None;
    payload.top_ver = None;
    payload.mcc_version = "0.0.0-test".to_string();
    payload
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/core_erc_view.expected.json")
}

#[test]
fn core_erc_view__golden_matches_baseline() {
    let _g = common::lock();
    let update = std::env::var("CORE_ERC_VIEW_UPDATE").is_ok();

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
            "missing golden — run `CORE_ERC_VIEW_UPDATE=1 cargo test --test shard7 \
             core_erc_view_golden` to (re)generate: {e}"
        )
    });
    let expected: serde_json::Value = serde_json::from_str(&raw).expect("parse golden");
    assert_eq!(
        actual, expected,
        "the core-erc view's bytes drifted — inspect tests/golden/core_erc_view.expected.json; \
         if the change is deliberate, regenerate with CORE_ERC_VIEW_UPDATE=1"
    );
}

/// The reserved `loc` side table: until a producer for the net side table
/// exists the view serializes none — the member is exercised here by
/// construction, the same discipline `diagview`'s reserved members follow.
#[test]
fn core_erc_view__the_loc_side_table_is_reserved_but_writable() {
    let snapshot = CoreErcSnapshot {
        nets: Vec::new(),
        findings: Vec::new(),
        loc: Some([(
            "VDD".to_string(),
            DiagLoc {
                uri: "file://x.mc".to_string(),
                line: 4,
                span: Some(3),
            },
        )]
        .into_iter()
        .collect()),
    };
    let v = serde_json::to_value(&snapshot).expect("snapshot serializes");
    assert_eq!(v["loc"]["VDD"]["line"], 4);
}

/// The finding round-trips under the CDDL member names — the member-set
/// guard in view_vocabulary holds the set equal; this pins the values.
#[test]
fn core_erc_view__finding_round_trips_under_the_cddl_member_names() {
    let f = Finding {
        code: "E4112".to_string(),
        level: Level::Warning,
        check: "unwired-instance".to_string(),
        msg: "Instance 'r1' has no pins connected to any net.".to_string(),
        net: "r1".to_string(),
        loc: DiagLoc {
            uri: "file://x.mc".to_string(),
            line: 5,
            span: None,
        },
        fix_hint: None,
    };
    let v = serde_json::to_value(&f).expect("Finding serializes");
    assert_eq!(v["code"], "E4112");
    assert_eq!(v["check"], "unwired-instance");
    assert_eq!(v["net"], "r1");
    assert!(v.get("fix_hint").is_none(), "the absent member is omitted");
}
