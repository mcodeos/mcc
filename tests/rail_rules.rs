// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-3 · Rail trichotomy acceptance test (MC_SCHEMATIC_ROADMAP_v6 P7-3 acceptance checklist)
//!
//! - main layer: GND edges = 0, rail flag boxes = 0, driver stage edges = 4,
//!   matching the §1.2 seven-line checklist item by item (edge-table assertions live in tests/renderdiff.rs).
//! - main layer `compute_isolated_ids` returns the empty set (USB/LDO/DCDC are no longer islands).
//! - Sub-layers: every GND endpoint has exactly 1 ground symbol (S1),
//!   every non-GND rail endpoint has exactly 1 rail dot (S2).
//!
//! Criteria are asserted by **box name/net name** (ids are unstable across processes).

use std::collections::HashSet;
use std::path::PathBuf;

use mcc::viz::api::{render_with_metrics, RenderOpts};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// The mcc_* workspace is global state; tests must be serialized (same as tests/renderdiff.rs)
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_graph() -> mcc::vector::graph::McVecGraph {
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    // Standard startup: mcc_init() auto-loads the mcode system library from the
    // data root (~/.mcode by default).
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table)
}

#[test]
fn main_layer_isolated_set_is_empty() {
    // Acceptance: driver stage edges bring USB/LDO/DCDC into the main flow;
    // compute_isolated_ids(main, hub) must return the empty set.
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut graph = build_graph();
    mcc::vector::graph::apply_promote_recursive(&mut graph);
    // Mirror pipeline: classify_rails runs before island computation (flow.rs phase_prepare → phase_placement)
    mcc::viz::layout::rails::classify_rails(&mut graph, /*is_top=*/ true);
    // hub = the box with the highest signal degree (main layer = MCU513)
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
    assert_eq!(
        hub_name, "MCU513",
        "main layer hub should be MCU513 (highest signal degree)"
    );

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
                format!(
                    "id={} name='{}' kind={:?} nets={:?}",
                    b.id, b.name, b.kind, nets
                )
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
    // Acceptance (F2-era, post d1464c0): every sub-layer uses the Device
    // pipeline (equipotential_tree), which renders ONE ground glyph per ground
    // NET (M6.5: "one ground net → one trunk → one ground glyph") and places
    // rail symbols geometrically — NOT via graph.rail_decorations. The
    // decorations_ground / decorations_power readings below are therefore 0 for
    // all sub-layers, and the cross-box ground/power edge counts reflect the
    // Device pipeline drawing those rails as real edges.
    //
    // Pre-F2, sub-layers ran the FlowLayouter's classify_rails, which placed
    // one symbol per rail ENDPOINT into rail_decorations; those per-endpoint
    // counts (MCU513 GND=8, etc.) are obsolete.
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

    // main: R-1/R-3 no symbols at top level; R-B hides Ground nets entirely
    let main = get("main");
    assert_eq!(
        main.decorations_ground, 0,
        "top-level R-1 places no GND symbol"
    );
    assert_eq!(
        main.decorations_power, 0,
        "top-level R-3 places no rail dot"
    );
    assert_eq!(main.gnd_edges, 0, "main R-B hides Ground nets");

    // Sub-layers (F2 Device pipeline): rail symbols are rendered per-net
    // geometrically, not tracked in rail_decorations, and ground/power rails
    // draw real cross-box edges. Values = current render contract, measured
    // from the projection graph (golden-coupled like the pre-F2 counts).
    let expect: &[(&str, usize, usize, usize, usize)] = &[
        // layer, decorations_gnd, decorations_pwr, gnd_edges, pwr_edges
        // Sub-layers render rail symbols geometrically (Device pipeline), not via
        // rail_decorations (decorations_* = 0). Ground nets are preserved verbatim
        // from the netlist — a ground net that spans ≥2 boxes draws a real
        // cross-box trunk (gnd_edges counts those); power stays a cross-box bus.
        ("MCU513", 0, 0, 3, 2),
        ("MIC", 0, 0, 1, 1),
        ("LDO", 0, 0, 1, 2),
        ("DCDC", 0, 0, 1, 2),
        ("SPK", 0, 0, 1, 1),
        ("USB", 0, 0, 1, 0),
    ];
    for (layer, gnd, pwr, gnd_edges, pwr_edges) in expect {
        let r = get(layer);
        assert_eq!(
            r.decorations_ground, *gnd,
            "layer {layer} F2 ground decoration count"
        );
        assert_eq!(r.decorations_power, *pwr, "layer {layer} F2 rail dot count");
        assert_eq!(r.gnd_edges, *gnd_edges, "layer {layer} ground edges");
        assert_eq!(r.power_edges, *pwr_edges, "layer {layer} power edges");
    }
}
