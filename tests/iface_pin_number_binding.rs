// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Regression: interface members bound to a physical pin range must align by
// the interface pin number, not by the member declaration order. An interface
// that declares pins out of numeric order (`[1,5] = [VBUS, GND]`) previously
// bound GND to physical pin 2 (its declaration position) instead of pin 5.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

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

module main(ps GND)
{
    CONN sock
    sock.IF.GND -> GND
}
"#;

/// Name matching must still win over pin-number alignment: a physical pin that
/// is already named like an interface member (flash pin 6 = SCLK, pin 1 = _CS)
/// keeps its name; only unnamed data lanes fall back to position.
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

module main(ps GND)
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
fn mat_ifacebind__named_pin_wins_over_pin_number_alignment() {
    let paths = net_endpoint_paths(NAMED_PIN_WINS_SOURCE);
    assert!(
        paths.iter().any(|p| p.ends_with("f.6")),
        "SCLK member must bind to the already-named pin 6, got endpoints: {paths:#?}"
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("f.2")),
        "SCLK member must NOT grab pin 2 by interface pin number, got endpoints: {paths:#?}"
    );
}
