// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Golden baseline for the `netlist` projection (CIMP §1 U280, second half
//! second slice).
//!
//! The view's serialized payload is an exact-byte snapshot stored at
//! `tests/golden/netlist_view.expected.json`. Any change that adds, removes,
//! renames or re-orders an island — or touches the envelope's stable fields —
//! fails the test on purpose, so the change is a reviewed contract change
//! rather than a silent drift.
//!
//! The three identity fields that move for reasons outside the contract are
//! normalized before comparison: `world_ver` / `top_ver` (a hashing change
//! must not masquerade as a payload change) and `mcc_version` (otherwise
//! every BUILDNr bump would rewrite the golden).
//!
//! Regenerate (review the diff — a new island means a deliberate contract
//! change): `NETLIST_VIEW_UPDATE=1 cargo test --test shard7 netlist_view_golden`.

use crate::common;

use mcc::stages::netlistview::{self, NetItem};
use mcc::stages::payload::StageViewData;

struct Case {
    name: &'static str,
    src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "wired",
        // Two resistors in series off a supply pin: a junction island between
        // them and the supply island at the pin — more than one island, each
        // with members from both instances.
        src: "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    R r2;\n    func main() {\n        r1.1 -> VDD\n        r1.2 -> r2.1\n        r2.2 -> VDD\n    }\n}",
    },
    Case {
        name: "bare",
        // Instances declared but never wired: no island carries their pins —
        // the empty reading is itself part of the contract (the counts still
        // name every word).
        src: "component R {\n    pins = [\n        1 = A\n    ]\n}\nmodule main {\n    R r1;\n    R r2;\n}",
    },
];

/// Build one case in a fresh workspace and return the normalized payload.
/// Entry resolution mirrors `mcc check` (`check_one_world`); the flat pass2
/// run is the same one the export face reads the table from.
fn build_case(c: &Case) -> StageViewData {
    common::reset();
    let uri = format!("/mcc/netlist-view-{}.mc", c.name);
    mcc::mcc_load_from_string(&uri, c.src);
    let mod_name = mcc::mcb_get_module_name_by_uri(&uri)
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| "main".to_string());
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(mod_name.as_str()),
        uri: mcc::uri_intern(&uri),
    };
    let (_tree, table) = mcc::mcb_pass2_flat(&entry, 1).expect("flat pass2 runs");
    let view = netlistview::netlist_view(&mod_name, &table);

    let mut payload = StageViewData::from(&view);
    payload.world_ver = None;
    payload.top_ver = None;
    payload.mcc_version = "0.0.0-test".to_string();
    payload
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/netlist_view.expected.json")
}

#[test]
fn netlist_view__golden_matches_baseline() {
    let _g = common::lock();
    let update = std::env::var("NETLIST_VIEW_UPDATE").is_ok();

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
            "missing golden — run `NETLIST_VIEW_UPDATE=1 cargo test --test shard7 \
             netlist_view_golden` to (re)generate: {e}"
        )
    });
    let expected: serde_json::Value = serde_json::from_str(&raw).expect("parse golden");
    assert_eq!(
        actual, expected,
        "the netlist view's bytes drifted — inspect tests/golden/netlist_view.expected.json; \
         if the change is deliberate, regenerate with NETLIST_VIEW_UPDATE=1"
    );
}

/// The item round-trips under the CDDL member names — the member-set guard
/// in view_vocabulary holds the set equal; this pins the values. (The
/// exclusion branch itself is pinned where the predicate lives, in
/// `src/export/netlist.rs`.)
#[test]
fn netlist_view__item_round_trips_under_the_cddl_member_names() {
    let _g = common::lock();
    let item = NetItem {
        name: "VDD".to_string(),
        points: vec!["r1.1".to_string()],
    };
    let v = serde_json::to_value(&item).expect("NetItem serializes");
    assert_eq!(v["name"], "VDD");
    assert_eq!(v["points"][0], "r1.1");
}
