// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Operator folding for the shadow vector-expression evaluator (`eval_expr`'s
//! operator arms; unified-core §7.7(1)).
//!
//! Every legality decision is **imported from [`opcheck`]**, never recomputed
//! here: a second copy of a rule is a second drift source (`majority_dir`
//! already demonstrated where "copy the rule once more" leads).
//!
//! [`opcheck`]: crate::semantic::opcheck

use super::ConcreteOpd;
use crate::semantic::common::Shape;
use crate::semantic::opcheck::{check_series_rows, parallel_attaches_right, OpCheck};

/// §5.2 series (`-` / `->` / `<-`): the running accumulator joined to the next
/// operand at `(acc.right, next.left)`; the result anchors right, so the
/// chain's external faces stay its two free ends.
pub struct SeriesStep {
    pub result: ConcreteOpd,
    /// The faces the internal wiring must join.
    pub pair: (Vec<super::Ep>, Vec<super::Ep>),
    /// `false` when the paired row counts differ (an illegal §5.2 operation —
    /// the caller reports it and generates no connection).
    pub legal: bool,
    /// True when either contracted face is empty. An empty face is not
    /// connectable, but it is **not** an illegal §5.2 operation either: the
    /// caller skips the leg silently, the same explicit empty guard
    /// `try_connect_adjacent` applied before its row check.
    pub skipped: bool,
}

/// Fold one `Series` leg. Mirrors `connect_adjacent_pair`: an empty face is not
/// connectable, so the leg is skipped rather than reported as a row mismatch.
pub fn fold_series(acc: &ConcreteOpd, next: &ConcreteOpd) -> SeriesStep {
    let skipped = acc.right.is_empty() || next.left.is_empty();
    let legal = !skipped
        && matches!(
            check_series_rows(Shape::vvec(acc.right.len()), Shape::vvec(next.left.len()),),
            OpCheck::Legal(_)
        );
    let (left, right) = (acc.left.clone(), next.right.clone());
    let result = ConcreteOpd {
        kind: ConcreteOpd::shape_from_faces(&left, &right),
        left,
        right,
        body: Vec::new(),
        lane: acc.lane.or(next.lane),
    };
    SeriesStep {
        pair: (acc.right.clone(), next.left.clone()),
        result,
        legal,
        skipped,
    }
}

/// §5.1 parallel (`+`): the result's external faces plus the pair of faces the
/// internal wiring must join.
pub struct ParallelFold {
    pub result: ConcreteOpd,
    pub pair: (Vec<super::Ep>, Vec<super::Ep>),
}

/// Fold `lhs + rhs` by the §5.1 face-side law.
///
/// The degenerate operand sticks to its **written** side, so it contributes no
/// free face; the non-degenerate operand's other face is what the parallel
/// exposes to its neighbours. This is exactly why the result is **not** simply
/// `opds[0]`: for `VCC + R101` the free right port is `R101.2`, which belongs
/// to `opds[1]` (unified-core §7.2 H3(a), vec-dianlu §5.1).
///
/// The pairing side is selected by the shared `opcheck` predicate, never by a
/// second copy of the rule.
pub fn fold_parallel(lhs: &ConcreteOpd, rhs: &ConcreteOpd) -> ParallelFold {
    let attaches_right = parallel_attaches_right(&lhs.kind, &rhs.kind);
    let (left, right) = if lhs.kind.is_degenerate() && !rhs.kind.is_degenerate() {
        // The degenerate left operand sticks to the left face it was written
        // against; the free face is the right operand's far side.
        (lhs.left.clone(), rhs.right.clone())
    } else {
        // Left-anchored: the result keeps the left operand's own two faces.
        (lhs.left.clone(), lhs.right.clone())
    };
    let result = ConcreteOpd {
        kind: ConcreteOpd::shape_from_faces(&left, &right),
        left,
        right,
        body: Vec::new(),
        lane: lhs.lane.or(rhs.lane),
    };
    let pair = if attaches_right {
        (lhs.right.clone(), rhs.left.clone())
    } else {
        (lhs.left.clone(), rhs.left.clone())
    };
    ParallelFold { result, pair }
}
