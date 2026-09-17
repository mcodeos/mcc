// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcrule.md` §11.6 — **an error does not block the build** (2026-09-16
//! ruling, CIMP §1 U51 withdrawn after being landed and reverted the same
//! day): a component reporting an **error** at instantiation is **kept**.
//!
//! The rationale is findability, not permissiveness: a part that stays in the
//! netlist shows up in listings and diagrams and is reported as unwired
//! (E4112/E4116), whereas a dropped part is invisible everywhere — the
//! defect its statement already reported is the only trace left of it.
//!
//! What this file pins, and why each case is a discriminator:
//!
//! 1. **a failed bind still builds** — `RX(10).Pullup(SPI)` (missing `n2`), a
//!    surplus argument (E4176), and a `_` receiver: the chain's own receiver
//!    is built and reported unwired. Neither a bind failure nor an empty
//!    connection list may silently remove it;
//! 2. **a fatal width mismatch still builds** — `[SPI{A, B}, GND]` against
//!    two formal slots (E4180). This is the case that pays the ruling's
//!    price: the residue expands the library body against a half-bound formal
//!    set and drags that body's own shape error out (E4007). The noise is
//!    accepted, and asserted here so the price stays visible rather than
//!    being rediscovered;
//! 3. **a surplus of single-lane elements stays pairable** — `[SPI, GND, VDD]`
//!    (`param-prefix-design.md` §3.2, conclusion 1): every member pairs and
//!    only the surplus is left over, so **both fork branches are built**,
//!    each wire-complete;
//! 4. **legitimately unwired is not an error at all** — a declared-only part
//!    with no connection (including one whose *written* name starts with `_`)
//!    is built, and reported unwired: that report is the mechanism this whole
//!    ruling is about;
//! 5. **a satisfiable chain is built** — the counterpart, so that "kept"
//!    cannot be mistaken for "kept only when broken".
//!
//! **Built is read structurally** — the arena's `Device` nodes — never from a
//! connection-derived listing: those render what is *wired*, so a
//! built-but-unwired part reads as absent there (`mcc show instances` reports
//! `count: 0` for such a part). The diagnostics corroborate: E4112/E4116 name
//! exactly the artifact a connection-derived view cannot see.

// Family naming `noblock__{essence}` deliberately doubles the underscore so
// the grep-able family token stays separate.
#![allow(non_snake_case)]

use crate::common;

use std::collections::BTreeSet;

use mcc::McIds;
use mcc::McURI;

/// Two-pin device with **two scalar formals** — the shape the bind-failure
/// family fills. Body mirrors the probe fixtures: wire the two nets through
/// the device.
const RX: &str = "component RX(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

/// Two-pin device with **one indexed formal over two slots** — the shape the
/// width family fills. The trailing `return` mirrors the library `CAP.Cap`
/// body, whose expansion against a half-bound formal set is what drags E4007
/// out of a kept residue.
const CAP: &str = "component CAP(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";

/// A device with **no func at all** — a BOM / declared-only part.
const PART: &str = "component PART {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// `main` with a one-lane `SPI` — the bind-failure family's template.
fn src_scalar(body: &str) -> String {
    format!(
        "{RX}module main {{\n    io SPI\n    io VDD\n    io GND\n    func M() {{\n{body}\n    }}\n}}\n"
    )
}

/// `main` with the two-member bus `SPI{A, B}` — the width family's template.
fn src_bus(body: &str) -> String {
    format!(
        "{CAP}module main {{\n    io SPI{{A, B}}\n    io VDD\n    io GND\n    func M() {{\n{body}\n    }}\n}}\n"
    )
}

/// `main` with a **one-member** bus. §11.6's whole-value fill needs a declared
/// bus of **more than one** member, so a one-member bus occupying the argument
/// table is a single leaf — one lane against two slots, not a fill.
fn src_bus1(body: &str) -> String {
    format!(
        "{CAP}module main {{\n    io SPI{{A}}\n    io VDD\n    io GND\n    func M() {{\n{body}\n    }}\n}}\n"
    )
}

/// A module body at **module level** (no `func`) — the declaration family,
/// whose parts are never wired.
fn src_decl(body: &str) -> String {
    format!("{PART}module main {{\n    io SPI\n{body}\n}}\n")
}

/// The names of the components the arena actually built for `main` — the
/// structural "was it built" read: `TreeView` walks the arena's `Device`
/// children. Every fixture here declares its call inside the root module's
/// `func`, and the arena has no function-scope node kind, so the root walk
/// sees all of them.
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

/// Diagnostic codes for `src`, **in emit order, with multiplicity** — a
/// deduped list cannot show "reported once per fork branch".
///
/// Built through `mcc_build_flat` (pass1 + pass2 + net checks): the wiring
/// reports this file corroborates with (E4112/E4116) are emitted by the net
/// checks, and `mcc_build_with_nets` deliberately runs none.
fn codes_of(src: &str, uri: &str) -> Vec<u32> {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &u, 1000);
    mcc::mcc_diagnose_all().iter().map(|d| d.code).collect()
}

fn count_code(src: &str, uri: &str, code: u32) -> usize {
    codes_of(src, uri).iter().filter(|c| **c == code).count()
}

/// The connection-derived wiring of `src` — one entry per net, holding the
/// point paths on it. Read only to show a *surviving* artifact is wire-complete.
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

/// §11.6: an error on **the component's own bind** does not remove it. Each
/// spelling here fails its own bind — and each keeps its receiver, which is
/// then reported unwired (E4112/E4116). That report is the whole point: it is
/// how the broken part stays findable in a listing or a diagram.
#[test]
fn noblock__failed_bind_still_builds_the_receiver() {
    for body in [
        // Missing required formal `n2`.
        "        RX(10).Pullup(SPI)",
        // Too many arguments: 3 against 2 slots.
        "        RX(10).Pullup(SPI, VDD, GND)",
        // `_` receiver: the prefix form hands the statement net to `n1` only.
        "        SPI => RX(10).Pullup(_)",
    ] {
        let src = src_scalar(body);
        assert_eq!(
            devices_of(&src, "/mcc/noblock-bind.mc"),
            BTreeSet::from(["_RX1".to_string()]),
            "a component whose own bind failed is still built; body={body:?}"
        );
        let codes = codes_of(&src, "/mcc/noblock-bind.mc");
        assert!(
            codes.contains(&4176),
            "the failed bind is still reported (E4176); body={body:?} codes={codes:?}"
        );
        assert!(
            codes.contains(&4112) && codes.contains(&4116),
            "and the kept receiver is reported unwired — the reason the ruling \
             keeps it at all; body={body:?} codes={codes:?}"
        );
    }
}

/// §11.6: a **fatal** width mismatch keeps the residue too. `[SPI{A, B}, GND]`
/// is an argument table of two top-level elements, the first owning two lanes,
/// so the members cannot each hand exactly one lane to a slot; the deficit
/// spellings (`[FOO]` undeclared, `[SPI]` a one-member bus — a single leaf)
/// are the same family from the other side.
///
/// The price, asserted rather than hidden: the kept residue expands the
/// library `Cap` body (`net1 - this - net2` plus `return [net1, net2]`)
/// against a half-bound formal set. **Measured**: the library body's shape
/// error (E4007) appears only for the *over-wide* spelling — the one where a
/// scalar formal receives a multi-lane bundle (`SPI{A, B}` into `net1`), so
/// the `return` is handed 3 lanes for 2 slots. The two *deficit* spellings
/// leave a formal unfilled instead, which is the missing-formal path (E4180
/// plus downstream E3179 / E5641 / E5642) and drags no E4007. The flag column
/// keeps that distinction explicit rather than flattening it into one claim.
#[test]
fn noblock__fatal_width_mismatch_still_builds_the_residue() {
    let cases: [(fn(&str) -> String, &str, bool); 3] = [
        // Group element against a scalar slot: 2 lanes where 1 may go.
        (src_bus, "        CAP(10).Cap([SPI{A, B}, GND])", true),
        // Undeclared name: one leaf against two slots.
        (src_bus, "        CAP(10).Cap([FOO])", false),
        // One-member bus: one leaf against two slots.
        (src_bus1, "        CAP(10).Cap([SPI])", false),
    ];
    for (src_of, body, drags_e4007) in cases {
        let src = src_of(body);
        let devs = devices_of(&src, "/mcc/noblock-width.mc");
        assert_eq!(
            devs,
            BTreeSet::from(["_C1".to_string()]),
            "an unpairable actual still builds its residue; body={body:?}"
        );
        let codes = codes_of(&src, "/mcc/noblock-width.mc");
        assert!(
            codes.contains(&4180),
            "the fatal mismatch is still reported (E4180); body={body:?} codes={codes:?}"
        );
        assert_eq!(
            codes.contains(&4007),
            drags_e4007,
            "the over-wide residue drags the library body's shape error out \
             (E4007) — the accepted cost of keeping it; a deficit residue \
             takes the missing-formal path instead and drags none; \
             body={body:?} codes={codes:?}"
        );
    }
}

/// conclusion 1 (`param-prefix-design.md` §3.2): the surplus is **kept**. `SPI`
/// occupies the argument table on its z-axis, so the statement forks per lane
/// — and each branch pairs its own two members against the two slots. Only a
/// leftover single-lane element remains, which the ruling does not punish, so
/// **both branches build**, each wire-complete.
///
/// This is the discriminator: without it, family 2's rule would degenerate
/// into "a width mismatch drops the part", which would drop exactly the case
/// the ruling spares.
#[test]
fn noblock__surplus_elements_still_build() {
    let body = "        CAP(10).Cap([SPI, GND, VDD])";
    let src = src_bus(body);
    let devs = devices_of(&src, "/mcc/noblock-surplus.mc");
    assert_eq!(
        devs,
        BTreeSet::from(["_C1".to_string(), "_C2".to_string()]),
        "both fork branches are complete, so both are built; body={body:?}"
    );
    assert_eq!(
        count_code(&src, "/mcc/noblock-surplus.mc", 4180),
        2,
        "and each branch reports the mismatch it saw; body={body:?}"
    );

    // `_C1` takes SPI.A, `_C2` takes SPI.B; each also lands on GND — 2 of 2
    // pins wired, i.e. "the part itself is complete".
    let nets = nets_of(&src, "/mcc/noblock-surplus.mc");
    for (dev, lane) in [("_C1", "SPI.A"), ("_C2", "SPI.B")] {
        let on = |net: &str| {
            nets.iter().any(|pts| {
                pts.iter()
                    .any(|p| p == net && pts.iter().any(|q| q.starts_with(dev)))
            })
        };
        assert!(
            on(lane) && on("GND"),
            "{dev} must be wired to both {lane} and GND; nets={nets:?}"
        );
    }
}

/// Blocker 1 of the U51 discussion, which survives the reversal unchanged:
/// "no connection at all" alone is **not** an error. A part that is declared
/// and never wired is legitimate (BOM-only / NC parts), so it is built and
/// kept — and reported as unwired, which is how it stays visible.
///
/// The second spelling is the same part under a **written** name that starts
/// with `_`. A criterion keyed on the `_` prefix — or on any instance name —
/// fails here, which is why the (now withdrawn) retraction criterion had to
/// be structural to begin with.
#[test]
fn noblock__unwired_declaration_is_kept() {
    for (body, name) in [("    PART p1", "p1"), ("    PART _p1", "_p1")] {
        let src = src_decl(body);
        assert_eq!(
            devices_of(&src, "/mcc/noblock-decl.mc"),
            BTreeSet::from([name.to_string()]),
            "an unwired declaration is not an error and must be built; body={body:?}"
        );
        let codes = codes_of(&src, "/mcc/noblock-decl.mc");
        assert!(
            codes.contains(&4112) && codes.contains(&4116),
            "and it must be reported as unwired, not silently dropped; \
             body={body:?} codes={codes:?}"
        );
    }
}

/// The counterpart of the families above: the same chain spelling with a
/// **satisfiable** bind builds its artifact, wire-complete and without the
/// unwired report — so "kept" cannot be misread as "kept only when broken".
#[test]
fn noblock__satisfiable_chain_is_built() {
    let body = "        RX(10).Pullup(SPI, VDD)";
    let src = src_scalar(body);
    assert_eq!(
        devices_of(&src, "/mcc/noblock-built.mc"),
        BTreeSet::from(["_RX1".to_string()]),
        "a chain whose bind succeeds builds its receiver; body={body:?}"
    );
    let codes = codes_of(&src, "/mcc/noblock-built.mc");
    assert!(
        !codes.contains(&4176) && !codes.contains(&4180),
        "and reports no bind error; body={body:?} codes={codes:?}"
    );
}
