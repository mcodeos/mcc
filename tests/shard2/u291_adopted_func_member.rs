// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U291③: a func a component reaches through `::` recipe adoption must be
//! visible to the member-resolution face (`resolve_cmie_member_locked`), not
//! only to the Pass2 dispatch (`funccall`, which already read the
//! defregistry effective-method ledger). Before b4000 the check face read
//! only the host's own `funcs` container, so a legal adopted-func call was
//! reported E3071 (and an adopting host whose only funcs were adopted was
//! reported E5253) while the very same call instantiated cleanly — the
//! instrumentation in log/9.25.u291-adopted-dispatch.md shows the miss
//! arriving with `own_funcs=0 adopts=[DecoupledPower]`.
//!
//! E3071/E5253 are check-face-only: `mcc parse` never emits them (all four
//! pre/post × adopted/local code sets are identical there), so these locks
//! drive `mcc check` against a temp file, same harness shape as
//! `product_order.rs::run_mcc`. Each test asserts only its target codes;
//! the unrelated warnings of the probe (`net` unresolved, func-body
//! instance noise) are tolerated.

use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

const SRC_ADOPTED: &str = r#"
recipe DecoupledPower { psnk [VCC, GND]
    func Bypass(vcc, gnd) { Byp::CAP(100nF, 10V)
        vcc - Byp - gnd } }
component U291Target :: DecoupledPower { psnk [VDD, GND] }
module main { U291Target u
    net vcc
    net gnd
    u.Bypass(vcc, gnd) }
"#;

const SRC_MISSING_METHOD: &str = r#"
recipe DecoupledPower { psnk [VCC, GND]
    func Bypass(vcc, gnd) { Byp::CAP(100nF, 10V)
        vcc - Byp - gnd } }
component U291Target :: DecoupledPower { psnk [VDD, GND] }
module main { U291Target u
    net vcc
    net gnd
    u.Nope(vcc, gnd) }
"#;

/// Per-call counter: the three tests of this file run in parallel in one
/// process, so a pid-only temp name would let them overwrite each other's
/// probe (the two absence locks would then pass vacuously).
static PROBE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Run `mcc check --local -f json` over `source` written to a private temp
/// file and return the pass 0 diagnostics array.
fn check(source: &str) -> Vec<Value> {
    let seq = PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("u291-{}-{seq}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let file = dir.join("main.mc");
    std::fs::write(&file, source).expect("write probe");
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "check",
            file.to_str().expect("utf8 temp path"),
            "--local",
            "-f",
            "json",
        ])
        .output()
        .expect("run mcc check");
    let _ = std::fs::remove_dir_all(&dir);
    // `mcc check` exits non-zero whenever any diagnostic has a failing
    // severity — including the armed-rule control below — so the JSON on
    // stdout, not the status, is the product read here.
    assert!(
        !output.stdout.is_empty(),
        "mcc check produced no JSON; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value =
        serde_json::from_slice(&output.stdout).expect("check JSON output");
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("pass 0 diagnostics")
        .clone()
}

fn codes(diags: &[Value], code: u64) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d["code"].as_u64() == Some(code))
        .map(|d| d["message"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// The adopted call must not be reported E3071 — the member face sees the
/// same effective method set the Pass2 dispatch sees. Before b4000 this
/// fired `function 'Bypass' not found in class 'U291Target'`.
#[test]
fn lock_check__adopted_func_call_resolves_at_member_face() {
    let diags = check(SRC_ADOPTED);
    let hits = codes(&diags, 3071);
    assert!(
        hits.is_empty(),
        "an adopted-func call must not fire E3071: {:?}",
        hits
    );
}

/// Same root, second face: an adopting host whose only funcs are adopted is
/// not an empty component (E5253 must not fire). Before b4000 the lint read
/// only the host's own `funcs` container.
#[test]
fn lock_check__adopted_only_host_not_reported_empty() {
    let diags = check(SRC_ADOPTED);
    let hits = codes(&diags, 5253);
    assert!(
        hits.is_empty(),
        "an adopting host with adopted funcs must not fire E5253: {:?}",
        hits
    );
}

/// The rule stays armed: a method the host neither owns nor adopts is still
/// E3071 (the miss path every lock above depends on staying a real miss).
#[test]
fn lock_check__unknown_method_still_fires_3071() {
    let diags = check(SRC_MISSING_METHOD);
    let hits = codes(&diags, 3071);
    assert!(
        hits.len() == 1 && hits[0].contains("Nope"),
        "a truly missing method must fire exactly one E3071; got: {:?}",
        hits
    );
}

/// Same declared face, third consumer: `show nets OWNER.FUNC` must find an
/// adopted func (`find_func_by_path` → `effective_method`), not only own
/// ones. Before b4002 the show face read only the host's own `funcs`
/// container and `OWNER.FUNC` fell through to not-applicable.
#[test]
fn lock_show__adopted_func_visible_via_owner_func_path() {
    let seq = PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("u291-{}-show-{seq}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let file = dir.join("main.mc");
    std::fs::write(&file, SRC_ADOPTED).expect("write probe");
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "show",
            "nets",
            "U291Target.Bypass",
            "-F",
            file.to_str().expect("utf8 temp path"),
            "-f",
            "json",
        ])
        .output()
        .expect("run mcc show nets");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "mcc show nets exited {:?}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value =
        serde_json::from_slice(&output.stdout).expect("show nets JSON output");
    let show = &value["result"]["show"];
    assert_eq!(
        show["kind"].as_str(),
        Some("func"),
        "U291Target.Bypass must resolve as a func through the adopted method set: {show}"
    );
    let nets = show["nets"].as_array().expect("nets array");
    assert!(
        !nets.is_empty(),
        "the adopted func's body connections must project: {show}"
    );
}
