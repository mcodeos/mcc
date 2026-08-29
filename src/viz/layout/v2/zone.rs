// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Zone tree — functional partitioning
//!
//! Builds a functional partition tree from `McVecBox.inst_path`.
//! The module structure of .mc source is the natural answer for partitioning;
//! no clustering algorithm needed.
//!
//! Algorithm:
//! 1. Build a prefix tree from inst_path
//! 2. Flatten single chains
//! 3. Merge small zones (box count < MIN_ZONE_SIZE)
//! 4. Depth cap MAX_ZONE_DEPTH = 2

use std::collections::BTreeMap;

use crate::vector::graph::BoxKind;
use crate::vector::graph::McVecGraph;

/// Minimum zone size (box count); zones below this are merged into the parent zone
pub const MIN_ZONE_SIZE: usize = 3;

/// Maximum zone depth (more than two visual partition levels on paper is messier)
pub const MAX_ZONE_DEPTH: usize = 2;

// ============================================================================
// Zone data structures
// ============================================================================

/// A single functional partition
#[derive(Debug, Clone)]
pub struct Zone {
    pub id: usize,
    /// Path, e.g. "main.ldo"
    pub path: String,
    /// Display title, e.g. "POWER_LDO"
    pub title: String,
    /// Box ids in this zone
    pub boxes: Vec<i64>,
    /// Child zone indices
    pub children: Vec<usize>,
    /// Parent zone index
    pub parent: Option<usize>,
}

/// Partition tree
#[derive(Debug, Clone, Default)]
pub struct ZoneTree {
    pub zones: Vec<Zone>,
    /// Root zone indices
    pub roots: Vec<usize>,
}

// ============================================================================
// Internal: prefix tree node
// ============================================================================

/// Prefix tree node (used during construction)
#[derive(Debug, Clone)]
struct TrieNode {
    /// Full path (e.g. "main.ldo")
    path: String,
    /// Box ids of this node (only leaves carry boxes)
    boxes: Vec<i64>,
    /// Child nodes
    children: BTreeMap<String, TrieNode>,
}

impl TrieNode {
    fn new(path: String) -> Self {
        TrieNode {
            path,
            boxes: Vec::new(),
            children: BTreeMap::new(),
        }
    }
}

// ============================================================================
// Build
// ============================================================================

impl ZoneTree {
    /// Build the partition tree
    ///
    /// Uses `inst_path` instead of `scope_chain` (v2 revision):
    /// build the prefix tree directly from paths; flattening/merging rules
    /// unchanged, one less layer of sync risk.
    pub fn build(graph: &McVecGraph) -> Self {
        // ── Collect boxes needing partitioning ──
        // PowerLabel / Dot do not enter any zone (they render in place)
        let mut zone_boxes: Vec<(i64, String)> = Vec::new();
        for b in &graph.boxes {
            if b.kind == BoxKind::PowerLabel || b.kind == BoxKind::Dot {
                continue;
            }
            // M4-1B fix: TwoPin devices inside submodules have no inst_path, still need to join a zone
            let path = if b.inst_path.is_empty() {
                "main".to_string()
            } else {
                b.inst_path.clone()
            };
            zone_boxes.push((b.id, path));
        }

        if zone_boxes.is_empty() {
            return ZoneTree::default();
        }

        // ── 1. Build a prefix tree from inst_path ──
        let mut root = TrieNode::new("".to_string());

        for (box_id, path) in &zone_boxes {
            let segments: Vec<&str> = path.split('.').collect();
            let mut node = &mut root;
            for (i, seg) in segments.iter().enumerate() {
                let full_path = segments[..=i].join(".");
                node = node
                    .children
                    .entry(seg.to_string())
                    .or_insert_with(|| TrieNode::new(full_path));
            }
            // Leaf node: add the box
            node.boxes.push(*box_id);
        }

        // ── 2. Flatten single chains ──
        // A zone with only one child and no direct boxes of its own merges with that child
        let mut flattened: Vec<Zone> = Vec::new();
        flatten_trie(&mut root, &mut flattened, None);

        // ── 3. Merge small zones ──
        // Zones with box count < MIN_ZONE_SIZE are merged into the parent zone
        merge_small_zones(&mut flattened);

        // ── 4. Depth limit ──
        // Zones beyond MAX_ZONE_DEPTH are squashed into the parent zone
        enforce_depth_limit(&mut flattened);

        // ── 5. Compute roots ──
        let roots: Vec<usize> = flattened
            .iter()
            .filter(|z| z.parent.is_none())
            .map(|z| z.id)
            .collect();

        // ── Logging ──
        let leaf_count = flattened.iter().filter(|z| z.children.is_empty()).count();
        let max_depth = flattened
            .iter()
            .map(|z| z.path.matches('.').count())
            .max()
            .unwrap_or(0);
        mcc_dbg!(
            "viz",
            "[zone] tree: {} root zone(s), {} leaf zone(s), depth={}",
            roots.len(),
            leaf_count,
            max_depth
        );
        for &root_id in &roots {
            log_zone_tree(&flattened, root_id, 0);
        }
        let tiny_merged = flattened
            .iter()
            .filter(|z| z.boxes.len() < MIN_ZONE_SIZE && z.children.is_empty())
            .count();
        if tiny_merged > 0 {
            mcc_dbg!(
                "viz",
                "[zone] merged {} tiny zone(s) into parent",
                tiny_merged
            );
        }

        ZoneTree {
            zones: flattened,
            roots,
        }
    }
}

// ============================================================================
// Internal functions
// ============================================================================

/// Flatten: convert the prefix tree into a Zone list, merging single chains
fn flatten_trie(root: &mut TrieNode, zones: &mut Vec<Zone>, parent: Option<usize>) {
    // Recurse into each child node
    let child_keys: Vec<String> = root.children.keys().cloned().collect();
    let mut child_zone_ids: Vec<usize> = Vec::new();
    let mut root_boxes: Vec<i64> = root.boxes.clone();

    for key in child_keys {
        let mut child = root.children.remove(&key).unwrap();
        let (mut boxes, created_id) = flatten_subtree(&mut child, zones, parent);

        if let Some(id) = created_id {
            child_zone_ids.push(id);
        } else {
            // The child created no zone; boxes go to the root
            root_boxes.append(&mut boxes);
        }
    }

    // If it has child zones or its own boxes → create a zone
    if !child_zone_ids.is_empty() || !root_boxes.is_empty() {
        let id = zones.len();
        let title = zone_title(if root.path.is_empty() {
            "main"
        } else {
            &root.path
        });
        let zone = Zone {
            id,
            path: if root.path.is_empty() {
                "main".to_string()
            } else {
                root.path.clone()
            },
            title,
            boxes: root_boxes,
            children: child_zone_ids.clone(),
            parent,
        };
        zones.push(zone);
        for &cid in &child_zone_ids {
            zones[cid].parent = Some(id);
        }
    }
}

/// Recursively process a subtree, returning (boxes_to_attach, Option<zone_id>)
///
/// - If a zone was created, returns (remaining_boxes, Some(zone_id))
/// - If flattened, returns (all_boxes, None)
fn flatten_subtree(
    node: &mut TrieNode,
    zones: &mut Vec<Zone>,
    parent: Option<usize>,
) -> (Vec<i64>, Option<usize>) {
    // Recurse into child nodes first
    let child_keys: Vec<String> = node.children.keys().cloned().collect();
    let mut child_zone_ids: Vec<usize> = Vec::new();
    let mut node_boxes: Vec<i64> = node.boxes.clone();

    for key in child_keys {
        let mut child = node.children.remove(&key).unwrap();
        let (mut boxes, created_id) = flatten_subtree(&mut child, zones, parent);
        if let Some(id) = created_id {
            child_zone_ids.push(id);
        } else {
            node_boxes.append(&mut boxes);
        }
    }

    // Flatten: only one child and no direct boxes of its own → merge into the child
    if child_zone_ids.len() == 1 && node_boxes.is_empty() {
        let child = &mut zones[child_zone_ids[0]];
        child.path = node.path.clone();
        child.parent = parent;
        return (Vec::new(), Some(child_zone_ids[0]));
    }

    // No children: create no zone; boxes return to the parent node
    if child_zone_ids.is_empty() {
        return (node_boxes, None);
    }

    // Create a zone
    let id = zones.len();
    let title = zone_title(&node.path);
    let zone = Zone {
        id,
        path: node.path.clone(),
        title,
        boxes: node_boxes,
        children: child_zone_ids.clone(),
        parent,
    };
    zones.push(zone);
    for &cid in &child_zone_ids {
        zones[cid].parent = Some(id);
    }

    (Vec::new(), Some(id))
}

/// Merge small zones: zones with box count < MIN_ZONE_SIZE are merged into the parent zone
fn merge_small_zones(zones: &mut Vec<Zone>) {
    let mut i = 0;
    while i < zones.len() {
        if zones[i].boxes.len() >= MIN_ZONE_SIZE || zones[i].parent.is_none() {
            i += 1;
            continue;
        }

        let pid = zones[i].parent.unwrap();
        // Merge boxes into the parent zone
        let boxes = zones[i].boxes.clone();
        let children = zones[i].children.clone();
        zones[pid].boxes.extend(boxes);

        // Re-attach the zone's children to the parent zone
        for &cid in &children {
            zones[cid].parent = Some(pid);
        }

        // Remove the current zone from the parent zone's children
        if let Some(pos) = zones[pid].children.iter().position(|&c| c == i) {
            zones[pid].children.remove(pos);
        }
        zones[pid].children.extend(children);

        i += 1;
    }
}

/// Depth limit: zones beyond MAX_ZONE_DEPTH are squashed into the parent zone
fn enforce_depth_limit(zones: &mut Vec<Zone>) {
    // Recursively compute each zone's actual depth
    let depths = compute_zone_depths(zones);

    let mut i = 0;
    while i < zones.len() {
        let depth = depths.get(&i).copied().unwrap_or(0);
        if depth <= MAX_ZONE_DEPTH || zones[i].parent.is_none() {
            i += 1;
            continue;
        }

        let pid = zones[i].parent.unwrap();
        // Merge boxes and children into the parent zone
        let boxes = zones[i].boxes.clone();
        let children = zones[i].children.clone();
        zones[pid].boxes.extend(boxes);

        for &cid in &children {
            zones[cid].parent = Some(pid);
        }
        if let Some(pos) = zones[pid].children.iter().position(|&c| c == i) {
            zones[pid].children.remove(pos);
        }
        zones[pid].children.extend(children);

        i += 1;
    }
}

/// Compute each zone's depth (from the root)
fn compute_zone_depths(zones: &[Zone]) -> BTreeMap<usize, usize> {
    let mut depths = BTreeMap::new();
    for zone in zones {
        let mut depth = 0usize;
        let mut cur = zone.parent;
        while let Some(pid) = cur {
            depth += 1;
            cur = zones[pid].parent;
        }
        depths.insert(zone.id, depth);
    }
    depths
}

/// Generate the zone title: prefer the class name of the last segment, fall back to the path
fn zone_title(path: &str) -> String {
    // Take the last segment
    let leaf = path.rsplit('.').next().unwrap_or(path);
    // If the last segment is main and the path is only main, return "main"
    if path == "main" {
        return "main".to_string();
    }
    leaf.to_string()
}

/// Recursively print the zone tree
fn log_zone_tree(zones: &[Zone], zone_id: usize, indent: usize) {
    let zone = &zones[zone_id];
    let prefix = "  ".repeat(indent);
    mcc_dbg!(
        "viz",
        "{}[zone] #{} '{}' ({} boxes) [{}]",
        prefix,
        zone.id,
        zone.title,
        zone.boxes.len(),
        zone.path
    );
    for &child_id in &zone.children {
        log_zone_tree(zones, child_id, indent + 1);
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::{IoSummary, McVecBox};
    use crate::vector::graph::{BoxKind, Symbol};

    fn make_box(id: i64, inst_path: &str, kind: BoxKind) -> McVecBox {
        McVecBox::new_v2(
            id,
            String::new(),
            String::new(),
            kind,
            Symbol::Unknown,
            None,
            None,
            0,
            IoSummary::default(),
            inst_path.to_string(),
            Vec::new(),
        )
    }

    fn empty_graph() -> McVecGraph {
        McVecGraph::new(0, String::new())
    }

    #[test]
    fn test_zone_tree_trivial() {
        // All boxes under main → single zone
        let mut graph = empty_graph();
        graph.boxes.push(make_box(1, "main.R1", BoxKind::TwoPin));
        graph.boxes.push(make_box(2, "main.R2", BoxKind::TwoPin));
        graph.boxes.push(make_box(3, "main.C1", BoxKind::TwoPin));

        let tree = ZoneTree::build(&graph);
        assert_eq!(tree.roots.len(), 1);
        assert_eq!(tree.zones[tree.roots[0]].boxes.len(), 3);
        assert_eq!(tree.zones[tree.roots[0]].title, "main");
    }

    #[test]
    fn test_zone_tree_modules() {
        // Multiple submodules → multiple zones
        let mut graph = empty_graph();
        graph
            .boxes
            .push(make_box(1, "main.modldo.ldo", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(2, "main.moddcdc.dcdc", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(3, "main.mic.MIC", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(4, "main.speaker.SPK", BoxKind::MultiPin));

        let tree = ZoneTree::build(&graph);
        // Each module should be one zone
        assert!(tree.roots.len() >= 1);
        let total_boxes: usize = tree.zones.iter().map(|z| z.boxes.len()).sum();
        assert_eq!(total_boxes, 4);
    }

    #[test]
    fn test_zone_tree_excludes_power_labels() {
        let mut graph = empty_graph();
        graph.boxes.push(make_box(1, "main.R1", BoxKind::TwoPin));
        graph
            .boxes
            .push(make_box(2, "main.GND", BoxKind::PowerLabel));
        graph
            .boxes
            .push(make_box(3, "main.VDD", BoxKind::PowerLabel));

        let tree = ZoneTree::build(&graph);
        let total_boxes: usize = tree.zones.iter().map(|z| z.boxes.len()).sum();
        // Only R1 enters a zone; PowerLabel does not
        assert_eq!(total_boxes, 1);
    }

    #[test]
    fn test_zone_tree_nested() {
        // Nested modules → reasonable structure after flattening
        let mut graph = empty_graph();
        graph
            .boxes
            .push(make_box(1, "main.pwr.ldo.ldo", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(2, "main.pwr.dcdc.dcdc", BoxKind::MultiPin));
        graph
            .boxes
            .push(make_box(3, "main.audio.mic.MIC", BoxKind::MultiPin));

        let tree = ZoneTree::build(&graph);
        // Should have a reasonable partition structure
        assert!(!tree.roots.is_empty());
    }
}
