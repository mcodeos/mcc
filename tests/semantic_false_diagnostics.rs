// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use serde_json::Value;
use std::process::Command;

fn parse(source: &str) -> Value {
    parse_with(source, false)
}

/// Like [`parse`] but with `--strict`, which surfaces strict-only diagnostics
/// (e.g. E5352 missing-required-constructor-arg) as warnings. The
/// Component-Spec Separation rework made those silent in dev mode, so tests
/// that assert a strict-only diagnostic must run strict.
fn parse_strict(source: &str) -> Value {
    parse_with(source, true)
}

fn parse_with(source: &str, strict: bool) -> Value {
    let mut args = vec![
        "parse".to_string(),
        "--code".to_string(),
        source.to_string(),
        "--local".to_string(),
        "--pass1".to_string(),
        "--pass2".to_string(),
        "--top".to_string(),
        "main".to_string(),
        "-f".to_string(),
        "json".to_string(),
    ];
    if strict {
        args.push("--strict".to_string());
    }
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args(&args)
        .output()
        .expect("run mcc parse");
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).expect("parse mcc JSON output")
}

fn diagnostics(value: &Value) -> &[Value] {
    value["result"]["pass0"]["diagnostics"]
        .as_array()
        .expect("Pass 0 diagnostics")
}

fn has_code(value: &Value, code: u64) -> bool {
    diagnostics(value)
        .iter()
        .any(|diagnostic| diagnostic["code"].as_u64() == Some(code))
}

#[test]
fn sem_falsediag__valid_labels_members_and_module_ports_are_quiet() {
    let source = r#"component SIMPLE_LED
{
    name = "LED"
    pins = [
        1 = ANODE
        2 = CATHODE
    ]
}

module LED_INDICATOR(in signal, psnk ground)
{
    SIMPLE_LED D_STATUS
    signal -> D_STATUS.ANODE
    D_STATUS.CATHODE -> ground
}

module main
{
    LED_INDICATOR STATUS_GREEN
    VCC -> STATUS_GREEN.signal
    STATUS_GREEN.ground -> GND
}
"#;
    let result = parse(source);
    let forbidden = [5641, 5151, 5154, 5351, 5352, 4108];
    assert!(
        diagnostics(&result)
            .iter()
            .all(|diagnostic| !forbidden.contains(&diagnostic["code"].as_u64().unwrap_or(0))),
        "unexpected false diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert_eq!(result["result"]["summary"]["errors"], 0);
    assert_eq!(result["result"]["summary"]["warnings"], 0);
}

#[test]
fn sem_falsediag__attr_cond_chain_keeps_final_else_without_empty_pin_block() {
    let source = r#"component CONFIG(kind::STRING)
{
    name = "Config"
    pins = [
        1 = INPUT
        2 = OUTPUT
    ]
    if (kind == "x")
    {
        package = "x"
    }
    else if (kind == "y")
    {
        package = "y"
    }
    else
    {
        package = "other"
    }
}

module main
{
    CONFIG("1206") U_CONFIG
    INPUT_NET -> U_CONFIG.INPUT
    U_CONFIG.OUTPUT -> OUTPUT_NET
}
"#;
    let result = parse(source);
    let forbidden = [5641, 5451, 5452, 5552];
    assert!(
        diagnostics(&result)
            .iter()
            .all(|diagnostic| !forbidden.contains(&diagnostic["code"].as_u64().unwrap_or(0))),
        "unexpected conditional diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn sem_falsediag__parameterless_component_no_empty_parens_hint() {
    let source = r#"component WITHOUT_PARENS
{
    name = "No parens"
    pins = [1 = SIGNAL]
}

module main
{
    WITHOUT_PARENS U_NO_PARAMS
}
"#;
    let result = parse(source);
    assert!(
        !diagnostics(&result).iter().any(|d| d["message"]
            .as_str()
            .is_some_and(|m| m.to_lowercase().contains("parenthes"))),
        "unexpected empty-parens hint: {result}"
    );
}

#[test]
fn sem_falsediag__invalid_component_member_still_reports_member_error() {
    let source = r#"component SIMPLE_LED
{
    name = "LED"
    pins = [1 = ANODE]
}

module main
{
    SIMPLE_LED D_STATUS
    INPUT_NET -> D_STATUS.MISSING
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 3179),
        "missing invalid-member diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn sem_falsediag__constructor_mismatch_names_the_component_class() {
    let source = r#"component PARAMETERIZED(value::STRING)
{
    name = "Parameterized"
    pins = [1 = SIGNAL]
}

module main
{
    PARAMETERIZED U_INSTANCE
}
"#;
    let result = parse_strict(source);
    let diagnostic = diagnostics(&result)
        .iter()
        .find(|diagnostic| diagnostic["code"].as_u64() == Some(5352))
        .expect("constructor mismatch diagnostic");
    let message = diagnostic["message"].as_str().expect("diagnostic message");
    assert!(message.contains("component 'PARAMETERIZED'"), "{message}");
    assert!(!message.contains("component 'U_INSTANCE'"), "{message}");
}

#[test]
fn sem_falsediag__real_scalar_type_mismatch_still_reports_type_error() {
    let source = r#"component INTEGER_PART(count::INT)
{
    name = "Integer part"
    pins = [1 = SIGNAL]
}

module main
{
    INTEGER_PART("not-an-int") U_PART
}
"#;
    let result = parse(source);
    assert!(
        has_code(&result, 5552),
        "missing scalar type mismatch diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn sem_falsediag__method_calls_do_not_become_port_or_reference_warnings() {
    let source = r#"component INDICATOR
{
    name = "Indicator"
    pins = [
        1 = ANODE
        2 = CATHODE
    ]

    func Connect(signal, ground)
    {
        signal -> ANODE
        CATHODE -> ground
    }
}

module main
{
    INDICATOR D_STATUS
    D_STATUS.Connect(STATUS_SIGNAL, GND)
}
"#;
    let result = parse(source);
    assert!(!has_code(&result, 4108), "unexpected E4108: {result}");
}

#[test]
fn sem_falsediag__named_inline_constructor_keeps_unit_arguments() {
    let source = r#"component FILTER_CAP(value::UV.CAP, rating::UV.VOLT)
{
    name = "Filter capacitor"
    pins = [
        1 = POSITIVE
        2 = NEGATIVE
    ]

    func Connect(rail, ground)
    {
        rail -> POSITIVE
        NEGATIVE -> ground
    }
}

module main
{
    C_FILTER::FILTER_CAP(100nF, 10V).Connect(VCC, GND)
}
"#;
    let result = parse(source);
    assert!(!has_code(&result, 5352), "unexpected E5352: {result}");
    assert!(!has_code(&result, 4108), "unexpected E4108: {result}");
}

#[test]
fn sem_falsediag__literal_default_is_constant_makes_param_optional() {
    let source = r#"component VARIANT(partno::STRING = "SMALL")
{
    name = "Variant"
    pins = [1 = SIGNAL]
}

module main
{
    VARIANT U_DEFAULT
}
"#;
    let result = parse(source);
    assert!(!has_code(&result, 5352), "unexpected E5352: {result}");
    assert!(!has_code(&result, 5357), "unexpected E5357: {result}");
}

#[test]
fn sem_falsediag__positional_interface_aliases_satisfy_complete_binding() {
    let source = r#"interface DIFFERENTIAL(role)
{
    pins = [
        1 = POSITIVE
        2 = NEGATIVE
    ]

    role Endpoint
    {
        name = "Endpoint"
    }
}

component TRANSCEIVER
{
    name = "Transceiver"
    pins = [
        [1,2] = DATA{P, N}::DIFFERENTIAL(Endpoint)
    ]
}

module main
{
    TRANSCEIVER U_TRANSCEIVER
    NET_P -> U_TRANSCEIVER.DATA.P
    NET_N -> U_TRANSCEIVER.DATA.N
}
"#;
    let result = parse(source);
    assert!(!has_code(&result, 4102), "unexpected E4102: {result}");
}

#[test]
fn sem_falsediag__embedded_square_interface_binding_no_pin_count_false_positive() {
    // `io [3,4] = I2C[SDA,SCL]::I2C()` is the embedded-square list form: the
    // single IDA `I2C[SDA,SCL]` registers its pins as `I2CSDA`/`I2CSCL`
    // (prefix + member, see derive_interface_subnames). The E5262 interface
    // pin-count check must count those physical pins as bound — a regression
    // guard for the real `SYS.CLOCK` timer.mc binding that reported 0/2.
    let source = r#"interface I2C
{
    pins = [
        1 = SCL, "Serial Clock"
        2 = SDA, "Serial Data"
    ]
}

component CLOCK_GEN
{
    name = "Clock generator"
    pins = [
        io [3,4] = I2C[SDA,SCL]::I2C(), ["I2C data", "I2C clock"]
    ]
}

module main
{
}
"#;
    let result = parse(source);
    assert!(!has_code(&result, 5262), "unexpected E5262: {result}");
}

#[test]
fn sem_falsediag__role_peer_multi_role_list_matches_defined_roles() {
    // `peer = [Master, Slave]` refers to multiple roles, each defined earlier
    // in the same interface. The check must compare each member individually,
    // not the whole bracket-list text (which never equals a single role name).
    let source = r#"interface UART.RS485
{
    role Master
    {
        name = "RS485 Master"
    }
    role Slave
    {
        name = "RS485 Slave"
    }
    role Repeater
    {
        name = "RS485 Repeater"
        peer = [Master, Slave]
    }
}

module main
{
}
"#;
    let result = parse(source);
    assert!(!has_code(&result, 5506), "unexpected E5506: {result}");
}

#[test]
fn sem_falsediag__role_peer_list_missing_member_still_reports() {
    // A peer list with a genuinely undefined member must still be flagged.
    let source = r#"interface UART.RS485
{
    role Master
    {
        name = "RS485 Master"
    }
    role Repeater
    {
        name = "RS485 Repeater"
        peer = [Master, Slave]
    }
}

module main
{
}
"#;
    let result = parse(source);
    assert!(has_code(&result, 5506), "expected E5506: {result}");
}

// The synthetic-wrapper carve-out (`is_synthetic_module`) must only exempt
// fabricated `VIRT_<T>` modules, never real user modules. These guards assert
// genuine single-character / shadowing names are still flagged.

#[test]
fn sem_falsediag__real_module_single_char_instance_still_reports() {
    let source = r#"interface ADC.DIFF(role)
{
    pins = [
        1 = P, "Positive"
        2 = N, "Negative"
    ]
}

module main
{
    ADC.DIFF u
}
"#;
    let result = parse(source);
    assert!(has_code(&result, 5054), "expected E5054: {result}");
}

#[test]
fn sem_falsediag__real_module_shadowing_instance_still_reports() {
    let source = r#"interface LIN(role)
{
    pins = [
        1 = LIN, "Data"
    ]
}

module main
{
    LIN LIN
}
"#;
    let result = parse(source);
    assert!(has_code(&result, 5052), "expected E5052: {result}");
    assert!(has_code(&result, 5056), "expected E5056: {result}");
}

#[test]
fn sem_falsediag__real_module_shadowing_param_still_reports() {
    let source = r#"interface LIN(role)
{
    pins = [
        1 = LIN, "Data"
    ]
}

module main(io LIN)
{
}
"#;
    let result = parse(source);
    assert!(has_code(&result, 5057), "expected E5057: {result}");
}

/// Header-DC direction rule (E3055)
/// A module-header interface-typed (power/DC) parameter must carry an explicit
/// energy-direction word. The no-direction sugar (`module X([VDD,GND]::DC(v))`,
/// `module X(dc{VDD,GND}::DC(v))`, `module X(pwr::DC(v))`) is removed: E3055,
/// no legacy tolerance. Every shape must error, and the directed form must not.
#[test]
fn sem_falsediag__directionless_module_header_power_param_is_e3055() {
    // Square net-list form, curly name-prefix form, and bare-name declare form.
    let source = r#"module SQ([VDD, GND]::DC(3.3V))
{
}
module CB(dc{VDD_3V3, GND}::DC(3.3V))
{
}
module BA(pwr::DC(5V))
{
}
module main
{
}
"#;
    let result = parse(source);
    let hits: Vec<&str> = diagnostics(&result)
        .iter()
        .filter(|d| d["code"].as_u64() == Some(3055))
        .map(|d| d["message"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        hits.len(),
        3,
        "expected E3055 once per directionless module header (SQ/CB/BA), and \
         no module beyond them (shape-driven, exactly the no-direction forms): {result}"
    );
    assert!(
        hits.iter().all(|m| m.contains("direction word")),
        "E3055 message must name the missing direction word: {hits:?}"
    );
}

#[test]
fn sem_falsediag__directed_and_func_header_power_params_quiet() {
    // Explicit direction words (any of psnk/psrc/psbi) on module-header power
    // declares are the canonical form and must be E3055-silent. A `func`-header
    // interface declare is a leaf-macro label, not a module port — also quiet.
    let source = r#"module main(psnk [VDD_3V3, GND]::DC(3.3V))
{
    func pwr([A, B]::DC(3.3V))
    {
    }
    pwr -> GND
}
module MIXED(psrc out_v{VBUS, GND}::DC(5V), psbi io_v{VCC, GND}::DC(3.3V))
{
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 3055),
        "directed module-header power params and func-header declares must be \
         E3055-silent: {result}"
    );
}

#[test]
fn sem_falsediag__directed_header_power_port_counts_toward_arity() {
    // A direction-word header port (`psnk [VDD_3V3, GND]::DC(3.3V)`) is a real
    // constructor formal: `PSUB p1(V3V3)` binds it by position, so it must
    // count toward the declared arity and stay E5352-silent.
    let source = r#"module PSUB(psnk [VDD_3V3, GND]::DC(3.3V))
{
}
module main()
{
    PSUB p1(V3V3)
}
"#;
    let result = parse(source);
    assert!(
        !has_code(&result, 5352),
        "a direction-word header port must count toward the module arity: {result}"
    );

    // The count is real, not suppressed: one extra arg still reports.
    let over = r#"module PSUB(psnk [VDD_3V3, GND]::DC(3.3V))
{
}
module main()
{
    PSUB p1(V3V3, V5V)
}
"#;
    assert!(
        has_code(&parse(over), 5352),
        "passing more args than the declared direction-word ports must report"
    );
}

/// A subscript fused onto a key's first segment is reported for *every* word,
/// registered or not: the criterion is the key's lexical form. The registry
/// only decides whether the message can name a legal spelling instead, so the
/// hint appears for a word the grammar reserves and not for an invented one.
#[test]
fn sem_falsediag__fused_subscript_key_reports_every_word_as_an_error() {
    for key in ["pins[1]", "spec[0]", "voltage[0]", "foo[0]", "layout[0]"] {
        let source = format!("component F {{\n    {key} = 1\n}}\nmodule main {{\n    F f1\n}}\n");
        let result = parse(&source);
        let hit = diagnostics(&result)
            .iter()
            .find(|d| d["code"].as_u64() == Some(5351))
            .unwrap_or_else(|| panic!("no 5351 for key `{key}`: {result}"));
        assert_eq!(
            hit["severity"], "error",
            "`{key}` must report at error level: {result}"
        );
    }
}

/// The hint is the only part the registry decides: a reserved word's row names
/// the legal spellings, an invented word's row does not.
#[test]
fn sem_falsediag__fused_subscript_hint_follows_the_registry() {
    let reserved = parse("component F {\n    pins[1] = 1\n}\nmodule main {\n    F f1\n}\n");
    assert!(
        diagnostics(&reserved)
            .iter()
            .any(|d| d["code"].as_u64() == Some(5351)
                && d["message"].as_str().unwrap_or("").contains("pins{...}")),
        "a reserved word's report must name its legal spelling: {reserved}"
    );

    let invented = parse("component F {\n    foo[0] = 1\n}\nmodule main {\n    F f1\n}\n");
    assert!(
        diagnostics(&invented)
            .iter()
            .any(|d| d["code"].as_u64() == Some(5351)
                && !d["message"].as_str().unwrap_or("").contains("{...}")),
        "an invented word has no legal spelling to name: {invented}"
    );
}

/// A key without a subscript is untouched: the check must not turn every
/// registered word into a report.
#[test]
fn sem_falsediag__plain_key_without_subscript_is_quiet() {
    let result =
        parse("component F {\n    voltage = 1\n    foo = 1\n}\nmodule main {\n    F f1\n}\n");
    assert!(
        !has_code(&result, 5351),
        "a subscript is what makes the report, not the word: {result}"
    );
}

/// A pin-name row that materializes zero pins is reported at the row, not
/// dropped in silence: E3004 for the arithmetic-name form (`A - B` is not a
/// pin name) and for the non-enumerable colon (`1:A = B`). Both otherwise left
/// the row with zero pins and the user with only an indirect downstream error.
#[test]
fn sem_falsediag__pin_row_that_declares_nothing_is_e3004() {
    for (row, what) in [
        ("1 = A - B", "an arithmetic name is not a pin name"),
        ("1 = A + B", "an arithmetic name is not a pin name"),
        ("1:A = B", "a colon whose endpoints are not enumerable"),
    ] {
        let source = format!(
            "component P {{\n    pins = [\n        {row}\n        2 = B\n    ]\n}}\nmodule main {{\n    P p1\n}}\n"
        );
        let result = parse(&source);
        let hits: Vec<&Value> = diagnostics(&result)
            .iter()
            .filter(|d| d["code"].as_u64() == Some(3004))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "`{row}` ({what}) must report E3004 exactly once: {result}"
        );
        assert_eq!(
            hits[0]["severity"], "error",
            "`{row}` must report at error level: {result}"
        );
    }
}

/// The row above's opposite: a well-formed pin list stays E3004-silent, so the
/// lock proves the report comes from the dropped row rather than from every
/// `pins` block.
#[test]
fn sem_falsediag__well_formed_pin_rows_are_e3004_silent() {
    let source = "component P {\n    pins = [\n        1 = A\n        [2,3] = B{C, D}\n        4:5 = E\n    ]\n}\nmodule main {\n    P p1\n}\n";
    let result = parse(source);
    assert!(
        !has_code(&result, 3004),
        "a well-formed pin list must not report E3004: {result}"
    );
    assert_eq!(result["result"]["summary"]["errors"], 0);
}
