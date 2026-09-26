// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U308 Step 0 — pre-surgery locks for the reference-face convergence
//! (`McEndpoint` → `McRef`) and the port-face unification onto `OpdShape`.
//!
//! Three locks, landed **before** the change (the `defspace_golden.rs`
//! "first lock, then change" discipline):
//!
//! 1. [`reference_face`] — the three reference forms as the parser actually
//!    produces them, read structurally rather than through a display string.
//!    The batch renames the variants (`Single`/`List`/`Node` →
//!    `Name`/`Group`/`Ports`); **the structure below must not move**, only the
//!    spelling of the enum arms in this file.
//! 2. [`value_face`] — the end-to-end wiring of the collected and two-sided
//!    forms, read through `mcb_pass2_flat`. This is the batch's main
//!    acceptance face: the same corpus must wire the same nets after the
//!    surgery. Modelled on `tests/shard5/curly_option.rs`.
//! 3. [`user_visible_json`] — the `mcc parse -f json` tree vocabulary, which
//!    the dead-API purge (Step 4) must leave byte-identical.
//!
//! The forms were **probed against the pre-surgery binary** before pinning; the
//! two facts that decided the design and are therefore pinned here as cells:
//! a two-sided `{a|b}` keeps its member selection **on the reference face**
//! (`modldo{vin}` / `modldo{vout}`, not a bare `modldo`), and a one-element
//! bracket list survives as a one-element group rather than being folded.

// Family naming `{family}__{essence}` doubles the underscore (matrix §1).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McPhrase, McURI};

/// Two-pin resistor and a two-pin capacitor whose `Cap` func takes a two-member
/// port list (the shape `tests/shard3/vector_lane_pass1.rs` uses to make a
/// declared vector receiver legal).
const FIXTURES: &str = r#"
component RES2 {
    pins = [
        1 = 1
        2 = 2
    ]
}
component CAP {
    pins = [
        1 = 1
        2 = 2
    ]
    func Cap([n1, n2]) {
        n1 - this - n2
    }
}
module POWER_LDO()
{
    io vin
    io vout
}
"#;

/// Benign residues of a minimal fixture: the never-called/unused-func family
/// the sibling anchors already filter (`vec_r0_operator_encoding.rs`).
const BENIGN: &[u32] = &[5054, 5641, 5642, 5643];

fn shape(p: &McPhrase) -> String {
    match p {
        McPhrase::Lead(_) => "Lead".to_string(),
        McPhrase::Endpoint(ep) => ep_shape(ep),
        McPhrase::Series(v, d) => format!("Series({d:?})[{}]", shapes(v)),
        McPhrase::Parallel(v) => format!("Parallel[{}]", shapes(v)),
        McPhrase::Multiple(v) => format!("Multiple[{}]", shapes(v)),
        McPhrase::Group(g) => format!("Group[{}]", shapes(&g.opds)),
        McPhrase::Transposed(b) => format!("Transposed({})", shape(b)),
        McPhrase::Reversed(b) => format!("Reversed({})", shape(b)),
        McPhrase::Closure(_) => "Closure".to_string(),
        McPhrase::FuncCall(f) => format!("FuncCall({})", f.func_name),
        McPhrase::Member(b, _) => format!("Member({})", shape(b)),
    }
}

fn shapes(v: &[McPhrase]) -> String {
    v.iter().map(shape).collect::<Vec<_>>().join(", ")
}

/// A reference-face leaf, rendered through `Display`. `Display` is used
/// deliberately: it is the only rendering that shows whether a leaf still
/// carries its member selection (`modldo{vin}`) or has decayed to a bare base
/// name (`modldo`).
fn ep_shape(ep: &mcc::McEndpoint) -> String {
    match ep {
        mcc::McEndpoint::Single(r) => format!("Single({r})"),
        mcc::McEndpoint::List(l) => format!(
            "List[{}]",
            l.iter().map(ep_shape).collect::<Vec<_>>().join(", ")
        ),
        mcc::McEndpoint::Node { input, output } => format!(
            "Node(in[{}], out[{}])",
            input.iter().map(ep_shape).collect::<Vec<_>>().join(", "),
            output.iter().map(ep_shape).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Build `main` around `body`, returning its parsed stmts plus the non-benign
/// diagnostic codes.
fn parse_body(body: &str) -> (Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    common::init();
    let src = format!("{FIXTURES}module main {{\n{body}\n}}\n");
    let u = McURI::from("/mcc/u308-ref-convergence.mc");
    mcc::mcc_load_from_string(&u, &src);
    let inst = mcc::mcc_build(&McIds::from("main"), &u).expect("build main");
    let stmts = inst.def.stmts.iter().map(shape).collect();
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !BENIGN.contains(c))
        .collect();
    codes.sort_unstable();
    codes.dedup();
    (stmts, codes)
}

/// One statement, asserted to be the only one and to be quiet.
fn only(body: &str) -> String {
    let (stmts, codes) = parse_body(body);
    assert_eq!(codes, Vec::<u32>::new(), "`{body}` must be quiet; got {codes:?}");
    assert_eq!(stmts.len(), 1, "`{body}` must parse to one stmt; got {stmts:?}");
    stmts.into_iter().next().unwrap()
}

/// The lane-structured receiver of the only `FuncCall` in `body`.
fn receiver(body: &str) -> String {
    fn walk(p: &McPhrase, out: &mut Vec<String>) {
        match p {
            McPhrase::FuncCall(f) => {
                if let Some(c) = &f.caller {
                    out.push(shape(c));
                    walk(c, out);
                }
            }
            McPhrase::Series(v, _) | McPhrase::Parallel(v) | McPhrase::Multiple(v) => {
                for e in v {
                    walk(e, out)
                }
            }
            McPhrase::Group(g) => {
                for e in &g.opds {
                    walk(e, out)
                }
            }
            McPhrase::Transposed(b) | McPhrase::Reversed(b) | McPhrase::Member(b, _) => walk(b, out),
            McPhrase::Closure(c) => {
                for e in &c.body {
                    walk(e, out)
                }
            }
            McPhrase::Lead(_) | McPhrase::Endpoint(_) => {}
        }
    }
    let _lock = common::lock();
    common::reset();
    common::init();
    let src = format!("{FIXTURES}module main {{\n{body}\n}}\n");
    let u = McURI::from("/mcc/u308-ref-convergence.mc");
    mcc::mcc_load_from_string(&u, &src);
    let inst = mcc::mcc_build(&McIds::from("main"), &u).expect("build main");
    let mut out = Vec::new();
    for s in &inst.def.stmts {
        walk(s, &mut out);
    }
    assert_eq!(out.len(), 1, "expected exactly one FuncCall receiver; got {out:?}");
    out.into_iter().next().unwrap()
}

// Lock 1: the three reference forms, read structurally.

/// Form 1 — a bare name (`Name`). A two-pin part contributes a row (`1*2`),
/// but the *reference* face is just the name; the row lives on the value face.
#[test]
fn reference_face__bare_name_is_a_single_reference() {
    assert_eq!(
        only("    RES2 R101\n    RES2 R102\n    R101 - R102"),
        "Series(Undirected)[Single(R101), Single(R102)]"
    );
}

/// Form 2 — a bracket list (`Group`). A **one-element** list survives as a
/// one-element group: the reference face preserves the spelling and does not
/// fold a degenerate group into the bare name.
#[test]
fn reference_face__bracket_list_of_one_is_not_folded() {
    assert_eq!(
        only("    RES2 R101\n    RES2 R102\n    R101 - [R102]"),
        "Series(Undirected)[Single(R101), Multiple[Single(R102)]]"
    );
}

/// Form 2 nesting — a group's members are **references**, so a group may hold a
/// group. This is the property the batch's type convergence preserves: `Group`
/// is a reference container, never a value-face node.
#[test]
fn reference_face__groups_nest_as_references() {
    assert_eq!(
        only("    RES2 R101\n    RES2 R102\n    RES2 R103\n    [R101, [R102, R103]]"),
        "Multiple[Single(R101), Multiple[Single(R102), Single(R103)]]"
    );
}

/// Form 3 — the two-sided `{a|b}` spelling. The decisive half: each side keeps
/// the **member selection** on the reference face (`modldo{vin}` /
/// `modldo{vout}`), not a bare base name. If a future change reads the member
/// off the value face and drops it from the reference, this cell goes red.
#[test]
fn reference_face__two_sided_spelling_keeps_member_selection() {
    assert_eq!(
        only(
            "    io V5V\n    io V3V3\n    POWER_LDO modldo\n    V5V -> modldo{vin|vout} -> V3V3"
        ),
        "Series(LtoR)[Single(V5V), \
         Node(in[Single(modldo{vin})], out[Single(modldo{vout})]), \
         Single(V3V3)]"
    );
}

/// A reference-face leaf reaches the same spelling from the dotted form: the
/// member selection is a property of the reference, not of the `{a|b}` syntax
/// that produced it.
#[test]
fn reference_face__dotted_member_reads_as_the_same_reference() {
    assert_eq!(
        only(
            "    io V5V\n    io V3V3\n    POWER_LDO modldo\n    modldo.vin - modldo.vout"
        ),
        "Series(Undirected)[Single(modldo{vin}), Single(modldo{vout})]"
    );
}

/// The lane-structured receiver — a declared vector slice used as a method-call
/// receiver resolves to one reference per ordered member. The lane structure is
/// carried in the tree, never re-parsed from a display string (§11.3 ③).
#[test]
fn reference_face__declared_vector_receiver_is_lane_structured() {
    assert_eq!(
        receiver("    io VDD\n    io GND\n    CAP c[1:2](1)\n    c[1:2].Cap([VDD, GND])"),
        "List[Single(c1), Single(c2)]"
    );
}

// Lock 2: the value face — the batch's main acceptance surface.

/// The end-to-end wiring of the two-sided form, read through `mcb_pass2_flat`:
/// `{vin|vout}` must put `modldo.vin` on the left net and `modldo.vout` on the
/// right one. This is the batch's main acceptance face — the same corpus must
/// wire the same nets after the surgery.
#[test]
fn value_face__two_sided_spelling_wires_input_left_output_right() {
    let table = flat(&format!(
        "{FIXTURES}module main {{\n    io V5V\n    io V3V3\n    POWER_LDO modldo\n    \
         V5V -> modldo{{vin|vout}} -> V3V3\n}}\n"
    ));
    let paths = entry_paths(&table);
    assert!(
        paths.iter().any(|p| p.ends_with("modldo.vin")),
        "modldo.vin missing: {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("modldo.vout")),
        "modldo.vout missing: {paths:?}"
    );
    for net in table.get_nets() {
        for &pid in &net.points {
            let Some(e) = table.get_entry(pid) else { continue };
            if e.path.ends_with("modldo.vin") {
                assert_eq!(net.name, "V5V", "modldo.vin must join V5V");
            }
            if e.path.ends_with("modldo.vout") {
                assert_eq!(net.name, "V3V3", "modldo.vout must join V3V3");
            }
        }
    }
}

/// The N-member right side of `{a|b}` against a column: the node's right face
/// is a `2*1` column, so the `2*1 -> 2*1` row pairing stays legal (§5.2) and
/// every member survives in order — no broadcast, no dropped trailing member.
#[test]
fn value_face__n_member_side_pairs_with_a_column_in_order() {
    let table = flat(
        "module MCU()\n{\n    io MIC\n    io DAC_OUT\n    io SPK_MUTE\n}\n\
         module main\n{\n    io VIN\n    io VOUT\n    io VOUT2\n    MCU mcu513\n    \
         VIN -> mcu513{ MIC | DAC_OUT, SPK_MUTE } -> [VOUT, VOUT2]\n}\n",
    );
    let paths = entry_paths(&table);
    for member in ["mcu513.MIC", "mcu513.DAC_OUT", "mcu513.SPK_MUTE"] {
        assert!(
            paths.iter().any(|p| p.ends_with(member)),
            "{member} missing from the netlist: {paths:?}"
        );
    }
    for net in table.get_nets() {
        for &pid in &net.points {
            let Some(e) = table.get_entry(pid) else { continue };
            if e.path.ends_with("mcu513.MIC") {
                assert_eq!(net.name, "VIN");
            }
            if e.path.ends_with("mcu513.DAC_OUT") {
                assert_eq!(net.name, "VOUT", "row order must be preserved");
            }
            if e.path.ends_with("mcu513.SPK_MUTE") {
                assert_eq!(net.name, "VOUT2", "row order must be preserved");
            }
        }
    }
}

fn flat(source: &str) -> mcc::InstTable {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/u308-value-face.mc".to_string();
    mcc::mcc_load_from_string(&uri, source);
    let entry = mcc::McSpaceName {
        ident: McIds::from("main"),
        uri: mcc::uri_intern(&uri),
    };
    let (_, table) = mcc::mcb_pass2_flat(&entry, 1).expect("pass2_flat");
    table
}

fn entry_paths(table: &mcc::InstTable) -> Vec<String> {
    let mut out = Vec::new();
    for net in table.get_nets() {
        for &pid in &net.points {
            if let Some(e) = table.get_entry(pid) {
                out.push(e.path.clone());
            }
        }
    }
    out
}

// Lock 3: the user-visible JSON vocabulary.

/// The `kind` vocabulary of the `mcc parse --tree -f json` view, which the
/// rename must **not** leak into: `McEndpoint::Single`/`List`/`Node` are
/// internal arm names, while the reader sees `Endpoint`/`EndpointList`/`Node`.
/// Renaming the Rust enum (`Name`/`Group`/`Ports`) must leave this output
/// byte-identical — this cell is the reason the renderer at
/// `cmds/parse.rs::endpoint_label` keeps its own spelling.
#[test]
fn user_visible_json__three_forms_keep_their_rendered_kinds() {
    let tree = parse_tree(&format!(
        "{FIXTURES}module main {{\n    io V5V\n    io V3V3\n    RES2 R101\n    RES2 R102\n    \
         POWER_LDO modldo\n    R101 - R102\n    V5V -> modldo{{vin|vout}} -> V3V3\n}}\n"
    ));
    let mut kinds = Vec::new();
    collect_kinds(&tree, &mut kinds);
    for kind in ["Endpoint", "Node", "Series"] {
        assert!(
            kinds.iter().any(|k| k == kind),
            "the rendered tree must keep kind `{kind}`; got {kinds:?}"
        );
    }
    // The two-sided form renders as `Node` with an `input`/`output` pair, and
    // each side keeps its member selection in the label.
    let node = find_kind(&tree, "Node").expect("a Node node");
    assert_eq!(
        node["input"][0]["label"], "port:modldo{vin}",
        "the left side must keep its member selection"
    );
    assert_eq!(
        node["output"][0]["label"], "port:modldo{vout}",
        "the right side must keep its member selection"
    );
}

/// The lane-structured receiver renders as `EndpointList` — the third rendered
/// kind, and the one a one-element group would fold away if the reference face
/// ever collapsed a degenerate group.
#[test]
fn user_visible_json__lane_receiver_renders_as_endpoint_list() {
    let tree = parse_tree(
        "component CAP {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n    \
         func Cap([n1, n2]) {\n        n1 - this - n2\n    }\n}\n\
         module main {\n    io VDD\n    io GND\n    CAP c[1:2](1)\n    \
         c[1:2].Cap([VDD, GND])\n}\n",
    );
    let list = find_kind(&tree, "EndpointList").expect("an EndpointList node");
    let labels: Vec<String> = list["children"]
        .as_array()
        .expect("children")
        .iter()
        .map(|c| c["label"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        labels,
        vec!["component:c1".to_string(), "component:c2".to_string()],
        "one lane per ordered member, in source order"
    );
}

/// Run `mcc parse --tree -f json` over `source` and return the rendered view.
fn parse_tree(source: &str) -> serde_json::Value {
    use std::process::Command;
    let out = Command::new(env!("CARGO_BIN_EXE_mcc"))
        .args([
            "parse", "--code", source, "--local", "--pass1", "--pass2", "--tree", "--top", "main",
            "-f", "json",
        ])
        .output()
        .expect("run mcc parse");
    assert!(
        out.status.success(),
        "mcc parse exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse mcc JSON output");
    doc["result"]["view"]["data"].clone()
}

/// Every `kind` value anywhere in the rendered tree.
fn collect_kinds(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(k) = map.get("kind").and_then(|k| k.as_str()) {
                out.push(k.to_string());
            }
            for child in map.values() {
                collect_kinds(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_kinds(item, out);
            }
        }
        _ => {}
    }
}

/// The first object anywhere in the tree carrying `kind`.
fn find_kind<'a>(v: &'a serde_json::Value, kind: &str) -> Option<&'a serde_json::Value> {
    match v {
        serde_json::Value::Object(map) => {
            if map.get("kind").and_then(|k| k.as_str()) == Some(kind) {
                return Some(v);
            }
            map.values().find_map(|child| find_kind(child, kind))
        }
        serde_json::Value::Array(items) => items.iter().find_map(|i| find_kind(i, kind)),
        _ => None,
    }
}
