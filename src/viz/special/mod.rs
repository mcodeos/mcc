// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Milestone 10 — Power/Ground/Bus Specialization
//!
//! Read-only analysis of power nets, ground nets, and bus groups.
//! Does NOT modify graph, layout, route, or render output.
//!
//! ## Pipeline
//! ```text
//! McVecGraph + Optional SemanticModel
//!   ↓
//! PowerGroundBusModel::analyze(graph, semantic)
//!   ↓
//! report line / metrics accumulation
//! ```

use std::collections::BTreeMap;

use crate::vector::graph::{McVecGraph, NetKind, VizNet};

// ============================================================================
// PowerGroundBusModel
// ============================================================================

#[derive(Debug, Clone)]
pub struct PowerGroundBusModel {
    pub power_nets: BTreeMap<i64, PowerGroundNetIntent>,
    pub ground_nets: BTreeMap<i64, PowerGroundNetIntent>,
    pub bus_groups: BTreeMap<usize, BusSpecialization>,
    pub endpoint_roles: BTreeMap<SpecialEndpointKey, SpecialEndpointRole>,
    pub report: PowerGroundBusReport,
    pub warnings: Vec<PowerGroundBusWarning>,
    /// Long PG stubs (stub_length > LONG_PG_STUB), for diagnostic output.
    pub long_pg_stubs: Vec<PowerGroundNetIntent>,
}

// ============================================================================
// SpecialEndpointKey
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SpecialEndpointKey {
    pub net_id: i64,
    pub box_id: i64,
    pub pin_id: i64,
}

// ============================================================================
// PowerGroundNetIntent
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct PowerGroundNetIntent {
    pub net_id: i64,
    pub name: String,
    pub is_ground: bool,
    pub endpoint_count: usize,
    pub real_endpoint_count: usize,
    pub flag_endpoint_count: usize,
    pub role: PowerGroundRole,
    pub preferred_view: PowerGroundView,
    pub max_stub_length: f64,
    pub stub_length: f64,
    pub is_long_stub: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerGroundRole {
    LocalFlagStub,
    SharedNamedRail,
    DecouplingStub,
    ModuleBoundarySupply,
    UnknownSupply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerGroundView {
    SameNameFlags,
    ShortStub,
    LocalRail,
    ExplicitTrunk,
}

// ============================================================================
// BusSpecialization
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct BusSpecialization {
    pub group_id: usize,
    pub base_name: String,
    pub width: usize,
    pub member_net_ids: Vec<i64>,
    pub bit_order: Vec<(usize, i64)>,
    pub endpoints: Vec<BusEndpointRole>,
    pub preferred_trunk_axis: Option<BusTrunkAxis>,
    pub label: Option<String>,
    pub taps: usize,
    pub tap_bends: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusTrunkAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BusEndpointRole {
    pub net_id: i64,
    pub bit_index: Option<usize>,
    pub box_id: i64,
    pub pin_id: i64,
    pub pin_name: String,
    pub order_key: BusEndpointOrderKey,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BusEndpointOrderKey {
    ConnectorPin(usize),
    BitIndex(usize),
    PositionAlongTrunk(f64),
    StableId(i64),
}

// ============================================================================
// SpecialEndpointRole
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialEndpointRole {
    PowerSource,
    PowerConsumer,
    GroundReturn,
    GroundSymbol,
    BusTap,
    BusTrunkMember,
    BusConnectorPin,
    BusBoundaryPort,
}

// ============================================================================
// PowerGroundBusReport
// ============================================================================

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PowerGroundBusReport {
    pub power_nets: usize,
    pub ground_nets: usize,
    pub bus_groups: usize,
    pub bus_bits_total: usize,
    pub bus_bits_ordered: usize,

    pub pg_flags_total: usize,
    pub pg_stubs_total: usize,
    pub pg_stub_length_total: f64,
    pub pg_avg_stub_length: f64,
    pub pg_long_stubs: usize,

    pub bus_trunks: usize,
    pub bus_taps: usize,
    pub bus_tap_bends: usize,
    pub bus_labels: usize,

    pub warnings: usize,
}

impl PowerGroundBusReport {
    /// Merge another layer's report into this one (accumulate across layers).
    pub fn merge(&mut self, other: &PowerGroundBusReport) {
        self.power_nets += other.power_nets;
        self.ground_nets += other.ground_nets;
        self.bus_groups += other.bus_groups;
        self.bus_bits_total += other.bus_bits_total;
        self.bus_bits_ordered += other.bus_bits_ordered;
        self.pg_flags_total += other.pg_flags_total;
        self.pg_stubs_total += other.pg_stubs_total;
        self.pg_stub_length_total += other.pg_stub_length_total;
        self.pg_long_stubs += other.pg_long_stubs;
        self.bus_trunks += other.bus_trunks;
        self.bus_taps += other.bus_taps;
        self.bus_tap_bends += other.bus_tap_bends;
        self.bus_labels += other.bus_labels;
        self.warnings += other.warnings;
        // Recompute derived
        self.pg_avg_stub_length = if self.pg_stubs_total > 0 {
            self.pg_stub_length_total / self.pg_stubs_total as f64
        } else {
            0.0
        };
    }

    pub fn report_line(&self) -> String {
        format!(
            "[metrics] SPECIAL: power={} ground={} bus_groups={} bus_bits={}/{} \
             flags={} pg_avg_stub={:.1} pg_long_stubs={} \
             bus_trunks={} bus_taps={} bus_tap_bends={} bus_labels={} warnings={}",
            self.power_nets,
            self.ground_nets,
            self.bus_groups,
            self.bus_bits_ordered,
            self.bus_bits_total,
            self.pg_flags_total,
            self.pg_avg_stub_length,
            self.pg_long_stubs,
            self.bus_trunks,
            self.bus_taps,
            self.bus_tap_bends,
            self.bus_labels,
            self.warnings,
        )
    }
}

// ============================================================================
// PowerGroundBusWarning
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum PowerGroundBusWarning {
    DuplicateBusBitIndex { base_name: String, bit_index: usize },
    MissingBusBitIndex { base_name: String, bit_index: usize },
    BusBitOrderFallback { base_name: String, net_id: i64 },
    LongPowerGroundStub { net_name: String, length: f64 },
    PowerGroundSignalBoundary { net_name: String },
}

// ============================================================================
// Constants
// ============================================================================

const LONG_PG_STUB: f64 = 120.0;

// ============================================================================
// analyze()
// ============================================================================

impl PowerGroundBusModel {
    pub fn analyze(
        graph: &McVecGraph,
        _semantic: Option<&crate::viz::semantic::SemanticModel>,
    ) -> Self {
        let mut warnings = Vec::new();
        let mut power_nets: BTreeMap<i64, PowerGroundNetIntent> = BTreeMap::new();
        let mut ground_nets: BTreeMap<i64, PowerGroundNetIntent> = BTreeMap::new();
        let mut bus_groups: BTreeMap<usize, BusSpecialization> = BTreeMap::new();
        let mut endpoint_roles: BTreeMap<SpecialEndpointKey, SpecialEndpointRole> = BTreeMap::new();

        // ── Phase 1: Collect power/ground nets ──
        for net in &graph.nets {
            match net.kind {
                NetKind::Power => {
                    let intent = analyze_power_ground_net(graph, net, false);
                    endpoint_roles.extend(collect_pg_endpoint_roles(
                        graph,
                        net,
                        false,
                        &intent.role,
                    ));
                    power_nets.insert(net.nid, intent);
                }
                NetKind::Ground => {
                    let intent = analyze_power_ground_net(graph, net, true);
                    endpoint_roles.extend(collect_pg_endpoint_roles(
                        graph,
                        net,
                        true,
                        &intent.role,
                    ));
                    ground_nets.insert(net.nid, intent);
                }
                _ => {}
            }
        }

        // ── Phase 2: Collect bus groups ──
        let mut next_group_id = 0usize;
        for net in &graph.nets {
            if let NetKind::Bus(width) = net.kind {
                if width == 0 {
                    continue;
                }
                // Check if this net is already in a group (by name prefix)
                if bus_groups
                    .values()
                    .any(|g| g.member_net_ids.contains(&net.nid))
                {
                    continue;
                }
                let base = extract_bus_base_name(&net.name);
                // Collect all Bus nets sharing this base name
                let mut members: Vec<&VizNet> = Vec::new();
                for other in &graph.nets {
                    if let NetKind::Bus(_) = other.kind {
                        let obase = extract_bus_base_name(&other.name);
                        if obase == base
                            && !bus_groups
                                .values()
                                .any(|g| g.member_net_ids.contains(&other.nid))
                        {
                            members.push(other);
                        }
                    }
                }
                if members.is_empty() {
                    members.push(net);
                }

                let mut bus = BusSpecialization {
                    group_id: next_group_id,
                    base_name: base.clone(),
                    width: 0,
                    member_net_ids: members.iter().map(|m| m.nid).collect(),
                    bit_order: Vec::new(),
                    endpoints: Vec::new(),
                    preferred_trunk_axis: None,
                    label: Some(base.clone()),
                    taps: 0,
                    tap_bends: 0,
                };

                // Build bit order
                let mut bit_order: Vec<(usize, i64)> = Vec::new();
                for m in &members {
                    if let Some(bit) = extract_bit_index(&m.name) {
                        if bit_order.iter().any(|(b, _)| *b == bit) {
                            warnings.push(PowerGroundBusWarning::DuplicateBusBitIndex {
                                base_name: base.clone(),
                                bit_index: bit,
                            });
                        }
                        bit_order.push((bit, m.nid));
                    } else {
                        warnings.push(PowerGroundBusWarning::BusBitOrderFallback {
                            base_name: base.clone(),
                            net_id: m.nid,
                        });
                        bit_order.push((bit_order.len(), m.nid));
                    }
                }
                bit_order.sort_by_key(|(bit, _)| *bit);
                bus.width = bit_order.len();
                bus.bit_order = bit_order;

                // Collect endpoints and roles
                for m in &members {
                    let bit = extract_bit_index(&m.name);
                    for ep in &m.endpoints {
                        bus.endpoints.push(BusEndpointRole {
                            net_id: m.nid,
                            bit_index: bit,
                            box_id: ep.box_id,
                            pin_id: ep.pin_id,
                            pin_name: ep.pin_name.clone(),
                            order_key: BusEndpointOrderKey::BitIndex(bit.unwrap_or(0)),
                        });
                        let key = SpecialEndpointKey {
                            net_id: m.nid,
                            box_id: ep.box_id,
                            pin_id: ep.pin_id,
                        };
                        // Determine if connector pin
                        if let Some(b) = graph.boxes.iter().find(|x| x.id == ep.box_id) {
                            if b.designator
                                .as_deref()
                                .map_or(false, |d| d.contains("CONN"))
                            {
                                endpoint_roles.insert(key, SpecialEndpointRole::BusConnectorPin);
                            } else {
                                endpoint_roles.insert(key, SpecialEndpointRole::BusTap);
                            }
                        }
                    }
                }

                // Count taps and bends
                let mut taps = 0usize;
                let mut tap_bends = 0usize;
                for m in &members {
                    if let Some(ref route) = m.route {
                        taps += 1;
                        tap_bends += route.segments.len().saturating_sub(1);
                    }
                }
                bus.taps = taps;
                bus.tap_bends = tap_bends;

                bus_groups.insert(next_group_id, bus);
                next_group_id += 1;
            }
        }

        // ── Phase 3: Build report ──
        let mut pg_flags = 0usize;
        let mut pg_stubs = 0usize;
        let mut pg_stub_len = 0.0f64;
        let mut pg_long = 0usize;
        let mut long_pg_stubs = Vec::new();

        for intent in power_nets.values().chain(ground_nets.values()) {
            pg_flags += intent.flag_endpoint_count;
            if intent.stub_length > 0.0 {
                pg_stubs += 1;
                pg_stub_len += intent.stub_length;
                if intent.is_long_stub {
                    pg_long += 1;
                    long_pg_stubs.push(intent.clone());
                }
            }
        }

        // Sort long stubs by length descending (most severe first)
        long_pg_stubs.sort_by(|a, b| {
            b.stub_length
                .partial_cmp(&a.stub_length)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let pg_avg = if pg_stubs > 0 {
            pg_stub_len / pg_stubs as f64
        } else {
            0.0
        };

        let bus_bits_ordered: usize = bus_groups.values().map(|g| g.bit_order.len()).sum();
        let bus_bits_total: usize = bus_groups.values().map(|g| g.width).sum();
        let bus_trunks = bus_groups.len();
        let bus_taps: usize = bus_groups.values().map(|g| g.taps).sum();
        let bus_tap_bends: usize = bus_groups.values().map(|g| g.tap_bends).sum();
        let bus_labels: usize = bus_groups.values().filter(|g| g.label.is_some()).count();

        let report = PowerGroundBusReport {
            power_nets: power_nets.len(),
            ground_nets: ground_nets.len(),
            bus_groups: bus_groups.len(),
            bus_bits_total,
            bus_bits_ordered,
            pg_flags_total: pg_flags,
            pg_stubs_total: pg_stubs,
            pg_stub_length_total: pg_stub_len,
            pg_avg_stub_length: pg_avg,
            pg_long_stubs: pg_long,
            bus_trunks,
            bus_taps,
            bus_tap_bends,
            bus_labels,
            warnings: warnings.len(),
        };

        PowerGroundBusModel {
            power_nets,
            ground_nets,
            bus_groups,
            endpoint_roles,
            report,
            warnings,
            long_pg_stubs,
        }
    }

    /// Return long PG stubs (stub_length > LONG_PG_STUB), sorted by length descending.
    pub fn long_power_ground_stubs(&self) -> &[PowerGroundNetIntent] {
        &self.long_pg_stubs
    }

    /// Log diagnostic vlog for each long PG stub.
    pub fn vlog_long_stubs(&self, layer_name: &str) {
        for stub in &self.long_pg_stubs {
            let kind = if stub.is_ground { "Ground" } else { "Power" };
            crate::vlog!(
                "[viz::special] long_pg_stub layer='{}' nid={} name='{}' kind={} len={:.1} ep_count={} flag_eps={} role={:?}",
                layer_name,
                stub.net_id,
                stub.name,
                kind,
                stub.stub_length,
                stub.real_endpoint_count,
                stub.flag_endpoint_count,
                stub.role,
            );
        }
    }
}

// ============================================================================
// Helper: analyze a single power/ground net
// ============================================================================

fn analyze_power_ground_net(
    graph: &McVecGraph,
    net: &VizNet,
    is_ground: bool,
) -> PowerGroundNetIntent {
    let real_count = net.endpoints.len();
    let flag_count =
        net.endpoints
            .iter()
            .filter(|ep| {
                graph.boxes.iter().any(|b| {
                    b.id == ep.box_id && b.kind == crate::vector::graph::BoxKind::PowerLabel
                })
            })
            .count();

    let stub_length = net
        .route
        .as_ref()
        .map(|r| {
            r.segments
                .iter()
                .map(|s| ((s.to.x - s.from.x).powi(2) + (s.to.y - s.from.y).powi(2)).sqrt())
                .sum::<f64>()
        })
        .unwrap_or(0.0);

    let role = classify_pg_role(graph, net, real_count, flag_count, &stub_length);
    let view = default_pg_view(&role);

    PowerGroundNetIntent {
        net_id: net.nid,
        name: net.name.clone(),
        is_ground,
        endpoint_count: net.endpoints.len(),
        real_endpoint_count: real_count,
        flag_endpoint_count: flag_count,
        max_stub_length: stub_length,
        stub_length,
        is_long_stub: stub_length > LONG_PG_STUB,
        role,
        preferred_view: view,
    }
}

fn classify_pg_role(
    _graph: &McVecGraph,
    _net: &VizNet,
    real_count: usize,
    flag_count: usize,
    stub_length: &f64,
) -> PowerGroundRole {
    if flag_count > 0 && real_count > 0 && *stub_length < LONG_PG_STUB {
        PowerGroundRole::LocalFlagStub
    } else if flag_count > 0 && real_count == 0 {
        PowerGroundRole::SharedNamedRail
    } else if *stub_length < 60.0 && real_count <= 2 {
        PowerGroundRole::DecouplingStub
    } else if *stub_length > LONG_PG_STUB * 2.0 {
        PowerGroundRole::ModuleBoundarySupply
    } else {
        PowerGroundRole::UnknownSupply
    }
}

fn default_pg_view(role: &PowerGroundRole) -> PowerGroundView {
    match role {
        PowerGroundRole::LocalFlagStub => PowerGroundView::ShortStub,
        PowerGroundRole::DecouplingStub => PowerGroundView::ShortStub,
        PowerGroundRole::SharedNamedRail => PowerGroundView::SameNameFlags,
        PowerGroundRole::ModuleBoundarySupply => PowerGroundView::ExplicitTrunk,
        PowerGroundRole::UnknownSupply => PowerGroundView::SameNameFlags,
    }
}

fn collect_pg_endpoint_roles(
    graph: &McVecGraph,
    net: &VizNet,
    is_ground: bool,
    _role: &PowerGroundRole,
) -> BTreeMap<SpecialEndpointKey, SpecialEndpointRole> {
    let mut roles = BTreeMap::new();
    for ep in &net.endpoints {
        let key = SpecialEndpointKey {
            net_id: net.nid,
            box_id: ep.box_id,
            pin_id: ep.pin_id,
        };
        let ep_role = if let Some(b) = graph.boxes.iter().find(|x| x.id == ep.box_id) {
            if b.kind == crate::vector::graph::BoxKind::PowerLabel {
                SpecialEndpointRole::PowerSource
            } else if is_ground {
                SpecialEndpointRole::GroundReturn
            } else {
                SpecialEndpointRole::PowerConsumer
            }
        } else if is_ground {
            SpecialEndpointRole::GroundReturn
        } else {
            SpecialEndpointRole::PowerConsumer
        };
        roles.insert(key, ep_role);
    }
    roles
}

// ============================================================================
// Bus name helpers
// ============================================================================

fn extract_bus_base_name(name: &str) -> String {
    // Strip trailing bit notation: DATA[0], DATA_0, DATA0
    let s = name.trim();
    // Pattern: NAME[bit]
    if let Some(pos) = s.rfind('[') {
        let prefix = s[..pos].trim_end_matches('_');
        return prefix.to_string();
    }
    // Pattern: NAME_N where N is the last part
    if let Some(pos) = s.rfind('_') {
        let suffix = &s[pos + 1..];
        if suffix.chars().all(|c| c.is_ascii_digit()) {
            return s[..pos].to_string();
        }
    }
    // Pattern: NAME123 where digits at end
    if let Some(pos) = s.rfind(|c: char| !c.is_ascii_digit()) {
        let suffix = &s[pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return s[..=pos].to_string();
        }
    }
    s.to_string()
}

fn extract_bit_index(name: &str) -> Option<usize> {
    let s = name.trim();
    // Pattern: NAME[bit]
    if let Some(start) = s.rfind('[') {
        if let Some(end) = s[start + 1..].find(']') {
            return s[start + 1..start + 1 + end].parse::<usize>().ok();
        }
    }
    // Pattern: NAME_N
    if let Some(pos) = s.rfind('_') {
        let suffix = &s[pos + 1..];
        if suffix.chars().all(|c| c.is_ascii_digit()) {
            return suffix.parse::<usize>().ok();
        }
    }
    // Pattern: NAME123
    if let Some(pos) = s.rfind(|c: char| !c.is_ascii_digit()) {
        let suffix = &s[pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return suffix.parse::<usize>().ok();
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::net_def::{IoDirection, Route};
    use crate::vector::graph::{
        BoxKind, EndpointRef, EntryPoint, EntrySide, IoSummary, McVecBox, NetKind, Point, Segment,
        Symbol, VizNet,
    };

    fn mk_box(id: i64, name: &str, kind: BoxKind, x: f64, y: f64, w: f64, h: f64) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            String::new(),
            kind,
            Symbol::Resistor,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        b.entry_points.push(EntryPoint {
            pin_id: 1,
            side: EntrySide::Right,
            offset: 0.0,
            pin_name: "1".into(),
        });
        b
    }

    fn mk_ep(box_id: i64, pin_id: i64) -> EndpointRef {
        EndpointRef {
            box_id,
            pin_id,
            pin_name: String::new(),
            io_type: IoDirection::Unknown,
            pin_number: None,
        }
    }

    fn mk_net(nid: i64, name: &str, kind: NetKind, len: f64) -> VizNet {
        let mut net = VizNet::new(nid, name.into(), kind, vec![]);
        net.route = Some(Route {
            segments: vec![Segment {
                from: Point { x: 0.0, y: 0.0 },
                to: Point { x: len, y: 0.0 },
            }],
            junctions: vec![],
        });
        net
    }

    fn mk_graph() -> McVecGraph {
        McVecGraph::new(0, "test".into())
    }

    // ── Power net detection ──

    #[test]
    fn detects_power_net() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "VCC", NetKind::Power, 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        assert_eq!(model.report.power_nets, 1);
        assert_eq!(model.report.ground_nets, 0);
    }

    // ── Ground net detection ──

    #[test]
    fn detects_ground_net() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "GND", NetKind::Ground, 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        assert_eq!(model.report.power_nets, 0);
        assert_eq!(model.report.ground_nets, 1);
    }

    // ── Power flag endpoint role ──

    #[test]
    fn power_flag_is_power_source() {
        let mut graph = mk_graph();
        graph.boxes.push(mk_box(
            1,
            "VCC",
            BoxKind::PowerLabel,
            10.0,
            10.0,
            20.0,
            20.0,
        ));
        graph
            .boxes
            .push(mk_box(2, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "VCC", NetKind::Power, 60.0);
        net.endpoints.push(mk_ep(1, 1));
        net.endpoints.push(mk_ep(2, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let key = SpecialEndpointKey {
            net_id: 1,
            box_id: 1,
            pin_id: 1,
        };
        assert_eq!(
            model.endpoint_roles.get(&key),
            Some(&SpecialEndpointRole::PowerSource)
        );
    }

    // ── Ground endpoint role ──

    #[test]
    fn ground_net_endpoint_is_ground_return() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "GND", NetKind::Ground, 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let key = SpecialEndpointKey {
            net_id: 1,
            box_id: 1,
            pin_id: 1,
        };
        assert_eq!(
            model.endpoint_roles.get(&key),
            Some(&SpecialEndpointRole::GroundReturn)
        );
    }

    // ── Stub length ──

    #[test]
    fn computes_stub_length() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "VCC", NetKind::Power, 100.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let intent = model.power_nets.get(&1).unwrap();
        assert!((intent.stub_length - 100.0).abs() < 0.01);
        assert!(!intent.is_long_stub);
    }

    // ── Long stub detection ──

    #[test]
    fn detects_long_stub() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "VCC", NetKind::Power, 200.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let intent = model.power_nets.get(&1).unwrap();
        assert!(intent.is_long_stub);
        assert_eq!(model.report.pg_long_stubs, 1);
    }

    // ── Bus group detection ──

    #[test]
    fn detects_bus_group() {
        let mut graph = mk_graph();
        for i in 0..4 {
            let mut net = mk_net(i as i64, &format!("DATA[{}]", i), NetKind::Bus(4), 50.0);
            net.endpoints.push(mk_ep(1, 1));
            graph.nets.push(net);
        }

        let model = PowerGroundBusModel::analyze(&graph, None);
        assert_eq!(model.report.bus_groups, 1);
        assert_eq!(model.report.bus_bits_ordered, 4);
        assert_eq!(model.report.bus_bits_total, 4);
    }

    // ── Bus bit order is stable ──

    #[test]
    fn bus_bit_order_stable() {
        let mut graph = mk_graph();
        let mut net2 = mk_net(2, "DATA[2]", NetKind::Bus(4), 50.0);
        net2.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net2);
        let mut net0 = mk_net(0, "DATA[0]", NetKind::Bus(4), 50.0);
        net0.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net0);
        let mut net1 = mk_net(1, "DATA[1]", NetKind::Bus(4), 50.0);
        net1.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net1);
        let mut net3 = mk_net(3, "DATA[3]", NetKind::Bus(4), 50.0);
        net3.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net3);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let bus = model.bus_groups.values().next().unwrap();
        let order: Vec<usize> = bus.bit_order.iter().map(|(b, _)| *b).collect();
        assert_eq!(order, vec![0, 1, 2, 3]);
    }

    // ── Bus name-based grouping ──

    #[test]
    fn bus_name_based_grouping() {
        let mut graph = mk_graph();
        let mut net0 = mk_net(0, "DATA_0", NetKind::Bus(4), 50.0);
        net0.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net0);
        let mut net1 = mk_net(1, "DATA_1", NetKind::Bus(4), 50.0);
        net1.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net1);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let bus = model.bus_groups.values().next().unwrap();
        assert_eq!(bus.base_name, "DATA");
        assert_eq!(bus.bit_order.len(), 2);
    }

    // ── Report is deterministic ──

    #[test]
    fn report_deterministic() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "VCC", NetKind::Power, 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let a = PowerGroundBusModel::analyze(&graph, None);
        let b = PowerGroundBusModel::analyze(&graph, None);
        assert_eq!(a.report, b.report);
    }

    // ── No power/ground/bus gives all-zero report ──

    #[test]
    fn empty_graph_all_zero_report() {
        let graph = mk_graph();
        let model = PowerGroundBusModel::analyze(&graph, None);
        assert_eq!(model.report.power_nets, 0);
        assert_eq!(model.report.ground_nets, 0);
        assert_eq!(model.report.bus_groups, 0);
        assert_eq!(model.report.bus_bits_total, 0);
        assert_eq!(model.report.pg_avg_stub_length, 0.0);
    }

    // ── Report line is stable ──

    #[test]
    fn report_line_stable() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "VCC", NetKind::Power, 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let line = model.report.report_line();
        assert!(line.contains("[metrics] SPECIAL"));
        assert!(line.contains("power=1"));
    }

    // ── Bus bit index underscore notation ──

    #[test]
    fn bus_bit_underscore_notation() {
        let mut graph = mk_graph();
        let mut net = mk_net(0, "ADDR_0", NetKind::Bus(8), 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);
        let mut net = mk_net(1, "ADDR_1", NetKind::Bus(8), 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let bus = model.bus_groups.values().next().unwrap();
        assert_eq!(bus.base_name, "ADDR");
        let order: Vec<usize> = bus.bit_order.iter().map(|(b, _)| *b).collect();
        assert_eq!(order, vec![0, 1]);
    }

    // ── Bus bit pure numeric notation ──

    #[test]
    fn bus_bit_numeric_notation() {
        let mut graph = mk_graph();
        let mut net = mk_net(0, "DATA0", NetKind::Bus(4), 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);
        let mut net = mk_net(1, "DATA1", NetKind::Bus(4), 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let bus = model.bus_groups.values().next().unwrap();
        assert_eq!(bus.base_name, "DATA");
        let order: Vec<usize> = bus.bit_order.iter().map(|(b, _)| *b).collect();
        assert_eq!(order, vec![0, 1]);
    }

    // ── Flag count is correct ──

    #[test]
    fn flag_count_correct() {
        let mut graph = mk_graph();
        graph.boxes.push(mk_box(
            1,
            "VCC",
            BoxKind::PowerLabel,
            10.0,
            10.0,
            20.0,
            20.0,
        ));
        graph
            .boxes
            .push(mk_box(2, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net = mk_net(1, "VCC", NetKind::Power, 60.0);
        net.endpoints.push(mk_ep(1, 1));
        net.endpoints.push(mk_ep(2, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let intent = model.power_nets.get(&1).unwrap();
        assert_eq!(intent.flag_endpoint_count, 1);
        assert_eq!(model.report.pg_flags_total, 1);
    }

    // ── Bus width 0 skipped ──

    #[test]
    fn bus_width_zero_skipped() {
        let mut graph = mk_graph();
        let mut net = mk_net(0, "EMPTY", NetKind::Bus(0), 0.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        assert_eq!(model.report.bus_groups, 0);
    }

    // ── Avg stub length ──

    #[test]
    fn avg_stub_length() {
        let mut graph = mk_graph();
        graph
            .boxes
            .push(mk_box(1, "IC1", BoxKind::MultiPin, 50.0, 50.0, 30.0, 30.0));
        let mut net1 = mk_net(1, "VCC", NetKind::Power, 50.0);
        net1.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net1);
        let mut net2 = mk_net(2, "VDD", NetKind::Power, 100.0);
        net2.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net2);

        let model = PowerGroundBusModel::analyze(&graph, None);
        assert!((model.report.pg_avg_stub_length - 75.0).abs() < 0.01);
    }

    // ── Warnings count ──

    #[test]
    fn warnings_count_correct() {
        let mut graph = mk_graph();
        let mut net = mk_net(0, "DATA[0]", NetKind::Bus(4), 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);
        let mut net = mk_net(1, "DATA[0]", NetKind::Bus(4), 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        assert_eq!(model.report.warnings, 1);
    }

    // ── Bus taps and bends ──

    #[test]
    fn bus_taps_and_bends() {
        let mut graph = mk_graph();
        let mut net = mk_net(0, "DATA[0]", NetKind::Bus(4), 50.0);
        net.endpoints.push(mk_ep(1, 1));
        graph.nets.push(net);

        let model = PowerGroundBusModel::analyze(&graph, None);
        let bus = model.bus_groups.values().next().unwrap();
        assert_eq!(bus.taps, 1);
        assert_eq!(bus.tap_bends, 0); // 1 segment → 0 bends
    }

    // ── Report merge across layers ──

    #[test]
    fn report_merge_accumulates() {
        let mut a = PowerGroundBusReport {
            power_nets: 2,
            ground_nets: 1,
            bus_groups: 1,
            bus_bits_total: 4,
            bus_bits_ordered: 4,
            pg_flags_total: 3,
            pg_stubs_total: 2,
            pg_stub_length_total: 100.0,
            pg_avg_stub_length: 50.0,
            pg_long_stubs: 0,
            bus_trunks: 1,
            bus_taps: 4,
            bus_tap_bends: 3,
            bus_labels: 1,
            warnings: 0,
        };
        let b = PowerGroundBusReport {
            power_nets: 1,
            ground_nets: 0,
            bus_groups: 0,
            bus_bits_total: 0,
            bus_bits_ordered: 0,
            pg_flags_total: 1,
            pg_stubs_total: 1,
            pg_stub_length_total: 200.0,
            pg_avg_stub_length: 200.0,
            pg_long_stubs: 1,
            bus_trunks: 0,
            bus_taps: 0,
            bus_tap_bends: 0,
            bus_labels: 0,
            warnings: 1,
        };
        a.merge(&b);
        assert_eq!(a.power_nets, 3);
        assert_eq!(a.ground_nets, 1);
        assert_eq!(a.bus_groups, 1);
        assert_eq!(a.pg_stubs_total, 3);
        assert_eq!(a.pg_stub_length_total, 300.0);
        assert!((a.pg_avg_stub_length - 100.0).abs() < 0.01);
        assert_eq!(a.pg_long_stubs, 1);
        assert_eq!(a.warnings, 1);
    }
}
