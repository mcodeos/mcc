// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M12 — DeterministicScore and quantized distance helpers
//!
//! Unified score tuple for candidate ranking. All tie-breaks use integer
//! penalties and StableDecisionKey, never bare `f64::partial_cmp`.

use super::key::StableDecisionKey;

// ============================================================================
// DeterministicScore
// ============================================================================

/// Unified score tuple for any candidate decision.
///
/// Lower is better. Fields are compared in declaration order.
/// `stable_key` is the final tie-break when all other fields are equal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeterministicScore {
    /// 0 = safe, 1 = target protected, 2 = anchor/target missing
    pub hard_violation: u8,
    /// Collision penalty: 0 = no collision, 100000 = box collision
    pub collision_penalty: i32,
    /// Semantic penalty: wrong side, etc.
    pub semantic_penalty: i32,
    /// Quantized distance penalty.
    pub distance_penalty: i32,
    /// Side preference penalty.
    pub side_penalty: i32,
    /// Bend count penalty (for routes).
    pub bend_penalty: i32,
    /// Canvas overflow penalty.
    pub canvas_penalty: i32,
    /// Final tie-break: deterministic stable key.
    pub stable_key: StableDecisionKey,
}

impl DeterministicScore {
    /// Create a zero (best) score with the given stable key.
    pub fn zero(key: StableDecisionKey) -> Self {
        Self {
            hard_violation: 0,
            collision_penalty: 0,
            semantic_penalty: 0,
            distance_penalty: 0,
            side_penalty: 0,
            bend_penalty: 0,
            canvas_penalty: 0,
            stable_key: key,
        }
    }

    /// Create a score for a protected target (cannot be moved).
    pub fn protected(key: StableDecisionKey) -> Self {
        Self {
            hard_violation: 1,
            collision_penalty: 0,
            semantic_penalty: 0,
            distance_penalty: 0,
            side_penalty: 0,
            bend_penalty: 0,
            canvas_penalty: 0,
            stable_key: key,
        }
    }

    /// Create a score for a colliding candidate.
    pub fn collision(key: StableDecisionKey) -> Self {
        Self {
            hard_violation: 0,
            collision_penalty: 100000,
            semantic_penalty: 0,
            distance_penalty: 0,
            side_penalty: 0,
            bend_penalty: 0,
            canvas_penalty: 0,
            stable_key: key,
        }
    }

    pub fn with_distance(self, penalty: i32) -> Self {
        Self {
            distance_penalty: penalty,
            ..self
        }
    }

    pub fn with_side(self, penalty: i32) -> Self {
        Self {
            side_penalty: penalty,
            ..self
        }
    }

    pub fn with_canvas(self, penalty: i32) -> Self {
        Self {
            canvas_penalty: penalty,
            ..self
        }
    }

    /// Returns true if the candidate is safely placeable (no hard violation, no collision).
    pub fn is_safe(&self) -> bool {
        self.hard_violation == 0 && self.collision_penalty == 0
    }
}

// ============================================================================
// Quantized distance
// ============================================================================

/// Quantize a float to 0.1px precision as an integer.
///
/// Never use bare `f64::partial_cmp` for final tie-break.
pub fn quantized_px(v: f64) -> i64 {
    (v * 10.0).round() as i64
}

/// Quantized Manhattan distance.
pub fn quantized_manhattan(ax: f64, ay: f64, bx: f64, by: f64) -> i64 {
    quantized_px((ax - bx).abs()) + quantized_px((ay - by).abs())
}

/// Quantized Euclidean distance.
pub fn quantized_euclidean(ax: f64, ay: f64, bx: f64, by: f64) -> i64 {
    let dx = (ax - bx).abs();
    let dy = (ay - by).abs();
    quantized_px(dx.hypot(dy))
}

/// Side preference penalty: 0 = preferred, 10 = secondary, 20 = fallback, 30 = opposite.
pub fn side_penalty(side_index: usize) -> i32 {
    match side_index {
        0 => 0,
        1 => 10,
        2 => 20,
        _ => 30,
    }
}

// ============================================================================
// PlacementCandidate
// ============================================================================

/// Internal structure for score-all candidate placement.
#[derive(Debug, Clone)]
pub struct PlacementCandidate {
    pub target_box_id: i64,
    pub anchor_box_id: i64,
    pub side_index: usize,
    pub candidate_index: usize,
    pub x: f64,
    pub y: f64,
    pub score: DeterministicScore,
}

impl PlacementCandidate {
    pub fn new(
        target_box_id: i64,
        anchor_box_id: i64,
        side_index: usize,
        candidate_index: usize,
        x: f64,
        y: f64,
        score: DeterministicScore,
    ) -> Self {
        Self {
            target_box_id,
            anchor_box_id,
            side_index,
            candidate_index,
            x,
            y,
            score,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantized_px_ignores_tiny_noise() {
        let a = quantized_px(100.00001);
        let b = quantized_px(100.00009);
        assert_eq!(a, b, "Tiny float noise should not affect quantized value");
    }

    #[test]
    fn quantized_manhattan_stable() {
        let d1 = quantized_manhattan(0.0, 0.0, 100.0, 50.0);
        let d2 = quantized_manhattan(0.0, 0.0, 100.0, 50.0);
        assert_eq!(d1, d2);
    }

    #[test]
    fn score_ordering() {
        let key = |i| StableDecisionKey::new(0, 0, 10, i, i, 0, 0, 0);
        let s1 = DeterministicScore::zero(key(1));
        let s2 = DeterministicScore::collision(key(2));
        assert!(s1 < s2, "Zero score should be better than collision");
    }

    #[test]
    fn score_safe_detection() {
        let key = StableDecisionKey::new(0, 0, 10, 1, 1, 0, 0, 0);
        let safe = DeterministicScore::zero(key.clone());
        let coll = DeterministicScore::collision(key.clone());
        let prot = DeterministicScore::protected(key);
        assert!(safe.is_safe());
        assert!(!coll.is_safe());
        assert!(!prot.is_safe());
    }
}
