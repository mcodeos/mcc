// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: a SPICE device letter comes from the component's *declared*
// class (the refdes prefix table), never from a spelled name. `XR9` is the lock
// that matters: the instance is called `R9` while its class has no prefix row,
// so a letter read off the name would print `RR9`.
//
// The submodule rows lock the other half: the class lookup joins through the
// instance's hierarchical path, so a part inside a submodule resolves too.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const URI: &str = "/mcc/export-spice.mc";

/// Two registered classes (`RES` / `CAP`), one class with no prefix row
/// (`MYSTERY`), a part of each inside a submodule, and two at the top.
const SOURCE: &str = r#"
component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}
component CAP
{
    pins = [
        1 = 1
        2 = 2
    ]
}
component MYSTERY
{
    pins = [
        1 = 1
        2 = 2
    ]
}
module inner(psnk GND)
{
    RES R1
    R1.1 -> GND
    R1.2 -> MID
    CAP C1
    C1.1 -> GND
    C1.2 -> MID
    MYSTERY R9
    R9.1 -> GND
    R9.2 -> MID
}
module main(psnk GND)
{
    inner U_IN
    U_IN.GND -> GND
    CAP C2
    C2.1 -> GND
    C2.2 -> TOPMID
    RES R2
    R2.1 -> TOPMID
    R2.2 -> GND
}
"#;

/// Freeze the circuit and build the SPICE netlist. The caller must hold
/// [`common::lock`] for its whole body: the registry is global state, so a
/// sibling `reset()` landing between the load and the build would strip the
/// class defs.
fn spice() -> (String, usize) {
    common::reset();
    let uri: McURI = URI.to_string();
    common::load_string(URI, SOURCE);
    let ident = McIds::from("main");
    let (tree, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&ident, &uri, 1).expect("pass2_flat failed");
    let (text, _, count) = mcc::export::spice::build_spice(&table, "main");
    (text, count)
}

/// The one row for `device` (`{prefix}{name} {net} {net}`), matched by device
/// name: these tests are about the letter prefix and the pads, not the file's
/// line order — that is `tests/shard7/product_order.rs`'s subject, and the
/// emitter keys on a `BTreeMap` so the order is the input's.
fn row<'a>(text: &'a str, device: &str) -> &'a str {
    text.lines()
        .find(|line| line.starts_with(device) && line.as_bytes().get(device.len()) == Some(&b' '))
        .unwrap_or_else(|| panic!("no SPICE row for {device} in:\n{text}"))
}

#[test]
fn export_spice_refdes__letter_comes_from_the_class_not_the_name() {
    let _lock = common::lock();
    let (text, count) = spice();

    assert_eq!(row(&text, "RR2"), "RR2 GND TOPMID");
    assert_eq!(row(&text, "CC2"), "CC2 GND TOPMID");
    assert_eq!(row(&text, "XR9"), "XR9 GND MID");
    assert!(
        !text.lines().any(|line| line.starts_with("RR9 ")),
        "a letter was read off the instance name:\n{text}"
    );
    assert_eq!(count, 5, "one row per two-terminal part:\n{text}");
}

#[test]
fn export_spice_refdes__a_part_inside_a_submodule_joins_its_class_too() {
    let _lock = common::lock();
    let (text, _) = spice();

    // Both parts live in `main.U_IN`; their class is read back through that
    // hierarchical path, while the printed name stays the module-local one.
    assert_eq!(row(&text, "RR1"), "RR1 GND MID");
    assert_eq!(row(&text, "CC1"), "CC1 GND MID");
}
