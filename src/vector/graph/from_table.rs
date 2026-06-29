// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Fallback solution: build `McVecGraph` directly from flat `InstTable`
//!
//! ## Purpose
//! When the caller doesn't have a `McVecBlock` available (rare, mainly for testing), they can
//! feed InstTable directly. The main flow is still [`super::from_block::build_mc_vec_graph`].
//!
//! ## Limitations
//! Here the edge types can only be simply divided into `Single` / `Bus(n)`, because there's no
//! `McVecBlock.nets` providing `ConnectionType` information.

use std::collections::HashMap;

use crate::instant::inst_table::{InstKind, InstTable};

use super::box_def::{IoSummary, McVecBox, Wire};
use super::detect::{compute_io, detect_kind, extract_last_segment, is_power_label, DetectedKind};
use super::graph_def::McVecGraph;
use super::kinds::{BoxKind, EdgeType};
use super::net_def::McVecEdge;

/// Build `McVecGraph` directly from `InstTable` (duck typing solution)
pub fn build_graph_from_table(table: &InstTable, root_id: u32) -> McVecGraph {
    let root_name = table
        .get_entry(root_id)
        .map(|e| extract_last_segment(&e.path))
        .unwrap_or_else(|| "root".into());

    let mut graph = McVecGraph::new(root_id as i64, root_name);

    // ── Phase 1: traverse direct children, use duck typing to determine identity ──
    let children = table.children_of(root_id);
    let mut box_ids: Vec<u32> = Vec::new();
    let mut sub_module_ids: Vec<u32> = Vec::new();

    for child in &children {
        let detected = detect_kind(table, child.id);
        let name = extract_last_segment(&child.path);

        match detected {
            DetectedKind::Component {
                pin_count,
                class_name,
            } => {
                let kind = if pin_count <= 2 {
                    BoxKind::TwoPin
                } else {
                    BoxKind::MultiPin
                };
                let pins = table.get_pins_of(child.id);
                let io = compute_io(&pins);
                crate::velog!("[graph] ✓ Component: {name} (class={class_name}, pins={pin_count})");
                graph.boxes.push(McVecBox::new(
                    child.id as i64,
                    name,
                    class_name,
                    kind,
                    pin_count,
                    io,
                ));
                box_ids.push(child.id);
            }
            DetectedKind::SubModule {
                port_count,
                class_name,
            } => {
                let ports = table.get_ports_of(child.id);
                let io = compute_io(&ports);
                crate::velog!("[graph] ✓ SubModule: {name} (class={class_name}, ports={port_count})");
                graph.boxes.push(McVecBox::new(
                    child.id as i64,
                    name,
                    class_name,
                    BoxKind::SubModule,
                    port_count,
                    io,
                ));
                box_ids.push(child.id);
                sub_module_ids.push(child.id);
            }
            DetectedKind::PowerLabel => {
                crate::velog!("[graph] ✓ PowerLabel: {name}");
                graph.boxes.push(McVecBox::new(
                    child.id as i64,
                    name,
                    String::new(),
                    BoxKind::PowerLabel,
                    0,
                    IoSummary::new(),
                ));
                box_ids.push(child.id);
            }
            DetectedKind::Skip => {
                // Bus itself isn't drawn, but its members may be power labels
                if child.kind == InstKind::Bus {
                    let members = table.children_of(child.id);
                    for member in &members {
                        let mname = extract_last_segment(&member.path);
                        if is_power_label(&mname) {
                            crate::velog!("[graph] ✓ PowerLabel (bus member): {mname}");
                            graph.boxes.push(McVecBox::new(
                                member.id as i64,
                                mname,
                                String::new(),
                                BoxKind::PowerLabel,
                                0,
                                IoSummary::new(),
                            ));
                            box_ids.push(member.id);
                        }
                    }
                }
            }
        }
    }

    // ── Phase 2: build edges from NetEntry ──
    let mut point_to_box: HashMap<u32, u32> = HashMap::new();

    for &bid in &box_ids {
        for pin in table.get_pins_of(bid) {
            point_to_box.insert(pin.id, bid);
        }
        for port in table.get_ports_of(bid) {
            point_to_box.insert(port.id, bid);
        }
        if let Some(entry) = table.get_entry(bid) {
            if entry.kind == InstKind::Label
                || entry.kind == InstKind::Bus
                || is_power_label(&extract_last_segment(&entry.path))
            {
                point_to_box.insert(bid, bid);
            }
        }
        for child_entry in table.children_of(bid) {
            if child_entry.kind == InstKind::Label {
                point_to_box.insert(child_entry.id, bid);
            }
        }
    }

    // Iterate all NetEntries
    let mut edge_map: HashMap<(u32, u32), Vec<Wire>> = HashMap::new();
    let mut edge_names: HashMap<(u32, u32), String> = HashMap::new();

    for net in table.get_nets() {
        let mut net_boxes: Vec<(u32, u32)> = Vec::new();
        for &pid in &net.points {
            if let Some(&bid) = point_to_box.get(&pid) {
                net_boxes.push((pid, bid));
            }
        }

        let mut by_box: HashMap<u32, Vec<u32>> = HashMap::new();
        for &(pid, bid) in &net_boxes {
            by_box.entry(bid).or_default().push(pid);
        }

        let unique_boxes: Vec<u32> = by_box.keys().cloned().collect();
        for i in 0..unique_boxes.len() {
            for j in (i + 1)..unique_boxes.len() {
                let (b1, b2) = (unique_boxes[i], unique_boxes[j]);
                let key = if b1 <= b2 { (b1, b2) } else { (b2, b1) };
                let pins1 = &by_box[&b1];
                let pins2 = &by_box[&b2];
                let p1 = pins1[0];
                let p2 = pins2[0];
                edge_map.entry(key).or_default().push(Wire {
                    src_pin_id: p1 as i64,
                    src_pin_name: table
                        .get_entry(p1)
                        .map(|e| extract_last_segment(&e.path))
                        .unwrap_or_default(),
                    dst_pin_id: p2 as i64,
                    dst_pin_name: table
                        .get_entry(p2)
                        .map(|e| extract_last_segment(&e.path))
                        .unwrap_or_default(),
                });
                edge_names.entry(key).or_insert_with(|| net.name.clone());
            }
        }
    }

    // Convert to McVecEdge
    for ((b1, b2), wires) in edge_map {
        let et = if wires.len() == 1 {
            EdgeType::Single
        } else {
            EdgeType::Bus(wires.len())
        };
        let net_name = edge_names.get(&(b1, b2)).cloned().unwrap_or_default();
        graph.edges.push(McVecEdge {
            src_box: b1 as i64,
            dst_box: b2 as i64,
            edge_type: et,
            wires,
            net_name,
        });
    }

    // ── Phase 3: recursively process sub-modules ──
    for &sub_id in &sub_module_ids {
        let sub_graph = build_graph_from_table(table, sub_id);
        graph.sub_graphs.push(sub_graph);
    }

    graph
}
