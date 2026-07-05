// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Iteration 05b · Pin placement optimization (连通性优先的 pin-side / offset)
//!
//! Replaces five scattered pin-side passes (`face_core_neighbor`,
//! `assign_entry_points_refine`, `align_hub_to_spokes`, `order_pins_by_neighbor`,
//! `enforce_unique_offsets`) with a single ordered pipeline:
//!
//!   desired_side(A) → straighten_facing_pairs(B) → order_within_side(C) → enforce_unique_offsets
//!
//! The key insight: pin side should be decided by **connectivity first, semantics
//! as fallback** (not the reverse).  Hub pins are no longer exempt — they face
//! their neighbours just like leaf pins.  The `lr_only` flag preserves the
//! professional IC symbol convention (pins only on Left/Right).

use std::collections::{HashMap, HashSet};

use crate::vector::graph::box_def::{EntryPoint, EntrySide};
use crate::vector::graph::net_def::IoDirection;
use crate::vector::graph::McVecGraph;

use super::entry_points::{
    collect_box_centers, collect_box_rects, collect_pin_io_types, collect_pin_neighbors,
    enforce_unique_offsets, normalize_offsets_per_side, open_perpendicular_side, path_blocked,
    pick_side_by_direction, sides_warrant_switch,
};
use super::flow::pin_abs;
use super::rails::is_rail_box;

// ============================================================================
// Public API
// ============================================================================

/// Run the pin-placement pipeline on `graph`.
///
/// `hub_id` is optional — if `Some`, the hub is *not* exempt from side
/// flipping (unlike the old `face_core_neighbor`).  If `None`, all boxes are
/// treated equally.
///
/// `lr_only` (default `true`) forces pins to Left/Right only.  `false` allows
/// Top/Bottom sides for pins whose neighbours are clearly above/below.
///
/// `hub_keep_semantic` (default `true` for backward compatibility) keeps the
/// old behaviour where hub pins stick to Input=Left / Output=Right.  `false`
/// lets connectivity override semantics for hub pins too.
pub fn pin_place_pipeline(
    graph: &mut McVecGraph,
    hub_id: Option<i64>,
    lr_only: bool,
    hub_keep_semantic: bool,
) {
    // A: Connectivity-first desired side
    desired_side_pass(graph, hub_id, hub_keep_semantic, lr_only);

    // C: Order within side (crossing-minimizing) — runs FIRST so relative order is set
    order_within_side(graph);

    // B: Straighten facing pairs — overrides offsets for straight wires
    straighten_facing_pairs(graph);

    // D: Hub↔spoke alignment (PR-A — folded in from the former flow::align_hub_to_spokes).
    //    Stretches the hub box tall enough to span all its spoke Ys, then aligns each hub pin's
    //    offset onto its spoke's Y so hub↔peripheral wires run straight. Runs AFTER straighten so
    //    peripheral offsets are final, and BEFORE enforce_unique_offsets — because the hub is now
    //    tall, its offsets are not crowded, so the guard leaves them intact. (In the old code this
    //    ran in flow AFTER pin_place, and a trailing enforce_unique_offsets flattened the alignment
    //    right back out; that patch is now deleted.)
    if let Some(hub) = hub_id {
        align_hub_to_spokes(graph, hub);
    }

    // E: Enforce unique offsets (hard guard) — the final EntryPoint writer.
    enforce_unique_offsets(graph);
}

/// ★ PR-A — hub↔spoke alignment (moved here from `flow.rs` so pin_place is the single writer of
/// `EntryPoint.{side,offset}`).
///
/// Peripheral devices are already placed; this stretches the hub's box height to span the Y range
/// of its spoke connection points, then aligns each hub pin's offset onto its spoke's Y so
/// hub↔peripheral wires run straight. Peripherals are **not** moved (no new collisions). Only
/// left/right signal pins are aligned (top/bottom and power pins keep their offset).
fn align_hub_to_spokes(graph: &mut McVecGraph, root_id: i64) {
    let flag_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    // hub pin → target Y (= peripheral pin's absolute Y)
    let mut hub_targets: HashMap<i64, f64> = HashMap::new();
    for net in &graph.nets {
        let cores: Vec<&crate::vector::graph::EndpointRef> = net
            .endpoints
            .iter()
            .filter(|e| !flag_ids.contains(&e.box_id))
            .collect();
        if cores.len() < 2 {
            continue;
        }
        let hub_ep = match cores.iter().find(|e| e.box_id == root_id) {
            Some(e) => e,
            None => continue,
        };
        let periph_ep = match cores.iter().find(|e| e.box_id != root_id) {
            Some(e) => e,
            None => continue,
        };
        let pb = match graph.boxes.iter().find(|b| b.id == periph_ep.box_id) {
            Some(b) => b,
            None => continue,
        };
        let ty = pb
            .entry_points
            .iter()
            .find(|ep| ep.pin_id == periph_ep.pin_id)
            .map(|ep| pin_abs(pb, &ep.side, ep.offset).1)
            .unwrap_or(pb.y + pb.h / 2.0);
        hub_targets
            .entry(hub_ep.pin_id)
            .and_modify(|v| *v = (*v + ty) / 2.0)
            .or_insert(ty);
    }
    if hub_targets.is_empty() {
        return;
    }

    let min_y = hub_targets.values().cloned().fold(f64::INFINITY, f64::min);
    let max_y = hub_targets
        .values()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    let margin = 36.0;
    let new_y = min_y - margin;
    let new_h = ((max_y - min_y) + 2.0 * margin).max(60.0);

    if let Some(hub) = graph.boxes.iter_mut().find(|b| b.id == root_id) {
        hub.y = new_y;
        hub.h = new_h;
        for ep in &mut hub.entry_points {
            // Only align left/right signal pins (Y ↔ offset); top/bottom and power pins don't move
            if !matches!(ep.side, EntrySide::Left | EntrySide::Right) {
                continue;
            }
            if let Some(&ty) = hub_targets.get(&ep.pin_id) {
                ep.offset = ((ty - new_y) / new_h).clamp(0.02, 0.98);
            }
        }
    }
}

// ============================================================================
// A · Connectivity-first desired side
// ============================================================================

/// Determine the target side for every pin that is NOT frozen by the author's
/// `layout` hint.  Connectivity wins; semantics are the fallback.
fn desired_side_pass(
    graph: &mut McVecGraph,
    hub_id: Option<i64>,
    hub_keep_semantic: bool,
    lr_only: bool,
) {
    let pin_io = collect_pin_io_types(graph);
    let pin_neighbors = collect_pin_neighbors(graph);
    let box_centers = collect_box_centers(graph);
    let box_rects = collect_box_rects(graph);

    let mut total_reassigned = 0usize;

    for b in &mut graph.boxes {
        if is_rail_box(b) {
            continue;
        }

        // The hub is ONE box (the max-degree node flow passes in), not "every IC".
        // Only exempt it, and only when explicitly asked (default false ⇒ connectivity-first everywhere).
        if hub_keep_semantic && hub_id == Some(b.id) {
            continue;
        }

        // Collect authored pin sides from layout_hint — match by pin_name string OR pin_id
        let authored: HashSet<String> = b
            .layout_hint
            .as_ref()
            .map(|h| {
                h.left
                    .iter()
                    .chain(h.right.iter())
                    .chain(h.top.iter())
                    .chain(h.bottom.iter())
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        let bcx = b.x + b.w / 2.0;
        let bcy = b.y + b.h / 2.0;
        let mut moved = 0usize; // per-box counter

        for ep in &mut b.entry_points {
            // Freeze authored pins — match by pin-name string OR pin_id
            if authored.contains(&ep.pin_name) || authored.contains(&ep.pin_id.to_string()) {
                continue;
            }

            let io = pin_io
                .get(&(b.id, ep.pin_id))
                .copied()
                .unwrap_or(IoDirection::Unknown);

            let nbrs = pin_neighbors
                .get(&(b.id, ep.pin_id))
                .map(|v| v.as_slice())
                .unwrap_or(&[]);

            // Compute neighbour centroid
            let (sum_x, sum_y, n) = nbrs
                .iter()
                .filter_map(|nid| box_centers.get(nid))
                .fold((0.0f64, 0.0f64, 0usize), |(sx, sy, k), &(x, y)| {
                    (sx + x, sy + y, k + 1)
                });

            if n == 0 {
                // No core neighbours → fall back to semantic side
                let target = semantic_default_side(io);
                if target != ep.side {
                    ep.side = target;
                    moved += 1;
                }
                continue;
            }

            let (ncx, ncy) = (sum_x / n as f64, sum_y / n as f64);
            let dx = ncx - bcx;
            let dy = ncy - bcy;

            let preferred = pick_side_by_direction(dx, dy);

            // Collision-aware: don't face into a blocking box
            let mut exclude: HashSet<i64> = nbrs.iter().copied().collect();
            exclude.insert(b.id);
            let blocked = path_blocked((bcx, bcy), (ncx, ncy), &box_rects, &exclude);
            let (target, forced) = if blocked {
                match open_perpendicular_side((b.x, b.y, b.w, b.h), &preferred, &box_rects, b.id) {
                    Some(s) => (s, true),
                    None => (preferred, false),
                }
            } else {
                (preferred, false)
            };

            // L/R-only: project Top/Bottom back to Left/Right
            let (target, forced) = if lr_only {
                match target {
                    EntrySide::Top | EntrySide::Bottom => {
                        let projected = if dx >= 0.0 {
                            EntrySide::Right
                        } else {
                            EntrySide::Left
                        };
                        (projected, true) // forced: bypass hysteresis for hard lr_only constraint
                    }
                    s => (s, forced),
                }
            } else {
                (target, forced)
            };

            if target == ep.side {
                continue;
            }

            // Hysteresis for non-forced switches
            if !forced && !sides_warrant_switch(&ep.side, &target, dx, dy) {
                continue;
            }

            ep.side = target;
            moved += 1;
        }

        if moved > 0 {
            normalize_offsets_per_side(b);
        }
        total_reassigned += moved;
    }

    crate::vlog!(
        "[pin_place] desired_side: {} pins reassigned across {} boxes",
        total_reassigned,
        graph.boxes.len()
    );
}

/// Fallback side when a pin has no core neighbours.
fn semantic_default_side(io: IoDirection) -> EntrySide {
    match io {
        IoDirection::Input => EntrySide::Left,
        IoDirection::Output => EntrySide::Right,
        _ => EntrySide::Left, // Passive/Power/Unknown default to Left
    }
}

// ============================================================================
// B · Straighten facing pairs
// ============================================================================

/// For every net whose two core endpoints are on facing sides (A.Right ↔ B.Left
/// or A.Left ↔ B.Right, or vertical Top↔Bottom), align their offsets so the
/// connecting wire runs straight (horizontal or vertical).
fn straighten_facing_pairs(graph: &mut McVecGraph) {
    let flag_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    // Collect adjustments first, then apply (avoids borrow conflict)
    let mut adjustments: Vec<(i64, i64, f64)> = Vec::new();

    for net in &graph.nets {
        let cores: Vec<&crate::vector::graph::EndpointRef> = net
            .endpoints
            .iter()
            .filter(|e| !flag_ids.contains(&e.box_id))
            .collect();

        if cores.len() != 2 {
            continue;
        }

        let a = graph.boxes.iter().find(|b| b.id == cores[0].box_id);
        let b = graph.boxes.iter().find(|b| b.id == cores[1].box_id);
        let (a, b) = match (a, b) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };

        let ep_a = a.entry_points.iter().find(|e| e.pin_id == cores[0].pin_id);
        let ep_b = b.entry_points.iter().find(|e| e.pin_id == cores[1].pin_id);
        let (ep_a, ep_b) = match (ep_a, ep_b) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };

        let facing = is_facing(&ep_a.side, &ep_b.side);
        if !facing {
            continue;
        }

        let is_horizontal = matches!(
            (&ep_a.side, &ep_b.side),
            (EntrySide::Left, EntrySide::Right) | (EntrySide::Right, EntrySide::Left)
        );

        if is_horizontal {
            let (_, ay) = pin_abs(a, &ep_a.side, ep_a.offset);
            let (_, by) = pin_abs(b, &ep_b.side, ep_b.offset);
            let target_y = (ay + by) / 2.0;

            let new_offset_a = match ep_a.side {
                EntrySide::Left | EntrySide::Right => ((target_y - a.y) / a.h).clamp(0.05, 0.95),
                _ => ep_a.offset,
            };
            let new_offset_b = match ep_b.side {
                EntrySide::Left | EntrySide::Right => ((target_y - b.y) / b.h).clamp(0.05, 0.95),
                _ => ep_b.offset,
            };

            adjustments.push((a.id, cores[0].pin_id, new_offset_a));
            adjustments.push((b.id, cores[1].pin_id, new_offset_b));
        } else {
            let (ax, _) = pin_abs(a, &ep_a.side, ep_a.offset);
            let (bx, _) = pin_abs(b, &ep_b.side, ep_b.offset);
            let target_x = (ax + bx) / 2.0;

            let new_offset_a = match ep_a.side {
                EntrySide::Top | EntrySide::Bottom => ((target_x - a.x) / a.w).clamp(0.05, 0.95),
                _ => ep_a.offset,
            };
            let new_offset_b = match ep_b.side {
                EntrySide::Top | EntrySide::Bottom => ((target_x - b.x) / b.w).clamp(0.05, 0.95),
                _ => ep_b.offset,
            };

            adjustments.push((a.id, cores[0].pin_id, new_offset_a));
            adjustments.push((b.id, cores[1].pin_id, new_offset_b));
        }
    }

    let mut aligned = 0usize;
    for (box_id, pin_id, new_offset) in adjustments {
        if let Some(ep) = find_entry_mut(graph, box_id, pin_id) {
            ep.offset = new_offset;
            aligned += 1;
        }
    }
    // Each pair contributes 2 adjustments
    aligned /= 2;

    crate::vlog!(
        "[pin_place] straighten_facing_pairs: {} net pairs aligned",
        aligned
    );
}

/// Are two sides facing each other?
fn is_facing(a: &EntrySide, b: &EntrySide) -> bool {
    matches!(
        (a, b),
        (EntrySide::Left, EntrySide::Right)
            | (EntrySide::Right, EntrySide::Left)
            | (EntrySide::Top, EntrySide::Bottom)
            | (EntrySide::Bottom, EntrySide::Top)
    )
}

/// Find a mutable entry point by box_id + pin_id.
fn find_entry_mut(graph: &mut McVecGraph, box_id: i64, pin_id: i64) -> Option<&mut EntryPoint> {
    graph
        .boxes
        .iter_mut()
        .find(|b| b.id == box_id)
        .and_then(|b| b.entry_points.iter_mut().find(|e| e.pin_id == pin_id))
}

// ============================================================================
// C · Order within side (crossing-minimizing)
// ============================================================================

/// For each box, order pins on each side by the Y (or X) of their neighbour's
/// position, minimising in-bundle crossings.
fn order_within_side(graph: &mut McVecGraph) {
    let flag_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    let centers: HashMap<i64, (f64, f64)> = graph
        .boxes
        .iter()
        .map(|b| (b.id, (b.x + b.w / 2.0, b.y + b.h / 2.0)))
        .collect();

    // (box_id, pin_id) → neighbour position
    let mut target: HashMap<(i64, i64), (f64, f64)> = HashMap::new();
    for net in &graph.nets {
        let cores: Vec<&crate::vector::graph::EndpointRef> = net
            .endpoints
            .iter()
            .filter(|e| !flag_ids.contains(&e.box_id))
            .collect();
        if cores.len() < 2 {
            continue;
        }
        for e in &cores {
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut cnt = 0.0;
            for o in &cores {
                if o.box_id == e.box_id && o.pin_id == e.pin_id {
                    continue;
                }
                if let Some(&(ox, oy)) = centers.get(&o.box_id) {
                    sx += ox;
                    sy += oy;
                    cnt += 1.0;
                }
            }
            if cnt > 0.0 {
                target.insert((e.box_id, e.pin_id), (sx / cnt, sy / cnt));
            }
        }
    }

    for b in &mut graph.boxes {
        if flag_ids.contains(&b.id) {
            continue;
        }
        for side in [
            EntrySide::Top,
            EntrySide::Bottom,
            EntrySide::Left,
            EntrySide::Right,
        ] {
            let mut indices: Vec<(usize, f64)> = b
                .entry_points
                .iter()
                .enumerate()
                .filter(|(_, ep)| ep.side == side)
                .filter_map(|(i, ep)| {
                    target.get(&(b.id, ep.pin_id)).map(|&(tx, ty)| {
                        let key = match side {
                            EntrySide::Top | EntrySide::Bottom => tx,
                            EntrySide::Left | EntrySide::Right => ty,
                        };
                        (i, key)
                    })
                })
                .collect();

            if indices.len() < 2 {
                continue;
            }

            // Sort by target position
            indices.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            // Redistribute offsets evenly
            let n = indices.len();
            let spacing = 1.0 / (n as f64 + 1.0);
            for (rank, (idx, _)) in indices.iter().enumerate() {
                b.entry_points[*idx].offset = spacing * (rank as f64 + 1.0);
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::IoSummary;
    use crate::vector::graph::box_def::PinLayout;
    use crate::vector::graph::net_def::{EndpointRef, VizNet};
    use crate::vector::graph::{BoxKind, NetKind, Symbol};

    fn make_box(
        id: i64,
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        symbol: Symbol,
    ) -> crate::vector::graph::McVecBox {
        let kind = match symbol {
            Symbol::Ic => BoxKind::MultiPin,
            _ => BoxKind::TwoPin,
        };
        let pin_count = match symbol {
            Symbol::Ic => 8,
            _ => 2,
        };
        let mut b = crate::vector::graph::McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            kind,
            symbol,
            None,
            None,
            pin_count,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        // Add entry points
        b.entry_points = vec![
            EntryPoint {
                pin_id: 1,
                pin_name: "1".into(),
                side: EntrySide::Left,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 2,
                pin_name: "2".into(),
                side: EntrySide::Right,
                offset: 0.5,
            },
        ];
        b
    }

    fn make_ic_box(id: i64, name: &str, x: f64, y: f64) -> crate::vector::graph::McVecBox {
        let mut b = crate::vector::graph::McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::MultiPin,
            Symbol::Ic,
            None,
            None,
            8,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 100.0;
        b.h = 120.0;
        // Add entry points: pin 1 on left, pin 2 on right
        b.entry_points = vec![
            EntryPoint {
                pin_id: 1,
                pin_name: "SPK_MUTE".into(),
                side: EntrySide::Left,
                offset: 0.3,
            },
            EntryPoint {
                pin_id: 2,
                pin_name: "2".into(),
                side: EntrySide::Right,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 3,
                pin_name: "3".into(),
                side: EntrySide::Left,
                offset: 0.7,
            },
        ];
        b
    }

    #[test]
    fn pin_face_neighbor_flips_hub_pin() {
        // Hub IC has a pin on Left that connects to a neighbour on the Right.
        // With hub_keep_semantic=false, the pin should flip to Right.
        let mut graph = McVecGraph::new(1, "test".into());

        let hub = make_ic_box(1, "U1", 0.0, 0.0);
        let neighbor = make_box(2, "U2", 300.0, 0.0, 80.0, 60.0, Symbol::Ic);

        let net = VizNet::new(
            1,
            "SPK_MUTE".into(),
            NetKind::Signal,
            vec![
                EndpointRef::new(1, 1, "SPK_MUTE"),
                EndpointRef::new(2, 1, "IN"),
            ],
        );
        graph.boxes.push(hub);
        graph.boxes.push(neighbor);
        graph.nets.push(net);

        pin_place_pipeline(&mut graph, Some(1), true, false);

        let hub = graph.boxes.iter().find(|b| b.id == 1).unwrap();
        let ep = hub.entry_points.iter().find(|e| e.pin_id == 1).unwrap();
        assert_eq!(
            ep.side,
            EntrySide::Right,
            "Hub pin connected to right neighbour should flip to Right"
        );
    }

    #[test]
    fn pin_straighten_facing_pair() {
        // Two boxes facing each other with misaligned offsets → after straighten,
        // offsets should be aligned.
        let mut graph = McVecGraph::new(1, "test".into());

        let mut a = make_ic_box(1, "U1", 0.0, 0.0);
        a.entry_points[0].side = EntrySide::Right;
        a.entry_points[0].offset = 0.2; // near top

        let mut b = make_ic_box(2, "U2", 300.0, 0.0);
        b.entry_points[0].side = EntrySide::Left;
        b.entry_points[0].offset = 0.8; // near bottom

        let net = VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1"), EndpointRef::new(2, 1, "1")],
        );
        graph.boxes.push(a);
        graph.boxes.push(b);
        graph.nets.push(net);

        straighten_facing_pairs(&mut graph);

        let a = graph.boxes.iter().find(|b| b.id == 1).unwrap();
        let b = graph.boxes.iter().find(|b| b.id == 2).unwrap();
        let ep_a = a.entry_points.iter().find(|e| e.pin_id == 1).unwrap();
        let ep_b = b.entry_points.iter().find(|e| e.pin_id == 1).unwrap();

        // Both should now have similar offsets (midpoint between 0.2 and 0.8)
        let midpoint = (0.2 + 0.8) / 2.0;
        assert!(
            (ep_a.offset - midpoint).abs() < 0.2,
            "Pin A offset should be near midpoint: got {}, expected ~{}",
            ep_a.offset,
            midpoint
        );
        assert!(
            (ep_b.offset - midpoint).abs() < 0.2,
            "Pin B offset should be near midpoint: got {}, expected ~{}",
            ep_b.offset,
            midpoint
        );
    }

    #[test]
    fn pin_order_minimizes_crossings() {
        // One box with 3 pins on Left, connecting to 3 boxes at different Y.
        // After ordering, pins should be sorted by target Y.
        let mut graph = McVecGraph::new(1, "test".into());

        let mut ic = make_ic_box(1, "U1", 100.0, 100.0);
        ic.entry_points = vec![
            EntryPoint {
                pin_id: 1,
                pin_name: "A".into(),
                side: EntrySide::Left,
                offset: 0.9,
            },
            EntryPoint {
                pin_id: 2,
                pin_name: "B".into(),
                side: EntrySide::Left,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 3,
                pin_name: "C".into(),
                side: EntrySide::Left,
                offset: 0.1,
            },
        ];

        let t = make_box(2, "T", 0.0, 0.0, 40.0, 30.0, Symbol::Resistor);
        let m = make_box(3, "M", 0.0, 100.0, 40.0, 30.0, Symbol::Resistor);
        let b = make_box(4, "B", 0.0, 200.0, 40.0, 30.0, Symbol::Resistor);

        let net_top = VizNet::new(
            1,
            "TOP".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "A"), EndpointRef::new(2, 1, "1")],
        );
        let net_mid = VizNet::new(
            2,
            "MID".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 2, "B"), EndpointRef::new(3, 1, "1")],
        );
        let net_bot = VizNet::new(
            3,
            "BOT".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 3, "C"), EndpointRef::new(4, 1, "1")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(t);
        graph.boxes.push(m);
        graph.boxes.push(b);
        graph.nets.push(net_top);
        graph.nets.push(net_mid);
        graph.nets.push(net_bot);

        order_within_side(&mut graph);

        let ic = graph.boxes.iter().find(|b| b.id == 1).unwrap();
        let offsets: Vec<f64> = ic
            .entry_points
            .iter()
            .filter(|e| e.side == EntrySide::Left)
            .map(|e| e.offset)
            .collect();

        // Offsets should be monotonically increasing (sorted by target Y)
        for w in offsets.windows(2) {
            assert!(w[0] < w[1], "Offsets should be sorted: {:?}", offsets);
        }
    }

    #[test]
    fn pin_authored_side_frozen() {
        // A pin with layout_hint should not be moved.
        let mut graph = McVecGraph::new(1, "test".into());

        let mut a = make_ic_box(1, "U1", 0.0, 0.0);
        a.layout_hint = Some(PinLayout {
            left: vec!["SPK_MUTE".into()],
            right: vec![],
            top: vec![],
            bottom: vec![],
        });
        a.entry_points[0].pin_name = "SPK_MUTE".into();
        a.entry_points[0].side = EntrySide::Left;

        let neighbor = make_box(2, "U2", 300.0, 0.0, 80.0, 60.0, Symbol::Ic);

        let net = VizNet::new(
            1,
            "SPK_MUTE".into(),
            NetKind::Signal,
            vec![
                EndpointRef::new(1, 1, "SPK_MUTE"),
                EndpointRef::new(2, 1, "IN"),
            ],
        );
        graph.boxes.push(a);
        graph.boxes.push(neighbor);
        graph.nets.push(net);

        pin_place_pipeline(&mut graph, Some(1), true, false);

        let a = graph.boxes.iter().find(|b| b.id == 1).unwrap();
        let ep = a
            .entry_points
            .iter()
            .find(|e| e.pin_name == "SPK_MUTE")
            .unwrap();
        assert_eq!(
            ep.side,
            EntrySide::Left,
            "Authored pin should stay on Left even though neighbour is on Right"
        );
    }

    #[test]
    fn pin_lr_only_default() {
        // With lr_only=true, no pin should end up on Top/Bottom.
        let mut graph = McVecGraph::new(1, "test".into());

        let mut a = make_ic_box(1, "U1", 0.0, 0.0);
        a.entry_points.push(EntryPoint {
            pin_id: 4,
            pin_name: "4".into(),
            side: EntrySide::Top,
            offset: 0.5,
        });

        let neighbor = make_box(2, "U2", 0.0, 300.0, 80.0, 60.0, Symbol::Ic);

        let net = VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 4, "4"), EndpointRef::new(2, 1, "1")],
        );
        graph.boxes.push(a);
        graph.boxes.push(neighbor);
        graph.nets.push(net);

        pin_place_pipeline(&mut graph, None, true, false);

        for b in &graph.boxes {
            for ep in &b.entry_points {
                assert!(
                    !matches!(ep.side, EntrySide::Top | EntrySide::Bottom),
                    "Pin {} should not be on Top/Bottom with lr_only=true",
                    ep.pin_id
                );
            }
        }
    }

    #[test]
    fn pin_place_deterministic() {
        let make_graph = || {
            let mut g = McVecGraph::new(1, "test".into());
            let hub = make_ic_box(1, "U1", 0.0, 0.0);
            let nbr = make_box(2, "U2", 300.0, 0.0, 80.0, 60.0, Symbol::Ic);
            let net = VizNet::new(
                1,
                "SIG".into(),
                NetKind::Signal,
                vec![
                    EndpointRef::new(1, 1, "SPK_MUTE"),
                    EndpointRef::new(2, 1, "IN"),
                ],
            );
            g.boxes.push(hub);
            g.boxes.push(nbr);
            g.nets.push(net);
            g
        };

        let mut g1 = make_graph();
        let mut g2 = make_graph();

        pin_place_pipeline(&mut g1, Some(1), true, false);
        pin_place_pipeline(&mut g2, Some(1), true, false);

        for (b1, b2) in g1.boxes.iter().zip(g2.boxes.iter()) {
            assert_eq!(b1.entry_points.len(), b2.entry_points.len());
            for (ep1, ep2) in b1.entry_points.iter().zip(b2.entry_points.iter()) {
                assert_eq!(ep1.side, ep2.side, "Side mismatch for pin {}", ep1.pin_id);
                assert!(
                    (ep1.offset - ep2.offset).abs() < 0.001,
                    "Offset mismatch for pin {}",
                    ep1.pin_id
                );
            }
        }
    }

    #[test]
    fn select_picks_hub_flip_when_better() {
        // Verify that pin_place_pipeline with hub_keep_semantic=false produces
        // a different result from hub_keep_semantic=true, and the pipeline runs
        // successfully in both modes.
        let make_graph = || {
            let mut g = McVecGraph::new(1, "test".into());
            let hub = make_ic_box(1, "U1", 0.0, 0.0);
            let nbr = make_box(2, "U2", 300.0, 0.0, 80.0, 60.0, Symbol::Ic);
            let net = VizNet::new(
                1,
                "SIG".into(),
                NetKind::Signal,
                vec![
                    EndpointRef::new(1, 1, "SPK_MUTE"),
                    EndpointRef::new(2, 1, "IN"),
                ],
            );
            g.boxes.push(hub);
            g.boxes.push(nbr);
            g.nets.push(net);
            g
        };

        let mut g_semantic = make_graph();
        let mut g_connectivity = make_graph();

        pin_place_pipeline(&mut g_semantic, Some(1), true, true);
        pin_place_pipeline(&mut g_connectivity, Some(1), true, false);

        // With hub_keep_semantic=true, the original Left side should stay
        let ep_sem = &g_semantic.boxes[0].entry_points[0];
        assert_eq!(ep_sem.side, EntrySide::Left);

        // With hub_keep_semantic=false, it should flip to Right
        let ep_conn = &g_connectivity.boxes[0].entry_points[0];
        assert_eq!(ep_conn.side, EntrySide::Right);
    }

    #[test]
    fn pin_face_neighbor_no_exempt_for_non_hub() {
        // Non-hub leaf pin connected to neighbour → should flip regardless of hub_keep_semantic.
        let mut graph = McVecGraph::new(1, "test".into());

        let leaf = make_box(1, "R1", 0.0, 0.0, 40.0, 30.0, Symbol::Resistor);
        let neighbor = make_box(2, "U1", 300.0, 0.0, 80.0, 60.0, Symbol::Ic);

        let net = VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1"), EndpointRef::new(2, 1, "1")],
        );
        graph.boxes.push(leaf);
        graph.boxes.push(neighbor);
        graph.nets.push(net);

        pin_place_pipeline(&mut graph, Some(2), true, true);

        let leaf = graph.boxes.iter().find(|b| b.id == 1).unwrap();
        let ep = leaf.entry_points.iter().find(|e| e.pin_id == 1).unwrap();
        assert_eq!(
            ep.side,
            EntrySide::Right,
            "Leaf pin connected to right neighbour should flip to Right"
        );
    }
}
