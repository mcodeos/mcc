// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for the boundary of a sub-module method formal that names no port of
// its container (CIMP §1 U108).
//
// `terminal-formal-design.md` §6 (principle C of `composition-terminal-design.md`
// §1) makes a func formal the container's *inner* terminal face: "a func adds no
// terminal of its own". §9 adds that an actual argument can only ever be a
// **position**, never a topology. Together they leave one hole: what if the
// formal name matches no port the container declares? There is then no terminal
// for the actual to land on, and the engine must say so.
//
// It minted one instead. Phase B reused the formal's own spelling as a fallback
// and built a boundary endpoint out of nothing, then wired the parent's net to
// it. Measured 2026-09-19 against `0c3a270` on the scalar form (`module SUB {
// io A, B  func wire(X) { … } }` called as `s.wire(P)`), the engine was **not**
// silent -- but what it printed does not carry the ruling:
//
//   * scalar: `warning[E3175] ...:11:5: Port(s) 'X' not found in module 's'.
//     Available ports: [A, B]`, at **Warning** level and therefore
//     `errors=0` -- the build reports success while `main.s.X` sits on the
//     parent's net as a fabricated second member. The report arrives from the
//     pass2 net-point validity check, i.e. *after* the phantom exists: the
//     engine notices the name only once it has already built the thing the
//     name was never entitled to.
//   * degenerate (bus): the only **error** is `E4005` "Shape mismatch in
//     parallel connection", anchored on the body's own `EXT + SPI`. That is a
//     complaint about the *shape of a fold*. The missing port is reported
//     separately, and again only as the same Warning-level `E3175`.
//
// So the defect is not silence; it is that the judgement is made at the wrong
// layer, at the wrong level, and too late to stop the wiring. The ruling is
// "report, do not mint" -- and the report already has a shape for it: `E3175`
// (`MODULE_PORT_NOT_FOUND`) carries exactly "the name that missed" plus "the
// ports that exist", and `semantic/instref.rs` raises it the same way (as an
// error, and the offending reference is dropped).
//
// These cases pin the two halves separately, because either alone passes on a
// broken engine. The **structure** half alone is satisfied by an engine that
// silently drops the wire; the **report** half alone by the old one, which
// reported the miss at Warning level after drawing the phantom wire. The level
// is part of the report half and is asserted here, since the code and the
// message text are *identical* on both sides of the fix -- `E3175` at Warning
// (old, from the net-point check) versus `E3175` at Error (new, from the
// binding site). Without the level assertion this lock would pass on the
// unfixed source.
//
// The unmarked twin of each case keeps the lock from passing on a blanket
// "always report" or "never wire" -- which is why every branch of the
// judgement has a paired case: formal that names a declared scalar port,
// formal that names none, and a value formal (an actual that is not a
// parent-scope reference at all) that this rule never touches.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashMap;

use mcc::{DiagnosticLevel, McIds};

/// `MODULE_PORT_NOT_FOUND` -- the code the ruling reuses, unchanged.
const MODULE_PORT_NOT_FOUND: u32 = 3175;

/// A sub-module with two scalar ports and one func, plus a top module whose
/// single port `P` is the actual handed to that func.
///
/// `io P` is a *port of main*, so the argument is a parent-scope reference and
/// the formal it binds is therefore a terminal formal (`actual_is_parent_ref`,
/// `fcallinst.rs`) -- the axis the whole judgement hangs on.
fn board(func: &str, call: &str) -> String {
    format!(
        r#"module SUB
{{
    io A, B
    func wire({func})
    {{
        {body}
    }}
}}

module main
{{
    io P
    SUB s
    {call}
}}
"#,
        body = if func == "X" { "X + A" } else { "A + B" }
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Diag {
    code: u32,
    level: DiagnosticLevel,
    msg: String,
}

struct Built {
    /// Every instance path in the flat table mapped to the net name it sits on,
    /// or `None` when it sits on none. The phantom endpoint this lock is about
    /// is exactly a path that acquires a net it was never wired to.
    net_of_path: HashMap<String, Option<String>>,
    /// Sorted by `(code, message)`, so assertions do not depend on report order.
    diags: Vec<Diag>,
}

fn build(tag: &str, source: &str) -> Built {
    let _lock = common::lock();
    common::reset();

    let uri = format!("/mcc/u108-{tag}.mc");
    mcc::mcc_load_from_string(&uri, source);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000)
        .unwrap_or_else(|e| panic!("flat build failed for {tag}: {e:?}"));

    let mut net_of_path: HashMap<String, Option<String>> = HashMap::new();
    for (_, entry) in table.iter() {
        net_of_path.entry(entry.path.clone()).or_insert(None);
    }
    for net in table.get_nets() {
        for point in &net.points {
            if let Some(entry) = table.get_entry(*point) {
                net_of_path.insert(entry.path.clone(), Some(net.name.clone()));
            }
        }
    }

    let mut diags: Vec<Diag> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| Diag {
            code: d.code,
            level: d.level,
            msg: d.msg.clone(),
        })
        .collect();
    diags.sort_by(|a, b| (a.code, &a.msg).cmp(&(b.code, &b.msg)));

    Built { net_of_path, diags }
}

impl Built {
    fn count(&self, code: u32) -> usize {
        self.diags.iter().filter(|d| d.code == code).count()
    }

    /// The sole diagnostic carrying `code`.
    fn only(&self, code: u32) -> &Diag {
        let hits: Vec<&Diag> = self.diags.iter().filter(|d| d.code == code).collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one E{code}: {:?}",
            self.diags
        );
        hits[0]
    }

    fn has_error(&self) -> bool {
        self.diags.iter().any(|d| d.level == DiagnosticLevel::Error)
    }

    fn net_of(&self, path: &str) -> Option<&str> {
        self.net_of_path.get(path).and_then(|n| n.as_deref())
    }
}

// -- 1. the diseased branch: the formal names no declared port --

/// `func wire(X)` where `SUB` declares only `A` and `B`. The rule fires: one
/// **Error-level** `E3175` names `X` and lists the ports the container does
/// have, and -- the other half -- the parent port `P` is left on **no** net.
///
/// Before the ruling `P` was on a net whose second member was the invented
/// `main.s.X`. The three assertions below therefore fail in different ways if
/// any half regresses: the old engine reports the code at Warning level (level
/// assertion), and an engine that reports but still wires leaves `P` netted
/// (structure assertion), while one that drops the wire silently reports no
/// code at all (count assertion).
#[test]
fn sem_formalport__formal_naming_no_declared_port_reports_and_mints_nothing() {
    let b = build("scalar", &board("X", "s.wire(P)"));

    assert_eq!(b.count(MODULE_PORT_NOT_FOUND), 1, "diags: {:?}", b.diags);
    let d = b.only(MODULE_PORT_NOT_FOUND);
    assert!(
        d.msg.contains("'X'"),
        "message must name the formal: {}",
        d.msg
    );
    // The ports that exist are named too, so the author is pointed at the
    // spelling that would have worked (ruling ②) -- never at a shape.
    assert!(
        d.msg.contains("A"),
        "message must list available ports: {}",
        d.msg
    );
    assert!(
        d.msg.contains("B"),
        "message must list available ports: {}",
        d.msg
    );
    // The level carries the ruling. The same code, with the same message text,
    // is what the old engine printed from the net-point check -- as a Warning,
    // which left the build at `errors=0`.
    assert_eq!(
        d.level,
        DiagnosticLevel::Error,
        "the miss must be an error, not a warning paired with a drawn wire"
    );

    // The actual is a parent port, so it exists as a point; the boundary that
    // would have carried it was not built.
    assert_eq!(b.net_of("main.P"), None, "parent port was wired anyway");

    // The body's own reference to `X` is a bare name inside the sub-module and
    // still becomes a floating label there (a separate, still-open half of
    // U108: it is a *label*, not a terminal, and it stays inside the
    // container). What must never happen is that label reaching the parent.
    assert_ne!(
        b.net_of("main.s.X").map(str::to_string),
        b.net_of("main.P").map(str::to_string),
        "the invented endpoint reached the parent's net"
    );
}

/// The same board with the formal **renamed onto a declared port** -- the
/// spelling ruling ① calls the one legal way to write it. Nothing is reported,
/// and the boundary is drawn: the parent port and the sub-module port share one
/// net.
///
/// Without this twin the test above would also pass on an engine that refused
/// every sub-module method call.
#[test]
fn sem_formalport__formal_naming_a_declared_port_wires_silently() {
    let b = build("legal", &board("A", "s.wire(P)"));

    assert_eq!(b.count(MODULE_PORT_NOT_FOUND), 0, "diags: {:?}", b.diags);
    // No error at all: the case is legal, and the only other diagnostics this
    // board draws are style warnings about the one-letter names.
    assert!(!b.has_error(), "unexpected error: {:?}", b.diags);
    let net = b.net_of("main.P").expect("parent port must be wired");
    assert_eq!(b.net_of("main.s.A"), Some(net), "boundary not drawn");
}

// -- 2. the degenerate (bus) branch, where a fold error must not stand in for the miss --

/// A sub-module with a four-member bus port and a func whose formal names no
/// port at all. The old engine minted a one-lane degenerate `EXT~0` inside and
/// the parent saw a shape mismatch, so the only error the case produced was
/// `E4005` "Shape mismatch in parallel connection" -- anchored on the body's
/// own `EXT + SPI`, a report about the *fold* rather than about the missing
/// port.
///
/// The formal judgement is the same one as the scalar branch, so the same code
/// fires here, at Error level, and the parent port stays unwired. `E4005` is
/// deliberately not asserted either way: it is raised (or not) by the body's
/// own `+` between a scalar and a bus, which is a separate question from
/// whether the boundary exists.
#[test]
fn sem_formalport__degenerate_form_reports_the_missing_port_instead_of_a_shape() {
    let source = r#"module MCU
{
    io SPI{SCLK, MOSI, CSN, MISO}
    func loadFlash(EXT)
    {
        EXT + SPI
    }
}

module main
{
    io P
    MCU uC
    uC.loadFlash(P)
}
"#;
    let b = build("degenerate", source);

    assert_eq!(b.count(MODULE_PORT_NOT_FOUND), 1, "diags: {:?}", b.diags);
    let d = b.only(MODULE_PORT_NOT_FOUND);
    assert!(
        d.msg.contains("'EXT'"),
        "message must name the formal: {}",
        d.msg
    );
    assert!(
        d.msg.contains("SPI"),
        "message must list available ports: {}",
        d.msg
    );
    assert_eq!(
        d.level,
        DiagnosticLevel::Error,
        "the miss must be an error, not a warning behind a shape complaint"
    );
    assert_eq!(b.net_of("main.P"), None, "parent port was wired anyway");
}

// -- 3. the branch this rule must not touch: a value formal --

/// The same func, called with a **literal**. The actual is not a parent-scope
/// reference, so the formal is a value formal and goes through ordinary
/// substitution -- the boundary judgement never runs, and no port is demanded of
/// it.
///
/// This is the guard against a lock that a blanket "a formal must name a port"
/// would satisfy: `X` names no port here either, yet nothing may be reported.
#[test]
fn sem_formalport__value_formal_is_not_judged_against_the_port_table() {
    let b = build("value", &board("X", "s.wire(5)"));

    assert_eq!(b.count(MODULE_PORT_NOT_FOUND), 0, "diags: {:?}", b.diags);
    assert_eq!(b.net_of("main.P"), None, "the literal must not wire P");
}
