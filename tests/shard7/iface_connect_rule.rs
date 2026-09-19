// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the interface connect rule (U128 §1.2 steps 1–2, design doc
//! `mcd/doc/net/interface-connect-rule-design.md` v0.2, landed in step ②b):
//!
//! * E4120 `IFACE_CROSS_FAMILY_CONNECT` — two endpoints of *different*
//!   interface families are connected.
//! * E4121 `IFACE_ROLE_INCOMPATIBLE` — two roles of one family are connected
//!   but neither names the other in its `peer` attribute.
//!
//! Both are check-only diagnostics: the connection itself is still made
//! (build-so-it's-findable principle), which the same-role cell asserts by
//! also checking the net partition. A side that carries no
//! role (an interface without a role table) skips step 2 silently — the
//! roleless path is the positional law of step ②a, not an error.
//!
//! These fixtures are rule-discriminating: the pre-②b engine accepted every
//! cell quietly (no family/role metadata was consulted at the join site), so
//! each cell's expectation separates the ②b engine from its predecessor.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// An interface with two mutual-peer roles. Each role names the other in its
/// `peer` attribute, so `Host <-> Dev` is the one wired pairing.
const MUTUAL_IFACE: &str = r#"
interface LINK(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Host { peer = Dev }
    role Dev  { peer = Host }
}

component HOSTDEV
{
    pins = [
        [1,2] = IF::LINK(Host)
    ]
}

component DEVDEV
{
    pins = [
        [1,2] = IF::LINK(Dev)
    ]
}
"#;

/// A second interface — a different family for the cross-family cell. Same
/// member count and member names as LINK on purpose: family identity is the
/// interface *name*, not the shape, so a shape-equal but differently named
/// family must still be rejected by step 1.
const OTHER_FAMILY: &str = r#"
interface LYNX(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Host { peer = Dev }
    role Dev  { peer = Host }
}

component LYNXDEV
{
    pins = [
        [1,2] = IF::LYNX(Dev)
    ]
}
"#;

/// An interface with no role table at all — both sides of its connects are
/// roleless, which must skip the role check silently.
const ROLELESS_IFACE: &str = r#"
interface BARE
{
    pins = [
        [1,2] = [A, B]
    ]
}

component BAREDEV
{
    pins = [
        [1,2] = IF::BARE()
    ]
}
"#;

/// Codes that are build-info, not a verdict (same set the vector-oracle family
/// tolerates).
fn benign(c: u32) -> bool {
    matches!(c, 5641 | 5642 | 5643 | 5054)
}

/// Build `main` with the body statement `body` and return (non-benign codes
/// sorted, net partition of the top module). The partition is normalized to a
/// sorted list of sorted member lists: net *names* are synthesized, so the
/// claim is about the grouping of points.
fn build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{MUTUAL_IFACE}{OTHER_FAMILY}{ROLELESS_IFACE}module main {{\n    HOSTDEV ha\n    HOSTDEV hb\n    DEVDEV da\n    LYNXDEV la\n    BAREDEV ba\n    BAREDEV bb\n{body}\n}}\n"
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

/// Only the two codes under test; anything else is another rule's business and
/// must not be masked by this fixture.
fn rule_codes(codes: &[u32]) -> Vec<u32> {
    codes
        .iter()
        .filter(|c| **c == 4120 || **c == 4121)
        .copied()
        .collect()
}

/// Step 2, happy path: `Host` on the left, `Dev` on the right, each role's
/// `peer` names the other — quiet. This is the cell the pre-②b engine also
/// passed; it guards against a check that fires on *any* role-bearing connect.
#[test]
fn iface_conn__mutual_peer_roles_are_quiet() {
    let (codes, _nets) = build("    ha.IF -> da.IF", "/mcc/iface-conn-mutual.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "mutual peers (Host peer=Dev, Dev peer=Host) must be quiet; got {codes:?}"
    );
}

/// Step 1: `LINK` and `LYNX` are different families even though their member
/// tables are shape-equal — family identity is the interface name. E4120.
#[test]
fn iface_conn__cross_family_is_e4120() {
    let (codes, _nets) = build("    ha.IF -> la.IF", "/mcc/iface-conn-family.mc");
    assert_eq!(
        rule_codes(&codes),
        vec![4120],
        "shape-equal but differently named families must raise E4120; got {codes:?}"
    );
}

/// Step 2: `Host` against `Host` — no side's `peer` names the other side's
/// role. E4121, and the connection is still made (check-only: the wires are
/// suspect but the endpoints must remain findable on one net).
#[test]
fn iface_conn__same_role_is_e4121_and_still_connects() {
    let (codes, nets) = build("    ha.IF -> hb.IF", "/mcc/iface-conn-role.mc");
    assert_eq!(
        rule_codes(&codes),
        vec![4121],
        "Host<->Host (peer=Dev on both) is not a mutual pair: E4121; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["ha.1".to_string(), "hb.1".to_string()]),
        "the check never blocks instantiation: pin 1 of both sides must land on one net; got {nets:?}"
    );
}

/// The roleless path: an interface with no role table skips step 2 silently —
/// the connect falls through to the positional law of step ②a, no E4120/E4121.
#[test]
fn iface_conn__roleless_interface_is_quiet() {
    let (codes, nets) = build("    ba.IF -> bb.IF", "/mcc/iface-conn-bare.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "a roleless interface must not raise the role check; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["ba.1".to_string(), "bb.1".to_string()],
            vec!["ba.2".to_string(), "bb.2".to_string()],
        ],
        "roleless connect still pairs positionally; got {nets:?}"
    );
}
