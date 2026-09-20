// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E4184 MODULE_PORT_IFACE_ROLE (replicated-binding-design R3): a module port
//! binding may not carry a role argument. Role (Controller / Peripheral /
//! Master / …) is the link identity of an endpoint terminal pin; a module port
//! is a role-less conductor whose identity is decided by whatever terminal it
//! is wired to on each side. `io bus[1:2]::TAG(Master)` fires; the same port
//! written `io bus[1:2]::TAG()` does not, and a component param with a valid
//! role (the 4104 family) stays clean.
//!
//! Same harness as `lock_pp_interface.rs`: `mcc parse --code … -f json`, each
//! test asserts only its target code; extra diagnostics are tolerated.

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

// E4184: role on a module port binding. `bus` is a conduit between the two
// endpoints of the chain, so `TAG(Master)` is a category error — the port
// does not join the chain as anyone.
#[test]
fn lock_pp_interface__module_port_role_4184_fires() {
    let source = format!(
        "{IFACE}\nmodule main(io bus[1:2]::TAG(Master))\n{{\n    bus.1 - bus.2\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4184);
    assert!(
        hits.len() == 1,
        "expected exactly one E4184; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("bus") && hits[0].contains("TAG") && hits[0].contains("Master"),
        "E4184 must name the port, the interface and the role: {}",
        hits[0]
    );
}

// The role-less spelling is the fixed form: no E4184.
#[test]
fn lock_pp_interface__module_port_role_free_no_4184() {
    let source =
        format!("{IFACE}\nmodule main(io bus[1:2]::TAG())\n{{\n    bus.1 - bus.2\n}}\n");
    let result = parse(&source);
    let hits = codes_with(&result, 4184);
    assert!(
        hits.is_empty(),
        "role-less port must not fire E4184: {:?}",
        hits
    );
}

// Role on a component param is the terminal-pin spelling and stays legal
// (this is the 4104 family's clean twin — valid role, nothing fires).
#[test]
fn lock_pp_interface__component_param_role_no_4184() {
    let source = format!(
        "{IFACE}\ncomponent C(u::TAG(Master))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4184);
    assert!(
        hits.is_empty(),
        "component terminal-pin role must not fire E4184: {:?}",
        hits
    );
}
