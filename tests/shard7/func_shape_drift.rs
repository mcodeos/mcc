// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E3192 (FUNC_SHAPE_DRIFT, funcshape.rs): same-name funcs on different
//! components declaring different parameter shapes warn — the shape is the
//! call-site spelling contract (scalar vs Set never auto-fold; matching-rules
//! B table), so drift breaks every consumer that carries the spelling across
//! components. Adjudication: doc/ee/func-param-shape-design.md. The majority
//! shape is the reference; constructor funcs (name == component base name)
//! are exempt — a construction arity is per-component by design; value-slot
//! spelling drift (same net quantifiers, different `::UV` annotations) stays
//! silent — the lint judges quantifiers, not vocabularies.

use crate::common;

use std::collections::HashSet;

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    common::reset();
    let uri = "/mcc/func-shape-drift-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn sem_fshape__set_vs_scalar_drift_warns() {
    let _lock = common::lock();

    let src = "component FLASH(p)\n{\n    func Power([vdd, gnd]::DC(3.3V))\n    {\n        vdd - this.1\n        gnd - this.2\n    }\n}\ncomponent TC275(p)\n{\n    func Power(vio, vmem)\n    {\n        vio - this.1\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_SHAPE_DRIFT),
        "E3192 not emitted for set-vs-scalar drift; got codes: {codes:?}"
    );
}

#[test]
fn sem_fshape__same_shape_stays_silent() {
    let _lock = common::lock();

    // Both Power funcs take one Set formal — the corpus majority. Same
    // quantifier, no drift.
    let src = "component FLASH(p)\n{\n    func Power([vdd, gnd]::DC(3.3V))\n    {\n        vdd - this.1\n    }\n}\ncomponent MCU(p)\n{\n    func Power([vdda, gnda]::DC(1.2V))\n    {\n        vdda - this.1\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_SHAPE_DRIFT),
        "E3192 false positive on agreeing shapes; got codes: {codes:?}"
    );
}

#[test]
fn sem_fshape__constructor_funcs_exempt() {
    let _lock = common::lock();

    // Same-name-as-component funcs declare the construction arity — a
    // per-component contract by design, never a cross-component convention.
    let src = "component FLASH(p)\n{\n    func FLASH([v3v3, gnd]::DC(3.3V))\n    {\n        v3v3 - this.1\n    }\n}\ncomponent SENSOR(p)\n{\n    func SENSOR(vrail)\n    {\n        vrail - this.1\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_SHAPE_DRIFT),
        "E3192 must not judge constructor funcs; got codes: {codes:?}"
    );
}

#[test]
fn sem_fshape__value_slot_spelling_drift_stays_silent() {
    let _lock = common::lock();

    // Same net quantifiers (net,net), different value spellings — the lint
    // judges quantifiers, not vocabularies.
    let src = "component A1(p)\n{\n    func Adjust(out, vsel::UV.VOLT)\n    {\n        out - this.1\n    }\n}\ncomponent B2(p)\n{\n    func Adjust(out, vset::UV.VOLT)\n    {\n        out - this.1\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_SHAPE_DRIFT),
        "E3192 false positive on value-slot spelling drift; got codes: {codes:?}"
    );
}
