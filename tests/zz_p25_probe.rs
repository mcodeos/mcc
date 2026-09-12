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
    probe(
        "a_prefix_bare_ps",
        RES_SCALAR,
        "        I2C0 => RESS(10).Pullup(_, VCC)",
    );
    probe(
        "b_prefix_set_ps",
        RES,
        "        I2C0 => RES(10).Pullup([_, VCC])",
    );
    probe(
        "c_group_prefix",
        RES_SCALAR,
        "        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, VCC)",
    );
    probe(
        "d_group_of_insts",
        RES,
        "        (res1::RES(10), res2::RES(10)) -> [NET, VCC]",
    );
    probe(
        "e_array_in_set",
        RES,
        "        I2C0 => RES(10).Pullup([res[1:2], VCC])",
    );
}

const RES_SET_FORMAL: &str = "component RESX(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n        return [n1, n2]\n    }\n}\n";

const RES_SCALAR_FORMAL: &str = "component RESX(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n        return [n1, n2]\n    }\n}\n";

#[test]
fn probe_anon_name_stem() {
    probe(
        "f_set_formal",
        RES_SET_FORMAL,
        "        I2C0 => RESX(10).Pullup(_, VCC)",
    );
    probe(
        "g_scalar_formal",
        RES_SCALAR_FORMAL,
        "        I2C0 => RESX(10).Pullup(_, VCC)",
    );
    probe(
        "h_array_set_formal",
        RES_SET_FORMAL,
        "        I2C0 -> res[1:2]::RESX(10)",
    );
}

#[test]
fn probe_sugar_return_and_group_prefix() {
    probe(
        "i_sugar_then_chain",
        RES_SCALAR,
        "        I2C0 => RESS(10).Pullup(_, VCC) -> [NET, NET]",
    );
    probe(
        "j_array_then_chain",
        RES_SCALAR,
        "        I2C0 -> [res[1:2]::RESS(10)] -> [VCC, VCC] -> [NET, NET]",
    );
    probe(
        "k_group_prefix_want",
        RES_SCALAR_FORMAL,
        "        (I2C0.SCL, I2C0.SDA) => RESX(10).Pullup(_, _)",
    );
    probe(
        "l_scalar_prefix",
        RES_SCALAR,
        "        VCC => RESS(10).Pullup(_, NET)",
    );
    probe(
        "m_group_prefix_two_slots",
        RES_SCALAR,
        "        (I2C0.SCL, I2C0.SDA) => RESS(10).Pullup(_, _)",
    );
    probe(
        "n_group_prefix_arity_mismatch",
        RES_SCALAR,
        "        (I2C0.SCL, I2C0.SDA, VCC) => RESS(10).Pullup(_, _)",
    );
    probe(
        "o_paren_single",
        RES_SCALAR,
        "        (VCC) => RESS(10).Pullup(_, NET)",
    );
}

#[test]
fn probe_group_of_instances() {
    probe("p1_ctor_left", RES, "        res1::RES(10) -> [NET, VCC]");
    probe(
        "p2_ctor_left_dash",
        RES,
        "        res1::RES(10) - [NET, VCC]",
    );
    probe(
        "p3_group_of_insts",
        RES,
        "        (res1::RES(10), res2::RES(10)) -> [NET, VCC]",
    );
    probe(
        "p4_group_same_net",
        RES,
        "        (res1::RES(10), res2::RES(10)) -> [NET, NET]",
    );
    probe(
        "p5_group_of_nets",
        RES,
        "        (NET, VCC) -> res1::RES(10)",
    );
}

#[test]
fn probe_declared_members() {
    probe(
        "q1_named_member_list",
        RES,
        "        res[1:2]::RES(10)\n        res1 -> [NET, VCC]",
    );
    probe(
        "q2_group_of_members",
        RES,
        "        res[1:2]::RES(10)\n        (res1, res2) -> [NET, VCC]",
    );
    probe(
        "q3_whole_array",
        RES,
        "        res[1:2]::RES(10)\n        res[1:2] -> [NET, VCC]",
    );
    probe(
        "q4_group_same_net",
        RES,
        "        res[1:2]::RES(10)\n        (res1, res2) -> [NET, NET]",
    );
    probe(
        "q5_group_nets_left",
        RES,
        "        res[1:2]::RES(10)\n        (NET, VCC) -> (res1, res2)",
    );
    probe(
        "q6_array_method",
        RES,
        "        res[1:2]::RES(10)\n        res[1:2].Pullup([NET, VCC])",
    );
}

#[test]
fn probe_cartesian_law() {
    probe(
        "s1_group_to_scalar",
        RES,
        "        res[1:2]::RES(10)\n        (res1, res2) -> NET",
    );
    probe(
        "s2_scalar_to_group",
        RES,
        "        res[1:2]::RES(10)\n        NET -> (res1, res2)",
    );
    probe(
        "s3_ctor_right_group_left",
        RES,
        "        (NET, VCC) -> res1::RES(10)",
    );
    probe(
        "s4_ctor_right_single_left",
        RES,
        "        NET -> res1::RES(10)",
    );
    probe(
        "s5_ctor_right_two_distinct",
        RES,
        "        (NET, VCC) -> res1::RES(10)\n        (NET, VCC) -> res2::RES(10)",
    );
}

/// Same two-pin resistor, but `Pullup` has **no** `return`.
const RES_NORET: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - this - n2\n    }\n}\n";

#[test]
fn probe_returnless_method() {
    probe(
        "v1_array_returnless",
        RES_NORET,
        "        res[1:2]::RES(10).Pullup([NET, VCC])",
    );
    probe(
        "v2_scalar_returnless",
        RES_NORET,
        "        res1::RES(10).Pullup([NET, VCC])",
    );
    probe(
        "v3_declared_returnless",
        RES_NORET,
        "        res[1:2]::RES(10)\n        res[1:2].Pullup([NET, VCC])",
    );
    probe(
        "v4_array_with_return",
        RES,
        "        res[1:2]::RES(10).Pullup([NET, VCC])",
    );
    // Localization: does the fan-out count matter, and is `this` the trigger?
    probe(
        "w1_single_member_array",
        RES_NORET,
        "        res[1:1]::RES(10).Pullup([NET, VCC])",
    );
    probe(
        "w2_single_member_array_with_return",
        RES,
        "        res[1:1]::RES(10).Pullup([NET, VCC])",
    );
    probe(
        "w5_no_this_no_return",
        RES_NOTHIS,
        "        res[1:2]::RESS(10).Pullup([NET, VCC])",
    );
}

/// Returnless, and the body never mentions `this` — isolates whether the
/// `this`-face (implicit return) is what goes missing on the array path.
const RES_NOTHIS: &str = "component RESS(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([n1, n2]) {\n        n1 - n2\n    }\n}\n";

#[test]
fn probe_group_expansion_fidelity() {
    // Group form: §7.3 rule 2 expands to `NET -> res1::RES(10)` and
    // `VCC -> res1::RES(10)`.
    probe(
        "u1_group",
        RES,
        "        (NET, VCC) -> res1::RES(10)",
    );
    // The same two branches written out explicitly.
    probe(
        "u2_explicit",
        RES,
        "        NET -> res1::RES(10)\n        VCC -> res1::RES(10)",
    );
    probe(
        "u3_group_named_members",
        RES,
        "        res[1:2]::RES(10)\n        (NET, VCC) -> (res1, res2)",
    );
    probe(
        "u4_explicit_named_members",
        RES,
        "        res[1:2]::RES(10)\n        NET -> res1\n        VCC -> res1\n        NET -> res2\n        VCC -> res2",
    );
}
