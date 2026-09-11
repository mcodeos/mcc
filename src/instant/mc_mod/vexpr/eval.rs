// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `eval_expr` — the shadow fold's node dispatch (unified-core §7.7(1), slices
//! S2 / S3).
//!
//! The fold walks a statement's [`McPhrase`] tree exactly as the design's node
//! table prescribes, one operand kind at a time:
//!
//! | node | behaviour | slice |
//! |---|---|---|
//! | `Series(members, dir)` | `acc = reduce(m0)`; then `step(acc, dir, reduce(m_i))` left-to-right | S1 |
//! | `Parallel(opds)` | fold by the §5.1 face-side law ([`fold_parallel`]) | S2 |
//! | `Group` | **not a fold arm** — a `(,)` group is a *statement list*, expanded before the fold | S3 |
//! | anything else | fall through to `reduce` (the production face accessors) | S1 |
//!
//! # Scope limits deliberately kept for later slices
//!
//! - **`Parallel` wires nothing here.** Production's internal `+` wiring is
//!   `wire_parallel_internal` (private to `stmt.rs`), which tags its
//!   connections `ConnOp::Parallel` and applies its own dimension-degrade
//!   rule. Fabricating an equivalent through `create_connection` would tag
//!   `ConnOp::Series` and produce a guaranteed false diff, so S2 delivers the
//!   **fold** (the true external faces and the pairing pair) only; the wiring
//!   joins at the flip.
//! - **Illegal series emits no diagnostic here.** The shadow is test-only and
//!   never runs in production, so `record_error` would either double-report (if
//!   the shadow ran alongside the engine) or invent a diagnostic stream of its
//!   own. The legality verdict is returned on [`EvalOutcome::legal`]; the
//!   production error code (`CONN_SERIES_SHAPE_MISMATCH`) is applied by the
//!   caller at the flip, reusing the one existing code rather than copying its
//!   message.
//! - **`Transposed` / `Reversed` / `Lead`** fall through to `reduce`, i.e. they
//!   keep reproducing current behaviour; giving them fold arms is S4.

use super::fold::{fold_parallel, fold_series};
use super::{ConcreteOpd, Ep};
use crate::instant::mc_mod::builder::InstantiationBuilder;
use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::ConnDir;
use std::ops::Range;

/// One statement's shadow product: the reduced operand, the connections the
/// fold emitted, and whether every leg was legal.
pub struct EvalOutcome {
    /// The fully reduced operand — the statement's two external faces.
    pub opd: ConcreteOpd,
    /// Index range of the connections this statement appended to
    /// `self.connections`, so a shadow run can diff its own product without
    /// opening a second construction path.
    pub emitted: Range<usize>,
    /// `false` when a `Series` leg was an illegal §5.2 operation (row
    /// mismatch); the fold generated no connection for that leg.
    pub legal: bool,
}

/// A folded operand plus the accumulated legality of the legs that built it.
struct Folded {
    opd: ConcreteOpd,
    legal: bool,
}

impl From<ConcreteOpd> for Folded {
    fn from(opd: ConcreteOpd) -> Self {
        Folded { opd, legal: true }
    }
}

impl InstantiationBuilder {
    /// Shadow-evaluate one connection statement, mirroring `process_stmt`'s
    /// opening steps: a `(,)` group is a statement list expanded into the
    /// standalone statements it stands for *before* the fold (so a group is
    /// never a fold operand — vec-dianlu §7.3, S3), and the remaining statement
    /// is flattened once into `(members, gaps)` so the fold sees the same
    /// normalized operands and edge directions the engine does.
    ///
    /// That normalization is not optional: `get_left_points` returns an empty
    /// face for a raw `Label` / `Component` member, so folding the parser's raw
    /// members would silently produce empty operands.
    pub(super) fn vexpr_eval(&mut self, phrase: &McPhrase) -> Result<Vec<EvalOutcome>, InstError> {
        if let Some(expanded) = phrase.expand_group_statements() {
            let mut out = Vec::new();
            for stmt in &expanded {
                out.extend(self.vexpr_eval(stmt)?);
            }
            return Ok(out);
        }

        let mut phrase = phrase.clone();
        Self::assign_phrase_ids(&mut phrase, &mut self.next_phrase_id);
        let (members, gaps) = self.phrase_to_members_gapped(&phrase);
        if members.is_empty() {
            return Ok(Vec::new());
        }

        let start = self.connections.len();
        let folded = self.vexpr_fold_chain(&members, &gaps)?;
        Ok(vec![EvalOutcome {
            opd: folded.opd,
            emitted: start..self.connections.len(),
            legal: folded.legal,
        }])
    }

    /// §7.2 series: fold the normalized member list left-to-right, `acc`
    /// anchoring right; `gaps[i]` is the direction of the `members[i]` ~
    /// `members[i+1]` boundary.
    fn vexpr_fold_chain(
        &mut self,
        members: &[McPhrase],
        gaps: &[ConnDir],
    ) -> Result<Folded, InstError> {
        debug_assert_eq!(gaps.len(), members.len().saturating_sub(1));
        let mut acc = self.vexpr_fold_member(&members[0])?;
        for i in 0..members.len().saturating_sub(1) {
            let next = self.vexpr_fold_member(&members[i + 1])?;
            acc = self.vexpr_step(&acc, &next, gaps[i])?;
        }
        Ok(acc)
    }

    /// Fold one chain member. A member that is not a `Parallel` is reduced
    /// through the production face accessors; `Group` cannot appear here (it was
    /// expanded at statement level), so there is deliberately no arm for it.
    fn vexpr_fold_member(&mut self, member: &McPhrase) -> Result<Folded, InstError> {
        match member {
            McPhrase::Parallel(opds) => self.vexpr_fold_parallel(opds),
            other => Ok(Folded::from(self.vexpr_reduce(other)?)),
        }
    }

    /// One series leg: legality from the shared `opcheck` rule, then the
    /// internal wiring of the pair the rule selected.
    fn vexpr_step(
        &mut self,
        acc: &Folded,
        next: &Folded,
        dir: ConnDir,
    ) -> Result<Folded, InstError> {
        let step = fold_series(&acc.opd, &next.opd);
        if step.legal {
            self.create_connection(
                points_of(&step.pair.0),
                points_of(&step.pair.1),
                dir,
                step.result.lane,
            )?;
        }
        Ok(Folded {
            opd: step.result,
            legal: acc.legal && next.legal && step.legal,
        })
    }

    /// §5.1 parallel: fold each `+` operand by the face-side law. The internal
    /// wiring is production's `wire_parallel_internal` (see the module docs),
    /// so no connection is emitted here.
    fn vexpr_fold_parallel(&mut self, opds: &[McPhrase]) -> Result<Folded, InstError> {
        let mut acc = self.vexpr_fold_member(&opds[0])?;
        for opd in &opds[1..] {
            let next = self.vexpr_fold_member(opd)?;
            let fold = fold_parallel(&acc.opd, &next.opd);
            acc = Folded {
                opd: fold.result,
                legal: acc.legal && next.legal,
            };
        }
        Ok(acc)
    }
}

/// The concrete points behind a face, in face order.
fn points_of(eps: &[Ep]) -> Vec<NetPoint> {
    eps.iter().map(|e| e.point.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instant::mc_mod::McModuleInst;
    use crate::instant::mc_net::ConnectionInst;
    use crate::semantic::basic::opd_shape::OpdShape;
    use crate::semantic::module::McModule;
    use std::sync::Arc;

    fn builder() -> InstantiationBuilder {
        InstantiationBuilder::new(McModuleInst::new(
            "main",
            Arc::new(McModule::test_stub("main")),
        ))
    }

    fn label(name: &str) -> McPhrase {
        McPhrase::label(name.to_string())
    }

    /// A net phrase in the normalized `Bus` form `get_left_points` /
    /// `get_right_points` actually resolve (a raw `Label` member carries no
    /// side; the engine normalizes it before wiring).
    fn bus(name: &str) -> McPhrase {
        McPhrase::from(crate::semantic::mc_inst::McInstance::Bus(
            crate::semantic::basic::mc_bus::McBus::new(name),
        ))
    }

    fn paths(eps: &[Ep]) -> Vec<String> {
        eps.iter().map(|e| e.point.path.clone()).collect()
    }

    fn point_paths(points: &[NetPoint]) -> Vec<String> {
        points.iter().map(|p| p.path.clone()).collect()
    }

    /// L3 mechanical harness (unified-core §7.7(3)): run **the same statement**
    /// through the production engine and through the shadow on two identical
    /// builders, so the two connection lists can be diffed statement by
    /// statement (two runs of one fixture, never a golden snapshot).
    fn engine_and_shadow(phrase: &McPhrase) -> (Vec<ConnectionInst>, Vec<ConnectionInst>) {
        let mut engine = builder();
        engine
            .process_stmt(phrase)
            .expect("engine must process the statement");
        let mut shadow = builder();
        shadow
            .vexpr_eval(phrase)
            .expect("shadow must fold the statement");
        (engine.connections.clone(), shadow.connections.clone())
    }

    /// One statement's connections, compared field by field (points, dir, op,
    /// lane). Ids are compared too — both builders start from the same counter.
    fn assert_same_connections(expected: &[ConnectionInst], actual: &[ConnectionInst]) {
        assert_eq!(
            expected.len(),
            actual.len(),
            "connection count differs:\n engine={:?}\n shadow={:?}",
            expected
                .iter()
                .map(|c| point_paths(&c.points))
                .collect::<Vec<_>>(),
            actual
                .iter()
                .map(|c| point_paths(&c.points))
                .collect::<Vec<_>>(),
        );
        for (e, a) in expected.iter().zip(actual.iter()) {
            assert_eq!(point_paths(&e.points), point_paths(&a.points));
            assert_eq!(e.dir, a.dir, "direction differs");
            assert_eq!(e.op, a.op, "operator differs");
            assert_eq!(e.lane, a.lane, "lane differs");
        }
    }

    #[test]
    fn eval__two_labels_series_wires_one_connection() {
        let mut inst = builder();
        let phrase = McPhrase::Series(vec![label("A"), label("B")], ConnDir::LtoR);
        let out = inst.vexpr_eval(&phrase).expect("fold must succeed");
        assert_eq!(out.len(), 1, "one statement -> one product");
        assert!(out[0].legal);
        assert_eq!(out[0].emitted.len(), 1, "one series leg -> one connection");
        assert_eq!(inst.connections.len(), 1);
        let conn = &inst.connections[0];
        assert_eq!(conn.points.len(), 2);
    }

    #[test]
    fn eval__series_anchors_right_and_keeps_outer_faces() {
        let mut inst = builder();
        let phrase = McPhrase::Series(vec![label("A"), label("B"), label("C")], ConnDir::LtoR);
        let out = inst.vexpr_eval(&phrase).expect("fold must succeed");
        assert_eq!(paths(&out[0].opd.left), vec!["A"]);
        assert_eq!(paths(&out[0].opd.right), vec!["C"]);
        assert_eq!(out[0].emitted.len(), 2, "two legs -> two connections");
        assert!(out[0].opd.check_i1().is_ok());
    }

    #[test]
    fn eval__parallel_folds_faces_without_wiring() {
        let mut inst = builder();
        let phrase = McPhrase::Parallel(vec![bus("VCC"), bus("GND")]);
        let out = inst.vexpr_eval(&phrase).expect("fold must succeed");
        assert_eq!(out.len(), 1);
        // Both operands are degenerate points, so the fold stays left-anchored.
        assert_eq!(paths(&out[0].opd.left), vec!["VCC"]);
        assert_eq!(paths(&out[0].opd.right), vec!["VCC"]);
        assert!(matches!(out[0].opd.kind, OpdShape::Point(_)));
        assert!(
            inst.connections.is_empty(),
            "parallel wiring is production's wire_parallel_internal, not the shadow's"
        );
    }

    #[test]
    fn eval__group_is_a_statement_list_expanded_before_the_fold() {
        // `(A - B, C - D)` stands for two statements; the fold sees each one
        // separately, which is why no `Group` arm exists (S3).
        let mut inst = builder();
        let group = McPhrase::Group(crate::semantic::basic::mc_group::McGroup {
            opds: vec![
                McPhrase::Series(vec![label("A"), label("B")], ConnDir::LtoR),
                McPhrase::Series(vec![label("C"), label("D")], ConnDir::LtoR),
            ],
            left_match: false,
            right_match: false,
        });
        let out = inst.vexpr_eval(&group).expect("statement-level expansion");
        assert_eq!(out.len(), 2, "one product per expanded statement");
        assert_eq!(inst.connections.len(), 2);
    }

    // ---- L3: shadow vs engine, same statement, same fixture ----

    #[test]
    fn l3__two_label_chain_matches_the_engine() {
        let phrase = McPhrase::Series(vec![label("A"), label("B")], ConnDir::LtoR);
        let (engine, shadow) = engine_and_shadow(&phrase);
        assert_same_connections(&engine, &shadow);
        assert_eq!(engine.len(), 1);
    }

    #[test]
    fn l3__three_member_chain_with_a_directed_tail_matches_the_engine() {
        let phrase = McPhrase::Series(vec![label("A"), label("B"), label("C")], ConnDir::LtoR);
        let (engine, shadow) = engine_and_shadow(&phrase);
        assert_same_connections(&engine, &shadow);
        assert_eq!(engine.len(), 2, "three members -> two wired legs");
    }

    #[test]
    fn l3__group_statement_list_matches_the_engine() {
        let group = McPhrase::Group(crate::semantic::basic::mc_group::McGroup {
            opds: vec![
                McPhrase::Series(vec![label("A"), label("B")], ConnDir::LtoR),
                McPhrase::Series(vec![label("C"), label("D")], ConnDir::LtoR),
            ],
            left_match: false,
            right_match: false,
        });
        let (engine, shadow) = engine_and_shadow(&group);
        assert_same_connections(&engine, &shadow);
        assert_eq!(engine.len(), 2);
    }
}
