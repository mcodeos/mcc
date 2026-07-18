// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase C — SchematicLayoutModel: unified layout intent model
//!
//! **Status: read-only report** — aggregates SemanticModel, SpecialModel,
//! IdiomModel, PinPlacement, RouteFeedback, and LabelPressure into a single
//! layout intent structure. Does NOT modify geometry. Consumed by FlowLayouter
//! in Phase D.
//!
//! ## Architecture
//!
//! ```text
//! SemanticModel ──┐
//! SpecialModel  ──┤
//! IdiomModel    ──┼── SchematicLayoutModel ──► report (Phase C)
//! PinPlacement  ──┤                             │
//! RouteFeedback ──┤                             └──► FlowLayouter (Phase D)
//! LabelPressure ──┘
//! ```

use std::collections::BTreeMap;

use crate::vector::graph::McVecGraph;
use crate::viz::idiom::model::IdiomInstance;
use crate::viz::idiom::model::IdiomInstanceKind;
use crate::viz::semantic::SemanticModel;
use crate::viz::special::PowerGroundBusModel;

// ============================================================================
// Box role in the layout model
// ============================================================================

/// The layout role of a box, derived from semantic + special analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BoxLayoutRole {
    Hub,
    SignalBlock,
    Passive,
    PowerRail,
    SubModule,
    Connector,
    Unknown,
}

/// A box entry in the layout model.
#[derive(Debug, Clone)]
pub struct LayoutBoxEntry {
    pub box_id: i64,
    pub name: String,
    pub role: BoxLayoutRole,
    pub hub_score: usize,
    pub geom_locked: bool,
    pub idiom: Option<IdiomKind>,
    pub label_pressure: LabelPressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdiomKind {
    Decoupling,
    Pullup,
    Pulldown,
    DiffPair,
}

#[derive(Debug, Clone, Default)]
pub struct LabelPressure {
    pub needs_designator_space: bool,
    pub needs_value_space: bool,
    pub needs_extra_margin: bool,
}

// ============================================================================
// Functional block
// ============================================================================

/// A functional block: a group of boxes that form a logical unit.
#[derive(Debug, Clone)]
pub struct FunctionalBlock {
    pub block_id: usize,
    pub kind: String,
    pub box_ids: Vec<i64>,
    pub anchor_box_id: Option<i64>,
}

// ============================================================================
// Flow lane
// ============================================================================

/// A visual flow lane: signal flows from hub through chain.
#[derive(Debug, Clone)]
pub struct FlowLane {
    pub lane_id: usize,
    pub hub_id: i64,
    pub node_box_ids: Vec<i64>,
    pub terminus_box_id: Option<i64>,
}

// ============================================================================
// Rail plan
// ============================================================================

/// A single power or ground rail in the layout.
#[derive(Debug, Clone)]
pub struct RailPlan {
    pub net_id: i64,
    pub name: String,
    pub is_power: bool,
    pub is_ground: bool,
    pub endpoint_count: usize,
    pub stub_length: f64,
    pub is_long_stub: bool,
}

// ============================================================================
// Bus plan
// ============================================================================

/// A bus trunk in the layout.
#[derive(Debug, Clone)]
pub struct BusPlan {
    pub group_id: usize,
    pub name: String,
    pub width: usize,
    pub member_net_ids: Vec<i64>,
}

// ============================================================================
// SchematicLayoutModel
// ============================================================================

/// The unified layout intent model — aggregate of all analysis layers.
#[derive(Debug, Clone)]
pub struct SchematicLayoutModel {
    pub boxes: Vec<LayoutBoxEntry>,
    pub functional_blocks: Vec<FunctionalBlock>,
    pub flow_lanes: Vec<FlowLane>,
    pub rail_plan: Vec<RailPlan>,
    pub bus_plan: Vec<BusPlan>,
    pub warnings: Vec<String>,
}

impl SchematicLayoutModel {
    /// Build the layout model from graph + analysis results.
    /// This is a **read-only** pass — no geometry is modified.
    pub fn build(
        graph: &McVecGraph,
        semantic: &SemanticModel,
        special: &PowerGroundBusModel,
        idioms: &[IdiomInstance],
    ) -> Self {
        let mut model = Self {
            boxes: Vec::new(),
            functional_blocks: Vec::new(),
            flow_lanes: Vec::new(),
            rail_plan: Vec::new(),
            bus_plan: Vec::new(),
            warnings: Vec::new(),
        };

        model.build_box_entries(graph, semantic, idioms);
        model.build_rail_plan(special);
        model.build_bus_plan(special);
        model.build_functional_blocks(semantic);
        model.build_flow_lanes(semantic);

        model
    }

    fn build_box_entries(
        &mut self,
        graph: &McVecGraph,
        semantic: &SemanticModel,
        idioms: &[IdiomInstance],
    ) {
        for b in &graph.boxes {
            let role = classify_box_role(b);
            let hub_score = semantic
                .boxes
                .get(&b.id)
                .map(|bs| bs.hub_score)
                .unwrap_or(0);

            let idiom = idioms
                .iter()
                .find(|i| i.satellite_box_ids.contains(&b.id) || i.anchor_box_id == b.id)
                .map(|i| match i.kind {
                    IdiomInstanceKind::Decoupling => IdiomKind::Decoupling,
                    IdiomInstanceKind::Pullup => IdiomKind::Pullup,
                    IdiomInstanceKind::Pulldown => IdiomKind::Pulldown,
                    IdiomInstanceKind::DiffPair => IdiomKind::DiffPair,
                });

            let label_pressure = LabelPressure {
                needs_designator_space: b.designator.is_some(),
                needs_value_space: b.value.is_some(),
                needs_extra_margin: b.pins.len() > 4,
            };

            self.boxes.push(LayoutBoxEntry {
                box_id: b.id,
                name: b.name.clone(),
                role,
                hub_score,
                geom_locked: b.geom_locked,
                idiom,
                label_pressure,
            });
        }
    }

    fn build_rail_plan(&mut self, special: &PowerGroundBusModel) {
        for (&net_id, intent) in &special.power_nets {
            self.rail_plan.push(RailPlan {
                net_id,
                name: intent.name.clone(),
                is_power: true,
                is_ground: false,
                endpoint_count: intent.endpoint_count,
                stub_length: intent.stub_length,
                is_long_stub: intent.is_long_stub,
            });
        }
        for (&net_id, intent) in &special.ground_nets {
            self.rail_plan.push(RailPlan {
                net_id,
                name: intent.name.clone(),
                is_power: false,
                is_ground: true,
                endpoint_count: intent.endpoint_count,
                stub_length: intent.stub_length,
                is_long_stub: intent.is_long_stub,
            });
        }
    }

    fn build_bus_plan(&mut self, special: &PowerGroundBusModel) {
        for (&group_id, bus) in &special.bus_groups {
            self.bus_plan.push(BusPlan {
                group_id,
                name: bus.base_name.clone(),
                width: bus.width,
                member_net_ids: bus.member_net_ids.clone(),
            });
        }
    }

    fn build_functional_blocks(&mut self, semantic: &SemanticModel) {
        for group in &semantic.component_groups {
            self.functional_blocks.push(FunctionalBlock {
                block_id: group.group_id,
                kind: format!("{:?}", group.kind),
                box_ids: group.member_box_ids.clone(),
                anchor_box_id: group.anchor_box_id,
            });
        }
    }

    fn build_flow_lanes(&mut self, semantic: &SemanticModel) {
        for (i, chain) in semantic.signal_chains.iter().enumerate() {
            let node_box_ids: Vec<i64> = chain.nodes.iter().map(|n| n.box_id).collect();
            self.flow_lanes.push(FlowLane {
                lane_id: i,
                hub_id: chain.hub_id,
                node_box_ids,
                terminus_box_id: chain.terminus_box_id,
            });
        }
    }

    // ── Report output ──

    /// Produce a human-readable report.
    pub fn report_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();

        lines.push(format!(
            "[layout-model] SchematicLayoutModel: {} boxes, {} blocks, {} lanes, {} rails, {} buses",
            self.boxes.len(),
            self.functional_blocks.len(),
            self.flow_lanes.len(),
            self.rail_plan.len(),
            self.bus_plan.len(),
        ));

        let mut role_counts: BTreeMap<String, usize> = BTreeMap::new();
        for entry in &self.boxes {
            *role_counts.entry(format!("{:?}", entry.role)).or_default() += 1;
        }
        let role_str: Vec<String> = role_counts
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();
        lines.push(format!("[layout-model] roles: {}", role_str.join(", ")));

        for rail in &self.rail_plan {
            let kind = if rail.is_ground { "GND" } else { "PWR" };
            lines.push(format!(
                "[layout-model] rail {} '{}': {} endpoints, stub={:.1}{}",
                kind,
                rail.name,
                rail.endpoint_count,
                rail.stub_length,
                if rail.is_long_stub { " LONG" } else { "" },
            ));
        }

        for bus in &self.bus_plan {
            lines.push(format!(
                "[layout-model] bus '{}': {} bits, {} nets",
                bus.name,
                bus.width,
                bus.member_net_ids.len(),
            ));
        }

        for warning in &self.warnings {
            lines.push(format!("[layout-model] WARNING: {}", warning));
        }

        lines
    }
}

// ============================================================================
// Box role classification
// ============================================================================

fn classify_box_role(b: &crate::vector::graph::McVecBox) -> BoxLayoutRole {
    use crate::vector::graph::BoxKind;

    if matches!(b.kind, BoxKind::MultiPin) {
        return BoxLayoutRole::Hub;
    }
    if matches!(b.kind, BoxKind::SubModule) {
        return BoxLayoutRole::SubModule;
    }
    if matches!(b.kind, BoxKind::PowerLabel | BoxKind::Dot) {
        return BoxLayoutRole::PowerRail;
    }
    if matches!(b.kind, BoxKind::TwoPin) {
        return BoxLayoutRole::Passive;
    }
    BoxLayoutRole::Unknown
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::IoSummary;
    use crate::vector::graph::{BoxKind, McVecBox, McVecGraph, Symbol};

    fn make_graph() -> McVecGraph {
        let mut graph = McVecGraph::new(1, "test".into());
        let b1 = {
            let mut b = McVecBox::new_v2(
                1,
                "U1".into(),
                "".into(),
                BoxKind::MultiPin,
                Symbol::Ic,
                None,
                None,
                2,
                IoSummary::new(),
            );
            b.x = 100.0;
            b.y = 100.0;
            b.w = 100.0;
            b.h = 80.0;
            b
        };
        let b2 = {
            let mut b = McVecBox::new_v2(
                2,
                "R1".into(),
                "".into(),
                BoxKind::TwoPin,
                Symbol::Resistor,
                None,
                None,
                2,
                IoSummary::new(),
            );
            b.x = 10.0;
            b.y = 10.0;
            b.w = 40.0;
            b.h = 20.0;
            b
        };
        graph.boxes.push(b1);
        graph.boxes.push(b2);
        graph
    }

    #[test]
    fn model_builds_without_panic() {
        let graph = make_graph();
        let semantic = SemanticModel::analyze(&graph);
        let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
        let model = SchematicLayoutModel::build(&graph, &semantic, &special, &[]);
        assert!(!model.boxes.is_empty());
        assert!(model.report_lines().len() > 0);
    }

    #[test]
    fn ic_classified_as_hub() {
        let graph = make_graph();
        let semantic = SemanticModel::analyze(&graph);
        let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
        let model = SchematicLayoutModel::build(&graph, &semantic, &special, &[]);
        let ic = model.boxes.iter().find(|b| b.name == "U1").unwrap();
        assert_eq!(ic.role, BoxLayoutRole::Hub);
    }

    #[test]
    fn resistor_classified_as_passive() {
        let graph = make_graph();
        let semantic = SemanticModel::analyze(&graph);
        let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
        let model = SchematicLayoutModel::build(&graph, &semantic, &special, &[]);
        let r = model.boxes.iter().find(|b| b.name == "R1").unwrap();
        assert_eq!(r.role, BoxLayoutRole::Passive);
    }

    #[test]
    fn model_deterministic() {
        let graph = make_graph();
        let semantic = SemanticModel::analyze(&graph);
        let special = PowerGroundBusModel::analyze(&graph, Some(&semantic));
        let m1 = SchematicLayoutModel::build(&graph, &semantic, &special, &[]);
        let m2 = SchematicLayoutModel::build(&graph, &semantic, &special, &[]);
        assert_eq!(m1.boxes.len(), m2.boxes.len());
        assert_eq!(m1.report_lines(), m2.report_lines());
    }
}
