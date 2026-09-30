// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ U366 · the spelled-out NC list `[NC, NC]` is a legal pins-row RHS.
//!
//! Why this test exists
//! `nc [5, 6] = NC` (single RHS) was always legal, but the spelled-out
//! mirror `nc [5, 6] = [NC, NC]` died E2083 at the `[`: NC is reachable
//! from `mc_pins_name` and `mc_param` only, never from `mc_phrase`, so the
//! bracket vector had no derivation. Worse, the E2083 recovery was not
//! row-local — the resync popped out of the pins block and every following
//! row died E2082, losing the whole foot table (adc.mc, 2026-10-01; user
//! ruling: this is a bug, the form should be allowed).
//!
//! The fix: the grammar gains an `mc_pins_name` arm —
//! `'[' mc_nc_list ']'` landing `MCAST_PIN_NAME(MCAST_OPD_SQUARE_VEC)`,
//! the same node shape the reader unfolds for the `[VREF, AGND]::face`
//! convention vector — and the pins reader accepts that vector as one
//! group-NC option. NC is position-free, so the reading equals the single
//! form exactly: every pinid on the row's id side registers NC, and the
//! rows after it survive untouched.
//!
//! What is locked (do not weaken)
//! 1. the list form and the single form produce IDENTICAL def-face pin
//!    tables (ids, names, directions) — the list is a spelling, not a
//!     different construction;
//! 2. no E2083 / E2082 / E3004 on either form — the row-local failure
//!    chain stays impossible;
//! 3. rows after the NC row keep registering (the recovery lock).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McCMIE, McIds, McURI};

const FIXTURE: &str = r#"
component ADC8
{
    pins = [
        1 = A
        NCROW
        7 = C
    ]
}
module main(psnk [P, G])
{
    ADC8 u1
}
"#;

fn src_of(nc_row: &str) -> String {
    FIXTURE.replace("NCROW", nc_row)
}

/// The def-face pin table of `ADC8`: sorted `(pid, names, iotype-spelled)`
/// rows, the same face `mcc show pins` renders.
fn pin_table(src: &str, uri: &str) -> Vec<(String, Vec<String>, String)> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    match mcc::get_kind_def(0, &McIds::from("ADC8"), &u) {
        Some(McCMIE::Component(c)) => {
            let mut rows: Vec<(String, Vec<String>, String)> = c
                .pins
                .pins
                .iter()
                .map(|(pid, pin)| {
                    let mut names = pin.names.clone();
                    names.sort();
                    (pid.clone(), names, format!("{:?}", pin.iotype))
                })
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            rows
        }
        None => panic!("component def 'ADC8' not found"),
        Some(_) => panic!("'ADC8' resolved to a non-component kind"),
    }
}

fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_with_nets(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// ① The list form reads exactly like the single form: same pin table,
/// same diagnostic face. Pre-fix the list form died E2083, dragged every
/// following row down with E2082, and the component lost its foot table.
#[test]
fn u366_nc_list__list_form_matches_single_form() {
    let list = pin_table(
        &src_of("nc [5, 6] = [NC, NC]"),
        "/mcc/u366-nc-list.mc",
    );
    let single = pin_table(&src_of("nc [5, 6] = NC"), "/mcc/u366-nc-single.mc");

    assert_eq!(
        list, single,
        "the spelled-out list is a spelling, not a different construction"
    );

    // The table itself: pins 5 and 6 registered, named NC, and the row
    // after the NC row survives (pid 7 with its name).
    let pid = |rows: &[(String, Vec<String>, String)], id: &str| -> (String, Vec<String>, String) {
        rows.iter()
            .find(|(p, _, _)| p == id)
            .cloned()
            .unwrap_or_else(|| panic!("pin {id} missing from the table: {rows:?}"))
    };
    for id in ["5", "6"] {
        let (_, names, _) = pid(&list, id);
        assert_eq!(names, vec!["NC".to_string()], "pin {id} must read NC");
    }
    let (_, names, _) = pid(&list, "7");
    assert_eq!(
        names,
        vec!["C".to_string()],
        "the row after the NC row must survive (recovery lock)"
    );
}

/// ② The failure chain stays impossible: no E2083 (no derivation), no
/// E2082 (recovery chain), no E3004 (unread name shape) on either form.
#[test]
fn u366_nc_list__no_failure_chain_on_either_form() {
    for (src, uri) in [
        (src_of("nc [5, 6] = [NC, NC]"), "/mcc/u366-codes-list.mc"),
        (src_of("nc [5, 6] = NC"), "/mcc/u366-codes-single.mc"),
    ] {
        let codes = codes_of(&src, uri);
        for bad in [2083, 2082, 3004] {
            assert!(
                !codes.contains(&bad),
                "E{bad} must not fire on the NC row; got {codes:?}"
            );
        }
    }
}

/// ③ One element covers the group, and extra spellings do not change the
/// reading — NC is position-free, so `[NC]` and `[NC, NC, NC]` land the
/// same table as `[NC, NC]`.
#[test]
fn u366_nc_list__element_count_does_not_change_the_reading() {
    let one = pin_table(&src_of("nc [5, 6] = [NC]"), "/mcc/u366-nc-one.mc");
    let three = pin_table(&src_of("nc [5, 6] = [NC, NC, NC]"), "/mcc/u366-nc-three.mc");
    let two = pin_table(&src_of("nc [5, 6] = [NC, NC]"), "/mcc/u366-nc-two.mc");
    assert_eq!(one, two, "one NC spelling covers the group");
    assert_eq!(three, two, "extra NC spellings change nothing");
}
