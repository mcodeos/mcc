// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M11 — Idiom placement report
//!
//! Tracks what idiom placement did and did not do, for metrics and debugging.

use std::collections::BTreeMap;

use super::model::{IdiomInstanceKind, PlacementDecisionRecord};

// ============================================================================
// IdiomPlacementReport
// ============================================================================

/// Report produced after idiom placement pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IdiomPlacementReport {
    /// Total idioms detected.
    pub idioms_detected: usize,
    /// Idioms that were applicable (had actionable constraints).
    pub idioms_applicable: usize,
    /// Idioms where placement was actually applied.
    pub idioms_applied: usize,
    /// Idioms skipped (protected, collision, etc.).
    pub idioms_skipped: usize,

    /// Breakdown by kind: detected.
    pub by_kind_detected: BTreeMap<IdiomInstanceKind, usize>,
    /// Breakdown by kind: applied.
    pub by_kind_applied: BTreeMap<IdiomInstanceKind, usize>,

    /// Skipped because box was protected (ladder-locked, etc.).
    pub protected_skips: usize,
    /// Skipped because placement would cause collision.
    pub collision_skips: usize,
    /// Applied but reverted because collision detected post-move.
    pub collision_reverted: usize,

    /// M12: Total candidate positions evaluated.
    pub candidate_count: usize,
    /// M12: Selected candidates for determinism tracking.
    pub selected_candidates: Vec<PlacementDecisionRecord>,
    /// M12: Skip reasons by category.
    pub skip_reasons: BTreeMap<IdiomPlacementSkipReason, usize>,

    /// Warnings generated during placement.
    pub warnings: Vec<String>,
}

/// Why an idiom placement was skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IdiomPlacementSkipReason {
    Protected,
    AllCandidatesCollide,
    AnchorMissing,
    TargetMissing,
    NoConstraint,
}

impl IdiomPlacementReport {
    /// Merge another report into this one (accumulate across layers).
    pub fn merge(&mut self, other: &IdiomPlacementReport) {
        self.idioms_detected += other.idioms_detected;
        self.idioms_applicable += other.idioms_applicable;
        self.idioms_applied += other.idioms_applied;
        self.idioms_skipped += other.idioms_skipped;
        for (k, v) in &other.by_kind_detected {
            *self.by_kind_detected.entry(*k).or_insert(0) += v;
        }
        for (k, v) in &other.by_kind_applied {
            *self.by_kind_applied.entry(*k).or_insert(0) += v;
        }
        self.protected_skips += other.protected_skips;
        self.collision_skips += other.collision_skips;
        self.collision_reverted += other.collision_reverted;
        self.candidate_count += other.candidate_count;
        self.selected_candidates
            .extend(other.selected_candidates.clone());
        for (k, v) in &other.skip_reasons {
            *self.skip_reasons.entry(*k).or_insert(0) += v;
        }
        self.warnings.extend(other.warnings.clone());
    }

    /// Single-line log summary.
    pub fn report_line(&self) -> String {
        let decoupling_detected = self
            .by_kind_detected
            .get(&IdiomInstanceKind::Decoupling)
            .copied()
            .unwrap_or(0);
        let decoupling_applied = self
            .by_kind_applied
            .get(&IdiomInstanceKind::Decoupling)
            .copied()
            .unwrap_or(0);
        let pullup_detected = self
            .by_kind_detected
            .get(&IdiomInstanceKind::Pullup)
            .copied()
            .unwrap_or(0);
        let pullup_applied = self
            .by_kind_applied
            .get(&IdiomInstanceKind::Pullup)
            .copied()
            .unwrap_or(0);
        let pulldown_detected = self
            .by_kind_detected
            .get(&IdiomInstanceKind::Pulldown)
            .copied()
            .unwrap_or(0);
        let pulldown_applied = self
            .by_kind_applied
            .get(&IdiomInstanceKind::Pulldown)
            .copied()
            .unwrap_or(0);
        let diffpair_detected = self
            .by_kind_detected
            .get(&IdiomInstanceKind::DiffPair)
            .copied()
            .unwrap_or(0);
        let diffpair_applied = self
            .by_kind_applied
            .get(&IdiomInstanceKind::DiffPair)
            .copied()
            .unwrap_or(0);

        format!(
            "[metrics] IDIOM-PLACE: detected={} applicable={} applied={} skipped={} \
             candidates={} \
             decoupling={}/{} pullup={}/{} pulldown={}/{} diff_pair={}/{} \
             protected={} collision_skip={} reverted={} warnings={}",
            self.idioms_detected,
            self.idioms_applicable,
            self.idioms_applied,
            self.idioms_skipped,
            self.candidate_count,
            decoupling_detected,
            decoupling_applied,
            pullup_detected,
            pullup_applied,
            pulldown_detected,
            pulldown_applied,
            diffpair_detected,
            diffpair_applied,
            self.protected_skips,
            self.collision_skips,
            self.collision_reverted,
            self.warnings.len(),
        )
    }
}
