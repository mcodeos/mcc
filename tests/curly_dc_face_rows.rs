// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Curly two-face DC chains through component power-group rows (model A,
//! "unified vector connect rule" — 82b80c0 / vec-arch.md §5, vec-dianlu.md).
//!
//! A DC pair on each side is routed *through* a component whose power pins are
//! declared as two power-group rows, written either as literal bracket rows
//! `oring{ [IN1, GND] | [OUT, GND] }` or as whole-group face names
//! `ldo{VIN | VOUT}`. Both are canonical spellings of the same model-A shape:
//! the left face binds to the input vector by position, the right face binds to
//! the output vector, and a return member shared across faces (GND on the same
//! physical pin) unifies on one net.
//!
//! Regressions fixed together:
//! 1. `mc_phrase.rs` CURLY_MN: a literal bracket row `[IN1, GND]` parses as a
//!    single MCAST_OPD_SQUARE_VEC whose `to_id_or_ida_or_num()` returned only
//!    its *first* child (`IN1`, dropping `GND`), so the face shrank to a 1-row
//!    vector and opcheck rejected the 2-row pair as E4007 Shape mismatch.
//! 2. `bus.rs` process_curly_mn_as_bus (component branch): a face naming a whole
//!    multi-member group (`ldo.VIN` → members Vin/GND on pins 1/2) collapsed to
//!    a single point, so instantiation saw L=2 vs R=1 and fired E4007. The face
//!    now expands to its member lanes (mirror of the submodule branch).
//! 3. `mc_pins` power-pair expansion (§5.3 whole-DC-pair face): the regression-2
//!    whole-group face only covered *plain* rows (`[1,2] = VIN{Vin, GND}`).
//!    The golden power rows are directional contracts `psnk [1,2] = VIN{...}::DC`,
//!    whose head registers differently, so a face naming the whole port (`VIN`)
//!    again shrank to the head's first pin → E4007. `power_pair_member_refs`
//!    now expands such a face to the captured row's dotted `[hot, ret]` refs
//!    (`VIN` → `VIN.Vin`, `VIN.GND`), in declaration order.
//! 4. §5.3 whole-DC-pair *endpoint* — the same whole-port head used as a chain
//!    operand (`bat.BAT` — referencing the whole declaration), not just as a curly face: the
//!    two-segment dot path shrank the head to its first pin again. It now
//!    expands to the full captured pair before the member-bus build.
//! 5. §5.3.1 sink whole-hang (no `|`): `[VDDA, GNDA] -> uc{AVDD, AGND}` drives a
//!    sink's DC pair with a pair literal (the ✓ replacement for the banned
//!    single-point broadcast). The no-bar single-side column must wire both
//!    members by position.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

const SRC: &str = r#"
component ORING2
{
    pins = [
        [1,2] = [IN1, GND]
        [3,4] = [IN2, GND]
        [5,6] = [OUT, GND]
    ]
}

component LDO2
{
    pins = [
        [1,2] = VIN{Vin, GND}
        [3,2] = VOUT{Vout, GND}
    ]
}

module top
{
    ORING2 oring
    LDO2 ldo

    [VBUS_RAW, GND] -> oring{ [IN1, GND] | [OUT, GND] } -> [VMAIN_5V, GND]
    [VMAIN_5V, GND] -> ldo{VIN | VOUT} -> [VDD, GND]
}
"#;

/// Golden-power spelling (LDO.SGM2019 / DCDC.LP3220 rows): the same through-
/// device face over *directional contract* rows `psnk [1,2] = VIN{Vin, GND}::DC`
/// / `psrc [3,2] = VOUT{Vout, GND}::DC`. Each whole-group face must expand to
/// its port's full [hot, ret] member refs (regression 3) and wire 2×2.
const SRC_PWR: &str = r#"
component LDO_PWR
{
    pins = [
        psnk [1,2] = VIN{Vin, GND}::DC(5V)
        psrc [3,2] = VOUT{Vout, GND}::DC(3.3V)
    ]
}

module top
{
    LDO_PWR ldo

    [VMAIN_5V, GND] -> ldo{VIN | VOUT} -> [VDD_3V3, GND]
}
"#;

/// Golden oring battery-leg spelling (main.mc ②, regression 4): the left
/// endpoint names the *whole* `psbi` pair (`b.BAT`, a named member-bus row
/// `BAT{VCC, GND}`), so it must carry the full 2-lane [hot, ret] into the
/// `[IN2, GND]` face — not collapse to the head's first pin.
const SRC_ORPWR: &str = r#"
component ORING_PWR
{
    pins = [
        psnk [1,2] = [IN1, GND]::DC(5V)
        psnk [3,4] = [IN2, GND]::DC(5V)
        psrc [5,6] = [OUT, GND]::DC(5V)
    ]
}

component BAT_PWR
{
    pins = [
        psbi [1,2] = BAT{VCC, GND}::DC(5V)
    ]
}

module top
{
    BAT_PWR b
    ORING_PWR o

    b.BAT -> o{ [IN2, GND] | [OUT, GND] } -> [VMAIN_5V, GND]
}
"#;

/// §5.3.1 ✓ sink whole-hang: the MCU analog sink row `psnk [17,18] = [AVDD, AGND]`
/// driven by a DC-pair literal through the no-bar single-side form (regression 5).
const SRC_SINKHANG: &str = r#"
component MCU_AUD
{
    pins = [
        psnk [17,18] = [AVDD, AGND]::DC(3.3V)
        psnk [5,21]  = [VDD, GND]::DC(3.3V)
    ]
}

module top
{
    MCU_AUD u

    [VDDA, GNDA] -> u{ AVDD, AGND }
}
"#;

fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/curly-dc-face-rows.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("top"), &uri);
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();
    codes
}

/// Return every (entry path, net name) pair of the flat pass-2 netlist.
fn net_pairs(src: &str) -> Vec<(String, String)> {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/curly-dc-face-rows.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let entry = mcc::McSpaceName {
        ident: McIds::from("top"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");

    let mut pairs = Vec::new();
    for net in table.get_nets() {
        for &point_id in &net.points {
            let Some(entry) = table.get_entry(point_id) else {
                continue;
            };
            pairs.push((entry.path.clone(), net.name.clone()));
        }
    }
    pairs
}

/// Net-name of the single entry whose path ends with `suffix` (panics if not
/// exactly one such entry — a dropped/mis-expanded member fails loudly).
fn net_of(pairs: &[(String, String)], suffix: &str) -> String {
    let mut hits: Vec<&str> = pairs
        .iter()
        .filter(|(p, _)| p.ends_with(suffix))
        .map(|(_, n)| n.as_str())
        .collect();
    hits.sort_unstable();
    hits.dedup();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one entry ending {suffix:?}; got {hits:?} in {pairs:?}"
    );
    hits[0].to_string()
}

/// Neither spelling may fire E4007 (shape mismatch), E3132 (stmt parse failed),
/// or E3152 (curly base wrong) — the two-face DC chain is legal model A.
#[test]
fn curly_dc__bracket_row_and_group_face_are_legal() {
    let codes = build_codes(SRC);
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "curly DC faces must not fire E4007; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "curly DC faces must not fire E3132; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CURLY_MN_WRONG_BASE),
        "curly DC faces must not fire E3152; got codes: {codes:?}"
    );
}

/// Model-A wiring: the bracket-row form binds IN1 (not just the row lead) to the
/// hot vector; the group-face form expands `ldo.VIN`/`VOUT` to their member
/// pins so pin1 (VIN hot) and pin3 (VOUT hot) reach their nets and the shared
/// pin2 (GND) unifies on the return net.
#[test]
fn curly_dc__row_members_and_group_lanes_wire() {
    let pairs = net_pairs(SRC);

    // Mechanism A — bracket-row face `[IN1, GND]`: GND must NOT be dropped from
    // the row (the old SQUARE_VEC flatten bug kept only the row lead `IN1`).
    // The face member resolves onto the row's pin (`IN1` = row pin 1, `OUT` =
    // row pin 5), so the endpoint is named by the pin path.
    assert_eq!(
        net_of(&pairs, "oring.1"),
        "VBUS_RAW",
        "oring IN1 (row pin 1) should join VBUS_RAW"
    );
    assert_eq!(net_of(&pairs, "oring.5"), "VMAIN_5V");

    // Mechanism B — whole-group face `VIN | VOUT`: each face must expand to its
    // member pins (pins 1 and 3 land on their nets; shared pin 2 is the return),
    // not collapse to a single point (the old E4007 L=2 vs R=1 defect).
    assert_eq!(
        net_of(&pairs, "ldo.1"),
        "VMAIN_5V",
        "ldo VIN hot (pin1) should join VMAIN_5V"
    );
    assert_eq!(
        net_of(&pairs, "ldo.3"),
        "VDD",
        "ldo VOUT hot (pin3) should join VDD"
    );

    // Return members stay on GND nets (never glued to a hot net or left
    // floating) — net *partition* between the two GND labels is not asserted.
    assert!(
        pairs
            .iter()
            .any(|(p, n)| p.ends_with("ldo.2") && n.starts_with("GND")),
        "ldo shared GND (pin2) should be on a return net: {pairs:?}"
    );
    assert!(
        pairs.iter().any(
            |(p, n)| (p.ends_with("oring.2") || p.ends_with("oring.6")) && n.starts_with("GND")
        ),
        "oring GND pins should be on return nets: {pairs:?}"
    );
}

/// Regression 3 — the whole-group face over *power-contract* rows must not
/// shrink to the head's first pin: no E4007/E3132/E3152, and each port's full
/// [hot, ret] member pair reaches its nets.
#[test]
fn curly_dc__power_row_whole_group_face_wires_full_pair() {
    let codes = build_codes(SRC_PWR);
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "whole-DC-pair faces on psnk/psrc rows must not fire E4007; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "whole-DC-pair faces on psnk/psrc rows must not fire E3132; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CURLY_MN_WRONG_BASE),
        "whole-DC-pair faces on psnk/psrc rows must not fire E3152; got codes: {codes:?}"
    );

    // Wiring: the face member spelling (`ldo.VIN.Vin`) folds onto the declared
    // pin it names — VIN hot is row pin 1, VOUT hot is row pin 3, and the `GND`
    // member both rows share is the one physical return pin 2. So the wiring
    // lands on pin paths, and the shared return is a single entry, not two.
    let pairs = net_pairs(SRC_PWR);
    assert_eq!(
        net_of(&pairs, "ldo.1"),
        "VMAIN_5V",
        "VIN hot member (pin1) should join the input hot vector"
    );
    assert_eq!(
        net_of(&pairs, "ldo.3"),
        "VDD_3V3",
        "VOUT hot member (pin3) should join the output hot vector"
    );
    let ret = net_of(&pairs, "ldo.2");
    assert!(
        ret.starts_with("GND"),
        "the shared return member (pin2) should be on a return net; got {ret}"
    );
}

/// Regression 4 — a whole DC-pair endpoint (`b.BAT`) feeding a 2×2 oring face
/// must carry both lanes: no E4007/E3132/E3152, battery hot lands on IN2,
/// output lands on VMAIN_5V, and both returns unify on the GND return.
#[test]
fn curly_dc__whole_pair_endpoint_wires_both_lanes() {
    let codes = build_codes(SRC_ORPWR);
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "whole-pair endpoint into a 2-wide face must not fire E4007; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED),
        "whole-pair endpoint into a 2-wide face must not fire E3132; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CURLY_MN_WRONG_BASE),
        "whole-pair endpoint into a 2-wide face must not fire E3152; got codes: {codes:?}"
    );

    let pairs = net_pairs(SRC_ORPWR);
    // Battery hot (pin1) joins the IN2 face's hot (row pin 3); oring output
    // (row pin 5) reaches the main pair.
    let n_in2 = net_of(&pairs, "o.3");
    assert_eq!(
        net_of(&pairs, "b.1"),
        n_in2,
        "battery hot pin1 must join oring IN2; pairs: {pairs:?}"
    );
    assert_eq!(
        net_of(&pairs, "o.5"),
        "VMAIN_5V",
        "oring OUT must join VMAIN_5V"
    );
    // Battery return (pin2) and the IN2 face return unify on the shared GND return.
    let n_b2 = net_of(&pairs, "b.2");
    let n_r4 = net_of(&pairs, "o.4");
    assert_eq!(
        n_b2, n_r4,
        "battery return and IN2 return must share one net"
    );
    assert!(
        n_b2.starts_with("GND"),
        "the shared return must be a GND net; got {n_b2}"
    );
}

/// Regression 5 — §5.3.1 sink whole-hang (no `|`): a DC-pair literal drives the
/// sink's two bare members by position (hot → AVDD pin17, return → AGND pin18).
#[test]
fn curly_dc__sink_whole_hang_wires_both_members() {
    let codes = build_codes(SRC_SINKHANG);
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH)
            && !codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED)
            && !codes.contains(&mcc::errcodes::CURLY_MN_WRONG_BASE),
        "single-side whole-hang must not fire E4007/E3132/E3152; got codes: {codes:?}"
    );

    let pairs = net_pairs(SRC_SINKHANG);
    assert_eq!(net_of(&pairs, "u.17"), "VDDA", "AVDD pin17 joins VDDA");
    assert_eq!(net_of(&pairs, "u.18"), "GNDA", "AGND pin18 joins GNDA");
    // The unwired second sink row (VDD/GND pins 5/21) must not be swept onto a
    // driven net by the whole-hang (the banned single-point broadcast would).
    assert!(
        !pairs.iter().any(|(p, n)| {
            (p.ends_with("u.5") || p.ends_with("u.21")) && (n == "VDDA" || n == "GNDA")
        }),
        "whole-hang must not leak onto the unwired VDD row: {pairs:?}"
    );
}

/// Regression 6 — a whole-pair curly face followed by a *lane-series* element
/// vector (`- [el, _] ->`, golden main.mc buck12 spelling) must keep the
/// through-device's return members wired. The chain contains a `_` lead, so it
/// is routed lane by lane. Placing the curly Node on lane 0 only wires the hot
/// lane (the VIN/LX hot members through the element) and silently drops the
/// device's shared GND return — zero explicit error, just NET_PARTIAL_CONNECTION
/// and a missing current path. `stmt.rs` places
/// the Node on every face lane, so the shared return pin (pin 2) lands on the
/// return net.
const SRC_LANE: &str = r#"
component BUCK2
{
    pins = [
        psnk [1,2] = VIN{Vin, GND}::DC(5V)
        psrc [3,2] = LX{Lx, GND}::DC(1.2V)
    ]
}

component IND2
{
    pins = [
        [1,2] = [A, B]
    ]
}

module top
{
    BUCK2 bk

    [VMAIN_5V, GND] -> bk{VIN | LX} - [IND2(), _] -> [VCC_1V2, GND]
}
"#;

#[test]
fn curly_dc__lane_series_keeps_through_device_return() {
    let codes = build_codes(SRC_LANE);
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH)
            && !codes.contains(&mcc::errcodes::CONN_STMT_PARSE_FAILED)
            && !codes.contains(&mcc::errcodes::CURLY_MN_WRONG_BASE),
        "lane-series through a curly face must not fire E4007/E3132/E3152; got codes: {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::NET_PARTIAL_CONNECTION),
        "lane-series must not leave the through-device partially connected; got codes: {codes:?}"
    );

    let pairs = net_pairs(SRC_LANE);
    // Hot lane: the VIN hot member (row pin 1) sits on the input rail; the LX
    // hot member (row pin 3) is the switching node (must NOT sit on VCC_1V2 —
    // the IND element in between isolates it, §4.3).
    assert_eq!(
        net_of(&pairs, "bk.1"),
        "VMAIN_5V",
        "buck VIN hot joins the input rail"
    );
    let lx = net_of(&pairs, "bk.3");
    assert_ne!(
        lx, "VCC_1V2",
        "buck switching node must sit on its own net through the IND, not VCC_1V2"
    );
    // The IND element bridges: pin 2 reaches the output rail (the element is
    // auto-named, so assert via a point on VCC_1V2 that is not the bk return).
    assert!(
        pairs
            .iter()
            .any(|(p, n)| n == "VCC_1V2" && p.contains("IND2") && p.ends_with(".2")),
        "IND element output pin must reach VCC_1V2: {pairs:?}"
    );
    // Return lane (the regression): both faces' `GND` members name the same
    // physical return pin 2, so the return is present once and sits on a GND net
    // — pre-fix the return members were silently absent (net_of panics).
    let ret = net_of(&pairs, "bk.2");
    assert!(
        ret.starts_with("GND"),
        "the shared return must be a GND net; got {ret}"
    );
}
