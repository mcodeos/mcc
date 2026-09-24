// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the organization directory (CIMP §1 U120,
// `doc/arch/space/organization-units-design.md` §8).
//
// The design's §8.4 finding was that the directory had no **one read**: the
// six def kinds were assembled by hand at each reader, and the three units
// that are not standalone defs — a func (a host member), a bus (no `DefId`),
// a clause (no declaration object) — had no read at all. What is locked here:
//
//   1. `DefinitionSpace::all_defs` walks exactly the kinds `DEF_KIND_ORDER`
//      names, in that order, and never yields a `Func` row (a func is its
//      host's member, so its read is the host-member one).
//   2. `org_unit_items` covers all eight class words, joins the def half and
//      the unit half in one place, and orders by `(kind, canonical key)`.
//   3. No row of the directory keys on a number: a key is a name, a
//      `host.member` pair or a source position, never an arena id.
//   4. Every branch of the row builders is filled: two hosts of each def kind,
//      two of each unit kind, a func name shared by two hosts, a clause with a
//      recorded end and one without, and a component with no funcs at all.
//
// The fixture is deliberately two-of-everything: a branch with one member (or
// none) lets the whole arm be bypassed while the suite still reports green.

// Family naming `{item}__{essence}` doubles the underscore to keep the
// grep-able token separate, matching the other lock files in this shard.
#![allow(non_snake_case)]

use crate::common;

use mcc::{DefKind, UnitKind};
use serde_json::Value;

const SOURCE: &str = r#"
recipe FILTER
{
    func smooth()
    {
        this.IN -> this.OUT
    }
}

recipe LIMIT
{
    func clamp()
    {
        this.OUT -> this.IN
    }
}

component AMP
{
    pins = [
        1 = A
        2 = B
    ]
    func gain()
    {
        this.A -> this.B
    }
    func reset()
    {
        this.B -> this.A
    }
}

component BUF
{
    pins = [
        1 = A
        2 = B
    ]
    func reset()
    {
        this.A -> this.B
    }
}

component NOP
{
    pins = [ 1 = 1 ]
}

component ZED
{
    pins = [ 1 = 1 ]
}

interface LINK
{
    pins = [ 1 = A ]
}

interface PORT
{
    pins = [ 1 = B ]
}

enum MODE { A, B }

enum KIND { C, D }

module main(psnk GND)
{
    out SIG{P, N}
    psnk PWR{VDD, GND}
    func drive()
    {
        SIG -> GND
    }
    func sense()
    {
        PWR -> SIG
    }
    AMP U_A
    AMP U_B
    SIG.P -> U_A.1
    U_A.2 -> U_B.1
}

module helper(psnk GND)
{
    out LANE{A, B}
    AMP U_C
    U_C.1 -> GND
    LANE.A -> GND
}
"#;

const URI: &str = "/mcc/org-directory.mc";

/// A **second** file for the order test alone.
///
/// One file cannot tell the directory's order — `(kind, canonical key)` over
/// the whole project — apart from a per-file order that happens to agree with
/// it (with one file, `component, func, clause` either way). This file repeats
/// the same three kinds, so a per-file order reads `component` in two runs and
/// fails the assertion below.
const AUX_URI: &str = "/mcc/org-directory-aux.mc";

const AUX_SOURCE: &str = r#"
component AUX
{
    pins = [ 1 = A ]
    func relay()
    {
        this.A -> this.A
    }
}

module aux_top(psnk GND)
{
    AUX U_X
    U_X.1 -> GND
}
"#;

/// The eight class words the directory can hold, in display order: the five
/// def kinds (no `Func` — a func is a host member) then the three unit kinds.
/// (The `define` kind retired with the keyword in b3953, U267③.)
const CLASS_WORDS: [&str; 8] = [
    "module",
    "component",
    "interface",
    "enum",
    "recipe",
    "func",
    "bus",
    "clause",
];

/// The rows of **this fixture**, out of a directory reading.
///
/// The definition space is process-global and the other files in this shard
/// load their own sources into it, some of them without taking
/// [`common::lock`] — so a reading taken here may hold rows of a design this
/// file never wrote. Every assertion below is about the fixture, so the read
/// is sliced by its URI first. (What the directory does with the *other*
/// files' rows is their own reading's business.)
fn of_fixture(rows: Vec<Value>) -> Vec<Value> {
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|r| r["uri"].as_str().is_some_and(|u| u.ends_with(URI)))
        .collect();
    assert!(
        !mine.is_empty(),
        "the fixture must contribute rows; a slice that drops them all would make every assertion below vacuous"
    );
    mine
}

/// Load the fixture once and hand back the directory, the def read and the
/// three unit readings — each sliced to the fixture. The caller holds
/// [`common::lock`] for its whole body: the definition space is global state.
fn directory() -> (
    Vec<Value>,
    Vec<(DefKind, String, String)>,
    Vec<Value>,
    Vec<Value>,
    Vec<Value>,
) {
    common::reset();
    common::load_string(URI, SOURCE);
    let defs: Vec<(DefKind, String, String)> = mcc::definition_space()
        .all_defs()
        .into_iter()
        .filter(|(_, sn, _)| sn.uri.to_string().ends_with(URI))
        .map(|(kind, sn, _)| (kind, sn.ident.to_string(), sn.uri.to_string()))
        .collect();
    assert!(
        defs.len() >= 10,
        "the fixture contributes two of every def kind: {defs:?}"
    );
    let unit = |kind: UnitKind| -> Vec<Value> {
        of_fixture(mcc::unit_rows(kind).into_iter().map(|r| r.json).collect())
    };
    let funcs = unit(UnitKind::Func);
    let buses = unit(UnitKind::Bus);
    let clauses = unit(UnitKind::Clause);
    (
        of_fixture(mcc::org_unit_items()),
        defs,
        funcs,
        buses,
        clauses,
    )
}

/// The rows of the fixture **and** of [`AUX_URI`], out of a directory reading.
///
/// The slice is widened to two files on purpose: with one file the order the
/// directory publishes cannot be told from a per-file grouping (see
/// [`AUX_URI`]). Rows of any other file the shard loaded are still dropped.
fn of_two_files(rows: Vec<Value>) -> Vec<Value> {
    let mine: Vec<Value> = rows
        .into_iter()
        .filter(|r| {
            r["uri"]
                .as_str()
                .is_some_and(|u| u.ends_with(URI) || u.ends_with(AUX_URI))
        })
        .collect();
    let uris: std::collections::BTreeSet<&str> = mine
        .iter()
        .map(|r| r["uri"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        uris.len(),
        2,
        "both files must contribute rows, else this slice is vacuous: {uris:?}"
    );
    mine
}

/// The directory over the two files, freshly loaded.
fn two_file_directory() -> Vec<Value> {
    common::reset();
    common::load_string(URI, SOURCE);
    common::load_string(AUX_URI, AUX_SOURCE);
    of_two_files(mcc::org_unit_items())
}

/// Every row of one class word, in the directory's order.
fn class_of<'a>(items: &'a [Value], class: &str) -> Vec<&'a Value> {
    items.iter().filter(|i| i["class"] == class).collect()
}

/// The `canon_key.path` of one row, which is what the directory orders by.
fn canon(row: &Value) -> &str {
    row["canon_key"]["path"]
        .as_str()
        .expect("every directory row carries canon_key.path")
}

/// The row whose `key` is `key` within `rows`.
fn by_key<'a>(rows: &'a [Value], key: &str) -> &'a Value {
    rows.iter()
        .find(|r| r["key"] == key)
        .unwrap_or_else(|| panic!("no row keyed '{key}' among {rows:?}"))
}

// 1. The def read walks DEF_KIND_ORDER and never yields a Func row.

#[test]
fn u120__all_defs_walks_def_kind_order_and_holds_no_func() {
    let _guard = common::lock();
    let (items, defs, _, _, _) = directory();

    // The order `all_defs` returns is the order of the const, walked as
    // written. Asserted as a sequence rather than as a set: the const is what
    // the read iterates, so a set comparison would not see a reordering.
    let walked: Vec<DefKind> = {
        let mut out: Vec<DefKind> = Vec::new();
        for (kind, _, _) in &defs {
            if out.last() != Some(kind) {
                out.push(*kind);
            }
        }
        out
    };
    assert_eq!(
        walked,
        mcc::DEF_KIND_ORDER.to_vec(),
        "all_defs must walk DEF_KIND_ORDER as written"
    );

    // A func is a member of its host (§12.1), so it is not a row of the def
    // read: including it would force every consumer to re-group rows by a
    // host link and would lose the host's own member order.
    assert!(
        !defs.iter().any(|(k, _, _)| *k == DefKind::Func),
        "all_defs must not yield Func rows: {defs:?}"
    );
    assert!(
        !items
            .iter()
            .any(|i| i["class"] == "func" && i["key"] == "main"),
        "the def half must not carry a func row"
    );
}

// 2. The directory covers every class word, and its order is (kind, canon).

#[test]
fn u120__directory_covers_every_class_word() {
    let _guard = common::lock();
    let (items, _, _, _, _) = directory();

    for word in CLASS_WORDS {
        // Two of every kind in the fixture: one member would let the whole
        // arm be bypassed (a 0-member branch reads as green).
        assert!(
            class_of(&items, word).len() >= 2,
            "class '{word}' must hold at least two rows: {:?}",
            class_of(&items, word)
        );
    }

    // The display order is the const's order, then the three unit kinds. The
    // sequence of first-seen classes is therefore exactly CLASS_WORDS.
    let mut seen: Vec<&str> = Vec::new();
    for row in &items {
        let class = row["class"].as_str().expect("every row carries a class");
        if seen.last() != Some(&class) {
            assert!(
                !seen.contains(&class),
                "class '{class}' appears in two runs; the order is not by kind"
            );
            seen.push(class);
        }
    }
    assert_eq!(seen, CLASS_WORDS.to_vec(), "directory display order");
}

#[test]
fn u120__order_is_kind_then_canonical_key_across_files() {
    let _guard = common::lock();
    let items = two_file_directory();

    // A class appearing in two runs means the order is per file (or per
    // anything else that cuts a kind in half), not per kind over the project.
    let mut seen: Vec<&str> = Vec::new();
    for row in &items {
        let class = row["class"].as_str().expect("every row carries a class");
        if seen.last() != Some(&class) {
            assert!(
                !seen.contains(&class),
                "class '{class}' appears in two runs over two files; the order groups by file, not by kind"
            );
            seen.push(class);
        }
    }

    // Non-vacuous: the second file contributes at least a component and a
    // func, so a per-file order would have cut `component` in two.
    assert!(
        seen.len() >= 3,
        "two files of three kinds must show at least three runs: {seen:?}"
    );

    // And the runs follow the display order, with the classes this fixture
    // does not hold skipped rather than reordered.
    let expected: Vec<&str> = CLASS_WORDS
        .iter()
        .copied()
        .filter(|w| seen.contains(w))
        .collect();
    assert_eq!(seen, expected, "the runs must follow the display order");
}

#[test]
fn u120__canonical_keys_are_unique_and_sorted_within_a_class() {
    let _guard = common::lock();
    let (items, _, _, _, _) = directory();

    let mut all: Vec<&str> = items.iter().map(canon).collect();
    let total = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), total, "canonical keys must be unique");

    for word in CLASS_WORDS {
        let rows = class_of(&items, word);
        if word == "clause" {
            // A clause's canonical key is a **position** `(uri, start)`, and
            // its `canon_key.path` is a spelling of that pair — not the key
            // itself. Ordered by the pair, not by the string: `…@1012` sorts
            // before `…@53` as text and after it as a position.
            let positions: Vec<(String, u64)> = rows
                .iter()
                .map(|r| {
                    (
                        r["uri"].as_str().unwrap().to_string(),
                        r["start"].as_u64().unwrap(),
                    )
                })
                .collect();
            let mut sorted = positions.clone();
            sorted.sort();
            assert_eq!(positions, sorted, "clauses must be in position order");
            continue;
        }
        let keys: Vec<&str> = rows.iter().map(|r| canon(r)).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(
            keys, sorted,
            "class '{word}' must be in canonical-key order"
        );
    }
}

// 3. No row keys on a number.

#[test]
fn u120__no_directory_key_is_a_number() {
    let _guard = common::lock();
    let (items, _, _, _, _) = directory();

    for row in &items {
        let key = row["key"].as_str().expect("every row carries a key");
        // A key that parses as an integer is an id wearing a name's clothes —
        // the shape design §0.1 forbids. The check is structural: it does not
        // enumerate the words a key may hold, it refuses the one shape that
        // means "this is a number".
        assert!(
            key.parse::<i64>().is_err(),
            "directory key '{key}' is a bare number"
        );
        assert!(
            !canon(row).is_empty(),
            "every row must carry a canonical key"
        );
    }

    // The three unit kinds each key on something a reader can write down: a
    // func on `host.func`, a bus on `host.bus`, a clause on its position.
    let (_, _, funcs, buses, clauses) = directory();
    for row in &funcs {
        assert_eq!(
            row["key"].as_str().unwrap(),
            format!(
                "{}.{}",
                row["host"].as_str().unwrap(),
                row["func"].as_str().unwrap()
            ),
            "a func keys on its (host, name) pair"
        );
    }
    for row in &buses {
        assert_eq!(
            row["key"].as_str().unwrap(),
            format!(
                "{}.{}",
                row["host"].as_str().unwrap(),
                row["bus"].as_str().unwrap()
            ),
            "a bus keys on its name in its host"
        );
    }
    for row in &clauses {
        assert_eq!(
            row["key"].as_str().unwrap(),
            format!(
                "{}@{}",
                row["uri"].as_str().unwrap(),
                row["start"].as_u64().unwrap()
            ),
            "a clause keys on its position"
        );
    }
}

// 4. Every branch of the three row builders is filled.

#[test]
fn u120__one_func_name_in_two_hosts_yields_two_rows() {
    let _guard = common::lock();
    let (_, _, funcs, _, _) = directory();

    // `reset` is declared on both AMP and BUF. A func name is unique only
    // within its host, so both rows survive and their keys differ — which is
    // exactly why a func row cannot key on the name alone.
    let named: Vec<&Value> = funcs.iter().filter(|r| r["func"] == "reset").collect();
    assert_eq!(named.len(), 2, "both hosts declare 'reset': {funcs:?}");
    let keys: Vec<&str> = named.iter().map(|r| r["key"].as_str().unwrap()).collect();
    assert!(keys.contains(&"AMP.reset"), "keys: {keys:?}");
    assert!(keys.contains(&"BUF.reset"), "keys: {keys:?}");

    // The host kind is the host's own kind, so a component method and a module
    // func are told apart by the row and not only by its key.
    assert_eq!(by_key(&funcs, "AMP.reset")["host_kind"], "component");
    assert_eq!(by_key(&funcs, "main.drive")["host_kind"], "module");
    assert_eq!(by_key(&funcs, "FILTER.smooth")["host_kind"], "recipe");
}

#[test]
fn u120__both_clause_shapes_are_present_and_say_what_they_know() {
    let _guard = common::lock();
    let (_, _, _, _, clauses) = directory();

    // A module-body statement records a full range; a function-body statement
    // records only its start. The rows say so (`end: null`) rather than
    // fabricating an end.
    let module_clause = clauses
        .iter()
        .find(|r| r["host"] == "main" && r["end"].is_number())
        .expect("a module-body clause carries an end");
    assert_eq!(
        module_clause["owner"], "",
        "a host-body clause has no owner"
    );

    let func_clause = clauses
        .iter()
        .find(|r| !r["owner"].as_str().unwrap_or("").is_empty())
        .expect("a function-body clause carries its owner");
    assert!(
        func_clause["end"].is_null(),
        "a function-body clause records no end: {func_clause}"
    );

    // Both hosts that carry statements are present: the module body and a func
    // body are two branches, and each needs more than one member.
    let hosts: Vec<&str> = clauses
        .iter()
        .filter_map(|r| r["host"].as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert!(hosts.contains(&"main"), "clause hosts: {hosts:?}");
    assert!(hosts.contains(&"AMP"), "clause hosts: {hosts:?}");
}

#[test]
fn u120__a_host_without_funcs_contributes_none_and_says_nothing() {
    let _guard = common::lock();
    let (items, _, funcs, buses, _) = directory();

    // NOP and ZED declare no func, LINK declares no func and no bus. The empty
    // case is a branch of its own — two hosts, because one member (or none)
    // lets the whole arm be bypassed while the suite still reports green: a
    // builder that invented a row here would report a member that does not
    // exist.
    for host in ["NOP", "ZED"] {
        assert!(
            !funcs.iter().any(|r| r["host"] == host),
            "{host} declares no func: {funcs:?}"
        );
    }
    assert!(
        !buses.iter().any(|r| r["host"] == "LINK"),
        "LINK declares no bus: {buses:?}"
    );

    // The def row is still there, with an empty funcs column rather than a
    // missing one.
    for host in ["NOP", "ZED"] {
        let row = items
            .iter()
            .find(|i| i["class"] == "component" && i["key"] == host)
            .unwrap_or_else(|| panic!("{host} is a def row"));
        assert_eq!(row["funcs"], serde_json::json!([]));
    }
}

#[test]
fn u120__a_bus_row_carries_its_members_and_its_host() {
    let _guard = common::lock();
    let (_, _, _, buses, _) = directory();

    let sig = by_key(&buses, "main.SIG");
    assert_eq!(sig["members"], serde_json::json!(["P", "N"]));
    assert_eq!(sig["host_kind"], "module");
    // A power port's member list is a bus too: the design's bus is a declared
    // member list, not a signal-only notion.
    let pwr = by_key(&buses, "main.PWR");
    assert_eq!(pwr["members"], serde_json::json!(["VDD", "GND"]));
}

// 5. The counts, and the one word that must not be among them.

#[test]
fn u120__counts_name_the_eight_classes_and_no_diagnostic_word() {
    let _guard = common::lock();
    let (items, _, _, _, _) = directory();
    let counts = mcc::org_unit_counts(&items);
    let map = counts.as_object().expect("counts is an object");

    let mut words: Vec<&str> = map.keys().map(String::as_str).collect();
    words.sort_unstable();
    let mut expected: Vec<&str> = vec![
        "modules",
        "components",
        "interfaces",
        "enums",
        "recipes",
        "funcs",
        "buses",
        "clauses",
    ];
    expected.sort_unstable();
    assert_eq!(words, expected);

    // This view reads a definition space, which carries no diagnostics: a
    // `diagnostics 0` word would read as a measurement rather than as "not
    // applicable".
    assert!(
        !map.contains_key("diagnostics"),
        "the directory must not report a diagnostic count"
    );

    // Every count equals the number of rows of that class, so the header and
    // the rows cannot disagree.
    assert_eq!(
        map["modules"],
        serde_json::json!(class_of(&items, "module").len())
    );
    assert_eq!(
        map["funcs"],
        serde_json::json!(class_of(&items, "func").len())
    );
    assert_eq!(
        map["clauses"],
        serde_json::json!(class_of(&items, "clause").len())
    );
}

// 6. Every row states where it is, and every func row sits in its file.

#[test]
fn u120__every_row_carries_a_source_location() {
    let _guard = common::lock();
    let (items, _, _, _, _) = directory();

    for row in &items {
        let loc = &row["loc"];
        assert!(loc.is_object(), "row has no loc: {row}");
        let uri = loc["uri"].as_str().unwrap_or("");
        assert!(
            uri.ends_with(URI),
            "loc.uri must name the fixture file: {loc}"
        );
        // The parse records an offset for every one of these units, so a line
        // of 0 here would mean an offset was lost rather than that the unit is
        // unsituated.
        assert!(
            loc["line"].as_u64().unwrap_or(0) > 0,
            "loc.line must be a real line: {loc}"
        );
    }
}

#[test]
fn u120__a_func_row_points_at_its_own_declaration_line() {
    let _guard = common::lock();
    let (_, _, funcs, _, _) = directory();

    // The line is derived from the fixture text rather than hardcoded, so a
    // fixture edit cannot silently invalidate the expectation.
    let line_of = |needle: &str| -> u64 {
        SOURCE
            .lines()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("fixture has no line containing '{needle}'")) as u64
            + 1
    };
    assert_eq!(
        by_key(&funcs, "main.drive")["loc"]["line"],
        serde_json::json!(line_of("func drive()"))
    );
    assert_eq!(
        by_key(&funcs, "AMP.gain")["loc"]["line"],
        serde_json::json!(line_of("func gain()"))
    );
}
