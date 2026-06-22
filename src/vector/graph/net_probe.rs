// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW —— end-to-end net flow tracing probe (net-flow probe)
//!
//! Fills the blind spot between `vector/builder/debug.rs` (MC_VEC_DUMP) and
//! `viz/debug.rs` (MC_VIZ_DUMP): the pipeline
//! **`McVecBlock.nets` → `McVecGraph.nets` → promote → route** has no
//! reconciliation — nets / endpoints can be **silently dropped or duplicated**
//! across these hops, and no one can see it.
//!
//! This module answers one question: "did the drawing vector layer fully receive
//! the net parsing results?"
//!
//! ## Enable
//! Set the environment variable `MC_NET_PROBE=1` (or any non-empty value other
//! than `0`/`false`). Off by default, zero overhead.
//! Independent of `MC_VEC_DUMP` / `MC_VIZ_DUMP`; each can be turned on separately
//! to debug a specific section.
//!
//! ## Probe points (in data-pipeline order)
//! - [`probe_block_to_graph`] — from_block boundary: `McVecBlock.nets` (input) vs
//!   `McVecGraph.nets` (output). Computes an endpoint id set difference and reports:
//!     * Endpoints in input but not output (DROPPED — net points lost at the drawing layer)
//!     * Endpoints in output but not input (ADDED — usually SPI/bus sub-member expansion, normal)
//!     * Duplicate endpoints within the same VizNet (DUP — catches the double-push bug in `generate_viznets_from_block`)
//!     * Topology (`topology()`) distribution for each VizNet
//! - [`probe_promote`] — promote boundary: per-layer count + names of dropped / orphan nets
//!   (currently `apply_promote_in_place` discards the result via `let _dropped`; here we recover and print it)
//! - [`probe_route`] — after route: how many VizNets didn't get a `Route` (routing failed),
//!   how many Routes are empty (0 segments). audit.rs only checks for overlap, not "never drawn at all".
//!
//! ## Design constraints
//! - Purely read-only, no side effects, never modifies any graph data (as a probe should).
//! - Does not accumulate state on hot paths; scans once at each boundary, off is a bool short-circuit.
//! - Only depends on `vector` layer's own types (`model` / `graph`); does not reverse-depend on `viz`, preserving the dependency graph.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use super::super::model::McVecBlock;
use super::graph_def::McVecGraph;
use super::net_def::VizNet;

// ============================================================================
// enablement check
// ============================================================================

static ENABLED: OnceLock<bool> = OnceLock::new();

/// Whether `MC_NET_PROBE` is enabled
#[inline]
pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| match std::env::var("MC_NET_PROBE") {
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t == "false" || t == "False" || t == "FALSE")
        }
        Err(_) => false,
    })
}

const TAG: &str = "[NET-PROBE]";

// ============================================================================
// Probe A: from_block boundary (McVecBlock.nets → McVecGraph.nets)
// ============================================================================

/// Reconciles nets / endpoints between `McVecBlock` (input) and `McVecGraph` (output).
///
/// Call once **just before** `build_mc_vec_graph` returns (pass the top-level
/// block and the top-level graph; both trees are recursively flattened internally).
pub fn probe_block_to_graph(block: &McVecBlock, graph: &McVecGraph) {
    if !enabled() {
        return;
    }

    // ── 1) Flatten the input side: each real endpoint id → which net names it appears in ──
    let mut in_ids: HashMap<i64, Vec<String>> = HashMap::new();
    let mut in_net_count = 0usize;
    flatten_block(block, &mut in_ids, &mut in_net_count);

    // ── 2) Flatten the output side: real endpoint id set / synthetic endpoint count / duplicate endpoint check ──
    let mut out_ids: HashMap<i64, Vec<String>> = HashMap::new();
    let mut out_net_count = 0usize;
    let mut synthetic_eps = 0usize;
    let mut dup_nets: Vec<(i64, String, usize)> = Vec::new(); // (nid, name, dup_count)
    let mut topo_hist: HashMap<&'static str, usize> = HashMap::new();
    flatten_graph(
        graph,
        &mut out_ids,
        &mut out_net_count,
        &mut synthetic_eps,
        &mut dup_nets,
        &mut topo_hist,
    );

    let in_set: HashSet<i64> = in_ids.keys().copied().collect();
    let out_set: HashSet<i64> = out_ids.keys().copied().collect();

    let dropped: Vec<i64> = in_set.difference(&out_set).copied().collect();
    let added: Vec<i64> = out_set.difference(&in_set).copied().collect();

    // ── 3) Report ──
    eprintln!("{TAG} ══ from_block boundary reconciliation (McVecBlock → McVecGraph) ══");
    eprintln!(
        "{TAG}   nets:      in(McVecNet)={in_net_count}  out(VizNet)={out_net_count}  \
         (out>in is normal: SPI/NtoN splits nets; out<in or out=0 means nets are lost)"
    );
    eprintln!(
        "{TAG}   endpoints: in(distinct real id)={}  out(distinct real id)={}  synthetic(pin_id<0)={}",
        in_set.len(),
        out_set.len(),
        synthetic_eps
    );

    // 3a) Dropped endpoints — this is the core: net points the drawing layer didn't receive
    if dropped.is_empty() {
        eprintln!("{TAG}   ✓ 0 endpoints DROPPED (all input endpoints made it into VizNets)");
    } else {
        eprintln!(
            "{TAG}   ✗ {} endpoint(s) DROPPED — these net points weren't mapped to a box in from_block / were skipped by the double-push logic:",
            dropped.len()
        );
        let mut sample: Vec<i64> = dropped.clone();
        sample.sort_unstable();
        for id in sample.iter().take(40) {
            let nets = in_ids.get(id).map(|v| v.join(",")).unwrap_or_default();
            eprintln!("{TAG}       - id={id}  (from net: {nets})");
        }
        if dropped.len() > 40 {
            eprintln!("{TAG}       ... and {} more", dropped.len() - 40);
        }
    }

    // 3b) Extra endpoints — usually sub-member expansion, normal, but listed for confirmation
    if !added.is_empty() {
        eprintln!(
            "{TAG}   · {} endpoint(s) ADDED (in output, not in input — usually SPI/bus sub-member expansion, expected)",
            added.len()
        );
        let mut sample: Vec<i64> = added.clone();
        sample.sort_unstable();
        for id in sample.iter().take(15) {
            let nets = out_ids.get(id).map(|v| v.join(",")).unwrap_or_default();
            eprintln!("{TAG}       + id={id}  (entered VizNet: {nets})");
        }
        if added.len() > 15 {
            eprintln!("{TAG}       ... and {} more", added.len() - 15);
        }
    }

    // 3c) Duplicate endpoints — directly catches the double-push bug in generate_viznets_from_block
    if dup_nets.is_empty() {
        eprintln!("{TAG}   ✓ 0 VizNets contain duplicate endpoints");
    } else {
        eprintln!(
            "{TAG}   ✗✗ {} VizNet(s) contain *duplicate endpoints* — very likely a double-push in generate_viznets_from_block:",
            dup_nets.len()
        );
        eprintln!(
            "{TAG}      (duplicate endpoints make topology() count a 2-endpoint net as 4 endpoints → misclassify as Star/MultiDriver → wrong routing!)"
        );
        for (nid, name, dups) in dup_nets.iter().take(30) {
            eprintln!("{TAG}       - VizNet #{nid} '{name}': {dups} duplicate endpoint(s)");
        }
        if dup_nets.len() > 30 {
            eprintln!("{TAG}       ... and {} more", dup_nets.len() - 30);
        }
    }

    // 3d) Topology distribution
    let mut topo: Vec<(&&str, &usize)> = topo_hist.iter().collect();
    topo.sort_by_key(|x| *x.0);
    let topo_str: Vec<String> = topo.iter().map(|(k, v)| format!("{k}={v}")).collect();
    eprintln!("{TAG}   topology: [{}]", topo_str.join("  "));
    eprintln!("{TAG} ════════════════════════════════════════════════════");
}

/// Recursively flatten `McVecBlock`: collect each real endpoint id → list of net names, and tally the net count.
fn flatten_block(block: &McVecBlock, ids: &mut HashMap<i64, Vec<String>>, net_count: &mut usize) {
    for net in &block.nets {
        *net_count += 1;
        for id in net.all_point_ids() {
            if id < 0 {
                continue; // input side should not contain synthetic endpoints; defensive skip
            }
            ids.entry(id).or_default().push(net.name.clone());
        }
    }
    for sub in &block.blocks {
        flatten_block(sub, ids, net_count);
    }
}

/// Recursively flatten `McVecGraph`: collect real endpoint ids, synthetic endpoint count, nets with duplicate endpoints, topology histogram.
fn flatten_graph(
    graph: &McVecGraph,
    ids: &mut HashMap<i64, Vec<String>>,
    net_count: &mut usize,
    synthetic: &mut usize,
    dup_nets: &mut Vec<(i64, String, usize)>,
    topo_hist: &mut HashMap<&'static str, usize>,
) {
    for net in &graph.nets {
        *net_count += 1;

        // topology histogram
        let key = topology_key(net);
        *topo_hist.entry(key).or_insert(0) += 1;

        // endpoint collection + duplicate detection within the same net (uniqueness by (box_id, pin_id))
        let mut seen: HashSet<(i64, i64)> = HashSet::new();
        let mut dup = 0usize;
        for ep in &net.endpoints {
            if ep.pin_id < 0 {
                *synthetic += 1;
                continue;
            }
            if !seen.insert((ep.box_id, ep.pin_id)) {
                dup += 1;
            }
            ids.entry(ep.pin_id).or_default().push(net.name.clone());
        }
        if dup > 0 {
            dup_nets.push((net.nid, net.name.clone(), dup));
        }
    }
    for sub in &graph.sub_graphs {
        flatten_graph(sub, ids, net_count, synthetic, dup_nets, topo_hist);
    }
}

fn topology_key(net: &VizNet) -> &'static str {
    use super::net_def::NetTopology::*;
    match net.topology() {
        Isolated => "isolated",
        TwoPoint => "2pt",
        StarOneDriver => "star",
        MultiDriver => "multidriver",
    }
}

// ============================================================================
// Probe B: promote boundary (nets dropped by apply_promote_in_place)
// ============================================================================

/// After promoting one layer, report nets that were dropped (intra-box nets,
/// touching only 1 box) or orphaned (0 box mappings).
///
/// Call this inside `apply_promote_in_place`, right after the result of
/// `classify_nets_by_box_coverage`, to print out information that would
/// otherwise be discarded by `let _dropped = ...`.
pub fn probe_promote(layer: &str, kept: &[VizNet], dropped: &[VizNet], orphan: &[VizNet]) {
    if !enabled() {
        return;
    }
    // When everything is 0, don't spam — only speak up when this layer actually dropped something
    if dropped.is_empty() && orphan.is_empty() {
        return;
    }
    eprintln!(
        "{TAG} [promote] layer '{}': kept={} dropped(intra-box,1 box)={} orphan(0 box)={}",
        layer,
        kept.len(),
        dropped.len(),
        orphan.len()
    );
    for n in dropped.iter().take(20) {
        eprintln!(
            "{TAG}   - dropped: #{} '{}' ({} eps, boxes={:?})",
            n.nid,
            n.name,
            n.endpoint_count(),
            n.box_ids()
        );
    }
    for n in orphan.iter().take(20) {
        eprintln!(
            "{TAG}   ⚠ orphan (residue from failed builder parsing, 0 endpoints land in this layer's boxes): #{} '{}' ({} eps)",
            n.nid,
            n.name,
            n.endpoint_count()
        );
    }
}

// ============================================================================
// Probe C: after route (unrouted / empty-route nets)
// ============================================================================

/// After routing, scan once: how many VizNets should be drawn but didn't get a Route,
/// or have an empty Route.
///
/// Call this in `viz::api::render_layer_recursive` after `route_all_with_channels(...)`,
/// alongside `audit_all(...)` (pass the current layer's graph; recursion is internal).
pub fn probe_route(graph: &McVecGraph) {
    if !enabled() {
        return;
    }
    let mut drawable = 0usize; // endpoint count ≥ 2, should have a wire
    let mut no_route = 0usize; // route == None
    let mut empty_route = 0usize; // route.segments is empty
    let mut routed = 0usize;
    collect_route_stats(
        graph,
        &mut drawable,
        &mut no_route,
        &mut empty_route,
        &mut routed,
    );

    // eprintln!(
    //     "{TAG} [route] drawable(eps≥2)={drawable}  routed={routed}  \
    //      NO-route={no_route}  empty-route(0 seg)={empty_route}"
    // );
    if no_route > 0 || empty_route > 0 {
        // eprintln!(
        //     "{TAG}   ✗ {} nets that should have wires have no valid route — these nets are invisible in the SVG",
        //     no_route + empty_route
        // );
        list_unrouted(graph, 0);
    }
}

fn collect_route_stats(
    graph: &McVecGraph,
    drawable: &mut usize,
    no_route: &mut usize,
    empty_route: &mut usize,
    routed: &mut usize,
) {
    for net in &graph.nets {
        if net.endpoint_count() < 2 {
            continue; // single-endpoint nets don't need wires, don't count as "should be drawn"
        }
        *drawable += 1;
        match &net.route {
            None => *no_route += 1,
            Some(r) if r.segments.is_empty() => *empty_route += 1,
            Some(_) => *routed += 1,
        }
    }
    for sub in &graph.sub_graphs {
        collect_route_stats(sub, drawable, no_route, empty_route, routed);
    }
}

fn list_unrouted(graph: &McVecGraph, depth: usize) {
    let mut shown = 0;
    for net in &graph.nets {
        if net.endpoint_count() < 2 {
            continue;
        }
        let bad = match &net.route {
            None => true,
            Some(r) => r.segments.is_empty(),
        };
        if bad && shown < 20 {
            eprintln!(
                "{TAG}   - [d{}] #{} '{}' ({} eps, {})",
                depth,
                net.nid,
                net.name,
                net.endpoint_count(),
                topology_key(net)
            );
            shown += 1;
        }
    }
    for sub in &graph.sub_graphs {
        list_unrouted(sub, depth + 1);
    }
}
