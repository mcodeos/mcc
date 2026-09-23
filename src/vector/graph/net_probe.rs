// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Net-flow probes — boundary reconciliation along
//! `McVecBlock -> McVecGraph -> promote -> route` (U259, plan
//! `6.22.net-probe-wiring.md`).
//!
//! Three boundaries are wired:
//! - **A** [`probe_block_to_graph`] — did the drawing vector layer receive the
//!   complete net resolution? Reports endpoints dropped between the projected
//!   `McVecBlock` and the built `McVecGraph`, endpoints added by bus/SPI
//!   expansion, duplicate endpoints inside one `VizNet`, and the topology
//!   histogram.
//! - **B** [`probe_promote`] — what `apply_promote_in_place` is about to throw
//!   away (`dropped` / `orphan`). A non-empty `orphan` list means a net whose
//!   endpoints touch no box of the layer — usually a broken box mapping or an
//!   upstream parse residue.
//! - **C** [`probe_route`] — nets that should be drawn but carry no route
//!   (inter-box, >= 2 endpoints, `route == None` or empty segments): invisible
//!   in the final SVG.
//!
//! §6 deep probes: [`probe_stage`] is a one-line endpoint-survival counter,
//! wired around the label/split/route stages of the layout pipeline so a net
//! lost mid-pipeline can be bracketed to one stage.
//!
//! All printing is gated behind the `MC_NET_PROBE` environment variable
//! (non-empty, non-`0`/`false`) — the same gate discipline as `MC_VIZ_DUMP`
//! ([`crate::viz::log`]). The audits themselves are pure and unit-tested; the
//! probe wrappers only add printing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use super::graphdef::McVecGraph;
use super::netdef::{NetTopology, VizNet};
use crate::vector::model::block::McVecBlock;

static PROBE_ENABLED: OnceLock<bool> = OnceLock::new();

/// Whether probe printing is enabled (`MC_NET_PROBE`).
pub fn enabled() -> bool {
    *PROBE_ENABLED.get_or_init(|| match std::env::var("MC_NET_PROBE") {
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t == "false" || t == "False" || t == "FALSE")
        }
        Err(_) => false,
    })
}

/// `eprintln!`-compatible macro that only prints when `MC_NET_PROBE` is set.
macro_rules! nlog {
    ($($arg:tt)*) => {
        if enabled() {
            eprintln!($($arg)*);
        }
    };
}

// ── Probe A: McVecBlock -> McVecGraph ──

/// Reconciliation result of the block-to-graph boundary (probe A).
#[derive(Debug, Default)]
pub struct BlockProbe {
    /// Number of input nets (whole block tree).
    pub nets_in: usize,
    /// Number of output nets (whole graph tree).
    pub nets_out: usize,
    /// Distinct real (id >= 0) endpoint ids entering the boundary.
    pub points_in: usize,
    /// Distinct synthetic (id < 0) endpoint ids entering the boundary.
    pub synthetic_in: usize,
    /// Distinct real endpoint ids leaving the boundary.
    pub points_out: usize,
    /// Distinct synthetic endpoint ids leaving the boundary (rail-synth etc.).
    pub synthetic_out: usize,
    /// Input point ids that never reached any `VizNet` endpoint, each with the
    /// first input net name that carried it.
    pub dropped: Vec<(i64, String)>,
    /// Output endpoint ids no input net carried (split/synthesis additions).
    pub added: Vec<i64>,
    /// Repeated endpoints inside one `VizNet`: `(net name, pin id, count)`.
    /// The historical double-push bug (plan §0) lands here.
    pub dups: Vec<(String, i64, usize)>,
    /// Topology histogram over all output nets.
    pub topology: BTreeMap<&'static str, usize>,
}

impl BlockProbe {
    fn topology_name(t: NetTopology) -> &'static str {
        match t {
            NetTopology::Isolated => "isolated",
            NetTopology::TwoPoint => "2pt",
            NetTopology::StarOneDriver => "star",
            NetTopology::MultiDriver => "multidriver",
        }
    }
}

/// Pure audit of the block-to-graph boundary (probe A). No printing.
pub fn audit_block_to_graph(block: &McVecBlock, graph: &McVecGraph) -> BlockProbe {
    let mut probe = BlockProbe::default();

    // Input side: every point id of the block tree, with a first-carrying net name.
    let mut in_ids: HashSet<i64> = HashSet::new();
    let mut in_owner: HashMap<i64, String> = HashMap::new();
    fn walk_block<'a>(
        b: &'a McVecBlock,
        in_ids: &mut HashSet<i64>,
        in_owner: &mut HashMap<i64, String>,
        nets_in: &mut usize,
    ) {
        for net in &b.nets {
            *nets_in += 1;
            for id in net.all_point_ids() {
                in_owner.entry(id).or_insert_with(|| net.name.clone());
                in_ids.insert(id);
            }
        }
        for sub in &b.blocks {
            walk_block(sub, in_ids, in_owner, nets_in);
        }
    }
    walk_block(block, &mut in_ids, &mut in_owner, &mut probe.nets_in);

    // Output side: every endpoint id of the graph tree + per-net duplicate scan.
    let mut out_ids: HashSet<i64> = HashSet::new();
    fn walk_graph(
        g: &McVecGraph,
        out_ids: &mut HashSet<i64>,
        probe: &mut BlockProbe,
    ) {
        for net in &g.nets {
            probe.nets_out += 1;
            *probe.topology.entry(BlockProbe::topology_name(net.topology())).or_insert(0) += 1;
            let mut seen: HashMap<i64, usize> = HashMap::new();
            for e in &net.endpoints {
                *seen.entry(e.pin_id).or_insert(0) += 1;
                out_ids.insert(e.pin_id);
            }
            for (pid, n) in seen {
                if n > 1 {
                    probe.dups.push((net.name.clone(), pid, n));
                }
            }
        }
        for sub in &g.sub_graphs {
            walk_graph(sub, out_ids, probe);
        }
    }
    walk_graph(graph, &mut out_ids, &mut probe);

    probe.points_in = in_ids.iter().filter(|&&id| id >= 0).count();
    probe.synthetic_in = in_ids.iter().filter(|&&id| id < 0).count();
    probe.points_out = out_ids.iter().filter(|&&id| id >= 0).count();
    probe.synthetic_out = out_ids.iter().filter(|&&id| id < 0).count();

    probe.dropped = in_ids
        .difference(&out_ids)
        .copied()
        .map(|id| (id, in_owner.get(&id).cloned().unwrap_or_default()))
        .collect();
    probe.dropped.sort_unstable_by_key(|(id, _)| *id);
    probe.added = out_ids.difference(&in_ids).copied().collect();
    probe.added.sort_unstable();
    probe.dups.sort_unstable();
    probe
}

/// Probe A: print the block-to-graph reconciliation (MC_NET_PROBE only).
pub fn probe_block_to_graph(block: &McVecBlock, graph: &McVecGraph) {
    if !enabled() {
        return;
    }
    let p = audit_block_to_graph(block, graph);
    nlog!("[NET-PROBE] == from_block boundary (McVecBlock -> McVecGraph) ==");
    nlog!(
        "[NET-PROBE]   nets:      in(McVecNet)={}  out(VizNet)={}  (out>in can be normal: SPI/NtoN split)",
        p.nets_in, p.nets_out
    );
    nlog!(
        "[NET-PROBE]   endpoints: in(distinct real)={}  out(distinct real)={}  synthetic(in)={}  (out)={}",
        p.points_in, p.points_out, p.synthetic_in, p.synthetic_out
    );
    if p.dropped.is_empty() {
        nlog!("[NET-PROBE]   ok 0 endpoints DROPPED (every input point reached a VizNet)");
    } else {
        nlog!("[NET-PROBE]   !! {} endpoint(s) DROPPED:", p.dropped.len());
        for (id, net) in p.dropped.iter().take(20) {
            nlog!("[NET-PROBE]        point {id} (net '{net}')");
        }
        if p.dropped.len() > 20 {
            nlog!("[NET-PROBE]        ... and {} more", p.dropped.len() - 20);
        }
    }
    if p.added.is_empty() {
        nlog!("[NET-PROBE]   ok 0 endpoints ADDED");
    } else {
        nlog!(
            "[NET-PROBE]   .  {} endpoint(s) ADDED (split/synthesis expansion, expected)",
            p.added.len()
        );
    }
    if p.dups.is_empty() {
        nlog!("[NET-PROBE]   ok 0 VizNet contains duplicate endpoints");
    } else {
        nlog!("[NET-PROBE]   !! {} VizNet duplicate endpoint(s):", p.dups.len());
        for (name, pid, n) in p.dups.iter().take(20) {
            nlog!("[NET-PROBE]        net '{name}' point {pid} x{n}");
        }
    }
    let hist: Vec<String> = p.topology.iter().map(|(k, v)| format!("{k}={v}")).collect();
    nlog!("[NET-PROBE]   topology: [{}]", hist.join("  "));
    nlog!("[NET-PROBE] ===============================================");
}

// ── Probe B: promote boundary ──

/// Reconciliation result of the promote boundary (probe B).
#[derive(Debug, Default)]
pub struct PromoteProbe {
    /// Layer name the promotion ran on.
    pub layer: String,
    /// Nets kept (>= 1 box mapping in this layer).
    pub kept: usize,
    /// Nets dropped by the coverage classifier: `(name, endpoints)`.
    pub dropped: Vec<(String, usize)>,
    /// Orphan nets (0 box mappings): `(name, endpoints)`. Non-empty is a
    /// warning: the net touches no box of this layer at all.
    pub orphan: Vec<(String, usize)>,
}

/// Pure audit of the promote boundary (probe B). No printing.
pub fn audit_promote(
    layer: &str,
    kept: &[VizNet],
    dropped: &[VizNet],
    orphan: &[VizNet],
) -> PromoteProbe {
    PromoteProbe {
        layer: layer.to_string(),
        kept: kept.len(),
        dropped: dropped
            .iter()
            .map(|n| (n.name.clone(), n.endpoint_count()))
            .collect(),
        orphan: orphan
            .iter()
            .map(|n| (n.name.clone(), n.endpoint_count()))
            .collect(),
    }
}

/// Probe B: print what promote is about to discard (MC_NET_PROBE only).
pub fn probe_promote(layer: &str, kept: &[VizNet], dropped: &[VizNet], orphan: &[VizNet]) {
    if !enabled() {
        return;
    }
    let p = audit_promote(layer, kept, dropped, orphan);
    nlog!(
        "[NET-PROBE]   promote '{}': kept={} dropped={} orphan={}",
        p.layer, p.kept, p.dropped.len(), p.orphan.len()
    );
    for (name, eps) in &p.orphan {
        nlog!("[NET-PROBE]     !! orphan net '{name}' ({eps} endpoint(s), 0 box mapping)");
    }
    for (name, eps) in &p.dropped {
        nlog!("[NET-PROBE]     .  dropped net '{name}' ({eps} endpoint(s))");
    }
}

// ── Probe C: post-route visibility ──

/// Reconciliation result of the post-route boundary (probe C).
#[derive(Debug, Default)]
pub struct RouteProbe {
    /// Total nets in this layer.
    pub nets: usize,
    /// Nets with a non-empty route.
    pub routed: usize,
    /// Inter-box nets with >= 2 endpoints but no drawable route:
    /// `(nid, name, endpoints)` — invisible in the SVG.
    pub unrouted: Vec<(i64, String, usize)>,
    /// Intra-box unrouted nets (single box; usually intentionally not drawn).
    pub unrouted_intra_box: usize,
}

/// Pure audit of the post-route boundary (probe C). No printing.
pub fn audit_route(graph: &McVecGraph) -> RouteProbe {
    let mut p = RouteProbe {
        nets: graph.nets.len(),
        ..RouteProbe::default()
    };
    for net in &graph.nets {
        let has_route = net
            .route
            .as_ref()
            .is_some_and(|r| !r.segments.is_empty());
        if has_route {
            p.routed += 1;
        } else if net.is_inter_box() && net.endpoint_count() >= 2 {
            p.unrouted.push((net.nid, net.name.clone(), net.endpoint_count()));
        } else {
            p.unrouted_intra_box += 1;
        }
    }
    p
}

/// Probe C: print nets that should be drawn but carry no route (MC_NET_PROBE
/// only). Reports the current layer only — the caller owns sub-graph recursion.
pub fn probe_route(graph: &McVecGraph) {
    if !enabled() {
        return;
    }
    let p = audit_route(graph);
    nlog!(
        "[NET-PROBE]   route '{}': nets={} routed={} unrouted(inter-box)={} unrouted(intra-box)={}",
        graph.name, p.nets, p.routed, p.unrouted.len(), p.unrouted_intra_box
    );
    for (nid, name, eps) in p.unrouted.iter().take(20) {
        nlog!("[NET-PROBE]     !! net {nid} '{name}' ({eps} endpoint(s)) has no route — invisible in SVG");
    }
}

// ── §6 deep probe: endpoint survival per stage ──

/// One-line endpoint-survival counter for a pipeline stage (§6 deep probe).
pub fn probe_stage(graph: &McVecGraph, stage: &str) {
    if !enabled() {
        return;
    }
    let endpoints: usize = graph.nets.iter().map(|n| n.endpoint_count()).sum();
    let unrouted = graph
        .nets
        .iter()
        .filter(|n| {
            n.route.as_ref().map_or(true, |r| r.segments.is_empty())
                && n.is_inter_box()
                && n.endpoint_count() >= 2
        })
        .count();
    nlog!(
        "[NET-PROBE]   stage '{stage}': nets={} endpoints={} unrouted(inter-box)={}",
        graph.nets.len(),
        endpoints,
        unrouted
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::kinds::NetKind;
    use crate::vector::graph::netdef::{EndpointRef, NetRole, Route};
    use crate::vector::model::net::McVecNet;
    use crate::vector::model::vec::McVec;

    fn block_with_net(nid: i64, name: &str, ids: Vec<i64>) -> McVecBlock {
        let mut b = McVecBlock::new(1, "top".to_string());
        b.nets.push(McVecNet::new(
            nid,
            name.to_string(),
            vec![McVec::new(ids)],
        ));
        b
    }

    fn graph_with_net(nid: i64, name: &str, endpoints: Vec<EndpointRef>) -> McVecGraph {
        let mut g = McVecGraph::new(1, "top".to_string());
        g.nets.push(VizNet::new(
            nid,
            name.to_string(),
            NetKind::Signal,
            NetRole::Signal,
            endpoints,
        ));
        g
    }

    #[test]
    fn dropped_endpoints_are_reported_with_their_net() {
        let block = block_with_net(1, "V5V", vec![3, 5, 7]);
        let graph = graph_with_net(
            1,
            "V5V",
            vec![EndpointRef::new(10, 3, "a"), EndpointRef::new(11, 7, "b")],
        );
        let p = audit_block_to_graph(&block, &graph);
        assert_eq!(p.points_in, 3);
        assert_eq!(p.points_out, 2);
        assert_eq!(p.dropped, vec![(5, "V5V".to_string())]);
        assert!(p.added.is_empty());
        assert!(p.dups.is_empty());
    }

    #[test]
    fn added_endpoints_from_expansion_are_reported() {
        let block = block_with_net(1, "bus", vec![3, 5]);
        let graph = graph_with_net(
            1,
            "bus",
            vec![
                EndpointRef::new(10, 3, "a"),
                EndpointRef::new(11, 5, "b"),
                EndpointRef::new(12, 99, "c"),
            ],
        );
        let p = audit_block_to_graph(&block, &graph);
        assert!(p.dropped.is_empty());
        assert_eq!(p.added, vec![99]);
    }

    #[test]
    fn duplicate_endpoints_inside_one_viznet_are_caught() {
        let block = block_with_net(1, "V5V", vec![3, 5]);
        let graph = graph_with_net(
            1,
            "V5V",
            vec![
                EndpointRef::new(10, 3, "a"),
                EndpointRef::new(10, 3, "a"),
                EndpointRef::new(11, 5, "b"),
            ],
        );
        let p = audit_block_to_graph(&block, &graph);
        assert_eq!(p.dups, vec![("V5V".to_string(), 3, 2)]);
    }

    #[test]
    fn topology_histogram_counts_each_shape_once_per_net() {
        let block = block_with_net(1, "V5V", vec![3, 5, 7]);
        let graph = graph_with_net(
            1,
            "V5V",
            vec![
                EndpointRef::new(10, 3, "a"),
                EndpointRef::new(11, 5, "b"),
                EndpointRef::new(12, 7, "c"),
            ],
        );
        let p = audit_block_to_graph(&block, &graph);
        assert_eq!(p.topology.get("star"), Some(&1));
        assert_eq!(p.nets_in, 1);
        assert_eq!(p.nets_out, 1);
    }

    #[test]
    fn promote_probe_counts_kept_dropped_orphan() {
        let kept = VizNet::new(
            1,
            "a".to_string(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::new(1, 10, "a"),
                EndpointRef::new(2, 11, "b"),
            ],
        );
        let orphan = VizNet::new(
            2,
            "ghost".to_string(),
            NetKind::Signal,
            NetRole::Signal,
            vec![EndpointRef::new(77, 12, "x")],
        );
        let p = audit_promote("L1", &[kept], &[], &[orphan]);
        assert_eq!(p.kept, 1);
        assert!(p.dropped.is_empty());
        assert_eq!(
            p.orphan,
            vec![("ghost".to_string(), 1)]
        );
    }

    #[test]
    fn route_probe_lists_inter_box_nets_without_a_route() {
        let mut unrouted = graph_with_net(
            1,
            "sig",
            vec![
                EndpointRef::new(1, 10, "a"),
                EndpointRef::new(2, 11, "b"),
            ],
        );
        // A route with no segments still counts as unrouted.
        unrouted.nets[0].route = Some(Route::new());
        let mut routed = graph_with_net(
            2,
            "ok",
            vec![
                EndpointRef::new(1, 12, "a"),
                EndpointRef::new(2, 13, "b"),
            ],
        );
        routed.nets[0].route = Some(Route {
            segments: vec![super::super::netdef::Segment {
                from: super::super::netdef::Point { x: 0.0, y: 0.0 },
                to: super::super::netdef::Point { x: 1.0, y: 1.0 },
            }],
            junctions: vec![],
            escalated: false,
        });
        // Intra-box net (single box): counted separately, not listed.
        let mut g = McVecGraph::new(1, "layer".to_string());
        g.nets.push(unrouted.nets.remove(0));
        g.nets.push(routed.nets.remove(0));
        g.nets.push(VizNet::new(
            3,
            "intra".to_string(),
            NetKind::Signal,
            NetRole::Signal,
            vec![
                EndpointRef::new(1, 14, "a"),
                EndpointRef::new(1, 15, "b"),
            ],
        ));
        let p = audit_route(&g);
        assert_eq!(p.nets, 3);
        assert_eq!(p.routed, 1);
        assert_eq!(p.unrouted, vec![(1, "sig".to_string(), 2)]);
        assert_eq!(p.unrouted_intra_box, 1);
    }
}
