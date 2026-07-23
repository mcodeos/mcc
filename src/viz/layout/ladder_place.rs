// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Phase B — Apply the ladder model: topology -> coordinates
//!
//! [`super::ladder_model`] answers *what connects to what, in which column*. This
//! file answers *where*, and nothing more: it is pure arithmetic over the model.
//! There is no search, no heuristic, no obstacle map — if the model is right, the
//! picture is right.
//!
//! ```text
//!            col_x[0]   col_x[1]   col_x[2]   col_x[3]
//!               │          │          │          │
//!  lane_y[0] ───┼──RES1────┼──RES3────┼──RES6────┼───  u2.3
//!    u1.3       │        CAP2       CAP5         │
//!  lane_y[1] ───┼──────────┼──RES4────┼──RES7────┼───  u2.6
//!    u1.6           (`_`)  │          │          │
//!
//!  series  ->  x = midpoint(col_x[from], col_x[to]),  y = lane_y[lane]
//!  bridge  ->  x = col_x[col],                        y = mid(lane_y[a], lane_y[b])
//! ```
//!
//! ## Ownership contract
//! This pass is the **last writer** of every box it touches (anchors + all ladder
//! passives): geometry, `entry_points.{side,offset}` and `layout_hint`. It therefore
//! must run **after** `pin_place_pipeline` — otherwise `order_within_side` /
//! `straighten_facing_pairs` / `align_hub_to_spokes` each rewrite a piece of it and
//! the lanes stop being lanes. Every box it places is marked `geom_locked` so the
//! post-layout passive passes in `select.rs` keep their hands off.
//!
//! ## Why the anchors are placed here too
//! A lane is only straight if the anchor's pin *is* on it. `two_lane_ladder` froze
//! the anchor's pin **side** but not its **offset**, so `order_within_side` spread
//! the pins to 1/3 and 2/3 of an 82-high box while the lanes sat 150 apart —
//! i.e. outside the box entirely, and every lane wire had to jog. So the anchor's
//! height and its per-lane pin offsets are derived from `lane_y`, not the reverse.

use std::collections::HashMap;

use crate::vector::graph::boxdef::PinLayout;
use crate::vector::graph::boxdef::VisualRole;
use crate::vector::graph::{EntryPoint, EntrySide, McVecGraph, Point};

use super::entry_points::distribute_terminal_pins;
use super::ladder_model::LadderModel;

// ============================================================================
// Constants
// ============================================================================

/// Canvas margin: left anchor's x, and every anchor's y.
const MARGIN: f64 = 100.0;
/// Horizontal clearance between a series box and the neighbouring bridge box.
const COL_GAP: f64 = 16.0;
/// Column pitch floor (keeps short ladders from looking cramped).
const SLOT_MIN: f64 = 180.0;
/// Anchor edge -> first / last column.
const ANCHOR_GAP: f64 = 40.0;

// ============================================================================
// Output
// ============================================================================

/// The realised geometry. Returned so Phase C can diff the graph against it and
/// name whoever moved something afterwards.
#[derive(Debug, Clone, PartialEq)]
pub struct LadderGeometry {
    pub lane_y: Vec<f64>,
    pub col_x: Vec<f64>,
    pub anchor_h: f64,
    pub col_step: f64,
}

// ============================================================================
// Entry
// ============================================================================

/// Vertical spacing between lanes within a band. Must match islands::ROW_H.
const ROW_H: f64 = 80.0;

/// Place passives only (no anchors). Used by island band assembly so each band's
/// passives are placed relative to the band's origin, leaving anchor placement
/// to a single global pass (Phase D).
pub fn apply_ladder_model_at(
    graph: &mut McVecGraph,
    m: &LadderModel,
    origin: Point,
    _x_right: f64,
) -> Option<LadderGeometry> {
    // ── 0. Everything we need to read, read before we start mutating ─────────
    let plan = Plan::build(graph, m)?;

    // ── 1. Lanes: spread evenly within the band's height, spacing = ROW_H ────
    let lane_y: Vec<f64> = (0..m.n_lanes)
        .map(|k| origin.y + (k as f64 + 0.5) * ROW_H)
        .collect();

    // ── 2. Columns ───────────────────────────────────────────────────────────
    let col_step = (plan.elem_w + plan.bridge_w + 2.0 * COL_GAP).max(SLOT_MIN);
    let inner_left = origin.x + plan.left_w + ANCHOR_GAP;
    let col_x: Vec<f64> = (0..m.n_cols)
        .map(|c| inner_left + c as f64 * col_step)
        .collect();

    // ── 3. Series elements: on their lane, between two columns ───────────────
    for s in &m.series {
        let cx = (col_x[s.from_col] + col_x[s.to_col]) / 2.0;
        let cy = lane_y[s.lane];
        let Some(&(left_pin, right_pin)) = plan.series_pins.get(&s.box_id) else {
            continue;
        };
        place_two_pin(
            graph,
            s.box_id,
            cx,
            cy,
            plan.elem_w,
            plan.elem_h,
            (left_pin, EntrySide::Left),
            (right_pin, EntrySide::Right),
            VisualRole::SeriesInline,
        );
    }

    // ── 4. Bridges: on their column, across two lanes ────────────────────────
    for b in &m.bridges {
        let cx = col_x[b.col];
        let cy = (lane_y[b.lane_a] + lane_y[b.lane_b]) / 2.0;
        let Some(&(top_pin, bot_pin)) = plan.bridge_pins.get(&b.box_id) else {
            continue;
        };
        place_two_pin(
            graph,
            b.box_id,
            cx,
            cy,
            plan.bridge_w,
            plan.bridge_h,
            (top_pin, EntrySide::Top),
            (bot_pin, EntrySide::Bottom),
            VisualRole::BridgePassive,
        );
    }

    let geo = LadderGeometry {
        lane_y,
        col_x,
        anchor_h: m.n_lanes as f64 * ROW_H,
        col_step,
    };
    crate::vlog!(
        "[ladder-place] lanes @ {:?} | cols @ {:?} | anchor_h={:.0} col_step={:.0}",
        geo.lane_y
            .iter()
            .map(|v| v.round() as i64)
            .collect::<Vec<_>>(),
        geo.col_x
            .iter()
            .map(|v| v.round() as i64)
            .collect::<Vec<_>>(),
        geo.anchor_h,
        geo.col_step
    );
    Some(geo)
}

/// Place every box the model describes. Returns the geometry it committed to.
/// This is now a wrapper around `apply_ladder_model_at` + anchor placement.
pub fn apply_ladder_model(graph: &mut McVecGraph, m: &LadderModel) -> Option<LadderGeometry> {
    let origin = Point::new(MARGIN, MARGIN);
    let geo = apply_ladder_model_at(graph, m, origin, 0.0)?;

    // ── Anchors ───────────────────────────────────────────────────────────
    let right_x = geo.col_x.last().copied().unwrap_or(origin.x) + ANCHOR_GAP;

    place_anchor(
        graph,
        m.left,
        MARGIN,
        MARGIN,
        geo.anchor_h,
        EntrySide::Right,
        &m.lane_pin,
        &geo.lane_y,
    );
    place_anchor(
        graph,
        m.right,
        right_x,
        MARGIN,
        geo.anchor_h,
        EntrySide::Left,
        &m.right_lane_pin,
        &geo.lane_y,
    );

    Some(geo)
}

// ============================================================================
// Plan — every read, done up front (no borrow fights with the writes below)
// ============================================================================

struct Plan {
    left_w: f64,
    anchor_h_now: f64,
    elem_w: f64,
    elem_h: f64,
    bridge_w: f64,
    bridge_h: f64,
    /// lane -> the left anchor's pin on that lane (index == lane).
    left_lane_pin: Vec<i64>,
    right_lane_pin: Vec<i64>,
    /// series box -> (pin facing the lower column, pin facing the higher column)
    series_pins: HashMap<i64, (i64, i64)>,
    /// bridge box -> (pin on the upper lane, pin on the lower lane)
    bridge_pins: HashMap<i64, (i64, i64)>,
}

impl Plan {
    fn build(graph: &McVecGraph, m: &LadderModel) -> Option<Plan> {
        // ── sizes ──
        // Uniform per class: all series get one size, all bridges another. Uniform
        // symbols read better, and it makes the result immune to a single box having
        // been resized upstream (e.g. align_hub_to_spokes stretching whichever
        // passive `choose_root` happened to crown).
        let mut elem_long = 0.0f64;
        let mut elem_short = 0.0f64;
        for s in &m.series {
            if let Some(b) = graph.boxes.iter().find(|x| x.id == s.box_id) {
                elem_long = elem_long.max(b.w.max(b.h));
                elem_short = elem_short.max(b.w.min(b.h));
            }
        }
        let mut br_long = 0.0f64;
        let mut br_short = 0.0f64;
        for b in &m.bridges {
            if let Some(bx) = graph.boxes.iter().find(|x| x.id == b.box_id) {
                br_long = br_long.max(bx.w.max(bx.h));
                br_short = br_short.max(bx.w.min(bx.h));
            }
        }
        if elem_long <= 0.0 {
            elem_long = 110.0;
        }
        if elem_short <= 0.0 {
            elem_short = 82.0;
        }
        if br_long <= 0.0 {
            br_long = 110.0;
        }
        if br_short <= 0.0 {
            br_short = 82.0;
        }

        let left_box = graph.boxes.iter().find(|b| b.id == m.left)?;
        let right_box = graph.boxes.iter().find(|b| b.id == m.right)?;
        let anchor_h_now = left_box.h.max(right_box.h);
        let left_w = left_box.w;

        // ── anchor pin per lane ──
        // The model gives the left anchor's seed pins directly; the right anchor's
        // are recovered through net_col (nid -> lane).
        let left_lane_pin = m.lane_pin.clone();
        if left_lane_pin.len() != m.n_lanes {
            return None;
        }
        let mut right_lane_pin: Vec<i64> = vec![-1; m.n_lanes];
        for net in &graph.nets {
            let Some(&(lane, _)) = m.net_col.get(&net.nid) else {
                continue;
            };
            if let Some(e) = net.endpoints.iter().find(|e| e.box_id == m.right) {
                right_lane_pin[lane] = e.pin_id;
            }
        }
        if right_lane_pin.iter().any(|&p| p < 0) {
            return None;
        }

        // ── element pin orientation, derived from net_col ──
        //   series: the pin on the lower-column net faces Left.
        //   bridge: the pin on the upper lane's net faces Top.
        let mut series_pins: HashMap<i64, (i64, i64)> = HashMap::new();
        for s in &m.series {
            let mut lo: Option<i64> = None;
            let mut hi: Option<i64> = None;
            for net in &graph.nets {
                let Some(e) = net.endpoints.iter().find(|e| e.box_id == s.box_id) else {
                    continue;
                };
                let Some(&(_, col)) = m.net_col.get(&net.nid) else {
                    continue;
                };
                if col == s.from_col {
                    lo = Some(e.pin_id);
                } else if col == s.to_col {
                    hi = Some(e.pin_id);
                }
            }
            match (lo, hi) {
                (Some(a), Some(b)) if a != b => {
                    series_pins.insert(s.box_id, (a, b));
                }
                _ => {
                    crate::vlog!(
                        "[ladder-place] series #{} pin orientation unresolved -> left as-is",
                        s.box_id
                    );
                }
            }
        }

        let mut bridge_pins: HashMap<i64, (i64, i64)> = HashMap::new();
        for b in &m.bridges {
            let mut top: Option<i64> = None;
            let mut bot: Option<i64> = None;
            for net in &graph.nets {
                let Some(e) = net.endpoints.iter().find(|e| e.box_id == b.box_id) else {
                    continue;
                };
                let Some(&(lane, _)) = m.net_col.get(&net.nid) else {
                    continue;
                };
                if lane == b.lane_a {
                    top = Some(e.pin_id);
                } else if lane == b.lane_b {
                    bot = Some(e.pin_id);
                }
            }
            match (top, bot) {
                (Some(t), Some(d)) if t != d => {
                    bridge_pins.insert(b.box_id, (t, d));
                }
                _ => {
                    crate::vlog!(
                        "[ladder-place] bridge #{} pin orientation unresolved -> left as-is",
                        b.box_id
                    );
                }
            }
        }

        Some(Plan {
            left_w,
            anchor_h_now,
            elem_w: elem_long,
            elem_h: elem_short,
            bridge_w: br_short, // vertical: short side across
            bridge_h: br_long,
            left_lane_pin,
            right_lane_pin,
            series_pins,
            bridge_pins,
        })
    }
}

// ============================================================================
// Writers
// ============================================================================

/// Anchor: box top-left + height, every pin on `side`, lane pins pinned onto their
/// lane's y. This is what makes the lanes straight.
fn place_anchor(
    graph: &mut McVecGraph,
    id: i64,
    x: f64,
    y: f64,
    h: f64,
    side: EntrySide,
    lane_pin: &[i64],
    lane_y: &[f64],
) {
    let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) else {
        return;
    };
    b.x = x;
    b.y = y;
    b.h = h;
    let connected: Vec<(i64, f64)> = lane_pin
        .iter()
        .zip(lane_y.iter())
        .map(|(&p, &ly)| (p, (ly - y) / h))
        .collect();
    distribute_terminal_pins(b, side, &connected);
    b.geom_locked = true;
}

/// A ladder passive: centred at `(cx, cy)`, sized `w x h`, with exactly two pins on
/// the two given sides. `visual_role` is set explicitly so the three passive passes
/// can skip it (both locks: `geom_locked` + `visual_role`).
#[allow(clippy::too_many_arguments)]
fn place_two_pin(
    graph: &mut McVecGraph,
    id: i64,
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    a: (i64, EntrySide),
    b_side: (i64, EntrySide),
    role: VisualRole,
) {
    let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) else {
        return;
    };
    b.w = w;
    b.h = h;
    b.x = cx - w / 2.0;
    b.y = cy - h / 2.0;

    let name_of = |bx: &crate::vector::graph::McVecBox, pid: i64| -> String {
        bx.entry_points
            .iter()
            .find(|e| e.pin_id == pid)
            .map(|e| e.pin_name.clone())
            .or_else(|| {
                bx.pins
                    .iter()
                    .find(|p| p.id == pid)
                    .map(|p| p.id.to_string())
            })
            .unwrap_or_else(|| pid.to_string())
    };
    let (a_pin, a_side) = a;
    let (b_pin, b_sd) = b_side;
    let a_name = name_of(b, a_pin);
    let b_name = name_of(b, b_pin);

    b.entry_points = vec![
        EntryPoint {
            pin_id: a_pin,
            pin_name: a_name.clone(),
            side: a_side.clone(),
            offset: 0.5,
        },
        EntryPoint {
            pin_id: b_pin,
            pin_name: b_name.clone(),
            side: b_sd.clone(),
            offset: 0.5,
        },
    ];

    let mut hint = PinLayout::default();
    let mut put = |side: &EntrySide, name: String, pid: i64| {
        let v = match side {
            EntrySide::Left => &mut hint.left,
            EntrySide::Right => &mut hint.right,
            EntrySide::Top => &mut hint.top,
            EntrySide::Bottom => &mut hint.bottom,
        };
        v.push(name);
        v.push(pid.to_string());
    };
    put(&a_side, a_name, a_pin);
    put(&b_sd, b_name, b_pin);
    b.set_layout_hint(hint);
    b.visual_role = Some(role);
    b.geom_locked = true;
}

// ============================================================================
// Phase C — the model is the truth; diff the graph against it
// ============================================================================

/// Re-read the graph and report every box that no longer matches what Phase B
/// committed. Call right before routing. Pure read; logs only.
pub fn probe_ladder_placement(graph: &McVecGraph, m: &LadderModel, geo: &LadderGeometry) {
    let mut violations = 0usize;
    let name = |id: i64| -> String {
        graph
            .boxes
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| format!("#{id}"))
    };

    // normalize_positions applies a rigid shift, so compare against the *offset*
    // between a reference box and each box, not against absolute coordinates.
    let Some(left) = graph.boxes.iter().find(|b| b.id == m.left) else {
        return;
    };
    let dx = left.x - MARGIN;
    let dy = left.y - MARGIN;

    let mut check = |id: i64, want_cx: f64, want_cy: f64, want_horizontal: bool| {
        let Some(b) = graph.boxes.iter().find(|b| b.id == id) else {
            return;
        };
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        if (cx - dx - want_cx).abs() > 1.0 || (cy - dy - want_cy).abs() > 1.0 {
            crate::vlog!(
                "[ladder-check] VIOLATION {} moved: want ({want_cx:.0},{want_cy:.0}) got ({:.0},{:.0})",
                name(id),
                cx - dx,
                cy - dy
            );
            violations += 1;
        }
        if (b.w > b.h) != want_horizontal {
            crate::vlog!(
                "[ladder-check] VIOLATION {} orientation: want {} got {}x{} (renderer rotates on h>w)",
                name(id),
                if want_horizontal { "horizontal" } else { "vertical" },
                b.w,
                b.h
            );
            violations += 1;
        }
    };

    for s in &m.series {
        let cx = (geo.col_x[s.from_col] + geo.col_x[s.to_col]) / 2.0;
        check(s.box_id, cx, geo.lane_y[s.lane], true);
    }
    for b in &m.bridges {
        let cy = (geo.lane_y[b.lane_a] + geo.lane_y[b.lane_b]) / 2.0;
        check(b.box_id, geo.col_x[b.col], cy, false);
    }

    // Anchor pins must sit exactly on their lanes, or the lanes are not straight.
    for (anchor, pins) in [(m.left, &m.lane_pin)] {
        let Some(b) = graph.boxes.iter().find(|x| x.id == anchor) else {
            continue;
        };
        for (lane, &pid) in pins.iter().enumerate() {
            let Some(ep) = b.entry_points.iter().find(|e| e.pin_id == pid) else {
                continue;
            };
            let y = b.y + b.h * ep.offset - dy;
            if (y - geo.lane_y[lane]).abs() > 1.0 {
                crate::vlog!(
                    "[ladder-check] VIOLATION {} pin {pid} off lane {lane}: want y={:.0} got {:.0}",
                    name(anchor),
                    geo.lane_y[lane],
                    y
                );
                violations += 1;
            }
        }
    }

    if violations == 0 {
        crate::vlog!("[ladder-check] clean: graph matches the model");
    } else {
        crate::vlog!("[ladder-check] {violations} violation(s) — someone wrote after Phase B");
    }
}
