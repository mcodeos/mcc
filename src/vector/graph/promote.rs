// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW -- cross-layer net promotion (core algorithm for top-level "simplest integration")
//!
//! ## User's original words
//! > The outermost layer describes the simplest connections; it's a highest-level integration
//! > that connects every module together.
//!
//! ## What this file solves
//! Currently drawing places all nets in every layer, causing the top-level view to be drowned
//! by the sub-modules' internal capacitors, crystals, and reset circuits. This algorithm keeps
//! only "cross-sub-module" nets at the top layer and folds internal nets into the expanded layer.
//!
//! ## Algorithm
//! Given one layer [`McVecGraph`](super::graph_def::McVecGraph) and its boxes, classify each
//! [`VizNet`] as follows:
//!
//! - **inter-box**: endpoints span >= 2 boxes of this layer -> **keep**, and `kind` is merged via
//!   [`merge_net_kinds`] (★ P08: Power/Ground no longer overridden by SubModuleIO)
//! - **intra-box**: all endpoints within 1 box -> **drop down to sub-layer**
//! - **orphan**: 0 endpoints map to any box of this layer -> skip
//!
//! "Drop down to sub-layer" is implemented by the caller -- this algorithm only **annotates**,
//! it does not actually move data.
//!
//! ## ★ P08 (S4) Changes
//! Previously `apply_promote_in_place` hard-overwrote the `kind` of all inter-box nets to
//! `SubModuleIO`, causing top-level nets like "VCC connects to mcu513" to no longer be recognized
//! as power nets, and the hierarchical layout's "power on top" promise failed on the spot.
//! P08 now uses [`merge_net_kinds`] to merge: Power/Ground take priority and won't be overridden
//! by SubModuleIO.
//!
//! ## Usage
//! ```ignore
//! let mut top_graph = build_mc_vec_graph(&top_block, &table);
//!
//! // Promote: top layer keeps only inter-box nets
//! let promoted = promote_to_inter_box_only(&top_graph);
//! top_graph.nets = promoted.kept;     // top layer keeps only inter-box nets
//! // Nets in promoted.dropped will appear in sub-layer graphs (handled recursively)
//! ```

use std::collections::HashSet;

use super::graph_def::McVecGraph;
use super::kinds::NetKind;
use super::net_def::{EndpointRef, VizNet};

// ============================================================================
// Promotion result
// ============================================================================

/// Return value of `promote_to_inter_box_only`
#[derive(Debug)]
pub struct PromoteResult {
    /// Inter-box nets (kept in current layer)
    pub kept: Vec<VizNet>,
    /// Intra-box nets (should drop down to sub-layer)
    pub dropped: Vec<VizNet>,
    /// Orphan nets (0 endpoints map to this layer) -- usually indicates data issues
    pub orphan: Vec<VizNet>,
}

impl PromoteResult {
    /// Output statistics line (for debugging scenarios like `MC_VEC_DUMP=1`)
    pub fn summary(&self) -> String {
        format!(
            "[promote] kept={} dropped={} orphan={}",
            self.kept.len(),
            self.dropped.len(),
            self.orphan.len()
        )
    }
}

// ============================================================================
// Main API
// ============================================================================

/// Split a layer's nets into "inter-box keep" / "intra-box drop" / "orphan"
///
/// Does not modify `graph`, returns classification as a pure function result.
pub fn promote_to_inter_box_only(graph: &McVecGraph) -> PromoteResult {
    let box_ids: HashSet<i64> = graph.boxes.iter().map(|b| b.id).collect();
    classify_nets_by_box_coverage(&graph.nets, &box_ids)
}

/// Promote one layer: remove intra-box nets, merge NetKind of inter-box nets via [`merge_net_kinds`]
/// (Power/Ground priority, otherwise downgraded to SubModuleIO)
///
/// Does not recurse into sub-layers -- the caller decides the sub-layer handling strategy
/// (typically each sub-layer promotes itself).
///
/// ## ★ P08 (S4) Changes
/// Previously all inter-box nets were rewritten to `NetKind::SubModuleIO`, causing VCC/GND
/// cross-sub-module power nets to be downgraded, and hierarchical layout losing the "this is
/// power" signal. Now [`merge_net_kinds`] merges SubModuleIO with the original kind: Power/Ground/
/// Bus win, Signal/Unknown degrade to SubModuleIO.
///
/// # Side effects
/// - `graph.nets` is replaced with only inter-box nets
/// - The `kind` field of inter-box nets is merged by [`merge_net_kinds`] (not hard-overwritten)
/// - Returns the list of dropped nets (callers can use this to drop down to sub-layers)
pub fn apply_promote_in_place(graph: &mut McVecGraph) -> Vec<VizNet> {
    let box_ids: HashSet<i64> = graph.boxes.iter().map(|b| b.id).collect();
    let result = classify_nets_by_box_coverage(&graph.nets, &box_ids);

    // ★ NEW: hand the to-be-dropped dropped/orphan nets to the probe (only prints when MC_NET_PROBE is enabled)
    super::net_probe::probe_promote(&graph.name, &result.kept, &result.dropped, &result.orphan);

    // ★ P08: use merge_net_kinds to merge, instead of hard-overwriting to SubModuleIO
    let mut kept = result.kept;
    for n in &mut kept {
        n.kind = merge_net_kinds(n.kind.clone(), NetKind::SubModuleIO);
    }

    graph.nets = kept;
    // Orphans are discarded together (they're usually artifacts of failed builder parsing, shouldn't enter the graph)
    result.dropped
}

/// Recursively promote the entire graph tree
///
/// The top layer only shows inter-box connections; each sub-layer also independently promotes
/// (its "inter-box" means connections among its own children). Suitable as one step in the
/// viz pipeline.
pub fn apply_promote_recursive(graph: &mut McVecGraph) {
    let _dropped = apply_promote_in_place(graph);
    for sub in &mut graph.sub_graphs {
        apply_promote_recursive(sub);
    }
}

// ============================================================================
// ★ P08 (S4) merge_net_kinds -- merge NetKind during promote instead of hard-overwrite
// ============================================================================

/// Merge two NetKinds, priority from high to low:
///
/// 1. **Power** (either is Power -> Power wins)
/// 2. **Ground** (either is Ground -> Ground wins)
/// 3. **Bus(_)** (either is Bus -> that Bus wins, a preferred)
/// 4. **Signal** (either is Signal -> Signal wins)
/// 5. **SubModuleIO** (fallback)
///
/// ## Design motivation (P08)
/// During the promote phase, when sub-layer nets are promoted to the top layer, the kind was
/// previously hardcoded to `SubModuleIO`, causing the top layer's VCC/GND/I2C nets (originally
/// Power/Ground/Bus) to be downgraded, and `HierarchicalLayouter::categorize_boxes` losing the
/// "this is power" signal. The "power on top, ground on bottom" promise fails on the spot.
///
/// This merge rule guarantees: when the sub-layer is Power/Ground, after promotion to the top
/// layer it remains Power/Ground, even if the caller passes `SubModuleIO` to override, Power/
/// Ground's priority will reverse it.
///
/// ## Usage
/// ```ignore
/// let merged = merge_net_kinds(NetKind::Power, NetKind::SubModuleIO);
/// assert_eq!(merged, NetKind::Power);   // Power wins
///
/// let merged = merge_net_kinds(NetKind::Signal, NetKind::SubModuleIO);
/// assert_eq!(merged, NetKind::SubModuleIO);  // Signal doesn't beat SubModuleIO? see rule
/// ```
///
/// Note: rule 4 (Signal vs SubModuleIO) -- we prefer SubModuleIO, because
/// "this net crosses modules" is more specific than "this is an ad-hoc signal", subsequent
/// router can use more suitable strategy when seeing SubModuleIO. But if P11 dispatch shows
/// they perform the same, this rule can be safely flipped.
pub fn merge_net_kinds(a: NetKind, b: NetKind) -> NetKind {
    // Rule 1: Power has highest priority
    if a == NetKind::Power || b == NetKind::Power {
        return NetKind::Power;
    }
    // Rule 2: Ground next
    if a == NetKind::Ground || b == NetKind::Ground {
        return NetKind::Ground;
    }
    // Rule 3: Bus third (a preferred)
    if let NetKind::Bus(_) = &a {
        return a;
    }
    if let NetKind::Bus(_) = &b {
        return b;
    }
    // Rule 4: SubModuleIO is more specific than Signal (cross-module semantics are clearer)
    if a == NetKind::SubModuleIO || b == NetKind::SubModuleIO {
        return NetKind::SubModuleIO;
    }
    // Rule 5: Fallback Signal
    NetKind::Signal
}

// ============================================================================
// Internal classification logic
// ============================================================================

fn classify_nets_by_box_coverage(nets: &[VizNet], layer_box_ids: &HashSet<i64>) -> PromoteResult {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    let mut orphan = Vec::new();

    for net in nets {
        let mapped: HashSet<i64> = net
            .endpoints
            .iter()
            .map(|e| e.box_id)
            .filter(|id| layer_box_ids.contains(id))
            .collect();

        match mapped.len() {
            0 => orphan.push(net.clone()),
            1 => dropped.push(net.clone()),
            _ => kept.push(net.clone()),
        }
    }

    PromoteResult {
        kept,
        dropped,
        orphan,
    }
}

// ============================================================================
// Helper: promote endpoint to sub-module port (instead of internal pin)
// ============================================================================

/// "Promote" a net's endpoints to sub-module ports
///
/// When a cross-module net connects to a specific pin inside a sub-module (e.g. `mcu513.pin42`),
/// the top-level view should display this endpoint as the corresponding sub-module port
/// (e.g. `mcu513.UART_TX`), rather than leaking the internal pin name.
///
/// This function currently only does **box_id truncation** (assign endpoints to the top-level
/// box they belong to). True "port name remapping" requires InstTable to provide `pin -> port`
/// mapping, to be implemented in P2/P3 phase.
///
/// # Current behavior
/// - For each endpoint, if its `box_id` is in `layer_box_ids`, keep it unchanged
/// - Otherwise try to walk up to find ancestor box_id (with `parent_lookup`)
/// - Endpoints that fail the walk-up are dropped
pub fn lift_endpoints_to_layer_boxes(
    net: &VizNet,
    layer_box_ids: &HashSet<i64>,
    parent_lookup: impl Fn(i64) -> Option<i64>,
) -> Vec<EndpointRef> {
    net.endpoints
        .iter()
        .filter_map(|e| {
            if layer_box_ids.contains(&e.box_id) {
                return Some(e.clone());
            }
            // Walk up to find ancestor
            let mut cur = e.box_id;
            loop {
                match parent_lookup(cur) {
                    Some(parent_id) => {
                        if layer_box_ids.contains(&parent_id) {
                            return Some(EndpointRef {
                                box_id: parent_id,
                                pin_id: e.pin_id,
                                pin_name: e.pin_name.clone(),
                                io_type: e.io_type, // P03: inherit io_type from original endpoint
                                pin_number: e.pin_number, // P01: inherit pin_number
                            });
                        }
                        cur = parent_id;
                    }
                    None => return None,
                }
            }
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::box_def::{IoSummary, McVecBox};
    use crate::vector::graph::kinds::{BoxKind, NetKind};

    fn mk_box(id: i64, kind: BoxKind) -> McVecBox {
        McVecBox::new(
            id,
            format!("box{}", id),
            String::new(),
            kind,
            1,
            IoSummary::new(),
        )
    }

    fn mk_net(nid: i64, name: &str, endpoints: Vec<(i64, i64)>) -> VizNet {
        VizNet::new(
            nid,
            name.to_string(),
            NetKind::Signal,
            endpoints
                .into_iter()
                .map(|(box_id, pin_id)| EndpointRef::new(box_id, pin_id, ""))
                .collect(),
        )
    }

    #[test]
    fn test_inter_box_kept_intra_dropped() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_box(1, BoxKind::SubModule));
        g.boxes.push(mk_box(2, BoxKind::SubModule));
        // net A: crosses box 1 <-> 2 -> kept
        g.nets.push(mk_net(101, "A", vec![(1, 11), (2, 21)]));
        // net B: entirely in box 1 -> dropped
        g.nets.push(mk_net(102, "B", vec![(1, 12), (1, 13)]));
        // net C: endpoints not in any box of this layer -> orphan
        g.nets.push(mk_net(103, "C", vec![(99, 991)]));

        let r = promote_to_inter_box_only(&g);
        assert_eq!(r.kept.len(), 1);
        assert_eq!(r.kept[0].name, "A");
        assert_eq!(r.dropped.len(), 1);
        assert_eq!(r.dropped[0].name, "B");
        assert_eq!(r.orphan.len(), 1);
        assert_eq!(r.orphan[0].name, "C");
    }

    #[test]
    fn test_apply_in_place_signal_promoted_to_submodule_io() {
        // ★ P08 (S4): kind=Signal cross-module net still downgrades to SubModuleIO
        // (because merge_net_kinds rule 4: SubModuleIO is more specific than Signal)
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_box(1, BoxKind::SubModule));
        g.boxes.push(mk_box(2, BoxKind::SubModule));
        g.nets.push(mk_net(101, "data", vec![(1, 11), (2, 21)]));

        let dropped = apply_promote_in_place(&mut g);
        assert_eq!(dropped.len(), 0);
        assert_eq!(g.nets.len(), 1);
        assert_eq!(g.nets[0].kind, NetKind::SubModuleIO);
    }

    #[test]
    fn test_apply_in_place_preserves_power_kind() {
        // ★ P08 (S4): kind=Power cross-module net remains Power after promotion, no longer overridden
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_box(1, BoxKind::SubModule));
        g.boxes.push(mk_box(2, BoxKind::SubModule));
        let mut vcc = mk_net(101, "VCC", vec![(1, 11), (2, 21)]);
        vcc.kind = NetKind::Power;
        g.nets.push(vcc);

        apply_promote_in_place(&mut g);
        assert_eq!(g.nets.len(), 1);
        assert_eq!(
            g.nets[0].kind,
            NetKind::Power,
            "Power net should NOT be downgraded to SubModuleIO"
        );
    }

    #[test]
    fn test_apply_in_place_preserves_ground_kind() {
        // ★ P08 (S4): Ground same as above
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_box(1, BoxKind::SubModule));
        g.boxes.push(mk_box(2, BoxKind::SubModule));
        let mut gnd = mk_net(101, "GND", vec![(1, 11), (2, 21)]);
        gnd.kind = NetKind::Ground;
        g.nets.push(gnd);

        apply_promote_in_place(&mut g);
        assert_eq!(g.nets[0].kind, NetKind::Ground);
    }

    #[test]
    fn test_apply_in_place_preserves_bus_kind() {
        // ★ P08 (S4): Bus same as above
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_box(1, BoxKind::SubModule));
        g.boxes.push(mk_box(2, BoxKind::SubModule));
        let mut bus = mk_net(101, "data", vec![(1, 11), (2, 21)]);
        bus.kind = NetKind::Bus(8);
        g.nets.push(bus);

        apply_promote_in_place(&mut g);
        assert_eq!(g.nets[0].kind, NetKind::Bus(8));
    }

    // ============================================================================
    // ★ P08 (S4) merge_net_kinds unit tests
    // ============================================================================

    #[test]
    fn merge_power_wins_over_submodule_io() {
        assert_eq!(
            merge_net_kinds(NetKind::Power, NetKind::SubModuleIO),
            NetKind::Power
        );
        assert_eq!(
            merge_net_kinds(NetKind::SubModuleIO, NetKind::Power),
            NetKind::Power
        );
    }

    #[test]
    fn merge_power_wins_over_signal() {
        assert_eq!(
            merge_net_kinds(NetKind::Power, NetKind::Signal),
            NetKind::Power
        );
    }

    #[test]
    fn merge_ground_wins_over_submodule_io() {
        assert_eq!(
            merge_net_kinds(NetKind::Ground, NetKind::SubModuleIO),
            NetKind::Ground
        );
        assert_eq!(
            merge_net_kinds(NetKind::SubModuleIO, NetKind::Ground),
            NetKind::Ground
        );
    }

    #[test]
    fn merge_power_wins_over_ground() {
        // Edge case: shouldn't happen (a net is both power and ground),
        // but rule is consistent: Power ranks above Ground
        assert_eq!(
            merge_net_kinds(NetKind::Power, NetKind::Ground),
            NetKind::Power
        );
    }

    #[test]
    fn merge_bus_wins_over_signal_and_submodule_io() {
        assert_eq!(
            merge_net_kinds(NetKind::Bus(4), NetKind::Signal),
            NetKind::Bus(4)
        );
        assert_eq!(
            merge_net_kinds(NetKind::Bus(8), NetKind::SubModuleIO),
            NetKind::Bus(8)
        );
        // But Power still beats Bus
        assert_eq!(
            merge_net_kinds(NetKind::Bus(4), NetKind::Power),
            NetKind::Power
        );
    }

    #[test]
    fn merge_signal_with_submodule_io_yields_submodule_io() {
        // Rule 4: SubModuleIO is more specific than Signal
        assert_eq!(
            merge_net_kinds(NetKind::Signal, NetKind::SubModuleIO),
            NetKind::SubModuleIO
        );
    }

    #[test]
    fn merge_signal_signal_yields_signal() {
        assert_eq!(
            merge_net_kinds(NetKind::Signal, NetKind::Signal),
            NetKind::Signal
        );
    }

    #[test]
    fn merge_submodule_io_submodule_io_yields_submodule_io() {
        assert_eq!(
            merge_net_kinds(NetKind::SubModuleIO, NetKind::SubModuleIO),
            NetKind::SubModuleIO
        );
    }

    #[test]
    fn test_recursive_apply() {
        let mut g = McVecGraph::new(0, "main".into());
        g.boxes.push(mk_box(1, BoxKind::SubModule));
        g.boxes.push(mk_box(2, BoxKind::SubModule));
        // Top layer nets
        g.nets.push(mk_net(101, "top_net", vec![(1, 11), (2, 21)]));

        // Sub-graph (inside box 1)
        let mut sub = McVecGraph::new(1, "sub".into());
        sub.boxes.push(mk_box(11, BoxKind::TwoPin));
        sub.boxes.push(mk_box(12, BoxKind::TwoPin));
        sub.nets
            .push(mk_net(201, "sub_net", vec![(11, 111), (12, 121)]));
        g.sub_graphs.push(sub);

        apply_promote_recursive(&mut g);
        assert_eq!(g.nets.len(), 1, "top layer keeps inter-module net");
        assert_eq!(
            g.sub_graphs[0].nets.len(),
            1,
            "sub-layer keeps its own inter-module net"
        );
    }
}
