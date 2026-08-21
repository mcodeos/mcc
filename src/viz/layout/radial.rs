// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ B2 · Radial layered layout for root block diagram
//!
//! ## Algorithm (§4.1~4.5)
//!
//! 1. **Select hub** by weighted degree: deg(b) = 2 × signal_edges + 1 × rail_edges.
//!    Tie-break: total degree, then source_span line number.
//! 2. **BFS ring assignment**: ring(hub)=0, ring(b) = undirected BFS hop count from hub.
//! 3. **Sector assignment** (ring≥1, by priority):
//!    a. Bidirectional/lane_count>1 bus edge with hub → W upper
//!    b. Rail edge where b is driver → W lower
//!    c. Signal edge into hub → N
//!    d. Signal edge out of hub → S
//!    e. No direct edge to hub → inherit nearest ring-1 neighbor's sector
//!    f. Only connected to external ports → E
//! 4. **Hub height** = max(west_pin_count, east_pin_count) × ROW_STEP.
//!    Each west pin occupies one row, aligned with the W-column box at the same row.
//! 5. **Coordinate table** matching §4.5.
//!
//! ## Constants
//! COL_STEP=280, ROW_STEP=140, BOX_W=160, BOX_H=80, HUB_W=200.

use std::collections::{HashMap, VecDeque};

use crate::vector::graph::{EntrySide, McVecBox, McVecGraph};

use super::edge_decide::{decide_edges, BlockEdge, EdgeKind};

// ── Layout constants (§4 preamble) ──
const COL_STEP: f64 = 280.0;
const ROW_STEP: f64 = 140.0;
const BOX_W: f64 = 160.0;
const BOX_H: f64 = 80.0;
const HUB_W: f64 = 200.0;

/// Run the radial layout pipeline for the root layer.
///
/// This is the single entry point called from `flow.rs` when `graph.is_root`.
/// After this function returns, all boxes have `geom_locked = true`.
pub fn place_radial(graph: &mut McVecGraph) {
    // Step 0: decide edges (R-P / R-B / R-R / R-M)
    let (edges, report) = decide_edges(graph);
    report.log(&graph.name);

    // Step 1: select hub by weighted degree (§4.1)
    let hub_id = select_hub(graph, &edges);

    // Step 2: BFS ring assignment (§4.2)
    let rings = assign_rings(graph, &edges, hub_id);

    // Step 3: sector assignment (§4.3)
    let sectors = assign_sectors(graph, &edges, hub_id, &rings);

    // Step 4: compute hub height from west pin count (§4.4)
    let hub_h = compute_hub_height(graph, &edges, hub_id, &sectors);

    // Step 5: assign coordinates (§4.5)
    assign_coordinates(graph, &edges, hub_id, &rings, &sectors, hub_h);

    // Step 6: set up facade entry_points (§4.6)
    setup_facade_entry_points(graph, &edges);

    crate::vlog!(
        "[radial] hub={} deg={} rings={:?} sectors={:?} hub_h={:.0}",
        hub_id,
        weighted_degree(graph, &edges, hub_id),
        rings,
        sectors,
        hub_h
    );
    for b in &graph.boxes {
        crate::vlog!(
            "[radial] box id={} name='{}' x={:.0} y={:.0} w={:.0} h={:.0} ring={:?} sector={:?}",
            b.id,
            b.name,
            b.x,
            b.y,
            b.w,
            b.h,
            rings.get(&b.id),
            sectors.get(&b.id)
        );
    }
}

// ============================================================================
// Step 1: Hub selection (§4.1)
// ============================================================================

/// deg(b) = 2 × signal_edges + 1 × rail_edges
/// Hub = argmax(deg); tie-break: total degree, then source_span line number.
fn select_hub(graph: &McVecGraph, edges: &[BlockEdge]) -> i64 {
    if graph.boxes.is_empty() {
        return 0;
    }

    let mut degs: Vec<(i64, usize, usize)> = graph
        .boxes
        .iter()
        .map(|b| {
            let w = weighted_degree(graph, edges, b.id);
            let total = count_degree(edges, b.id);
            (b.id, w, total)
        })
        .collect();

    // Sort by: weighted degree desc, total degree desc, source_span line asc
    degs.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)).then_with(|| {
            let line_a = graph
                .boxes
                .iter()
                .find(|bx| bx.id == a.0)
                .and_then(|bx| bx.source_span.as_ref())
                .map(|p| p.offset)
                .unwrap_or(u32::MAX);
            let line_b = graph
                .boxes
                .iter()
                .find(|bx| bx.id == b.0)
                .and_then(|bx| bx.source_span.as_ref())
                .map(|p| p.offset)
                .unwrap_or(u32::MAX);
            line_a.cmp(&line_b)
        })
    });

    degs.first().map(|(id, _, _)| *id).unwrap_or(0)
}

fn weighted_degree(_graph: &McVecGraph, edges: &[BlockEdge], box_id: i64) -> usize {
    let sig =
        count_kind(edges, box_id, EdgeKind::Signal) + count_kind(edges, box_id, EdgeKind::Bus);
    let rail = count_kind(edges, box_id, EdgeKind::Power);
    2 * sig + rail
}

fn count_degree(edges: &[BlockEdge], box_id: i64) -> usize {
    edges
        .iter()
        .filter(|e| e.from_box == box_id || e.to_box == box_id)
        .count()
}

fn count_kind(edges: &[BlockEdge], box_id: i64, kind: EdgeKind) -> usize {
    edges
        .iter()
        .filter(|e| (e.from_box == box_id || e.to_box == box_id) && e.kind == kind)
        .count()
}

// ============================================================================
// Step 2: Ring assignment (§4.2)
// ============================================================================

/// ring(hub)=0; ring(b) = undirected BFS hop count from hub.
fn assign_rings(graph: &McVecGraph, edges: &[BlockEdge], hub_id: i64) -> HashMap<i64, usize> {
    let mut rings = HashMap::new();
    let mut queue = VecDeque::new();

    rings.insert(hub_id, 0);
    queue.push_back(hub_id);

    while let Some(current) = queue.pop_front() {
        let cur_ring = rings[&current];
        for neighbor in edge_neighbors(edges, current) {
            if !rings.contains_key(&neighbor) {
                rings.insert(neighbor, cur_ring + 1);
                queue.push_back(neighbor);
            }
        }
    }

    // Any unvisited boxes get ring = max + 1
    let max_ring = rings.values().max().copied().unwrap_or(0);
    for b in &graph.boxes {
        rings.entry(b.id).or_insert(max_ring + 1);
    }

    rings
}

fn edge_neighbors(edges: &[BlockEdge], box_id: i64) -> Vec<i64> {
    let mut neighbors = Vec::new();
    for e in edges {
        if e.from_box == box_id {
            neighbors.push(e.to_box);
        } else if e.to_box == box_id {
            neighbors.push(e.from_box);
        }
    }
    neighbors
}

// ============================================================================
// Step 3: Sector assignment (§4.3)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Sector {
    Hub,
    /// W (upper) — bidirectional or lane_count>1 bus with hub
    WestUpper,
    /// W (lower) — rail edge where box is driver
    WestLower,
    /// N — signal into hub
    North,
    /// S — signal out of hub
    South,
    /// E — only external port connections
    East,
}

/// Assign sectors by priority rules (§4.3 a→f).
fn assign_sectors(
    graph: &McVecGraph,
    edges: &[BlockEdge],
    hub_id: i64,
    rings: &HashMap<i64, usize>,
) -> HashMap<i64, Sector> {
    let mut sectors = HashMap::new();
    sectors.insert(hub_id, Sector::Hub);

    // Build adjacency from edges
    let adj: HashMap<i64, Vec<&BlockEdge>> = {
        let mut m: HashMap<i64, Vec<&BlockEdge>> = HashMap::new();
        for e in edges {
            m.entry(e.from_box).or_default().push(e);
            m.entry(e.to_box).or_default().push(e);
        }
        m
    };

    for b in &graph.boxes {
        if b.id == hub_id {
            continue;
        }
        let ring = rings.get(&b.id).copied().unwrap_or(usize::MAX);

        if ring == 1 {
            // Ring 1: apply rules a→d (§4.3)
            let hub_edges: Vec<&&BlockEdge> = adj
                .get(&b.id)
                .map(|v| {
                    v.iter()
                        .filter(|e| e.from_box == hub_id || e.to_box == hub_id)
                        .collect()
                })
                .unwrap_or_default();

            // Check direction of edges to/from hub
            // ★ Bus edges with bidirectional=true are bidirectional buses (e.g., SPI).
            let has_bidirectional = hub_edges.iter().any(|e| e.bidirectional);
            let has_signal_in = hub_edges.iter().any(|e| {
                (e.kind == EdgeKind::Signal || e.kind == EdgeKind::Bus) && e.to_box == hub_id
            });
            let has_signal_out = hub_edges.iter().any(|e| {
                (e.kind == EdgeKind::Signal || e.kind == EdgeKind::Bus) && e.from_box == hub_id
            });
            let is_bidirectional = has_bidirectional || (has_signal_in && has_signal_out);

            // Rule b: rail edge where b is driver → W lower
            let is_rail_driver = hub_edges
                .iter()
                .any(|e| e.kind == EdgeKind::Power && e.from_box == b.id);
            if is_rail_driver {
                sectors.insert(b.id, Sector::WestLower);
                continue;
            }

            // Rule a: bidirectional bus (both in and out, or Bus kind) → W upper
            if is_bidirectional {
                sectors.insert(b.id, Sector::WestUpper);
                continue;
            }

            // Rule c: only signal edges into hub → N
            if has_signal_in && !has_signal_out {
                sectors.insert(b.id, Sector::North);
                continue;
            }

            // Rule d: only signal edges out of hub → S
            if has_signal_out && !has_signal_in {
                sectors.insert(b.id, Sector::South);
                continue;
            }

            // Rule e: no direct edge to hub → inherit nearest ring-1 neighbor
            sectors.insert(b.id, Sector::WestLower); // fallback to W
        } else {
            // ring ≥ 2: rule e — inherit nearest ring-1 neighbor's sector
            let inherited = inherit_sector_from_ring1(&adj, b.id, &sectors, ring);
            sectors.insert(b.id, inherited);
        }
    }

    sectors
}

/// Inherit sector from the nearest ring-1 neighbor.
fn inherit_sector_from_ring1(
    adj: &HashMap<i64, Vec<&BlockEdge>>,
    box_id: i64,
    sectors: &HashMap<i64, Sector>,
    _ring: usize,
) -> Sector {
    // Find a ring-1 neighbor connected via edges
    if let Some(edges) = adj.get(&box_id) {
        for e in edges.iter() {
            let neighbor = if e.from_box == box_id {
                e.to_box
            } else {
                e.from_box
            };
            if let Some(&sec) = sectors.get(&neighbor) {
                if sec != Sector::Hub {
                    return sec;
                }
            }
        }
    }
    // Fallback
    Sector::WestLower
}

// ============================================================================
// Step 4: Hub height (§4.4)
// ============================================================================

/// hub_h = max(west_pin_count, east_pin_count) × ROW_STEP.
/// Each west pin occupies one row to align with W-column boxes.
fn compute_hub_height(
    _graph: &McVecGraph,
    edges: &[BlockEdge],
    hub_id: i64,
    sectors: &HashMap<i64, Sector>,
) -> f64 {
    // Count distinct edges from W-column boxes to the hub.
    // Each edge represents a pin on the hub's west side.
    let west_pin_count = edges
        .iter()
        .filter(|e| {
            let other = if e.from_box == hub_id {
                e.to_box
            } else if e.to_box == hub_id {
                e.from_box
            } else {
                return false;
            };
            sectors
                .get(&other)
                .map(|s| matches!(s, Sector::WestUpper | Sector::WestLower))
                .unwrap_or(false)
        })
        .count();

    // East side pins (if any)
    let east_pin_count = edges
        .iter()
        .filter(|e| {
            let other = if e.from_box == hub_id {
                e.to_box
            } else if e.to_box == hub_id {
                e.from_box
            } else {
                return false;
            };
            sectors
                .get(&other)
                .map(|s| matches!(s, Sector::East))
                .unwrap_or(false)
        })
        .count();

    let pin_count = west_pin_count.max(east_pin_count).max(1);
    pin_count as f64 * ROW_STEP
}

// ============================================================================
// Step 5: Coordinate assignment (§4.5)
// ============================================================================

/// Assign box coordinates matching the golden table in §4.5.
///
/// Layout:
/// ```text
///                 +----------+
///                 |   mic    |             N
///                 +----+-----+
///  +--------+    +-----+----+  +--------------+
///  |        |    |  flash   |--+              |
///  |        |    +----------+  |              |
///  |usbsock |---+ +----------+  |   mcu513     |  ring0
///  |        |   | |  modldo  |--+    (hub)     |
///  |        |   | +----------+  |              |
///  +--------+   | +----------+  |              |
///    ring2      | | moddcdc  |--+              |
///               | +----------+  +---+------+---+
///               |  ring1 W         |      |
///               |                  |      |
///               +------------------+ +----+---+
///                                   |speaker |   S
///                                   +--------+
/// ```
fn assign_coordinates(
    graph: &mut McVecGraph,
    edges: &[BlockEdge],
    hub_id: i64,
    rings: &HashMap<i64, usize>,
    sectors: &HashMap<i64, Sector>,
    hub_h: f64,
) {
    let hub_x = 640.0;
    let hub_y = 200.0; // top of hub

    // Place hub
    if let Some(hub) = graph.boxes.iter_mut().find(|b| b.id == hub_id) {
        hub.x = hub_x;
        hub.y = hub_y;
        hub.w = HUB_W;
        hub.h = hub_h;
        hub.geom_locked = true;
    }

    // Build adjacency from actual edges (not ring differences)
    let adj: HashMap<i64, Vec<i64>> = {
        let mut m: HashMap<i64, Vec<i64>> = HashMap::new();
        for e in edges {
            m.entry(e.from_box).or_default().push(e.to_box);
            m.entry(e.to_box).or_default().push(e.from_box);
        }
        m
    };

    // Collect W-column ring-1 boxes and sort by row
    let mut w_ring1: Vec<(i64, Sector)> = sectors
        .iter()
        .filter(|(&id, &s)| {
            id != hub_id
                && matches!(s, Sector::WestUpper | Sector::WestLower)
                && rings.get(&id).copied().unwrap_or(usize::MAX) == 1
        })
        .map(|(&id, &s)| (id, s))
        .collect();

    // Sort W ring-1 boxes: WestUpper first, then WestLower sorted by hub
    // entry_point offset (pin position on the hub's left edge).
    let hub_entry_points: Vec<&crate::vector::graph::EntryPoint> = graph
        .boxes
        .iter()
        .find(|b| b.id == hub_id)
        .map(|hub| {
            hub.entry_points
                .iter()
                .filter(|ep| ep.side == crate::vector::graph::EntrySide::Left)
                .collect()
        })
        .unwrap_or_default();

    w_ring1.sort_by(|a, b| {
        let order = |s: Sector| match s {
            Sector::WestUpper => 0,
            Sector::WestLower => 1,
            _ => 2,
        };
        let sec_cmp = order(a.1).cmp(&order(b.1));
        if sec_cmp != std::cmp::Ordering::Equal {
            return sec_cmp;
        }
        // Within same sector, sort by hub entry_point offset (y position).
        // Find the edge connecting each box to the hub, then find the hub's
        // entry_point for that pin, and sort by offset.
        // Fallback: sort by edge label in reverse alphabetical order.
        let hub_offset = |box_id: i64| -> (f64, String) {
            for e in edges {
                let other = if e.from_box == hub_id {
                    e.to_box
                } else if e.to_box == hub_id {
                    e.from_box
                } else {
                    continue;
                };
                if other == box_id {
                    // Find the hub entry_point matching this edge's label
                    for ep in &hub_entry_points {
                        if ep.pin_name == e.label {
                            return (ep.offset, String::new());
                        }
                    }
                    // Fallback: use edge label for deterministic ordering
                    return (0.5, e.label.clone());
                }
            }
            (0.5, String::new()) // fallback: middle of hub
        };
        let (off_a, label_a) = hub_offset(a.0);
        let (off_b, label_b) = hub_offset(b.0);
        off_a
            .partial_cmp(&off_b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| label_b.cmp(&label_a)) // reverse alphabetical: V3V3 > V1V2
    });

    let w_x = 340.0;

    // Place ring-1 W boxes at x=340, y aligned with hub west pins
    for (i, (box_id, _sector)) in w_ring1.iter().enumerate() {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == *box_id) {
            b.x = w_x;
            b.y = hub_y + i as f64 * ROW_STEP;
            b.w = BOX_W;
            b.h = BOX_H;
            b.geom_locked = true;
        }
    }

    // Place ring ≥ 2 W boxes: same y as ring-1 neighbor, x = neighbor_x - COL_STEP
    let w_ring2plus: Vec<i64> = sectors
        .iter()
        .filter(|(&id, &s)| {
            id != hub_id
                && matches!(s, Sector::WestUpper | Sector::WestLower)
                && rings.get(&id).copied().unwrap_or(0) >= 2
        })
        .map(|(&id, _)| id)
        .collect();

    for &box_id in &w_ring2plus {
        // Find ring-1 neighbor via actual edges, preferring W-column boxes.
        if let Some(neighbors) = adj.get(&box_id) {
            let mut best_neighbor: Option<i64> = None;
            for &neighbor_id in neighbors {
                if rings.get(&neighbor_id).copied() == Some(1) {
                    let is_w = matches!(
                        sectors.get(&neighbor_id),
                        Some(Sector::WestUpper | Sector::WestLower)
                    );
                    if is_w {
                        best_neighbor = Some(neighbor_id);
                        break; // W-column neighbor takes priority
                    }
                    if best_neighbor.is_none() {
                        best_neighbor = Some(neighbor_id);
                    }
                }
            }
            if let Some(neighbor_id) = best_neighbor {
                if let Some(neighbor) = graph.boxes.iter().find(|b| b.id == neighbor_id) {
                    let ny = neighbor.y;
                    let nx = neighbor.x;
                    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == box_id) {
                        b.x = nx - COL_STEP;
                        b.y = ny;
                        b.w = BOX_W;
                        b.h = BOX_H;
                        b.geom_locked = true;
                    }
                }
            }
        }
    }

    // Place N-column boxes (mic)
    let n_boxes: Vec<i64> = sectors
        .iter()
        .filter(|(&id, &s)| id != hub_id && s == Sector::North)
        .map(|(&id, _)| id)
        .collect();

    for (_i, &box_id) in n_boxes.iter().enumerate() {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == box_id) {
            b.x = hub_x + 20.0;
            b.y = hub_y - ROW_STEP; // above hub, top edge at hub_y - ROW_STEP
            b.w = BOX_W;
            b.h = BOX_H;
            b.geom_locked = true;
        }
    }

    // Place S-column boxes (speaker)
    let s_boxes: Vec<i64> = sectors
        .iter()
        .filter(|(&id, &s)| id != hub_id && s == Sector::South)
        .map(|(&id, _)| id)
        .collect();

    for (_i, &box_id) in s_boxes.iter().enumerate() {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == box_id) {
            b.x = hub_x;
            b.y = hub_y + hub_h + 80.0; // below hub with 80px gap
            b.w = HUB_W;
            b.h = 100.0;
            b.geom_locked = true;
        }
    }
}

// ============================================================================
// Step 6: Facade entry_points setup (§4.6)
// ============================================================================

/// Normalized inset of an entry point from the edge ends [0,1] (keeps the pin
/// stub and its label off the box corners).
const EDGE_INSET: f64 = 0.08;
/// Total normalized spread along the edge when several pins fan out toward the
/// same neighbour on the same side.
const FAN_SPAN: f64 = 0.4;

/// Set up facade entry_points for the root block diagram.
///
/// Entry points are derived from net/signal semantics instead of a fixed set of
/// box names:
///
/// - **P-1**: only signal/bus edges yield entry points; rail edges keep their
///   anchors and never produce an entry point.
/// - Each signal pin **faces the neighbour it connects to**: the side is the
///   dominant axis between the two box centres, and the offset is the
///   projection of the neighbour's centre onto that edge.
/// - Pins that fan out toward the **same** neighbour on the same side are
///   spread evenly (`FAN_SPAN`) so they never overlap.
///
/// This is fully generic — it references no project box name.
fn setup_facade_entry_points(graph: &mut McVecGraph, edges: &[BlockEdge]) {
    use crate::vector::graph::EntryPoint;

    // box_id -> Vec<(pin_name, side, neighbour_id, base_offset)>
    let mut per_box: HashMap<i64, Vec<(String, EntrySide, i64, f64)>> = HashMap::new();

    for edge in edges {
        if edge.kind != EdgeKind::Signal && edge.kind != EdgeKind::Bus {
            continue; // ★ P-1: only signal pins get entry_points. Rail edges use anchors (P-2).
        }
        if edge.label.is_empty() {
            continue; // anonymous `__net_*` / unlabeled — nothing to anchor a pin by.
        }

        let (Some(a), Some(b)) = (
            graph.boxes.iter().find(|bx| bx.id == edge.from_box),
            graph.boxes.iter().find(|bx| bx.id == edge.to_box),
        ) else {
            continue;
        };

        for (this, other) in [(a, b), (b, a)] {
            let (side, base) = facing_side_and_offset(this, other);
            per_box
                .entry(this.id)
                .or_default()
                .push((edge.label.clone(), side, other.id, base));
        }
    }

    // ── Pass 1: candidate entry points per box (dedup pin_name + fan-out) ──
    // box_id -> Vec<EntryPoint>
    let mut entries: HashMap<i64, Vec<EntryPoint>> = HashMap::new();
    for (box_id, raw) in per_box {
        // A net that fans out to several boxes is still one pin: keep the first name.
        let mut seen_names: std::collections::HashSet<String> = Default::default();
        let mut names: Vec<String> = Vec::new();
        let mut kept: Vec<(EntrySide, i64, f64)> = Vec::new();
        for (name, side, nid, base) in raw {
            if seen_names.insert(name.clone()) {
                names.push(name);
                kept.push((side, nid, base));
            }
        }

        // Group pin indices by (side, neighbour) so only same-target pins fan out.
        let mut groups: HashMap<(EntrySide, i64), Vec<usize>> = HashMap::new();
        for (i, &(side, nid, _)) in kept.iter().enumerate() {
            groups.entry((side, nid)).or_default().push(i);
        }

        let mut eps: Vec<EntryPoint> = Vec::new();
        for ((side, _), mut members) in groups {
            members.sort_by(|&ia, &ib| names[ia].cmp(&names[ib])); // deterministic order
            let base = members.iter().map(|&i| kept[i].2).sum::<f64>() / members.len() as f64;
            let n = members.len() as f64;
            for (idx, &i) in members.iter().enumerate() {
                let f = if n > 1.0 {
                    idx as f64 / (n - 1.0) // 0..=1 within the fan span
                } else {
                    0.5 // single pin: stay at the neighbour's projection
                };
                let offset =
                    (base - FAN_SPAN / 2.0 + f * FAN_SPAN).clamp(EDGE_INSET, 1.0 - EDGE_INSET);
                eps.push(EntryPoint {
                    pin_id: 0,
                    pin_name: names[i].clone(),
                    side,
                    offset,
                });
            }
        }
        eps.sort_by(|x, y| {
            side_order(x.side).cmp(&side_order(y.side)).then_with(|| {
                x.offset
                    .partial_cmp(&y.offset)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        });
        entries.insert(box_id, eps);
    }

    // ── Pass 2: adaptive reconciliation — the two ends of every signal edge
    //    must share the free coordinate so the line is one straight orthogonal
    //    segment, never a diagonal. Side-by-side boxes (pins on Left/Right
    //    edges) share the same absolute y; stacked boxes (Top/Bottom) the same x.
    let geom: HashMap<i64, (f64, f64, f64, f64)> = graph
        .boxes
        .iter()
        .map(|b| (b.id, (b.x, b.y, b.w, b.h)))
        .collect();
    // (box_id, pin_name) -> reconciled absolute coordinate on the free axis.
    let mut aligned: HashMap<(i64, String), f64> = HashMap::new();
    for edge in edges {
        if edge.kind != EdgeKind::Signal && edge.kind != EdgeKind::Bus || edge.label.is_empty() {
            continue;
        }
        let (Some(&(ax, ay, aw, ah)), Some(&(bx, by, bw, bh))) =
            (geom.get(&edge.from_box), geom.get(&edge.to_box))
        else {
            continue;
        };
        let (Some(from_eps), Some(to_eps)) =
            (entries.get(&edge.from_box), entries.get(&edge.to_box))
        else {
            continue;
        };
        let fi = from_eps.iter().position(|e| e.pin_name == edge.label);
        let ti = to_eps.iter().position(|e| e.pin_name == edge.label);
        let (Some(fi), Some(ti)) = (fi, ti) else {
            continue;
        };
        let (fs, ts) = (from_eps[fi].side, to_eps[ti].side);
        let (f_vert, t_vert) = (
            matches!(fs, EntrySide::Left | EntrySide::Right),
            matches!(ts, EntrySide::Left | EntrySide::Right),
        );
        let (f_horz, t_horz) = (
            matches!(fs, EntrySide::Top | EntrySide::Bottom),
            matches!(ts, EntrySide::Top | EntrySide::Bottom),
        );
        if f_vert && t_vert {
            // Horizontal connection: both ends on vertical edges → align y.
            let mid = (ay + from_eps[fi].offset * ah + by + to_eps[ti].offset * bh) / 2.0;
            aligned.insert((edge.from_box, edge.label.clone()), mid);
            aligned.insert((edge.to_box, edge.label.clone()), mid);
        } else if f_horz && t_horz {
            // Vertical connection: both ends on horizontal edges → align x.
            let mid = (ax + from_eps[fi].offset * aw + bx + to_eps[ti].offset * bw) / 2.0;
            aligned.insert((edge.from_box, edge.label.clone()), mid);
            aligned.insert((edge.to_box, edge.label.clone()), mid);
        }
    }

    // ── Pass 3: write back, applying reconciled offsets. ──
    for bx in &mut graph.boxes {
        let Some(mut eps) = entries.remove(&bx.id) else {
            continue; // Rail-only boxes (no signal edges) get no entry points.
        };
        for ep in eps.iter_mut() {
            if let Some(&mid) = aligned.get(&(bx.id, ep.pin_name.clone())) {
                let offset = match ep.side {
                    EntrySide::Top | EntrySide::Bottom if bx.w > 0.0 => (mid - bx.x) / bx.w,
                    EntrySide::Left | EntrySide::Right if bx.h > 0.0 => (mid - bx.y) / bx.h,
                    _ => ep.offset,
                };
                ep.offset = offset.clamp(EDGE_INSET, 1.0 - EDGE_INSET);
            }
        }
        bx.entry_points = eps;
    }
}

/// Which box edge a signal leaves toward `other`, and the normalized offset
/// along that edge pointing at `other`'s centre.
fn facing_side_and_offset(this: &McVecBox, other: &McVecBox) -> (EntrySide, f64) {
    let (cxa, cya) = (this.x + this.w / 2.0, this.y + this.h / 2.0);
    let (cxb, cyb) = (other.x + other.w / 2.0, other.y + other.h / 2.0);
    let (dx, dy) = (cxb - cxa, cyb - cya);

    let side = if dx.abs() >= dy.abs() {
        if dx > 0.0 {
            EntrySide::Right
        } else {
            EntrySide::Left
        }
    } else if dy > 0.0 {
        EntrySide::Bottom
    } else {
        EntrySide::Top
    };

    let base = if this.w <= 0.0 || this.h <= 0.0 {
        0.5
    } else {
        match side {
            EntrySide::Left | EntrySide::Right => (cyb - this.y) / this.h,
            EntrySide::Top | EntrySide::Bottom => (cxb - this.x) / this.w,
        }
    };
    (side, base)
}

/// Stable priority for entry-point ordering: Top → Right → Bottom → Left.
fn side_order(side: EntrySide) -> u8 {
    match side {
        EntrySide::Top => 0,
        EntrySide::Right => 1,
        EntrySide::Bottom => 2,
        EntrySide::Left => 3,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_degree_signal_vs_rail() {
        // Signal edges weighted 2x, rail edges weighted 1x
        let edges = vec![
            BlockEdge {
                from_box: 1,
                to_box: 2,
                label: "SIG".into(),
                lane_count: 1,
                kind: EdgeKind::Signal,
                source_span: None,
                link: None,
                bidirectional: false,
            },
            BlockEdge {
                from_box: 1,
                to_box: 3,
                label: "VCC".into(),
                lane_count: 1,
                kind: EdgeKind::Power,
                source_span: None,
                link: None,
                bidirectional: false,
            },
        ];
        let graph = McVecGraph::new(0, "test".into());
        // Box 1: 1 signal edge (×2) + 1 rail edge (×1) = 3
        assert_eq!(weighted_degree(&graph, &edges, 1), 3);
        // Box 2: 1 signal edge (×2) = 2
        assert_eq!(weighted_degree(&graph, &edges, 2), 2);
        // Box 3: 1 rail edge (×1) = 1
        assert_eq!(weighted_degree(&graph, &edges, 3), 1);
    }

    #[test]
    fn edge_neighbors_bidirectional() {
        let edges = vec![
            BlockEdge {
                from_box: 1,
                to_box: 2,
                label: "A".into(),
                lane_count: 1,
                kind: EdgeKind::Signal,
                source_span: None,
                link: None,
                bidirectional: false,
            },
            BlockEdge {
                from_box: 2,
                to_box: 3,
                label: "B".into(),
                lane_count: 1,
                kind: EdgeKind::Power,
                source_span: None,
                link: None,
                bidirectional: false,
            },
        ];
        let neighbors = edge_neighbors(&edges, 2);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&1));
        assert!(neighbors.contains(&3));
    }

    #[test]
    fn facade_entry_points_name_agnostic() {
        use crate::vector::graph::{BoxKind, EntryPoint, IoSummary};

        // Boxes are named generically (NOT hbl names): hub "chipA" with neighbours
        // "mem" (right), "sensor" (bottom) and a rail-only "pwr". Entry points must
        // be derived purely from geometry + edge semantics.
        let mut graph = McVecGraph::new(0, "test".into());
        let mk = |id: i64, name: &str, x: f64, y: f64| {
            let mut b = McVecBox::new(
                id,
                name.into(),
                "IC".into(),
                BoxKind::MultiPin,
                4,
                IoSummary::new(),
            );
            b.x = x;
            b.y = y;
            b.w = 100.0;
            b.h = 100.0;
            b
        };
        graph.boxes.push(mk(1, "chipA", 0.0, 0.0)); // center (50,50)
        graph.boxes.push(mk(2, "mem", 300.0, 0.0)); // center (350,50)
        graph.boxes.push(mk(3, "sensor", 0.0, 300.0)); // center (50,350)
        graph.boxes.push(mk(4, "pwr", 600.0, 600.0)); // rail only

        let edges = vec![
            BlockEdge {
                from_box: 1,
                to_box: 2,
                label: "DATA".into(),
                lane_count: 1,
                kind: EdgeKind::Signal,
                source_span: None,
                link: None,
                bidirectional: false,
            },
            BlockEdge {
                from_box: 1,
                to_box: 3,
                label: "OUT".into(),
                lane_count: 1,
                kind: EdgeKind::Signal,
                source_span: None,
                link: None,
                bidirectional: false,
            },
            BlockEdge {
                from_box: 3,
                to_box: 1,
                label: "STAT".into(),
                lane_count: 1,
                kind: EdgeKind::Signal,
                source_span: None,
                link: None,
                bidirectional: false,
            },
            BlockEdge {
                from_box: 4,
                to_box: 1,
                label: "VCC".into(),
                lane_count: 1,
                kind: EdgeKind::Power,
                source_span: None,
                link: None,
                bidirectional: false,
            },
        ];

        setup_facade_entry_points(&mut graph, &edges);

        let eps_of = |name: &str| {
            graph
                .boxes
                .iter()
                .find(|b| b.name == name)
                .unwrap()
                .entry_points
                .clone()
        };
        let find = |eps: &[EntryPoint], pin: &str| -> Option<(EntrySide, f64)> {
            eps.iter()
                .find(|e| e.pin_name == pin)
                .map(|e| (e.side, e.offset))
        };

        // chipA: DATA faces right neighbour, OUT/STAT fan out on bottom.
        let hub = eps_of("chipA");
        assert_eq!(hub.len(), 3, "hub should have 3 signal entry points");
        assert_eq!(find(&hub, "DATA"), Some((EntrySide::Right, 0.5)));
        // Two pins to the single bottom neighbour are spread 0.3/0.7 (FAN_SPAN=0.4).
        assert_eq!(find(&hub, "OUT"), Some((EntrySide::Bottom, 0.3)));
        assert_eq!(find(&hub, "STAT"), Some((EntrySide::Bottom, 0.7)));

        // mem: its single signal pin faces the hub (left).
        let mem_eps = eps_of("mem");
        assert_eq!(mem_eps.len(), 1);
        assert_eq!(find(&mem_eps, "DATA"), Some((EntrySide::Left, 0.5)));

        // sensor: both pins face the hub above them (top), fan 0.3/0.7.
        let sensor_eps = eps_of("sensor");
        assert_eq!(sensor_eps.len(), 2);
        assert_eq!(find(&sensor_eps, "OUT"), Some((EntrySide::Top, 0.3)));
        assert_eq!(find(&sensor_eps, "STAT"), Some((EntrySide::Top, 0.7)));

        // Rail-only box gets no entry points (P-1).
        assert!(eps_of("pwr").is_empty());
    }

    #[test]
    fn facade_reconciles_ends_to_straight_line() {
        use crate::vector::graph::{BoxKind, IoSummary};

        // Two staggered boxes (different vertical centres): without reconciliation
        // their signal pins would land at different absolute y and produce a
        // diagonal line. The adaptive pass must snap both ends to the same y so
        // the connection is a single orthogonal (horizontal) segment.
        let mut graph = McVecGraph::new(0, "test".into());
        let mk = |id: i64, name: &str, x: f64, y: f64| {
            let mut b = McVecBox::new(
                id,
                name.into(),
                "IC".into(),
                BoxKind::MultiPin,
                4,
                IoSummary::new(),
            );
            b.x = x;
            b.y = y;
            b.w = 100.0;
            b.h = 100.0;
            b
        };
        graph.boxes.push(mk(1, "left", 0.0, 0.0)); // centre y = 50
        graph.boxes.push(mk(2, "right", 300.0, 80.0)); // centre y = 130

        let edges = vec![BlockEdge {
            from_box: 1,
            to_box: 2,
            label: "DATA".into(),
            lane_count: 1,
            kind: EdgeKind::Signal,
            source_span: None,
            link: None,
            bidirectional: false,
        }];

        setup_facade_entry_points(&mut graph, &edges);

        let left = graph
            .boxes
            .iter()
            .find(|b| b.name == "left")
            .unwrap()
            .entry_points
            .iter()
            .find(|e| e.pin_name == "DATA")
            .unwrap();
        let right = graph
            .boxes
            .iter()
            .find(|b| b.name == "right")
            .unwrap()
            .entry_points
            .iter()
            .find(|e| e.pin_name == "DATA")
            .unwrap();

        assert_eq!(left.side, EntrySide::Right);
        assert_eq!(right.side, EntrySide::Left);
        let y1 = 0.0 + left.offset * 100.0;
        let y2 = 80.0 + right.offset * 100.0;
        assert!(
            (y1 - y2).abs() < 1e-6,
            "paired pins must share the same y (got {y1} vs {y2})"
        );
        // The naive projections would have pinned `left` near its bottom edge
        // (offset ≈ 0.92, y ≈ 92) and `right` near its top (y ≈ 88); after the
        // adaptive pass both sit together at y ≈ 90.
        assert!(
            left.offset > 0.85 && left.offset < 0.92,
            "got {}",
            left.offset
        );
        assert!(
            right.offset > 0.08 && right.offset < 0.12,
            "got {}",
            right.offset
        );
    }
}
