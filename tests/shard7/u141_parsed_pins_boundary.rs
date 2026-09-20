// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the U141 ruling: the parameterized pin tables (`parsed_pins`, the
//! `if (volt == ...)` branches of an interface body) are attach-face wiring
//! spellings, never a boundary identity source. The module-port boundary
//! spells its member segment from the declared family table; the ordinal is
//! the identity carrier, the per-parameter names vary per instantiation.
//!
//! The counterfactual is real: the b3624 probe round let the boundary helper
//! read `parsed_pins` first and the four real boards split their name-based
//! net merges (hbl1/hs: +1 net, +5 errors) — the per-parameter spelling does
//! not match the spellings the merge machinery keys on.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// A family whose declared table spells B1/B2 and whose 3.3V branch renames
/// ordinal 1 to REN — the same shape as the DC power interface (per-voltage
/// rail names over a stable ordinal pair).
const PARAM_FAMILY: &str = r#"
interface PDC(volt)
{
    pins = [
        1 = B1, "family table"
        2 = B2, "family table"
    ]
    if (volt == 3.3V)
        pins = [
            1 = REN, "parameterized table"
            2 = B2, "parameterized table"
        ]
}

module MA
{
    io P::PDC(3.3V)
}

module MB
{
    io P::PDC(3.3V)
}

module MC
{
    io P::PDC(5.0V)
}
"#;

/// Codes that are build-info, not a verdict (same set the shard7 family
/// tolerates).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
}

/// Build `main` over the parameterized-family fixture; same normalization as
/// the `iface_connect_rule` builders: sorted list of sorted boundary-point
/// paths per net.
fn pplbl_build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{PARAM_FAMILY}module main {{\n    \
         MA xa\n    MB xb\n    MC mc\n{body}\n}}\n"
    );
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();
    let mut partition: Vec<Vec<String>> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(_, pts)| {
                    let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
                    ps.sort();
                    ps
                })
                .filter(|ps| !ps.is_empty())
                .collect()
        })
        .unwrap_or_default();
    partition.sort();
    (codes, partition)
}

/// Both sides carry the 3.3V branch (ordinal 1 renamed to REN), yet the
/// boundary spells the declared family table on both ends: B1/B2. The
/// parsed-pins counterfactual is any `REN` path — a boundary spelled from the
/// parameterized table.
#[test]
fn u141__boundary_labels_ignore_the_parameterized_pin_tables() {
    let (codes, nets) = pplbl_build("    xa.P -> xb.P", "/mcc/u141-pplbl-same-param.mc");
    assert!(
        codes.is_empty(),
        "same family, no roles: quiet; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["xa.P.B1".to_string(), "xb.P.B1".to_string()],
            vec!["xa.P.B2".to_string(), "xb.P.B2".to_string()],
        ],
        "the boundary spells the declared table, never the branch names; the \
         parsed-pins counterfactual is any xa.P.REN / xb.P.REN path; got {nets:?}"
    );
}

/// Different parameters across one wire: the ordinal pairing is unchanged and
/// the labels stay on the declared table — a per-parameter relabel of the
/// boundary would spell the two ends differently and split the merge.
#[test]
fn u141__cross_parameter_wire_keeps_declared_labels_and_ordinal_pairing() {
    let (codes, nets) = pplbl_build("    xa.P -> mc.P", "/mcc/u141-pplbl-cross-param.mc");
    assert!(
        codes.is_empty(),
        "same family, no roles: quiet; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["mc.P.B1".to_string(), "xa.P.B1".to_string()],
            vec!["mc.P.B2".to_string(), "xa.P.B2".to_string()],
        ],
        "ordinal k meets ordinal k across parameters; labels stay on the \
         declared table; got {nets:?}"
    );
}
