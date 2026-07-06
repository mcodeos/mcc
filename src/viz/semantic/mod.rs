// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Milestone 5 — Semantic Analyzer：电路语义分析层
//!
//! A read-only derived analysis layer over [`McVecGraph`]. It answers:
//! - Which nets are power / ground / signal / bus?
//! - Which endpoints are input / output / passive / power / ground?
//! - Which boxes are hubs?
//! - Which signal chains are present?
//! - Which idioms are recognized?
//!
//! It does NOT:
//! - Modify nets, endpoints, or box coordinates.
//! - Route or render.
//! - Replace Pass2 net truth.
//!
//! ## Usage
//! ```ignore
//! let semantic = SemanticModel::analyze(&graph);
//! for line in semantic.report_lines() {
//!     println!("{line}");
//! }
//! ```

use std::collections::{BTreeMap, HashSet};

use crate::vector::graph::naming;
use crate::vector::graph::net_def::{IoDirection, NetTopology};
use crate::vector::graph::{BoxKind, EntrySide, McVecBox, McVecGraph, NetKind, Symbol};
use crate::viz::idiom::{self, IdiomMatch};
use crate::viz::layout::chain::{self, ChainDir};
use crate::viz::layout::rails;

// ============================================================================
// PinKey
// ============================================================================

/// Composite key for a pin: (box_id, pin_id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PinKey {
    pub box_id: i64,
    pub pin_id: i64,
}

impl PinKey {
    pub fn new(box_id: i64, pin_id: i64) -> Self {
        Self { box_id, pin_id }
    }
}

// ============================================================================
// BoxRole
// ============================================================================

/// Semantic role of a box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxRole {
    Hub,
    Connector,
    Passive,
    PowerFlag,
    GroundFlag,
    ModuleBoundary,
    JunctionDot,
    Unknown,
}

// ============================================================================
// BoxSemantic
// ============================================================================

/// Semantic analysis of a single box.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxSemantic {
    pub box_id: i64,
    pub name: String,
    pub kind: BoxKind,
    pub symbol: Symbol,
    pub role: BoxRole,
    pub is_hub_candidate: bool,
    pub hub_score: usize,
    pub group_ids: Vec<usize>,
}

// ============================================================================
// NetRole
// ============================================================================

/// Semantic role of a net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetRole {
    Power,
    Ground,
    BusMember,
    SignalTrunk,
    SignalLeaf,
    SignalPointToPoint,
    ModuleIo,
    InternalOrIsolated,
}

// ============================================================================
// NetSemantic
// ============================================================================

/// Semantic analysis of a single net.
#[derive(Debug, Clone, PartialEq)]
pub struct NetSemantic {
    pub net_id: i64,
    pub name: String,
    pub kind: NetKind,
    pub topology: NetTopology,
    pub endpoint_count: usize,
    pub driver_count: usize,
    pub role: NetRole,
    pub bus_group: Option<usize>,
    pub rail_intent: Option<usize>,
}

// ============================================================================
// PinSideReason
// ============================================================================

/// Reason behind a pin's preferred side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinSideReason {
    PowerTop,
    GroundBottom,
    InputLeft,
    OutputRight,
    PassiveByNeighbor,
    BusOrder,
    ConnectorOrder,
    Unknown,
}

// ============================================================================
// PinSemantic
// ============================================================================

/// Semantic analysis of a single pin/endpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct PinSemantic {
    pub key: PinKey,
    pub pin_name: String,
    pub io_direction: IoDirection,
    pub connected_net_ids: Vec<i64>,
    pub preferred_side: Option<EntrySide>,
    pub preferred_side_reason: PinSideReason,
    pub actual_side: Option<EntrySide>,
    pub is_synthetic: bool,
}

// ============================================================================
// ChainNodeSemantic
// ============================================================================

/// A node in a signal chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainNodeSemantic {
    pub box_id: i64,
    pub net_id: i64,
}

// ============================================================================
// SignalChainSemantic
// ============================================================================

/// A signal chain extracted from the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct SignalChainSemantic {
    pub hub_id: i64,
    pub hub_pin: i64,
    pub hub_pin_name: String,
    pub direction_hint: Option<EntrySide>,
    pub nodes: Vec<ChainNodeSemantic>,
    pub terminus_box_id: Option<i64>,
    pub loops_to_hub: bool,
}

// ============================================================================
// PassiveChainSemantic
// ============================================================================

/// A passive chain (skeleton — to be filled in a future milestone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassiveChainSemantic {
    pub chain_id: usize,
    pub net_ids: Vec<i64>,
    pub box_ids: Vec<i64>,
    pub endpoints: Vec<PinKey>,
}

// ============================================================================
// ComponentGroupKind
// ============================================================================

/// Kind of component group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentGroupKind {
    HubLocalCluster,
    SignalChain,
    PassiveChain,
    PowerDecoupling,
    DifferentialPair,
    PullupNetwork,
    BusCluster,
}

// ============================================================================
// ComponentGroup
// ============================================================================

/// A group of related components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentGroup {
    pub group_id: usize,
    pub kind: ComponentGroupKind,
    pub member_box_ids: Vec<i64>,
    pub anchor_box_id: Option<i64>,
}

// ============================================================================
// BusGroup
// ============================================================================

/// A bus group (skeleton — to be filled in a future milestone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusGroup {
    pub group_id: usize,
    pub base_name: String,
    pub width: usize,
    pub member_net_ids: Vec<i64>,
    pub bit_order: Vec<(usize, i64)>,
}

// ============================================================================
// RailRole
// ============================================================================

/// Role of a rail intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RailRole {
    LocalFlag,
    SharedRail,
    Stub,
    SyntheticFlag,
}

// ============================================================================
// RailIntent
// ============================================================================

/// A power/ground rail intent (skeleton — to be filled in a future milestone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RailIntent {
    pub intent_id: usize,
    pub net_id: i64,
    pub name: String,
    pub is_ground: bool,
    pub role: RailRole,
    pub endpoint_pins: Vec<PinKey>,
}

// ============================================================================
// SemanticWarning
// ============================================================================

/// A warning produced during semantic analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticWarning {
    pub message: String,
}

impl SemanticWarning {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

// ============================================================================
// SemanticSummary
// ============================================================================

/// Summary statistics from semantic analysis.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SemanticSummary {
    pub boxes_total: usize,
    pub nets_total: usize,
    pub pins_total: usize,
    pub hubs_detected: usize,
    pub signal_chains_detected: usize,
    pub passive_chains_detected: usize,
    pub component_groups_detected: usize,
    pub bus_groups_detected: usize,
    pub rail_intents_detected: usize,
    pub idioms_detected: usize,
    pub pins_with_preferred_side: usize,
    pub pins_with_actual_side: usize,
    pub preferred_actual_side_mismatches: usize,
    pub warnings: usize,
}

impl SemanticSummary {
    /// Merge another summary into self (accumulate for multi-layer).
    pub fn merge(&mut self, other: &SemanticSummary) {
        self.boxes_total += other.boxes_total;
        self.nets_total += other.nets_total;
        self.pins_total += other.pins_total;
        self.hubs_detected += other.hubs_detected;
        self.signal_chains_detected += other.signal_chains_detected;
        self.passive_chains_detected += other.passive_chains_detected;
        self.component_groups_detected += other.component_groups_detected;
        self.bus_groups_detected += other.bus_groups_detected;
        self.rail_intents_detected += other.rail_intents_detected;
        self.idioms_detected += other.idioms_detected;
        self.pins_with_preferred_side += other.pins_with_preferred_side;
        self.pins_with_actual_side += other.pins_with_actual_side;
        self.preferred_actual_side_mismatches += other.preferred_actual_side_mismatches;
        self.warnings += other.warnings;
    }
}

// ============================================================================
// SemanticModel
// ============================================================================

/// The semantic analysis model — a read-only derived analysis over [`McVecGraph`].
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticModel {
    pub summary: SemanticSummary,
    pub boxes: BTreeMap<i64, BoxSemantic>,
    pub nets: BTreeMap<i64, NetSemantic>,
    pub pins: BTreeMap<PinKey, PinSemantic>,
    pub signal_chains: Vec<SignalChainSemantic>,
    pub passive_chains: Vec<PassiveChainSemantic>,
    pub component_groups: Vec<ComponentGroup>,
    pub bus_groups: Vec<BusGroup>,
    pub rail_intents: Vec<RailIntent>,
    pub idioms: Vec<IdiomMatch>,
    pub warnings: Vec<SemanticWarning>,
}

impl SemanticModel {
    // ========================================================================
    // Main entry point
    // ========================================================================

    /// Analyze a [`McVecGraph`] and produce a [`SemanticModel`].
    ///
    /// Read-only: does not modify the graph.
    pub fn analyze(graph: &McVecGraph) -> Self {
        let mut warnings: Vec<SemanticWarning> = Vec::new();

        let boxes = Self::collect_box_semantics(graph, &mut warnings);
        let nets = Self::collect_net_semantics(graph, &mut warnings);
        let (pins, pins_with_preferred_side, pins_with_actual_side, mismatches) =
            Self::collect_pin_semantics(graph, &boxes, &nets, &mut warnings);

        let (hubs_detected, hub_ids) = Self::detect_hubs(graph, &boxes);
        let (signal_chains, signal_chains_detected) =
            Self::detect_signal_chains(graph, &hub_ids, &mut warnings);
        let (passive_chains, passive_chains_detected) =
            Self::detect_passive_chains(graph, &mut warnings);
        let (bus_groups, bus_groups_detected) = Self::detect_bus_groups(graph, &mut warnings);
        let (rail_intents, rail_intents_detected) = Self::detect_rail_intents(graph, &mut warnings);
        let (component_groups, component_groups_detected) = Self::detect_component_groups(
            &signal_chains,
            &passive_chains,
            &bus_groups,
            &rail_intents,
            &mut warnings,
        );
        let idioms = idiom::analyze(graph);
        let idioms_detected = idioms.len();

        Self::validate_semantic_model(graph, &boxes, &nets, &pins, &signal_chains, &mut warnings);

        let summary = SemanticSummary {
            boxes_total: boxes.len(),
            nets_total: nets.len(),
            pins_total: pins.len(),
            hubs_detected,
            signal_chains_detected,
            passive_chains_detected,
            component_groups_detected,
            bus_groups_detected,
            rail_intents_detected,
            idioms_detected,
            pins_with_preferred_side,
            pins_with_actual_side,
            preferred_actual_side_mismatches: mismatches,
            warnings: warnings.len(),
        };

        SemanticModel {
            summary,
            boxes,
            nets,
            pins,
            signal_chains,
            passive_chains,
            component_groups,
            bus_groups,
            rail_intents,
            idioms,
            warnings,
        }
    }

    // ========================================================================
    // Phase 1: collect_box_semantics
    // ========================================================================

    fn collect_box_semantics(
        graph: &McVecGraph,
        _warnings: &mut Vec<SemanticWarning>,
    ) -> BTreeMap<i64, BoxSemantic> {
        let mut map = BTreeMap::new();

        for b in &graph.boxes {
            let role = Self::classify_box_role(b);
            let is_hub = b.id >= 0
                && matches!(b.kind, BoxKind::MultiPin | BoxKind::SubModule)
                && b.pin_count >= 3;
            let hub_score = if b.kind == BoxKind::MultiPin {
                10000 + b.pin_count.max(b.entry_points.len())
            } else {
                b.pin_count.max(b.entry_points.len())
            };

            map.insert(
                b.id,
                BoxSemantic {
                    box_id: b.id,
                    name: b.name.clone(),
                    kind: b.kind.clone(),
                    symbol: b.symbol.clone(),
                    role,
                    is_hub_candidate: is_hub,
                    hub_score,
                    group_ids: Vec::new(),
                },
            );
        }

        map
    }

    fn classify_box_role(b: &McVecBox) -> BoxRole {
        // 1. Junction dot
        if b.kind == BoxKind::Dot {
            return BoxRole::JunctionDot;
        }
        // 2. Rail box (ground vs power via rails module + naming/symbol)
        if rails::is_rail_box(b) {
            if naming::is_ground(&b.name) || b.symbol.is_ground() {
                return BoxRole::GroundFlag;
            }
            return BoxRole::PowerFlag;
        }
        // 3. Module boundary
        if b.kind == BoxKind::SubModule {
            return BoxRole::ModuleBoundary;
        }
        // 4. Power/ground flag (non-rail)
        if b.kind == BoxKind::PowerLabel {
            if naming::is_ground(&b.name) || b.symbol.is_ground() {
                return BoxRole::GroundFlag;
            }
            if b.symbol.is_power_rail() || naming::is_power(&b.name) {
                return BoxRole::PowerFlag;
            }
        }
        // 5. Two-pin passive
        if b.symbol.is_two_pin_passive() {
            return BoxRole::Passive;
        }
        // 6. Hub candidate (MultiPin with enough pins)
        if b.kind == BoxKind::MultiPin && b.pin_count >= 3 {
            return BoxRole::Hub;
        }
        // 7. Connector (MultiPin with few pins)
        if b.kind == BoxKind::MultiPin {
            return BoxRole::Connector;
        }
        BoxRole::Unknown
    }

    // ========================================================================
    // Phase 2: collect_net_semantics
    // ========================================================================

    fn collect_net_semantics(
        graph: &McVecGraph,
        _warnings: &mut Vec<SemanticWarning>,
    ) -> BTreeMap<i64, NetSemantic> {
        let mut map = BTreeMap::new();

        for net in &graph.nets {
            let topology = net.topology();
            let endpoint_count = net.endpoints.len();
            let driver_count = net
                .endpoints
                .iter()
                .filter(|e| matches!(e.io_type, IoDirection::Output | IoDirection::Bidir))
                .count();

            let role = Self::classify_net_role(&net.kind, endpoint_count, driver_count);

            map.insert(
                net.nid,
                NetSemantic {
                    net_id: net.nid,
                    name: net.name.clone(),
                    kind: net.kind.clone(),
                    topology,
                    endpoint_count,
                    driver_count,
                    role,
                    bus_group: None,
                    rail_intent: None,
                },
            );
        }

        map
    }

    fn classify_net_role(kind: &NetKind, endpoint_count: usize, driver_count: usize) -> NetRole {
        match kind {
            NetKind::Power => NetRole::Power,
            NetKind::Ground => NetRole::Ground,
            NetKind::Bus(_) => NetRole::BusMember,
            NetKind::SubModuleIO => NetRole::ModuleIo,
            NetKind::Signal => {
                if endpoint_count <= 1 {
                    NetRole::InternalOrIsolated
                } else if endpoint_count == 2 {
                    NetRole::SignalPointToPoint
                } else if driver_count <= 1 {
                    NetRole::SignalTrunk
                } else {
                    NetRole::SignalLeaf
                }
            }
        }
    }

    // ========================================================================
    // Phase 3: collect_pin_semantics
    // ========================================================================

    fn collect_pin_semantics(
        graph: &McVecGraph,
        _boxes: &BTreeMap<i64, BoxSemantic>,
        _nets: &BTreeMap<i64, NetSemantic>,
        _warnings: &mut Vec<SemanticWarning>,
    ) -> (BTreeMap<PinKey, PinSemantic>, usize, usize, usize) {
        // First pass: collect all pin keys and their connected net ids
        let mut pin_net_ids: BTreeMap<PinKey, Vec<i64>> = BTreeMap::new();
        let mut pin_names: BTreeMap<PinKey, String> = BTreeMap::new();
        let mut pin_ios: BTreeMap<PinKey, IoDirection> = BTreeMap::new();

        for net in &graph.nets {
            for ep in &net.endpoints {
                let key = PinKey::new(ep.box_id, ep.pin_id);
                pin_net_ids.entry(key).or_default().push(net.nid);
                pin_names.entry(key).or_insert_with(|| ep.pin_name.clone());
                pin_ios.entry(key).or_insert(ep.io_type);
            }
        }

        let mut pins = BTreeMap::new();
        let mut pins_with_preferred_side = 0usize;
        let mut pins_with_actual_side = 0usize;
        let mut mismatches = 0usize;

        // Build a lookup for actual sides from box entry_points
        let entry_side_map: BTreeMap<PinKey, EntrySide> = graph
            .boxes
            .iter()
            .flat_map(|b| {
                b.entry_points
                    .iter()
                    .map(move |ep| (PinKey::new(b.id, ep.pin_id), ep.side.clone()))
            })
            .collect();

        for (key, connected_net_ids) in &pin_net_ids {
            let io_direction = pin_ios.get(key).copied().unwrap_or_default();
            let pin_name = pin_names.get(key).cloned().unwrap_or_default();

            let (preferred_side, preferred_side_reason) = Self::infer_preferred_side(io_direction);

            let actual_side = entry_side_map.get(key).cloned();

            if preferred_side.is_some() {
                pins_with_preferred_side += 1;
            }
            if actual_side.is_some() {
                pins_with_actual_side += 1;
            }
            if preferred_side.is_some() && actual_side.is_some() && preferred_side != actual_side {
                mismatches += 1;
            }

            pins.insert(
                *key,
                PinSemantic {
                    key: *key,
                    pin_name,
                    io_direction,
                    connected_net_ids: connected_net_ids.clone(),
                    preferred_side,
                    preferred_side_reason,
                    actual_side,
                    is_synthetic: key.pin_id < 0,
                },
            );
        }

        (
            pins,
            pins_with_preferred_side,
            pins_with_actual_side,
            mismatches,
        )
    }

    fn infer_preferred_side(io: IoDirection) -> (Option<EntrySide>, PinSideReason) {
        match io {
            IoDirection::Power => (Some(EntrySide::Top), PinSideReason::PowerTop),
            IoDirection::Ground => (Some(EntrySide::Bottom), PinSideReason::GroundBottom),
            IoDirection::Input => (Some(EntrySide::Left), PinSideReason::InputLeft),
            IoDirection::Output => (Some(EntrySide::Right), PinSideReason::OutputRight),
            IoDirection::Passive | IoDirection::Bidir | IoDirection::Unknown => {
                (None, PinSideReason::Unknown)
            }
        }
    }

    // ========================================================================
    // Phase 4: detect_hubs
    // ========================================================================

    fn detect_hubs(
        graph: &McVecGraph,
        _boxes: &BTreeMap<i64, BoxSemantic>,
    ) -> (usize, HashSet<i64>) {
        let hub_id = chain::find_hub(graph);
        let mut hub_ids = HashSet::new();
        if let Some(id) = hub_id {
            hub_ids.insert(id);
        }
        (hub_ids.len(), hub_ids)
    }

    // ========================================================================
    // Phase 5: detect_signal_chains
    // ========================================================================

    fn detect_signal_chains(
        graph: &McVecGraph,
        _hub_ids: &HashSet<i64>,
        _warnings: &mut Vec<SemanticWarning>,
    ) -> (Vec<SignalChainSemantic>, usize) {
        let result = chain::extract_signal_chains(graph);
        let chains: Vec<SignalChainSemantic> = result
            .chains
            .iter()
            .map(|sc| SignalChainSemantic {
                hub_id: sc.hub_id,
                hub_pin: sc.hub_pin,
                hub_pin_name: sc.hub_pin_name.clone(),
                direction_hint: match sc.direction {
                    ChainDir::Left => Some(EntrySide::Left),
                    ChainDir::Right => Some(EntrySide::Right),
                    ChainDir::Up => Some(EntrySide::Top),
                    ChainDir::Down => Some(EntrySide::Bottom),
                },
                nodes: sc
                    .nodes
                    .iter()
                    .map(|n| ChainNodeSemantic {
                        box_id: n.box_id,
                        net_id: n.net_id,
                    })
                    .collect(),
                terminus_box_id: sc.terminus.as_ref().map(|t| t.box_id),
                loops_to_hub: sc.loops_to_hub,
            })
            .collect();

        let count = chains.len();
        (chains, count)
    }

    // ========================================================================
    // Phase 6: detect_passive_chains (skeleton)
    // ========================================================================

    fn detect_passive_chains(
        graph: &McVecGraph,
        _warnings: &mut Vec<SemanticWarning>,
    ) -> (Vec<PassiveChainSemantic>, usize) {
        let _ = graph;
        (Vec::new(), 0)
    }

    // ========================================================================
    // Phase 7: detect_bus_groups (skeleton)
    // ========================================================================

    fn detect_bus_groups(
        graph: &McVecGraph,
        _warnings: &mut Vec<SemanticWarning>,
    ) -> (Vec<BusGroup>, usize) {
        let _ = graph;
        (Vec::new(), 0)
    }

    // ========================================================================
    // Phase 8: detect_rail_intents (skeleton)
    // ========================================================================

    fn detect_rail_intents(
        graph: &McVecGraph,
        _warnings: &mut Vec<SemanticWarning>,
    ) -> (Vec<RailIntent>, usize) {
        let _ = graph;
        (Vec::new(), 0)
    }

    // ========================================================================
    // Phase 9: detect_component_groups
    // ========================================================================

    fn detect_component_groups(
        signal_chains: &[SignalChainSemantic],
        _passive_chains: &[PassiveChainSemantic],
        _bus_groups: &[BusGroup],
        _rail_intents: &[RailIntent],
        _warnings: &mut Vec<SemanticWarning>,
    ) -> (Vec<ComponentGroup>, usize) {
        let mut groups = Vec::new();
        let mut next_id = 0usize;

        // Create groups from signal chains
        for sc in signal_chains {
            let mut member_ids: Vec<i64> = sc.nodes.iter().map(|n| n.box_id).collect();
            if let Some(t) = sc.terminus_box_id {
                if !member_ids.contains(&t) {
                    member_ids.push(t);
                }
            }
            // Include the hub itself
            if !member_ids.contains(&sc.hub_id) {
                member_ids.push(sc.hub_id);
            }
            member_ids.sort();
            member_ids.dedup();

            if !member_ids.is_empty() {
                groups.push(ComponentGroup {
                    group_id: next_id,
                    kind: ComponentGroupKind::SignalChain,
                    member_box_ids: member_ids,
                    anchor_box_id: Some(sc.hub_id),
                });
                next_id += 1;
            }
        }

        let count = groups.len();
        (groups, count)
    }

    // ========================================================================
    // Phase 10: attach_idioms (done inline in analyze())
    // ========================================================================

    // ========================================================================
    // Phase 11: validate_semantic_model
    // ========================================================================

    fn validate_semantic_model(
        graph: &McVecGraph,
        boxes: &BTreeMap<i64, BoxSemantic>,
        nets: &BTreeMap<i64, NetSemantic>,
        pins: &BTreeMap<PinKey, PinSemantic>,
        _signal_chains: &[SignalChainSemantic],
        warnings: &mut Vec<SemanticWarning>,
    ) {
        let _ = graph;

        for b in boxes.values() {
            if b.role == BoxRole::Unknown {
                warnings.push(SemanticWarning::new(format!(
                    "box {} ({}) has unknown role",
                    b.box_id, b.name
                )));
            }
        }

        for n in nets.values() {
            if n.role == NetRole::InternalOrIsolated && n.endpoint_count > 0 {
                warnings.push(SemanticWarning::new(format!(
                    "net {} ({}) is isolated or has only one endpoint",
                    n.net_id, n.name
                )));
            }
        }

        for p in pins.values() {
            if p.preferred_side.is_some()
                && p.actual_side.is_some()
                && p.preferred_side != p.actual_side
            {
                warnings.push(SemanticWarning::new(format!(
                    "pin {:?} prefers {:?} but actual side is {:?}",
                    p.key, p.preferred_side, p.actual_side
                )));
            }
        }
    }

    // ========================================================================
    // Report
    // ========================================================================

    /// Produce a single-line summary report for metrics output.
    pub fn report_lines(&self) -> Vec<String> {
        vec![format!(
            "[metrics] SEMANTIC: hubs={} signal_chains={} passive_chains={} \
             component_groups={} bus_groups={} rail_intents={} idioms={} \
             pins_preferred_side={} pins_actual_side={} side_mismatches={} \
             warnings={}",
            self.summary.hubs_detected,
            self.summary.signal_chains_detected,
            self.summary.passive_chains_detected,
            self.summary.component_groups_detected,
            self.summary.bus_groups_detected,
            self.summary.rail_intents_detected,
            self.summary.idioms_detected,
            self.summary.pins_with_preferred_side,
            self.summary.pins_with_actual_side,
            self.summary.preferred_actual_side_mismatches,
            self.summary.warnings,
        )]
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::{BoxPin, EntryPoint, IoSummary};
    use crate::vector::graph::{EndpointRef, Symbol, VizNet};
    use crate::viz::idiom::IdiomKind;

    // ── Helpers ────────────────────────────────────────────────────────────

    fn mk_box(
        id: i64,
        name: &str,
        kind: BoxKind,
        symbol: Symbol,
        pin_count: usize,
        x: f64,
        y: f64,
    ) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            symbol,
            None,
            None,
            pin_count,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 60.0;
        b.h = 40.0;
        b
    }

    fn mk_ic(id: i64, name: &str, pin_count: usize) -> McVecBox {
        mk_box(id, name, BoxKind::MultiPin, Symbol::Ic, pin_count, 0.0, 0.0)
    }

    fn add_pin(b: &mut McVecBox, pin_id: i64, desc: &str, io: IoDirection) {
        b.pins.push(BoxPin {
            id: pin_id,
            pin_id: desc.into(),
            description: desc.into(),
            io,
        });
    }

    fn add_entry(b: &mut McVecBox, pin_id: i64, side: EntrySide, pin_name: &str) {
        b.entry_points.push(EntryPoint {
            pin_id,
            pin_name: pin_name.into(),
            side,
            offset: 0.5,
        });
    }

    fn ep(box_id: i64, pin_id: i64, pin_name: &str, io: IoDirection) -> EndpointRef {
        EndpointRef::with_io(box_id, pin_id, pin_name, io)
    }

    fn ep_synth(box_id: i64, pin_id: i64, pin_name: &str) -> EndpointRef {
        EndpointRef::new(box_id, pin_id, pin_name)
    }

    // ── Test 1: analyze does not modify the graph ──────────────────────────

    #[test]
    fn analyze_does_not_modify_graph() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            2,
            0.0,
            0.0,
        ));
        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![ep(1, 1, "1", IoDirection::Passive)],
        ));

        let clone_before = graph.clone();
        let _model = SemanticModel::analyze(&graph);

        assert_eq!(graph.boxes.len(), clone_before.boxes.len());
        assert_eq!(graph.nets.len(), clone_before.nets.len());
        for (a, b) in graph.boxes.iter().zip(clone_before.boxes.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.name, b.name);
        }
    }

    // ── Test 2: net role ───────────────────────────────────────────────────

    #[test]
    fn power_net_role_is_power() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "VCC".into(),
            NetKind::Power,
            vec![ep(1, 1, "VCC", IoDirection::Power)],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::Power);
    }

    #[test]
    fn ground_net_role_is_ground() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "GND".into(),
            NetKind::Ground,
            vec![ep(1, 1, "GND", IoDirection::Ground)],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::Ground);
    }

    #[test]
    fn signal_net_role_is_point_to_point() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(1, 1, "A", IoDirection::Output),
                ep(2, 2, "B", IoDirection::Input),
            ],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::SignalPointToPoint);
    }

    #[test]
    fn multi_endpoint_signal_net_is_trunk() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "BUS".into(),
            NetKind::Signal,
            vec![
                ep(1, 1, "A", IoDirection::Output),
                ep(2, 2, "B", IoDirection::Input),
                ep(3, 3, "C", IoDirection::Input),
            ],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::SignalTrunk);
        assert_eq!(model.nets[&1].driver_count, 1);
    }

    // ── Test 3: preferred side ─────────────────────────────────────────────

    fn graph_with_pin(box_id: i64, pin_id: i64, io: IoDirection) -> McVecGraph {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut b = mk_box(box_id, "B1", BoxKind::MultiPin, Symbol::Ic, 4, 0.0, 0.0);
        add_pin(&mut b, pin_id, "P1", io);
        graph.boxes.push(b);
        graph.nets.push(VizNet::new(
            1,
            "NET".into(),
            NetKind::Signal,
            vec![ep(box_id, pin_id, "P1", io)],
        ));
        graph
    }

    #[test]
    fn power_pin_prefers_top() {
        let graph = graph_with_pin(1, 1, IoDirection::Power);
        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.preferred_side, Some(EntrySide::Top));
        assert_eq!(pin.preferred_side_reason, PinSideReason::PowerTop);
    }

    #[test]
    fn ground_pin_prefers_bottom() {
        let graph = graph_with_pin(1, 1, IoDirection::Ground);
        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.preferred_side, Some(EntrySide::Bottom));
        assert_eq!(pin.preferred_side_reason, PinSideReason::GroundBottom);
    }

    #[test]
    fn input_pin_prefers_left() {
        let graph = graph_with_pin(1, 1, IoDirection::Input);
        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.preferred_side, Some(EntrySide::Left));
        assert_eq!(pin.preferred_side_reason, PinSideReason::InputLeft);
    }

    #[test]
    fn output_pin_prefers_right() {
        let graph = graph_with_pin(1, 1, IoDirection::Output);
        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.preferred_side, Some(EntrySide::Right));
        assert_eq!(pin.preferred_side_reason, PinSideReason::OutputRight);
    }

    #[test]
    fn passive_pin_has_no_preferred_side() {
        let graph = graph_with_pin(1, 1, IoDirection::Passive);
        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.preferred_side, None);
        assert_eq!(pin.preferred_side_reason, PinSideReason::Unknown);
    }

    #[test]
    fn unknown_pin_has_no_preferred_side() {
        let graph = graph_with_pin(1, 1, IoDirection::Unknown);
        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.preferred_side, None);
    }

    // ── Test 4: hub detection ──────────────────────────────────────────────

    #[test]
    fn hub_detection_marks_ic_as_hub() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut ic = mk_ic(1, "U1", 8);
        add_pin(&mut ic, 1, "VCC", IoDirection::Power);
        add_pin(&mut ic, 2, "GND", IoDirection::Ground);
        add_pin(&mut ic, 3, "IN", IoDirection::Input);
        add_pin(&mut ic, 4, "OUT", IoDirection::Output);
        graph.boxes.push(ic);

        let model = SemanticModel::analyze(&graph);
        let ic_box = &model.boxes[&1];
        assert!(ic_box.is_hub_candidate);
        assert_eq!(model.summary.hubs_detected, 1);
    }

    #[test]
    fn no_hub_when_no_ic() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            2,
            0.0,
            0.0,
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.summary.hubs_detected, 0);
    }

    // ── Test 5: two-point signal net ───────────────────────────────────────

    #[test]
    fn two_point_signal_net_is_point_to_point() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![EndpointRef::new(1, 1, "A"), EndpointRef::new(2, 2, "B")],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::SignalPointToPoint);
        assert_eq!(model.nets[&1].topology, NetTopology::TwoPoint);
    }

    // ── Test 6: multi-endpoint signal net ──────────────────────────────────

    #[test]
    fn multi_driver_signal_net_is_leaf() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "BUS".into(),
            NetKind::Signal,
            vec![
                ep(1, 1, "A", IoDirection::Output),
                ep(2, 2, "B", IoDirection::Output),
                ep(3, 3, "C", IoDirection::Input),
            ],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::SignalLeaf);
        assert_eq!(model.nets[&1].driver_count, 2);
    }

    // ── Test 7: synthetic endpoint ─────────────────────────────────────────

    #[test]
    fn synthetic_endpoint_is_marked() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![ep_synth(1, -1, "SYNTH"), ep(2, 2, "B", IoDirection::Input)],
        ));
        let model = SemanticModel::analyze(&graph);
        let synth_pin = &model.pins[&PinKey::new(1, -1)];
        assert!(synth_pin.is_synthetic);
        let real_pin = &model.pins[&PinKey::new(2, 2)];
        assert!(!real_pin.is_synthetic);
    }

    // ── Test 8: idiom matches included ─────────────────────────────────────

    #[test]
    fn idiom_matches_included() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut cap = mk_box(2, "C1", BoxKind::TwoPin, Symbol::Capacitor, 2, 50.0, 50.0);
        add_pin(&mut cap, 1, "1", IoDirection::Passive);
        add_pin(&mut cap, 2, "2", IoDirection::Passive);
        graph.boxes.push(cap);

        graph.nets.push(VizNet::new(
            1,
            "VDD_3V3".into(),
            NetKind::Power,
            vec![ep(2, 1, "1", IoDirection::Power)],
        ));
        graph.nets.push(VizNet::new(
            2,
            "GND".into(),
            NetKind::Ground,
            vec![ep(2, 2, "2", IoDirection::Ground)],
        ));

        let model = SemanticModel::analyze(&graph);
        assert!(!model.idioms.is_empty());
        assert!(model.idioms.iter().any(|m| m.kind == IdiomKind::Decoupling));
        assert_eq!(model.summary.idioms_detected, model.idioms.len());
    }

    // ── Test 9: report lines ───────────────────────────────────────────────

    #[test]
    fn report_lines_are_stable() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            2,
            0.0,
            0.0,
        ));
        let a = SemanticModel::analyze(&graph);
        let b = SemanticModel::analyze(&graph);
        assert_eq!(a.report_lines(), b.report_lines());
    }

    #[test]
    fn report_lines_contain_expected_fields() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            2,
            0.0,
            0.0,
        ));
        let model = SemanticModel::analyze(&graph);
        let lines = model.report_lines();
        assert!(lines.iter().any(|l| l.contains("[metrics] SEMANTIC:")));
        assert!(lines.iter().any(|l| l.contains("hubs=")));
        assert!(lines.iter().any(|l| l.contains("signal_chains=")));
    }

    // ── Test 10: determinism ───────────────────────────────────────────────

    #[test]
    fn analyze_is_deterministic() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            2,
            0.0,
            0.0,
        ));
        graph.boxes.push(mk_box(
            2,
            "C1",
            BoxKind::TwoPin,
            Symbol::Capacitor,
            2,
            50.0,
            0.0,
        ));
        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![
                ep(1, 1, "A", IoDirection::Output),
                ep(2, 2, "B", IoDirection::Input),
            ],
        ));

        let a = SemanticModel::analyze(&graph);
        let b = SemanticModel::analyze(&graph);
        assert_eq!(a, b);
    }

    // ── Additional: actual side detection ──────────────────────────────────

    #[test]
    fn actual_side_detected_from_entry_points() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut b = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4, 0.0, 0.0);
        add_pin(&mut b, 1, "VCC", IoDirection::Power);
        add_entry(&mut b, 1, EntrySide::Top, "VCC");
        graph.boxes.push(b);
        graph.nets.push(VizNet::new(
            1,
            "VCC".into(),
            NetKind::Power,
            vec![ep(1, 1, "VCC", IoDirection::Power)],
        ));

        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.actual_side, Some(EntrySide::Top));
        // Power prefers Top, actual is Top → no mismatch
        assert_eq!(model.summary.preferred_actual_side_mismatches, 0);
    }

    #[test]
    fn side_mismatch_detected() {
        let mut graph = McVecGraph::new(0, "test".into());
        let mut b = mk_box(1, "U1", BoxKind::MultiPin, Symbol::Ic, 4, 0.0, 0.0);
        add_pin(&mut b, 1, "VCC", IoDirection::Power);
        // Actual side is Bottom, but Power prefers Top
        add_entry(&mut b, 1, EntrySide::Bottom, "VCC");
        graph.boxes.push(b);
        graph.nets.push(VizNet::new(
            1,
            "VCC".into(),
            NetKind::Power,
            vec![ep(1, 1, "VCC", IoDirection::Power)],
        ));

        let model = SemanticModel::analyze(&graph);
        let pin = &model.pins[&PinKey::new(1, 1)];
        assert_eq!(pin.preferred_side, Some(EntrySide::Top));
        assert_eq!(pin.actual_side, Some(EntrySide::Bottom));
        assert_eq!(model.summary.preferred_actual_side_mismatches, 1);
    }

    // ── Additional: BoxRole classification ─────────────────────────────────

    #[test]
    fn resistor_is_passive_role() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "R1",
            BoxKind::TwoPin,
            Symbol::Resistor,
            2,
            0.0,
            0.0,
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.boxes[&1].role, BoxRole::Passive);
    }

    #[test]
    fn submodule_is_module_boundary_role() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "sub",
            BoxKind::SubModule,
            Symbol::Module,
            4,
            0.0,
            0.0,
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.boxes[&1].role, BoxRole::ModuleBoundary);
    }

    #[test]
    fn dot_is_junction_dot_role() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph
            .boxes
            .push(mk_box(1, "dot", BoxKind::Dot, Symbol::Dot, 1, 0.0, 0.0));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.boxes[&1].role, BoxRole::JunctionDot);
    }

    #[test]
    fn power_label_is_power_flag_role() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "V3V3",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: false },
            1,
            0.0,
            0.0,
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.boxes[&1].role, BoxRole::PowerFlag);
    }

    #[test]
    fn ground_label_is_ground_flag_role() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.boxes.push(mk_box(
            1,
            "GND",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground: true },
            1,
            0.0,
            0.0,
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.boxes[&1].role, BoxRole::GroundFlag);
    }

    #[test]
    fn bus_net_role_is_bus_member() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "DATA".into(),
            NetKind::Bus(8),
            vec![ep(1, 1, "D0", IoDirection::Bidir)],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::BusMember);
    }

    #[test]
    fn submodule_io_net_role_is_module_io() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "UART0".into(),
            NetKind::SubModuleIO,
            vec![ep(1, 1, "TX", IoDirection::Output)],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::ModuleIo);
    }

    #[test]
    fn isolated_net_role() {
        let mut graph = McVecGraph::new(0, "test".into());
        graph.nets.push(VizNet::new(
            1,
            "SIG".into(),
            NetKind::Signal,
            vec![ep(1, 1, "A", IoDirection::Unknown)],
        ));
        let model = SemanticModel::analyze(&graph);
        assert_eq!(model.nets[&1].role, NetRole::InternalOrIsolated);
    }
}
