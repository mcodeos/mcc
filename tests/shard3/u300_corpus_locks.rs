// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U300 corpus bucket — valid/invalid zero-coverage locks (audit §4.1/§4.2,
//! `mcd/log/9.25.u300-grammar-alignment-audit.md`; b4006 already landed the
//! chained T3/I1/I3 locks in `u300_grammar_alignment.rs`, so this file covers
//! the remaining T1, T2, T4-T13 and I2, I4-I10 items).
//!
//! Valid-shape locks assert on the diagnostic *level* (no error-level
//! emissions, no parser diagnostics), because the empty-body / unused-port
//! warning family (W2115/W5252/W5253/W5254/W5641/W5642) legitimately fires on
//! minimal corpus fixtures. Every absence lock carries a presence
//! counterpart: the fixture mutated into the ruled-broken shape must fire the
//! code the valid shape must not fire.
//!
//! Fixture shapes were probed against the b4006 binary before pinning; two
//! audit-item corrections fell out of that probing and are noted inline
//! (T7's `role` formal is legal in func/interface headers but rejected at
//! value positions, and the `±` W2112 emitter lives in the cond/judge path).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McDiagnostic, McIds, McURI};

fn build_diags(src: &str) -> Vec<McDiagnostic> {
    let _lock = common::lock();
    common::reset();
    // SWITCH.BUTTON / RES / CAP live in the system library (~/.mcode/mcode).
    mcc::mcc_init();
    let uri: McURI = "/mcc/u300-corpus.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    let mut diags = mcc::mcc_diagnose_all();
    diags.sort_by_key(|d| d.code);
    diags
}

fn codes(diags: &[McDiagnostic]) -> Vec<u32> {
    diags.iter().map(|d| d.code).collect()
}

/// The valid-face assertion: nothing at error level, and no parser-family
/// (2xxx) diagnostic at all — warnings stay visible and allowed.
fn assert_clean(diags: &[McDiagnostic], what: &str) {
    let errors: Vec<u32> = diags
        .iter()
        .filter(|d| d.level == mcc::DiagnosticLevel::Error)
        .map(|d| d.code)
        .collect();
    assert!(
        errors.is_empty(),
        "{what} must not emit error-level diagnostics; got errors {errors:?} (all: {:?})",
        codes(diags)
    );
    // W2115 (empty body) is the legitimate residue of minimal corpus fixtures
    // and is warning-level; every other 2xxx diagnostic is a parse defect.
    let parser: Vec<u32> = codes(diags)
        .into_iter()
        .filter(|c| (2000..3000).contains(c) && *c != mcc::errcodes::PARSER_EMPTY_BODY)
        .collect();
    assert!(
        parser.is_empty(),
        "{what} must not emit parser diagnostics; got {parser:?} (all: {:?})",
        codes(diags)
    );
}

fn has(codes: &[u32], code: u32) -> bool {
    codes.contains(&code)
}

// T1 — top-level `bom` block (U267/U290 acceptance surface had zero corpus)

const SRC_T1: &str = r#"
bom B { mpn = "X" }
module top
{
    io A
}
"#;

#[test]
fn u300corpus__t1_top_level_bom_declaration_is_error_free() {
    let diags = build_diags(SRC_T1);
    assert_clean(&diags, "a minimal top-level bom block");
}

#[test]
fn u300corpus__t1_malformed_bom_body_still_reports_parse_error() {
    // Presence counterpart: a bom body with a bare undeclared keyword row is
    // not a legal attr row and must not go quiet.
    let src = SRC_T1.replace("mpn = \"X\"", "mpn");
    let codes = codes(&build_diags(&src));
    assert!(
        has(&codes, mcc::errcodes::PARSER_TOP_INVALID),
        "a malformed bom body row must fire E2081; got codes: {codes:?}"
    );
}

// T2 — capability adoption `component X :: CAP` (+ multi), invalid `:: CAP()`

const SRC_T2: &str = r#"
component CUT1 :: CAP
{
}
component CUT2 :: CAP, RES
{
}
module top
{
    io A
}
"#;

#[test]
fn u300corpus__t2_capability_adoption_single_and_multi_is_error_free() {
    let diags = build_diags(SRC_T2);
    assert_clean(&diags, "capability adoption (:: CAP and :: CAP, RES)");
}

#[test]
fn u300corpus__t2_parenthesized_capability_is_rejected() {
    // The invalid counterpart: `:: CAP()` has no grammar arm (the dcolon
    // capability position takes no argument list).
    let src = SRC_T2.replace("component CUT1 :: CAP\n", "component CUT1 :: CAP()\n");
    let codes = codes(&build_diags(&src));
    assert!(
        has(&codes, mcc::errcodes::PARSER_TOP_INVALID),
        "`:: CAP()` must fire E2081; got codes: {codes:?}"
    );
}

// T4 — U153 mixed port row (grammatical disambiguation, zero corpus)

const SRC_T4: &str = r#"
interface SPI
{
    pins = [
        1 = SCLK
        2 = MOSI
    ]
}

module top
{
    io MIC{P, N}, BUS::SPI(), UART0
}
"#;

#[test]
fn u300corpus__t4_mixed_port_row_is_error_free() {
    let diags = build_diags(SRC_T4);
    assert_clean(
        &diags,
        "the mixed port row (curly faces + interface instance + plain io)",
    );
}

// T5 — domain @class rail block (pwrint live shape, mc_rail zero corpus)

const SRC_T5: &str = r#"
module top
{
    conduit GND @role(main)

    domain DVDD @class(digital)
    {
        rail [VDD_3V3, GND]::DC(3.3V, tol:±5%, capacity:500mA, eff:0.95)
    }
}
"#;

#[test]
fn u300corpus__t5_domain_class_rail_block_is_error_free() {
    let diags = build_diags(SRC_T5);
    assert_clean(&diags, "the domain/@class/rail::DC block (pwrint live shape)");
}

// T6 — `<=` judge comparison (the only comparison op with zero coverage)

const SRC_T6: &str = r#"
module top
{
    func pick(count)
    {
        if (count <= 3)
        {
            return count
        }
        return count
    }
}
"#;

#[test]
fn u300corpus__t6_le_judge_in_cond_is_error_free() {
    let diags = build_diags(SRC_T6);
    assert_clean(&diags, "`count <= 3` in a func-body cond");
}

// T7 — role formal in func/interface headers. Probe correction: the audit's
// `func f(role)` host is right, but `role` cannot also appear at a value
// position (`return role` dies with E2082) — the body references a plain
// name instead, so W5641 (formal unused) is the tolerated residue. The
// presence counterpart pins the D8 gate: the same formal on a *component*
// header is the semantic rejection E5356.

const SRC_T7: &str = r#"
module top
{
    func f(role)
    {
        return x
    }
}
"#;

#[test]
fn u300corpus__t7_func_role_formal_is_error_free() {
    let diags = build_diags(SRC_T7);
    assert_clean(&diags, "`func f(role)`");
}

const SRC_T7_DEFAULT: &str = r#"
module top
{
    func f(role = MAIN)
    {
        return x
    }
}
"#;

#[test]
fn u300corpus__t7_func_role_formal_with_default_is_error_free() {
    let diags = build_diags(SRC_T7_DEFAULT);
    assert_clean(&diags, "`func f(role = MAIN)`");
}

#[test]
fn u300corpus__t7_interface_role_formal_is_error_free() {
    let src = "interface UART.TTL(role)\n{\n    pins = [\n        1 = VCC\n        2 = GND\n    ]\n}\n";
    let diags = build_diags(src);
    assert_clean(&diags, "`interface UART.TTL(role)`");
}

#[test]
fn u300corpus__t7_component_header_role_formal_is_gated() {
    // D8 counterpart: the role formal is interface/func header material; on a
    // component header the semantic gate fires.
    let codes = codes(&build_diags("component C(role)\n{\n    pins = [ 1 = A ]\n}\n"));
    assert!(
        has(&codes, 5356),
        "a component-header role formal must fire E5356; got codes: {codes:?}"
    );
}

// T8 — `&[a, b]` reference-vector formal (§6.1 form 13)

const SRC_T8: &str = r#"
component CUT(&[a, b])
{
}
"#;

#[test]
fn u300corpus__t8_reference_vector_formal_is_error_free() {
    let diags = build_diags(SRC_T8);
    assert_clean(&diags, "`component CUT(&[a, b])`");
}

// T9 — type/unit family: FLOAT, compound units, charge units, parenless
// func, named-bus return

const SRC_T9: &str = r#"
component C1(ratio::FLOAT = 1.5)
{
}
component C2(p::UV.VOLT*UV.AMP = 1)
{
}
component C3(q::UV.VOLT/UV.AMP = 1)
{
}
component C4(c::UV.CHARGE = 10mAh)
{
}
"#;

#[test]
fn u300corpus__t9_float_compound_and_charge_param_types_are_error_free() {
    let diags = build_diags(SRC_T9);
    assert_clean(&diags, "FLOAT / compound-unit / charge-unit param declares");
}

#[test]
fn u300corpus__t9_parenless_func_and_bus_return_are_error_free() {
    let src = "module top\n{\n    func f\n    {\n        return A\n    }\n    func f4()\n    {\n        return BUS{A}\n    }\n}\n";
    let diags = build_diags(src);
    assert_clean(&diags, "`func f {}` and `return BUS{A}`");
}

// T10 — use forms: version + alias + symbol import combine

#[test]
fn u300corpus__t10_use_version_alias_symbol_combine_keeps_2092_silent() {
    // Parse face only: `x@1.2.3` (dotted version), `as R` alias and `: s`
    // symbol import are orthogonal and combine. The file itself does not
    // exist, so load-time not-found diagnostics are tolerated — the lock is
    // on E2092 (version without a decimal point) staying silent.
    let codes = codes(&build_diags("use x@1.2.3 as R : s\nmodule top\n{\n    io A\n}\n"));
    assert!(
        !has(&codes, mcc::errcodes::PARSER_USE_INVALID),
        "`use x@1.2.3 as R : s` must not fire E2092; got codes: {codes:?}"
    );
}

#[test]
fn u300corpus__t10_dotless_version_reports_2092() {
    // Presence counterpart for the version form above.
    let codes = codes(&build_diags("use x@1\nmodule top\n{\n    io A\n}\n"));
    assert!(
        has(&codes, mcc::errcodes::PARSER_USE_INVALID),
        "`use x@1` must fire E2092 (version needs a decimal point); got codes: {codes:?}"
    );
}

// T11 — multi-record attr block + multi-group pin alias rows

const SRC_T11: &str = r#"
component C
{
    Rdson = [ a = 80mΩ, Vgs:-10V ]
    pins = [ 1 = A ]
}
"#;

#[test]
fn u300corpus__t11_multi_record_attr_block_is_error_free() {
    let diags = build_diags(SRC_T11);
    assert_clean(&diags, "the multi-record attr block (= and : records)");
}

const SRC_T11_ALIAS: &str = r#"
component C
{
    pins = [
        [1, 2] = [P, N] | [A, K]
    ]
}
"#;

#[test]
fn u300corpus__t11_multi_group_pin_alias_is_error_free() {
    let diags = build_diags(SRC_T11_ALIAS);
    assert_clean(&diags, "the multi-group alias row ([1,2]=[P,N] | [A,K])");
}

// T12 — cond block: multiple attr clauses + elif chain; the else-less chain
// keeps its E5452 info hint, which doubles as the presence signal

const SRC_T12: &str = r#"
component C
{
    pins = [ 1 = A ]
    a = 1
    if (a == 1)
    {
        x = 1
        y = 2
    }
    else if (a == 2)
    {
        x = 3
    }
}
"#;

#[test]
fn u300corpus__t12_cond_multi_clause_elif_chain_is_legal() {
    let diags = build_diags(SRC_T12);
    let codes = codes(&diags);
    assert!(
        has(&codes, mcc::errcodes::COND_IF_WITHOUT_ELSE),
        "the else-less chain must keep its E5452 info hint (presence signal); got codes: {codes:?}"
    );
    assert_clean(&diags, "the multi-clause elif chain (beyond the E5452 hint)");
}

// T13 — @role/@return as pins-row trailing attr keys

const SRC_T13: &str = r#"
component C
{
    pins = [
        1 = IO0 @role(main)
        2 = IO1 @return(A)
    ]
}
"#;

#[test]
fn u300corpus__t13_pins_row_role_return_tattr_keys_are_error_free() {
    let diags = build_diags(SRC_T13);
    assert_clean(&diags, "@role(...)/@return(...) on pins rows");
}

// I2 — E4216 replication count guard (`*1` / `×1` / `×0`)

#[test]
fn u300corpus__i2_degenerate_replication_counts_report_4216() {
    for (suffix, label) in [("*1", "*1"), ("×1", "×1"), ("×0", "×0")] {
        let codes = codes(&build_diags(&format!(
            "module top\n{{\n    io A\n    A - RES(1k){suffix} - B\n}}\n"
        )));
        assert!(
            has(&codes, mcc::errcodes::CONN_REPLICATION_COUNT),
            "`RES(1k){label}` must fire E4216; got codes: {codes:?}"
        );
    }
}

// I4 — single-quoted string cannot carry an inner quote (`'abc''s'` → E2082)

#[test]
fn u300corpus__i4_inner_single_quote_reports_2082() {
    let codes = codes(&build_diags("component C\n{\n    partno = 'abc''s'\n}\n"));
    assert!(
        has(&codes, mcc::errcodes::PARSER_CLAUSE_INVALID),
        "`'abc''s'` must fire E2082; got codes: {codes:?}"
    );
}

// I5 — bare `uv@uv` in a scalar attr slot → E3022. U299② (the AT arm in the
// attr evaluator) will re-home this lock when it lands.

#[test]
fn u300corpus__i5_bare_uv_at_uv_scalar_attr_reports_3022() {
    let codes = codes(&build_diags("component C\n{\n    rate = 1Mbps@0.5m\n}\n"));
    assert!(
        has(&codes, mcc::errcodes::ATTR_TYPE_NOT_SUPPORTED),
        "bare `uv@uv` in a scalar attr slot must fire E3022; got codes: {codes:?}"
    );
}

// I6 — E2083 line-level recovery shapes in pins rows

#[test]
fn u300corpus__i6_pin_row_recovery_shapes_report_2083() {
    for row in ["1 = {1:3}", "1+1 = A"] {
        let codes = codes(&build_diags(&format!(
            "component C\n{{\n    pins = [\n        {row}\n    ]\n}}\n"
        )));
        assert!(
            has(&codes, mcc::errcodes::PARSER_PIN_INVALID),
            "pins row `{row}` must fire E2083; got codes: {codes:?}"
        );
    }
}

// I7 — enum values are plain identifiers only (`HIGH` is an MCONST token)

#[test]
fn u300corpus__i7_keyword_enum_value_reports_2081() {
    let codes = codes(&build_diags("enum Grade { HIGH }\nmodule top\n{\n    io A\n}\n"));
    assert!(
        has(&codes, mcc::errcodes::PARSER_TOP_INVALID),
        "`enum Grade {{ HIGH }}` must fire E2081; got codes: {codes:?}"
    );
}

// I8 — library-path scalar hyperparameter reports E4176 (M3 ruling: the
// library path must behave like the local one; the pre-batch 4176 locks were
// all local-component)

#[test]
fn u300corpus__i8_library_scalar_hyperparam_reports_4176() {
    let codes = codes(&build_diags(
        "module top\n{\n    io V5V\n    io GND\n    CAP C1(1uF)\n    C1.Cap(V5V, GND)\n}\n",
    ));
    assert!(
        has(&codes, mcc::errcodes::INST_PARAM_BIND_FAILED),
        "library-path `C1.Cap(V5V, GND)` scalar bind must fire E4176; got codes: {codes:?}"
    );
}

// I10 — binary `±` outside a tolerance context fires W2112 in the cond/judge
// path (the GLR reduce arm mca.tab.c:4929), and `&` glued to a comparison
// judge has no production (E2082; no judge & judge form exists)

#[test]
fn u300corpus__i10_binary_plusminus_reports_2112() {
    let codes = codes(&build_diags(
        "module top\n{\n    func g(x)\n    {\n        if (x ± 1)\n        {\n            return x\n        }\n        return x\n    }\n}\n",
    ));
    assert!(
        has(&codes, mcc::errcodes::PARSER_PLUSMINUS),
        "binary `±` outside a tolerance context must fire W2112; got codes: {codes:?}"
    );
}

#[test]
fn u300corpus__i10_ampersand_glued_to_judge_reports_2082() {
    let codes = codes(&build_diags(
        "module top\n{\n    func f(flag)\n    {\n        if ((partno == 'a') & flag)\n        {\n            return x\n        }\n        return x\n    }\n}\n",
    ));
    assert!(
        has(&codes, mcc::errcodes::PARSER_CLAUSE_INVALID),
        "`(judge) & flag` has no production and must fire E2082; got codes: {codes:?}"
    );
}
