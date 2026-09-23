// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The AC mains face gates (U217, ac-interface-design.md §7): the ERC slice
//! of the `::AC.1P` landing, three judges over the flat table reading the
//! flatten-time `AcFaceCarry` — the face a row's own `::AC.*` declaration
//! states, never a name or a family table.
//!
//! - **6057 `AC_FACE_RETURN_MISSING`** — a direction-word `psrc/psnk` row
//!   declares a face whose members the variant's registry group positions:
//!   a wired phase while the declared return reaches no net is a torn face
//!   (the four-wire `AC.3P` group judges by the same law as the pair).
//!   Judged per face; the all-dangling face is the unused-declaration
//!   silence law, and unwired *pins* stay E4119's object.
//! - **6058 `AC_NOMINAL_CONFLICT`** — two `::AC.*` faces stating different
//!   region nominals on one copper (230 V/50 Hz vs 120 V/60 Hz) is a contract
//!   contradiction. The empty form states no nominal and conflicts with
//!   nothing — the region-neutral inlet shape must stay quiet.
//! - **6059 `PROTECTIVE_PIN_NO_COPPER`** — a pin's `@role(protective)`/
//!   `@role(earth)` word demands protective copper: the net must touch a
//!   conductor its scope declares with that role. The pin's own word is the
//!   demand, never the witness; an unwired protective pin stays the
//!   unwired-pin family's object.
//!
//! Acceptance discipline: every verdict branch carries a member, including
//! the silence branches (both members wired, both dangling, agreeing
//! nominals, empty-form meeting, protective conduit witness, unwired PE).

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The AC mains family under its real name: the family gate reads the
/// pre-dot segment (`AC`), the nominal decode reads the declared params.
/// No library is loaded here — the harness declares the face inline, the
/// same shape `ifs/ac.mc` states (members L/N, no roles, PE is not a member).
const ACP: &str = r#"
interface AC.1P(volt::UV.VOLT, freq::UV.HZ)
{
    topology = "point to point"
    pins = [
        1 = L
        2 = N
    ]
}
"#;

/// The earthed inlet component: the two interface members adopt the
/// region-neutral empty form `::AC.1P()` (the connector-face law), PE is a
/// separate pin demanding protective copper by its own role word.
const INLET: &str = r#"
component INLET
{
    pins = [
        [1,2] = [L,N]::AC.1P(), ["Line","Neutral"]
        3 = PE @role(protective), "Protective earth"
    ]
}
"#;

/// The three-phase family, the same inline shape the canon states (members
/// L1..L3 + N, no roles, PE is not a member) — the four-wire Y face.
const AC3P: &str = r#"
interface AC.3P(volt::UV.VOLT, freq::UV.HZ)
{
    topology = "point to point"
    pins = [
        1 = L1
        2 = L2
        3 = L3
        4 = N
    ]
}
"#;

/// Build `main` with the body statements and return the sorted diagnostic
/// codes.
fn build(body: &str) -> Vec<u32> {
    build_with(body, "")
}

/// The same harness with an extra interface family declared beside `AC.1P` —
/// the three-phase tests glue [`AC3P`] in.
fn build_with(body: &str, extra_iface: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!("{ACP}{extra_iface}{INLET}module main {{\n{body}\n}}\n");
    let uri: McURI = "/mcc/ac-face-gates.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

fn count(code: u32, body: &str) -> usize {
    build(body).iter().filter(|&&c| c == code).count()
}

fn count3(code: u32, body: &str) -> usize {
    build_with(body, AC3P).iter().filter(|&&c| c == code).count()
}

/// The wired inlet chain: both interface members reach the inlet, PE left
/// for the protective tests to wire.
const CHAIN: &str = "\
    psnk mains{L, N}::AC.1P(230V, 50Hz)\n\
    INLET inlet\n\
    mains.L -> inlet.L\n\
    mains.N -> inlet.N\n";

/// 6057's defect: the face is the pair. `mains.L` reaches the inlet while
/// `mains.N` is never mentioned — a single-line supply. Exactly one fire,
/// anchored on the dangling member.
#[test]
fn ac_face_return__single_wired_member_fires_once() {
    let body = "    psnk mains{L, N}::AC.1P(230V, 50Hz)\n    INLET inlet\n    mains.L -> inlet.L";
    assert_eq!(
        count(mcc::errcodes::AC_FACE_RETURN_MISSING, body),
        1,
        "L wired with N dangling is a single-line supply → one 6057; got {:#?}",
        build(body)
    );
}

/// The healthy chain: both members wired — the face is complete, the gate
/// says nothing. (The inlet's PE stays unwired here; that is E4119's
/// object, not 6057's.)
#[test]
fn ac_face_return__both_members_wired_is_quiet() {
    assert_eq!(
        count(mcc::errcodes::AC_FACE_RETURN_MISSING, CHAIN),
        0,
        "a complete face is quiet; got {:#?}",
        build(CHAIN)
    );
}

/// The silence law: neither member wired is an unused declaration, not a
/// torn one — the same all-dangling silence the exclusive-peer gate keeps.
#[test]
fn ac_face_return__both_members_dangling_is_silent() {
    let body = "    psnk mains{L, N}::AC.1P(230V, 50Hz)";
    assert_eq!(
        count(mcc::errcodes::AC_FACE_RETURN_MISSING, body),
        0,
        "an unused face is not a torn face; got {:#?}",
        build(body)
    );
}

/// The four-wire Y face judges by the same law as the pair: all four members
/// wired face-to-face — the face is complete, the gate says nothing. This is
/// the branch the two-member guard used to skip silently: the multi-member
/// group must enter the gate, not pass beside it.
#[test]
fn ac_face_return_3p__all_four_members_wired_is_quiet() {
    let body = "\
    psrc feed{L1, L2, L3, N}::AC.3P(400V, 50Hz)\n\
    psnk load{L1, L2, L3, N}::AC.3P(400V, 50Hz)\n\
    feed.L1 -> load.L1\n\
    feed.L2 -> load.L2\n\
    feed.L3 -> load.L3\n\
    feed.N -> load.N";
    assert_eq!(
        count3(mcc::errcodes::AC_FACE_RETURN_MISSING, body),
        0,
        "a complete three-phase face is quiet; got {:#?}",
        build_with(body, AC3P)
    );
}

/// The three-phase defect: phases wired, the shared return dangling on both
/// faces — one fire per face, each anchored on its own `N` member. The
/// return is the position the registry's group puts `Neutral` in (position
/// 4), never a read of the written name.
#[test]
fn ac_face_return_3p__return_dangling_fires_once_per_face() {
    let body = "\
    psrc feed{L1, L2, L3, N}::AC.3P(400V, 50Hz)\n\
    psnk load{L1, L2, L3, N}::AC.3P(400V, 50Hz)\n\
    feed.L1 -> load.L1\n\
    feed.L2 -> load.L2\n\
    feed.L3 -> load.L3";
    assert_eq!(
        count3(mcc::errcodes::AC_FACE_RETURN_MISSING, body),
        2,
        "each face's dangling return fires once → two 6057; got {:#?}",
        build_with(body, AC3P)
    );
}

/// 6058's defect: two stated nominals on one copper. 230 V/50 Hz meeting
/// 120 V/60 Hz is not one mains — one fire per axis (voltage, frequency).
#[test]
fn ac_nominal_conflict__two_families_on_one_copper_fires_per_axis() {
    let body = "\
    psrc feed{L, N}::AC.1P(230V, 50Hz)\n\
    psnk load{L, N}::AC.1P(120V, 60Hz)\n\
    feed.L -> load.L\n\
    feed.N -> load.N";
    assert_eq!(
        count(mcc::errcodes::AC_NOMINAL_CONFLICT, body),
        2,
        "voltage and frequency both disagree → two 6058; got {:#?}",
        build(body)
    );
}

/// The agreeing meeting: same nominal on both faces — one mains, quiet.
#[test]
fn ac_nominal_conflict__agreeing_nominals_are_quiet() {
    let body = "\
    psrc feed{L, N}::AC.1P(230V, 50Hz)\n\
    psnk load{L, N}::AC.1P(230V, 50Hz)\n\
    feed.L -> load.L\n\
    feed.N -> load.N";
    assert_eq!(
        count(mcc::errcodes::AC_NOMINAL_CONFLICT, body),
        0,
        "one stated region is one mains; got {:#?}",
        build(body)
    );
}

/// The empty form is region-neutral: the inlet's `::AC.1P()` faces state no
/// nominal, so the stated source face conflicts with nothing — the
/// connector-face law must not turn every inlet into a conflict.
#[test]
fn ac_nominal_conflict__empty_form_meets_stated_nominal_is_quiet() {
    assert_eq!(
        count(mcc::errcodes::AC_NOMINAL_CONFLICT, CHAIN),
        0,
        "a region-neutral inlet cannot conflict; got {:#?}",
        build(CHAIN)
    );
}

/// 6059's defect: PE wired, but onto a role-less conduit — the net touches
/// no protective copper. The pin's own word is the demand; the demand goes
/// unanswered. Exactly one fire.
#[test]
fn protective_pin__role_less_conduit_fires_once() {
    let body = format!(
        "{CHAIN}    conduit CHASSIS\n    inlet.PE -> CHASSIS"
    );
    assert_eq!(
        count(mcc::errcodes::PROTECTIVE_PIN_NO_COPPER, &body),
        1,
        "PE onto a role-less conduit is unanswered protective demand; got {:#?}",
        build(&body)
    );
}

/// The witness branch: the conduit the scope declares
/// `@role(protective)` is the protective copper — the same wiring goes
/// quiet. The pin's own word is the demand, the peer conductor's declared
/// role is the witness.
#[test]
fn protective_pin__protective_conduit_is_quiet() {
    let body = format!(
        "{CHAIN}    conduit PEW @role(protective)\n    inlet.PE -> PEW"
    );
    assert_eq!(
        count(mcc::errcodes::PROTECTIVE_PIN_NO_COPPER, &body),
        0,
        "PE onto a declared protective conduit is answered; got {:#?}",
        build(&body)
    );
}

/// An unwired protective pin makes no demand on any net — the unwired-pin
/// family's object (E4119), never this gate's.
#[test]
fn protective_pin__unwired_pin_is_quiet() {
    assert_eq!(
        count(mcc::errcodes::PROTECTIVE_PIN_NO_COPPER, CHAIN),
        0,
        "an unwired PE pin is E4119's object; got {:#?}",
        build(CHAIN)
    );
}

/// The messages name the facts the design doc promises: 6057 the face and
/// both members, 6059 the pin, the role word and the net.
#[test]
fn ac_gate_messages_name_their_facts() {
    let _lock = common::lock();
    common::reset();
    let body = "\
    psnk mains{L, N}::AC.1P(230V, 50Hz)\n\
    INLET inlet\n\
    conduit CHASSIS\n\
    mains.L -> inlet.L\n\
    inlet.PE -> CHASSIS\n";
    let src = format!("{ACP}{INLET}module main {{\n{body}\n}}\n");
    let uri: McURI = "/mcc/ac-face-gates-msg.mc".to_string();
    mcc::mcc_load_from_string(&uri, &src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let diags = mcc::mcc_diagnose_all();
    let ret: Vec<_> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::AC_FACE_RETURN_MISSING)
        .collect();
    assert_eq!(ret.len(), 1, "N dangling behind a wired L → one 6057: {diags:?}");
    let m = &ret[0].msg;
    assert!(
        m.contains("mains") && m.contains("L") && m.contains("N"),
        "6057 must name the face and both members: {m}"
    );
    let pe: Vec<_> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::PROTECTIVE_PIN_NO_COPPER)
        .collect();
    assert_eq!(pe.len(), 1, "one unanswered PE demand: {diags:?}");
    let m = &pe[0].msg;
    assert!(
        m.contains("PE") && m.contains("protective") && m.contains("CHASSIS"),
        "6059 must name the pin, the role word and the net: {m}"
    );
}
