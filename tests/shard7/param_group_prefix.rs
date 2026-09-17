// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! param-prefix §9.1: **group prefix** `(A, B) => f(...)` is a **statement
//! fork**, not one actual and not a fill of `_` slots.
//!
//! The group is z-axis (statement) structure — its members carry no order — so
//! each member becomes its **own** prefix statement, folded on its own:
//!
//! ```text
//! (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)
//!   ≡ I2C0.SCL => RESS(10).Pullup(_, VCC)   and
//!     I2C0.SDA => RESS(10).Pullup(_, VCC)   -- TWO components
//! ```
//!
//! This is what separates it from the **bus** prefix (§5), which *replicates*
//! the call once per lane of ONE multi-member operand. The load-bearing
//! assertions below are therefore the instance **count** and the
//! **order-insensitivity**: the withdrawn one-to-one fill (zip) produced ONE
//! component whose slots followed the written order, so `(A, B)` and `(B, A)`
//! wired different circuits — a `contains`-style lock would miss both.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// Two-pin resistor whose `Pullup` body wires `n1 - this - n2`.
///
/// `Pullup` declares **two scalar network formals** on purpose: the fork's
/// branches are two-operand calls (`f(A, VCC)`), and §9.7 states the group
/// spelling as "two scalar formals, two actuals". The library's own
/// `Pullup([net, vcc])` is one **index** formal — that shape takes a single
/// `_` fed by a vector prefix (`[A, B] => …Pullup(_)`), which is a different
/// axis (§3) and not this file's subject.
const RES: &str = "component RESS(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

/// Module skeleton: the two nets the group members name, plus a rail.
const HEAD: &str = "module main {\n    io I2C0{SCL, SDA}\n    io VCC\n    func M() {\n";

/// Same skeleton plus a third port, for the chain-tail case.
const HEAD_TAIL: &str =
    "module main {\n    io I2C0{SCL, SDA}\n    io VCC\n    io TAIL\n    func M() {\n";

fn src_of(body: &str) -> String {
    format!("{RES}{HEAD}{body}\n    }}\n}}\n")
}

fn src_of_tail(body: &str) -> String {
    format!("{RES}{HEAD_TAIL}{body}\n    }}\n}}\n")
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped.
fn partition_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut parts: Vec<Vec<String>> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(_, pts)| {
                    let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
                    ps.sort();
                    ps
                })
                .filter(|ps| !ps.is_empty())
                .collect()
        })
        .unwrap_or_default();
    parts.sort();
    parts
}

/// Every diagnostic code emitted while building `src`, sorted and deduped.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// How many diagnostics carry `code` while building `src`.
fn count_code(src: &str, uri: &str, code: u32) -> usize {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == code)
        .count()
}

/// Wiring codes (`4xxx`) only — the fork must not report a wiring failure.
fn wiring_codes_of(src: &str, uri: &str) -> Vec<u32> {
    codes_of(src, uri)
        .into_iter()
        .filter(|c| (4000..5000).contains(c))
        .collect()
}

/// The **component** heads appearing in the partition's point paths: a point
/// whose last segment is a numeric pin (`_RESS1.1`), which excludes the module
/// port paths (`I2C0.SCL`) — those name the nets, not the components.
fn instance_heads(parts: &[Vec<String>]) -> BTreeSet<String> {
    parts
        .iter()
        .flatten()
        .filter_map(|p| {
            let (head, pin) = p.rsplit_once('.')?;
            pin.parse::<u32>().ok()?;
            Some(head.to_string())
        })
        .collect()
}

/// Replace every auto instance head with `<inst>` so two runs compare on
/// wiring alone — the fork's members are anonymous, and their sequential
/// auto-names (`_RESS1` / `_RESS2`) follow statement order, which is exactly
/// what must NOT be observable.
fn strip_heads(parts: Vec<Vec<String>>) -> Vec<Vec<String>> {
    let heads = instance_heads(&parts);
    let mut out: Vec<Vec<String>> = parts
        .into_iter()
        .map(|ps| {
            ps.into_iter()
                .map(|p| {
                    for h in &heads {
                        if let Some(rest) = p.strip_prefix(&format!("{h}.")) {
                            return format!("<inst>.{rest}");
                        }
                    }
                    p
                })
                .collect()
        })
        .collect();
    out.sort();
    out
}

/// §9.1: each member is its own statement — TWO components, each with the
/// member on pin 1 and the rail on pin 2.
#[test]
fn group_prefix__forks_into_one_statement_per_member() {
    let parts = partition_of(
        &src_of("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)"),
        "/mcc/group-prefix.mc",
    );

    let heads = instance_heads(&parts);
    assert_eq!(
        heads.len(),
        2,
        "a group prefix forks into one call per member; heads={heads:?} (nets={parts:?})"
    );
    let scl = parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains("I2C0.SCL")))
        .expect("SCL net");
    let sda = parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains("I2C0.SDA")))
        .expect("SDA net");
    // Each member must land pin 1 of its OWN component — the zip fingerprint
    // was member[1] landing pin 2 of member[0]'s component.
    for (name, net) in [("SCL", scl), ("SDA", sda)] {
        let pin1: Vec<&String> = net.iter().filter(|p| p.ends_with(".1")).collect();
        assert_eq!(
            pin1.len(),
            1,
            "{name} must land pin 1 of one component; net={net:?}"
        );
    }
}

/// §9.1: the fork is the two written statements — nothing more, nothing less.
#[test]
fn group_prefix__equals_two_written_statements() {
    let forked = strip_heads(partition_of(
        &src_of("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)"),
        "/mcc/group-prefix-eq.mc",
    ));
    let written = strip_heads(partition_of(
        &src_of(
            "        I2C0.SCL => RESS(10).Pullup(_, VCC)\n        \
             I2C0.SDA => RESS(10).Pullup(_, VCC)",
        ),
        "/mcc/group-prefix-written.mc",
    ));

    assert_eq!(
        forked, written,
        "the group fork must wire what the two written statements wire"
    );
}

/// §9.1 + §10.6: the group has **no order** — `(A, B)` and `(B, A)` must wire
/// the same circuit. The withdrawn zip fill failed exactly here (its slots
/// followed the written order), so this is the fork's fingerprint lock.
#[test]
fn group_prefix__order_insensitive() {
    let ab = strip_heads(partition_of(
        &src_of("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)"),
        "/mcc/group-prefix-ab.mc",
    ));
    let ba = strip_heads(partition_of(
        &src_of("        (I2C0.SDA, I2C0.SCL) => RESS(10).Pullup(_, VCC)"),
        "/mcc/group-prefix-ba.mc",
    ));

    assert_eq!(
        ab, ba,
        "writing the group in the other order must not change the circuit"
    );
}

/// §9.7: two `_` in the actual list is E4176 per branch — and NOTHING lands.
/// Anti-false-green: the withdrawn engine wired ONE component here (member[0]
/// on pin 1, member[1] on pin 2), so an E4176-only assertion would pass on it.
#[test]
fn group_prefix__two_placeholders_is_e4176_and_nothing() {
    for body in [
        "        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, _)",
        "        (I2C0.SDA, I2C0.SCL) => RESS(10).Pullup(_, _)",
    ] {
        assert!(
            codes_of(&src_of(body), "/mcc/group-prefix-two.mc").contains(&4176),
            "two `_` against one prefix operand must report E4176; body={body:?}"
        );
        assert_eq!(
            count_code(&src_of(body), "/mcc/group-prefix-two.mc", 4176),
            2,
            "each fork branch reports its own E4176; body={body:?}"
        );
        assert_eq!(
            partition_of(&src_of(body), "/mcc/group-prefix-two.mc"),
            Vec::<Vec<String>>::new(),
            "and must not wire anything; body={body:?}"
        );
    }
}

/// The fork is the canonical spelling, not an error path.
#[test]
fn group_prefix__expands_quietly() {
    assert_eq!(
        wiring_codes_of(
            &src_of("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)"),
            "/mcc/group-prefix-quiet.mc"
        ),
        Vec::<u32>::new(),
        "the group fork must not raise a wiring code"
    );
}

/// §9.1 rule 3 / §10.6 rule 2: the chain tail rides **each** branch — the fold
/// emits a `Multiple` that sits inside the `Series`, so the statement
/// expansion has to see through the chain, not just the top level.
#[test]
fn group_prefix__chain_tail_rides_each_branch() {
    let parts = partition_of(
        &src_of_tail("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC) -> TAIL"),
        "/mcc/group-prefix-chain.mc",
    );

    let heads = instance_heads(&parts);
    assert_eq!(
        heads.len(),
        2,
        "each fork branch carries the chain tail once; heads={heads:?} (nets={parts:?})"
    );
    let tail = parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains("TAIL")))
        .expect("TAIL net");
    let tail_pins: Vec<&String> = tail.iter().filter(|p| p.ends_with(".2")).collect();
    assert_eq!(
        tail_pins.len(),
        2,
        "the tail must reach BOTH branches' pin 2; net={tail:?}"
    );
}

/// A one-member group is just its member (`(VCC)` ≡ `VCC`) — it must not be
/// mistaken for a fork, and must not replicate.
#[test]
fn group_prefix__single_member_is_the_member() {
    let parts = partition_of(
        &src_of("        (VCC) => RESS(10).Pullup(_, I2C0.SCL)"),
        "/mcc/group-prefix-single.mc",
    );
    assert_eq!(
        instance_heads(&parts).len(),
        1,
        "one component; nets={parts:?}"
    );
    assert_eq!(parts.len(), 2, "VCC and SCL nets; nets={parts:?}");
}
