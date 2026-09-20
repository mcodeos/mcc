// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E4185 IFACE_ROLE_ARG_LITERAL (U144 first slice): a role-bearing interface's
//! constructor argument must be a bare identifier. `TAG("Master")` / `TAG(123)`
//! classify as plain-parameter bindings, so the role is never recorded and
//! E4104 / E4184 are silently bypassed — this check makes the literal an
//! explicit error instead (ident-vs-literal ruling; evidence matrix in mcd
//! log/9.20.u143-ref-position-literal-audit.md).
//!
//! Same harness as `shard3/module_port_role_free.rs`: `mcc parse --code … -f
//! json`, each test asserts only its target code; extra diagnostics tolerated.

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

fn diagnostics(value: &Value) -> &[Value] {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
}

fn codes_with(value: &Value, code: u64) -> Vec<String> {
    diagnostics(value)
        .iter()
        .filter(|d| d["code"].as_u64() == Some(code))
        .map(|d| d["message"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A replicated interface with a role block, so the case needs no library.
const IFACE: &str = r#"interface TAG(role)
{
    pins = [
        1 = P, "signal"
    ]
    role Master
    {
        name = "Master role"
        peer = Slave
    }
    role Slave
    {
        name = "Slave role"
        peer = Master
    }
}
"#;

// Quoted role arg on a component param: E4185 fires and names the binding.
#[test]
fn lock_pp_interface__component_role_arg_quoted_4185_fires() {
    let source = format!(
        "{IFACE}\ncomponent C(u::TAG(\"Master\"))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "expected exactly one E4185; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains('C') && hits[0].contains("TAG") && hits[0].contains("\"Master\""),
        "E4185 must name the component, the interface and the literal: {}",
        hits[0]
    );
}

// Numeric role arg is the same literal family.
#[test]
fn lock_pp_interface__component_role_arg_number_4185_fires() {
    let source = format!(
        "{IFACE}\ncomponent C(u::TAG(123))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "expected exactly one E4185; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// The bare spelling is the fixed form: no E4185.
#[test]
fn lock_pp_interface__component_role_arg_bare_no_4185() {
    let source = format!(
        "{IFACE}\ncomponent C(u::TAG(Master))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.is_empty(),
        "bare role arg must not fire E4185: {:?}",
        hits
    );
}

// Quoted role arg on a module port: the same miss in an R3 position (the bare
// twin fires E4184; the literal twin must not sail past both gates).
#[test]
fn lock_pp_interface__module_port_role_arg_quoted_4185_fires() {
    let source = format!(
        "{IFACE}\nmodule main(io bus[1:2]::TAG(\"Master\"))\n{{\n    bus.1 - bus.2\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "expected exactly one E4185; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("bus") && hits[0].contains("TAG"),
        "E4185 must name the port and the interface: {}",
        hits[0]
    );
}

// A role-less interface takes value args; its literal args stay legal.
#[test]
fn lock_pp_interface__role_less_interface_literal_arg_no_4185() {
    let source = "interface VDC()\n{\n    pins = [\n        1 = P, \"pin\"\n    ]\n}\n\ncomponent C(p::VDC(3.3V))\n{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}\n\nmodule main\n{\n    io VDD\n}\n";
    let result = parse(source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.is_empty(),
        "role-less interface literal args must not fire E4185: {:?}",
        hits
    );
}

// An interface that does not resolve is E4106's business, not E4185's.
#[test]
fn lock_pp_interface__unknown_interface_literal_arg_no_4185() {
    let source = "component C(u::NOSUCH(\"Master\"))\n{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}\n\nmodule main\n{\n    io VDD\n}\n";
    let result = parse(source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.is_empty(),
        "unresolvable interface must not fire E4185: {:?}",
        hits
    );
}
