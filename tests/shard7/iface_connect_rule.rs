// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Locks the interface connect rule (U128 §1.2 steps 1–2, design doc
//! `interface-connect-rule-design.md` v0.2, landed in step ②b):
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
    func j(S) { S + IF }
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

/// The `+`/§5.1 parallel shape must be guarded too: its wiring is emitted by
/// the engine's second site (`vexpr_wire_parallel`), which deliberately does
/// not go through `create_connection`. Host against Host → E4121, reported
/// once (both row nets dedupe on the statement offset), and both pins still
/// merge onto one net each (check-only).
#[test]
fn iface_conn__parallel_join_is_e4121_once() {
    let (codes, nets) = build("    ha.IF + hb.IF", "/mcc/iface-conn-parallel.mc");
    assert_eq!(
        rule_codes(&codes),
        vec![4121],
        "the parallel wiring site must run the same rule; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["ha.1".to_string(), "hb.1".to_string()]),
        "check-only: pin 1 of both sides still merges onto one net; got {nets:?}"
    );
}

/// A mutual-peer pair joined through a func body (`j(S) { S + IF }`, called
/// `ha.j(da.IF)`) stays quiet — the func boundary is the same engine, not a
/// second rule. The net assertion is the non-vacuity guard: quiet means
/// nothing only if the body join demonstrably happened (the actual merges
/// with `ha`'s own port on one net). (The cross-role func call is covered by
/// the parallel cell: the body join `S + IF` IS the `+` wiring.)
#[test]
fn iface_conn__func_body_mutual_pair_is_quiet() {
    let (codes, nets) = build("    ha.j(da.IF)", "/mcc/iface-conn-funcbody.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "a func-body join of mutual peers must be quiet; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["da.1".to_string(), "ha.1".to_string()]),
        "the body join must actually merge the actual with the host port (guards against a \
         vacuous quiet where the func never ran); got {nets:?}"
    );
}

/// U128 step ④ — definition-side self-check cells. The fixture below
/// replicates the *former* library shape (D8: `UART.RS485`'s Repeater
/// role, deleted from mcode/ifs/uart.mc 2026-09-21): the repeater names
/// its peers (`peer = [Master,
/// Slave]`) but neither peer names it back, and its member table is 6
/// members against the peers' 3. D8 (ruled 2026-09-20) settled this shape as
/// **legal one-to-many relay semantics**, so the definition-side self-check
/// must stay quiet on it — the multi-peer set exempts the pair from both the
/// mutuality and the width sub-check. Family naming keeps the
/// `{family}__{essence}` discipline.
const D8_RS485: &str = r#"
interface RS485D8(role)
{
    role Master {
        pins = [
            1 = A
            2 = B
            3 = GND
        ]
        peer = Slave
    }
    role Slave {
        pins = [
            1 = A
            2 = B
            3 = GND
        ]
        peer = Master
    }
    role Repeater {
        pins = [
            1 = A_IN
            2 = B_IN
            3 = GND_IN
            4 = A_OUT
            5 = B_OUT
            6 = GND_OUT
        ]
        peer = [Master, Slave]
    }
}

component D8TERM
{
    pins = [
        [1,2,3] = IF::RS485D8(Master)
    ]
}

component D8TRM
{
    pins = [
        [1,2,3] = IF::RS485D8(Slave)
    ]
}

component D8REP
{
    pins = [
        [1,2,3,4,5,6] = IF::RS485D8(Repeater)
    ]
}
"#;

/// Build `main` from `fixture` (interface definitions) plus `main_body`
/// (statements inside `module main`). Same return shape as [`build`].
fn build_iface(fixture: &str, main_body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!("{fixture}module main {{\n{main_body}\n}}\n");
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

/// The D8 fixture with its three components instantiated, plus optional
/// statements `body`.
fn build_d8(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    build_iface(
        D8_RS485,
        &format!("    D8TERM dm\n    D8TRM ds\n    D8REP rp\n{body}"),
        uri,
    )
}

/// Definition-side self-check, D8 cell (ruled 2026-09-20): the Repeater
/// shape is **legal one-to-many relay semantics** — `peer = [Master, Slave]`
/// is a multi-peer set, so the pair is exempt from both the mutuality
/// (E5508) and the width (E5509) sub-check, and the definition loads quiet.
/// Before the ruling this same fixture raised both codes as a "peer
/// disease"; the former `UART.RS485` library shape (deleted 2026-09-21)
/// is the authority that it is authoring intent, not a defect.
#[test]
fn iface_def__d8_repeater_relay_semantics_is_quiet_at_definition_load() {
    let (codes, _nets) = build_d8("", "/mcc/iface-def-d8-relay.mc");
    assert!(
        !codes.contains(&5508),
        "a relay declaration must not raise the not-mutual self-check; got {codes:?}"
    );
    assert!(
        !codes.contains(&5509),
        "a relay declaration must not raise the peer-width self-check; got {codes:?}"
    );
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "no connection statement exists, so no join-site code may fire; got {codes:?}"
    );
}

/// Negative controls for the D8 exemption: with no relay side in sight, the
/// self-check keeps firing. `Alpha.peer = Beta` while `Beta` declares no
/// `peer` attribute at all — not named back (E5508) — and 3 members against
/// 2 (E5509). Single-peer pairs are judged exactly as before the ruling.
const PEER_NEG: &str = r#"
interface PeerNeg(role)
{
    role Alpha {
        pins = [
            1 = A
            2 = B
            3 = GND
        ]
        peer = Beta
    }
    role Beta {
        pins = [
            1 = A
            2 = B
        ]
    }
}
"#;

#[test]
fn iface_def__single_peer_non_mutual_and_width_still_fire() {
    let (codes, _nets) = build_iface(PEER_NEG, "", "/mcc/iface-def-peer-neg.mc");
    assert!(
        codes.contains(&5508),
        "a single-peer pair with no named-back peer must still raise E5508; got {codes:?}"
    );
    assert!(
        codes.contains(&5509),
        "a single-peer pair with unequal widths must still raise E5509; got {codes:?}"
    );
}

/// The whole-port D8 statement (`rp.IF -> dm.IF`, 6 members against 3) is
/// caught by the width axis — E4007, per §3.2 row 3 — and is NOT silent. The
/// join-site role check is unreachable behind the shape rejection on this
/// shape; the peer disease itself is named by the definition-side cell above.
#[test]
fn iface_conn__d8_whole_port_statement_is_rejected_by_width_not_silently() {
    let (codes, _nets) = build_d8("    rp.IF -> dm.IF", "/mcc/iface-conn-d8-repeater.mc");
    assert!(
        codes.contains(&4007),
        "the 6-vs-3 whole-port connect must be visibly rejected by the shape layer; got {codes:?}"
    );
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "width rejection precedes the join, so no join-site role code fires here; got {codes:?}"
    );
}

/// An equal-width variant isolates the join-site role axis: `Tap` names
/// `Master` but Master only names `Slave` back, all tables 3 members wide.
/// The join is shape-legal, so it reaches the engine and the mutual-pair
/// test fires E4121 — the connect-side half of the D8 disease, on its own.
const D8_EQUAL: &str = r#"
interface RS485EQ(role)
{
    role Master {
        pins = [
            1 = A
            2 = B
            3 = GND
        ]
        peer = Slave
    }
    role Slave {
        pins = [
            1 = A
            2 = B
            3 = GND
        ]
        peer = Master
    }
    role Tap {
        pins = [
            1 = A
            2 = B
            3 = GND
        ]
        peer = [Master]
    }
}

component EQDEV
{
    pins = [
        [1,2,3] = IF::RS485EQ(Master)
    ]
}

component EQTAP
{
    pins = [
        [1,2,3] = IF::RS485EQ(Tap)
    ]
}
"#;

/// Build `main` from the equal-width variant. Same return shape as [`build`].
fn build_d8_equal(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!("{D8_EQUAL}module main {{\n    EQDEV dm\n    EQTAP tp\n{body}\n}}\n");
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

/// The connect-side half of the D8 disease, isolated from the width axis:
/// equal widths, shape-legal join, `Tap` names `Master` but Master names only
/// `Slave` back — the mutual-pair test fails at the engine join, E4121, and
/// the connection is still made (check-only).
#[test]
fn iface_conn__d8_equal_width_not_named_back_is_e4121() {
    let (codes, nets) = build_d8_equal("    tp.IF -> dm.IF", "/mcc/iface-conn-d8-equal.mc");
    assert_eq!(
        rule_codes(&codes),
        vec![4121],
        "equal-width not-named-back join must fail the mutual-pair test; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["dm.1".to_string(), "tp.1".to_string()]),
        "check-only: the join must still land on one net; got {nets:?}"
    );
}

/// The control: Master against Slave is the one wired pairing in the D8
/// family — both sides name each other, so the same fixture must stay quiet.
/// Guards against a check that fires on any multi-role family.
#[test]
fn iface_conn__d8_master_slave_mutual_pair_is_quiet() {
    let (codes, nets) = build_d8("    dm.IF -> ds.IF", "/mcc/iface-conn-d8-master-slave.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "Master<->Slave (each names the other) must stay quiet on the D8 fixture; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["dm.1".to_string(), "ds.1".to_string()]),
        "the mutual connect must actually merge pin 1 of both sides; got {nets:?}"
    );
}

// U133 phase 1 (interface-member-config-design.md §3): the direction word on
// an adoption row (`out [1,2] = IF::DRX(Tx)`) is the carrier for per-member
// direction. Phase 1 judges exactly one matrix cell — out↔out is a drive
// fight (E5511). in↔in is E4103's territory (it fires in the flatten stage,
// hence the `dir_flat` helper); the bidir cells are phase 2; a side without a
// declared direction word skips the cell (the D9 discipline).

/// One family, six adoption shapes. Tx/Rx are mutual peers like LINK; the
/// components differ only in the direction word (or its absence) on the pin
/// row and in which role they adopt.
const DIR_IFACE: &str = r#"
interface DRX(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Tx { peer = Rx }
    role Rx { peer = Tx }
}

component OUTDEV
{
    pins = [
        out [1,2] = IF::DRX(Tx)
    ]
}

component OUTDEV2
{
    pins = [
        out [1,2] = IF::DRX(Rx)
    ]
}

component INDEV
{
    pins = [
        in [1,2] = IF::DRX(Rx)
    ]
}

component INDEV2
{
    pins = [
        in [1,2] = IF::DRX(Tx)
    ]
}

component IODEV
{
    pins = [
        io [1,2] = IF::DRX(Rx)
    ]
}

component NODEV
{
    pins = [
        [1,2] = IF::DRX(Rx)
    ]
}
"#;

/// Build `main` with the DIR_IFACE fixture and `body`, keeping only codes
/// outside the benign set. Same normalization as `build`.
fn build_dir(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{DIR_IFACE}module main {{\n    OUTDEV o1\n    OUTDEV2 o2\n    INDEV i1\n    INDEV2 i2\n    IODEV g1\n    NODEV n1\n{body}\n}}\n"
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

/// The in↔in cell through the flatten stage: the net-level no-driver check
/// (E4103) only runs inside `mcc_build_flat`, so `build_with_nets` alone
/// cannot see it. Returns the non-benign code list.
fn dir_flat(body: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{DIR_IFACE}module main {{\n    OUTDEV o1\n    OUTDEV2 o2\n    INDEV i1\n    INDEV2 i2\n    IODEV g1\n    NODEV n1\n{body}\n}}\n"
    );
    let u = McURI::from("/mcc/iface-dir-flat.mc");
    mcc::mcc_load_from_string(&u, &src);
    mcc::mcc_build_flat(&McIds::from("main"), &u, 1000).expect("flat build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();
    codes
}

/// out↔out: two declared push-pull outputs wired against each other — E5511,
/// and per the build-so-it's-findable principle the connection itself is
/// still made.
#[test]
fn u133__out_out_is_e5511_and_still_connects() {
    let (codes, nets) = build_dir("    o1.IF -> o2.IF", "/mcc/iface-dir-out-out.mc");
    assert_eq!(
        codes,
        vec![5511],
        "exactly the drive-fight code; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["o1.1".to_string(), "o2.1".to_string()]),
        "the connection must still merge pin 1 of both sides; got {nets:?}"
    );
}

/// out↔in: the driven input — quiet, and still one net.
#[test]
fn u133__out_in_is_quiet() {
    let (codes, nets) = build_dir("    o1.IF -> i1.IF", "/mcc/iface-dir-out-in.mc");
    assert!(
        !codes.contains(&5511) && !codes.contains(&4120) && !codes.contains(&4121),
        "no direction or role complaint; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["i1.1".to_string(), "o1.1".to_string()]),
        "driver and load share one net; got {nets:?}"
    );
}

/// in↔in: two inputs, no driver — E4103's cell, not 5511's. Phase 1 must not
/// shadow the net-level check.
#[test]
fn u133__in_in_is_e4103_territory_not_5511() {
    let codes = dir_flat("    i1.IF -> i2.IF");
    assert!(
        !codes.contains(&5511),
        "in↔in is not a direction conflict; got {codes:?}"
    );
    assert!(
        codes.contains(&4103),
        "two inputs and no driver is the no-driver check's cell; got {codes:?}"
    );
}

/// out↔io: the bidir cells are quiet (ruling — bidir is the neutral direction).
#[test]
fn u133__out_io_stays_quiet_in_phase_1() {
    let (codes, _nets) = build_dir("    o1.IF -> g1.IF", "/mcc/iface-dir-out-io.mc");
    assert!(
        !codes.contains(&5511),
        "the bidir cells are phase 2; got {codes:?}"
    );
}

/// One side undeclared (`NODEV`'s row carries no direction word): the cell is
/// skipped entirely — no declaration, no inference.
#[test]
fn u133__one_side_undeclared_skips() {
    let (codes, nets) = build_dir("    o1.IF -> n1.IF", "/mcc/iface-dir-undeclared.mc");
    assert!(
        !codes.contains(&5511),
        "a side without a declared direction skips the cell; got {codes:?}"
    );
    assert!(
        nets.contains(&vec!["n1.1".to_string(), "o1.1".to_string()]),
        "the connection itself is unaffected; got {nets:?}"
    );
}

// ── U128 §6.2 step 4 residual: the three positive pairing cells ──
//
// The cells above lock the *diagnostic* face of the rule (which code fires,
// which stays silent). The rule's actual claim is about *which pin lands on
// which* — the design doc's step 4 asks for the three minimal examples whose
// net partition IS the verdict (§1.5.1: "aligned by name" and "crossed" are
// two outcomes written by the same kind of table, never two rules). Each
// fixture mirrors a real library family table-for-table (T1 plus the role
// tables, `mcode/ifs/uart.mc` / `spi.mc` grammar verbatim).

/// The RS485 shape: both roles' tables agree in names *and* order, so pairing
/// by position and pairing by name produce the same wires. Alone this cell
/// cannot separate the two laws — its value is the aligned half of the trio
/// and the positive result the diagnostic-only cells never asserted.
const ALIGNED_FAMILY: &str = r#"
interface ALGN(role)
{
    pins = [
        1 = A, "Bus line A"
        2 = B, "Bus line B"
    ]
    role Master {
        name = "ALGN Master"
        pins = [
            1 = A, "Bus line A"
            2 = B, "Bus line B"
        ]
        peer = Slave
    }
    role Slave {
        name = "ALGN Slave"
        pins = [
            1 = A, "Bus line A"
            2 = B, "Bus line B"
        ]
        peer = Master
    }
}

component ALGA
{
    pins = [
        [1,2] = IF::ALGN(Master)
    ]
}

component ALGB
{
    pins = [
        [1,2] = IF::ALGN(Slave)
    ]
}
"#;

/// The UART.TTL shape: the two roles' tables are name-swapped —
/// `DCE{1=TX, 2=RX}` against `DTE{1=RX, 2=TX}` — so position and name
/// disagree on **every** wire. The crossing is *declared*: DTE writes RX
/// first, and that is why the wire crosses. A matcher that repaired by name
/// would pair DCE pin 1 (TX) with DTE pin 2 (TX) and produce `{xa.1, xb.2}`;
/// the positional law pairs pin 1 with pin 1. This is the cell that kills
/// name-first at the interface-port surface.
const CROSSED_FAMILY: &str = r#"
interface XING(role)
{
    pins = [
        1 = TX, "Transmit"
        2 = RX, "Receive"
    ]
    role DCE {
        name = "XING DCE"
        pins = [
            1 = TX, "Transmit"
            2 = RX, "Receive"
        ]
        peer = DTE
    }
    role DTE {
        name = "XING DTE"
        pins = [
            1 = RX, "Receive"
            2 = TX, "Transmit"
        ]
        peer = DCE
    }
}

component XA
{
    pins = [
        [1,2] = IF::XING(DCE)
    ]
}

component XB
{
    pins = [
        [1,2] = IF::XING(DTE)
    ]
}
"#;

/// The SPI data shape: positions 1–2 agree by name (CS, SCLK), positions 3–4
/// cross (MISO↔SO, MOSI↔SI) — one family carrying §1.5.1's both outcomes at
/// once, so neither an always-align nor an always-cross reading can pass.
const PARTIAL_FAMILY: &str = r#"
interface PART(role)
{
    pins = [
        1 = CS, "Chip Select"
        2 = SCLK, "Serial Clock"
        3 = MISO, "Master In Slave Out"
        4 = MOSI, "Master Out Slave In"
    ]
    role Master {
        name = "PART Master"
        pins = [
            1 = CS, "Chip Select"
            2 = SCLK, "Serial Clock"
            3 = MISO, "Master In Slave Out"
            4 = MOSI, "Master Out Slave In"
        ]
        peer = Slave
    }
    role Slave {
        name = "PART Slave"
        pins = [
            1 = CS, "Chip Select"
            2 = SCLK, "Serial Clock"
            3 = SO, "Slave Out"
            4 = SI, "Slave In"
        ]
        peer = Master
    }
}

component PM
{
    pins = [
        [1,2,3,4] = IF::PART(Master)
    ]
}

component PS
{
    pins = [
        [1,2,3,4] = IF::PART(Slave)
    ]
}
"#;

/// Build `main` over the three pairing fixtures with all six devices
/// instantiated; same normalization as `build`. The partition is path-only on
/// purpose: at the device-pin face the wire truth *is* the pin pairing (the
/// role tables put ordinal k on pin k, so a name-first repair would flip the
/// crossed cells to `{xa.1, xb.2}`). The member-name face is a different,
/// unresolved surface (probe 2026-09-20: module-port boundary points are
/// labeled from the T1 table, never from the role-local names) and is not
/// locked here.
fn pairing_build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{ALIGNED_FAMILY}{CROSSED_FAMILY}{PARTIAL_FAMILY}module main {{\n    \
         ALGA ma\n    ALGB sb\n    XA xa\n    XB xb\n    PM pm\n    PS ps\n{body}\n}}\n"
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

/// The RS485 cell: aligned tables pair positionally, and each wire stays on
/// its own pin number — the positive result behind the quiet cells above.
#[test]
fn iface_pair__aligned_tables_land_pin_on_pin() {
    let (codes, nets) = pairing_build("    ma.IF -> sb.IF", "/mcc/iface-pair-aligned.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "mutual peers must be quiet; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["ma.1".to_string(), "sb.1".to_string()],
            vec!["ma.2".to_string(), "sb.2".to_string()],
        ],
        "aligned tables: ordinal k lands on ordinal k, pin k on pin k; got {nets:?}"
    );
}

/// The UART cell: the name-swapped tables must cross the wires *because the
/// tables say so* — pin 1 meets pin 1 even though the two members there carry
/// different names (TX against RX). The name-repair counterfactual is
/// `{xa.1, xb.2}` (TX meeting TX); if this partition ever grows it, a
/// name-first path has returned.
#[test]
fn iface_pair__crossed_tables_cross_because_declared() {
    let (codes, nets) = pairing_build("    xa.IF -> xb.IF", "/mcc/iface-pair-crossed.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "crossing is declared by the tables, not an error; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["xa.1".to_string(), "xb.1".to_string()],
            vec!["xa.2".to_string(), "xb.2".to_string()],
        ],
        "DCE pin 1 (TX) meets DTE pin 1 (RX): ordinal k with ordinal k, never name with name. \
         The name-repair counterfactual is {{xa.1, xb.2}} (TX meeting the DTE's TX on pin 2); \
         got {nets:?}"
    );
}

/// The SPI cell: positions 1–2 align by name while 3–4 cross — one connect
/// statement producing §1.5.1's both outcomes, so the partition pins the
/// whole table down member for member.
#[test]
fn iface_pair__partial_family_aligns_and_crosses_in_one_statement() {
    let (codes, nets) = pairing_build("    pm.IF -> ps.IF", "/mcc/iface-pair-partial.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "mutual peers must be quiet; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["pm.1".to_string(), "ps.1".to_string()],
            vec!["pm.2".to_string(), "ps.2".to_string()],
            vec!["pm.3".to_string(), "ps.3".to_string()],
            vec!["pm.4".to_string(), "ps.4".to_string()],
        ],
        "CS and SCLK align, MISO meets SO and MOSI meets SI — all by table order; got {nets:?}"
    );
}

// The single-pin-id adoption shapes: the point is port-level (`a.IFX`, no
// pin segment) and the single-pin interface arm registers the port under its
// own name precisely so `iface_endpoint_of_point` can resolve the adoption to
// its family — the kept cells (family / role / direction) judge these
// adoptions like any other. The electrical-axis cells that once rode the same
// entry (`@drive`/`@pull`, E5512/E5513) are retired with their keys.

/// A single-pin-id adoption reaches the kept direction cell: out↔out through
/// port-level points is E5511, same as the pin-level shape — the port-level
/// resolution must not silently skip the rule (GPIO included). The trailing
/// unconnected-pin reports ride along (the build is not blocked by them);
/// the lock retains the direction code.
#[test]
fn u133p2__single_pin_adoption_reaches_the_direction_cell() {
    let _lock = common::lock();
    common::reset();
    let src = r#"
interface ONE(role)
{
    pins = [
        [1] = [A]
    ]
    role P { peer = Q }
    role Q { peer = P }
}
component SA
{
    pins = [
        out [1] = IFX::ONE(P)
    ]
}
component SB
{
    pins = [
        out [1] = IFX::ONE(Q)
    ]
}
module main {
    SA dev1
    SB dev2
    dev1.IFX -> dev2.IFX
}
"#;
    let u = McURI::from("/mcc/iface-single-dir.mc");
    mcc::mcc_load_from_string(&u, src);
    mcc::mcc_build_flat(&McIds::from("main"), &u, 1000).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !benign(*c))
        .collect();
    codes.sort_unstable();
    codes.retain(|c| *c == 5511);
    assert_eq!(
        codes,
        vec![5511],
        "the single-pin adoption must reach the direction cell via the port registration; got {codes:?}"
    );
}

// ── U137: the boundary member label rides the table that supplies the
// ordinal. The family table (T1) below spells third-party names M1/M2 on
// purpose: a boundary labeled from T1 surfaces `U.M1` even though both ports
// declare roles whose tables spell TX/RX differently — the probe of
// 2026-09-20 (log/9.20.u128-trio-locks.md §3) caught exactly that. The law
// after the fix (interface-connect rule §1.5.1): a member name is that role's
// local term for ordinal k, so each boundary point is spelled by its own
// side's table — the wires stay ordinal-paired, names are labels. ──

const BNDLBL_FAMILY: &str = r#"
interface XNG(role)
{
    pins = [
        1 = M1, "third-party label"
        2 = M2, "third-party label"
    ]
    role DCE {
        name = "XNG DCE"
        pins = [
            1 = TX, "Transmit"
            2 = RX, "Receive"
        ]
        peer = DTE
    }
    role DTE {
        name = "XNG DTE"
        pins = [
            1 = RX, "Receive"
            2 = TX, "Transmit"
        ]
        peer = DCE
    }
}

module MA
{
    io U::XNG(DCE)
}

module MB
{
    io U::XNG(DTE)
}

module MC
{
    io W::XNG()
}

module MD
{
    io W::XNG()
}
"#;

/// Build `main` over the boundary-label fixture; same normalization as
/// `build`. Here the path partition IS the member-name face: module-port
/// boundary points carry their member segment in the path, so the crossed
/// names are directly observable.
fn bndlbl_build(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{BNDLBL_FAMILY}module main {{\n    \
         MA xa\n    MB xb\n    MC mc\n    MD md\n{body}\n}}\n"
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

/// Each side spells its own lane: xa's boundary is DCE-local (TX, RX), xb's is
/// DTE-local (RX, TX), and ordinal k still meets ordinal k — so the partition
/// shows crossed names on purpose (TX meeting RX is the declared crossing).
/// The T1 repair counterfactual is any `U.M1`/`U.M2` path: a boundary spelled
/// from the family table means the role tables stopped supplying the labels.
#[test]
fn iface_lbl__boundary_members_follow_the_role_table_not_t1() {
    let (codes, nets) = bndlbl_build("    xa.U -> xb.U", "/mcc/iface-bndlbl-roles.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "mutual peers must be quiet; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["xa.U.RX".to_string(), "xb.U.TX".to_string()],
            vec!["xa.U.TX".to_string(), "xb.U.RX".to_string()],
        ],
        "each boundary spells its own role's local names; the T1 counterfactual \
         is any xa.U.M1 / xb.U.M2 path; got {nets:?}"
    );
}

/// The control: a port that declares no role has no local table to ride, so
/// the label falls back to the family table (T1) — on both sides alike.
#[test]
fn iface_lbl__roleless_port_falls_back_to_the_family_table() {
    let (codes, nets) = bndlbl_build("    mc.W -> md.W", "/mcc/iface-bndlbl-roleless.mc");
    assert_eq!(
        rule_codes(&codes),
        Vec::<u32>::new(),
        "roleless connects skip the role check silently; got {codes:?}"
    );
    assert_eq!(
        nets,
        vec![
            vec!["mc.W.M1".to_string(), "md.W.M1".to_string()],
            vec!["mc.W.M2".to_string(), "md.W.M2".to_string()],
        ],
        "no role declared: the family table labels both boundaries; got {nets:?}"
    );
}

// U139 — the two §3.2 rows the landing batches never reached: criterion 4
// (E4122, endpoint count vs the declared `topology`) and criterion 5 (E4123,
// attribute compatibility, judged only where both sides declare it).
//
// E4122 reads the family definition's `topology` attribute: a family that
// declares `point to point` fits exactly two endpoints on one net. E4123
// reads the per-endpoint attribute face of the connect judgment — the
// selected role's own attribute table (family equality already pins both
// definitions to one object, so definition-level attributes cannot disagree;
// `peer` is the pairing mechanism and `name` is display text, so neither is
// an operating-point declaration). Severity follows the §3.2 table: E4122
// Error, E4123 Warning (declared-then-judged).

const TOPO_IFACE: &str = r#"
interface P2P(role)
{
    topology = "point to point"
    pins = [
        [1,2] = [A, B]
    ]
    role Tx { peer = Rx }
    role Rx { peer = Tx }
}

interface MANY(role)
{
    topology = "multi-point"
    pins = [
        [1,2] = [A, B]
    ]
    role Tx { peer = Rx }
    role Rx { peer = Tx }
}

interface VLT(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role Lo
    {
        peer = Hi
        voltage = [1.8V]
    }
    role Hi
    {
        peer = Lo
        voltage = [5V]
    }
}

interface VEQ(role)
{
    pins = [
        [1,2] = [A, B]
    ]
    role A5
    {
        peer = B5
        voltage = [5V]
    }
    role B5
    {
        peer = A5
        voltage = [5V]
    }
}

component PDEV { pins = [ [1,2] = IF::P2P(Tx) ] }
component QDEV { pins = [ [1,2] = IF::P2P(Rx) ] }
component RDEV { pins = [ [1,2] = IF::P2P(Rx) ] }
component MDEV { pins = [ [1,2] = IF::MANY(Tx) ] }
component NDEV { pins = [ [1,2] = IF::MANY(Rx) ] }
component ODEV { pins = [ [1,2] = IF::MANY(Rx) ] }
component LODEV { pins = [ [1,2] = IF::VLT(Lo) ] }
component HIDEV { pins = [ [1,2] = IF::VLT(Hi) ] }
component LADEV { pins = [ [1,2] = IF::VEQ(A5) ] }
component LBDEV { pins = [ [1,2] = IF::VEQ(B5) ] }
"#;

/// Only the two codes this section locks.
fn rule139_codes(codes: &[u32]) -> Vec<u32> {
    codes
        .iter()
        .filter(|c| **c == 4122 || **c == 4123)
        .copied()
        .collect()
}

/// Build `main` with the TOPO_IFACE fixture and `body`. Same normalization as
/// `build`.
fn build_topo(body: &str, uri: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{TOPO_IFACE}module main {{\n    PDEV p1\n    QDEV q1\n    RDEV r1\n    MDEV m1\n    NDEV n1\n    ODEV o1\n    LODEV lo\n    HIDEV hi\n    LADEV la\n    LBDEV lb\n{body}\n}}\n"
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

/// Criterion 4: three `P2P` endpoints on one `+` net — the family declares
/// `point to point`, so the third endpoint is one too many. E4122, once.
#[test]
fn iface_topo__point_to_point_family_rejects_a_third_endpoint() {
    let (codes, _nets) = build_topo("    p1.IF + q1.IF + r1.IF", "/mcc/iface-topo-three.mc");
    assert_eq!(
        rule139_codes(&codes),
        vec![4122],
        "a point-to-point family with three endpoints on one net must raise E4122 exactly once; got {codes:?}"
    );
}

/// Control: two endpoints fit the declared topology — quiet.
#[test]
fn iface_topo__two_endpoints_fit_point_to_point() {
    let (codes, _nets) = build_topo("    p1.IF + q1.IF", "/mcc/iface-topo-two.mc");
    assert_eq!(
        rule139_codes(&codes),
        Vec::<u32>::new(),
        "two endpoints are exactly what point to point declares; got {codes:?}"
    );
}

/// Control: `multi-point` families take n endpoints — the attribute is judged
/// only where declared, and a declared `multi-point` never fires E4122.
#[test]
fn iface_topo__multi_point_family_takes_three_endpoints_quietly() {
    let (codes, _nets) = build_topo("    m1.IF + n1.IF + o1.IF", "/mcc/iface-topo-multi.mc");
    assert_eq!(
        rule139_codes(&codes),
        Vec::<u32>::new(),
        "a multi-point family fits any endpoint count; got {codes:?}"
    );
}

/// Criterion 5: both selected roles declare `voltage` and the declared value
/// sets share nothing — E4123 (Warning severity per the §3.2 table).
#[test]
fn iface_attr__disjoint_declared_voltage_sets_warn_e4123() {
    let (codes, _nets) = build_topo("    lo.IF -> hi.IF", "/mcc/iface-attr-disjoint.mc");
    assert_eq!(
        rule139_codes(&codes),
        vec![4123],
        "roles declaring the same attribute with disjoint value sets must raise E4123; got {codes:?}"
    );
}

/// Control: both roles declare `voltage` with the same value — the sets share
/// a member, the pair is compatible, quiet.
#[test]
fn iface_attr__equal_declared_voltage_is_quiet() {
    let (codes, _nets) = build_topo("    la.IF -> lb.IF", "/mcc/iface-attr-equal.mc");
    assert_eq!(
        rule139_codes(&codes),
        Vec::<u32>::new(),
        "identical declared value sets are compatible; got {codes:?}"
    );
}
