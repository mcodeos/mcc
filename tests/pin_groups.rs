// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration tests for `pins.<group> = [...]` (attribute status-design.md §2.5,
// §2.7; CIMP U40 closed 2026-09-16). The block's first child is the group name;
// it is a component-level grouping of the pin ids the block declares, and enters
// nothing else — no identity, no addressing, no netlist, no ERC. The rows walk
// the same loop a flat `pins = [...]` body does, so the two forms register
// identically (locked first, below). W2171, which reported the name as
// unsupported syntax, is retired.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{DiagnosticLevel, McIds};
use std::ops::Range;

/// The retired code, kept numeric: it must never appear in output again.
const RETIRED_UNSUPPORTED: u32 = 2171;

/// The declaration block of one group: its name, the pin ids it registered and
/// the block's source extent.
type Group = (String, Vec<String>, Range<usize>);

/// Every diagnostic as `(code, level, message)`, sorted — the group name shifts
/// source columns, so the position is deliberately not part of the comparison.
fn diags_of(src: &str, uri: &str) -> Vec<(u32, DiagnosticLevel, String)> {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri.to_string());
    let mut diags: Vec<(u32, DiagnosticLevel, String)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.level, d.msg.clone()))
        .collect();
    diags.sort_by(|a, b| (a.0, &a.2).cmp(&(b.0, &b.2)));
    diags
}

/// The net partition of `main`: point paths sorted, net names dropped.
fn nets_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let (_, _, _, net_store) =
        mcc::mcc_build_with_nets(&McIds::from("main"), &uri.to_string()).expect("build");
    net_store
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
        .unwrap_or_default()
}

/// The pin-id count of the component instance `inst` — the identity surface the
/// group name must not touch.
fn pin_count_of(src: &str, uri: &str, inst: &str) -> usize {
    def_of(src, uri, inst).pins.pins.len()
}

fn def_of(src: &str, uri: &str, inst: &str) -> std::sync::Arc<mcc::McComponent> {
    let _lock = common::lock();
    common::reset();
    mcc::mcc_load_from_string(&uri.to_string(), src);
    let (instance, arena, store, _net_store) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri.to_string()).expect("build");
    let view = mcc::TreeView::new(&arena, &store);
    let def = view
        .components(&instance)
        .find(|c| c.name == inst)
        .unwrap_or_else(|| panic!("no instance named {inst}"))
        .def
        .clone();
    def
}

fn groups_of(src: &str, uri: &str, inst: &str) -> Vec<Group> {
    def_of(src, uri, inst)
        .pins
        .groups
        .iter()
        .map(|g| (g.name.clone(), g.pins.clone(), g.span.clone()))
        .collect()
}

fn group_names(src: &str, uri: &str, inst: &str) -> Vec<String> {
    groups_of(src, uri, inst)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect()
}

// The two locks this batch exists for.

/// Equivalence: a grouped body and a flat body differing only in the group name
/// register the same pins, reach the same nets and emit the same diagnostics.
/// This is the half the U40 row recorded as "measured, no lock".
#[test]
fn sem_pingroup__grouped_body_registers_identically() {
    let uri = "/mcc/pin-group-equivalence.mc";
    let flat = r#"
component C1
{
    pins = [
        1 = A
        2 = B
        io 3 = C
    ]
}

module main
{
    io VDD
    io SIG
    C1 u1
    VDD -> u1.A
    SIG -> u1.B
}
"#;
    let grouped = flat.replacen("pins = [", "pins.bank = [", 1);

    // Without this the lock is vacuous: if the grouped fixture did not parse as
    // a group, both sides would be the flat parse and compare equal by identity.
    assert!(
        group_names(&grouped, uri, "u1") == vec!["bank".to_string()],
        "the grouped fixture must really declare a group"
    );
    assert!(
        group_names(flat, uri, "u1").is_empty(),
        "the flat fixture must declare none"
    );

    let (flat_diags, group_diags) = (diags_of(flat, uri), diags_of(&grouped, uri));
    let (flat_nets, group_nets) = (nets_of(flat, uri), nets_of(&grouped, uri));

    assert!(
        !flat_diags
            .iter()
            .any(|(code, _, _)| *code == RETIRED_UNSUPPORTED),
        "the flat form must not report the retired code; got {flat_diags:?}"
    );
    assert!(
        !group_diags
            .iter()
            .any(|(code, _, _)| *code == RETIRED_UNSUPPORTED),
        "the grouped form must not report the retired code; got {group_diags:?}"
    );
    assert_eq!(
        flat_diags, group_diags,
        "grouping a body must not change its diagnostics"
    );
    assert_eq!(
        flat_nets, group_nets,
        "grouping a body must not change nets"
    );
    assert!(
        !group_nets.is_empty(),
        "the fixture must reach at least one net, or this lock proves nothing"
    );
    assert_eq!(
        pin_count_of(flat, uri, "u1"),
        pin_count_of(&grouped, uri, "u1"),
        "the two forms must register the same pin ids"
    );
}

/// Retirement: the name is no longer reported as unsupported syntax, in either
/// the non-empty or the empty form. The empty block still reports the parser's
/// own emptiness (2116), which is a different fact at a different position.
#[test]
fn sem_pingroup__retires_unsupported_warning() {
    let uri = "/mcc/pin-group-retire.mc";
    let nonempty = r#"
component C1
{
    pins.bank = [ 1 = A ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let empty = nonempty.replacen("[ 1 = A ]", "[]", 1);
    assert_ne!(nonempty, empty, "the fixture rewrite must apply");

    for (label, src) in [("non-empty", nonempty), ("empty", empty.as_str())] {
        assert_eq!(
            group_names(src, uri, "u1"),
            vec!["bank".to_string()],
            "the {label} fixture must really declare the group, or silence proves nothing"
        );
        let diags = diags_of(src, uri);
        assert!(
            !diags
                .iter()
                .any(|(code, _, _)| *code == RETIRED_UNSUPPORTED),
            "`pins.<group>` ({label}) must not report the retired code; got {diags:?}"
        );
    }

    let diags = diags_of(&empty, uri);
    assert!(
        diags
            .iter()
            .any(|(code, level, _)| *code == mcc::errcodes::PARSER_EMPTY_PINS
                && *level == DiagnosticLevel::Error),
        "an empty group block must still report the parser's emptiness; got {diags:?}"
    );
}

// The table's shape.

#[test]
fn sem_pingroup__records_names_members_and_declaration_order() {
    let uri = "/mcc/pin-group-shape.mc";
    let src = r#"
component C1
{
    pins = [ 9 = Z ]
    pins.left = [
        1 = A
        2 = B
    ]
    pins.right = [
        3 = C
    ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    assert_eq!(
        group_names(src, uri, "u1"),
        vec!["left".to_string(), "right".to_string()],
        "groups must appear in first-occurrence source order"
    );
    let groups = groups_of(src, uri, "u1");
    assert_eq!(groups[0].1, vec!["1".to_string(), "2".to_string()]);
    assert_eq!(groups[1].1, vec!["3".to_string()]);
    assert_eq!(
        pin_count_of(src, uri, "u1"),
        4,
        "a pin outside every group is still a pin"
    );
}

/// Groups are a list of sets, not a partition: a pin declared once may be
/// claimed by more than one group, and the second claim must survive (this is
/// why the collector is a block cursor and not a `decl_order` diff — that vector
/// only grows at first registration).
#[test]
fn sem_pingroup__overlapping_groups_both_keep_the_pin() {
    let uri = "/mcc/pin-group-overlap.mc";
    let src = r#"
component C1
{
    pins.first = [
        1 = A
        2 = B
    ]
    pins.second = [
        2 = B
        3 = C
    ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let groups = groups_of(src, uri, "u1");
    assert_eq!(groups[0].1, vec!["1".to_string(), "2".to_string()]);
    assert_eq!(
        groups[1].1,
        vec!["2".to_string(), "3".to_string()],
        "a pin already registered by an earlier group must still join the later one"
    );
}

/// A pin declared before the group block joins the group too: the block states
/// which pins belong to it, not which pins it happens to declare first.
#[test]
fn sem_pingroup__claims_pins_declared_before_the_block() {
    let uri = "/mcc/pin-group-before.mc";
    let src = r#"
component C1
{
    pins = [
        1 = A
        2 = B
    ]
    pins.bank = [
        1 = A
        2 = B
    ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let groups = groups_of(src, uri, "u1");
    assert_eq!(groups.len(), 1, "one group; got {groups:?}");
    assert_eq!(
        groups[0].1,
        vec!["1".to_string(), "2".to_string()],
        "re-declaring a pin inside the block must not add it twice"
    );
}

#[test]
fn sem_pingroup__same_name_blocks_merge_into_one_entry() {
    let uri = "/mcc/pin-group-merge.mc";
    let src = r#"
component C1
{
    pins.bank = [ 1 = A ]
    pins.bank = [ 2 = B ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let groups = groups_of(src, uri, "u1");
    assert_eq!(
        group_names(src, uri, "u1"),
        vec!["bank".to_string()],
        "blocks sharing a name are one group"
    );
    assert_eq!(groups[0].1, vec!["1".to_string(), "2".to_string()]);
}

/// A power-direction row registers through the same path as any other, so it
/// joins its group too.
#[test]
fn sem_pingroup__power_row_joins_its_group() {
    let uri = "/mcc/pin-group-power.mc";
    let src = r#"
component C1
{
    pins.supply = [
        psnk 1 = GND
        2 = OUT
    ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let groups = groups_of(src, uri, "u1");
    assert_eq!(groups.len(), 1, "one group; got {groups:?}");
    assert_eq!(
        groups[0].1,
        vec!["1".to_string(), "2".to_string()],
        "a psnk row is registered like any other and belongs to the block it sits in"
    );
}

/// The block's extent starts at the group name and runs over the rows — it is
/// the declaration's span, not the name's (`mc_value_link` extends the name
/// node's length over the chain it links). Viz's group box anchors here.
#[test]
fn sem_pingroup__span_covers_the_declaration_block() {
    let uri = "/mcc/pin-group-span.mc";
    let src = r#"
component C1
{
    pins.bank = [
        1 = A
        2 = B
    ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let groups = groups_of(src, uri, "u1");
    let (_, _, span) = &groups[0];
    let text = &src[span.clone()];
    assert!(
        text.starts_with("bank"),
        "the span must start at the group name; got {text:?}"
    );
    assert!(
        text.contains("2 = B"),
        "the span must cover the block's rows, not just the name; got {text:?}"
    );
}

/// The empty block links nothing, so the same span expression degenerates to the
/// name alone — still inside the declaration, and the block is still an error.
#[test]
fn sem_pingroup__empty_block_span_covers_the_name() {
    let uri = "/mcc/pin-group-empty-span.mc";
    let src = r#"
component C1
{
    pins.bank = []
}

module main
{
    io VDD
    C1 u1
}
"#;
    let groups = groups_of(src, uri, "u1");
    let (_, pins, span) = &groups[0];
    assert!(pins.is_empty(), "an empty block registers nothing");
    assert_eq!(
        &src[span.clone()],
        "bank",
        "the empty arm links no chain, so its span is the name"
    );
}

// The name enters nothing.

/// The group name is not a pin name, an alias or a bus: it must not appear in
/// `names_to_id`, must not change the pin count, and must not be reported.
#[test]
fn sem_pingroup__name_enters_no_identity() {
    let uri = "/mcc/pin-group-identity.mc";
    let src = r#"
component C1
{
    pins.bank = [
        1 = A
        2 = B
    ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let def = def_of(src, uri, "u1");
    assert!(
        def.pins.names_to_id.get("bank").is_none(),
        "the group name must not be registered as a name"
    );
    assert_eq!(def.pins.pins.len(), 2, "the block declares two pins");
    assert_eq!(pin_count_of(src, uri, "u1"), 2);
    assert!(
        def.pins.names_to_id.contains_key("A") && def.pins.names_to_id.contains_key("B"),
        "pin names register as usual"
    );

    let diags = diags_of(src, uri);
    assert!(
        !diags.iter().any(|(_, _, msg)| msg.contains("bank")),
        "nothing may report the group name; got {diags:?}"
    );
}

// The two places a group could be lost.

/// A parameterized bank inside a group is not knowable at parse time, so it
/// registers no pin and the group would come out empty and mute. The block is
/// recorded, and the line carries the group so the instantiation side can
/// attribute the pins it materializes.
#[test]
fn sem_pingroup__dynamic_bank_carries_its_group() {
    let uri = "/mcc/pin-group-dynamic.mc";
    let src = r#"
component C1(cols::INT = 2)
{
    pins.bank = [
        io [1:cols] = D[1:cols]
    ]
}

module main
{
    io VDD
    C1 u1
}
"#;
    let def = def_of(src, uri, "u1");
    assert_eq!(group_names(src, uri, "u1"), vec!["bank".to_string()]);
    assert_eq!(
        def.pins.dynamic_pins.len(),
        1,
        "the row is a dynamic line; got {:?}",
        def.pins.dynamic_pins
    );
    assert_eq!(
        def.pins.dynamic_pins[0].group.as_deref(),
        Some("bank"),
        "the group must ride the line, not be dropped"
    );
}

/// A group inside a conditional body whose condition cannot be evaluated yet
/// lives in that body's own pin table — where its pins live — not in the
/// definition's. Nothing is dropped at parse time.
#[test]
fn sem_pingroup__conditional_body_keeps_its_own_group() {
    let uri = "/mcc/pin-group-conditional.mc";
    let src = r#"
component C1(mode::INT)
{
    pins = [ 1 = A ]

    if (mode == 1)
    {
        pins.bank = [ 2 = B ]
    }
}

module main
{
    io VDD
    C1(1) u1
}
"#;
    let def = def_of(src, uri, "u1");
    assert!(
        def.pins.groups.is_empty(),
        "an unevaluated branch's group does not join the definition's; got {:?}",
        def.pins.groups
    );
    let branches = def
        .cond_pins
        .first()
        .expect("the unevaluable condition is deferred");
    let branch_groups = &branches.if_blocks[0].1.groups;
    assert_eq!(
        branch_groups.len(),
        1,
        "the branch captures the group; got {branch_groups:?}"
    );
    assert_eq!(branch_groups[0].name, "bank");
    assert_eq!(branch_groups[0].pins, vec!["2".to_string()]);
}

/// When the condition IS evaluable at definition time, the branch's rows are
/// parsed straight into the definition, and so is its group.
#[test]
fn sem_pingroup__evaluated_condition_lands_in_the_definition() {
    let uri = "/mcc/pin-group-conditional-eval.mc";
    let src = r#"
component C1(mode::INT = 1)
{
    pins = [ 1 = A ]

    if (mode == 1)
    {
        pins.bank = [ 2 = B ]
    }
}

module main
{
    io VDD
    C1 u1
}
"#;
    let groups = groups_of(src, uri, "u1");
    assert_eq!(
        groups.iter().map(|g| g.0.as_str()).collect::<Vec<_>>(),
        vec!["bank"],
        "an evaluated branch declares its group on the definition; got {groups:?}"
    );
    assert_eq!(groups[0].1, vec!["2".to_string()]);
}
