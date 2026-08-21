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
//! At the shape layer (§8.1 / §8.3) each operand's left / right port is passed
//! separately as a single-sided column vector `N*1`, so the whole §5 table
//! reduces to a row-count rule on the participating ports:
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
//!   becomes the full-width column (inner left + right merged, both ports
//!   equal) — and feeds that transposed shape to `check_series` /
//!   `check_parallel`. A row mismatch is then an ordinary illegal operation
//!   (E4007 / E4005); there is no pair-by-min / lane-hang recovery.
//!
//! Both Pass1 (`is_connectable` in `mc_phrase.rs`) and Pass2
//! (`try_connect_adjacent` in `mc_mod/stmt.rs`) share this module so the
//! legality rule can never drift between the two passes.

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

/// Series legality (`-` / `->` / `<-`, vec-dianlu.md §5.2): connects
/// `lhs.right x rhs.left`.
///
/// - Either side unknown (`rows == 0`, e.g. an unresolved FuncCall return
///   value) → wildcard pass;
/// - Equal rows → legal (§5.2 rows: node `1*1`, row-vector right `1*1`,
///   column `N*1`, asymmetric-node right `N*1`);
/// - Unequal rows → illegal. In particular `1*1` vs `N*1` (single-point
///   broadcast like `X -> [A, B]`) is **not** a §5 combo and is rejected —
///   a 1-row point only connects to another 1-row point.
pub fn check_series(lhs: Shape, rhs: Shape) -> OpCheck {
    if lhs.is_unknown() || rhs.is_unknown() {
        let shape = if lhs.is_unknown() { rhs } else { lhs };
        return OpCheck::Legal(shape);
    }
    if lhs.rows == rhs.rows {
        return OpCheck::Legal(Shape::new(lhs.rows));
    }
    OpCheck::Illegal(OpIllegal::SeriesRowsMismatch { lhs, rhs })
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

/// Parallel left/right alignment legality (`+`, vec-dianlu.md §5.1).
///
/// - Either side unknown → wildcard pass;
/// - Equal rows → legal (§5.1 rows: node `1*1`, row-vector left `1*1`,
///   column `N*1`, asymmetric-node left `M*1`);
/// - Unequal rows → illegal — `1*1 + N*1` fails left alignment and is not a
///   §5 combo (no broadcast carve-out for parallel). The `align` argument
///   selects which port pair was checked so the diagnostic reason names the
///   failing side (`ParallelLeftMismatch` vs `ParallelRightMismatch`).
pub fn check_parallel(lhs: Shape, rhs: Shape, align: ParallelAlign) -> OpCheck {
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

    // ---- §5.2 series (`-` / `->` / `<-`) ----

    #[test]
    fn series_node_node_ok() {
        // node 1*1 - node 1*1
        assert!(matches!(
            check_series(Shape::node(), Shape::node()),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn series_column_column_ok() {
        // column N*1 - column N*1 (same rows N)
        assert!(matches!(
            check_series(Shape::vvec(4), Shape::vvec(4)),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn series_asym_node_column_ok() {
        // node M*1,N*1 - column N*1: right N == left N
        assert!(matches!(
            check_series(Shape::vvec(3), Shape::vvec(3)),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn series_broadcast_illegal() {
        // `1*1` vs `N*1` (single-point broadcast `X -> [A, B]` / `[A, B] -> GND`)
        // is not a §5 series combo and no broadcast is allowed: a 1-row point
        // must connect to another 1-row point.
        assert_eq!(
            check_series(Shape::node(), Shape::vvec(3)),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(3),
            })
        );
        assert_eq!(
            check_series(Shape::vvec(3), Shape::node()),
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
            check_series(Shape::vvec(2), Shape::vvec(3)),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::vvec(2),
                rhs: Shape::vvec(3),
            })
        );
    }

    #[test]
    fn series_unknown_wildcard() {
        assert!(matches!(
            check_series(Shape::unknown(), Shape::vvec(4)),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series(Shape::vvec(4), Shape::unknown()),
            OpCheck::Legal(_)
        ));
    }

    // ---- §5.1 parallel (`+`) ----

    #[test]
    fn parallel_node_node_ok() {
        // node 1*1 + node 1*1
        assert!(matches!(
            check_parallel(Shape::node(), Shape::node(), ParallelAlign::Left),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn parallel_column_column_ok() {
        // column N*1 + column N*1 (same rows N)
        assert!(matches!(
            check_parallel(Shape::vvec(3), Shape::vvec(3), ParallelAlign::Left),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn parallel_left_mismatch_illegal() {
        // node 1*1 + column 2*1 fails left alignment — not a §5 combo.
        assert_eq!(
            check_parallel(Shape::node(), Shape::vvec(2), ParallelAlign::Left),
            OpCheck::Illegal(OpIllegal::ParallelLeftMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(2),
            })
        );
        assert!(matches!(
            check_parallel(Shape::vvec(2), Shape::vvec(3), ParallelAlign::Left),
            OpCheck::Illegal(_)
        ));
    }

    #[test]
    fn parallel_right_mismatch_illegal() {
        // Row vector 1*2 + row vector 1*2 with different right ports: the
        // left ports align (1*1) but the right ports do not (§5.1: both sides
        // carry an independent right port, so the right ports must also align).
        assert_eq!(
            check_parallel(Shape::node(), Shape::vvec(2), ParallelAlign::Right),
            OpCheck::Illegal(OpIllegal::ParallelRightMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(2),
            })
        );
        // Right ports align when the row counts are equal.
        assert!(matches!(
            check_parallel(Shape::vvec(2), Shape::vvec(2), ParallelAlign::Right),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn parallel_unknown_wildcard() {
        assert!(matches!(
            check_parallel(Shape::unknown(), Shape::vvec(2), ParallelAlign::Left),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_parallel(Shape::unknown(), Shape::vvec(2), ParallelAlign::Right),
            OpCheck::Legal(_)
        ));
    }
}
