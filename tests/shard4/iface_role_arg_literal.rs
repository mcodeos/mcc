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

// ── Pins-row face (U144 third slice) ──
//
// `io p = TAG::TAG(..)` inside a component `pins` block binds through McPins,
// never through `classify_declare`, so the param-face locks above do not
// exercise it. The port retains the raw constructor args, and against a
// role-bearing interface every argument is a role reference: a literal is
// E4185, a bare name that matches no role is E4104.

// Quoted role arg on a pins row: E4185 fires (it sailed clean before this
// gate — probe evidence in mcd log/9.20.u144-ctor-arg-family-gate.md).
#[test]
fn lock_pp_interface__pins_row_role_arg_quoted_4185_fires() {
    let source = format!(
        "{IFACE}\ncomponent C\n{{\n    name = \"C\"\n    pins = [\n        io p = TAG::TAG(\"Master\")\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "expected exactly one E4185; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains('C') && hits[0].contains("TAG"),
        "E4185 must name the component and the interface: {}",
        hits[0]
    );
}

// Numeric role arg on a pins row is the same literal family.
#[test]
fn lock_pp_interface__pins_row_role_arg_number_4185_fires() {
    let source = format!(
        "{IFACE}\ncomponent C\n{{\n    name = \"C\"\n    pins = [\n        io p = TAG::TAG(123)\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "expected exactly one E4185; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// A bare name that matches no role is E4104's miss — the rule the param face
// already enforces, which the pins face never had (it checked clean before).
#[test]
fn lock_pp_interface__pins_row_misspelled_role_4104_fires() {
    let source = format!(
        "{IFACE}\ncomponent C\n{{\n    name = \"C\"\n    pins = [\n        io p = TAG::TAG(Masterr)\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4104);
    assert!(
        hits.len() == 1,
        "expected exactly one E4104; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("Masterr"),
        "E4104 must name the offending role: {}",
        hits[0]
    );
}

// The bare valid spelling is the fixed form: neither gate fires.
#[test]
fn lock_pp_interface__pins_row_bare_valid_role_clean() {
    let source = format!(
        "{IFACE}\ncomponent C\n{{\n    name = \"C\"\n    pins = [\n        io p = TAG::TAG(Master)\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    for code in [4185, 4104] {
        assert!(
            codes_with(&result, code).is_empty(),
            "E{code} must not fire on the bare valid spelling; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
    }
}

// An empty argument list on a role-bearing interface stays unjudged: whether
// a role is required there is E4104/E4184's business, already settled on
// their own faces (`GPIO[3,4]::GPIO()` is the corpus norm).
#[test]
fn lock_pp_interface__pins_row_empty_args_clean() {
    let source = format!(
        "{IFACE}\ncomponent C\n{{\n    name = \"C\"\n    pins = [\n        io p = TAG::TAG()\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    for code in [4185, 4104] {
        assert!(
            codes_with(&result, code).is_empty(),
            "E{code} must not fire on an empty argument list; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
    }
}

// A role-less interface's pins-row value args stay legal (the corpus form
// `XTAL::XTAL(32kHz)`).
#[test]
fn lock_pp_interface__pins_row_role_less_value_arg_clean() {
    let source = "interface VDC()\n{\n    pins = [\n        1 = P, \"pin\"\n    ]\n}\n\ncomponent C\n{\n    name = \"C\"\n    pins = [\n        io p = VDC::VDC(3.3V)\n    ]\n}\n\nmodule main\n{\n    io VDD\n}\n";
    let result = parse(source);
    for code in [4185, 4104] {
        assert!(
            codes_with(&result, code).is_empty(),
            "E{code} must not fire on a role-less interface's value arg; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
    }
}

// Module-body instance face (U144 fourth slice): a body row `io T0::TAG(arg)`
// lands as an McInstance::Interface in McModule.insts — it reaches neither
// classify_declare nor a pins block, so the earlier slices could not see it.
// Same judging rule as the pins face.

// Quoted role arg on a module-body instance: E4185 fires.
#[test]
fn lock_pp_interface__module_inst_role_arg_quoted_4185_fires() {
    let source = format!(
        "{IFACE}\nmodule main\n{{\n    io T0::TAG(\"Master\")\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "expected exactly one E4185; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("T0") && hits[0].contains("TAG") && hits[0].contains("Master"),
        "E4185 must name the instance, the interface and the literal: {}",
        hits[0]
    );
}

// Numeric role arg is the same literal family.
#[test]
fn lock_pp_interface__module_inst_role_arg_number_4185_fires() {
    let source = format!("{IFACE}\nmodule main\n{{\n    io T0::TAG(123)\n}}\n");
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1,
        "expected exactly one E4185; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// A bare name matching no role is E4104's miss — the module-body face never
// checked it before this gate.
#[test]
fn lock_pp_interface__module_inst_misspelled_role_4104_fires() {
    let source = format!("{IFACE}\nmodule main\n{{\n    io T0::TAG(Mastr)\n}}\n");
    let result = parse(&source);
    let hits = codes_with(&result, 4104);
    assert!(
        hits.len() == 1,
        "expected exactly one E4104; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("Mastr") && hits[0].contains("TAG"),
        "E4104 must name the misspelled role and the interface: {}",
        hits[0]
    );
}

// The bare spelling with a valid role is the fixed form: clean on both codes.
#[test]
fn lock_pp_interface__module_inst_bare_valid_role_clean() {
    let source = format!("{IFACE}\nmodule main\n{{\n    io T0::TAG(Master)\n}}\n");
    let result = parse(&source);
    for code in [4185, 4104] {
        assert!(
            codes_with(&result, code).is_empty(),
            "E{code} must not fire on a valid bare role; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
    }
}

// An empty argument list stays unjudged (E4104/E4184 territory).
#[test]
fn lock_pp_interface__module_inst_empty_args_clean() {
    let source = format!("{IFACE}\nmodule main\n{{\n    io T0::TAG()\n}}\n");
    let result = parse(&source);
    for code in [4185, 4104] {
        assert!(
            codes_with(&result, code).is_empty(),
            "E{code} must not fire on an empty argument list; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
    }
}

// Bracket members are port labels, not constructor args — the anon-bus form
// `[VDD,GND]::DC(3.3V)` must sail past (insts.rs precedent).
#[test]
fn lock_pp_interface__module_inst_anon_bracket_clean() {
    let source = "interface VDC()\n{\n    pins = [\n        1 = P, \"pin\"\n    ]\n}\n\nmodule main\n{\n    io [VDD,GND]::DC(3.3V)\n}\n";
    let result = parse(source);
    for code in [4185, 4104] {
        assert!(
            codes_with(&result, code).is_empty(),
            "E{code} must not fire on a bracket-member bind; diagnostics: {}",
            result["result"]["pass0"]["diagnostics"]
        );
    }
}

// Mixed-arg A4 escape (U144, ruled b3643): `TAG(123, Master)` used to lose
// the literal — InterfaceWithRole kept only the role value. The classifier
// now retains the literals and the gate judges them, in either order.
#[test]
fn lock_pp_interface__component_mixed_arg_literal_first_4185_fires() {
    let source = format!(
        "{IFACE}\ncomponent C(u::TAG(123, Master))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1 && hits[0].contains("123"),
        "expected one E4185 naming the literal; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn lock_pp_interface__component_mixed_arg_literal_last_4185_fires() {
    let source = format!(
        "{IFACE}\ncomponent C(u::TAG(Master, 123))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1 && hits[0].contains("123"),
        "expected one E4185 naming the literal; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// The module-port face judges the same A4 literals (role-less conduits).
#[test]
fn lock_pp_interface__module_port_mixed_arg_4185_fires() {
    let source = format!(
        "{IFACE}\nmodule main(io bus::TAG(Master, 123))\n{{\n    bus - bus\n}}\n"
    );
    let result = parse(&source);
    let hits = codes_with(&result, 4185);
    assert!(
        hits.len() == 1 && hits[0].contains("123"),
        "expected one E4185 naming the literal; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

// A bare role arg plus nothing else stays the legal spelling: no fire.
#[test]
fn lock_pp_interface__component_mixed_arg_bare_only_clean() {
    let source = format!(
        "{IFACE}\ncomponent C(u::TAG(Master))\n{{\n    name = \"C\"\n    pins = [\n        1 = X, \"x\"\n    ]\n}}\n\nmodule main\n{{\n    io VDD\n}}\n"
    );
    let result = parse(&source);
    assert!(
        codes_with(&result, 4185).is_empty(),
        "a single bare role arg must stay legal; diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}
