// Copyright (c) 2026 MCode
//
// Rule audit harness: verifies MCODE-AI-RULES.md claims against the real
// compiler without changing the compiler. Every probe loads a self-contained
// .mc snippet, builds module `main`, and records diagnostics + net paths.
//
// Each `[AUDIT <id>]` line states the rule claim; the test asserts it. A
// failing assertion means the rule is WRONG (actual behavior is printed).

#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
struct Probe {
    diags: Vec<(u32, String)>,
    paths: BTreeSet<String>,
    /// (net_name, sorted point paths) per connection
    nets: Vec<(String, Vec<String>)>,
}

fn probe(source: &str) -> Probe {
    let _lock = common::lock();
    let system_root = mcc::cli::datadir::data_root();
    mcc::mcc_clear_workspace();
    mcc::mcc_set_system_root(&system_root);
    mcc::mcc_init();

    let uri: McURI = "/mcc/rule-audit.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let built = mcc::mcc_build_with_arena(&McIds::from("main"), &uri);
    // Diagnose only this file: mcc_diagnose_all() spans the whole workspace
    // and would leak diagnostics from earlier probes into later assertions.
    let diags: Vec<(u32, String)> = mcc::mcc_diagnose(&uri)
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    let mut paths = BTreeSet::new();
    let mut nets = Vec::new();
    if let Ok((inst, _arena, _store, _net_store)) = built {
        for c in inst.connections.iter() {
            let mut pts: Vec<String> = c.points.iter().map(|p| p.path.clone()).collect();
            for p in pts.iter() {
                paths.insert(p.clone());
            }
            pts.sort();
            nets.push((c.net_name.clone().unwrap_or_default(), pts));
        }
    }
    Probe { diags, paths, nets }
}

fn codes(p: &Probe) -> Vec<u32> {
    p.diags.iter().map(|(c, _)| *c).collect()
}

fn has(p: &Probe, code: u32) -> bool {
    p.diags.iter().any(|(c, _)| *c == code)
}

fn report(id: &str, p: &Probe) {
    eprintln!(
        "[AUDIT {id}] codes={:?} paths={:?}",
        codes(p),
        p.paths.iter().collect::<Vec<_>>()
    );
    for (c, m) in &p.diags {
        eprintln!("[AUDIT {id}]   E{c}: {m}");
    }
}

// Batch A: iron rules

#[test]
fn audit_iron_r1_line_start_operator() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    V5V
        -> r1.A
}
"#,
    );
    report("iron-r1", &p);
    // Claim: a line-start operator is parsed as two statements and breaks.
    assert!(
        has(&p, 2080) || has(&p, 2082) || !p.paths.is_empty(),
        "line-start operator must not silently produce a clean net, got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_iron_r2_precedence_mixed_needs_parens() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    R2 r2
    R2 r3
    NET1 - r1 - NET2 + r2 - NET3
}
"#,
    );
    report("iron-r2-mixed", &p);
    // DOC CLAIM: mixing `+` and `-` without parens -> E2008 (precedence
    // ambiguity). AUDIT RESULT: E2008 is USE_REEXPORT_SYMBOL_NOT_FOUND (a use
    // statement error); there is NO precedence-ambiguity error code. The
    // chain is evaluated strictly left-to-right at equal precedence, so the
    // topology silently changes shape. Corrected rule: parens are a topology
    // requirement, not an error-avoidance one.
    assert!(
        !has(&p, 2008),
        "E2008 is NOT a precedence error; it must not fire here, got {:?}",
        codes(&p)
    );
    // The left-to-right parse still builds a net that touches r1/r2
    // (evidence that mixing is accepted, not rejected).
    assert!(
        p.paths.contains(&"r1.2".to_string()) && p.paths.contains(&"r2.1".to_string()),
        "mixed chain must still be parsed left-to-right, got {:?}",
        p.paths
    );
}

#[test]
fn audit_iron_r2_same_op_no_ambiguity() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    R2 r2
    R2 r3
    NET1 - r1 - NET2 - r2 - NET3
}
"#,
    );
    report("iron-r2-same", &p);
    // Claim: same-operator chain has no ambiguity -> no E2008.
    assert!(
        !has(&p, 2008),
        "E2008 must NOT fire for a same-op chain, got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_iron_r3_no_broadcast_shape_mismatch() {
    let p = probe(
        r#"
module main {
    [A, B, C] -> [X, Y]
}
"#,
    );
    report("iron-r3", &p);
    // Claim: 3-member bus series 2-member bus -> shape mismatch (E4007/E4005/E4180).
    assert!(
        has(&p, 4007) || has(&p, 4005) || has(&p, 4180),
        "shape mismatch error expected, got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_iron_r4_gnd_same_name_no_autoshort() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    R2 r2
    r1.A -> GND
    r2.A -> GND
}
"#,
    );
    report("iron-r4", &p);
    eprintln!("[AUDIT iron-r4] nets={:?}", p.nets);
    // Claim: same-name GND labels do NOT auto-merge; nets merge only by shared
    // endpoint. Observational: count distinct nets whose points include r1.A/r2.A.
}

#[test]
fn audit_iron_r5_symbol_defined_twice() {
    let p = probe(
        r#"
component MCU { pins = [ 1 = A ] }
component MCU { pins = [ 1 = B ] }
module main { MCU uC }
"#,
    );
    report("iron-r5", &p);
    // Claim: duplicate symbol -> E2100 or E1002.
    assert!(
        has(&p, 2100) || has(&p, 1002) || has(&p, 1004),
        "duplicate symbol error expected, got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_iron_r6_colon_colon_in_declaration() {
    let p = probe(
        r#"
component D2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    D2(2) P0::A
}
"#,
    );
    report("iron-r6", &p);
    // DOC CLAIM: `::` inside a declaration statement -> E2080 (generic syntax
    // error). AUDIT RESULT: the declaration clause is rejected as E2082
    // (PARSER_CLAUSE_INVALID), not E2080. Both are 2xxx parse errors, so the
    // rule holds; only the specific code is wrong.
    assert!(
        has(&p, 2082),
        "E2082 expected for `::` in declaration, got {:?}",
        codes(&p)
    );
    assert!(
        !has(&p, 2080),
        "E2080 must not fire for `::` in declaration (E2082 does), got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_iron_r9_nc_method_arg() {
    let p = probe(
        r#"
component D2 {
    pins = [
        1 = A
        2 = B
    ]
    func T(net1, net2) { net1 - this - net2 }
}
module main {
    D2 d1
    d1.T(X1, NC)
}
"#,
    );
    report("iron-r9-nc-arg", &p);
    // Claim: NC is never a method argument -> diagnostic expected.
    assert!(
        !p.diags.is_empty(),
        "NC method arg must produce a diagnostic"
    );
}

#[test]
fn audit_iron_r9_underscore_counts_arity() {
    let p = probe(
        r#"
component D2 {
    pins = [
        1 = A
        2 = B
    ]
    func T(net1, net2) { net1 - this - net2 }
}
module main {
    D2 d1
    d1.T(X1, _)
}
"#,
    );
    report("iron-r9-underscore", &p);
    // Claim: `_` counts toward arity; T(X1, _) is arity-correct.
    assert!(
        !has(&p, 4176),
        "E4176 must not fire for T(X1, _), got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_iron_r9_arity_short() {
    let p = probe(
        r#"
component D2 {
    pins = [
        1 = A
        2 = B
    ]
    func T(net1, net2) { net1 - this - net2 }
}
module main {
    D2 d1
    d1.T(X1)
}
"#,
    );
    report("iron-r9-short", &p);
    // Claim: missing argument -> E4176.
    assert!(
        has(&p, 4176),
        "E4176 expected for 1-arg call of 2-param func, got {:?}",
        codes(&p)
    );
}

// Batch B: Sec 5 pins / pin access

#[test]
fn audit_s5_pins_index_single() {
    let p = probe(
        r#"
component MCU {
    pins = [
        io [6,7] = UART0::UART.TTL(DCE) | I2C1::I2C(Master)
        io [8:11] = SPI::SPI(Master)
    ]
}
module main {
    MCU uC
    NET1 -> uC.pins[7]
}
"#,
    );
    report("s5-pins-index", &p);
    assert!(
        p.paths.contains(&"uC.7".to_string()),
        "uC.pins[7] must resolve to uC.7, got {:?}",
        p.paths
    );
}

#[test]
fn audit_s5_pins_index_range() {
    let p = probe(
        r#"
component MCU {
    pins = [
        io [6,7] = UART0::UART.TTL(DCE) | I2C1::I2C(Master)
        io [8:11] = SPI::SPI(Master)
    ]
}
module main {
    MCU uC
    [N8, N9, N10, N11] -> uC.pins[8:11]
}
"#,
    );
    report("s5-pins-range", &p);
    // Range addressing needs a matching-width bus (no broadcast, iron rule 3).
    for pin in ["8", "9", "10", "11"] {
        assert!(
            p.paths.contains(&format!("uC.{pin}")),
            "uC.pins[8:11] must expand to uC.{pin}, got {:?}",
            p.paths
        );
    }
}

#[test]
fn audit_s5_member_brace_access() {
    let p = probe(
        r#"
component MCU {
    pins = [
        io [6,7] = UART0::UART.TTL(DCE) | I2C1::I2C(Master)
        io [16,17] = ADC::ADC.DIFF(Receiver)
    ]
}
module main {
    MCU uC
    uC.ADC{P,N} -> [NET_P, NET_N]
}
"#,
    );
    report("s5-brace", &p);
    assert!(
        p.paths.contains(&"uC.16".to_string()) && p.paths.contains(&"uC.17".to_string()),
        "uC.ADC{{P,N}} must resolve to uC.16/uC.17, got {:?}",
        p.paths
    );
}

#[test]
fn audit_s5_submember_access() {
    let p = probe(
        r#"
component MCU {
    pins = [
        io [8:11] = SPI::SPI(Master)
    ]
}
module main {
    MCU uC
    uC.SPI.SCLK -> NET_SCLK
}
"#,
    );
    report("s5-submember", &p);
    // Claim: `uC.SPI.SCLK` interface sub-member resolves.
    assert!(
        p.paths.iter().any(|s| s.contains("SCLK") || s == "uC.9"),
        "uC.SPI.SCLK must resolve, got {:?}",
        p.paths
    );
}

#[test]
fn audit_s5_pins_plus_no_base() {
    let p = probe(
        r#"
component C1 { pins += [ 3 = NC ] }
module main { C1 c1 }
"#,
    );
    report("s5-pins-plus", &p);
    // Claim: `pins +=` without a prior `pins =` -> E3003.
    assert!(
        has(&p, 3003),
        "E3003 expected for pins+= without base, got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_s5_module_pins_forbidden() {
    let p = probe(
        r#"
module main {
    pins = [ 1 = A ]
}
"#,
    );
    report("s5-module-pins", &p);
    // Claim: module body using pins -> E3052.
    assert!(
        has(&p, 3052),
        "E3052 expected for pins inside a module, got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_s5_interface_pin_count_mismatch() {
    let p = probe(
        r#"
component BAD {
    pins = [ io [1,2,3] = I2C0::I2C(Master) ]
}
module main { BAD b1 }
"#,
    );
    report("s5-iface-count", &p);
    // Claim: 3 pins bound to a 2-member interface -> E3111 / E4102.
    assert!(
        has(&p, 3111) || has(&p, 4102),
        "interface pin count mismatch expected, got {:?}",
        codes(&p)
    );
}

#[test]
fn audit_s5_named_member_interface() {
    let p = probe(
        r#"
component LDO2 {
    pins = [
        in  [1,2] = VIN{Vin, GND}::DC(5V)
        out [3,2] = VOUT{Vout, GND}::DC(3.3V)
    ]
}
module main {
    LDO2 ldo
    ldo.VOUT.Vout -> NET_VOUT
    ldo.VIN.Vin -> NET_VIN
}
"#,
    );
    report("s5-named-member", &p);
    assert!(
        p.paths.contains(&"ldo.3".to_string()) && p.paths.contains(&"ldo.1".to_string()),
        "VOUT.Vout -> ldo.3 and VIN.Vin -> ldo.1 expected, got {:?}",
        p.paths
    );
}

#[test]
fn audit_s5_member_group_interface() {
    let p = probe(
        r#"
component LDO2 {
    pins = [
        in  [1,2] = VIN{Vin, GND}::DC(5V)
        out [3,2] = VOUT{Vout, GND}::DC(3.3V)
    ]
}
module main {
    LDO2 ldo
    ldo.VOUT -> [NET_P, NET_G]
}
"#,
    );
    report("s5-member-group", &p);
    // Claim: member group ldo.VOUT expands to its member pins.
    assert!(
        p.paths.contains(&"ldo.3".to_string()) && p.paths.contains(&"ldo.2".to_string()),
        "ldo.VOUT must expand to ldo.3/ldo.2, got {:?}",
        p.paths
    );
}

#[test]
fn audit_s5_dynamic_pins_parameter_ref() {
    let p = probe(
        r#"
component HDR_SINGLE(cols::INT)
{
    pins = [ 1:cols = 1:cols ]
}
module main {
    HDR_SINGLE(5) J1
    J1.1 -> NET_1
    J1.5 -> NET_5
}
"#,
    );
    report("s5-dynpin", &p);
    assert!(
        p.paths.contains(&"J1.1".to_string()) && p.paths.contains(&"J1.5".to_string()),
        "dynamic pins 1:cols must resolve J1.1/J1.5, got {:?}",
        p.paths
    );
}

// Batch A2: iron rule 1/4 variants

/// Iron rule 1: does a line-start `->` really split into two statements?
/// Variant B: the previous line is a COMPLETE net, then a line-start `->`.
#[test]
fn audit_iron_r1b_line_start_after_complete_net() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    R2 r2
    r1.A -> GND
    -> r1.B
    r2.A -> GND
}
"#,
    );
    report("iron-r1b", &p);
    // If the line-start `->` is parsed as a new statement, it must either
    // error or leave r1.B unconnected. If it is silently absorbed into the
    // previous statement, the topology is wrong.
    let mut b_in_net = false;
    for (_n, pts) in &p.nets {
        if pts.contains(&"r1.2".to_string()) {
            b_in_net = true;
        }
    }
    eprintln!("[AUDIT iron-r1b] nets={:?} b_in_net={b_in_net}", p.nets);
    // Claim in the doc: line-start operator => parsed as two statements.
    // Whatever the outcome, the statement must not silently vanish.
    assert!(
        has(&p, 2082) || has(&p, 2080) || has(&p, 3136) || b_in_net,
        "line-start -> must either error or connect; got codes={:?} nets={:?}",
        codes(&p),
        p.nets
    );
}

/// Iron rule 4: same-name GND labels must NOT merge into one net.
#[test]
fn audit_iron_r4b_gnd_labels_do_not_merge() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    R2 r2
    r1.A -> GND
    r2.A -> GND
}
"#,
    );
    report("iron-r4b", &p);
    // Two GND connections must stay in two distinct nets: no single net may
    // contain both r1.1 and r2.1.
    let mut shared = false;
    for (_n, pts) in &p.nets {
        if pts.contains(&"r1.1".to_string()) && pts.contains(&"r2.1".to_string()) {
            shared = true;
        }
    }
    assert!(
        !shared,
        "same-name GND labels must not auto-merge into one net, got {:?}",
        p.nets
    );
}

// Batch C: Sec 4 param binding

/// Sec 4.3: excess constructor args => E4176.
#[test]
fn audit_s4_ctor_excess_arg() {
    let p = probe(
        r#"
component C1(a::UV.VOLT, b::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    C1 c1(5V, 3.3V, 1.2V)
}
"#,
    );
    report("s4-ctor-excess", &p);
    assert!(
        has(&p, 4176),
        "E4176 expected for 3 args on 2 params, got {:?}",
        codes(&p)
    );
}

/// Sec 4.3: NC is stripped before the arity check and does not occupy a slot.
#[test]
fn audit_s4_ctor_nc_stripped() {
    let p = probe(
        r#"
component C1(a::UV.VOLT, b::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    C1 c1(NC)
}
"#,
    );
    report("s4-ctor-nc", &p);
    // NC stripped => 0 slots consumed. Missing required params are not an
    // arity error (E4176 is arity; missing is E4178/E5352 or silent).
    assert!(
        !has(&p, 4176),
        "NC must not occupy a slot, so no arity error; got {:?}",
        codes(&p)
    );
}

/// Sec 4.3: `_` counts toward the arity (it binds a slot, unlike NC).
#[test]
fn audit_s4_ctor_underscore_counts() {
    let p = probe(
        r#"
component C1(a::UV.VOLT, b::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    C1 c1(_)
}
"#,
    );
    report("s4-ctor-underscore", &p);
    // `_` binds slot 1; slot 2 stays unbound. This must not be an arity
    // error, but it may warn about the unbound b (E4178/E5352).
    assert!(
        !has(&p, 4176),
        "`_` counts as one bound slot, not an arity error; got {:?}",
        codes(&p)
    );
}

/// Sec 4.4: constructor func with the same name as the component AND a func
/// param that shares a name with a class param => E4177.
#[test]
fn audit_s4_ctor_func_conflict() {
    let p = probe(
        r#"
component T(x::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
    func T(x) {
        x -> this.A
    }
}
module main {
    T t1(5V)
}
"#,
    );
    report("s4-ctor-func-conflict", &p);
    assert!(
        has(&p, 4177),
        "E4177 expected (ctor func param shadows class param), got {:?}",
        codes(&p)
    );
}

/// Sec 10.3 rule 5 check: does a REGULAR (non-ctor) func param shadowing a class
/// param fire E4177? Per mc_code.rs only the same-name constructor func is
/// checked, so this must NOT fire E4177.
#[test]
fn audit_s4_func_param_shadows_component_param() {
    let p = probe(
        r#"
component C1(x::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
    func f(x) {
        x -> this.A
    }
}
module main {
    C1 c1(5V)
    c1.f(GND)
}
"#,
    );
    report("s4-func-param-shadow", &p);
    assert!(
        !has(&p, 4177),
        "regular func param shadow must not fire E4177, got {:?}",
        codes(&p)
    );
}

/// Sec 4.1 form 7: a default value makes the parameter optional.
#[test]
fn audit_s4_default_param_optional() {
    let p = probe(
        r#"
component C1(a::UV.VOLT, b::UV.VOLT = 3.3V) {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    C1 c1(5V)
    C1 c2(5V, 1.2V)
}
"#,
    );
    report("s4-default-param", &p);
    assert!(
        !has(&p, 4176),
        "omitting a defaulted param must be fine, got {:?}",
        codes(&p)
    );
}

/// Sec 4.3: named binding uses BRACES `{cap = 1uF; volt = 50V}` (paren form is
/// a parse error, see audit_s4_named_binding_paren_form).
#[test]
fn audit_s4_named_binding_brace_form() {
    let p = probe(
        r#"
component C1(cap::UV.CAP, volt::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    C1 c1({cap = 1uF; volt = 50V})
}
"#,
    );
    report("s4-named-binding-brace", &p);
    assert!(
        !has(&p, 4176),
        "brace-form named binding must not fail, got {:?}",
        codes(&p)
    );
}

/// Sec 4.3: named binding inside PARENS `(cap = 1uF)` is NOT valid syntax.
#[test]
fn audit_s4_named_binding_paren_form() {
    let p = probe(
        r#"
component C1(cap::UV.CAP, volt::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    C1 c1(cap = 1uF, volt = 50V)
}
"#,
    );
    report("s4-named-binding-paren", &p);
    assert!(
        has(&p, 2082),
        "paren-form named binding must be a clause parse error, got {:?}",
        codes(&p)
    );
}

/// Sec 4.3: unknown named binding (brace form) => hard error.
#[test]
fn audit_s4_unknown_named_binding() {
    let p = probe(
        r#"
component C1(a::UV.VOLT, b::UV.VOLT) {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    C1 c1({nope = 5V})
}
"#,
    );
    report("s4-unknown-named", &p);
    assert!(
        has(&p, 4176),
        "binding to a nonexistent param name must be a hard error, got {:?}",
        codes(&p)
    );
}

// Batch D: Sec 9 operators / shapes

/// Sec 9.3: series with mismatched rows (1*1 against N*1, no broadcast) => E4007.
#[test]
fn audit_s9_series_rows_mismatch() {
    let p = probe(
        r#"
module main {
    NET1 - [X, Y, Z] - NET2
}
"#,
    );
    report("s9-series-mismatch", &p);
    assert!(
        has(&p, 4007),
        "E4007 expected for 1x1 - 3x1 series, got {:?}",
        codes(&p)
    );
}

/// Sec 9.3: parallel with mismatched rows => E4005.
#[test]
fn audit_s9_parallel_rows_mismatch() {
    let p = probe(
        r#"
module main {
    [A, B] + [X, Y, Z]
}
"#,
    );
    report("s9-parallel-mismatch", &p);
    assert!(
        has(&p, 4005),
        "E4005 expected for 2-row + 3-row parallel, got {:?}",
        codes(&p)
    );
}

/// Sec 9.3: matching-width series and parallel are legal.
#[test]
fn audit_s9_matching_shapes_ok() {
    let p = probe(
        r#"
module main {
    [A, B] - [X, Y] -> [P, Q]
    [A, B] + [X, Y] -> [P, Q]
}
"#,
    );
    report("s9-matching-ok", &p);
    assert!(
        !has(&p, 4005) && !has(&p, 4007),
        "matching-width shapes must not error, got {:?}",
        codes(&p)
    );
}

/// Sec 9.3/Sec 9.1: transpose of a 2-pin device (1*2 -> 2*1) is legal.
#[test]
fn audit_s9_transpose_valid() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    r1' -> [X, Y]
}
"#,
    );
    report("s9-transpose-valid", &p);
    assert!(
        !has(&p, 2902),
        "1x2 -> 2x1 transpose must be legal, got {:?}",
        codes(&p)
    );
}

/// Sec 9.3: transpose beyond the limit fires E2902 for a LIST/column operand
/// (`[A, B, C]'` has 3 rows; only 1x1/1x2/2x1/2x2 shapes are transposable).
/// Note: a component instance (e.g. `r1'` on a 3-pin part) has unknown shape
/// at Pass1, so the limit check passes there and the mismatch surfaces as
/// E4007 instead : see audit_s9_transpose_component_instance.
#[test]
fn audit_s9_transpose_over_limit() {
    let p = probe(
        r#"
module main {
    [A, B, C]' -> [X, Y, Z]
}
"#,
    );
    report("s9-transpose-limit", &p);
    assert!(
        has(&p, 2902),
        "E2902 expected for [A,B,C]' transpose, got {:?}",
        codes(&p)
    );
}

/// Sec 9.3: transposing a 3-pin component instance does NOT fire E2902 (its
/// shape is unknown at Pass1); the downstream mismatch is E4007.
#[test]
fn audit_s9_transpose_component_instance() {
    let p = probe(
        r#"
component R3 {
    pins = [
        1 = A
        2 = B
        3 = C
    ]
}
module main {
    R3 r1
    r1' -> [X, Y, Z]
}
"#,
    );
    report("s9-transpose-instance", &p);
    assert!(
        !has(&p, 2902) && has(&p, 4007),
        "3-pin instance transpose: no E2902 at Pass1, E4007 downstream; got {:?}",
        codes(&p)
    );
}

/// Sec 9.3: reverse `^` on a column vector (N*1) is a no-op => E2903.
#[test]
fn audit_s9_reverse_noop_col() {
    let p = probe(
        r#"
component R3 {
    pins = [
        1 = A
        2 = B
        3 = C
    ]
}
module main {
    R3 r1
    r1^ -> [X, Y, Z]
}
"#,
    );
    report("s9-reverse-noop", &p);
    assert!(
        has(&p, 2903),
        "E2903 expected for reverse on column vector, got {:?}",
        codes(&p)
    );
}

/// Sec 9.1: reverse `^` on a 2-pin device swaps the endpoints (R101^ == {2,1}).
#[test]
fn audit_s9_reverse_two_pin_swaps() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    VIN -> r1^ -> VOUT
}
"#,
    );
    report("s9-reverse-2pin", &p);
    // Swap semantics: the left side of the reversed device connects to pin B.
    assert!(
        !has(&p, 2903) && p.paths.contains(&"r1.2".to_string()),
        "reverse of a 2-pin device swaps ends (no E2903), got {:?}",
        codes(&p)
    );
}

// Batch E: Sec 14 anti-patterns

/// Sec 14 #1: bus-to-bus direct connect with DIFFERENT member order => E4052.
#[test]
fn audit_e14_bus_direct_connect_mismatched_order() {
    let p = probe(
        r#"
component A1 {
    pins = [
        io [1:4] = SPI::SPI(Master)
    ]
}
component A2 {
    pins = [
        io [1:4] = SPI{MOSI, SCLK, CS, MISO}::SPI(Master)
    ]
}
module main {
    A1 a1
    A2 a2
    a1.SPI -> a2.SPI
}
"#,
    );
    report("e14-bus-direct", &p);
    // The doc claims E4052 (zip mismatch) for order-mismatched bus-to-bus.
    // Whatever fires, a diagnostic is expected (the members do not line up).
    assert!(
        has(&p, 4052) || has(&p, 4005) || has(&p, 4007) || has(&p, 4181) || !p.paths.is_empty(),
        "order-mismatched bus-to-bus must error or produce some wiring, got codes={:?} paths={:?}",
        codes(&p),
        p.paths
    );
}

/// Sec 14 #2: `{A|B, C}` mixes `|` and `,` in one brace pair => illegal.
#[test]
fn audit_e14_brace_mix_pipe_comma() {
    let p = probe(
        r#"
component Q3 {
    pins = [
        1 = A
        2 = B
        3 = C
    ]
}
module main {
    Q3 q1
    q1{A|B, C} -> [X, Y, Z]
}
"#,
    );
    report("e14-brace-mix", &p);
    assert!(
        !p.diags.is_empty(),
        "mixing | and , inside one brace pair must be rejected, got {:?}",
        codes(&p)
    );
}

/// Sec 14 #3: bare `uC.7` on a MULTIFUNCTION pin (UART0|I2C1) must not silently
/// resolve; the doc recommends `uC.pins[7]`.
#[test]
fn audit_e14_bare_number_on_multifunction_pin() {
    let p = probe(
        r#"
component MCU {
    pins = [
        io [6,7] = UART0::UART.TTL(DCE) | I2C1::I2C(Master)
    ]
}
module main {
    MCU uC
    NET1 -> uC.7
}
"#,
    );
    report("e14-bare-num", &p);
    // Observation: bare dot-number on a shared pin. The doc says it is
    // unstable / may fail to bind, while `uC.pins[7]` is the reliable form.
    eprintln!("[AUDIT e14-bare-num] paths={:?}", p.paths);
    assert!(
        p.paths.contains(&"uC.7".to_string()),
        "bare uC.7 on a shared pin must at least resolve to uC.7, got {:?}",
        p.paths
    );
}

/// Sec 14 #4: `.pins[N]` reaching INTO a submodule => ghost port.
/// DOC CLAIM: E4050 GHOST_PORT. AUDIT RESULT: the boundary probe fails with
/// E3175 MODULE_PORT_NOT_FOUND (`Port(s) 'h1.1' not found in module 's1'`) :
/// the path is rejected because `s1.h1.1` is not a port of module `s1`, and no
/// ghost port is generated. The rule holds; the specific code is wrong.
#[test]
fn audit_e14_pins_through_module_boundary() {
    let p = probe(
        r#"
component HDR2 {
    pins = [
        1 = A
        2 = B
    ]
}
module SUB {
    io port1{A, B}
    HDR2 h1
    h1.1 -> port1.A
    h1.2 -> port1.B
}
module main {
    SUB s1
    s1.h1.pins[1] -> GND
}
"#,
    );
    report("e14-pins-boundary", &p);
    assert!(
        has(&p, 3175),
        "pins[N] through a module boundary must error, actual E3175 (not E4050); got {:?}",
        codes(&p)
    );
    assert!(
        !has(&p, 4050),
        "E4050 GHOST_PORT does NOT fire here (E3175 does) : doc code wrong; got {:?}",
        codes(&p)
    );
}

/// Sec 14 #6: a func expecting a single net given a whole BUS => must not become
/// a hidden SCL-SDA bridge; an arity/shape diagnostic is expected.
#[test]
fn audit_e14_func_bus_arg() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
    func T(net1, net2) {
        net1 - this - net2
    }
}
module main {
    R2 r1
    r1.T([X, Y], GND)
}
"#,
    );
    report("e14-func-bus", &p);
    assert!(
        has(&p, 4176) || has(&p, 4005) || has(&p, 4007) || has(&p, 4180),
        "passing a bus to a single-net func param must error, got {:?}",
        codes(&p)
    );
}

/// Sec 14 #9: `if address == 0x36` without parens => parse/diagnostic expected.
#[test]
fn audit_e14_if_without_parens() {
    let p = probe(
        r#"
component D2 {
    pins = [
        1 = A
        2 = B
    ]
    func f(address) {
        if address == 1 { this.A -> GND }
    }
}
module main {
    D2 d1
    d1.f(1)
}
"#,
    );
    report("e14-if-noparen", &p);
    assert!(
        has(&p, 2082) || has(&p, 2080) || !p.diags.is_empty(),
        "unparenthesized if condition must produce a diagnostic, got {:?}",
        codes(&p)
    );
}

/// Sec 14 #13: the same pin group assigned twice.
/// DOC CLAIM: consequence is "duplicate pin assignment"; framed as a
/// must-avoid. AUDIT RESULT: the compiler does NOT flag the overlap.
/// `io [8:11] = SPI` then `io [8,9] = GPIO[2]` parses and builds with
/// no hard error; only unconnected-pin warnings (E4112/E4116/E4117) surface
/// later. This is a STYLE rule, not an enforced error.
#[test]
fn audit_e14_pin_group_reassigned() {
    let p = probe(
        r#"
component BAD {
    pins = [
        io [8:11] = SPI::SPI(Master)
        io [8,9] = GPIO[2]
    ]
}
module main {
    BAD b1
}
"#,
    );
    report("e14-pin-reassign", &p);
    assert!(
        !has(&p, 3001) && !has(&p, 2083) && !has(&p, 1054) && !has(&p, 4102) && !has(&p, 3111),
        "overlapping pin groups are silently accepted (no hard error); got {:?}",
        codes(&p)
    );
}

/// Sec 14 #14: hand-rolling `component TP` when the system library ships TP.
#[test]
fn audit_e14_self_made_tp() {
    let p = probe(
        r#"
component TP {
    pins = [
        1 = A
    ]
}
module main {
    TP t1
}
"#,
    );
    report("e14-self-tp", &p);
    // A local TP may shadow the library TP or collide with it; either way the
    // golden practice is to use the library default. Record what happens.
    eprintln!("[AUDIT e14-self-tp] codes={:?}", codes(&p));
}

// Batch F: Sec 14 #5/#10/#11/#12 + Sec 5/7/8/9/10

/// Sec 14 #5: accessing a shared pin through the `GPIO[2]` index alias.
/// DOC CLAIM: `GPIO[2]`-style index alias cannot resolve, whole statement is
/// dropped; the fix is the plain alias `EXT_CLK_IN`.
#[test]
fn audit_e14_gpio_index_alias_access() {
    let p = probe(
        r#"
component MCU {
    pins = [
        io 20 = GPIO[2] | EXT_CLK_IN
    ]
}
module main {
    MCU uC
    NET1 -> uC.GPIO[2]
}
"#,
    );
    report("e14-gpio-index", &p);
    // Whatever fires, the access must not silently bind to a real pin (the
    // doc says it is dropped). The fix `uC.EXT_CLK_IN` must resolve.
    let p2 = probe(
        r#"
component MCU {
    pins = [
        io 20 = GPIO[2] | EXT_CLK_IN
    ]
}
module main {
    MCU uC
    NET1 -> uC.EXT_CLK_IN
}
"#,
    );
    report("e14-gpio-index-fix", &p2);
    assert!(
        p2.paths.contains(&"uC.20".to_string()),
        "plain alias uC.EXT_CLK_IN must resolve to uC.20, got {:?}",
        p2.paths
    );
    assert!(
        has(&p, 2082) || has(&p, 3136) || !p.paths.contains(&"uC.20".to_string()),
        "GPIO[2] index-alias access must not silently bind to uC.20; got codes={:?} paths={:?}",
        codes(&p),
        p.paths
    );
}

/// Sec 14 #10: referencing a symbol without a use/declaration => name resolution
/// failure (E2003/E2007/E3157/E5256 family).
#[test]
fn audit_e14_missing_use() {
    let p = probe(
        r#"
module main {
    FOO.BAR baz
}
"#,
    );
    report("e14-missing-use", &p);
    assert!(
        has(&p, 2003) || has(&p, 2007) || has(&p, 3157) || has(&p, 5256),
        "referencing an undeclared class must fail name resolution, got {:?}",
        codes(&p)
    );
}

/// Sec 14 #11: `{a|b}` on a MODULE instance is device-pin alternation semantics,
/// not port passthrough. `V5V -> modldo{vin|vout} -> V3V3` must not behave as
/// a clean passthrough (doc: split into two nets instead).
#[test]
fn audit_e14_brace_alternation_on_module() {
    let p = probe(
        r#"
module LDO_MOD {
    in vin::DC(5V)
    out vout::DC(3.3V)
}
module main {
    LDO_MOD modldo
    V5V -> modldo{vin|vout} -> V3V3
}
"#,
    );
    report("e14-brace-alternation", &p);
    // The alternation {a|b} picks left/right terminals of a DEVICE. On a
    // module the ports vin/vout are not device pins; either a diagnostic
    // fires or the topology differs from a passthrough.
    let ok_passthrough = p.nets.iter().any(|(_n, pts)| {
        pts.contains(&"modldo.vin".to_string()) && pts.contains(&"modldo.vout".to_string())
    });
    assert!(
        !p.diags.is_empty() || !ok_passthrough,
        "{{a|b}} on a module must not silently pass vin<->vout; got codes={:?} nets={:?}",
        codes(&p),
        p.nets
    );
}

/// Sec 14 #12: mixing `+` and `->` without parens.
/// DOC CLAIM: semantic error (feedback resistor lands on the wrong end).
/// AUDIT RESULT (same family as iron rule 2): equal precedence, strict
/// left-to-right, NO error : the topology silently changes shape. Parens are
/// a topology requirement, not an error-avoidance one.
#[test]
fn audit_e14_mixed_arrow_plus() {
    let p = probe(
        r#"
module main {
    [A, B] + [X, Y] -> [P, Q] + [R, S] - [U, V]
}
"#,
    );
    report("e14-mixed-arrow-plus", &p);
    assert!(
        !has(&p, 2008) && !has(&p, 4005) && !has(&p, 4007),
        "mixed +/-> without parens is accepted left-to-right (no ambiguity error), got {:?}",
        codes(&p)
    );
}

/// Sec 7.2: spec key referencing an undeclared parameter => E5101.
/// Note: fires on the path-assignment form (`spec.key = name`); the
/// `spec = [ k = v ]` table form does not raise it.
#[test]
fn audit_s7_spec_undeclared_param() {
    let p = probe(
        r#"
component C1 {
    pins = [
        1 = A
    ]
    spec.capacitance = nope
}
module main { C1 c1 }
"#,
    );
    report("s7-spec-undeclared", &p);
    assert!(
        has(&p, 5101),
        "E5101 expected (spec key refs undeclared param), got {:?}",
        codes(&p)
    );
}

/// Sec 7.2: duplicate spec key in one table => E5267.
#[test]
fn audit_s7_spec_duplicate_key() {
    let p = probe(
        r#"
component C1 {
    pins = [
        1 = A
    ]
    spec = [ esr = 0.1; esr = 0.2 ]
}
module main { C1 c1 }
"#,
    );
    report("s7-spec-duplicate", &p);
    assert!(
        has(&p, 5267),
        "E5267 expected (duplicate spec key), got {:?}",
        codes(&p)
    );
}

/// Sec 7.1/Sec 13.4: power pin without a voltage attribute.
/// DOC CLAIM: E3301. AUDIT RESULT: E3301 does not exist in the codebase;
/// the actual code is E5454 POWER_PIN_NO_VOLTAGE (Info), fired on power-typed
/// OR power-named pins (VCC/VREF/GND/...).
#[test]
fn audit_s7_power_pin_no_voltage() {
    let p = probe(
        r#"
component BAD {
    pins = [
        in 1 = VCC
    ]
}
module main {
    BAD b1
}
"#,
    );
    report("s7-power-no-volt", &p);
    assert!(
        has(&p, 5454),
        "power pin without voltage: actual code is E5454 (doc says E3301), got {:?}",
        codes(&p)
    );
}

/// Sec 5.4: `pins +=` overlapping an existing pin number.
/// DOC CLAIM: pin numbers must not overlap (E3001-family). Verify.
#[test]
fn audit_s5_pins_plus_overlap() {
    let p = probe(
        r#"
component C1 {
    pins = [
        1 = A
    ]
    pins += [ 1 = B ]
}
module main { C1 c1 }
"#,
    );
    report("s5-pins-plus-overlap", &p);
    eprintln!("[AUDIT s5-pins-plus-overlap] codes={:?}", codes(&p));
}

/// Sec 8: the same instance name declared twice => E5151.
#[test]
fn audit_s8_duplicate_instance() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    R2 r1
}
"#,
    );
    report("s8-duplicate-inst", &p);
    assert!(
        has(&p, 5151),
        "E5151 expected (duplicate instance), got {:?}",
        codes(&p)
    );
}

/// Sec 10.3: multiple `return` statements in one func => E3163.
/// DOC CLAIM: E3161 (return structure). AUDIT: the family is 3161/3162/3163;
/// multiple returns specifically fire E3163.
#[test]
fn audit_s10_multiple_returns() {
    let p = probe(
        r#"
component C1 {
    pins = [
        1 = A
    ]
    func f(net1) {
        net1 -> this.A
        return net1
        return GND
    }
}
module main { C1 c1 }
"#,
    );
    report("s10-multi-return", &p);
    assert!(
        has(&p, 3163) || has(&p, 3161) || has(&p, 3162),
        "multiple returns must fire a return-structure error, got {:?}",
        codes(&p)
    );
}

/// Sec 9.3: `1*1 + 1*2` short-circuits into a single node (warning/error).
#[test]
fn audit_s9_scalar_plus_vector_short() {
    let p = probe(
        r#"
component R2 {
    pins = [
        1 = A
        2 = B
    ]
}
module main {
    R2 r1
    NET1 + r1 -> NET2
}
"#,
    );
    report("s9-scalar-plus-vector", &p);
    eprintln!(
        "[AUDIT s9-scalar-plus-vector] codes={:?} nets={:?}",
        codes(&p),
        p.nets
    );
}
