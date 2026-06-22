// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! [`VizLayer`] —— single-layer layout + render result
//!
//! Every `McVecBlock` (top level + each sub_module recursively) corresponds to one
//! `VizLayer`, containing the SVG for that layer + which boxes are clickable to expand.
//!
//! ## Relationship with [`super::doc::VizDocument`]
//! `VizDocument` is a map of all layers; `VizLayer` is a single layer.
//! The frontend switches layers by taking the corresponding `VizLayer` from
//! `VizDocument.layers[bid]`, and simply `innerHTML`-ing the `svg` field.

// ============================================================================
// VizLayer
// ============================================================================

/// Layout + render result of a single layer
#[derive(Debug, Clone)]
pub struct VizLayer {
    /// This layer's block ID
    pub bid: i64,
    /// This layer's block name (module instance name)
    pub name: String,
    /// Parent layer bid (top level is `None`)
    ///
    /// Used for "go back to previous layer" and breadcrumb navigation.
    pub parent_bid: Option<i64>,
    /// This layer's canvas size (width, height) —— SVG viewBox
    pub canvas: (f64, f64),
    /// ★ Pre-rendered SVG string for this layer
    ///
    /// The frontend can just do `canvas.innerHTML = layer.svg` to display it.
    pub svg: String,
    /// Box IDs in this layer that can be clicked to expand (i.e. sub-module box IDs)
    ///
    /// On click, the frontend uses `bid` to look up the next layer in `VizDocument.layers`.
    pub clickable_subs: Vec<i64>,
}

impl VizLayer {
    pub fn new(bid: i64, name: String, parent_bid: Option<i64>) -> Self {
        Self {
            bid,
            name,
            parent_bid,
            canvas: (0.0, 0.0),
            svg: String::new(),
            clickable_subs: Vec::new(),
        }
    }

    /// Whether this is the top level (no parent)
    pub fn is_root(&self) -> bool {
        self.parent_bid.is_none()
    }

    /// SVG character count (for diagnostics)
    pub fn svg_size(&self) -> usize {
        self.svg.len()
    }
}
