use serde_json::Value;
use std::process::Command;

fn parse(source: &str) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", source, "--pass1", "--pass2", "--top", "main", "-f", "json",
        ])
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
fn valid_labels_members_and_module_ports_are_quiet() {
    let source = r#"component SIMPLE_LED
{
    name = "LED"
    pins = [
        1 = ANODE
        2 = CATHODE
    ]
}

module LED_INDICATOR(in signal, ps ground)
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
    let forbidden = [1402, 2310, 2401, 2403, 2410, 2801, 3105];
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
fn attribute_condition_chain_keeps_final_else_without_empty_pin_block() {
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
    let forbidden = [1402, 3001, 3002, 3402];
    assert!(
        diagnostics(&result)
            .iter()
            .all(|diagnostic| !forbidden.contains(&diagnostic["code"].as_u64().unwrap_or(0))),
        "unexpected conditional diagnostics: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn parameterless_component_without_parentheses_has_no_empty_parens_hint() {
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
    assert!(!has_code(&result, 2204), "unexpected E2204: {result}");
}

#[test]
fn invalid_component_member_still_reports_member_error() {
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
        has_code(&result, 1802),
        "missing invalid-member diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
    assert!(!has_code(&result, 2403));
}

#[test]
fn constructor_mismatch_names_the_component_class() {
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
    let result = parse(source);
    let diagnostic = diagnostics(&result)
        .iter()
        .find(|diagnostic| diagnostic["code"].as_u64() == Some(2801))
        .expect("constructor mismatch diagnostic");
    let message = diagnostic["message"].as_str().expect("diagnostic message");
    assert!(message.contains("component 'PARAMETERIZED'"), "{message}");
    assert!(!message.contains("component 'U_INSTANCE'"), "{message}");
}

#[test]
fn real_scalar_type_mismatch_still_reports_type_error() {
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
        has_code(&result, 3402),
        "missing scalar type mismatch diagnostic: {}",
        result["result"]["pass0"]["diagnostics"]
    );
}

#[test]
fn method_calls_do_not_become_port_or_reference_warnings() {
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
    assert!(!has_code(&result, 2403), "unexpected E2403: {result}");
    assert!(!has_code(&result, 3105), "unexpected E3105: {result}");
}

#[test]
fn named_inline_constructor_keeps_unit_arguments() {
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
    assert!(!has_code(&result, 2403), "unexpected E2403: {result}");
    assert!(!has_code(&result, 2801), "unexpected E2801: {result}");
    assert!(!has_code(&result, 3105), "unexpected E3105: {result}");
}

#[test]
fn literal_default_is_constant_and_makes_parameter_optional() {
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
    assert!(!has_code(&result, 2801), "unexpected E2801: {result}");
    assert!(!has_code(&result, 2806), "unexpected E2806: {result}");
}

#[test]
fn positional_interface_aliases_satisfy_complete_binding() {
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
    assert!(!has_code(&result, 3101), "unexpected E3101: {result}");
}
