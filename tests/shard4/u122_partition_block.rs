// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Integration test: the in-body partition `block` (CIMP §1 U122,
// `doc/arch/space/organization-units-design.md` §9.5).
//
// A partition is a **grouping and nothing else**. It opens no scope, declares
// no id and adds no level: the clauses written inside it belong to the body
// that encloses it, exactly as if the two braces were not there. That is the
// whole ruling, and it is what makes the feature cheap — the grammar carries
// one node, the semantics carry none, and the only thing that changes for a
// reader is that it must not *stop* at the partition.
//
// What is locked here:
//
//   1. Grouping is transparent in both directions and at any depth: a name
//      declared inside a partition is the enclosing body's declaration (it
//      resolves outside), and a name declared outside is visible inside. The
//      instance paths say the same thing structurally — they are the module's
//      own paths, with no segment for the partition, because there is no scope
//      for one to be named after.
//   2. Every body that holds clauses reads them through the one walk, not just
//      the module body: a partition in a component, an interface and
//      a capability is transparent too. Each is proved by the *content* it was
//      supposed to contribute (the pin table, the attribute, the signal table),
//      not by the absence of a complaint — a body that swallowed the partition
//      would be silent about the component pins and still silent about the
//      capability signals.
//   3. A partition must be named. `block { … }` is not a clause the grammar can
//      reduce, so it is reported — and, as everywhere else, the report does not
//      block the board: the clauses inside are read as the body's own, which is
//      what they would have been anyway. The twin (the same board with a name)
//      is silent, so the lock cannot pass on an engine that rejects every
//      partition — and no assertion here has to claim that a mistake erases the
//      circuit. The two boards' *rows* are deliberately not compared: after
//      recovery the unnamed one spells its pin rows differently (measured:
//      `main.R1/1` against the named board's `main.R1.1`), which is a fact
//      about the recoverable path and not about partitions.
//
// Deliberately **not** locked: that the level word is carried whole (`block`
// versus the family's other words is a grammar choice, not a runtime reading),
// and the readout of a partition's span in the organization directory (that
// directory is U120's surface and carries its own lock).

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use std::collections::HashMap;

use mcc::{DiagnosticLevel, McIds};

/// `PARSER_CLAUSE_INVALID` — "Invalid clause in a body", the report a `block`
/// with no name draws (the grammar has no reduction for it; the recoverable
/// clause error is what reaches the author).
const PARSER_CLAUSE_INVALID: u32 = 2082;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Diag {
    code: u32,
    level: DiagnosticLevel,
    msg: String,
}

struct Built {
    /// Every instance path in the flat table, mapped to the net name it sits
    /// on, or `None` when it sits on none.
    net_of_path: HashMap<String, Option<String>>,
    diags: Vec<Diag>,
}

/// Load `source` as one file and flatten `main`, collecting paths and reports.
///
/// The caller must hold [`common::lock`] for its whole body: the definition
/// space is process-global.
fn build(tag: &str, source: &str) -> Built {
    let uri = format!("/mcc/u122-{tag}.mc");
    common::reset();
    common::load_string(&uri, source);
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

    fn has_error(&self) -> bool {
        self.diags.iter().any(|d| d.level == DiagnosticLevel::Error)
    }

    fn net_of(&self, path: &str) -> Option<&str> {
        self.net_of_path.get(path).and_then(|n| n.as_deref())
    }

    /// Whether a name the flat table was walked over exists at all.
    fn has_path(&self, path: &str) -> bool {
        self.net_of_path.contains_key(path)
    }
}

/// A board whose whole body is partitioned: two sibling partitions, one nested
/// inner partition, and two statements outside every partition.
///
/// `R_out` is declared before the partitions and wired inside the nested one;
/// `R_in` is declared inside the first partition and wired outside. Both
/// directions are exercised, so neither "declaration escapes" nor "reference
/// seen from outside" alone can carry the lock.
const NESTED: &str = r#"component RES
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main(psnk GND)
{
    RES R_out
    block power
    {
        RES R_in
        R_in.1 -> GND
        R_in.2 -> GND
    }
    block shelved
    {
        block inner
        {
            R_out.1 -> GND
            R_out.2 -> GND
        }
    }
    R_in.1 -> R_out.1
}
"#;

// -- 1. grouping is transparent, in both directions and at any depth --

/// A name declared inside a partition is the enclosing body's own: the part
/// declared in `block power` is wired from *outside* it, and the part declared
/// outside is wired from inside a doubly nested partition.
///
/// The path assertion is the structural half. `main.R_in` and `main.R_out` are
/// the only two paths there can be — a partition that opened a scope would name
/// them `main.power.R_in` / `main.shelved.inner.R_out` instead, and a partition
/// that minted anything would show up as a third path here.
#[test]
fn u122__a_partition_groups_without_opening_a_scope() {
    let _guard = common::lock();
    let b = build("nested", NESTED);

    assert!(!b.has_error(), "a partition is legal syntax: {:?}", b.diags);
    assert_eq!(b.count(PARSER_CLAUSE_INVALID), 0, "diags: {:?}", b.diags);

    // The declaration made inside the partition is the module's, and the
    // declaration made outside it was visible inside the nested partition.
    assert!(b.has_path("main.R_in"), "paths: {:?}", b.net_of_path);
    assert!(b.has_path("main.R_out"), "paths: {:?}", b.net_of_path);
    // Nothing named after a partition: grouping is not a level.
    for p in b.net_of_path.keys() {
        assert!(
            !p.contains("power") && !p.contains("shelved") && !p.contains("inner"),
            "a partition became a path segment: {p}"
        );
    }

    // The wire written outside reaches the part declared inside, and the wire
    // written inside the nested partition reaches the part declared outside:
    // two directions, one net. The wired rows are the pins (an instance row
    // carries the part, not a wire), so the paths asserted are pin paths.
    let net = b
        .net_of("main.R_in.1")
        .expect("R_in's pin must be wired from outside the partition")
        .to_string();
    assert_eq!(
        b.net_of("main.R_out.1"),
        Some(net.as_str()),
        "the part declared outside must be wired from inside the nested partition"
    );
    assert_eq!(
        b.net_of("main.GND"),
        Some(net.as_str()),
        "the wires written in the partitions are on the same one net"
    );
}

// -- 2. every body that holds clauses is transparent, not only the module body --

/// One source holding a partition in each of the three other definition bodies,
/// each partition holding the clause that body exists for: pins in a component
/// and in an interface, a signal in a capability. (The fourth body the lock
/// used to cover, the `define`, retired with its keyword in b3953, U267③.)
///
/// Each is asserted on the **content it contributed**, so a body that stopped
/// at the partition cannot pass by saying nothing: the component and interface
/// pin tables would be empty, and the capability's signal table would hold
/// no port.
const EVERY_BODY: &str = r#"component CAP2
{
    block meta
    {
        pins = [
            1 = 1
            2 = 2
        ]
    }
}

interface IF2
{
    block meta
    {
        pins = [
            1 = A
            2 = B
        ]
    }
}

capability CAPX
{
    block meta
    {
        psnk VCC
        io SDA
    }
}

module main(psnk GND)
{
    CAP2 c1
    c1.1 -> GND
    c1.2 -> GND
}
"#;

#[test]
fn u122__a_partition_is_transparent_in_every_body() {
    let _guard = common::lock();
    let uri = "/mcc/u122-every-body.mc";
    common::reset();
    common::load_string(uri, EVERY_BODY);
    let _ = mcc::mcc_build_flat(&McIds::from("main"), &uri.to_string(), 1000);

    let diags: Vec<Diag> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| Diag {
            code: d.code,
            level: d.level,
            msg: d.msg.clone(),
        })
        .collect();
    assert!(
        !diags.iter().any(|d| d.level == DiagnosticLevel::Error),
        "diags: {diags:?}"
    );

    // component: the pin block written inside the partition is the component's
    // pin table.
    let components = mcc::definition_space().workspace_components();
    let cap2 = components
        .iter()
        .find(|(_, c)| c.name.to_string() == "CAP2")
        .map(|(_, c)| c.clone())
        .expect("CAP2 must be declared");
    let mut pins: Vec<String> = cap2.pins.names_to_id.keys().cloned().collect();
    pins.sort();
    assert_eq!(
        pins,
        vec!["1".to_string(), "2".to_string()],
        "component pins"
    );

    // interface: same shape, through the interface's own pin table.
    let ifs = mcc::definition_space().workspace_interfaces();
    let if2 = ifs
        .iter()
        .find(|(_, i)| i.name.to_string() == "IF2")
        .map(|(_, i)| i.clone())
        .expect("IF2 must be declared");
    let mut ipins: Vec<String> = if2.pins.names_to_id.keys().cloned().collect();
    ipins.sort();
    assert_eq!(ipins.len(), 2, "interface pins: {ipins:?}");

    // capability: the declared signal lives inside the partition and must be in
    // the capability's own signal table.
    let caps = mcc::definition_space().workspace_capabilities();
    let capx = caps
        .iter()
        .find(|(_, c)| c.name.to_string() == "CAPX")
        .map(|(_, c)| c.clone())
        .expect("CAPX must be declared");
    let mut signals: Vec<String> = capx
        .signals
        .iter_ports()
        .map(|(n, _)| n.to_string())
        .collect();
    signals.sort();
    assert!(
        signals.iter().any(|s| s == "VCC") && signals.iter().any(|s| s == "SDA"),
        "capability signals: {signals:?}"
    );
}

// -- 3. a partition must be named --

/// The same module twice: once with the partition named, once with the braces
/// bare. The bare one is refused and mints nothing; the named one is silent and
/// does.
///
/// The pair is what keeps this from being satisfied by "reject every block":
/// both boards put the *same* statement inside the braces, so only the name
/// differs.
fn board(name: Option<&str>) -> String {
    let head = match name {
        Some(n) => format!("block {n}"),
        None => "block".to_string(),
    };
    format!(
        r#"component RES
{{
    pins = [
        1 = 1
        2 = 2
    ]
}}

module main(psnk GND)
{{
    {head}
    {{
        RES R1
        R1.1 -> GND
    }}
}}
"#
    )
}

#[test]
fn u122__an_unnamed_partition_is_reported() {
    let _guard = common::lock();
    let unnamed = build("unnamed", &board(None));
    let named = build("named-twin", &board(Some("power")));

    // The report: the grammar has no clause to reduce, so the recoverable
    // clause error reaches the author — and it is an error, not a warning.
    assert!(
        unnamed
            .diags
            .iter()
            .any(|d| d.code == PARSER_CLAUSE_INVALID && d.level == DiagnosticLevel::Error),
        "an unnamed partition must be reported: {:?}",
        unnamed.diags
    );
    // The twin is the control: the same board with a name is silent, so the
    // report above comes from the missing name and nothing else.
    assert_eq!(
        named.count(PARSER_CLAUSE_INVALID),
        0,
        "diags: {:?}",
        named.diags
    );

    // The report is not a stop: the part written inside the refused clause is
    // still built, and still built as the *module's* own part — there is no
    // path segment for the partition either way, which is the same thing the
    // named twin shows (grouping is not a scope, so a clause the parser could
    // not reduce lands where one it could would have landed anyway).
    assert!(
        unnamed.has_path("main.R1"),
        "paths: {:?}",
        unnamed.net_of_path
    );
    for p in unnamed.net_of_path.keys() {
        assert!(
            !p.contains("power"),
            "a partition became a path segment: {p}"
        );
    }
}

/// The twin: the identical board with the partition named. Nothing is reported
/// and the part inside it is a real instance of the module.
#[test]
fn u122__a_named_partition_is_silent_and_instantiates() {
    let _guard = common::lock();
    let b = build("named", &board(Some("power")));

    assert_eq!(b.count(PARSER_CLAUSE_INVALID), 0, "diags: {:?}", b.diags);
    assert!(!b.has_error(), "diags: {:?}", b.diags);
    assert!(
        b.has_path("main.R1"),
        "the named partition's part must exist: {:?}",
        b.net_of_path
    );
    assert_eq!(
        b.net_of("main.R1.1"),
        b.net_of("main.GND"),
        "the statement inside it must be wired"
    );
}
