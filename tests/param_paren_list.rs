// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! param-prefix §3.1: a **parenthesized list** `(a, b)` written inside an
//! argument table is one of the enumerating forms — it contributes **one leaf
//! per member**, exactly like `X{a, b}` or `X[1:2]`.
//!
//! It is deliberately none of the two other things a parenthesis can be:
//!
//! - **not a statement fork** — that is the group in *statement* position
//!   (§9.1, locked by `param_group_prefix.rs`), which forks the statement;
//!   inside an argument table the call is one call, whatever the table holds;
//! - **not one operand** — a group is not a declared bus and owns no lane
//!   width of its own; there is nothing for §5 to lane-expand.
//!
//! The defect this locks (CIMP §1 U38, ruled 2026-09-15): the argument-table
//! conversion read the group's **own face**, which exposes only `opds[0]`. So
//! `RES(10).Pullup([(SPI.SCLK, SPI.MOSI)])` counted ONE leaf against the
//! two-formal `Pullup`, raised E4180 — and still built a half-open component:
//! the second member was gone from the circuit with no diagnostic naming it.
//!
//! The load-bearing assertions are therefore the net **partition** (both
//! members land on their own pin of the *same* component) plus the
//! **equivalence to the sibling spellings**. A code-list-only lock would be
//! satisfied by "expand into nothing"; a `contains` lock by the half-open
//! component of the old behaviour; an instance-count-only lock by the
//! statement fork the ruling rejects.

// Family naming `{family}__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::{McIds, McURI};

/// A two-pin resistor whose `Pullup` declares **one indexed formal** over two
/// member slots (`Pullup([n1, n2])`) — the shape every argument table in this
/// file fills, and the one whose leaf count the ruling turns on.
const RES: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Module skeleton: a 2-member bus port, so `SPI{SCLK, MOSI}` is a declared
/// selection and `SPI.SCLK` / `SPI.MOSI` are its lanes.
const HEAD: &str = "module main {\n    io SPI{SCLK, MOSI}\n    io VDD\n    func M() {\n";

fn src_of(body: &str) -> String {
    format!("{RES}{HEAD}{body}\n    }}\n}}\n")
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped. Point paths keep their instance prefix so the sibling
/// spellings' component wiring is comparable.
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

/// Diagnostic codes for `src`, sorted and deduped.
///
/// Only the naming-convention warnings of the short probe fixture are filtered
/// — the width error this file is about must survive (see [`benign`]).
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Codes unrelated to this lock: the short probe names warn (5641/5642/5643).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643)
}

/// Diagnostic messages for `src`, in emit order.
fn messages_of(src: &str, uri: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.msg.clone())
        .collect()
}

/// The auto-instance heads (`_Rn`) appearing in the partition's point paths.
fn instance_names(parts: &[Vec<String>]) -> BTreeSet<String> {
    parts
        .iter()
        .flatten()
        .filter_map(|p| p.rsplit_once('.').map(|(head, _)| head.to_string()))
        .filter(|head| head.starts_with('_'))
        .collect()
}

/// The net carrying `needle`, by substring match over its point paths.
fn net_holding<'a>(parts: &'a [Vec<String>], needle: &str) -> Option<&'a Vec<String>> {
    parts
        .iter()
        .find(|ps| ps.iter().any(|p| p.contains(needle)))
}

/// The two sibling spellings whose leaf count the ruling declares equal: the
/// parenthesized list and the explicit curly selection.
const PAREN: &str = "        RES(10).Pullup([(SPI.SCLK, SPI.MOSI)])";
const CURLY: &str = "        RES(10).Pullup([SPI{SCLK, MOSI}])";
const PLAIN: &str = "        RES(10).Pullup([SPI.SCLK, SPI.MOSI])";

/// §3.1: both members of the parenthesized list land — one leaf each — and the
/// call is ONE component, so the partition is the handwritten two-slot form's.
#[test]
fn paren_list__members_fill_the_formals() {
    let paren = partition_of(&src_of(PAREN), "/mcc/paren-list-fill.mc");
    let plain = partition_of(&src_of(PLAIN), "/mcc/paren-list-fill-hand.mc");

    // Anti-false-green: the component must be a real two-pin one. The old
    // behaviour built one half-open component (`SCLK` on pin 1, pin 2
    // floating), which would satisfy an instance-count assertion alone.
    let names = instance_names(&paren);
    assert_eq!(
        names.len(),
        1,
        "a parenthesized list is an argument list, not a statement fork: one \
         component; got {names:?} (partition={paren:?})"
    );
    let sclk = net_holding(&paren, "SPI.SCLK")
        .unwrap_or_else(|| panic!("SCLK member must land on a net; partition={paren:?}"));
    let mosi = net_holding(&paren, "SPI.MOSI")
        .unwrap_or_else(|| panic!("MOSI member must land on a net; partition={paren:?}"));
    assert_ne!(
        sclk, mosi,
        "the two members must take the two member slots separately; \
         partition={paren:?}"
    );

    assert_eq!(
        paren, plain,
        "the parenthesized list must land the handwritten two-slot partition"
    );
}

/// §3.1: the parenthesized list counts leaves the same way the explicit curly
/// selection does — same partition, same diagnostics.
#[test]
fn paren_list__equals_the_curly_selection() {
    let paren = partition_of(&src_of(PAREN), "/mcc/paren-list-curly.mc");
    let curly = partition_of(&src_of(CURLY), "/mcc/paren-list-curly-ref.mc");

    assert_eq!(
        instance_names(&curly).len(),
        1,
        "the reference spelling must itself be non-trivial; partition={curly:?}"
    );
    assert_eq!(
        paren, curly,
        "the parenthesized list and the curly selection must count the same"
    );
    assert_eq!(
        codes_of(&src_of(PAREN), "/mcc/paren-list-curly-code.mc"),
        codes_of(&src_of(CURLY), "/mcc/paren-list-curly-code-ref.mc"),
        "the parenthesized list and the curly selection must report the same \
         diagnostics"
    );
}

/// The whole-table spelling is clean: enumerating the members must not emit the
/// width error the un-counted call used to raise.
#[test]
fn paren_list__covers_the_whole_table_quietly() {
    assert_eq!(
        codes_of(&src_of(PAREN), "/mcc/paren-list-quiet.mc"),
        Vec::<u32>::new(),
        "a parenthesized list that fills exactly the formals must be quiet"
    );
}

/// With a sibling actual the count overflows — and the *message* has to carry
/// the real leaf count. That is the fingerprint of "counted, not dropped": the
/// old behaviour reported nothing at all here, and reported `provides 1` for
/// the whole-table spelling.
#[test]
fn paren_list__with_a_sibling_is_e4180() {
    const PAREN_SIBLING: &str = "        RES(10).Pullup([(SPI.SCLK, SPI.MOSI), VDD])";
    const CURLY_SIBLING: &str = "        RES(10).Pullup([SPI{SCLK, MOSI}, VDD])";

    assert!(
        codes_of(&src_of(PAREN_SIBLING), "/mcc/paren-list-sibling.mc").contains(&4180),
        "2 members + a sibling against a 2-slot formal must report E4180"
    );
    assert!(
        messages_of(&src_of(PAREN_SIBLING), "/mcc/paren-list-sibling-msg.mc")
            .iter()
            .any(|m| m.contains("provides 3")),
        "the width error must count the list's members (2 + 1 = 3), not take the \
         group for one operand"
    );
    assert_eq!(
        codes_of(&src_of(PAREN_SIBLING), "/mcc/paren-list-sibling-code.mc"),
        codes_of(&src_of(CURLY_SIBLING), "/mcc/paren-list-sibling-code-ref.mc"),
        "the overflowing parenthesized list must report what the overflowing \
         curly selection reports"
    );
}

/// The statement **fork** stays where it belongs. A group inside an argument
/// table must not fork the call: the ruling is "enumerating form", not "the
/// group's z-axis meaning carried into the argument face". Locking it here is
/// what keeps `param_group_prefix.rs` from being the only word on the subject.
#[test]
fn paren_list__does_not_fork_the_statement() {
    let parts = partition_of(
        &src_of("        RES(10).Pullup([(SPI.SCLK, SPI.MOSI), VDD])"),
        "/mcc/paren-list-nofork.mc",
    );
    assert_eq!(
        instance_names(&parts).len(),
        1,
        "the call is written once and must be built once — the statement fork \
         is the group's *statement* position, not its argument position; \
         partition={parts:?}"
    );
}

/// A one-member list is just its member (`(A)` ≡ `A`) — same partition, same
/// diagnostics, no extra leaf and no fork.
#[test]
fn paren_list__single_member_is_the_member() {
    let one = partition_of(&src_of("        RES(10).Pullup([(SPI.SCLK), VDD])"), "/mcc/paren-list-one.mc");
    let bare = partition_of(
        &src_of("        RES(10).Pullup([SPI.SCLK, VDD])"),
        "/mcc/paren-list-one-bare.mc",
    );

    assert_eq!(instance_names(&bare).len(), 1, "partition={bare:?}");
    assert_eq!(one, bare, "`(a)` must be `a`");
}
