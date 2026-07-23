// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW —— viz pipeline debug log (triggered by MC_VIZ_DUMP=1)
//!
//! Analogous to [`crate::vector::builder::debug`]:
//! outputs reconciliation information after each stage of layout / route / render to
//! help you locate "why this line didn't draw".
//!
//! ## Enable
//! ```bash
//! MC_VIZ_DUMP=1 cargo run --bin mcviz <project> Main
//! ```
//!
//! ## Three output sections
//! - `[VIZ-LAYOUT]` —— layout stage: box count, each box's (x, y, w, h), any overlap
//! - `[VIZ-ROUTE ]` —— route stage: net count, router choice, each net's endpoint count + segment count
//! - `[VIZ-RENDER]` —— render stage: total SVG bytes, each layer's bytes

use std::sync::OnceLock;

use crate::vector::graph::{McVecGraph, NetKind};

use super::doc::VizDocument;

// ============================================================================
// Enable check
// ============================================================================

static DUMP_ENABLED: OnceLock<bool> = OnceLock::new();

pub fn dump_enabled() -> bool {
    *DUMP_ENABLED.get_or_init(|| match std::env::var("MC_VIZ_DUMP") {
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t == "false" || t == "False" || t == "FALSE")
        }
        Err(_) => false,
    })
}

// ============================================================================
// dump_layout: called after layout
// ============================================================================

pub fn dump_layout(graph: &McVecGraph, layouter_name: &str, canvas: (f64, f64)) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VIZ-LAYOUT][{}]", graph.name);
    eprintln!("{p} ── BEGIN ─────────────────────────────────");
    eprintln!("{p} layouter   = {layouter_name}");
    eprintln!("{} canvas     = {:.0} x {:.0}", p, canvas.0, canvas.1);
    eprintln!("{} boxes      = {}", p, graph.boxes.len());

    // Per-box coordinates
    for b in &graph.boxes {
        eprintln!(
            "{}   #{}  '{}'  pos=({:.0},{:.0})  size=({:.0}x{:.0})  kind={}",
            p, b.id, b.name, b.x, b.y, b.w, b.h, b.kind
        );
    }

    // Consistency check: any overlap?
    let mut overlaps = 0;
    for i in 0..graph.boxes.len() {
        for j in (i + 1)..graph.boxes.len() {
            let a = &graph.boxes[i];
            let b = &graph.boxes[j];
            // 容器/边框盒天然框住子盒，不算碰撞。
            if a.is_container_box() || b.is_container_box() {
                continue;
            }
            let x_overlap = a.x < b.x + b.w && b.x < a.x + a.w;
            let y_overlap = a.y < b.y + b.h && b.y < a.y + a.h;
            if x_overlap && y_overlap {
                overlaps += 1;
                eprintln!(
                    "{}   ⚠ overlap: #{} '{}' ↔ #{} '{}'",
                    p, a.id, a.name, b.id, b.name
                );
            }
        }
    }
    if overlaps > 0 {
        eprintln!("{p} ⚠ {overlaps} overlapping pair(s) detected");
    }

    eprintln!("{p} ── END ───────────────────────────────────");
}

// ============================================================================
// dump_route: called after route
// ============================================================================

pub fn dump_route(graph: &McVecGraph) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VIZ-ROUTE ][{}]", graph.name);
    eprintln!("{p} ── BEGIN ─────────────────────────────────");
    eprintln!("{} nets       = {}", p, graph.nets.len());

    // Distribution by NetKind
    let mut by_kind: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    for n in &graph.nets {
        let k = match &n.kind {
            NetKind::Power => "power",
            NetKind::Ground => "ground",
            NetKind::Signal => "signal",
            NetKind::Bus(_) => "bus",
            NetKind::SubModuleIO => "submodule_io",
        };
        *by_kind.entry(k).or_insert(0) += 1;
    }
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by_key(|x| x.0);
    for (k, n) in kinds {
        eprintln!("{p}   net[{k}] = {n}");
    }

    // Per-net details
    let mut routed = 0;
    let mut unrouted = 0;
    for n in &graph.nets {
        let segs = n.route.as_ref().map(|r| r.segments.len()).unwrap_or(0);
        let juncs = n.route.as_ref().map(|r| r.junctions.len()).unwrap_or(0);
        if n.route.is_some() {
            routed += 1;
        } else {
            unrouted += 1;
        }
        eprintln!(
            "{}   net #{} '{}'  kind={}  endpoints={}  segments={}  junctions={}",
            p,
            n.nid,
            n.name,
            n.kind,
            n.endpoints.len(),
            segs,
            juncs
        );
    }
    if unrouted > 0 {
        eprintln!("{p} ⚠ {unrouted} net(s) without route");
    }
    eprintln!("{p} routed={routed} unrouted={unrouted}");
    eprintln!("{p} ── END ───────────────────────────────────");
}

// ============================================================================
// dump_document: full VizDocument overview
// ============================================================================

pub fn dump_document(doc: &VizDocument) {
    if !dump_enabled() {
        return;
    }
    let p = "[VIZ-DOC   ]";
    eprintln!("{p} ── BEGIN ─────────────────────────────────");
    eprintln!("{} root_bid   = {}", p, doc.root_bid);
    eprintln!("{} root_name  = {}", p, doc.root_name);
    eprintln!("{} layers     = {}", p, doc.layer_count());
    eprintln!("{} total SVG  = {} bytes", p, doc.total_svg_bytes());

    let mut bids: Vec<i64> = doc.layers.keys().copied().collect();
    bids.sort();
    for bid in bids {
        let l = &doc.layers[&bid];
        eprintln!(
            "{}   layer #{}  name={:20}  parent={:?}  canvas=({:.0}x{:.0})  svg={} bytes  subs={}",
            p,
            l.bid,
            l.name,
            l.parent_bid,
            l.canvas.0,
            l.canvas.1,
            l.svg.len(),
            l.clickable_subs.len(),
        );
    }
    eprintln!("{p} ── END ───────────────────────────────────");
}
