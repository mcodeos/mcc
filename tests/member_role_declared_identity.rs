// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Member-role declaration identity (classification-retirement batch2 R1): a
//! module-header port member gets an electrical role only when it names THIS
//! module's OWN declared copper — a `conduit` name / declared DC rail `ret`
//! member → Ground side; a declared DC rail `hot` member → Power side. The old
//! `is_ground_name`/`is_supply_name` name fallback is removed: a bare member
//! with no declaration (a legacy module, or an undeclared member of a declared
//! one) is Signal → no `member_info` is recorded (design ③: no declaration →
//! never judged, never guessed).
//!
//! Locks the accepted behavior change: a legacy `vin{VDD, GND}` header no
//! longer paints GND/VDD as Ground/Power rails purely from their names; only a
//! `conduit GND` (or declared DC rail) does.

#![allow(non_snake_case)]

mod common;

use mcc::{mcb_pass2_flat, McIds, McSpaceName, MemberRole};

/// `(member path, member_info role or "none")` for every flattened entry under
/// a `vin{...}` header port, sorted.
fn vin_member_roles(src: &str) -> Vec<(String, String)> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/member-role-declared-identity.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    let entry = McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");

    let mut out: Vec<(String, String)> = table
        .iter()
        .filter_map(|(_, e)| {
            if e.path.contains(".vin.") {
                let role = e
                    .member_info
                    .as_ref()
                    .map(|m| format!("{:?}", m.role))
                    .unwrap_or_else(|| "none".to_string());
                Some((e.path.clone(), role))
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

const LEGACY: &str = r#"
module main {
    io vin{VDD, GND}
}
"#;

const DECLARED: &str = r#"
module main {
    conduit GND @role(main)
    io vin{VDD, GND}
}
"#;

#[test]
fn legacy_members_have_no_role() {
    let roles = vin_member_roles(LEGACY);
    // No `conduit`/`rail` declaration → `GND` is not this module's declared
    // return copper and `VDD` is not a declared rail hot → both Signal, no
    // member_info (no name guessing).
    assert!(
        !roles.is_empty(),
        "expected vin header members in flat table"
    );
    for (path, role) in &roles {
        assert_eq!(
            role, "none",
            "legacy member {path} must not get a name-guessed role, got {role:?}"
        );
    }
}

#[test]
fn declared_conduit_ground_member_gets_ground() {
    let roles = vin_member_roles(DECLARED);
    let gnd = roles
        .iter()
        .find(|(p, _)| p.ends_with("vin.GND"))
        .unwrap_or_else(|| panic!("expected main.vin.GND member, got: {roles:?}"));
    assert_eq!(
        gnd.1,
        format!("{:?}", MemberRole::Ground),
        "conduit-declared GND member must be Ground, got: {roles:?}"
    );
    // `VDD` is still not declared anywhere in this module → stays Signal
    // (only the declared return copper anchors a role).
    let vdd = roles
        .iter()
        .find(|(p, _)| p.ends_with("vin.VDD"))
        .unwrap_or_else(|| panic!("expected main.vin.VDD member, got: {roles:?}"));
    assert_eq!(
        vdd.1, "none",
        "undeclared VDD member must stay Signal, got: {roles:?}"
    );
}

// Connection-point DC pair (classification-retirement-design §4, C full
// capture, batch 2): a `::DC` pair WRITTEN on a connection point declares its
// members positionally — 1st member = hot (supply side), 2nd = ret (declared
// return / ground side). Module-header DC ports and component DC pins both get
// their role this way, independent of the member name.

/// (path, role) for every flattened entry whose path satisfies `keep`, sorted.
fn flat_roles(src: &str, keep: &dyn Fn(&str) -> bool) -> Vec<(String, String)> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/member-role-dc-pair.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    let entry = McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcb_pass2_flat(&entry, 1).expect("pass2_flat failed");
    let mut out: Vec<(String, String)> = table
        .iter()
        .filter_map(|(_, e)| {
            if keep(&e.path) {
                let role = e
                    .member_info
                    .as_ref()
                    .map(|m| format!("{:?}", m.role))
                    .unwrap_or_else(|| "none".to_string());
                Some((e.path.clone(), role))
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

/// A module-header DC port pair `psnk dc{hot, ret}::DC` (curly-bus signature
/// form, the canonical header shape — MIC_SIP/US513 declare `psnk dc{...}::DC`
/// exactly this way) — the `::DC` interface is the entry criterion, so the ret
/// member (sitting 2nd) is a declared return → Ground, and the hot member (1st)
/// is the supply face → Power. Neither role depends on the member names (here
/// they deliberately do NOT contain GND/VDD keywords — positional decode, no
/// name guessing).
///
/// Shape note: the header pair only expands to member ports when a parent
/// binds actual nets into it (`[hot, ret] -> sub.dc`, MIC_SIP-in-main shape) —
/// an uninstantiated module's own `::DC` header row stays one opaque port. A
/// minimal in-source `interface DC` supplies the interface class so the header
/// row resolves to an Interface without the system library (the pin decl
/// mirrors ifs/dc.mc's DC shape; the written `dc{hot, ret}` members override
/// the interface's default pins).
#[test]
fn module_header_dc_pair_ret_member_is_ground() {
    const DC_HEADER: &str = r#"
interface DC {
    pins = [ 1 = VCC; 2 = GND ]
}
module SUB(psnk dc{V3V3_SUPPLY, RTN}::DC(3.3V)) {
    R1::RES(1k)
    dc.V3V3_SUPPLY -> R1.1
    dc.RTN -> R1.2
}
module main {
    SUB s
    [V3V3_SUPPLY, RTN] -> s.dc
}
"#;
    let roles = flat_roles(DC_HEADER, &|p| {
        p.ends_with("dc.RTN") || p.ends_with("dc.V3V3_SUPPLY")
    });
    let rtn = roles
        .iter()
        .find(|(p, _)| p.ends_with("dc.RTN"))
        .unwrap_or_else(|| panic!("expected main.s.dc.RTN member, got: {roles:?}"));
    assert_eq!(
        rtn.1,
        format!("{:?}", MemberRole::Ground),
        "2nd member of a ::DC pair is the declared return → Ground, got: {roles:?}"
    );
    let hot = roles
        .iter()
        .find(|(p, _)| p.ends_with("dc.V3V3_SUPPLY"))
        .unwrap_or_else(|| panic!("expected main.s.dc.V3V3_SUPPLY member, got: {roles:?}"));
    assert_eq!(
        hot.1,
        format!("{:?}", MemberRole::Power),
        "1st member of a ::DC pair is the supply face → Power, got: {roles:?}"
    );
}

/// A component DC pin row's ret member is a declared return at THAT component's
/// connection point — the pin whose function name is the row's ret reads Ground
/// even though the module declares no conduit/rail (the component's own
/// `McPwrPin` write is the declaration, pin≡port). Names kept non-keyword so a
/// pass proves it is positional, not name-driven.
#[test]
fn component_dc_row_ret_pin_is_ground() {
    const BOARD: &str = r#"
component LDO.A {
    pins = [
        psnk [7, 8] = [VCAP, RTNA]::DC(3.3V)
    ]
}
module main {
    io vin{VCAP, RTNA} @class(analog)
    LDO.A l1
    VCAP -> l1.7
    RTNA -> l1.8
}
"#;
    // The module does NOT declare conduit/rail — only the component's own
    // `psnk [7,8]=[VCAP,RTNA]::DC` row declares the pair. Pin 8 (func RTNA) is
    // that row's ret → Ground; pin 7 (func VCAP) its hot → Power.
    let roles = flat_roles(BOARD, &|p| {
        p == "main.l1.8" || p == "main.l1.7" || p.ends_with(".vin.RTNA") || p.ends_with(".vin.VCAP")
    });
    let rtn_pin = roles
        .iter()
        .find(|(p, _)| p == "main.l1.8")
        .unwrap_or_else(|| panic!("expected main.l1.8 pin entry, got: {roles:?}"));
    assert_eq!(
        rtn_pin.1,
        format!("{:?}", MemberRole::Ground),
        "component DC row ret pin must be Ground, got: {roles:?}"
    );
    let hot_pin = roles
        .iter()
        .find(|(p, _)| p == "main.l1.7")
        .unwrap_or_else(|| panic!("expected main.l1.7 pin entry, got: {roles:?}"));
    assert_eq!(
        hot_pin.1,
        format!("{:?}", MemberRole::Power),
        "component DC row hot pin must be Power, got: {roles:?}"
    );
}

/// A **scalar** `::DC` port (`in vin::DC(5V)`, i.e. no pair written on the row)
/// carries the same declared face pair as the written forms. The faces come
/// from the DC interface's own pin table, positionally — so the port's members
/// get roles whichever of the two equivalent spellings the author used.
///
/// The interface here declares `ALPHA`/`BETA`, deliberately NOT GND/VDD-shaped
/// words: a pass proves the role is the *position* (1st = supply face, 2nd =
/// declared return), never the name. This is the spelling-independence lock —
/// the written forms are covered by `module_header_dc_pair_ret_member_is_ground`
/// above, and a `::DC` row that writes its own pair still has those names
/// override the interface's (`extract_port_bus_members` prefers the written
/// members, and the pair decode reads the same source).
#[test]
fn scalar_dc_port_carries_its_declared_face_pair() {
    const SCALAR_DC: &str = r#"
interface DC {
    pins = [ 1 = ALPHA; 2 = BETA ]
}
module main {
    in vin::DC(5V)
    out vout::DC(3.3V)
}
"#;
    let roles = flat_roles(SCALAR_DC, &|p| {
        p.ends_with(".vin.ALPHA")
            || p.ends_with(".vin.BETA")
            || p.ends_with(".vout.ALPHA")
            || p.ends_with(".vout.BETA")
    });
    for port in ["vin", "vout"] {
        let hot = roles
            .iter()
            .find(|(p, _)| p.ends_with(&format!(".{port}.ALPHA")))
            .unwrap_or_else(|| panic!("expected main.{port}.ALPHA member, got: {roles:?}"));
        assert_eq!(
            hot.1,
            format!("{:?}", MemberRole::Power),
            "scalar ::DC port '{port}': 1st interface face is the supply face → Power, got: {roles:?}"
        );
        let ret = roles
            .iter()
            .find(|(p, _)| p.ends_with(&format!(".{port}.BETA")))
            .unwrap_or_else(|| panic!("expected main.{port}.BETA member, got: {roles:?}"));
        assert_eq!(
            ret.1,
            format!("{:?}", MemberRole::Ground),
            "scalar ::DC port '{port}': 2nd interface face is the declared return → Ground, got: {roles:?}"
        );
    }
}

/// The pair is a `::DC` property, not "an interface with two members": a port
/// on any other interface declares no supply/return faces, so its members stay
/// Signal (ruling ① — no declaration, no role; never name-guessed). This is the
/// exact boundary of the scalar-DC change.
#[test]
fn non_dc_interface_port_declares_no_faces() {
    const OTHER: &str = r#"
interface WIRE2 {
    pins = [ 1 = ALPHA; 2 = BETA ]
}
module main {
    io bus::WIRE2()
}
"#;
    let roles = flat_roles(OTHER, &|p| {
        p.ends_with(".bus.ALPHA") || p.ends_with(".bus.BETA")
    });
    assert_eq!(roles.len(), 2, "expected both bus members, got: {roles:?}");
    for (path, role) in &roles {
        assert_eq!(
            role, "none",
            "non-DC interface member {path} must stay Signal (no face pair declared), got {role:?}"
        );
    }
}

#[test]
fn probe_dc_member_names() {
    const SRC: &str = r#"
module main {
    in vin::DC(5V)
    out vout::DC(3.3V)
}
"#;
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = "/mcc/probe-dc.mc".to_string();
    mcc::mcc_load_from_string(&uri, SRC);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    let entry = McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2");
    for (_, e) in table.iter() {
        eprintln!("P2 {} kind={:?}", e.path, e.kind);
    }
}
