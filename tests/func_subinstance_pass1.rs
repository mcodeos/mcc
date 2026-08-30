// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! resolve-gate §3 Part 1+2 (pass1): component func-body subinstance
//! registration + eager connection-operator return-shape fill.
//!
//! Part 1 — `McComponent::parse_declare` set-difference: a func-body chain
//! declare (`XTAL2 y(...).Setup(VSS)`) registers the subinstance into the
//! component's `insts`, so the receiver endpoint resolves and sibling funcs
//! can `find_inst("y")` (§3.1).
//!
//! Part 2 — eager `-` / `+` / `<-` return-shape fill (mirroring the existing
//! `->` site, §3.2): the operator opcheck runs during parse, so the FuncCall
//! receiver's return face must be resolved eagerly — the func body never runs
//! through the pass1b hook (module/mod.rs:125), which covers module top-level
//! stmts only.
//!
//! Regression target: mclibs/clock/mcp7940m.mc:48
//! `XTAL2(32.768kHz, 10nF).Setup(VSS) - XTAL` (design §5 item 2). Pre-fix it
//! emitted E4007 + E3134. The anonymous form additionally required
//! `McComponent::gen_anon_name` + `add_component` (the module-level
//! implementations already existed; the component ones were empty stubs).
//!
//! Fixtures must be loaded with the mcode system lib (`mcc_init`) so the real
//! `XTAL2` / `XTAL` / `UV.*` / `DC` defs resolve.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests sharing mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// All diagnostic codes after loading `src` under `uri` and building `main`.
fn codes(src: &str, uri: &str) -> Vec<u32> {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_clear_workspace();
    let root = mcc::cli::datadir::data_root();
    mcc::mcc_set_system_root(&root);
    mcc::mcc_init();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &u);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// Assert every code in `absent` is not present in `codes`.
fn assert_absent(codes: &[u32], absent: &[u32], what: &str) {
    for c in absent {
        assert!(
            !codes.contains(c),
            "{what}: code {c} unexpectedly present; got {codes:?}"
        );
    }
}

/// The codes Part 1+2 must keep away from every fixture.
const GATE_CODES: [u32; 6] = [
    mcc::errcodes::CONN_STMT_PARSE_FAILED,         // E3132
    mcc::errcodes::FUNC_STMT_DROPPED,              // E3134
    mcc::errcodes::INSTANCE_REF_UNDECLARED,        // E3182
    mcc::errcodes::CONN_LEFT_ARROW_SHAPE_MISMATCH, // E4002
    mcc::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,   // E4005
    mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH,     // E4007
];

/// The HOST component (mcp7940m.mc mirror): a 2-wide `XTAL` interface bus on
/// pins [1,2] and a DC `[VCC,VSS]` power pair, plus `func Xtal()` whose body
/// is the statement under test.
const HOST_PINS: &str = "    pins = [\n        ps [8,4] = [VCC,VSS]::DC()\n        in [1,2] = XTAL{X1,X2}::XTAL()\n    ]\n";

fn host_with_func(body: &str) -> String {
    format!(
        "component HOST\n{{\n{}    func Xtal()\n    {{\n{}{}}}\n}}\nmodule main\n{{\n    io VDD\n    HOST U1\n}}\n",
        HOST_PINS,
        body.lines().map(|l| format!("        {l}\n")).collect::<String>(),
        "\n"
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// `-` (Series): anonymous + named receiver
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn anonymous_minus_in_component_func_body_is_clean() {
    // mcp7940m.mc:48 — the design §5 item 2 regression target. Anonymous
    // receiver `XTAL2(...)` must name itself (@XTAL2{n}) via
    // `McComponent::gen_anon_name` (component/mod.rs), become an Endpoint via
    // `add_component`, and the `-` opcheck must see Setup's `return
    // XTAL{X1,X2}` 2-wide face (Part 2 eager fill).
    let src = host_with_func("XTAL2(32.768kHz, 10nF).Setup(VSS) - XTAL");
    let codes = codes(&src, "/mcc/anon-minus.mc");
    assert_absent(&codes, &GATE_CODES, "anonymous '-' in component func body");
}

#[test]
fn named_minus_in_component_func_body_is_clean() {
    // Named receiver `XTAL2 y(...)`: Part 1 registers `y` into the component's
    // insts (set-difference via McComponent::parse_declare), so the receiver
    // endpoint resolves; Part 2 fills the return face before the opcheck.
    let src = host_with_func("XTAL2 y(32.768kHz, 10nF).Setup(VSS) - XTAL");
    let codes = codes(&src, "/mcc/named-minus.mc");
    assert_absent(&codes, &GATE_CODES, "named '-' in component func body");
}

// ═══════════════════════════════════════════════════════════════════════════
// `+` (Parallel)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn plus_in_component_func_body_is_clean() {
    // `+` is Parallel — both arms are connection faces, so Part 2 fills both
    // opd1 and opd2 (doc §3.2).
    let src = host_with_func("XTAL2(32.768kHz, 10nF).Setup(VSS) + XTAL");
    let codes = codes(&src, "/mcc/plus.mc");
    assert_absent(&codes, &GATE_CODES, "'+' in component func body");
}

// ═══════════════════════════════════════════════════════════════════════════
// `<-` (reverse Series)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn leftarrow_in_component_func_body_is_clean() {
    // Data flow is opd2 -> opd1, so Part 2 fills the ORIGINAL opd1 (the left
    // operand) before the opd2/opd1 swap in the `<-` handler (doc §3.2).
    let src = host_with_func("XTAL2(32.768kHz, 10nF).Setup(VSS) <- XTAL");
    let codes = codes(&src, "/mcc/leftarrow.mc");
    assert_absent(&codes, &GATE_CODES, "'<-' in component func body");
}

// ═══════════════════════════════════════════════════════════════════════════
// Part 1 sibling-func benefit: find_inst sees the registered subinstance
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn sibling_func_resolves_registered_subinstance() {
    // `y` is registered by Part 1 when `Xtal()` parses; a sibling func's
    // `find_inst("y")` must resolve it and its bus members (`y.XTAL.X1`) —
    // no E3182. Pre-Part 1, `y` resolved nowhere in the component scope.
    let src = format!(
        "component HOST\n{{\n{}    func Xtal()\n    {{\n        XTAL2 y(32.768kHz, 10nF).Setup(VSS) - XTAL\n    }}\n    func Check()\n    {{\n        y.XTAL.X1 -> y.XTAL.X2\n    }}\n}}\nmodule main\n{{\n    io VDD\n    HOST U1\n}}\n",
        HOST_PINS
    );
    let codes = codes(&src, "/mcc/sibling-ref.mc");
    assert_absent(
        &codes,
        &[mcc::errcodes::INSTANCE_REF_UNDECLARED],
        "sibling func resolves registered subinstance 'y'",
    );
}
