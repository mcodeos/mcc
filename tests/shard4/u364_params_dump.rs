// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U364 batch A: the parameter-table dump face (`show param-table`).
//!
//! The wire shape is the binding-contract §2.1 dump — faces × rows × value
//! slots × cond × datasheet provenance — the schema mce's transcriber
//! (`models/src/dump.rs`) consumes. Every corpus value shape must round-trip:
//! `a ~ b` ranges, kvalue dict slots, `q@cond` envelope points (both the
//! numeric and the hyphen-joined bareword condition), axis words with
//! arguments kept verbatim, and `@ds(p, trust, cond)` provenance — at row
//! level AND per list item (U363 gap 3b's third-chain bag).

#![allow(non_snake_case)]

use crate::common;

use mcc::{ParamsDump, RowDump};
use serde_json::Value;

/// One load + dump run over an inline component source. The dump reads the
/// AST visit capture — the same substrate the CLI face re-parses for.
fn run_dump(src: &str) -> (Option<ParamsDump>, Vec<(u32, String)>) {
    common::reset();
    mcc::set_ast_visit_json(true);
    mcc::clear_ast_visit_json();
    mcc::mcc_load_from_string(&"a.mc".to_string(), src);
    let tree = mcc::take_ast_visit_json_for("a.mc");
    let diags = mcc::mcc_diagnose_all()
        .into_iter()
        .map(|d| (d.code, d.msg))
        .collect();
    let found = tree.as_ref().and_then(|t| mcc::dump_component("PROBE", t));
    (found, diags)
}

/// The first row whose key spells `key` (the call-target meta name).
fn row_of<'a>(dump: &'a ParamsDump, key: &str) -> &'a RowDump {
    dump.faces
        .iter()
        .flat_map(|f| f.rows.iter())
        .find(|r| r.key == key)
        .unwrap_or_else(|| panic!("no row keyed {key} in {dump:?}"))
}

/// The JSON projection must carry the same field names the mce fixture
/// spells (`models/fixtures/esp32h2.json`) — the cross-repo contract.
fn row_json(dump: &ParamsDump) -> Value {
    let mut faces = serde_json::to_value(dump).unwrap();
    faces["faces"][0]["rows"][0].take()
}

const SCAFFOLD: &str = r#"component PROBE
{
    pins = [
        psnk [[18], [33]] = [VBAT, GND]::DC(
ROWS
        )
    ]
}
"#;

fn scaffold(rows: &str) -> String {
    SCAFFOLD.replace("ROWS", rows)
}

/// The headline shapes: range, kvalue dict slot, per-item annotated calls,
/// envelope points over bareword conditions, axis words with arguments.
#[test]
fn u364_dump__face_shell_and_row_shapes() {
    let _guard = common::lock();
    let rows = r#"            vin  = supply_range(3V ~ 3.6V)   @ds(p=49, trust=max)
            vmax = absmax(-0.3V ~ 3.6V)        @ds(p=49, trust=max)
            idraw = [
                current_draw(mode = tx(20dBm), value = [peak: 140mA])
                                @ds(p=51, trust=max, cond="BLE, 3.3 V, 25 C")
                current_draw(mode = modem_sleep(96MHz), value = [10mA@periph-clk-off, 17mA@periph-clk-on])
                                @ds(p=51, trust=max, cond="CPU working")
                current_draw(mode = rx, value = 24mA@3.3V)
                                @ds(p=51, trust=max, cond="BLE")
            ]
"#;
    let (dump, diags) = run_dump(&scaffold(rows));
    assert!(
        diags.iter().all(|(c, _)| *c != 2082 && *c != 2083),
        "the scaffold must parse clean: {diags:?}"
    );
    let dump = dump.expect("the component must dump");

    // Face shell: kind word + rails in face order.
    assert_eq!(dump.faces.len(), 1, "one psnk face: {dump:?}");
    let face = &dump.faces[0];
    assert_eq!(face.kind, "psnk");
    assert_eq!(face.rails, ["VBAT", "GND"]);

    // Range row: verbatim halves, ds provenance from the row-level bag.
    let vin = row_of(&dump, "supply_range");
    assert_eq!(
        vin.value.range.as_deref(),
        Some(&["3V".to_string(), "3.6V".to_string()][..]),
        "range halves ride their own spelling"
    );
    let ds = vin.ds.as_ref().expect("row-level @ds survives");
    assert_eq!(ds.p, 49);
    assert_eq!(ds.trust.as_deref(), Some("max"));

    // Dict-slot row: the axis word stays verbatim, the slot keeps its word.
    let tx = row_of(&dump, "current_draw");
    assert_eq!(tx.axis.get("mode").map(String::as_str), Some("tx(20dBm)"));
    let slots = tx.value.slots.as_ref().expect("dict slots");
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].slot.as_deref(), Some("peak"));
    assert_eq!(slots[0].q, "140mA");
    assert_eq!(tx.cond.as_deref(), Some("BLE, 3.3 V, 25 C"));
    assert_eq!(tx.ds.as_ref().map(|d| d.p), Some(51));

    // The list expands: every item is its own row with its own bag.
    let draws: Vec<&RowDump> = dump
        .faces
        .iter()
        .flat_map(|f| f.rows.iter())
        .filter(|r| r.key == "current_draw")
        .collect();
    assert_eq!(draws.len(), 3, "three list items → three rows: {draws:?}");

    // Envelope points over bareword conditions keep the word whole.
    let sleep = draws[1];
    assert_eq!(
        sleep.axis.get("mode").map(String::as_str),
        Some("modem_sleep(96MHz)"),
        "the axis word with its argument, verbatim"
    );
    let slots = sleep.value.slots.as_ref().expect("envelope points");
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].q, "10mA");
    assert_eq!(slots[0].at.as_deref(), Some("periph-clk-off"));
    assert_eq!(slots[1].q, "17mA");
    assert_eq!(slots[1].at.as_deref(), Some("periph-clk-on"));
    assert_eq!(sleep.cond.as_deref(), Some("CPU working"));

    // A bare q@q point on a single-row call rides slots too.
    let rx = draws[2];
    let slots = rx.value.slots.as_ref().expect("the q@q point");
    assert_eq!(slots[0].q, "24mA");
    assert_eq!(slots[0].at.as_deref(), Some("3.3V"));

    // The JSON projection spells the fixture's field names.
    let j = row_json(&dump);
    assert_eq!(j["key"], "supply_range");
    assert!(j["value"]["range"].is_array());
    assert_eq!(j["ds"]["p"], 49);
}

/// Rows without a call value (`::DC(3.3V)` positional, bare scalars) are not
/// parameter rows and stay out of the dump; a call row with no `value`
/// argument still dumps its axis.
#[test]
fn u364_dump__non_rows_stay_out() {
    let _guard = common::lock();
    let rows = r#"            vcore = 3.3V
            idraw = current_draw(mode = off)
"#;
    let (dump, diags) = run_dump(&scaffold(rows));
    assert!(diags.is_empty(), "scaffold must be clean: {diags:?}");
    let dump = dump.expect("the component must dump");
    let keys: Vec<&str> = dump
        .faces
        .iter()
        .flat_map(|f| f.rows.iter())
        .map(|r| r.key.as_str())
        .collect();
    assert_eq!(keys, ["current_draw"], "the bare scalar is not a row: {keys:?}");
    let row = row_of(&dump, "current_draw");
    assert_eq!(row.axis.get("mode").map(String::as_str), Some("off"));
    assert!(row.value.q.is_none(), "no value argument, no value");
}
