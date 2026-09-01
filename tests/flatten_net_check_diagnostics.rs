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
            95,
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

/// ── Lock: unconnected bidirectional port of a sub-module ──────────────────
/// Mirrors the reported case (`main.MCU513.I2C0` in the hbl view): the
/// sub-module's `io I2C0` port is never wired, so E4117 must anchor at the
/// port's declaration in the sub-module body (`io I2C0`) — not at offset 0
/// (file:1:1).
#[test]
fn submodule_unconnected_bidir_port_anchors_at_declaration() {
    let src = "component R {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\nmodule SUB {\n    io I2C0\n}\nmodule main {\n    SUB sub1\n}";
    let diags = build_flat_diags(src);
    let expected = [
        (
            5642,
            83,
            "/mcc/flat-diag.mc",
            "Port 'I2C0' in 'SUB' is declared but never used in any net connection.",
        ),
        (
            4117,
            83,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.sub1.I2C0' is not connected to any net.",
        ),
    ];
    assert_lock(diags, &expected, "sub-module port anchor changed");
}

/// ── Lock: cross-file sub-module port anchor (the reported hbl case) ───────
/// `module main` instantiates `SUB MCU513` from a def file; SUB's `io I2C0`
/// port is never wired, so E4117 on `main.MCU513.I2C0` anchors at the port's
/// declaration in the def file (`io I2C0`) — not at offset 0 / file:1:1.
/// `use ./defs.mc` resolves against the real file system, so both files are
/// written to a temp dir and loaded by canonical path (the same pattern the
/// `circuit_deps_record_entry_and_class_resolutions` cross-file test uses).
#[test]
fn cross_file_submodule_port_anchors_at_def_declaration() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let dir = std::env::temp_dir().join(format!("mcc-flat-cross-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("defs.mc"), "module SUB {\n    io I2C0\n}\n").unwrap();
    std::fs::write(
        dir.join("main.mc"),
        "use ./defs.mc\nmodule main {\n    SUB MCU513\n}",
    )
    .unwrap();
    let defs_uri = std::fs::canonicalize(dir.join("defs.mc"))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let main_uri = std::fs::canonicalize(dir.join("main.mc"))
        .unwrap()
        .to_string_lossy()
        .into_owned();

    mcc::mcc_load_from_string(
        &defs_uri,
        &std::fs::read_to_string(dir.join("defs.mc")).unwrap(),
    );
    mcc::mcc_load_from_string(
        &main_uri,
        &std::fs::read_to_string(dir.join("main.mc")).unwrap(),
    );
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &main_uri, 1000).expect("flat build");
    let diags: Vec<(u32, u32, String, String)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.loc.pos, d.loc.uri.to_string(), d.msg.clone()))
        .collect();
    let expected = [
        (
            5642,
            20,
            defs_uri.as_str(),
            "Port 'I2C0' in 'SUB' is declared but never used in any net connection.",
        ),
        (
            4117,
            20,
            defs_uri.as_str(),
            "Bidirectional port 'main.MCU513.I2C0' is not connected to any net.",
        ),
    ];
    assert_lock(diags, &expected, "cross-file port anchor changed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// ── Lock: bracket-form signature port anchors at its declaration ──────────
/// `[VDD_3V3, GND]::DC(3.3V)` is a Multiple-form signature interface param:
/// its whole-name span lives in `def.params.def_spans`, not `def.insts`
/// (`filter_port_spans` drops the whole-bracket name). E4117 for the
/// unconnected bracket port (and its members) must anchor at the declaration
/// (`VDD_3V3` inside the bracket) instead of file:1:1. The empty US513 body
/// also emits 2115; only the 4117 entries are asserted here.
#[test]
fn bracket_signature_port_anchors_at_declaration() {
    let src = "module US513([VDD_3V3,GND]::DC(3.3V)) {\n}\nmodule main {\n    US513 UC\n}\n";
    let diags = build_flat_diags(src);
    let bidir: Vec<(u32, u32, String, String)> =
        diags.into_iter().filter(|d| d.0 == 4117).collect();
    let expected = [
        (
            4117,
            14,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.UC.[VDD_3V3, GND]' is not connected to any net.",
        ),
        (
            4117,
            14,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.UC.VDD_3V3' is not connected to any net.",
        ),
        (
            4117,
            14,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.UC.GND' is not connected to any net.",
        ),
    ];
    assert_lock(bidir, &expected, "bracket signature port anchor changed");
}

/// ── Lock: curly-bus port members anchor at the port declaration ───────────
/// `io MIC{P,N}` materializes as the port `MIC` plus three member shapes:
/// dotted (`MIC.P` / `MIC.N`), bus-slash (`MIC/P` / `MIC/N`) and bare
/// (`N` / `P`). Every unconnected shape must anchor at the `io MIC{P,N}`
/// declaration (pos 20) instead of file:1:1.
#[test]
fn curly_bus_port_members_anchor_at_declaration() {
    let src = "module SUB {\n    io MIC{P,N}\n}\nmodule main {\n    SUB s1\n}\n";
    let diags = build_flat_diags(src);
    let expected = [
        (
            5642,
            20,
            "/mcc/flat-diag.mc",
            "Port 'MIC' in 'SUB' is declared but never used in any net connection.",
        ),
        (
            4117,
            20,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.s1.MIC' is not connected to any net.",
        ),
        (
            4117,
            20,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.s1.MIC.P' is not connected to any net.",
        ),
        (
            4117,
            20,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.s1.MIC.N' is not connected to any net.",
        ),
        (
            4117,
            20,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.s1.MIC/P' is not connected to any net.",
        ),
        (
            4117,
            20,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.s1.MIC/N' is not connected to any net.",
        ),
        (
            4117,
            20,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.s1.N' is not connected to any net.",
        ),
        (
            4117,
            20,
            "/mcc/flat-diag.mc",
            "Bidirectional port 'main.s1.P' is not connected to any net.",
        ),
    ];
    assert_lock(diags, &expected, "curly bus member anchor changed");
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
