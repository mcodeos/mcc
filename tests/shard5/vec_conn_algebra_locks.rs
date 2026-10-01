// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection-algebra law locks (U372, `curly-wrapper-transpose-design.md` §5).
//!
//! The design doc's law tables (L1–L23) carry a 现状 column: 已对齐 faces are
//! locked here **green**, on the net partition, so every law cell that the
//! engine already honors has an executable anchor. Cells whose current state
//! is a **leg target** (leg1 wrapper dissolution, leg2 label-row face order,
//! leg3 node transpose) are deliberately absent — they land with their leg, so
//! the shard never carries a red that documents a known defect.
//!
//! Probe census source: design doc §2 (56 examples, mcc @ b4377); the `#N`
//! references in the comments point into that table.

// Family naming `{family}__{essence}` uses a doubled underscore to separate the
// grep-able family token from the essence (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::{McIds, McURI};

/// The U372 §2 probe fixture, in-process: a two-pin device (`T{A,K}`) twice,
/// plus the io label rail the probes wired against.
const FIXTURE: &str = "component T { pins = [ 1 = A\n    2 = K ] }\nmodule main {\n    io L0\n    io L1\n    io L2\n    io L3\n    io L4\n    io L9\n    io VCC\n    io GND\n    io P\n    io Q\n    io R\n    io S\n    T D1\n    T D2\n{}\n}\n";

/// Build the fixture with `statements` as the module body tail and return
/// (sorted diagnostic codes, normalized net partition) — same normalization as
/// `vec_r0_operator_fidelity`: net names are synthesized, the **grouping of
/// points** is the claim.
fn build(statements: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    build_src(&FIXTURE.replace("{}", statements))
}

/// The full-source variant of [`build`] — for locks whose statements must sit
/// at a body position the shared fixture cannot offer (e.g. a true statement
/// start, before the instance rows).
fn build_src(src: &str) -> (Vec<u32>, Vec<Vec<String>>) {
    let _lock = common::lock();
    common::reset();
    let uri = "/mcc/u372-conn-law.mc";
    let u = McURI::from(uri);
    mcc::mcc_load_from_string(&u, src);
    let (_, _, _, net_store) = mcc::mcc_build_with_nets(&McIds::from("main"), &u).expect("build");
    let mut codes: Vec<u32> = mcc::mcc_diagnose_all().iter().map(|d| d.code).collect();
    codes.sort_unstable();

    let mut partition: Vec<Vec<String>> = net_store
        .get("main")
        .map(|t| {
            t.iter()
                .map(|(_, pts)| {
                    let mut ps: Vec<String> = pts.iter().map(|p| p.path.clone()).collect();
                    ps.sort();
                    ps
                })
                .filter(|ps| !ps.is_empty())
                .collect()
        })
        .unwrap_or_default();
    partition.sort();
    (codes, partition)
}

/// True when some net carries both points.
fn wired(nets: &[Vec<String>], a: &str, b: &str) -> bool {
    nets.iter()
        .any(|n| n.iter().any(|p| p == a) && n.iter().any(|p| p == b))
}

/// L18 zip 位配对律 (#1, #4): a member vector zips **positionally** against the
/// other side — `{A,K}` lanes pair 1:1, they do not merge onto one endpoint.
/// The explicit-list spelling is the same vector (L14 基挂形).
#[test]
fn conn_law__member_vector_zips_positionally() {
    for (tag, stmt) in [
        ("curly", "    D1{A,K} - D2{A,K}"),
        ("list", "    D1{A,K} - [D2.A, D2.K]"),
    ] {
        let (codes, nets) = build(stmt);
        assert!(
            !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
            "{tag}: two lanes vs two lanes must zip clean; got {codes:?}"
        );
        assert!(
            wired(&nets, "D1.1", "D2.1") && wired(&nets, "D1.2", "D2.2"),
            "{tag}: zip must pair near-with-near; got {nets:?}"
        );
        assert!(
            !wired(&nets, "D1.1", "D2.2"),
            "{tag}: lanes must not cross; got {nets:?}"
        );
    }
}

/// L10 基数配对律 (#3): a transposed explicit list is a **row** (1×2); against
/// a two-lane vector the cardinality refuses with E4007 and nothing wires.
#[test]
fn conn_law__transposed_list_row_vs_two_lanes_is_refused() {
    let (codes, nets) = build("    D1{A,K} - [D2.A, D2.K]'");
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "row 1*2 vs column 2*1 must refuse with E4007; got {codes:?}"
    );
    assert!(
        !nets.iter().any(|n| n.iter().any(|p| p.starts_with("D1.")
            || p.starts_with("D2."))),
        "a refused statement wires nothing; got {nets:?}"
    );
}

/// L4 面序律, device side (#22): a row operand is a **directed two-terminal** —
/// series concatenation touches only the near face; the far face continues the
/// chain. `L0 - D1 - L9` wires L0→D1's left face (pin 1) and L9→its right face
/// (pin 2), and the two faces never short together.
#[test]
fn conn_law__device_row_series_touches_only_near_faces() {
    let (codes, nets) = build("    L0 - D1 - L9");
    assert!(
        wired(&nets, "L0", "D1.1") && wired(&nets, "L9", "D1.2"),
        "the chain must touch the near faces; got {nets:?}"
    );
    assert!(
        !nets.iter().any(|n| n.contains(&"L0".to_string())
            && n.contains(&"L9".to_string())),
        "the device's two faces must not short into one net; got {nets:?}"
    );
}

/// L4 面序律, label-row side (#21) — U372 leg2: a row operand between scalars
/// is still a **directed two-terminal**, exactly like the device row of the
/// cell above. `L0 - [L1,L2]' - L9` touches the near face (L1) with L0 and
/// lets the far face (L2) continue the chain to L9; the row's two faces never
/// short into one harness net.
#[test]
fn conn_law__label_row_series_touches_only_near_faces() {
    let (codes, nets) = build("    L0 - [L1,L2]' - L9");
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "scalar - row - scalar is a legal two-terminal chain; got {codes:?}"
    );
    assert!(
        wired(&nets, "L0", "L1") && wired(&nets, "L2", "L9"),
        "the chain must touch the near face and continue from the far face; got {nets:?}"
    );
    assert!(
        !wired(&nets, "L1", "L2"),
        "the row's two faces must not short together; got {nets:?}"
    );
    assert!(
        !wired(&nets, "L0", "L9"),
        "the row must not harness its neighbors into one net; got {nets:?}"
    );
}

/// L3 单点恒等律, spelling side (#23, #27, #28): the transpose wrapper and the
/// group form on a single-point label are the identity — and (unlike the `^`
/// spellings, whose E2903 noise leg1 narrows) these spellings are already
/// silent.
#[test]
fn conn_law__label_wrapper_identities_are_silent() {
    for (tag, stmt) in [
        ("apos", "    L1' - L2"),
        ("paren", "    (L1) - L2"),
        ("paren_apos", "    (L1)' - L2"),
    ] {
        let (codes, nets) = build(stmt);
        assert!(
            !codes.contains(&mcc::errcodes::SHAPE_REVERSE_NOOP),
            "{tag}: single-point identity must not warn; got {codes:?}"
        );
        assert!(
            wired(&nets, "L1", "L2"),
            "{tag}: the wrapper must be the identity; got {nets:?}"
        );
    }
}

/// L3, pin side (#30): `D1.pins.1'` is the same single point.
#[test]
fn conn_law__single_pin_transpose_is_identity() {
    let (_, nets) = build("    D1.pins.1' - L1");
    assert!(
        wired(&nets, "L1", "D1.1"),
        "the transposed pin reference must wire as itself; got {nets:?}"
    );
}

/// L2 对合律, group spelling (#9): `(X')'` composes back to the baseline — the
/// group-machine path is the reference truth for the involution (the adjacent
/// spelling `X''` is leg4's grammar leg).
#[test]
fn conn_law__group_involution_composes_to_baseline() {
    let (baseline_codes, baseline_nets) = build("    D1{A,K} - D2{A,K}");
    let (codes, nets) = build("    D1{A,K} - (D2{A,K}')'");
    assert_eq!(
        codes, baseline_codes,
        "the involution must diagnose exactly like the baseline"
    );
    assert_eq!(
        nets, baseline_nets,
        "(X')' must wire exactly like X; got {nets:?} vs {baseline_nets:?}"
    );
}

/// L2 对合律, adjacent spelling (#47) — U372 leg4: the lexer now reads `''`
/// as two transpose suffixes (the old rule lexed the pair as an empty
/// single-quoted string and swallowed the rest of the line), so the
/// involution composes on the operand face exactly like the group spelling:
/// `X''` diagnoses and wires exactly like X.
#[test]
fn conn_law__adjacent_involution_composes_to_baseline() {
    let (baseline_codes, baseline_nets) = build("    L1 - D2{A,K}");
    let (codes, nets) = build("    L1 - D2{A,K}''");
    assert_eq!(
        codes, baseline_codes,
        "the involution must diagnose exactly like the baseline"
    );
    assert_eq!(
        nets, baseline_nets,
        "X'' must wire exactly like X; got {nets:?} vs {baseline_nets:?}"
    );
    // and the adjacent spelling ≡ the group spelling `(X')'` (the group
    // machine path is the reference truth, L2).
    let (grp_codes, grp_nets) = build("    L1 - (D2{A,K}')'");
    assert_eq!(
        (&codes, &nets),
        (&grp_codes, &grp_nets),
        "the adjacent spelling ≡ the group spelling; got {nets:?} vs {grp_nets:?}"
    );
}

/// L2, label side: `''` between labels is two transpose suffixes composing to
/// the identity, and the old empty-string cascade (E2082/E2115) is gone.
#[test]
fn conn_law__adjacent_involution_label_is_identity() {
    let (codes, nets) = build("    L1'' - L2");
    assert!(
        !codes.contains(&mcc::errcodes::PARSER_CLAUSE_INVALID)
            && !codes.contains(&mcc::errcodes::PARSER_EMPTY_BODY),
        "the empty-string cascade must not fire; got {codes:?}"
    );
    assert!(
        wired(&nets, "L1", "L2"),
        "the label involution must be the identity; got {nets:?}"
    );
}

/// L2, chain side: `X''` ≡ `(X')'` byte-for-byte inside a chain, and both
/// wire like the bare operand. (The transposed spellings *diagnose* one
/// E4007 where the bare operand diagnoses two: the transpose-bridge
/// withholding the whole chain is shared by every transposed spelling,
/// including the pre-existing group form — not an involution cell.)
#[test]
fn conn_law__adjacent_involution_matches_group_spelling_in_chains() {
    for (bare, dbl, grp) in [
        (
            "    L0 - D2{A,K} - L9",
            "    L0 - D2{A,K}'' - L9",
            "    L0 - (D2{A,K}')' - L9",
        ),
        (
            "    L0 - [L1,L2] - L9",
            "    L0 - [L1,L2]'' - L9",
            "    L0 - ([L1,L2]')' - L9",
        ),
    ] {
        let (dbl_codes, dbl_nets) = build(dbl);
        let (grp_codes, grp_nets) = build(grp);
        assert_eq!(
            (&dbl_codes, &dbl_nets),
            (&grp_codes, &grp_nets),
            "adjacent ≡ group in a chain; got {dbl_codes:?} vs {grp_codes:?}"
        );
        let (_, bare_nets) = build(bare);
        assert_eq!(
            &dbl_nets, &bare_nets,
            "the involution wires like the bare operand; got {dbl_nets:?} vs {bare_nets:?}"
        );
    }
}

/// L14 brace ≡ list law (#40, #41, #42) — U372 leg5: the bare comma brace
/// opens as a member vector, the SAME tree as the bracket list (ruling ⑦),
/// so the two spellings diagnose and wire identically in every position the
/// two share: the rhs operand, and the group face (where the list already
/// refuses E3136, the brace inherits the same verdict). Before leg5 the brace
/// died E2082 at `{` (no statement-level production; the pipe form `{a|b}`
/// always had its bare arm, U339).
///
/// The statement-start position is NOT shared: there the `{` is claimed by
/// the declaration-position member face (`T D2 {L1,L2}`, U343-C2, bison arm
/// `@phr.dcla1{,}`) — the vector arm never competes with it, at HEAD or with
/// leg5. That boundary has its own lock below.
#[test]
fn conn_law__bare_curly_equals_bracket_list() {
    for (tag, list, brace) in [
        ("rhs", "    [P,Q] - [L1,L2]", "    [P,Q] - {L1,L2}"),
        ("group", "    ([L1,L2]) - [P,Q]", "    ({L1,L2}) - [P,Q]"),
    ] {
        let (list_codes, list_nets) = build(list);
        let (brace_codes, brace_nets) = build(brace);
        assert_eq!(
            (&list_codes, &list_nets),
            (&brace_codes, &brace_nets),
            "{tag}: the bare brace ≡ the bracket list; got {brace_codes:?} vs {list_codes:?}"
        );
    }
}

/// L14 边界（面二相）: at statement start a `{` line is the
/// declaration-position member face, not a vector — the instance row before
/// it absorbs the brace whether the two are spelled on one line or across
/// the newline (the face grammar is newline-insensitive; verified identical
/// bison traces `@phr.dcla1{,}` at HEAD and with leg5). The vector spelling
/// never competes for that position, so the absorbed form is the same
/// statement however it is laid out.
#[test]
fn conn_law__rowstart_brace_is_the_declaration_face() {
    // D1 once as a plain row, then the face spelling — D1 declared twice
    // (E5151) is part of the absorbed reading, exactly as for the inline
    // declaration face.
    let (inline_codes, inline_nets) = build("    T D1 {L1,L2} - [P,Q]");
    for (tag, stmt) in [("next_line", "    T D1\n    {L1,L2} - [P,Q]")] {
        let (codes, nets) = build(stmt);
        assert_eq!(
            (&codes, &nets),
            (&inline_codes, &inline_nets),
            "{tag}: the brace line after an instance row is the declaration face; got {codes:?} vs {inline_codes:?}"
        );
        assert!(
            codes.contains(&mcc::errcodes::INST_DECLARED_MULTIPLE),
            "{tag}: the absorbed reading redeclares D1 (E5151); got {codes:?}"
        );
    }
}

/// L14 边界（真行首）: at a true statement start — no instance row before the
/// brace — the port-row face still owns `{` (E4023, wiring stays empty), and
/// the vector arm does not compete; the bracket list at the same position
/// opens as a vector and wires. This asymmetry is the 面二相 law of the brace
/// at row start, byte-identical to HEAD (leg5 opens operand positions only);
/// lifting it would be a follow-up ruling against the port-row face.
#[test]
fn conn_law__rowstart_brace_stays_on_the_port_row_face() {
    // The shared fixture declares the instances before the body tail, so a
    // brace statement there would be absorbed by the declaration face (the
    // lock above). This lock needs a TRUE statement start: io rows only, the
    // statements, the instances after.
    let head = "component T { pins = [ 1 = A\n    2 = K ] }\nmodule main {\n    io L1\n    io P\n    io Q\n";
    let (brace_codes, brace_nets) =
        build_src(&format!("{head}    {{L1,L2}} - [P,Q]\n    T D1\n    T D2\n}}\n"));
    assert!(
        brace_codes.contains(&mcc::errcodes::PORT_ROW_WITH_CONNECTION),
        "the brace at a true statement start stays on the port-row face (E4023); got {brace_codes:?}"
    );
    assert!(
        brace_nets.is_empty(),
        "the port-row refusal wires nothing; got {brace_nets:?}"
    );
    let (_, list_nets) =
        build_src(&format!("{head}    [L1,L2] - [P,Q]\n    T D1\n    T D2\n}}\n"));
    assert_eq!(
        list_nets,
        vec![
            vec!["L1".to_string(), "P".to_string()],
            vec!["L2".to_string(), "Q".to_string()],
        ],
        "the bracket list at the same position opens as a vector; got {list_nets:?}"
    );
}

/// L14, suffix face: the brace takes suffix operators on the same terms as
/// the list — `{L1,L2}'` refuses against two lanes exactly like `[L1,L2]'`
/// (the transposed column is 2×1, the same E4007), and the device
/// member-vector list keeps its shape-limit verdict (L9) under both
/// spellings.
#[test]
fn conn_law__bare_curly_takes_suffixes_like_the_list() {
    for (tag, list, brace) in [
        (
            "transposed_labels",
            "    [P,Q] - [L1,L2]'",
            "    [P,Q] - {L1,L2}'",
        ),
        (
            "overwide_transpose",
            "    [D1{A,K},D2{A,K}]'",
            "    {D1{A,K},D2{A,K}}'",
        ),
    ] {
        let (list_codes, _) = build(list);
        let (brace_codes, _) = build(brace);
        assert_eq!(
            list_codes, brace_codes,
            "{tag}: the brace takes suffixes on the list's terms; got {brace_codes:?} vs {list_codes:?}"
        );
    }
    let (codes, _) = build("    {D1{A,K},D2{A,K}}'");
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_TRANSPOSE_LIMIT),
        "the overwide brace transpose must still report E2902; got {codes:?}"
    );
}

/// L22 三端选择律 (#16): the pipe-node form is the node-equation truth —
/// `{L1,L3|L2,L4}` assigns the written row pairs as node memberships.
#[test]
fn conn_law__pipenode_selection_is_the_node_equation_truth() {
    let (_, nets) = build("    D1{A,K} - {L1,L3|L2,L4}");
    assert!(
        wired(&nets, "L1", "D1.1") && wired(&nets, "L3", "D1.2"),
        "the pipe-node rows are the node assignments; got {nets:?}"
    );
}

/// L6 节点转置换 lane 律 (#16): transposing a pipe node really swaps the
/// lanes — the new face columns are the old rows: `{L1,L2|L3,L4}'` must wire
/// exactly like `{L1,L3|L2,L4}` (and the mirror `{L1,L3|L2,L4}'` like
/// `{L1,L2|L3,L4}`). Before U372 leg3 the suffix was a silent no-op.
#[test]
fn conn_law__node_transpose_swaps_lanes() {
    let untransposed_12_34 = build("    D1{A,K} - {L1,L2|L3,L4}").1;
    let untransposed_13_24 = build("    D1{A,K} - {L1,L3|L2,L4}").1;
    assert_ne!(
        untransposed_12_34, untransposed_13_24,
        "sanity: the two pipe spellings wire differently"
    );
    let (codes, nets) = build("    D1{A,K} - {L1,L2|L3,L4}'");
    assert_eq!(
        nets, untransposed_13_24,
        "{{L1,L2|L3,L4}}' ≡ {{L1,L3|L2,L4}} (L6); got {nets:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "the transposed node pairs clean against 2 lanes; got {codes:?}"
    );
    let (_, nets2) = build("    D1{A,K} - {L1,L3|L2,L4}'");
    assert_eq!(
        nets2, untransposed_12_34,
        "{{L1,L3|L2,L4}}' ≡ {{L1,L2|L3,L4}} (L6 mirror); got {nets2:?}"
    );
}

/// L6 嵌套形正则等式 (#14, #15) — U372 leg3: the stacked directed-row node is
/// a real node, so the outer transpose composes (壳序复合):
/// `[[L1,L2]',[L3,L4]']'` ≡ `{L1,L2|L3,L4}` and the untransposed stack
/// `[[L1,L2]',[L3,L4]']` ≡ `{L1,L3|L2,L4}` (near faces into column 1, far
/// faces into column 2). Row faces never short (L4); before leg3 this spelling
/// debug-panicked on a flat width-4 column whose transpose is unrepresentable.
#[test]
fn conn_law__nested_rowstack_transpose_equals_pipe_node() {
    let pipe_12_34 = build("    D1{A,K} - {L1,L2|L3,L4}").1;
    let pipe_13_24 = build("    D1{A,K} - {L1,L3|L2,L4}").1;
    let (codes, nets) = build("    D1{A,K} - [[L1,L2]',[L3,L4]']'");
    assert_eq!(
        nets, pipe_12_34,
        "#14: nested stack + outer transpose ≡ the pipe node; got {nets:?}"
    );
    assert!(
        !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "the nested form is a legal chain member; got {codes:?}"
    );
    let (_, nets15) = build("    D1{A,K} - [[L1,L2]',[L3,L4]']");
    assert_eq!(
        nets15, pipe_13_24,
        "#15: the untransposed stack ≡ the transposed pipe node; got {nets15:?}"
    );
    assert_ne!(
        nets, nets15,
        "the outer transpose is not a no-op: #14 and #15 wire differently"
    );
}

/// L5 嵌套叠加律 (#17, #18, #19): same-orientation list members stack
/// vertically (column+column → N×1), mixed layers flatten — all three
/// spellings are the same 4×1 vector and zip 1:1 against a 4-wide row.
#[test]
fn conn_law__column_stack_flattens_to_one_vector() {
    let expected = vec![
        vec!["L1".to_string(), "P".to_string()],
        vec!["L2".to_string(), "Q".to_string()],
        vec!["L3".to_string(), "R".to_string()],
        vec!["L4".to_string(), "S".to_string()],
    ];
    for (tag, stmt) in [
        ("colstack", "    [[L1,L2],[L3,L4]] - [P,Q,R,S]"),
        ("flat", "    [L1,L2,L3,L4] - [P,Q,R,S]"),
        ("mixed", "    [L1,[L2,L3],L4] - [P,Q,R,S]"),
    ] {
        let (codes, nets) = build(stmt);
        assert!(
            !codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
            "{tag}: 4-wide vs 4-wide must zip clean; got {codes:?}"
        );
        assert_eq!(nets, expected, "{tag}: flatten then zip 1:1; got {nets:?}");
    }
}

/// L10, stacked side (#20): a stacked 4×1 column against a two-lane member
/// vector refuses — the user's original asking cell, confirmed by ruling ⑤.
#[test]
fn conn_law__stacked_column_vs_two_lanes_is_refused() {
    let (codes, nets) = build("    D1{A,K} - [[L1,L2],[L3,L4]]");
    assert!(
        codes.contains(&mcc::errcodes::CONN_SERIES_SHAPE_MISMATCH),
        "4*1 vs 2 lanes must refuse with E4007; got {codes:?}"
    );
    assert!(
        !nets.iter().any(|n| n.iter().any(|p| p.starts_with("D1."))),
        "a refused statement wires nothing; got {nets:?}"
    );
}

/// L9 形状超限律 (#45): a transpose over a row-stacked 2×2-of-rows operand
/// exceeds the admitted shapes — E2902 fires (list spelling reports at parse
/// phase; the test asserts the code, not the phase).
#[test]
fn conn_law__overwide_transpose_reports_shape_limit() {
    let (codes, _) = build("    [D1',D2']'");
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_TRANSPOSE_LIMIT),
        "transpose over the row-stacked shape must report E2902; got {codes:?}"
    );
}

/// L8 退化反向律, multi-element side (#48): `^` on a multi-element list is the
/// merge face (并网) and keeps its E2903 — the narrowing that leg1 performs
/// exempts only single points, never this shape.
#[test]
fn conn_law__multi_element_reverse_keeps_warning_and_merges() {
    let (codes, nets) = build("    L1 - [VCC,GND]^");
    assert!(
        codes.contains(&mcc::errcodes::SHAPE_REVERSE_NOOP),
        "multi-element退化 ^ keeps E2903 after the leg1 narrowing; got {codes:?}"
    );
    assert!(
        wired(&nets, "L1", "VCC") && wired(&nets, "L1", "GND"),
        "the merge face is the existing behavior; got {nets:?}"
    );
}
