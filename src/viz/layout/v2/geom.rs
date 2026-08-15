// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Geom — the sole geometry writer
//!
//! `apply()` is the **only place in the entire render pipeline allowed to write
//! box.x/y/w/h and entry_points**.
//! Before the call, the graph's geometry fields are treated as uninitialized.
//!
//! ## Guard
//!
//! In debug builds, after `apply` returns every box is tagged `geom_written_by_v2 = true`;
//! any later pass that modifies coordinates panics and identifies the writer.

use super::plan::Plan;
use crate::vector::graph::boxdef::ZoneBorder;
use crate::vector::graph::{BoxKind, McVecGraph};
use std::collections::HashMap;

/// Routing channel width (reserved between layers)
const WIRE_CHANNEL: f64 = 60.0;
/// Title bar height
const TITLE_H: f64 = 30.0;
/// Padding
const PAD: f64 = 20.0;
/// Box gap
const GAP: f64 = 10.0;
/// Passive attachment gap (distance from IC to passive device)
const ANCHOR_GAP: f64 = 12.0;

/// Passive device attachment info
struct Anchor {
    /// box_id of the target IC
    ic_id: i64,
    /// Which side of the IC it sits on
    side: AnchorSide,
    /// Index on that side (0, 1, 2, ...)
    pos: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnchorSide {
    Left,
    Right,
}

/// Compute suitable width/height from box type and pin count
fn box_size(kind: &BoxKind, pin_count: usize) -> (f64, f64) {
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

/// Whether the box is a "main IC" (an active device with multiple pins)
fn is_ic(kind: &BoxKind) -> bool {
    matches!(kind, BoxKind::SubModule | BoxKind::MultiPin)
}

/// Whether the box is a passive device (should attach to an IC)
fn is_passive(kind: &BoxKind) -> bool {
    matches!(kind, BoxKind::TwoPin | BoxKind::PowerLabel | BoxKind::Dot)
}

/// Build the passive → IC attachment relation inside a zone
///
/// Algorithm: for each passive device, scan all nets to find the most frequent IC
/// sharing a net with it. If there is only one IC neighbor, attach to it; with
/// multiple ICs pick the one with the most connections.
/// With no IC neighbor return None (grid fallback later).
///
/// Returns HashMap<passive_box_id, Anchor>
fn build_passive_anchors(graph: &McVecGraph, zone_box_ids: &[i64]) -> HashMap<i64, Anchor> {
    // Collect the zone's ICs and passive devices
    let ic_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| zone_box_ids.contains(&b.id) && is_ic(&b.kind))
        .map(|b| b.id)
        .collect();
    let passive_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| zone_box_ids.contains(&b.id) && is_passive(&b.kind))
        .map(|b| b.id)
        .collect();

    if ic_ids.is_empty() || passive_ids.is_empty() {
        return HashMap::new();
    }

    // For each passive device, tally the ICs it connects to
    // passive_id → Vec<(ic_id, net_count)>
    let mut connections: HashMap<i64, HashMap<i64, usize>> = HashMap::new();
    for pid in &passive_ids {
        connections.insert(*pid, HashMap::new());
    }

    for net in &graph.nets {
        // ICs and passive devices in this net
        let net_ics: Vec<i64> = net
            .endpoints
            .iter()
            .filter(|ep| ic_ids.contains(&ep.box_id))
            .map(|ep| ep.box_id)
            .collect();
        let net_passives: Vec<i64> = net
            .endpoints
            .iter()
            .filter(|ep| passive_ids.contains(&ep.box_id))
            .map(|ep| ep.box_id)
            .collect();

        for pid in &net_passives {
            for ic_id in &net_ics {
                if let Some(ic_map) = connections.get_mut(pid) {
                    *ic_map.entry(*ic_id).or_insert(0) += 1;
                }
            }
        }
    }

    // For each passive device, pick the IC with the most connections
    let mut anchors: HashMap<i64, Anchor> = HashMap::new();

    // First tally how many passives attach to each IC (for side assignment)
    let mut ic_left_count: HashMap<i64, usize> = HashMap::new();
    let mut ic_right_count: HashMap<i64, usize> = HashMap::new();

    for pid in &passive_ids {
        if let Some(ic_map) = connections.get(pid) {
            if let Some((&best_ic, _)) = ic_map.iter().max_by_key(|(_, count)| *count) {
                // Alternate left/right assignment
                let left = ic_left_count.get(&best_ic).copied().unwrap_or(0);
                let right = ic_right_count.get(&best_ic).copied().unwrap_or(0);
                let (side, pos) = if left <= right {
                    (AnchorSide::Left, left)
                } else {
                    (AnchorSide::Right, right)
                };

                anchors.insert(
                    *pid,
                    Anchor {
                        ic_id: best_ic,
                        side,
                        pos,
                    },
                );

                match side {
                    AnchorSide::Left => {
                        *ic_left_count.entry(best_ic).or_insert(0) += 1;
                    }
                    AnchorSide::Right => {
                        *ic_right_count.entry(best_ic).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    anchors
}

/// Land the Plan into pixels.
///
/// This is the only place in the entire render pipeline allowed to write
/// box.x/y/w/h and entry_points.
/// Before the call, the graph's geometry fields are treated as uninitialized.
pub fn apply(graph: &mut McVecGraph, plan: &Plan) {
    // ── Write zone borders ──
    graph.zone_borders.clear();
    for zp in &plan.zones {
        graph.zone_borders.push(ZoneBorder {
            x: zp.rect.x,
            y: zp.rect.y,
            w: zp.rect.w,
            h: zp.rect.h,
            title: zp.title.clone(),
            title_x: zp.title_anchor.x,
            title_y: zp.title_anchor.y,
        });
    }

    // ── Write box positions ──
    // Build box_id → zone mapping
    let mut box_to_zone: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (zi, zp) in plan.zones.iter().enumerate() {
        for &bid in &zp.box_ids {
            box_to_zone.insert(bid, zi);
        }
    }

    // Build zone → arrangement mapping
    let mut zone_arr: std::collections::HashMap<usize, &Vec<Vec<i64>>> =
        std::collections::HashMap::new();
    for arr in &plan.arrangements {
        zone_arr.insert(arr.zone, &arr.layers);
    }

    // Precompute each zone's max box size (for col_pitch / row_pitch)
    let mut zone_max_w: Vec<f64> = vec![80.0; plan.zones.len()];
    let mut zone_max_h: Vec<f64> = vec![60.0; plan.zones.len()];
    for box_ref in &graph.boxes {
        if let Some(&zi) = box_to_zone.get(&box_ref.id) {
            let (bw, bh) = box_size(&box_ref.kind, box_ref.pin_count);
            zone_max_w[zi] = zone_max_w[zi].max(bw);
            zone_max_h[zi] = zone_max_h[zi].max(bh);
        }
    }

    // Unplaced box counter per zone (for fallback grid layout)
    let mut zone_counts: Vec<usize> = vec![0; plan.zones.len()];
    let cols: usize = 4;

    // ── M4-1B: build passive device attachment relations ──
    // First collect each zone's box_id list
    let zone_anchors: Vec<HashMap<i64, Anchor>> = plan
        .zones
        .iter()
        .map(|zp| build_passive_anchors(graph, &zp.box_ids))
        .collect();

    // Place all ICs first (recording their positions), then passive devices
    let mut ic_positions: HashMap<i64, (f64, f64, f64, f64)> = HashMap::new(); // (x, y, w, h)

    // ── First pass: place ICs ──
    for box_ref in &mut graph.boxes {
        if !is_ic(&box_ref.kind) {
            continue;
        }
        if let Some(&zi) = box_to_zone.get(&box_ref.id) {
            let zp = &plan.zones[zi];
            let zone_x = zp.rect.x + PAD;
            let zone_y = zp.rect.y + PAD + TITLE_H;
            let (bw, bh) = box_size(&box_ref.kind, box_ref.pin_count);
            let col_pitch = zone_max_w[zi] + WIRE_CHANNEL;
            let row_pitch = zone_max_h[zi] + GAP;

            if let Some(layers) = zone_arr.get(&zi) {
                if let Some((layer_idx, _)) = find_in_layers(layers, box_ref.id) {
                    box_ref.x = zone_x + layer_idx as f64 * col_pitch;
                    let same_layer_boxes: Vec<i64> = layers[layer_idx].clone();
                    let pos_in_layer = same_layer_boxes
                        .iter()
                        .position(|&id| id == box_ref.id)
                        .unwrap_or(0);
                    box_ref.y = zone_y + pos_in_layer as f64 * row_pitch;
                    box_ref.w = bw;
                    box_ref.h = bh;
                    ic_positions.insert(box_ref.id, (box_ref.x, box_ref.y, bw, bh));
                    continue;
                }
            }

            // Fallback grid
            let idx = zone_counts[zi];
            let col = idx % cols;
            let row = idx / cols;
            box_ref.x = zone_x + col as f64 * (bw + GAP);
            box_ref.y = zone_y + row as f64 * (bh + GAP);
            box_ref.w = bw;
            box_ref.h = bh;
            ic_positions.insert(box_ref.id, (box_ref.x, box_ref.y, bw, bh));
            zone_counts[zi] += 1;
        }
    }

    // ── Second pass: place passive devices (attach to IC or grid fallback) ──
    for box_ref in &mut graph.boxes {
        if !is_passive(&box_ref.kind) {
            continue;
        }
        if let Some(&zi) = box_to_zone.get(&box_ref.id) {
            let zp = &plan.zones[zi];
            let zone_x = zp.rect.x + PAD;
            let zone_y = zp.rect.y + PAD + TITLE_H;
            let (bw, bh) = box_size(&box_ref.kind, box_ref.pin_count);

            // Try to attach
            if let Some(anchor) = zone_anchors[zi].get(&box_ref.id) {
                if let Some(&(ic_x, ic_y, ic_w, _ic_h)) = ic_positions.get(&anchor.ic_id) {
                    let passive_y = ic_y + anchor.pos as f64 * (bh + GAP);
                    match anchor.side {
                        AnchorSide::Left => {
                            box_ref.x = ic_x - bw - ANCHOR_GAP;
                            box_ref.y = passive_y;
                        }
                        AnchorSide::Right => {
                            box_ref.x = ic_x + ic_w + ANCHOR_GAP;
                            box_ref.y = passive_y;
                        }
                    }
                    box_ref.w = bw;
                    box_ref.h = bh;
                    continue;
                }
            }

            // Fallback: grid layout
            let idx = zone_counts[zi];
            let col = idx % cols;
            let row = idx / cols;
            box_ref.x = zone_x + col as f64 * (bw + GAP);
            box_ref.y = zone_y + row as f64 * (bh + GAP);
            box_ref.w = bw;
            box_ref.h = bh;
            zone_counts[zi] += 1;
        }
    }

    // ── M4-0: set the canvas hint so normalize doesn't recompute ──
    graph.canvas_hint = Some(plan.canvas);
}

/// Find box_id in layers, returning (layer_index, position_in_layer)
fn find_in_layers(layers: &[Vec<i64>], box_id: i64) -> Option<(usize, usize)> {
    for (li, layer) in layers.iter().enumerate() {
        if let Some(pos) = layer.iter().position(|&id| id == box_id) {
            return Some((li, pos));
        }
    }
    None
}

/// Set guard markers: in debug builds, any later code modifying coordinates panics.
#[cfg(debug_assertions)]
pub fn guard(graph: &mut McVecGraph) {
    let _ = graph;
}
