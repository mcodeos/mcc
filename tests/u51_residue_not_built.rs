// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U51 (`mcrule.md` §11.6, "an error blocks the build" - a global principle,
//! CIMP §1 U51): a component
//! reporting an **error** at instantiation is **not built**.
//!
//! The criterion is "**the part itself cannot be bound**" — its *own* bind
//! failed — and never
//! "its statement reported something": well-typed siblings of the same
//! statement are built. The shipped criterion is **structural** (`CIMP.md` U51
//! landing criterion: keyed on no instance name and no `_` prefix special
//! case): a chain call's artifact is the
//! auto-named receiver (its `anchor`), and it is dropped only at the failure
//! site that makes its own bind fail — a required formal left unfilled (E4176)
//! or an actual whose members cannot each hand exactly one lane to a formal
//! slot (E4180). Nothing is keyed on the instance's spelling.
//!
//! The ruled families and this file's locks:
//!
//! 1. **bind failure** — `RX(10).Pullup(SPI)` (missing `n2`), a surplus
//!    argument, and a `_` receiver: E4176, and the connection-free `_RX1` the
//!    canon names is **not built**;
//! 2. **fatal width mismatch** — `[SPI{A, B}, GND]` against two formal slots:
//!    the group contributes **one element owning two lanes**, so the members
//!    cannot pair; E4180, not built — and no E4007;
//! 3. **surplus single-lane elements stay pairable** — `[SPI, GND, VDD]`:
//!    every member pairs and only the surplus is left over, which the ruling
//!    keeps (conclusion 1), so **both fork branches are built**. This is the
//!    discriminator that stops family 2 from degenerating into "any width
//!    mismatch drops the part";
//! 4. **legitimately unwired is not unbuilt** — a declared-only part with no
//!    connection at all (including one whose *written* name starts with `_`)
//!    is built and kept; the canon's blocker 1 forbids "no connection at all"
//!    from standing
//!    alone as the criterion, and this is the case that pins it.
//!
//! **Built is read structurally** — the arena's `Device` nodes — never from a
//! connection-derived listing: those render what is *wired*, and a U51 artifact
//! is precisely what is not. The diagnostics corroborate: a built-but-unwired
//! instance is named by E4112/E4116, which a retracted one never carries.

// Family naming `u51__{essence}` deliberately doubles the underscore so the
// grep-able family token stays separate.
#![allow(non_snake_case)]

mod common;

use std::collections::BTreeSet;

use mcc::McIds;
use mcc::McURI;

/// Two-pin device with **two scalar formals** — the shape the bind-failure
/// family fills. Body mirrors the probe fixtures: wire the two nets through
/// the device.
const RX: &str = "component RX(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup(n1, n2) {\n        n1 - this - n2\n    }\n}\n";

/// Two-pin device with **one indexed formal over two slots** — the shape the
/// width family fills. The trailing `return` mirrors the library `CAP.Cap`
/// body, whose expansion against a half-bound formal set was what the residue
/// dragged E4007 out of (CIMP U51 blocker 3).
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
/// children, and a retracted node has left both stores. Every fixture here
/// declares its call inside the root module's `func`, and the arena has no
/// function-scope node kind, so the root walk sees all of them.
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

/// §11.6: an error on **the component's own bind** means it never enters the
/// netlist. Each spelling here fails its own bind, so each builds nothing —
/// and, crucially, reports nothing about an unconnected instance, which is
/// what a built residue would look like (E4112/E4116).
#[test]
fn u51__bind_failure_builds_nothing() {
    for body in [
        // Missing required formal `n2` — the canon's own `RX(10).Pullup(SPI)`.
        "        RX(10).Pullup(SPI)",
        // Too many arguments: 3 against 2 slots.
        "        RX(10).Pullup(SPI, VDD, GND)",
        // `_` receiver: the prefix form hands the statement net to `n1` only.
        "        SPI => RX(10).Pullup(_)",
    ] {
        let src = src_scalar(body);
        assert_eq!(
            devices_of(&src, "/mcc/u51-bind.mc"),
            BTreeSet::<String>::new(),
            "a component whose own bind failed must not be built; body={body:?}"
        );
        let codes = codes_of(&src, "/mcc/u51-bind.mc");
        assert!(
            codes.contains(&4176),
            "the failed bind must still be reported (E4176); body={body:?} codes={codes:?}"
        );
        assert!(
            !codes.contains(&4112) && !codes.contains(&4116),
            "a retracted artifact carries no unconnected-instance report — \
             that report is precisely the built-residue signature; \
             body={body:?} codes={codes:?}"
        );
    }
}

/// §11.6 + conclusion 1: a **fatal** width mismatch. `[SPI{A, B}, GND]` is an
/// argument table of two top-level elements, the first of which owns two
/// lanes: the members cannot each hand exactly one lane to a slot, so the
/// bind fails and nothing is built. The deficit spellings (`[FOO]` undeclared,
/// `[SPI]` a one-member bus — a single leaf, §11.6's whole-value fill needing
/// a declared bus of **more than one** member) are the same family from the
/// other side.
///
/// No E4007: the residue used to be built with a half-bound formal set, and
/// expanding the library `Cap` body (`return [net1, net2]`) against it dragged
/// the shape error out (CIMP U51 blocker 3). With no residue there is no body
/// expansion to go wrong.
#[test]
fn u51__fatal_width_mismatch_builds_nothing() {
    let cases: [(fn(&str) -> String, &str); 3] = [
        // Group element against a scalar slot: 2 lanes where 1 may go.
        (src_bus, "        CAP(10).Cap([SPI{A, B}, GND])"),
        // Undeclared name: one leaf against two slots.
        (src_bus, "        CAP(10).Cap([FOO])"),
        // One-member bus: one leaf against two slots.
        (src_bus1, "        CAP(10).Cap([SPI])"),
    ];
    for (src_of, body) in cases {
        let src = src_of(body);
        assert_eq!(
            devices_of(&src, "/mcc/u51-width.mc"),
            BTreeSet::<String>::new(),
            "an unpairable actual must not be built; body={body:?}"
        );
        let codes = codes_of(&src, "/mcc/u51-width.mc");
        assert!(
            codes.contains(&4180),
            "the fatal mismatch must still be reported (E4180); body={body:?} codes={codes:?}"
        );
        assert!(
            !codes.contains(&4007),
            "the residue's downstream shape error must be gone with the \
             residue; body={body:?} codes={codes:?}"
        );
    }
}

/// conclusion 1 (`param-prefix-design.md` §3.2): the surplus is **kept**. `SPI`
/// occupies the argument table on its z-axis, so the statement forks per lane
/// — and each branch pairs its own two members against the two slots. Only a
/// leftover single-lane element remains, which the ruling does not punish, so
/// **both branches build**, each wire-complete.
///
/// This is the discriminator: without it, family 2's rule would be "a width
/// mismatch drops the part", which would drop exactly the case the ruling
/// spares.
#[test]
fn u51__surplus_elements_still_build() {
    let body = "        CAP(10).Cap([SPI, GND, VDD])";
    let src = src_bus(body);
    let devs = devices_of(&src, "/mcc/u51-surplus.mc");
    assert_eq!(
        devs,
        BTreeSet::from(["_C1".to_string(), "_C2".to_string()]),
        "both fork branches are complete, so both are built; body={body:?}"
    );
    assert_eq!(
        count_code(&src, "/mcc/u51-surplus.mc", 4180),
        2,
        "and each branch reports the mismatch it saw; body={body:?}"
    );

    // `_C1` takes SPI.A, `_C2` takes SPI.B; each also lands on GND — 2 of 2
    // pins wired, i.e. "the part itself is complete".
    let nets = nets_of(&src, "/mcc/u51-surplus.mc");
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

/// CIMP U51 blocker 1: "no connection at all" alone is **not** the criterion.
/// A part that
/// is declared and never wired is legitimate (BOM-only / NC parts), so it is
/// built and kept — and reported as unwired, which is how it stays visible.
///
/// The second spelling is the same part under a **written** name that starts
/// with `_`: the same name shape the retracted `_RX1` carries, opposite fate.
/// A criterion keyed on the `_` prefix — or on any instance name — fails here.
#[test]
fn u51__unwired_declaration_is_kept() {
    for (body, name) in [("    PART p1", "p1"), ("    PART _p1", "_p1")] {
        let src = src_decl(body);
        assert_eq!(
            devices_of(&src, "/mcc/u51-decl.mc"),
            BTreeSet::from([name.to_string()]),
            "an unwired declaration is not an error and must be built; body={body:?}"
        );
        let codes = codes_of(&src, "/mcc/u51-decl.mc");
        assert!(
            codes.contains(&4112) && codes.contains(&4116),
            "and it must be reported as unwired, not silently dropped; \
             body={body:?} codes={codes:?}"
        );
    }
}

/// The counterpart of the two families above: the same chain spelling with a
/// **satisfiable** bind builds its artifact, wire-complete. The retraction is
/// scoped to the failure, not to the spelling.
#[test]
fn u51__satisfiable_chain_is_built() {
    let body = "        RX(10).Pullup(SPI, VDD)";
    let src = src_scalar(body);
    assert_eq!(
        devices_of(&src, "/mcc/u51-built.mc"),
        BTreeSet::from(["_RX1".to_string()]),
        "a chain whose bind succeeds must build its receiver; body={body:?}"
    );
    let codes = codes_of(&src, "/mcc/u51-built.mc");
    assert!(
        !codes.contains(&4176) && !codes.contains(&4180),
        "and report no bind error; body={body:?} codes={codes:?}"
    );
}
