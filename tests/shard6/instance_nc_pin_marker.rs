// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for the instance-site per-pin NC marker (`@ncpin(…)`) — the third layer
// of NC (design: `erc/nc-design.md` §5, `attribute/status-design.md` §2.8;
// CIMP §1 U48).
//
// The marker is written as the declaration line's trailing attribute
// (`CHIP d1 @ncpin(1,3)`) and means "this pin, of *this* instance, is
// intentionally not connected". The netlist, the connections, the BOM and the
// viz are untouched — the "unconnected" diagnostic family reads it as a
// suppression, and since U305 (`nc-design.md` §4.3) `check_nc_connected` reads
// it as a prohibition: a marked terminal that gets wired reports E4109. That
// is why the net-face lock below compares the nets of a marked design against
// its unmarked twin.
//
// The measurement this file encodes (2026-09-15, `mcc show ast` + probes):
//   * The grammar reaches the trailer only through `mc_net: mc_phrase
//     mc_tattrs_opt` with `mc_phrase: mc_declare_a1` — the *single-instance*
//     form. The attribute therefore arrives as a **sibling of the
//     `MCAST_DECLARE` node** inside the net clause, which is what
//     `semantic::nc_pin::read_nc_pins` walks.
//   * A comma list (`CHIP d3, d4 @ncpin(1)`) is a syntax error (E2082), and in a
//     multi-clause body the recovery discards the neighbouring declarations.
//     A vector clause (`CHIP d[1:2] @ncpin(1)`) parses, and the marker covers
//     **every instance the clause expands to** (ruling ②).
//   * `@ncpin()` and `@ncpin(1,)` are syntax errors too — they never reach the
//     decoder, so 3158/3159 must not fire for them.
//
// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McIds;

// ── codes under test ──
const NC_PIN_LIST_MISSING: u32 = 3158;
const NC_PIN_VALUE_INVALID: u32 = 3159;
const MODULE_PORT_NOT_FOUND: u32 = 3175;
const COMPONENT_PIN_NOT_FOUND: u32 = 3179;
// ── the "unconnected" family the marker suppresses ──
const NET_INPUT_UNCONNECTED: u32 = 4108;
const NET_NC_CONNECTED: u32 = 4109;
const NET_OUTPUT_UNDRIVEN: u32 = 4110;
const NET_INSTANCE_UNCONNECTED: u32 = 4112;
const NET_MODULE_PORT_UNCONNECTED: u32 = 4114;
const NET_PARTIAL_CONNECTION: u32 = 4116;
const NET_BIDIR_UNCONNECTED: u32 = 4117;

/// A four-pin passive, all pins bidirectional.
const CHIP: &str = "component CHIP\n{\n    pins = [\n        io 1 = A\n        io 2 = B\n        io 3 = C\n        io 4 = D\n    ]\n}\n";

/// A three-pin part with real directions, so E4108 (input floating) and E4110
/// (output undriven) are reachable as well as E4117.
const DIRC: &str = "component DIRC\n{\n    pins = [\n        in  1 = A\n        out 2 = B\n        io  3 = C\n    ]\n}\n";

/// A sub-module whose ports use every id form: scalar, curly group, bracket list.
const LEAF: &str =
    "module Leaf()\n{\n    in  VIN\n    out VOUT\n    io  MIC{P,N}\n    io  [VDD_3V3, GND]\n}\n";

/// A part whose pin 1 is NC at class level (`nc` direction word).
const NCP: &str = "component NCP\n{\n    pins = [\n        nc 1 = A\n        io 2 = B\n    ]\n}\n";

/// Two pins NC at class level (`nc` word) plus one ordinary pin, so the mixed
/// instance — closed by both spellings at once — has members on either side.
const MIXP: &str = "component MIXP\n{\n    pins = [\n        nc 1 = A\n        nc 2 = B\n        in 3 = C\n    ]\n}\n";

/// A part whose pin 1 is merely *named* `NC` — no direction word anywhere.
const NAME_NC: &str =
    "component NAMED\n{\n    pins = [\n        io 1 = NC\n        io 2 = B\n    ]\n}\n";

struct Built {
    /// Paths of the entries flagged not-connected at the instance site — the
    /// structural fact, read straight off the flat table.
    marked: Vec<String>,
    /// `(net name, sorted point paths)`. The marker must never move these.
    nets: Vec<(String, Vec<String>)>,
    /// `(code, message)`, sorted, so assertions do not depend on report order.
    diags: Vec<(u32, String)>,
}

fn build(defs: &str, body: &str) -> Built {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/instance-nc-pin-marker.mc".to_string();
    let source = format!("{defs}\nmodule main\n{{\n{body}\n}}\n");
    mcc::mcc_load_from_string(&uri, &source);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");

    let mut marked: Vec<String> = table
        .iter()
        .filter(|(_, e)| e.nc_marked)
        .map(|(_, e)| e.path.clone())
        .collect();
    marked.sort();

    let mut nets: Vec<(String, Vec<String>)> = table
        .get_nets()
        .iter()
        .map(|net| {
            let mut pts: Vec<String> = net
                .points
                .iter()
                .filter_map(|p| table.get_entry(*p).map(|e| e.path.clone()))
                .collect();
            pts.sort();
            (net.name.clone(), pts)
        })
        .collect();
    nets.sort();

    let mut diags: Vec<(u32, String)> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| (d.code, d.msg.clone()))
        .collect();
    diags.sort();

    Built {
        marked,
        nets,
        diags,
    }
}

impl Built {
    fn marked_paths(&self) -> Vec<&str> {
        self.marked.iter().map(String::as_str).collect()
    }

    fn count(&self, code: u32) -> usize {
        self.diags.iter().filter(|(c, _)| *c == code).count()
    }

    /// The message of the sole diagnostic carrying `code`.
    fn only(&self, code: u32) -> String {
        let hits: Vec<&String> = self
            .diags
            .iter()
            .filter(|(c, _)| *c == code)
            .map(|(_, m)| m)
            .collect();
        assert_eq!(hits.len(), 1, "expected exactly one E{code}: {hits:?}");
        hits[0].clone()
    }

    /// Does the sole diagnostic carrying `code` name `needle`?
    fn reports(&self, code: u32, needle: &str) -> bool {
        self.diags
            .iter()
            .any(|(c, m)| *c == code && m.contains(needle))
    }
}

// ── 1. component face: pin ids, pin names, ranges ──

/// `@ncpin(1,3)` marks exactly pins 1 and 3: their "not connected" report
/// disappears, their neighbours' does not, and the partial-connection
/// denominator drops by the two.
#[test]
fn sem_instncpin__component_pin_ids() {
    let b = build(CHIP, "    CHIP d1 @ncpin(1,3)");
    assert_eq!(b.marked_paths(), ["main.d1.1", "main.d1.3"]);
    assert!(!b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(!b.reports(NET_BIDIR_UNCONNECTED, "main.d1.3"));
    // The two unmarked pins are still unconnected and still reported.
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.2"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.4"));
    assert_eq!(b.count(NET_BIDIR_UNCONNECTED), 2, "diags: {:?}", b.diags);
    // And they are the whole of the denominator now.
    assert_eq!(
        b.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 0 of 2 pins connected."
    );
}

/// The unmarked twin of the case above — without the marker all four pins
/// report and the denominator is whole. Locking only the marked case would pass
/// even if the suppression were a blanket "never report".
#[test]
fn sem_instncpin__component_pin_ids_unmarked_twin() {
    let b = build(CHIP, "    CHIP d1 @zzz(1,3)");
    assert!(b.marked.is_empty());
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.3"));
    assert_eq!(b.count(NET_BIDIR_UNCONNECTED), 4, "diags: {:?}", b.diags);
    assert_eq!(
        b.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 0 of 4 pins connected."
    );
}

/// A range is inclusive at both ends, and a pin *name* resolves through the
/// same authority a connection does (`A` -> pin `1`).
#[test]
fn sem_instncpin__range_and_pin_names() {
    let range = build(CHIP, "    CHIP d1 @ncpin(1:3)");
    assert_eq!(
        range.marked_paths(),
        ["main.d1.1", "main.d1.2", "main.d1.3"],
        "`1:3` must include both ends"
    );
    assert!(!range.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(range.reports(NET_BIDIR_UNCONNECTED, "main.d1.4"));

    let names = build(CHIP, "    CHIP d1 @ncpin(A,C)");
    assert_eq!(
        names.marked_paths(),
        ["main.d1.1", "main.d1.3"],
        "pin names must reach the same pins as the ids do"
    );
}

/// Every arm of the unconnected family the marker is allowed to suppress, on
/// pins that can actually produce them.
#[test]
fn sem_instncpin__unconnected_family_arms() {
    let control = build(DIRC, "    DIRC d1 @zzz(1)");
    assert_eq!(control.count(NET_INPUT_UNCONNECTED), 1);
    assert_eq!(control.count(NET_OUTPUT_UNDRIVEN), 1);
    assert_eq!(control.count(NET_BIDIR_UNCONNECTED), 1);

    let marked = build(DIRC, "    DIRC d1 @ncpin(1:3)");
    assert_eq!(marked.marked_paths().len(), 3);
    assert_eq!(marked.count(NET_INPUT_UNCONNECTED), 0, "{:?}", marked.diags);
    assert_eq!(marked.count(NET_OUTPUT_UNDRIVEN), 0, "{:?}", marked.diags);
    assert_eq!(marked.count(NET_BIDIR_UNCONNECTED), 0, "{:?}", marked.diags);

    // One of three marked: only that one is silenced, and its count leaves the
    // denominator.
    let one = build(DIRC, "    DIRC d1 @ncpin(1)");
    assert_eq!(one.marked_paths(), ["main.d1.1"]);
    assert_eq!(one.count(NET_INPUT_UNCONNECTED), 0);
    assert_eq!(one.count(NET_OUTPUT_UNDRIVEN), 1);
    assert_eq!(
        one.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 0 of 2 pins connected."
    );
}

// ── 2. module face: every port id form ──

/// A scalar port named by its own name.
#[test]
fn sem_instncpin__module_scalar_port() {
    let b = build(LEAF, "    Leaf m1 @ncpin(VIN)");
    assert_eq!(b.marked_paths(), ["main.m1.VIN"]);
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VIN"));
    // Its neighbours keep reporting — two of them, so the branch cannot pass by
    // suppressing the whole instance.
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VOUT"));
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.MIC.P"));
}

/// U305 ① on the module face: the ports join the same equipotential
/// partition as component pins, so the new law reads them through the same
/// rule — wiring a marked port reports E4109, the wire survives, and the
/// unmarked neighbours keep their own verdicts.
#[test]
fn sem_instncpin__module_marked_and_connected_is_a_violation() {
    let b = build(LEAF, "    Leaf m1 @ncpin(VIN)\n    in p1\n    p1 -> m1.VIN");
    assert_eq!(b.count(NET_NC_CONNECTED), 1, "{:?}", b.diags);
    assert!(b
        .diags
        .iter()
        .any(|(_, m)| m.contains("NC-marked terminal 'main.m1.VIN'")));
    // The wire survives; the unmarked neighbours keep reporting.
    assert_eq!(b.nets.len(), 1, "{:?}", b.nets);
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VOUT"));
}

/// Naming a grouping header covers the whole port. The header itself is
/// de-electrified and never reported on its own, so a marker that stopped at
/// the header would suppress nothing at all — a silent no-op.
#[test]
fn sem_instncpin__module_group_header_covers_members() {
    let b = build(LEAF, "    Leaf m1 @ncpin(MIC)");
    assert_eq!(
        b.marked_paths(),
        ["main.m1.MIC", "main.m1.MIC.N", "main.m1.MIC.P"]
    );
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.MIC.P"));
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.MIC.N"));
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VIN"));
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VOUT"));
}

/// Drawing the group and then naming a single member marks that member only —
/// the sibling member stays reported.
#[test]
fn sem_instncpin__module_group_member_only() {
    let b = build(LEAF, "    Leaf m1 @ncpin(MIC.P)");
    assert_eq!(b.marked_paths(), ["main.m1.MIC.P"]);
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.MIC.P"));
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.MIC.N"));
}

/// The curly group spelling expands to its members, in member order.
#[test]
fn sem_instncpin__module_group_spelling_expands() {
    let b = build(LEAF, "    Leaf m1 @ncpin(MIC{P,N})");
    assert_eq!(b.marked_paths(), ["main.m1.MIC.N", "main.m1.MIC.P"]);
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.MIC.P"));
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.MIC.N"));
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VIN"));
}

/// An anonymous list port (`io [VDD_3V3, GND]`) registers its members as flat
/// rows (`main.m1.VDD_3V3`) — the written list IS the member list, prefix or
/// no prefix, so the members survive instantiation instead of collapsing into
/// the synthetic header. A member name therefore addresses exactly that
/// member and silences its unconnected report; the sibling member stays
/// reported. The list spelling is the Set form: it marks every member it
/// names. The header itself still has no writable spelling — the sibling lock
/// below.
#[test]
fn sem_instncpin__bracket_port_member_addressing() {
    // A bare member name marks that member only.
    let b = build(LEAF, "    Leaf m1 @ncpin(VDD_3V3)");
    assert_eq!(b.marked_paths(), ["main.m1.VDD_3V3"]);
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VDD_3V3"));
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.GND"));

    // ... and the same by the other member.
    let b = build(LEAF, "    Leaf m1 @ncpin(GND)");
    assert_eq!(b.marked_paths(), ["main.m1.GND"]);
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.GND"));
    assert!(b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VDD_3V3"));

    let b = build(LEAF, "    Leaf m1 @ncpin([VDD_3V3,GND])");
    assert_eq!(b.marked_paths(), ["main.m1.GND", "main.m1.VDD_3V3"]);
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.VDD_3V3"));
    assert!(!b.reports(NET_MODULE_PORT_UNCONNECTED, "main.m1.GND"));
}

/// Even the synthetic header is out of reach: `@3` is not an operand the
/// attribute-value grammar accepts, so the attempt is a syntax error rather
/// than a marker diagnostic. Between this and the two misses above, the whole
/// bracket port is unreachable — and unreachable *loudly*, which is the point.
///
/// Note the header's own number is an internal counter (the same declaration
/// is `@1` in a design with one anonymous list port and `@3` in one with
/// three), so writing it would not be a stable spelling even if it parsed.
#[test]
fn sem_instncpin__bracket_port_has_no_spelling() {
    let b = build(LEAF, "    Leaf m1 @ncpin(@3)");
    assert!(b.marked.is_empty());
    assert_eq!(b.count(NC_PIN_LIST_MISSING), 0);
    assert_eq!(b.count(NC_PIN_VALUE_INVALID), 0);
    assert_eq!(b.count(MODULE_PORT_NOT_FOUND), 0);
    assert_eq!(b.count(COMPONENT_PIN_NOT_FOUND), 0);
}

// ── 3. malformed and unknown operands ──

/// A bare marker lists no pins at all — reported, and it marks nothing.
#[test]
fn sem_instncpin__bare_marker_reports() {
    let b = build(CHIP, "    CHIP d1 @ncpin");
    assert!(b.marked.is_empty());
    assert_eq!(b.count(NC_PIN_LIST_MISSING), 1);
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
}

/// Operands that are values but not pin identities. Each must report 3159, and
/// the part must behave exactly like an unmarked one.
#[test]
fn sem_instncpin__non_pin_operands_report() {
    for operand in ["1.5", "\"x\"", "HIGH", "this", "_", "0x1"] {
        let b = build(CHIP, &format!("    CHIP d1 @ncpin({operand})"));
        assert!(
            b.marked.is_empty(),
            "operand `{operand}` must mark nothing, got {:?}",
            b.marked
        );
        assert_eq!(b.count(NC_PIN_VALUE_INVALID), 1, "operand `{operand}`");
        assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    }
}

/// Two spellings never reach the decoder at all: they are syntax errors in the
/// attribute-value grammar. Locked so that nobody later "fixes" them into
/// 3158/3159 — the marker's own codes must stay quiet for them.
#[test]
fn sem_instncpin__unparsable_spellings_are_not_ours() {
    for operand in ["", "1,"] {
        let b = build(CHIP, &format!("    CHIP d1 @ncpin({operand})"));
        assert_eq!(b.count(NC_PIN_LIST_MISSING), 0, "operand `{operand}`");
        assert_eq!(b.count(NC_PIN_VALUE_INVALID), 0, "operand `{operand}`");
        assert!(b.marked.is_empty());
    }
}

/// A name that resolves to nothing is reported with the list of what the
/// instance does have — on both faces.
#[test]
fn sem_instncpin__unknown_operands_report() {
    let comp = build(CHIP, "    CHIP d1 @ncpin(NOSUCH,1)");
    // The resolvable member of the pair still marks; only the miss reports.
    assert_eq!(comp.marked_paths(), ["main.d1.1"]);
    assert!(comp.reports(COMPONENT_PIN_NOT_FOUND, "Available pins: [1, 2, 3, 4]"));
    assert!(comp.reports(COMPONENT_PIN_NOT_FOUND, "NOSUCH"));

    let range = build(DIRC, "    DIRC d1 @ncpin(9:11)");
    assert!(range.marked.is_empty());
    assert!(range.reports(COMPONENT_PIN_NOT_FOUND, "'9:11'"));

    let module = build(LEAF, "    Leaf m1 @ncpin(NOSUCH)");
    assert!(module.marked.is_empty());
    // The hint list reads the module's ports in **written order** (CIMP §1
    // U119): `Leaf` writes `VIN`, `VOUT`, `MIC`, then the anonymous
    // `io [VDD_3V3, GND]` that is synthesized as `@3`. Same four ports, the
    // author's order -- not the name order this lock used to state.
    assert!(module.reports(
        MODULE_PORT_NOT_FOUND,
        "Available ports: [VIN, VOUT, MIC, @3]"
    ));

    // A module port name is never a number, so a range can only miss there.
    let module_range = build(LEAF, "    Leaf m1 @ncpin(1:2)");
    assert!(module_range.marked.is_empty());
    assert!(module_range.reports(MODULE_PORT_NOT_FOUND, "'1:2'"));
}

/// The same written mistake in a clause that expands into two instances is
/// still one mistake: the report is deduplicated by position.
#[test]
fn sem_instncpin__clause_expansion_reports_once() {
    let b = build(CHIP, "    CHIP d[1:2] @ncpin(NOSUCH)");
    assert_eq!(b.count(COMPONENT_PIN_NOT_FOUND), 1, "{:?}", b.diags);
}

// ── 4. the connection-side verdict: a marked pin that is wired is a violation ──

/// U305 ① (`nc-design.md` §4.3, overturning U48 ruling ②): the single-instance
/// constraint is enforced symmetrically — a pin that is marked *and* wired is
/// reported (E4109 per wired marked terminal), the wire itself survives, and
/// because both marked pins are wired the denominator must **not** shrink.
#[test]
fn sem_instncpin__marked_and_connected_is_a_violation() {
    let body = "    CHIP d1 @ncpin(1,3)\n    in p1\n    in p2\n    p1 -> d1.1\n    p2 -> d1.3";
    let b = build(CHIP, body);
    assert_eq!(b.marked_paths(), ["main.d1.1", "main.d1.3"]);
    assert_eq!(b.count(NET_NC_CONNECTED), 2, "{:?}", b.diags);
    assert!(b
        .diags
        .iter()
        .any(|(_, m)| m.contains("NC-marked terminal 'main.d1.1'")));
    assert!(b
        .diags
        .iter()
        .any(|(_, m)| m.contains("NC-marked terminal 'main.d1.3'")));
    // Wired pins keep their nets …
    assert_eq!(b.nets.len(), 2, "{:?}", b.nets);
    // … and the two marked-but-wired pins stay in the denominator.
    assert_eq!(
        b.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 2 of 4 pins connected."
    );
}

/// Mixed: one marked pin is wired, one is not. The wired one is the violation
/// and stays in the denominator, the unwired one leaves it.
#[test]
fn sem_instncpin__denominator_counts_only_marked_and_unwired() {
    let b = build(CHIP, "    CHIP d1 @ncpin(1,3)\n    in p1\n    p1 -> d1.1");
    assert_eq!(b.count(NET_NC_CONNECTED), 1);
    assert_eq!(
        b.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 1 of 3 pins connected."
    );
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.2"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.4"));
    // The marked-and-unwired pin is silent.
    assert!(!b.reports(NET_BIDIR_UNCONNECTED, "main.d1.3"));
}

// ── 5. the marker never touches the netlist ──

/// **Pure suppression marker.** The same design with and without the marker
/// must produce the same nets, point for point; only the diagnostic set moves.
#[test]
fn sem_instncpin__nets_are_untouched() {
    let body_marked =
        "    CHIP d1 @ncpin(3,4)\n    in p1\n    in p2\n    p1 -> d1.1\n    p2 -> d1.2";
    let body_plain = "    CHIP d1 @zzz(3,4)\n    in p1\n    in p2\n    p1 -> d1.1\n    p2 -> d1.2";
    let marked = build(CHIP, body_marked);
    let plain = build(CHIP, body_plain);
    assert_eq!(marked.nets, plain.nets, "the marker moved the netlist");
    assert!(marked.nets.iter().any(|(n, _)| n == "p1"));
    assert!(marked.nets.iter().any(|(n, _)| n == "p2"));

    // The diagnostic face is the only thing that changed — and here it is the
    // whole point: the plain twin reports, the marked one does not.
    assert_eq!(
        plain.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 2 of 4 pins connected."
    );
    assert_eq!(
        marked.count(NET_PARTIAL_CONNECTION),
        0,
        "{:?}",
        marked.diags
    );
    assert_eq!(marked.count(NET_BIDIR_UNCONNECTED), 0, "{:?}", marked.diags);
}

// ── 6. clause expansion and class-level NC ──

/// Ruling ②: the trailer belongs to the clause, so it covers every instance the
/// clause expands into. There is no way to mark "just one" of them.
#[test]
fn sem_instncpin__vector_clause_covers_every_instance() {
    let b = build(CHIP, "    CHIP d[1:2] @ncpin(1)");
    assert_eq!(b.marked_paths(), ["main.d1.1", "main.d2.1"]);
    assert!(!b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(!b.reports(NET_BIDIR_UNCONNECTED, "main.d2.1"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.2"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d2.2"));
}

/// The marker belongs to the declaration it trails. `read_nc_pins` walks the
/// `next` sibling chain, so this is worth pinning: the neighbouring declaration
/// of the same body must come out unmarked, both in the table and in the
/// reports that speak about the *other* instance.
#[test]
fn sem_instncpin__marker_stays_with_its_own_declaration() {
    let b = build(CHIP, "    CHIP d1\n    CHIP d2 @ncpin(1)");
    assert_eq!(b.marked_paths(), ["main.d2.1"]);

    // The marked instance: pin 1 silent, its three neighbours still reported,
    // and the denominator drops by exactly the one marked-unwired pin.
    assert!(!b.reports(NET_BIDIR_UNCONNECTED, "main.d2.1"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d2.2"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d2.4"));
    assert!(
        b.reports(
            NET_PARTIAL_CONNECTION,
            "'main.d2' has 0 of 3 pins connected."
        ),
        "{:?}",
        b.diags
    );

    // The neighbour: nothing about it moved — the full four-pin denominator,
    // every pin still reported.
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.4"));
    assert!(b.reports(
        NET_PARTIAL_CONNECTION,
        "'main.d1' has 0 of 4 pins connected."
    ));

    // Neither instance is fully marked, so the "deliberately unwired" silence
    // does not apply to either of them.
    assert_eq!(b.count(NET_INSTANCE_UNCONNECTED), 2);
}

/// A pin that is already NC at class level is left alone by the marker, so the
/// two never overlap and nothing is counted twice. The marked and unmarked
/// designs must be *identical* here, diagnostics included.
#[test]
fn sem_instncpin__class_nc_pin_is_not_double_counted() {
    let marked = build(NCP, "    NCP d1 @ncpin(1)");
    let plain = build(NCP, "    NCP d1 @zzz(1)");
    assert!(
        !marked.marked_paths().contains(&"main.d1.1"),
        "a class-level NC pin must not carry the instance marker as well"
    );
    assert_eq!(marked.marked, plain.marked);
    assert_eq!(marked.diags, plain.diags);
    assert_eq!(
        marked.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 0 of 1 pins connected."
    );
}

/// Every pin marked ⇒ the instance is *deliberately* unwired, so E4112 ("no
/// pins connected to any net") is suppressed as well — it speaks about the
/// instance, and the marker is exactly the "this is on purpose" statement.
/// A partly marked instance keeps reporting it: the pins nobody marked still
/// want wiring.
#[test]
fn sem_instncpin__fully_marked_instance_reports_no_unwired_instance() {
    let plain = build(DIRC, "    DIRC d1 @zzz(1:3)");
    assert_eq!(plain.count(NET_INSTANCE_UNCONNECTED), 1);

    let listed = build(DIRC, "    DIRC d1 @ncpin(1,2,3)");
    assert_eq!(
        listed.marked_paths(),
        ["main.d1.1", "main.d1.2", "main.d1.3"]
    );
    assert_eq!(
        listed.count(NET_INSTANCE_UNCONNECTED),
        0,
        "{:?}",
        listed.diags
    );

    // The range spelling covers every pin of the instance too.
    let ranged = build(DIRC, "    DIRC d1 @ncpin(1:3)");
    assert_eq!(
        ranged.count(NET_INSTANCE_UNCONNECTED),
        0,
        "{:?}",
        ranged.diags
    );

    // One pin left out: that pin still wants wiring, so the report stands.
    let partial = build(DIRC, "    DIRC d1 @ncpin(1)");
    assert_eq!(
        partial.count(NET_INSTANCE_UNCONNECTED),
        1,
        "{:?}",
        partial.diags
    );
}

/// The two spellings of "intentionally unconnected" are one fact for E4112 as
/// well as for the pin-level family: an instance closed *partly* by the
/// class-level `nc` and *partly* by the instance marker is still wholly
/// deliberate, so nothing is reported. A class-level NC pin never carries
/// `nc_marked` (the idempotence lock above), so reading the instance marker
/// alone left this mixed instance loud — the boundary this test closes.
#[test]
fn sem_instncpin__class_nc_and_marker_close_one_instance_together() {
    // Pins 1/2 are class-level NC, pin 3 is closed by the marker: nothing is
    // connected and nothing of it is accidental.
    let mixed = build(MIXP, "    MIXP d1 @ncpin(3)");
    assert_eq!(mixed.marked_paths(), ["main.d1.3"]);
    assert_eq!(
        mixed.count(NET_INSTANCE_UNCONNECTED),
        0,
        "{:?}",
        mixed.diags
    );
    assert_eq!(mixed.count(NET_INPUT_UNCONNECTED), 0, "{:?}", mixed.diags);
    assert_eq!(mixed.count(NET_PARTIAL_CONNECTION), 0, "{:?}", mixed.diags);

    // Control: the very same class-NC pins, the ordinary pin left open — the
    // instance is not closed by anybody, so both reports stand.
    let open = build(MIXP, "    MIXP d1");
    assert_eq!(open.count(NET_INSTANCE_UNCONNECTED), 1, "{:?}", open.diags);
    assert_eq!(open.count(NET_INPUT_UNCONNECTED), 1, "{:?}", open.diags);
    assert_eq!(
        open.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 0 of 1 pins connected."
    );

    // Half-closed: the marker names the class-NC pin, which is a no-op — an
    // ordinary pin is still open, so "deliberate" must not leak to it.
    let half = build(MIXP, "    MIXP d1 @ncpin(1)");
    assert!(half.marked.is_empty(), "{:?}", half.marked);
    assert_eq!(half.count(NET_INSTANCE_UNCONNECTED), 1, "{:?}", half.diags);
    assert_eq!(half.count(NET_INPUT_UNCONNECTED), 1, "{:?}", half.diags);
}

// ── 7. a name carries no NC semantics (`erc/nc-design.md` §4.1, CIMP U47) ──

/// A name is a name: a pin called `NC` is an ordinary pin, so leaving it
/// unwired is reported exactly like its neighbour, and it stays in the E4116
/// denominator instead of leaving the instance looking smaller.
#[test]
fn sem_instncpin__name_nc_is_an_ordinary_pin() {
    let b = build(NAME_NC, "    NAMED d1");
    assert!(b.marked.is_empty());
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.2"));
    assert_eq!(b.count(NET_BIDIR_UNCONNECTED), 2, "diags: {:?}", b.diags);
    assert_eq!(
        b.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 0 of 2 pins connected."
    );
}

/// Both faces of the judgement read the direction word alone now: wiring a pin
/// named `NC` is ordinary (no E4109), while the `nc` twin contradicts the very
/// same connection — the asymmetry this ruling removed.
#[test]
fn sem_instncpin__one_predicate_on_both_faces() {
    let named = build(NAME_NC, "    io VDD\n    NAMED d1\n    d1.1 -> VDD");
    assert_eq!(named.count(NET_NC_CONNECTED), 0, "diags: {:?}", named.diags);
    assert!(named.reports(NET_BIDIR_UNCONNECTED, "main.d1.2"));
    assert_eq!(
        named.only(NET_PARTIAL_CONNECTION),
        "'main.d1' has 1 of 2 pins connected."
    );

    let worded = build(NCP, "    io VDD\n    NCP d1\n    d1.1 -> VDD");
    assert_eq!(
        worded.count(NET_NC_CONNECTED),
        1,
        "diags: {:?}",
        worded.diags
    );
}

/// The instance-site marker is what closes a pin merely named `NC` now: the
/// class-level arm it used to short-circuit on does not match it any more.
#[test]
fn sem_instncpin__marker_still_closes_a_name_nc_pin() {
    let b = build(NAME_NC, "    NAMED d1 @ncpin(1)");
    assert_eq!(b.marked_paths(), ["main.d1.1"]);
    assert!(!b.reports(NET_BIDIR_UNCONNECTED, "main.d1.1"));
    assert!(b.reports(NET_BIDIR_UNCONNECTED, "main.d1.2"));
}
