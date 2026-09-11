// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Operator folding for the unified connection core — how a statement's operand
//! chain reduces to nets (unified-core §7.3 L4).
//!
//! The pipeline is split so every part has exactly one implementation:
//!
//! | part | owner |
//! |---|---|
//! | statement orchestration — `(,)` expansion, P2-5 expansion, member pre-pass, trunk context | `stmt::process_stmt` / `stmt::process_series_members` |
//! | operand algebra — `reduce`, the `+` face-side fold, `^` / `'`, the `_` lead | this module |
//! | the `->` leg — pair selection, §5.2 legality, connection emission | [`InstantiationBuilder::vexpr_step`] |
//! | lane wiring — a chain holding a `_` lead or a standalone `'` | [`super::lane`] |
//! | connection construction (1:1 / 1:N / N:M, interface expansion) | `group::create_connection` |
//!
//! Every legality decision is **imported from `opcheck`**, never recomputed
//! here: a second copy of a rule is a second drift source (`majority_dir`
//! already demonstrated where "copy the rule once more" leads).
//!
//! # `Parallel` internals
//!
//! A `+` member's internal wiring is `wire_parallel_internal`, reached through
//! the member pre-pass ([`InstantiationBuilder::process_member_internal`]) the
//! same way the engine reaches it. The fold therefore delivers the `+`
//! **fold** — its true external faces and the pairing pair — and never wires
//! the internals itself, which is what keeps a `+` edge tagged
//! `ConnOp::Parallel` instead of `ConnOp::Series`.
//!
//! # `Group` as a chain member
//!
//! The fold deliberately has no `Group` arm: the law for "Group as a chain
//! member" is still an open semantic item (unified-core §7.6 step 0 (3)), so
//! the adjacent path keeps delegating that one shape to `connect_to_group`.

use super::fold::{fold_parallel, fold_series};
use super::{BodyConn, ConcreteOpd, Ep};
use crate::instant::mc_mod::builder::InstantiationBuilder;
use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::basic::opd_shape::OpdShape;
use crate::semantic::common::ConnDir;

/// One lane's product inside a lane chain (unified-core §4.6 C-4 / §7.7 S4b).
/// A lane chain yields one of these per lane instead of a single statement
/// product, because each lane is wired — and lane-tagged — independently.
pub struct LaneOutcome {
    /// The lane's own external faces, carrying `Some(lane)`.
    pub opd: ConcreteOpd,
    /// Index range of the connections this lane appended.
    pub emitted: std::ops::Range<usize>,
}

impl InstantiationBuilder {
    /// Fold one chain member: `Parallel` (S2), the operand transforms `^` / `'`
    /// and the `_` lead (S4), and otherwise the plain reduction through the
    /// production face accessors. `Group` cannot appear here (either it was
    /// expanded at statement level or the adjacent path routes it to
    /// `connect_to_group`), so there is deliberately no arm for it.
    pub(in crate::instant::mc_mod) fn vexpr_fold_member(
        &mut self,
        member: &McPhrase,
    ) -> Result<ConcreteOpd, InstError> {
        match member {
            McPhrase::Parallel(opds) => self.vexpr_fold_parallel(opds),
            McPhrase::Reversed(inner) => self.vexpr_fold_reversed(member, inner),
            McPhrase::Transposed(inner) => self.vexpr_fold_transposed(member, inner),
            McPhrase::Lead => self.vexpr_fold_lead(member),
            other => self.vexpr_reduce(other),
        }
    }

    /// §6.3 `^`: the reversed operand's left face is the inner operand's right
    /// face and vice versa. An operand with no order of its own has nothing to
    /// reverse, so `^` is the identity there (`^^` must not drift the face) —
    /// the same guard `get_left_points` applies, asked through the one shared
    /// predicate rather than re-derived.
    fn vexpr_fold_reversed(
        &mut self,
        member: &McPhrase,
        inner: &McPhrase,
    ) -> Result<ConcreteOpd, InstError> {
        if member.reverse_is_noop() {
            return self.vexpr_reduce(member);
        }
        Ok(self.vexpr_fold_member(inner)?.reversed())
    }

    /// §5.2 / §6.2 strict math transpose (`'`): a column vector becomes a row
    /// vector and vice versa, preserving member order; both faces then expose
    /// the transposed shape's ports. The shape is taken from the inner
    /// operand's **unexpanded** element lists, exactly as `get_left_points`
    /// does, so the transpose sees the same widths the engine sees.
    ///
    /// An unrepresentable transpose never reaches Pass2 (Pass1's
    /// `check_transpose_allowed` rejects it, E2902), and the `FuncCall`
    /// transpose resolves through the unified func-return face resolver — both
    /// fall back to the plain reduction.
    fn vexpr_fold_transposed(
        &mut self,
        member: &McPhrase,
        inner: &McPhrase,
    ) -> Result<ConcreteOpd, InstError> {
        if matches!(inner, McPhrase::FuncCall(_)) {
            return self.vexpr_reduce(member);
        }
        let shape = OpdShape::from_sides(inner.get_left(), inner.get_right());
        let Ok(transposed) = shape.transpose() else {
            return self.vexpr_reduce(member);
        };
        let left = self.vexpr_expand_elems(&transposed.port_left())?;
        let right = self.vexpr_expand_elems(&transposed.port_right())?;
        Ok(ConcreteOpd {
            kind: transposed,
            left,
            right,
            body: Vec::new(),
            lane: None,
        })
    }

    /// `_` is a width slot, not an endpoint: it occupies one row and carries the
    /// **Lead** body — an ideal wire, so two nets meeting under a lead is a
    /// short, not a device shunt (vec-dianlu §5.4). Lane wiring replaces a lead
    /// chain lane by lane, so the fold marks the body and never wires the
    /// placeholder itself.
    fn vexpr_fold_lead(&mut self, member: &McPhrase) -> Result<ConcreteOpd, InstError> {
        let mut opd = self.vexpr_reduce(member)?;
        opd.body = vec![BodyConn::Lead];
        Ok(opd)
    }

    /// Expand a single-sided element list to its concrete points, one row per
    /// leaf member.
    fn vexpr_expand_elems(&mut self, elems: &[McBus]) -> Result<Vec<Ep>, InstError> {
        let mut out = Vec::new();
        for elem in elems {
            for point in self.expand_node_element_to_points(elem)? {
                out.push(Ep::classify(&point));
            }
        }
        Ok(out)
    }

    /// One `->` leg: legality from the shared `opcheck` rule, then the internal
    /// wiring of the pair the rule selected.
    ///
    /// An **empty** face is skipped silently (the engine's explicit empty-port
    /// guard: there is nothing to connect). An **illegal** leg — unequal §5.2
    /// rows — emits `CONN_SERIES_SHAPE_MISMATCH` and generates no connection,
    /// the same verdict and the same code the engine's adjacency leg
    /// (`connect_adjacent_pair`) reported.
    pub(in crate::instant::mc_mod) fn vexpr_step(
        &mut self,
        acc: &ConcreteOpd,
        next: &ConcreteOpd,
        dir: ConnDir,
    ) -> Result<(), InstError> {
        let step = fold_series(acc, next);
        if step.skipped {
            return Ok(());
        }
        if step.legal {
            self.create_connection(
                points_of(&step.pair.0),
                points_of(&step.pair.1),
                dir,
                step.result.lane,
            )?;
        } else {
            self.record_error(
                crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                crate::errcodes::format_msg(crate::errcodes::CONN_SERIES_SHAPE_MISMATCH, &[]),
            );
        }
        Ok(())
    }

    /// §5.1 parallel: fold each `+` operand by the face-side law. The internal
    /// wiring is production's `wire_parallel_internal` (see the module docs),
    /// so no connection is emitted here.
    fn vexpr_fold_parallel(&mut self, opds: &[McPhrase]) -> Result<ConcreteOpd, InstError> {
        let mut acc = self.vexpr_fold_member(&opds[0])?;
        for opd in &opds[1..] {
            let next = self.vexpr_fold_member(opd)?;
            acc = fold_parallel(&acc, &next).result;
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
    use crate::semantic::common::ConnOp;
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

    /// Run one statement through the production wiring (the fold + the lane
    /// path) and return the connections it produced.
    fn wired(phrase: &McPhrase) -> Vec<ConnectionInst> {
        let mut inst = builder();
        inst.process_stmt(phrase).expect("statement must process");
        inst.connections.clone()
    }

    #[test]
    fn adjacent__two_labels_series_wires_one_connection() {
        let phrase = McPhrase::Series(vec![label("A"), label("B")], ConnDir::LtoR);
        let conns = wired(&phrase);
        assert_eq!(conns.len(), 1, "one series leg -> one connection");
        assert_eq!(point_paths(&conns[0].points), vec!["A", "B"]);
        assert_eq!(conns[0].dir, ConnDir::LtoR);
    }

    #[test]
    fn adjacent__three_labels_series_wires_each_written_leg() {
        let phrase = McPhrase::Series(vec![label("A"), label("B"), label("C")], ConnDir::LtoR);
        let conns = wired(&phrase);
        assert_eq!(conns.len(), 2, "two legs -> two connections");
        assert_eq!(point_paths(&conns[0].points), vec!["A", "B"]);
        assert_eq!(point_paths(&conns[1].points), vec!["B", "C"]);
    }

    #[test]
    fn parallel__wires_its_internal_net_via_the_member_pre_pass() {
        let phrase = McPhrase::Parallel(vec![bus("VCC"), bus("GND")]);
        let conns = wired(&phrase);
        // The internal `+` net is production's `wire_parallel_internal`, reached
        // through the member pre-pass — it must be a `Parallel` edge, never a
        // `Series` one (§4.6 C-1).
        assert_eq!(conns.len(), 1, "one `+` net");
        assert_eq!(point_paths(&conns[0].points), vec!["VCC", "GND"]);
        assert_eq!(conns[0].op, Some(ConnOp::Parallel));
        assert_eq!(conns[0].dir, ConnDir::Undirected);
    }

    #[test]
    fn group__is_a_statement_list_expanded_before_the_fold() {
        // `(A - B, C - D)` stands for two statements; the fold sees each one
        // separately, which is why no `Group` arm exists (S3).
        let group = McPhrase::Group(crate::semantic::basic::mc_group::McGroup {
            opds: vec![
                McPhrase::Series(vec![label("A"), label("B")], ConnDir::LtoR),
                McPhrase::Series(vec![label("C"), label("D")], ConnDir::LtoR),
            ],
            left_match: false,
            right_match: false,
        });
        let conns = wired(&group);
        assert_eq!(conns.len(), 2, "one connection per expanded statement");
    }

    // ---- S4: operand transforms and the lead ----

    #[test]
    fn fold__lead_is_a_width_slot_carrying_the_lead_body() {
        let mut inst = builder();
        let opd = inst
            .vexpr_fold_member(&McPhrase::Lead)
            .expect("a lead must reduce");
        assert_eq!(opd.body, vec![BodyConn::Lead]);
        assert_eq!(opd.left.len(), 1, "a lead occupies exactly one row");
        assert_eq!(opd.right.len(), 1);
        // The placeholder name is derived from the phrase's address, so a
        // comparison must treat it as opaque rather than compare it verbatim.
        assert!(
            opd.left[0]
                .point
                .path
                .starts_with(crate::instant::mc_net::LEAD_PLACEHOLDER_PREFIX),
            "lead placeholder must keep its reserved prefix, got {:?}",
            opd.left[0].point.path
        );
        assert!(
            wired(&McPhrase::Lead).is_empty(),
            "a lead is never wired by the fold"
        );
    }

    #[test]
    fn fold__transposed_degenerate_operand_is_an_identity() {
        // A **standalone** top-level `'` member never reaches the fold: the
        // statement dispatch hands it to the lane path first (S4b), mirroring
        // production's `needs_lane_by_lane`. The transpose fold arm therefore
        // serves a *nested* operand (`(A')`), which is why the fixture wraps it
        // in a parallel group instead of standing alone.
        let phrase = McPhrase::Parallel(vec![McPhrase::Transposed(Box::new(bus("A")))]);
        let mut inst = builder();
        let opd = inst
            .vexpr_fold_member(&phrase)
            .expect("the transposed operand must fold");
        assert_eq!(paths(&opd.left), vec!["A"]);
        assert_eq!(paths(&opd.right), vec!["A"]);
        assert!(opd.check_i1().is_ok());
    }

    #[test]
    fn fold__reversed_order_less_operand_is_an_identity() {
        let phrase = McPhrase::Reversed(Box::new(bus("A")));
        let mut inst = builder();
        let opd = inst
            .vexpr_fold_member(&phrase)
            .expect("the reversed operand must fold");
        assert_eq!(paths(&opd.left), vec!["A"]);
        assert_eq!(paths(&opd.right), vec!["A"]);
    }

    // ---- S4b: lane chains ----

    #[test]
    fn lane__chain_tags_every_connection_with_its_lane() {
        // `[A1, _] -> [B1, B2] -> [C1, C2]`: the lead makes this a lane chain,
        // so the connections are emitted per lane and tagged with the lane
        // index. Lane 0 wires `A1 -> B1 -> C1`; lane 1 has the lead
        // pass-through, so it only wires `B2 -> C2`. Both lanes must appear
        // (a two-member chain where the lead leaves lane 1 with a single
        // element wires nothing there by construction).
        let phrase = McPhrase::Series(
            vec![
                McPhrase::Multiple(vec![bus("A1"), McPhrase::Lead]),
                McPhrase::Multiple(vec![bus("B1"), bus("B2")]),
                McPhrase::Multiple(vec![bus("C1"), bus("C2")]),
            ],
            ConnDir::LtoR,
        );
        let conns = wired(&phrase);
        assert!(!conns.is_empty(), "the lane chain must wire something");
        assert!(
            conns.iter().all(|c| c.lane.is_some()),
            "the lane path must tag every connection with its lane"
        );
        let lanes: Vec<u16> = conns.iter().filter_map(|c| c.lane).collect();
        assert!(
            lanes.contains(&0) && lanes.contains(&1),
            "lanes 0 and 1, got {lanes:?}"
        );
    }

    // ---- C-1: the fold's own R0 obligations ----

    /// §4.6 C-1: the fold must not **reorder** the chain's members, must not
    /// **erase** a written gap's direction, and must never fold a `Parallel`
    /// member into a `Series` connection. This cell drives a mixed-direction
    /// chain — the parser's nested form `Series(LtoR)[Series(Undirected)[A, B],
    /// C]` — so both halves are visible at once: the emitted connections stay in
    /// written order and each carries its own gap direction. (The third
    /// prohibition, `Parallel` never becoming a `Series` edge, is pinned by
    /// `parallel__wires_its_internal_net_via_the_member_pre_pass`.)
    #[test]
    fn c1__mixed_direction_chain_keeps_written_order_and_each_edge_direction() {
        let phrase = McPhrase::Series(
            vec![
                McPhrase::Series(vec![label("A"), label("B")], ConnDir::Undirected),
                label("C"),
            ],
            ConnDir::LtoR,
        );
        let conns = wired(&phrase);
        assert_eq!(conns.len(), 2, "two legs -> two wired connections");
        assert_eq!(point_paths(&conns[0].points), vec!["A", "B"]);
        assert_eq!(
            conns[0].dir,
            ConnDir::Undirected,
            "the first gap keeps its written direction"
        );
        assert_eq!(point_paths(&conns[1].points), vec!["B", "C"]);
        assert_eq!(
            conns[1].dir,
            ConnDir::LtoR,
            "the second gap keeps its written direction"
        );
    }

    #[test]
    fn series__mismatched_rows_emit_the_shape_error_and_no_connection() {
        // `[A, B] -> C` is an illegal §5.2 leg (2 rows into 1); the leg reports
        // E4007 and wires nothing.
        let mut inst = builder();
        let phrase = McPhrase::Series(
            vec![McPhrase::Multiple(vec![bus("A"), bus("B")]), bus("C")],
            ConnDir::LtoR,
        );
        inst.process_stmt(&phrase)
            .expect("the statement must process");
        assert!(
            inst.connections.is_empty(),
            "an illegal leg must not wire anything"
        );
        assert!(
            inst.has_errors(),
            "an illegal leg must report CONN_SERIES_SHAPE_MISMATCH"
        );
    }
}
