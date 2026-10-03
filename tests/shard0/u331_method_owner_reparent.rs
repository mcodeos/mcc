// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U331: a func-created instance carries its creation-time owner
//! (`InstOrigin::FuncCall.owner` = the receiver whose method body expanded to
//! it), and the P8-1 pass-2 re-parent keys on that owner — never on
//! `fn_name`, which is the class name at creation and is rebranded to the
//! dispatched method name (M0-B-E.1).
//!
//! Pre-fix failure faces locked here:
//!
//! 1. `PB.func Cap` body's `CX().Cap([VA, VB])` produced 0 nets, silently:
//!    the re-parent keyed on the method name, hit any Declared component
//!    declaring a same-named func (`func Cap`), moved the instance to
//!    `main.b._CX1`, and the bare connection points (`_CX1.1`) recorded
//!    during expansion resolved nowhere. Renaming the LOCAL func (`Capx`)
//!    revived the corpus — the drop rode a name, not the structure. The fix
//!    keys the re-parent on the recorded receiver, and the flat projection
//!    keeps the pre-re-parent spelling resolvable (alias entries), so both
//!    spellings land on the same nets — sick and renamed forms are now
//!    identical.
//! 2. A method body whose statements are ALL conditional parses into
//!    `conds` with `stmts` empty; `run_component_method` bailed on
//!    `stmts.is_empty()` BEFORE the conds block, so the if/else body
//!    produced zero connections while the same body with one plain statement
//!    worked. The bail now requires the body to have neither.
//!
//! Fixtures are self-contained (inline classes, no `use mcode`) — the live
//! system root's RES lost `func Pullup` to library drift, which would couple
//! these locks to `~/.mcode` state.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

#[derive(Debug, Default)]
struct Probe {
    /// (sorted point paths) per connection, in creation order
    nets: Vec<Vec<String>>,
    diags: Vec<u32>,
}

fn probe(uri_path: &str, source: &str) -> Probe {
    // The workspace tables are process-global — the whole probe runs under
    // the file lock, so parallel tests never interleave clear/init/build.
    let _lock = common::lock();
    let system_root = mcc::cli::datadir::data_root();
    mcc::mcc_clear_workspace();
    mcc::mcc_set_system_root(&system_root);
    mcc::mcc_init();
    let uri: McURI = uri_path.to_string();
    mcc::mcc_load_from_string(&uri, source);
    let mut p = Probe::default();
    if let Ok((inst, _arena, _store, _net_store)) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri)
    {
        for c in inst.connections.iter() {
            let mut pts: Vec<String> = c.points.iter().map(|x| x.path.clone()).collect();
            pts.sort();
            p.nets.push(pts);
        }
    }
    p.diags = mcc::mcc_diagnose(&uri).iter().map(|d| d.code).collect();
    p
}

/// The two-pin inline class both fixtures construct through a method call:
/// `Cap` series-wires the created instance between the two actuals, `Pullup`
/// ties each actual to one pin.
const CLASSES: &str = r#"
component CX
{
    pins = [
        1 = 1, "T1"
        2 = 2, "T2"
    ]

    func Cap([net1, net2])
    {
        net1 - this - net2
    }
}

component RX
{
    pins = [
        1 = 1, "T1"
        2 = 2, "T2"
    ]

    func Pullup([node, supply])
    {
        node - this.1
        supply - this.2
    }
}
"#;

/// The sick face: a local func whose body creates an instance through a
/// method call on the inline construction.
const SICK_SOURCE: &str = r#"
component CX
{
    pins = [
        1 = 1, "T1"
        2 = 2, "T2"
    ]

    func Cap([net1, net2])
    {
        net1 - this - net2
    }
}

component PB
{
    pins = [
        [1, 2] = [VA, VB]
    ]

    func Cap()
    {
        CX().Cap([VA, VB])
    }
}

module main
{
    PB b
    b.Cap()
}
"#;

/// The twin: only the LOCAL func is renamed (`Cap` → `Capx`). Pre-fix this
/// form lived while `SICK_SOURCE` died — the drop rode the name.
const RENAMED_SOURCE: &str = r#"
component CX
{
    pins = [
        1 = 1, "T1"
        2 = 2, "T2"
    ]

    func Cap([net1, net2])
    {
        net1 - this - net2
    }
}

component PB
{
    pins = [
        [1, 2] = [VA, VB]
    ]

    func Capx()
    {
        CX().Cap([VA, VB])
    }
}

module main
{
    PB b
    b.Capx()
}
"#;

/// The starved face: a method body whose statements are ALL conditional.
const CONDS_ONLY_SOURCE: &str = r#"
component RX
{
    pins = [
        1 = 1, "T1"
        2 = 2, "T2"
    ]

    func Pullup([node, supply])
    {
        node - this.1
        supply - this.2
    }
}

component PB
{
    pins = [
        [1, 2, 3] = [VA, VB, A0]
    ]

    func Address(address)
    {
        if (address & 0x01) RX().Pullup([A0, VA]) else RX().Pullup([A0, VB])
    }
}

module main
{
    net P1
    net P2
    PB b
    b.Address(P1)
}
"#;

/// The presence twin: the same body with one plain statement ahead of the if
/// — the shape that always worked.
const MIXED_BODY_SOURCE: &str = r#"
component RX
{
    pins = [
        1 = 1, "T1"
        2 = 2, "T2"
    ]

    func Pullup([node, supply])
    {
        node - this.1
        supply - this.2
    }
}

component PB
{
    pins = [
        [1, 2, 3] = [VA, VB, A0]
    ]

    func Address(address)
    {
        RX().Pullup([A0, VA])
        if (address & 0x01) RX().Pullup([A0, VB]) else RX().Pullup([A0, VB])
    }
}

module main
{
    net P1
    net P2
    PB b
    b.Address(P1)
}
"#;

fn sorted_nets(p: &Probe) -> Vec<Vec<String>> {
    let mut nets = p.nets.clone();
    nets.sort();
    nets
}

/// The re-parented instance keeps BOTH of its nets — the flat-spelled
/// connection points recorded during expansion resolve through the alias
/// entries the pass-2 projection registers.
#[test]
fn u331__method_created_instance_keeps_both_nets() {
    let p = probe("/mcc/u331-f1a.mc", SICK_SOURCE);
    assert!(
        !p.nets.is_empty(),
        "the method-created instance must land both nets, got {:?}",
        p.nets
    );
    assert_eq!(
        sorted_nets(&p),
        vec![
            vec![String::from("_CX1.1"), String::from("b.1")],
            vec![String::from("_CX1.2"), String::from("b.2")]
        ],
        "the net shape must be the healthy series form"
    );
    assert!(
        !p.diags.contains(&mcc::errcodes::NET_DROPPED_STATEMENT),
        "a healthy two-net corpus must not report a dropped statement, got {:?}",
        p.diags
    );
}

/// The netlist may not depend on the local func's spelling: sick and renamed
/// forms produce the same sorted net multiset.
#[test]
fn u331__owner_is_the_receiver_not_the_func_name() {
    let sick = probe("/mcc/u331-f1a.mc", SICK_SOURCE);
    let renamed = probe("/mcc/u331-f1c.mc", RENAMED_SOURCE);
    assert_eq!(
        sorted_nets(&sick),
        sorted_nets(&renamed),
        "renaming the local func must not change the netlist"
    );
}

/// The flat projection places the instance under the recorded receiver
/// (`main.b._CX1`), and both the hierarchical and the flat spelling of its
/// pins resolve.
#[test]
fn u331__flat_projection_reparents_under_the_receiver() {
    let _lock = common::lock();
    let system_root = mcc::cli::datadir::data_root();
    mcc::mcc_clear_workspace();
    mcc::mcc_set_system_root(&system_root);
    mcc::mcc_init();
    let uri: McURI = "/mcc/u331-f1a-flat.mc".to_string();
    mcc::mcc_load_from_string(&uri, SICK_SOURCE);
    let (_tree, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000)
        .expect("flat build");
    for path in ["main.b._CX1", "main.b._CX1.1", "main.b._CX1.2"] {
        assert!(
            table.get_id_by_path(path).is_some(),
            "{path} must be registered (re-parented under the receiver)"
        );
        // The pre-re-parent flat spelling stays resolvable (alias entry) —
        // the expansion recorded its connection points against it.
        let flat = path.replacen("main.b.", "main.", 1);
        assert!(
            table.get_id_by_path(&flat).is_some(),
            "the flat spelling {flat} must stay resolvable after the re-parent"
        );
    }
}

/// A method body whose statements are ALL conditional runs its conds — the
/// starved channel is open (the if-arm lands its pull).
#[test]
fn u331__conds_only_body_no_longer_starves() {
    let p = probe("/mcc/u331-f3a.mc", CONDS_ONLY_SOURCE);
    assert_eq!(
        sorted_nets(&p),
        vec![
            vec![String::from("_RX1.1"), String::from("b.3")],
            vec![String::from("_RX1.2"), String::from("b.2")]
        ],
        "the conditional body must land its branch's pull nets"
    );
}

/// Presence lock: the mixed body (plain statement + if) keeps its four
/// connections — the ③ fix must not change the shape that always worked.
#[test]
fn u331__mixed_body_stays_healthy() {
    let p = probe("/mcc/u331-f3g.mc", MIXED_BODY_SOURCE);
    assert_eq!(
        p.nets.len(),
        4,
        "the mixed body keeps its four connections (two per pull)"
    );
}
