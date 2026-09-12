// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! resolve-gate §3.3 / vec-dianlu §7.6 per-member dispatch — §2.6 Table A
//! regression tests.
//!
//! All four Table A forms must materialize one instance per array member and
//! dispatch the method per member (scalar args become nets shared by every
//! member) — never a literal `name[1:2]` instance, never an anonymous
//! `_R1`/`_C1` collapse, no E3179. formN below is the expansion-shape number
//! of §2.6 Table A (matrix §2 row 20):
//!
//! 1. `CLASS y[1:2](args).Method(...)` — class-first named array subinstance
//!    → `dispatch__form1_named_subinstance_per_member`
//! 2. declared `r[1:2]::RES(0)` + `r[1:2].Pullup([net,vcc])` — array receiver
//!    → `dispatch__form2_declared_receiver_per_member`
//! 3. same receiver with `.Pullup` (E3179 phantom form, folded into form 2)
//! 4. `x[1:2]::RES(0).Pullup(...)` — construct + trailing method (⑫ collapse)
//!    → `dispatch__form4_ctor_trailing_method_per_member`
//! 5. module-level declared receiver → `dispatch__module_declared_receiver_per_member`

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

mod common;

use mcc::{McIds, McURI};

const CAP_COMP: &str = "component CAP(cap::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Cap([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";
const RES_COMP: &str = "component RES(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([net1, net2]) {\n        net1 - this - net2\n        return [net1, net2]\n    }\n}\n";

/// HOST with pins `[1,2] = NET, VCC` and a `func F()` whose body is `body`.
const HOST_2PIN: &str = "component HOST {\n    pins = [\n        1 = NET\n        2 = VCC\n    ]\n";

fn host_with_body(body: &str) -> String {
    format!(
        "{HOST_2PIN}    func F() {{\n{}{}}}\n}}\nmodule main {{\n    io VDD\n    HOST U1\n    func M() {{\n        U1.F()\n    }}\n}}\n",
        body.lines().map(|l| format!("        {l}\n")).collect::<String>(),
        "\n"
    )
}

/// Build `main` from `src`, returning (paths, nets, diagnostic codes).
fn build(src: &str, uri: &str) -> (Vec<String>, Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &u, 1000).expect("flat build");

    let mut paths: Vec<String> = table.iter().map(|(_, e)| e.path.clone()).collect();
    paths.sort();

    let mut netlines: Vec<String> = Vec::new();
    for net in table.get_nets() {
        let mut pts: Vec<String> = net
            .points
            .iter()
            .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
            .collect();
        pts.sort();
        netlines.push(format!("{} <= [{}]", net.name, pts.join(", ")));
    }
    netlines.sort();

    let codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    (paths, netlines, codes)
}

/// Find the net whose point set contains `path`.
fn net_containing<'a>(nets: &'a [String], path: &str) -> Option<&'a str> {
    nets.iter().find(|n| n.contains(path)).map(|s| s.as_str())
}

/// The point paths carried by a `name <= [p, q, ...]` net line.
fn points_of(net_line: &str) -> Vec<String> {
    let start = net_line.find('[').expect("net line has a point list") + 1;
    let end = net_line.rfind(']').expect("net line closes its point list");
    net_line[start..end]
        .split(", ")
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Pin-side exclusivity: every point on `net_line` must sit on `pin`
/// (`".1"` / `".2"`). A silent short that welds both sides into one net
/// satisfies a `contains` check on both members — it does not satisfy this.
fn assert_pin_side_exclusive(net_line: &str, pin: &str, what: &str) {
    let pts = points_of(net_line);
    assert!(
        !pts.is_empty(),
        "{what}: net carries no points; got {net_line}"
    );
    for p in &pts {
        assert!(
            p.ends_with(pin),
            "{what}: point '{p}' is not on the {pin} side — sides merged? got {net_line}"
        );
    }
}

/// No point on `net_line` sits on pin number `pin` — the counterpart of
/// [`assert_pin_side_exclusive`] for nets that also carry module-port points
/// (which have no numeric pin suffix at all).
fn assert_no_pin_number(net_line: &str, pin: &str, what: &str) {
    for p in points_of(net_line) {
        assert!(
            !p.ends_with(pin),
            "{what}: point '{p}' must not be on pin {pin} — sides merged? got {net_line}"
        );
    }
}

fn assert_no_path_containing(paths: &[String], fragment: &str, what: &str) {
    for p in paths {
        assert!(
            !p.contains(fragment),
            "{what}: path '{p}' must not contain '{fragment}'; got {paths:?}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Form 1 — class-first named array subinstance
// `CAP c[1:2](1).Cap([NET, VCC])`
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dispatch__form1_named_subinstance_per_member() {
    let src = format!(
        "{CAP_COMP}component HOST {{\n    pins = [\n        1 = NET\n        2 = VCC\n    ]\n    func F() {{\n        CAP c[1:2](1).Cap([NET, VCC])\n    }}\n}}\nmodule main {{\n    io VDD\n    HOST U1\n    func M() {{\n        U1.F()\n    }}\n}}\n"
    );
    let (paths, nets, codes) = build(&src, "/mcc/tablea-f1.mc");
    // c1/c2 materialized (per member), no anonymous `_C1` collapse.
    assert!(
        paths.iter().any(|p| p == "main.c1"),
        "c1 materialized; got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p == "main.c2"),
        "c2 materialized; got {paths:?}"
    );
    assert_no_path_containing(&paths, "_C1", "form1");
    // No literal array instance.
    assert_no_path_containing(&paths, "c[1:2]", "form1");
    assert!(
        !codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "no E3179; got {codes:?}"
    );
    // Dispatch wiring: both members share U1.NET(1) and U1.VCC(2).
    let n1 = net_containing(&nets, "main.U1.1").expect("net on U1 pin 1");
    assert!(
        n1.contains("main.c1.1") && n1.contains("main.c2.1"),
        "NET side shared by both members; got {n1}"
    );
    let n2 = net_containing(&nets, "main.U1.2").expect("net on U1 pin 2");
    assert!(
        n2.contains("main.c1.2") && n2.contains("main.c2.2"),
        "VCC side shared by both members; got {n2}"
    );
    // Anti-false-green: a silent short into ONE net also satisfies both
    // `contains` checks above (the merged net carries every pin).
    assert_ne!(n1, n2, "form1: NET and VCC must be distinct nets");
    assert_pin_side_exclusive(n1, ".1", "form1 NET side");
    assert_pin_side_exclusive(n2, ".2", "form1 VCC side");
}

// ═══════════════════════════════════════════════════════════════════════════
// Forms 2 + 3 — declared array receiver (separate statements)
// `r[1:2]::RES(0)` then `r[1:2].Pullup([NET, VCC])` / `.Cap([NET, VCC])`
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dispatch__form2_declared_receiver_per_member() {
    // Form 3 (the E3179 phantom form): `.Pullup` on declared RES members.
    let pullup = host_with_body("r[1:2]::RES(0)\nr[1:2].Pullup([NET, VCC])");
    let src = format!("{RES_COMP}{pullup}");
    let (paths, nets, codes) = build(&src, "/mcc/tablea-f23-pullup.mc");
    assert!(
        paths.iter().any(|p| p == "main.U1.r1"),
        "r1 materialized; got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p == "main.U1.r2"),
        "r2 materialized; got {paths:?}"
    );
    assert_no_path_containing(&paths, "r[1:2]", "form2/3 Pullup");
    assert!(
        !codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "no E3179; got {codes:?}"
    );
    let n1 = net_containing(&nets, "main.U1.1").expect("net on U1 pin 1");
    assert!(
        n1.contains("main.U1.r1.1") && n1.contains("main.U1.r2.1"),
        "NET side shared by both members; got {n1}"
    );
    let n2 = net_containing(&nets, "main.U1.2").expect("net on U1 pin 2");
    assert!(
        n2.contains("main.U1.r1.2") && n2.contains("main.U1.r2.2"),
        "VCC side shared by both members; got {n2}"
    );
    // Anti-false-green (see form1): pin the sides apart.
    assert_ne!(n1, n2, "form2/3: NET and VCC must be distinct nets");
    assert_pin_side_exclusive(n1, ".1", "form2/3 NET side");
    assert_pin_side_exclusive(n2, ".2", "form2/3 VCC side");

    // Form 2: same receiver shape with `.Cap` on declared CAP members.
    let cap = host_with_body("cap[1:2]::CAP(1)\ncap[1:2].Cap([NET, VCC])");
    let src2 = format!("{CAP_COMP}{cap}");
    let (paths2, nets2, codes2) = build(&src2, "/mcc/tablea-f23-cap.mc");
    assert!(
        paths2.iter().any(|p| p == "main.U1.cap1"),
        "cap1 materialized; got {paths2:?}"
    );
    assert!(
        paths2.iter().any(|p| p == "main.U1.cap2"),
        "cap2 materialized; got {paths2:?}"
    );
    assert_no_path_containing(&paths2, "cap[1:2]", "form2 Cap");
    assert!(
        !codes2.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "no E3179; got {codes2:?}"
    );
    let n1b = net_containing(&nets2, "main.U1.1").expect("net on U1 pin 1 (Cap)");
    assert!(
        n1b.contains("main.U1.cap1.1") && n1b.contains("main.U1.cap2.1"),
        "NET side shared by both members (Cap); got {n1b}"
    );
    // Anti-false-green: the Cap form must not weld pin 2 onto the NET side.
    assert_pin_side_exclusive(n1b, ".1", "form2 Cap NET side");
    let n2b = net_containing(&nets2, "main.U1.2").expect("net on U1 pin 2 (Cap)");
    assert!(
        n2b.contains("main.U1.cap1.2") && n2b.contains("main.U1.cap2.2"),
        "VCC side shared by both members (Cap); got {n2b}"
    );
    assert_ne!(n1b, n2b, "form2 Cap: NET and VCC must be distinct nets");
    assert_pin_side_exclusive(n2b, ".2", "form2 Cap VCC side");
}

// ═══════════════════════════════════════════════════════════════════════════
// Form 4 — construct + trailing method (⑫ `_R1`/`_C1` collapse)
// `x[1:2]::RES(0).Pullup([NET, VCC])`
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dispatch__form4_ctor_trailing_method_per_member() {
    let src = format!(
        "{RES_COMP}component HOST {{\n    pins = [\n        1 = NET\n        2 = VCC\n    ]\n    func F() {{\n        x[1:2]::RES(0).Pullup([NET, VCC])\n    }}\n}}\nmodule main {{\n    io VDD\n    HOST U1\n    func M() {{\n        U1.F()\n    }}\n}}\n"
    );
    let (paths, nets, codes) = build(&src, "/mcc/tablea-f4.mc");
    assert!(
        paths.iter().any(|p| p == "main.x1"),
        "x1 materialized; got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p == "main.x2"),
        "x2 materialized; got {paths:?}"
    );
    // ⑫ collapse is replaced by per-member materialization — no `_R1`.
    assert_no_path_containing(&paths, "_R1", "form4");
    assert_no_path_containing(&paths, "x[1:2]", "form4");
    assert!(
        !codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "no E3179; got {codes:?}"
    );
    let n1 = net_containing(&nets, "main.U1.1").expect("net on U1 pin 1");
    assert!(
        n1.contains("main.x1.1") && n1.contains("main.x2.1"),
        "NET side shared by both members; got {n1}"
    );
    let n2 = net_containing(&nets, "main.U1.2").expect("net on U1 pin 2");
    assert!(
        n2.contains("main.x1.2") && n2.contains("main.x2.2"),
        "VCC side shared by both members; got {n2}"
    );
    // Anti-false-green: this is exactly the assertion pair a silent
    // "one net, all six pins" collapse would satisfy (see C4 in
    // resolve-gate-design §3.3b) — pin the sides apart explicitly.
    assert_ne!(n1, n2, "form4: NET and VCC must be distinct nets");
    assert_pin_side_exclusive(n1, ".1", "form4 NET side");
    assert_pin_side_exclusive(n2, ".2", "form4 VCC side");
}

/// Same two-pin resistor, but `Pullup` has **no** `return`. Per
/// func-return-design §5/§6 a returnless body's connection face is the
/// instance's own port face (implicit `this`) — *not* a bus the callee hands
/// back. Every other fixture here returns `[net1, net2]`, i.e. a `Label`
/// return that overrides both faces; that mask is precisely what hid the
/// defect this lock covers (resolve-gate-design §3.3b C4).
const RES_NORET_COMP: &str = "component RESN(res::INT) {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    func Pullup([net1, net2]) {\n        net1 - this - net2\n    }\n}\n";

#[test]
fn dispatch__form4_returnless_implicit_this_per_member() {
    let src = format!(
        "{RES_NORET_COMP}component HOST {{\n    pins = [\n        1 = NET\n        2 = VCC\n    ]\n    func F() {{\n        x[1:2]::RESN(0).Pullup([NET, VCC])\n    }}\n}}\nmodule main {{\n    io VDD\n    HOST U1\n    func M() {{\n        U1.F()\n    }}\n}}\n"
    );
    let (paths, nets, codes) = build(&src, "/mcc/tablea-f4-noret.mc");
    assert!(
        paths.iter().any(|p| p == "main.x1"),
        "x1 materialized; got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p == "main.x2"),
        "x2 materialized; got {paths:?}"
    );
    assert_no_path_containing(&paths, "_R1", "form4 returnless");
    assert_no_path_containing(&paths, "x[1:2]", "form4 returnless");
    // §3.3: the N dispatched calls are N *independent statements*. Flattening
    // them as one chain joins the members with an undirected gap and welds
    // every pin of every member plus both nets onto ONE net — silently, with
    // no shape diagnostic at all (the pre-fix behaviour). Assert the silence
    // for the *shape* codes; the wiring below carries the real teeth.
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "no E4007; got {codes:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "no E3179; got {codes:?}"
    );
    // D7 PULLUP_DEGENERATE fires once per dispatched member (each pullup runs
    // between two host pins that carry no rail role) — two members, two
    // independent pullups, two diagnostics. Not a defect of this shape.
    assert_eq!(
        codes
            .iter()
            .filter(|c| **c == mcc::errcodes::PULLUP_DEGENERATE)
            .count(),
        2,
        "one D7 per dispatched member; got {codes:?}"
    );
    let n1 = net_containing(&nets, "main.U1.1").expect("net on U1 pin 1");
    assert!(
        n1.contains("main.x1.1") && n1.contains("main.x2.1"),
        "NET side shared by both members; got {n1}"
    );
    let n2 = net_containing(&nets, "main.U1.2").expect("net on U1 pin 2");
    assert!(
        n2.contains("main.x1.2") && n2.contains("main.x2.2"),
        "VCC side shared by both members; got {n2}"
    );
    // Anti-false-green: "one net, all six pins" satisfies both `contains`
    // checks above — that is the exact pre-fix symptom.
    assert_ne!(
        n1, n2,
        "form4 returnless: NET and VCC must be distinct nets"
    );
    assert_pin_side_exclusive(n1, ".1", "form4 returnless NET side");
    assert_pin_side_exclusive(n2, ".2", "form4 returnless VCC side");
}

// ═══════════════════════════════════════════════════════════════════════════
// Module-level declared array receiver (§3.5) — same semantics at module top
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dispatch__module_declared_receiver_per_member() {
    let src = format!(
        "{RES_COMP}module main {{\n    io VDD\n    io NET\n    io VCC\n    func M() {{\n        res[1:2]::RES(0)\n        res[1:2].Pullup([NET, VCC])\n    }}\n}}\n"
    );
    let (paths, nets, codes) = build(&src, "/mcc/tablea-module.mc");
    assert!(
        paths.iter().any(|p| p == "main.res1"),
        "res1 materialized; got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p == "main.res2"),
        "res2 materialized; got {paths:?}"
    );
    assert_no_path_containing(&paths, "res[1:2]", "module-level");
    assert!(
        !codes.contains(&mcc::errcodes::COMPONENT_PIN_NOT_FOUND),
        "no E3179; got {codes:?}"
    );
    let n1 = net_containing(&nets, "main.NET").expect("net on NET");
    assert!(
        n1.contains("main.res1.1") && n1.contains("main.res2.1"),
        "NET side shared by both members; got {n1}"
    );
    let n2 = net_containing(&nets, "main.VCC").expect("net on VCC");
    assert!(
        n2.contains("main.res1.2") && n2.contains("main.res2.2"),
        "VCC side shared by both members; got {n2}"
    );
    // Anti-false-green: a single welded net also carries `main.NET` and
    // `main.VCC`, so both `contains` checks above would still pass.
    assert_ne!(n1, n2, "module-level: NET and VCC must be distinct nets");
    assert_no_pin_number(n1, ".2", "module-level NET side");
    assert_no_pin_number(n2, ".1", "module-level VCC side");
}
