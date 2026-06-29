// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW —— Star multi-endpoint routing
//!
//! ## Motivation
//! One VCC connects 5 chips + 10 capacitors = 15 endpoints.
//! The pairwise model splits this into 105 wires → cluttered mess.
//!
//! Correct approach: pick **one center point** (network name label / geometric centroid),
//! connect all endpoints → center point → visually clean.
//!
//! ## Algorithm
//! 1. Find geometric centroid = average of all endpoint exit points
//! 2. For each endpoint → center, draw Manhattan polyline
//! 3. Place a junction at center (for renderer to draw T-shaped node)
//!
//! ## Applicable NetKind
//! - `Power` / `Ground`: power / ground, naturally star-shaped
//! - `Signal`: single-driver multi-receiver (1 → N) signals
//! - `SubModuleIO`: cross-module port connections after promotion
//!
//! ## Not applicable
//! - `Bus(n)`: use [`super::bus_bundle::BusBundleRouter`] to draw thick line + taps
//!
//! ## ★ P09 (S5) refactor
//! The geometric centroid often **lands inside some box** (especially when
//! multi-endpoint net endpoints surround the main chip). At this point going
//! from center outward to endpoints = passing through chips. `pick_hub_point`
//! detects whether the geometric centroid is inside any obstacle, and if so,
//! scans radially outward looking for a clear position.

use crate::vector::graph::net_def::IoDirection;
use crate::vector::graph::{BoxKind, McVecGraph, Point, Route, Segment, VizNet};

use super::obstacles::{best_orthogonal_path, ObstacleMap};
use super::orthogonal::orthogonal_path;
use super::side::{compute_exit_for_pin, ExitSide};
use crate::viz::traits::Router;

pub struct StarRouter;

impl Router for StarRouter {
    fn route(&self, graph: &McVecGraph, net: &mut VizNet) {
        let mut route = Route::new();

        if net.endpoints.len() < 2 {
            net.route = Some(route);
            return;
        }

        // Degenerate: 2 endpoints → direct Manhattan, no star
        if net.endpoints.len() == 2 {
            let a = &net.endpoints[0];
            let b = &net.endpoints[1];
            let ba = graph.boxes.iter().find(|x| x.id == a.box_id);
            let bb = graph.boxes.iter().find(|x| x.id == b.box_id);
            if let (Some(ba), Some(bb)) = (ba, bb) {
                let (sp, ss) = compute_exit_for_pin(ba, a.pin_id, Some(bb));
                let (dp, ds) = compute_exit_for_pin(bb, b.pin_id, Some(ba));
                let pts = orthogonal_path(sp, dp, ss, ds);
                for w in pts.windows(2) {
                    route.segments.push(Segment {
                        from: Point::new(w[0].0, w[0].1),
                        to: Point::new(w[1].0, w[1].1),
                    });
                }
            }
            net.route = Some(route);
            return;
        }

        // ── Multi-endpoint: star ──

        // Step 1: compute each endpoint's exit point (pretend target is geometric centroid)
        let positions: Vec<(f64, f64)> = net
            .endpoints
            .iter()
            .filter_map(|e| {
                graph
                    .boxes
                    .iter()
                    .find(|b| b.id == e.box_id)
                    .map(|b| (b.x + b.w / 2.0, b.y + b.h / 2.0))
            })
            .collect();
        if positions.is_empty() {
            net.route = Some(route);
            return;
        }

        // Step 2: ★ P09 pick hub point (obstacle-aware)
        // Build obstacle map excluding own endpoint boxes
        let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
        let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);
        let (cx, cy) = pick_hub_point(&positions, &obstacles);
        let center = Point::new(cx, cy);

        // Step 3: each endpoint → center runs Manhattan
        for (idx, ep) in net.endpoints.iter().enumerate() {
            let pos = positions[idx];
            let bx = graph.boxes.iter().find(|x| x.id == ep.box_id);
            if let Some(b) = bx {
                // Exit direction: towards center
                let dx = cx - pos.0;
                let dy = cy - pos.1;
                let s_side = if dx.abs() > dy.abs() {
                    if dx > 0.0 {
                        ExitSide::Right
                    } else {
                        ExitSide::Left
                    }
                } else if dy > 0.0 {
                    ExitSide::Bottom
                } else {
                    ExitSide::Top
                };
                let (sp, ss) = compute_exit_for_pin(b, ep.pin_id, None);
                // Override with computed direction (compute_exit_for_pin defaults to Right without target)
                let ss = if b.find_entry(ep.pin_id).is_some() {
                    ss
                } else {
                    s_side
                };

                // Center defaults to entering from opposite side
                let d_side = match ss {
                    ExitSide::Right => ExitSide::Left,
                    ExitSide::Left => ExitSide::Right,
                    ExitSide::Bottom => ExitSide::Top,
                    ExitSide::Top => ExitSide::Bottom,
                };

                let pts = orthogonal_path(sp, (cx, cy), ss, d_side);
                for w in pts.windows(2) {
                    route.segments.push(Segment {
                        from: Point::new(w[0].0, w[0].1),
                        to: Point::new(w[1].0, w[1].1),
                    });
                }
            }
        }

        // Step 4: place a junction at center (only when ≥ 3 endpoints, 2 endpoints is meaningless)
        if net.endpoints.len() >= 3 {
            route.junctions.push(center);
        }

        net.route = Some(route);
    }

    fn name(&self) -> &'static str {
        "star"
    }
}

// ============================================================================
// ★ P09 (S5) pick_hub_point — select a hub point that is not inside an obstacle
// ============================================================================

/// Given a set of endpoint positions, pick a point suitable for the star hub
///
/// ## Algorithm
/// 1. Compute geometric centroid (average of endpoint positions)
/// 2. If the center is **not** inside any obstacle, use it directly (common case)
/// 3. Otherwise, scan with increasing radius (20px / 40 / 60 ...), 12 angles per ring,
///    find the first point not inside any obstacle (fallback)
/// 4. If all fail (no clean spot in 20 rings) still return the geometric centroid
///    (allow collision, the user can see the problem)
pub fn pick_hub_point(positions: &[(f64, f64)], obstacles: &ObstacleMap) -> (f64, f64) {
    if positions.is_empty() {
        return (0.0, 0.0);
    }
    let n = positions.len() as f64;
    let cx = positions.iter().map(|p| p.0).sum::<f64>() / n;
    let cy = positions.iter().map(|p| p.1).sum::<f64>() / n;

    if !obstacles.point_inside_any(cx, cy) {
        return (cx, cy);
    }

    // Center is covered by obstacle, spiral scan
    let radius_step = 20.0_f64;
    for ring in 1..20 {
        let r = ring as f64 * radius_step;
        for angle_deg in (0..360).step_by(30) {
            let a = (angle_deg as f64).to_radians();
            let tx = cx + r * a.cos();
            let ty = cy + r * a.sin();
            if !obstacles.point_inside_any(tx, ty) {
                crate::vlog!(
                    "[route::star] hub moved from ({cx:.0},{cy:.0}) to ({tx:.0},{ty:.0}) \
                     (ring {ring}, blocker avoidance)"
                );
                return (tx, ty);
            }
        }
    }

    // Fallback
    crate::vlog!(
        "[route::star] WARN hub ({cx:.0},{cy:.0}) inside obstacle, no clear spot found in 20 rings"
    );
    (cx, cy)
}

// ============================================================================
// ★ FIX (collision) — obstacle-aware polyline for spokes
// ============================================================================

/// Compute polyline segments for a single spoke (peer pin → hub pin), avoiding other boxes.
///
/// First try natural Manhattan path by exit direction of both ends (`ss`/`ds`); if that
/// path crosses any obstacle box, fall back to [`best_orthogonal_path`] which picks the
/// non-colliding and shortest among 4 L/Z candidates; if still collides, detour.
/// Consistent with the orthogonal router's obstacle-avoidance semantics. Previously
/// route_hub_star called `orthogonal_path` directly without checking obstacles, so the
/// fanned-out lines often crossed through the middle boxes (user-reported "wire collisions").
fn spoke_segments(
    obstacles: &ObstacleMap,
    sp: (f64, f64),
    dp: (f64, f64),
    ss: ExitSide,
    ds: ExitSide,
) -> Vec<(f64, f64, f64, f64)> {
    let pts = orthogonal_path(sp, dp, ss, ds);
    let segs: Vec<(f64, f64, f64, f64)> = pts
        .windows(2)
        .map(|w| (w[0].0, w[0].1, w[1].0, w[1].1))
        .collect();
    if obstacles.first_hit(&segs).is_none() {
        segs
    } else {
        best_orthogonal_path(sp.0, sp.1, dp.0, dp.1, obstacles)
    }
}

// ============================================================================
// ★ FIX (sub-graph) — hub-star: radiate from "main device pin" as hub
//                    (multiple wires fanning out from the component)
// ============================================================================

/// Use the **main device (IC/SubModule/driver) pin in the net** as hub and draw
/// orthogonal lines to the remaining endpoints —— multiple wires fan out from the
/// device, replacing the TrunkTap / BusBundle "shared trunk + single-point box entry".
///
/// Two forms:
/// - **Hub side has only 1 pin** (ordinary single-driver net): all peers converge to
///   that pin, drop a junction when ≥ 2 peers.
/// - **Hub side has ≥ 2 pins** (bus, e.g. SPI's MOSI/SCLK/MISO/CSN all on the uC):
///   pair each peer to its corresponding hub pin by **member name**, draw independent
///   lines, **no junction** —— i.e. the user wants "N pins spread into N wires
///   connecting to the component", rather than converging to one bus and entering the
///   device at a single point.
///
/// Difference from [`StarRouter`]: StarRouter uses the **geometric centroid** as hub
/// (the box still only connects one wire to the center); here the hub lands on **the
/// main device's own pin exit point**, visually the wires grow from the device to
/// various destinations —— exactly fan-out rather than tree-root collapse.
///
/// Called by `scheduler::route_one_net_with_channels` when `graph.fanout_star == true`
/// and this net is dispatched as TrunkTap / BusBundle. Does not use channels (no
/// trunk, no need to reserve slots).
pub fn route_hub_star(graph: &McVecGraph, net: &mut VizNet) {
    let mut route = Route::new();
    let n = net.endpoints.len();
    if n < 2 {
        net.route = Some(route);
        return;
    }

    // ★ FIX (collision): obstacle map (exclude this net's own endpoint boxes) ——
    //               spoke routing avoids other boxes.
    let exclude: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
    let obstacles = ObstacleMap::from_graph(graph, 8.0, &exclude);

    // 2 endpoints: direct Manhattan (consistent with StarRouter)
    if n == 2 {
        let a = &net.endpoints[0];
        let b = &net.endpoints[1];
        if let (Some(ba), Some(bb)) = (
            graph.boxes.iter().find(|x| x.id == a.box_id),
            graph.boxes.iter().find(|x| x.id == b.box_id),
        ) {
            let (sp, ss) = compute_exit_for_pin(ba, a.pin_id, Some(bb));
            let (dp, ds) = compute_exit_for_pin(bb, b.pin_id, Some(ba));
            for (x1, y1, x2, y2) in spoke_segments(&obstacles, sp, dp, ss, ds) {
                route.segments.push(Segment {
                    from: Point::new(x1, y1),
                    to: Point::new(x2, y2),
                });
            }
        }
        net.route = Some(route);
        return;
    }

    // ── Multi-endpoint: pick hub endpoint → hub box ──
    let hub_idx = pick_hub_endpoint(graph, net);
    let hub_box_id = net.endpoints[hub_idx].box_id;
    let hub_box = match graph.boxes.iter().find(|b| b.id == hub_box_id) {
        Some(b) => b,
        None => {
            net.route = Some(route);
            return;
        }
    };

    // Split endpoints into "hub box side" (may be multiple pins) and "opposite peers"
    let hub_side: Vec<usize> = net
        .endpoints
        .iter()
        .enumerate()
        .filter(|(_, e)| e.box_id == hub_box_id)
        .map(|(i, _)| i)
        .collect();
    let peers: Vec<usize> = net
        .endpoints
        .iter()
        .enumerate()
        .filter(|(_, e)| e.box_id != hub_box_id)
        .map(|(i, _)| i)
        .collect();
    if peers.is_empty() {
        net.route = Some(route);
        return;
    }

    // ════════════════════════════════════════════════════════════════════
    // Case A: hub side has only 1 pin → classic fan-out (all peers converge to that pin, junction when ≥ 2)
    // ════════════════════════════════════════════════════════════════════
    if hub_side.len() <= 1 {
        let hub_ep = &net.endpoints[hub_idx];

        // Peers geometric centroid (used to pick exit side when hub pin has no entry)
        let (mut sx, mut sy, mut k) = (0.0_f64, 0.0_f64, 0.0_f64);
        for &i in &peers {
            if let Some(b) = graph.boxes.iter().find(|x| x.id == net.endpoints[i].box_id) {
                sx += b.x + b.w / 2.0;
                sy += b.y + b.h / 2.0;
                k += 1.0;
            }
        }
        let hub_cx = hub_box.x + hub_box.w / 2.0;
        let hub_cy = hub_box.y + hub_box.h / 2.0;
        let (ocx, ocy) = if k > 0.0 {
            (sx / k, sy / k)
        } else {
            (hub_cx, hub_cy)
        };

        let (hp, h_side_raw) = compute_exit_for_pin(hub_box, hub_ep.pin_id, None);
        let h_side = if hub_box.find_entry(hub_ep.pin_id).is_some() {
            h_side_raw
        } else {
            let (dx, dy) = (ocx - hub_cx, ocy - hub_cy);
            if dx.abs() > dy.abs() {
                if dx > 0.0 {
                    ExitSide::Right
                } else {
                    ExitSide::Left
                }
            } else if dy > 0.0 {
                ExitSide::Bottom
            } else {
                ExitSide::Top
            }
        };

        for &i in &peers {
            let e = &net.endpoints[i];
            let cb = match graph.boxes.iter().find(|b| b.id == e.box_id) {
                Some(b) => b,
                None => continue,
            };
            let (cp, cs) = compute_exit_for_pin(cb, e.pin_id, Some(hub_box));
            for (x1, y1, x2, y2) in spoke_segments(&obstacles, cp, hp, cs, h_side) {
                route.segments.push(Segment {
                    from: Point::new(x1, y1),
                    to: Point::new(x2, y2),
                });
            }
        }
        if peers.len() >= 2 {
            route.junctions.push(Point::new(hp.0, hp.1));
        }

        net.route = Some(route);
        return;
    }

    // ════════════════════════════════════════════════════════════════════
    // Case B: hub side ≥ 2 pins (bus) → pair each peer to a hub pin, independent lines, no junction.
    //   This is exactly the "spread" the user wants: N member bits = N wires each from
    //   its own device pin, instead of converging to one trunk.
    //   Pairing priority: ① same member name and unused ② nearest unused pin ③ nearest pin (allow reuse).
    // ════════════════════════════════════════════════════════════════════

    // member name = last segment of pin name (strip "SPI." etc. prefix), used for bit pairing
    let member_of = |pin_name: &str| -> String {
        pin_name
            .rsplit('.')
            .next()
            .unwrap_or(pin_name)
            .trim()
            .to_ascii_lowercase()
    };
    let dist2 = |a: (f64, f64), b: (f64, f64)| {
        let dx = a.0 - b.0;
        let dy = a.1 - b.1;
        dx * dx + dy * dy
    };

    struct HubPin {
        member: String,
        hp: (f64, f64),
        side: ExitSide,
    }
    let hub_pins: Vec<HubPin> = hub_side
        .iter()
        .map(|&i| {
            let e = &net.endpoints[i];
            let (hp, side) = compute_exit_for_pin(hub_box, e.pin_id, None);
            HubPin {
                member: member_of(&e.pin_name),
                hp,
                side,
            }
        })
        .collect();
    let mut used = vec![false; hub_pins.len()];

    for &i in &peers {
        let e = &net.endpoints[i];
        let cb = match graph.boxes.iter().find(|b| b.id == e.box_id) {
            Some(b) => b,
            None => continue,
        };
        let (cp, cs) = compute_exit_for_pin(cb, e.pin_id, Some(hub_box));
        let pm = member_of(&e.pin_name);

        // ① Same member name and unused
        let mut j_opt = (0..hub_pins.len()).find(|&j| !used[j] && hub_pins[j].member == pm);
        // ② Nearest unused pin
        if j_opt.is_none() {
            j_opt = (0..hub_pins.len()).filter(|&j| !used[j]).min_by(|&x, &y| {
                dist2(cp, hub_pins[x].hp)
                    .partial_cmp(&dist2(cp, hub_pins[y].hp))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        // ③ Nearest pin (allow reuse —— fallback when peers outnumber hub pins)
        let j = j_opt.unwrap_or_else(|| {
            (0..hub_pins.len())
                .min_by(|&x, &y| {
                    dist2(cp, hub_pins[x].hp)
                        .partial_cmp(&dist2(cp, hub_pins[y].hp))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap_or(0)
        });
        used[j] = true;

        let h = &hub_pins[j];
        // ★ FIX (collision): spokes for bus member pins also use obstacle-aware routing.
        for (x1, y1, x2, y2) in spoke_segments(&obstacles, cp, h.hp, cs, h.side) {
            route.segments.push(Segment {
                from: Point::new(x1, y1),
                to: Point::new(x2, y2),
            });
        }
    }

    net.route = Some(route);
}

/// Pick the hub endpoint index:
/// 1) Driver end (Output/Bidir) and box is IC/SubModule
/// 2) Any IC/SubModule box (take largest area)
/// 3) Any driver end
/// 4) Largest area box
/// 5) The 0th one
fn pick_hub_endpoint(graph: &McVecGraph, net: &VizNet) -> usize {
    let area = |bid: i64| {
        graph
            .boxes
            .iter()
            .find(|b| b.id == bid)
            .map(|b| b.w * b.h)
            .unwrap_or(0.0)
    };
    let is_ic = |bid: i64| {
        graph
            .boxes
            .iter()
            .find(|b| b.id == bid)
            .map(|b| matches!(b.kind, BoxKind::MultiPin | BoxKind::SubModule))
            .unwrap_or(false)
    };

    if let Some(i) = net.endpoints.iter().position(|e| {
        is_ic(e.box_id) && matches!(e.io_type, IoDirection::Output | IoDirection::Bidir)
    }) {
        return i;
    }
    if let Some(i) = net
        .endpoints
        .iter()
        .enumerate()
        .filter(|(_, e)| is_ic(e.box_id))
        .max_by(|(_, a), (_, b)| {
            area(a.box_id)
                .partial_cmp(&area(b.box_id))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
    {
        return i;
    }
    if let Some(i) = net
        .endpoints
        .iter()
        .position(|e| matches!(e.io_type, IoDirection::Output | IoDirection::Bidir))
    {
        return i;
    }
    net.endpoints
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            area(a.box_id)
                .partial_cmp(&area(b.box_id))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::obstacles::ObstacleMap;
    use super::*;
    use crate::vector::graph::{BoxKind, IoSummary, McVecBox};

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
    fn p09_hub_at_centroid_when_clear() {
        let positions = vec![(0.0, 0.0), (100.0, 0.0), (50.0, 100.0)];
        let empty = ObstacleMap::empty();
        let (hx, hy) = pick_hub_point(&positions, &empty);
        assert!((hx - 50.0).abs() < 1.0);
        assert!((hy - 33.33).abs() < 1.0);
    }

    #[test]
    fn p09_hub_avoids_obstacle_at_centroid() {
        // Geometric centroid = (50, 33.33), deliberately place a box to cover it
        let positions = vec![(0.0, 0.0), (100.0, 0.0), (50.0, 100.0)];
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes.push(mk_box(99, 30.0, 20.0, 40.0, 30.0)); // covers (50, 33)
        let om = ObstacleMap::from_graph(&g, 0.0, &[]);

        let (hx, hy) = pick_hub_point(&positions, &om);
        // Should not be inside obstacle
        assert!(
            !om.point_inside_any(hx, hy),
            "hub must not be inside obstacle"
        );
    }

    #[test]
    fn p09_hub_empty_positions_yields_origin() {
        let positions: Vec<(f64, f64)> = vec![];
        let empty = ObstacleMap::empty();
        let (hx, hy) = pick_hub_point(&positions, &empty);
        assert_eq!(hx, 0.0);
        assert_eq!(hy, 0.0);
    }
}
