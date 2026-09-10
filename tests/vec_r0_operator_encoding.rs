// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! R0 (`vec-dianlu.md` §2.4, source-order and operator fidelity) — the
//! **representation anchor**.
//!
//! Until this file, the operator encoding had no direct test anchor: `mcc`
//! exposes no phrase tree from a string, so the `+` landing (S2 of
//! `mcd/doc/vector-conn-r0-implementation-design.md`) could only be observed
//! through a *diagnostic-code migration* in `vec_parallel_transposed_bridge.rs`
//! — an explicitly weaker proxy (§7.1 of that draft).
//!
//! The anchor turns out not to need new API. `McFunction.stmts` **is** the
//! parser product: `mc_func.rs` pushes `McPhrase::new(&subnode, ..)` straight
//! into it (`stmts.push(net)`), with no post-pass in between. So reading it
//! back shows the encoding the parser actually produced — including every
//! parse-time transform R0 is about.
//!
//! **What is asserted** is §2.4.1's `(node kind, direction)` table itself:
//! `+`⇒`Parallel`, `-`⇒`Series(_, Undirected)`, `->`⇒`Series(_, LtoR)`,
//! `<-`⇒`Series(_, RtoL)`, plus §2.4.3's ordering law (same-kind chains
//! flatten in source order, mixed directions do not flatten) and §2.4.4's
//! wrapping rule. Cells that record a **known violation as a baseline** say so
//! in their name and doc comment; they are not compliance claims.

// Family naming `{family}__{essence}` uses a doubled underscore (matrix §1).
#![allow(non_snake_case)]

mod common;

use mcc::{McEndpoint, McIds, McPhrase, McURI};

/// Plain two-pin resistor, pins `1 = 1` / `2 = 2`.
const RES2: &str = "component RES2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Plain two-pin capacitor, pins `1 = 1` / `2 = 2`.
const CAP2: &str = "component CAP2 {\n    pins = [\n        1 = 1\n        2 = 2\n    ]\n}\n";

/// Render a phrase as `Kind(Dir)[operand, ..]` — structure and order only,
/// so an assertion reads as §2.4.1's encoding table rather than as a snapshot
/// of `Debug`. Instance names come from `McInstance::get_name`.
fn shape(p: &McPhrase) -> String {
    match p {
        McPhrase::Lead => "Lead".to_string(),
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

fn ep_shape(ep: &McEndpoint) -> String {
    match ep {
        McEndpoint::Single(r) => r.base.get_name(),
        McEndpoint::List(l) => format!(
            "List[{}]",
            l.iter().map(ep_shape).collect::<Vec<_>>().join(", ")
        ),
        McEndpoint::Node { input, output } => format!(
            "Node(in[{}], out[{}])",
            input.iter().map(ep_shape).collect::<Vec<_>>().join(", "),
            output.iter().map(ep_shape).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Build a module whose `func M` holds `stmt`, and return the shapes of the
/// statements the parser produced for it, plus the non-benign diagnostic codes.
///
/// A func body is used (rather than the module body) because that is the
/// established extraction path (`vector_lane_pass1.rs`); both go through the
/// same `McPhrase::new`, so the encoding under test is the same one.
fn encoded(stmt: &str) -> (Vec<String>, Vec<u32>) {
    let _lock = common::lock();
    common::reset();
    let src = format!(
        "{RES2}{CAP2}module main {{\n    RES2 R101\n    RES2 R102\n    RES2 R103\n    CAP2 C1\n    func M() {{\n        {stmt}\n    }}\n}}\n"
    );
    let u = McURI::from("/mcc/r0-encoding.mc");
    mcc::mcc_load_from_string(&u, &src);
    let inst = mcc::mcc_build(&McIds::from("main"), &u).expect("build");
    let shapes = inst
        .def
        .funcs
        .find("M")
        .map(|f| f.stmts.iter().map(shape).collect())
        .unwrap_or_default();
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all()
        .iter()
        .map(|d| d.code)
        .filter(|c| !matches!(c, 5641 | 5642 | 5643 | 5054))
        .collect();
    codes.sort_unstable();
    codes.dedup();
    (shapes, codes)
}

/// One statement, assert it is the only one, and return its shape.
fn only(stmt: &str) -> String {
    let (shapes, codes) = encoded(stmt);
    assert_eq!(
        codes,
        Vec::<u32>::new(),
        "`{stmt}` must be quiet; got {codes:?}"
    );
    assert_eq!(
        shapes.len(),
        1,
        "`{stmt}` must parse to one stmt; got {shapes:?}"
    );
    shapes.into_iter().next().unwrap()
}

/// Like `only`, but tolerates exactly the listed diagnostic codes.
fn only_with(stmt: &str, allowed: &[u32]) -> String {
    let (shapes, codes) = encoded(stmt);
    assert_eq!(codes, allowed, "codes for `{stmt}`");
    assert_eq!(
        shapes.len(),
        1,
        "`{stmt}` must parse to one stmt; got {shapes:?}"
    );
    shapes.into_iter().next().unwrap()
}

// ── §2.4.1 the encoding table ───────────────────────────────────────────────

/// `-` ⇒ `Series(_, Undirected)`.
#[test]
fn dash__is_series_undirected() {
    assert_eq!(only("R101 - R102"), "Series(Undirected)[R101, R102]");
}

/// `->` ⇒ `Series(_, LtoR)`.
#[test]
fn arrow__is_series_l_to_r() {
    assert_eq!(only("R101 -> R102"), "Series(LtoR)[R101, R102]");
}

/// `<-` ⇒ `Series(_, RtoL)` — the direction half of the encoding. The operand
/// order is a separate question and is pinned by the baseline cell below.
#[test]
fn back_arrow__is_series_r_to_l() {
    let s = only("R101 <- R102");
    assert!(
        s.starts_with("Series(RtoL)["),
        "`<-` must encode as Series(_, RtoL); got {s}"
    );
}

/// `+` ⇒ `Parallel` — **the S2 landing**, directly anchored.
///
/// Before S2 the parser folded `X + Y'` into a `Series` through one of three
/// specialized arms, so `+` could be recorded as `-` (prohibition 2). With the
/// arms gone every `+` is a `Parallel`, whatever its operand shapes.
#[test]
fn plus__is_parallel_not_series() {
    assert_eq!(only("R101 + R102"), "Parallel[R101, R102]");
    assert_eq!(
        only("[R101, R102] + C1'"),
        "Parallel[Multiple[R101, R102], Transposed(C1)]"
    );
}

/// §2.4.2 prohibition 1: `+` keeps its operands in **written** order, both
/// ways round. The shunted operand's face is derived from which side it was
/// written on (§5.1), so a swap of the written order must show up here.
#[test]
fn plus__keeps_written_operand_order() {
    assert_eq!(
        only("[R101, R102] + C1'"),
        "Parallel[Multiple[R101, R102], Transposed(C1)]",
        "operand written second stays second"
    );
    assert_eq!(
        only("C1' + [R101, R102]"),
        "Parallel[Transposed(C1), Multiple[R101, R102]]",
        "operand written first stays first"
    );
}

/// `[...]` is an operand list — `Multiple`, not `Parallel` (it carries no
/// `+`; the two are different nodes and must not be conflated).
#[test]
fn brackets__are_multiple_not_parallel() {
    assert_eq!(only("[R101, R102]"), "Multiple[R101, R102]");
}

// ── §2.4.3 ordering law ─────────────────────────────────────────────────────

/// A same-direction chain flattens into **one** `Series` preserving source
/// order (the one transform §2.4.3 permits).
#[test]
fn chain__flattens_same_direction_in_source_order() {
    assert_eq!(
        only("R101 - R102 - R103"),
        "Series(Undirected)[R101, R102, R103]",
        "same-kind chain flattens without reordering"
    );
}

/// A chain that changes direction is **not** flattened: the inner `-` keeps
/// its own `Undirected` encoding nested inside the outer `LtoR`.
#[test]
fn mixed_directions__are_not_flattened() {
    assert_eq!(
        only("R101 - R102 -> R103"),
        "Series(LtoR)[Series(Undirected)[R101, R102], R103]",
        "different directions keep separate Series nodes"
    );
}

// ── §2.4.4 unary operators wrap ─────────────────────────────────────────────

/// `'` is a **wrapper**: the operand survives structurally underneath, not
/// rewritten into the operator.
#[test]
fn apost__wraps_without_rewriting() {
    assert_eq!(only("C1'"), "Transposed(C1)");
    // Parenthesized chains keep their group *and* their direction underneath.
    assert_eq!(
        only("(R101 -> R102)'"),
        "Transposed(Group[Series(LtoR)[R101, R102]])"
    );
}

/// `^` is a **wrapper** too (§2.4.4): the operand survives structurally
/// underneath, whatever its shape. Swapping a node's ports, swapping a two-pin
/// component's pins and reversing a chain's members are all applied when the
/// expression is *evaluated* (§2.4.5) — the parse tree only records the wrap.
///
/// This cell used to be a recorded baseline: the old caret arm fell into five
/// hand-written branches that rewrote the tree (reversing members, swapping
/// `Node` input/output, rewriting a two-pin component into a swapped `Node`),
/// and for a parenthesized chain it produced a **vestigial one-element**
/// `Series(Undirected)` — the `phrases.reverse()` ran on a single element,
/// so the members were never reversed and the operator survived as a
/// mislabelled wrapper.
#[test]
fn caret__wraps_without_rewriting() {
    assert_eq!(
        only("(R101 - R102)^"),
        "Reversed(Group[Series(Undirected)[R101, R102]])"
    );
    assert_eq!(only("R101^"), "Reversed(R101)");
    assert_eq!(
        only("(R101 - R102 - R103)^"),
        "Reversed(Group[Series(Undirected)[R101, R102, R103]])"
    );
}

/// `^` on an operand that carries no order to reverse is a **no-op**, but it
/// is still recorded as the operator that was written (E2903 warns about it).
#[test]
fn caret__on_orderless_operand_still_wraps() {
    // E2903 (`SHAPE_REVERSE_NOOP`) is raised at parse; the tree still records
    // the operator that was written.
    // `(A + B)`: two bare labels stack into a point (`1*1 + 1*1 = 1*1`), whose
    // two faces are the same element list. `A`/`B` are deliberately undeclared
    // — a bare label is a point, while a declared-but-unused port is
    // shape-by-use (unknown width) and would not present a point at all; the
    // E3136 that undeclared func-body labels raise is incidental to this cell.
    assert_eq!(
        only_with("(A + B)^", &[2903, 3136]),
        "Reversed(Group[Parallel[A, B]])"
    );
    // `C1'` is a column (`2*1`) — left face == right face.
    assert_eq!(only_with("C1'^", &[2903]), "Reversed(Transposed(C1))");
}

/// The other half of the same judgement: `R101 + R102` — two **two-pin** parts
/// — stacks into a `1*2` node (`1*2 +- 1*2 = 1*2`) whose faces are genuinely
/// different (left = the two pin-1s, right = the two pin-2s), so the reversal
/// is real and E2903 must **not** fire. The judgement is the shape one
/// (vec-dianlu.md §6.3 / eval.md §5.6), not "any `Parallel`".
#[test]
fn caret__on_twopin_parallel_is_a_real_reversal() {
    assert_eq!(
        only("(R101 + R102)^"),
        "Reversed(Group[Parallel[R101, R102]])",
        "a two-pin parallel is a `1*2` node: the reversal is real"
    );
}

/// `^^` is the identity — the wrapper nests, it does not accumulate state.
#[test]
fn caret__double_reverse_nests() {
    assert_eq!(only("R101^^"), "Reversed(Reversed(R101))");
}

// ── recorded baselines (known R0 violations — not compliance claims) ────────

/// **BASELINE, not compliance.** `<-` reorders its operands: the parser's
/// left-arrow arm builds the line as `[opd2, opd1]` (`mc_phrase.rs`, the
/// `MCAST_OPD_LEFTARROW` arm — its own comment reads "Result line order:
/// `[opd2, opd1]`").
///
/// §2.4.5's corollary rules the other way (vec-dianlu.md: direction is carried
/// by `ConnDir` alone, and a `Series`'s member list always equals the written
/// source order) — so `R101 <- R102` should be `Series(RtoL)[R101, R102]`,
/// with the reversal carried by the direction alone. Today it is
/// `[R102, R101]`, i.e. the reversal is encoded **twice** (direction *and*
/// order), which is §2.4.2 prohibition 1.
///
/// This is **not** a mechanical fix: the members being in flow order is
/// load-bearing. `audit_dc_binding_dir` (6028, `dc_binding_arrow_dir.rs`)
/// judges the direction word's position "arrow-glyph independent" *because*
/// `<-` swaps operands — its doc comment says so. Adjudication is open; this
/// cell pins the current shape so whichever way it is decided, the change is
/// visible.
#[test]
fn back_arrow__operand_order_BASELINE_reorders() {
    assert_eq!(
        only("R101 <- R102"),
        "Series(RtoL)[R102, R101]",
        "current parser product: operands reordered, direction also set"
    );
}

/// **BASELINE, not compliance.** `<-` also calls the parse-time tree
/// rewriters `set_left_in` / `set_right_out` (A2), which is why the operand
/// order above and the direction encoding come out of two separate mechanisms.
/// Retiring them is batch 2.
#[test]
fn arrow__still_drives_parse_time_rewriters() {
    // Both directions encode as `Series(_, dir)`; the difference between them
    // is carried by the direction, and — for `<-` — *additionally* by the
    // operand order. Batch 2 removes the second mechanism.
    assert_eq!(only("R101 -> R102"), "Series(LtoR)[R101, R102]");
    assert_eq!(only("R101 <- R102"), "Series(RtoL)[R102, R101]");
}

/// A transposed operand is classified by the shape it **transposes to**
/// (vec-dianlu.md §6.2 / §6.3), not by the `Transposed` node itself: a row
/// transposes into a column and a point into itself — both orderless — while a
/// column transposes into a **row**, whose two faces differ, so its reversal is
/// real. This is the shape layer (`eval_port_elems`) answering; the
/// representative layer (`get_left`/`get_right`, no `context`) stays
/// conservative on `Transposed` and is ledgered as a known residual.
#[test]
fn caret__on_transposed_operand_follows_the_transposed_shape() {
    // Row -> Column: orderless.
    assert_eq!(
        only_with("(R101 + R102)'^", &[2903]),
        "Reversed(Transposed(Group[Parallel[R101, R102]]))"
    );
    // Point -> Point: orderless (`A` is a bare label; the E3136 it raises as an
    // undeclared func-body label is incidental to this cell).
    assert_eq!(only_with("A'^", &[2903, 3136]), "Reversed(Transposed(A))");
    // Column -> Row: genuinely two-faced, so the reversal is real.
    assert_eq!(
        only("[VCC, GND]'^"),
        "Reversed(Transposed(Multiple[VCC, GND]))"
    );
}
