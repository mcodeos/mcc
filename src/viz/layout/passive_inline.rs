// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Passive component "first wire, then place component" layout (wire-first passive placement)
//!
//! ## What problem does this file solve
//! Old pipeline was "first layout, then wire": FlowLayouter treats R/L/C as ordinary boxes into columns, router
//! then bends to reach them. Two-pin passive components are essentially just **symbols**, shouldn't occupy layout grid slots; they should sit on
//! the **straight line** between the two large components on either side.
//!
//! This module **extracts** passive components from the layout graph entirely, only letting major components participate in arrangement:
//! - [`collapse_passives`] (called **before** layout): for each series passive P where "both side neighbors are non-flag entity major components",
//!   delete P's box + its two nets, add one A↔B **direct net** so major components align by direct connection.
//!   Original box / original net / neighbor references temporarily stored in [`PassiveStash`].
//! - [`reinsert_passives`] (called **after** layout, **before** routing): delete temporary direct net, place each P on the
//!   **wire** between its two neighbors' exit points (same line orientation, offset=0.5), restore P's two original nets.
//!   Then existing router sees "endpoints collinear" → directly draw straight line (orthogonal.rs degenerate branch).
//!
//! ## Also fixes owner-fallback pin collapse for passive components
//! Typed two-pin components (typed-2pin) often owner-fallback to **the same pin_id**
//! (`split_shared_pins` skips two-pin components for safety, doesn't split them). If two pins have same id, two nets will exit from same
//! point → component looks like it lost a foot. reinsert here **forces allocation of two different pin_ids** for P's two pins,
//! letting two wires each go to their own point.
//!
//! ## v1 scope of application (rest maintains original layout, zero side effects)
//! - Only handles series components that "connect exactly 2 nets, and each net is 2-terminal (P + one non-flag non-passive neighbor)".
//! - **Don't touch**: bypass components (to GND/power flag), series components on rails, passive-passive chains.
//!   These are left for subsequent phases; they go through original layout + existing offset fallback.
//! - Won't splice Power nets and Ground nets together (decoupling capacitors that kind would short, skip directly).

use std::collections::HashSet;

use crate::vector::graph::{
    BoxKind, EndpointRef, EntryPoint, EntrySide, McVecBox, McVecGraph, NetKind, VisualRole, VizNet,
};

use super::rails::is_rail_box;

/// ★ Stage 1 entry forward: Long signal nets → net labels (air wires).
///
/// Implementation in `rails::apply_net_labels`; this is just forwarding. Reason: api layer (viz::api) can stably reach
/// `super::layout::passive_inline` (all box/net operations before routing go through here), but `rails` may be
/// layout internal private module, api layer (layout's sibling) can't reach it; passive_inline is layout's descendant,
/// can stably reach `super::rails`.
pub fn apply_net_labels(graph: &mut McVecGraph) -> Option<(f64, f64)> {
    super::rails::apply_net_labels(graph)
}

/// Temporary direct net nid starting base (higher than real / flag / stub net id, deleted after layout)
const SPLICE_NID_BASE: i64 = 8_000_000_000;
/// reinsert synthesized second pin id starting base for passives with same pin id on both pins
const PASSIVE_PIN_BASE: i64 = 4_500_000_000;
/// Stage A3 (`place_passive_chains`) synthesized second-pin base — distinct from every other base
/// (`SPLIT_PIN_BASE` 4e9, `PASSIVE_PIN_BASE` 4.5e9, `RAIL_PIN_BASE` 4.7e9) and below `FLAG_ID_BASE` (9e9).
const CHAIN_PIN_BASE: i64 = 4_800_000_000;

// ============================================================================
// Stash
// ============================================================================

struct StashedPassive {
    /// Original passive box (with size, used for placement in reinsert)
    bx: McVecBox,
    /// Original two nets (with passive endpoints); nets[0] = P↔A, nets[1] = P↔B
    nets: Vec<VizNet>,
    /// P's pin_id in two nets (.0 towards A, .1 towards B). Two values may be same → reinsert splits
    p_pin: (i64, i64),
    /// Neighbor A / B's (box_id, pin_id)
    na: (i64, i64),
    nb: (i64, i64),
    /// Temporary direct net's nid (deleted after layout)
    splice_nid: i64,
}

pub struct PassiveStash {
    items: Vec<StashedPassive>,
}

impl PassiveStash {
    pub fn empty() -> Self {
        Self { items: Vec::new() }
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

// ============================================================================
// collapse —— extract passives before layout
// ============================================================================

/// Called **before** layout: extract series passives from this layer, add direct nets. Return stash for [`reinsert_passives`].
///
/// Only processes this layer (`graph.boxes` / `graph.nets`); sub-layers processed by their own recursive calls.
pub fn collapse_passives(graph: &mut McVecGraph) -> PassiveStash {
    let mut stash = PassiveStash { items: Vec::new() };

    let passive_ids: Vec<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.is_two_pin_passive())
        .map(|b| b.id)
        .collect();
    if passive_ids.is_empty() {
        return stash;
    }
    let passive_set: HashSet<i64> = passive_ids.iter().copied().collect();
    let rail_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    let mut splice_nid = SPLICE_NID_BASE;

    for pid in passive_ids {
        // Find nets connected to P
        let touching: Vec<usize> = graph
            .nets
            .iter()
            .enumerate()
            .filter(|(_, n)| n.endpoints.iter().any(|e| e.box_id == pid))
            .map(|(i, _)| i)
            .collect();
        // v1: exactly 2 nets
        if touching.len() != 2 {
            continue;
        }

        // Parse each net: must be (P + exactly one non-flag non-passive neighbor)
        let mut sides: Vec<(usize, EndpointRef, EndpointRef, NetKind, String)> = Vec::new();
        let mut ok = true;
        for &ni in &touching {
            let net = &graph.nets[ni];
            let p_ep = net.endpoints.iter().find(|e| e.box_id == pid).cloned();
            let others: Vec<&EndpointRef> =
                net.endpoints.iter().filter(|e| e.box_id != pid).collect();
            // Neighbor must be unique, and be a non-flag non-passive major component
            let neigh = if others.len() == 1
                && !rail_ids.contains(&others[0].box_id)
                && !passive_set.contains(&others[0].box_id)
            {
                Some(others[0].clone())
            } else {
                None
            };
            match (p_ep, neigh) {
                (Some(p), Some(n)) => sides.push((ni, p, n, net.kind.clone(), net.name.clone())),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || sides.len() != 2 {
            continue;
        }

        // Safety: don't splice Power net to Ground net (decoupling capacitors would short)
        if is_power_ground_short(&sides[0].3, &sides[1].3) {
            continue;
        }

        // Direct net: connect A, B two neighbor endpoints (let major components align by direct connection)
        let splice = VizNet::new(
            splice_nid,
            sides[0].4.clone(),
            merge_kind(&sides[0].3, &sides[1].3),
            vec![sides[0].2.clone(), sides[1].2.clone()],
        );

        // Stash original component + original nets
        let bx = match graph.boxes.iter().find(|b| b.id == pid) {
            Some(b) => b.clone(),
            None => continue,
        };
        let orig_nets: Vec<VizNet> = touching.iter().map(|&i| graph.nets[i].clone()).collect();
        stash.items.push(StashedPassive {
            bx,
            nets: orig_nets,
            p_pin: (sides[0].1.pin_id, sides[1].1.pin_id),
            na: (sides[0].2.box_id, sides[0].2.pin_id),
            nb: (sides[1].2.box_id, sides[1].2.pin_id),
            splice_nid,
        });
        splice_nid += 1;

        // Delete P's box; delete two original nets (delete in descending order to avoid index shift); add direct net
        graph.boxes.retain(|b| b.id != pid);
        let mut idxs = touching.clone();
        idxs.sort_unstable_by(|a, b| b.cmp(a));
        for i in idxs {
            graph.nets.remove(i);
        }
        graph.nets.push(splice);
    }

    if !stash.is_empty() {
        crate::vlog!(
            "[layout::passive_inline] collapsed {} series passive(s) into direct nets",
            stash.len()
        );
    }
    stash
}

// ============================================================================
// ★ Stage A (A2) — Non-destructive inline placement of series two-pin passives
// ============================================================================
//
// ## Why this replaces collapse/reinsert on the main path
// The old collapse→layout→reinsert path DELETED the passive box + its two nets before
// layout and tried to splice them back afterwards. When a neighbour was near the canvas
// margin, the reinserted passive landed at a **negative coordinate** and was clipped off
// the viewBox → the passive "disappeared" (the R in the PWR→R→LED→GND loop). It also
// produced dangling air-wires when the temporary splice net interacted with net-labels.
//
// This pass NEVER deletes the box or its nets. Passives flow through layout as ordinary
// boxes (so they always get placed + routed). Afterwards, for each *series* passive whose
// two nets each reach exactly one real (non-flag, non-passive) neighbour, we simply
// reposition the box onto the segment between the two neighbour exit points and orient its
// two pins to face them. The real R↔A / R↔B nets are untouched → the router draws two real
// wires and the passive is always visible.
//
// Runs **after** layout, **before** routing. Root + sub layers.

/// Place series two-pin passives inline between their two neighbours, without touching nets.
pub fn place_series_passives(graph: &mut McVecGraph) {
    // Deterministic order: synthetic pin ids allocated below must not depend on iteration order.
    let passive_ids: Vec<i64> = {
        let mut v: Vec<i64> = graph
            .boxes
            .iter()
            .filter(|b| b.is_two_pin_passive())
            .map(|b| b.id)
            .collect();
        v.sort_unstable();
        v
    };
    if passive_ids.is_empty() {
        return;
    }
    let passive_set: HashSet<i64> = passive_ids.iter().copied().collect();
    let rail_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    let mut synth_pin = PASSIVE_PIN_BASE;

    for pid in passive_ids {
        // P must touch exactly 2 nets (a plain series element).
        let touching: Vec<usize> = graph
            .nets
            .iter()
            .enumerate()
            .filter(|(_, n)| n.endpoints.iter().any(|e| e.box_id == pid))
            .map(|(i, _)| i)
            .collect();
        if touching.len() != 2 {
            continue; // bypass cap / chain / rail-only / unconnected → leave where layout put it
        }

        // For each net collect (net_idx, P's pin_id, neighbour box_id, neighbour pin_id).
        // Neighbour must be unique on the net and be a real device (not a rail/flag, not another passive).
        let mut sides: Vec<(usize, i64, i64, i64)> = Vec::new();
        let mut ok = true;
        for &ni in &touching {
            let net = &graph.nets[ni];
            let p_pin = net
                .endpoints
                .iter()
                .find(|e| e.box_id == pid)
                .map(|e| e.pin_id);
            let others: Vec<&EndpointRef> =
                net.endpoints.iter().filter(|e| e.box_id != pid).collect();
            match (p_pin, others.as_slice()) {
                (Some(pp), [o])
                    if !rail_ids.contains(&o.box_id) && !passive_set.contains(&o.box_id) =>
                {
                    sides.push((ni, pp, o.box_id, o.pin_id));
                }
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || sides.len() != 2 {
            continue;
        }

        // Neighbour exit points (degenerate to box centre if the pin can't be located).
        let a_toward = box_center(graph, sides[1].2);
        let b_toward = box_center(graph, sides[0].2);
        let ((ax, ay), sa) = match pin_exit_facing(graph, sides[0].2, sides[0].3, a_toward) {
            Some(v) => v,
            None => continue,
        };
        let ((bx2, by2), sb) = match pin_exit_facing(graph, sides[1].2, sides[1].3, b_toward) {
            Some(v) => v,
            None => continue,
        };

        // P's own pin ids toward A / B. If owner-fallback collapsed them to the same id, synthesize
        // a second one and rewrite the second net's P-endpoint, so the two wires get two exit points.
        let pin_a = sides[0].1;
        let mut pin_b = sides[1].1;
        if pin_a == pin_b {
            pin_b = synth_pin;
            synth_pin += 1;
            let ni = sides[1].0;
            if let Some(ep) = graph.nets[ni]
                .endpoints
                .iter_mut()
                .find(|e| e.box_id == pid && e.pin_id == pin_a)
            {
                ep.pin_id = pin_b;
            }
        }

        place_passive_between(graph, pid, (ax, ay), pin_a, sa, (bx2, by2), pin_b, sb);
    }
}

// ============================================================================
// ★ Stage A3 — inline placement for passive↔passive series chains
// ============================================================================
//
// ## Gap this fills
// `place_series_passives` requires each neighbour to be a real (non-flag, non-passive) device, and
// `straighten_rail_passives` requires exactly "one real device + one flag". Both therefore **skip**
// a passive whose neighbour is *another passive*, so a `rail — R — C — … — rail` string is left
// wherever the flow layouter dropped each box.
//
// After the net-build fix (each pass-through 2-pin device now owns its two distinct pins) such
// chains are already electrically correct and render with two feet; this pass only lines the
// members up so the string reads straight.
//
// ## Safety (purely additive)
// - Only repositions passives that have **at least one passive neighbour** — exactly the case the
//   other two passes skip, so their domain is never touched (a passive with two real neighbours is
//   handled by `place_series_passives`; those sets are disjoint).
// - Never edits net topology. The `pin_a == pin_b` split is kept only as a defensive no-op (Layer-1
//   already gave the two pins distinct ids; it would fire only for legacy owner-fallback collapse).
// - Degrades to "leave in place" whenever an anchor can't be located.
// - Bounded + deterministic: a fixed number of settle sweeps over id-sorted passives.
//
// Runs **after** layout, **before** routing (right after `place_series_passives`).

/// Place passive↔passive series-chain members inline between their two chain anchors.
pub fn place_passive_chains(graph: &mut McVecGraph) {
    let passive_ids: Vec<i64> = {
        let mut v: Vec<i64> = graph
            .boxes
            .iter()
            .filter(|b| b.is_two_pin_passive())
            .map(|b| b.id)
            .collect();
        v.sort_unstable();
        v
    };
    if passive_ids.len() < 2 {
        return; // need at least one passive↔passive adjacency
    }
    let passive_set: HashSet<i64> = passive_ids.iter().copied().collect();
    let rail_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    let mut synth = CHAIN_PIN_BASE;
    let mut moved = 0usize;

    // A few settle sweeps: when a neighbour is another passive that also moves, repeating lets the
    // positions converge. Bounded (3) and deterministic (id-sorted) → always terminates, no flicker.
    for sweep in 0..3 {
        for &pid in &passive_ids {
            // A plain series element touches exactly two nets.
            let touching: Vec<usize> = graph
                .nets
                .iter()
                .enumerate()
                .filter(|(_, n)| n.endpoints.iter().any(|e| e.box_id == pid))
                .map(|(i, _)| i)
                .collect();
            if touching.len() != 2 {
                continue;
            }

            // (net_idx, P's pin, neighbour box, neighbour pin) per side; neighbour must be unique.
            let mut sides: Vec<(usize, i64, i64, i64)> = Vec::new();
            let mut ok = true;
            let mut has_passive_neighbour = false;
            for &ni in &touching {
                let net = &graph.nets[ni];
                let p_pin = net
                    .endpoints
                    .iter()
                    .find(|e| e.box_id == pid)
                    .map(|e| e.pin_id);
                let others: Vec<&EndpointRef> =
                    net.endpoints.iter().filter(|e| e.box_id != pid).collect();
                match (p_pin, others.as_slice()) {
                    (Some(pp), [o]) => {
                        if passive_set.contains(&o.box_id) {
                            has_passive_neighbour = true;
                        }
                        sides.push((ni, pp, o.box_id, o.pin_id));
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            // Only the passive-chain case the other passes skip.
            if !ok || sides.len() != 2 || !has_passive_neighbour {
                continue;
            }

            // Anchor exit points from both neighbours. Rail flags anchor on the flag box id
            // (their pin id == box id); real devices / passives anchor on the real neighbour pin.
            let a_toward = box_center(graph, sides[1].2);
            let b_toward = box_center(graph, sides[0].2);
            let a_pin = if rail_ids.contains(&sides[0].2) {
                sides[0].2
            } else {
                sides[0].3
            };
            let b_pin = if rail_ids.contains(&sides[1].2) {
                sides[1].2
            } else {
                sides[1].3
            };
            let ((ax, ay), sa) = match pin_exit_facing(graph, sides[0].2, a_pin, a_toward) {
                Some(v) => v,
                None => continue,
            };
            let ((bx2, by2), sb) = match pin_exit_facing(graph, sides[1].2, b_pin, b_toward) {
                Some(v) => v,
                None => continue,
            };

            // Defensive: if P's two pins still share an id (legacy owner-fallback collapse), mint a
            // second one so the two wires get two exit points. Normally a no-op after the net fix.
            let pin_a = sides[0].1;
            let mut pin_b = sides[1].1;
            if pin_a == pin_b {
                pin_b = synth;
                synth += 1;
                let ni = sides[1].0;
                if let Some(ep) = graph.nets[ni]
                    .endpoints
                    .iter_mut()
                    .find(|e| e.box_id == pid && e.pin_id == pin_a)
                {
                    ep.pin_id = pin_b;
                }
            }

            place_passive_between(graph, pid, (ax, ay), pin_a, sa, (bx2, by2), pin_b, sb);
            if sweep == 2 {
                moved += 1;
            }
        }
    }

    if moved > 0 {
        crate::vlog!(
            "[layout::passive_inline] placed {} passive-chain member(s) inline",
            moved
        );
    }
}

// ============================================================================
// reinsert —— place passives back onto wires after layout (LEGACY — off the main path)
// ============================================================================

/// Called **after** layout, **before** routing: delete direct nets, place each passive on the wire between two neighbors' exit points,
/// restore its two original nets (split two-pin pin_id if necessary), hand to existing router for straight lines.
pub fn reinsert_passives(graph: &mut McVecGraph, stash: PassiveStash) {
    if stash.is_empty() {
        return;
    }

    // Delete all temporary direct nets
    let splice_ids: HashSet<i64> = stash.items.iter().map(|s| s.splice_nid).collect();
    graph.nets.retain(|n| !splice_ids.contains(&n.nid));

    let mut synth_pin = PASSIVE_PIN_BASE;

    for mut s in stash.items {
        let pid = s.bx.id;
        // Both neighbors are now placed by layout → get exit point (degenerate to box center if unavailable)
        let a_partner = box_center(graph, s.nb.0);
        let b_partner = box_center(graph, s.na.0);
        let ((ax, ay), sa) = match pin_exit_facing(graph, s.na.0, s.na.1, a_partner) {
            Some(p) => p,
            None => {
                // Neither neighbor placed well: restore as-is, don't place (revert to old behavior)
                graph.boxes.push(s.bx);
                graph.nets.extend(s.nets);
                continue;
            }
        };
        let ((bx2, by2), sb) = match pin_exit_facing(graph, s.nb.0, s.nb.1, b_partner) {
            Some(p) => p,
            None => {
                graph.boxes.push(s.bx);
                graph.nets.extend(s.nets);
                continue;
            }
        };

        // Two-pin pin_id: if owner-fallback collapsed to same one, synthesize second pin and rewrite 2nd original net
        let pin_a = s.p_pin.0;
        let mut pin_b = s.p_pin.1;
        if pin_a == pin_b {
            pin_b = synth_pin;
            synth_pin += 1;
            // s.nets[1] = P↔B, change P-end pin_id to pin_b
            if let Some(ep) = s.nets[1]
                .endpoints
                .iter_mut()
                .find(|e| e.box_id == pid && e.pin_id == pin_a)
            {
                ep.pin_id = pin_b;
            }
        }

        // First restore box + nets, then use place_passive_between with collision avoidance to place
        // (place_passive_between needs box already in graph.boxes to find it via iter_mut)
        graph.boxes.push(s.bx);
        graph.nets.extend(s.nets);
        place_passive_between(graph, pid, (ax, ay), pin_a, sa, (bx2, by2), pin_b, sb);
    }

    if crate::viz::debug::dump_enabled() {
        crate::vlog!("[layout::passive_inline] reinserted passives onto routed lines");
    }
}

// ============================================================================
// ★ v2 —— Series passives connected to power rails: place on [real neighbor]→[flag] wires
// ============================================================================
//
// ## Why collapse/reinsert can't handle these (main graph has no effect reason)
// `explode_power_rails_to_flags` runs **inside** `layouter.layout()`; collapse runs
// **before** layout. During collapse phase, V3V3 is still **one shared multi-terminal rail box** (not 2-terminal),
// per-consumer flags also don't exist yet → collapse skips these rail-connected passives.
//
// This pass runs **after** layout: by now rails have exploded into per-consumer flags, rail-connected resistor's rail side is already
// one 2-terminal stub (R ↔ flag). We place the resistor on [real neighbor exit point] → [flag pin] wire,
// set collinear pins, existing router then draws both segments straight.
//
// ## Safety (zero regression)
// - **Don't change network topology** (except "split second pin when two pins have same id", pure local rename).
// - Only change resistor box's x/y/entry_points. Worst case degenerates to "same as before".
// - Only recognize resistors with "**one side real major component (2-terminal) + one side flag (2-terminal)**";
//   device↔device series passives (both sides real) have been handled by collapse/reinsert → automatically **skip** here
//   (they have no flag side); bypass passives (both sides flag), multi-terminal nets, passive chains → skip (defer to later).

/// Starting base for temporary synthesized second pin id (offset from reinsert's, avoid collision)
const RAIL_PIN_BASE: i64 = 4_700_000_000;

/// Called **after** layout, **before** routing: place rail-connected series passives on [neighbor]→[flag] wires.
pub fn straighten_rail_passives(graph: &mut McVecGraph) {
    let rail_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();
    let passive_set: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.is_two_pin_passive())
        .map(|b| b.id)
        .collect();
    if passive_set.is_empty() || rail_ids.is_empty() {
        return;
    }
    let passive_ids: Vec<i64> = {
        // [P0-DET] iterate in deterministic id order, not HashSet order: the loop
        // below allocates synthetic pin ids (`synth`) per passive, so iteration
        // order leaks into routing if left to HashSet randomization.
        let mut v: Vec<i64> = passive_set.iter().copied().collect();
        v.sort_unstable();
        v
    };
    let mut synth = RAIL_PIN_BASE;
    let mut moved = 0usize;

    for pid in passive_ids {
        let touching: Vec<usize> = graph
            .nets
            .iter()
            .enumerate()
            .filter(|(_, n)| n.endpoints.iter().any(|e| e.box_id == pid))
            .map(|(i, _)| i)
            .collect();
        if touching.len() != 2 {
            continue;
        }

        // Parse both sides: one side real major component, one side flag, each must be 2-terminal (P + single opposite endpoint)
        let mut real: Option<(i64, i64, i64)> = None; // (Xbox, Xpin, R's pin in real net)
        let mut flag: Option<(usize, i64, i64)> = None; // (net index, Fbox, R's pin in flag net)
        let mut bad = false;
        for &ni in &touching {
            let net = &graph.nets[ni];
            let rpin = net
                .endpoints
                .iter()
                .find(|e| e.box_id == pid)
                .map(|e| e.pin_id)
                .unwrap_or(0);
            let others: Vec<&EndpointRef> =
                net.endpoints.iter().filter(|e| e.box_id != pid).collect();
            if others.len() != 1 {
                bad = true;
                break;
            }
            let o = others[0];
            if rail_ids.contains(&o.box_id) {
                flag = Some((ni, o.box_id, rpin));
            } else if !passive_set.contains(&o.box_id) {
                real = Some((o.box_id, o.pin_id, rpin));
            } else {
                bad = true; // Neighbor is another passive → chain, skip
                break;
            }
        }
        if bad {
            continue;
        }
        let (Some((xb, xp, r_real_pin)), Some((flag_ni, fb, r_flag_pin))) = (real, flag) else {
            continue; // Not "one real one flag" → skip (device-to-device series passives are skipped here)
        };

        // Anchor: neighbor exit point + flag pin (flag pin pin_id == flag box_id)
        let pcenter = box_center(graph, pid);
        let ((ax, ay), sa) = match pin_exit_facing(graph, xb, xp, box_center(graph, fb).or(pcenter))
        {
            Some(p) => p,
            None => continue,
        };
        let ((fx, fy), sf) = match pin_exit_facing(graph, fb, fb, box_center(graph, xb).or(pcenter))
        {
            Some(p) => p,
            None => continue,
        };

        // Both pins same id (owner-fallback collapse) → synthesize new pin for flag side, rewrite R end in flag net
        let pin_real = r_real_pin;
        let mut pin_flag = r_flag_pin;
        if pin_real == pin_flag {
            pin_flag = synth;
            synth += 1;
            if let Some(ep) = graph.nets[flag_ni]
                .endpoints
                .iter_mut()
                .find(|e| e.box_id == pid && e.pin_id == pin_real)
            {
                ep.pin_id = pin_flag;
            }
        }

        place_passive_between(graph, pid, (ax, ay), pin_real, sa, (fx, fy), pin_flag, sf);
        // ★ R has moved to real neighbor's exit line → also move flag to R far-end pin front, connect into straight line
        //   (otherwise flag still at where place_flags placed it based on R's old position, R→flag will be diagonal/bent).
        align_flag_to_passive(graph, fb, pid, pin_flag);
        moved += 1;
    }

    if moved > 0 {
        crate::vlog!(
            "[layout::passive_inline] straightened {} rail-adjacent passive(s)",
            moved
        );
    }
}

/// Place passive component `pid` on **exit direction extension line of an anchor point, right next to it** (aligned placement):
/// near pin locked on anchor point exit point extension line → wire **straight out** from anchor into resistor; far pin faces outward,
/// router makes one bend to opposite endpoint. Align to anchor with "horizontal exit (Left/Right)" (horizontal draw + vertical bus is clean
/// style; if no horizontal exit, align to a). Center along axis towards opposite endpoint, slide outward if blocked / vertical fallback.
fn place_passive_between(
    graph: &mut McVecGraph,
    pid: i64,
    a: (f64, f64),
    pin_a: i64,
    side_a: EntrySide,
    b: (f64, f64),
    pin_b: i64,
    side_b: EntrySide,
) {
    // Size + existing pin names (immutable snapshot, avoids borrow conflict with iter_mut below)
    let (bw, bh, name_a, name_b) = match graph.boxes.iter().find(|x| x.id == pid) {
        Some(bx) => (bx.w, bx.h, pin_name_of(bx, pin_a), pin_name_of(bx, pin_b)),
        None => return,
    };
    let long = bw.max(bh);
    let short = bw.min(bh);

    // ── Select alignment anchor ──
    // Prefer anchor with "horizontal exit (Left/Right)" (horizontal draw is clean); if both horizontal/both vertical → align to a.
    let align_a = side_is_horizontal(&side_a) || !side_is_horizontal(&side_b);
    let (anchor, aside, far_pt, near_pin, near_name, far_pin, far_name) = if align_a {
        (
            a,
            side_a.clone(),
            b,
            pin_a,
            name_a.clone(),
            pin_b,
            name_b.clone(),
        )
    } else {
        (
            b,
            side_b.clone(),
            a,
            pin_b,
            name_b.clone(),
            pin_a,
            name_a.clone(),
        )
    };

    // Orientation = anchor exit axis: horizontal exit → horizontal placement (w≥h); vertical exit → vertical placement (h>w).
    let horizontal = side_is_horizontal(&aside);
    let (ow, oh) = if horizontal {
        (long, short)
    } else {
        (short, long)
    };

    // ── Landing point: near pin locked on anchor exit point extension line, centered along axis towards opposite endpoint (clamp to at least GAP) ──
    let (px, py) = anchor;
    let (qx, qy) = far_pt;
    const GAP0: f64 = 22.0;
    let base = passive_spot_on_ray(px, py, qx, qy, &aside, ow, oh, GAP0);

    // Collision avoidance: 1) slide along exit direction (maintain alignment); 2) if still blocked, move perpendicular (introduce small elbow, router connects).
    let mut spot = if !overlaps_any_box(graph, pid, base.0, base.1, ow, oh) {
        Some(base)
    } else {
        None
    };
    if spot.is_none() {
        let step = long.max(28.0) * 0.7;
        for k in 1..=5 {
            let cand = shift_along_ray(base, &aside, step * k as f64);
            if !overlaps_any_box(graph, pid, cand.0, cand.1, ow, oh) {
                spot = Some(cand);
                break;
            }
        }
    }
    if spot.is_none() {
        let perp = short.max(24.0) * 1.4;
        'perp: for k in 1..=4 {
            let d = perp * k as f64;
            for sign in [d, -d] {
                let cand = shift_perp_ray(base, &aside, sign);
                if !overlaps_any_box(graph, pid, cand.0, cand.1, ow, oh) {
                    spot = Some(cand);
                    break 'perp;
                }
            }
        }
    }
    let (xx, yy) = spot.unwrap_or(base);

    // ── Place + post-orientation width/height + pins (near faces anchor / far faces outward) ──
    let bx = match graph.boxes.iter_mut().find(|x| x.id == pid) {
        Some(b) => b,
        None => return,
    };
    bx.x = xx;
    bx.y = yy;
    bx.w = ow; // ★ Write back post-orientation width/height (rendering layer judges horizontal/vertical based on this)
    bx.h = oh;
    let (near_side, far_side) = match aside {
        EntrySide::Right => (EntrySide::Left, EntrySide::Right),
        EntrySide::Left => (EntrySide::Right, EntrySide::Left),
        EntrySide::Bottom => (EntrySide::Top, EntrySide::Bottom),
        EntrySide::Top => (EntrySide::Bottom, EntrySide::Top),
    };
    bx.entry_points = vec![
        EntryPoint {
            pin_id: near_pin,
            pin_name: near_name,
            side: near_side,
            offset: 0.5,
        },
        EntryPoint {
            pin_id: far_pin,
            pin_name: far_name,
            side: far_side,
            offset: 0.5,
        },
    ];
}

/// Whether exit direction is horizontal (Left/Right)
fn side_is_horizontal(s: &EntrySide) -> bool {
    matches!(s, EntrySide::Left | EntrySide::Right)
}

/// Anchor point (px,py) exit direction side: place the ow×oh resistor on the extension line of the exit, returns top-left corner.
/// - Perpendicular to exit direction (perp): locked at anchor coordinates → near pin (offset 0.5) lands right on anchor extension line → straight wire;
/// - Along exit direction (along): take midpoint of anchor and opposite end, lean toward opposite end, but clamp to at least GAP (not behind anchor).
fn passive_spot_on_ray(
    px: f64,
    py: f64,
    qx: f64,
    qy: f64,
    side: &EntrySide,
    ow: f64,
    oh: f64,
    gap: f64,
) -> (f64, f64) {
    match side {
        EntrySide::Right => {
            let along = ((px + qx) / 2.0).max(px + gap + ow / 2.0);
            (along - ow / 2.0, py - oh / 2.0)
        }
        EntrySide::Left => {
            let along = ((px + qx) / 2.0).min(px - gap - ow / 2.0);
            (along - ow / 2.0, py - oh / 2.0)
        }
        EntrySide::Bottom => {
            let along = ((py + qy) / 2.0).max(py + gap + oh / 2.0);
            (px - ow / 2.0, along - oh / 2.0)
        }
        EntrySide::Top => {
            let along = ((py + qy) / 2.0).min(py - gap - oh / 2.0);
            (px - ow / 2.0, along - oh / 2.0)
        }
    }
}

/// Slide the landing point outward along exit direction by d (away from anchor) —— keep near pin aligned
fn shift_along_ray((x, y): (f64, f64), side: &EntrySide, d: f64) -> (f64, f64) {
    match side {
        EntrySide::Right => (x + d, y),
        EntrySide::Left => (x - d, y),
        EntrySide::Bottom => (x, y + d),
        EntrySide::Top => (x, y - d),
    }
}

/// Move landing point perpendicular to exit direction by d (fallback, will introduce small elbow)
fn shift_perp_ray((x, y): (f64, f64), side: &EntrySide, d: f64) -> (f64, f64) {
    match side {
        EntrySide::Left | EntrySide::Right => (x, y + d),
        EntrySide::Top | EntrySide::Bottom => (x + d, y),
    }
}

/// Whether candidate landing point AABB hits any **other** box (major / flag / other passives), with PAD margin
fn overlaps_any_box(graph: &McVecGraph, self_id: i64, x: f64, y: f64, w: f64, h: f64) -> bool {
    const PAD: f64 = 4.0;
    graph.boxes.iter().any(|b| {
        b.id != self_id
            && x < b.x + b.w + PAD
            && x + w + PAD > b.x
            && y < b.y + b.h + PAD
            && y + h + PAD > b.y
    })
}

/// Get existing name of a pin on box (empty string if none)
fn pin_name_of(b: &McVecBox, pin_id: i64) -> String {
    b.entry_points
        .iter()
        .find(|e| e.pin_id == pin_id)
        .map(|e| e.pin_name.clone())
        .unwrap_or_default()
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Center point of a box (returns None if not found)
fn box_center(graph: &McVecGraph, box_id: i64) -> Option<(f64, f64)> {
    graph
        .boxes
        .iter()
        .find(|b| b.id == box_id)
        .map(|b| (b.x + b.w / 2.0, b.y + b.h / 2.0))
}

/// Get the exit point of a pin on a box; if pin_id doesn't match (might have been split-modified during layout), degenerate to
/// "midpoint of the edge towards `toward` direction". `toward` usually passes the opposite neighbor's center.
fn pin_exit_facing(
    graph: &McVecGraph,
    box_id: i64,
    pin_id: i64,
    toward: Option<(f64, f64)>,
) -> Option<((f64, f64), EntrySide)> {
    let b = graph.boxes.iter().find(|x| x.id == box_id)?;
    // 1) Exact pin_id match
    if let Some(ep) = b.entry_points.iter().find(|e| e.pin_id == pin_id) {
        return Some((abs_of(b, &ep.side, ep.offset), ep.side.clone()));
    }
    // 2) Degenerate: pick edge midpoint based on toward direction
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let (tx, ty) = toward.unwrap_or((cx, cy));
    let side = if (tx - cx).abs() >= (ty - cy).abs() {
        if tx >= cx {
            EntrySide::Right
        } else {
            EntrySide::Left
        }
    } else if ty >= cy {
        EntrySide::Bottom
    } else {
        EntrySide::Top
    };
    let pt = abs_of(b, &side, 0.5);
    Some((pt, side))
}

/// (side, offset) → absolute coordinates
fn abs_of(b: &McVecBox, side: &EntrySide, offset: f64) -> (f64, f64) {
    match side {
        EntrySide::Top => (b.x + b.w * offset, b.y),
        EntrySide::Bottom => (b.x + b.w * offset, b.y + b.h),
        EntrySide::Left => (b.x, b.y + b.h * offset),
        EntrySide::Right => (b.x + b.w, b.y + b.h * offset),
    }
}

/// Move flag `flag_id` to right in front of the exit direction of passive `passive_id`'s pin `far_pin`, so that
/// `R → flag` goes in a straight line (flag's pin turns back to face R's pin). Only moves flag box's x/y + that single pin's side.
fn align_flag_to_passive(graph: &mut McVecGraph, flag_id: i64, passive_id: i64, far_pin: i64) {
    // R far-end pin's exit point + side
    let (fpx, fpy, fside) = match graph.boxes.iter().find(|b| b.id == passive_id) {
        Some(r) => match r.entry_points.iter().find(|e| e.pin_id == far_pin) {
            Some(ep) => {
                let (x, y) = abs_of(r, &ep.side, ep.offset);
                (x, y, ep.side.clone())
            }
            None => return,
        },
        None => return,
    };
    const FGAP: f64 = 28.0;
    if let Some(f) = graph.boxes.iter_mut().find(|b| b.id == flag_id) {
        let (fw, fh) = (f.w, f.h);
        // Flag placed on extension line of R far-end pin's exit direction, its pin turns back to face R
        let (nx, ny, flag_side) = match fside {
            EntrySide::Right => (fpx + FGAP, fpy - fh / 2.0, EntrySide::Left),
            EntrySide::Left => (fpx - FGAP - fw, fpy - fh / 2.0, EntrySide::Right),
            EntrySide::Bottom => (fpx - fw / 2.0, fpy + FGAP, EntrySide::Top),
            EntrySide::Top => (fpx - fw / 2.0, fpy - FGAP - fh, EntrySide::Bottom),
        };
        f.x = nx;
        f.y = ny;
        // Flag only has one pin: side set to face R, offset 0.5 → pin lands right on extension line of R far-end pin
        if let Some(ep) = f.entry_points.first_mut() {
            ep.side = flag_side;
            ep.offset = 0.5;
        }
    }
}

/// When splicing two nets: Power↔Ground directly deemed "will short", skip
fn is_power_ground_short(a: &NetKind, b: &NetKind) -> bool {
    matches!(
        (a, b),
        (NetKind::Power, NetKind::Ground) | (NetKind::Ground, NetKind::Power)
    )
}

/// What kind to use for direct-connect net (layout adjacency isn't kind-sensitive, take the "more semantic" side)
fn merge_kind(a: &NetKind, b: &NetKind) -> NetKind {
    use NetKind::*;
    match (a, b) {
        (SubModuleIO, _) | (_, SubModuleIO) => SubModuleIO,
        (Power, _) | (_, Power) => Power,
        (Ground, _) | (_, Ground) => Ground,
        (Bus(n), _) | (_, Bus(n)) => Bus(*n),
        _ => Signal,
    }
}

// ============================================================================
// PROBE-D′ — rail-adjacent 去耦候选计数
// ----------------------------------------------------------------------------
// 在 straighten_rail_passives 运行前统计"应当被它处理"的 (b) 类去耦候选。
// 谓词与 straighten_rail_passives 的匹配逻辑逐条对齐。
// 期望 [PROBE-D'] candidates = N 与 straighten 的 "straightened N" 相等。
// ============================================================================

pub fn probe_rail_passive_candidates(graph: &McVecGraph) {
    if !crate::viz::debug::dump_enabled() {
        return;
    }

    let rail_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();
    let passive_set: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.is_two_pin_passive())
        .map(|b| b.id)
        .collect();
    if rail_ids.is_empty() || passive_set.is_empty() {
        crate::vlog!("[PROBE-D'] candidates = 0 (no rails or no passives)");
        return;
    }

    let mut names: Vec<String> = Vec::new();
    for &pid in &passive_set {
        let touching: Vec<&VizNet> = graph
            .nets
            .iter()
            .filter(|n| n.endpoints.iter().any(|e| e.box_id == pid))
            .collect();
        if touching.len() != 2 {
            continue;
        }
        let mut has_real = false;
        let mut has_flag = false;
        let mut bad = false;
        for net in &touching {
            let others: Vec<&EndpointRef> =
                net.endpoints.iter().filter(|e| e.box_id != pid).collect();
            if others.len() != 1 {
                bad = true;
                break;
            }
            let o = others[0];
            if rail_ids.contains(&o.box_id) {
                has_flag = true;
            } else if !passive_set.contains(&o.box_id) {
                has_real = true;
            } else {
                bad = true; // neighbor is another passive → chain
                break;
            }
        }
        if !bad && has_real && has_flag {
            let nm = graph
                .boxes
                .iter()
                .find(|b| b.id == pid)
                .map(|b| b.name.clone())
                .unwrap_or_else(|| format!("#{pid}"));
            names.push(nm);
        }
    }

    names.sort();
    crate::vlog!(
        "[PROBE-D'] rail-adjacent decap candidates = {}: {:?}  (期望 == straightened N)",
        names.len(),
        names
    );
}

// ============================================================================
// PROBE-C — 普查每层"散乱元素"的真实构成
// ----------------------------------------------------------------------------
// 三类：Dot 占位、0-degree 孤儿、dangling pin 悬空引脚。
// 看哪一类数大，Plan C 就打哪一类。
// ============================================================================

pub fn probe_scatter_census(graph: &McVecGraph) {
    if !crate::viz::debug::dump_enabled() {
        return;
    }

    let mut connected_boxes: HashSet<i64> = HashSet::new();
    let mut connected_pins: HashSet<(i64, i64)> = HashSet::new();
    for n in &graph.nets {
        for e in &n.endpoints {
            connected_boxes.insert(e.box_id);
            connected_pins.insert((e.box_id, e.pin_id));
        }
    }

    let mut dot = 0usize;
    let mut power_label = 0usize;
    let mut two_pin = 0usize;
    let mut zero_degree = 0usize;
    let mut boxes_with_dangling = 0usize;
    let mut dangling_pins = 0usize;
    let mut zero_deg_names: Vec<String> = Vec::new();

    for b in &graph.boxes {
        match b.kind {
            BoxKind::Dot => dot += 1,
            BoxKind::PowerLabel => power_label += 1,
            BoxKind::TwoPin => two_pin += 1,
            _ => {}
        }
        if !connected_boxes.contains(&b.id) {
            zero_degree += 1;
            if zero_deg_names.len() < 12 {
                zero_deg_names.push(b.name.clone());
            }
        }
        let d = b
            .pins
            .iter()
            .filter(|p| !connected_pins.contains(&(b.id, p.id)))
            .count();
        if d > 0 {
            boxes_with_dangling += 1;
            dangling_pins += d;
        }
    }

    crate::vlog!(
        "[PROBE-C] layer '{}': boxes={} nets={} | Dot={} PowerLabel={} TwoPin={} | 0-degree={} {:?} | dangling: {} box(es)/{} pin(s)",
        graph.name,
        graph.boxes.len(),
        graph.nets.len(),
        dot,
        power_label,
        two_pin,
        zero_degree,
        zero_deg_names,
        boxes_with_dangling,
        dangling_pins
    );
}

// ============================================================================
// PROBE-COLL — 点名 box_box 碰撞对
// ----------------------------------------------------------------------------
// 在 audit_all 之前调用，输出每对重叠盒子的名字+kind+重叠量。
// 据此定位是 place_flags 堆叠还是去重叠覆盖不足。
// ============================================================================

pub fn probe_box_collisions(graph: &McVecGraph) {
    if !crate::viz::debug::dump_enabled() {
        return;
    }
    let bs = &graph.boxes;
    let mut pairs: Vec<(String, String, f64)> = Vec::new();
    for i in 0..bs.len() {
        for j in (i + 1)..bs.len() {
            let a = &bs[i];
            let b = &bs[j];
            let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
            let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
            if ox > 0.0 && oy > 0.0 {
                let overlap = ox.min(oy);
                pairs.push((
                    format!("{}[{:?}]", a.name, a.kind),
                    format!("{}[{:?}]", b.name, b.kind),
                    overlap,
                ));
            }
        }
    }

    if pairs.is_empty() {
        crate::vlog!("[PROBE-COLL] layer '{}': no box-box overlaps", graph.name);
    } else {
        crate::vlog!(
            "[PROBE-COLL] layer '{}': {} overlapping pair(s):",
            graph.name,
            pairs.len()
        );
        for (a, b, ov) in &pairs {
            crate::vlog!("[PROBE-COLL]   {} ✕ {}  (overlap={:.0})", a, b, ov);
        }
    }
}

// ============================================================================
// ★ P2: Bridge passive placement (transposed CAP/R in two-lane series)
// ============================================================================
//
// A bridge passive is a 2-pin passive whose two pins are in *different* nets,
// and each net has 2+ non-passive, non-rail neighbours (one on each side of the
// bridge). This corresponds to CAP' in expressions like:
//
//   [RES, RES] -> CAP' -> [RES, RES]
//
// We place the passive **vertically** between the two horizontal lanes, with
// pin1 on the top lane and pin2 on the bottom lane.

/// Detect and place bridge passives vertically between their two lanes.
/// Runs **after** layout, **before** routing (right after place_passive_chains).
pub fn place_bridge_passives(graph: &mut McVecGraph) {
    let passive_ids: Vec<i64> = {
        let mut v: Vec<i64> = graph
            .boxes
            .iter()
            .filter(|b| b.is_two_pin_passive())
            .map(|b| b.id)
            .collect();
        v.sort_unstable();
        v
    };
    if passive_ids.is_empty() {
        return;
    }
    let passive_set: HashSet<i64> = passive_ids.iter().copied().collect();
    let rail_ids: HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| is_rail_box(b))
        .map(|b| b.id)
        .collect();

    let mut count = 0usize;

    for pid in passive_ids {
        // P must touch exactly 2 nets.
        let touching: Vec<usize> = graph
            .nets
            .iter()
            .enumerate()
            .filter(|(_, n)| n.endpoints.iter().any(|e| e.box_id == pid))
            .map(|(i, _)| i)
            .collect();
        if touching.len() != 2 {
            continue;
        }

        // For each net, collect the non-passive, non-rail neighbours.
        let mut lane_neighbours: Vec<Vec<i64>> = Vec::new();
        let mut lane_pins: Vec<i64> = Vec::new();
        let mut ok = true;
        for &ni in &touching {
            let net = &graph.nets[ni];
            let p_pin = net
                .endpoints
                .iter()
                .find(|e| e.box_id == pid)
                .map(|e| e.pin_id);
            let others: Vec<i64> = net
                .endpoints
                .iter()
                .filter(|e| {
                    e.box_id != pid
                        && !rail_ids.contains(&e.box_id)
                        && !passive_set.contains(&e.box_id)
                })
                .map(|e| e.box_id)
                .collect();
            if others.len() < 2 {
                // Need at least 2 non-passive neighbours (one on each side)
                ok = false;
                break;
            }
            if let Some(pp) = p_pin {
                lane_neighbours.push(others);
                lane_pins.push(pp);
            } else {
                ok = false;
                break;
            }
        }
        if !ok || lane_neighbours.len() != 2 {
            continue;
        }

        // Safety: don't short Power to Ground
        let k0 = &graph.nets[touching[0]].kind;
        let k1 = &graph.nets[touching[1]].kind;
        if is_power_ground_short(k0, k1) {
            continue;
        }

        // Compute the average Y of each lane's neighbours.
        let avg_y = |nbrs: &[i64]| -> Option<f64> {
            let ys: Vec<f64> = nbrs
                .iter()
                .filter_map(|&bid| graph.boxes.iter().find(|b| b.id == bid))
                .map(|b| b.y + b.h / 2.0)
                .collect();
            if ys.is_empty() {
                None
            } else {
                Some(ys.iter().sum::<f64>() / ys.len() as f64)
            }
        };

        let y0 = match avg_y(&lane_neighbours[0]) {
            Some(v) => v,
            None => continue,
        };
        let y1 = match avg_y(&lane_neighbours[1]) {
            Some(v) => v,
            None => continue,
        };

        // Determine top and bottom lane. Pin 0 → top lane, Pin 1 → bottom lane.
        let (top_y, bot_y, top_pin, bot_pin, top_nbrs, bot_nbrs) = if y0 < y1 {
            (
                y0,
                y1,
                lane_pins[0],
                lane_pins[1],
                &lane_neighbours[0],
                &lane_neighbours[1],
            )
        } else {
            (
                y1,
                y0,
                lane_pins[1],
                lane_pins[0],
                &lane_neighbours[1],
                &lane_neighbours[0],
            )
        };

        // Compute the X position: midpoint of the neighbours' X range.
        let x_center = {
            let mut xs: Vec<f64> = top_nbrs
                .iter()
                .chain(bot_nbrs.iter())
                .filter_map(|&bid| graph.boxes.iter().find(|b| b.id == bid))
                .map(|b| b.x + b.w / 2.0)
                .collect();
            if xs.is_empty() {
                continue;
            }
            xs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            (xs[0] + xs[xs.len() - 1]) / 2.0
        };

        // Get the box dimensions.
        let (bw, bh, pin_name_a, pin_name_b) = match graph.boxes.iter().find(|x| x.id == pid) {
            Some(bx) => (
                bx.w,
                bx.h,
                pin_name_of(bx, top_pin),
                pin_name_of(bx, bot_pin),
            ),
            None => continue,
        };
        let long = bw.max(bh);
        let short = bw.min(bh);

        // Place vertically: width=short, height=long.
        let ow = short;
        let oh = long;
        let mid_y = (top_y + bot_y) / 2.0;

        let bx = match graph.boxes.iter_mut().find(|x| x.id == pid) {
            Some(b) => b,
            None => continue,
        };
        bx.x = x_center - ow / 2.0;
        bx.y = mid_y - oh / 2.0;
        bx.w = ow;
        bx.h = oh;
        bx.visual_role = Some(VisualRole::BridgePassive);

        // Entry points: top pin on Top side, bottom pin on Bottom side.
        bx.entry_points = vec![
            EntryPoint {
                pin_id: top_pin,
                pin_name: pin_name_a,
                side: EntrySide::Top,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: bot_pin,
                pin_name: pin_name_b,
                side: EntrySide::Bottom,
                offset: 0.5,
            },
        ];

        count += 1;
        crate::vlog!(
            "[layout::passive_inline] placed bridge passive {} (pid={pid}) between y={top_y:.0} and y={bot_y:.0}",
            bx.name,
        );
    }

    if count > 0 {
        crate::vlog!(
            "[layout::passive_inline] placed {} bridge passive(s)",
            count
        );
    }
}
