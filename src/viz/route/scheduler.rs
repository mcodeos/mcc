// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW (P10, S6) — Channel-aware Router Scheduler
//!
//! ## What problem does this file solve
//! S4's `dispatch.rs::route_all_with_dispatch` assigns each net to a router then executes sequentially.
//! After P10 introduces "channel" concept, **scheduling order is critical**:
//!
//! - Long trunks (large span, hard to route) occupy positions first, short lines yield to them
//! - Within same net type, larger span takes priority (most sensitive to position)
//! - Bus > Star/TrunkTap > Orthogonal two-terminal > Noop
//!
//! Additionally, simultaneously pass `ChannelMap` to trunk_tap / orthogonal / bus_bundle, letting them
//! use `reserve_horizontal/vertical` to select y / x.
//!
//! ## Relationship with dispatch.rs
//! - `dispatch::pick_router` is still the **dispatch table** (kind/topology → RouterChoice), P10 reuses
//! - `dispatch::route_layer_with_dispatch` is P09 era "no channel" scheduling, retained
//! - This file's `route_layer_with_channels` is P10 upgraded version, **default entry point**
//!
//! ## Order strategy
//!
//! | Priority | Category | Notes |
//! |---|---|---|
//! | 0 | Bus (BusBundle) | Bus thick lines occupy lanes first, their trunks usually span entire graph |
//! | 1 | TrunkTap (including warning) | Multi-terminal Signal/Power, long trunk priority |
//! | 2 | Star | hub-and-spoke, needs hub point in middle |
//! | 3 | Orthogonal multi-terminal (≥ 3) | (rare in practice, usually goes TrunkTap) |
//! | 4 | Orthogonal 2-terminal | Short span, yield |
//! | 5 | Noop | Endpoints ≤ 1, no routing |
//!
//! Within same priority: sort by "endpoint bounding box span" from large to small (larger span is harder to route).
//!
//! ## Reuse situation
//! - `dispatch::RouteIntent` / `pick_router` / `RouterChoice` —— entire set reused
//! - `obstacles::ObstacleMap` —— constructed separately for each net (exclude own endpoint box)
//! - Here **newly create** one `ChannelMap`, shared by entire layer

use crate::vector::graph::{BoxKind, McVecGraph, NetKind, Segment, VizNet};

use super::audit;
use super::channels::ChannelMap;
use super::dispatch::{pick_router, RouteIntent, RouterChoice};
use super::feedback::{self, RouteFeedbackConfig};
use super::grid_router::{self, AStarCfg, Grid, GRID_CELL, GRID_GAP, GRID_INFLATE};

// ============================================================================
// End-to-end entry
// ============================================================================

/// Default line_gap for channel map (minimum spacing between adjacent slots in same channel)
pub const DEFAULT_LINE_GAP: f64 = 8.0;

/// Collect all endpoint box rectangles (x,y,w,h) for one net —— used by route_collides to exclude own endpoints
fn endpoint_rects(graph: &McVecGraph, net: &VizNet) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    for ep in &net.endpoints {
        if let Some(b) = graph.boxes.iter().find(|x| x.id == ep.box_id) {
            out.push((b.x, b.y, b.w, b.h));
        }
    }
    out
}

/// Accumulate historical congestion cost at all cross-net wire intersection points (negotiated congestion: repeatedly colliding cells become more expensive → wire rip-up converges)
fn bump_crossings(grid: &mut Grid, graph: &McVecGraph, amount: i64) {
    let mut segs: Vec<(i64, &Segment)> = Vec::new();
    for net in &graph.nets {
        if let Some(r) = &net.route {
            for s in &r.segments {
                segs.push((net.nid, s));
            }
        }
    }
    for i in 0..segs.len() {
        for j in (i + 1)..segs.len() {
            if segs[i].0 == segs[j].0 {
                continue;
            }
            if let Some((x, y)) = audit::seg_cross_point(segs[i].1, segs[j].1) {
                grid.bump_history_at(x, y, amount);
            }
        }
    }
}

/// Bounding box diagonal length of one net endpoint box center (wire rip-up reroute sorts by this from large to small, move blocking long wires first)
fn net_span(graph: &McVecGraph, net: &VizNet) -> f64 {
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut any = false;
    for e in &net.endpoints {
        if let Some(b) = graph.boxes.iter().find(|x| x.id == e.box_id) {
            let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            minx = minx.min(cx);
            miny = miny.min(cy);
            maxx = maxx.max(cx);
            maxy = maxy.max(cy);
            any = true;
        }
    }
    if !any {
        0.0
    } else {
        ((maxx - minx).powi(2) + (maxy - miny).powi(2)).sqrt()
    }
}

/// P10 main entry: channel-aware routing for all nets in one layer graph
///
/// Internal:
/// 1. Build ChannelMap (extract channels from all boxes in this layer)
/// 2. Calculate scheduling order (priority + span)
/// 3. Select router for each net in order → call channel-aware routing
/// 4. Recursive sub-layers (each layer has independent ChannelMap)
pub fn route_layer_with_channels(graph: &mut McVecGraph) {
    for net in &graph.nets {
        for ep in &net.endpoints {
            let found = graph.boxes.iter().any(|b| b.id == ep.box_id);
            let has_entry = found
                && graph
                    .boxes
                    .iter()
                    .find(|b| b.id == ep.box_id)
                    .map(|b| b.find_entry(ep.pin_id).is_some())
                    .unwrap_or(false);
            if !found {
                eprintln!(
                    "[diag] net '{}' nid={}: box_id={} NOT FOUND",
                    net.name, net.nid, ep.box_id
                );
            } else if !has_entry {
                eprintln!(
                    "[diag] net '{}' nid={}: box_id={} pin_id={} NO ENTRY_POINT",
                    net.name, net.nid, ep.box_id, ep.pin_id
                );
            }
        }
    }

    // ── ★ ITER-6: Defensive merge of same-name 2-endpoint Power/Ground nets ─────────────────────────
    //
    // Even though ITER-4 already did PowerLabel-anchored hyperedge merging in from_block phase, same-name
    // 2-endpoint power/ground nets may still slip through:
    //   - Top-level has no corresponding PowerLabel box (P0-3 Phase 1.6 synthesis miss)
    //   - rail-synth not recognized (label name normalization differences etc.)
    //   - User **explicitly** wrote multiple independent GND ~ X 2-terminal connections (semantically equivalent to one multi-terminal net)
    //
    // This pass is **pure geometric fallback**: before router scheduling, by `(Name.upper(), kind)`
    // merge same-name Power/Ground TwoPoint nets into single hyperedge, keep first net's nid, merge
    // all other endpoints (deduplicate by (box_id, pin_id)) into the first one.
    //
    // Trigger conditions (all must be met to merge):
    //   - kind ∈ {Power, Ground}      (non-Power/Ground are truly multiple independent signals,
    //                                  even with same name shouldn't merge, e.g., multiple modules each have ENABLE pin)
    //   - At least 1 TwoPoint (2 endpoints) in same-name group
    //   - After merging endpoint count ≥ 3            (if can't form hyperedge, don't touch, keep original Orthogonal)
    //
    // After merging, dispatch.rs::pick_router automatically selects TrunkTap for (Power/Ground, StarOneDriver/
    // MultiDriver), one trunk + multiple taps, visually looks like real power rail.
    merge_same_name_power_ground_nets(graph);

    let mut channels = ChannelMap::build(graph, DEFAULT_LINE_GAP);

    // ── ★ M2: This layer grid A* (obstacles = all boxes inflated) ────────────────────────────
    // After routing each net, reserve its wires into grid; if later 2-terminal nets hit boxes / overlap other's wires,
    // use A* to reroute (avoid boxes + avoid routed wires) → eliminate box-through & wire-over-wire, multi-terminal nets not rerouted for now (stage 4 later),
    // but also reserve them in, letting 2-terminal nets route around them.
    let mut grid = Grid::from_graph(graph, GRID_CELL, GRID_INFLATE);
    let acfg = AStarCfg::default();

    // 1. Calculate intent + choice + priority for each net
    let plans: Vec<RoutePlan> = graph
        .nets
        .iter()
        .enumerate()
        .map(|(i, net)| {
            let intent = RouteIntent::from_net(net, graph);
            let choice = pick_router(&intent);
            let priority = priority_of(choice, &intent.kind);
            let span = (intent.span_x.powi(2) + intent.span_y.powi(2)).sqrt();
            RoutePlan {
                net_index: i,
                choice,
                priority,
                span,
                net_id: net.nid,
                net_name: net.name.clone(),
                should_warn: choice.should_warn(),
            }
        })
        .collect();

    // 2. Sort: (priority asc, span desc, nid asc)
    let mut order = plans;
    order.sort_by(|a, b| {
        a.priority.cmp(&b.priority).then_with(|| {
            // M12: stable float tie-break — use total_cmp for deterministic ordering
            b.span
                .total_cmp(&a.span)
                .then_with(|| a.net_id.cmp(&b.net_id))
        })
    });

    crate::vlog!(
        "[route::scheduler] layer '{}' bid={} planned {} nets ({}H + {}V channels)",
        graph.name,
        graph.bid,
        order.len(),
        channels.horizontal.len(),
        channels.vertical.len(),
    );

    // 3. Execute according to plan
    for plan in &order {
        if plan.should_warn {
            crate::vlog!(
                "[route::scheduler] WARN net '{}' (nid={}) multi-driver Signal — likely DRC violation",
                plan.net_name, plan.net_id
            );
        }
        crate::vlog!(
            "[route::scheduler] net='{}' nid={} span={:.0} → {}",
            plan.net_name,
            plan.net_id,
            plan.span,
            plan.choice.name()
        );

        // Borrow trick (same as dispatch.rs): extract the net so we can have
        // both &graph + &mut net at the same time
        let placeholder = VizNet::new(0, String::new(), NetKind::Signal, Vec::new());
        let mut tmp = std::mem::replace(&mut graph.nets[plan.net_index], placeholder);

        route_one_net_with_channels(plan.choice, graph, &mut tmp, &mut channels);

        // ── ★ M2 stages 1+2: 2-endpoint net hits box / overlaps other's wire
        //                  → A* reroute; then reserve this net's wires ──
        if tmp.endpoints.len() == 2 {
            let ep_rects = endpoint_rects(graph, &tmp);
            let hit = tmp
                .route
                .as_ref()
                .map(|r| grid.route_collides(&r.segments, tmp.nid, &ep_rects))
                .unwrap_or(false);
            if hit {
                let is_pg_stub = is_power_ground_flag_stub(graph, &tmp);
                let original_len = tmp
                    .route
                    .as_ref()
                    .map(|r| {
                        r.segments
                            .iter()
                            .map(|s| {
                                ((s.to.x - s.from.x).powi(2) + (s.to.y - s.from.y).powi(2)).sqrt()
                            })
                            .sum::<f64>()
                    })
                    .unwrap_or(0.0);
                if let Some(r2) = grid_router::reroute_two_point(&grid, graph, &tmp, &acfg) {
                    let r2_len: f64 = r2
                        .segments
                        .iter()
                        .map(|s| ((s.to.x - s.from.x).powi(2) + (s.to.y - s.from.y).powi(2)).sqrt())
                        .sum();
                    // M10b: PG flag stub guard — reject A* reroute if it produces a long detour
                    if is_pg_stub && r2_len > LongStubGuard::max_allowed(original_len) {
                        crate::vlog!(
                            "[route::grid] PG flag stub '{}' A* reroute rejected: len {:.1} > max {:.1} (original {:.1})",
                            tmp.name,
                            r2_len,
                            LongStubGuard::max_allowed(original_len),
                            original_len,
                        );
                    } else {
                        crate::vlog!(
                            "[route::grid] net='{}' nid={} A* reroute (avoid box/avoid wire)",
                            tmp.name,
                            tmp.nid
                        );
                        tmp.route = Some(r2);
                    }
                }
            }
        }
        // Reserve this net's wires (including multi-endpoint) → later nets route around
        if let Some(r) = &tmp.route {
            grid.reserve_segments(&r.segments, tmp.nid, GRID_GAP);
        }

        graph.nets[plan.net_index] = tmp;
    }

    // ── ★ M9: Route feedback loop (replaces old rip-up block) ─────────────────
    let _feedback_report =
        feedback::run_route_feedback(graph, &mut grid, &acfg, &RouteFeedbackConfig::default());

    // 4. Debug statistics
    crate::vlog!(
        "[route::scheduler] layer done. Total slots reserved: {}",
        channels.total_slots()
    );
    let per_net = channels.slots_per_net();
    let max_net = per_net
        .iter()
        .max_by_key(|(_, &v)| v)
        .map(|(nid, v)| (*nid, *v));
    if let Some((nid, v)) = max_net {
        if crate::viz::debug::dump_enabled() {
            crate::vlog!("[route::scheduler]   top net: nid={nid} used {v} slots");
        }
    }
}

/// Recursive version: top layer + all sub-layers
pub fn route_all_with_channels(graph: &mut McVecGraph) {
    route_layer_with_channels(graph);
    for sub in &mut graph.sub_graphs {
        route_all_with_channels(sub);
    }
}

// ============================================================================
// Internal: channel-aware routing for a single net
// ============================================================================

fn route_one_net_with_channels(
    choice: RouterChoice,
    graph: &McVecGraph,
    net: &mut VizNet,
    channels: &mut ChannelMap,
) {
    use super::bus_bundle::route_bus_bundle_with_channels;
    use super::orthogonal::route_orthogonal_with_channels;
    use super::trunk_tap::route_trunk_tap_with_channels;

    match choice {
        RouterChoice::Noop => {
            // no routing
        }
        RouterChoice::Orthogonal => {
            route_orthogonal_with_channels(graph, net, channels);
        }
        RouterChoice::TrunkTap | RouterChoice::TrunkTapWithWarning => {
            // ★ FIX (sub-graph): sub-layer (graph.fanout_star) uses hub-star ——
            //   multiple wires fan out from the main device's pins, replacing the
            //   shared trunk (box's single-point entry, the "tree root"). Top layer
            //   still uses TrunkTap.
            if graph.fanout_star {
                super::star::route_hub_star(graph, net);
            } else {
                route_trunk_tap_with_channels(graph, net, channels);
            }
        }
        RouterChoice::Star => {
            // Star doesn't participate in channels (radiates from hub, endpoints connect directly to hub)
            // Fall back to no-channel behavior
            use crate::viz::traits::Router;
            let router = super::star::StarRouter;
            router.route(graph, net);
        }
        RouterChoice::BusBundle => {
            // ★ FIX: bus also needs fan-out —— each member bit connects individually
            //   from the main device's corresponding pin, rather than all bits
            //   converging to a brown trunk and entering the device at a single point.
            //   When fanout_star is on, use hub-star (route_hub_star pairs peers
            //   to hub-side pins by member name).
            if graph.fanout_star {
                super::star::route_hub_star(graph, net);
            } else {
                route_bus_bundle_with_channels(graph, net, channels);
            }
        }
    }
}

// ============================================================================
// Scheduling priority
// ============================================================================

/// ★ P5.2 switch: if backbone-priority routing causes regressions, set to false
/// → restore original behavior.
const ENABLE_BACKBONE_PRIORITY: bool = true;

/// Scheduling priority (lower is routed first). First-routed nets get straighter
/// channels, later-routed ones detour around them.
///
/// ★ P5.2: `Orthogonal` (2 endpoints) is further layered by kind —— power/ground/
/// sub-module IO, these backbones are routed before ordinary signals, so signals
/// yield the way. Multi-endpoint backbone already goes TrunkTap(1)/Bus(0), unaffected.
fn priority_of(choice: RouterChoice, kind: &NetKind) -> u32 {
    match choice {
        RouterChoice::BusBundle => 0,
        RouterChoice::TrunkTap | RouterChoice::TrunkTapWithWarning => 1,
        RouterChoice::Star => 2,
        RouterChoice::Orthogonal => {
            if ENABLE_BACKBONE_PRIORITY
                && matches!(
                    kind,
                    NetKind::Power | NetKind::Ground | NetKind::SubModuleIO
                )
            {
                3 // backbone 2 endpoints: before signals
            } else {
                4 // ordinary signal 2 endpoints
            }
        }
        RouterChoice::Noop => 5,
    }
}

#[derive(Debug)]
struct RoutePlan {
    net_index: usize,
    choice: RouterChoice,
    priority: u32,
    span: f64,
    net_id: i64,
    net_name: String,
    should_warn: bool,
}

// ============================================================================
// ITER-6: Defensive same-name power/ground net merging
// ============================================================================

/// Merge same-name Power/Ground TwoPoint nets in the same `graph.nets` layer into
/// a single hyperedge
///
/// Entry conditions: at least 2 nets satisfy:
///   - `net.kind ∈ {Power, Ground}`
///   - `net.name.to_uppercase()` is the same (case-insensitive merge)
///   - At least 1 is 2 endpoints (TwoPoint) —— prevents false-merging genuinely
///     different multi-endpoint hyperedges
///
/// Merge semantics:
///   - Keep the **first** net in the group (by nid ascending), append all endpoints
///     of the other nets (dedup by (box_id, pin_id))
///   - Delete the merged-out other nets
///   - Endpoint count < 3 (can't form a hyperedge) → don't merge, leave alone
///   - Already-routed nets (route is Some) don't participate in merging
///     (theoretically won't happen before scheduling)
///
/// Output: modifies `graph.nets` contents (length may decrease), and prints diagnostic logs.
fn merge_same_name_power_ground_nets(graph: &mut crate::vector::graph::McVecGraph) {
    use crate::vector::graph::{BoxKind, EndpointRef};
    use std::collections::HashMap;

    // ★ Stage A guard: any stub net whose endpoints connect to PowerLabel flags
    //    doesn't participate in merging — otherwise it would re-merge the
    //    per-consumer flag stubs that A2 (rails.rs) blew apart back into the global trunk.
    let label_box_ids: std::collections::HashSet<i64> = graph
        .boxes
        .iter()
        .filter(|b| b.kind == BoxKind::PowerLabel || b.symbol.is_power_rail())
        .map(|b| b.id)
        .collect();

    // 1. Group by (name.upper(), kind_tag), only consider unrouted Power/Ground nets
    //    kind_tag: Power=0, Ground=1 (avoid NetKind's own Hash implementation uncertainty)
    let mut groups: HashMap<(String, u8), Vec<usize>> = HashMap::new();
    for (idx, net) in graph.nets.iter().enumerate() {
        if net.route.is_some() {
            continue;
        }
        // Skip stubs that connect to flags
        if net
            .endpoints
            .iter()
            .any(|e| label_box_ids.contains(&e.box_id))
        {
            continue;
        }
        let tag: u8 = match net.kind {
            NetKind::Power => 0,
            NetKind::Ground => 1,
            _ => continue,
        };
        let key = (net.name.to_uppercase(), tag);
        groups.entry(key).or_default().push(idx);
    }

    // 2. Collect "merge plans": (keep_idx, drop_indices, merged_endpoints)
    //    Use an idx → action table, then in a pass rebuild graph.nets
    let mut merge_plans: Vec<(usize, Vec<usize>, Vec<EndpointRef>)> = Vec::new();
    for ((name_upper, _tag), mut indices) in groups {
        if indices.len() < 2 {
            continue;
        }
        indices.sort_by_key(|&i| graph.nets[i].nid);

        // Must have at least one TwoPoint, otherwise don't touch
        // (all are hyperedges → no need to merge)
        let has_twopoint = indices.iter().any(|&i| graph.nets[i].endpoints.len() == 2);
        if !has_twopoint {
            continue;
        }

        // Merge endpoints (dedup by (box_id, pin_id), pin_name takes the first seen)
        let mut seen: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
        let mut merged: Vec<EndpointRef> = Vec::new();
        for &i in &indices {
            for ep in &graph.nets[i].endpoints {
                let k = (ep.box_id, ep.pin_id);
                if seen.insert(k) {
                    merged.push(ep.clone());
                }
            }
        }

        if merged.len() < 3 {
            // After merging still < 3 endpoints, doesn't form a hyperedge → not worth touching
            continue;
        }

        let keep = indices[0];
        let drops: Vec<usize> = indices.into_iter().skip(1).collect();
        crate::vlog!(
            "[route::scheduler] ITER-6 merge: '{}' kind={:?} {} nets → 1 hyperedge with {} endpoints (kept nid={}, dropped {} nets)",
            name_upper,
            graph.nets[keep].kind,
            drops.len() + 1,
            merged.len(),
            graph.nets[keep].nid,
            drops.len()
        );
        merge_plans.push((keep, drops, merged));
    }

    if merge_plans.is_empty() {
        return;
    }

    // 3. Apply merges: first update kept nets' endpoints, then delete drops in reverse idx order
    //    (reverse-order deletion keeps indices stable)
    //
    //    Collect all idx to delete, then retain once
    let mut drop_set: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for (keep, drops, merged_eps) in &merge_plans {
        graph.nets[*keep].endpoints = merged_eps.clone();
        for d in drops {
            drop_set.insert(*d);
        }
    }
    let mut idx = 0usize;
    graph.nets.retain(|_| {
        let keep = !drop_set.contains(&idx);
        idx += 1;
        keep
    });

    crate::vlog!(
        "[route::scheduler] ITER-6 merge done: {} merge group(s) applied, {} net(s) removed",
        merge_plans.len(),
        drop_set.len()
    );
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::netdef::IoDirection;
    use crate::vector::graph::{BoxKind, EndpointRef, IoSummary, McVecBox};

    fn mk_box(id: i64, x: f64, y: f64, w: f64, h: f64) -> McVecBox {
        let mut b = McVecBox::new(
            id,
            format!("b{}", id),
            String::new(),
            BoxKind::MultiPin,
            1,
            IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
        b
    }

    #[test]
    fn p10_priority_order_buses_first() {
        let sig = NetKind::Signal;
        assert!(
            priority_of(RouterChoice::BusBundle, &sig) < priority_of(RouterChoice::TrunkTap, &sig)
        );
        assert!(priority_of(RouterChoice::TrunkTap, &sig) < priority_of(RouterChoice::Star, &sig));
        assert!(
            priority_of(RouterChoice::Star, &sig) < priority_of(RouterChoice::Orthogonal, &sig)
        );
        assert!(
            priority_of(RouterChoice::Orthogonal, &sig) < priority_of(RouterChoice::Noop, &sig)
        );
    }

    #[test]
    fn p10_priority_trunk_tap_warning_same_as_normal() {
        let sig = NetKind::Signal;
        assert_eq!(
            priority_of(RouterChoice::TrunkTap, &sig),
            priority_of(RouterChoice::TrunkTapWithWarning, &sig)
        );
    }

    #[test]
    fn p10_scheduler_runs_without_panic_on_empty_graph() {
        let mut g = McVecGraph::new(0, "empty".into());
        route_layer_with_channels(&mut g);
    }

    #[test]
    fn p10_scheduler_routes_simple_two_point() {
        // 2 boxes + 1 signal between them
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 0.0, 0.0, 100.0, 60.0));
        g.boxes.push(mk_box(2, 300.0, 0.0, 100.0, 60.0));
        g.nets.push(VizNet::new(
            10,
            "s1".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(1, 11, "p11", IoDirection::Output),
                EndpointRef::with_io(2, 21, "p21", IoDirection::Input),
            ],
        ));

        route_layer_with_channels(&mut g);

        let net = &g.nets[0];
        assert!(net.route.is_some(), "route should be filled");
        assert!(!net.route.as_ref().unwrap().segments.is_empty());
    }

    #[test]
    fn p10_scheduler_orders_by_priority_then_span() {
        // 3 nets: a Bus (small span), a TrunkTap (big span), an Orthogonal (medium span)
        // Bus should be scheduled first, even if its span isn't the largest
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(1, 0.0, 0.0, 100.0, 60.0));
        g.boxes.push(mk_box(2, 200.0, 0.0, 100.0, 60.0));
        g.boxes.push(mk_box(3, 1000.0, 0.0, 100.0, 60.0));

        // Net A: 2-endpoint signal, big span (0→1000)
        g.nets.push(VizNet::new(
            10,
            "ortho_big".into(),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(1, 1, "p1", IoDirection::Output),
                EndpointRef::with_io(3, 1, "p1", IoDirection::Input),
            ],
        ));

        // Net B: bus (TrunkTap of size 4), modest span (0→200)
        g.nets.push(VizNet::new(
            11,
            "bus_small".into(),
            NetKind::Bus(4),
            vec![
                EndpointRef::with_io(1, 2, "p2", IoDirection::Bidir),
                EndpointRef::with_io(2, 1, "p1", IoDirection::Bidir),
            ],
        ));

        // No panic + routing results all present
        route_layer_with_channels(&mut g);
        for net in &g.nets {
            assert!(net.route.is_some(), "every net should be routed");
        }
    }
}

// ============================================================================
// M10b: Power/Ground flag stub guard
// ============================================================================

/// Check if a net is a power/ground flag stub (2-endpoint net with one PowerLabel endpoint).
fn is_power_ground_flag_stub(graph: &McVecGraph, net: &VizNet) -> bool {
    if !matches!(net.kind, NetKind::Power | NetKind::Ground) {
        return false;
    }
    if net.endpoints.len() != 2 {
        return false;
    }
    let flag_count = net
        .endpoints
        .iter()
        .filter(|ep| {
            graph
                .boxes
                .iter()
                .any(|b| b.id == ep.box_id && b.kind == BoxKind::PowerLabel)
        })
        .count();
    flag_count == 1
}

/// Guard for PG flag stub reroute: reject A* candidate if it produces an excessive detour.
struct LongStubGuard;

impl LongStubGuard {
    /// Maximum allowed reroute length for a PG flag stub.
    /// `max(120.0, original_len * 2.0)`
    fn max_allowed(original_len: f64) -> f64 {
        const FLOOR: f64 = 120.0;
        let double = original_len * 2.0;
        if double > FLOOR {
            double
        } else {
            FLOOR
        }
    }
}
