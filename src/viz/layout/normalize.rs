// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Coordinate normalization: shift all boxes so minimum coordinates ≥ margin
//!
//! Called at the last step of the layout main flow, ensuring SVG viewBox starts from (0,0).

use crate::vector::graph::McVecGraph;

/// Margin (left / top)
pub const CANVAS_MARGIN: f64 = 30.0;
/// Extra padding for bottom-right of canvas
pub const CANVAS_PADDING: f64 = 60.0;

/// Shift all coordinates so `min_x >= MARGIN` and `min_y >= MARGIN`
pub fn normalize_positions(graph: &mut McVecGraph) {
    if graph.boxes.is_empty() {
        return;
    }

    let min_x = graph.boxes.iter().map(|b| b.x).fold(f64::MAX, f64::min);
    let min_y = graph.boxes.iter().map(|b| b.y).fold(f64::MAX, f64::min);

    let shift_x = if min_x < CANVAS_MARGIN {
        CANVAS_MARGIN - min_x
    } else {
        0.0
    };
    let shift_y = if min_y < CANVAS_MARGIN {
        CANVAS_MARGIN - min_y
    } else {
        0.0
    };

    if shift_x > 0.0 || shift_y > 0.0 {
        for b in &mut graph.boxes {
            b.x += shift_x;
            b.y += shift_y;
        }
        // ★ 已经落好的 route 必须跟着走。任何"先落线再归一化"的确定性摆位器
        // （sp_place / ladder_place / 未来的 placer）都依赖这条不变式：
        // normalize_positions 是一个刚体平移，几何与布线一起动。
        for n in &mut graph.nets {
            let Some(r) = n.route.as_mut() else { continue };
            for s in &mut r.segments {
                s.from.x += shift_x;
                s.from.y += shift_y;
                s.to.x += shift_x;
                s.to.y += shift_y;
            }
            for j in &mut r.junctions {
                j.x += shift_x;
                j.y += shift_y;
            }
        }
    }
}

/// Re-run position normalization (public alias used by the render pipeline after
/// post-layout passes such as `place_series_passives`, which may push a box to a
/// negative coordinate). Idempotent: a no-op when everything is already ≥ margin.
pub fn renormalize(graph: &mut McVecGraph) {
    normalize_positions(graph);
}

/// Compute normalized canvas size `(width, height)`
///
/// Tolerant of negative minimums: even if a post-layout pass left a box at a negative
/// coordinate (before `renormalize` runs, or in an unnormalized graph), the canvas still
/// covers the full bounding box instead of clipping content off the top-left.
pub fn compute_canvas(graph: &McVecGraph) -> (f64, f64) {
    if graph.boxes.is_empty() {
        return (200.0, 100.0);
    }
    let min_x = graph
        .boxes
        .iter()
        .map(|b| b.x)
        .fold(f64::MAX, f64::min)
        .min(0.0);
    let min_y = graph
        .boxes
        .iter()
        .map(|b| b.y)
        .fold(f64::MAX, f64::min)
        .min(0.0);
    let max_x = graph.boxes.iter().map(|b| b.x + b.w).fold(0.0f64, f64::max);
    let max_y = graph.boxes.iter().map(|b| b.y + b.h).fold(0.0f64, f64::max);
    (
        (max_x - min_x) + CANVAS_PADDING,
        (max_y - min_y) + CANVAS_PADDING,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::netdef::{Point, Route, Segment, VizNet};
    use crate::vector::graph::{BoxKind, McVecBox, NetKind, Symbol};

    #[test]
    fn shift_moves_routes_with_boxes() {
        let mut g = McVecGraph::new(1, "t".into());
        let mut b = McVecBox::new_v2(
            1,
            "b".into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Ic,
            None,
            None,
            2,
            IoSummary::new(),
        );
        b.x = -70.0;
        b.y = 10.0;
        b.w = 20.0;
        b.h = 20.0;
        g.boxes.push(b);
        let mut net = VizNet::new(0, "n".into(), NetKind::Signal, vec![]);
        let mut r = Route::new();
        r.segments.push(Segment {
            from: Point::new(-70.0, 20.0),
            to: Point::new(30.0, 20.0),
        });
        r.junctions.push(Point::new(30.0, 20.0));
        net.route = Some(r);
        g.nets.push(net);

        normalize_positions(&mut g);

        let shift = CANVAS_MARGIN - (-70.0);
        let r = g.nets[0].route.as_ref().unwrap();
        assert_eq!(g.boxes[0].x, -70.0 + shift);
        assert_eq!(r.segments[0].from.x, -70.0 + shift);
        assert_eq!(r.junctions[0].x, 30.0 + shift);
        // 线仍贴在盒子边上
        assert_eq!(r.segments[0].from.x, g.boxes[0].x);
    }
}
