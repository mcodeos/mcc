// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U153: a module port declared `io P::Iface()` takes its members from the
//! interface's role-less conductor table — a **positional** set (`_(<pinid>)`
//! spellings, U148 ③; conductor-view R-CV1: ordinal k is the wire).
//!
//! Two pre-fix failure faces, both from grafting name-valued alias spaces onto
//! the positional set:
//!
//! 1. P2-2 boundary-formal registration copied the paired peer's member NAMES
//!    onto the port's bus (`peer_port_members` → `ensure_bus`), and
//! 2. `run_component_method` registered the component's same-named interface
//!    port under the **bare** name, merging the component's member view into
//!    the module port's.
//!
//! Either graft unions two alias spaces under one name: every fold expansion
//! of the port then walks 2N members, N of which fail the declared-set check
//! (E3181 ×N) and the chain fold sees unequal member spaces (E4005). The fix
//! skips both grafts when the module port declares its own member set; the
//! fold zips lane k to lane k.
//!
//! Discrimination needs ≥2× width: 10 lanes, peer pin ids written in a
//! scrambled order so lane identity can never ride pin-id order (U152's
//! dictionary-order lesson). Same in-process harness as `u152_body_face_decl_order`.

#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

#[derive(Debug, Default)]
struct Probe {
    /// (sorted point paths) per connection, in creation order
    nets: Vec<Vec<String>>,
    diags: Vec<(u32, String)>,
}

fn probe(source: &str) -> Probe {
    let _lock = common::lock();
    let system_root = mcc::cli::datadir::data_root();
    mcc::mcc_clear_workspace();
    mcc::mcc_set_system_root(&system_root);
    mcc::mcc_init();

    let uri: McURI = "/mcc/u153-anon-port.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let mut p = Probe::default();
    if let Ok((inst, _arena, _store, _net_store)) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri)
    {
        for c in inst.connections.iter() {
            let mut pts: Vec<String> = c.points.iter().map(|x| x.path.clone()).collect();
            pts.sort();
            p.nets.push(pts);
        }
    }
    p.diags = mcc::mcc_diagnose(&uri)
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    p
}

/// A 10-lane interface: role-less view = 10 anonymous lanes (ordinal = wire);
/// Master names M01..M10 in wire order, Slave names S01..S10 in wire order.
fn iface10() -> String {
    let mut s = String::from("interface SPI10(role)\n{\n    pins = [\n");
    for i in 1..=10 {
        s.push_str(&format!("        {i} = _\n"));
    }
    s.push_str("    ]\n    role Master {\n        pins = [\n");
    for i in 1..=10 {
        s.push_str(&format!("            {i} = M{i:02}\n"));
    }
    s.push_str("        ]\n        peer = Slave\n    }\n    role Slave {\n        pins = [\n");
    for i in 1..=10 {
        s.push_str(&format!("            {i} = S{i:02}\n"));
    }
    s.push_str("        ]\n        peer = Master\n    }\n}\n");
    s
}

/// Master-side component: pin ids written in a scrambled wire order — wire k
/// lands on pin `M_PINS[k]`; role names the lanes M01..M10.
const M_PINS: [usize; 10] = [5, 3, 1, 10, 7, 9, 2, 8, 6, 4];
/// Slave-side component: a different scramble — wire k lands on `S_PINS[k]`.
const S_PINS: [usize; 10] = [2, 9, 4, 6, 1, 8, 3, 10, 5, 7];

fn components() -> String {
    format!(
        "{}\ncomponent UCM\n{{\n    name = \"UCM\"\n    pins = [\n        [{}] = SPI10::SPI10(Master)\n    ]\n}}\n\ncomponent FLH\n{{\n    name = \"FLH\"\n    pins = [\n        [{}] = SPI10::SPI10(Slave)\n    ]\n}}\n",
        iface10(),
        M_PINS.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "),
        S_PINS.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "),
    )
}

/// The anonymous port + same-name func formal must zip lane k to lane k:
/// wire k pairs FLH's `S_PINS[k]` with UCM's `M_PINS[k]`, and NO diagnostic
/// fires. Pre-fix, the two name grafts produced E3181 ×10 + E4005 and no
/// correct pairing.
#[test]
fn lock_u153__anon_port_func_formal_zips_lanes() {
    let src = format!(
        "{}\nmodule DEV()\n{{\n    io SPI10::SPI10()\n    UCM uc\n\n    func loadFlash(SPI10)\n    {{\n        SPI10 + uc.SPI10\n    }}\n}}\n\nmodule main()\n{{\n    DEV dev\n    FLH flh\n    dev.loadFlash(flh.SPI10)\n}}\n",
        components()
    );
    let p = probe(&src);
    let fatal: Vec<_> = p
        .diags
        .iter()
        .filter(|(c, _)| *c == 3181 || *c == 4005)
        .collect();
    assert!(
        fatal.is_empty(),
        "U153 graft faces must be silent, got {fatal:?}"
    );
    // One net per wire: FLH lane k pin ↔ UCM lane k pin.
    for k in 0..10 {
        let mut want = vec![
            format!("dev.uc.{}", M_PINS[k]),
            format!("flh.{}", S_PINS[k]),
        ];
        want.sort();
        assert!(
            p.nets.contains(&want),
            "wire {} missing: expected net {want:?} in {:?}",
            k + 1,
            p.nets
        );
    }
}

/// The U153 grammar slot: the interface declare rides in a **mixed** port row
/// (`io SPI10::SPI10(), UART0, I2C1`) instead of a row of its own. The parser
/// emits one flat MCAST_NET_PORTS child list (OPD / DECLARE siblings) and the
/// semantic layer dispatches by child type, so pairing stays lane-correct and
/// the graft gates still hold (pre-U153 this row died E2082 at the `::`).
#[test]
fn lock_u153__mixed_port_row_parses_and_zips_lanes() {
    let src = format!(
        "{}\nmodule DEV()\n{{\n    io SPI10::SPI10(), UART0, I2C1\n    UCM uc\n\n    func loadFlash(SPI10)\n    {{\n        SPI10 + uc.SPI10\n    }}\n}}\n\nmodule main()\n{{\n    DEV dev\n    FLH flh\n    dev.loadFlash(flh.SPI10)\n}}\n",
        components()
    );
    let p = probe(&src);
    let fatal: Vec<_> = p
        .diags
        .iter()
        .filter(|(c, _)| {
            *c == 3181 || *c == 4005 || *c == 2082
        })
        .collect();
    assert!(
        fatal.is_empty(),
        "mixed port row must parse and pair silently, got {fatal:?}"
    );
    for k in 0..10 {
        let mut want = vec![
            format!("dev.uc.{}", M_PINS[k]),
            format!("flh.{}", S_PINS[k]),
        ];
        want.sort();
        assert!(
            p.nets.contains(&want),
            "wire {} missing: expected net {want:?} in {:?}",
            k + 1,
            p.nets
        );
    }
}

/// A hand-written named member access on the anonymous port stays an error:
/// the positional set is the port's only member identity, and the U153 gates
/// must not open a name-valued side door.
#[test]
fn lock_u153__named_member_on_anon_port_stays_undeclared() {
    let src = format!(
        "{}\nmodule main()\n{{\n    io SPI10::SPI10()\n    UCM uc\n    SPI10.M01 - uc.3\n}}\n",
        components()
    );
    let p = probe(&src);
    assert!(
        p.diags.iter().any(|(c, _)| *c == 3181),
        "named member on an anonymous port must stay E3181, got {:?}",
        p.diags
    );
}

/// The named member-group port keeps its pre-U153 path: the port declares
/// name-valued members, P2-2 registration is a no-op union of equal sets, and
/// the func-formal fold pairs by the shared names.
#[test]
fn lock_u153__named_port_func_formal_unchanged() {
    let members: Vec<String> = (1..=10).map(|i| format!("A{i:02}")).collect();
    let uc_names: Vec<String> = M_PINS.iter().map(|i| format!("A{i:02}")).collect();
    let mut s = String::new();
    s.push_str(&iface10());
    // UCM adopts role-less with explicit member names A01..A10 in wire order.
    s.push_str(&format!(
        "component UCM\n{{\n    name = \"UCM\"\n    pins = [\n        [{}] = SPI10{{{}}}::SPI10()\n    ]\n}}\n",
        M_PINS.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "),
        uc_names.join(", "),
    ));
    s.push_str(&format!(
        "component FLH\n{{\n    name = \"FLH\"\n    pins = [\n        [{}] = SPI10{{{}}}::SPI10()\n    ]\n}}\n",
        S_PINS.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "),
        S_PINS.iter().map(|i| format!("A{i:02}")).collect::<Vec<_>>().join(", "),
    ));
    // FLH's wire-k member is A<S_PINS[k]> — written in wire order.
    s.push_str(&format!(
        "module DEV()\n{{\n    io SPI10{{{}}}\n    UCM uc\n\n    func loadFlash(SPI10)\n    {{\n        SPI10 + uc.SPI10\n    }}\n}}\n",
        members.join(", ")
    ));
    s.push_str("module main()\n{\n    DEV dev\n    FLH flh\n    dev.loadFlash(flh.SPI10)\n}\n");
    let p = probe(&s);
    let fatal: Vec<_> = p
        .diags
        .iter()
        .filter(|(c, _)| *c == 3181 || *c == 4005)
        .collect();
    assert!(
        fatal.is_empty(),
        "named-port path must stay silent, got {fatal:?}"
    );
    // Wire k: FLH lane k pin ↔ UCM lane k pin (the fold zips lanes; the
    // declared names are the ports' display identity, not the pairing key).
    for k in 0..10 {
        let mut want = vec![
            format!("dev.uc.{}", M_PINS[k]),
            format!("flh.{}", S_PINS[k]),
        ];
        want.sort();
        assert!(
            p.nets.contains(&want),
            "wire {} missing: expected net {want:?} in {:?}",
            k + 1,
            p.nets
        );
    }
}
