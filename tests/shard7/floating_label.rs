// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! E3136 (FUNC_FLOATING_LABEL): a bare identifier in a func body net stmt that
//! resolves to no declared pin / interface / param member / func-local instance
//! becomes a dangling net label. The criterion is positional — the reference
//! must land on a container terminal — and the count rule is per stream: in
//! func bodies the miss reports whatever its reference count (a second func
//! writing the same spelling shares a net that was never declared), while at a
//! module's top level a label tagged at two endpoints is the via-label idiom
//! and stays silent; a single stub still reports.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashSet;

/// Build `src` in a fresh workspace and return the emitted diagnostic codes.
fn build_codes(src: &str) -> HashSet<u32> {
    common::reset();
    let uri = "/mcc/floating-label-test.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&mcc::McIds::from("main"), &uri);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

#[test]
fn sem_flabel__dangling_net_endpoint_warns() {
    let _lock = common::lock();

    // `DC` is used once, resolves to nothing declared → floating label.
    let src = "component FLT(pwr)\n{\n    func F(pwr)\n    {\n        pwr -> DC\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 not emitted for a dangling net endpoint; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__declared_pins_params_func_local_do_not_warn() {
    let _lock = common::lock();

    // `pwr` is a param, `VIN` is a pin, `R1`/`R2` are func-local declares →
    // every bare name resolves to something declared.
    let src = "component OK(pwr)\n{\n    pins = [\n        in 1 = VIN\n    ]\n    func F(pwr)\n    {\n        RES R[1:2](5.1kΩ)\n        pwr -> R1 -> R2 -> VIN\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 false positive on declared names; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__shared_undeclared_net_across_funcs_warns() {
    let _lock = common::lock();

    // `VSW` is written in both funcs and declared nowhere. Two funcs joining the
    // same spelling does not declare a net — it invents one — so this is the
    // LDO2/LDO3 shape and it warns (compat-period verdict: warning, not error).
    let src = "component SHARED(pwr)\n{\n    pins = [\n        in 1 = VA\n        in 2 = VB\n    ]\n    func A(pwr)\n    {\n        VSW -> VA\n    }\n    func B(pwr)\n    {\n        VB -> VSW\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 expected for an undeclared net shared by two funcs; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__declared_shared_net_does_not_warn() {
    let _lock = common::lock();

    // The same two-func shape with `VSW` declared as a pin: the reference lands
    // on a container terminal, so every writing of it is legitimate.
    let src = "component DECLARED(pwr)\n{\n    pins = [\n        in 1 = VA\n        in 2 = VB\n        in 3 = VSW\n    ]\n    func A(pwr)\n    {\n        VSW -> VA\n    }\n    func B(pwr)\n    {\n        VB -> VSW\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 false positive on a declared terminal; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__repeated_reference_in_one_func_warns() {
    let _lock = common::lock();

    // Both references sit in one func: the count is irrelevant to a positional
    // criterion, the name still lands on no terminal.
    let src = "component REPEAT(pwr)\n{\n    pins = [\n        in 1 = VA\n        in 2 = VB\n    ]\n    func F(pwr)\n    {\n        VSW -> VA\n        VB -> VSW\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 expected for a twice-written undeclared name in one func; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__call_receiver_does_not_warn() {
    let _lock = common::lock();

    // `ld` is only ever a method-call receiver (`ld.ldrop(...)`) — an inline
    // constructed instance, not a wire. E3136 must not flag it. (VA/VB are
    // declared pins so the call's arguments resolve; only `ld` is undeclared.)
    let src = "component RECV(pwr)\n{\n    pins = [\n        in 1 = VA\n        in 2 = VB\n    ]\n    func F(pwr)\n    {\n        DC.LDO() ld\n        ld.ldrop(VA, VB)\n    }\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 false positive on a call receiver; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__module_via_label_two_ends_stay_silent() {
    let _lock = common::lock();

    // U13 revision, 2026-09-21: at a module's top level a bare label tagged at
    // two endpoints is the via-label idiom — the same spelling on both ends is
    // the net, no middle wire — so two top-level endpoint references stay
    // silent and the net layer judges the net (the E3137 division of labor).
    let src = "module VIA()\n{\n    in A\n    in B\n    SPK -> A\n    SPK -> B\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        !codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 false positive on a module top-level via label; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__module_single_stub_label_warns() {
    let _lock = common::lock();

    // One top-level endpoint and nothing at the other end is still the typo
    // signal — the via-label exemption needs both ends written.
    let src = "module STUB()\n{\n    in A\n    SPK -> A\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 expected for a single-end stub label at module top level; got codes: {codes:?}"
    );
}

#[test]
fn sem_flabel__func_miss_reports_despite_module_top_writes() {
    let _lock = common::lock();

    // The func stream keeps the U13 rule: a bare name in a func body that
    // lands on no container terminal reports however often it is written —
    // two extra top-level writings do not rescue a func miss, because a func
    // reference that silently stays internal has no backstop anywhere else.
    let src = "module MIX()\n{\n    in A\n    in B\n    func F()\n    {\n        VSW -> A\n    }\n    VSW -> B\n    VSW - A\n}\nmodule main { io VDD }";
    let codes = build_codes(src);
    assert!(
        codes.contains(&mcc::errcodes::FUNC_FLOATING_LABEL),
        "E3136 expected for a func-body miss even with two top-level writings; got codes: {codes:?}"
    );
}
