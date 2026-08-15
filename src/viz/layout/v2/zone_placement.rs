// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Zone rough paper placement (M2-3)
//!
//! Converts the zone tree into ZonePlans on paper, including:
//! 1. Rect size computation for each zone
//! 2. Greedy left-to-right ordering between zones (signal flow: power → MCU → peripherals)
//! 3. Canvas size computation

use super::plan::{Rect, ZonePlan};
use super::zone::ZoneTree;
use crate::vector::graph::{BoxKind, McVecGraph};

/// Zone paper placement parameters
const ZONE_GAP: f64 = 80.0; // zone gap (M4-0: increased from 40 to 80, reserving routing channels)
const ZONE_PAD: f64 = 20.0; // zone padding
const ZONE_MIN_W: f64 = 200.0; // zone minimum width
const ZONE_MIN_H: f64 = 150.0; // zone minimum height
const WIRE_CHANNEL: f64 = 60.0; // routing channel width (consistent with geom.rs)
const TITLE_H: f64 = 30.0; // title bar height
const BOX_PER_ROW: usize = 4; // boxes per row
const MIN_CANVAS_W: f64 = 1200.0; // top-level minimum canvas width
const MIN_CANVAS_H: f64 = 800.0; // top-level minimum canvas height

// M4-1a: submodule-specific parameters
const SUB_ZONE_MIN_W: f64 = 120.0; // submodule zone minimum width
const SUB_ZONE_MIN_H: f64 = 100.0; // submodule zone minimum height
const SUB_MIN_CANVAS_W: f64 = 400.0; // submodule minimum canvas width
const SUB_MIN_CANVAS_H: f64 = 300.0; // submodule minimum canvas height

/// Estimate size from box type and pin count (kept consistent with geom::box_size)
fn est_box_size(kind: &BoxKind, pin_count: usize) -> (f64, f64) {
    match kind {
        BoxKind::PowerLabel | BoxKind::Dot => (24.0, 24.0),
        BoxKind::TwoPin => (80.0, 60.0),
        BoxKind::MultiPin => {
            let w = (120.0_f64).max(pin_count as f64 * 10.0);
            let h = (80.0_f64).max(pin_count as f64 * 8.0);
            (w, h)
        }
        BoxKind::SubModule => {
            let w = (140.0_f64).max(pin_count as f64 * 10.0);
            let h = (100.0_f64).max(pin_count as f64 * 8.0);
            (w, h)
        }
        BoxKind::PortTerminal => (24.0, 24.0),
    }
}

/// Compute paper positions for zones
pub fn place_zones(graph: &McVecGraph, tree: &ZoneTree, is_submodule: bool) -> Vec<ZonePlan> {
    if tree.zones.is_empty() {
        return Vec::new();
    }

    // ── 1. Compute each zone's rect (using actual box sizes) ──
    let mut zone_rects: Vec<Rect> = Vec::new();
    for zone in &tree.zones {
        zone_rects.push(compute_zone_rect(zone, graph, is_submodule));
    }

    // ── 2. Greedy ordering by inter-zone connection count ──
    let order = order_zones_by_size(tree);

    // ── 3. Horizontal arrangement (left to right) ──
    let mut x = ZONE_PAD;
    let mut max_h = 0.0f64;
    let mut plans: Vec<ZonePlan> = Vec::new();

    for &zone_id in &order {
        let rect = zone_rects[zone_id];
        let zone = &tree.zones[zone_id];

        plans.push(ZonePlan {
            zone: zone_id,
            box_ids: zone.boxes.clone(),
            rect: Rect {
                x,
                y: ZONE_PAD,
                w: rect.w,
                h: rect.h,
            },
            title_anchor: super::plan::Point {
                x: x + ZONE_PAD,
                y: ZONE_PAD + ZONE_PAD,
            },
            title: zone.title.clone(),
        });

        x += rect.w + ZONE_GAP;
        max_h = max_h.max(rect.h);
    }

    plans
}

/// Compute a zone's rect size (using actual box sizes + routing channels)
fn compute_zone_rect(zone: &super::zone::Zone, graph: &McVecGraph, is_submodule: bool) -> Rect {
    let (min_w, min_h) = if is_submodule {
        (SUB_ZONE_MIN_W, SUB_ZONE_MIN_H)
    } else {
        (ZONE_MIN_W, ZONE_MIN_H)
    };

    let box_count = zone.boxes.len();
    if box_count == 0 {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: min_w,
            h: min_h,
        };
    }

    // Collect actual sizes of all boxes in this zone
    let mut max_w: f64 = 80.0;
    let mut max_h: f64 = 60.0;
    let mut found = 0usize;
    for b in &graph.boxes {
        if zone.boxes.contains(&b.id) {
            let (bw, bh) = est_box_size(&b.kind, b.pin_count);
            max_w = max_w.max(bw);
            max_h = max_h.max(bh);
            found += 1;
        }
    }
    if found == 0 {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: min_w,
            h: min_h,
        };
    }

    // Estimate by the arrangement's maximum possible layer count (at most 1 box per layer, i.e. N layers)
    let max_layers = box_count;
    let cols = BOX_PER_ROW.min(box_count);
    let rows = (box_count + cols - 1) / cols;

    // Width = layers × (max box width + routing channel) + padding
    let w = (max_layers as f64 * (max_w + WIRE_CHANNEL) + ZONE_PAD * 2.0).max(min_w);
    // Height = rows × (max box height + gap) + title bar + padding
    let h = (rows as f64 * (max_h + 10.0) + TITLE_H + ZONE_PAD * 2.0).max(min_h);

    Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
    }
}

/// Order zones by box count (large zones first), root placed at the very front
fn order_zones_by_size(tree: &ZoneTree) -> Vec<usize> {
    let mut ids: Vec<usize> = (0..tree.zones.len()).collect();
    ids.sort_by_key(|&id| {
        let zone = &tree.zones[id];
        // Root zones first
        let is_root = tree.roots.contains(&id);
        let box_count = zone.boxes.len();
        // Root zones at the very front, the rest by box count descending
        (!is_root, -(box_count as i64))
    });
    ids
}

/// Compute canvas size from zone plans (with minimum constraints; submodules use smaller minimums)
pub fn compute_canvas(plans: &[ZonePlan], is_submodule: bool) -> (f64, f64) {
    let (min_w, min_h) = if is_submodule {
        (SUB_MIN_CANVAS_W, SUB_MIN_CANVAS_H)
    } else {
        (MIN_CANVAS_W, MIN_CANVAS_H)
    };

    let mut max_x = 0.0f64;
    let mut max_y = 0.0f64;

    for plan in plans {
        let right = plan.rect.x + plan.rect.w;
        let bottom = plan.rect.y + plan.rect.h;
        max_x = max_x.max(right);
        max_y = max_y.max(bottom);
    }

    let w = (max_x + ZONE_PAD).max(min_w);
    let h = (max_y + ZONE_PAD).max(min_h);
    (w, h)
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{IoSummary, McVecBox};
    use crate::vector::graph::{BoxKind, Symbol};

    fn make_box(id: i64, inst_path: &str, kind: BoxKind) -> McVecBox {
        McVecBox::new_v2(
            id,
            String::new(),
            String::new(),
            kind,
            Symbol::Unknown,
            None,
            None,
            0,
            IoSummary::default(),
            inst_path.to_string(),
            Vec::new(),
        )
    }

    #[test]
    fn test_place_zones_trivial() {
        let mut graph = McVecGraph::new(0, String::new());
        graph.boxes.push(make_box(1, "main.R1", BoxKind::TwoPin));
        graph.boxes.push(make_box(2, "main.R2", BoxKind::TwoPin));
        graph.boxes.push(make_box(3, "main.C1", BoxKind::TwoPin));

        let tree = super::super::zone::ZoneTree::build(&graph);
        let plans = place_zones(&graph, &tree, false);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].rect.w >= ZONE_MIN_W);
        assert!(plans[0].rect.h >= ZONE_MIN_H);
    }

    #[test]
    fn test_place_zones_multiple() {
        let mut graph = McVecGraph::new(0, String::new());
        graph
            .boxes
            .push(make_box(1, "main.modldo.ldo", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(2, "main.moddcdc.dcdc", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(3, "main.mic.MIC", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(4, "main.speaker.SPK", BoxKind::MultiPin));

        let tree = super::super::zone::ZoneTree::build(&graph);
        let plans = place_zones(&graph, &tree, false);
        // 4 single-box zones → each should be non-zero
        assert!(plans.len() >= 1);
        for plan in &plans {
            assert!(plan.rect.w > 0.0);
            assert!(plan.rect.h > 0.0);
        }
    }

    #[test]
    fn test_compute_canvas() {
        let plans = vec![
            ZonePlan {
                zone: 0,
                box_ids: vec![],
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 200.0,
                    h: 150.0,
                },
                title_anchor: super::super::plan::Point { x: 0.0, y: 0.0 },
                title: String::new(),
            },
            ZonePlan {
                zone: 1,
                box_ids: vec![],
                rect: Rect {
                    x: 240.0,
                    y: 0.0,
                    w: 200.0,
                    h: 150.0,
                },
                title_anchor: super::super::plan::Point { x: 0.0, y: 0.0 },
                title: String::new(),
            },
        ];
        let (w, h) = compute_canvas(&plans, false);
        assert!(w >= 440.0 + ZONE_PAD);
        assert!(h >= 150.0 + ZONE_PAD);
    }
}
