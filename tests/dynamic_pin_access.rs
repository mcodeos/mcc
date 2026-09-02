use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state
/// (same pattern as `tests/dynamic_pin_expansion.rs`).
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[test]
fn conditional_pin_alias_resolves_to_physical_pin() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/dynamic-pin-access.mc".to_string();
    let source = r#"
component CONFIGURABLE(partno::STRING = "BASE")
{
    pins = [
        io 1 = BASE_IO
    ]

    if (partno == "WIDE")
    {
        pins += [
            io [2:3] = GPIO[8:9]
        ]
    }
}

module main
{
    CONFIGURABLE("WIDE") U_WIDE
    SIGNAL -> U_WIDE.GPIO8
}
"#;

    mcc::mcc_load_from_string(&uri, source);
    let (instance, arena, store, _net_store) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build dynamic pin fixture");
    let diagnostics = mcc::mcc_diagnose_all();

    assert!(
        diagnostics.iter().all(|diagnostic| diagnostic.code != 1802),
        "dynamic pin alias was rejected: {:?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, &diagnostic.msg))
            .collect::<Vec<_>>()
    );

    let paths: Vec<&str> = instance
        .connections
        .iter()
        .flat_map(|connection| connection.points.iter().map(|point| point.path.as_str()))
        .collect();
    assert!(paths.contains(&"U_WIDE.2"), "resolved paths: {paths:?}");
    assert!(
        !paths.contains(&"U_WIDE.GPIO8"),
        "resolved paths: {paths:?}"
    );

    let view = mcc::TreeView::new(&arena, &store);
    let component = view
        .components(&instance)
        .find(|component| component.name == "U_WIDE")
        .expect("U_WIDE instance");
    assert_eq!(component.pin_name("2").as_deref(), Some("GPIO8"));
}

/// Regression: LSP goto-definition span for `label::Class(...)` pin declarations.
///
/// `io [16,17,21] = ADC::ADC.DIFF(Receiver)` must record the span of the io
/// label `ADC` (the instance name, `label_pos..label_pos+3`), not the whole
/// binding expression or the class name `ADC.DIFF`. The class/instance AST nodes
/// are linked in reverse source order by mc_declare_b, so the span must come
/// directly from the parsed instance node.
#[test]
fn declare_io_label_span_is_the_instance_name() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/pin-name-span.mc".to_string();
    let source = r#"
interface ADC.DIFF(role)
{
    pins = [
        1 = P, "Positive Input"
        2 = N, "Negative Input"
        3 = GND, "Ground"
    ]
    role Receiver { name = "ADC.DIFF Receiver" }
    role Transmitter { name = "ADC.DIFF Transmitter" }
}

interface UART.TTL(role)
{
    pins = [
        1 = TX, "Transmit"
        2 = RX, "Receive"
    ]
    role DCE {
        name = "UART.TTL DCE"
        pins = [
            1 = TX, "Transmit"
            2 = RX, "Receive"
        ]
        peer = DTE
    }
    role DTE {
        name = "UART.TTL DTE"
        pins = [
            1 = RX, "Receive"
            2 = TX, "Transmit"
        ]
        peer = DCE
    }
}

component MCU
{
    pins = [
        io [16, 17, 21] = ADC::ADC.DIFF(Receiver)
        io [6, 7] = UART0::UART.TTL(DCE) | UART2::UART.TTL(DTE)
    ]
}

module main
{
    func loadFlash(spi)
    {
        spi + uC.UART0
    }

    MCU uC
}
"#;

    mcc::mcc_load_from_string(&uri, source);
    let (instance, arena, store, _net_store) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build pin name span fixture");

    let view = mcc::TreeView::new(&arena, &store);
    let component = view
        .components(&instance)
        .find(|component| component.name == "uC")
        .expect("uC instance");

    // `ADC::ADC.DIFF(Receiver)` — span must be the io label `ADC`, not the class
    // or the whole binding expression.
    let adc_label_pos = source
        .find("ADC::ADC.DIFF")
        .expect("io label `ADC` present in source");
    let adc_span = component
        .def
        .pins
        .pin_name_spans
        .get("ADC")
        .expect("pin name span for `ADC`");
    assert_eq!(
        *adc_span,
        adc_label_pos..adc_label_pos + 3,
        "goto-def must land exactly on the io label `ADC`, not the whole binding expression"
    );

    // `UART0::UART.TTL(DCE) | UART2::UART.TTL(DTE)` — multi-option form like
    // `us513.mc` line 20. Each alternative's io label must keep its own precise
    // span (the instance name), not share the last option's span or cover the
    // whole binding expression.
    let uart0_label_pos = source
        .find("UART0::UART.TTL")
        .expect("io label `UART0` present in source");
    let uart0_span = component
        .def
        .pins
        .pin_name_spans
        .get("UART0")
        .expect("pin name span for `UART0`");
    assert_eq!(
        *uart0_span,
        uart0_label_pos..uart0_label_pos + 5,
        "goto-def must land exactly on the io label `UART0`, not the whole binding expression"
    );
    let uart2_label_pos = source
        .find("UART2::UART.TTL")
        .expect("io label `UART2` present in source");
    let uart2_span = component
        .def
        .pins
        .pin_name_spans
        .get("UART2")
        .expect("pin name span for `UART2`");
    assert_eq!(
        *uart2_span,
        uart2_label_pos..uart2_label_pos + 5,
        "each alternative's goto-def must land exactly on its own io label"
    );

    // `spi + uC.UART0` inside `func loadFlash(spi)` — the chain `uC.UART0`
    // must be recorded into the function's own insts (module-level instances
    // are referenced from func bodies; us513.mc line 144 `spi + uC.SPI` is
    // the same shape).
    let func = instance
        .def
        .funcs
        .iter()
        .find(|f| f.name.to_string() == "loadFlash")
        .expect("loadFlash func");
    let uart0_chain = func
        .insts
        .iter_chain_refs()
        .find(|(_, segments, _)| {
            segments
                .iter()
                .any(|s| matches!(s, mcc::refdef::ChainSegment::Ident(n) if n == "uC"))
                && segments
                    .iter()
                    .any(|s| matches!(s, mcc::refdef::ChainSegment::Ident(n) if n == "UART0"))
        })
        .expect("func body must record the `uC.UART0` chain ref");
    assert_eq!(uart0_chain.1.len(), 2, "chain must be [uC, UART0]");

    // Resolution side: the func-body chain resolves against MODULE insts
    // (`uC` is a module instance, not a func-local one) and lands on the
    // uC component's `UART0` io label with a precise span.
    let hit = mcc::refdef::chain::resolve_member_chain_from_segments(
        &uri,
        &uart0_chain.1,
        &instance.def.insts,
        &instance.def.params,
    )
    .expect("func-body chain `uC.UART0` must resolve");
    assert_eq!(hit.name, "uC.UART0");
    assert_eq!(
        hit.span,
        uart0_label_pos..uart0_label_pos + 5,
        "func-body goto-def must land exactly on the uC io label `UART0`"
    );
}

/// Regression: E4102 (IFACE_PINS_NOT_ALL_BOUND) must be reported at the
/// interface binding label (`ADC` in `io [16, 17] = ADC::ADC.DIFF(Receiver)`),
/// not at the component class name. The precise binding span is available via
/// `McPins::pin_name_spans` (same key as `names_to_id`).
#[test]
fn iface_pins_not_all_bound_reported_at_binding() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/iface-bound-span.mc".to_string();
    let source = r#"
interface ADC.DIFF(role)
{
    pins = [
        1 = P, "Positive Input"
        2 = N, "Negative Input"
        3 = GND, "Ground"
    ]
    role Receiver { name = "ADC.DIFF Receiver" }
}

component MCU
{
    pins = [
        io [16, 17] = ADC::ADC.DIFF(Receiver)
    ]
}

module main
{
    MCU uC
}
"#;

    mcc::mcc_load_from_string(&uri, source);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);

    let binding_pos = source
        .find("ADC::ADC.DIFF")
        .expect("binding present in source") as u32;

    let all = mcc::mcc_diagnose_all();
    let e4102: Vec<_> = all
        .iter()
        .filter(|d| d.code == mcc::errcodes::IFACE_PINS_NOT_ALL_BOUND)
        .collect();
    assert!(!e4102.is_empty(), "E4102 must be emitted");
    for d in &e4102 {
        assert_eq!(
            d.loc.pos, binding_pos,
            "E4102 must point at the binding label `ADC` (pos {binding_pos}), not the \
             component class name; got pos {}: {}",
            d.loc.pos, d.msg
        );
    }
}

/// Regression: the chain span recorded for a module-body reference like
/// `uC.ADC{P,N}` must include the closing `}`. The parser's AST nodes exclude
/// trailing delimiters (the curly node covers `P{N` without `}`), so the
/// recorded span was previously one byte short and the hover/tooltip showed
/// `uC.ADC{P,N` truncated.
#[test]
fn module_body_chain_span_includes_closing_brace() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/chain-span.mc".to_string();
    let source = r#"
component MCU
{
    pins = [
        io [16, 17, 21] = ADC::ADC.DIFF(Receiver)
        io [6, 7] = UART0::UART.TTL(DCE) | UART2::UART.TTL(DTE)
    ]
}

module main
{
    MIC{P,N} -> [C4::CAP(),C5::CAP()] -> uC.ADC{P,N}
    MCU uC
}
"#;

    mcc::mcc_load_from_string(&uri, source);
    let instance = mcc::mcc_build(&McIds::from("main"), &uri).expect("build");

    let chain_pos = source.find("uC.ADC{P,N}").expect("chain present");
    let (span, _segments, _scope) = instance
        .def
        .insts
        .iter_chain_refs()
        .find(|(_, segs, _)| {
            segs.iter().any(|s| {
                matches!(
                    s,
                    mcc::refdef::ChainSegment::Group { base, members }
                        if base == "ADC" && members.as_slice() == ["P", "N"]
                )
            })
        })
        .expect("chain `uC.ADC{P,N}` recorded");
    assert_eq!(
        *span,
        chain_pos..chain_pos + "uC.ADC{P,N}".len(),
        "recorded span must cover the whole `uC.ADC{{P,N}}` including the closing brace"
    );
}
