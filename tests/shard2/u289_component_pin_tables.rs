// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U289 C3 — component-level full pin-table locks.
//!
//! The pin-expansion *rules* each carry a rule-level lock (§2.x forms in
//! `dynamic_pin_expansion.rs` and friends); what the implementation audit
//! found missing is the other half: no test asserts the **whole expanded
//! table of a real component** — every pin id with its complete alias set —
//! so a per-rule regression that redistributes names between pins would
//! pass every rule lock while scrambling the component.
//!
//! The fixtures are the real `tests/fixtures/hbl` project, whose `us513.mc`
//! was written for exactly this audit (pin-semantics design appendices 6–8):
//! `MCU.US513_20_F` is the multi-alias/multi-interface conflict hub (I2C,
//! UART, PDM, PBus, SPI, GPIO, JTAG options share physical pins), plus
//! `FLASH.GD25Q32E` (§2.7 aliases + SPI slave) and `Crystal2.DST310S`.
//!
//! The expected tables below are the **current truth of the read face**
//! (`McPins.pin_id_to_names`, the same map `mcc show pins` renders), not the
//! design appendix: the appendix predates the b3804 SPI wire-order face and
//! is corrected in the same batch.
//!
//! NOTE: like `dynamic_pin_expansion.rs`, these tests share global mcc
//! state; `common::lock()` serializes them.

use crate::common;

use mcc::{IOType, McCMIE, McIds, McURI};
use std::path::PathBuf;

/// One expected pin row: id, direction, full alias set (any order).
type Row<'a> = (&'a str, IOType, &'a [&'a str]);

/// Load the hbl fixture project (whole-project entry) and return the URI of
/// `us513.mc` — the file declaring the three components under lock —
/// canonicalized so it equals the workspace key used during loading
/// (macOS /var → /private/var). `get_component_def` matches name+defining-URI.
fn load_hbl() -> McURI {
    let _lock = common::lock();
    common::reset();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl");
    mcc::mcc_set_project_root(&root);
    // The system library carries the interface definitions (I2C, XTAL, SPI,
    // …) the pins parser qualifies member names against, so the manifest's
    // `[dependencies]` set loads first — the same route the CLI's `-F`
    // prepare() takes (show.rs: collect_libs → load_libs → load_project).
    let libs = mcc::cli::loadctx::resolve_load_context(Some(&root), &[]);
    mcc::cli::loadctx::load_all(&mcc::cli::loadctx::LoadContext::from_resolved(
        libs.lib_names(),
    ));
    let entry = std::fs::canonicalize(root.join("src/hbl.mc")).expect("canonicalize hbl.mc");
    mcc::mcc_load_project(&McURI::from(entry.to_string_lossy().to_string()));
    let us513 = std::fs::canonicalize(root.join("src/us513.mc")).expect("canonicalize us513.mc");
    McURI::from(us513.to_string_lossy().to_string())
}

/// The `pin_id → (iotype, sorted alias set)` table of one component
/// definition, read off `McPins.pins` — the same per-pin `names` vectors the
/// `mcc show pins` table renders.
fn pin_table(name: &str, uri: &McURI) -> Vec<(String, IOType, Vec<String>)> {
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

/// Assert the whole table: no pin missing, no pin extra, every alias set and
/// direction exact. A per-rule regression that redistributes names between
/// pins trips here even when every rule-level lock still passes.
fn assert_table(name: &str, uri: &McURI, expected: &[Row]) {
    let table = pin_table(name, uri);
    assert_eq!(
        table.len(),
        expected.len(),
        "{name}: pin count drifted (got {table:?})"
    );
    for ((pid, io, names), (want_pid, want_io, want_names)) in table.iter().zip(expected) {
        assert_eq!(pid, want_pid, "{name}: pin id set drifted");
        assert_eq!(io, want_io, "{name}.{pid}: direction drifted");
        assert_eq!(
            names, want_names,
            "{name}.{pid}: alias set drifted (redistribution between pins?)"
        );
    }
}

// §2 priority ①: the multi-alias/multi-interface conflict hub. Pins 8–11
// carry FOUR coexisting options (PDM list, PBus bus, GPIO range, SPI bus with
// the b3804 explicit Master wire order [8, 9, 11, 10]); pins 1–2 and 6–7 and
// 10–11 share I2C instance names across different physical pins (I2C0 here,
// I2C1 twice); pins 12–13 re-share GPIO5/GPIO6 with pins 6–7 under a
// different interface (UART1).
#[test]
fn u289_c3__us513_20_f_full_pin_table() {
    let uri = load_hbl();
    assert_table(
        "MCU.US513_20_F",
        &uri,
        &[
            ("1", IOType::InOut, &["GPIO3", "I2C0.SCL"]),
            ("2", IOType::InOut, &["GPIO4", "I2C0.SDA"]),
            ("3", IOType::In, &["XTAL.X1"]),
            ("4", IOType::In, &["XTAL.X2"]),
            ("5", IOType::Power, &["VDD"]),
            ("6", IOType::InOut, &["GPIO5", "I2C1.SCL", "UART0.TX"]),
            ("7", IOType::InOut, &["GPIO6", "I2C1.SDA", "UART0.RX"]),
            (
                "8",
                IOType::InOut,
                &["GPIO7", "PBus.CLK", "PDMCLK", "SPI.SCLK"],
            ),
            (
                "9",
                IOType::InOut,
                &["GPIO8", "PBus.DATA", "PDMDATA", "SPI.MOSI"],
            ),
            ("10", IOType::InOut, &["GPIO9", "I2C1.SCL", "SPI.CSN"]),
            ("11", IOType::InOut, &["GPIO10", "I2C1.SDA", "SPI.MISO"]),
            ("12", IOType::InOut, &["GPIO5", "UART1.TX"]),
            ("13", IOType::InOut, &["GPIO6", "UART1.RX"]),
            ("14", IOType::Power, &["VDD_CORE"]),
            ("15", IOType::In, &["AVDD09_CAP"]),
            ("16", IOType::InOut, &["ADC.P"]),
            ("17", IOType::InOut, &["ADC.N"]),
            ("18", IOType::InOut, &["GPIO0", "JTAG"]),
            ("19", IOType::InOut, &["GPIO1", "JTAG"]),
            ("20", IOType::InOut, &["EXT_CLK_IN", "GPIO[2]"]),
            ("21", IOType::Power, &["GND"]),
        ],
    );
}

// §2 priority ③ (component half): §2.7 aliases (`SO | IO1`, `_CS`) plus an
// SPI slave bus with the b3804 explicit Slave wire order [6, 5, 2, 1]; the
// power rows are the anonymous `[VCC, VSS]::DC(3.3V)` form.
#[test]
fn u289_c3__gd25q32e_full_pin_table() {
    let uri = load_hbl();
    assert_table(
        "FLASH.GD25Q32E",
        &uri,
        &[
            ("1", IOType::In, &["SPI.CS", "_CS"]),
            ("2", IOType::Out, &["IO1", "SO", "SPI.SO"]),
            ("3", IOType::None, &["IO2", "_WP"]),
            ("4", IOType::None, &["VSS"]),
            ("5", IOType::In, &["IO0", "SI", "SPI.SI"]),
            ("6", IOType::In, &["SCLK", "SPI.SCLK"]),
            ("7", IOType::None, &["IO3", "_HOLD"]),
            ("8", IOType::None, &["VCC"]),
        ],
    );
}

// §2 priority ③ (component half): a two-pin crystal via the versionless
// interface spelling `XTAL::XTAL` — member names come from the interface's
// own id → member table.
#[test]
fn u289_c3__dst310s_pin_table() {
    let uri = load_hbl();
    assert_table(
        "Crystal2.DST310S",
        &uri,
        &[
            ("1", IOType::None, &["XTAL.X1"]),
            ("2", IOType::None, &["XTAL.X2"]),
        ],
    );
}
