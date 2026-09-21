// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The interface checks' lookup face is DomainFilter::Any, not Project: the
//! role-bearing population lives mostly in the system library, so a
//! Project-only lookup (a) turns every valid lib-interface role binding into
//! a spurious E4106 IFACE_NOT_LOADED, (b) misreports a wrong lib role as
//! E4106 instead of E4104 IFACE_ROLE_NOT_FOUND (hiding the available-roles
//! list), and (c) lets a literal role arg against a lib interface bypass
//! E4185 IFACE_ROLE_ARG_LITERAL entirely. All three were measured on
//! `DBG.UARTBOOT` before the widening.
//!
//! These locks bind the installed system library's `DBG.UARTBOOT`
//! (roles `Host`/`Target`) — the same runtime root
//! `shard3::system_lib_reload` already relies on.

use serde_json::Value;
use std::process::Command;

/// Run `mcc parse --code <source> --local --pass1 --pass2 --top main -f json`
/// and return the parsed JSON result.
fn parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", source, "--local", "--pass1", "--pass2", "--top", "main", "-f",
            "json",
        ])
        .output()
        .expect("run mcc parse");
    assert!(
        output.status.success(),
        "mcc parse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse mcc JSON output")
}

fn codes_with(value: &Value, code: u64) -> Vec<String> {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
        .iter()
        .filter(|d| d["code"].as_u64() == Some(code))
        .map(|d| d["message"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn comp_with_role(role_arg: &str) -> String {
    format!(
        "component C(u::DBG.UARTBOOT({}))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n",
        role_arg
    )
}

// A valid role against a system-library interface resolves silently — the
// pre-widening behavior was a spurious E4106 "interface not loaded".
#[test]
fn lock_pp_interface__lib_iface_valid_role_no_not_loaded() {
    let result = parse(&comp_with_role("Host"));
    assert!(
        codes_with(&result, 4106).is_empty(),
        "valid lib-interface role must not fire E4106"
    );
    assert!(
        codes_with(&result, 4104).is_empty(),
        "valid lib-interface role must not fire E4104: {:?}",
        codes_with(&result, 4104)
    );
}

// A role the lib interface does define-versus-not is judged with the right
// code: unknown role on a loaded lib interface is E4104 naming the available
// roles — not E4106 (the pre-widening misclassification).
#[test]
fn lock_pp_interface__lib_iface_bad_role_reports_4104_not_4106() {
    let result = parse(&comp_with_role("NOSUCHROLE"));
    let hits = codes_with(&result, 4104);
    assert!(
        hits.len() == 1,
        "unknown lib role must fire E4104 exactly once: {:?}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("DBG.UARTBOOT") && hits[0].contains("Available roles"),
        "E4104 must name the interface and list available roles: {}",
        hits[0]
    );
    assert!(
        codes_with(&result, 4106).is_empty(),
        "a loaded lib interface must not be reported as not-loaded"
    );
}

// The E4106 arm stays reachable for a name no definition answers to.
#[test]
fn lock_pp_interface__unknown_iface_still_not_loaded() {
    let result = parse(&comp_with_role("Host").replace("DBG.UARTBOOT", "NOSUCH.DEF"));
    let hits = codes_with(&result, 4106);
    assert!(
        hits.len() == 1,
        "a truly unloaded interface must fire E4106 exactly once: {:?}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// A literal in the role position against a lib interface fires E4185 — the
// pre-widening Project-only role-bearing set let it bypass silently.
#[test]
fn lock_pp_interface__lib_iface_literal_role_arg_fires_4185() {
    let result = parse(&comp_with_role("\"Host\""));
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "literal role arg on a lib interface must fire E4185 exactly once: {:?}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("DBG.UARTBOOT"),
        "E4185 must name the interface: {}",
        hits[0]
    );
}
