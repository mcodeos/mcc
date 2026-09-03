// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §11.4 / vector-pipeline Phase 2.2: GAP2 — the global "net statement
//! materializes ≥1 physical pin" invariant (E4057 NET_DROPPED_STATEMENT
//! upgraded from its local `NAME[k]` alias site to the flattened net table).
//!
//! A module-level net whose endpoints ALL fail to resolve to a registered
//! physical entry (component pin / port) — every point is an unresolved
//! structured ghost or an unregistered path — materializes **0 physical pins**
//! and is dropped by `flatten_nets`. GAP2 reports it once, at the first
//! point's wiring site.
//!
//! The E3137↔GAP2 domain split (no double-reporting): the two criteria are
//! mutually exclusive on the same net. E3137 (pass1) owns the naming-layer
//! fact "a structured ghost is referenced exactly once"; GAP2 (pass2 flat)
//! owns the materialization fact "a net produced 0 pins". A 0-pin net is
//! E4057; a stub net that kept ≥1 physical point whose ghost is single-use is
//! E3137 (the orphaned pin is the 41xx unconnected checks' domain). Neither
//! fires for the other's case.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::McIds;

/// Build `main` (pass1 diagnostics only) and return the diagnostic codes.
fn build_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/gap2-pass1.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build(&McIds::from("main"), &uri);
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v
}

/// Build `main` and flatten to the InstTable (pass2 + flat net checks logged).
fn build_flat_codes(src: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/gap2-flat.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let mut v: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    v.sort_unstable();
    v
}

fn count(codes: &[u32], code: u32) -> usize {
    codes.iter().filter(|c| **c == code).count()
}

/// ── GAP2: a 0-pin net reports E4057, without double-reporting E3137 ────────
/// `uC.ADC.P -> a.b.c.d` twice: both endpoints are 3+/4-segment structured
/// ghosts whose bases are declared nowhere. Each ghost is referenced twice, so
/// the pass1 single-use E3137 stays quiet (multi-use). The merged net's points
/// resolve to nothing at flatten (the last-dot→slash fallback cannot match the
/// `uC/ADC.P` ghost labels), so it materializes 0 pins → exactly one E4057.
#[test]
fn mat_gap2__zero_pin_net_reports_gap2_once() {
    let src = "module main {\n    func main() {\n        uC.ADC.P -> a.b.c.d\n        uC.ADC.P -> a.b.c.d\n    }\n}";
    let codes = build_flat_codes(src);
    assert_eq!(
        count(&codes, 4057),
        1,
        "0-pin net reports GAP2 exactly once; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 3137),
        0,
        "multi-use ghosts are not single-use (no E3137 alongside GAP2); got {codes:?}"
    );
}

/// ── Domain split: a single-use ghost net keeps E3137, never E4057 ─────────
/// `uC.ADC.P -> VDD`: the net keeps 1 physical point (the VDD port), so it is
/// a stub, not 0-pin — GAP2 stays quiet. E3137 fires (single-use ghost).
#[test]
fn mat_gap2__single_use_ghost_stub_is_e3137_not_gap2() {
    let src = "module main {\n    io VDD\n    func main() {\n        uC.ADC.P -> VDD\n    }\n}";
    let codes = build_flat_codes(src);
    assert_eq!(
        count(&codes, 3137),
        1,
        "single-use ghost warns E3137; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 4057),
        0,
        "stub net kept ≥1 pin, no GAP2; got {codes:?}"
    );
}

/// ── Domain split: ghost → real pin is a stub, not 0-pin ───────────────────
/// `uC.ADC.P -> r1.1`: r1.1 is a real registered pin, so the net has 1
/// physical point → no GAP2. The single-use ghost is E3137; the orphaned pin
/// r1.1 is the 41xx unconnected checks' domain (not asserted here).
#[test]
fn mat_gap2__ghost_to_real_pin_is_stub_not_gap2() {
    let src = "component R {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\nmodule main {\n    io VDD\n    io GND\n    R r1\n    uC.ADC.P -> r1.1\n}";
    let codes = build_flat_codes(src);
    assert_eq!(
        count(&codes, 3137),
        1,
        "single-use ghost warns E3137; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 4057),
        0,
        "1-pin net is not 0-pin, no GAP2; got {codes:?}"
    );
}

/// ── Domain split: two resolvable ghost labels are a net, not 0-pin ────────
/// `FOO.BAR -> BAZ.QUX` (2-segment ghosts): flatten's last-dot→slash fallback
/// resolves both to their registered ghost labels, so the net keeps 2 points —
/// no GAP2. Each ghost is single-use, so E3137 fires twice (one per ghost).
#[test]
fn mat_gap2__two_resolvable_ghost_labels_are_not_gap2() {
    let src = "module main {\n    func main() {\n        FOO.BAR -> BAZ.QUX\n    }\n}";
    let codes = build_flat_codes(src);
    assert_eq!(
        count(&codes, 3137),
        2,
        "two single-use ghosts warn E3137; got {codes:?}"
    );
    assert_eq!(
        count(&codes, 4057),
        0,
        "ghost labels resolved into a net, no GAP2; got {codes:?}"
    );
}

/// ── Real nets never trigger GAP2 ──────────────────────────────────────────
#[test]
fn mat_gap2__real_net_is_quiet() {
    let src = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\nmodule main {\n    io VDD\n    io GND\n    CAP c(1)\n    c.Cap([VDD, GND])\n}";
    let codes = build_flat_codes(src);
    assert_eq!(
        count(&codes, 4057),
        0,
        "real net materializes pins; got {codes:?}"
    );
}

/// ── GAP2 (module half): a phantom-only connection reports E4057 ───────────
/// `res[1:2] -> led[3:4]` with both bases undeclared: both endpoints carry
/// `[`, so `NetPoint::new` quarantines them as `@_phantom_<N>`, the union-find
/// entry (`add_connection`) keeps zero physical points, and the statement is
/// otherwise completely silent — no net, no E3137, no 41xx. GAP2 reports the
/// 0-pin drop at the wiring site.
#[test]
fn mat_gap2__phantom_only_connection_reports_gap2() {
    let src = "module main {\n    func main() {\n        res[1:2] -> led[3:4]\n    }\n}";
    let codes = build_flat_codes(src);
    assert_eq!(
        count(&codes, 4057),
        1,
        "phantom-only connection reports GAP2 once; got {codes:?}"
    );
}

/// ── The local `NAME[k]` alias site still fires E4057 (pass1) ──────────────
/// `[GPIO2] -> VDD` (single-element square bracket on an unknown name): the
/// pass1 indexed-alias site in mc_phrase.rs reports NET_DROPPED_STATEMENT on
/// the plain `mcc_build` path (no flat build needed) — the local half of GAP2
/// is unchanged. (The index form `GPIO[2]` on a fully-undeclared name instead
/// quarantines as a phantom and is the module-half GAP2's domain.)
#[test]
fn mat_gap2__namek_alias_local_site_still_fires_e4057() {
    let src = "module main {\n    io VDD\n    func main() {\n        [GPIO2] -> VDD\n    }\n}";
    let codes = build_codes(src);
    assert_eq!(
        count(&codes, 4057),
        1,
        "local single-bracket alias drop reports E4057; got {codes:?}"
    );
}

/// ── Domain split: a 1-pin stub (phantom + real port) is not 0-pin ─────────
/// `res[1:2] -> VDD`: the phantom `res[1:2]` is quarantined but VDD is a real
/// port, so the connection keeps 1 physical point — not 0-pin, no GAP2. The
/// phantom reference drops silently (no E3137 — it is a literal label, not a
/// structured ghost); the orphaned port is the 41xx unconnected domain.
#[test]
fn mat_gap2__phantom_to_real_port_is_not_gap2() {
    let src = "module main {\n    io VDD\n    func main() {\n        res[1:2] -> VDD\n    }\n}";
    let codes = build_flat_codes(src);
    assert_eq!(
        count(&codes, 4057),
        0,
        "1-pin connection is not 0-pin, no GAP2; got {codes:?}"
    );
}
