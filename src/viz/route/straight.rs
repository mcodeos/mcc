// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Straight-line routing (debug / simple scenarios)
//!
//! Connect net's endpoints pairwise with straight lines —— no detour, for clearly
//! seeing "logical connections" during debugging.
//! Suitable for: two boxes in the same row/column, or for diagnosing whether the
//! Manhattan algorithm went wrong.
//!
//! ★ B1: orthogonality invariant — StraightRouter only accepts collinear endpoints
//! (same x or same y, tolerance 1.0). Non-collinear endpoints must use
//! orthogonal.rs for L/Z-shaped routing.

use crate::vector::graph::{McVecGraph, Point, Route, Segment, VizNet};

use crate::viz::traits::Router;

/// Tolerance for "same x" or "same y" collinearity check.
const COLINEAR_EPS: f64 = 1.0;

pub struct StraightRouter;

impl Router for StraightRouter {
    fn route(&self, graph: &McVecGraph, net: &mut VizNet) {
        let mut route = Route::new();

        for i in 0..net.endpoints.len() {
            for j in (i + 1)..net.endpoints.len() {
                let a = &net.endpoints[i];
                let b = &net.endpoints[j];
                let box_a = graph.boxes.iter().find(|x| x.id == a.box_id);
                let box_b = graph.boxes.iter().find(|x| x.id == b.box_id);
                if let (Some(ba), Some(bb)) = (box_a, box_b) {
                    let acx = ba.x + ba.w / 2.0;
                    let acy = ba.y + ba.h / 2.0;
                    let bcx = bb.x + bb.w / 2.0;
                    let bcy = bb.y + bb.h / 2.0;

                    // ★ B1: collinearity precondition — straight lines only allowed
                    // when endpoints share x or y within tolerance.
                    let dx = (acx - bcx).abs();
                    let dy = (acy - bcy).abs();
                    if dx > COLINEAR_EPS && dy > COLINEAR_EPS {
                        // Non-collinear: StraightRouter cannot produce an orthogonal
                        // segment. In debug builds, panic to catch the bug early.
                        // In release builds, emit a warning and skip this pair.
                        #[cfg(debug_assertions)]
                        panic!(
                            "StraightRouter requires colinear endpoints for net '{}' \
                             (box '{}' at ({:.1},{:.1}) vs '{}' at ({:.1},{:.1}), dx={:.1} dy={:.1})",
                            net.name, ba.name, acx, acy, bb.name, bcx, bcy, dx, dy
                        );
                        #[cfg(not(debug_assertions))]
                        {
                            crate::vlog!(
                                "[B1] StraightRouter: non-colinear endpoints for net '{}' \
                                 (box '{}' vs '{}'), skipping pair",
                                net.name,
                                ba.name,
                                bb.name
                            );
                            continue;
                        }
                    }

                    route.segments.push(Segment {
                        from: Point::new(acx, acy),
                        to: Point::new(bcx, bcy),
                    });
                }
            }
        }

        net.route = Some(route);
    }

    fn name(&self) -> &'static str {
        "straight"
    }
}
