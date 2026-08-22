// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Unified connection-operation legality check (vec-dianlu.md §5).
//!
//! The vector-circuit design doc defines a **closed legal set** of connection
//! operations in §5.1 (parallel `+`) and §5.2 (series `-` / `->` / `<-`):
//! the eight combos in each sub-table, together with their preconditions.
//! Any combination not listed there is an **illegal operation**: the operator
//! evaluation reports a diagnostic and Pass2 must not generate a connection
//! statement for it.
//!
//! At the shape layer (§8.1 / §8.3) each operand's left / right port is a
//! single-sided column vector `N*1`, so the whole §5 table reduces to a
//! row-count rule on the participating ports. This module takes a full
//! [`OpdShape`] and selects the **contact side** internally (vec-arch.md
//! §5.3), so the side-selection rule never drifts between callers:
//!
//! - **Series** connects `lhs.right x rhs.left` (§5.2). Legal iff the row
//!   counts are equal — this covers all eight listed combos (node `1*1`,
//!   row-vector right `1*1`, column `N*1`, asymmetric-node right `N*1`).
//!   A `1*1`-vs-`N*1` pair (`X -> [A, B]` / `[A, B] -> GND`) is **not** a
//!   §5 series operation and is rejected — there is no broadcast carve-out:
//!   a 1-row point connects only to another 1-row point.
//! - **Parallel** connects `lhs.left x rhs.left` (left alignment, §5.1).
//!   Legal iff the left row counts are equal (all eight combos); there is no
//!   broadcast carve-out — `1*1 + N*1` fails left alignment and is illegal.
//!   When **both** operands carry an independent right port (row vector /
//!   node), the right ports must also align (only one side independent →
//!   its right side merges into the result without alignment).
//! - **Transposed operands** (`'` / `^`, vec-dianlu.md §6.2/§6.3) carry no
//!   carve-out: the caller first transposes the operand — its effective port
//!   becomes the transposed column (strict math transpose, §6.2) — and feeds
//!   that transposed shape to `check_series` / `check_parallel`. A row
//!   mismatch is then an ordinary illegal operation (E4007 / E4005); there is
//!   no pair-by-min / lane-hang recovery.
//!
//! Both Pass1 (`is_connectable` in `mc_phrase.rs`) and Pass2
//! (`try_connect_adjacent` in `mc_mod/stmt.rs`) share this module so the
//! legality rule can never drift between the two passes. Pass1 feeds full
//! `OpdShape` values (side selection here); Pass2 has already expanded its
//! operand to a concrete point list, so it feeds the real row counts through
//! [`check_series_rows`] / [`check_parallel_rows`].

use super::basic::opd_shape::OpdShape;
use super::common::Shape;

/// Outcome of one operator-legality check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCheck {
    /// The combination is listed in §5 and the operation may proceed; carries
    /// the connection shape.
    Legal(Shape),
    /// The combination is not listed in §5 — an illegal operation.
    Illegal(OpIllegal),
}

/// Reason an operation was judged illegal (vec-dianlu.md §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpIllegal {
    /// §5.2 series: `lhs.right` vs `rhs.left` rows mismatch. Includes the
    /// single-point-to-column `1*1` vs `N*1` case — not a §5 combo and no
    /// broadcast is allowed (a 1-row point must match a 1-row point).
    SeriesRowsMismatch { lhs: Shape, rhs: Shape },
    /// §5.1 parallel: `lhs.left` vs `rhs.left` left-alignment mismatch.
    ParallelLeftMismatch { lhs: Shape, rhs: Shape },
    /// §5.1 parallel: both operands carry an independent right port (row
    /// vector / node) but the right ports do not align.
    ParallelRightMismatch { lhs: Shape, rhs: Shape },
}

/// Which side of the parallel operands is being aligned (vec-dianlu.md §5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelAlign {
    /// `lhs.left x rhs.left` — the primary left-alignment rule (all 8 §5.1
    /// combos align the left ports; the result anchors the left operand).
    Left,
    /// `lhs.right x rhs.right` — required only when **both** operands carry an
    /// independent right port (row vector `1*2` / node); if only one side has
    /// an independent right port, that right side merges into the result
    /// without alignment.
    Right,
}

/// Series legality (`-` / `->` / `<-`, vec-dianlu.md §5.2): connects
/// `lhs.right x rhs.left`, selecting the contact sides from the full operand
/// shapes (vec-arch.md §5.3).
///
/// - Either contact side unknown (`rows == 0`, e.g. an unresolved FuncCall
///   return value) → wildcard pass;
/// - Equal rows → legal (§5.2 rows: node `1*1`, row-vector right `1*1`,
///   column `N*1`, asymmetric-node right `N*1`);
/// - Unequal rows → illegal. In particular `1*1` vs `N*1` (single-point
///   broadcast like `X -> [A, B]`) is **not** a §5 combo and is rejected —
///   a 1-row point only connects to another 1-row point.
pub fn check_series(lhs: &OpdShape, rhs: &OpdShape) -> OpCheck {
    check_series_rows(Shape::new(lhs.size_right()), Shape::new(rhs.size_left()))
}

/// Parallel left/right alignment legality (`+`, vec-dianlu.md §5.1), selecting
/// the aligned side from the full operand shapes (vec-arch.md §5.3).
///
/// - Either side unknown → wildcard pass;
/// - Equal rows → legal (§5.1 rows: node `1*1`, row-vector left `1*1`,
///   column `N*1`, asymmetric-node left `M*1`);
/// - Unequal rows → illegal — `1*1 + N*1` fails left alignment and is not a
///   §5 combo (no broadcast carve-out for parallel). The `align` argument
///   selects which port pair was checked so the diagnostic reason names the
///   failing side (`ParallelLeftMismatch` vs `ParallelRightMismatch`).
pub fn check_parallel(lhs: &OpdShape, rhs: &OpdShape, align: ParallelAlign) -> OpCheck {
    let (l, r) = match align {
        ParallelAlign::Left => (lhs.size_left(), rhs.size_left()),
        ParallelAlign::Right => (lhs.size_right(), rhs.size_right()),
    };
    check_parallel_rows(Shape::new(l), Shape::new(r), align)
}

/// Pass2 series entry: the operands have already been expanded to concrete
/// point lists whose lengths are the real (known) row counts — there is no
/// `Deferred`/unknown state left at this stage (vec-arch.md §5.3). The
/// explicit empty guard lives in the caller (`try_connect_adjacent`), so this
/// only receives `len >= 1`.
pub fn check_series_rows(lhs: Shape, rhs: Shape) -> OpCheck {
    if lhs.is_unknown() || rhs.is_unknown() {
        let shape = if lhs.is_unknown() { rhs } else { lhs };
        return OpCheck::Legal(shape);
    }
    if lhs.rows == rhs.rows {
        return OpCheck::Legal(Shape::new(lhs.rows));
    }
    OpCheck::Illegal(OpIllegal::SeriesRowsMismatch { lhs, rhs })
}

/// Pass2 parallel entry (row-count form). Same unknown/equal/mismatch rule as
/// [`check_series_rows`], applied to the `align`-selected side.
pub fn check_parallel_rows(lhs: Shape, rhs: Shape, align: ParallelAlign) -> OpCheck {
    if lhs.is_unknown() || rhs.is_unknown() {
        let shape = if lhs.is_unknown() { rhs } else { lhs };
        return OpCheck::Legal(shape);
    }
    if lhs.rows == rhs.rows {
        return OpCheck::Legal(Shape::new(lhs.rows));
    }
    OpCheck::Illegal(match align {
        ParallelAlign::Left => OpIllegal::ParallelLeftMismatch { lhs, rhs },
        ParallelAlign::Right => OpIllegal::ParallelRightMismatch { lhs, rhs },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::basic::mc_bus::McBus;

    fn bus(name: &str) -> McBus {
        McBus::new(name)
    }

    fn point(name: &str) -> OpdShape {
        OpdShape::Point(bus(name))
    }

    fn row(l: &str, r: &str) -> OpdShape {
        OpdShape::Row(bus(l), bus(r))
    }

    fn column(names: &[&str]) -> OpdShape {
        OpdShape::Column(names.iter().map(|n| bus(n)).collect())
    }

    fn node(l: &[&str], r: &[&str]) -> OpdShape {
        OpdShape::Node(
            l.iter().map(|n| bus(n)).collect(),
            r.iter().map(|n| bus(n)).collect(),
        )
    }

    // ---- §5.2 series (`-` / `->` / `<-`) ----

    #[test]
    fn series_node_node_ok() {
        // node 1*1 - node 1*1
        assert!(matches!(
            check_series(&point("A"), &point("B")),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn series_column_column_ok() {
        // column N*1 - column N*1 (same rows N)
        assert!(matches!(
            check_series(
                &column(&["A", "B", "C", "D"]),
                &column(&["E", "F", "G", "H"])
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn series_asym_node_column_ok() {
        // node M*1,N*1 - column N*1: right N == left N
        assert!(matches!(
            check_series(
                &node(&["A", "B", "C"], &["D", "E", "F"]),
                &column(&["D", "E", "F"])
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn series_broadcast_illegal() {
        // `1*1` vs `N*1` (single-point broadcast `X -> [A, B]` / `[A, B] -> GND`)
        // is not a §5 series combo and no broadcast is allowed: a 1-row point
        // must connect to another 1-row point.
        assert_eq!(
            check_series(&point("X"), &column(&["A", "B", "C"])),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(3),
            })
        );
        assert_eq!(
            check_series(&column(&["A", "B", "C"]), &point("GND")),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::vvec(3),
                rhs: Shape::node(),
            })
        );
    }

    #[test]
    fn series_rows_mismatch_illegal() {
        // column 2*1 - column 3*1: not a §5 combo, no broadcast (both >= 2).
        assert_eq!(
            check_series(&column(&["A", "B"]), &column(&["C", "D", "E"])),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::vvec(2),
                rhs: Shape::vvec(3),
            })
        );
    }

    #[test]
    fn series_unknown_wildcard() {
        assert!(matches!(
            check_series(&OpdShape::Unknown, &column(&["A", "B", "C", "D"])),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series(&column(&["A", "B", "C", "D"]), &OpdShape::Unknown),
            OpCheck::Legal(_)
        ));
    }

    // A `return <expr>` FuncCall is `Node([], right)`: its left contact side is
    // empty, which opcheck treats as a Deferred wildcard (vec-arch.md §5.3).
    #[test]
    fn series_empty_left_contact_wildcard() {
        let ret = node(&[], &["OUT"]);
        assert_eq!(ret.size_left(), 0);
        assert!(matches!(check_series(&point("X"), &ret), OpCheck::Legal(_)));
    }

    // ---- §5.1 parallel (`+`) ----

    #[test]
    fn parallel_node_node_ok() {
        // node 1*1 + node 1*1
        assert!(matches!(
            check_parallel(&point("A"), &point("B"), ParallelAlign::Left),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn parallel_column_column_ok() {
        // column N*1 + column N*1 (same rows N)
        assert!(matches!(
            check_parallel(
                &column(&["A", "B", "C"]),
                &column(&["D", "E", "F"]),
                ParallelAlign::Left
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn parallel_left_mismatch_illegal() {
        // node 1*1 + column 2*1 fails left alignment — not a §5 combo.
        assert_eq!(
            check_parallel(&point("A"), &column(&["B", "C"]), ParallelAlign::Left),
            OpCheck::Illegal(OpIllegal::ParallelLeftMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(2),
            })
        );
        assert!(matches!(
            check_parallel(
                &column(&["A", "B"]),
                &column(&["C", "D", "E"]),
                ParallelAlign::Left
            ),
            OpCheck::Illegal(_)
        ));
    }

    #[test]
    fn parallel_right_mismatch_illegal() {
        // Row vector 1*2 + row vector 1*2 with different right ports: the
        // left ports align (1*1) but the right ports do not (§5.1: both sides
        // carry an independent right port, so the right ports must also align).
        assert_eq!(
            check_parallel(
                &row("A.1", "A.2"),
                &column(&["B", "C"]),
                ParallelAlign::Right
            ),
            OpCheck::Illegal(OpIllegal::ParallelRightMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(2),
            })
        );
        // Right ports align when the row counts are equal.
        assert!(matches!(
            check_parallel(
                &column(&["A", "B"]),
                &column(&["C", "D"]),
                ParallelAlign::Right
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn parallel_unknown_wildcard() {
        assert!(matches!(
            check_parallel(
                &OpdShape::Unknown,
                &column(&["A", "B"]),
                ParallelAlign::Left
            ),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_parallel(
                &OpdShape::Unknown,
                &column(&["A", "B"]),
                ParallelAlign::Right
            ),
            OpCheck::Legal(_)
        ));
    }

    // ---- row-count entry points (Pass2) ----

    #[test]
    fn rows_entry_series() {
        assert!(matches!(
            check_series_rows(Shape::vvec(2), Shape::vvec(2)),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series_rows(Shape::vvec(2), Shape::vvec(3)),
            OpCheck::Illegal(_)
        ));
    }

    #[test]
    fn rows_entry_parallel() {
        assert!(matches!(
            check_parallel_rows(Shape::vvec(2), Shape::vvec(2), ParallelAlign::Left),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_parallel_rows(Shape::node(), Shape::vvec(2), ParallelAlign::Left),
            OpCheck::Illegal(_)
        ));
    }

    // ---- tri-state unknown-shape semantics (vec-arch.md §5.3) ----

    /// The three states of an operand shape:
    ///
    /// - `Known(n)` = `size >= 1` — opcheck compares rows strictly (equal ->
    ///   legal, unequal -> illegal).
    /// - `Deferred` = a `0`-width contact side ([`OpdShape::Unknown`], or the
    ///   empty side of `Node([], right)` / `Node(left, [])`) — Pass1
    ///   wildcard-passes because the symbol layer genuinely does not know the
    ///   width yet (shape-by-use port / unresolved FuncCall return).
    /// - `Error` = an empty expansion / `<error` endpoint — Pass2 skips opcheck
    ///   entirely via the explicit empty guard in `try_connect_adjacent`, so it
    ///   is never represented as a row count fed into opcheck.
    ///
    /// The final asserts lock the structural fact that motivates that guard:
    /// `Shape::vvec(0)` collapses to `rows == 0`, identical to `Shape::unknown`,
    /// so opcheck would wildcard it as `Deferred` if Pass2 ever passed an empty
    /// expansion through as `vvec(0)`. The explicit guard is what keeps the
    /// `Error` state distinct from `Deferred`.
    #[test]
    fn tri_state_semantics() {
        // Known(n): equal rows legal, unequal rows illegal.
        assert!(matches!(
            check_series(&column(&["A", "B"]), &column(&["C", "D"])),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series(&column(&["A", "B"]), &column(&["C", "D", "E"])),
            OpCheck::Illegal(_)
        ));

        // Deferred: Unknown wildcard-passes on either side.
        assert!(matches!(
            check_series(&OpdShape::Unknown, &column(&["A", "B", "C"])),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series(&column(&["A", "B", "C"]), &OpdShape::Unknown),
            OpCheck::Legal(_)
        ));

        // Known(n) is never mistaken for Deferred (size >= 1).
        assert!(!point("A").is_unknown());
        assert_eq!(point("A").size_left(), 1);
        assert_eq!(column(&["A", "B", "C", "D"]).size_left(), 4);

        // The coincidence that motivates the Pass2 explicit empty guard:
        // vvec(0) is structurally identical to unknown, and would be wildcarded
        // as Deferred by opcheck if an empty expansion were fed through as it.
        assert_eq!(Shape::vvec(0), Shape::unknown());
        assert!(Shape::vvec(0).is_unknown());
        assert!(matches!(
            check_series_rows(Shape::vvec(0), Shape::vvec(3)),
            OpCheck::Legal(_)
        ));
    }
}
