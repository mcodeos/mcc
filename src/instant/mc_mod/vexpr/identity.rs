// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §7.5 element-identity contracts — **locked** (promoted from the shadow
//! L1 debug assertions on 2026-09-11, `unified-core-design.md`
//! §7.5 / §7.3 L3).
//!
//! An element's identity is an [`EpKind`]: `Pin(owner, member)` (the pin ->
//! component id back-link), `Port(path)` or `Net(name)`. Only `Pin` / `Port`
//! are id-bearing; a bare `Net` element (a `_` lead, a rail, a top-level
//! label) occupies a row but carries no owner id.
//!
//! The four contracts, and where each is enforced:
//!
//! | contract | meaning | enforced by |
//! |---|---|---|
//! | I1 single-source count | shape rows == face lengths == concrete point count, all from the same reduce | [`ConcreteOpd::check_i1`] |
//! | I2 identity threading | a fold merges points into one net; no step may create or drop an element id | [`check_i2`] |
//! | I3 device-body isolation | a device body is a cross-net shunt, not a counting element: it adds no row and no id | [`check_i3`] |
//! | I4 per-step cross-check | after every fold step the result re-satisfies I1 and the step conserves I2 — divergence is red, never silent | [`check_i4`] |
//!
//! I2 counts the **merged** faces as surviving identity: the pair a step joins
//! is not discarded, it becomes the internal net's element list, so its ids
//! must reappear on the output side (result faces ++ merged faces). Dropping
//! the merged terms is itself a violation — see
//! `i2__merged_faces_count_as_surviving_identity`.
//!
//! §7.5 I4 is written as "surviving `Pin` count == the step's shape row width".
//! That is the special case of an all-pin chain (`R1 - R2 - R3`); the general
//! law is I1's "one endpoint element per row", because a face may legitimately
//! be a bare net (`VCC - R1`). [`check_i4`] asserts the general law, and
//! [`pins_on`] lets a fixture assert the all-pin special case where it applies.
//!
//! The checks are pure and allocation-light; production calls them through
//! [`enforce_i4`], which is a `debug_assertions`-only panic (the design's
//! "debug assertion" — the *lock* is that the unit tests below drive every branch,
//! positive and negative).

use super::{ConcreteOpd, Ep, EpKind};
use std::collections::BTreeMap;

// identity multiset

/// Canonical key for one element identity. The kind tag keeps the three
/// namespaces apart: a pin `R101.1`, a port `R101.1` and a net `R101.1` are
/// three different elements, and a bare `path` comparison would conflate them.
pub fn id_key(kind: &EpKind) -> String {
    match kind {
        EpKind::Pin { owner, member } => format!("pin:{owner}.{member}"),
        EpKind::Port(path) => format!("port:{path}"),
        EpKind::Net(name) => format!("net:{name}"),
    }
}

/// The identity multiset of an element list: one count per [`id_key`].
pub fn id_counts<'a>(eps: impl IntoIterator<Item = &'a Ep>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for ep in eps {
        *counts.entry(id_key(&ep.kind)).or_insert(0) += 1;
    }
    counts
}

/// How many elements of a face are **id-bearing** (`Pin` / `Port`). I4's
/// "surviving `Pin` count == row width" reads this on an all-pin face.
pub fn pins_on(eps: &[Ep]) -> usize {
    eps.iter()
        .filter(|e| matches!(e.kind, EpKind::Pin { .. } | EpKind::Port(_)))
        .count()
}

fn merge_into(into: &mut BTreeMap<String, usize>, from: BTreeMap<String, usize>) {
    for (key, n) in from {
        *into.entry(key).or_insert(0) += n;
    }
}

/// Readable difference between two multisets, for the failure message.
fn delta(want: &BTreeMap<String, usize>, got: &BTreeMap<String, usize>) -> String {
    let mut parts = Vec::new();
    for (key, n) in want {
        let m = got.get(key).copied().unwrap_or(0);
        if *n != m {
            parts.push(format!("{key}: expected {n}, got {m}"));
        }
    }
    for (key, m) in got {
        if !want.contains_key(key) {
            parts.push(format!("{key}: expected 0, got {m}"));
        }
    }
    parts.join("; ")
}

// I2 / I3 / I4

/// §7.5 I2 — identity threading. The step's inputs' identity multiset equals
/// the multiset of what survives: the result's two faces **plus** the merged
/// faces the step turns into an internal net. No id created, none dropped.
pub fn check_i2(
    label: &str,
    inputs: &[&ConcreteOpd],
    result: &ConcreteOpd,
    merged: &[&[Ep]],
) -> Result<(), String> {
    let mut want: BTreeMap<String, usize> = BTreeMap::new();
    for opd in inputs {
        merge_into(
            &mut want,
            id_counts(opd.left.iter().chain(opd.right.iter())),
        );
    }

    let mut got: BTreeMap<String, usize> = BTreeMap::new();
    merge_into(
        &mut got,
        id_counts(result.left.iter().chain(result.right.iter())),
    );
    for net in merged {
        merge_into(&mut got, id_counts(net.iter()));
    }

    if want == got {
        return Ok(());
    }
    Err(format!(
        "I2 violated at `{label}`: identity multiset not conserved ({})",
        delta(&want, &got)
    ))
}

/// §7.5 I3 — device-body isolation. A `Device` body is the shunt between the
/// two nets an element crosses; it is **not** a counting element. Two teeth:
///
/// 1. I1 still holds with the body present — a body that were counted would
///    push the row count past the face lengths;
/// 2. every element on a merged net is an element an input already carried —
///    the body contributes no element of its own.
pub fn check_i3(label: &str, inputs: &[&ConcreteOpd], merged: &[&[Ep]]) -> Result<(), String> {
    for (i, opd) in inputs.iter().enumerate() {
        opd.check_i1().map_err(|why| {
            format!(
                "I3 violated at `{label}`: operand {i} (body {:?}) does not count its rows \
                 without the body — {why}",
                opd.body
            )
        })?;
    }

    let mut available: BTreeMap<String, usize> = BTreeMap::new();
    for opd in inputs {
        merge_into(
            &mut available,
            id_counts(opd.left.iter().chain(opd.right.iter())),
        );
    }
    for net in merged {
        for ep in net.iter() {
            let key = id_key(&ep.kind);
            match available.get_mut(&key) {
                Some(n) if *n > 0 => *n -= 1,
                _ => {
                    return Err(format!(
                        "I3 violated at `{label}`: the merged net carries `{key}`, \
                         which no operand element supplies — the body created an id"
                    ))
                }
            }
        }
    }
    Ok(())
}

/// §7.5 I4 — the per-step cross-check. After a fold step the result must
/// re-satisfy I1 (`check_i1`) and the step must conserve identity (I2). Run
/// after *every* step; a divergence is a hard failure, not a silent drift.
pub fn check_i4(
    label: &str,
    inputs: &[&ConcreteOpd],
    result: &ConcreteOpd,
    merged: &[&[Ep]],
) -> Result<(), String> {
    result
        .check_i1()
        .map_err(|why| format!("I4 violated at `{label}`: the step's result is not I1 — {why}"))?;
    check_i2(label, inputs, result, merged)
}

/// The per-step call site form: panic on divergence in debug builds, compiled
/// away in release (`cfg!` folds the branch out, like `debug_assert!`). Kept
/// as a free function so every fold site names the same contract.
pub fn enforce_i4(label: &str, inputs: &[&ConcreteOpd], result: &ConcreteOpd, merged: &[&[Ep]]) {
    if cfg!(debug_assertions) {
        if let Err(why) = check_i4(label, inputs, result, merged) {
            panic!("unified-core §7.5 I4: {why}");
        }
    }
    let _ = (label, inputs, result, merged);
}

/// [`check_i3`] at the call site, same debug-only panic form as
/// [`enforce_i4`]. Used by the `+` wiring, whose nets are not a conservation
/// step (the operator keeps only its external faces) but must still be built
/// from the operands' own elements.
pub fn enforce_i3(label: &str, inputs: &[&ConcreteOpd], merged: &[&[Ep]]) {
    if cfg!(debug_assertions) {
        if let Err(why) = check_i3(label, inputs, merged) {
            panic!("unified-core §7.5 I3: {why}");
        }
    }
    let _ = (label, inputs, merged);
}

// tests

#[cfg(test)]
mod tests {
    use super::super::fold::{fold_parallel_chain, fold_series};
    use super::*;
    use crate::instant::mc_net::NetPoint;
    use crate::semantic::basic::mc_bus::McBus;
    use crate::semantic::basic::opd_shape::OpdShape;
    use crate::semantic::common::IOType;

    fn label(name: &str) -> NetPoint {
        NetPoint::new(name, IOType::None)
    }

    fn pin(path: &str, owner: &str, member: &str) -> NetPoint {
        NetPoint::with_owner(path, owner, IOType::None).with_member_name(member)
    }

    fn ep(point: &NetPoint) -> Ep {
        Ep::classify(point)
    }

    fn opd(left: Vec<NetPoint>, right: Vec<NetPoint>) -> ConcreteOpd {
        ConcreteOpd::from_sides(left, right)
    }

    /// A two-terminal device with a `Device` body: it crosses two different
    /// nets through an intentional shunt.
    fn device(owner: &str) -> ConcreteOpd {
        let mut opd = opd(
            vec![pin(&format!("{owner}.1"), owner, "1")],
            vec![pin(&format!("{owner}.2"), owner, "2")],
        );
        opd.body = vec![super::super::BodyConn::Device];
        opd
    }

    fn total(counts: &BTreeMap<String, usize>) -> usize {
        counts.values().sum()
    }

    // id_key

    #[test]
    fn id_key__separates_the_three_element_namespaces() {
        let pin_key = id_key(&EpKind::Pin {
            owner: "R101".to_string(),
            member: "1".to_string(),
        });
        let port_key = id_key(&EpKind::Port("R101.1".to_string()));
        let net_key = id_key(&EpKind::Net("R101.1".to_string()));
        assert_ne!(pin_key, port_key, "a pin and a port are different elements");
        assert_ne!(pin_key, net_key, "a pin and a bare net are different");
        assert_ne!(port_key, net_key);
    }

    // I2

    #[test]
    fn i2__series_step_conserves_the_identity_multiset() {
        // `A0 - R1.2` joined to `R2.1 - B1`: the two written ends merge.
        let a = opd(vec![label("A0")], vec![pin("R1.2", "R1", "2")]);
        let b = opd(vec![pin("R2.1", "R2", "1")], vec![label("B1")]);
        let step = fold_series(&a, &b);
        assert!(step.legal);

        let want = id_counts(
            a.left
                .iter()
                .chain(a.right.iter())
                .chain(b.left.iter())
                .chain(b.right.iter()),
        );
        assert_eq!(total(&want), 4, "the fixture must carry four elements");

        assert_eq!(
            check_i2(
                "series",
                &[&a, &b],
                &step.result,
                &[&step.pair.0, &step.pair.1]
            ),
            Ok(())
        );
    }

    #[test]
    fn i2__merged_faces_count_as_surviving_identity() {
        // The pair the step joins is not discarded: without the merged terms
        // the conservation is broken. This pins the *reading* of the contract,
        // not just its implementation.
        let a = opd(vec![label("A0")], vec![pin("R1.2", "R1", "2")]);
        let b = opd(vec![pin("R2.1", "R2", "1")], vec![label("B1")]);
        let step = fold_series(&a, &b);
        let without_merged = check_i2("series", &[&a, &b], &step.result, &[]);
        assert!(
            without_merged.is_err(),
            "dropping the merged faces must be a violation"
        );
        assert!(without_merged.unwrap_err().contains("R1.2"));
    }

    #[test]
    fn i2__detects_a_dropped_id() {
        // Negative control, drop direction: the result loses R1.2 and the
        // merged pair does not carry it either.
        let a = opd(vec![label("A0")], vec![pin("R1.2", "R1", "2")]);
        let b = opd(vec![pin("R2.1", "R2", "1")], vec![label("B1")]);
        // The result keeps the two external ends, and the merged list carries
        // only R2.1 — R1.2 fell out of the accounting entirely.
        let broken = opd(vec![label("A0")], vec![label("B1")]);
        let err = check_i2(
            "series",
            &[&a, &b],
            &broken,
            &[&[ep(&pin("R2.1", "R2", "1"))]],
        );
        assert!(err.is_err(), "a dropped id must be red");
        assert!(err.unwrap_err().contains("I2 violated"));
    }

    #[test]
    fn i2__detects_a_created_id() {
        // Negative control, create direction: an element appears that no input
        // supplied (a "second copy of the rule" style drift).
        let a = opd(vec![label("A0")], vec![pin("R1.2", "R1", "2")]);
        let b = opd(vec![pin("R2.1", "R2", "1")], vec![label("B1")]);
        let step = fold_series(&a, &b);
        let ghost = ep(&pin("R9.7", "R9", "7"));
        let err = check_i2(
            "series",
            &[&a, &b],
            &step.result,
            &[&step.pair.0, &step.pair.1, &[ghost]],
        );
        assert!(err.is_err(), "a newly minted id must be red");
        assert!(err.unwrap_err().contains("R9.7"));
    }

    #[test]
    fn i2__parallel_chain_nets_carry_only_input_ids() {
        // `R101 + R102`: the `+` internal nets must be built from the operands'
        // own elements — the wiring creates no id.
        let r101 = opd(
            vec![pin("R101.1", "R101", "1")],
            vec![pin("R101.2", "R101", "2")],
        );
        let r102 = opd(
            vec![pin("R102.1", "R102", "1")],
            vec![pin("R102.2", "R102", "2")],
        );
        let wiring =
            fold_parallel_chain(&[r101.clone(), r102.clone()], &[false, false]).expect("wiring");
        assert!(!wiring.illegal);
        assert_eq!(
            wiring.nets.len(),
            2,
            "a two-pin parallel ties pin 1s into one net and pin 2s into another"
        );

        // I3's teeth over the `+` internal nets: every element on them is one
        // an operand already carries, with multiplicity.
        let mut pool = id_counts(r101.left.iter().chain(r101.right.iter()));
        for (key, n) in id_counts(r102.left.iter().chain(r102.right.iter())) {
            *pool.entry(key).or_insert(0) += n;
        }
        for net in &wiring.nets {
            for element in net {
                let key = id_key(&element.kind);
                let left = pool.get_mut(&key).expect("net carries an unknown id");
                assert!(*left > 0, "net over-draws `{key}`");
                *left -= 1;
            }
        }
        // Each operand contributes exactly once, so the nets must drain the
        // pool exactly: four pins, all accounted for.
        assert_eq!(pool.values().sum::<usize>(), 0);
    }

    // I3

    #[test]
    fn i3__device_body_carries_its_pins_without_adding_a_row() {
        // `R101` bridges two nets through its body: the merged net carries the
        // pin ids themselves, and the row count is unaffected by the body.
        let r101 = device("R101");
        assert!(r101.check_i1().is_ok(), "the body must not change the rows");

        let merged = vec![vec![
            r101.left[0].clone(),
            r101.right[0].clone(),
            ep(&label("MID")),
        ]];
        let merged_refs: Vec<&[Ep]> = merged.iter().map(|v| v.as_slice()).collect();

        // The label is not supplied by the operand, so this is a violation ...
        assert!(check_i3("device", &[&r101], &merged_refs).is_err());

        // ... and with only the operand's own elements it holds.
        let ok = vec![r101.left[0].clone(), r101.right[0].clone()];
        let ok_refs: Vec<&[Ep]> = vec![ok.as_slice()];
        assert_eq!(check_i3("device", &[&r101], &ok_refs), Ok(()));
        // Both pins are id-bearing, so the all-pin reading of I4 holds too.
        assert_eq!(pins_on(&ok), 2);
        assert_eq!(ok.len(), 2);
    }

    #[test]
    fn i3__detects_a_body_that_is_counted() {
        // Negative control: an operand whose *kind* claims one row per face but
        // whose face lists carry two — exactly the shape a body wrongly counted
        // as an element would produce.
        let mut forged = device("R101");
        forged.left = vec![
            ep(&pin("R101.1", "R101", "1")),
            ep(&pin("R101.3", "R101", "3")),
        ];
        forged.kind = OpdShape::Row(McBus::new("R101.1"), McBus::new("R101.2"));
        let err = check_i3("device", &[&forged], &[]);
        assert!(err.is_err(), "a body counted as an element must be red");
        assert!(err.unwrap_err().contains("I3 violated"));
    }

    // I4

    #[test]
    fn i4__every_step_of_a_series_chain_passes() {
        // Three device rows in series: fold left to right and cross-check each
        // step. Every face is an id-bearing pin, so the all-pin reading of I4
        // (`pins == width`) must hold at every step too.
        let chain = [device("R1"), device("R2"), device("R3")];
        let mut acc = chain[0].clone();
        for next in &chain[1..] {
            let step = fold_series(&acc, next);
            assert!(step.legal, "equal-width device rows are legal");
            assert_eq!(
                check_i4(
                    "chain",
                    &[&acc, next],
                    &step.result,
                    &[&step.pair.0, &step.pair.1]
                ),
                Ok(()),
                "every step must re-satisfy I1 and conserve I2"
            );
            assert_eq!(
                pins_on(&step.result.left),
                step.result.kind.size_left(),
                "on an all-pin chain the surviving pin count is the row width"
            );
            assert_eq!(pins_on(&step.result.right), step.result.kind.size_right());
            acc = step.result;
        }
        // The chain's two external faces are the first and last pin.
        assert_eq!(id_key(&acc.left[0].kind), "pin:R1.1");
        assert_eq!(id_key(&acc.right[0].kind), "pin:R3.2");
    }

    #[test]
    fn i4__mixed_pin_and_label_faces_are_not_the_all_pin_case() {
        // `VCC -> R1`: a bare-net element on the left face. The general law
        // (one endpoint per row) holds; the literal all-pin form does not —
        // which is why `check_i4` asserts the general law.
        let vcc = opd(vec![label("VCC")], vec![label("VCC")]);
        let r1 = device("R1");
        let step = fold_series(&vcc, &r1);
        assert!(step.legal);
        assert_eq!(
            check_i4(
                "mixed",
                &[&vcc, &r1],
                &step.result,
                &[&step.pair.0, &step.pair.1]
            ),
            Ok(())
        );
        assert_eq!(pins_on(&step.result.left), 0, "VCC is a bare net element");
        assert_eq!(step.result.kind.size_left(), 1, "the row is still occupied");
    }

    #[test]
    fn i4__detects_a_result_that_lost_i1() {
        // Negative control: a result whose shape rows and face lengths diverge.
        let forged = ConcreteOpd {
            left: vec![ep(&label("A")), ep(&label("B"))],
            right: vec![ep(&label("A"))],
            kind: OpdShape::Row(McBus::new("A"), McBus::new("A")),
            body: Vec::new(),
            lane: None,
        };
        let err = check_i4("forged", &[&forged], &forged, &[]);
        assert!(err.is_err(), "an I1-breaking result must be red");
        assert!(err.unwrap_err().contains("I4 violated"));
    }

    #[test]
    fn i4__detects_an_unconserved_step() {
        // Negative control: I1 holds but the step loses an id.
        let a = opd(vec![label("A0")], vec![pin("R1.2", "R1", "2")]);
        let b = opd(vec![pin("R2.1", "R2", "1")], vec![label("B1")]);
        let step = fold_series(&a, &b);
        let err = check_i4("series", &[&a, &b], &step.result, &[]);
        assert!(err.is_err(), "an I2-breaking step must be red");
        assert!(err.unwrap_err().contains("I2 violated"));
    }
}
