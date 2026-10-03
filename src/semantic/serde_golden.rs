// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U392 leg A golden: the semantic value face that the library parse cache
//! (lib-parse-cache-design §1) will persist must round-trip through serde
//! without drift. The lock is byte-stability: serialize → deserialize →
//! re-serialize must reproduce the first encoding exactly, for every value
//! face the cache stores. It runs first-green today and guards every future
//! field addition that forgets its serde story.

use crate::ast::macros::*;
use crate::db::infra::mc_code::McCode;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_expr::McExpression;
use crate::semantic::basic::mc_ids::McIds;
use crate::semantic::component::mc_attr::McAttributes;
use crate::semantic::component::mc_pins::McPins;
use crate::semantic::component::McComponent;
use crate::semantic::mc_enum::McEnumDef;
use crate::semantic::mc_ifs::McInterface;

/// Serialize → deserialize → re-serialize; the two encodings must carry the
/// same value. JSON (not bincode) so a failure diff is human-readable, and the
/// comparison is [`serde_json::Value`] equality — map-key order in the JSON
/// text is not part of the value (the pin-span maps are `HashMap`s), while
/// any field or variant that genuinely fails to survive the trip shows up as
/// a value mismatch.
fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned>(v: &T, label: &str) {
    let a = serde_json::to_string(v).unwrap_or_else(|e| panic!("{label}: serialize failed: {e}"));
    let back: T =
        serde_json::from_str(&a).unwrap_or_else(|e| panic!("{label}: deserialize failed: {e}"));
    let b = serde_json::to_string(&back).unwrap_or_else(|e| panic!("{label}: re-serialize: {e}"));
    let va: serde_json::Value =
        serde_json::from_str(&a).expect("golden encoding is valid JSON");
    let vb: serde_json::Value =
        serde_json::from_str(&b).expect("round-trip encoding is valid JSON");
    assert_eq!(va, vb, "{label}: round-trip drift");
}

/// Write the corpus to a temp file (parse_ast loads from disk through the C
/// frontend — a `mem://` URI would fail `mcc_load`) and parse it. The `McCode`
/// must outlive the borrowed AST.
fn parse(label: &str, src: &str) -> McCode {
    let path = std::path::PathBuf::from("/tmp").join(format!(
        "mcc-serde-golden-{}-{label}.mc",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write golden corpus");
    let uri = path.to_string_lossy().to_string();
    let mut code = McCode::new_from_string(&uri, src).expect("mc code from string");
    code.parse_ast();
    code
}

const CORPUS: &str = r#"
component RES.SMD0603
{
    name = "resistor"
    pins = [
        a 1 = PASS
        b 2 = PASS
    ]
}

interface Isolation
{
    pins = [
        1 = IN
        2 = OUT
    ]
}

enum CAP { C0G, X7R }
"#;

#[test]
fn component_roundtrips_without_drift() {
    const LABEL: &str = "res";
    let uri = "mem://serde_golden/res.mc".to_string();
    let code = parse(LABEL, CORPUS);
    let node = code
        .ast
        .iter()
        .find(|n| n.is_type(MCAST_COMPONENT))
        .expect("corpus has a component");
    let comp = McComponent::new(&node, &uri).expect("component parses");
    roundtrip(&comp, "McComponent");
}

#[test]
fn interface_roundtrips_without_drift() {
    const LABEL: &str = "ifs";
    let uri = "mem://serde_golden/ifs.mc".to_string();
    let code = parse(LABEL, CORPUS);
    let node = code
        .ast
        .iter()
        .find(|n| n.is_type(MCAST_INTERFACE))
        .expect("corpus has an interface");
    let ifs = McInterface::new(&node, &uri).expect("interface parses");
    roundtrip(&ifs, "McInterface");
}

#[test]
fn enum_def_roundtrips_without_drift() {
    const LABEL: &str = "enum";
    let uri = "mem://serde_golden/enum.mc".to_string();
    let code = parse(LABEL, CORPUS);
    let node = code
        .ast
        .iter()
        .find(|n| n.is_type(MCAST_ENUM))
        .expect("corpus has an enum");
    let def = McEnumDef::new(&node, &uri).expect("enum parses");
    roundtrip(&def, "McEnumDef");
}

/// Leaf value faces the composite roots are built from — each locks its own
/// encoding so a drift in a leaf is attributed to the leaf, not buried in a
/// whole-component diff.
#[test]
fn leaf_value_faces_roundtrip_without_drift() {
    let ids = McIds::from("RES.SMD0603");
    roundtrip(&ids, "McIds");

    let bus = McBus::new("PRI[1,2]");
    roundtrip(&bus, "McBus");

    let pins = McPins::new();
    roundtrip(&pins, "McPins (empty)");

    let attrs = McAttributes::new();
    roundtrip(&attrs, "McAttributes (empty)");

    roundtrip(
        &McExpression::Int(crate::semantic::basic::mc_literal::McInt { value: 42 }),
        "McExpression::Int",
    );
}
