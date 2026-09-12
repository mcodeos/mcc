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
//! - **Parallel** pairs the sides given by the **face-side law** (vec-dianlu.md
//!   §1.4 / §5.1, [`parallel_attaches_right`]): a degenerate operand — one
//!   whose two ports are the same element list (`1*1` point / `N*1` column) —
//!   has no left/right of its own, so it attaches to the face on its
//!   **written** side. Concretely the left faces pair, except that a
//!   degenerate **right** operand pairs the left operand's right face. Legal
//!   iff the paired row counts are equal; there is no broadcast carve-out —
//!   `1*1 + N*1` fails the pairing and is illegal. When **both** operands are
//!   non-degenerate they carry two genuinely distinct faces, so the right
//!   faces must pair as well (two-face joint).
//! - **Transposed operands** (`'` / `^`, vec-dianlu.md §6.2/§6.3) carry no
//!   carve-out: the caller first transposes the operand — its effective port
//!   becomes the transposed column (strict math transpose, §6.2) — and feeds
//!   that transposed shape to `check_series` / `check_parallel`. A row
//!   mismatch is then an ordinary illegal operation (E4007 / E4005); there is
//!   no pair-by-min / lane-hang recovery.
//!
//! Both Pass1 and Pass2 share this module so the legality rule can never drift
//! between the two passes. Pass1 feeds full `OpdShape` values through
//! [`check_series`] / [`check_parallel`], which select the contact sides (and
//! so are the only place that rule lives — the parser carries no side
//! selection of its own); Pass2 has already expanded its operand to a concrete
//! point list, so it feeds the real row counts through [`check_series_rows`] /
//! [`check_parallel_rows`].

use super::basic::mc_bus::McBus;
use super::basic::opd_shape::OpdShape;
use super::common::{representative, ConnDir, ConnOp, Shape};

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
    /// §5.1 parallel: the paired ports (see [`parallel_pair_sides`]) carry
    /// different row counts. Includes the `1*1 + N*1` case -- parallel has no
    /// broadcast carve-out.
    ParallelPairedMismatch { lhs: Shape, rhs: Shape },
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
pub fn check_series(dir: ConnDir, lhs: &OpdShape, rhs: &OpdShape) -> OpCheck {
    let (a, b) = (
        port_row_shape(&lhs.port_right()),
        port_row_shape(&rhs.port_left()),
    );
    log_single_port(ConnOp::Series, dir, a, b);
    check_series_rows(a, b)
}

/// Row count of a port element list as a [`Shape`] — the single row-count
/// rule for the phrase layer (Pass1 / eval.md §1).
///
/// - Empty list → [`Shape::unknown`] (unresolved, e.g. a FuncCall return value);
/// - `<error` placeholder marker → also unknown: a placeholder is not a shape;
/// - Otherwise each `McBus` element contributes one row **per member**, so a
///   bus carrying members (`RS485{A,B}`) counts as N rows. This is the leaf
///   count, matching [`OpdShape::size_left`] / [`OpdShape::size_right`] and the
///   Pass2 lane expansion.
///
/// The column count is always 1 at this stage: a 2-pin device's
/// `get_left`/`get_right` only exposes a single point, so the `1*2` row-vector
/// shape is invisible at the phrase layer and only fully expanded in Pass2.
pub fn port_row_shape(elems: &[McBus]) -> Shape {
    if elems.is_empty() || elems.iter().any(|e| e.name.contains("<error")) {
        return Shape::unknown();
    }
    Shape::new(elems.iter().map(|e| e.size()).sum::<usize>().max(1))
}

/// §4 single-port (1*1) representative note (eval.md §4): a single-port
/// connection has no left/right distinction, so one representative is chosen —
/// `-` / `->` take the **second** written operand, `<-` the **first**, `+` the
/// first. That is `representative`'s own rule (`semantic/common.rs`), decided
/// from the operator and its direction over written-order operands; Pass2
/// reaches the same label under the written order used everywhere, so no
/// operand reordering is involved (`<-` names its net after op1 — the chain
/// head — not after a swapped tail).
///
/// A 1-row x 1-row pair is legal on the row-count rule anyway; this only makes
/// the chosen representative observable.
fn log_single_port(op: ConnOp, dir: ConnDir, lhs: Shape, rhs: Shape) {
    if lhs.rows == 1 && rhs.rows == 1 {
        mcc_dbg!(
            "sem::conds",
            "[vec] single-port representative: dir={dir:?} lhs={lhs} rhs={rhs} rep={rep}",
            rep = representative(op, dir, lhs, rhs)
        );
    }
}

/// Does a degenerate **right** operand attach to the left operand's **right**
/// face? (vec-dianlu.md §1.4 / §5.1 face-side law.)
///
/// Parallel consumes no port, so a degenerate operand -- one with no left/right
/// of its own (`1*1` point / `N*1` column) -- must pick a face, and only its
/// **written** side can decide which. A chain-accumulated value has its new
/// operands written on the right, so those attach to the face they were written
/// against: the left operand's right face.
///
/// This is false in every other combination:
/// - right operand non-degenerate → it has a left face of its own, so the left
///   faces pair;
/// - left operand also degenerate → nothing was written against a distinct
///   face, and the result stays left-anchored.
pub fn parallel_attaches_right(lhs: &OpdShape, rhs: &OpdShape) -> bool {
    rhs.is_degenerate() && !lhs.is_degenerate()
}

/// The two ports `+` pairs, as element lists, by [`parallel_attaches_right`].
///
/// The pairing side is fully derivable from `(lhs, rhs)` -- there is no
/// separate alignment mode to pass in.
pub fn parallel_pair_sides(lhs: &OpdShape, rhs: &OpdShape) -> (Vec<McBus>, Vec<McBus>) {
    if parallel_attaches_right(lhs, rhs) {
        // The degenerate right operand's two ports are the same list, so
        // either accessor is the port being attached.
        (lhs.port_right(), rhs.port_left())
    } else {
        (lhs.port_left(), rhs.port_left())
    }
}

/// Parallel pairing legality (`+`, vec-dianlu.md §5.1), selecting the paired
/// sides from the full operand shapes (vec-arch.md §5.3).
///
/// - Either side unknown → wildcard pass;
/// - Equal rows → legal (§5.1 rows: node `1*1`, row-vector left `1*1`,
///   column `N*1`, asymmetric-node left `M*1`);
/// - Unequal rows → illegal — `1*1 + N*1` fails the paired alignment and is
///   not a §5 combo (no broadcast carve-out for parallel);
/// - When **both** operands are non-degenerate they carry two genuinely
///   distinct faces, so the right ports must pair as well (two-face joint);
///   a degenerate operand's right face is the same as its left and was already
///   consumed by the first pairing.
pub fn check_parallel(dir: ConnDir, lhs: &OpdShape, rhs: &OpdShape) -> OpCheck {
    let (side_l, side_r) = parallel_pair_sides(lhs, rhs);
    let (a, b) = (port_row_shape(&side_l), port_row_shape(&side_r));
    log_single_port(ConnOp::Parallel, dir, a, b);
    let paired = check_parallel_rows(a, b);
    if !matches!(paired, OpCheck::Legal(_)) || lhs.is_degenerate() || rhs.is_degenerate() {
        return paired;
    }
    check_parallel_rows(
        port_row_shape(&lhs.port_right()),
        port_row_shape(&rhs.port_right()),
    )
}

/// Pass2 series entry: the operands have already been expanded to concrete
/// point lists whose lengths are the real (known) row counts — there is no
/// `Deferred`/unknown state left at this stage (vec-arch.md §5.3). The
/// explicit empty guard lives in the caller (`connect_adjacent_pair`), so this
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
/// [`check_series_rows`], applied to one paired port pair.
pub fn check_parallel_rows(lhs: Shape, rhs: Shape) -> OpCheck {
    if lhs.is_unknown() || rhs.is_unknown() {
        let shape = if lhs.is_unknown() { rhs } else { lhs };
        return OpCheck::Legal(shape);
    }
    if lhs.rows == rhs.rows {
        return OpCheck::Legal(Shape::new(lhs.rows));
    }
    OpCheck::Illegal(OpIllegal::ParallelPairedMismatch { lhs, rhs })
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

    // §5.2 series (`-` / `->` / `<-`)

    #[test]
    fn sem_opcheck__series_node_node_ok() {
        // node 1*1 - node 1*1
        assert!(matches!(
            check_series(ConnDir::Undirected, &point("A"), &point("B")),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn sem_opcheck__series_column_column_ok() {
        // column N*1 - column N*1 (same rows N)
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &column(&["A", "B", "C", "D"]),
                &column(&["E", "F", "G", "H"])
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn sem_opcheck__series_asym_node_column_ok() {
        // node M*1,N*1 - column N*1: right N == left N
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &node(&["A", "B", "C"], &["D", "E", "F"]),
                &column(&["D", "E", "F"])
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn sem_opcheck__series_broadcast_illegal() {
        // `1*1` vs `N*1` (single-point broadcast `X -> [A, B]` / `[A, B] -> GND`)
        // is not a §5 series combo and no broadcast is allowed: a 1-row point
        // must connect to another 1-row point.
        assert_eq!(
            check_series(ConnDir::Undirected, &point("X"), &column(&["A", "B", "C"])),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(3),
            })
        );
        assert_eq!(
            check_series(
                ConnDir::Undirected,
                &column(&["A", "B", "C"]),
                &point("GND")
            ),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::vvec(3),
                rhs: Shape::node(),
            })
        );
    }

    #[test]
    fn sem_opcheck__series_rows_mismatch_illegal() {
        // column 2*1 - column 3*1: not a §5 combo, no broadcast (both >= 2).
        assert_eq!(
            check_series(
                ConnDir::Undirected,
                &column(&["A", "B"]),
                &column(&["C", "D", "E"])
            ),
            OpCheck::Illegal(OpIllegal::SeriesRowsMismatch {
                lhs: Shape::vvec(2),
                rhs: Shape::vvec(3),
            })
        );
    }

    #[test]
    fn sem_opcheck__series_unknown_wildcard() {
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &OpdShape::Unknown,
                &column(&["A", "B", "C", "D"])
            ),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &column(&["A", "B", "C", "D"]),
                &OpdShape::Unknown
            ),
            OpCheck::Legal(_)
        ));
    }

    // A `return <expr>` FuncCall is `Node([], right)`: its left contact side is
    // empty, which opcheck treats as a Deferred wildcard (vec-arch.md §5.3).
    #[test]
    fn sem_opcheck__series_empty_left_contact_wildcard() {
        let ret = node(&[], &["OUT"]);
        assert_eq!(ret.size_left(), 0);
        assert!(matches!(
            check_series(ConnDir::Undirected, &point("X"), &ret),
            OpCheck::Legal(_)
        ));
    }

    // §5.1 parallel (`+`)

    // The four degenerate/non-degenerate combinations, one test each, each
    // carrying at least two members so no branch is bypassed vacuously.

    /// The pairing-side table (design doc §2.1), observed through the verdict.
    /// An `Illegal` names the paired row counts, so the pair actually chosen is
    /// pinned too -- a verdict `Legal` alone would not distinguish "paired the
    /// right sides" from "paired the left sides".
    #[test]
    fn sem_opcheck__parallel_pairing_table_by_face_law() {
        let und = ConnDir::Undirected;

        // (degenerate, degenerate) -> left x left, both anchored left.
        assert!(matches!(
            check_parallel(und, &point("A"), &point("B")),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_parallel(und, &column(&["A", "B", "C"]), &column(&["D", "E", "F"])),
            OpCheck::Legal(_)
        ));
        // Mismatch still names the left faces.
        assert_eq!(
            check_parallel(und, &column(&["A", "B"]), &column(&["C", "D", "E"])),
            OpCheck::Illegal(OpIllegal::ParallelPairedMismatch {
                lhs: Shape::vvec(2),
                rhs: Shape::vvec(3),
            })
        );

        // (degenerate lhs, non-degenerate rhs) -> left x left: the lhs has no
        // left/right of its own, and it was written on the left.
        assert!(matches!(
            check_parallel(und, &point("A"), &node(&["B"], &["C", "D"])),
            OpCheck::Legal(_)
        ));
        assert_eq!(
            check_parallel(und, &column(&["A", "B"]), &node(&["C"], &["D", "E"])),
            OpCheck::Illegal(OpIllegal::ParallelPairedMismatch {
                lhs: Shape::vvec(2),
                rhs: Shape::node(),
            })
        );

        // (non-degenerate lhs, degenerate rhs) -> the degenerate rhs attaches
        // to the lhs RIGHT face, the side it was written against: the pair is
        // (lhs.right, rhs), NOT (lhs.left, rhs).
        assert_eq!(
            check_parallel(und, &row("A.1", "A.2"), &column(&["B", "C"])),
            OpCheck::Illegal(OpIllegal::ParallelPairedMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(2),
            })
        );
        assert!(matches!(
            check_parallel(und, &node(&["A.1"], &["A.2", "A.3"]), &column(&["B", "C"])),
            OpCheck::Legal(_)
        ));

        // (non-degenerate, non-degenerate) -> left x left, plus the right faces
        // (covered by `sem_opcheck__parallel_both_faces_must_pair`).
        assert!(matches!(
            check_parallel(und, &row("A.1", "A.2"), &row("B.1", "B.2")),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn sem_opcheck__parallel_pair_sides_returns_element_lists() {
        // The degenerate right operand's list is the same on both sides.
        let (l, r) = parallel_pair_sides(&row("A.1", "A.2"), &column(&["B", "C"]));
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].name, "A.2");
        assert_eq!(r.len(), 2);

        let (l, r) = parallel_pair_sides(&column(&["A", "B"]), &column(&["C", "D"]));
        assert_eq!(l.len(), 2);
        assert_eq!(r.len(), 2);
        assert_eq!(l[0].name, "A");
        assert_eq!(r[1].name, "D");
    }

    #[test]
    fn sem_opcheck__parallel_attaches_right_only_for_written_side() {
        // Degenerate right operand, non-degenerate left: attaches right.
        assert!(parallel_attaches_right(
            &row("A.1", "A.2"),
            &column(&["B", "C"])
        ));
        assert!(parallel_attaches_right(
            &node(&["A.1"], &["A.2", "A.3"]),
            &point("B")
        ));
        // Every other combination keeps the left faces.
        assert!(!parallel_attaches_right(&point("A"), &column(&["B", "C"])));
        assert!(!parallel_attaches_right(
            &column(&["A", "B"]),
            &column(&["C", "D"])
        ));
        assert!(!parallel_attaches_right(&point("A"), &node(&["B"], &["C"])));
        assert!(!parallel_attaches_right(
            &row("A.1", "A.2"),
            &row("B.1", "B.2")
        ));
    }

    #[test]
    fn sem_opcheck__parallel_point_point_ok() {
        // node 1*1 + node 1*1 -- both degenerate, left-anchored.
        assert!(matches!(
            check_parallel(ConnDir::Undirected, &point("A"), &point("B")),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn sem_opcheck__parallel_column_column_ok() {
        // column N*1 + column N*1 (same rows N) -- both degenerate.
        assert!(matches!(
            check_parallel(
                ConnDir::Undirected,
                &column(&["A", "B", "C"]),
                &column(&["D", "E", "F"])
            ),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_parallel(
                ConnDir::Undirected,
                &column(&["A", "B"]),
                &column(&["C", "D"])
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn sem_opcheck__parallel_paired_mismatch_illegal() {
        // node 1*1 + column 2*1 fails the paired alignment -- not a §5 combo.
        assert_eq!(
            check_parallel(ConnDir::Undirected, &point("A"), &column(&["B", "C"])),
            OpCheck::Illegal(OpIllegal::ParallelPairedMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(2),
            })
        );
        // column N*1 + column M*1, N != M.
        assert!(matches!(
            check_parallel(
                ConnDir::Undirected,
                &column(&["A", "B"]),
                &column(&["C", "D", "E"])
            ),
            OpCheck::Illegal(_)
        ));
    }

    /// The two mis-judgments of the old unconditional-left-alignment rule
    /// (design doc §2.2): a degenerate **right** operand must pair the left
    /// operand's right face, not its left one.
    #[test]
    fn sem_opcheck__parallel_degenerate_rhs_uses_written_side() {
        // node N*1,M*1 + column 2*1 with N=1, M=2: the written side pairs
        // 2 x 2 -> LEGAL. The old rule compared left faces (1 x 2) and
        // rejected a legal form.
        assert!(matches!(
            check_parallel(
                ConnDir::Undirected,
                &node(&["A.1"], &["A.2", "A.3"]),
                &column(&["B", "C"])
            ),
            OpCheck::Legal(_)
        ));
        // node N*1,M*1 + column 2*1 with N=2, M=1: the written side pairs
        // 1 x 2 -> ILLEGAL. The old rule compared left faces (2 x 2) and
        // accepted an illegal form.
        assert_eq!(
            check_parallel(
                ConnDir::Undirected,
                &node(&["A.1", "A.2"], &["A.3"]),
                &column(&["B", "C"])
            ),
            OpCheck::Illegal(OpIllegal::ParallelPairedMismatch {
                lhs: Shape::node(),
                rhs: Shape::vvec(2),
            })
        );
    }

    #[test]
    fn sem_opcheck__parallel_both_faces_must_pair() {
        // Both operands non-degenerate: the left faces pair (1 x 1) but the
        // right faces do not (2 x 1) -> illegal on the second face.
        assert_eq!(
            check_parallel(
                ConnDir::Undirected,
                &node(&["A.1"], &["A.2", "A.3"]),
                &row("B.1", "B.2")
            ),
            OpCheck::Illegal(OpIllegal::ParallelPairedMismatch {
                lhs: Shape::vvec(2),
                rhs: Shape::node(),
            })
        );
        // Equal right rows -> legal on both faces.
        assert!(matches!(
            check_parallel(
                ConnDir::Undirected,
                &node(&["A.1"], &["A.2", "A.3"]),
                &node(&["B.1"], &["B.2", "B.3"])
            ),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_parallel(
                ConnDir::Undirected,
                &node(&["A.1"], &["A.2"]),
                &row("B.1", "B.2")
            ),
            OpCheck::Legal(_)
        ));
    }

    #[test]
    fn sem_opcheck__parallel_unknown_wildcard() {
        for rhs in [column(&["A", "B"]), node(&["A"], &["B", "C"])] {
            assert!(matches!(
                check_parallel(ConnDir::Undirected, &OpdShape::Unknown, &rhs),
                OpCheck::Legal(_)
            ));
            assert!(matches!(
                check_parallel(ConnDir::Undirected, &rhs, &OpdShape::Unknown),
                OpCheck::Legal(_)
            ));
        }
    }

    // row-count entry points (Pass2)

    #[test]
    fn sem_opcheck__rows_entry_series() {
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
    fn sem_opcheck__rows_entry_parallel() {
        assert!(matches!(
            check_parallel_rows(Shape::vvec(2), Shape::vvec(2)),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_parallel_rows(Shape::node(), Shape::vvec(2)),
            OpCheck::Illegal(_)
        ));
    }

    // tri-state unknown-shape semantics (vec-arch.md §5.3)

    /// The three states of an operand shape:
    ///
    /// - `Known(n)` = `size >= 1` — opcheck compares rows strictly (equal ->
    ///   legal, unequal -> illegal).
    /// - `Deferred` = a `0`-width contact side ([`OpdShape::Unknown`], or the
    ///   empty side of `Node([], right)` / `Node(left, [])`) — Pass1
    ///   wildcard-passes because the symbol layer genuinely does not know the
    ///   width yet (shape-by-use port / unresolved FuncCall return).
    /// - `Error` = an empty expansion / `<error` endpoint — Pass2 skips opcheck
    ///   entirely via the explicit empty guard in `connect_adjacent_pair`, so it
    ///   is never represented as a row count fed into opcheck.
    ///
    /// The final asserts lock the structural fact that motivates that guard:
    /// `Shape::vvec(0)` collapses to `rows == 0`, identical to `Shape::unknown`,
    /// so opcheck would wildcard it as `Deferred` if Pass2 ever passed an empty
    /// expansion through as `vvec(0)`. The explicit guard is what keeps the
    /// `Error` state distinct from `Deferred`.
    #[test]
    fn sem_opcheck__tri_state_semantics() {
        // Known(n): equal rows legal, unequal rows illegal.
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &column(&["A", "B"]),
                &column(&["C", "D"])
            ),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &column(&["A", "B"]),
                &column(&["C", "D", "E"])
            ),
            OpCheck::Illegal(_)
        ));

        // Deferred: Unknown wildcard-passes on either side.
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &OpdShape::Unknown,
                &column(&["A", "B", "C"])
            ),
            OpCheck::Legal(_)
        ));
        assert!(matches!(
            check_series(
                ConnDir::Undirected,
                &column(&["A", "B", "C"]),
                &OpdShape::Unknown
            ),
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
