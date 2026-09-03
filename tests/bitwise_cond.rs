// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: bitwise `&` / `|` conditions in `if` blocks must be
// parsed and evaluated instead of being silently dropped (P0-2).
//
// Regression: `parse_cond_if` / `parse_cond_else_with_cond` whitelisted only
// the comparison judge nodes (==, !=, <, >, <=, >=, in) and missed
// MCAST_JUDGE_BITAND / MCAST_JUDGE_BITOR — so `if (address & 0x01) ... else ...`
// (e.g. mcd/mclibs/others/pca9555.mc) lost the whole branch without a
// diagnostic.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McCondOperand, McCondition, McConds, McIds, McURI};

const SOURCE: &str = r#"
component PCA9555T (partno)
{
    pins = [
        io [1] = A0
    ]

    func Address(address)
    {
        if (address & 0x01) A0 -> VDD else A0 -> GND
    }
}

module main
{
    io VDD
}
"#;

#[test]
fn sem_bitcond__and_parsed_not_dropped() {
    let _lock = common::lock();
    common::reset();

    let uri: McURI = "/mcc/bitwise-cond.mc".to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);
    let result = mcc::mcc_build(&McIds::from("main"), &uri);
    result.expect("build failed");

    let cmie = mcc::get_def(&McIds::from("PCA9555T"), &uri).expect("component not found");
    let mcc::McCMIE::Component(comp) = cmie else {
        panic!("PCA9555T is not a Component");
    };
    let addr = comp
        .funcs
        .find("Address")
        .unwrap_or_else(|| panic!("func 'Address' missing"));
    assert!(
        !addr.conds.is_empty(),
        "bitwise if/else was silently dropped (P0-2)"
    );

    let cond = &addr.conds[0].if_blocks[0].condition;
    match cond {
        McCondition::BitAnd { .. } => {}
        other => panic!("expected BitAnd condition, got {other:?}"),
    }
}

#[test]
fn sem_bitcond__eval_uses_nonzero_result() {
    // `0x36 & 0x01 == 0` -> false; `0x37 & 0x01 != 0` -> true
    let left = McCondOperand::Ident(McIds::from("address"));

    let cond = McCondition::BitAnd {
        left: left.clone(),
        right: McCondOperand::Literal("0x01".to_string()),
    };
    let params_even = vec![(McIds::from("address"), "0x36".to_string())];
    let params_odd = vec![(McIds::from("address"), "0x37".to_string())];
    assert!(!McConds::check_condition(&cond, &params_even));
    assert!(McConds::check_condition(&cond, &params_odd));

    // Decimal operands work too
    let cond_dec = McCondition::BitAnd {
        left,
        right: McCondOperand::Literal("1".to_string()),
    };
    let params_4 = vec![(McIds::from("address"), "4".to_string())];
    let params_5 = vec![(McIds::from("address"), "5".to_string())];
    assert!(!McConds::check_condition(&cond_dec, &params_4));
    assert!(McConds::check_condition(&cond_dec, &params_5));

    // BitOr: true when the result is non-zero (`x | 0 = x`)
    let cond_or = McCondition::BitOr {
        left: McCondOperand::Ident(McIds::from("address")),
        right: McCondOperand::Literal("0x00".to_string()),
    };
    let params_0 = vec![(McIds::from("address"), "0x00".to_string())];
    let params_1 = vec![(McIds::from("address"), "0x01".to_string())];
    assert!(!McConds::check_condition(&cond_or, &params_0));
    assert!(McConds::check_condition(&cond_or, &params_1));
}
