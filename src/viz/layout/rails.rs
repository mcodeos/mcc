// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-3 —— Rail triage (implementation of contract C1, MC_SCHEMATIC_ROADMAP_v6 §1.2)
//!
//! For every net carrying a [`RailSpec`] (resolved from port declarations by the
//! projection layer viz/project.rs):
//!
//! ```text
//! driver(N) = the box owning the endpoint that spec.driver_pin points to; None = passive power domain (GND belongs here)
//!
//! R-1  rail without driver: draw no edges at all.
//!      Top level: no symbols on the consumer side either (the block diagram draws not even GND, target figure 1).
//!      Sub-layers: each endpoint gets an in-place symbol (Ground class → ground symbol pointing down; Power class → terminal pointing up).
//!
//! R-2  rail with a driver, for each consumer C:
//!      draw the driver → C edge ⟺ C is a power-domain node (has an Out endpoint on a
//!      Power rail) or the hub of this layer (the box with the highest signal degree).
//!
//! R-3  consumers judged "no edge" by R-2:
//!      top level places no symbols; sub-layers place an in-place rail terminal symbol
//!      (dot + net name, pointing up).
//! ```
//!
//! ## Terminals are not boxes (discipline 11)
//! All R-1/R-3 symbols go into `graph.rail_decorations` (pin render attributes):
//! zero layout cost, zero routing cost, never in `graph.boxes`.
//! Only R-2 driver segments build real `VizNet`s participating in routing, both ends being real boxes.
//!
//! ## C5 · top-level block diagram draws no passives
//! The top level (`is_top == true`) additionally removes two-pin passives (R/C/L) from
//! the canvas and revokes their endpoints from signal nets; nets emptied (<2 endpoints)
//! are deleted too —— decoupling/pull-up resistors belong to the device-level view.
//!
//! ## Removed (anti-pattern §2.3 "name as criterion")
//! `explode_power_rails_to_flags` / `is_rail_box` / `name_has_power_token` ——
//! the old machine that indiscriminately exploded flags per (rail, consumer) with no
//! driver concept is removed wholesale.

use std::collections::{HashMap, HashSet};

use crate::vector::graph::naming;
use crate::vector::graph::graphdef::RailDecoration;
use crate::vector::graph::netdef::{IoDirection, NetRole};
use crate::vector::graph::{
    BoxKind, EndpointRef, EntryPoint, EntrySide, IoSummary, McVecBox, McVecGraph, NetKind, Symbol,
    VizNet,
};
use crate::vector::model::RailClass;

use super::normalize::{compute_canvas, normalize_positions};

/// Whether this is a power/ground label box.
///
/// ★ P7-3: the old `is_rail_box` was `(symbol.is_power_rail() || kind==PowerLabel)
/// && name_has_power_token(name)` —— the name token table was deleted along with the
/// explosion machine (anti-pattern §2.3 "name as criterion"). Replaced with a pure kind
/// check: rail flag boxes no longer exist after P7-3; the remaining PowerLabel boxes are
/// net label boxes made by `apply_net_labels` (which also must be excluded from core layout).
/// The 20+ downstream "exclusion guards" (pin_place / passive_inline / islands / sp /
/// ladder / coalesce / semantic) keep their semantics; only the criterion changed from
/// name to structure.
pub fn is_rail_box(b: &McVecBox) -> bool {
    b.kind == BoxKind::PowerLabel
}

/// Net id base for driver segments (avoids conflicts with existing nids)
const DRIVER_NET_ID_BASE: i64 = 9_600_000_000;

/// ★ P7-3 main entry: run the R-1/R-2/R-3 triage on this layer + (top level) C5.
pub fn classify_rails(graph: &mut McVecGraph, is_top: bool) {
    let has_rails = graph.nets.iter().any(|n| n.rail.is_some());
    if !has_rails {
        if is_top {
            drop_top_passives(graph);
        }
        return;
    }

    // ── Per-box metadata (computed before deleting any nets) ────────────
    // Power-domain nodes: boxes owning an Out endpoint on a Power rail (modldo.VCC / moddcdc.VCC_1V2)
    let mut power_domain_boxes: HashSet<i64> = HashSet::new();
    // Signal degree: participation count in this layer's signal nets (hub = highest, ties by smallest id).
    // ★ Both Signal and SubModuleIO count —— promote (P08) rewrites cross-module Signal
    //   nets into SubModuleIO; counting only Signal yields an empty set and hub detection
    //   breaks (hit in P7-3 field testing).
    let mut signal_degree: HashMap<i64, usize> = HashMap::new();
    for net in &graph.nets {
        match net.kind {
            NetKind::Signal | NetKind::SubModuleIO => {
                for b in net.box_ids() {
                    *signal_degree.entry(b).or_insert(0) += 1;
                }
            }
            _ => {}
        }
        if let Some(spec) = &net.rail {
            if spec.class == RailClass::Power {
                for e in &net.endpoints {
                    if e.io_type == IoDirection::Output {
                        power_domain_boxes.insert(e.box_id);
                    }
                }
            }
        }
    }
    let hub: Option<i64> = signal_degree
        .iter()
        .max_by_key(|(id, deg)| (**deg, -*id))
        .map(|(id, _)| *id);

    // ── Triage each rail net ────────────────────────────────────────────
    let mut driver_edges: Vec<VizNet> = Vec::new();
    let mut decorations: Vec<RailDecoration> = Vec::new();
    let mut keep = vec![true; graph.nets.len()];
    let mut next_nid = DRIVER_NET_ID_BASE;

    for (idx, net) in graph.nets.iter().enumerate() {
        let Some(spec) = net.rail.clone() else { continue };
        keep[idx] = false; // the original rail net is always replaced (edges/decorations/deletion)

        // First endpoint per box as representative (multiple pins in one box = duplicate endpoints of the same consumer)
        let mut per_box: Vec<(i64, EndpointRef)> = Vec::new();
        for e in &net.endpoints {
            if !per_box.iter().any(|(b, _)| *b == e.box_id) {
                per_box.push((e.box_id, e.clone()));
            }
        }

        let driver = spec
            .driver_pin
            .and_then(|pin| per_box.iter().find(|(_, e)| e.pin_id == pin).cloned());

        crate::vlog!(
            "[layout::rails] layer='{}' rail net '{}' (class={:?}, driver_pin={:?}): {} endpoint(s) over {} box(es) → {:?}",
            graph.name,
            net.name,
            spec.class,
            spec.driver_pin,
            net.endpoints.len(),
            per_box.len(),
            driver.as_ref().map(|(b, e)| (b, e.pin_id))
        );

        match driver {
            None => {
                // ── R-1: no driver (GND / generation side not found) ──────────
                // S1: every GND endpoint (pin by pin, including multiple pins in one box) gets exactly 1 symbol
                if !is_top {
                    for e in &net.endpoints {
                        decorations.push(RailDecoration {
                            box_id: e.box_id,
                            pin_id: e.pin_id,
                            is_ground: spec.class == RailClass::Ground,
                            label: net.name.clone(),
                        });
                    }
                }
            }
            Some((drv_box, drv_ep)) => {
                // ── R-2 / R-3 ────────────────────────────────────────────
                let mut driver_consumed = false;
                for (cbox, cep) in &per_box {
                    if *cbox == drv_box {
                        continue;
                    }
                    let qualifies = power_domain_boxes.contains(cbox) || Some(*cbox) == hub;
                    if qualifies {
                        driver_consumed = true;
                        let eps = vec![drv_ep.clone(), cep.clone()];
                        driver_edges.push(VizNet::new(
                            next_nid,
                            net.name.clone(),
                            NetKind::Power,
                            NetRole::Rail { volt: spec.volt.clone() },
                            eps,
                        ));
                        next_nid += 1;
                    } else if !is_top {
                        // R-3 sub-layer: in-place rail terminal
                        decorations.push(RailDecoration {
                            box_id: cep.box_id,
                            pin_id: cep.pin_id,
                            is_ground: false,
                            label: net.name.clone(),
                        });
                    }
                }
                // Sub-layer: a driver pin not consumed by any driver segment also gets a
                // terminal, otherwise the pin dangles visually (power comes from here;
                // draw a dot + net name).
                if !is_top && !driver_consumed {
                    decorations.push(RailDecoration {
                        box_id: drv_box,
                        pin_id: drv_ep.pin_id,
                        is_ground: false,
                        label: net.name.clone(),
                    });
                }
            }
        }
    }

    // ── Apply: rail nets → driver segments + decorations ────────────────
    let n_rail = keep.iter().filter(|k| !**k).count();
    let mut idx = 0usize;
    graph.nets.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
    graph.nets.extend(driver_edges);
    graph.rail_decorations.extend(decorations);

    crate::vlog!(
        "[layout::rails] P7-3: {} rail net(s) classified → {} driver edge(s), {} decoration(s) (is_top={})",
        n_rail,
        graph.nets.iter().filter(|n| n.nid >= DRIVER_NET_ID_BASE).count(),
        graph.rail_decorations.len(),
        is_top
    );

    // ── C5: top level draws no passives ─────────────────────────────────
    if is_top {
        drop_top_passives(graph);
    }
}

/// ★ C5: remove two-pin passives from the top level (block-diagram granularity) and
/// revoke their endpoints on signal nets. Nets drained to <2 endpoints (e.g. the _WP
/// pull-up left with only flash.3) are deleted too.
fn drop_top_passives(graph: &mut McVecGraph) {
    let passive_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.is_two_pin_passive())
        .map(|b| b.id)
        .collect();
    if passive_ids.is_empty() {
        return;
    }
    let n_boxes = passive_ids.len();
    graph.boxes.retain(|b| !passive_ids.contains(&b.id));

    let mut dropped_nets = 0usize;
    let mut cleaned: Vec<VizNet> = Vec::with_capacity(graph.nets.len());
    for mut net in std::mem::take(&mut graph.nets) {
        let before = net.endpoints.len();
        net.endpoints.retain(|e| !passive_ids.contains(&e.box_id));
        if net.endpoints.len() < 2 && before >= 2 {
            dropped_nets += 1;
            continue; // drained nets (single dangling end) are not drawn
        }
        cleaned.push(net);
    }
    graph.nets = cleaned;
    crate::vlog!(
        "[layout::rails] C5: dropped {} top-level passive box(es), {} emptied net(s)",
        n_boxes,
        dropped_nets
    );
}

// ============================================================================
// ★ Stage 1: net labels / air wires (long-net → named stubs)
// ============================================================================
//
// Long signal nets spanning the whole graph pass through a bunch of boxes → a bunch of crossings → a bunch of jumpers (bridges), the graph becomes messy. Industrial schematic
// standard practice is **net labels (net label / air wires)**: don't draw that long wire, but place a same-name short label stub next to each endpoint,
// same name = electrically connected. This pass transforms "long signal nets" into such label stubs:
//   - Create a **single-pin PowerLabel** next to each endpoint (reuses existing flag rendering, same style as sub-graph boundary ports) +
//     one **short stub** (label pin ↔ original pin), then **delete that long net**.
//   - Only modify nets of `NetKind::Signal` with **span over threshold**; power/ground (already flags), buses, and nets with either endpoint
//     already connected to label/flag are not touched.
//
// Must run **after layout, before routing** (at this point boxes have coordinates, can judge "long" by span; routing hasn't run yet,
// modifying boxes is safe). Hooked in api.rs Phase 1.8. Returns new canvas size (added label boxes, boundary needs recalculation).

const NETLABEL_LONG_SPAN: f64 = 650.0; // Span over this value (px) to convert to air wire (adjustable)
const NETLABEL_GAP: f64 = 42.0; // Distance of label from pin
const NETLABEL_W: f64 = 14.0;
const NETLABEL_H: f64 = 14.0;

/// Pin coordinates = box edge + offset (consistent with renderer pin_position, inlined to avoid cross-module dependencies).
fn pin_xy(b: &McVecBox, ep: &EntryPoint) -> (f64, f64) {
    match ep.side {
        EntrySide::Top => (b.x + b.w * ep.offset, b.y),
        EntrySide::Bottom => (b.x + b.w * ep.offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * ep.offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * ep.offset),
    }
}

/// ★ Stage 1 main entry: convert long signal nets to net label stubs. Returns `Some(new canvas)` if changed, else `None`.
pub fn apply_net_labels(graph: &mut McVecGraph) -> Option<(f64, f64)> {
    // 1. (box_id, pin_id) → (pin coordinates, side); record which boxes are labels/flags (PowerLabel).
    let mut pin_pos: HashMap<(i64, i64), ((f64, f64), EntrySide)> = HashMap::new();
    let mut label_boxes: HashSet<i64> = HashSet::new();
    // ★ Stage A (A3): a net touching any two-pin passive must keep a real wire (never an air-wire),
    //   otherwise a plain series R/C loop turns into unreadable dangling labels (see image2).
    let mut passive_boxes: HashSet<i64> = HashSet::new();
    for b in &graph.boxes {
        if b.kind == BoxKind::PowerLabel {
            label_boxes.insert(b.id);
        }
        if b.is_two_pin_passive() {
            passive_boxes.insert(b.id);
        }
        for ep in &b.entry_points {
            pin_pos.insert((b.id, ep.pin_id), (pin_xy(b, ep), ep.side.clone()));
        }
    }

    // New box / new net ids increment from existing max value, eliminating collisions (two namespaces are independent).
    let mut next_box = graph.boxes.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    let mut next_net = graph.nets.iter().map(|n| n.nid).max().unwrap_or(0) + 1;

    let mut new_boxes: Vec<McVecBox> = Vec::new();
    let mut new_stubs: Vec<VizNet> = Vec::new();
    let mut drop_idx: HashSet<usize> = HashSet::new();

    for (idx, net) in graph.nets.iter().enumerate() {
        // ★ P7-5 S9: single-endpoint signal-class nets are module-boundary
        // signals whose pseudo-endpoint was removed by the projection pass.
        // The router gives them an empty route — they would render as
        // nothing (dangling). Terminate the open end on a net-label stub,
        // same style as the target figure's edge labels.
        if net.endpoints.len() == 1 {
            let named = matches!(net.kind, NetKind::Signal | NetKind::SubModuleIO)
                && !net.name.is_empty()
                && !net.name.starts_with("__net");
            let e = &net.endpoints[0];
            if named && !label_boxes.contains(&e.box_id) {
                if let Some(((px, py), side)) = pin_pos.get(&(e.box_id, e.pin_id)).cloned() {
                    let (is_gnd, lio) = if naming::is_ground(&net.name) {
                        (true, IoDirection::Ground)
                    } else {
                        (false, IoDirection::Passive)
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
                    drop_idx.insert(idx);
                }
            }
            continue;
        }
        if !matches!(net.kind, NetKind::Signal) {
            continue; // Only process signal nets
        }
        if net.endpoints.len() < 2 {
            continue;
        }
        // ★ FIX: Only convert "meaningfully named" nets to air wires. Anonymous nets (__net_N / empty name) converting to labels is meaningless
        //   —— both ends labeled __net_25 users can't read, equals making visible wires disappear (this regression root cause). Anonymous nets drawn normally.
        if net.name.is_empty() || net.name.starts_with("__net") {
            continue;
        }
        // Either endpoint already label/flag → already "labeled", don't repeat (includes sub-graph boundary ports, power flags).
        if net
            .endpoints
            .iter()
            .any(|e| label_boxes.contains(&e.box_id))
        {
            continue;
        }
        // ★ Stage A (A3): never air-wire a net that touches a two-pin passive — its pins must
        //   be reached by real wires.
        if net
            .endpoints
            .iter()
            .any(|e| passive_boxes.contains(&e.box_id))
        {
            continue;
        }
        // ★ Stage A (A3): a net spanning only two boxes is a plain point-to-point wire, not a
        //   long bus worth labelling — route it normally regardless of pixel span.
        if net.box_ids().len() < 3 {
            continue;
        }
        // Endpoint coordinates + span (max pairwise distance between endpoints).
        let pts: Vec<(f64, f64)> = net
            .endpoints
            .iter()
            .filter_map(|e| pin_pos.get(&(e.box_id, e.pin_id)).map(|(p, _)| *p))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        let mut span = 0.0_f64;
        for a in 0..pts.len() {
            for b in (a + 1)..pts.len() {
                let d = ((pts[a].0 - pts[b].0).powi(2) + (pts[a].1 - pts[b].1).powi(2)).sqrt();
                if d > span {
                    span = d;
                }
            }
        }
        if span < NETLABEL_LONG_SPAN {
            continue; // Short nets drawn normally
        }

        // ── Long signal net → one same-name label + one short stub per endpoint ──
        let is_gnd = naming::is_ground(&net.name);
        let lio = if is_gnd {
            IoDirection::Ground
        } else {
            IoDirection::Passive
        };
        for e in &net.endpoints {
            let ((px, py), side) = match pin_pos.get(&(e.box_id, e.pin_id)) {
                Some(v) => (v.0, v.1.clone()),
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
        return None;
    }

    // Apply: delete long net, add label + stub.
    let mut i = 0usize;
    graph.nets.retain(|_| {
        let keep = !drop_idx.contains(&i);
        i += 1;
        keep
    });
    let n_lbl = new_boxes.len();
    let n_drop = drop_idx.len();
    graph.boxes.extend(new_boxes);
    graph.nets.extend(new_stubs);

    crate::vlog!(
        "[viz::net_label] layer '{}' bid={}: {} long signal net(s) → {} label stub(s)",
        graph.name,
        graph.bid,
        n_drop,
        n_lbl
    );

    // Labels may extend past original canvas / land in negative coordinates → renormalize + recompute canvas (no routing yet, only modifying boxes is safe).
    normalize_positions(graph);
    Some(compute_canvas(graph))
}

/// ★ P7-5: create one net-label box + one short stub net for a single endpoint
/// (shared by the long-net conversion and the S9 single-endpoint rescue).
/// The label sits `NETLABEL_GAP` away from the pin along its outward side;
/// the label's own pin faces back so the stub is a short straight line.
/// Steps further outward while the label rect (audit-inflated) would overlap
/// any other box — keeps the G12 collision gate at zero.
#[allow(clippy::too_many_arguments)]
fn push_label_stub(
    net_name: &str,
    net_kind: &NetKind,
    is_gnd: bool,
    lio: IoDirection,
    e: &crate::vector::graph::netdef::EndpointRef,
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
    // ★ P7-5 G12 guard: try the pin's outward side first, stepping further
    // out; when blocked (e.g. the outward direction points into a
    // neighbouring box), flip through the other three sides before giving up.
    // The avoidance rect uses the ESTIMATED TEXT width (the 14px label box is
    // a click target; the rendered text is wider and is what must not hit
    // neighbouring boxes).
    const INFLATE: f64 = 8.0;
    let text_w = (net_name.chars().count() as f64 * 7.0).max(NETLABEL_W);
    let rect_clear = |bx: f64, by: f64, new_boxes: &Vec<McVecBox>| {
        let hits = |ob: &McVecBox| {
            bx < ob.x + ob.w + INFLATE
                && bx + text_w + INFLATE > ob.x
                && by < ob.y + ob.h + INFLATE
                && by + NETLABEL_H + INFLATE > ob.y
        };
        // Earlier labels created by this same pass are boxes too.
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
    let pin_id = box_id; // Single pin, pin_id reuses box_id for uniqueness

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
    // stub inherits original kind → SubModuleIO air wire stubs remain purple, consistent with same-name other segments visually
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

    fn mk_rail(id: i64, name: &str, is_ground: bool) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground },
            None,
            None,
            1,
            IoSummary::new(),
            name.to_string(),
            Vec::new(),
        )
    }

    fn mk_mod(id: i64, name: &str) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            BoxKind::SubModule,
            Symbol::Module,
            None,
            None,
            4,
            IoSummary::new(),
            name.to_string(),
            Vec::new(),
        )
    }

    /// Set box position + one pin (for net-label testing).
    fn placed(mut b: McVecBox, x: f64, w: f64, pin: i64, side: EntrySide) -> McVecBox {
        b.x = x;
        b.y = 0.0;
        b.w = w;
        b.h = 100.0;
        b.entry_points = vec![EntryPoint {
            pin_id: pin,
            pin_name: "S".into(),
            side,
            offset: 0.5,
        }];
        b
    }

    #[test]
    fn net_label_converts_long_signal_net() {
        // Stage A (A3): a net touching only 2 boxes is a point-to-point wire
        // and is NOT converted to labels regardless of span.
        // This test verifies the 3-box minimum requirement.
        let mut g = McVecGraph::new(0, "main".into());
        // A (right) at (100,50) → C (pass-through) → B (left) at (1000,50)
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 1000.0, 100.0, 21, EntrySide::Left));
        g.boxes
            .push(placed(mk_mod(3, "C"), 400.0, 100.0, 31, EntrySide::Right));
        g.nets.push(VizNet::new(
            50,
            "SIG".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Output),
                EndpointRef::with_io(3, 31, "S", IoDirection::Input),
                EndpointRef::with_io(2, 21, "S", IoDirection::Input),
            ],
        ));

        let r = apply_net_labels(&mut g);
        assert!(
            r.is_some(),
            "Long signal net (3 boxes, span > 650) should be converted to label"
        );
        assert!(
            g.nets.iter().all(|n| n.nid != 50),
            "Original long net should be deleted"
        );
    }

    #[test]
    fn net_label_leaves_short_net_alone() {
        // A right pin (100,50) ↔ B left pin (150,50): span 50 < 650 → don't touch
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 150.0, 100.0, 21, EntrySide::Left));
        g.nets.push(VizNet::new(
            50,
            "SIG".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Output),
                EndpointRef::with_io(2, 21, "S", IoDirection::Input),
            ],
        ));

        let r = apply_net_labels(&mut g);
        assert!(r.is_none(), "Short net doesn't convert to label");
        assert_eq!(g.nets.len(), 1, "Short net stays as is");
        assert!(
            g.boxes.iter().all(|x| x.kind != BoxKind::PowerLabel),
            "Shouldn't create label boxes"
        );
    }

    #[test]
    fn net_label_skips_power_net() {
        // Same distance, but kind=Ground → don't process (power/ground have their own flag rendering)
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes
            .push(placed(mk_mod(1, "A"), 0.0, 100.0, 11, EntrySide::Right));
        g.boxes
            .push(placed(mk_mod(2, "B"), 1000.0, 100.0, 21, EntrySide::Left));
        g.nets.push(VizNet::new(
            50,
            "GND".into(),
            NetKind::Ground,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "S", IoDirection::Ground),
                EndpointRef::with_io(2, 21, "S", IoDirection::Ground),
            ],
        ));
        assert!(
            apply_net_labels(&mut g).is_none(),
            "Ground net doesn't convert to label"
        );
        assert_eq!(g.nets.len(), 1);
    }

    /// ★ P7-5 S9: a single-endpoint named signal net (module boundary pseudo
    /// endpoint removed) must be terminated on a net-label stub instead of
    /// rendering as nothing.
    #[test]
    fn net_label_rescues_single_endpoint_boundary_net() {
        let mut g = McVecGraph::new(0, "mcu513".into());
        g.boxes
            .push(placed(mk_mod(1, "uC"), 0.0, 100.0, 11, EntrySide::Right));
        g.nets.push(VizNet::new(
            50,
            "SPK_MUTE".into(),
            NetKind::SubModuleIO,
            NetRole::Signal,
            vec![EndpointRef::with_io(1, 11, "S", IoDirection::Output)],
        ));

        let r = apply_net_labels(&mut g);
        assert!(r.is_some(), "single-endpoint named net gets a label stub");
        assert!(
            g.nets.iter().all(|n| n.nid != 50),
            "dangling original net replaced"
        );
        let stub = g
            .nets
            .iter()
            .find(|n| n.endpoints.len() == 2)
            .expect("stub net connects label ↔ pin");
        assert_eq!(stub.name, "SPK_MUTE");
        let label = g
            .boxes
            .iter()
            .find(|b| b.kind == BoxKind::PowerLabel)
            .expect("label box created");
        assert_eq!(label.name, "SPK_MUTE");
        assert_eq!(stub.endpoints[0].box_id, label.id);
    }

    /// ★ P7-5 S9: an anonymous single-endpoint net stays untouched (a label
    /// reading "__net_7" carries no information).
    #[test]
    fn net_label_leaves_anonymous_dangling_net_alone() {
        let mut g = McVecGraph::new(0, "mcu513".into());
        g.boxes
            .push(placed(mk_mod(1, "uC"), 0.0, 100.0, 11, EntrySide::Right));
        g.nets.push(VizNet::new(
            50,
            "__net_7".into(),
            NetKind::SubModuleIO,
            NetRole::Signal,
            vec![EndpointRef::with_io(1, 11, "S", IoDirection::Output)],
        ));
        assert!(
            apply_net_labels(&mut g).is_none(),
            "anonymous dangling net is not rescued"
        );
        assert_eq!(g.nets.len(), 1);
    }

    // ── ★ P7-3 classify_rails triage tests (R-1 / R-2 / R-3 / C5) ──────────

    fn rail_net(
        nid: i64,
        name: &str,
        class: RailClass,
        driver_pin: Option<i64>,
        eps: Vec<(i64, i64, IoDirection)>,
    ) -> VizNet {
        let mut n = VizNet::new(
            nid,
            name.into(),
            if class == RailClass::Ground { NetKind::Ground } else { NetKind::Power },
            NetRole::Rail { volt: None },
            eps.into_iter()
                .map(|(b, p, io)| EndpointRef::with_io(b, p, name, io))
                .collect(),
        );
        n.rail = Some(crate::vector::model::RailSpec {
            class,
            driver_pin,
            volt: None,
        });
        n
    }

    #[test]
    fn r1_ground_no_driver_no_edge_no_symbol_at_top() {
        // R-1 top level: GND without driver —— no edges drawn, no symbols placed
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "A"));
        g.boxes.push(mk_mod(2, "B"));
        g.nets.push(rail_net(
            10,
            "GND",
            RailClass::Ground,
            None,
            vec![(1, 11, IoDirection::Passive), (2, 21, IoDirection::Passive)],
        ));
        classify_rails(&mut g, /*is_top=*/ true);
        assert!(g.nets.is_empty(), "GND net should be deleted: {:?}", g.nets);
        assert!(g.rail_decorations.is_empty(), "top-level R-1 places no symbols");
    }

    #[test]
    fn r1_ground_symbols_per_pin_at_sub_layer() {
        // R-1 sub-layer: every GND endpoint gets exactly 1 ground symbol (S1)
        let mut g = McVecGraph::new(0, "modA".into());
        g.boxes.push(mk_mod(1, "IC"));
        g.boxes.push(mk_mod(2, "C1"));
        g.nets.push(rail_net(
            10,
            "GND",
            RailClass::Ground,
            None,
            vec![
                (1, 11, IoDirection::Passive),
                (1, 12, IoDirection::Passive), // second GND pin in the same box
                (2, 21, IoDirection::Passive),
            ],
        ));
        classify_rails(&mut g, /*is_top=*/ false);
        assert!(g.nets.is_empty());
        assert_eq!(g.rail_decorations.len(), 3, "one symbol per endpoint (multiple pins in one box included)");
        assert!(g.rail_decorations.iter().all(|d| d.is_ground));
    }

    #[test]
    fn r2_edges_only_to_power_domain_and_hub() {
        // Distillation of the seven-line checklist: V3V3 = driver modldo → {moddcdc(power
        // domain✓), mcu513(hub✓), speaker(✗), flash(✗)} → exactly 2 driver edges;
        // R-3 top level places no symbols
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "modldo"));   // driver (VCC Out)
        g.boxes.push(mk_mod(2, "moddcdc"));  // power-domain node (VCC_1V2 Out is on another rail)
        g.boxes.push(mk_mod(3, "mcu513"));   // hub (8 signal nets → 2 here is already the max)
        g.boxes.push(mk_mod(4, "speaker"));
        // Signal nets: make mcu513 the hub
        g.nets.push(VizNet::new(
            20,
            "S1".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(3, 31, "S", IoDirection::Output),
                EndpointRef::with_io(4, 41, "S", IoDirection::Input),
            ],
        ));
        g.nets.push(VizNet::new(
            21,
            "S2".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(3, 32, "S", IoDirection::Output),
                EndpointRef::with_io(4, 42, "S", IoDirection::Input),
            ],
        ));
        // moddcdc's power-domain qualification: an Out endpoint on another Power rail (V1V2 is driven by it)
        g.nets.push(rail_net(
            11,
            "V1V2",
            RailClass::Power,
            Some(22),
            vec![(2, 22, IoDirection::Output), (3, 33, IoDirection::Input)],
        ));
        // Rail under test: V3V3
        g.nets.push(rail_net(
            10,
            "V3V3",
            RailClass::Power,
            Some(11),
            vec![
                (1, 11, IoDirection::Output),   // modldo.VCC = driver
                (2, 21, IoDirection::Input),    // moddcdc consumes
                (3, 34, IoDirection::Bidir),    // mcu513 consumes
                (4, 43, IoDirection::Input),    // speaker consumes
            ],
        ));
        classify_rails(&mut g, /*is_top=*/ true);

        // V1V2: driver moddcdc(2) → mcu513(3, hub) 1 edge; V3V3: modldo(1) → {moddcdc(2 power domain), mcu513(3 hub)} 2 edges
        let power_edges: Vec<&VizNet> = g
            .nets
            .iter()
            .filter(|n| matches!(n.kind, NetKind::Power))
            .collect();
        assert_eq!(power_edges.len(), 3, "V1V2 1 edge + V3V3 2 edges = 3 driver edges");
        let v33: Vec<(i64, i64)> = power_edges
            .iter()
            .filter(|n| n.name == "V3V3")
            .map(|n| (n.endpoints[0].box_id, n.endpoints[1].box_id))
            .collect();
        assert!(v33.contains(&(1, 2)), "modldo→moddcdc (power domain): {v33:?}");
        assert!(v33.contains(&(1, 3)), "modldo→mcu513 (hub): {v33:?}");
        assert!(!v33.iter().any(|(_, t)| *t == 4), "speaker R-3 draws no edge");
        assert!(g.rail_decorations.is_empty(), "top-level R-3 places no symbols");
    }

    #[test]
    fn r3_sub_layer_consumers_get_rail_terminals() {
        // R-3 sub-layer: consumers without edges get rail terminals (dot + net name, not ground)
        let mut g = McVecGraph::new(0, "modLDO".into());
        g.boxes.push(mk_mod(1, "ldo")); // driver (sub-layer component, pin without out → designated by spec)
        g.boxes.push(mk_mod(2, "CAP")); // ordinary consumer
        g.nets.push(rail_net(
            10,
            "VCC",
            RailClass::Power,
            Some(11),
            vec![(1, 11, IoDirection::Passive), (2, 21, IoDirection::Passive)],
        ));
        classify_rails(&mut g, /*is_top=*/ false);
        // Consumer CAP unqualified → no edge; sub-layer places a terminal; driver pin drew an edge so gets none
        assert!(g.nets.iter().all(|n| n.rail.is_none()), "rail nets should be replaced");
        // hub determination: no signal nets → hub=None; CAP has no power-domain qualification → 0 edges
        // driver pin not consumed by an edge → also gets a terminal
        assert_eq!(g.rail_decorations.len(), 2, "one terminal each for driver pin + consumer pin");
        assert!(g.rail_decorations.iter().all(|d| !d.is_ground));
        assert_eq!(g.rail_decorations[0].label, "VCC");
    }

    #[test]
    fn c5_top_layer_drops_two_pin_passives() {
        // C5: top level draws no passives; the drained _WP net disappears, cross-module net kept
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_mod(1, "flash"));
        g.boxes.push(mk_mod(2, "mcu"));
        let mut res = mk_mod(3, "RES");
        res.kind = BoxKind::TwoPin;
        res.symbol = Symbol::Resistor;
        res.class_name = "RES".into();
        g.boxes.push(res);
        // _WP: flash.3 ~ RES.1 —— after removing RES only 1 end remains → delete
        g.nets.push(VizNet::new(
            30,
            "_WP".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 13, "3", IoDirection::Passive),
                EndpointRef::with_io(3, 31, "1", IoDirection::Passive),
            ],
        ));
        // CSN: flash.1 ~ RES.2 ~ mcu.10 —— after removing RES still 2 ends → keep
        g.nets.push(VizNet::new(
            31,
            "CSN".into(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::with_io(1, 11, "1", IoDirection::Passive),
                EndpointRef::with_io(3, 32, "2", IoDirection::Passive),
                EndpointRef::with_io(2, 21, "10", IoDirection::Passive),
            ],
        ));
        classify_rails(&mut g, /*is_top=*/ true);
        assert!(!g.boxes.iter().any(|b| b.id == 3), "passive box should be deleted");
        assert_eq!(g.nets.len(), 1, "_WP deleted, CSN kept: {:?}", g.nets.iter().map(|n| &n.name).collect::<Vec<_>>());
        assert_eq!(g.nets[0].name, "CSN");
        assert_eq!(g.nets[0].endpoints.len(), 2);
    }

    #[test]
    fn is_rail_box_is_kind_based_not_name_based() {
        // ★ P7-3: the name_has_power_token keyword table is deleted —— the criterion is kind only
        assert!(is_rail_box(&mk_rail(1, "any name", true)), "PowerLabel kind is a rail box");
        assert!(!is_rail_box(&mk_mod(2, "V3V3_ldo_power")), "a name with a token still doesn't count");
    }
}
