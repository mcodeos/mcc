// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ M6.0 · end-to-end pipeline test — coalesce through the REAL hbl project.
//!
//! The `equi_audit` fixtures build `McVecGraph` by hand, which bypasses the
//! projection → coalesce chain. This test closes that hole: it compiles the hbl
//! project, projects it to a real `McVecGraph`, runs `coalesce_equipotential_nets`
//! and asserts the ground-net count is conserved (distinct per-consumer grounds
//! must never fold into one — A16's coalesce-side guarantee), then renders the
//! whole tree and asserts the SVG contains no NaN/Infinity.

use std::path::PathBuf;

use mcc::vector::graph::{McVecGraph, NetKind};
use mcc::viz::api::{render_with_metrics, RenderOpts};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

/// The mcc_* workspace is global state; rendering must be serialized (parallel
/// runs stomp on each other → SIGABRT).
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_hbl_graph() -> McVecGraph {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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

/// Count ground nets across every subgraph reachable from the root (the graph
/// is a tree of `sub_graphs`).
fn count_ground_nets(g: &McVecGraph) -> usize {
    let mine = g.nets.iter().filter(|n| n.kind == NetKind::Ground).count();
    let children: usize = g.sub_graphs.iter().map(|c| count_ground_nets(c)).sum();
    mine + children
}

#[test]
fn e2e_hbl_coalesce_preserves_ground_nets() {
    let mut graph = build_hbl_graph();
    let gnd_before = count_ground_nets(&graph);

    // The real pipeline's coalesce pass — must not fold distinct GND nets.
    let removed = mcc::viz::layout::coalesce::coalesce_equipotential_nets(&mut graph);
    let gnd_after = count_ground_nets(&graph);

    assert_eq!(
        gnd_before, gnd_after,
        "coalesce merged {gnd_before} -> {gnd_after} ground nets (removed {removed}); \
         distinct per-consumer grounds must survive"
    );
}

#[test]
fn e2e_hbl_render_has_no_nan() {
    let graph = build_hbl_graph();
    let (doc, _metrics) = render_with_metrics(graph, RenderOpts::default());

    assert!(doc.layer_count() >= 1, "render produced no layers");

    let mut total_bytes = 0usize;
    for layer in doc.layers.values() {
        total_bytes += layer.svg.len();
        assert!(
            !layer.svg.is_empty(),
            "layer '{}' has empty svg",
            layer.name
        );
        assert!(
            !layer.svg.contains("NaN"),
            "NaN in layer '{}' SVG — layout has undefined coordinates",
            layer.name
        );
        assert!(
            !layer.svg.contains("Infinity") && !layer.svg.contains("inf"),
            "Infinity/inf in layer '{}' SVG",
            layer.name
        );
    }
    assert!(
        total_bytes > 0,
        "render produced zero SVG bytes across all layers"
    );
}

/// Diagnostic (not a real assertion test): render the whole hbl tree and print
/// per-layer diagnostics so layout bugs can be pinned to actual output.
/// Ignored by default; run explicitly with `-- --ignored` to export the SVGs
/// to /tmp/m6_diag/ for eyeballing.
#[test]
#[ignore = "diagnostic only"]
fn e2e_diag_layers() {
    let graph = build_hbl_graph();
    let (doc, _metrics) = render_with_metrics(graph, RenderOpts::default());
    let mut bids: Vec<i64> = doc.layers.keys().copied().collect();
    bids.sort();
    for bid in bids {
        let layer = &doc.layers[&bid];
        // Extract the viewBox width/height from the layer SVG.
        let mut viewbox = String::new();
        if let Some(pos) = layer.svg.find("viewBox") {
            let rest = &layer.svg[pos..];
            let end = rest.find('>').unwrap_or(rest.len());
            viewbox = rest[..end.min(120)].to_string();
        }
        eprintln!(
            "DIAG layer {bid} '{}' svg={}B viewBox={{ {} }}",
            layer.name,
            layer.svg.len(),
            viewbox
        );
        let dir = std::path::Path::new("/tmp/m6_diag");
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join(format!("{}.svg", layer.name)), &layer.svg);
    }
}

/// Diagnostic: walk the built graph tree (BEFORE rendering) and print each
/// layer's net list with kinds, so ground-net classification can be checked.
/// Ignored by default; run explicitly with `-- --ignored`.
#[test]
#[ignore = "diagnostic only"]
fn e2e_diag_graph_nets() {
    fn walk(g: &McVecGraph, depth: usize) {
        eprintln!(
            "{:indent$}GRAPH bid={} '{}' style={:?} nets={} boxes={}",
            "",
            g.bid,
            g.name,
            g.layer_style,
            g.nets.len(),
            g.boxes.len(),
            indent = depth * 2
        );
        for n in &g.nets {
            let eps: Vec<String> = n
                .endpoints
                .iter()
                .map(|e| format!("{}:{}", e.box_id, e.pin_id))
                .collect();
            eprintln!(
                "{:indent$}  net nid={} '{}' kind={:?} eps=[{}]",
                "",
                n.nid,
                n.name,
                n.kind,
                eps.join(","),
                indent = depth * 2
            );
        }
        for b in &g.boxes {
            eprintln!(
                "{:indent$}  box id={} '{}' kind={:?} pins={}",
                "",
                b.id,
                b.name,
                b.kind,
                b.pins.len(),
                indent = depth * 2
            );
        }
        for sg in &g.sub_graphs {
            walk(sg, depth + 1);
        }
    }
    let graph = build_hbl_graph();
    walk(&graph, 0);
}
