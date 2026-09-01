// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §11.2 / vector-pipeline Phase 1.4: declaration-side `McVectorInst`
//! materialization.
//!
//! A declared vector (`c[1:2]`) materializes its flat member instances into
//! `McModuleInst.components` (existing consumption paths unchanged) AND a
//! grouping node into `McModuleInst.vectors` — the modeling-layer coordinate
//! for the ordered member set. Contract E: single-member ranges
//! (`c[2]`) stay scalar and produce NO vector node.
//!
//! Member names are the member_set product (strict written order); member_ids
//! are the physical instance coordinates resolving against `components`
//! (module-level bare `c1`, func-prefixed `s1.r1`).

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Build `main` and return the module instance plus the Phase D frozen string
/// net-table store (the tree never carries `NetPoint`).
fn build_main(src: &str, uri: &str) -> (mcc::McModuleInst, mcc::NetTableStore) {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build")
}

/// Build `main` and flatten to the InstTable (§11.1 projection view).
fn build_flat(src: &str) -> mcc::InstTable {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri: McURI = "/mcc/vinst-flat.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    table
}

/// Entry `vector_info` for a flat path, if any.
fn vector_info_of(table: &mcc::InstTable, path: &str) -> Option<mcc::VectorMemberInfo> {
    table
        .iter()
        .find(|(_, e)| e.path == path)
        .and_then(|(_, e)| e.vector_info.clone())
}

fn find_vector<'a>(inst: &'a mcc::McModuleInst, base: &str) -> &'a mcc::McVectorInst {
    inst.vectors
        .iter()
        .find(|v| v.base == base)
        .unwrap_or_else(|| {
            panic!(
                "no McVectorInst for base '{base}'; vectors={:?}",
                inst.vectors
            )
        })
}

/// ── Module-body vector declare: grouping node + flat member instances ─────
/// `CAP c[1:2](1)` in the module body → `main.vectors` holds
/// `{ base: "c", members: ["c1","c2"], ids: ["c1","c2"] }` and `main.components`
/// holds c1, c2 (member-set written order).
#[test]
fn module_body_vector_declare_materializes_group() {
    let src = format!("{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n}}\n");
    let (inst, _) = build_main(&src, "/mcc/vinst-module-body.mc");
    let v = find_vector(&inst, "c");
    assert_eq!(v.member_names, vec!["c1", "c2"], "ordered member set");
    assert_eq!(v.member_ids, vec!["c1", "c2"], "module-level physical ids");
    assert!(v.shape.is_none(), "no 2D shape for 1D vector");
    for m in ["c1", "c2"] {
        assert!(
            inst.components.iter().any(|c| c.name == m),
            "member '{m}' materialized as component"
        );
    }
    assert_eq!(
        inst.vectors.len(),
        1,
        "exactly one vector group; got {:?}",
        inst.vectors
    );
}

/// ── Func-local vector declare (module func auto-invoke): same group ───────
/// `CAP c[1:2](1)` inside `func M()` → the func's standalone declarations are
/// materialized at module level (empty prefix), so the vector group lands on
/// the module with bare member ids. A declare-only func is never auto-invoked
/// (it has no body stmts), so the func must also use the members.
#[test]
fn func_local_vector_declare_materializes_group() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    func M() {{\n        CAP c[1:2](1)\n        c[1:2].Cap([VDD, GND])\n    }}\n}}\n"
    );
    let (inst, _) = build_main(&src, "/mcc/vinst-func-local.mc");
    let v = find_vector(&inst, "c");
    assert_eq!(v.member_names, vec!["c1", "c2"]);
    assert_eq!(v.member_ids, vec!["c1", "c2"]);
    for m in ["c1", "c2"] {
        assert!(inst.components.iter().any(|c| c.name == m));
    }
}

/// ── Name-first declare form (`c[1:2]::CAP(1)`) also registers the group ───
#[test]
fn name_first_vector_declare_materializes_group() {
    let src = format!("{CAP_COMP}module main {{\n    io VDD\n    io GND\n    c[1:2]::CAP(1)\n}}\n");
    let (inst, _) = build_main(&src, "/mcc/vinst-name-first.mc");
    let v = find_vector(&inst, "c");
    assert_eq!(v.member_names, vec!["c1", "c2"]);
    assert_eq!(v.member_ids, vec!["c1", "c2"]);
}

/// ── Contract E: single-member range stays scalar, no vector node ─────────
/// `CAP c[2](1)` → member `c2` materializes as a plain scalar component; the
/// `>= 2` guard at pass1 registration means no `vectors` entry at all.
#[test]
fn single_member_range_stays_scalar() {
    let src = format!("{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[2](1)\n}}\n");
    let (inst, _) = build_main(&src, "/mcc/vinst-single.mc");
    assert!(
        inst.vectors.iter().all(|v| v.base != "c"),
        "no vector group for single-member range; vectors={:?}",
        inst.vectors
    );
    assert!(
        inst.components.iter().any(|c| c.name == "c2"),
        "member c2 materialized as scalar component"
    );
    assert!(
        !inst.components.iter().any(|c| c.name == "c1"),
        "no c1 for c[2]"
    );
}

/// ── §11.3/1.6: single-index member reference stays scalar ────────────────
/// `res[1:2]::RES(0)` declared, then `res[2] -> GND`: the single-index
/// reference is a scalar member (contract E), NOT a re-link of the whole
/// vector group. Phase 1.6 removed the sibling-probing heuristic (which
/// scanned base+digit siblings) in favor of direct vector-node lookup; this
/// locks that a scalar member reference connects only itself.
#[test]
fn single_index_member_reference_stays_scalar() {
    let res_comp = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";
    let src = format!(
        "{res_comp}module main {{\n    io VDD\n    io GND\n    res[1:2]::RES(0)\n    res[2] -> GND\n}}\n"
    );
    let (inst, net_store) = build_main(&src, "/mcc/vinst-single-index.mc");
    let v = find_vector(&inst, "res");
    assert_eq!(v.member_names, vec!["res1", "res2"]);
    // The `res[2]` operand connects only res2's pin — no broadcast to res1.
    let gnd_net = net_store
        .get(&inst.name.to_string())
        .unwrap_or_default()
        .iter()
        .find(|(n, _)| n.starts_with("GND"))
        .cloned()
        .unwrap_or_else(|| panic!("no GND net"));
    let paths: Vec<String> = gnd_net.1.iter().map(|p| p.path.clone()).collect();
    assert!(
        paths.iter().any(|p| p.contains("res2.2")),
        "res2 pin 2 on GND net; got {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p.contains("res1.")),
        "res1 must NOT be broadcast onto the GND net; got {paths:?}"
    );
}

/// ── Sub-module vector declare: group lives on the sub-module instance ─────
/// `SM` declared in main with `CAP c[1:2](1)` in SM's body → `main.sub_modules`
/// holds an SM instance whose own `vectors` has the group (module-scope
/// isolation of vector bases, §11.2).
#[test]
fn submodule_vector_declare_materializes_group() {
    let src = format!(
        "{CAP_COMP}module SM {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n}}\nmodule main {{\n    io VDD\n    io GND\n    SM s1()\n}}\n"
    );
    let (inst, _) = build_main(&src, "/mcc/vinst-submodule.mc");
    let sm = inst
        .sub_modules
        .iter()
        .find(|m| m.name == "s1")
        .unwrap_or_else(|| panic!("sub-module s1 not found; got {:?}", inst.sub_modules));
    let v = find_vector(sm, "c");
    assert_eq!(v.member_names, vec!["c1", "c2"]);
    assert_eq!(v.member_ids, vec!["c1", "c2"]);
}

/// ── §11.1 flatten projection: vector members carry `vector_info` ──────────
/// flatten projects `c[1:2]` to per-member flat entries `main.c1` / `main.c2`
/// (invariant B — no literal `c[1:2]` path), each carrying the vector-group
/// projection `{ vector_base: "c", member, index }`. A scalar sibling has
/// `vector_info: None`.
#[test]
fn flatten_projects_vector_members_with_vector_info() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n    CAP solo(1)\n}}\n"
    );
    let table = build_flat(&src);

    let c1 = vector_info_of(&table, "main.c1").expect("main.c1 has vector_info");
    assert_eq!(c1.vector_base, "c");
    assert_eq!(c1.member, "c1");
    assert_eq!(c1.index, 0);

    let c2 = vector_info_of(&table, "main.c2").expect("main.c2 has vector_info");
    assert_eq!(c2.vector_base, "c");
    assert_eq!(c2.member, "c2");
    assert_eq!(c2.index, 1);

    // Scalar sibling: no vector projection.
    assert!(
        vector_info_of(&table, "main.solo").is_none(),
        "scalar component carries no vector_info"
    );

    // Invariant B: no literal `c[1:2]` path anywhere in the flat table.
    for (_, e) in table.iter() {
        assert!(
            !e.path.contains("[1:2]"),
            "invariant B violated: literal vector path '{}'",
            e.path
        );
    }
}

/// ── §11.1 reverse index: `vector_member_paths(base)` returns member paths ──
/// The base → member entry paths reverse query resolves `c` to the two flat
/// member paths in member order, so an LSP-style `c[1:2]` lookup can map the
/// vector group onto its flat entries without a new path format.
#[test]
fn vector_member_paths_reverse_queries_member_entries() {
    let src = format!("{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n}}\n");
    let table = build_flat(&src);
    assert_eq!(
        table.vector_member_paths("c"),
        vec!["main.c1", "main.c2"],
        "reverse query returns member paths in member order"
    );
    assert!(
        table.vector_member_paths("nope").is_empty(),
        "unknown base yields no paths"
    );
}

/// ── §11.1 sub-module vector: projection attaches to the member entries ────
/// `SM.c[1:2]` in a sub-module flattens to `main.s1.c1` / `main.s1.c2`, both
/// carrying the group projection (module-scope isolation of vector bases).
#[test]
fn flatten_projects_submodule_vector_members() {
    let src = format!(
        "{CAP_COMP}module SM {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n}}\nmodule main {{\n    io VDD\n    io GND\n    SM s1()\n}}\n"
    );
    let table = build_flat(&src);
    let c1 = vector_info_of(&table, "main.s1.c1").expect("main.s1.c1 has vector_info");
    assert_eq!((c1.vector_base.as_str(), c1.member.as_str()), ("c", "c1"));
    assert_eq!(c1.index, 0);
    let c2 = vector_info_of(&table, "main.s1.c2").expect("main.s1.c2 has vector_info");
    assert_eq!((c2.vector_base.as_str(), c2.member.as_str()), ("c", "c2"));
    assert_eq!(c2.index, 1);
    assert_eq!(
        table.vector_member_paths("c"),
        vec!["main.s1.c1", "main.s1.c2"]
    );
}
