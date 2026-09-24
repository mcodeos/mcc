// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the ratings gate (src/semantic/validation/ratings.rs, PostParse):
//!   E5361 RATING_PARAM_OUT_OF_RANGE - an instance's bound parameter value
//!          (explicit argument or the declaration's default) falls outside
//!          the interval its class's `ratings` clause declares
//!   E5362 RATING_KEY_NOT_A_PARAM    - a ratings entry key names no
//!          constructor parameter of its class
//!   E5360 ATTR_VALUE_NOT_IN_VOCABULARY (reused) - a ratings bound side word
//!          outside the closed {low, high} vocabulary
//! Each lock runs `mcc parse --code <src> ... -f json` through the real
//! binary and asserts on the PostParse validation codes, ratings-gated only:
//! unrelated extra diagnostics are acceptable, but a lock that also fires
//! E5361 on an in-bounds fixture is a false gate.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

fn parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args(&[
            "parse", "--code", source, "--local", "--pass1", "--pass2", "--top", "main", "-f",
            "json",
        ])
        .output()
        .expect("run mcc parse");
    assert!(
        output.status.success(),
        "mcc parse exited {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse mcc JSON output")
}

fn gate_codes(value: &Value) -> Vec<u32> {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
        .iter()
        .filter_map(|d| d["code"].as_u64().map(|c| c as u32))
        .filter(|c| (5360..5400).contains(c))
        .collect()
}

#[test]
fn lock_pp_ratings__explicit_arg_over_high_bound_emits_5361() {
    // §3.2 closed interval - `36V` exceeds `high:30V` on the same formal the
    // argument binds to by unit claiming.
    let result = parse(
        "component LDO (vin::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = IN ]\n\
         \x20   ratings = [ vin:[low:0V, high:30V] ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   LDO r1(36V)\n\
         }",
    );
    assert!(
        gate_codes(&result).contains(&5361),
        "expected E5361 RATING_PARAM_OUT_OF_RANGE: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn lock_pp_ratings__explicit_arg_under_low_bound_emits_5361() {
    // §3.2 single-sided bound - `low:0.8V` alone, `0.5V` below it.
    let result = parse(
        "component LDO (vout::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = OUT ]\n\
         \x20   ratings = [ vout:[low:0.8V] ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   LDO r1(0.5V)\n\
         }",
    );
    assert!(
        gate_codes(&result).contains(&5361),
        "expected E5361 RATING_PARAM_OUT_OF_RANGE: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn lock_pp_ratings__default_out_of_range_emits_5361() {
    // §3.2 - the declaration's default must clear the bound too: a default
    // outside the ratings is a declaration-time error, reported at every
    // instance (here the instance passes no argument at all).
    let result = parse(
        "component LDO (vout::UV.VOLT = 0.5V)\n\
         {\n\
         \x20   pins = [ 1 = OUT ]\n\
         \x20   ratings = [ vout:[low:0.8V] ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   LDO r1\n\
         }",
    );
    assert!(
        gate_codes(&result).contains(&5361),
        "expected E5361 RATING_PARAM_OUT_OF_RANGE for the default: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn lock_pp_ratings__unit_prefixes_normalise_into_base_units() {
    // §3.2 - `2500mV` normalises to 2.5V, below `low:3V`; the gate compares
    // base-unit magnitudes, not the written prefixes.
    let result = parse(
        "component LDO (vin::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = IN ]\n\
         \x20   ratings = [ vin:[low:3V] ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   LDO r1(2500mV)\n\
         }",
    );
    assert!(
        gate_codes(&result).contains(&5361),
        "expected E5361 RATING_PARAM_OUT_OF_RANGE after normalisation: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn lock_pp_ratings__boundary_value_stays_inside_closed_interval() {
    // §3.2 closed bounds - exactly at `high:30V` is inside; the gate must
    // stay silent (no 536x at all).
    let result = parse(
        "component LDO (vin::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = IN ]\n\
         \x20   ratings = [ vin:[low:0V, high:30V] ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   LDO r1(30V)\n\
         }",
    );
    assert!(
        gate_codes(&result).is_empty(),
        "expected no ratings diagnostics at the boundary: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn lock_pp_ratings__entry_key_names_no_formal_emits_5362() {
    // §8-1 - an entry key that resolves to no constructor formal is
    // reported, never silently read as "no declaration" - a silent no-op
    // is a defect, not a tolerance.
    let result = parse(
        "component LDO (vin::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = IN ]\n\
         \x20   ratings = [ vinx:[low:0V] ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         }",
    );
    assert!(
        gate_codes(&result).contains(&5362),
        "expected E5362 RATING_KEY_NOT_A_PARAM: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn lock_pp_ratings__side_word_outside_vocabulary_emits_5360() {
    // §2.1 - the closed side vocabulary is {low, high} (U243①); a typo'd
    // side must not widen the bound by silence. The readable `high` side
    // still judges (31V > 30V), so both codes fire.
    let result = parse(
        "component LDO (vin::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = IN ]\n\
         \x20   ratings = [ vin:[lo:0V, high:30V] ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   LDO r1(31V)\n\
         }",
    );
    let codes = gate_codes(&result);
    assert!(
        codes.contains(&5360),
        "expected E5360 ATTR_VALUE_NOT_IN_VOCABULARY for the side word: {:?}",
        codes
    );
    assert!(
        codes.contains(&5361),
        "expected E5361 to still judge the readable side: {:?}",
        codes
    );
}

#[test]
fn lock_pp_ratings__class_without_ratings_is_never_judged() {
    // §2.2 - ratings only constrains the formals its clause declares; a
    // class without the clause keeps the free-parameter behaviour.
    let result = parse(
        "component LDO (vin::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = IN ]\n\
         }\n\
         \n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   LDO r1(999V)\n\
         }",
    );
    assert!(
        gate_codes(&result).is_empty(),
        "expected no ratings diagnostics without the clause: {:?}",
        gate_codes(&result)
    );
}
