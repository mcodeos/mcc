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

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::McIds;

/// Build `main` and flatten to the InstTable; return the full ordered
/// diagnostic sequence as (code, pos, uri, message) tuples.
fn build_flat_diags(src: &str) -> Vec<(u32, u32, String, String)> {
    let _lock = common::lock();
    common::reset();
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
/// 4101 driver conflict, two 4108 floating inputs (the pads' own directional
/// float), two 4116 partial-wiring checks. C4/E4114 no longer re-reports the
/// unconnected `In` pads as "module ports" — component pads belong to the
/// directional checks (A′-scope), so the former 4114 rows on `main.b1.1` /
/// `main.b2.1` are gone. Order is the `run_net_checks` pass order, positions
/// are byte offsets into the fixture text.
#[test]
fn dlu_flatchk__driver_conflict_sequence_locked() {
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
/// The resistor chain is clean; the unused top-level `io GND` produces only
/// the pass1 port-unused (5642). The instance float (4117/4114) does not fire
/// on the top module's own ports — those are the design's external contract
/// (C4 skips top ports; the directional checks are A′-scoped to pads), so the
/// former `main.GND` 4117 row is gone.
#[test]
fn dlu_flatchk__unused_io_port_sequence_locked() {
    let src = "component R {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\nmodule main {\n    io VDD\n    io GND\n    R r1\n    R r2\n    r1.1 -> VDD\n    r1.2 -> r2.1\n    r2.2 -> VDD\n}";
    let diags = build_flat_diags(src);
    let expected = [(
        5642,
        95,
        "/mcc/flat-diag.mc",
        "Port 'GND' in 'main' is declared but never used in any net connection.",
    )];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// ── Lock: fully unwired instance ───────────────────────────────────────────
/// `b1.A -> b1.A` loops the input onto itself but leaves every pin unwired:
/// 4108 floating input, 4110 output drives nothing, 4112 no pins connected,
/// 4116 0-of-2 partial wiring. The former 4114 "module port" rows on the
/// `In`/`Out` pads are gone — component pads are owned by the directional
/// checks (A′-scope), so C4 no longer double-reports them.
#[test]
fn dlu_flatchk__unwired_instance_sequence_locked() {
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
            4116,
            94,
            "/mcc/flat-diag.mc",
            "'main.b1' has 0 of 2 pins connected.",
        ),
    ];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// ── Lock: unconnected io port of a sub-module ─────────────────────────────
/// Mirrors the reported case (`main.MCU513.I2C0` in the hbl view): the
/// sub-module's `io I2C0` port is never wired, so C4/E4114 must anchor at the
/// port's declaration in the sub-module body (`io I2C0`) — not at offset 0
/// (file:1:1). A′-scope routes module-boundary ports to C4, so the code is
/// 4114 (formerly 4117).
#[test]
fn dlu_flatchk__submodule_unconnected_bidir_port_anchors_declaration() {
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
            4114,
            83,
            "/mcc/flat-diag.mc",
            "Module port 'main.sub1.I2C0' (InOut) is not connected to any net.",
        ),
    ];
    assert_lock(diags, &expected, "sub-module port anchor changed");
}

/// ── Lock: cross-file sub-module port anchor (the reported hbl case) ───────
/// `module main` instantiates `SUB MCU513` from a def file; SUB's `io I2C0`
/// port is never wired, so C4/E4114 on `main.MCU513.I2C0` anchors at the
/// port's declaration in the def file (`io I2C0`) — not at offset 0 /
/// file:1:1. A′-scope routes module-boundary ports to C4, so the code is 4114
/// (formerly 4117). `use ./defs.mc` resolves against the real file system, so
/// both files are written to a temp dir and loaded by canonical path (the same
/// pattern the `circuit_deps_record_entry_and_class_resolutions` cross-file
/// test uses).
#[test]
fn dlu_flatchk__cross_file_submodule_port_anchors_def_declaration() {
    let _lock = common::lock();
    common::reset();

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
            4114,
            20,
            defs_uri.as_str(),
            "Module port 'main.MCU513.I2C0' (InOut) is not connected to any net.",
        ),
    ];
    assert_lock(diags, &expected, "cross-file port anchor changed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// ── Lock: bracket-form signature port anchors at its declaration ──────────
/// `[VDD_3V3, GND]::DC(3.3V)` is a Multiple-form signature interface param:
/// its whole-name span lives in `def.params.def_spans`, not `def.insts`
/// (`filter_port_spans` drops the whole-bracket name). C4/E4114 for the
/// unconnected bracket *members* must anchor at the declaration
/// (`VDD_3V3` inside the bracket) instead of file:1:1. The bracketed aggregate
/// (`main.UC.[VDD_3V3, GND]`) is a non-physical grouping header (A′
/// 2026-09-04) — de-electrified, it must NOT surface here; the members carry
/// the check. A′-scope routes these module-boundary members to C4 (formerly
/// E4117). The empty US513 body also emits 2115; only the 4114 entries are
/// asserted here.
#[test]
fn dlu_flatchk__bracket_signature_port_anchors_declaration() {
    let src = "module US513([VDD_3V3,GND]::DC(3.3V)) {\n}\nmodule main {\n    US513 UC\n}\n";
    let diags = build_flat_diags(src);
    let bidir: Vec<(u32, u32, String, String)> =
        diags.into_iter().filter(|d| d.0 == 4114).collect();
    let expected = [
        (
            4114,
            14,
            "/mcc/flat-diag.mc",
            "Module port 'main.UC.VDD_3V3' (InOut) is not connected to any net.",
        ),
        (
            4114,
            14,
            "/mcc/flat-diag.mc",
            "Module port 'main.UC.GND' (InOut) is not connected to any net.",
        ),
    ];
    assert_lock(bidir, &expected, "bracket signature port anchor changed");
}

/// ── Lock: curly-bus unconnected members anchor at the port declaration ─────
/// `io MIC{P,N}` registers dotted member Ports (`main.s1.MIC.P` / `MIC.N`).
/// The aggregate (`main.s1.MIC`), bus-slash (`MIC/P` / `MIC/N`) and bare
/// (`P` / `N`) spellings are non-physical aliases folded onto the members
/// (A′ 2026-09-04): de-electrified, they must NOT surface as unconnected.
/// Only the member Ports carry C4/E4114 (module-boundary ports are A′-scoped
/// to C4; formerly E4117), anchored at the `io MIC{P,N}` declaration (pos 20)
/// instead of file:1:1.
#[test]
fn dlu_flatchk__curly_bus_port_members_anchor_declaration() {
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
            4114,
            20,
            "/mcc/flat-diag.mc",
            "Module port 'main.s1.MIC.P' (InOut) is not connected to any net.",
        ),
        (
            4114,
            20,
            "/mcc/flat-diag.mc",
            "Module port 'main.s1.MIC.N' (InOut) is not connected to any net.",
        ),
    ];
    assert_lock(diags, &expected, "curly bus member anchor changed");
}

/// ── Lock: E4110 alias shapes of an `out` curly bus are gone (A′) ───────────
/// The reported hbl shape is `out MIC{P,N}` (an *output* bus): pre-A′ the
/// aggregate (`main.s1.MIC`), bus-slash (`MIC/P` / `MIC/N`) and bare (`P` / `N`)
/// Out entries were never net endpoints yet carried io Out → false E4110
/// "drives nothing". A′ de-electrifies + folds those aliases, and the
/// directional E4110 is A′-scoped to pads — module-boundary `out` member Ports
/// are C4/E4114's domain. So only the physical member Ports
/// (`main.s1.MIC.P` / `.N`) carry C4, at the declaration; E4110 surfaces
/// nothing here.
#[test]
fn dlu_flatchk__curly_bus_out_members_e4110_only_on_members() {
    let src = "module SUB {\n    out MIC{P,N}\n}\nmodule main {\n    SUB s1\n}\n";
    let diags = build_flat_diags(src);
    let e4110: Vec<(u32, u32, String, String)> =
        diags.iter().filter(|d| d.0 == 4110).cloned().collect();
    assert_lock(
        e4110,
        &[],
        "out curly bus members must not surface as E4110",
    );
    let e4114: Vec<(u32, u32, String, String)> =
        diags.iter().filter(|d| d.0 == 4114).cloned().collect();
    assert_lock(
        e4114,
        &[
            (
                4114,
                21,
                "/mcc/flat-diag.mc",
                "Module port 'main.s1.MIC.P' (Out) is not connected to any net.",
            ),
            (
                4114,
                21,
                "/mcc/flat-diag.mc",
                "Module port 'main.s1.MIC.N' (Out) is not connected to any net.",
            ),
        ],
        "out curly bus member Ports must carry C4/E4114 only",
    );
}

/// ── Lock: coverage-gap net D-family positive locks (reorg doc §8.3/§8.5) ───
/// Positive locks for the remaining net-D-family checks that did not yet have a
/// firing test: 4103 (no driver), 4109 (NC connected), 4111 (backfeed),
/// 4113 (outputs without input), 4118 (power-net count). Each fixture is
/// chosen so the target code is the *only* semantically meaningful diagnostic;
/// the co-emitted 5454 (power pin with no voltage attribute) is intrinsic to a
/// bare `ps` pin and is kept in the golden. 4105 (NET_VOLTAGE_MISMATCH) needs
/// the DC interface library (`interface DC(volt::UV.VOLT, …)` in a project
/// lib) that the test harness does not load, and 4115 (NET_DANGLING_ENDPOINT)
/// needs a single-point net that flatten never produces from top-level wiring
/// (self-loops collapse, unwired io ports are dropped) — both are recorded as
/// context-gated in the reorg doc §8.3 rather than forced fixtures.

/// Two-input receiver (4103: two in-pins joined with no driver).
const TWIN: &str = "component TW {\n    pins = [\n        in 1 = A\n        in 2 = B\n    ]\n}\n";

/// 4103 NET_NO_DRIVER: `c1.A -> c2.A` merges two `In` pins onto one net with
/// no output or power supply driving it. The `B` inputs of both instances
/// float, adding the 4108/4116 per-instance pair (the former C4/E4114 rows on
/// the pads are gone — pads are owned by the directional checks).
#[test]
fn dlu_flatchk__no_driver_net_locked() {
    let src = format!("{TWIN}module main {{\n    TW c1\n    TW c2\n    c1.A -> c2.A\n}}");
    let diags = build_flat_diags(&src);
    let expected = [
        (
            4103,
            108,
            "/mcc/flat-diag.mc",
            "Net '_net0' has inputs but no output/power driver.",
        ),
        (
            4108,
            56,
            "/mcc/flat-diag.mc",
            "Input 'main.c1.2' is not connected to any net.",
        ),
        (
            4108,
            56,
            "/mcc/flat-diag.mc",
            "Input 'main.c2.2' is not connected to any net.",
        ),
        (
            4116,
            91,
            "/mcc/flat-diag.mc",
            "'main.c1' has 1 of 2 pins connected.",
        ),
        (
            4116,
            101,
            "/mcc/flat-diag.mc",
            "'main.c2' has 1 of 2 pins connected.",
        ),
    ];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// 4109 NET_NC_CONNECTED: a no-connect pin wired into a net. The pin is named
/// `NC`, which collides with the `nc` iotype keyword, so it cannot be reached
/// by `c1.NC` (that spells a clause and dies at parse, E2082); it is reached
/// by its numeric pin id `c1.2`.
#[test]
fn dlu_flatchk__nc_connected_by_pin_id_locked() {
    let src = "component TW {\n    pins = [\n        in 1 = A\n        nc 2 = NC\n    ]\n}\nmodule main {\n    io VDD\n    TW c1\n    c1.A -> VDD\n    c1.2 -> VDD\n}";
    let diags = build_flat_diags(&src);
    let expected = [(
        4109,
        126,
        "/mcc/flat-diag.mc",
        "NC port 'main.c1.2' is connected to net 'VDD'.",
    )];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// ── Lock: E4116 counts physical pads, not name entries ─────────────────────
/// A pad declared with several `|` alternate names (`1 = A | B | C`) registers
/// one `names_to_id` entry per name, so a fully-wired 2-pad part used to report
/// "has 2 of 5 pins connected". The denominator is the number of distinct
/// non-NC pads (matching the connected-pin numerator), so a fully wired
/// multi-alias part emits no E4116 at all.
const MULTI_ALIAS: &str =
    "component F {\n    pins = [\n        1 = A | B | C\n        2 = D | E\n    ]\n}\n";

#[test]
fn dlu_flatchk__multi_alias_part_fully_wired_no_partial_warning() {
    let src = format!("{MULTI_ALIAS}module main {{\n    io VDD\n    io GND\n    F f1\n    f1.1 -> VDD\n    f1.2 -> GND\n}}");
    let diags = build_flat_diags(&src);
    assert_lock(
        diags,
        &[],
        "fully-wired multi-alias part must not warn E4116",
    );
}

/// E4116 still fires when a pad genuinely floats, and the denominator now
/// reports the physical pad count (2) rather than the name-entry count (5).
#[test]
fn dlu_flatchk__multi_alias_part_partial_wiring_reports_pad_count() {
    let src = format!("{MULTI_ALIAS}module main {{\n    io VDD\n    F f1\n    f1.1 -> VDD\n}}");
    let diags = build_flat_diags(&src);
    let expected = [(
        4116,
        106,
        "/mcc/flat-diag.mc",
        "'main.f1' has 1 of 2 pins connected.",
    )];
    assert_lock(
        diags,
        &expected,
        "partial-wiring diagnostic sequence changed",
    );
}

/// One-output driver + power-supply component for the 4111/4113 fixtures.
const DRV: &str = "component D {\n    pins = [\n        out 1 = Y\n    ]\n}\n";
const PSU: &str = "component PSU {\n    pins = [\n        ps 1 = P\n    ]\n}\n";

/// 4111 NET_BACKFEED_RISK: `d1.Y -> ps1.P` puts an output and a power supply
/// on the same net. The bare `ps` pin emits its intrinsic 5454 first.
#[test]
fn dlu_flatchk__output_tied_to_power_net_locked() {
    let src = format!("{DRV}{PSU}module main {{\n    D d1\n    PSU ps1\n    d1.Y -> ps1.P\n}}");
    let diags = build_flat_diags(&src);
    let expected = [
        (
            5454,
            93,
            "/mcc/flat-diag.mc",
            "Component 'PSU': power pin 'P' (1) has no associated voltage attribute. Consider adding e.g. `voltage = \"5V\"`.",
        ),
        (
            4111,
            146,
            "/mcc/flat-diag.mc",
            "Net '_net0' has both output and power supply. Backfeed risk.",
        ),
    ];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// 4113 NET_OUTPUTS_NO_INPUT: two outputs plus a power supply on one net and
/// still no input — 4101 (two drivers) and 4111 (backfeed) are the earlier
/// checks in the same pass and precede 4113.
#[test]
fn dlu_flatchk__outputs_power_no_input_locked() {
    let src = format!("{DRV}{PSU}module main {{\n    D d1\n    D d2\n    PSU ps1\n    d1.Y -> ps1.P\n    d2.Y -> ps1.P\n}}");
    let diags = build_flat_diags(&src);
    let expected = [
        (
            5454,
            93,
            "/mcc/flat-diag.mc",
            "Component 'PSU': power pin 'P' (1) has no associated voltage attribute. Consider adding e.g. `voltage = \"5V\"`.",
        ),
        (
            4101,
            155,
            "/mcc/flat-diag.mc",
            "Net '_net0' has 2 drivers: main.d1.1, main.d2.1. Possible short circuit.",
        ),
        (
            4111,
            155,
            "/mcc/flat-diag.mc",
            "Net '_net0' has both output and power supply. Backfeed risk.",
        ),
        (
            4113,
            155,
            "/mcc/flat-diag.mc",
            "Net '_net0' has 2 outputs and power but no input.",
        ),
    ];
    assert_lock(diags, &expected, "flatten diagnostic sequence changed");
}

/// 4118 NET_POWER_NET_COUNT: twelve `ps` pins on twelve separate io rails
/// exceed the power-net consolidation threshold. The 4118 diagnostic anchors
/// at file offset 0 (design scope, no single site).
#[test]
fn dlu_flatchk__power_net_count_threshold_locked() {
    let mut src =
        String::from("component PSU {\n    pins = [\n        ps 1 = P\n    ]\n}\nmodule main {\n");
    for i in 1..=12 {
        src.push_str(&format!("    io R{i}\n"));
    }
    for i in 1..=12 {
        src.push_str(&format!("    PSU ps{i}\n"));
    }
    for i in 1..=12 {
        src.push_str(&format!("    ps{i}.P -> R{i}\n"));
    }
    src.push_str("}\n");
    let diags = build_flat_diags(&src);
    let expected = [
        (
            5454,
            40,
            "/mcc/flat-diag.mc",
            "Component 'PSU': power pin 'P' (1) has no associated voltage attribute. Consider adding e.g. `voltage = \"5V\"`.",
        ),
        (4118, 0, "/mcc/flat-diag.mc", "Design has 12 power nets. Review for consolidation."),
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
