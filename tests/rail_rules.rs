// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-3 · Rail trichotomy acceptance test (MC_SCHEMATIC_ROADMAP_v6 P7-3 acceptance checklist)
//!
//! - main layer: GND edges = 0, rail flag boxes = 0, driver stage edges = 4,
//!   matching the §1.2 seven-line checklist item by item (edge-table assertions live in tests/renderdiff.rs).
//! - main layer `compute_isolated_ids` returns the empty set (usbsocket/modldo/moddcdc are no longer islands).
//! - Sub-layers: every GND endpoint has exactly 1 ground symbol (S1),
//!   every non-GND rail endpoint has exactly 1 rail dot (S2).
//!
//! Criteria are asserted by **box name/net name** (ids are unstable across processes).

use std::collections::HashSet;
use std::path::PathBuf;

use mcc::viz::api::{render_with_metrics, RenderOpts};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

/// The mcc_* workspace is global state; tests must be serialized (same as tests/renderdiff.rs)
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_graph() -> mcc::vector::graph::McVecGraph {
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    let mcode_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode");
    mcc::mcc_set_system_root(mcode_dir.as_path());
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    mcc::mcb_load_lib("mcode", mcode_dir.as_path());
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table)
}

#[test]
fn main_layer_isolated_set_is_empty() {
    // Acceptance: driver stage edges bring usbsocket/modldo/moddcdc into the main flow;
    // compute_isolated_ids(main, hub) must return the empty set.
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut graph = build_graph();
    mcc::vector::graph::apply_promote_recursive(&mut graph);
    // Mirror pipeline: classify_rails runs before island computation (flow.rs phase_prepare → phase_placement)
    mcc::viz::layout::rails::classify_rails(&mut graph, /*is_top=*/ true);
    // hub = the box with the highest signal degree (main layer = mcu513)
    let mut degree: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for n in &graph.nets {
        let mut boxes: Vec<i64> = Vec::new();
        for e in &n.endpoints {
            if !boxes.contains(&e.box_id) {
                boxes.push(e.box_id);
            }
        }
        if boxes.len() >= 2 {
            for b in boxes {
                *degree.entry(b).or_insert(0) += 1;
            }
        }
    }
    let hub = *degree
        .iter()
        .max_by_key(|(id, d)| (**d, std::cmp::Reverse(**id)))
        .map(|(id, _)| id)
        .expect("hub");
    let hub_name = graph
        .boxes
        .iter()
        .find(|b| b.id == hub)
        .map(|b| b.name.clone())
        .unwrap_or_default();
    assert_eq!(hub_name, "mcu513", "main layer hub should be mcu513 (highest signal degree)");

    let isolated: HashSet<i64> = mcc::viz::layout::flow::compute_isolated_ids(&graph, hub);
    let detail: Vec<String> = isolated
        .iter()
        .filter_map(|id| {
            graph.boxes.iter().find(|b| b.id == *id).map(|b| {
                let nets: Vec<String> = graph
                    .nets
                    .iter()
                    .filter(|n| n.box_ids().contains(&b.id))
                    .map(|n| format!("{}({:?})", n.name, n.kind))
                    .collect();
                format!("id={} name='{}' kind={:?} nets={:?}", b.id, b.name, b.kind, nets)
            })
        })
        .collect();
    assert!(
        isolated.is_empty(),
        "main layer island set should be empty (power modules already wired into the main flow by the driver stage), got {} entries: {:?}",
        isolated.len(),
        detail
    );
}

#[test]
fn sub_layers_s1_s2_decoration_counts() {
    // Acceptance: S1 —— every GND endpoint has exactly 1 ground symbol (decoration count = sub-layer Ground rail endpoint count)
    //             S2 —— every non-GND rail endpoint has exactly 1 rail dot
    // Expected values come from the golden netlist (PASS2 §1.8) per-module GND / power net endpoint counts.
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let graph = build_graph();
    let (_doc, metrics) = render_with_metrics(graph, RenderOpts::default());

    let get = |layer: &str| {
        metrics
            .renderdiff_layers
            .iter()
            .find(|r| r.layer == layer)
            .unwrap_or_else(|| panic!("layer {layer} is not in the report"))
    };

    // main: R-1/R-3 no symbols at top level
    let main = get("main");
    assert_eq!(main.decorations_ground, 0, "top-level R-1 places no GND symbol");
    assert_eq!(main.decorations_power, 0, "top-level R-3 places no rail dot");

    // Sub-layer expectations = golden netlist rail net endpoint counts − pseudo-endpoints
    // removed by P7-2 rule (c) (each rail net's port.X / member.X boundary declaration
    // points, 1 each; derivation below):
    //   mcu513   GND 9−1=8          power = VDD_3V3 7−1 + VCC_1V2 3−1 = 8
    //   mic      dc.GND 7−1=6       power = dc.VDD_3V3 3−1 = 2
    //   modldo   GND 4−1=3          power = POWER_SYS 4−1 + VCC 3−1 = 5
    //   moddcdc  GND 8−1=7          power = VDD_3V3 4−1 + VCC_1V2 5−1 = 7
    //   speaker  USB_VBUS_1.GND 8−1=7  power = VDD_3V 3−1 = 2
    //            (the VDD_3V3 net is down to the single endpoint R7.1, already
    //            adjudicated as a stub deletion by the P7-2 audit)
    //   usbsocket vin.GND 7−1=6     power = vin.POWER_SYS 2−1 = 1
    let expect: &[(&str, usize, usize)] = &[
        ("mcu513", 8, 8),
        ("mic", 6, 2),
        ("modldo", 3, 5),
        ("moddcdc", 7, 7),
        ("speaker", 7, 2),
        ("usbsocket", 6, 1),
    ];
    for (layer, gnd, pwr) in expect {
        let r = get(layer);
        assert_eq!(
            r.decorations_ground, *gnd,
            "layer {layer} S1 ground symbol count (golden GND endpoint count)"
        );
        assert_eq!(
            r.decorations_power, *pwr,
            "layer {layer} S2 rail dot count (golden non-GND rail endpoint count)"
        );
        // S1/S2 semantics: these endpoints no longer have cross-box edges
        assert_eq!(r.gnd_edges, 0, "layer {layer} should have no cross-box GND edges");
        assert_eq!(r.power_edges, 0, "layer {layer} should have no cross-box power edges");
    }
}
