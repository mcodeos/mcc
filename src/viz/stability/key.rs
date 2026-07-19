// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M12 — Stable keys for deterministic ordering
//!
//! Every key type implements `Ord` so that Vec can be sorted deterministically.
//! Keys are designed to be stable across repeated runs of the same input.

use crate::vector::graph::{EndpointRef, McVecBox, NetKind, Symbol, VizNet};

// ============================================================================
// StableBoxKey
// ============================================================================

/// Stable sort key for boxes.
///
/// Levels: box_id → source_order → name → symbol_rank
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StableBoxKey {
    pub box_id: i64,
    pub source_order: usize,
    pub name: String,
    pub symbol_rank: u8,
}

impl StableBoxKey {
    pub fn from_box(b: &McVecBox, source_order: usize) -> Self {
        Self {
            box_id: b.id,
            source_order,
            name: b.name.clone(),
            symbol_rank: symbol_rank(&b.symbol),
        }
    }

    pub fn from_graph(graph: &crate::vector::graph::McVecGraph, box_id: i64) -> Option<Self> {
        graph
            .boxes
            .iter()
            .enumerate()
            .find(|(_, b)| b.id == box_id)
            .map(|(i, b)| Self::from_box(b, i))
    }
}

fn symbol_rank(s: &Symbol) -> u8 {
    match s {
        Symbol::Ic => 0,
        Symbol::Module => 1,
        Symbol::Capacitor | Symbol::PolarCapacitor => 2,
        Symbol::Resistor => 3,
        Symbol::PowerRail { .. } => 4,
        Symbol::Dot => 5,
        Symbol::Unknown => 6,
        _ => 7,
    }
}

// ============================================================================
// StableNetKey
// ============================================================================

/// Stable sort key for nets.
///
/// Levels: net_id → source_order → kind_rank → name
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StableNetKey {
    pub net_id: i64,
    pub source_order: usize,
    pub kind_rank: u8,
    pub name: String,
}

impl StableNetKey {
    pub fn from_net(net: &VizNet, source_order: usize) -> Self {
        Self {
            net_id: net.nid,
            source_order,
            kind_rank: net_kind_rank(&net.kind),
            name: net.name.clone(),
        }
    }

    pub fn from_graph(graph: &crate::vector::graph::McVecGraph, net_id: i64) -> Option<Self> {
        graph
            .nets
            .iter()
            .enumerate()
            .find(|(_, n)| n.nid == net_id)
            .map(|(i, n)| Self::from_net(n, i))
    }
}

fn net_kind_rank(kind: &NetKind) -> u8 {
    match kind {
        NetKind::Power => 0,
        NetKind::Ground => 1,
        NetKind::SubModuleIO => 2,
        NetKind::Signal => 3,
        NetKind::Bus(_) => 4,
    }
}

// ============================================================================
// StablePinKey
// ============================================================================

/// Stable sort key for pins.
///
/// Levels: box_id → pin_id → authored_index → pin_name
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StablePinKey {
    pub box_id: i64,
    pub pin_id: i64,
    pub authored_index: usize,
    pub pin_name: String,
}

impl StablePinKey {
    pub fn new(box_id: i64, pin_id: i64, authored_index: usize, pin_name: String) -> Self {
        Self {
            box_id,
            pin_id,
            authored_index,
            pin_name,
        }
    }
}

// ============================================================================
// StableEndpointKey
// ============================================================================

/// Stable sort key for endpoints.
///
/// Levels: net_id → box_id → pin_id → endpoint_index → pin_name
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StableEndpointKey {
    pub net_id: i64,
    pub box_id: i64,
    pub pin_id: i64,
    pub endpoint_index: usize,
    pub pin_name: String,
}

impl StableEndpointKey {
    pub fn from_endpoint(net_id: i64, ep: &EndpointRef, endpoint_index: usize) -> Self {
        Self {
            net_id,
            box_id: ep.box_id,
            pin_id: ep.pin_id,
            endpoint_index,
            pin_name: ep.pin_name.clone(),
        }
    }
}

// ============================================================================
// StableDecisionKey
// ============================================================================

/// Stable tie-break key for any candidate decision.
///
/// Used when scores are equal to pick a deterministic winner.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct StableDecisionKey {
    pub phase_rank: u8,
    pub decision_kind_rank: u8,
    pub priority: i32,
    pub target_box_id: i64,
    pub anchor_box_id: i64,
    pub net_id: i64,
    pub pin_id: i64,
    pub candidate_index: usize,
}

impl StableDecisionKey {
    pub fn new(
        phase_rank: u8,
        decision_kind_rank: u8,
        priority: i32,
        target_box_id: i64,
        anchor_box_id: i64,
        net_id: i64,
        pin_id: i64,
        candidate_index: usize,
    ) -> Self {
        Self {
            phase_rank,
            decision_kind_rank,
            priority,
            target_box_id,
            anchor_box_id,
            net_id,
            pin_id,
            candidate_index,
        }
    }
}

// ============================================================================
// Convenience sort helpers
// ============================================================================

/// Sort boxes by StableBoxKey.
pub fn sort_boxes_stable(boxes: &mut [McVecBox]) {
    boxes.sort_by_key(|b| StableBoxKey::from_box(b, 0));
}

/// Sort nets by StableNetKey.
pub fn sort_nets_stable(nets: &mut [VizNet]) {
    nets.sort_by_key(|n| StableNetKey::from_net(n, 0));
}

/// Sort endpoints by StableEndpointKey within a net.
pub fn sort_endpoints_stable(net_id: i64, eps: &mut [EndpointRef]) {
    eps.sort_by_key(|ep| StableEndpointKey::from_endpoint(net_id, ep, 0));
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::{BoxKind, EndpointRef, McVecBox, NetKind, Symbol, VizNet};

    fn make_box(id: i64, name: &str, symbol: Symbol) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::TwoPin,
            symbol,
            None,
            None,
            2,
            IoSummary::new(),
        )
    }

    #[test]
    fn stable_box_key_orders_by_id() {
        let b1 = make_box(1, "B", Symbol::Resistor);
        let b2 = make_box(2, "A", Symbol::Capacitor);
        let k1 = StableBoxKey::from_box(&b1, 0);
        let k2 = StableBoxKey::from_box(&b2, 0);
        assert!(k1 < k2, "box_id should be primary sort key");
    }

    #[test]
    fn stable_box_key_same_id_uses_name() {
        let b1 = make_box(1, "A", Symbol::Resistor);
        let b2 = make_box(1, "B", Symbol::Resistor);
        let k1 = StableBoxKey::from_box(&b1, 0);
        let k2 = StableBoxKey::from_box(&b2, 0);
        assert!(k1 < k2, "name should be fallback when id equal");
    }

    #[test]
    fn stable_net_key_orders_by_id() {
        let n1 = VizNet::new(1, "B".into(), NetKind::Signal, vec![]);
        let n2 = VizNet::new(2, "A".into(), NetKind::Power, vec![]);
        let k1 = StableNetKey::from_net(&n1, 0);
        let k2 = StableNetKey::from_net(&n2, 0);
        assert!(k1 < k2);
    }

    #[test]
    fn stable_net_key_kind_rank() {
        let n1 = VizNet::new(1, "VDD".into(), NetKind::Power, vec![]);
        let n2 = VizNet::new(2, "SIG".into(), NetKind::Signal, vec![]);
        let k1 = StableNetKey::from_net(&n1, 0);
        let k2 = StableNetKey::from_net(&n2, 0);
        assert!(k1 < k2);
    }

    #[test]
    fn stable_endpoint_key_ordering() {
        let ep1 = EndpointRef::new(1, 1, "A");
        let ep2 = EndpointRef::new(1, 2, "B");
        let k1 = StableEndpointKey::from_endpoint(1, &ep1, 0);
        let k2 = StableEndpointKey::from_endpoint(1, &ep2, 0);
        assert!(k1 < k2);
    }

    #[test]
    fn sort_boxes_stable_is_deterministic() {
        let mut boxes = vec![
            make_box(3, "C", Symbol::Ic),
            make_box(1, "A", Symbol::Resistor),
            make_box(2, "B", Symbol::Capacitor),
        ];
        sort_boxes_stable(&mut boxes);
        assert_eq!(boxes[0].id, 1);
        assert_eq!(boxes[1].id, 2);
        assert_eq!(boxes[2].id, 3);
    }

    #[test]
    fn stable_decision_key_ordering() {
        let k1 = StableDecisionKey::new(0, 0, 10, 1, 2, 3, 4, 0);
        let k2 = StableDecisionKey::new(0, 0, 10, 1, 2, 3, 4, 1);
        assert!(k1 < k2, "candidate_index should be tie-break");
    }
}
