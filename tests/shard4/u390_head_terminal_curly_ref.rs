// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U390: a bare head formal is a declared terminal (U384 N5-a) — it lives in
//! `params` as `McParamTypeKind::Terminal`, never in `def.insts`, yet it is a
//! boundary endpoint by declaration. The re-call binder accepts it
//! (`bindable_formals` → `is_power_terminal`) and the pass2 net-point gate
//! accepts it (it reads the instance-side `ports`, where the mint carries
//! `terminal=true`); the curly member face (`f1{pin}`) read only the def
//! store and answered E3175 with an empty availability hint, so the one
//! member spelling that cannot collide with a body label was the one that
//! could not address the terminal.
//!
//! The lock pins three branches the fix must keep apart: the terminal member
//! resolves and joins the bound net (the same net signature the dot form
//! already produced); a value head formal (`count::INT`) stays
//! unaddressable — the declaration channel is admitted by kind, not by
//! name-miss; and an unknown member still reports E3175, with the declared
//! terminal now present in the availability hint.

#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::{DiagnosticLevel, McIds};

/// The board every test below builds on: a two-pin source, a module whose
/// head declares one bare terminal, and the re-call that binds `src.VDD` to
/// it. No system-library class is needed — the terminal face is def-side.
/// `main` is left OPEN: each test appends its own body lines and the brace.
const BOARD: &str = r#"
component DC
{
    pins = [ 1:2 = [VDD, GND] ]
}

module Fwd(pin)
{
}

module Cnt(count::INT)
{
}

module main
{
    DC src
    Fwd f1
    f1(src.VDD)
"#;

struct Built {
    /// Every net's member paths, as a set per net: order within a net is not
    /// a reading, and the terminal point's own path spelling is the export
    /// face's business, not this lock's.
    nets: Vec<BTreeSet<String>>,
    errors: Vec<(u32, String)>,
}

impl Built {
    /// The single net holding `path`, or a panic naming what the board had.
    fn net_of(&self, path: &str) -> &BTreeSet<String> {
        let hits: Vec<&BTreeSet<String>> = self
            .nets
            .iter()
            .filter(|n| n.iter().any(|m| m == path))
            .collect();
        assert_eq!(hits.len(), 1, "`{path}` must be on exactly one net: {hits:?}");
        hits[0]
    }

    /// The net holding `path`, keyed by each member's last path segment —
    /// the spelling-tolerant view (`pin` vs `f1.pin` is U390's own export
    /// residual, deliberately not pinned here).
    fn member_keys_of(&self, path: &str) -> BTreeSet<String> {
        self.net_of(path)
            .iter()
            .map(|m| m.rsplit(['.', '/']).next().unwrap_or(m).to_string())
            .collect()
    }
}

fn build(tag: &str, main_body_lines: &str) -> Built {
    let _lock = common::lock();
    common::reset();

    let source = format!("{BOARD}{main_body_lines}\n}}\n");
    let uri = format!("/mcc/u390-{tag}.mc");
    mcc::mcc_load_from_string(&uri, &source);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000)
        .unwrap_or_else(|e| panic!("flat build failed for {tag}: {e:?}"));

    let nets: Vec<BTreeSet<String>> = table
        .get_nets()
        .iter()
        .map(|net| {
            net.points
                .iter()
                .filter_map(|p| table.get_entry(*p).map(|e| e.path.clone()))
                .collect()
        })
        .filter(|n: &BTreeSet<String>| !n.is_empty())
        .collect();

    let errors: Vec<(u32, String)> = mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .map(|d| (d.code, d.msg.clone()))
        .collect();

    Built { nets, errors }
}

/// The curly member reference addresses the declared terminal: no E3175, and
/// `src.VDD`, the terminal, and `src.GND` share one net — the net signature
/// the dot form (`f1.pin`) already produced.
#[test]
fn curly_member_ref_joins_the_bound_terminal_net() {
    let b = build("curly-joins", "    f1{pin} - src.GND");
    assert!(
        b.errors.is_empty(),
        "the curly terminal ref must be clean: {:?}",
        b.errors
    );
    let net = b.member_keys_of("main.src.1");
    let want: BTreeSet<String> = ["1", "2", "pin"].iter().map(|s| s.to_string()).collect();
    assert_eq!(net, want, "the terminal must join src.VDD and src.GND on one net");
}

/// The dot form keeps producing the same net — the fix must not have moved
/// the face that was already green.
#[test]
fn dot_member_ref_keeps_the_same_net() {
    let b = build("dot-joins", "    f1.pin - src.GND");
    assert!(
        b.errors.is_empty(),
        "the dot terminal ref must stay clean: {:?}",
        b.errors
    );
    let net = b.member_keys_of("main.src.1");
    let want: BTreeSet<String> = ["1", "2", "pin"].iter().map(|s| s.to_string()).collect();
    assert_eq!(net, want, "both spellings must land the same net");
}

/// A value head formal stays unaddressable: the declaration channel is
/// admitted by kind (`McParamTypeKind::Terminal`), not by name-miss, so a
/// `::INT` formal must keep answering E3175 exactly as before.
#[test]
fn value_head_formal_stays_unaddressable() {
    let b = build("value-formal-guard", "    Cnt(3) c1\n    c1{count} - src.GND");
    let hits: Vec<&(u32, String)> = b.errors.iter().filter(|(c, _)| *c == 3175).collect();
    assert_eq!(hits.len(), 1, "exactly one E3175 for the value formal: {:?}", b.errors);
}

/// An unknown member still reports E3175, and the availability hint now
/// lists the declared terminal — the hint must not stay empty when the
/// module's real boundary is the head formal.
#[test]
fn unknown_member_hint_lists_the_declared_terminal() {
    let b = build("unknown-member-hint", "    f1{nope} - src.GND");
    let hits: Vec<&(u32, String)> = b.errors.iter().filter(|(c, _)| *c == 3175).collect();
    assert_eq!(hits.len(), 1, "exactly one E3175: {:?}", b.errors);
    assert!(
        hits[0].1.contains("pin"),
        "the hint must name the declared terminal: {:?}",
        hits[0].1
    );
}
