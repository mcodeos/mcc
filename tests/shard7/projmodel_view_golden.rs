// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Golden baseline for the `project-model` projection (CIMP §1 U280, second
//! half third slice).
//!
//! The view's serialized payload is an exact-byte snapshot stored at
//! `tests/golden/project_model_view.expected.json`. Any change that adds,
//! removes, renames or re-orders a node, parameter or port row — or touches
//! the envelope's stable fields — fails the test on purpose, so the change is
//! a reviewed contract change rather than a silent drift.
//!
//! The three identity fields that move for reasons outside the contract are
//! normalized before comparison: `world_ver` / `top_ver` (a hashing change
//! must not masquerade as a payload change) and `mcc_version` (otherwise
//! every BUILDNr bump would rewrite the golden).
//!
//! Regenerate (review the diff — a new node means a deliberate contract
//! change): `PROJECT_MODEL_VIEW_UPDATE=1 cargo test --test shard7 projmodel_view_golden`.

use crate::common;

use mcc::stages::payload::StageViewData;
use mcc::stages::projmodel;
use mcc::stages::read::Loaded;

struct Case {
    name: &'static str,
    /// The entry module the flat build instantiates. Named explicitly: the
    /// workspace's own fallback (`mcb_get_first_module_name`) resolves by the
    /// registry's `(uri, ident)` order, and this file's subject is the view
    /// builder, not that resolution.
    top: &'static str,
    src: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "nested",
        // A submodule instance with a port binding, and a component created
        // inside the submodule's func — the tree walks deeper than one level
        // and the submodule node resolves its instance content from the store.
        top: "main",
        src: "module ZREG(in VIN) {\n    func Add(net) {\n        CAP(1).Cap([net, VIN])\n    }\n}\ncomponent CAP(cap::INT = 1) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\nmodule main {\n    io VDD\n    io GND\n    ZREG ldo(VDD)\n    ldo.Add(GND)\n}",
    },
    Case {
        name: "defaults",
        // One instance takes its declaration default, one binds its own
        // value (the class header's parameter list is the formal table); the
        // net binding reads through the same island map the netlist view
        // publishes.
        top: "main",
        src: "component R (res::INT = 10) {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r1;\n    R r2(res = 22);\n    func main() {\n        r1.1 -> VDD\n        r1.2 -> r2.1\n        r2.2 -> VDD\n    }\n}",
    },
    Case {
        name: "vector",
        // A declared vector grouping: the grouping node reads as a node with
        // an empty definition site instead of vanishing, beside the members
        // it groups.
        top: "main",
        src: "component R {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\nmodule main {\n    io VDD\n    R r[1:2];\n    func main() {\n        r.1 -> VDD\n    }\n}",
    },
];

/// Build one case in a fresh workspace and return the normalized payload.
/// The full-fidelity flat build is the same workspace-grounded read the CLI
/// and RPC faces take — the tree walk needs the arena + instance store the
/// plain flat entry throws away.
fn build_case(c: &Case) -> StageViewData {
    common::reset();
    let uri = format!("/mcc/project-model-{}.mc", c.name);
    mcc::mcc_load_from_string(&uri, c.src);
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(c.top),
        uri: mcc::uri_intern(&uri),
    };
    let (tree, table, arena, store, diags) =
        mcc::mcb_pass2_flat_with(&entry, 1, None).expect("flat pass2 runs");
    let loaded = Loaded::new(tree, table, arena, store, c.top, diags.len());
    let view = projmodel::project_model_view(&loaded);

    let mut payload = StageViewData::from(&view);
    payload.world_ver = None;
    payload.top_ver = None;
    payload.mcc_version = "0.0.0-test".to_string();
    payload
}

fn golden_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/project_model_view.expected.json")
}

#[test]
fn project_model_view__golden_matches_baseline() {
    let _g = common::lock();
    let update = std::env::var("PROJECT_MODEL_VIEW_UPDATE").is_ok();

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
            "missing golden — run `PROJECT_MODEL_VIEW_UPDATE=1 cargo test --test shard7 \
             projmodel_view_golden` to (re)generate: {e}"
        )
    });
    let expected: serde_json::Value = serde_json::from_str(&raw).expect("parse golden");
    assert_eq!(
        actual, expected,
        "the project-model view's bytes drifted — inspect \
         tests/golden/project_model_view.expected.json; if the change is deliberate, \
         regenerate with PROJECT_MODEL_VIEW_UPDATE=1"
    );
}

/// The whole read is total: every node the arena holds surfaces in the items,
/// and the counts agree with the walk — a vector grouping node included, a
/// port only as its module's port row.
#[test]
fn project_model_view__counts_match_the_walk() {
    use mcc::stages::projmodel::project_model_counts;

    let _g = common::lock();
    for c in CASES {
        let payload = build_case(c);
        let items = payload.items.as_array().cloned().unwrap_or_default();
        let counts = project_model_counts(&items);
        let walk = |items: &[serde_json::Value], ports: &mut usize| -> usize {
            let mut nodes = 0;
            let mut stack: Vec<&serde_json::Value> = items.iter().collect();
            while let Some(item) = stack.pop() {
                nodes += 1;
                *ports += item["ports"].as_array().map(|a| a.len()).unwrap_or(0);
                if let Some(children) = item["children"].as_array() {
                    stack.extend(children);
                }
            }
            nodes
        };
        let mut ports = 0;
        let nodes = walk(&items, &mut ports);
        assert_eq!(counts["nodes"], nodes, "case {}: node count drifted", c.name);
        assert_eq!(counts["ports"], ports, "case {}: port count drifted", c.name);
    }
}


