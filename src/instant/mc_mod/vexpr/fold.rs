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

use super::{identity, ConcreteOpd, Ep};
use crate::semantic::common::Shape;
use crate::semantic::opcheck::{check_series_rows, OpCheck};

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
    let pair = (acc.right.clone(), next.left.clone());
    let result = ConcreteOpd {
        kind: ConcreteOpd::shape_from_faces(&left, &right),
        left,
        right,
        body: Vec::new(),
        lane: acc.lane.or(next.lane),
    };
    // §7.5 I4, promoted to a lock 2026-09-11: every step re-satisfies I1 and
    // conserves the identity multiset across the pair it merges. This holds for
    // skipped / illegal steps too — a step that wires nothing still must not
    // create or drop an element id, so the check runs unconditionally.
    identity::enforce_i4("fold_series", &[acc, next], &result, &[&pair.0, &pair.1]);
    SeriesStep {
        pair,
        result,
        legal,
        skipped,
    }
}

/// Fold `lhs + rhs` by the §5.1 face-side law — the operand's **external**
/// faces.
///
/// The degenerate operand sticks to its **written** side, so it contributes no
/// free face; the non-degenerate operand's other face is what the parallel
/// exposes to its neighbours. This is exactly why the result is **not** simply
/// `opds[0]`: for `VCC + R101` the free right port is `R101.2`, which belongs
/// to `opds[1]` (unified-core §7.2 H3(a), vec-dianlu §5.1).
///
/// This is only the **external** half of `+`; the internal nets the faces rest
/// on come from [`fold_parallel_chain`], which carries the other contract
/// (r0 design §3.2 D1).
pub fn fold_parallel(lhs: &ConcreteOpd, rhs: &ConcreteOpd) -> ConcreteOpd {
    let (left, right) = if lhs.kind.is_degenerate() && !rhs.kind.is_degenerate() {
        // The degenerate left operand sticks to the left face it was written
        // against; the free face is the right operand's far side.
        (lhs.left.clone(), rhs.right.clone())
    } else {
        // Left-anchored: the result keeps the left operand's own two faces.
        (lhs.left.clone(), lhs.right.clone())
    };
    ConcreteOpd {
        kind: ConcreteOpd::shape_from_faces(&left, &right),
        left,
        right,
        body: Vec::new(),
        lane: lhs.lane.or(rhs.lane),
    }
}

/// §5.1 `+` internals: the nets the operator **short-circuits**, i.e. the
/// internal attachment points that make the operand's external faces.
///
/// This is the other half of `+`. [`fold_parallel`] produces the faces a
/// neighbour sees; those faces only exist because the operator ties every
/// branch's corresponding end into one net, and that net list is what this
/// carries. Keeping both in one module is the point: a face with no net list
/// is an operand whose external port nothing supports.
pub struct ParallelWiring {
    /// One entry per net — the elements shorted together, in written order.
    pub nets: Vec<Vec<Ep>>,
    /// `true` when an operand matched neither the anchor's width nor the
    /// implicit-transpose view, so it was dropped: the caller reports
    /// `CONN_PARALLEL_SHAPE_MISMATCH` (§5.1 left-alignment).
    pub illegal: bool,
}

/// Fold the **internal wiring** of a whole `+` chain (vec-dianlu.md §5.1).
///
/// The chain is anchored on the first operand that exposes a left face
/// (`Lead` placeholders expose none), then every other operand's ends are
/// accumulated into a left net and a right net:
///
/// 1. a **transposed** operand is a shunt written at a face, so it joins the
///    face on its **written** side (`i == 0` → left, otherwise right);
/// 2. a **degenerate** operand (point / column) carries no left/right of its
///    own, so it attaches to the anchor's free face — the face-side law's
///    "attach to the anchor's output side", repeated once per anchor lane when
///    the anchor is a wide chain;
/// 3. a non-degenerate operand whose two faces together span the anchor's
///    width is an **implicit transpose** (`XTAL + R442::RES` written without
///    `'`): its `left ++ right` is a lane view zipped against the anchor;
/// 4. a non-degenerate operand matching the anchor's width on both faces zips
///    to the two nets;
/// 5. anything else is a width mismatch — reported, and the operand is dropped.
///
/// The two nets are then split into one net per lane (when the anchor is wider
/// than one lane and the net divides evenly), dropping `_` placeholders, so a
/// bridge lands on the branch it was written against. The right net is
/// suppressed when the anchor is single-ended (both nets would carry the same
/// nodes).
///
/// `transposed[i]` says whether operand `i` is a written transpose; the operand
/// shapes alone cannot say that (a transposed row and a column look alike).
/// Returns `None` when there is nothing to wire (fewer than two operands, or no
/// operand exposing a face at all).
pub fn fold_parallel_chain(ops: &[ConcreteOpd], transposed: &[bool]) -> Option<ParallelWiring> {
    debug_assert_eq!(ops.len(), transposed.len());
    if ops.len() < 2 {
        return None;
    }

    let same_face = |l: &[Ep], r: &[Ep]| -> bool {
        l.len() == r.len() && l.iter().zip(r).all(|(a, b)| a.point.path == b.point.path)
    };

    // Anchor: the first operand exposing a left face (`Lead` exposes none).
    let anchor_idx = (0..ops.len()).find(|&i| !ops[i].left.is_empty())?;
    let anchor_left = &ops[anchor_idx].left;
    let anchor_right = &ops[anchor_idx].right;
    let anchor_dim = anchor_left.len();
    let anchor_is_chain = !same_face(anchor_left, anchor_right);

    let mut left_net: Vec<Ep> = anchor_left.clone();
    let mut right_net: Vec<Ep> = anchor_right.clone();
    let mut illegal = false;

    for i in 0..ops.len() {
        if i == anchor_idx {
            continue;
        }
        let lp = &ops[i].left;
        let rp = &ops[i].right;
        if lp.is_empty() && rp.is_empty() {
            continue; // Lead or an operand with no face at all
        }

        if transposed[i] {
            // The transposed operand already merges its two faces into `lp`
            // (`rp == lp`), so it is pushed exactly once — onto the net its
            // written side faces.
            let bridge_net = if i == 0 {
                &mut left_net
            } else {
                &mut right_net
            };
            bridge_net.extend(lp.iter().cloned());
        } else if same_face(lp, rp) {
            // Degenerate operand: attach to the anchor's output face. A wide
            // anchor repeats the point once per lane so the lane split below
            // hands it to every lane's right end; a single-ended anchor keeps
            // it on the left (there the two nets are the same nodes anyway).
            if anchor_dim >= 2 && anchor_is_chain {
                for _ in 0..anchor_dim {
                    right_net.extend(lp.iter().cloned());
                }
            } else if anchor_is_chain {
                right_net.extend(lp.iter().cloned());
            } else {
                left_net.extend(lp.iter().cloned());
            }
        } else if anchor_dim >= 2 && lp.len() + rp.len() == anchor_dim {
            // Implicit transpose: `left ++ right` is the operand's N-row view.
            left_net.extend(lp.iter().cloned());
            left_net.extend(rp.iter().cloned());
        } else if lp.len() == anchor_dim && rp.len() == anchor_right.len() {
            left_net.extend(lp.iter().cloned());
            right_net.extend(rp.iter().cloned());
        } else {
            illegal = true; // width mismatch: drop the operand
        }
    }

    let mut nets: Vec<Vec<Ep>> = Vec::new();

    if left_net.len() >= 2 {
        if anchor_dim >= 2 && left_net.len() % anchor_dim == 0 {
            let lanes = left_net.len() / anchor_dim;
            for i in 0..anchor_dim {
                let lane = lane_slice(&left_net, i, anchor_dim, lanes);
                if lane.len() >= 2 {
                    nets.push(lane);
                }
            }
        } else {
            nets.push(left_net.clone());
        }
    }

    if right_net.len() >= 2 && anchor_is_chain && !same_face(&right_net, &left_net) {
        let right_dim = anchor_right.len();
        if right_dim >= 2 && right_net.len() % right_dim == 0 {
            let lanes = right_net.len() / right_dim;
            for i in 0..right_dim {
                let lane = lane_slice(&right_net, i, right_dim, lanes);
                if lane.len() >= 2 {
                    nets.push(lane);
                }
            }
        } else {
            nets.push(right_net.clone());
        }
    }

    // §7.5 I3, promoted to a lock 2026-09-11: the `+` internal nets are built
    // from the operands' own elements — a `Device` body is a shunt between the
    // two nets, never a counting element of its own. (`+` keeps only its
    // external faces, so this is a subset contract, not a conservation one.)
    let op_refs: Vec<&ConcreteOpd> = ops.iter().collect();
    let net_refs: Vec<&[Ep]> = nets.iter().map(|net| net.as_slice()).collect();
    identity::enforce_i3("fold_parallel_chain", &op_refs, &net_refs);

    Some(ParallelWiring { nets, illegal })
}

/// One lane of a net: every `dim`th element starting at `lane`, with `_`
/// placeholders dropped (a placeholder holds its width slot but is not an
/// endpoint, so a lane it occupied collapses below the two-point floor).
fn lane_slice(net: &[Ep], lane: usize, dim: usize, lanes: usize) -> Vec<Ep> {
    (0..lanes)
        .map(|j| net[j * dim + lane].clone())
        .filter(|e| !e.point.is_lead_placeholder())
        .collect()
}
