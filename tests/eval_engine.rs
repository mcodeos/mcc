// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: one value engine for conditions (doc/eval, CIMP U39).
//
// The condition path used to compare operands by stripping the unit suffix off
// the text and comparing the numbers that were left. That made `1200mV == 1.2V`
// false and `1200mV == 1.2` true, and it read a bare number as a value with no
// family at all. The engine normalizes both sides through the one suffix table
// and lets a unitless number take the family of the dimensioned side, so the
// same comparison holds whichever notation the author wrote.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McCondOperand, McCondition, McConds, McIds, McURI};

const SOURCE: &str = r#"
component REG_IFACE (partno)
{
    pins = [
        io [1] = A0
        pw [2] = NET12
        pw [3] = NET33
        pw [4] = NET50
    ]

    func Connect(volt)
    {
        if (volt == 1.2V) A0 -> NET12
        else if (volt == 3.3V) A0 -> NET33
        else A0 -> NET50
    }
}

module main
{
    io VDD
}
"#;

fn eq(left: McCondOperand, right: McCondOperand) -> McCondition {
    McCondition::Eq { left, right }
}

fn lit(text: &str) -> McCondOperand {
    McCondOperand::Literal(text.to_string())
}

fn ident(name: &str) -> McCondOperand {
    McCondOperand::Ident(McIds::from(name))
}

/// `1200mV == 1.2V` — the same quantity written two ways is one value.
#[test]
fn eval__scaled_unit_text_compares_equal() {
    let _lock = common::lock();
    common::reset();

    let source = r#"
component CMP(volt)
{
    pins = [ io [1] = A0 ]
    attr = [ rail = volt ]
}

module main { io VDD }
"#;
    let uri: McURI = "/mcc/eval-scaled-unit.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    mcc::mcc_build(&McIds::from("main"), &uri).expect("build failed");

    // The condition is built directly so the comparison itself is under test,
    // not the parse of a unit literal.
    let cond = eq(ident("volt"), lit("1.2V"));
    let same = vec![(McIds::from("volt"), "1200mV".to_string())];
    let other = vec![(McIds::from("volt"), "1300mV".to_string())];

    assert!(
        McConds::check_condition(&cond, &same),
        "1200mV must equal 1.2V"
    );
    assert!(
        !McConds::check_condition(&cond, &other),
        "1300mV must not equal 1.2V"
    );

    // The other direction: the literal carries the scaled notation.
    let cond_mv = eq(ident("volt"), lit("1200mV"));
    let volts = vec![(McIds::from("volt"), "1.2V".to_string())];
    assert!(
        McConds::check_condition(&cond_mv, &volts),
        "1.2V must equal 1200mV"
    );
}

/// A unitless number carries no family of its own, so it adopts the family of
/// the dimensioned side — that is what makes `volt < 0` a voltage test.
#[test]
fn eval__unitless_number_adopts_the_dimensioned_family() {
    let _lock = common::lock();
    common::reset();

    let cond = McCondition::Lt {
        left: ident("volt"),
        right: lit("0"),
    };
    let negative = vec![(McIds::from("volt"), "-2.5V".to_string())];
    let positive = vec![(McIds::from("volt"), "2.5V".to_string())];
    assert!(
        McConds::check_condition(&cond, &negative),
        "-2.5V must be below the bare 0"
    );
    assert!(
        !McConds::check_condition(&cond, &positive),
        "2.5V must not be below the bare 0"
    );

    // The bare side may equally be the left operand.
    let cond_zero_first = eq(lit("0V"), ident("volt"));
    let zero = vec![(McIds::from("volt"), "0".to_string())];
    assert!(McConds::check_condition(&cond_zero_first, &zero));
}

/// Hex and decimal spellings of one integer are one value.
#[test]
fn eval__hex_and_decimal_are_the_same_number() {
    let _lock = common::lock();
    common::reset();

    let cond = eq(ident("address"), lit("0x36"));
    let decimal = vec![(McIds::from("address"), "54".to_string())];
    let hex = vec![(McIds::from("address"), "0x36".to_string())];
    assert!(McConds::check_condition(&cond, &decimal));
    assert!(McConds::check_condition(&cond, &hex));

    let cond_bit = McCondition::BitAnd {
        left: ident("address"),
        right: lit("0x01"),
    };
    let odd = vec![(McIds::from("address"), "0x37".to_string())];
    assert!(McConds::check_condition(&cond_bit, &odd));
}

/// `in` reads its listed values through the same engine as `==`.
#[test]
fn eval__in_list_uses_the_same_values() {
    let _lock = common::lock();
    common::reset();

    let rails = McCondition::In {
        left: ident("volt"),
        values: vec!["1200mV".to_string(), "3.3V".to_string()],
    };
    let scaled = vec![(McIds::from("volt"), "1.2V".to_string())];
    let unlisted = vec![(McIds::from("volt"), "1.5V".to_string())];
    assert!(McConds::check_condition(&rails, &scaled));
    assert!(!McConds::check_condition(&rails, &unlisted));

    // A list of bare numbers against a dimensioned operand of the same family.
    let numbers = McCondition::In {
        left: ident("volt"),
        values: vec!["0".to_string(), "5".to_string()],
    };
    let zero = vec![(McIds::from("volt"), "0V".to_string())];
    assert!(McConds::check_condition(&numbers, &zero));
}

/// A comparison between text and a quantity has no reading: it fails as a
/// diagnostic instead of quietly coming out false.
#[test]
fn eval__ill_typed_comparison_is_reported() {
    let _lock = common::lock();
    common::reset();

    let cond = eq(lit("auto"), lit("1.2V"));
    let params: Vec<(McIds, String)> = Vec::new();
    let err = McConds::check_condition_result(&cond, &params)
        .expect_err("text against a quantity must not evaluate");
    assert_eq!(
        err.code(),
        mcc::errcodes::EVAL_OPERAND_NOT_NUMERIC,
        "error carries the registered code"
    );

    // The boolean face keeps its old signature: the caller that has no node to
    // report at sees the condition as unsatisfied.
    assert!(!McConds::check_condition(&cond, &params));

    // Two pieces of text still compare as text (partno == "PA9555").
    let text_cond = eq(ident("package_style"), lit("DFN"));
    let text_params = vec![(McIds::from("package_style"), "DFN".to_string())];
    assert!(McConds::check_condition(&text_cond, &text_params));

    // Text has no order: `"A" < "B"` is not a comparison the domain defines.
    let ordered_text = McCondition::Lt {
        left: ident("package_style"),
        right: lit("DFN"),
    };
    assert!(
        McConds::check_condition_result(&ordered_text, &text_params).is_err(),
        "ordering two pieces of text must not evaluate"
    );
}

/// End to end: a bound argument selects the matching branch of the func.
#[test]
fn eval__func_branch_follows_the_bound_value() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/eval-engine.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);
    mcc::mcc_build(&McIds::from("main"), &uri).expect("build failed");

    let cmie = mcc::get_def(&McIds::from("REG_IFACE"), &uri).expect("component not found");
    let mcc::McCMIE::Component(comp) = cmie else {
        panic!("REG_IFACE is not a Component");
    };
    let connect = comp
        .funcs
        .find("Connect")
        .unwrap_or_else(|| panic!("func 'Connect' missing"));
    let conds = connect
        .conds
        .first()
        .expect("func 'Connect' lost its condition blocks");
    assert_eq!(conds.if_blocks.len(), 2, "if/else if must both be parsed");

    let branch = |text: &str| -> String {
        let params = vec![(McIds::from("volt"), text.to_string())];
        conds
            .evaluate(&params)
            .iter()
            .map(|stmt| stmt.to_string())
            .collect::<Vec<_>>()
            .join(" | ")
    };

    // The argument reaches the branch in the notation the caller wrote; the
    // comparison is the same whichever notation that is.
    assert!(branch("1200mV").contains("NET12"), "{}", branch("1200mV"));
    assert!(branch("1.2V").contains("NET12"), "{}", branch("1.2V"));
    assert!(branch("3300mV").contains("NET33"), "{}", branch("3300mV"));
    assert!(branch("5V").contains("NET50"), "{}", branch("5V"));
}

/// The interface lives in `supply.mc`; every consumer below declares its own
/// pins through it, so the failing condition belongs to a file the consumer
/// never edited. (`rail` would be a keyword, and a keyword in a `use` path is a
/// parse error, so the file is not called that.)
const RAIL_IFACE: &str = r#"
interface RAIL(volt)
{
    if (volt == 3.3V)
        pins = [
            1 = VCC3V3, "positive", voltage:3.3V
            2 = GND, "ground", voltage:0.0V
        ]
    else
        pins = [
            1 = VCC, "positive", voltage:volt
            2 = GND, "ground", voltage:0.0V
        ]
}
"#;

/// `{arg}` is the interface argument under test.
const RAIL_CONSUMER: &str = r#"
use ./supply.mc

component LDO_LIT
{
    pins = [
        in [1,2] = VIN{Vin, GND}::RAIL({arg})
    ]
}

module main
{
    io VMAIN
    io VDD
    LDO_LIT ldo
    VMAIN -> ldo.VIN.Vin
    ldo.VIN.GND -> GND
    ldo.VIN.Vin -> VDD
}
"#;

/// Load `rail.mc` + a consumer built with `arg` and return its diagnostics as
/// `(code, uri, row, message)`.
fn rail_diagnostics(tag: &str, arg: &str) -> Vec<(u32, String, u32, String)> {
    let _lock = common::lock();
    let dir = std::env::temp_dir().join(format!("mcc-eval-rail-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join("supply.mc"), RAIL_IFACE).expect("write supply.mc");
    std::fs::write(dir.join("main.mc"), RAIL_CONSUMER.replace("{arg}", arg))
        .expect("write main.mc");
    let uri: McURI = dir
        .join("main.mc")
        .canonicalize()
        .expect("canonicalize")
        .to_string_lossy()
        .to_string();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&dir);
    mcc::mcc_load_project(&uri);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000);

    let diags = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.loc.uri.clone(), d.loc.row, d.msg.clone()))
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    diags
}

/// An interface condition is written in the interface file, but a failing one is
/// the *consumer's* problem — the argument is the consumer's syntax. A range
/// literal handed to a single-value parameter is exactly that, and it must be
/// reported once, in the consuming file, naming the argument as written.
#[test]
fn eval__literal_arg_condition_reports_in_the_consumer() {
    let diags = rail_diagnostics("literal", "2.5V~5.5V");
    let hits: Vec<_> = diags
        .iter()
        .filter(|(code, ..)| *code == mcc::errcodes::EVAL_OPERAND_NOT_NUMERIC)
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "one construction reports one condition failure; got {diags:?}"
    );
    let (_, uri, _, msg) = hits[0];
    assert!(
        uri.ends_with("/main.mc"),
        "the failure belongs to the consuming file, not to supply.mc; got {uri}"
    );
    assert!(
        msg.contains("2.5V~5.5V"),
        "the message must name the argument the consumer wrote; got {msg}"
    );
}

/// A symbolic argument (`::RAIL(volt)`) and an absent one (`::RAIL()`) leave the
/// parameter unreduced: the value may still arrive from the instance or the
/// spec, so the condition is undecided rather than wrong, and saying nothing is
/// the only honest answer.
#[test]
fn eval__unreduced_arg_condition_is_not_reported() {
    for arg in ["volt", ""] {
        let diags = rail_diagnostics("unreduced", arg);
        let hits: Vec<_> = diags
            .iter()
            .filter(|(code, ..)| (5413..=5415).contains(code))
            .collect();
        assert!(
            hits.is_empty(),
            "::RAIL({arg}) must not be reported as an operator error; got {hits:?}"
        );
    }
}

/// A definition with no parameters still has an environment: a condition over
/// literals alone decides with no argument at all, so the branch that wins must
/// land its own pins on the instance.
#[test]
fn eval__noparam_condition_still_evaluates() {
    let _lock = common::lock();
    common::reset();

    let source = r#"
component CA
{
    if (1 == 1)
        pins = [ 1 = NETA ]
    else
        pins = [ 1 = NETB ]
}

component CB
{
    if (1 == 2)
        pins = [ 1 = NETA ]
    else
        pins = [ 2 = NETB ]
}

module main
{
    io X
    CA u1
    CB u2
    u1.1 -> X
    u2.2 -> X
}
"#;
    let uri: McURI = "/mcc/eval-noparam-cond.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let (_, _, _, net_store) =
        mcc::mcc_build_with_nets(&McIds::from("main"), &uri).expect("build failed");

    // A dropped block leaves the connection pointing at a pin the instance
    // never got, and the only sign is the downstream warning. The net path
    // itself is the same either way (a ghost point prints like a real one),
    // so the warning is what tells the two apart.
    let not_found: Vec<String> = mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| d.code == mcc::errcodes::COMPONENT_PIN_NOT_FOUND)
        .map(|d| format!("{}:{} {}", d.loc.uri, d.loc.row, d.msg))
        .collect();
    assert!(
        not_found.is_empty(),
        "the conditional blocks' pins never landed: {not_found:?}"
    );

    // Both conditions are literal-only, so both decide with no argument at
    // all: the true branch takes pin 1, the false branch pin 2.
    let paths: Vec<String> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .flat_map(|(_, pts)| pts.iter().map(|p| p.path.clone()))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        paths.iter().any(|p| p.contains("u1.1")),
        "u1's if-branch connection is gone: {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.contains("u2.2")),
        "u2's else-branch connection is gone: {paths:?}"
    );
}
