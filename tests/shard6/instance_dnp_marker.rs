// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Retirement lock for the code-face `@dnp` marker (U326②, 2026-09-27) — the
// word no longer reads anywhere.
//
// U305⑤ introduced `@dnp` on both statement-line faces; U326① retired the
// module subtree push-down; U326② moved the device-level not-fitted authority
// to the bom overlay's `path = DNP` row and retired the marker outright
// (user ruling: anonymous parts get named and move to the overlay — the
// overlay is the only device-level authority). What remains to pin:
//
// 1. `@dnp` on either face is just an unknown tail marker now — it reports
//    3188 (the U305③ vocabulary gate) and applies to nothing.
// 2. The retired 3189 ("marker with nothing to apply to") is gone with it:
//    no statement shape may resurrect it.
// 3. The overlay DNP row and the constructor `NC` argument keep landing the
//    same flat-table flag the marker used to (`not_fitted`), so every
//    consumer — BOM, viz, export — still sees a not-fitted part. The overlay
//    face is locked in `bom_binding.rs`; this file pins the constructor face
//    through one downstream reader.
#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

// ── codes under test ──
const STMT_MARKER_UNKNOWN: u32 = 3188;
/// Retired with the marker (U326②) — must never fire again.
const RETIRED_MARKER_NO_TARGET: u32 = 3189;
// ── the "unconnected" family that keeps reporting on a not-fitted part ──
const NET_BIDIR_UNCONNECTED: u32 = 4117;

/// A two-pin passive, both pins bidirectional.
const CHIP: &str =
    "component CHIP\n{\n    pins = [\n        io 1 = A\n        io 2 = B\n    ]\n}\n";

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

/// `@dnp` on an instance line reports the vocabulary gate (3188) and marks
/// nothing — the word left the line's vocabulary with U326②.
#[test]
fn sem_dnp__retired_word_on_instance_line_reports_unknown() {
    let b = build(CHIP, "    CHIP d1 @dnp");
    assert_eq!(
        b.count(STMT_MARKER_UNKNOWN),
        1,
        "the retired marker must report as an unknown word; diags: {:?}",
        b.diags
    );
    assert!(
        b.reports(STMT_MARKER_UNKNOWN, "dnp"),
        "the report names the word; diags: {:?}",
        b.diags
    );
    assert!(
        b.not_fitted.is_empty(),
        "the retired word marks nothing: {:?}",
        b.not_fitted
    );
}

/// `@dnp` on a connection line reports the same gate — and the line-specific
/// code 3189 must not resurrect for it (the inline carrier is retired with
/// the word).
#[test]
fn sem_dnp__retired_word_on_connection_line_reports_unknown_no_3189() {
    let b = build(
        CHIP,
        "    io N1\n    io N2\n    CHIP d1\n    d1.1 -> d1.2 @dnp",
    );
    assert_eq!(
        b.count(STMT_MARKER_UNKNOWN),
        1,
        "the retired marker reports on a connection line too; diags: {:?}",
        b.diags
    );
    assert_eq!(
        b.count(RETIRED_MARKER_NO_TARGET),
        0,
        "3189 is retired with the word and must never fire; diags: {:?}",
        b.diags
    );
    assert!(b.not_fitted.is_empty(), "{:?}", b.not_fitted);
}

/// The gate did not widen: a word that never left the vocabulary still
/// passes. Guards the retirement against turning into a blanket "reject all
/// tail markers" regression.
#[test]
fn sem_dnp__live_words_still_pass_the_gate() {
    let b = build(
        CHIP,
        "    CHIP d1 @ncpin(1)\n    io p1\n    io p2\n    p1 -> d1.A @bridge(p1, p2)",
    );
    assert_eq!(
        b.count(STMT_MARKER_UNKNOWN),
        0,
        "live vocabulary words must still pass; diags: {:?}",
        b.diags
    );
}

/// The constructor `NC` argument still lands the flat-table flag and keeps
/// reporting unconnected (no ERC exemption), and it still reaches a
/// downstream reader. The marker face this file once locked alongside it is
/// gone — the overlay face lives in `bom_binding.rs`.
#[test]
fn sem_dnp__constructor_nc_face_keeps_reaching_the_readers() {
    let b = build(CHIP, "    CHIP d1(NC)");
    assert_eq!(b.fitted_paths(), ["main.d1"], "{:?}", b.diags);
    assert!(
        b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"),
        "a not-fitted part's pins keep reporting; diags: {:?}",
        b.diags
    );

    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/instance-dnp-readers.mc".to_string();
    let source = format!("{CHIP}\nmodule main\n{{\n    CHIP a(NC)\n}}\n");
    mcc::mcc_load_from_string(&uri, &source);
    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &uri, 1000).expect("flat build");
    let (kicad, _, _) =
        mcc::export::kicad::build_kicad_netlist(&tree, &table, &arena, &store, "main");
    assert_eq!(
        kicad.matches("(name \"DNP\") (value \"yes\")").count(),
        1,
        "the constructor face must still reach the DNP property:\n{kicad}"
    );
}
