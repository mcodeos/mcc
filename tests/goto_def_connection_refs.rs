// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! LSP goto-def regression for connection-expression refs inside funcall
//! brackets and for dotted member chains:
//!
//! - `CAP(100nF).Cap([dc.VDD_3V3 -> wm7121.VCC], dc.GND)` must register the
//!   arrow's right operand `wm7121.VCC` as a PinNameRef (mapping to pin
//!   `4 = VCC`) and the left operand `dc.VDD_3V3` as a BusMemberRef (mapping to
//!   the curly param-bus member declaration), with no stray interval covering
//!   the whole `->` expression.
//! - `lpa.IN.N` (dotted pin name) must resolve by longest match to pin
//!   `4 = IN.N`.
//!
//! NOTE: These tests share global mcc state, so a mutex serializes them.

use mcc::McURI;
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const SOURCE: &str = r#"
interface DC(volt)
{
    pins = [
        1 = VOUT, "DC power positive"
        2 = GND, "DC power ground"
    ]
}

component CAP(cap)
{
    pins = [
        1 = 1, "terminal one"
        2 = 2, "terminal two"
    ]
    func Cap(net1, net2)
    {
        net1 - this - net2
        return net1, net2
    }
}

component MICROPHONE.WM7121P
{
    pins = [
        1 = P, "MIC signal positive"
        [2,3] = GND, "ground"
        4 = VCC, "power input"
    ]
}

component LPA4871
{
    pins = [
        3 = IN.P, "input positive"
        4 = IN.N, "input negative"
        5 = VO1, "output one"
        8 = VO2, "output two"
    ]
}

component SPEAKER.PHB2AWB
{
    pins = [
        1 = P, "signal positive"
        2 = N, "signal negative"
        3 = GND, "ground"
        4 = GND, "ground"
    ]
}

component DIO.ESD
{
    pins = [
        1 = A, "anode"
        2 = K, "cathode"
    ]
}

module main(dc{VDD_3V3, GND}::DC(3.3V))
{
    MICROPHONE.WM7121P wm7121(NC)
    CAP(100nF).Cap([dc.VDD_3V3 -> wm7121.VCC], dc.GND)
    LPA4871 lpa
    SPEAKER.PHB2AWB spk
    lpa.IN.N -> dc.GND
    (spk.3 + spk.4) -> dc.GND
    spk.P -> DIO.ESD("ESD9B5V-2/TR", NC) -> dc.GND
}
"#;

/// Parse `span=[  123,  456]` from a F12_DIAG dump line.
fn extract_span(line: &str) -> Option<(usize, usize)> {
    let s = line.find("span=[")?;
    let rest = &line[s + 6..];
    let comma = rest.find(',')?;
    let close = rest.find(']')?;
    let a: usize = rest[..comma].trim().parse().ok()?;
    let b: usize = rest[comma + 1..close].trim().parse().ok()?;
    Some((a, b))
}

/// LAPPER_REF interval (kind tag, ref id, span) for the dump line whose ref
/// span equals `span` (the dump shows `name='?'` for string-loaded files, so
/// refs are matched by source span instead of by name).
fn ref_interval(dump: &str, kind: &str, span: (usize, usize)) -> Option<u32> {
    dump.lines()
        .filter(|l| l.contains("F12_DIAG LAPPER_REF:"))
        .filter(|l| l.contains(&format!("kind={kind}")))
        .filter(|l| extract_span(l) == Some(span))
        .filter_map(|l| {
            l.find("id=")
                .and_then(|i| l[i + 3..].split_whitespace().next())
                .and_then(|s| s.parse().ok())
        })
        .next()
}

/// Def span from the MAP line `Ref(<kind>/<ku>, id=<ref_id>, ...) => Def(<def_kind>/<ku>, span=[a,b], ...)`.
fn map_def_span(dump: &str, kind: &str, ref_id: u32) -> Option<(usize, usize)> {
    dump.lines()
        .filter(|l| l.contains("F12_DIAG MAP:"))
        .filter(|l| {
            l.contains(&format!("Ref({kind}/")) && l.contains(&format!("id={ref_id:5}"))
        })
        .filter_map(|l| {
            let idx = l.find("=> Def")?;
            extract_span(&l[idx..])
        })
        .next()
}

/// Loads SOURCE into a fresh workspace and returns the F12 dump for the uri.
fn load_and_dump() -> String {
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/goto-def-connection-refs.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);
    mcc::dump_symbols_f12_text(&uri).expect("f12 dump")
}

#[test]
fn funcall_bracket_arrow_registers_pin_and_bus_member_refs() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner());
    let dump = load_and_dump();

    // 1. `wm7121.VCC` (arrow right operand inside `[...]`) is a PinNameRef
    //    covering the chain span and maps to the pin def `4 = VCC`.
    let chain_span = {
        let p = SOURCE.find("wm7121.VCC").expect("wm7121.VCC in source");
        (p, p + "wm7121.VCC".len())
    };
    let vcc_def = SOURCE.find("4 = VCC").map(|p| p + 4).expect("4 = VCC in source");
    let ref_id = ref_interval(&dump, "PinNameRef", chain_span)
        .expect("PinNameRef interval for wm7121.VCC");
    assert_eq!(
        map_def_span(&dump, "PinNameRef", ref_id),
        Some((vcc_def, vcc_def + 3)),
        "wm7121.VCC must map to PinNameDef VCC at {vcc_def}..{}",
        vcc_def + 3
    );

    // 2. `dc.VDD_3V3` (arrow left operand) is a BusMemberRef covering the chain
    //    span and maps to the curly param-bus member `VDD_3V3` declaration.
    let chain_span = {
        let p = SOURCE.find("dc.VDD_3V3").expect("dc.VDD_3V3 in source");
        (p, p + "dc.VDD_3V3".len())
    };
    let vdd_def = SOURCE.find("VDD_3V3").expect("VDD_3V3 declaration in source");
    let ref_id = ref_interval(&dump, "BusMemberRef", chain_span)
        .expect("BusMemberRef interval for dc.VDD_3V3");
    assert_eq!(
        map_def_span(&dump, "BusMemberRef", ref_id),
        Some((vdd_def, vdd_def + 7)),
        "dc.VDD_3V3 must map to BusMemberDef at {vdd_def}..{}",
        vdd_def + 7
    );

    // 3. `dc.GND` (second funcall arg) maps to the `GND` member of the same bus.
    let chain_span = {
        let p = SOURCE.find("dc.GND").expect("dc.GND in source");
        (p, p + "dc.GND".len())
    };
    let gnd_def = SOURCE
        .find("VDD_3V3, GND")
        .map(|p| p + "VDD_3V3, ".len())
        .expect("GND member in curly bus");
    let ref_id = ref_interval(&dump, "BusMemberRef", chain_span)
        .expect("BusMemberRef interval for dc.GND");
    assert_eq!(
        map_def_span(&dump, "BusMemberRef", ref_id),
        Some((gnd_def, gnd_def + 3)),
        "dc.GND must map to BusMemberDef at {gnd_def}..{}",
        gnd_def + 3
    );

    // 4. No ref interval may cover the whole `->` expression: the old flat name
    //    lookup registered one interval spanning `dc.VDD_3V3 -> wm7121.VCC`.
    let whole = SOURCE
        .find("dc.VDD_3V3 -> wm7121.VCC")
        .expect("arrow expression in source");
    let stray = (whole, whole + "dc.VDD_3V3 -> wm7121.VCC".len());
    assert!(
        !dump.lines().any(|l| {
            l.contains("F12_DIAG LAPPER_REF:") && extract_span(l) == Some(stray)
        }),
        "no ref interval may span the whole 'dc.VDD_3V3 -> wm7121.VCC' expression at {stray:?}"
    );
}

#[test]
fn numeric_pin_id_resolves_to_pin_id_def() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner());
    let dump = load_and_dump();

    // `spk.3` / `spk.4` are pin-ID refs: the chain resolver must land on the
    // PinIdDef for `3` / `4` in `pins = [ ... 3 = GND ... ]`, not a LabelDef.
    let chain_span = {
        let p = SOURCE.find("spk.3").expect("spk.3 in source");
        (p, p + "spk.3".len())
    };
    let id3_def = SOURCE.find("3 = GND").expect("pin id 3 def in source");
    let ref_id = ref_interval(&dump, "PinIdRef", chain_span)
        .expect("PinIdRef interval for spk.3");
    assert_eq!(
        map_def_span(&dump, "PinIdRef", ref_id),
        Some((id3_def, id3_def + 1)),
        "spk.3 must map to PinIdDef at {id3_def}..{}",
        id3_def + 1
    );

    let chain_span = {
        let p = SOURCE.find("spk.4").expect("spk.4 in source");
        (p, p + "spk.4".len())
    };
    let id4_def = SOURCE.find("4 = GND").expect("pin id 4 def in source");
    let ref_id = ref_interval(&dump, "PinIdRef", chain_span)
        .expect("PinIdRef interval for spk.4");
    assert_eq!(
        map_def_span(&dump, "PinIdRef", ref_id),
        Some((id4_def, id4_def + 1)),
        "spk.4 must map to PinIdDef at {id4_def}..{}",
        id4_def + 1
    );
}

#[test]
fn dotted_pin_name_resolves_by_longest_match() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner());
    let dump = load_and_dump();

    // `lpa.IN.N` must resolve to the literal pin name `IN.N` (longest match),
    // not stop at `IN` or `N`.
    let chain_span = {
        let p = SOURCE.find("lpa.IN.N").expect("lpa.IN.N in source");
        (p, p + "lpa.IN.N".len())
    };
    let in_n_def = SOURCE.find("4 = IN.N").map(|p| p + 4).expect("4 = IN.N in source");
    let ref_id = ref_interval(&dump, "PinNameRef", chain_span)
        .expect("PinNameRef interval for lpa.IN.N");
    assert_eq!(
        map_def_span(&dump, "PinNameRef", ref_id),
        Some((in_n_def, in_n_def + 4)),
        "lpa.IN.N must map to PinNameDef IN.N at {in_n_def}..{}",
        in_n_def + 4
    );
}

#[test]
fn dotted_chain_base_resolves_to_param_decl() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner());
    let dump = load_and_dump();

    // The base `dc` of `(spk.3 + spk.4) -> dc.GND` must resolve to the module
    // param declaration `dc{VDD_3V3, GND}` (PortRef → ParamDef), not to the
    // whole-chain member `GND` and not to the chain base's own text.
    let arrow = SOURCE.find("-> dc.GND").expect("arrow dc.GND in source");
    let dc_off = arrow + "-> ".len();
    let base_span = (dc_off, dc_off + "dc".len());
    let param_decl = SOURCE.find("dc{VDD_3V3, GND}").expect("param decl in source");
    let ref_id = ref_interval(&dump, "PortRef", base_span)
        .expect("PortRef interval for chain base dc");
    assert_eq!(
        map_def_span(&dump, "PortRef", ref_id),
        Some((param_decl, param_decl + "dc{VDD_3V3, GND}".len())),
        "chain base dc must map to ParamDef at {param_decl}..{}",
        param_decl + "dc{VDD_3V3, GND}".len()
    );

    // The instance base `spk` in `spk.3` stays an InstRef → InstDef (not a
    // PortRef), so numeric pin members keep resolving to pin-id defs.
    let chain_span = {
        let p = SOURCE.find("spk.3").expect("spk.3 in source");
        (p, p + "spk.3".len())
    };
    let spk_def = SOURCE.find("SPEAKER.PHB2AWB spk").map(|p| p + "SPEAKER.PHB2AWB ".len())
        .expect("spk instance in source");
    let base_ref_id = ref_interval(&dump, "InstRef", (chain_span.0, chain_span.0 + 3))
        .expect("InstRef interval for chain base spk");
    assert_eq!(
        map_def_span(&dump, "InstRef", base_ref_id),
        Some((spk_def, spk_def + 3)),
        "chain base spk must map to InstDef at {spk_def}..{}",
        spk_def + 3
    );
}

#[test]
fn dotted_funcall_class_ref_spans_full_class_name() {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap_or_else(|e| e.into_inner());
    let dump = load_and_dump();

    // `DIO.ESD("ESD9B5V-2/TR", NC)` is a dotted class funcall. The ClassRef
    // interval must cover the whole `DIO.ESD` text (not `ESD(` + the string
    // argument) and map to the `component DIO.ESD` declaration.
    let p = SOURCE.find("DIO.ESD(").expect("DIO.ESD( in source");
    let full_span = (p, p + "DIO.ESD".len());
    let ref_id = ref_interval(&dump, "ClassRef", full_span)
        .expect("ClassRef interval for DIO.ESD");
    let dio_def = SOURCE
        .find("component DIO.ESD")
        .map(|q| q + "component ".len())
        .expect("DIO.ESD component def in source");
    assert_eq!(
        map_def_span(&dump, "ClassRef", ref_id),
        Some((dio_def, dio_def + "DIO.ESD".len())),
        "DIO.ESD must map to ClassDef at {dio_def}..{}",
        dio_def + "DIO.ESD".len()
    );

    // Sanity: no ClassRef interval may bleed into the string argument
    // (the pre-fix span started at `ESD(` and covered `ESD("ES`).
    let esd_start = p + "DIO.".len();
    let broken = (esd_start, esd_start + "ESD(\"ES".len());
    assert!(
        !dump.lines().any(|l| {
            l.contains("F12_DIAG LAPPER_REF:") && l.contains("kind=ClassRef")
                && extract_span(l) == Some(broken)
        }),
        "no ClassRef interval may span 'ESD(\"ES' at {broken:?}"
    );
}

