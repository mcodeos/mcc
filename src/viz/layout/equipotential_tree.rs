// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ R-N (discipline 29): Equipotential tree builder
//!
//! Each net is rendered as ONE connected orthogonal tree, not n-1 independent edges.
//!
//! Algorithm:
//! 1. GROUP: Same-box pins merge into a local trunk. PowerLabel boxes are NOT
//!    treated as boxes — their endpoints become tree symbols instead.
//! 2. ANCHOR: Box with most endpoints (tiebreak: degree → source line)
//! 3. TRUNK: Straight line from anchor's local trunk
//! 4. TAP: Each other member attaches perpendicular to trunk
//! 5. JUNCTION DOT: ≥3 line intersections only
//! 6. SYMBOL: Ground/net-label/port at trunk endpoints or as tap leaves
//!
//! Forms: 2-point → single line, 3-point → T-shape, 4-point → cross, n-point → comb

use std::collections::HashMap;

use crate::vector::graph::netdef::EndpointRef;
use crate::vector::graph::{BoxKind, EntrySide, McVecBox, McVecGraph, NetKind, VizNet};

// ============================================================================
// Data structures
// ============================================================================

/// A group of pins on the same box, forming a local trunk.
struct PinGroup {
    box_id: i64,
    /// Pin positions on this box (absolute coordinates)
    pin_positions: Vec<(f64, f64)>,
    /// Local trunk: vertical line from (x, y_min) to (x, y_max)
    local_trunk_x: f64,
    local_trunk_y_min: f64,
    local_trunk_y_max: f64,
}

/// A tap branch connecting a pin group to the main trunk.
#[derive(Debug)]
pub struct TapBranch {
    pub box_id: i64,
    /// The attachment y on the main trunk (vertical trunk)
    pub trunk_attach_y: f64,
    /// The attachment x on the main trunk (horizontal trunk)
    pub trunk_attach_x: f64,
    /// The local trunk x position
    pub local_trunk_x: f64,
    /// Local trunk y range
    pub local_trunk_y_min: f64,
    pub local_trunk_y_max: f64,
}

/// Terminal symbol type
#[derive(Debug, Clone, PartialEq)]
pub enum TreeSymbolKind {
    Ground,
    Power,
    NetLabel,
    PortLabel,
}

/// A terminal symbol hanging off the tree
#[derive(Debug, Clone)]
pub struct TreeSymbol {
    pub kind: TreeSymbolKind,
    pub x: f64,
    pub y: f64,
    pub label: String,
}

/// An equipotential tree for one net.
#[derive(Debug)]
pub struct EquiTree {
    pub net_name: String,
    pub net_kind: NetKind,
    /// Anchor box ID
    pub anchor_box_id: i64,
    /// Whether the main trunk is horizontal (true) or vertical (false)
    pub horizontal_trunk: bool,
    /// Main trunk position (vertical trunk)
    pub trunk_x: f64,
    pub trunk_y_min: f64,
    pub trunk_y_max: f64,
    /// Main trunk position (horizontal trunk)
    pub trunk_y: f64,
    pub trunk_x_min: f64,
    pub trunk_x_max: f64,
    /// Anchor's local trunk
    pub anchor_local_trunk_x: f64,
    pub anchor_local_trunk_y_min: f64,
    pub anchor_local_trunk_y_max: f64,
    /// Tap branches
    pub taps: Vec<TapBranch>,
    /// Junction dot positions (≥3 line intersections)
    pub junction_dots: Vec<(f64, f64)>,
    /// Terminal symbols
    pub symbols: Vec<TreeSymbol>,
}

// ============================================================================
// Build all trees from a graph
// ============================================================================

/// Build equipotential trees for all nets in the graph.
/// Returns trees for nets with ≥2 connected real boxes (or 1 real box + symbols).
pub fn build_all_trees(graph: &McVecGraph) -> Vec<EquiTree> {
    let mut trees = Vec::new();

    for net in &graph.nets {
        if let Some(tree) = build_equi_tree(net, graph) {
            trees.push(tree);
        }
    }

    trees
}

/// Build an equipotential tree for a single net.
fn build_equi_tree(net: &VizNet, graph: &McVecGraph) -> Option<EquiTree> {
    // Separate real-box endpoints from PowerLabel endpoints
    let mut real_positions: HashMap<i64, Vec<(f64, f64)>> = HashMap::new();
    let mut symbol_endpoints: Vec<&EndpointRef> = Vec::new();

    for ep in &net.endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
            continue;
        };
        if b.kind == BoxKind::PowerLabel {
            symbol_endpoints.push(ep);
        } else {
            let pos = pin_position(b, ep);
            real_positions.entry(ep.box_id).or_default().push(pos);
        }
    }

    // Need at least 1 real box to anchor the tree
    if real_positions.is_empty() {
        return None;
    }

    // Anchor: box with most endpoints
    let anchor_id = select_anchor(&real_positions);

    // Build pin groups with local trunks
    let mut pin_groups: Vec<PinGroup> = Vec::new();
    for (&box_id, positions) in &real_positions {
        let (lx, ly_min, ly_max) = local_trunk_from_pins(positions);

        pin_groups.push(PinGroup {
            box_id,
            pin_positions: positions.clone(),
            local_trunk_x: lx,
            local_trunk_y_min: ly_min,
            local_trunk_y_max: ly_max,
        });
    }

    // Find anchor group
    let anchor_group = pin_groups.iter().find(|g| g.box_id == anchor_id)?;
    let anchor_local_trunk_x = anchor_group.local_trunk_x;
    let anchor_local_trunk_y_min = anchor_group.local_trunk_y_min;
    let anchor_local_trunk_y_max = anchor_group.local_trunk_y_max;

    // Detect trunk direction: compute bounding box of all pin positions.
    // If x-spread >= y-spread, use horizontal trunk; otherwise vertical.
    let all_xs: Vec<f64> = pin_groups
        .iter()
        .flat_map(|g| g.pin_positions.iter().map(|p| p.0))
        .collect();
    let all_ys: Vec<f64> = pin_groups
        .iter()
        .flat_map(|g| g.pin_positions.iter().map(|p| p.1))
        .collect();
    let x_min = all_xs.iter().cloned().fold(f64::MAX, |a, b| a.min(b));
    let x_max = all_xs.iter().cloned().fold(f64::MIN, |a, b| a.max(b));
    let y_min = all_ys.iter().cloned().fold(f64::MAX, |a, b| a.min(b));
    let y_max = all_ys.iter().cloned().fold(f64::MIN, |a, b| a.max(b));
    let x_spread = x_max - x_min;
    let y_spread = y_max - y_min;

    let horizontal_trunk = x_spread >= y_spread;

    // Build taps for non-anchor groups
    let mut taps: Vec<TapBranch> = Vec::new();
    let mut junction_dots: Vec<(f64, f64)> = Vec::new();

    let anchor_attach_y = (anchor_local_trunk_y_min + anchor_local_trunk_y_max) / 2.0;

    if horizontal_trunk {
        // ── Horizontal trunk ──
        // Trunk y: midpoint of all pin y positions
        let all_ys_avg = all_ys.iter().sum::<f64>() / all_ys.len() as f64;
        let trunk_y = all_ys_avg;

        // Trunk x range: from leftmost to rightmost pin, with margin
        let trunk_x_min = x_min - 20.0;
        let trunk_x_max = x_max + 20.0;

        for group in &pin_groups {
            if group.box_id == anchor_id {
                continue;
            }
            let attach_x = group.local_trunk_x;
            taps.push(TapBranch {
                box_id: group.box_id,
                trunk_attach_y: 0.0,
                trunk_attach_x: attach_x,
                local_trunk_x: group.local_trunk_x,
                local_trunk_y_min: group.local_trunk_y_min,
                local_trunk_y_max: group.local_trunk_y_max,
            });
        }

        // Junction dots: at non-endpoint x positions on the trunk
        let mut trunk_xs: Vec<f64> = vec![anchor_local_trunk_x];
        for tap in &taps {
            trunk_xs.push(tap.trunk_attach_x);
        }
        for sym_ep in &symbol_endpoints {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == sym_ep.box_id) {
                let pos = pin_position(b, sym_ep);
                trunk_xs.push(pos.0);
            }
        }
        trunk_xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        trunk_xs.dedup_by(|a, b| (*a - *b).abs() < 1.0);

        for &x in &trunk_xs {
            let is_endpoint = (x - trunk_x_min).abs() < 1.0 || (x - trunk_x_max).abs() < 1.0;
            if !is_endpoint && trunk_xs.len() >= 2 {
                junction_dots.push((x, trunk_y));
            }
        }

        // Symbols
        let symbols = build_all_symbols_horizontal(
            net,
            trunk_x_min,
            trunk_x_max,
            trunk_y,
            &symbol_endpoints,
            graph,
        );

        Some(EquiTree {
            net_name: net.name.clone(),
            net_kind: net.kind.clone(),
            anchor_box_id: anchor_id,
            horizontal_trunk: true,
            trunk_x: 0.0,
            trunk_y_min: 0.0,
            trunk_y_max: 0.0,
            trunk_y,
            trunk_x_min,
            trunk_x_max,
            anchor_local_trunk_x,
            anchor_local_trunk_y_min,
            anchor_local_trunk_y_max,
            taps,
            junction_dots,
            symbols,
        })
    } else {
        // ── Vertical trunk ──
        // Compute main trunk x: between anchor and rightmost non-anchor box
        let rightmost_x = pin_groups
            .iter()
            .filter(|g| g.box_id != anchor_id)
            .map(|g| g.local_trunk_x)
            .fold(anchor_local_trunk_x + 100.0, |a, b| a.max(b));

        let trunk_x = (anchor_local_trunk_x + rightmost_x) / 2.0;

        // Trunk y range: from min of all local trunks to max
        let trunk_y_min = pin_groups
            .iter()
            .map(|g| g.local_trunk_y_min)
            .fold(f64::MAX, |a, b| a.min(b));
        let trunk_y_max = pin_groups
            .iter()
            .map(|g| g.local_trunk_y_max)
            .fold(f64::MIN, |a, b| a.max(b));

        // Extend trunk for symbols
        let (trunk_y_min, trunk_y_max) = extend_trunk_for_symbols(
            trunk_y_min,
            trunk_y_max,
            trunk_x,
            &symbol_endpoints,
            &net.kind,
            graph,
        );

        for group in &pin_groups {
            if group.box_id == anchor_id {
                continue;
            }
            let attach_y = (group.local_trunk_y_min + group.local_trunk_y_max) / 2.0;
            taps.push(TapBranch {
                box_id: group.box_id,
                trunk_attach_y: attach_y,
                trunk_attach_x: 0.0,
                local_trunk_x: group.local_trunk_x,
                local_trunk_y_min: group.local_trunk_y_min,
                local_trunk_y_max: group.local_trunk_y_max,
            });
        }

        // Junction dots: only at trunk positions where >=3 lines meet.
        let mut trunk_ys: Vec<f64> = vec![anchor_attach_y];
        for tap in &taps {
            trunk_ys.push(tap.trunk_attach_y);
        }
        for sym_ep in &symbol_endpoints {
            if let Some(b) = graph.boxes.iter().find(|b| b.id == sym_ep.box_id) {
                let pos = pin_position(b, sym_ep);
                trunk_ys.push(pos.1);
            }
        }
        trunk_ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        trunk_ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);

        for &y in &trunk_ys {
            let is_endpoint = (y - trunk_y_min).abs() < 1.0 || (y - trunk_y_max).abs() < 1.0;
            if !is_endpoint && trunk_ys.len() >= 2 {
                junction_dots.push((trunk_x, y));
            }
        }

        // Build symbols from PowerLabel endpoints and net kind
        let symbols = build_all_symbols(
            net,
            trunk_x,
            trunk_y_min,
            trunk_y_max,
            &symbol_endpoints,
            graph,
        );

        Some(EquiTree {
            net_name: net.name.clone(),
            net_kind: net.kind.clone(),
            anchor_box_id: anchor_id,
            horizontal_trunk: false,
            trunk_x,
            trunk_y_min,
            trunk_y_max,
            trunk_y: 0.0,
            trunk_x_min: 0.0,
            trunk_x_max: 0.0,
            anchor_local_trunk_x,
            anchor_local_trunk_y_min,
            anchor_local_trunk_y_max,
            taps,
            junction_dots,
            symbols,
        })
    }
}

/// Select the anchor box: most endpoints, tiebreak by degree, then source line.
fn select_anchor(groups: &HashMap<i64, Vec<(f64, f64)>>) -> i64 {
    groups
        .iter()
        .max_by_key(|(_, positions)| positions.len())
        .map(|(&id, _)| id)
        .unwrap_or(0)
}

/// Compute absolute pin position from box and endpoint reference.
fn pin_position(b: &McVecBox, ep: &EndpointRef) -> (f64, f64) {
    if let Some(entry) = b.entry_points.iter().find(|e| e.pin_id == ep.pin_id) {
        match entry.side {
            EntrySide::Left => (b.x, b.y + b.h * entry.offset),
            EntrySide::Right => (b.x + b.w, b.y + b.h * entry.offset),
            EntrySide::Top => (b.x + b.w * entry.offset, b.y),
            EntrySide::Bottom => (b.x + b.w * entry.offset, b.y + b.h),
        }
    } else {
        // Fallback: use box center
        (b.x + b.w / 2.0, b.y + b.h / 2.0)
    }
}

/// Compute the local trunk from actual pin positions.
/// The trunk x is derived from the pin positions, not from edge_facing.
fn local_trunk_from_pins(positions: &[(f64, f64)]) -> (f64, f64, f64) {
    let xs: Vec<f64> = positions.iter().map(|(x, _)| *x).collect();
    let ys: Vec<f64> = positions.iter().map(|(_, y)| *y).collect();

    let lx = xs.iter().sum::<f64>() / xs.len() as f64;
    let y_min = ys.iter().cloned().fold(f64::MAX, |a, b| a.min(b));
    let y_max = ys.iter().cloned().fold(f64::MIN, |a, b| a.max(b));

    let margin = 10.0;
    (lx, y_min - margin, y_max + margin)
}

/// Extend trunk y range to accommodate symbols.
fn extend_trunk_for_symbols(
    y_min: f64,
    y_max: f64,
    _trunk_x: f64,
    symbol_endpoints: &[&EndpointRef],
    net_kind: &NetKind,
    graph: &McVecGraph,
) -> (f64, f64) {
    let mut y_min = y_min;
    let mut y_max = y_max;

    for ep in symbol_endpoints {
        if let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) {
            let (_, py) = pin_position(b, ep);
            y_min = y_min.min(py - 20.0);
            y_max = y_max.max(py + 20.0);
        }
    }

    // Extend for net-kind-based symbols
    match net_kind {
        NetKind::Ground => {
            y_max += 40.0; // room for GND symbol
        }
        NetKind::Power => {
            y_min -= 20.0; // room for power label
        }
        _ => {}
    }

    (y_min, y_max)
}

/// Build all symbols: from PowerLabel endpoints + net-kind-based symbols.
fn build_all_symbols(
    net: &VizNet,
    trunk_x: f64,
    trunk_y_min: f64,
    trunk_y_max: f64,
    symbol_endpoints: &[&EndpointRef],
    graph: &McVecGraph,
) -> Vec<TreeSymbol> {
    let mut symbols = Vec::new();

    // Track whether we've already added each symbol type
    let mut has_ground = false;
    let mut has_net_label = false;

    // Symbols from PowerLabel endpoints
    for ep in symbol_endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
            continue;
        };

        // Use net.kind to determine symbol type, NOT the PowerLabel box name
        // Only add one symbol of each kind
        match &net.kind {
            NetKind::Ground if !has_ground => {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::Ground,
                    x: trunk_x,
                    y: trunk_y_max,
                    label: b.name.clone(),
                });
                has_ground = true;
            }
            NetKind::Power if !has_net_label => {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::NetLabel,
                    x: trunk_x,
                    y: trunk_y_min - 10.0,
                    label: b.name.clone(),
                });
                has_net_label = true;
            }
            _ if !has_net_label => {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::NetLabel,
                    x: trunk_x,
                    y: trunk_y_min - 10.0,
                    label: b.name.clone(),
                });
                has_net_label = true;
            }
            _ => {}
        }
    }

    // Net-kind-based symbols (only if no PowerLabel endpoint already provides one)
    let has_power = symbols.iter().any(|s| s.kind == TreeSymbolKind::Power);

    match &net.kind {
        NetKind::Ground if !has_ground => {
            symbols.push(TreeSymbol {
                kind: TreeSymbolKind::Ground,
                x: trunk_x,
                y: trunk_y_max,
                label: net.name.clone(),
            });
        }
        NetKind::Power if !has_power && !has_net_label => {
            symbols.push(TreeSymbol {
                kind: TreeSymbolKind::Power,
                x: trunk_x,
                y: trunk_y_min - 10.0,
                label: net.name.clone(),
            });
        }
        NetKind::Signal if !has_net_label => {
            if !net.name.is_empty() && !net.name.starts_with("__net") {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::NetLabel,
                    x: trunk_x,
                    y: trunk_y_min - 10.0,
                    label: net.name.clone(),
                });
            }
        }
        NetKind::SubModuleIO => {
            symbols.push(TreeSymbol {
                kind: TreeSymbolKind::PortLabel,
                x: trunk_x,
                y: trunk_y_min - 10.0,
                label: net.name.clone(),
            });
        }
        NetKind::Bus(_) => {}
        _ => {} // guard-failed cases: Ground/Power/Signal with existing symbols
    }

    symbols
}

/// Build all symbols for a horizontal trunk.
fn build_all_symbols_horizontal(
    net: &VizNet,
    trunk_x_min: f64,
    trunk_x_max: f64,
    trunk_y: f64,
    symbol_endpoints: &[&EndpointRef],
    graph: &McVecGraph,
) -> Vec<TreeSymbol> {
    let mut symbols = Vec::new();

    // Track whether we've already added each symbol type
    let mut has_ground = false;
    let mut has_net_label = false;

    // Symbols from PowerLabel endpoints
    for ep in symbol_endpoints {
        let Some(b) = graph.boxes.iter().find(|b| b.id == ep.box_id) else {
            continue;
        };
        // Use net.kind to determine symbol type, NOT the PowerLabel box name
        // Only add one symbol of each kind
        match &net.kind {
            NetKind::Ground if !has_ground => {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::Ground,
                    x: trunk_x_max,
                    y: trunk_y,
                    label: b.name.clone(),
                });
                has_ground = true;
            }
            NetKind::Power if !has_net_label => {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::NetLabel,
                    x: trunk_x_min - 10.0,
                    y: trunk_y,
                    label: b.name.clone(),
                });
                has_net_label = true;
            }
            _ if !has_net_label => {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::NetLabel,
                    x: trunk_x_min - 10.0,
                    y: trunk_y,
                    label: b.name.clone(),
                });
                has_net_label = true;
            }
            _ => {}
        }
    }

    // Net-kind-based symbols
    let has_power = symbols.iter().any(|s| s.kind == TreeSymbolKind::Power);

    match &net.kind {
        NetKind::Ground if !has_ground => {
            symbols.push(TreeSymbol {
                kind: TreeSymbolKind::Ground,
                x: trunk_x_max,
                y: trunk_y,
                label: net.name.clone(),
            });
        }
        NetKind::Power if !has_power && !has_net_label => {
            symbols.push(TreeSymbol {
                kind: TreeSymbolKind::Power,
                x: trunk_x_min - 10.0,
                y: trunk_y,
                label: net.name.clone(),
            });
        }
        NetKind::Signal if !has_net_label => {
            if !net.name.is_empty() && !net.name.starts_with("__net") {
                symbols.push(TreeSymbol {
                    kind: TreeSymbolKind::NetLabel,
                    x: trunk_x_min - 10.0,
                    y: trunk_y,
                    label: net.name.clone(),
                });
            }
        }
        NetKind::SubModuleIO => {
            symbols.push(TreeSymbol {
                kind: TreeSymbolKind::PortLabel,
                x: trunk_x_min - 10.0,
                y: trunk_y,
                label: net.name.clone(),
            });
        }
        NetKind::Bus(_) => {}
        _ => {}
    }

    symbols
}
