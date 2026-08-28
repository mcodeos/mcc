// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-4 diagnostic probe: two consecutive flat builds in one process,
//! comparing netlist fingerprints layer by layer to locate non-determinism
//! injection points (frontend InstTable / builder / graph construction).
//!
//! Three fingerprint sections (all sorted before comparison; exposes "content differs",
//! not "order differs"):
//!  - flat_nets  : InstTable level (net names + sorted endpoint paths); unstable ⇒ frontend id/naming
//!  - block_nets : McVecBlock level of build_mc_vec (net names + sorted point ids)
//!  - graph_nets : fromblock graph level (layer names + net names + sorted endpoint box ids)

use std::path::PathBuf;

use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

static BUILD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_once() -> (String, String, String, String) {
    let _guard = BUILD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let project_root = hbl_project_dir();
    let entry_uri = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();

    // Standard startup: mcc_init() auto-loads the mcode system library from the
    // data root (~/.mcode by default).
    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");

    // ── Fingerprint 1: all InstTable nets (names + sorted endpoint paths) ──
    let mut flat_lines: Vec<String> = table
        .get_nets()
        .iter()
        .map(|n| {
            let mut pts = n.points.clone();
            pts.sort();
            let paths: Vec<String> = pts
                .iter()
                .filter_map(|&p| table.get_entry(p).map(|e| e.path.clone()))
                .collect();
            format!("{} #{}: {}", n.name, n.id, paths.join(","))
        })
        .collect();
    flat_lines.sort();
    // ★ Diagnostic: print flat VCC / RES related lines
    for l in &flat_lines {
        if l.contains("VCC #") || l.contains("RES") {
            eprintln!("[det_probe] FLAT: {l}");
        }
    }
    // ── Fingerprint 1b: entry id → path (probe whether id allocation order is stable) ──
    let mut entry_lines: Vec<String> = table
        .iter()
        .map(|(id, e)| format!("{id} {} {:?}", e.path, e.kind))
        .collect();
    entry_lines.sort();
    flat_lines.extend(entry_lines);
    let flat = flat_lines.join("\n");

    // ── Fingerprint 2: McVecBlock ──
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    let mut block_lines: Vec<String> = Vec::new();
    let mut path_lines: Vec<String> = Vec::new();
    fn walk_block(
        b: &mcc::vector::model::block::McVecBlock,
        table: &mcc::InstTable,
        out: &mut Vec<String>,
        paths_out: &mut Vec<String>,
    ) {
        let mut nets: Vec<String> = b
            .nets
            .iter()
            .map(|n| {
                let mut ids = n.all_point_ids();
                ids.sort();
                format!("{}:{:?}", n.name, ids)
            })
            .collect();
        nets.sort();
        out.push(format!("[{}] {}", b.name, nets.join(" | ")));
        // Fingerprint 4: member paths (id → path expansion, probing member differences)
        let mut pnet: Vec<String> = b
            .nets
            .iter()
            .map(|n| {
                let mut paths: Vec<String> = n
                    .all_point_ids()
                    .into_iter()
                    .filter_map(|id| table.get_entry(id as u32).map(|e| e.path.clone()))
                    .collect();
                paths.sort();
                format!("{}: {}", n.name, paths.join(","))
            })
            .collect();
        pnet.sort();
        paths_out.push(format!("[{}] {}", b.name, pnet.join(" | ")));
        for sub in &b.blocks {
            walk_block(sub, table, out, paths_out);
        }
    }
    walk_block(&vec_block, &table, &mut block_lines, &mut path_lines);
    let block = block_lines.join("\n");
    let pathf = path_lines.join("\n");

    // ── Fingerprint 3: graph ──
    let graph = mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);
    let mut graph_lines: Vec<String> = Vec::new();
    fn walk_graph(g: &mcc::vector::graph::McVecGraph, out: &mut Vec<String>) {
        let mut nets: Vec<String> = g
            .nets
            .iter()
            .map(|n| {
                let mut ids: Vec<i64> = n.endpoints.iter().map(|e| e.box_id).collect();
                ids.sort();
                format!("{}:{:?}", n.name, ids)
            })
            .collect();
        nets.sort();
        out.push(format!("[{}] {}", g.name, nets.join(" | ")));
        for sub in &g.sub_graphs {
            walk_graph(sub, out);
        }
    }
    walk_graph(&graph, &mut graph_lines);
    let graphf = graph_lines.join("\n");

    (flat, block, graphf, pathf)
}

#[test]
fn probe_two_builds_are_identical() {
    let (f1, b1, g1, p1) = build_once();
    let (f2, b2, g2, p2) = build_once();

    for (name, a, b) in [
        ("flat", &f1, &f2),
        ("block_paths", &p1, &p2),
        ("block", &b1, &b2),
        ("graph", &g1, &g2),
    ] {
        if a != b {
            let da: Vec<&str> = a.lines().collect();
            let db: Vec<&str> = b.lines().collect();
            let mut shown = 0;
            for i in 0..da.len().max(db.len()) {
                let (x, y) = (da.get(i), db.get(i));
                if x != y {
                    eprintln!("[det_probe] {name} line {i}:\n  1st: {x:?}\n  2nd: {y:?}");
                    shown += 1;
                    if shown >= 6 {
                        break;
                    }
                }
            }
            panic!(
                "{name} fingerprint differs across two builds (first {shown} differences printed)"
            );
        }
    }
}
