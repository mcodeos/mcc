// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U339 — the member-func shape law (2026-09-28 ruling, canon
//! `mcd/doc/vector/vec-dianlu.md` §7.7): a member-func call's connection
//! effect is decided solely by the shape of its evaluated result. The two
//! silent phantom faces the ruling abolishes:
//!
//! - a **scalar chain head** (`HEAD1 => CAP.Cap(_) -> [X, Y]`) used to
//!   half-bind the `[net1, net2]` formal and leave `net2` a bare literal that
//!   the body wired as the phantom net `_C1.net2`;
//! - a **scalar single argument** (`CAP(..).Cap(P)`) used to leave `net2`
//!   unbound so `cap.2` silently landed on an anonymous `_net0`.
//!
//! Both faces are deficit width mismatches (fewer actual lanes than formal
//! members): the boundary reports E4180 and — since the U339 landing — the
//! func body no longer runs on rejected bindings (the E4176 skip precedent).
//! The receiver itself stays built: an error does not block instantiation
//! (`mcrule.md` §11.6), it only withholds the body's wiring.
//!
//! The compliant column-vector head is locked as the positive control: one
//! instance bridging both lanes, head lane + tail member + cap pin on one
//! net per lane. The surplus actual (`[a, b, c]` against two slots) is
//! deliberately **not** re-locked here — it keeps building
//! (`noblock__surplus_elements_still_build`): the fork + zip clamp leaves
//! every member bound, so there is no phantom to abolish.
//!
//! Fixtures are self-contained (inline component, no system library) so the
//! locks cannot drift with the live `~/.mcode` root.

// Family naming `u339__{essence}` deliberately doubles the underscore so the
// grep-able family token stays separate.
#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::McIds;
use mcc::McURI;

/// Two-pin device with one indexed formal over two slots — the shape the
/// deficit family fills. Body mirrors the library `CAP.Cap`.
const CAP: &str = "component CAP(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";

/// `main` whose `func M()` body is the case under test. The calls sit inside
/// a func so the `=>` pre-closure face is reachable (the scalar-head family's
/// template, mirroring the pwrint `components.mc` form).
fn src(body: &str) -> String {
    format!(
        "{CAP}module main {{\n    io P\n    io Q\n    io X\n    io Y\n    io HEAD1\n    func M() {{\n{body}\n    }}\n}}\n"
    )
}

/// The names of the components the arena actually built for `main` — the
/// structural "was it built" read (see
/// `error_does_not_block_instantiation.rs` for why this is structural).
fn devices_of(src: &str, uri: &str) -> BTreeSet<String> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (tree, arena, store, _) =
        mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    mcc::TreeView::new(&arena, &store)
        .components(&tree)
        .map(|c| c.name.clone())
        .collect()
}

/// Diagnostic codes for `src`, in emit order with multiplicity. Built through
/// `mcc_build_flat` so the connection/net checks run (E4007's phase).
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &u, 1000);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

/// The connection-derived wiring of `src` — one entry per net, holding the
/// point paths on it.
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

fn has_cap_pin_net(nets: &[Vec<String>]) -> bool {
    nets.iter()
        .any(|net| net.iter().any(|p| p.contains("_C1.") || p.contains("_C2.")))
}

/// Scalar single argument: the deficit is reported and no phantom net is
/// produced — `cap.2` no longer silently lands on an anonymous `_net0`
/// (the abolished pwrint `main.mc` face).
#[test]
fn u339__scalar_single_arg_reports_width_error_without_phantom_net() {
    let src = src("        CAP(10).Cap(P)");
    let nets = nets_of(&src, "/mcc/u339-single-arg.mc");
    assert!(
        !has_cap_pin_net(&nets),
        "no cap pin may sit on a net after the deficit rejection; nets={nets:?}"
    );
    assert!(
        codes_of(&src, "/mcc/u339-single-arg.mc").contains(&4180),
        "the scalar single argument still reports the width error (E4180)"
    );
    assert_eq!(
        devices_of(&src, "/mcc/u339-single-arg.mc"),
        BTreeSet::from(["_C1".to_string()]),
        "the receiver stays built — an error does not block instantiation"
    );
}

/// Scalar chain head: the deficit is reported (E4180 at the call boundary;
/// the statement face independently reports the chain shape E4007) and the
/// body's phantom `_C1.net2` is no longer produced (the abolished pwrint
/// `components.mc` face).
#[test]
fn u339__scalar_chain_head_reports_width_error_without_phantom_net() {
    let src = src("        HEAD1 => CAP(10).Cap(_) -> [X, Y]");
    let nets = nets_of(&src, "/mcc/u339-scalar-head.mc");
    assert!(
        !has_cap_pin_net(&nets),
        "no cap pin may sit on a net after the deficit rejection; nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-scalar-head.mc");
    assert!(
        codes.contains(&4180),
        "the scalar head still reports the width error (E4180); codes={codes:?}"
    );
    assert!(
        codes.contains(&4007),
        "the chain statement face independently reports the shape mismatch \
         (E4007) — the general connection judgment, not a form-specific gate; \
         codes={codes:?}"
    );
    assert_eq!(
        devices_of(&src, "/mcc/u339-scalar-head.mc"),
        BTreeSet::from(["_C1".to_string()]),
        "the receiver stays built — an error does not block instantiation"
    );
}

/// Positive control: a compliant two-lane column-vector head still bridges —
/// one instance, its pins one per lane, each net carrying the head lane, the
/// tail member and the cap pin. The law's column arm must survive the
/// deficit error-ization untouched.
#[test]
fn u339__column_head_still_bridges_both_lanes() {
    let src = src("        [P, Q] => CAP(10).Cap(_) -> [X, Y]");
    let mut nets = nets_of(&src, "/mcc/u339-column-head.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("_C1.")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["P".to_string(), "X".to_string(), "_C1.1".to_string()],
            vec!["Q".to_string(), "Y".to_string(), "_C1.2".to_string()],
        ],
        "the column head bridges one instance across both lanes; nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-column-head.mc");
    assert!(
        !codes.contains(&4180) && !codes.contains(&4007),
        "the compliant column head is diagnosed by nothing; codes={codes:?}"
    );
}

// ── Return-face shape carrying (U339 ③ row/node half, landed part) ──

/// Two-pin tie whose pins carry a named pin group `NODE{P, N}` — the shape
/// the node-return family spells against.
const TIE_PINS: &str = "component TIE2 {\n    pins = [ io [1,2] = NODE{P, N} ]\n";

fn tie_main(func_body: &str, stmt: &str) -> String {
    format!(
        "{TIE_PINS}    func Pass([a, b]) {{\n{func_body}\n    }}\n}}\n\
         module main {{\n    io A\n    io B\n    io X\n    io Y\n    TIE2 t\n{stmt}\n}}\n"
    )
}

/// A node return (`return this{P | N}`) carries its sides: the chain legs
/// land per mouth — left leg on the left-port member (`P` = pin 1), right
/// leg on the right-port member (`N` = pin 2). Before the landing both
/// mouths answered the flattened right side, so the left leg could only
/// land by the receiver pass-through fallback.
#[test]
fn u339__node_return_lands_each_chain_leg_its_own_side() {
    let src = tie_main(
        "        return this{P | N}",
        "    X - t.Pass([X, Y]) - Y",
    );
    let mut nets = nets_of(&src, "/mcc/u339-node-return-inline.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("t.")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["X".to_string(), "t.1".to_string()],
            vec!["Y".to_string(), "t.2".to_string()],
        ],
        "left leg lands the left port, right leg the right port; nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-node-return-inline.mc");
    assert!(
        !codes.contains(&4007),
        "the per-side landing is legal; codes={codes:?}"
    );
}

/// One-lane tail: the tail mouth is the right mouth, so it answers the
/// right-port member (`N` = pin 2).
#[test]
fn u339__node_return_one_lane_tail_answers_the_right_side() {
    let src = tie_main(
        "        return this{P | N}",
        "    t.Pass([X, Y]) -> X",
    );
    let nets = nets_of(&src, "/mcc/u339-node-return-tail.mc");
    assert_eq!(
        nets,
        vec![vec!["X".to_string(), "t.2".to_string()]],
        "the tail mouth resolves the return's right side; nets={nets:?}"
    );
}

/// The degenerate (column) return keeps its flat pair-chaining untouched —
/// the comma spelling answers both mouths with the same member list, so the
/// two-lane trunk chains pair by position exactly as before the shape
/// carrying landed.
#[test]
fn u339__column_return_pair_chains_unchanged() {
    let src = tie_main(
        "        a - this - b\n        return NODE{P, N}",
        "    t.Pass([A, B]) -> [A, B]",
    );
    let mut nets = nets_of(&src, "/mcc/u339-column-return.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("t.")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["A".to_string(), "t.1".to_string()],
            vec!["B".to_string(), "t.2".to_string()],
        ],
        "the column return still pairs both lanes by position; nets={nets:?}"
    );
}

/// A node return against a two-lane trunk on a **declared-instance**
/// receiver still reports the shape mismatch: the per-lane ×N replication
/// builds a fresh component per lane, and a declared instance cannot be
/// cloned — replication is construction-face only (`w{1,2} => BOX(..).M(_)`,
/// the row locks below). This face therefore keeps its honest E4007 rather
/// than silently landing one wiring or the other.
#[test]
fn u339__node_return_two_lane_trunk_still_reports_shape_mismatch() {
    let src = tie_main(
        "        a - this - b\n        return this{P | N}",
        "    t.Pass([A, B]) -> [A, B]",
    );
    let nets = nets_of(&src, "/mcc/u339-node-return-trunk.mc");
    assert!(
        nets.iter().all(|net| !net.iter().any(|p| p.contains("t."))),
        "no tie pin may land when the receiver cannot be cloned; nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-node-return-trunk.mc");
    assert!(
        codes.contains(&4007),
        "the unclonable row face stays an honest E4007; codes={codes:?}"
    );
}

// ── Row-return replication (U339 ③, the shape law's row half) ──

/// Two-pin wires providing the lane trunks, and a two-pin box whose method
/// returns a bare two-face row — the spelling the shape law reads as a row.
const ROW_BOX: &str = "component WIRE2 {\n    pins = [\n        io [1, 2] = [A, B]\n    ]\n}\n\
     component BOX {\n    pins = [\n        io [1, 2] = [P, Q]\n    ]\n";

fn row_main(func_body: &str, stmt: &str) -> String {
    format!(
        "{ROW_BOX}    func RowRet([na, nb]) {{\n{func_body}\n    }}\n}}\n\
         module main {{\n    WIRE2 w\n    WIRE2 t\n{stmt}\n}}\n"
    )
}

/// The landed row half (vec-dianlu.md §7.7): a row return on a two-lane
/// trunk replicates per lane — two fresh components, lane k binding
/// (head lane k, tail lane k). The bare `{na | nb}` return is the row
/// spelling (the grammar's base-less two-face form), and the body's own
/// `this{P | Q}` wiring lands each component's pins on its lane pair.
#[test]
fn u339__row_return_replicates_per_lane_on_a_construction_receiver() {
    let src = row_main(
        "        na - this{P | Q} - nb\n        return {na | nb}",
        "    w{1,2} => BOX().RowRet(_) -> t{2,1}",
    );
    let mut nets = nets_of(&src, "/mcc/u339-row-replicate.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("BOX")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["_BOX1.1".to_string(), "w.1".to_string()],
            vec!["_BOX1.2".to_string(), "t.2".to_string()],
            vec!["_BOX2.1".to_string(), "w.2".to_string()],
            vec!["_BOX2.2".to_string(), "t.1".to_string()],
        ],
        "lane k gets its own component bound (head lane k, tail lane k); nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-row-replicate.mc");
    assert!(
        !codes.contains(&4007) && !codes.contains(&4180),
        "the compliant row return is diagnosed by nothing; codes={codes:?}"
    );
}

/// `return this` (implicit own face) reads as the two-pin entry/exit row,
/// so a body-less method on a construction receiver replicates the same
/// way — the chain wiring gives each fresh component its lane-pair pin
/// contacts. This was the face that sat silently zero-connected before the
/// row half landed.
#[test]
fn u339__implicit_this_row_return_replicates_per_lane() {
    let src = row_main("", "    w{1,2} => BOX().RowRet(_) -> t{2,1}");
    let mut nets = nets_of(&src, "/mcc/u339-row-this.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("BOX")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["_BOX1.1".to_string(), "w.1".to_string()],
            vec!["_BOX1.2".to_string(), "t.2".to_string()],
            vec!["_BOX2.1".to_string(), "w.2".to_string()],
            vec!["_BOX2.2".to_string(), "t.1".to_string()],
        ],
        "the implicit this row replicates per lane; nets={nets:?}"
    );
}

// ── Row-return ×N per-lane replication (U339 ③-b, construction callees) ──

/// Two-pin tie with a `NODE{P, N}` pin group, a scalar formal and a row
/// return — the construction the shape law's row arm replicates one instance
/// of per lane. `io R` (third lane) is declared for the scaling lock; the
/// two-lane locks simply leave it unwired.
const PULL: &str = "component RES(res::INT) {\n    pins = [ io [1,2] = NODE{P, N} ]\n    func Pull(net) {\n        net - this.1\n        return this{P | N}\n    }\n}\n";

fn pull_src(body: &str) -> String {
    format!(
        "{PULL}module main {{\n    io P\n    io Q\n    io R\n    io X\n    io Y\n    io Z\n    \
         func M() {{\n{body}\n    }}\n}}\n"
    )
}

/// The §7.7 row arm on a construction callee: a `1*2` return on a two-lane
/// trunk reads as one instance PER LANE, in series within its lane —
/// `[P, Q] => RES(10k).Pull(_) -> [X, Y]` builds the same circuit as the two
/// scalar statements. The column control above (`column_head_still_bridges`)
/// is the contrast: a column return bridges one instance across both lanes.
#[test]
fn u339__row_return_constructor_replicates_one_instance_per_lane() {
    let src = pull_src("        [P, Q] => RES(10k).Pull(_) -> [X, Y]");
    let mut nets = nets_of(&src, "/mcc/u339-row-fork.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("_R")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["P".to_string(), "_R1.1".to_string()],
            vec!["Q".to_string(), "_R2.1".to_string()],
            vec!["X".to_string(), "_R1.2".to_string()],
            vec!["Y".to_string(), "_R2.2".to_string()],
        ],
        "one instance per lane, in series within its lane; nets={nets:?}"
    );
    assert_eq!(
        devices_of(&src, "/mcc/u339-row-fork.mc"),
        BTreeSet::from(["_R1".to_string(), "_R2".to_string()]),
        "the row return materializes one construction per lane"
    );
    let codes = codes_of(&src, "/mcc/u339-row-fork.mc");
    assert!(
        !codes.contains(&4007) && !codes.contains(&4180),
        "the replicated row face is legal; codes={codes:?}"
    );
}

/// The replication scales with the trunk, not fixed at two: three head lanes
/// materialize three instances, each bound to its own lane's tail member.
#[test]
fn u339__row_return_fork_scales_to_three_lanes() {
    let src = pull_src("        [P, Q, R] => RES(10k).Pull(_) -> [X, Y, Z]");
    assert_eq!(
        devices_of(&src, "/mcc/u339-row-fork3.mc"),
        BTreeSet::from([
            "_R1".to_string(),
            "_R2".to_string(),
            "_R3".to_string()
        ]),
        "one construction per lane at any trunk width"
    );
    let codes = codes_of(&src, "/mcc/u339-row-fork3.mc");
    assert!(
        !codes.contains(&4007) && !codes.contains(&4180),
        "the three-lane replication is legal; codes={codes:?}"
    );
}

/// The per-lane copies inherit the ordinary binding law: a width-deficit
/// formal (the `[n1, n2]` Set against a per-lane scalar actual) reports its
/// E4180 once per copy, the receivers stay built, and the rejected bodies'
/// wiring is withheld — the head lanes never reach the instances, while the
/// exit mouth degrades to the instance terminals (the b4154 face).
#[test]
fn u339__row_return_fork_keeps_each_lane_width_honest() {
    let two = PULL.replace(
        "func Pull(net) {\n        net - this.1\n        return this{P | N}\n    }",
        "func Pull([n1, n2]) {\n        n1 - this - n2\n        return this{P | N}\n    }",
    );
    let src = format!(
        "{two}module main {{\n    io P\n    io Q\n    io X\n    io Y\n    \
         func M() {{\n        [P, Q] => RES(10k).Pull(_) -> [X, Y]\n    }}\n}}\n"
    );
    let codes = codes_of(&src, "/mcc/u339-row-fork-deficit.mc");
    assert_eq!(
        codes.iter().filter(|c| **c == 4180).count(),
        2,
        "one deficit report per lane copy; codes={codes:?}"
    );
    assert!(
        !codes.contains(&4007),
        "the shape itself forks cleanly — only the width deficit reports; codes={codes:?}"
    );
    assert_eq!(
        devices_of(&src, "/mcc/u339-row-fork-deficit.mc"),
        BTreeSet::from(["_R1".to_string(), "_R2".to_string()]),
        "an error does not block instantiation"
    );
    let mut nets = nets_of(&src, "/mcc/u339-row-fork-deficit.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("_R")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["X".to_string(), "_R1.2".to_string()],
            vec!["Y".to_string(), "_R2.2".to_string()],
        ],
        "the rejected body's wiring is withheld; the head lanes stay off the \
         instances; nets={nets:?}"
    );
}

// ── Row-face label scope (U339 ③, the returned face's own labels) ──

/// A bare row return whose sides name the body's own nets (`{n | ob}`): per
/// lane copy, the exit-mouth tail must land THAT copy's body net — the same
/// instance-prefix pass body statements get. Without it every lane's tail
/// welds onto one caller-scope label net (`ob` shared across lanes) while
/// the copies' own exit pins strand on private nets — a silent cross-lane
/// short. The based spelling (`this{P | N}`) is the contrast: its sides
/// already carry the substituted instance prefix.
#[test]
fn u339__row_face_bare_labels_land_their_own_copy() {
    let src = format!(
        "{ROW_BOX}    func RowRet(n) {{\n        n - this.1\n        ob - this.2\n        return {{n | ob}}\n    }}\n}}\n\
         module main {{\n    WIRE2 w\n    WIRE2 t\n    [w.1, w.2] => BOX().RowRet(_) -> [t.2, t.1]\n}}\n"
    );
    let mut nets = nets_of(&src, "/mcc/u339-row-face-labels.mc");
    nets.retain(|net| net.iter().any(|p| p.contains("BOX")));
    nets.sort();
    assert_eq!(
        nets,
        vec![
            vec!["_BOX1.1".to_string(), "w.1".to_string()],
            vec![
                "_BOX1.2".to_string(),
                "_BOX1.ob".to_string(),
                "t.2".to_string()
            ],
            vec!["_BOX2.1".to_string(), "w.2".to_string()],
            vec![
                "_BOX2.2".to_string(),
                "_BOX2.ob".to_string(),
                "t.1".to_string()
            ],
        ],
        "each lane's tail lands its own copy's body net; nets={nets:?}"
    );
    let codes = codes_of(&src, "/mcc/u339-row-face-labels.mc");
    assert!(
        !codes.contains(&4007) && !codes.contains(&4180),
        "the compliant bare-label row face is diagnosed by nothing; codes={codes:?}"
    );
}
