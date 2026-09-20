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
/// replicates the *real* library shape (D8: `UART.RS485`'s Repeater role,
/// mcode/ifs/uart.mc): the repeater names its peers (`peer = [Master,
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
/// disease"; the real `UART.RS485` library shape is the authority that it
/// is authoring intent, not a defect.
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

// ---------------------------------------------------------------------------
// U133 phase 1 (interface-member-config-design.md §3): the direction word on
// an adoption row (`out [1,2] = IF::DRX(Tx)`) is the carrier for per-member
// direction. Phase 1 judges exactly one matrix cell — out↔out is a drive
// fight (E5511). in↔in is E4103's territory (it fires in the flatten stage,
// hence the `dir_flat` helper); the bidir cells are phase 2; a side without a
// declared direction word skips the cell (the D9 discipline).
// ---------------------------------------------------------------------------

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
    assert_eq!(codes, vec![5511], "exactly the drive-fight code; got {codes:?}");
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

/// out↔io: bidir cells are phase 2 (od/pull deferred) — quiet in phase 1.
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
