// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Vector member access (`c[1:2].1`) locks: the dot suffix rides on the
//! segment tree as a trailing `DotInt`/`DotIda` (`McIds::new`, MCAST_IDS), so
//! `McPhrase::new` splits it structurally (`split_vector_member`) instead of
//! re-parsing the display text. A `Member(List, member)` phrase expands
//! per-lane in Pass2: `c[1:2].1 - c[1:2].2` pairs c1.1~c1.2 and c2.1~c2.2,
//! never cross-paired.

use mcc::{McIds, McURI};
use std::sync::{Mutex, OnceLock};

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

/// Build `main`, return the diagnostic codes (sorted).
fn codes(src: &str, uri: &str) -> Vec<u32> {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &u);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v
}

/// Build `main` with nets, returning the frozen net-table store.
fn build_net_store(src: &str, uri: &str) -> Vec<(String, Vec<String>)> {
    let _lock = TEST_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_clear_workspace();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(n, pts)| (n.clone(), pts.iter().map(|p| p.path.clone()).collect()))
                .collect()
        })
        .unwrap_or_default()
}

/// The net that holds `path`, if any.
fn net_holding<'a>(nets: &'a [(String, Vec<String>)], path: &str) -> Option<&'a Vec<String>> {
    nets.iter()
        .find(|(_, ps)| ps.iter().any(|p| p == path))
        .map(|(_, ps)| ps)
}

/// `c[1:2].1 - c[1:2].2` — member access on both ends. The access is quiet
/// (no width errors) and the per-lane pairing is c1.1~c1.2 / c2.1~c2.2 with
/// no cross-pairing, proving the split kept the shared member per lane.
#[test]
fn vector_member_access_pairs_per_lane() {
    let src = format!(
        "{CAP_COMP}module main {{\n    io VDD\n    io GND\n    CAP c[1:2](1)\n    c[1:2].1 - c[1:2].2\n}}\n"
    );
    let codes = codes(&src, "/mcc/vec-member-pair.mc");
    let rest: Vec<u32> = codes
        .iter()
        .copied()
        .filter(|c| !matches!(c, 5641 | 5642 | 5054))
        .collect();
    assert_eq!(
        rest,
        Vec::<u32>::new(),
        "member access is quiet; got {codes:?}"
    );

    let nets = build_net_store(&src, "/mcc/vec-member-pair.mc");
    let n11 = net_holding(&nets, "c1.1").expect("net carrying c1.1");
    assert!(
        n11.iter().any(|p| p == "c1.2"),
        "c1.1 net also carries c1.2; got {n11:?}"
    );
    assert!(
        !n11.iter().any(|p| p.starts_with("c2.")),
        "no cross-pairing on the c1.1 net; got {n11:?}"
    );
    let n21 = net_holding(&nets, "c2.1").expect("net carrying c2.1");
    assert!(
        n21.iter().any(|p| p == "c2.2"),
        "c2.1 net also carries c2.2; got {n21:?}"
    );
    assert!(
        !n21.iter().any(|p| p.starts_with("c1.")),
        "no cross-pairing on the c2.1 net; got {n21:?}"
    );
}
