// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P2 · supply-bundle model structural assertions on a real project (hbl1).
//!
//! `supply_bundle` decides the trunk bundles, their declared driver and the tap
//! landing points for the root block diagram. The unit tests in `supply_bundle.rs`
//! drive the geometry with hand-built edges; this file pins the bundle model on
//! the real hbl1 root layer — which edges join which trunk, who the declared
//! driver is, and that every consumer lands one tap. A change in the model (a new
//! member, a different driver, a lost tap) is a visible regression here before it
//! ever reaches the drawing.
//!
//! Members live on the `SupplyGroups` from `plan_groups` (edge indices), geometry
//! on the `SupplyBundlePlan` from `build_plan_for` — both are exercised here in
//! the same order the render path uses them. Layout is not run (boxes stay at the
//! origin), so the assertions are structural; real coordinates are covered by the
//! render/golden path.

mod common;

use std::path::{Path, PathBuf};

use mcc::vector::builder::build_mc_vec_with_arena;
use mcc::vector::graph::{build_mc_vec_graph, McVecGraph};
use mcc::viz::layout::edge_decide::{decide_edges, EdgeKind};
use mcc::viz::layout::supply_bundle::{build_plan_for, plan_groups, SupplyBundlePlan, SupplyGroups};
use mcc::{
    mcc_build_flat_with_arena, mcc_init, mcc_load_project, mcc_set_project_root,
    mcc_set_system_root, McIds,
};

fn entry_uri(project_root: &Path, module_name: &str) -> String {
    let target = format!("{}.mc", module_name);
    let mut first: Option<String> = None;
    for dir in [project_root.to_path_buf(), project_root.join("src")] {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) != Some("mc") {
                    continue;
                }
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if name == target.to_lowercase() {
                    return std::fs::canonicalize(&p)
                        .ok()
                        .and_then(|p| p.to_str().map(str::to_string))
                        .expect("canonical entry");
                }
                if first.is_none() {
                    first = std::fs::canonicalize(&p)
                        .ok()
                        .and_then(|p| p.to_str().map(str::to_string));
                }
            }
        }
        if first.is_some() {
            return first.unwrap();
        }
    }
    panic!("no .mc entry under {}", project_root.display());
}

/// Build hbl1's root layer graph and run the supply-bundle plan over it, in the
/// same order the render path does (edge decision first, then the grouping and
/// the plan — the renderer never re-derives any of it).
fn hbl1_plan() -> (McVecGraph, SupplyGroups, SupplyBundlePlan) {
    let root = PathBuf::from(std::env::var("MCC_GOLDEN_PROJECT").unwrap_or_else(|_| "mcs/hbl1".into()));
    let project = root.as_path();
    mcc_set_system_root(project);
    mcc_set_project_root(project);
    mcc_init();
    let entry = entry_uri(project, "hbl");
    mcc_load_project(&entry);
    let ident = McIds::from("main");
    let (inst, table, arena, store) =
        mcc_build_flat_with_arena(&ident, &entry, 1000).expect("flat");
    let vec_block = build_mc_vec_with_arena(&inst, &table, &arena, &store);
    let mut graph = build_mc_vec_graph(&vec_block, &table);
    graph.block_edges = decide_edges(&graph).0;
    let groups = plan_groups(&graph.block_edges);
    let plan = build_plan_for(&graph, &graph.block_edges);
    (graph, groups, plan)
}

fn box_id(graph: &McVecGraph, name: &str) -> i64 {
    graph
        .boxes
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("{} box", name))
        .id
}

/// The root layer's power bundles on hbl1: one fan-out trunk (V3V3) whose
/// declared driver is the LDO module, plus two point-to-point power edges (the
/// socket's 5V feed into the LDO, and the DCDC's 1.2V into the MCU). Every
/// trunk member lands exactly one tap, and the ret-lineage geometry rides the
/// same plan.
#[test]
fn hbl1_bundle_structure_and_driver() {
    let _lock = common::lock();
    let (graph, groups, plan) = hbl1_plan();

    let ldo = box_id(&graph, "modldo");
    let socket = box_id(&graph, "usbsocket");
    let dcdc = box_id(&graph, "moddcdc");
    let mcu = box_id(&graph, "mcu513");

    // ── Trunks: exactly one, V3V3, fed by the LDO module ──
    assert_eq!(groups.trunks.len(), 1, "hbl1 root draws one trunk");
    assert_eq!(groups.trunks[0].label, "V3V3");
    assert_eq!(plan.trunks.len(), 1, "one trunk reaches the plan");
    let trunk = &plan.trunks[0];
    assert_eq!(trunk.label, "V3V3");

    // Every trunk member is a Power edge out of the LDO module (the declared
    // driver, not a frequency vote).
    for &idx in &groups.trunks[0].members {
        let e = &graph.block_edges[idx];
        assert_eq!(e.kind, EdgeKind::Power, "trunk member kind");
        assert_eq!(e.driver_box, Some(ldo), "declared driver is the LDO module");
        assert_eq!(e.from_box, ldo, "driver edge leaves the LDO module");
    }

    // The driver end resolves to an anchor on the LDO, and every consumer
    // lands one tap (tap count == member count).
    assert!(trunk.driver.is_some(), "trunk resolves a driver anchor");
    assert_eq!(trunk.taps.len(), groups.trunks[0].members.len());

    // ── Individual power edges: the two point-to-point supplies ──
    // The socket's 5V edge is drawn with its structured trunk name
    // "usbsocket": the net's first connection is instance-first, so the
    // declared-trunk name is the driver instance (P0's written-pair rule only
    // fires for an explicit `[hot, ret]` pair, not the scalar `V5V::DC(5V)`
    // form). The driver identity is the socket itself — only the label is
    // instance-derived.
    let indiv_power: Vec<&str> = plan
        .individual
        .iter()
        .filter(|draw| draw.kind == EdgeKind::Power)
        .map(|draw| draw.label.as_str())
        .collect();
    assert_eq!(indiv_power, vec!["usbsocket", "V1V2"], "the two point-to-point supplies");
    let v5v = graph
        .block_edges
        .iter()
        .find(|e| e.label == "usbsocket")
        .expect("socket 5V edge");
    assert_eq!(
        (v5v.from_box, v5v.to_box, v5v.driver_box),
        (socket, ldo, Some(socket)),
        "socket feeds the LDO module"
    );
    let v1v2 = graph
        .block_edges
        .iter()
        .find(|e| e.label == "V1V2")
        .expect("V1V2 edge");
    assert_eq!(
        (v1v2.from_box, v1v2.to_box, v1v2.driver_box),
        (dcdc, mcu, Some(dcdc)),
        "DCDC feeds the MCU"
    );

    // ── Ret lineage rides the same plan (P3 integration) ──
    assert!(trunk.ret_stub.is_some(), "V3V3 fan-out carries the driver return stub");
    for draw in plan.individual.iter().filter(|d| d.kind == EdgeKind::Power) {
        assert!(draw.ret_lane.is_some(), "'{}' point-to-point power edge carries a ret lane", draw.label);
    }
}

/// Building the same project twice yields the same bundle model (same trunk
/// order, same members, same driver anchor) — the plan is deterministic.
#[test]
fn hbl1_bundle_plan_is_deterministic() {
    let _lock = common::lock();
    let (g1, gr1, p1) = hbl1_plan();
    let (g2, gr2, p2) = hbl1_plan();

    let key = |g: &McVecGraph, gr: &SupplyGroups| -> Vec<(String, Vec<i64>)> {
        gr.trunks
            .iter()
            .map(|t| {
                (
                    t.label.clone(),
                    t.members.iter().map(|&i| g.block_edges[i].from_box).collect(),
                )
            })
            .collect()
    };
    assert_eq!(key(&g1, &gr1), key(&g2, &gr2), "trunk order and members");
    assert_eq!(gr1.trunks[0].members, gr2.trunks[0].members, "member indices");
    assert_eq!(p1.trunks[0].driver, p2.trunks[0].driver, "driver anchor");
}
