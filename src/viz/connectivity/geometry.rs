// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M13 — Geometry primitives for rendered connectivity
//!
//! Point-to-segment distance, segment intersection, overlap detection,
//! and junction clustering. All functions use quantized comparison for
//! deterministic results.

use super::model::{Point2D, SegmentOrientation};

// ============================================================================
// Epsilon constants
// ============================================================================

/// Touch epsilon: points within this distance are considered touching.
pub const TOUCH_EPSILON: f64 = 0.5;

/// Near-miss epsilon: points within this distance generate warnings.
pub const NEAR_MISS_EPSILON: f64 = 2.0;

// ============================================================================
// Point-to-segment distance
// ============================================================================

/// Compute the minimum distance from a point to a line segment.
pub fn point_to_segment_distance(p: Point2D, a: Point2D, b: Point2D) -> f64 {
    let ab_x = b.x - a.x;
    let ab_y = b.y - a.y;
    let ab_len_sq = ab_x * ab_x + ab_y * ab_y;

    if ab_len_sq < 1e-12 {
        return point_distance(p, a);
    }

    let ap_x = p.x - a.x;
    let ap_y = p.y - a.y;
    let t = ((ap_x * ab_x + ap_y * ab_y) / ab_len_sq).clamp(0.0, 1.0);

    let proj_x = a.x + t * ab_x;
    let proj_y = a.y + t * ab_y;

    let dx = p.x - proj_x;
    let dy = p.y - proj_y;
    (dx * dx + dy * dy).sqrt()
}

/// Check if a point touches a segment (within TOUCH_EPSILON).
pub fn point_touches_segment(p: Point2D, a: Point2D, b: Point2D) -> bool {
    point_to_segment_distance(p, a, b) <= TOUCH_EPSILON
}

/// Check if a point is near a segment but not touching (within NEAR_MISS_EPSILON).
pub fn point_near_miss_segment(p: Point2D, a: Point2D, b: Point2D) -> bool {
    let d = point_to_segment_distance(p, a, b);
    d > TOUCH_EPSILON && d <= NEAR_MISS_EPSILON
}

// ============================================================================
// Point-to-point distance
// ============================================================================

/// Euclidean distance between two points.
pub fn point_distance(a: Point2D, b: Point2D) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

/// Check if two points are within epsilon.
pub fn points_touch(a: Point2D, b: Point2D, epsilon: f64) -> bool {
    point_distance(a, b) <= epsilon
}

// ============================================================================
// Segment intersection
// ============================================================================

/// Determine if two segments intersect and return the intersection point if so.
pub fn segment_intersection(a1: Point2D, a2: Point2D, b1: Point2D, b2: Point2D) -> Option<Point2D> {
    let d1_x = a2.x - a1.x;
    let d1_y = a2.y - a1.y;
    let d2_x = b2.x - b1.x;
    let d2_y = b2.y - b1.y;

    let cross = d1_x * d2_y - d1_y * d2_x;

    if cross.abs() < 1e-12 {
        return None; // Parallel or collinear
    }

    let t = ((b1.x - a1.x) * d2_y - (b1.y - a1.y) * d2_x) / cross;
    let u = ((b1.x - a1.x) * d1_y - (b1.y - a1.y) * d1_x) / cross;

    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(Point2D::new(a1.x + t * d1_x, a1.y + t * d1_y))
    } else {
        None
    }
}

/// Check if two segments have a collinear overlap (share a non-zero length of their path).
pub fn segment_collinear_overlap(
    a1: Point2D,
    a2: Point2D,
    b1: Point2D,
    b2: Point2D,
) -> Option<(Point2D, Point2D)> {
    // Check if same orientation
    let a_orient = segment_orientation(a1, a2);
    let b_orient = segment_orientation(b1, b2);
    if a_orient != b_orient || a_orient == SegmentOrientation::Degenerate {
        return None;
    }

    let (a_min, a_max, b_min, b_max) = match a_orient {
        SegmentOrientation::Horizontal => {
            if (a1.y - b1.y).abs() > TOUCH_EPSILON {
                return None;
            }
            (
                a1.x.min(a2.x),
                a1.x.max(a2.x),
                b1.x.min(b2.x),
                b1.x.max(b2.x),
            )
        }
        SegmentOrientation::Vertical => {
            if (a1.x - b1.x).abs() > TOUCH_EPSILON {
                return None;
            }
            (
                a1.y.min(a2.y),
                a1.y.max(a2.y),
                b1.y.min(b2.y),
                b1.y.max(b2.y),
            )
        }
        _ => return None,
    };

    let overlap_min = a_min.max(b_min);
    let overlap_max = a_max.min(b_max);

    if overlap_max - overlap_min > TOUCH_EPSILON {
        let (p1, p2) = match a_orient {
            SegmentOrientation::Horizontal => (
                Point2D::new(overlap_min, a1.y),
                Point2D::new(overlap_max, a1.y),
            ),
            SegmentOrientation::Vertical => (
                Point2D::new(a1.x, overlap_min),
                Point2D::new(a1.x, overlap_max),
            ),
            _ => return None,
        };
        Some((p1, p2))
    } else {
        None
    }
}

// ============================================================================
// Segment orientation
// ============================================================================

/// Determine the orientation of a segment.
pub fn segment_orientation(a: Point2D, b: Point2D) -> SegmentOrientation {
    let dx = (b.x - a.x).abs();
    let dy = (b.y - a.y).abs();

    if dx < 1e-6 && dy < 1e-6 {
        SegmentOrientation::Degenerate
    } else if dy < 1e-6 {
        SegmentOrientation::Horizontal
    } else if dx < 1e-6 {
        SegmentOrientation::Vertical
    } else {
        SegmentOrientation::Diagonal
    }
}

// ============================================================================
// Point clustering
// ============================================================================

/// Cluster points that are within epsilon of each other.
/// Returns groups of indices into the input points.
pub fn cluster_points(points: &[Point2D], epsilon: f64) -> Vec<Vec<usize>> {
    let n = points.len();
    let mut visited = vec![false; n];
    let mut clusters = Vec::new();

    for i in 0..n {
        if visited[i] {
            continue;
        }
        let mut cluster = vec![i];
        visited[i] = true;
        let mut frontier = vec![i];
        while let Some(cur) = frontier.pop() {
            for j in 0..n {
                if !visited[j] && points_touch(points[cur], points[j], epsilon) {
                    visited[j] = true;
                    cluster.push(j);
                    frontier.push(j);
                }
            }
        }
        cluster.sort(); // deterministic ordering
        clusters.push(cluster);
    }

    clusters.sort_by_key(|c| c[0]); // stable ordering by first element
    clusters
}

/// Compute the centroid of a set of points.
pub fn centroid(points: &[Point2D]) -> Point2D {
    let n = points.len() as f64;
    let sum_x: f64 = points.iter().map(|p| p.x).sum();
    let sum_y: f64 = points.iter().map(|p| p.y).sum();
    Point2D::new(sum_x / n, sum_y / n)
}

// ============================================================================
// Bounding box
// ============================================================================

/// Check if a point is within the bounding box of a segment, expanded by epsilon.
pub fn point_in_segment_bbox(p: Point2D, a: Point2D, b: Point2D, epsilon: f64) -> bool {
    let min_x = a.x.min(b.x) - epsilon;
    let max_x = a.x.max(b.x) + epsilon;
    let min_y = a.y.min(b.y) - epsilon;
    let max_y = a.y.max(b.y) + epsilon;
    p.x >= min_x && p.x <= max_x && p.y >= min_y && p.y <= max_y
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_on_horizontal_segment() {
        let p = Point2D::new(50.0, 0.0);
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(100.0, 0.0);
        assert!(point_touches_segment(p, a, b));
    }

    #[test]
    fn point_off_segment() {
        let p = Point2D::new(50.0, 10.0);
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(100.0, 0.0);
        assert!(!point_touches_segment(p, a, b));
    }

    #[test]
    fn point_near_miss() {
        let p = Point2D::new(50.0, 1.0);
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(100.0, 0.0);
        assert!(point_near_miss_segment(p, a, b));
    }

    #[test]
    fn segments_intersect_crossing() {
        let a1 = Point2D::new(0.0, 50.0);
        let a2 = Point2D::new(100.0, 50.0);
        let b1 = Point2D::new(50.0, 0.0);
        let b2 = Point2D::new(50.0, 100.0);
        let result = segment_intersection(a1, a2, b1, b2);
        assert!(result.is_some());
        let p = result.unwrap();
        assert!((p.x - 50.0).abs() < 0.01);
        assert!((p.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn parallel_segments_no_intersection() {
        let a1 = Point2D::new(0.0, 0.0);
        let a2 = Point2D::new(100.0, 0.0);
        let b1 = Point2D::new(0.0, 50.0);
        let b2 = Point2D::new(100.0, 50.0);
        assert!(segment_intersection(a1, a2, b1, b2).is_none());
    }

    #[test]
    fn collinear_overlap_horizontal() {
        let a1 = Point2D::new(0.0, 0.0);
        let a2 = Point2D::new(100.0, 0.0);
        let b1 = Point2D::new(50.0, 0.0);
        let b2 = Point2D::new(150.0, 0.0);
        let result = segment_collinear_overlap(a1, a2, b1, b2);
        assert!(result.is_some());
        let (p1, p2) = result.unwrap();
        assert!((p1.x - 50.0).abs() < 0.01);
        assert!((p2.x - 100.0).abs() < 0.01);
    }

    #[test]
    fn cluster_points_deterministic() {
        let pts = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(0.1, 0.1),
            Point2D::new(10.0, 10.0),
            Point2D::new(10.1, 10.0),
        ];
        let c1 = cluster_points(&pts, 0.5);
        let c2 = cluster_points(&pts, 0.5);
        assert_eq!(c1, c2, "Clustering should be deterministic");
        assert_eq!(c1.len(), 2);
    }

    #[test]
    fn segment_orientation_horizontal() {
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(100.0, 0.0);
        assert_eq!(segment_orientation(a, b), SegmentOrientation::Horizontal);
    }

    #[test]
    fn segment_orientation_vertical() {
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(0.0, 100.0);
        assert_eq!(segment_orientation(a, b), SegmentOrientation::Vertical);
    }
}
