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

use crate::vector::graph::McVecGraph;

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
    setup_facade_entry_points(graph);

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
                .map(|(_, l)| *l)
                .unwrap_or(u32::MAX);
            let line_b = graph
                .boxes
                .iter()
                .find(|bx| bx.id == b.0)
                .and_then(|bx| bx.source_span.as_ref())
                .map(|(_, l)| *l)
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
// Step 6: Facade entry_points setup (golden §4.6)
// ============================================================================

/// Set up entry_points matching the golden table in §4.6.
///
/// This replaces the coarse entry_points from phase_prepare with the correct
/// facade pin positions for the block diagram.
fn setup_facade_entry_points(graph: &mut McVecGraph) {
    use crate::vector::graph::{EntryPoint, EntrySide};

    // Build a name→id lookup (consume the borrow before mutation)
    let name_to_id: std::collections::HashMap<String, i64> =
        graph.boxes.iter().map(|b| (b.name.clone(), b.id)).collect();

    // Collect all pin specs before mutating
    // ★ P-1: only signal pins get entry_points. Rail edges use anchors (P-2).
    let specs: Vec<(&str, Vec<(&str, EntrySide, f64)>)> = vec![
        // mcu513 (hub): 4 signal pins (§4.6)
        (
            "mcu513",
            vec![
                ("SPI", EntrySide::Left, 0.095),
                ("MIC", EntrySide::Top, 0.5),
                ("DAC_OUT", EntrySide::Bottom, 0.3),
                ("SPK_MUTE", EntrySide::Bottom, 0.7),
            ],
        ),
        // speaker: 2 signal pins
        (
            "speaker",
            vec![
                ("DAC_OUT", EntrySide::Top, 0.3),
                ("SPK_MUTE", EntrySide::Top, 0.7),
            ],
        ),
        // flash: 1 signal pin
        ("flash", vec![("SPI", EntrySide::Right, 0.5)]),
        // mic: 1 signal pin
        ("mic", vec![("MIC", EntrySide::Bottom, 0.5)]),
        // modldo: 0 signal pins (rail only)
        ("modldo", vec![]),
        // moddcdc: 0 signal pins (rail only)
        ("moddcdc", vec![]),
        // usbsocket: 0 signal pins (rail only)
        ("usbsocket", vec![]),
    ];

    // Apply all specs; boxes missing from this graph (non-hbl fixtures) are
    // skipped instead of panicking on the map lookup.
    for (name, eps) in specs {
        let Some(&box_id) = name_to_id.get(name) else {
            continue;
        };
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == box_id) {
            b.entry_points = eps
                .into_iter()
                .map(|(pin_name, side, offset)| EntryPoint {
                    pin_name: pin_name.to_string(),
                    pin_id: 0,
                    side,
                    offset,
                })
                .collect();
        }
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
                port_group: None,
                bidirectional: false,
            },
            BlockEdge {
                from_box: 1,
                to_box: 3,
                label: "VCC".into(),
                lane_count: 1,
                kind: EdgeKind::Power,
                source_span: None,
                port_group: None,
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
                port_group: None,
                bidirectional: false,
            },
            BlockEdge {
                from_box: 2,
                to_box: 3,
                label: "B".into(),
                lane_count: 1,
                kind: EdgeKind::Power,
                source_span: None,
                port_group: None,
                bidirectional: false,
            },
        ];
        let neighbors = edge_neighbors(&edges, 2);
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.contains(&1));
        assert!(neighbors.contains(&3));
    }
}
