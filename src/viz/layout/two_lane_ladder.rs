// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Two-lane bridged-passive ladder — **anchor-only** placement (M11 `c07_pins`).
//!
//! Design principle (see `passive_inline`): a two-terminal passive is an *inline
//! wire element*, not a layout box. So this pass **only** places the real
//! components — the two port-like anchors u1 / u2 — at left / right on a shared
//! baseline. The series resistors on each lane are then dropped onto the lane
//! wire by `place_passive_chains`, and the bridge caps by `place_bridge_passives`
//! (both already run in `select::run_single`, after layout, before routing).
//!
//! Because u1's A(top) / B(bottom) pins sit at two different heights, the two
//! lanes separate automatically: each passive inherits its lane from whichever
//! anchor pin its chain hangs off. We therefore do **not** touch any RES/CAP
//! box here — doing so previously fought the inline passes and produced the
//! stranded / zig-zagged wires.
//!
//! Invoked from `FlowLayouter::layout` as a placement override: AFTER
//! `phase_size` (boxes sized) and AFTER `phase_placement`, but BEFORE
//! `pin_place` — so pin sides are assigned from these final anchor positions.

use crate::vector::graph::{BoxKind, McVecGraph, VisualRole};

const MARGIN: f64 = 100.0;
/// Horizontal room reserved per inline passive/rung slot on a lane.
const SLOT: f64 = 220.0;

/// If the graph is a two-lane bridged-passive ladder, pin the two anchors at
/// left/right on a shared baseline and return `Some(())`. Passives are left for
/// the inline passes. Returns `None` (no-op) if the pattern doesn't match.
pub fn try_two_lane_ladder(graph: &mut McVecGraph) -> Option<()> {
    // ── 1. anchors: non-passive TwoPin boxes carrying I/O direction ──────────
    let anchor_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| {
            b.id >= 0
                && b.kind == BoxKind::TwoPin
                && !b.is_two_pin_passive()
                && (b.io_summary.outputs > 0 || b.io_summary.inputs > 0)
        })
        .map(|b| b.id)
        .collect();
    if anchor_ids.len() != 2 {
        return None;
    }

    // ── 2. require at least one BridgePassive — otherwise it isn't a bridged
    //       two-lane ladder and Flow's generic placement is fine. ─────────────
    let has_bridge = graph
        .boxes
        .iter()
        .any(|b| b.visual_role == Some(VisualRole::BridgePassive));
    if !has_bridge {
        return None;
    }

    // left = source (more outputs), right = sink. Deterministic on tie.
    let out_of = |id: i64| {
        graph
            .boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.io_summary.outputs)
            .unwrap_or(0)
    };
    let (left, right) = if out_of(anchor_ids[0]) >= out_of(anchor_ids[1]) {
        (anchor_ids[0], anchor_ids[1])
    } else {
        (anchor_ids[1], anchor_ids[0])
    };

    // ── 3. space the anchors so the inline resistors + caps have room ────────
    let n_pass = graph
        .boxes
        .iter()
        .filter(|b| b.is_two_pin_passive() && b.visual_role != Some(VisualRole::BridgePassive))
        .count();
    let n_bridge = graph
        .boxes
        .iter()
        .filter(|b| b.visual_role == Some(VisualRole::BridgePassive))
        .count();
    let span = ((n_pass + n_bridge) as f64) * SLOT;

    // ── 4. place ONLY the two anchors, same baseline, left / right ───────────
    let lw = graph.boxes.iter().find(|b| b.id == left).map(|b| b.w).unwrap_or(0.0);

    let y = MARGIN; // shared baseline → A-A and B-B are level (two horizontal lanes)
    let left_x = MARGIN;
    let right_x = MARGIN + lw + span;

    for b in graph.boxes.iter_mut() {
        if b.id == left {
            b.x = left_x;
            b.y = y;
        } else if b.id == right {
            b.x = right_x;
            b.y = y;
        }
        // NB: deliberately touch nothing else — RES/CAP are handled inline.
    }

    crate::vlog!(
        "[ladder] anchors only: left={} @x{:.0} right={} @x{:.0} (span for {} passives + {} rungs)",
        left,
        left_x,
        right,
        right_x,
        n_pass,
        n_bridge
    );

    Some(())
}