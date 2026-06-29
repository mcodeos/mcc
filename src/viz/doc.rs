// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`VizDocument`] —— multi-layer pre-rendered document (fixed version)
//!
//! ## Fixes (vs P2 original)
//! - `to_json` outputs additional `crate::vlog!` diagnostics: reports whether root_bid is actually in layers
//! - Added `validate()` method: users can verify consistency on the Rust side

use std::collections::HashMap;

use super::layer::VizLayer;
use crate::vector::graph::json_escape;

#[derive(Debug, Clone)]
pub struct VizDocument {
    pub root_bid: i64,
    pub root_name: String,
    pub layers: HashMap<i64, VizLayer>,
}

impl VizDocument {
    pub fn new(root_bid: i64, root_name: String) -> Self {
        Self {
            root_bid,
            root_name,
            layers: HashMap::new(),
        }
    }

    pub fn add_layer(&mut self, layer: VizLayer) {
        self.layers.insert(layer.bid, layer);
    }

    pub fn root_layer(&self) -> Option<&VizLayer> {
        self.layers.get(&self.root_bid)
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn total_svg_bytes(&self) -> usize {
        self.layers.values().map(|l| l.svg_size()).sum()
    }

    pub fn path_to(&self, bid: i64) -> Vec<i64> {
        let mut path = Vec::new();
        let mut cur = Some(bid);
        while let Some(c) = cur {
            path.push(c);
            cur = self.layers.get(&c).and_then(|l| l.parent_bid);
        }
        path.reverse();
        path
    }

    // ─── ★ NEW: consistency validation ──────────────────────────────────────────────

    /// Validate document consistency, returns the list of issues (empty = everything is fine)
    ///
    /// Check items:
    /// - Whether root_bid is actually in layers
    /// - Whether every clickable_sub has a corresponding layer
    /// - Whether every parent_bid actually exists
    /// - Whether any layer has empty svg
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // 1) root_bid must exist
        if !self.layers.contains_key(&self.root_bid) {
            issues.push(format!(
                "★ CRITICAL: root_bid={} not in layers (layers contains: {:?})",
                self.root_bid,
                self.layers.keys().collect::<Vec<_>>()
            ));
        }

        // 2) Each layer's parent must exist (except root)
        for (bid, layer) in &self.layers {
            if let Some(parent) = layer.parent_bid {
                if !self.layers.contains_key(&parent) {
                    issues.push(format!(
                        "Layer #{} '{}' has parent_bid={} but parent not in layers",
                        bid, layer.name, parent
                    ));
                }
            }
        }

        // 3) Each clickable_sub must have a corresponding layer
        for (bid, layer) in &self.layers {
            for &sub_bid in &layer.clickable_subs {
                if !self.layers.contains_key(&sub_bid) {
                    issues.push(format!(
                        "Layer #{} '{}' lists clickable_sub={} but no such layer",
                        bid, layer.name, sub_bid
                    ));
                }
            }
        }

        // 4) Empty svg warning
        for (bid, layer) in &self.layers {
            if layer.svg.is_empty() {
                issues.push(format!(
                    "WARN: Layer #{} '{}' has empty svg",
                    bid, layer.name
                ));
            }
        }

        issues
    }

    // ─── JSON serialization ──────────────────────────────────────────────────

    pub fn to_json(&self) -> String {
        // ★ Run consistency check before serializing (only print under MC_VIZ_DUMP)
        let issues = self.validate();
        if super::debug::dump_enabled() {
            if !issues.is_empty() {
                crate::vlog!(
                    "[viz::doc] ⚠ VizDocument validation found {} issue(s):",
                    issues.len()
                );
                for issue in &issues {
                    crate::vlog!("[viz::doc]   {issue}");
                }
            } else {
                crate::vlog!(
                    "[viz::doc] ✓ document OK: root_bid={}, {} layers, total {} bytes SVG",
                    self.root_bid,
                    self.layer_count(),
                    self.total_svg_bytes()
                );
            }
        }

        let mut out = String::new();
        out.push_str("{\"root_bid\":");
        out.push_str(&self.root_bid.to_string());
        out.push_str(",\"root_name\":\"");
        out.push_str(&json_escape(&self.root_name));
        out.push_str("\",\"layers\":{");

        let mut bids: Vec<i64> = self.layers.keys().copied().collect();
        bids.sort();
        for (i, bid) in bids.iter().enumerate() {
            let layer = &self.layers[bid];
            out.push('"');
            out.push_str(&bid.to_string());
            out.push_str("\":{");
            out.push_str("\"bid\":");
            out.push_str(&bid.to_string());
            out.push_str(",\"name\":\"");
            out.push_str(&json_escape(&layer.name));
            out.push('"');
            if let Some(parent) = layer.parent_bid {
                out.push_str(",\"parent\":");
                out.push_str(&parent.to_string());
            } else {
                out.push_str(",\"parent\":null");
            }
            out.push_str(",\"canvas\":[");
            out.push_str(&format!("{:.1},{:.1}", layer.canvas.0, layer.canvas.1));
            out.push_str("],\"clickable_subs\":[");
            for (j, &sub) in layer.clickable_subs.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&sub.to_string());
            }
            out.push_str("],\"svg\":\"");
            out.push_str(&json_escape(&layer.svg));
            out.push_str("\"}");
            if i + 1 < bids.len() {
                out.push(',');
            }
        }
        out.push_str("}}");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_layer(bid: i64, name: &str, parent: Option<i64>) -> VizLayer {
        let mut l = VizLayer::new(bid, name.to_string(), parent);
        l.canvas = (800.0, 600.0);
        l.svg = format!("<svg id=\"layer{}\"/>", bid);
        l
    }

    #[test]
    fn test_validate_ok() {
        let mut doc = VizDocument::new(100, "main".into());
        doc.add_layer(mk_layer(100, "main", None));
        doc.add_layer(mk_layer(200, "sub", Some(100)));
        assert!(doc.validate().is_empty());
    }

    #[test]
    fn test_validate_missing_root() {
        let mut doc = VizDocument::new(999, "ghost".into());
        doc.add_layer(mk_layer(100, "main", None));
        let issues = doc.validate();
        assert!(!issues.is_empty());
        assert!(issues[0].contains("CRITICAL"));
        assert!(issues[0].contains("999"));
    }

    #[test]
    fn test_validate_missing_parent() {
        let mut doc = VizDocument::new(100, "main".into());
        doc.add_layer(mk_layer(100, "main", None));
        doc.add_layer(mk_layer(200, "sub", Some(999))); // 999 does not exist
        let issues = doc.validate();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("parent_bid=999"));
    }
}
