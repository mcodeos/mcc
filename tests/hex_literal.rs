// Copyright (c) 2026 MCode
//
// Integration test: hex literal attributes (MCAST_HEX) must be parsed and
// preserved instead of being silently dropped (P1-4).
//
// Regression: `McLiteral::new` routed MCAST_HEX through `McInt::new`, which
// only handles MCAST_INT/MCAST_UNIT_INT and returns None — so `key = 0xFF`
// attributes vanished without any diagnostic.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

/// Global mutex to serialize tests that share mcc's global workspace state.
static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const SOURCE: &str = r#"
component HEXT (pin = P1)
{
    key = 0xFF
    mask = 0x0F
    pins = [
        1 = pin
    ]
}

module main
{
    io VDD
}
"#;

/// Load `SOURCE`, build `main`, and return the stringified `key`/`mask` attrs.
fn load_attrs() -> Vec<(String, String)> {
    let lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();

    let uri: McURI = "/mcc/hex-literal.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");

    let cmie = mcc::get_def(&McIds::from("HEXT"), &uri).expect("HEXT definition not found");
    drop(lock);

    let mcc::McCMIE::Component(comp) = cmie else {
        panic!("HEXT is not a Component");
    };
    comp.attrs
        .iter()
        .map(|a| {
            let values = a
                .values
                .iter()
                .map(|v| format!("{v}"))
                .collect::<Vec<_>>()
                .join(",");
            (a.id.to_string(), values)
        })
        .collect()
}

#[test]
fn hex_attributes_are_preserved() {
    let attrs = load_attrs();

    let key = attrs
        .iter()
        .find(|(name, _)| name == "key")
        .unwrap_or_else(|| panic!("attr 'key' missing; got {attrs:?}"));
    assert_eq!(key.1, "0xFF", "hex value for 'key' was dropped or mangled");

    let mask = attrs
        .iter()
        .find(|(name, _)| name == "mask")
        .unwrap_or_else(|| panic!("attr 'mask' missing; got {attrs:?}"));
    assert_eq!(
        mask.1, "0x0F",
        "hex value for 'mask' was dropped or mangled"
    );
}
