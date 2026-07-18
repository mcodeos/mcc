// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase 4 · Idiom recognition (范式识别) + M11 Idiom-aware Placement
//!
//! Analyze a laid-out `McVecGraph` to identify sub-circuit idioms (decoupling
//! caps, diff pairs, pullups, pulldowns, etc.) and compute penalties for violations
//! of conventional drawing practice. M11 extends this from read-only reporting to
//! generating placement constraints that can be applied in FlowLayouter's pipeline.
//!
//! ## Modules
//! - `mod.rs` — public API + detection orchestration + legacy read-only analysis
//! - `model.rs` — IdiomPlacementModel, IdiomInstance, PlacementConstraint
//! - `place.rs` — apply_idiom_placement (pre-pin and post-pin phases)
//! - `report.rs` — IdiomPlacementReport

pub mod model;
pub mod place;
pub mod report;

use std::collections::{HashMap, HashSet};

use crate::vector::graph::{McVecBox, McVecGraph, NetKind, Symbol};
use crate::viz::layout::schematic_radial::collect_connected_net_kinds;

use model::{IdiomInstance, IdiomInstanceKind, InstanceSource, PlacementConstraint};

// ============================================================================
// Data types (legacy read-only)
// ============================================================================

/// Identified sub-circuit idiom.
#[derive(Debug, Clone, PartialEq)]
pub struct IdiomMatch {
    pub kind: IdiomKind,
    /// Box IDs that participate in this idiom.
    pub member_box_ids: Vec<i64>,
    /// Continuous symmetry penalty (e.g., diff pair y-offset).
    pub symmetry_penalty: f64,
    /// Whether this idiom violates conventional drawing practice.
    pub idiom_violation: bool,
}

/// Recognized idiom categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdiomKind {
    /// Decoupling capacitor (I1): 2-pin cap with one Power + one Ground net.
    Decoupling,
    /// Differential pair (I6): two nets/boxes forming P/N pair.
    DiffPair,
    /// Pullup resistor (I7): resistor between signal and power rail.
    Pullup,
    /// Pulldown resistor (I8): resistor between signal and ground.
    Pulldown,
}

// ============================================================================
// Main API (legacy read-only)
// ============================================================================

/// Analyze a laid-out graph and return all recognized idioms.
///
/// Read-only: does not modify the graph.
pub fn analyze(graph: &McVecGraph) -> Vec<IdiomMatch> {
    let connected = collect_connected_net_kinds(graph);
    let net_kind_map = build_net_kind_map(graph);

    let mut matches = Vec::new();

    // I1: Decoupling capacitors
    matches.extend(detect_decoupling(graph, &connected, &net_kind_map));

    // I6: Differential pairs
    matches.extend(detect_diff_pair(graph, &net_kind_map));

    // I7: Pullup resistors
    matches.extend(detect_pullup(graph, &connected));

    // I8: Pulldown resistors
    matches.extend(detect_pulldown(graph, &connected));

    matches
}

/// Aggregate idiom matches into (symmetry_penalty, idiom_violation) for scoring.
pub fn penalty_summary(matches: &[IdiomMatch]) -> (f64, usize) {
    let sym = matches
        .iter()
        .map(|m| m.symmetry_penalty)
        .sum::<f64>()
        .max(0.0);
    let vio = matches.iter().filter(|m| m.idiom_violation).count();
    (sym, vio)
}

// ============================================================================
// M11 — Placement-oriented detection
// ============================================================================

/// Detect idiom instances suitable for placement.
///
/// Builds `IdiomInstance` structs with anchor/satellite/pin detail, respecting
/// protected boxes (ladder-locked, geom_locked, etc.).
pub fn detect_placement_instances(
    graph: &McVecGraph,
    protected_box_ids: &HashSet<i64>,
) -> Vec<IdiomInstance> {
    let connected = collect_connected_net_kinds(graph);
    let net_kind_map = build_net_kind_map(graph);
    let mut instances = Vec::new();

    // Decoupling capacitors
    instances.extend(detect_decoupling_instances(
        graph,
        &connected,
        &net_kind_map,
        protected_box_ids,
    ));

    // Pullup resistors
    instances.extend(detect_pullup_instances(
        graph,
        &connected,
        protected_box_ids,
    ));

    // Pulldown resistors
    instances.extend(detect_pulldown_instances(
        graph,
        &connected,
        protected_box_ids,
    ));

    // Diff pairs
    instances.extend(detect_diff_pair_instances(
        graph,
        &net_kind_map,
        protected_box_ids,
    ));

    instances
}

/// Generate placement constraints from idiom instances.
pub fn generate_constraints(instances: &[IdiomInstance]) -> Vec<PlacementConstraint> {
    let mut constraints = Vec::new();

    for inst in instances {
        match inst.kind {
            IdiomInstanceKind::Decoupling => {
                for &sat_id in &inst.satellite_box_ids {
                    constraints.push(PlacementConstraint {
                        kind: model::ConstraintKind::NearAnchor,
                        source_kind: inst.kind,
                        target_box_id: sat_id,
                        anchor_box_id: inst.anchor_box_id,
                        preferred_side: Some(model::AnchorSide::Above),
                        align_axis: Some(model::AlignAxis::Vertical),
                        distance_range: Some((40.0, 120.0)),
                        priority: 10,
                        hard: false,
                    });
                }
            }
            IdiomInstanceKind::Pullup => {
                for &sat_id in &inst.satellite_box_ids {
                    constraints.push(PlacementConstraint {
                        kind: model::ConstraintKind::NearAnchor,
                        source_kind: inst.kind,
                        target_box_id: sat_id,
                        anchor_box_id: inst.anchor_box_id,
                        preferred_side: Some(model::AnchorSide::Above),
                        align_axis: Some(model::AlignAxis::Vertical),
                        distance_range: Some((40.0, 100.0)),
                        priority: 10,
                        hard: false,
                    });
                }
            }
            IdiomInstanceKind::Pulldown => {
                for &sat_id in &inst.satellite_box_ids {
                    constraints.push(PlacementConstraint {
                        kind: model::ConstraintKind::NearAnchor,
                        source_kind: inst.kind,
                        target_box_id: sat_id,
                        anchor_box_id: inst.anchor_box_id,
                        preferred_side: Some(model::AnchorSide::Below),
                        align_axis: Some(model::AlignAxis::Vertical),
                        distance_range: Some((40.0, 100.0)),
                        priority: 10,
                        hard: false,
                    });
                }
            }
            IdiomInstanceKind::DiffPair => {
                // Diff pair: soft placement only in v1 — report, don't move
                // Add pin-side intent constraints for same-side P/N pins
                for &sat_id in &inst.satellite_box_ids {
                    constraints.push(PlacementConstraint {
                        kind: model::ConstraintKind::PinSideIntent,
                        source_kind: inst.kind,
                        target_box_id: sat_id,
                        anchor_box_id: inst.anchor_box_id,
                        preferred_side: None,
                        align_axis: None,
                        distance_range: None,
                        priority: 20,
                        hard: false,
                    });
                }
            }
        }
    }

    // Sort by priority for deterministic application
    constraints.sort_by_key(|c| (c.priority, c.target_box_id, c.anchor_box_id));
    constraints
}

// ============================================================================
// Helpers
// ============================================================================

/// Build net_name → NetKind map.
fn build_net_kind_map(graph: &McVecGraph) -> HashMap<String, NetKind> {
    let mut map = HashMap::new();
    for net in &graph.nets {
        map.insert(net.name.clone(), net.kind.clone());
    }
    map
}

/// Find the box by id, or None.
fn find_box(graph: &McVecGraph, id: i64) -> Option<&McVecBox> {
    graph.boxes.iter().find(|b| b.id == id)
}

/// Get the entry_points for a box (sorted for deterministic output).
fn sorted_entry_points(b: &McVecBox) -> Vec<&crate::vector::graph::box_def::EntryPoint> {
    let mut eps: Vec<&crate::vector::graph::box_def::EntryPoint> = b.entry_points.iter().collect();
    eps.sort_by_key(|e| (e.pin_id, e.pin_name.clone()));
    eps
}

/// Find the net IDs connected to a given box.
fn nets_for_box(graph: &McVecGraph, box_id: i64) -> Vec<&crate::vector::graph::VizNet> {
    graph
        .nets
        .iter()
        .filter(|n| n.endpoints.iter().any(|ep| ep.box_id == box_id))
        .collect()
}

/// Find the best signal anchor for a resistor (non-passive, non-rail endpoint).
fn find_signal_anchor(graph: &McVecGraph, box_id: i64, signal_net_id: i64) -> Option<(i64, i64)> {
    let net = graph.nets.iter().find(|n| n.nid == signal_net_id)?;
    // Find non-resistor, non-rail endpoints on the signal net
    let mut candidates: Vec<_> = net
        .endpoints
        .iter()
        .filter(|ep| ep.box_id != box_id)
        .filter_map(|ep| {
            let b = find_box(graph, ep.box_id)?;
            // Prefer IC, connector, module — avoid resistor, cap, rail
            if b.symbol == Symbol::Ic || b.symbol == Symbol::Module {
                Some((ep.box_id, ep.pin_id, 0))
            } else if b.symbol != Symbol::Resistor
                && b.symbol != Symbol::Capacitor
                && b.symbol != Symbol::PolarCapacitor
            {
                Some((ep.box_id, ep.pin_id, 1))
            } else {
                None
            }
        })
        .collect();

    // Sort by priority (lower = better), then stable by box_id
    candidates.sort_by_key(|(bid, _, pri)| (*pri, *bid));
    candidates.first().map(|(bid, pid, _)| (*bid, *pid))
}

// ============================================================================
// I1: Decoupling capacitor detection
// ============================================================================

/// Detect decoupling capacitors: TwoPin cap with one Power + one Ground net.
///
/// Penalty: larger Manhattan distance from cap center to IC power pin → higher
/// `idiom_violation` count (1 if within threshold, 2 if far).
fn detect_decoupling(
    graph: &McVecGraph,
    connected: &HashMap<i64, Vec<NetKind>>,
    _net_kind_map: &HashMap<String, NetKind>,
) -> Vec<IdiomMatch> {
    let mut matches = Vec::new();

    for b in &graph.boxes {
        if !matches!(b.symbol, Symbol::Capacitor | Symbol::PolarCapacitor) {
            continue;
        }

        let nk = connected.get(&b.id).cloned().unwrap_or_default();
        let has_power = nk.iter().any(|k| matches!(k, NetKind::Power));
        let has_ground = nk.iter().any(|k| matches!(k, NetKind::Ground));
        if !has_power || !has_ground {
            continue;
        }

        let cap_center = (b.x + b.w / 2.0, b.y + b.h / 2.0);

        let mut nearest_ic_dist: Option<f64> = None;
        for net in &graph.nets {
            let cap_on_net = net.endpoints.iter().any(|ep| ep.box_id == b.id);
            if !cap_on_net {
                continue;
            }
            for ep in &net.endpoints {
                if ep.box_id == b.id {
                    continue;
                }
                if let Some(ic) = find_box(graph, ep.box_id) {
                    if ic.symbol == Symbol::Ic {
                        let ic_center = (ic.x + ic.w / 2.0, ic.y + ic.h / 2.0);
                        let dist =
                            (cap_center.0 - ic_center.0).abs() + (cap_center.1 - ic_center.1).abs();
                        nearest_ic_dist = Some(nearest_ic_dist.map_or(dist, |d| d.min(dist)));
                    }
                }
            }
        }

        let violation = nearest_ic_dist.map_or(true, |d| d > 200.0);
        matches.push(IdiomMatch {
            kind: IdiomKind::Decoupling,
            member_box_ids: vec![b.id],
            symmetry_penalty: 0.0,
            idiom_violation: violation,
        });
    }

    matches
}

/// M11: Decoupling detection producing placement-ready instances.
fn detect_decoupling_instances(
    graph: &McVecGraph,
    connected: &HashMap<i64, Vec<NetKind>>,
    _net_kind_map: &HashMap<String, NetKind>,
    protected: &HashSet<i64>,
) -> Vec<IdiomInstance> {
    let mut instances = Vec::new();

    for b in &graph.boxes {
        if !matches!(b.symbol, Symbol::Capacitor | Symbol::PolarCapacitor) {
            continue;
        }
        // Skip protected caps (ladder bridge caps, etc.)
        if protected.contains(&b.id) || b.geom_locked {
            continue;
        }

        let nk = connected.get(&b.id).cloned().unwrap_or_default();
        let has_power = nk.iter().any(|k| matches!(k, NetKind::Power));
        let has_ground = nk.iter().any(|k| matches!(k, NetKind::Ground));
        if !has_power || !has_ground {
            continue;
        }

        // Find the power net and ground net for this cap
        let cap_nets = nets_for_box(graph, b.id);
        let power_net = cap_nets.iter().find(|n| matches!(n.kind, NetKind::Power));
        let _ground_net = cap_nets.iter().find(|n| matches!(n.kind, NetKind::Ground));

        // Find the best anchor IC on the power net
        let mut anchor: Option<(i64, i64, f64)> = None; // (box_id, pin_id, confidence)
        if let Some(pn) = power_net {
            for ep in &pn.endpoints {
                if ep.box_id == b.id {
                    continue;
                }
                if let Some(ic) = find_box(graph, ep.box_id) {
                    if ic.symbol == Symbol::Ic || ic.symbol == Symbol::Module {
                        let cap_cx = b.x + b.w / 2.0;
                        let cap_cy = b.y + b.h / 2.0;
                        let ic_cx = ic.x + ic.w / 2.0;
                        let ic_cy = ic.y + ic.h / 2.0;
                        let dist = (cap_cx - ic_cx).abs() + (cap_cy - ic_cy).abs();
                        let conf = if dist < 200.0 { 0.9 } else { 0.6 };
                        match anchor {
                            Some((_, _, c)) if conf > c => {
                                anchor = Some((ep.box_id, ep.pin_id, conf));
                            }
                            None => {
                                anchor = Some((ep.box_id, ep.pin_id, conf));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Fallback: closest IC on any shared net
        if anchor.is_none() {
            for net in &graph.nets {
                let cap_on_net = net.endpoints.iter().any(|ep| ep.box_id == b.id);
                if !cap_on_net {
                    continue;
                }
                for ep in &net.endpoints {
                    if ep.box_id == b.id {
                        continue;
                    }
                    if let Some(ic) = find_box(graph, ep.box_id) {
                        if ic.symbol == Symbol::Ic || ic.symbol == Symbol::Module {
                            let cap_cx = b.x + b.w / 2.0;
                            let cap_cy = b.y + b.h / 2.0;
                            let ic_cx = ic.x + ic.w / 2.0;
                            let ic_cy = ic.y + ic.h / 2.0;
                            let dist = (cap_cx - ic_cx).abs() + (cap_cy - ic_cy).abs();
                            let conf = if dist < 200.0 { 0.8 } else { 0.5 };
                            match anchor {
                                Some((_, _, c)) if conf > c => {
                                    anchor = Some((ep.box_id, ep.pin_id, conf));
                                }
                                None => {
                                    anchor = Some((ep.box_id, ep.pin_id, conf));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        if let Some((anchor_box_id, anchor_pin_id, confidence)) = anchor {
            instances.push(IdiomInstance {
                kind: IdiomInstanceKind::Decoupling,
                anchor_box_id,
                satellite_box_ids: vec![b.id],
                anchor_pin_id: Some(anchor_pin_id),
                signal_net_id: None,
                power_net_id: power_net.map(|n| n.nid),
                ground_net_id: None,
                confidence,
                source: InstanceSource::NetSemantic,
            });
        }
    }

    instances
}

// ============================================================================
// I6: Differential pair detection
// ============================================================================

/// Detect differential pairs: two nets whose names form P/N or +/- pairs.
///
/// Penalty: y-offset between the two boxes that carry the pair → `symmetry_penalty`.
fn detect_diff_pair(
    graph: &McVecGraph,
    _net_kind_map: &HashMap<String, NetKind>,
) -> Vec<IdiomMatch> {
    let mut matches = Vec::new();

    let pairs = find_diff_pairs(graph);

    for (_base, (net_p, net_n)) in &pairs {
        let boxes_p: Vec<i64> = net_p.endpoints.iter().map(|e| e.box_id).collect();
        let boxes_n: Vec<i64> = net_n.endpoints.iter().map(|e| e.box_id).collect();

        let mut member_ids: Vec<i64> = boxes_p.iter().chain(boxes_n.iter()).copied().collect();
        member_ids.sort();
        member_ids.dedup();

        if member_ids.len() < 2 {
            continue;
        }

        let mut penalty = 0.0;
        let mut centers_p = Vec::new();
        let mut centers_n = Vec::new();

        for &bid in &boxes_p {
            if let Some(b) = find_box(graph, bid) {
                centers_p.push(b.y + b.h / 2.0);
            }
        }
        for &bid in &boxes_n {
            if let Some(b) = find_box(graph, bid) {
                centers_n.push(b.y + b.h / 2.0);
            }
        }

        if !centers_p.is_empty() && !centers_n.is_empty() {
            let avg_p = centers_p.iter().sum::<f64>() / centers_p.len() as f64;
            let avg_n = centers_n.iter().sum::<f64>() / centers_n.len() as f64;
            penalty = (avg_p - avg_n).abs();
        }

        matches.push(IdiomMatch {
            kind: IdiomKind::DiffPair,
            member_box_ids: member_ids,
            symmetry_penalty: penalty,
            idiom_violation: false,
        });
    }

    matches
}

/// M11: Diff pair detection producing placement-ready instances.
fn detect_diff_pair_instances(
    graph: &McVecGraph,
    _net_kind_map: &HashMap<String, NetKind>,
    _protected: &HashSet<i64>,
) -> Vec<IdiomInstance> {
    let mut instances = Vec::new();
    let pairs = find_diff_pairs(graph);

    for (_base, (net_p, net_n)) in &pairs {
        let boxes_p: Vec<i64> = net_p.endpoints.iter().map(|e| e.box_id).collect();
        let boxes_n: Vec<i64> = net_n.endpoints.iter().map(|e| e.box_id).collect();

        let mut satellite_ids: Vec<i64> = Vec::new();
        let mut anchor_box_id: Option<i64> = None;

        // Find the common source/load boxes (anchor candidates)
        for &bid in &boxes_p {
            if boxes_n.contains(&bid) {
                // Box is on both P and N — this is a common endpoint (anchor)
                if let Some(b) = find_box(graph, bid) {
                    if b.symbol == Symbol::Ic || b.symbol == Symbol::Module {
                        anchor_box_id = Some(bid);
                    } else if anchor_box_id.is_none() {
                        anchor_box_id = Some(bid);
                    }
                }
            } else {
                satellite_ids.push(bid);
            }
        }
        for &bid in &boxes_n {
            if !boxes_p.contains(&bid) {
                satellite_ids.push(bid);
            }
        }

        satellite_ids.sort();
        satellite_ids.dedup();

        let anchor = anchor_box_id.unwrap_or_else(|| {
            // Fallback: use the first box as anchor
            satellite_ids.first().copied().unwrap_or(0)
        });

        instances.push(IdiomInstance {
            kind: IdiomInstanceKind::DiffPair,
            anchor_box_id: anchor,
            satellite_box_ids: satellite_ids,
            anchor_pin_id: None,
            signal_net_id: Some(net_p.nid),
            power_net_id: None,
            ground_net_id: None,
            confidence: 0.8,
            source: InstanceSource::NetNameHeuristic,
        });
    }

    instances
}

/// Find net pairs that look like differential pairs (P/N, +/-, etc.).
fn find_diff_pairs(
    graph: &McVecGraph,
) -> HashMap<String, (&crate::vector::graph::VizNet, &crate::vector::graph::VizNet)> {
    let mut pairs: HashMap<String, (&crate::vector::graph::VizNet, &crate::vector::graph::VizNet)> =
        HashMap::new();
    let mut seen = Vec::new();

    for net in &graph.nets {
        if let Some((base, is_p)) = diff_pair_base(&net.name) {
            let key = format!("{}:{}", base, if is_p { "P" } else { "N" });
            if seen.contains(&key) {
                continue;
            }
            seen.push(key.clone());

            for other in &graph.nets {
                if other.nid == net.nid {
                    continue;
                }
                if let Some((other_base, other_is_p)) = diff_pair_base(&other.name) {
                    if other_base == base && other_is_p != is_p {
                        let (net_p, net_n) = if is_p { (net, other) } else { (other, net) };
                        pairs.insert(base.to_string(), (net_p, net_n));
                    }
                }
            }
        }
    }

    pairs
}

/// Check if a net name looks like a differential pair member.
/// Returns (base_name, is_p) if recognized.
fn diff_pair_base(name: &str) -> Option<(&str, bool)> {
    if let Some(base) = name.strip_suffix("_P") {
        return Some((base, true));
    }
    if let Some(base) = name.strip_suffix("_N") {
        return Some((base, false));
    }
    if let Some(base) = name.strip_suffix('+') {
        return Some((base, true));
    }
    if let Some(base) = name.strip_suffix('-') {
        return Some((base, false));
    }
    None
}

// ============================================================================
// I7: Pullup resistor detection
// ============================================================================

/// Detect pullup resistors: Resistor with one end on a signal net, one end on Power.
fn detect_pullup(graph: &McVecGraph, connected: &HashMap<i64, Vec<NetKind>>) -> Vec<IdiomMatch> {
    let mut matches = Vec::new();

    for b in &graph.boxes {
        if b.symbol != Symbol::Resistor {
            continue;
        }

        let nk = connected.get(&b.id).cloned().unwrap_or_default();
        let has_power = nk.iter().any(|k| matches!(k, NetKind::Power));
        let has_signal = nk.iter().any(|k| matches!(k, NetKind::Signal));
        if !has_power || !has_signal {
            continue;
        }

        matches.push(IdiomMatch {
            kind: IdiomKind::Pullup,
            member_box_ids: vec![b.id],
            symmetry_penalty: 0.0,
            idiom_violation: false,
        });
    }

    matches
}

/// M11: Pullup detection producing placement-ready instances.
fn detect_pullup_instances(
    graph: &McVecGraph,
    connected: &HashMap<i64, Vec<NetKind>>,
    protected: &HashSet<i64>,
) -> Vec<IdiomInstance> {
    let mut instances = Vec::new();

    for b in &graph.boxes {
        if b.symbol != Symbol::Resistor {
            continue;
        }
        if protected.contains(&b.id) || b.geom_locked {
            continue;
        }

        let nk = connected.get(&b.id).cloned().unwrap_or_default();
        let has_power = nk.iter().any(|k| matches!(k, NetKind::Power));
        let has_signal = nk.iter().any(|k| matches!(k, NetKind::Signal));
        if !has_power || !has_signal {
            continue;
        }

        // Find the signal net and its anchor
        let r_nets = nets_for_box(graph, b.id);
        let signal_net = r_nets.iter().find(|n| matches!(n.kind, NetKind::Signal));
        let power_net = r_nets.iter().find(|n| matches!(n.kind, NetKind::Power));

        let signal_anchor = signal_net.and_then(|sn| find_signal_anchor(graph, b.id, sn.nid));

        if let Some((anchor_box_id, anchor_pin_id)) = signal_anchor {
            instances.push(IdiomInstance {
                kind: IdiomInstanceKind::Pullup,
                anchor_box_id,
                satellite_box_ids: vec![b.id],
                anchor_pin_id: Some(anchor_pin_id),
                signal_net_id: signal_net.map(|n| n.nid),
                power_net_id: power_net.map(|n| n.nid),
                ground_net_id: None,
                confidence: 0.85,
                source: InstanceSource::NetSemantic,
            });
        }
    }

    instances
}

// ============================================================================
// I8: Pulldown resistor detection
// ============================================================================

/// Detect pulldown resistors: Resistor with one end on a signal net, one end on Ground.
fn detect_pulldown(graph: &McVecGraph, connected: &HashMap<i64, Vec<NetKind>>) -> Vec<IdiomMatch> {
    let mut matches = Vec::new();

    for b in &graph.boxes {
        if b.symbol != Symbol::Resistor {
            continue;
        }

        let nk = connected.get(&b.id).cloned().unwrap_or_default();
        let has_ground = nk.iter().any(|k| matches!(k, NetKind::Ground));
        let has_signal = nk.iter().any(|k| matches!(k, NetKind::Signal));
        if !has_ground || !has_signal {
            continue;
        }

        matches.push(IdiomMatch {
            kind: IdiomKind::Pulldown,
            member_box_ids: vec![b.id],
            symmetry_penalty: 0.0,
            idiom_violation: false,
        });
    }

    matches
}

/// M11: Pulldown detection producing placement-ready instances.
fn detect_pulldown_instances(
    graph: &McVecGraph,
    connected: &HashMap<i64, Vec<NetKind>>,
    protected: &HashSet<i64>,
) -> Vec<IdiomInstance> {
    let mut instances = Vec::new();

    for b in &graph.boxes {
        if b.symbol != Symbol::Resistor {
            continue;
        }
        if protected.contains(&b.id) || b.geom_locked {
            continue;
        }

        let nk = connected.get(&b.id).cloned().unwrap_or_default();
        let has_ground = nk.iter().any(|k| matches!(k, NetKind::Ground));
        let has_signal = nk.iter().any(|k| matches!(k, NetKind::Signal));
        if !has_ground || !has_signal {
            continue;
        }

        let r_nets = nets_for_box(graph, b.id);
        let signal_net = r_nets.iter().find(|n| matches!(n.kind, NetKind::Signal));
        let ground_net = r_nets.iter().find(|n| matches!(n.kind, NetKind::Ground));

        let signal_anchor = signal_net.and_then(|sn| find_signal_anchor(graph, b.id, sn.nid));

        if let Some((anchor_box_id, anchor_pin_id)) = signal_anchor {
            instances.push(IdiomInstance {
                kind: IdiomInstanceKind::Pulldown,
                anchor_box_id,
                satellite_box_ids: vec![b.id],
                anchor_pin_id: Some(anchor_pin_id),
                signal_net_id: signal_net.map(|n| n.nid),
                power_net_id: None,
                ground_net_id: ground_net.map(|n| n.nid),
                confidence: 0.85,
                source: InstanceSource::NetSemantic,
            });
        }
    }

    instances
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::IoSummary;
    use crate::vector::graph::{BoxKind, EndpointRef, McVecBox, Symbol, VizNet};

    fn make_box(id: i64, name: &str, symbol: Symbol, x: f64, y: f64, w: f64, h: f64) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::TwoPin,
            symbol,
            None,
            None,
            2,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        b
    }

    fn make_ic_box(id: i64, name: &str, x: f64, y: f64) -> McVecBox {
        let mut b = McVecBox::new_v2(
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
        b
    }

    #[test]
    fn generated_constraints_carry_source_kind_for_reporting() {
        let instances = vec![
            IdiomInstance {
                kind: IdiomInstanceKind::Pullup,
                anchor_box_id: 10,
                satellite_box_ids: vec![20],
                anchor_pin_id: Some(1),
                signal_net_id: Some(100),
                power_net_id: Some(200),
                ground_net_id: None,
                confidence: 0.85,
                source: InstanceSource::NetSemantic,
            },
            IdiomInstance {
                kind: IdiomInstanceKind::Pulldown,
                anchor_box_id: 11,
                satellite_box_ids: vec![21],
                anchor_pin_id: Some(2),
                signal_net_id: Some(101),
                power_net_id: None,
                ground_net_id: Some(201),
                confidence: 0.85,
                source: InstanceSource::NetSemantic,
            },
        ];

        let constraints = generate_constraints(&instances);
        assert_eq!(constraints.len(), 2);
        assert_eq!(constraints[0].source_kind, IdiomInstanceKind::Pullup);
        assert_eq!(constraints[1].source_kind, IdiomInstanceKind::Pulldown);
    }

    #[test]
    fn detect_decoupling_cap() {
        let mut graph = McVecGraph::new(1, "test".into());

        let ic = make_ic_box(1, "U1", 50.0, 50.0);
        let cap = make_box(2, "C1", Symbol::Capacitor, 50.0, 200.0, 40.0, 30.0);

        let net_power = VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "VDD"), EndpointRef::new(2, 2, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(2, 2, "2"), EndpointRef::new(1, 1, "GND")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(cap);
        graph.nets.push(net_power);
        graph.nets.push(net_gnd);

        let matches = analyze(&graph);
        let decaps: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Decoupling)
            .collect();
        assert!(
            !decaps.is_empty(),
            "Should detect decoupling cap. Matches: {:?}",
            matches
        );
        assert!(decaps[0].member_box_ids.contains(&2));
    }

    #[test]
    fn decoupling_penalty_grows_with_distance() {
        let mut graph = McVecGraph::new(1, "test".into());

        let ic = make_ic_box(1, "U1", 50.0, 50.0);
        let cap = make_box(2, "C1", Symbol::Capacitor, 500.0, 500.0, 40.0, 30.0);

        let net_power = VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "VDD"), EndpointRef::new(2, 2, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(2, 2, "2"), EndpointRef::new(1, 1, "GND")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(cap);
        graph.nets.push(net_power);
        graph.nets.push(net_gnd);

        let matches = analyze(&graph);
        let decaps: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Decoupling)
            .collect();
        assert!(!decaps.is_empty());
        assert!(decaps[0].idiom_violation, "Far cap should be a violation");
    }

    #[test]
    fn detect_diff_pair_pn() {
        let mut graph = McVecGraph::new(1, "test".into());

        let b1 = make_box(1, "R1", Symbol::Resistor, 50.0, 50.0, 40.0, 30.0);
        let b2 = make_box(2, "R2", Symbol::Resistor, 50.0, 100.0, 40.0, 30.0);

        let net_p = VizNet::new(
            1,
            "DIO_MIC_P".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1")],
        );
        let net_n = VizNet::new(
            2,
            "DIO_MIC_N".into(),
            NetKind::Signal,
            vec![EndpointRef::new(2, 2, "1")],
        );

        graph.boxes.push(b1);
        graph.boxes.push(b2);
        graph.nets.push(net_p);
        graph.nets.push(net_n);

        let matches = analyze(&graph);
        let diff_pairs: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::DiffPair)
            .collect();
        assert!(
            !diff_pairs.is_empty(),
            "Should detect diff pair. Matches: {:?}",
            matches
        );
        assert!(diff_pairs[0].member_box_ids.contains(&1));
        assert!(diff_pairs[0].member_box_ids.contains(&2));
    }

    #[test]
    fn diffpair_symmetric_zero_penalty() {
        let mut graph = McVecGraph::new(1, "test".into());

        let b1 = make_box(1, "R1", Symbol::Resistor, 50.0, 100.0, 40.0, 30.0);
        let b2 = make_box(2, "R2", Symbol::Resistor, 150.0, 100.0, 40.0, 30.0);

        let net_p = VizNet::new(
            1,
            "SIG_P".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1")],
        );
        let net_n = VizNet::new(
            2,
            "SIG_N".into(),
            NetKind::Signal,
            vec![EndpointRef::new(2, 2, "1")],
        );

        graph.boxes.push(b1);
        graph.boxes.push(b2);
        graph.nets.push(net_p);
        graph.nets.push(net_n);

        let matches = analyze(&graph);
        let diff_pairs: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::DiffPair)
            .collect();
        assert!(!diff_pairs.is_empty());
        assert!(
            diff_pairs[0].symmetry_penalty <= 1.0,
            "Symmetric placement should have near-zero penalty, got {}",
            diff_pairs[0].symmetry_penalty
        );
    }

    #[test]
    fn detect_pullup() {
        let mut graph = McVecGraph::new(1, "test".into());

        let r = make_box(1, "R1", Symbol::Resistor, 50.0, 50.0, 40.0, 30.0);

        let net_sig = VizNet::new(
            1,
            "SIGNAL".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1")],
        );
        let net_pwr = VizNet::new(
            2,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "2")],
        );

        graph.boxes.push(r);
        graph.nets.push(net_sig);
        graph.nets.push(net_pwr);

        let matches = analyze(&graph);
        let pullups: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Pullup)
            .collect();
        assert!(
            !pullups.is_empty(),
            "Should detect pullup. Matches: {:?}",
            matches
        );
        assert!(pullups[0].member_box_ids.contains(&1));
    }

    #[test]
    fn detect_pulldown() {
        let mut graph = McVecGraph::new(1, "test".into());

        let r = make_box(1, "R1", Symbol::Resistor, 50.0, 50.0, 40.0, 30.0);

        let net_sig = VizNet::new(
            1,
            "SIGNAL".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(1, 1, "2")],
        );

        graph.boxes.push(r);
        graph.nets.push(net_sig);
        graph.nets.push(net_gnd);

        let matches = analyze(&graph);
        let pulldowns: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Pulldown)
            .collect();
        assert!(
            !pulldowns.is_empty(),
            "Should detect pulldown. Matches: {:?}",
            matches
        );
        assert!(pulldowns[0].member_box_ids.contains(&1));
    }

    #[test]
    fn pulldown_not_confused_with_pullup() {
        let mut graph = McVecGraph::new(1, "test".into());

        let r = make_box(1, "R1", Symbol::Resistor, 50.0, 50.0, 40.0, 30.0);

        let net_sig = VizNet::new(
            1,
            "SIGNAL".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(1, 1, "2")],
        );

        graph.boxes.push(r);
        graph.nets.push(net_sig);
        graph.nets.push(net_gnd);

        let matches = analyze(&graph);
        // Should be pulldown, not pullup
        let pullups: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Pullup)
            .collect();
        assert!(pullups.is_empty(), "Signal+Ground should NOT be pullup");
        let pulldowns: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Pulldown)
            .collect();
        assert!(!pulldowns.is_empty(), "Signal+Ground should be pulldown");
    }

    #[test]
    fn placement_instances_decoupling() {
        let mut graph = McVecGraph::new(1, "test".into());

        let ic = make_ic_box(1, "U1", 50.0, 50.0);
        let cap = make_box(2, "C1", Symbol::Capacitor, 50.0, 200.0, 40.0, 30.0);

        let net_power = VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "VDD"), EndpointRef::new(2, 2, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(2, 2, "2"), EndpointRef::new(1, 1, "GND")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(cap);
        graph.nets.push(net_power);
        graph.nets.push(net_gnd);

        let protected = HashSet::new();
        let instances = detect_placement_instances(&graph, &protected);
        let decaps: Vec<_> = instances
            .iter()
            .filter(|i| i.kind == IdiomInstanceKind::Decoupling)
            .collect();
        assert!(!decaps.is_empty(), "Should detect decoupling instance");
        assert_eq!(decaps[0].anchor_box_id, 1);
        assert!(decaps[0].satellite_box_ids.contains(&2));
        assert!(decaps[0].confidence > 0.5);
    }

    #[test]
    fn protected_boxes_skipped() {
        let mut graph = McVecGraph::new(1, "test".into());

        let ic = make_ic_box(1, "U1", 50.0, 50.0);
        let cap = make_box(2, "C1", Symbol::Capacitor, 50.0, 200.0, 40.0, 30.0);

        let net_power = VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "VDD"), EndpointRef::new(2, 2, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(2, 2, "2"), EndpointRef::new(1, 1, "GND")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(cap);
        graph.nets.push(net_power);
        graph.nets.push(net_gnd);

        let mut protected = HashSet::new();
        protected.insert(2); // Protect the cap
        let instances = detect_placement_instances(&graph, &protected);
        let decaps: Vec<_> = instances
            .iter()
            .filter(|i| i.kind == IdiomInstanceKind::Decoupling)
            .collect();
        assert!(
            decaps.is_empty(),
            "Protected cap should not generate placement instance"
        );
    }

    #[test]
    fn analyze_deterministic() {
        let mut graph = McVecGraph::new(1, "test".into());

        let ic = make_ic_box(1, "U1", 50.0, 50.0);
        let cap = make_box(2, "C1", Symbol::Capacitor, 50.0, 150.0, 40.0, 30.0);
        let r = make_box(3, "R1", Symbol::Resistor, 200.0, 50.0, 40.0, 30.0);

        let net_pwr = VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![
                EndpointRef::new(1, 1, "VDD"),
                EndpointRef::new(2, 2, "1"),
                EndpointRef::new(3, 3, "1"),
            ],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(2, 2, "2")],
        );
        let net_sig = VizNet::new(
            3,
            "SIG".into(),
            NetKind::Signal,
            vec![EndpointRef::new(3, 3, "2")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(cap);
        graph.boxes.push(r);
        graph.nets.push(net_pwr);
        graph.nets.push(net_gnd);
        graph.nets.push(net_sig);

        let a = analyze(&graph);
        let b = analyze(&graph);
        assert_eq!(a.len(), b.len(), "Deterministic: same input → same count");
        for (m1, m2) in a.iter().zip(b.iter()) {
            assert_eq!(m1.kind, m2.kind);
            assert_eq!(m1.member_box_ids, m2.member_box_ids);
            assert_eq!(m1.idiom_violation, m2.idiom_violation);
        }
    }

    #[test]
    fn placement_instances_deterministic() {
        let mut graph = McVecGraph::new(1, "test".into());

        let ic = make_ic_box(1, "U1", 50.0, 50.0);
        let cap = make_box(2, "C1", Symbol::Capacitor, 50.0, 150.0, 40.0, 30.0);

        let net_pwr = VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "VDD"), EndpointRef::new(2, 2, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(2, 2, "2")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(cap);
        graph.nets.push(net_pwr);
        graph.nets.push(net_gnd);

        let protected = HashSet::new();
        let a = detect_placement_instances(&graph, &protected);
        let b = detect_placement_instances(&graph, &protected);
        assert_eq!(a.len(), b.len());
        for (i1, i2) in a.iter().zip(b.iter()) {
            assert_eq!(i1.kind, i2.kind);
            assert_eq!(i1.anchor_box_id, i2.anchor_box_id);
            assert_eq!(i1.satellite_box_ids, i2.satellite_box_ids);
        }
    }

    #[test]
    fn layout_hint_pins_not_penalized() {
        let mut graph = McVecGraph::new(1, "test".into());

        let mut cap = make_box(2, "C1", Symbol::Capacitor, 50.0, 200.0, 40.0, 30.0);
        cap.layout_hint = Some(crate::vector::graph::box_def::PinLayout {
            left: vec!["1".into()],
            right: vec!["2".into()],
            top: vec![],
            bottom: vec![],
        });

        let ic = make_ic_box(1, "U1", 50.0, 50.0);

        let net_pwr = VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![EndpointRef::new(1, 1, "VDD"), EndpointRef::new(2, 2, "1")],
        );
        let net_gnd = VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![EndpointRef::new(2, 2, "2"), EndpointRef::new(1, 1, "GND")],
        );

        graph.boxes.push(ic);
        graph.boxes.push(cap);
        graph.nets.push(net_pwr);
        graph.nets.push(net_gnd);

        let matches = analyze(&graph);
        let decaps: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Decoupling)
            .collect();
        assert!(!decaps.is_empty(), "Layout hint should not block detection");
    }
}
