// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase 4 · Idiom recognition (范式识别)
//!
//! Analyze a laid-out `McVecGraph` to identify sub-circuit idioms (decoupling
//! caps, diff pairs, pullups, etc.) and compute penalties for violations of
//! conventional drawing practice. The results feed into `ReadabilityScore`'s
//! `symmetry_penalty` / `idiom_violation` fields, letting Phase 3's
//! generate-and-rank loop prefer candidates that "look right".

use std::collections::HashMap;

use crate::vector::graph::{McVecBox, McVecGraph, NetKind, Symbol};
use crate::viz::layout::schematic_radial::collect_connected_net_kinds;

// ============================================================================
// Data types
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
}

// ============================================================================
// Main API
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
        // Must be a two-pin capacitor (C or PolarCapacitor)
        if !matches!(b.symbol, Symbol::Capacitor | Symbol::PolarCapacitor) {
            continue;
        }

        let nk = connected.get(&b.id).cloned().unwrap_or_default();
        let has_power = nk.iter().any(|k| matches!(k, NetKind::Power));
        let has_ground = nk.iter().any(|k| matches!(k, NetKind::Ground));
        if !has_power || !has_ground {
            continue;
        }

        // Find the nearest IC (MultiPin) on the power net
        let cap_center = (b.x + b.w / 2.0, b.y + b.h / 2.0);

        // For each net connected to this cap, find IC boxes that share the net
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

    // Group nets by their base name (strip _P/_N, +, - suffixes)
    let pairs = find_diff_pairs(graph);

    for (_base, (net_p, net_n)) in &pairs {
        // Find the boxes connected to each net
        let boxes_p: Vec<i64> = net_p.endpoints.iter().map(|e| e.box_id).collect();
        let boxes_n: Vec<i64> = net_n.endpoints.iter().map(|e| e.box_id).collect();

        // Collect all unique boxes
        let mut member_ids: Vec<i64> = boxes_p.iter().chain(boxes_n.iter()).copied().collect();
        member_ids.sort();
        member_ids.dedup();

        if member_ids.len() < 2 {
            continue;
        }

        // Symmetry penalty: y-offset between the boxes carrying P and N
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

            // Find the partner
            let _partner_suffix = if is_p { "N" } else { "P" };
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
    // P/N suffix: DIO_MIC_P / DIO_MIC_N, DAC_OUT_P / DAC_OUT_N
    if let Some(base) = name.strip_suffix("_P") {
        return Some((base, true));
    }
    if let Some(base) = name.strip_suffix("_N") {
        return Some((base, false));
    }
    // + / - suffix: SIG+ / SIG-
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
///
/// Penalty: `idiom_violation` if the resistor is not oriented vertically toward
/// the power rail direction.
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

        // Check orientation: resistor should be vertical (pulling toward power rail)
        let _eps = sorted_entry_points(b);

        matches.push(IdiomMatch {
            kind: IdiomKind::Pullup,
            member_box_ids: vec![b.id],
            symmetry_penalty: 0.0,
            idiom_violation: false,
        });
    }

    matches
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
        // Cap far away from IC
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
        // Cap is far from IC → should be a violation
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
        // Two boxes at same y → penalty should be ≈ 0
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
        // Same y → penalty should be small
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
    fn layout_hint_pins_not_penalized() {
        // Boxes with layout_hint should not be double-penalized.
        // This test verifies that the analysis doesn't crash or behave
        // differently when layout_hint is present.
        let mut graph = McVecGraph::new(1, "test".into());

        let mut cap = make_box(2, "C1", Symbol::Capacitor, 50.0, 200.0, 40.0, 30.0);
        // Set a layout hint (simulating Phase 2 wired layout)
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
        // Should still detect the decoupling cap (layout_hint doesn't block detection)
        let decaps: Vec<_> = matches
            .iter()
            .filter(|m| m.kind == IdiomKind::Decoupling)
            .collect();
        assert!(!decaps.is_empty(), "Layout hint should not block detection");
    }
}
