// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Regression: interface members bound to a physical pin range must align by
// the interface pin number, not by the member declaration order. An interface
// that declares pins out of numeric order (`[1,5] = [VBUS, GND]`) must bind GND
// to physical pin 5, not to pin 2 (its declaration position).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const OUT_OF_ORDER_IFACE_SOURCE: &str = r#"
interface MINI(role)
{
    pins = [
        [1,5] = [VBUS, GND]
        [2,3] = [DP, DM]
        4 = ID
    ]
    role Peripheral { name = "Peripheral" }
}

component CONN
{
    pins = [
        [1:5] = IF::MINI(Peripheral)
    ]
}

module main(psnk GND)
{
    CONN sock
    sock.IF.GND -> GND
}
"#;

/// D6 (ruled 2026-09-20): device-side pin names do NOT win — the name-first
/// branch was the leaf-name heuristic (identity-design.md §8.2 R10) and is
/// retired. A device pin already named like an interface member (flash pin 6
/// = SCLK, pin 1 = _CS) does not attract the member; the interface's own pin
/// number governs (SCLK is interface pin 2 → physical pin 2, whose device
/// name is SO — the two names coexist on the pin).
const NAMED_PIN_WINS_SOURCE: &str = r#"
interface SPI4(role)
{
    pins = [
        1 = CS
        2 = SCLK
        3 = MISO
        4 = MOSI
    ]
    role Slave { name = "Slave" }
}

component FLASH
{
    pins = [
        1 = _CS
        2 = SO
        5 = SI
        6 = SCLK
        [1,2,5,6] = SP::SPI4(Slave)
    ]
}

module main(psnk GND)
{
    FLASH f
    f.SP.SCLK -> GND
}
"#;

/// Collect every endpoint path (e.g. "main.sock.5") present on any net.
fn net_endpoint_paths(source: &str) -> Vec<String> {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/iface-pin-number-binding.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_inst, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");

    let mut paths = Vec::new();
    for net in table.get_nets() {
        for &pid in &net.points {
            if let Some(e) = table.get_entry(pid) {
                paths.push(e.path.clone());
            }
        }
    }

    paths
}

#[test]
fn mat_ifacebind__out_of_order_pins_bind_by_pin_number() {
    let paths = net_endpoint_paths(OUT_OF_ORDER_IFACE_SOURCE);
    assert!(
        paths.iter().any(|p| p.ends_with("sock.5")),
        "GND member must bind to physical pin 5, got endpoints: {paths:#?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("sock.2")),
        "GND member must NOT bind to physical pin 2, got endpoints: {paths:#?}"
    );
}

#[test]
fn mat_ifacebind__device_pin_names_do_not_win__interface_pin_number_governs() {
    let paths = net_endpoint_paths(NAMED_PIN_WINS_SOURCE);
    assert!(
        paths.iter().any(|p| p.ends_with("f.2")),
        "SCLK member must bind by its interface pin number (2), got endpoints: {paths:#?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("f.6")),
        "SCLK member must NOT be attracted by the device's own pin-6 name, got endpoints: {paths:#?}"
    );
}
