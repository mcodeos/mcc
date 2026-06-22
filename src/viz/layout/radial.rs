// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Radial hub-and-spoke layout (main chip + peripheral star circuit)
//!
//! ## Algorithm
//! 1. Pick a hub from box subset (highest frequency + main chip name bonus)
//! 2. Hub direct neighbors distributed evenly on first ring
//! 3. Second-degree neighbors on second ring (along parent extension line)
//! 4. Unconnected nodes in a row at the bottom
//!
//! ## Suitable / Not suitable
//! ✓ Suitable: one MCU + multiple peripheral sensors
//! ✗ Not suitable: power chains / bus flow (use `chain` / `hierarchical` instead)

use std::collections::{HashMap, HashSet};

use crate::vector::graph::{BoxKind, McVecBox, McVecGraph};

// ============================================================================
// Public constants
// ============================================================================

pub const RING1_RADIUS: f64 = 250.0;
pub const RING2_RADIUS: f64 = 500.0;

// ============================================================================
// Choose hub
// ============================================================================

/// Choose a hub within a given box subset
///
/// Scoring:
/// - degree × 15
/// - kind bonus: SubModule +150 / MultiPin +80
/// - name contains mcu/cpu/soc/fpga: +200
/// - pin count × 2
///
/// Preferred pool: not PowerLabel **and** degree > 0
/// Fallback pool: all non-PowerLabel
pub fn find_hub_in_subset(
    boxes: &[McVecBox],
    degrees: &HashMap<i64, usize>,
    subset: &HashSet<i64>,
) -> i64 {
    let connected: Vec<&McVecBox> = boxes
        .iter()
        .filter(|b| {
            subset.contains(&b.id)
                && b.kind != BoxKind::PowerLabel
                && *degrees.get(&b.id).unwrap_or(&0) > 0
        })
        .collect();

    let (pool, fallback_mode): (Vec<&McVecBox>, bool) = if !connected.is_empty() {
        (connected, false)
    } else {
        let non_power: Vec<&McVecBox> = boxes
            .iter()
            .filter(|b| subset.contains(&b.id) && b.kind != BoxKind::PowerLabel)
            .collect();
        if non_power.is_empty() {
            // Extreme fallback: all PowerLabel
            let any: Vec<&McVecBox> = boxes.iter().filter(|b| subset.contains(&b.id)).collect();
            return any.first().map(|b| b.id).unwrap_or(0);
        }
        (non_power, true)
    };

    let mut best_id = pool[0].id;
    let mut best_score: i32 = i32::MIN;

    for b in &pool {
        let deg = *degrees.get(&b.id).unwrap_or(&0) as i32;
        let kind_bonus: i32 = match b.kind {
            BoxKind::SubModule => 150,
            BoxKind::MultiPin => 80,
            BoxKind::TwoPin => 0,
            BoxKind::PowerLabel => -100,
        };

        let is_main_chip = crate::vector::graph::naming::is_main_chip(&b.name);
        let chip_bonus = if is_main_chip { 200 } else { 0 };

        let score = deg * 15 + kind_bonus + chip_bonus + b.pin_count as i32 * 2;
        if score > best_score {
            best_score = score;
            best_id = b.id;
        }
    }

    if crate::viz::debug::dump_enabled() {
        if fallback_mode {
            eprintln!(
                "[layout::radial] Hub (fallback: no degree>0): id={best_id} (score={best_score})"
            );
        } else {
            eprintln!("[layout::radial] Hub: id={best_id} (score={best_score})");
        }
    }
    best_id
}

// ============================================================================
// BFS layering (subset-restricted version)
// ============================================================================

/// BFS from hub, return (ring1, ring2, unplaced)
pub fn bfs_rings_in_subset(
    hub: i64,
    adj: &HashMap<i64, Vec<i64>>,
    subset: &HashSet<i64>,
) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
    let mut visited: HashSet<i64> = HashSet::new();
    visited.insert(hub);

    // Ring 1: hub's direct neighbors (restricted to subset)
    let ring1: Vec<i64> = adj
        .get(&hub)
        .map(|v| {
            v.iter()
                .filter(|id| subset.contains(id) && !visited.contains(id))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for &id in &ring1 {
        visited.insert(id);
    }

    // Ring 2: ring1's neighbors
    let mut ring2 = Vec::new();
    for &r1_id in &ring1 {
        if let Some(neighbors) = adj.get(&r1_id) {
            for &n in neighbors {
                if subset.contains(&n) && !visited.contains(&n) {
                    ring2.push(n);
                    visited.insert(n);
                }
            }
        }
    }

    // Still unvisited in component (≥ 3 hops away tail)
    let unplaced: Vec<i64> = subset
        .iter()
        .filter(|id| !visited.contains(id))
        .cloned()
        .collect();

    (ring1, ring2, unplaced)
}

// ============================================================================
// Placement: set_center / place_ring / place_ring2 / place_unconnected
// ============================================================================

/// Place box center at (cx, cy)
pub fn set_center(graph: &mut McVecGraph, id: i64, cx: f64, cy: f64) {
    if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
        b.x = cx - b.w / 2.0;
        b.y = cy - b.h / 2.0;
    }
}

/// Place first ring (hub direct neighbors, evenly distributed on the circle)
pub fn place_ring(
    graph: &mut McVecGraph,
    ring: &[i64],
    cx: f64,
    cy: f64,
    radius: f64,
    _adj: &HashMap<i64, Vec<i64>>,
    _hub: i64,
) {
    if ring.is_empty() {
        return;
    }

    let n = ring.len();
    let start_angle = -std::f64::consts::FRAC_PI_2; // start at top, clockwise

    let mut angle_positions: Vec<(i64, f64)> = Vec::new();
    for (i, &id) in ring.iter().enumerate() {
        let base_angle = start_angle + (i as f64 / n as f64) * 2.0 * std::f64::consts::PI;
        angle_positions.push((id, base_angle));
    }

    // Mutual push: force apart if angle difference between any two points < min_angle
    let mut adjustments: Vec<f64> = vec![0.0; ring.len()];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                let angle1 = angle_positions[i].1;
                let angle2 = angle_positions[j].1;
                let angle_diff = (angle1 - angle2).abs() % (2.0 * std::f64::consts::PI);
                let min_angle = 0.5;
                if angle_diff < min_angle {
                    let adjustment = (min_angle - angle_diff) / 2.0;
                    adjustments[i] += adjustment;
                    adjustments[j] -= adjustment;
                }
            }
        }
    }
    for i in 0..n {
        angle_positions[i].1 += adjustments[i];
    }

    for &(id, angle) in &angle_positions {
        let bx = cx + radius * angle.cos();
        let by = cy + radius * angle.sin();
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
            b.x = bx - b.w / 2.0;
            b.y = by - b.h / 2.0;
        }
    }
}

/// Place second ring (close to its first-ring connection direction)
pub fn place_ring2(
    graph: &mut McVecGraph,
    ring2: &[i64],
    ring1: &[i64],
    cx: f64,
    cy: f64,
    radius: f64,
    adj: &HashMap<i64, Vec<i64>>,
) {
    let box_map: HashMap<i64, (f64, f64)> = graph
        .boxes
        .iter()
        .map(|b| (b.id, (b.x + b.w / 2.0, b.y + b.h / 2.0)))
        .collect();

    for &id in ring2 {
        // Find ring1 point connected to this node
        let parent = adj
            .get(&id)
            .and_then(|neighbors| neighbors.iter().find(|n| ring1.contains(n)));

        let (bx, by) = if let Some(&parent_id) = parent {
            if let Some(&(px, py)) = box_map.get(&parent_id) {
                let dx = px - cx;
                let dy = py - cy;
                let len = (dx * dx + dy * dy).sqrt().max(1.0);
                let nx = dx / len;
                let ny = dy / len;
                (cx + nx * radius, cy + ny * radius)
            } else {
                (cx + radius, cy)
            }
        } else {
            let angle = (ring2.iter().position(|&x| x == id).unwrap_or(0) as f64) * 0.8;
            (cx + radius * angle.cos(), cy + radius * angle.sin())
        };

        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
            b.x = bx - b.w / 2.0;
            b.y = by - b.h / 2.0;
        }
    }
}

/// Place unconnected nodes (bottom row)
pub fn place_unconnected(graph: &mut McVecGraph, ids: &[i64], cx: f64, cy: f64, offset: f64) {
    let start_x = cx - (ids.len() as f64 * 100.0) / 2.0;
    for (i, &id) in ids.iter().enumerate() {
        if let Some(b) = graph.boxes.iter_mut().find(|b| b.id == id) {
            b.x = start_x + i as f64 * 100.0;
            b.y = cy + offset;
        }
    }
}
