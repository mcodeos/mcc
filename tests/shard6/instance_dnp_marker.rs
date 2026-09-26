// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for the instance-line `@dnp` flag (U305⑤) — "this part is not fitted".
//
// The flag is the statement-line sibling of the constructor `NC` argument: it
// lands the same flat-table flag (`InstEntry.not_fitted`), so every consumer
// that already reads the flag — BOM, viz, export — treats the part as
// mounted-but-absent without any new reader. Two rulings pin the semantics
// (2026-09-26): the unconnected-pin diagnostics keep reporting on a DNP part
// (no ERC exemption — the pins really are unconnected), and a module instance
// may be `@dnp`, which takes its whole subtree off the board with it.
//
// A module entry's `not_fitted` therefore covers descendants: the forward
// pass in `InstTable::propagate_not_fitted` runs after the flatten, and a
// parent always registers before its children.
//
// The same marker also reads on a **connection line**, where it means "every
// part this statement puts on the board is not fitted" (U305⑤ inline carrier,
// b4063). A connection line declares no instance to flag, so the flag is
// applied statement-wide at the build's product funnels — the place that
// knows what the statement actually built — and a flagged line that built
// nothing reports 3189 instead of letting the marker apply to nothing.
#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

// ── codes under test ──
const ATTR_VALUE_NOT_IN_VOCABULARY: u32 = 5360;
/// The flagged connection line built no part — the marker reached nothing.
const STMT_MARKER_NO_TARGET: u32 = 3189;
// ── the "unconnected" family that keeps reporting on a DNP part ──
const NET_BIDIR_UNCONNECTED: u32 = 4117;
const NET_MODULE_PORT_UNCONNECTED: u32 = 4114;

/// A two-pin passive, both pins bidirectional.
const CHIP: &str =
    "component CHIP\n{\n    pins = [\n        io 1 = A\n        io 2 = B\n    ]\n}\n";

/// A sub-module instantiating `CHIP` and exposing one port to the parent.
const SUB: &str = "module SUB\n{\n    io A\n    CHIP u1\n    A -> u1.1\n}\n";

struct Built {
    /// Paths of the entries flagged not-fitted — the structural fact, read
    /// straight off the flat table.
    not_fitted: Vec<String>,
    /// `(code, message)`, sorted, so assertions do not depend on report order.
    diags: Vec<(u32, String)>,
}

fn build(defs: &str, body: &str) -> Built {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/instance-dnp-marker.mc".to_string();
    let source = format!("{defs}\nmodule main\n{{\n{body}\n}}\n");
    mcc::mcc_load_from_string(&uri, &source);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");

    let mut not_fitted: Vec<String> = table
        .iter()
        .filter(|(_, e)| e.not_fitted)
        .map(|(_, e)| e.path.clone())
        .collect();
    not_fitted.sort();

    let mut diags: Vec<(u32, String)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    diags.sort();

    Built { not_fitted, diags }
}

impl Built {
    fn fitted_paths(&self) -> Vec<&str> {
        self.not_fitted.iter().map(String::as_str).collect()
    }

    fn count(&self, code: u32) -> usize {
        self.diags.iter().filter(|(c, _)| *c == code).count()
    }

    /// Does any diagnostic carrying `code` name `needle`?
    fn reports(&self, code: u32, needle: &str) -> bool {
        self.diags
            .iter()
            .any(|(c, m)| *c == code && m.contains(needle))
    }
}

/// `@dnp` on a component instance marks exactly that instance not-fitted —
/// and its pins keep reporting unconnected (the ruling keeps ERC
/// unexempted: the pins really are unconnected).
#[test]
fn sem_dnp__component_flag_marks_not_fitted_and_keeps_reports() {
    let b = build(CHIP, "    CHIP d1 @dnp");
    assert_eq!(b.fitted_paths(), ["main.d1"]);
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.2"));
    assert_eq!(
        b.count(NET_BIDIR_UNCONNECTED),
        2,
        "a DNP part's unconnected pins must keep reporting; diags: {:?}",
        b.diags
    );
}

/// The unmarked twin: nothing is not-fitted. Locking only the marked case
/// would pass even if the flag were a blanket table-wide default.
#[test]
fn sem_dnp__unmarked_twin_stays_fitted() {
    let b = build(CHIP, "    CHIP d1");
    assert!(b.not_fitted.is_empty(), "{:?}", b.not_fitted);
}

/// `@dnp` on a module instance marks the module **and its whole subtree** —
/// the part mounted inside a DNP assembly is off the board with it.
#[test]
fn sem_dnp__module_flag_marks_the_whole_subtree() {
    let b = build(&format!("{CHIP}{SUB}"), "    SUB s1 @dnp");
    assert_eq!(
        b.fitted_paths(),
        ["main.s1", "main.s1.u1"],
        "a DNP module takes its subtree with it; diags: {:?}",
        b.diags
    );
    // The subtree's own diagnostics keep reporting too (no exemption).
    assert!(
        b.reports(NET_MODULE_PORT_UNCONNECTED, "main.s1.A")
            || b.reports(NET_BIDIR_UNCONNECTED, "main.s1.u1.2"),
        "the DNP assembly's unconnected faces must keep reporting; diags: {:?}",
        b.diags
    );
}

/// The bare flag is the only legal shape: `@dnp(yes)` reports the flag-arity
/// code (5360) and still counts as written — the part is marked either way.
#[test]
fn sem_dnp__flag_with_value_reports_5360_and_still_marks() {
    let b = build(CHIP, "    CHIP d1 @dnp(yes)");
    assert_eq!(
        b.count(ATTR_VALUE_NOT_IN_VOCABULARY),
        1,
        "diags: {:?}",
        b.diags
    );
    assert_eq!(b.fitted_paths(), ["main.d1"]);
}

// ── the inline carrier: `@dnp` on a connection line ──

/// A connection line declares no instance, so its `@dnp` flags the part the
/// line **builds** — here an anonymous inline construction. The part reaches
/// the same flat-table flag as the declaration face, so the BOM bucket and
/// every other `not_fitted` consumer see it with no new reader.
#[test]
fn sem_dnp__inline_construction_on_a_connection_line_marks_the_part() {
    let b = build(CHIP, "    io N1\n    io N2\n    N1 - CHIP() - N2 @dnp");
    assert_eq!(
        b.fitted_paths().len(),
        1,
        "the line builds exactly one part; diags: {:?}",
        b.diags
    );
    assert!(
        b.fitted_paths()[0].starts_with("main._"),
        "the built part is the statement's own anonymous instance: {:?}",
        b.fitted_paths()
    );
    assert_eq!(b.count(STMT_MARKER_NO_TARGET), 0);
}

/// The unmarked twin of the line above. Without it a blanket "everything is
/// not fitted" default would pass the marked case.
#[test]
fn sem_dnp__inline_twin_on_a_connection_line_stays_fitted() {
    let b = build(CHIP, "    io N1\n    io N2\n    N1 - CHIP() - N2");
    assert!(b.not_fitted.is_empty(), "{:?}", b.not_fitted);
}

/// A line that builds **two** parts flags both: the marker is the statement's,
/// not the construction's, so a bracket vector's every expansion member is
/// covered without the flag being readable off any one of them.
#[test]
fn sem_dnp__inline_carrier_covers_every_part_the_statement_builds() {
    let b = build(
        CHIP,
        "    io N1\n    io N2\n    io N3\n    N1 - [a[1:2]::CHIP()] - [N2, N3] @dnp",
    );
    assert_eq!(
        b.fitted_paths().len(),
        2,
        "both replica members are the statement's products; diags: {:?}",
        b.diags
    );
}

/// The marker with nothing to mark: a connection line that builds no part
/// leaves `@dnp` claimed by nobody, which reports (the U305③ rule) instead of
/// vanishing. The spelling is legal here — a mistyped word is 3188 — so this
/// is its own code.
#[test]
fn sem_dnp__connection_line_marker_without_a_construction_reports() {
    let b = build(
        CHIP,
        "    io N1\n    io N2\n    CHIP d1\n    d1.1 -> d1.2 @dnp",
    );
    assert_eq!(
        b.count(STMT_MARKER_NO_TARGET),
        1,
        "a flagged connection line that builds nothing must report; diags: {:?}",
        b.diags
    );
    // And the reference line's own `@dnp` is *not* borrowed by the marker: the
    // declared part keeps its own status.
    assert!(b.not_fitted.is_empty(), "{:?}", b.not_fitted);
}

/// The flag's arity rule reads the same on a connection line: `@dnp(yes)`
/// reports 5360 and the line's part is marked anyway.
#[test]
fn sem_dnp__inline_flag_with_value_reports_5360_and_still_marks() {
    let b = build(CHIP, "    io N1\n    io N2\n    N1 - CHIP() - N2 @dnp(yes)");
    assert_eq!(
        b.count(ATTR_VALUE_NOT_IN_VOCABULARY),
        1,
        "diags: {:?}",
        b.diags
    );
    assert_eq!(b.fitted_paths().len(), 1, "diags: {:?}", b.diags);
}

/// The two spellings are **one verdict on every reader**, not just on the flat
/// table. A reader that picks the instance's `nc` field sees only the
/// constructor face, which is how `export kicad`'s DNP property went missing
/// for a `@dnp` part — this lock is written after that probe. Both parts below
/// are not fitted, and one reader must say so about both.
#[test]
fn sem_dnp__every_reader_sees_both_spellings() {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/instance-dnp-readers.mc".to_string();
    let source = format!("{CHIP}\nmodule main\n{{\n    CHIP a(NC)\n    CHIP b @dnp\n}}\n");
    mcc::mcc_load_from_string(&uri, &source);
    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &uri, 1000).expect("flat build");
    let (kicad, _, _) =
        mcc::export::kicad::build_kicad_netlist(&tree, &table, &arena, &store, "main");
    assert_eq!(
        kicad.matches("(name \"DNP\") (value \"yes\")").count(),
        2,
        "the constructor face and the marker face must both reach the DNP \
         property — a reader that picks one field loses the other:\n{kicad}"
    );
}
