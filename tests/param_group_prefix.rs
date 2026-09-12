// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! param-prefix §2: **group prefix** `(A, B) => f(_, _)`.
//!
//! A group prefix is **not** one actual. Its members fill the bare `_`
//! placeholder slots **one-to-one, in order**, so the fold is the plain call
//! `f(A, B)` — ONE component, one net per slot:
//!
//! ```text
//! (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, _)
//!   ≡ RESS(10).Pullup(I2C0.SCL, I2C0.SDA)
//! ```
//!
//! This is what separates it from the **bus** prefix (§5), which *replicates*
//! the call once per lane. The load-bearing assertion below is therefore the
//! instance **count**: a replication bug (or the old all-placeholder rule,
//! which folded the whole group into a single actual and then hit E4176) is
//! caught by `heads.len() == 1`, which a `contains`-style lock would miss.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// Two-pin resistor whose `Pullup` body wires `n1 - this - n2`.
const RES: &str = "component RESS(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

/// Module skeleton: the two nets the group members name, plus a rail.
const HEAD: &str = "module main {\n    io I2C0{SCL, SDA}\n    io VCC\n    func M() {\n";

fn src_of(body: &str) -> String {
    format!("{RES}{HEAD}{body}\n    }}\n}}\n")
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

/// Wiring codes (`4xxx`) only — the fold must not report a wiring failure.
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

/// §2: the group members fill the `_` slots one-to-one — ONE component,
/// member[0] on pin 1 and member[1] on pin 2.
#[test]
fn group_prefix__fills_slots_one_to_one() {
    let parts = partition_of(
        &src_of("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, _)"),
        "/mcc/group-prefix.mc",
    );

    // Anti-false-green: exactly one component. A replication bug (or the old
    // all-placeholder fold) yields zero or several and is invisible to a
    // "the net contains X" assertion.
    let heads = instance_heads(&parts);
    assert_eq!(
        heads.len(),
        1,
        "a group prefix is ONE call, not one per member; heads={heads:?} (nets={parts:?})"
    );
    assert_eq!(parts.len(), 2, "one net per slot; nets={parts:?}");
    let head = heads.iter().next().unwrap();
    let scl = parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains("I2C0.SCL")))
        .expect("SCL net");
    assert!(
        scl.iter().any(|p| p == &format!("{head}.1")),
        "member 0 must land pin 1; net={scl:?}"
    );
    let sda = parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains("I2C0.SDA")))
        .expect("SDA net");
    assert!(
        sda.iter().any(|p| p == &format!("{head}.2")),
        "member 1 must land pin 2; net={sda:?}"
    );
}

/// §2 states the fold directly: the group form ≡ the explicit call. The two
/// instances are anonymous, so the comparison is on the partition, not names.
#[test]
fn group_prefix__equals_explicit_call() {
    let grouped = partition_of(
        &src_of("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, _)"),
        "/mcc/group-prefix-eq.mc",
    );
    let explicit = partition_of(
        &src_of("        RESS(10).Pullup(I2C0.SCL, I2C0.SDA)"),
        "/mcc/group-prefix-explicit.mc",
    );

    // Normalize the instance head away: the grouped form's auto-name is not
    // the point, the wiring is.
    let strip = |parts: Vec<Vec<String>>| -> Vec<Vec<String>> {
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
    };

    assert_eq!(
        strip(grouped),
        strip(explicit),
        "the group prefix must fold to the explicit call"
    );
}

/// The fold is the canonical spelling, not an error path.
#[test]
fn group_prefix__expands_quietly() {
    assert_eq!(
        wiring_codes_of(
            &src_of("        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, _)"),
            "/mcc/group-prefix-quiet.mc"
        ),
        Vec::<u32>::new(),
        "the group prefix must not raise a wiring code"
    );
}

/// §7 strict arity: a group wider than the slot list is **diagnosed**, never
/// silently expanded or silently dropped.
#[test]
fn group_prefix__arity_mismatch_is_diagnosed() {
    let codes = codes_of(
        &src_of("        (I2C0.SCL, I2C0.SDA, VCC) => RESS(10).Pullup(_, _)"),
        "/mcc/group-prefix-mismatch.mc",
    );
    assert!(
        codes.contains(&4176),
        "a member/slot count mismatch must report E4176; codes={codes:?}"
    );
    assert_eq!(
        partition_of(
            &src_of("        (I2C0.SCL, I2C0.SDA, VCC) => RESS(10).Pullup(_, _)"),
            "/mcc/group-prefix-mismatch.mc"
        ),
        Vec::<Vec<String>>::new(),
        "and must not wire anything"
    );
}

/// Ruling 2026-09-12 (§2): a multi-member group prefix is defined ONLY against
/// an actual list that is exactly one bare `_` per member. Any other spelling
/// is a strict-arity violation — E4176, nothing wired — and never "stuff the
/// whole group into the first slot", which used to land member[0], drop
/// member[1] and report nothing at all.
#[test]
fn group_prefix__non_placeholder_actuals_are_diagnosed() {
    for body in [
        // 2 members, 1 slot: the old silent-drop case.
        "        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)",
        // Already-written actual beside the slot.
        "        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(I2C0.SCL, _)",
    ] {
        let codes = codes_of(&src_of(body), "/mcc/group-prefix-nonuscore.mc");
        assert!(
            codes.contains(&4176),
            "a group prefix against non-`_` actuals must report E4176; body={body:?} codes={codes:?}"
        );
        // Anti-false-green: the old bug wired member[0] into a net and stayed
        // silent, so an E4176-only assertion would pass on the broken engine.
        assert_eq!(
            partition_of(&src_of(body), "/mcc/group-prefix-nonuscore.mc"),
            Vec::<Vec<String>>::new(),
            "and must not wire anything; body={body:?}"
        );
    }
}

/// A one-member group is just its member (`(VCC)` ≡ `VCC`) — it must not be
/// mistaken for a group prefix, and must not replicate.
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
