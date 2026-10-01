// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the condition-axis gate (src/semantic/validation/cond_axis.rs,
//! U371 leg4 — the first consumer of a paired value's condition half):
//!   E5363 COND_AXIS_MIXED - rows of one key paired on different unit
//!         families (`@10km` next to `@25℃`) leave no single axis for a
//!         working-point selection (ruling 20, case c)
//!   silence - one axis family stays clean; a speed-grade ladder at ONE
//!         axis point stays clean (the K form is a point *set*: `25MHz@0.1m,
//!         50MHz@0.1m, …` is `sdio.mc`'s own legitimate spelling, and the
//!         corpus refuted this gate's first draft that warned on it); a
//!         single row and an axis-free row set stay clean.
//!
//! Each lock runs `mcc parse --code <src> ... -f json` through the real
//! binary (same harness as lock_pp_ratings) and asserts only on the
//! cond_axis codes; unrelated fixture noise is acceptable.

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

/// The cond_axis codes across both pass views.
fn gate_codes(value: &Value) -> Vec<u32> {
    ["pass0", "pass1"]
        .iter()
        .flat_map(|phase| {
            value["result"][phase]["diagnostics"]
                .as_array()
                .expect("pass diagnostics")
                .iter()
                .filter_map(|d| d["code"].as_u64().map(|c| c as u32))
                .collect::<Vec<_>>()
        })
        .filter(|c| *c == 5363 || *c == 5364)
        .collect()
}

const IFACE_HEAD: &str = "interface CAN(role)\n{\n    pins = [ 1 = H, 2 = L ]\n";

#[test]
fn cond_axis__mixed_axis_families_on_the_attr_face_warn_5363() {
    // Distance next to temperature under one key: no single axis to select
    // rows by, whatever envelope shape G2 later rules.
    let result = parse(&format!(
        "{IFACE_HEAD}    maxspeed = [10kbps@10km, 125kbps@25degC]\n}}\nmodule main\n{{\n    io X\n}}"
    ));
    assert!(
        gate_codes(&result).contains(&5363),
        "expected E5363 COND_AXIS_MIXED on a distance/temperature mix: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn cond_axis__one_axis_family_stays_silent() {
    // The corpus's own shape (can.mc): three rows, all on the length axis.
    let result = parse(&format!(
        "{IFACE_HEAD}    maxspeed = [10kbps@10km, 125kbps@500m, 1Mbps@40m]\n}}\nmodule main\n{{\n    io X\n}}"
    ));
    assert!(
        gate_codes(&result).is_empty(),
        "a single-axis row set must stay clean: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn cond_axis__speed_grade_ladder_at_one_point_stays_silent() {
    // The withdrawal lock: alternative capabilities at ONE axis point are
    // the K form's point-set semantics, not a defect (sdio.mc writes four
    // speed grades at the same 0.1m). The gate's first draft warned here;
    // the corpus refuted it — this lock keeps the refutation landed.
    let result = parse(&format!(
        "{IFACE_HEAD}    maxspeed = [25MHz@0.1m, 50MHz@0.1m, 100MHz@0.1m, 208MHz@0.1m]\n}}\nmodule main\n{{\n    io X\n}}"
    ));
    assert!(
        gate_codes(&result).is_empty(),
        "a same-point speed-grade ladder must stay clean: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn cond_axis__single_row_and_axis_free_rows_stay_silent() {
    // A lone paired row carries no comparison; an axis-free set matches any
    // declared axis (Pass C's axis-free supply). Neither is judged.
    let result = parse(&format!(
        "{IFACE_HEAD}    maxspeed = [20kbps]\n    rate = [10Mbps, 100Mbps]\n}}\nmodule main\n{{\n    io X\n}}"
    ));
    assert!(
        gate_codes(&result).is_empty(),
        "single-row and axis-free sets must stay clean: {:?}",
        gate_codes(&result)
    );
}

#[test]
fn cond_axis__mixed_axis_families_on_the_param_face_warn_5363() {
    // The parameter face: an instance argument set read through the tolerant
    // binder; the key is reported instance-qualified.
    let result = parse(
        "component CD(v::UV.VOLT)\n\
         {\n\
         \x20   pins = [ 1 = A ]\n\
         }\n\
         module main\n\
         {\n\
         \x20   io VDD\n\
         \x20   CD(v = [1A@5V, 2A@25degC]) c1\n\
         }",
    );
    assert!(
        gate_codes(&result).contains(&5363),
        "expected E5363 COND_AXIS_MIXED on an instance set argument: {:?}",
        gate_codes(&result)
    );
}
