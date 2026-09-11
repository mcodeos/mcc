// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The unified connection core — operand algebra for vector expressions
//! (`vector-conn-unified-core-design.md` §7.3 / §7.6 step 4).
//!
//! This module is the **production** home of the `->` leg: the statement
//! orchestrator ([`super::stmt::process_series_members`]) folds each adjacent
//! pair here instead of carrying its own copy of the rule. The fold re-derives
//! the operand / port algebra on top of the existing building blocks
//! (`get_left_points` / `get_right_points`, `OpdShape`, `opcheck`), so the
//! faces and the §5.2 legality have exactly one implementation.
//!
//! It lives under `mc_mod` because `reduce` wraps the builder's
//! `get_left_points` / `get_right_points`, which are `pub(super)` on
//! [`InstantiationBuilder`] — a sibling top-level module could not reach them.
//!
//! The slices below follow unified-core §7.7(2) ("do not build it all before
//! comparing — advance one operand kind at a time"):
//!
//! - **S1** `ConcreteOpd` + `reduce` + the I1 identity assertion (this file).
//!   Coverage: plain `Series` of simple components; no `_` / `Transposed` /
//!   `Reversed` / `Parallel` / `Group`.
//! - **S2** `Parallel` folding by the §5.1 face-side law ([`fold::fold_parallel`]).
//! - **S3** `Group` is a *statement-level* construct — it is expanded before
//!   the fold, and a `Group` used as a chain member is still delegated to the
//!   engine's `connect_to_group` (the law is an open item), so there is no arm
//!   for it in either place (vec-dianlu §7.3).
//! - **S4** `_` Lead + `Transposed` + `Reversed` + lane production.
//!
//! Everything here mirrors the §7.7 blueprint's plain-data shapes verbatim;
//! the legality rule is **imported from `opcheck`**, never re-written (a second
//! copy of a rule is a second drift source — the design's hard requirement).
//!
//! Some of the pure-algebra pieces (the I1 assertion, the full `BodyConn`
//! classification, `fold_parallel`'s pair) are exercised by the unit tests and
//! the design's shape checks but are not all consumed by the production legs
//! yet, so the module keeps a local `dead_code` allowance.
#![allow(dead_code)]

pub mod eval;
pub mod fold;
pub mod lane;

use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::basic::opd_shape::OpdShape;
use crate::semantic::common::IOType;

// ============================================================================
// Element identity (§7.5 I2) and body (§7.2 H3(b) / vec-dianlu §5.4)
// ============================================================================

/// §7.5 I2: the identity of one endpoint element. It is pinned to the [`Ep`]
/// and carried through the whole fold, so a port keeps its identity no matter
/// how many operators wrap it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EpKind {
    /// Component pin (`R101.1`): `owner` = instance name, `member` = pin id.
    Pin { owner: String, member: String },
    /// Module port (`sub1.clk`): the full `owner.port` path.
    Port(String),
    /// Bare net / rail / label (no owner).
    Net(String),
}

/// §7.7: one concrete endpoint = its identity plus the point it resolves to.
#[derive(Debug, Clone)]
pub struct Ep {
    pub kind: EpKind,
    pub point: NetPoint,
}

/// §5.4 body of a through-element: what an operand traverses between its two
/// ends when they land on **different** nets. Filled by the caller that knows
/// the source phrase (S4 refines the classification alongside L0 ②); the pure
/// port algebra never needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyConn {
    /// Component body: crossing nets is an intentional shunt.
    Device,
    /// `_` ideal wire: crossing nets is a short circuit (warning, not error).
    Lead,
    /// Rail / label: no body at all — parallel labelling is legal, series is not.
    Net,
}

/// §7.7 `ConcreteOpd`: a fully reduced operand — its two external faces, its
/// shape, its body and its lane. `reduce` produces one of these per operand;
/// the fold consumes and produces them.
#[derive(Debug, Clone)]
pub struct ConcreteOpd {
    pub left: Vec<Ep>,
    pub right: Vec<Ep>,
    pub kind: OpdShape,
    pub body: Vec<BodyConn>,
    pub lane: Option<u16>,
}

impl Ep {
    /// Classify a resolved [`NetPoint`] into its element identity.
    ///
    /// - no owner           -> [`EpKind::Net`]  (label / rail / bare net)
    /// - owner, no `IOType` -> [`EpKind::Pin`]  (component pin: `R101.1`)
    /// - owner, an `IOType` -> [`EpKind::Port`] (module port: `sub1.clk`)
    pub fn classify(point: &NetPoint) -> Ep {
        let kind = match &point.owner {
            None => EpKind::Net(point.path.clone()),
            Some(owner) => {
                if matches!(point.iotype, IOType::None) {
                    EpKind::Pin {
                        owner: owner.clone(),
                        member: point
                            .member_name
                            .clone()
                            .unwrap_or_else(|| member_of(&point.path, owner)),
                    }
                } else {
                    EpKind::Port(point.path.clone())
                }
            }
        };
        Ep {
            kind,
            point: point.clone(),
        }
    }
}

impl ConcreteOpd {
    /// Build an operand from its two already-reduced faces.
    pub fn from_sides(left: Vec<NetPoint>, right: Vec<NetPoint>) -> Self {
        let kind = OpdShape::from_sides(buses(&left), buses(&right));
        ConcreteOpd {
            left: left.iter().map(Ep::classify).collect(),
            right: right.iter().map(Ep::classify).collect(),
            kind,
            body: Vec::new(),
            lane: None,
        }
    }

    /// §7.5 I1: the shape's row counts, the two face lengths and the concrete
    /// point counts are one and the same number. `Unknown` is the wildcard and
    /// is exempt (its side lists are empty by construction).
    pub fn check_i1(&self) -> Result<(), String> {
        if self.kind.is_unknown() {
            return Ok(());
        }
        let (sl, sr) = (self.kind.size_left(), self.kind.size_right());
        if sl != self.left.len() || sr != self.right.len() {
            return Err(format!(
                "I1 violated: shape rows ({sl}, {sr}) != face lengths ({}, {})",
                self.left.len(),
                self.right.len()
            ));
        }
        for ep in self.left.iter().chain(self.right.iter()) {
            if ep.point.path.is_empty() {
                return Err("I1 violated: an element carries an empty point path".to_string());
            }
        }
        Ok(())
    }

    /// The two faces as [`McBus`] elements, for shape re-derivation.
    pub(super) fn buses_of(eps: &[Ep]) -> Vec<McBus> {
        eps.iter().map(|e| bus_of(&e.point)).collect()
    }

    /// §6.3 directional reverse (`^`): swap the two faces where they are
    /// independent. The shape is reversed through [`OpdShape::reverse`], never
    /// re-derived from the swapped faces — re-deriving would lose the
    /// multi-member element encoding (`Bus("UART0", ["TX"])`) that the expanded
    /// point list cannot reproduce.
    pub fn reversed(&self) -> ConcreteOpd {
        ConcreteOpd {
            left: self.right.clone(),
            right: self.left.clone(),
            kind: self.kind.reverse(),
            body: self.body.clone(),
            lane: self.lane,
        }
    }

    /// Re-derive the [`OpdShape`] from this operand's two faces.
    pub(super) fn shape_from_faces(left: &[Ep], right: &[Ep]) -> OpdShape {
        OpdShape::from_sides(Self::buses_of(left), Self::buses_of(right))
    }
}

// ============================================================================
// reduce (builder side)
// ============================================================================

use super::builder::InstantiationBuilder;

impl InstantiationBuilder {
    /// §7.7 `reduce`: wrap the two existing face accessors into a
    /// [`ConcreteOpd`]. The face accessors are `pub(super)` on this builder,
    /// which is why the fold lives inside `mc_mod`.
    pub(super) fn vexpr_reduce(&mut self, phrase: &McPhrase) -> Result<ConcreteOpd, InstError> {
        let left = self.get_left_points(phrase)?;
        let right = self.get_right_points(phrase)?;
        Ok(ConcreteOpd::from_sides(left, right))
    }
}

// ============================================================================
// helpers
// ============================================================================

/// The member part of `owner.member` (fallback when a point carries no
/// `member_name`).
fn member_of(path: &str, owner: &str) -> String {
    path.strip_prefix(owner)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(path)
        .to_string()
}

fn bus_of(point: &NetPoint) -> McBus {
    let mut bus = McBus::new(&point.path);
    if let Some(member) = &point.member_name {
        bus.member = vec![member.clone()];
        bus.full_members = vec![member.clone()];
    }
    bus
}

fn buses(points: &[NetPoint]) -> Vec<McBus> {
    points.iter().map(bus_of).collect()
}

// ============================================================================
// tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::fold::{fold_parallel, fold_series};
    use super::*;

    fn label(name: &str) -> NetPoint {
        NetPoint::new(name, IOType::None)
    }

    fn pin(path: &str, owner: &str, member: &str) -> NetPoint {
        NetPoint::with_owner(path, owner, IOType::None).with_member_name(member)
    }

    fn opd(left: Vec<NetPoint>, right: Vec<NetPoint>) -> ConcreteOpd {
        ConcreteOpd::from_sides(left, right)
    }

    fn paths(eps: &[Ep]) -> Vec<String> {
        eps.iter().map(|e| e.point.path.clone()).collect()
    }

    #[test]
    fn classify__label_pin_and_port_are_distinct_kinds() {
        assert_eq!(
            Ep::classify(&label("VCC")).kind,
            EpKind::Net("VCC".to_string())
        );
        assert_eq!(
            Ep::classify(&pin("R101.1", "R101", "1")).kind,
            EpKind::Pin {
                owner: "R101".to_string(),
                member: "1".to_string()
            }
        );
        let port = NetPoint::with_owner("sub1.clk", "sub1", IOType::In);
        assert_eq!(
            Ep::classify(&port).kind,
            EpKind::Port("sub1.clk".to_string())
        );
    }

    #[test]
    fn classify__member_falls_back_to_the_path_tail() {
        // No `member_name` on the point: the pin member is read from the path.
        let bare = NetPoint::with_owner("R101.2", "R101", IOType::None);
        assert_eq!(
            Ep::classify(&bare).kind,
            EpKind::Pin {
                owner: "R101".to_string(),
                member: "2".to_string()
            }
        );
    }

    #[test]
    fn from_sides__shapes_are_derived_from_the_two_faces() {
        // Point: left == right.
        let point = opd(vec![label("VCC")], vec![label("VCC")]);
        assert!(matches!(point.kind, OpdShape::Point(_)));
        // Row: one distinct point on each face (a 2-pin component).
        let row = opd(
            vec![pin("R101.1", "R101", "1")],
            vec![pin("R101.2", "R101", "2")],
        );
        assert!(matches!(row.kind, OpdShape::Row(_, _)));
        // Column: left == right, more than one element.
        let column = opd(vec![label("A"), label("B")], vec![label("A"), label("B")]);
        assert!(matches!(column.kind, OpdShape::Column(_)));
        // Node: distinct faces of differing width.
        let node = opd(
            vec![label("A"), label("B")],
            vec![label("C"), label("D"), label("E")],
        );
        assert!(matches!(node.kind, OpdShape::Node(_, _)));
    }

    #[test]
    fn i1__holds_for_every_basic_shape() {
        for operand in [
            opd(vec![label("VCC")], vec![label("VCC")]),
            opd(
                vec![pin("R101.1", "R101", "1")],
                vec![pin("R101.2", "R101", "2")],
            ),
            opd(vec![label("A"), label("B")], vec![label("A"), label("B")]),
            opd(
                vec![label("A"), label("B")],
                vec![label("C"), label("D"), label("E")],
            ),
        ] {
            assert!(operand.check_i1().is_ok(), "I1 must hold: {operand:?}");
        }
    }

    #[test]
    fn parallel__degenerate_left_takes_the_free_face_from_the_right() {
        // `VCC + R101`: the single label sticks to the left face, so the free
        // right port is R101.2 — it belongs to the *second* operand.
        let vcc = opd(vec![label("VCC")], vec![label("VCC")]);
        let r101 = opd(
            vec![pin("R101.1", "R101", "1")],
            vec![pin("R101.2", "R101", "2")],
        );
        let fold = fold_parallel(&vcc, &r101);
        assert_eq!(paths(&fold.result.left), vec!["VCC"]);
        assert_eq!(paths(&fold.result.right), vec!["R101.2"]);
        assert!(matches!(fold.result.kind, OpdShape::Row(_, _)));
        assert_eq!(paths(&fold.pair.0), vec!["VCC"]);
        assert_eq!(paths(&fold.pair.1), vec!["R101.1"]);
        assert!(fold.result.check_i1().is_ok());
    }

    #[test]
    fn parallel__degenerate_right_attaches_to_the_left_operands_right_face() {
        // `R101 + VCC`: the label is written on the right, so it sticks to the
        // row vector's right face and the result keeps R101's own faces.
        let r101 = opd(
            vec![pin("R101.1", "R101", "1")],
            vec![pin("R101.2", "R101", "2")],
        );
        let vcc = opd(vec![label("VCC")], vec![label("VCC")]);
        let fold = fold_parallel(&r101, &vcc);
        assert_eq!(paths(&fold.result.left), vec!["R101.1"]);
        assert_eq!(paths(&fold.result.right), vec!["R101.2"]);
        assert_eq!(paths(&fold.pair.0), vec!["R101.2"]);
        assert_eq!(paths(&fold.pair.1), vec!["VCC"]);
    }

    #[test]
    fn parallel__both_degenerate_keeps_the_first_operands_faces() {
        // `VCC + GND`: two single labels — the pairing side is the left face
        // of each, and the result still exposes VCC's degenerate face.
        let vcc = opd(vec![label("VCC")], vec![label("VCC")]);
        let gnd = opd(vec![label("GND")], vec![label("GND")]);
        let fold = fold_parallel(&vcc, &gnd);
        assert_eq!(paths(&fold.result.left), vec!["VCC"]);
        assert_eq!(paths(&fold.result.right), vec!["VCC"]);
        assert_eq!(paths(&fold.pair.0), vec!["VCC"]);
        assert_eq!(paths(&fold.pair.1), vec!["GND"]);
    }

    #[test]
    fn reversed__swaps_the_two_faces_and_reverses_the_shape() {
        // A two-pin row vector: `^` exchanges pin 1 / pin 2 (vec-arch §6.3).
        let r101 = opd(
            vec![pin("R101.1", "R101", "1")],
            vec![pin("R101.2", "R101", "2")],
        );
        let flipped = r101.reversed();
        assert_eq!(paths(&flipped.left), vec!["R101.2"]);
        assert_eq!(paths(&flipped.right), vec!["R101.1"]);
        assert!(matches!(flipped.kind, OpdShape::Row(_, _)));
        assert!(flipped.check_i1().is_ok());
    }

    #[test]
    fn reversed__is_an_identity_for_degenerate_operands() {
        // A point / column has no order of its own, so reversing it is a no-op.
        for operand in [
            opd(vec![label("VCC")], vec![label("VCC")]),
            opd(vec![label("A"), label("B")], vec![label("A"), label("B")]),
        ] {
            let flipped = operand.reversed();
            assert_eq!(paths(&flipped.left), paths(&operand.left));
            assert_eq!(paths(&flipped.right), paths(&operand.right));
            assert_eq!(flipped.kind, operand.kind);
        }
    }

    #[test]
    fn series__equal_rows_are_legal_and_anchor_right() {
        let a = opd(vec![label("A0")], vec![pin("R1.2", "R1", "2")]);
        let b = opd(vec![pin("R2.1", "R2", "1")], vec![label("B1")]);
        let step = fold_series(&a, &b);
        assert!(step.legal);
        assert_eq!(paths(&step.result.left), vec!["A0"]);
        assert_eq!(paths(&step.result.right), vec!["B1"]);
        assert_eq!(paths(&step.pair.0), vec!["R1.2"]);
        assert_eq!(paths(&step.pair.1), vec!["R2.1"]);
        assert!(step.result.check_i1().is_ok());
    }

    #[test]
    fn series__mismatched_rows_are_illegal() {
        let a = opd(vec![label("A0")], vec![label("X0"), label("X1")]);
        let b = opd(vec![label("Y0")], vec![label("B1")]);
        let step = fold_series(&a, &b);
        assert!(
            !step.legal,
            "2 rows into 1 row is an illegal §5.2 operation"
        );
    }

    #[test]
    fn series__an_empty_face_is_not_connectable() {
        let a = opd(vec![label("A0")], vec![label("X0")]);
        let empty = opd(Vec::new(), Vec::new());
        let step = fold_series(&a, &empty);
        assert!(!step.legal);
    }
}
