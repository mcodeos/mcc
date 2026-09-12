// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! THROWAWAY PROBE — delete before commit.
//!
//! Prefix-sugar replication face: which spellings bind per lane today?

use mcc::{McIds, McURI};

mod common;

const RES: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n        return [n1, n2]\n    }\n}\n";

const RES_SCALAR: &str = "component RESS(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n        return [n1, n2]\n    }\n}\n";

const HEAD: &str = "module main {\n    io I2C0{SCL, SDA}\n    io VCC\n    io NET\n    func M() {\n";

fn probe(label: &str, comp: &str, body: &str) {
    let _lock = common::lock();
    common::reset();
    let src = format!("{comp}{HEAD}{body}\n    }}\n}}\n");
    let uri = format!("/mcc/zz-pfx-{label}.mc");
    let u = McURI::from(uri.as_str());
    mcc::mcc_load_from_string(&u, &src);
    let (_, _, _, store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut nets: Vec<String> = Vec::new();
    for (net, pts) in store.get("main").unwrap_or(&[]).iter() {
        let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
        ps.sort();
        nets.push(format!("{net} -> {ps:?}"));
    }
    nets.sort();
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes.dedup();
    println!("[{label}] nets={nets:?}");
    println!("[{label}] codes={codes:?}");
}

#[test]
fn probe_prefix_faces() {
    probe("a_prefix_bare_ps", RES_SCALAR, "        I2C0 => RESS(10).Pullup(_, VCC)");
    probe("b_prefix_set_ps", RES, "        I2C0 => RES(10).Pullup([_, VCC])");
    probe("c_group_prefix", RES_SCALAR, "        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)");
    probe("d_group_of_insts", RES, "        (res1::RES(10), res2::RES(10)) -> [NET, VCC]");
    probe("e_array_in_set", RES, "        I2C0 => RES(10).Pullup([res[1:2], VCC])");
}

const RES_SET_FORMAL: &str = "component RESX(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n        return [n1, n2]\n    }\n}\n";

const RES_SCALAR_FORMAL: &str = "component RESX(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n        return [n1, n2]\n    }\n}\n";

#[test]
fn probe_anon_name_stem() {
    probe("f_set_formal", RES_SET_FORMAL, "        I2C0 => RESX(10).Pullup(_, VCC)");
    probe("g_scalar_formal", RES_SCALAR_FORMAL, "        I2C0 => RESX(10).Pullup(_, VCC)");
    probe("h_array_set_formal", RES_SET_FORMAL, "        I2C0 -> res[1:2]::RESX(10)");
}
