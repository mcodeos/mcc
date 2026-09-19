// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: a module's ports come out in WRITTEN order (CIMP U119).
//
// The component side has carried an explicit written-order field all along
// (`McPins.decl_order`); the module side had only the name order of `insts` (a
// `BTreeMap`), so every surface that listed ports showed them alphabetically.
// The order the author wrote is what a reader of the interface expects, and it
// is the order the positional binder already used (it sorted the candidate
// ports by their declaration span) -- so the two faces disagreed wherever a
// module declared its ports in an order other than the alphabetical one.
//
// The fixture is the same module in both orders: one written as `zeta, alpha,
// mike`, i.e. deliberately NOT the order the name map hands out. What is locked
// here is the ORDER only -- the two accessors must always describe the same set,
// which the first case checks, because an "order fix" that loses a port would
// otherwise look like a pass.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{InstKind, McIds, McURI};

/// Ports declared `zeta`, `alpha`, `mike`: written order and name order differ,
/// and neither is a rotation of the other.
const SOURCE: &str = r#"
module main(io zeta, io alpha, io mike)
{
    zeta -> alpha
}
"#;

const URI: &str = "/mcc/u119-written-order.mc";

/// Written order, as the source spells it.
const WRITTEN: [&str; 3] = ["zeta", "alpha", "mike"];

/// Name order, as the `BTreeMap` hands it out (the order before U119).
const BY_NAME: [&str; 3] = ["alpha", "mike", "zeta"];

/// The `main` module of the fixture, after a parse.
fn main_module() -> mcc::McModule {
    let _lock = common::lock();
    common::reset();
    common::load_string(URI, SOURCE);
    let _ = mcc::mcc_diagnose_all();
    mcc::definition_space()
        .workspace_modules()
        .into_iter()
        .find(|(sn, _)| sn.ident.to_string() == "main")
        .map(|(_, m)| (*m).clone())
        .expect("the fixture declares `main`")
}

/// The ports of the built board's own `main`, in the flat table's own order
/// (the table is keyed by id, and port ids are allocated as the module's port
/// table is walked) -- i.e. what the drawing and the port listings read.
fn flat_port_paths() -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = URI.to_string();
    common::load_string(URI, SOURCE);
    let ident = McIds::from("main");
    let (_inst, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&ident, &uri, 1).expect("the fixture builds");

    let mut out: Vec<String> = Vec::new();
    for (_, entry) in table.iter() {
        // Direct children of the top module only: a nested module's ports would
        // carry a further dot (the fixture has none, and this keeps it so).
        if entry.kind == InstKind::Port && entry.path.matches('.').count() == 1 {
            out.push(entry.path.clone());
        }
    }
    out
}

/// The written order is the accessor's order, and the name-order accessor is
/// unchanged -- the two differ here, which is the whole point of the fixture.
#[test]
fn u119__ports_iterate_in_written_order_not_name_order() {
    let m = main_module();
    let written: Vec<String> = m
        .insts
        .iter_ports_in_decl_order()
        .map(|(n, _)| n.to_string())
        .collect();
    let by_name: Vec<String> = m.insts.iter_ports().map(|(n, _)| n.to_string()).collect();

    assert_eq!(written, WRITTEN, "the port table iterates in written order");
    assert_eq!(
        by_name, BY_NAME,
        "the name-order accessor still hands out the map's order"
    );
}

/// The two accessors describe the SAME SET: reordering is not allowed to drop
/// or invent a port.
#[test]
fn u119__the_two_port_orders_describe_the_same_set() {
    let m = main_module();
    let mut written: Vec<String> = m
        .insts
        .iter_ports_in_decl_order()
        .map(|(n, _)| n.to_string())
        .collect();
    let mut by_name: Vec<String> = m.insts.iter_ports().map(|(n, _)| n.to_string()).collect();
    written.sort();
    by_name.sort();

    assert_eq!(
        written, by_name,
        "written order and name order must hold the same ports"
    );
    assert_eq!(by_name.len(), WRITTEN.len(), "and no port is missing");
}

/// The instantiated port table -- what the drawing, the port listings and the
/// per-instance dumps read -- follows the written order too.
#[test]
fn u119__the_instantiated_port_table_follows_written_order() {
    let paths = flat_port_paths();
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.trim_start_matches("main.").to_string())
        .collect();

    assert_eq!(
        names, WRITTEN,
        "the flat table's port rows come out in written order: {paths:?}"
    );
}

/// The project-wide port listing (`list ports`) is grouped by module and runs
/// in written order inside each module, so the author's order survives the trip
/// through the CLI.
#[test]
fn u119__the_port_listing_runs_in_written_order_per_module() {
    let _lock = common::lock();
    common::reset();
    common::load_string(URI, SOURCE);
    let _ = mcc::mcc_diagnose_all();

    let names: Vec<String> = mcc::mcb_iter_ports()
        .into_iter()
        .filter(|(_, _, module, _)| module == "main")
        .map(|(name, _, _, _)| name)
        .collect();

    assert_eq!(names, WRITTEN, "`list ports` runs in written order");
}
