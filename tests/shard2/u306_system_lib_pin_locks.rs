// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U306 — system-library real-file pin-table locks (implementation-audit C3
//! buckets ②③④).
//!
//! Batch b4031 locked the component half (whole expanded tables of real
//! components); this file locks the half the audit found at zero: the **real
//! installed system library** (`~/.mcode/mcode`, loaded here through the hbl
//! fixture's `[dependencies]` chain — the same route the CLI takes). Every
//! interface role table the library promises — I2C.SMBUS, the SPI family,
//! the full UART role set, GPIO, XTAL, and the conditional DC shape — plus
//! the connector range forms (`HDR.1X9` `1:9`, `HDR.2X9` `1:18 =
//! R[1:2]C[1:9]`) is pinned role-for-role against the same read face as
//! `mcc show pins` (`McPins.pin_id_to_names`).
//!
//! Audit naming corrections recorded with the locks: the three-wire SPI
//! interface is spelled `SPI.3` (`SPI.3WIRE` survives only in the ifs.mc
//! import comment), and the dc.mc source carries three conditional headers
//! (`volt < 0V` / `volt > 0V` / `else`) — the def face keeps the ELSE default
//! branch (`McInterface::parse_first_cond_pins`), so `DC` locks the ELSE
//! table (`1 = VCC`, `2 = GND`).
//!
//! Direction words map onto `IOType` as usual (`in`→In, `out`→Out,
//! `io`→InOut, bare/anonymous→None); peer-only roles (GPIO
//! Provider/Consumer, XTAL Oscillator/Resonator) lock as empty tables so a
//! stray pin sneaking into them trips the count check.
//!
//! NOTE: like `u289_component_pin_tables.rs`, these tests share global mcc
//! state; `common::lock()` serializes them.

use crate::common;

use mcc::{IOType, McCMIE, McIds, McURI};
use std::path::PathBuf;

/// One expected pin row: id, direction, full alias set (any order).
type Row<'a> = (&'a str, IOType, &'a [&'a str]);

/// One expected role table: role name plus its rows, in declaration order.
type RoleTable<'a> = (&'a str, &'a [Row<'a>]);

/// Load the hbl fixture project exactly as `u289_component_pin_tables.rs`
/// does — the manifest `[dependencies]` chain loads the real installed system
/// library (`mcode = "*"`), then the project.
fn load_hbl() {
    let _lock = common::lock();
    common::reset();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    mcc::mcc_set_project_root(&root);
    let libs = mcc::cli::loadctx::resolve_load_context(Some(&root), &[]);
    mcc::cli::loadctx::load_all(&mcc::cli::loadctx::LoadContext::from_resolved(
        libs.lib_names(),
    ));
    let entry = std::fs::canonicalize(root.join("src/hbl.mc")).expect("canonicalize hbl.mc");
    mcc::mcc_load_project(&McURI::from(entry.to_string_lossy().to_string()));
}

/// Canonical URI of one real library file, e.g. `lib_uri("ifs/i2c.mc")` —
/// `get_kind_def` matches name + defining-URI, so a lock only hits the
/// installed file the loader actually read (macOS /var → /private/var).
fn lib_uri(rel: &str) -> McURI {
    let p = std::fs::canonicalize(mcc::cli::datadir::mcode_dir().join(rel))
        .unwrap_or_else(|e| panic!("library file mcode/{rel} must exist: {e}"));
    McURI::from(p.to_string_lossy().to_string())
}

/// The def-face table of one interface: (role-less table, role tables in
/// declaration order). Rows carry the same per-pin sorted alias sets the
/// `mcc show pins` face renders.
fn iface_tables(
    name: &str,
    uri: &McURI,
) -> (
    Vec<(String, IOType, Vec<String>)>,
    Vec<(String, Vec<(String, IOType, Vec<String>)>)>,
) {
    match mcc::get_kind_def(2, &McIds::from(name), uri) {
        Some(McCMIE::Interface(i)) => {
            let table = |pins: &mcc::McPins| {
                let mut rows: Vec<(String, IOType, Vec<String>)> = pins
                    .pins
                    .iter()
                    .map(|(pid, pin)| {
                        let mut sorted = pin.names.clone();
                        sorted.sort();
                        (pid.clone(), pin.iotype.clone(), sorted)
                    })
                    .collect();
                rows.sort_by(|a, b| mcc::pin_id_cmp(&a.0, &b.0));
                rows
            };
            let roles = i
                .roles
                .iter()
                .map(|r| (r.name.to_string(), table(&r.pins)))
                .collect();
            (table(&i.pins), roles)
        }
        _ => panic!("interface def '{name}' not found"),
    }
}

/// Assert one table row-for-row: no pin missing, no pin extra, every alias
/// set and direction exact.
fn assert_rows(context: &str, table: &[(String, IOType, Vec<String>)], expected: &[Row]) {
    assert_eq!(
        table.len(),
        expected.len(),
        "{context}: pin count drifted (got {table:?})"
    );
    for ((pid, io, names), (want_pid, want_io, want_names)) in table.iter().zip(expected) {
        assert_eq!(pid, want_pid, "{context}: pin id set drifted");
        assert_eq!(io, want_io, "{context}.{pid}: direction drifted");
        assert_eq!(
            names, want_names,
            "{context}.{pid}: alias set drifted (redistribution between pins?)"
        );
    }
}

/// Assert a whole interface: role-less table plus every role table, in
/// declaration order, no role missing and none extra.
fn assert_iface(name: &str, uri: &McURI, roleless: &[Row], roles: &[RoleTable]) {
    let (table, role_tables) = iface_tables(name, uri);
    assert_rows(&format!("{name} (role-less)"), &table, roleless);
    assert_eq!(
        role_tables.len(),
        roles.len(),
        "{name}: role count drifted (got {:?})",
        role_tables.iter().map(|(n, _)| n).collect::<Vec<_>>()
    );
    for ((got_name, got_rows), (want_name, want_rows)) in role_tables.iter().zip(roles) {
        assert_eq!(got_name, want_name, "{name}: role order/name drifted");
        assert_rows(&format!("{name}::{got_name}"), got_rows, want_rows);
    }
}


/// The `pin_id → (iotype, sorted alias set)` table of one component
/// definition, read off `McPins.pins` — the same face
/// `u289_component_pin_tables.rs` locks.
fn component_table(name: &str, uri: &McURI) -> Vec<(String, IOType, Vec<String>)> {
    match mcc::get_component_def(&McIds::from(name), uri) {
        Some(McCMIE::Component(c)) => {
            let mut rows: Vec<(String, IOType, Vec<String>)> = c
                .pins
                .pins
                .iter()
                .map(|(pid, pin)| {
                    let mut sorted = pin.names.clone();
                    sorted.sort();
                    (pid.clone(), pin.iotype.clone(), sorted)
                })
                .collect();
            rows.sort_by(|a, b| mcc::pin_id_cmp(&a.0, &b.0));
            rows
        }
        _ => panic!("component def '{name}' not found"),
    }
}
#[test]
fn u306__i2c_family_pin_tables() {
    load_hbl();
    let uri = lib_uri("ifs/i2c.mc");
    assert_iface(
        "I2C",
        &uri,
        &[
            ("1", IOType::None, &["SCL"]),
            ("2", IOType::None, &["SDA"]),
        ],
        &[
            (
                "Master",
                &[
                    ("1", IOType::Out, &["SCL"]),
                    ("2", IOType::InOut, &["SDA"]),
                ],
            ),
            (
                "Slave",
                &[
                    ("1", IOType::In, &["SCL"]),
                    ("2", IOType::InOut, &["SDA"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/i2c.mc");
    assert_iface(
        "I2C.SMBUS",
        &uri,
        &[
            ("1", IOType::None, &["SCL"]),
            ("2", IOType::None, &["SDA"]),
            ("3", IOType::None, &["ALERT"]),
        ],
        &[
            (
                "Host",
                &[
                    ("1", IOType::Out, &["SCL"]),
                    ("2", IOType::InOut, &["SDA"]),
                    ("3", IOType::In, &["ALERT"]),
                ],
            ),
            (
                "Slave",
                &[
                    ("1", IOType::In, &["SCL"]),
                    ("2", IOType::InOut, &["SDA"]),
                    ("3", IOType::Out, &["ALERT"]),
                ],
            ),
        ],
    );
}

#[test]
fn u306__spi_family_pin_tables() {
    load_hbl();
    let uri = lib_uri("ifs/spi.mc");
    assert_iface(
        "SPI",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
            ("4", IOType::None, &[]),
        ],
        &[
            (
                "Master",
                &[
                    ("1", IOType::Out, &["SCLK"]),
                    ("2", IOType::Out, &["MOSI"]),
                    ("3", IOType::In, &["MISO"]),
                    ("4", IOType::Out, &["CS"]),
                ],
            ),
            (
                "Slave",
                &[
                    ("1", IOType::In, &["SCLK"]),
                    ("2", IOType::In, &["SI"]),
                    ("3", IOType::Out, &["SO"]),
                    ("4", IOType::In, &["CS"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/spi.mc");
    assert_iface(
        "SPI.3",
        &uri,
        &[
            ("1", IOType::None, &["CS"]),
            ("2", IOType::None, &["SCLK"]),
            ("3", IOType::None, &["SDA"]),
        ],
        &[
            (
                "Master",
                &[
                    ("1", IOType::Out, &["CS"]),
                    ("2", IOType::Out, &["SCLK"]),
                    ("3", IOType::InOut, &["SDA"]),
                ],
            ),
            (
                "Slave",
                &[
                    ("1", IOType::In, &["CS"]),
                    ("2", IOType::In, &["SCLK"]),
                    ("3", IOType::InOut, &["SDA"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/spi.mc");
    assert_iface(
        "SPI.QUAD",
        &uri,
        &[
            ("1", IOType::None, &["CS"]),
            ("2", IOType::None, &["SCLK"]),
            ("3", IOType::None, &["IO0"]),
            ("4", IOType::None, &["IO1"]),
            ("5", IOType::None, &["IO2"]),
            ("6", IOType::None, &["IO3"]),
        ],
        &[
            (
                "Master",
                &[
                    ("1", IOType::Out, &["CS"]),
                    ("2", IOType::Out, &["SCLK"]),
                    ("3", IOType::InOut, &["IO0"]),
                    ("4", IOType::InOut, &["IO1"]),
                    ("5", IOType::InOut, &["IO2"]),
                    ("6", IOType::InOut, &["IO3"]),
                ],
            ),
            (
                "Slave",
                &[
                    ("1", IOType::In, &["CS"]),
                    ("2", IOType::In, &["SCLK"]),
                    ("3", IOType::InOut, &["IO0"]),
                    ("4", IOType::InOut, &["IO1"]),
                    ("5", IOType::InOut, &["IO2"]),
                    ("6", IOType::InOut, &["IO3"]),
                ],
            ),
        ],
    );
}

#[test]
fn u306__gpio_xtal_dc_pin_tables() {
    load_hbl();
    let uri = lib_uri("ifs/gpio.mc");
    assert_iface(
        "GPIO",
        &uri,
        &[
            ("1", IOType::None, &[]),
        ],
        &[
            (
                "Provider",
                &[]
            ),
            (
                "Consumer",
                &[]
            ),
        ],
    );
    let uri = lib_uri("ifs/xtal.mc");
    assert_iface(
        "XTAL",
        &uri,
        &[
            ("1", IOType::None, &["X1"]),
            ("2", IOType::None, &["X2"]),
        ],
        &[
            (
                "Oscillator",
                &[]
            ),
            (
                "Resonator",
                &[]
            ),
        ],
    );
    let uri = lib_uri("ifs/dc.mc");
    assert_iface(
        "DC",
        &uri,
        &[
            ("1", IOType::None, &["VCC"]),
            ("2", IOType::None, &["GND"]),
        ],
        &[

        ],
    );
}

#[test]
fn u306__uart_ttl_pin_tables() {
    load_hbl();
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.TTL",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
        ],
        &[
            (
                "DCE",
                &[
                    ("1", IOType::Out, &["TX"]),
                    ("2", IOType::In, &["RX"]),
                ],
            ),
            (
                "DCE_1V8",
                &[
                    ("1", IOType::Out, &["TX"]),
                    ("2", IOType::In, &["RX"]),
                ],
            ),
            (
                "DCE_3V3",
                &[
                    ("1", IOType::Out, &["TX"]),
                    ("2", IOType::In, &["RX"]),
                ],
            ),
            (
                "DCE_5V",
                &[
                    ("1", IOType::Out, &["TX"]),
                    ("2", IOType::In, &["RX"]),
                ],
            ),
            (
                "DTE",
                &[
                    ("1", IOType::In, &["RX"]),
                    ("2", IOType::Out, &["TX"]),
                ],
            ),
            (
                "DTE_1V8",
                &[
                    ("1", IOType::In, &["RX"]),
                    ("2", IOType::Out, &["TX"]),
                ],
            ),
            (
                "DTE_3V3",
                &[
                    ("1", IOType::In, &["RX"]),
                    ("2", IOType::Out, &["TX"]),
                ],
            ),
            (
                "DTE_5V",
                &[
                    ("1", IOType::In, &["RX"]),
                    ("2", IOType::Out, &["TX"]),
                ],
            ),
        ],
    );
}

#[test]
fn u306__uart_rs232_pin_tables() {
    load_hbl();
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS232.3",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
        ],
        &[
            (
                "DCE",
                &[
                    ("1", IOType::In, &["RXD"]),
                    ("2", IOType::Out, &["TXD"]),
                    ("3", IOType::None, &["GND"]),
                ],
            ),
            (
                "DTE",
                &[
                    ("1", IOType::Out, &["TXD"]),
                    ("2", IOType::In, &["RXD"]),
                    ("3", IOType::None, &["GND"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS232.5",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
            ("4", IOType::None, &[]),
            ("5", IOType::None, &[]),
        ],
        &[
            (
                "DCE",
                &[
                    ("1", IOType::In, &["RXD"]),
                    ("2", IOType::Out, &["TXD"]),
                    ("3", IOType::None, &["GND"]),
                    ("4", IOType::In, &["RTS"]),
                    ("5", IOType::Out, &["CTS"]),
                ],
            ),
            (
                "DTE",
                &[
                    ("1", IOType::Out, &["TXD"]),
                    ("2", IOType::In, &["RXD"]),
                    ("3", IOType::None, &["GND"]),
                    ("4", IOType::In, &["CTS"]),
                    ("5", IOType::Out, &["RTS"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS232.9",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
            ("4", IOType::None, &[]),
            ("5", IOType::None, &[]),
            ("6", IOType::None, &[]),
            ("7", IOType::None, &[]),
            ("8", IOType::None, &[]),
            ("9", IOType::None, &[]),
        ],
        &[
            (
                "DCE",
                &[
                    ("1", IOType::Out, &["DCD"]),
                    ("2", IOType::In, &["RXD"]),
                    ("3", IOType::Out, &["TXD"]),
                    ("4", IOType::In, &["DTR"]),
                    ("5", IOType::None, &["GND"]),
                    ("6", IOType::Out, &["DSR"]),
                    ("7", IOType::In, &["RTS"]),
                    ("8", IOType::Out, &["CTS"]),
                    ("9", IOType::Out, &["RI"]),
                ],
            ),
            (
                "DTE",
                &[
                    ("1", IOType::In, &["DCD"]),
                    ("2", IOType::Out, &["TXD"]),
                    ("3", IOType::In, &["RXD"]),
                    ("4", IOType::In, &["DSR"]),
                    ("5", IOType::None, &["GND"]),
                    ("6", IOType::Out, &["DTR"]),
                    ("7", IOType::In, &["CTS"]),
                    ("8", IOType::Out, &["RTS"]),
                    ("9", IOType::In, &["RI"]),
                ],
            ),
        ],
    );
}

#[test]
fn u306__uart_rs422_rs423_rs449_pin_tables() {
    load_hbl();
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS422",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
        ],
        &[
            (
                "Transmitter",
                &[
                    ("1", IOType::Out, &["A"]),
                    ("2", IOType::Out, &["B"]),
                    ("3", IOType::None, &["GND"]),
                ],
            ),
            (
                "Receiver",
                &[
                    ("1", IOType::In, &["A"]),
                    ("2", IOType::In, &["B"]),
                    ("3", IOType::None, &["GND"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS422.2",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
        ],
        &[
            (
                "Transmitter",
                &[
                    ("1", IOType::Out, &["A"]),
                    ("2", IOType::Out, &["B"]),
                ],
            ),
            (
                "Receiver",
                &[
                    ("1", IOType::In, &["A"]),
                    ("2", IOType::In, &["B"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS423",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
            ("4", IOType::None, &[]),
            ("5", IOType::None, &[]),
        ],
        &[
            (
                "DCE",
                &[
                    ("1", IOType::In, &["RXD"]),
                    ("2", IOType::Out, &["TXD"]),
                    ("3", IOType::None, &["GND"]),
                    ("4", IOType::In, &["RTS"]),
                    ("5", IOType::Out, &["CTS"]),
                ],
            ),
            (
                "DTE",
                &[
                    ("1", IOType::Out, &["TXD"]),
                    ("2", IOType::In, &["RXD"]),
                    ("3", IOType::None, &["GND"]),
                    ("4", IOType::In, &["CTS"]),
                    ("5", IOType::Out, &["RTS"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS449",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
            ("4", IOType::None, &[]),
            ("5", IOType::None, &[]),
            ("6", IOType::None, &[]),
            ("7", IOType::None, &[]),
            ("8", IOType::None, &[]),
            ("9", IOType::None, &[]),
            ("10", IOType::None, &[]),
            ("11", IOType::None, &[]),
            ("12", IOType::None, &[]),
            ("13", IOType::None, &[]),
            ("14", IOType::None, &[]),
            ("15", IOType::None, &[]),
            ("16", IOType::None, &[]),
        ],
        &[
            (
                "DCE",
                &[
                    ("1", IOType::In, &["SD"]),
                    ("2", IOType::Out, &["RD"]),
                    ("3", IOType::In, &["RS"]),
                    ("4", IOType::Out, &["CS"]),
                    ("5", IOType::Out, &["DR"]),
                    ("6", IOType::Out, &["CD"]),
                    ("7", IOType::In, &["TM"]),
                    ("8", IOType::Out, &["TT"]),
                    ("9", IOType::Out, &["RT"]),
                    ("10", IOType::None, &["SG"]),
                    ("11", IOType::In, &["SD2"]),
                    ("12", IOType::Out, &["RD2"]),
                    ("13", IOType::In, &["RS2"]),
                    ("14", IOType::Out, &["CS2"]),
                    ("15", IOType::Out, &["DR2"]),
                    ("16", IOType::Out, &["CD2"]),
                ],
            ),
            (
                "DTE",
                &[
                    ("1", IOType::In, &["RD"]),
                    ("2", IOType::Out, &["SD"]),
                    ("3", IOType::In, &["CS"]),
                    ("4", IOType::Out, &["RS"]),
                    ("5", IOType::In, &["CD"]),
                    ("6", IOType::In, &["DR"]),
                    ("7", IOType::Out, &["TT"]),
                    ("8", IOType::In, &["TM"]),
                    ("9", IOType::In, &["RT"]),
                    ("10", IOType::None, &["SG"]),
                    ("11", IOType::In, &["RD2"]),
                    ("12", IOType::Out, &["SD2"]),
                    ("13", IOType::In, &["CS2"]),
                    ("14", IOType::Out, &["RS2"]),
                    ("15", IOType::In, &["CD2"]),
                    ("16", IOType::Out, &["DR2"]),
                ],
            ),
        ],
    );
}

#[test]
fn u306__uart_rs485_pin_tables() {
    load_hbl();
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS485.3",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
            ("3", IOType::None, &[]),
        ],
        &[
            (
                "Master",
                &[
                    ("1", IOType::InOut, &["A"]),
                    ("2", IOType::InOut, &["B"]),
                    ("3", IOType::None, &["GND"]),
                ],
            ),
            (
                "Slave",
                &[
                    ("1", IOType::InOut, &["A"]),
                    ("2", IOType::InOut, &["B"]),
                    ("3", IOType::None, &["GND"]),
                ],
            ),
        ],
    );
    let uri = lib_uri("ifs/uart.mc");
    assert_iface(
        "UART.RS485",
        &uri,
        &[
            ("1", IOType::None, &[]),
            ("2", IOType::None, &[]),
        ],
        &[
            (
                "Master",
                &[
                    ("1", IOType::InOut, &["A"]),
                    ("2", IOType::InOut, &["B"]),
                ],
            ),
            (
                "Slave",
                &[
                    ("1", IOType::InOut, &["A"]),
                    ("2", IOType::InOut, &["B"]),
                ],
            ),
        ],
    );
}

#[test]
fn u306__hdr_range_forms_pin_tables() {
    load_hbl();
    let uri = lib_uri("conn/hdr.mc");
    let table = component_table("HDR.1X8", &uri);
    assert_rows("HDR.1X8", &table,
&[
            ("1", IOType::None, &["1"]),
            ("2", IOType::None, &["2"]),
            ("3", IOType::None, &["3"]),
            ("4", IOType::None, &["4"]),
            ("5", IOType::None, &["5"]),
            ("6", IOType::None, &["6"]),
            ("7", IOType::None, &["7"]),
            ("8", IOType::None, &["8"]),
        ],
    );
    let uri = lib_uri("conn/hdr.mc");
    let table = component_table("HDR.1X9", &uri);
    assert_rows("HDR.1X9", &table,
&[
            ("1", IOType::None, &["1"]),
            ("2", IOType::None, &["2"]),
            ("3", IOType::None, &["3"]),
            ("4", IOType::None, &["4"]),
            ("5", IOType::None, &["5"]),
            ("6", IOType::None, &["6"]),
            ("7", IOType::None, &["7"]),
            ("8", IOType::None, &["8"]),
            ("9", IOType::None, &["9"]),
        ],
    );
    let uri = lib_uri("conn/hdr.mc");
    let table = component_table("HDR.2X9", &uri);
    assert_rows("HDR.2X9", &table,
&[
            ("1", IOType::None, &["R1C1"]),
            ("2", IOType::None, &["R1C2"]),
            ("3", IOType::None, &["R1C3"]),
            ("4", IOType::None, &["R1C4"]),
            ("5", IOType::None, &["R1C5"]),
            ("6", IOType::None, &["R1C6"]),
            ("7", IOType::None, &["R1C7"]),
            ("8", IOType::None, &["R1C8"]),
            ("9", IOType::None, &["R1C9"]),
            ("10", IOType::None, &["R2C1"]),
            ("11", IOType::None, &["R2C2"]),
            ("12", IOType::None, &["R2C3"]),
            ("13", IOType::None, &["R2C4"]),
            ("14", IOType::None, &["R2C5"]),
            ("15", IOType::None, &["R2C6"]),
            ("16", IOType::None, &["R2C7"]),
            ("17", IOType::None, &["R2C8"]),
            ("18", IOType::None, &["R2C9"]),
        ],
    );
}

// Bucket ③: module US513 port table (hbl fixture).

/// The module-level half of the C3 split (the component half landed with
/// b4031): the whole port table of `module US513` read off
/// `McModuleInst.ports` — the same rows `mcc show ports` renders. Head `psnk`
/// pairs surface as bus ports named by their display form, body `io`/`out`
/// rows as plain ports; a curly port carries its bus members in declaration
/// order. The mcc fixture spells `I2C0`/`SPI`/`UART0`/`UART1` as plain names,
/// so their tables are empty here.
#[test]
fn u306__module_us513_port_table() {
    load_hbl();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    let us513 = std::fs::canonicalize(root.join("src/us513.mc")).expect("canonicalize us513.mc");
    let uri = McURI::from(us513.to_string_lossy().to_string());
    let (inst, _arena, _store, _nets) = mcc::mcc_build_with_arena(&McIds::from("US513"), &uri)
        .expect("build module US513");

    let got: Vec<(String, IOType, Vec<String>)> = inst
        .ports
        .iter()
        .map(|p| (p.name.clone(), p.iotype.clone(), p.bus_members.clone()))
        .collect();
    let expected: &[(&str, IOType, &[&str])] = &[
        ("[VDD_3V3, GND]", IOType::Power, &["VDD_3V3", "GND"]),
        ("[VCC_1V2, GND]", IOType::Power, &["VCC_1V2", "GND"]),
        ("MIC", IOType::InOut, &["P", "N"]),
        ("I2C0", IOType::InOut, &[]),
        ("SPI", IOType::InOut, &[]),
        ("UART0", IOType::InOut, &[]),
        ("UART1", IOType::InOut, &[]),
        ("port1", IOType::InOut, &["A", "B", "C", "D"]),
        ("DAC_OUT", IOType::Out, &[]),
        ("SPK_MUTE", IOType::Out, &[]),
    ];
    assert_eq!(
        got.len(),
        expected.len(),
        "module US513: port count drifted (got {got:?})"
    );
    for ((name, io, members), (want_name, want_io, want_members)) in got.iter().zip(expected) {
        assert_eq!(name, want_name, "module US513: port set drifted");
        assert_eq!(io, want_io, "module US513.{name}: direction drifted");
        assert_eq!(
            members, want_members,
            "module US513.{name}: bus members drifted"
        );
    }
}

// Bucket ④: bom overlay face (bom_mic fixture).

/// The bom.mc "description" lock, re-scoped by ruling: the legacy `define`
/// description form retired with U267, so what locks is the **modern overlay
/// face** — the engineering `bom` block buying a part for an abstract slot.
/// The fixture mirrors power.mc's LDO shape with the electret-microphone
/// family: the slot `MICROPHONE.ELECTRET` is bound by the row
/// `mic = MICROPHONE.SIP2_1_25MM_WA` (the modern counterpart of the legacy
/// `MICROPHONE.SIP2` define). The flat row must ride the variant's identity
/// with both overlay checks silent — the same assertions the U245 pilot
/// (`bom_hbl.rs`) makes for the LDO family.
#[test]
fn u306__bom_overlay_binds_the_sip2_slot() {
    let _guard = common::lock();
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bom_mic");
    let entry_uri = project_root
        .join("src/main.mc")
        .to_string_lossy()
        .into_owned();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (_, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000)
            .expect("build bom_mic");

    let id = table
        .get_id_by_path("main.mic")
        .expect("the mic slot row exists");
    let entry = table.get_entry(id).unwrap();
    assert_eq!(
        entry.class_name, "MICROPHONE.SIP2_1_25MM_WA",
        "the bound row rides the variant's identity"
    );
    assert!(!entry.unselected, "a bound slot is selected");

    for d in mcc::mcc_diagnose_all() {
        assert_ne!(
            d.code,
            mcc::errcodes::BOM_VALUE_NOT_DESCENDANT,
            "the bind must be legal (value is a `:` descendant of the slot): {}",
            d.msg
        );
        assert_ne!(
            d.code,
            mcc::errcodes::BOM_KEY_NOT_SLOT,
            "the key must hit a real slot: {}",
            d.msg
        );
    }

    // Hand the workspace back with no project root, so no later test in this
    // binary inherits the fixture's overlay.
    mcc::mcc_set_project_root(std::path::Path::new(""));
}
