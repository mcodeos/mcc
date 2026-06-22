// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Simple grid layout (alternative / debugging)
//!
//! Doesn't look at graph structure, simply arranges by rows and columns at equal intervals.
//! Use case: quick verification / small internal sub-module circuits / quickly see "which boxes exist" while debugging.

use crate::vector::graph::McVecGraph;

use super::entry_points::assign_entry_points;
use super::normalize::{compute_canvas, normalize_positions, CANVAS_MARGIN};
use super::size::{assign_default_sizes, MIN_GAP};
use crate::viz::traits::Layouter;

/// Grid layout
///
/// Arrange boxes by rows and columns at equal intervals, row width = `cols`, row height adapts to tallest box in row.
pub struct GridLayouter {
    /// Number of boxes per row (default 4)
    pub cols: usize,
}

impl Default for GridLayouter {
    fn default() -> Self {
        Self { cols: 4 }
    }
}

impl Layouter for GridLayouter {
    fn layout(&self, graph: &mut McVecGraph) -> (f64, f64) {
        if graph.boxes.is_empty() {
            return (200.0, 100.0);
        }

        assign_default_sizes(graph);
        assign_entry_points(graph);

        let cols = self.cols.max(1);
        let mut cur_y = CANVAS_MARGIN;
        let n = graph.boxes.len();
        let mut i = 0;

        while i < n {
            // Range of this row [i, end)
            let end = (i + cols).min(n);
            // Tallest in this row
            let row_h = graph.boxes[i..end]
                .iter()
                .map(|b| b.h)
                .fold(0.0f64, f64::max);
            // Lay out horizontally
            let mut cur_x = CANVAS_MARGIN;
            for k in i..end {
                let b = &mut graph.boxes[k];
                b.x = cur_x;
                b.y = cur_y;
                cur_x += b.w + MIN_GAP;
            }
            cur_y += row_h + MIN_GAP;
            i = end;
        }

        normalize_positions(graph);
        compute_canvas(graph)
    }

    fn name(&self) -> &'static str {
        "grid"
    }
}
