// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Wire/Label split pass — decides whether each net should be drawn as a wire
//! or annotated with net labels.
//!
//! ## Problem
//! The router draws ALL nets as wires. Reference diagrams use net labels for ~20
//! nets that span multiple columns and draw actual wires only for ~10 local nets.
//! The router is being asked to draw lines that should be labels.
//!
//! ## Algorithm
//! For each net (after placement, before route):
//! - span(N) = manhattan(|dx|,|dy|) of endpoint box centers
//! - Boundary net (PortTerminal) → label
//! - Anonymous net with span >= 2*col_pitch → diagnostic, keep as wire
//! - span >= 2*col_pitch → label
//! - |endpoints| >= 4 AND span >= 1*col_pitch → label
//! - Otherwise → wire (no change)
//!
//! Label mode: each endpoint gets a PowerLabel box with the net name, placed
//! outward from the pin. The original net is dropped.
//!
//! ## Integration
//! Called from select.rs::run_single, between apply_net_labels and
//! route_all_with_channels. Reads graph.col_pitch (set by the layouter).

use std::collections::{HashMap, HashSet};

use crate::vector::graph::naming;
use crate::vector::graph::netdef::IoDirection;
use crate::vector::graph::{
    BoxKind, EndpointRef, EntryPoint, EntrySide, IoSummary, McVecBox, McVecGraph, NetKind, NetRole,
    Symbol, VizNet,
};

// ============================================================================
// Constants (mirrored from rails.rs for the label box layout)
// ============================================================================

const NETLABEL_GAP: f64 = 42.0;
const NETLABEL_W: f64 = 14.0;
const NETLABEL_H: f64 = 14.0;
const INFLATE: f64 = 8.0;

// ============================================================================
// Public API
// ============================================================================

/// Apply wire/label split to all nets in one graph layer.
/// Returns true if any nets were converted to labels (boxes added, nets dropped).
pub fn apply_wire_label_split(graph: &mut McVecGraph) -> bool {
    let col_pitch = graph.col_pitch;
    if col_pitch <= 0.0 {
        return false;
    }

    // Collect pin positions, PowerLabel boxes, and passive boxes.
    let mut pin_pos: HashMap<(i64, i64), ((f64, f64), EntrySide)> = HashMap::new();
    let mut label_boxes: HashSet<i64> = HashSet::new();
    let mut passive_boxes: HashSet<i64> = HashSet::new();
    let mut port_terminal_boxes: HashSet<i64> = HashSet::new();
    for b in &graph.boxes {
        if b.kind == BoxKind::PowerLabel {
            label_boxes.insert(b.id);
        }
        if b.kind == BoxKind::PortTerminal {
            port_terminal_boxes.insert(b.id);
        }
        if b.is_two_pin_passive() {
            passive_boxes.insert(b.id);
        }
        for ep in &b.entry_points {
            pin_pos.insert(
                (b.id, ep.pin_id),
                (pin_xy(b, ep), ep.side),
            );
        }
    }

    let mut next_box = graph.boxes.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    let mut next_net = graph.nets.iter().map(|n| n.nid).max().unwrap_or(0) + 1;
    let mut new_boxes: Vec<McVecBox> = Vec::new();
    let mut new_stubs: Vec<VizNet> = Vec::new();
    let mut drop_idx: HashSet<usize> = HashSet::new();

    for (idx, net) in graph.nets.iter().enumerate() {
        // Skip if already routed
        if net.route.is_some() {
            continue;
        }
        // Skip isolated nets
        if net.endpoints.len() <= 1 {
            continue;
        }
        // Buses always use wires
        if matches!(net.kind, NetKind::Bus(_)) {
            continue;
        }
        // Power and Ground nets always use wires (part of the driver stage)
        if matches!(net.kind, NetKind::Power | NetKind::Ground) {
            continue;
        }
        // Skip nets already touching PowerLabel boxes (handled by apply_net_labels)
        if net
            .endpoints
            .iter()
            .any(|e| label_boxes.contains(&e.box_id))
        {
            continue;
        }
        // Skip nets touching two-pin passives (must keep real wires)
        if net
            .endpoints
            .iter()
            .any(|e| passive_boxes.contains(&e.box_id))
        {
            continue;
        }

        let span = manhattan_span(graph, net);
        let is_boundary = net
            .endpoints
            .iter()
            .any(|e| port_terminal_boxes.contains(&e.box_id));
        let is_anonymous = net.name.is_empty() || net.name.starts_with("__net");

        // Decision rules (order matters)
        let needs_label = if is_boundary {
            true
        } else if is_anonymous && span >= 2.0 * col_pitch {
            // Anonymous net spanning too far → diagnostic, keep as wire
            crate::vlog!(
                "[wire_label_split] WARNING: anonymous net '{}' (nid={}) span {:.0} >= threshold {:.0} — cannot use labels, routing as wire",
                net.name,
                net.nid,
                span,
                2.0 * col_pitch
            );
            false
        } else if span >= 2.0 * col_pitch {
            true
        } else if net.endpoints.len() >= 4 && span >= col_pitch {
            true
        } else {
            false
        };

        if !needs_label {
            continue;
        }

        // Convert to per-endpoint label stubs
        let is_gnd = naming::is_ground(&net.name);
        let lio = if is_gnd {
            IoDirection::Ground
        } else {
            IoDirection::Passive
        };

        for e in &net.endpoints {
            let ((px, py), side) = match pin_pos.get(&(e.box_id, e.pin_id)) {
                Some(v) => (v.0, v.1),
                None => continue,
            };
            push_label_stub(
                &net.name,
                &net.kind,
                is_gnd,
                lio,
                e,
                (px, py),
                side,
                &graph.boxes,
                &mut next_box,
                &mut next_net,
                &mut new_boxes,
                &mut new_stubs,
            );
        }
        drop_idx.insert(idx);
    }

    if new_boxes.is_empty() {
        return false;
    }

    // Apply: drop converted nets, add label boxes and stub nets.
    let n_drop = drop_idx.len();
    let n_lbl = new_boxes.len();
    let mut i = 0usize;
    graph.nets.retain(|_| {
        let keep = !drop_idx.contains(&i);
        i += 1;
        keep
    });
    graph.boxes.extend(new_boxes);
    graph.nets.extend(new_stubs);

    crate::vlog!(
        "[wire_label_split] layer '{}' bid={}: {} net(s) → {} label stub(s) (col_pitch={:.0})",
        graph.name,
        graph.bid,
        n_drop,
        n_lbl,
        graph.col_pitch,
    );

    true
}

// ============================================================================
// Helpers
// ============================================================================

/// Manhattan span of a net: max(|dx|, |dy|) of endpoint box centers.
fn manhattan_span(graph: &McVecGraph, net: &VizNet) -> f64 {
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut any = false;
    for e in &net.endpoints {
        if let Some(b) = graph.boxes.iter().find(|x| x.id == e.box_id) {
            let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            minx = minx.min(cx);
            miny = miny.min(cy);
            maxx = maxx.max(cx);
            maxy = maxy.max(cy);
            any = true;
        }
    }
    if !any {
        0.0
    } else {
        (maxx - minx).max(maxy - miny)
    }
}

/// Pin xy position from a box and its entry point.
fn pin_xy(b: &McVecBox, ep: &EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Top => (b.x + b.w * ep.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * ep.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * ep.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * ep.offset),
    }
}

/// Create a label box + stub net for one endpoint.
/// Adapted from rails.rs::push_label_stub with the same collision-avoidance logic.
#[allow(clippy::too_many_arguments)]
fn push_label_stub(
    net_name: &str,
    net_kind: &NetKind,
    is_gnd: bool,
    lio: IoDirection,
    e: &EndpointRef,
    (px, py): (f64, f64),
    side: EntrySide,
    boxes: &[McVecBox],
    next_box: &mut i64,
    next_net: &mut i64,
    new_boxes: &mut Vec<McVecBox>,
    new_stubs: &mut Vec<VizNet>,
) {
    let opposite = |s: EntrySide| match s {
        EntrySide::Right => EntrySide::Left,
        EntrySide::Left => EntrySide::Right,
        EntrySide::Top => EntrySide::Bottom,
        EntrySide::Bottom => EntrySide::Top,
    };

    let text_w = (net_name.chars().count() as f64 * 7.0).max(NETLABEL_W);
    let rect_clear = |bx: f64, by: f64, new_boxes: &Vec<McVecBox>| {
        let hits = |ob: &McVecBox| {
            bx < ob.x + ob.w + INFLATE
                && bx + text_w + INFLATE > ob.x
                && by < ob.y + ob.h + INFLATE
                && by + NETLABEL_H + INFLATE > ob.y
        };
        !boxes.iter().any(hits) && !new_boxes.iter().any(hits)
    };
    let base_rect = |s: EntrySide| match s {
        EntrySide::Right => (px + NETLABEL_GAP, py - NETLABEL_H / 2.0),
        EntrySide::Left => (px - NETLABEL_GAP - NETLABEL_W, py - NETLABEL_H / 2.0),
        EntrySide::Top => (px - NETLABEL_W / 2.0, py - NETLABEL_GAP - NETLABEL_H),
        EntrySide::Bottom => (px - NETLABEL_W / 2.0, py + NETLABEL_GAP),
    };
    let step_out = |s: EntrySide, bx: &mut f64, by: &mut f64| match s {
        EntrySide::Right => *bx += NETLABEL_W + INFLATE,
        EntrySide::Left => *bx -= NETLABEL_W + INFLATE,
        EntrySide::Top => *by -= NETLABEL_H + INFLATE,
        EntrySide::Bottom => *by += NETLABEL_H + INFLATE,
    };
    let try_sides = [side, opposite(side), EntrySide::Top, EntrySide::Bottom];
    let mut chosen = (base_rect(side).0, base_rect(side).1, opposite(side));
    'outer: for s in try_sides {
        let (mut tx, mut ty) = base_rect(s);
        for _ in 0..4 {
            if rect_clear(tx, ty, new_boxes) {
                chosen = (tx, ty, opposite(s));
                break 'outer;
            }
            step_out(s, &mut tx, &mut ty);
        }
    }
    let (bx, by, lside) = chosen;

    let box_id = *next_box;
    *next_box += 1;
    let pin_id = box_id;

    let mut io = IoSummary::new();
    io.other += 1;
    let mut lbox = McVecBox::new_v2(
        box_id,
        net_name.to_string(),
        String::new(),
        BoxKind::PowerLabel,
        Symbol::PowerRail { is_ground: is_gnd },
        None,
        None,
        1,
        io,
        net_name.to_string(),
        Vec::new(),
    );
    lbox.x = bx;
    lbox.y = by;
    lbox.w = NETLABEL_W;
    lbox.h = NETLABEL_H;
    lbox.entry_points = vec![EntryPoint {
        pin_id,
        pin_name: net_name.to_string(),
        side: lside,
        offset: 0.5,
    }];
    new_boxes.push(lbox);

    let eps = vec![
        EndpointRef::with_io(box_id, pin_id, net_name.to_string(), lio),
        e.clone(),
    ];
    new_stubs.push(VizNet::new(
        *next_net,
        net_name.to_string(),
        net_kind.clone(),
        NetRole::Signal,
        eps,
    ));
    *next_net += 1;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_box(id: i64, name: &str, kind: BoxKind, x: f64, y: f64, w: f64, h: f64) -> McVecBox {
        let mut io = IoSummary::new();
        io.other += 1;
        let pin_id = id;
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            Symbol::Unknown,
            None,
            None,
            1,
            io,
            String::new(),
            Vec::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        b.entry_points = vec![EntryPoint {
            pin_id,
            pin_name: "1".into(),
            side: EntrySide::Right,
            offset: 0.5,
        }];
        b
    }

    fn mk_net(
        nid: i64,
        name: &str,
        kind: NetKind,
        endpoints: Vec<EndpointRef>,
    ) -> VizNet {
        VizNet::new(nid, name.into(), kind, NetRole::Signal, endpoints)
    }

    fn mk_ep(box_id: i64, pin_id: i64) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, "1", IoDirection::Passive)
    }

    #[test]
    fn short_net_unchanged() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(2, "B", BoxKind::MultiPin, 300.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "ENABLE",
            NetKind::Signal,
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        assert!(!apply_wire_label_split(&mut graph));
        // Net should be unchanged
        assert_eq!(graph.nets.len(), 1);
        assert_eq!(graph.boxes.len(), 2);
    }

    #[test]
    fn long_net_becomes_label() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        // Span = 1200 > 960 = 2*col_pitch
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(2, "B", BoxKind::MultiPin, 1300.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "SPI_SCLK",
            NetKind::Signal,
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        assert!(apply_wire_label_split(&mut graph));
        // Original net dropped, 2 label boxes + 2 stub nets added
        assert_eq!(graph.nets.len(), 2); // 2 stub nets
        assert_eq!(graph.boxes.len(), 4); // 2 original + 2 label boxes
        assert!(graph.boxes.iter().any(|b| b.kind == BoxKind::PowerLabel));
    }

    #[test]
    fn anonymous_net_unchanged_with_warning() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        // Span = 1200 > 960 = 2*col_pitch
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(2, "B", BoxKind::MultiPin, 1300.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "__net_14",
            NetKind::Signal,
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        assert!(!apply_wire_label_split(&mut graph));
        // Anonymous net should NOT be converted to labels
        assert_eq!(graph.nets.len(), 1);
        assert_eq!(graph.boxes.len(), 2);
    }

    #[test]
    fn bus_net_skipped() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(2, "B", BoxKind::MultiPin, 1300.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "SPI",
            NetKind::Bus(4),
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        assert!(!apply_wire_label_split(&mut graph));
        assert_eq!(graph.nets.len(), 1);
    }

    #[test]
    fn high_fanout_medium_span_becomes_label() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        // 4 endpoints, span = 600 > 480 = 1*col_pitch
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(2, "B", BoxKind::MultiPin, 200.0, 100.0, 100.0, 100.0),
            mk_box(3, "C", BoxKind::MultiPin, 300.0, 100.0, 100.0, 100.0),
            mk_box(4, "D", BoxKind::MultiPin, 700.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "DATA",
            NetKind::Signal,
            vec![mk_ep(1, 1), mk_ep(2, 2), mk_ep(3, 3), mk_ep(4, 4)],
        )];

        assert!(apply_wire_label_split(&mut graph));
        assert_eq!(graph.nets.len(), 4); // 4 stub nets
        assert_eq!(graph.boxes.len(), 8); // 4 original + 4 label boxes
    }

    #[test]
    fn passive_net_skipped() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        let mut r1 = mk_box(1, "R1", BoxKind::TwoPin, 100.0, 100.0, 100.0, 100.0);
        r1.symbol = Symbol::Resistor;
        graph.boxes = vec![
            r1,
            mk_box(2, "B", BoxKind::MultiPin, 1300.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "SIG",
            NetKind::Signal,
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        // Two-pin passive → must keep wires
        assert!(!apply_wire_label_split(&mut graph));
        assert_eq!(graph.nets.len(), 1);
    }

    #[test]
    fn boundary_net_becomes_label() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        // Short span but boundary net → still label
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(
                2,
                "PT",
                BoxKind::PortTerminal,
                300.0,
                100.0,
                100.0,
                100.0,
            ),
        ];
        graph.nets = vec![mk_net(
            1,
            "I2C0_SDA",
            NetKind::Signal,
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        assert!(apply_wire_label_split(&mut graph));
        assert_eq!(graph.nets.len(), 2); // 2 stub nets
    }

    #[test]
    fn power_net_skipped() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        // Long span but power net → must keep wires
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(2, "B", BoxKind::MultiPin, 1300.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "V3V3.VCC",
            NetKind::Power,
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        assert!(!apply_wire_label_split(&mut graph));
        assert_eq!(graph.nets.len(), 1);
        assert_eq!(graph.boxes.len(), 2);
    }

    #[test]
    fn ground_net_skipped() {
        let mut graph = McVecGraph::new(1, "test".into());
        graph.col_pitch = 480.0;
        graph.boxes = vec![
            mk_box(1, "A", BoxKind::MultiPin, 100.0, 100.0, 100.0, 100.0),
            mk_box(2, "B", BoxKind::MultiPin, 1300.0, 100.0, 100.0, 100.0),
        ];
        graph.nets = vec![mk_net(
            1,
            "GND",
            NetKind::Ground,
            vec![mk_ep(1, 1), mk_ep(2, 2)],
        )];

        assert!(!apply_wire_label_split(&mut graph));
        assert_eq!(graph.nets.len(), 1);
    }
}