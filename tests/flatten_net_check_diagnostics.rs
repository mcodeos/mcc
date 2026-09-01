// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Phase A (dianlu-tree refactor) P0.4 golden lock: the flat electrical net
//! checks (run inside `DianLu::flatten`) produce a deterministic ordered
//! diagnostic sequence — same codes, levels, uris and positions regardless of
//! where the logging happens. Locked before Phase A moves the logging out of
//! DianLu, so the refactor is a pure move with zero observable change.
//!
//! Each test builds a fixture through `mcc_build_flat` (pass1 + pass2 +
//! flatten net checks) and asserts the exact ordered diagnostic sequence
//! (code, pos, message). `InstTable` stores entries and nets in `BTreeMap`s,
//! so the sequence is deterministic across runs.
//!
//! This file intentionally duplicates `gap2_materialization.rs`'s pattern:
//! a global lock serializes tests because the diagnostic manager is global.

use mcc::McIds;
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Build `main` and flatten to the InstTable; return the full ordered
/// diagnostic sequence as (code, pos, uri, message) tuples.
fn build_flat_diags(src: &str) -> Vec<(u32, u32, String, String)> {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let uri = "/mcc/flat-diag.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.loc.pos, d.loc.uri.to_string(), d.msg.clone()))
        .collect()
}

/// Two-input/one-output buffer used by the driver-conflict fixtures.
const BUF: &str = "component BUF {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n";

/// ── Lock: driver conflict + floating inputs + partial wiring ──────────────
/// `b1.Y -> b2.Y` merges two `Out` pins onto one net. Expected sequence:
/// 4101 driver conflict, two 4108 floating inputs, two 4114 module-port
/// checks, two 4116 partial-wiring checks. Order is the `run_net_checks`
/// pass order, positions are byte offsets into the fixture text.
#[test]
fn driver_conflict_sequence_is_locked() {
    let src = format!("{BUF}module main {{\n    BUF b1\n    BUF b2\n    b1.Y -> b2.Y\n}}");
    let diags = build_flat_diags(&src);
    let expected = [
        (
            4101,
            112,
            "/mcc/flat-diag.mc",
            "Net '_net0' has 2 drivers: main.b1.2, main.b2.2. Possible short circuit.",
        ),
        (
            4108,
            40,
            "/mcc/flat-diag.mc",
            "Input 'main.b1.1' is not connected to any net.",
        ),
        (
            4108,
            40,
            "/mcc/flat-diag.mc",
            "Input 'main.b2.1' is not connected to any net.",
        ),
        (
            4114,
            40,
            "/mcc/flat-diag.mc",
            "Module port 'main.b1.1' (In) is not connected to any net.",
        ),
        (
            4114,
            40,
            "/mcc/flat-diag.mc",
            "Module port 'main.b2.1' (In) is not connected to any net.",
        ),
        (
            4116,
            94,
            "/mcc/flat-diag.mc",
            "'main.b1' has 1 of 2 pins connected.",
        ),
        (
            4116,
            105,
            "/mcc/flat-diag.mc",
            "'main.b2' has 1 of 2 pins connected.",
        ),
    ];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// ── Lock: passive chain with an unused io port ─────────────────────────────
/// The resistor chain is clean; the unused `GND` io port produces the
/// port-unused (5642) and bidirectional-port-unconnected (4117) pair.
#[test]
fn unused_io_port_sequence_is_locked() {
    let src = "component R {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\nmodule main {\n    io VDD\n    io GND\n    R r1\n    R r2\n    r1.1 -> VDD\n    r1.2 -> r2.1\n    r2.2 -> VDD\n}";
    let diags = build_flat_diags(src);
    let expected = [
        (
            5642,
            95,
            "/mcc/flat-diag.mc",
            "Port 'GND' in 'main' is declared but never used in any net connection.",
        ),
        (
            4117,
            0,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.GND' is not connected to any net.",
        ),
    ];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// ── Lock: fully unwired instance ───────────────────────────────────────────
/// `b1.A -> b1.A` loops the input onto itself but leaves every pin unwired:
/// 4108 floating input, 4110 output drives nothing, 4112 no pins connected,
/// 4114 module-port checks, 4116 0-of-2 partial wiring.
#[test]
fn unwired_instance_sequence_is_locked() {
    let src = format!("{BUF}module main {{\n    BUF b1\n    b1.A -> b1.A\n}}");
    let diags = build_flat_diags(&src);
    let expected = [
        (
            4108,
            40,
            "/mcc/flat-diag.mc",
            "Input 'main.b1.1' is not connected to any net.",
        ),
        (
            4110,
            58,
            "/mcc/flat-diag.mc",
            "Output 'main.b1.2' drives nothing.",
        ),
        (
            4112,
            94,
            "/mcc/flat-diag.mc",
            "Instance 'main.b1' has no pins connected to any net.",
        ),
        (
            4114,
            40,
            "/mcc/flat-diag.mc",
            "Module port 'main.b1.1' (In) is not connected to any net.",
        ),
        (
            4114,
            58,
            "/mcc/flat-diag.mc",
            "Module port 'main.b1.2' (Out) is not connected to any net.",
        ),
        (
            4116,
            94,
            "/mcc/flat-diag.mc",
            "'main.b1' has 0 of 2 pins connected.",
        ),
    ];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// Assert the actual ordered diagnostic sequence equals the expected golden
/// sequence of (code, pos, uri, message) tuples — order-sensitive.
fn assert_lock(
    actual: Vec<(u32, u32, String, String)>,
    expected: &[(u32, u32, &str, &str)],
    what: &str,
) {
    let got: Vec<String> = actual
        .iter()
        .map(|(c, p, u, m)| format!("({c},{p},{u}) {m}"))
        .collect();
    let want: Vec<String> = expected
        .iter()
        .map(|(c, p, u, m)| format!("({c},{p},{u}) {m}"))
        .collect();
    assert_eq!(got, want, "{what}");
}
