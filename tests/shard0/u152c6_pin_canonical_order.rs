// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U152 ⑥: the flat table's per-component pin registration follows the
//! canonical pin order (`pin_id_cmp`), not dictionary order.
//!
//! `flatten_module` sorts each component's pin keys "for stable order" — but
//! the sort was plain lexicographic, so with ≥10 numeric pins the registration
//! (and with it the entry-id allocation order, since `entries` is a BTreeMap
//! keyed by the allocation id) ran `1, 10, 11, 12, 2, …`. `pin_id_cmp` is the
//! one canonical definition every pin listing uses, so the registration now
//! reads `1, 2, … 12` and places non-numeric ids naturally (`A9 < A10`,
//! where dictionary order runs `A10 < A9`).

#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

const SRC: &str = r#"
component P
{
    name = "P"
    pins = [
        in 2 = P02
        in 11 = P11
        in 1 = P01
        in 10 = P10
        in 3 = P03
        in 12 = P12
        in 4 = P04
        in 9 = P09
        in 5 = P05
        in 8 = P08
        in 6 = P06
        in 7 = P07
        in A10 = PA10
        in A9 = PA9
    ]
}

module main {
    P u
}
"#;

fn pin_keys_of_u() -> Vec<String> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/u152c6-pin-order.mc".to_string();
    mcc::mcc_load_from_string(&uri, SRC);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let comp = table
        .get_components()
        .into_iter()
        .find(|e| e.path == "main.u")
        .expect("component P instantiated as main.u");
    table
        .get_pins_of(comp.id)
        .iter()
        .map(|e| {
            e.path
                .strip_prefix("main.u.")
                .expect("pin path under main.u")
                .to_string()
        })
        .collect()
}

/// 12 numeric lanes (declared scrambled, so the written order cannot be what
/// the lock observes by accident) plus the natural-order pair A9/A10. Pre-fix
/// the key sequence ran `1, 10, 11, 12, 2, …, 9, A10, A9` on both counts.
#[test]
fn lock_u152c6__pin_registration_follows_pin_id_cmp() {
    let got = pin_keys_of_u();
    let expect: Vec<String> = (1..=12)
        .map(|i| i.to_string())
        .chain(["A9".to_string(), "A10".to_string()])
        .collect();
    assert_eq!(
        got, expect,
        "pin registration must follow the canonical pin order (pin_id_cmp)"
    );
}
