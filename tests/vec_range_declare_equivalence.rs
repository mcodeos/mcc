// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! resolve-gate §3.3b: **range-declare expansion equivalence**.
//!
//! `base[a:b]::CLASS(cargs).Method(margs)` is the *declared* multi-instance
//! form: it materializes one **named** instance per member and dispatches the
//! trailing method per member, with the arguments **shared by every member**
//! (never indexed or sliced). What is locked here is that equivalence — the
//! declared form lands exactly the partition of the N explicit lines — plus the
//! role the array plays in a position (a *set* of those instances; `[...]` is
//! what gives it a lane order, which is how it pairs with a bus).
//!
//! The assertion is on the net **partition** (point-sets that share a net,
//! canonicalized, net names dropped) — never on a diagnostic-code list, which
//! a short-circuited net would satisfy. The anti-false-green guards below are
//! the load-bearing part: `contains`-style assertions on a *merged* net pass
//! vacuously (see §3.3b "candidate defect" — the existing Table A form4 lock is blind
//! to exactly that), so every test also asserts the two sides stay **apart**.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// Two-pin component whose method **returns the vector it wired**, which is
/// what the array path reads back (resolve-gate §3.3, same origin as E3179).
const RES: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n        return [n1, n2]\n    }\n}\n";

/// Header with the bus to pair against and the two nets the pull-ups sit on.
const HEAD: &str = "module main {\n    io I2C0{SCL, SDA}\n    io NET\n    io VCC\n    func M() {\n";

fn src_of(body: &str) -> String {
    format!("{RES}{HEAD}{body}\n    }}\n}}\n")
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped. Point paths keep their instance prefix, so the forms'
/// component wiring is comparable.
fn nets_of(src: &str, uri: &str) -> Vec<Vec<String>> {
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

/// Wiring codes (`4xxx`) emitted while building `src`. Codes are read only as
/// a wiring-failure guard, never as the oracle: the partition is.
fn wiring_codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| (4000..5000).contains(c))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// The net carrying `needle`, by substring match over its point paths.
fn net_holding<'a>(parts: &'a [Vec<String>], needle: &str) -> Option<&'a Vec<String>> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains(needle)))
}

/// The instance heads appearing in the partition's point paths.
fn instance_heads(parts: &[Vec<String>]) -> BTreeSet<String> {
    parts
        .iter()
        .flatten()
        .filter_map(|p| p.rsplit_once('.').map(|(head, _)| head.to_string()))
        .collect()
}

/// No point on `net` may be a pin of `instance` with pin number `pin` — the
/// anti-merge guard: a `contains`-style lock passes on a merged net too.
fn assert_pin_absent(net: &[String], instance: &str, pin: &str, what: &str) {
    assert!(
        !net.iter()
            .any(|p| p.starts_with(instance) && p.ends_with(pin)),
        "{what}: {instance} pin {pin} must not be on this net; net={net:?}"
    );
}

/// §3.3b: the declared form ≡ the N explicit lines, byte for byte.
#[test]
fn range__declared_form_equals_explicit_lines() {
    let declared = nets_of(
        &src_of("        res[1:2]::RES(10).Pullup([NET, VCC])"),
        "/mcc/range-declared.mc",
    );
    let explicit = nets_of(
        &src_of(
            "        res1::RES(10).Pullup([NET, VCC])\n        res2::RES(10).Pullup([NET, VCC])",
        ),
        "/mcc/range-explicit.mc",
    );

    // Anti-false-green: both sides must build the two named instances, and each
    // side must keep the two nets APART — a merged net satisfies equality but
    // is not the law.
    for (label, parts) in [("declared", &declared), ("explicit", &explicit)] {
        let heads = instance_heads(parts);
        assert!(
            heads.contains("res1") && heads.contains("res2"),
            "{label}: both named members must materialize; heads={heads:?} (nets={parts:?})"
        );
        assert_eq!(
            parts.len(),
            2,
            "{label}: exactly the NET and VCC nets; nets={parts:?}"
        );
        let net_side = net_holding(parts, "res1.1").expect("res1 pin 1 landed");
        assert_pin_absent(net_side, "res1", ".2", label);
        assert_pin_absent(net_side, "res2", ".2", label);
        let vcc_side = net_holding(parts, "res1.2").expect("res1 pin 2 landed");
        assert_pin_absent(vcc_side, "res1", ".1", label);
        assert_pin_absent(vcc_side, "res2", ".1", label);
    }

    assert_eq!(
        declared, explicit,
        "the declared form must land the explicit lines' partition"
    );
}

/// The declared form expands quietly (no `4xxx` wiring code) — it is the
/// *canonical* spelling, not an error path.
#[test]
fn range__declared_form_expands_quietly() {
    assert_eq!(
        wiring_codes_of(
            &src_of("        res[1:2]::RES(10).Pullup([NET, VCC])"),
            "/mcc/range-quiet.mc"
        ),
        Vec::<u32>::new(),
        "the declared form must not raise a wiring code"
    );
}

/// §3.3b position role: the array in a chain position is the *set* of its members —
/// bare, inside `[...]`, or spelled out as a list, all three land the same
/// partition. `[...]` adds lane ORDER, it does not create the instances.
#[test]
fn range__chain_array_equals_explicit_list() {
    let bare = nets_of(
        &src_of("        I2C0 -> res[1:2]::RES(10)"),
        "/mcc/range-chain-bare.mc",
    );
    let squared = nets_of(
        &src_of("        I2C0 -> [res[1:2]::RES(10)]"),
        "/mcc/range-chain-square.mc",
    );
    let listed = nets_of(
        &src_of("        I2C0 -> [res1::RES(10), res2::RES(10)]"),
        "/mcc/range-chain-list.mc",
    );

    assert_eq!(bare, squared, "bare array and [array] must agree");
    assert_eq!(squared, listed, "[array] and the explicit list must agree");

    // The pairing itself, stated so a silently-empty expansion cannot pass.
    let scl = net_holding(&squared, "I2C0.SCL").expect("SCL net");
    assert!(
        scl.iter().any(|p| p.starts_with("res1.1")),
        "SCL must reach member 1; net={scl:?}"
    );
    let sda = net_holding(&squared, "I2C0.SDA").expect("SDA net");
    assert!(
        sda.iter().any(|p| p.starts_with("res2.1")),
        "SDA must reach member 2; net={sda:?}"
    );
}

/// The workhorse spelling the design recommends (I2C0 pull-ups): the array as
/// a chain member paired **per lane** on the bus side, with the second pin of
/// every member on the **shared** rail.
#[test]
fn range__chain_array_with_rail_wires_per_lane() {
    let parts = nets_of(
        &src_of("        I2C0 -> [res[1:2]::RES(10)] -> [VCC, VCC]"),
        "/mcc/range-chain-rail.mc",
    );

    assert_eq!(
        parts.len(),
        3,
        "one net per lane plus the shared rail; nets={parts:?}"
    );
    let scl = net_holding(&parts, "I2C0.SCL").expect("SCL net");
    assert!(
        scl.iter().any(|p| p.starts_with("res1.1")) && !scl.iter().any(|p| p.starts_with("res2")),
        "SCL pairs with member 1 only; net={scl:?}"
    );
    let sda = net_holding(&parts, "I2C0.SDA").expect("SDA net");
    assert!(
        sda.iter().any(|p| p.starts_with("res2.1")) && !sda.iter().any(|p| p.starts_with("res1")),
        "SDA pairs with member 2 only; net={sda:?}"
    );
    let rail = net_holding(&parts, "res1.2").expect("rail net");
    assert!(
        rail.iter().any(|p| p.starts_with("res2.2")) && rail.iter().any(|p| p.starts_with("VCC")),
        "both second pins share the rail; net={rail:?}"
    );
}

/// §3.3b candidate defect — the array path reads the method's **return** to learn the
/// member's shape, so a method with no `return` silently merges every pin onto
/// one net instead of reporting. Ignored until the defect is ruled on; kept
/// here so the day it is fixed, the fix has a switch to flip.
#[test]
#[ignore = "known defect (resolve-gate §3.3b): array path with a returnless method silently merges nets"]
fn range__returnless_method_must_not_merge_nets() {
    const NO_RETURN: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";
    let src =
        format!("{NO_RETURN}{HEAD}        res[1:2]::RES(10).Pullup([NET, VCC])\n    }}\n}}\n");
    let parts = nets_of(&src, "/mcc/range-no-return.mc");
    assert_eq!(
        parts.len(),
        2,
        "a returnless method must still land two nets (or diagnose); nets={parts:?}"
    );
}
