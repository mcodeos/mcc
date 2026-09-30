// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ U368 · a dotted class construction in an infix chain must build its part.
//!
//! Why this test exists
//! In `V33 - IND.FB(600Ω, 500mA, 100MHz) - GND` the parser lowers the
//! qualified call in chain position to a receiver chain — `FuncCall func="FB"`
//! whose first child is a lone Label endpoint `IND` — and instantiation only
//! ever looked the bare tail ("FB") up in the CMIE table. The lookup missed,
//! the call dead-ended on the instance-method path (E0944 against the head
//! "IND") and was silently dropped as a failed class: **no part, no
//! diagnostics, and the net lost its branch** while `check` stayed green.
//! The undotted twin (`IND(...)`, `RES(...)`) always worked, and the same
//! dotted class in declaration position (`IND.FB fb1`) always worked — the
//! defect was strictly qualified-ctor-in-chain.
//!
//! The fix (instantiation face, no grammar change) joins the head label with
//! the func name and retries the table when the head cannot be a declared
//! instance — the same caller_unknown guard set as P2-7-XTAL. On a hit the
//! existing P2-7 arm sees label+func == comp_def.name, discards the label,
//! and auto-names the part, so the chain form lands exactly what the
//! declaration form lands.
//!
//! What is locked (do not weaken)
//! 1. every dotted chain spelling builds a real part — the net carries the
//!    part's two pins, not just the two rail ports (pre-fix this was 2
//!    endpoints and silence);
//! 2. the chain form is judged by the same face as the declaration form:
//!    zero diagnostics beyond the baseline both forms share;
//! 3. a head that IS a declared instance keeps instance-method precedence —
//!    `U1.nosuch(...)` on a declared `RES U1` must stay an instance-method
//!    miss (E3071), never recombine into a class lookup.
//!
//! Self-contained: the fixtures declare the RES / RES.SMD / IND.FB ladder
//! inline, no system library needed.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

const LADDER_FIXTURE: &str = r#"
component RES(rs::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

component RES.SMD(rs::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

component IND.FB(impd::INT, irated::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main(psnk [V33, GND])
{
    STMT
}
"#;

const DECL_FIXTURE: &str = r#"
component IND.FB(impd::INT, irated::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main(psnk [V33, GND])
{
    STMT
}
"#;

/// Instance-method fixture: a declared instance whose name is also a class
/// head (`RES U1`) — the head must keep instance precedence.
const INSTANCE_FIXTURE: &str = r#"
component RES(rs::INT)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main(psnk [V33, GND])
{
    RES U1
    STMT
}
"#;

fn src_of(fixture: &str, stmt: &str) -> String {
    fixture.replace("STMT", stmt)
}

/// The net partition of `src`: point-sets sharing a net, inner+outer sorted,
/// net NAMES dropped.
fn nets_of(src: &str, uri: &str) -> Vec<Vec<String>> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut parts: Vec<Vec<String>> = net_store
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
    parts.sort();
    parts
}

/// Every diagnostic code emitted while building `src`, sorted, deduped.
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

/// A two-pin part built between the rails: the part's `.1`/`.2` land on two
/// different nets (series topology — the rails stay separate), one net per
/// rail, same instance stem on both. Pre-fix the dotted forms landed only the
/// two bare rails — the part was gone.
fn assert_twopin_built(nets: &[Vec<String>], what: &str) {
    assert_eq!(nets.len(), 2, "{what}: one net per rail; got {nets:?}");
    for net in nets {
        assert_eq!(
            net.len(),
            2,
            "{what}: rail + one part pin per net; got {net:?}"
        );
    }
    let stems: Vec<Option<&str>> = nets
        .iter()
        .map(|net| {
            net.iter()
                .find(|p| p.ends_with(".1") || p.ends_with(".2"))
                .map(|p| p.rsplit_once('.').unwrap().0)
        })
        .collect();
    assert!(
        stems.iter().all(|s| s.is_some()),
        "{what}: each net carries one part pin; got {nets:?}"
    );
    assert_eq!(
        stems[0], stems[1],
        "{what}: both nets carry the SAME instance (one part joined by its pins); got {nets:?}"
    );
}

/// ① Every dotted chain spelling builds its part, judged on the same face.
#[test]
fn u368_dotted_chain_ctor__dotted_forms_build_their_part() {
    let ind_fb = nets_of(
        &src_of(LADDER_FIXTURE, "V33 - IND.FB(60, 50) - GND"),
        "/mcc/u368-ind-fb-chain.mc",
    );
    assert_twopin_built(&ind_fb, "IND.FB in chain");
    assert!(
        !ind_fb.iter().flatten().any(|p| p.starts_with("IND.")),
        "the head label is discarded (auto-named part), not kept as `IND`; got {ind_fb:?}"
    );

    let res_smd = nets_of(
        &src_of(LADDER_FIXTURE, "V33 - RES.SMD(47) - GND"),
        "/mcc/u368-res-smd-chain.mc",
    );
    assert_twopin_built(&res_smd, "RES.SMD in chain");

    // The same partition as the undotted twin: the chain form is a spelling,
    // not a different construction.
    let res_bare = nets_of(
        &src_of(LADDER_FIXTURE, "V33 - RES(47) - GND"),
        "/mcc/u368-res-chain.mc",
    );
    assert_twopin_built(&res_bare, "RES in chain (control)");
    assert_eq!(
        res_smd.len(),
        res_bare.len(),
        "dotted and undotted chain forms land the same net count"
    );
}

/// ② The chain form shares the declaration form's diagnostic face: whatever
/// codes the declaration position emits, the chain position emits too — the
/// fix may not silence anything, and the pre-fix silence (part gone, zero
/// diagnostics) must stay impossible.
#[test]
fn u368_dotted_chain_ctor__chain_face_matches_declaration_face() {
    let decl = codes_of(
        &src_of(DECL_FIXTURE, "IND.FB fb1(60, 50)\n    V33 - fb1 - GND"),
        "/mcc/u368-decl-form.mc",
    );
    let chain = codes_of(
        &src_of(DECL_FIXTURE, "V33 - IND.FB(60, 50) - GND"),
        "/mcc/u368-chain-form.mc",
    );
    assert!(
        chain == decl,
        "chain form codes {chain:?} must equal declaration form codes {decl:?}"
    );
}

/// ③ Instance precedence: a head that IS a declared instance never recombines.
/// `RES U1` + `U1.nosuch(...)` stays an instance-method miss (E3071) — no
/// phantom `RES.nosuch` class lookup, no part.
#[test]
fn u368_dotted_chain_ctor__declared_instance_head_keeps_method_path() {
    let codes = codes_of(
        &src_of(INSTANCE_FIXTURE, "U1.nosuch(47)"),
        "/mcc/u368-instance-head.mc",
    );
    assert!(
        codes.contains(&3071),
        "instance-method miss must surface E3071; got {codes:?}"
    );
    let nets = nets_of(
        &src_of(INSTANCE_FIXTURE, "U1.nosuch(47)"),
        "/mcc/u368-instance-head-nets.mc",
    );
    assert!(
        nets.iter().flatten().all(|p| !p.contains(".1") || p.starts_with("U1.")),
        "no part may be built from the recombination; got {nets:?}"
    );
}
