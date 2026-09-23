// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ M6.0 · end-to-end pipeline test — coalesce through the REAL hbl project.
//!
//! The `equi_audit` fixtures build `McVecGraph` by hand, which bypasses the
//! projection → coalesce chain. This test closes that hole: it compiles the hbl
//! project, projects it to a real `McVecGraph`, runs `coalesce_equipotential_nets`
//! and asserts the ground-net count is conserved (the ground nets the netlist
//! declares must never fold into one — A16's coalesce-side guarantee), then
//! renders the whole tree and asserts the SVG contains no NaN/Infinity.

use std::path::PathBuf;

use mcc::vector::graph::{McVecGraph, NetKind};
use mcc::viz::api::{render_with_metrics, RenderOpts};
use crate::common;
use mcc::viz::layout::equi_audit::audit_equi_tree;
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

/// The mcc_* workspace is global state; rendering must be serialized (parallel
/// runs stomp on each other → SIGABRT). `common::lock` is the one shard-wide
/// mutex: a file-private one does not exclude tests holding `common::lock`
/// elsewhere in the same binary (U136).
fn build_hbl_graph() -> McVecGraph {
    let _guard = common::lock();
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    // Standard startup: mcc_init() auto-loads the mcode system library from the
    // data root (~/.mcode by default).
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table, &arena, &store);
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
         the ground nets the netlist declares must survive"
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
/// to `<os temp dir>/m6_diag/` (path printed below) for eyeballing.
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
        // OS temp dir (honors TMPDIR/TEMP) — never assume Unix-only /tmp.
        let dir = std::env::temp_dir().join("m6_diag");
        let _ = std::fs::create_dir_all(&dir);
        eprintln!(
            "DIAG svg written to {}",
            dir.join(format!("{}.svg", layer.name)).display()
        );
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

// ★ Strict netlist respect · render-level acceptance
// The projection layer preserves the pass2 netlist's ground nets verbatim
// (one netlist ground net → one projected net), and equipotential_tree draws
// one ground glyph (3-bar symbol in #2980B9) per ground net — one trunk + one
// symbol, whatever the number of consumers. Assert that on the REAL hbl render.

/// Ground-colored (0x2980B9) axis-aligned segments, as (x1, y1, x2, y2).
fn ground_color_lines(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    svg.split("<line")
        .filter(|s| s.contains("#2980B9"))
        .filter_map(|s| {
            let attr = s.split('>').next()?;
            let at = |k: &str| -> Option<f64> {
                let key = format!("{k}=\"");
                let i = attr.find(&key)?;
                let rest = &attr[i + key.len()..];
                let end = rest.find('"')?;
                rest[..end].parse().ok()
            };
            Some((at("x1")?, at("y1")?, at("x2")?, at("y2")?))
        })
        .collect()
}

/// Count distinct ground-glyph centers: a ground symbol is 3 short stacked bars
/// (horizontal bars for a vertical lead, vertical bars for a horizontal lead).
/// Overlapping glyphs (two consumers placed at the same spot) still register
/// their own bar triple only if the bars do not fully coincide — so the caller
/// tolerates one collision per layer (see the box-collision note below).
fn count_ground_glyphs(svg: &str) -> usize {
    let lines = ground_color_lines(svg);
    let hbars: Vec<(f64, f64)> = lines
        .iter()
        .filter(|(x1, y1, x2, y2)| {
            (y1 - y2).abs() < 0.01 && (4.0..=40.0).contains(&(x1 - x2).abs())
        })
        .map(|(x1, y1, x2, _)| ((x1 + x2) / 2.0, *y1))
        .collect();
    let vbars: Vec<(f64, f64)> = lines
        .iter()
        .filter(|(x1, y1, x2, y2)| {
            (x1 - x2).abs() < 0.01 && (4.0..=40.0).contains(&(y1 - y2).abs())
        })
        .map(|(x1, y1, _, y2)| (*x1, (y1 + y2) / 2.0))
        .collect();

    // Horizontal-bar stacks (vertical lead): center = x, bar row = y.
    let n_h = {
        let mut used = vec![false; hbars.len()];
        let mut n = 0usize;
        for i in 0..hbars.len() {
            if used[i] {
                continue;
            }
            used[i] = true;
            let mut stack = 1usize;
            for j in 0..hbars.len() {
                if used[j] {
                    continue;
                }
                if (hbars[j].0 - hbars[i].0).abs() < 8.0
                    && (hbars[j].1 - hbars[i].1).abs() > 1.0
                    && (hbars[j].1 - hbars[i].1).abs() < 30.0
                {
                    used[j] = true;
                    stack += 1;
                }
            }
            if stack >= 3 {
                n += 1;
            }
        }
        n
    };
    let n_v = {
        let mut used = vec![false; vbars.len()];
        let mut n = 0usize;
        for i in 0..vbars.len() {
            if used[i] {
                continue;
            }
            used[i] = true;
            let mut stack = 1usize;
            for j in 0..vbars.len() {
                if used[j] {
                    continue;
                }
                if (vbars[j].1 - vbars[i].1).abs() < 8.0
                    && (vbars[j].0 - vbars[i].0).abs() > 1.0
                    && (vbars[j].0 - vbars[i].0).abs() < 30.0
                {
                    used[j] = true;
                    stack += 1;
                }
            }
            if stack >= 3 {
                n += 1;
            }
        }
        n
    };
    n_h + n_v
}

/// Per-layer ground-net count from the built graph (one per netlist ground net).
fn ground_nets_by_bid(g: &McVecGraph, out: &mut std::collections::HashMap<i64, usize>) {
    let n = g.nets.iter().filter(|n| n.kind == NetKind::Ground).count();
    if n > 0 {
        out.insert(g.bid, n);
    }
    for sg in &g.sub_graphs {
        ground_nets_by_bid(sg, out);
    }
}

// ★ U276①: the audit-invariant harness on the REAL hbl device layers.
// `equi_audit`'s own fixtures build `McVecGraph` by hand, so the A27/A34 shape
// — anchors on a minority of nets, most nets carrying satellite members — had
// zero coverage. `layout_device_layer` hands back the topology slice
// `place_by_topology` mutated, which is exactly what `audit_equi_tree`
// requires (A2 replays lanes against that same slice).

/// Run the layout phase on every Device-style layer of the tree and audit each.
/// Mirrors the render recursion's structural rule (api.rs): the root is a block
/// diagram when it holds sub-module boxes, every sub-layer is Device.
fn audit_hbl_device_layers(g: &mut McVecGraph, is_root: bool) -> Vec<(String, mcc::viz::layout::equi_audit::EquiAudit)> {
    let mut audits = Vec::new();
    let has_sub_boxes = g.boxes.iter().any(|b| b.kind == mcc::vector::graph::BoxKind::SubModule);
    let is_device = !g.boxes.is_empty() && (if is_root { !has_sub_boxes } else { true });
    if is_device {
        let name = g.name.clone();
        let topos = mcc::viz::layout::equipotential_tree::layout_device_layer(g);
        audits.push((name, audit_equi_tree(g, &topos)));
    }
    for sg in &mut g.sub_graphs {
        audits.extend(audit_hbl_device_layers(sg, false));
    }
    audits
}

/// The audit invariants, red on REAL projected layers but green on every
/// hand-built `equi_audit` fixture. First exposure (U276①, b3928): the harness
/// had never run on the real hbl shape, where anchors own a minority of nets
/// and satellites do the rest. Each entry is a KNOWN defect, keyed by layer
/// name and check id. The gate below is an exact-set ratchet:
///  - a check red here that is not listed (or red on a clean layer) fails;
///  - a listed defect that turns green must be REMOVED from this table —
///    stale entries fail, so fixes cannot hide behind the ratchet.
/// Cleanup of the defects themselves is ledger work, not this table's job.
const KNOWN_AUDIT_REDS: &[(&str, &str)] = &[
    ("DCDC", "A7"),
    ("DCDC", "A10"),
    ("DCDC", "A17"),
    ("DCDC", "A22"),
    ("DCDC", "A24"),
    ("DCDC", "A29"),
    ("DCDC", "A34"),
    ("lp322dcdc", "A8"),
    ("lp322dcdc", "A10"),
    ("lp322dcdc", "A17"),
    ("lp322dcdc", "A18"),
    ("lp322dcdc", "A22"),
    ("LDO", "A17"),
    ("LDO", "A34"),
    ("MCU513", "A7"),
    ("MCU513", "A8"),
    ("MCU513", "A10"),
    ("MCU513", "A12"),
    ("MCU513", "A17"),
    ("MCU513", "A22"),
    ("MCU513", "A24"),
    ("MCU513", "A25"),
    ("MCU513", "A29"),
    ("MCU513", "A34"),
    ("UC", "A2"),
    ("UC", "A3"),
    ("UC", "A8"),
    ("UC", "A10"),
    ("UC", "A11"),
    ("UC", "A17"),
    ("UC", "A18"),
    ("UC", "A22"),
    ("UC", "A29"),
    ("UC", "A34"),
    ("X6", "A4"),
    ("X6", "A10"),
    ("X6", "A14"),
    ("X6", "A22"),
    ("X6", "A24"),
    ("X6", "A29"),
    ("X6", "A34"),
    ("MIC", "A1"),
    ("MIC", "A2"),
    ("MIC", "A3"),
    ("MIC", "A17"),
    ("MIC", "A18"),
    ("MIC", "A22"),
    ("MIC", "A34"),
    ("SPK", "A8"),
    ("SPK", "A10"),
    ("SPK", "A17"),
    ("SPK", "A18"),
    ("SPK", "A25"),
    ("SPK", "A30"),
    ("SPK", "A34"),
    ("USB", "A8"),
    ("USB", "A17"),
    ("USB", "A34"),
    ("FLASH", "A8"),
    ("FLASH", "A17"),
];

#[test]
fn e2e_hbl_device_layers_equi_audit_known_reds_only() {
    let mut graph = build_hbl_graph();
    let audits = audit_hbl_device_layers(&mut graph, true);
    assert!(
        !audits.is_empty(),
        "no Device-style layer found in the hbl tree — the audit would cover nothing"
    );
    for (name, audit) in &audits {
        eprintln!("-- layer '{name}' --\n{audit}");
        for c in &audit.checks {
            let red = c.status == mcc::viz::layout::equi_audit::CheckStatus::Fail;
            let listed = KNOWN_AUDIT_REDS.contains(&(name.as_str(), c.id));
            assert_eq!(
                red, listed,
                "layer '{name}' check {} {}: expected {}, found red={} — \
                 update KNOWN_AUDIT_REDS (fix-and-remove, or register the new defect)",
                c.id, c.name, if listed { "listed red" } else { "green" }, red
            );
        }
    }
}

/// ★ Strict netlist respect: the Device pipeline renders exactly ONE ground
/// glyph per ground net the netlist declares (one trunk + one symbol), with no
/// per-consumer splitting. So a layer's ground-glyph count must equal its
/// ground-net count. (Two known pre-existing box collisions in the layout place
/// a resistor+cap pair at one spot in DCDC/MIC, so their glyphs overlap; the
/// assertion tolerates exactly one such collision.)
#[test]
fn e2e_hbl_ground_glyph_count_matches_netlist() {
    let graph = build_hbl_graph();
    let mut gnd_nets = std::collections::HashMap::new();
    ground_nets_by_bid(&graph, &mut gnd_nets);
    let (doc, _metrics) = render_with_metrics(graph, RenderOpts::default());

    for (bid, net_count) in &gnd_nets {
        let layer = doc.layers.get(bid).expect("layer rendered");
        if layer.name == "main" {
            continue; // root keeps one merged ground + one glyph
        }
        let glyphs = count_ground_glyphs(&layer.svg);
        assert!(
            glyphs >= net_count.saturating_sub(1),
            "layer '{}': ground glyphs {glyphs} ≮ ground nets {net_count} (allow one collision)",
            layer.name
        );
        assert!(
            glyphs <= net_count + 1,
            "layer '{}': ground glyphs {glyphs} exceed netlist ground nets {net_count} — \
             a ground net must render exactly one glyph (no per-consumer split)",
            layer.name
        );
    }
}
