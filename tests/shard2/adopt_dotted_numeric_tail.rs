// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U149: the adoption side of a dotted interface name must keep the numeric
//! tail. The grammar casts `UART.RS485.3`'s numeric segment into an
//! `OPD_DOT`-linked `MCAST_INT`, which `McIds::new` silently dropped — every
//! binding-side extraction then resolved the family `UART.RS485` (2-wire) and
//! the `.3` spelling (3-wire) either errored against the wrong width or, when
//! the base happened to match, sailed through silently. The extraction paths
//! now use `McIds::new_with_dot` (b3646); these locks pin both verdicts of
//! the discriminator, neither of which the pre-fix binary satisfies.
//!
//! Same harness as `ctor_arg_family.rs`: `mcc parse --code … -f json`, each
//! test asserts only its target code; extra diagnostics tolerated.

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

/// The pin names the top module's first component instance ended up with.
fn instance_pins(value: &Value) -> Vec<String> {
    value["result"]["pass2"]["instances"]["components"]
        .as_array()
        .expect("pass2 components")
        .iter()
        .flat_map(|c| c["pins"].as_array().expect("pins").iter())
        .map(|p| p["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A name family with a 2-wire base and a 3-wire `.3` member, in the real
/// library's shape (anonymous conductor lanes, role tables naming them).
const FAMILY: &str = r#"interface UART.RS485(role)
{
    pins = [
        1 = _
        2 = _
    ]
    role Master { pins = [1 = A, 2 = B] peer = Slave }
    role Slave { pins = [1 = A, 2 = B] peer = Master }
}

interface UART.RS485.3(role)
{
    pins = [
        1 = _
        2 = _
        3 = _
    ]
    role Master { pins = [1 = A, 2 = B, 3 = GND] peer = Slave }
    role Slave { pins = [1 = A, 2 = B, 3 = GND] peer = Master }
}

"#;

/// Adoption body with the given pin-id list against the `.3` spelling.
fn adopt(ids: &str) -> String {
    format!(
        "{FAMILY}component FLASH\n{{\n    name = \"FLASH\"\n    pins = [\n        [{ids}] = U3::UART.RS485.3(Master)\n    ]\n}}\n\nmodule main\n{{\n    FLASH f\n}}\n"
    )
}

/// Three pins against the 3-member `.3` family: the exact spelling resolves
/// exactly — no E3111, and the three members carry the `.3` prefix. Before
/// b3646 the tail was dropped, the 2-wire base answered, and this width
/// errored against the wrong family.
#[test]
fn lock_pp_bind__adopt_numeric_tail_three_pins_clean() {
    let result = parse(&adopt("10,11,12"));
    let hits = codes_with(&result, 3111);
    assert!(
        hits.is_empty(),
        "a 3-pin adoption of the 3-member .3 family must not fire E3111: {:?}",
        hits
    );
    let pins = instance_pins(&result);
    assert_eq!(
        pins,
        vec![
            "U3._(1)".to_string(),
            "U3._(2)".to_string(),
            "U3._(3)".to_string()
        ],
        "members must come from the .3 family, not the 2-wire base"
    );
}

/// The discriminator: two pins against the same `.3` spelling must fire
/// E3111 naming the three members. Before b3646 this resolved to the
/// 2-wire base, the widths matched, and the cell was silently green —
/// the silent fallback this family of locks exists to keep dead.
#[test]
fn lock_pp_bind__adopt_numeric_tail_two_pins_fires_3111() {
    let result = parse(&adopt("10,11"));
    let hits = codes_with(&result, 3111);
    assert!(
        hits.len() == 1,
        "a 2-pin adoption of the 3-member .3 family must fire exactly one E3111; got: {:?}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(
        hits[0].contains("3 pin(s)"),
        "E3111 must count the .3 family's members, not the base's: {}",
        hits[0]
    );
}
