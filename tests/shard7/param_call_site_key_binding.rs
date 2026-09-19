// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Call-site named arguments (`CLASS inst( k = v )`) bind against
//! **two name faces** (contract-design.md §2.4, six rulings):
//!
//! 1. the formal parameters of the class signature, and
//! 2. the attribute keys the class's own body declares.
//!
//! A key whose value is a bare reference to a declared parameter
//! (`spec.Vout = vout`) is that parameter's second naming face, so an argument
//! through it binds the parameter. Any other key carries its call-site value to
//! the instance, replacing the definition's value there. `spec = [ Vout = vout ]`
//! and `spec.Vout = vout` are one fact written two ways (G2), so both spellings
//! reach the same verdict and produce the same value.
//!
//! Two guards, both hard errors at the instance's own declaration:
//! **collision** (one name claimed by a parameter *and* a key, or by two keys)
//! and **orphan** (a name on neither face). The bindable name is the key's bare
//! last segment: a key and a parameter share one namespace at the call site, so
//! only a bare name can collide — a dotted path such as `spec.volt` is not a
//! name there.
//!
//! The `pins` root is a third shape — a row, not an attribute key: it renames
//! pins the definition already declares, so it is exempt from both guards. An
//! id the definition never declared invents no pin and is the same bind failure
//! (E4176) at the call site; a class with dynamic pins closes no id set and is
//! not judged.
//!
//! Family naming `{family}__{essence}` uses a doubled underscore to separate
//! the grep-able family token from the essence (matrix §1 taxonomy).
//!
//! The named arguments are written either at the instance's own declaration
//! (`CLASS inst( … )`) or inline with no instance name (`CLASS( … )`);
//! both state one orphan once, anchored at the construction as written.
//!
//! A method call site (`inst.method( … )`) is the third position. Its key face
//! comes from the **receiver's class**, not from the func — a func declares no
//! attribute keys — so an assignment through such a key lands on the receiver
//! instance (a post-hoc rewrite: the receiver is built before the call). The
//! func's formals and the receiver's keys share one namespace there too, so the
//! same two guards apply, and both bind failures anchor at the call statement.
//! There the two faces are matched by name alone: a receiver key's formal link
//! points into its own class's formal table, not the func's, so it is dropped
//! rather than followed into the wrong table. Names are compared exactly.
#![allow(non_snake_case)]

use crate::common;

use mcc::{DiagnosticLevel, McIds};

const CODE: u32 = mcc::errcodes::INST_PARAM_BIND_FAILED;

/// One diagnostic, flattened to what the assertions read.
#[derive(Debug)]
struct Diag {
    code: u32,
    level: DiagnosticLevel,
    row: u32,
    col: u32,
    msg: String,
}

/// One build of `src`: every diagnostic, plus each `main` component's resolved
/// attributes rendered the way the renderers print them.
fn probe(src: &str, uri: &str) -> (Vec<Diag>, Vec<(String, Vec<String>)>) {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (tree, arena, store, _) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build");
    let diags: Vec<Diag> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| Diag {
            code: d.code,
            level: d.level,
            row: d.loc.row,
            col: d.loc.col,
            msg: d.msg.clone(),
        })
        .collect();
    let view = mcc::TreeView::new(&arena, &store);
    let attrs: Vec<(String, Vec<String>)> = view
        .components(&tree)
        .map(|c| {
            (
                c.name.clone(),
                c.resolved_attrs.iter().map(|a| a.to_string()).collect(),
            )
        })
        .collect();
    (diags, attrs)
}

fn bind_hits(diags: &[Diag]) -> Vec<&Diag> {
    diags.iter().filter(|d| d.code == CODE).collect()
}

/// The resolved attributes of instance `inst`.
fn attrs_of(attrs: &[(String, Vec<String>)], inst: &str) -> Vec<String> {
    attrs
        .iter()
        .find(|(name, _)| name == inst)
        .map(|(_, a)| a.clone())
        .unwrap_or_default()
}

/// The class header used by the spelling-equivalence rows: `Vout` is a key
/// whose value is the declared parameter `vout`, so `Vout = …` binds the
/// parameter.
const VOUT_HEAD: &str = "component C (vout::UV.VOLT = 3.3V) {\n";

const PINS: &str = "    pins = [\n        1 = A\n        2 = B\n    ]\n}\n";

/// A parameter-backed key binds that parameter: the dotted spelling.
#[test]
fn pa_keys__dotted_key_names_a_parameter() {
    let src = format!(
        "{VOUT_HEAD}    spec.Vout = vout\n{PINS}\nmodule main {{\n    C c1( Vout = 2.5V )\n}}\n"
    );
    let (diags, _) = probe(&src, "/mcc/keys-dotted-param.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
}

/// The table spelling is the same fact (G2) and must reach the same verdict.
#[test]
fn pa_keys__table_key_names_a_parameter() {
    let src = format!("{VOUT_HEAD}    spec = [\n        Vout = vout\n    ]\n{PINS}\nmodule main {{\n    C c1( Vout = 2.5V )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-table-param.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
}

/// Guard 2: an orphan name — on neither face — is an error, reported **once**
/// and **at the instance's own declaration**. The single-report part locks the
/// Pass2 duplicate away; the row part locks the row-1 collapse away.
#[test]
fn pa_keys__orphan_is_an_error_at_the_instance() {
    let src = "component C {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    C c1( nope = 5 )\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-orphan.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "one orphan is one fact; got {diags:?}");
    assert_eq!(hits[0].level, DiagnosticLevel::Error, "got {diags:?}");
    assert!(
        hits[0].msg.contains("nope"),
        "the message must name the orphan; got {}",
        hits[0].msg
    );
    assert!(
        hits[0].msg.contains("Unknown parameter"),
        "got {}",
        hits[0].msg
    );
    assert_eq!(
        hits[0].row, 9,
        "the error belongs on the instance declaration, not the file's first row; got {diags:?}"
    );
}

/// A class with neither parameters nor attribute keys: every named argument at
/// the call site is an orphan.
const BARE_HEAD: &str = "component C {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n";

/// The inline construction (no instance name) states one orphan **once**.
/// Pass1 anchors it at the construction's own node and Pass2 would re-derive it
/// at the enclosing statement's start, so the two anchors only coincide when
/// the construction opens the statement.
#[test]
fn pa_keys__inline_ctor_orphan_reports_once() {
    let src = format!("{BARE_HEAD}\nmodule main {{\n    C( nope = 5 ) -> GND\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-inline-head.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "one orphan is one fact; got {diags:?}");
    assert_eq!(hits[0].row, 9, "got {diags:?}");
}

/// The same orphan with the construction mid-statement: the two anchors sit at
/// different offsets here, and the surviving one is the construction itself —
/// not the statement start, which is a column to its left.
#[test]
fn pa_keys__inline_ctor_orphan_anchors_the_construction() {
    let stmt = "    GND -> C( nope = 1 )";
    let src = format!("{BARE_HEAD}\nmodule main {{\n{stmt}\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-inline-mid.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert_eq!(
        hits[0].col,
        stmt.find("C(").unwrap() as u32 + 1,
        "the anchor is the construction, not the statement start; got {diags:?}"
    );
}

/// Two bad constructions in one statement are two facts; the statement-wide
/// mute must not collapse them into one.
#[test]
fn pa_keys__two_inline_ctors_in_one_statement_report_twice() {
    let src = format!("{BARE_HEAD}\nmodule main {{\n    C( nope = 1 ) -> C( bogus = 2 )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-inline-two.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 2, "got {diags:?}");
    assert!(
        hits.iter().any(|h| h.msg.contains("nope")) && hits.iter().any(|h| h.msg.contains("bogus")),
        "each construction must name its own orphan; got {diags:?}"
    );
}

/// A construction inside a func body is expanded by Pass2 under the body's own
/// line, where Pass1 already anchored the orphan.
#[test]
fn pa_keys__inline_ctor_in_a_func_body_reports_once() {
    let src = "component C {\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\ncomponent W {\n    pins = [\n        1 = X\n        2 = Y\n    ]\n\n    func link() {\n        C( nope = 5 ) -> GND\n    }\n}\n\nmodule main {\n    W w1\n    w1.link()\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-inline-func.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert_eq!(
        hits[0].row, 15,
        "the anchor is inside the body, not the file's first row; got {diags:?}"
    );
}

/// Guard 1a: one name claimed by both a parameter and a key refers to nothing.
#[test]
fn pa_keys__formal_and_key_collision_errors() {
    let src = "component C (Vout::UV.VOLT = 3.3V) {\n    spec.Vout = Vout\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    C c1( Vout = 2.5V )\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-collision.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(
        hits[0].msg.contains("Ambiguous name") && hits[0].msg.contains("Vout"),
        "got {}",
        hits[0].msg
    );
}

/// Guard 1b: two keys of the same bare name `X`, under different containers,
/// are the same kind of collision and go through the same gate.
#[test]
fn pa_keys__two_containers_same_key_errors() {
    let src = "component C {\n    a.X = 1\n    b.X = 2\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    C c1( X = 3 )\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-two-containers.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(
        hits[0].msg.contains("Ambiguous name") && hits[0].msg.contains("'X'"),
        "got {}",
        hits[0].msg
    );
}

/// The bindable name is the key's bare last segment, so the dotted path is not
/// a name at the call site: `spec.volt = 9V` is an orphan, not a write.
#[test]
fn pa_keys__dotted_path_is_not_a_bindable_name() {
    let src = "component C {\n    spec.volt = 5V\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    C c1( spec.volt = 9V )\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-dotted-orphan.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(
        hits[0].msg.contains("spec.volt"),
        "the message must quote the spelling as written; got {}",
        hits[0].msg
    );
}

/// A literal-valued key has no parameter behind it, so the call site's value
/// replaces the definition's value on the instance — the dotted spelling.
#[test]
fn pa_keys__dotted_literal_key_is_assigned() {
    let src = "component C {\n    spec.volt = 5V\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    C c1( volt = 9V )\n}\n";
    let (diags, attrs) = probe(src, "/mcc/keys-dotted-assign.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
    assert_eq!(
        attrs_of(&attrs, "c1"),
        vec!["spec.volt = 9V".to_string()],
        "the call-site value must replace the definition's; got {attrs:?}"
    );
}

/// The table spelling assigns the same fact into the same row (G2 at the value
/// level, not just at the verdict level).
#[test]
fn pa_keys__table_literal_key_is_assigned() {
    let src = "component C {\n    spec = [\n        volt = 5V\n    ]\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    C c1( volt = 9V )\n}\n";
    let (diags, attrs) = probe(src, "/mcc/keys-table-assign.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
    assert_eq!(
        attrs_of(&attrs, "c1"),
        vec!["spec = [volt = 9V]".to_string()],
        "the table row must carry the call-site value; got {attrs:?}"
    );
}

/// The class whose key a method call may assign, plus a two-formal method.
const METHOD_CLASS: &str = "component C {\n    spec.volt = 5V\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    func link(a, b) {\n        a - b\n    }\n}\n";

/// The row of the first line containing `needle`, 1-based.
fn row_of(src: &str, needle: &str) -> u32 {
    src[..src.find(needle).unwrap()].matches('\n').count() as u32 + 1
}

/// A method call site binds against the receiver class's declared keys: the
/// assignment through `spec.volt` reaches the receiver instance, replacing the
/// value the definition gave it.
#[test]
fn pa_keys__method_assigns_a_receiver_class_key() {
    let src =
        format!("{METHOD_CLASS}\nmodule main {{\n    C c1\n    c1.link(GND, GND, volt = 9V)\n}}\n");
    let (diags, attrs) = probe(&src, "/mcc/keys-method-assign.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
    assert_eq!(
        attrs_of(&attrs, "c1"),
        vec!["spec.volt = 9V".to_string()],
        "the receiver must carry the call-site value; got {attrs:?}"
    );
}

/// Guard 2 at the method site: an orphan is an error, anchored at the call
/// statement — never collapsed to the file's first row.
#[test]
fn pa_keys__method_orphan_anchors_the_call() {
    let src =
        format!("{METHOD_CLASS}\nmodule main {{\n    C c1\n    c1.link(GND, GND, nope = 9V)\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-method-orphan.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "one orphan is one fact; got {diags:?}");
    assert!(hits[0].msg.contains("nope"), "got {}", hits[0].msg);
    assert_eq!(
        hits[0].row,
        row_of(&src, "c1.link"),
        "the anchor is the call statement, not the file's first row; got {diags:?}"
    );
}

/// The same anchor rule for an arity failure: a missing formal is stated once,
/// at the call statement.
#[test]
fn pa_keys__method_missing_formal_anchors_the_call() {
    let src = format!("{METHOD_CLASS}\nmodule main {{\n    C c1\n    c1.link(GND)\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-method-missing.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert_eq!(hits[0].level, DiagnosticLevel::Error, "got {diags:?}");
    assert!(
        hits[0].msg.contains("Missing required parameter") && hits[0].msg.contains('b'),
        "got {}",
        hits[0].msg
    );
    assert_eq!(hits[0].row, row_of(&src, "c1.link"), "got {diags:?}");
}

/// Guard 1 at the method site: a name claimed by the method's formal *and* by
/// the receiver class's key refers to nothing.
#[test]
fn pa_keys__method_formal_and_receiver_key_collision_errors() {
    let src = "component C {\n    spec.volt = 5V\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    func link(volt) {\n        volt - GND\n    }\n}\n\nmodule main {\n    C c1\n    c1.link(volt = 9V)\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-method-collision.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(
        hits[0].msg.contains("Ambiguous name") && hits[0].msg.contains("volt"),
        "got {}",
        hits[0].msg
    );
}

/// Negative control: the method's own formal name alone (no key of that name)
/// still binds by name, so the key face does not shadow the formal face.
#[test]
fn pa_keys__method_formal_name_still_binds() {
    let src = "component C {\n    spec.volt = 5V\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    func link(a) {\n        a - GND\n    }\n}\n\nmodule main {\n    C c1\n    c1.link(a = GND)\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-method-formal.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
}

/// The class whose key at the call site is a parameter-backed one
/// (`spec.Vout = vout`, a formal link), plus a method with its own formals.
const LINKED_CLASS: &str = "component C (vout::UV.VOLT = 3.3V) {\n    spec.Vout = vout\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    func link(a, b) {\n        a - b\n    }\n}\n";

/// A parameter-backed key at the method site is still a **receiver key**: it
/// matches by name, and its formal link is not followed into the func's own
/// formal table — the assignment lands on the receiver, where the definition's
/// value was.
#[test]
fn pa_keys__method_parameter_backed_key_assigns_the_receiver() {
    let src =
        format!("{LINKED_CLASS}\nmodule main {{\n    C c1\n    c1.link(GND, GND, Vout = 9V)\n}}\n");
    let (diags, attrs) = probe(&src, "/mcc/keys-method-linked-key.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
    assert_eq!(
        attrs_of(&attrs, "c1"),
        vec!["spec.Vout = 9V".to_string()],
        "the receiver must carry the call-site value, not the func's formal; got {attrs:?}"
    );
}

/// Names match exactly (case-sensitive): `Vout` names the receiver's key,
/// while the differently-cased `vout` names nothing at the call site — it is
/// neither a key nor a formal of `link`.
#[test]
fn pa_keys__method_key_name_is_exact() {
    let accepted =
        format!("{LINKED_CLASS}\nmodule main {{\n    C c1\n    c1.link(GND, GND, Vout = 9V)\n}}\n");
    let (diags, _) = probe(&accepted, "/mcc/keys-method-case-ok.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");

    let rejected =
        format!("{LINKED_CLASS}\nmodule main {{\n    C c1\n    c1.link(GND, GND, vout = 9V)\n}}\n");
    let (diags, _) = probe(&rejected, "/mcc/keys-method-case-bad.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(hits[0].msg.contains("vout"), "got {}", hits[0].msg);
}

/// The pin-name face of each `main` component: instance name → its pin ids with
/// the names the call site put on them, sorted so two spellings compare equal.
fn probe_pin_names(src: &str, uri: &str) -> Vec<(String, Vec<(String, Vec<String>)>)> {
    let _lock = common::lock();
    common::reset();
    let uri: mcc::McURI = uri.to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (tree, arena, store, _) =
        mcc::mcc_build_with_arena(&McIds::from("main"), &uri).expect("build");
    let view = mcc::TreeView::new(&arena, &store);
    view.components(&tree)
        .map(|c| {
            let mut rows: Vec<(String, Vec<String>)> = c
                .cond_pin_names
                .iter()
                .map(|(id, names)| (id.clone(), names.clone()))
                .collect();
            rows.sort();
            (c.name.clone(), rows)
        })
        .collect()
}

/// The pin-name face of instance `inst`.
fn pin_names_of(rows: &[(String, Vec<(String, Vec<String>)>)], inst: &str) -> Vec<String> {
    rows.iter()
        .find(|(name, _)| name == inst)
        .map(|(_, r)| {
            r.iter()
                .map(|(id, ns)| format!("{id} = {}", ns.join("|")))
                .collect()
        })
        .unwrap_or_default()
}

const NAMED_PINS: &str =
    "component C {\n    pins = [\n        1 = A\n        2 = B\n        3 = P\n    ]\n}\n";

/// `pins{6:9} = SWDBG` at a call site is the compact spelling of a definition
/// pin row: it names the pins it selects and is not judged against the
/// definition's formals or attribute keys (no orphan, no arity warning).
#[test]
fn pa_keys__pins_rooted_argument_names_pins() {
    let src = format!("{NAMED_PINS}\nmodule main {{\n    C c1( pins{{1:2}} = SWDBG )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-pins-row.mc");
    assert_eq!(
        diags.iter().filter(|d| d.code == CODE).count(),
        0,
        "a pins-rooted key is not an orphan; got {diags:?}"
    );
    assert_eq!(
        pin_names_of(&probe_pin_names(&src, "/mcc/keys-pins-row.mc"), "c1"),
        vec!["1 = SWDBG".to_string(), "2 = SWDBG".to_string()],
        "3 stays unnamed: the row names 1 and 2 only"
    );
}

/// A single id names one pin, and the name wins over the definition's own.
#[test]
fn pa_keys__pins_rooted_argument_overrides_the_declared_name() {
    let src = format!("{NAMED_PINS}\nmodule main {{\n    C c1( pins{{3}} = ALT )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-pins-one.mc");
    assert_eq!(
        diags.iter().filter(|d| d.code == CODE).count(),
        0,
        "got {diags:?}"
    );
    assert_eq!(
        pin_names_of(&probe_pin_names(&src, "/mcc/keys-pins-one.mc"), "c1"),
        vec!["3 = ALT".to_string()],
        "the call-site row is the instance's pin-name face"
    );
}

/// The exemption is structural, not a name match: `foo{6:9}` is an ordinary
/// key and stays an orphan.
#[test]
fn pa_keys__only_the_pins_root_is_exempt() {
    let src = format!("{NAMED_PINS}\nmodule main {{\n    C c1( foo{{1:2}} = SWDBG )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-pins-foo.mc");
    let hits: Vec<&Diag> = diags.iter().filter(|d| d.code == CODE).collect();
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(hits[0].msg.contains("foo{1:2}"), "got {}", hits[0].msg);
}

/// Naming an id the definition never declared invents no pin, so the row is a
/// bind failure at the call site — not a silent no-op. The class declares 1, 2
/// and 3, so 9 is unknown.
#[test]
fn pa_keys__pins_rooted_argument_naming_an_undeclared_pin_errors() {
    let src = format!("{NAMED_PINS}\nmodule main {{\n    C c1( pins{{9:9}} = GHOST )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-pins-absent.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "one unknown id is one fact; got {diags:?}");
    assert_eq!(hits[0].level, DiagnosticLevel::Error, "got {diags:?}");
    assert!(
        hits[0].msg.contains("pin row names pin 9"),
        "the message must name the unknown id; got {}",
        hits[0].msg
    );
    assert_eq!(
        hits[0].row, 10,
        "the error belongs on the instance declaration; got {diags:?}"
    );
    assert_eq!(
        pin_names_of(&probe_pin_names(&src, "/mcc/keys-pins-absent.mc"), "c1"),
        Vec::<String>::new(),
        "the error does not make the row invent a pin"
    );
}

/// A row that names several unknown ids states them in row order, deduplicated.
#[test]
fn pa_keys__pins_rooted_argument_names_every_undeclared_pin() {
    let src = format!("{NAMED_PINS}\nmodule main {{\n    C c1( pins{{8:9}} = GHOST )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-pins-range-absent.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(
        hits[0].msg.contains("pins 8, 9"),
        "the message must list the unknown ids; got {}",
        hits[0].msg
    );
}

/// An id a conditional block adds is declared: 3 exists only under
/// `partno == "one"`, and the declaration is judged closed, so the row binds.
const COND_PINS: &str = "component K (partno::STRING = \"one\") {\n    pins = [\n        1 = A\n        2 = B\n    ]\n\n    if (partno == \"one\")\n    {\n        pins += [\n            3 = P\n        ]\n    }\n}\n";

#[test]
fn pa_keys__pins_rooted_argument_accepts_a_conditional_pin() {
    let src = format!("{COND_PINS}\nmodule main {{\n    K(\"one\") k1( pins{{3:3}} = ALT )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-pins-cond.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
    assert_eq!(
        pin_names_of(&probe_pin_names(&src, "/mcc/keys-pins-cond.mc"), "k1"),
        vec!["3 = ALT".to_string()],
        "the conditional pin is declared, so the row names it"
    );
}

/// A class with dynamic pins closes no id set — its ids exist only once an
/// instance is built — so its pin rows escape the judgement.
const DYNAMIC_PINS: &str = "component D (n::INT) {\n    pins = [\n        1:n = 1:n\n    ]\n}\n";

#[test]
fn pa_keys__pins_rooted_argument_on_dynamic_pins_is_not_judged() {
    let src = format!("{DYNAMIC_PINS}\nmodule main {{\n    D(4) d1( pins{{9:9}} = GHOST )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-pins-dynamic.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
}

// The colon spelling of a named argument (`k: v`).
//
// intent-reference-layer-design.md §7 writes the key with a colon
// (`uc.power(io33: DVDD, core12: DCORE)`) and the srcv3 corpus follows it, while
// the call site only ever *built* `k = v`. A colon argument fell through to the
// colon-expression rules, whose node no argument reader recognises, so it
// reached neither face: never bound, and never reported either. Both spellings
// now build one shape, so each row below states one fact twice and the two
// halves must agree.

/// A diagnostic flattened to the fields a spelling comparison may read. The
/// file URI is deliberately absent: the two halves compared here are two
/// spellings of one fact, not two files.
fn spelling_key(diags: &[Diag]) -> Vec<(u32, String, u32, u32, String)> {
    diags
        .iter()
        .map(|d| {
            (
                d.code,
                format!("{:?}", d.level),
                d.row,
                d.col,
                d.msg.clone(),
            )
        })
        .collect()
}

/// One parameter-backed key, one literal key and a closed pin row: the three
/// faces a named argument can reach.
const COLON_CLASS: &str = "component C (vout::UV.VOLT = 3.3V) {\n    spec.Vout = vout\n    spec.volt = 5V\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n";

/// Both spellings of one argument are one fact: the same verdict at the same
/// position, and the same value on the instance.
#[test]
fn pa_keys__colon_and_equal_spellings_are_one_fact() {
    for (equal, colon) in [
        ("C c1( Vout = 2.5V )", "C c1( Vout: 2.5V )"),
        ("C c1( nope = 1 )", "C c1( nope: 1 )"),
        ("C c1( spec.volt = 9V )", "C c1( spec.volt: 9V )"),
        (
            "C c1( Vout = 2.5V, nope = 1 )",
            "C c1( Vout: 2.5V, nope: 1 )",
        ),
    ] {
        let (d_eq, a_eq) = probe(
            &format!("{COLON_CLASS}\nmodule main {{\n    {equal}\n}}\n"),
            "/mcc/keys-colon-spelling.mc",
        );
        let (d_co, a_co) = probe(
            &format!("{COLON_CLASS}\nmodule main {{\n    {colon}\n}}\n"),
            "/mcc/keys-colon-spelling.mc",
        );
        assert_eq!(
            spelling_key(&d_eq),
            spelling_key(&d_co),
            "`{equal}` and `{colon}` must reach the same verdict"
        );
        assert_eq!(
            a_eq, a_co,
            "`{equal}` and `{colon}` must reach the same value"
        );
    }
}

/// The colon spelling is judged, not swallowed: an orphan names nothing exactly
/// as its `=` twin does, once, at the instance's own declaration.
#[test]
fn pa_keys__colon_orphan_is_an_error_at_the_instance() {
    let src = format!("{COLON_CLASS}\nmodule main {{\n    C c1( nope: 1 )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-colon-orphan.mc");
    let hits = bind_hits(&diags);
    assert_eq!(
        hits.len(),
        1,
        "a swallowed argument reports nothing at all; got {diags:?}"
    );
    assert!(
        hits[0].msg.contains("nope") && hits[0].msg.contains("Unknown parameter"),
        "got {}",
        hits[0].msg
    );
    assert_eq!(
        hits[0].row,
        row_of(&src, "C c1( nope: 1 )"),
        "the error belongs on the instance declaration; got {diags:?}"
    );
}

/// A colon argument through a parameter-backed key binds that parameter, so the
/// instance carries the call-site value rather than the definition's.
#[test]
fn pa_keys__colon_binds_a_parameter_backed_key() {
    let src = format!("{COLON_CLASS}\nmodule main {{\n    C c1( Vout: 2.5V )\n}}\n");
    let (diags, attrs) = probe(&src, "/mcc/keys-colon-binds.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
    let got = attrs_of(&attrs, "c1");
    assert!(
        got.iter().any(|a| a.contains("2.5V")),
        "the call-site value must reach the instance; got {got:?}"
    );
}

/// Guard 1 under the colon spelling: a name claimed by both a parameter and a
/// key is ambiguous, exactly as it is under `=`.
#[test]
fn pa_keys__colon_formal_and_key_collision_errors() {
    let src = "component C (Vout::UV.VOLT = 3.3V) {\n    spec.Vout = Vout\n    pins = [\n        1 = A\n        2 = B\n    ]\n}\n\nmodule main {\n    C c1( Vout: 2.5V )\n}\n";
    let (diags, _) = probe(src, "/mcc/keys-colon-collision.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert!(
        hits[0].msg.contains("Ambiguous name") && hits[0].msg.contains("Vout"),
        "got {}",
        hits[0].msg
    );
}

/// The inline construction (no instance name) states the colon orphan once, at
/// the construction that wrote it.
#[test]
fn pa_keys__inline_ctor_colon_orphan_reports_once() {
    let src = format!("{BARE_HEAD}\nmodule main {{\n    C( nope: 1 )\n}}\n");
    let (diags, _) = probe(&src, "/mcc/keys-colon-inline.mc");
    let hits = bind_hits(&diags);
    assert_eq!(hits.len(), 1, "got {diags:?}");
    assert_eq!(hits[0].row, row_of(&src, "C( nope: 1 )"), "got {diags:?}");
}

/// A method call site: the colon spelling assigns the receiver class's key, so
/// the receiver carries the call-site value just as it does under `=`.
#[test]
fn pa_keys__method_colon_assigns_a_receiver_class_key() {
    let src =
        format!("{METHOD_CLASS}\nmodule main {{\n    C c1\n    c1.link(GND, GND, volt: 9V)\n}}\n");
    let (diags, attrs) = probe(&src, "/mcc/keys-method-colon.mc");
    assert_eq!(bind_hits(&diags).len(), 0, "got {diags:?}");
    assert_eq!(
        attrs_of(&attrs, "c1"),
        vec!["spec.volt = 9V".to_string()],
        "the receiver must carry the call-site value; got {attrs:?}"
    );
}
