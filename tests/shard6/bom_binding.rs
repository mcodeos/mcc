// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Lock for the BOM overlay bind machinery and its two ERC checks (U245;
//! mc-carrier rework U267①): a key binds an abstract slot to a `:` descendant
//! variant (the class name rides the variant, the unselected W clears,
//! 5067/5068 stay silent), and every failed shape reports — value not a
//! descendant (E5067), value unresolved (E5067), value naming the base
//! itself (①a, E5067), cross-family variant (①b, E5067), key on a concrete instance
//! (b2 E5068 Error), key repeating the concrete class (b1 E5068 Warning),
//! duplicate key same value (b1 W), duplicate key conflicting values (b2 E),
//! dangling key (b3 E5068), header top mismatch (all rows dangle into b3),
//! and a parse-broken carrier clears the registry while reporting through the
//! normal parse diagnostic domain. The carrier is `bom.mc` — one
//! `bom <top> { path = Class }` block (bom-overlay-design.md v0.2);
//! `bom__` locks judge rows, never the carrier. Param-authoring-design.md
//! section 4.

#![allow(non_snake_case)]

use crate::common;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

const BOARD_SRC: &str = r#"
abstract component PART.SHAPE
{
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

component PART.SHAPE_V2 : PART.SHAPE
{
    partno = "PS-V2"
}

component OTHER.THING
{
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

component OTHER.V2 : OTHER.THING
{
    partno = "OT-V2"
}

module BOARD
{
    PART.SHAPE slot
    PART.SHAPE open
    OTHER.THING fixed
}

module main
{
    BOARD b
}
"#;

/// Wrap `rows` in the single canonical carrier block.
fn bom_block(rows: &str) -> String {
    format!("bom main {{\n{rows}}}\n")
}

/// Materialize a throwaway project dir whose only content is the overlay
/// file, and point the workspace at it. `None` clears the overlay.
fn set_overlay(overlay: Option<&str>) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir: PathBuf = std::env::temp_dir().join(format!("mcc_u245_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp project dir");
    if let Some(text) = overlay {
        std::fs::write(dir.join("bom.mc"), text).expect("write overlay");
    }
    mcc::mcc_set_project_root(&dir);
    dir
}

fn discard(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    // Hand the workspace back with no project root, so no later test in this
    // binary inherits our overlay.
    mcc::mcc_set_project_root(std::path::Path::new(""));
}

/// Reset, point at `overlay`, load the board, build flat; return the table
/// and the diagnostics. The board source enters through a file under the
/// project dir — the overlay only speaks for builds whose entry lives under
/// the root that owns it.
fn build_with_src(src: &str, overlay: Option<&str>) -> (mcc::InstTable, Vec<mcc::McDiagnostic>) {
    // The lock spans the whole build: the workspace, the project root and the
    // overlay registry are process-global, and shard tests run in parallel.
    let _lock = common::lock();
    common::reset();
    let dir = set_overlay(overlay);
    let board_uri = dir.join("board.mc").to_string_lossy().into_owned();
    common::load_string(&board_uri, src);
    let built = mcc::mcc_build_flat(&mcc::McIds::from("main"), &board_uri, 1000);
    let table = built.expect("flat build").1;
    let diags = mcc::mcc_diagnose_all();
    discard(&dir);
    (table, diags)
}

fn build_with(overlay: Option<&str>) -> (mcc::InstTable, Vec<mcc::McDiagnostic>) {
    build_with_src(BOARD_SRC, overlay)
}

fn count_code(diags: &[mcc::McDiagnostic], code: u32) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

/// (class_name, unselected) of the flat row at `path`.
fn row_of(table: &mcc::InstTable, path: &str) -> (String, bool) {
    let id = table
        .get_id_by_path(path)
        .unwrap_or_else(|| panic!("row {path} present"));
    let e = table.get_entry(id).unwrap();
    (e.class_name.clone(), e.unselected)
}

/// A key on an abstract slot binds the descendant variant: the class name
/// rides the variant, the row is selected, the unbound sibling stays
/// unselected with exactly one W, and both overlay checks stay silent.
#[test]
fn bom__key_binds_abstract_slot_to_descendant_variant() {
    let (table, diags) = build_with(Some(&bom_block(
        "    b.slot = PART.SHAPE_V2\n",
    )));
    let (slot_class, slot_unselected) = row_of(&table, "main.b.slot");
    assert_eq!(slot_class, "PART.SHAPE_V2", "the bound def is the variant");
    assert!(!slot_unselected, "a bound slot is selected");
    let (open_class, open_unselected) = row_of(&table, "main.b.open");
    assert_eq!(open_class, "PART.SHAPE", "the unbound slot keeps its base");
    assert!(open_unselected, "the unbound slot stays unselected");
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        0
    );
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT),
        0
    );
    let unselected: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::ABSTRACT_PART_UNSELECTED)
        .map(|d| d.msg.as_str())
        .collect();
    assert_eq!(unselected.len(), 1, "exactly the unbound slot warns");
    assert!(
        unselected[0].contains("main.b.open"),
        "W names the open slot"
    );
}

/// A value that is not a `:` descendant of the slot class fires E5067 and
/// the slot keeps its declared base.
#[test]
fn bom__value_not_descendant_fires_5067() {
    let (table, diags) = build_with(Some(&bom_block(
        "    b.slot = OTHER.THING\n",
    )));
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        1
    );
    let (slot_class, _) = row_of(&table, "main.b.slot");
    assert_eq!(slot_class, "PART.SHAPE", "a failed bind keeps the base");
}

/// A value that resolves to no live def fires E5067 as well.
#[test]
fn bom__value_unresolved_fires_5067() {
    let (_, diags) = build_with(Some(&bom_block(
        "    b.slot = PART.NOPE\n",
    )));
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        1
    );
}

/// ①a: naming the abstract base itself resolves but is no selection — the
/// row would leave the slot dangling under a concrete name, so E5067 fires.
#[test]
fn bom__value_naming_the_base_itself_fires_5067() {
    let (_, diags) = build_with(Some(&bom_block(
        "    b.slot = PART.SHAPE\n",
    )));
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        1
    );
}

/// ①b: a variant of another base is a real selection for the wrong slot —
/// E5067 fires and the slot keeps its declared base.
#[test]
fn bom__value_cross_family_variant_fires_5067() {
    let (_, diags) = build_with(Some(&bom_block(
        "    b.slot = OTHER.V2\n",
    )));
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        1
    );
}

/// A key whose instance declares a concrete class that differs from the
/// overlay value fires E5068 as an Error (b2) and the instance is untouched.
#[test]
fn bom__key_on_concrete_instance_diff_class_fires_5068_error() {
    let (table, diags) = build_with(Some(&bom_block(
        "    b.fixed = PART.SHAPE_V2\n",
    )));
    let slot_errors = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .count();
    assert_eq!(slot_errors, 1, "b2 fires exactly once");
    let (fixed_class, fixed_unselected) = row_of(&table, "main.b.fixed");
    assert_eq!(fixed_class, "OTHER.THING", "a non-slot is never retyped");
    assert!(!fixed_unselected);
}

/// b1: a key repeating exactly the class the instance already declares is
/// the duplicate form — E5068 at Warning level, no retyping.
#[test]
fn bom__key_repeats_declared_class_fires_5068_warning() {
    let (_, diags) = build_with(Some(&bom_block(
        "    b.fixed = OTHER.THING\n",
    )));
    let b1: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .collect();
    assert_eq!(b1.len(), 1, "b1 fires exactly once");
    assert!(
        !matches!(b1[0].level, mcc::DiagnosticLevel::Error),
        "the duplicate form is a Warning, not an Error"
    );
    assert!(
        b1[0].msg.contains("redundant"),
        "the message names the redundancy: {}",
        b1[0].msg
    );
}

/// A key that matches no instance fires E5068 exactly once (b3), and the mc
/// carrier anchors it at the overlay row — not at position zero.
#[test]
fn bom__dangling_key_fires_5068() {
    let (_, diags) = build_with(Some(&bom_block(
        "    b.nope = PART.SHAPE_V2\n",
    )));
    let b3: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .collect();
    assert_eq!(b3.len(), 1);
    assert!(
        b3[0].msg.contains("no instance"),
        "the message names the empty path: {}",
        b3[0].msg
    );
}

/// No overlay file: no overlay diagnostics, abstract rows stay unselected.
#[test]
fn bom__no_overlay_leaves_slots_unselected_and_clean() {
    let (table, diags) = build_with(None);
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        0
    );
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT),
        0
    );
    for path in ["main.b.slot", "main.b.open"] {
        assert!(row_of(&table, path).1, "{path} is unselected");
    }
}

/// A block header naming a top the build does not enter consumes nothing:
/// every row dangles into the b3 domain (bom-overlay-design.md §6).
#[test]
fn bom__header_top_mismatch_dangles_all_rows() {
    let (_, diags) = build_with(Some("bom other {\n    b.slot = PART.SHAPE_V2\n}\n"));
    let b3 = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .count();
    assert_eq!(b3, 1, "the only row dangles (E5068)");
}

/// Duplicate keys inside one block: same value is the b1 W, conflicting
/// values are the b2 E — and the first row still wins the bind.
#[test]
fn bom__duplicate_keys_same_value_warn_conflict_errors() {
    // Same value twice: one W duplicate report, the bind itself legal.
    let (table, diags) = build_with(Some(&bom_block(
        "    b.slot = PART.SHAPE_V2\n    b.slot = PART.SHAPE_V2\n",
    )));
    let (slot_class, _) = row_of(&table, "main.b.slot");
    assert_eq!(slot_class, "PART.SHAPE_V2", "the first row wins the bind");
    let key_diags: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .collect();
    assert_eq!(key_diags.len(), 1, "one duplicate report");
    assert!(
        !matches!(key_diags[0].level, mcc::DiagnosticLevel::Error),
        "same-value duplicate is a Warning"
    );
    assert!(key_diags[0].msg.contains("keep one row"));

    // Conflicting values: one E duplicate report, first row still wins.
    let (table, diags) = build_with(Some(&bom_block(
        "    b.slot = PART.SHAPE_V2\n    b.slot = OTHER.THING\n",
    )));
    let (slot_class, _) = row_of(&table, "main.b.slot");
    assert_eq!(slot_class, "PART.SHAPE_V2", "the first row wins the bind");
    let key_diags: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .collect();
    assert_eq!(key_diags.len(), 1);
    assert!(
        matches!(key_diags[0].level, mcc::DiagnosticLevel::Error),
        "conflicting duplicate is an Error"
    );
    assert!(
        key_diags[0].msg.contains("first row wins"),
        "the message names the resolution: {}",
        key_diags[0].msg
    );
}

/// A parse-broken carrier leaves the registry empty — no slot binds, no
/// overlay ERC fires, the slot falls back to the existing unselected W — and
/// a lexical error in the carrier reports through the normal parse
/// diagnostic domain (E1000 anchored at the overlay file's own uri).
#[test]
fn bom__parse_broken_carrier_reports_and_clears() {
    // Truncated row: the parser discards the block; nothing binds.
    let (table, diags) = build_with(Some("bom main { b.slot = \n"));
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        0,
        "no row survives to bind"
    );
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT),
        0
    );
    let (slot_class, slot_unselected) = row_of(&table, "main.b.slot");
    assert_eq!(slot_class, "PART.SHAPE");
    assert!(
        slot_unselected,
        "the cleared registry leaves the slot unselected"
    );

    // A lexical error (integer literal over 20 digits) lands in the
    // diagnostic domain under the overlay uri, and the registry still clears.
    let (_, diags) = build_with(Some(&bom_block(
        "    b.slot = 99999999999999999999999999999999\n",
    )));
    let e1000 = diags.iter().filter(|d| d.code == 1000).count();
    assert!(e1000 >= 1, "the E1000 lands in the diagnostic domain");
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT),
        0,
        "the registry cleared"
    );
}

// U326②: the device-level DNP row (`path = DNP`). The code-face `@dnp`
// marker is retired (locked in `instance_dnp_marker.rs`), so the overlay is
// the only device-level DNP authority: a DNP row marks the part not fitted
// on the same flat-table flag the constructor `NC` argument writes — one
// verdict for every reader (A/B multiset).

/// The DNP board: same shape as `BOARD_SRC`, plus a concrete part for the
/// overlay's DNP row to name.
const DNP_BOARD_SRC: &str = r#"
abstract component PART.SHAPE
{
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

component PART.SHAPE_V2 : PART.SHAPE
{
    partno = "PS-V2"
}

component OTHER.THING
{
    pins = [
        in 1 = A
        out 2 = Y
    ]
}

module BOARD
{
    PART.SHAPE slot
    OTHER.THING fixed
}

module main
{
    BOARD b
}
"#;

/// `not_fitted` of the flat row at `path`.
fn fitted_of(table: &mcc::InstTable, path: &str) -> bool {
    let id = table
        .get_id_by_path(path)
        .unwrap_or_else(|| panic!("row {path} present"));
    table.get_entry(id).unwrap().not_fitted
}

/// Every flat row as (path, not_fitted), in path order — the A/B multiset.
fn fitted_map(table: &mcc::InstTable) -> Vec<(String, bool)> {
    let mut rows: Vec<(String, bool)> = table
        .iter()
        .map(|(_, e)| (e.path.clone(), e.not_fitted))
        .collect();
    rows.sort();
    rows
}

/// The diagnostic multiset, code + message, sorted — the A/B half that must
/// not move when the same verdict changes spelling.
fn diag_multiset(diags: &[mcc::McDiagnostic]) -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = diags
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    v.sort();
    v
}

/// A DNP row marks the named part not fitted and nothing else; the verdict
/// lands on the same flag the code face writes, and both overlay checks stay
/// silent (a consumed DNP key is not dangling).
#[test]
fn bom__dnp_row_marks_the_part_not_fitted() {
    let (table, diags) = build_with(Some(&bom_block("    b.fixed = DNP\n")));
    assert!(fitted_of(&table, "main.b.fixed"), "the row names the part");
    assert!(
        !fitted_of(&table, "main.b.slot"),
        "the sibling stays fitted"
    );
    assert_eq!(count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT), 0);
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        0
    );
}

/// A/B: the overlay DNP row and the constructor `NC` argument — the two
/// surviving not-fitted producers — are one verdict: the fitted map and the
/// whole diagnostic multiset are equal across the two spellings
/// (single-variable comparison, same board).
#[test]
fn bom__dnp_row_matches_nc_constructor_multiset() {
    let src_a = DNP_BOARD_SRC.replace(
        "    OTHER.THING fixed\n",
        "    OTHER.THING fixed(NC)\n",
    );
    let (table_a, diags_a) = build_with_src(&src_a, None);
    let (table_b, diags_b) = build_with_src(DNP_BOARD_SRC, Some(&bom_block("    b.fixed = DNP\n")));
    assert_eq!(fitted_map(&table_a), fitted_map(&table_b), "same verdict");
    assert_eq!(diag_multiset(&diags_a), diag_multiset(&diags_b), "same diags");
    assert!(
        fitted_map(&table_a).iter().any(|(p, f)| p == "main.b.fixed" && *f),
        "arm A really marks the part"
    );
}

/// A DNP row reaches named inline constructions too: `r1::PART.SHAPE()`
/// materializes through the inline-construction funnel, which consults the
/// same overlay authority as the declaration seams (U326② corpus migration —
/// the anonymous corpus parts get named and move to the overlay). An
/// anonymous product matches no key anyone can write, so it never dangles.
#[test]
fn bom__dnp_row_marks_a_named_inline_construction() {
    let src = DNP_BOARD_SRC.replace(
        "module main\n{\n    BOARD b\n}",
        "module BOARD2\n{\n    io N1\n    io N2\n    r1::PART.SHAPE() - N1 - N2\n}\n\nmodule main\n{\n    BOARD b\n    BOARD2 b2\n}",
    );
    assert!(src.contains("BOARD2"), "the splice landed");
    let overlay = "bom main {\n    b2.r1 = DNP\n}\n";
    let (table, diags) = build_with_src(&src, Some(overlay));
    assert!(
        fitted_of(&table, "main.b2.r1"),
        "the named inline construction is bom-addressable; diags: {:?}",
        diag_multiset(&diags)
    );
    assert_eq!(count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT), 0);
}

/// The DNP row's verdict is marking only: a part it names keeps its binding
/// face untouched (class, selection state) — a not-fitted part is still the
/// part that was chosen.
#[test]
fn bom__dnp_row_leaves_the_binding_face_untouched() {
    let (table, diags) = build_with(Some(&bom_block("    b.fixed = DNP\n")));
    let id = table.get_id_by_path("main.b.fixed").unwrap();
    let e = table.get_entry(id).unwrap();
    assert!(e.not_fitted, "the row marks the part");
    assert!(!e.unselected, "marking is not unselection");
    assert_eq!(count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT), 0);
}

/// One key, one row of each kind: the variant is chosen and the part is not
/// fitted — "a part chosen for the board and left off it".
#[test]
fn bom__slot_takes_selection_row_and_dnp_row_together() {
    let (table, diags) = build_with(Some(&bom_block(
        "    b.slot = PART.SHAPE_V2\n    b.slot = DNP\n",
    )));
    let id = table.get_id_by_path("main.b.slot").unwrap();
    let e = table.get_entry(id).unwrap();
    assert_eq!(e.class_name, "PART.SHAPE_V2", "the selection row binds");
    assert!(e.not_fitted, "the DNP row marks the part");
    assert!(!e.unselected, "a bound slot is selected");
    assert_eq!(count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT), 0);
}

/// A DNP row on a module instance marks the module's own entry — the subtree
/// push-down retired with U326①, so the mounted parts keep their own face.
#[test]
fn bom__dnp_row_on_module_instance_marks_the_module_only() {
    let (table, diags) = build_with(Some(&bom_block("    b = DNP\n")));
    assert!(fitted_of(&table, "main.b"), "the module entry is marked");
    assert!(
        !fitted_of(&table, "main.b.fixed"),
        "the mounted part keeps its own face"
    );
    assert!(!fitted_of(&table, "main.b.slot"));
    assert_eq!(count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT), 0);
}

/// Duplicate DNP rows judge by the same b1 shape: same value is a W, and the
/// first row still marks the part.
#[test]
fn bom__duplicate_dnp_rows_warn_and_first_row_wins() {
    let (table, diags) = build_with(Some(&bom_block(
        "    b.fixed = DNP\n    b.fixed = DNP\n",
    )));
    assert!(fitted_of(&table, "main.b.fixed"), "the first row wins");
    let key_diags: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .collect();
    assert_eq!(key_diags.len(), 1, "one duplicate report");
    assert!(
        !matches!(key_diags[0].level, mcc::DiagnosticLevel::Error),
        "same-value duplicate is a Warning"
    );
}

/// A DNP key that names no instance dangles into the b3 domain (E5068),
/// anchored at the overlay row.
#[test]
fn bom__dangling_dnp_key_fires_5068() {
    let (_, diags) = build_with(Some(&bom_block("    b.nope = DNP\n")));
    let b3: Vec<&mcc::McDiagnostic> = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .collect();
    assert_eq!(b3.len(), 1);
    assert!(
        b3[0].msg.contains("no instance"),
        "the message names the empty path: {}",
        b3[0].msg
    );
}

/// A DNP row under a header naming a top the build does not enter consumes
/// nothing and dangles, exactly like a selection row.
#[test]
fn bom__dnp_row_top_mismatch_dangles() {
    let (_, diags) = build_with(Some("bom other {\n    b.fixed = DNP\n}\n"));
    let b3 = diags
        .iter()
        .filter(|d| d.code == mcc::errcodes::BOM_KEY_NOT_SLOT)
        .count();
    assert_eq!(b3, 1, "the row dangles (E5068)");
}

/// The reservation is exact: a lowercase value is not the DNP word — it
/// routes to the selection namespace, and on an abstract slot resolves to no
/// live def (E5067).
#[test]
fn bom__dnp_word_is_case_exact() {
    let (_, diags) = build_with(Some(&bom_block("    b.slot = dnp\n")));
    assert_eq!(
        count_code(&diags, mcc::errcodes::BOM_VALUE_NOT_DESCENDANT),
        1,
        "a lowercase value is an unresolved selection, not a DNP row"
    );
    assert_eq!(count_code(&diags, mcc::errcodes::BOM_KEY_NOT_SLOT), 0);
}
