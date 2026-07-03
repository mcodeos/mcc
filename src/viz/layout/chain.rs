// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Chain topology detection + linear layout + IC-pin signal chain extraction

use crate::vector::graph::{naming, BoxKind, EntrySide, McVecGraph, NetKind, VizNet};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Part 1: Linear chain detection (existing, unchanged)
// ============================================================================

pub fn try_linearize_chain(comp: &[i64], adj: &HashMap<i64, Vec<i64>>) -> Option<Vec<i64>> {
    let mut endpoints: Vec<i64> = Vec::new();
    let mut middles: Vec<i64> = Vec::new();
    for &id in comp {
        let d = adj.get(&id).map(|v| v.len()).unwrap_or(0);
        match d {
            0 => return None,
            1 => endpoints.push(id),
            2 => middles.push(id),
            _ => return None,
        }
    }
    if endpoints.len() != 2 || endpoints.len() + middles.len() != comp.len() {
        return None;
    }
    let start = endpoints[0];
    let mut chain = Vec::with_capacity(comp.len());
    let mut visited = HashSet::new();
    chain.push(start);
    visited.insert(start);
    let mut cur = start;
    while chain.len() < comp.len() {
        let neighbors = adj.get(&cur).cloned().unwrap_or_default();
        match neighbors.into_iter().find(|n| !visited.contains(n)) {
            Some(n) => {
                chain.push(n);
                visited.insert(n);
                cur = n;
            }
            None => return None,
        }
    }
    Some(chain)
}

pub fn layout_chain_horizontal(
    graph: &mut McVecGraph,
    chain: &[i64],
    start_x: f64,
    start_y: f64,
) -> (f64, f64) {
    const CHAIN_GAP: f64 = 50.0;
    let max_h: f64 = chain
        .iter()
        .filter_map(|id| graph.boxes.iter().find(|b| b.id == *id))
        .map(|b| b.h)
        .fold(0.0f64, f64::max);
    let row_cy = start_y + max_h / 2.0;
    let mut cur_x = start_x;
    for &id in chain {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
            b.x = cur_x;
            b.y = row_cy - b.h / 2.0;
            cur_x += b.w + CHAIN_GAP;
        }
    }
    ((cur_x - CHAIN_GAP - start_x).max(0.0), max_h)
}

// ============================================================================
// Part 2: Signal chain extraction
// ============================================================================

#[derive(Debug, Clone)]
pub struct ChainNode {
    pub box_id: i64,
    pub net_id: i64, // net connecting to previous node
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChainDir {
    Left,
    Right,
    Up,
    Down,
}
impl ChainDir {
    pub fn from_side(side: &EntrySide) -> Self {
        match side {
            EntrySide::Left => ChainDir::Left,
            EntrySide::Right => ChainDir::Right,
            EntrySide::Top => ChainDir::Up,
            EntrySide::Bottom => ChainDir::Down,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignalChain {
    pub hub_id: i64,
    pub hub_pin: i64,
    pub hub_pin_name: String,
    pub direction: ChainDir,
    /// TwoPin passives in order (hub → terminus)
    pub nodes: Vec<ChainNode>,
    /// Terminal box (non-TwoPin), None if open or loops back
    pub terminus: Option<ChainNode>,
    pub loops_to_hub: bool,
}

impl SignalChain {
    pub fn is_direct(&self) -> bool {
        self.nodes.is_empty() && self.terminus.is_some()
    }
    pub fn all_box_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.nodes.iter().map(|n| n.box_id).collect();
        if let Some(t) = &self.terminus {
            ids.push(t.box_id);
        }
        ids
    }
}

#[derive(Debug)]
pub struct SignalChainResult {
    pub hub_id: i64,
    pub hub_name: String,
    pub chains: Vec<SignalChain>,
    pub chained_ids: HashSet<i64>,
    pub orphan_ids: HashSet<i64>,
}

impl SignalChainResult {
    pub fn by_pin(&self) -> HashMap<i64, Vec<&SignalChain>> {
        let mut m: HashMap<i64, Vec<&SignalChain>> = HashMap::new();
        for c in &self.chains {
            m.entry(c.hub_pin).or_default().push(c);
        }
        m
    }
    pub fn dump(&self, graph: &McVecGraph) -> String {
        let name_of = |id: i64| -> String {
            graph
                .boxes
                .iter()
                .find(|b| b.id == id)
                .map(|b| b.name.clone())
                .unwrap_or_else(|| format!("#{}", id))
        };
        let mut s = format!(
            "[chain] hub='{}' (id={}), {} chain(s), {} orphan(s)\n",
            self.hub_name,
            self.hub_id,
            self.chains.len(),
            self.orphan_ids.len()
        );
        for (i, c) in self.chains.iter().enumerate() {
            let path: Vec<String> = c.nodes.iter().map(|n| name_of(n.box_id)).collect();
            let end = match &c.terminus {
                Some(t) => format!("-> [{}]", name_of(t.box_id)),
                None if c.loops_to_hub => "-> hub(loop)".into(),
                None => "-> (open)".into(),
            };
            s.push_str(&format!(
                "[chain]   [{}] {:?} '{}': {} {}\n",
                i,
                c.direction,
                c.hub_pin_name,
                if path.is_empty() {
                    "(direct)".into()
                } else {
                    path.join(" -> ")
                },
                end,
            ));
        }
        if !self.orphan_ids.is_empty() {
            let names: Vec<String> = self.orphan_ids.iter().map(|id| name_of(*id)).collect();
            s.push_str(&format!("[chain]   orphans: {:?}\n", names));
        }
        s
    }
}

// ============================================================================
// Hub detection
// ============================================================================

pub fn find_hub(graph: &McVecGraph) -> Option<i64> {
    graph
        .boxes
        .iter()
        .filter(|b| b.id >= 0 && matches!(b.kind, BoxKind::MultiPin | BoxKind::SubModule))
        .max_by_key(|b| {
            let tier: usize = if b.kind == BoxKind::MultiPin {
                10000
            } else {
                0
            };
            tier + b.pin_count.max(b.entry_points.len())
        })
        .map(|b| b.id)
}

// ============================================================================
// Extraction — pin_id-free approach
// ============================================================================

/// Build index: box_id → [net_index] (which nets touch this box?)
fn build_box_net_index(graph: &McVecGraph) -> HashMap<i64, Vec<usize>> {
    let mut idx: HashMap<i64, Vec<usize>> = HashMap::new();
    for (ni, net) in graph.nets.iter().enumerate() {
        // Deduplicate: if a net has multiple endpoints on the same box, add net index only once
        let mut seen = HashSet::new();
        for ep in &net.endpoints {
            if seen.insert(ep.box_id) {
                idx.entry(ep.box_id).or_default().push(ni);
            }
        }
    }
    idx
}

/// Is this net a power/ground rail?
///
/// Rails are recognised primarily by their [`NetKind`] (the synthesized rail
/// hyperedges from `from_block::synthesize_rail_nets` carry `Power`/`Ground`),
/// with a name-based fallback for nets whose kind was left as `Signal`.
fn net_is_rail(net: &VizNet) -> bool {
    matches!(net.kind, NetKind::Power | NetKind::Ground) || naming::is_power_rail(&net.name)
}

/// Resolve the best *signal* name for a hub pin.
///
/// ★ The net endpoint's `pin_name` is unreliable for direction assignment:
/// - For a real IC pin it is often the bare pin **number** (`"3"`), not the
///   functional name (`"LX"`).
/// - For a rail-synth endpoint promoted by `promote_synthetic_pins`, it is the
///   **net** name (which is how a hub pin ends up labelled `"lp322dcdc"`).
///
/// The authoritative functional name lives in the hub box's `BoxPin.description`
/// (mcode `pins = [ <number> = <description> ]`, e.g. `3 = LX`). Prefer that,
/// then the pin number, then the entry-point name, then the raw endpoint name.
fn resolve_hub_pin_name(graph: &McVecGraph, hub_id: i64, pin_id: i64, fallback: &str) -> String {
    if let Some(b) = graph.boxes.iter().find(|b| b.id == hub_id) {
        if let Some(p) = b.pins.iter().find(|p| p.id == pin_id) {
            if !p.description.is_empty() {
                return p.description.clone();
            }
            if !p.pin_id.is_empty() {
                return p.pin_id.clone();
            }
        }
        if let Some(ep) = b.entry_points.iter().find(|e| e.pin_id == pin_id) {
            if !ep.pin_name.is_empty() {
                return ep.pin_name.clone();
            }
        }
    }
    fallback.to_string()
}

pub fn extract_signal_chains(graph: &McVecGraph) -> SignalChainResult {
    let hub_id = match find_hub(graph) {
        Some(id) => id,
        None => return empty_result(graph, -1),
    };
    let hub_name = graph
        .boxes
        .iter()
        .find(|b| b.id == hub_id)
        .map(|b| b.name.clone())
        .unwrap_or_default();

    let box_nets = build_box_net_index(graph);
    let box_set: HashSet<i64> = graph.boxes.iter().map(|b| b.id).collect();

    // Collect hub's nets
    let hub_net_indices = box_nets.get(&hub_id).cloned().unwrap_or_default();

    let mut chained: HashSet<i64> = HashSet::new();
    chained.insert(hub_id);
    let mut chains: Vec<SignalChain> = Vec::new();

    // For each net touching the hub, look at other-side endpoints
    for &ni in &hub_net_indices {
        let net = &graph.nets[ni];

        // Get the hub's pin info from this net (for labeling)
        let hub_ep = net.endpoints.iter().find(|e| e.box_id == hub_id);
        let (hub_pin, raw_name) = hub_ep
            .map(|e| (e.pin_id, e.pin_name.clone()))
            .unwrap_or((-1, String::new()));
        // ★ Problem 2 fix: derive the functional signal name (LX/EN/FB…) from the
        //   hub's BoxPin.description, not the (numeric / net-name) endpoint pin_name.
        let hub_pin_name = resolve_hub_pin_name(graph, hub_id, hub_pin, &raw_name);

        // ★ Problem 1 fix: is this hub net itself a shared power/ground rail?
        //   If so, its passive consumers are reached via their *signal* pins on
        //   other nets — spawning chains from the rail side only duplicates them
        //   and makes the trace wander across every consumer on the rail.
        let hub_net_is_rail = net_is_rail(net);

        // Direction: use entry_point if available, else fallback Right
        let dir = graph
            .boxes
            .iter()
            .find(|b| b.id == hub_id)
            .and_then(|b| b.entry_points.iter().find(|ep| ep.pin_id == hub_pin))
            .map(|ep| ChainDir::from_side(&ep.side))
            .unwrap_or(ChainDir::Right);

        // Find OTHER boxes on this net (not hub)
        let others: Vec<_> = net
            .endpoints
            .iter()
            .filter(|e| e.box_id != hub_id && box_set.contains(&e.box_id))
            .collect();

        for other in &others {
            let other_box = match graph.boxes.iter().find(|b| b.id == other.box_id) {
                Some(b) => b,
                None => continue,
            };

            // On a shared rail hub net, only record non-passive termini (rail
            // label, boundary submodule). Passives are picked up via their
            // signal-side hub nets (see hub_net_is_rail above).
            if hub_net_is_rail && other_box.kind == BoxKind::TwoPin {
                continue;
            }

            let mut chain = SignalChain {
                hub_id,
                hub_pin,
                hub_pin_name: hub_pin_name.clone(),
                direction: dir.clone(),
                nodes: Vec::new(),
                terminus: None,
                loops_to_hub: false,
            };

            if other_box.kind == BoxKind::TwoPin {
                let mut visited = HashSet::new();
                visited.insert(hub_id);
                trace_by_box(
                    graph,
                    &box_nets,
                    &box_set,
                    hub_id,
                    &mut chain,
                    other.box_id,
                    net.nid,
                    &mut visited,
                );
            } else {
                chain.terminus = Some(ChainNode {
                    box_id: other.box_id,
                    net_id: net.nid,
                });
            }

            for n in &chain.nodes {
                chained.insert(n.box_id);
            }
            if let Some(t) = &chain.terminus {
                chained.insert(t.box_id);
            }
            chains.push(chain);
        }
    }

    let orphans = box_set.difference(&chained).copied().collect();
    SignalChainResult {
        hub_id,
        hub_name,
        chains,
        chained_ids: chained,
        orphan_ids: orphans,
    }
}

fn empty_result(graph: &McVecGraph, hub_id: i64) -> SignalChainResult {
    SignalChainResult {
        hub_id,
        hub_name: String::new(),
        chains: Vec::new(),
        chained_ids: HashSet::new(),
        orphan_ids: graph.boxes.iter().map(|b| b.id).collect(),
    }
}

// ============================================================================
// Chain tracing — follows nets by BOX, not by pin
// ============================================================================

/// Trace through TwoPin passives by following net connectivity.
///
/// ★ Key insight: TwoPin has exactly 2 pins, connected by exactly 2 nets.
/// We entered via `from_net`. The OTHER net on this box is the exit.
/// No need to match pin_ids — just find "the other net".
fn trace_by_box(
    graph: &McVecGraph,
    box_nets: &HashMap<i64, Vec<usize>>,
    box_set: &HashSet<i64>,
    hub_id: i64,
    chain: &mut SignalChain,
    box_id: i64,
    from_net_id: i64,
    visited: &mut HashSet<i64>,
) {
    if !visited.insert(box_id) {
        if box_id == hub_id {
            chain.loops_to_hub = true;
        }
        return;
    }

    let b = match graph.boxes.iter().find(|b| b.id == box_id) {
        Some(b) => b,
        None => return,
    };

    // Non-TwoPin → terminus
    if b.kind != BoxKind::TwoPin {
        chain.terminus = Some(ChainNode {
            box_id,
            net_id: from_net_id,
        });
        return;
    }

    // Add this passive to chain
    chain.nodes.push(ChainNode {
        box_id,
        net_id: from_net_id,
    });

    // ★ Find exit: ALL nets on this box EXCEPT the one we came from
    let my_nets = box_nets.get(&box_id).cloned().unwrap_or_default();

    for &ni in &my_nets {
        let net = &graph.nets[ni];
        if net.nid == from_net_id {
            continue;
        } // skip entry net

        // ★ Problem 1 fix: a power/ground exit net TERMINATES the chain. Never
        //   traverse a shared rail hyperedge to another passive (that would wander
        //   across every consumer on the rail). A decoupling cap ends here:
        //   loop back to the hub if the hub sits on this rail, otherwise terminate
        //   at the rail's label box.
        if net_is_rail(net) {
            if net
                .endpoints
                .iter()
                .any(|e| e.box_id == hub_id && e.box_id != box_id)
            {
                chain.loops_to_hub = true;
                return;
            }
            if let Some(rail_ep) = net.endpoints.iter().find(|e| {
                e.box_id != box_id
                    && box_set.contains(&e.box_id)
                    && graph
                        .boxes
                        .iter()
                        .any(|b| b.id == e.box_id && b.kind == BoxKind::PowerLabel)
            }) {
                chain.terminus = Some(ChainNode {
                    box_id: rail_ep.box_id,
                    net_id: net.nid,
                });
            }
            return;
        }

        // Look for next unvisited box on exit net
        if let Some(next) = net.endpoints.iter().find(|e| {
            e.box_id != box_id && !visited.contains(&e.box_id) && box_set.contains(&e.box_id)
        }) {
            trace_by_box(
                graph,
                box_nets,
                box_set,
                hub_id,
                chain,
                next.box_id,
                net.nid,
                visited,
            );
            return;
        }

        // Check hub loop-back
        if net
            .endpoints
            .iter()
            .any(|e| e.box_id == hub_id && e.box_id != box_id)
        {
            chain.loops_to_hub = true;
            return;
        }
    }
    // No exit found — chain ends here (open)
}
