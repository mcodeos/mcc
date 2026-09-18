// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `join` — two adjacent segments of the chain, matched by key.
//!
//! Design: `mcd/doc/pipeline/stage-readout-design.md` §5 / §5.3 ②. The chain is
//!
//! ```text
//! stmt(loc) --join--> stage.p2 --join--> stage.vec --join--> stage.viz
//! ```
//!
//! and `join` answers "why is this source line not drawn?" — not by putting two
//! dumps side by side, but by matching the two sides on a key and reporting the
//! cardinality of every mismatch. This module holds the `src -> p2` hop.
//!
//! ## The source side: statements, not lines
//!
//! One line writes one statement, and one statement can write N endpoints
//! (`Cap([VCC, GND])` writes two), so counting lines would report every
//! two-endpoint statement as lost (§5.3 source-side item 1).
//!
//! The item set spans **every loaded source file** — each file's top-level `use`
//! clauses plus the clauses of every module and component body. Two exclusions,
//! both structural:
//!
//! - **`func` bodies are not walked.** A `func` is a wiring macro: the rows it
//!   produces carry the *call* site's position, so its own statements own no row
//!   of their own and counting them would report one false `drop` per library
//!   `func`.
//! - **A `func` clause is not an item either** — a definition is not a
//!   statement on the chain, however it is spelled in the body.
//!
//! A third exclusion is a property of the object rather than of the walk: a
//! **bus member owns no physical point** (§2.4), so it is not a chain object at
//! this hop and is counted in the header instead of being classified.
//!
//! ## The two sub-hops, counted apart
//!
//! `src -> p2` is two hops, not one (§5.3 source-side item: the sub-hops must be
//! told apart). Hop 1 is the
//! AST clause against the Pass-1 statement record, matched on the offset
//! `parse_body` pushes; hop 2 is that record's span against the Pass-2 rows
//! inside it. Without the split, "the parser dropped the statement" and "Pass 2
//! never wrote it" would be one number — and the first is the one a reader can
//! act on.
//!
//! ## Three non-matches that are not pipeline events
//!
//! §5.2 hard constraint 3 requires a key's defect to be reported apart from a generated
//! object, so these three are counted in the header rather than dressed up as a
//! class — each is a category error the six words would otherwise absorb:
//!
//! - **A row with no anchor at all** — neither `src_pos` nor `fallback_pos`.
//!   These are overwhelmingly the auto-named devices (`_C1`, `_R1`), whose
//!   anchor does exist (`AutoAnchor.offset` is a real source offset) but is not
//!   carried onto the flattened row. Calling them `synth` would be exactly the
//!   confusion `synth` is defined to prevent.
//! - **A pure declaration** (`Type name`) — `parse_body`'s `MCAST_DECLARE`
//!   branch registers the instance without a Pass-1 statement record, and no
//!   declaration site reaches the row. Its clause therefore owns nothing to
//!   match, and reporting `drop` would cry wolf at every declaration.
//! - **A row anchored inside a `func` body** — an excluded template, not an
//!   object with no upstream. The statement that wrote it is the call site's,
//!   which is in the item set; the row's own position just points into the
//!   macro. `synth` would call that a generated object.
//!
//! ## Classification is per item, not per connected component
//!
//! A connected component would be transitively closed through shared nets, and
//! in a real design every statement is a few nets from every other one, so the
//! whole table would collapse into a single unreadable blob. Per item it is the
//! design's own table — a clause is classified by how many rows it exclusively
//! owns, a row by how many clauses contain it:
//!
//! | shape | class | what it says |
//! |---|---|---|
//! | clause, 1 row | `carry` | one statement became exactly one row |
//! | clause, N rows | `expand` | a declaration plus its pins; a two-endpoint `Cap` |
//! | row, N clauses | `merge` | N statements wired into one net |
//! | clause, 0 rows | `drop` / `skip` | suspicious (a statement) / as-designed (a kind) |
//! | row, 0 clauses | `synth` | nothing upstream wrote it |
//!
//! `skip` is decided by **AST node kind**, never by name or text (§5.3 source-side item 2):
//! whether an item participates in modelling is a structural fact. It stays
//! `skip` even when rows do land inside its span — a `pins = [...]` block is
//! skipped *as a construct*, and the rows it declares are listed on it rather
//! than reclassifying it as an expansion.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::ast::macros::{
    MCAST_ATTRIBUTE, MCAST_ATTRIBUTE_PIN, MCAST_ATTRIBUTE_PINADD, MCAST_BODY, MCAST_COMPONENT,
    MCAST_DECLARE, MCAST_DOMAIN, MCAST_FUNCTION, MCAST_MODULE, MCAST_NET, MCAST_NET_PORTS,
    MCAST_RAIL, MCAST_REF, MCAST_USE, MCAST_USE_PUB,
};
use crate::ast::node::AstNode;
use crate::db::cmie::tables::WORKSPACE;
use crate::instant::insttab::{InstKind, InstTable};
use crate::semantic::common::SourcePos;

use super::{loc_cell, loc_of, render_table, SourceText, StageView};

/// The `view` value of this readout. Spelled with `->` rather than the doc's
/// `→`: a `view` value is an identifier a consumer greps for, and the arrow is
/// there to be read by a human.
pub const SRC_P2_VIEW: &str = "join.src->p2";

/// The six class words of the summary line, in print order. Fixed, so two runs
/// cannot differ by which words appear, and printed even at zero, so an absent
/// word cannot read as "not implemented" (§5.3).
pub const SIX_WORDS: &[&str] = &["carry", "expand", "merge", "drop", "synth", "skip"];

/// The annotations the text face prints, and the two labels of its sub-hop line.
///
/// §5.3 fixes these strings, and this readout is meant to be read by the user, so
/// the printed text stays as the design specifies it. They are written as
/// escapes because this repository's commit gate rejects non-ASCII characters in
/// Rust sources — the escapes keep the file ASCII while the readout is unchanged.
///
/// Each annotation is constant per class on purpose: the design pins the wording
/// ("as-designed, it took no part in modelling"; "no p2 row is anchored in this
/// statement's span"), so a reader can tell a `skip` member from a loss without
/// parsing prose. The `drop` note states the check that ran and stops there —
/// "downstream truly has none" would over-claim, since this build holds rows with
/// no anchor at all (§5.2 hard constraint 3).
const SKIP_NOTE: &str = "\u{6309} AST kind \u{4e0d}\u{53c2}\u{4e0e}\u{5efa}\u{6a21}";
const DROP_NOTE: &str =
    "\u{65e0} p2 \u{884c}\u{951a}\u{5728}\u{672c}\u{8bed}\u{53e5}\u{8de8}\u{5ea6}\u{5185}";
/// Appended to the member count, so a `merge` row always shows how many
/// statements it merged and not merely that it merged some.
const MERGE_NOTE: &str = "\u{9879}\u{5e76} 1";
const SYNTH_NOTE: &str = "\u{4e0b}\u{6e38}\u{6709}\u{3001}\u{4e0a}\u{6e38}\u{786e}\u{5b9e}\u{65e0}";
const SUBHOP_LABEL: &str = "\u{5b50}\u{8df3}";
const MISMATCH_LABEL: &str = "\u{5931}\u{914d}";

/// Group order of the text face: the `drop` group on top, because it is the
/// debug entry point (§5.3 ②), then `synth` (the other flagged class), then the
/// rest in count-word order. Also the sort order of the JSON `items`, so the two
/// faces present the same sequence and not merely the same set.
const GROUP_ORDER: &[&str] = &["drop", "synth", "carry", "expand", "merge", "skip"];

/// One clause of a source file: the unit this hop counts.
struct Clause {
    uri: String,
    /// Clause span, used for containment. The whole clause, so a trailing
    /// attribute is inside the statement it trails.
    start: usize,
    end: usize,
    /// The offset Pass 1 records for this statement. `parse_body` pushes the
    /// *sub-node*'s position rather than the clause's, so this is what sub-hop 1
    /// compares. `None` when Pass 1 records nothing for this clause.
    stmt_at: Option<usize>,
    /// Pass 1 keeps a statement record for statements in a **module** body only
    /// (`McComponent` has no `stmts`), so only those take part in sub-hop 1.
    pass1: bool,
    /// A declaration (`Type name`): participating, but with no Pass-1 record.
    declare: bool,
    /// Non-participating by AST node kind.
    skip: bool,
    /// The clause's source text, for the text face's detail column.
    text: String,
}

/// A downstream row: one line of the flat table.
struct DRow {
    class: &'static str,
    /// The row's key, spelled exactly as `stage.p2` spells it (`D<id>` for an
    /// instance, the `PointId` for a point, `net:<label>` for a labelled net,
    /// `null` for the objects that own no key).
    key: Value,
    loc: Value,
    /// `(uri, offset, matched_on_declaration)`. The third field is true when the
    /// row has no wiring site and was matched on its declaration site instead —
    /// the text face says so rather than letting a declaration pass for a wire.
    anchor: Option<(String, usize, bool)>,
    /// The entry id, so a net row can borrow its members' clauses.
    entry_id: u32,
    /// Member entry ids, for a net row.
    members: Vec<u32>,
    /// Member entry paths, sorted. A net's key is its label, and a label is not
    /// an identity — two nets in different modules may both be called `GND` and
    /// would then share a key. §5.3's rule for this class is that the criterion
    /// can only be the member set, never an id, so the set is what the item
    /// carries and what the text face shows.
    member_paths: Vec<String>,
    /// Deterministic tie-break inside a class.
    sort: String,
}

/// Build `join src->p2`: the source statements of the loaded world against the
/// flat instance table, class by class.
pub fn build_join_src_p2(table: &InstTable, top: &str, diagnostics: usize) -> StageView {
    let (clauses, func_spans, header_spans) = in_scope_clauses();
    let (rows, bus_rows) = downstream_rows(table);

    // A row's clause set. Anchor first, so a net can never be why a point row
    // acquired a clause.
    let mut row_clauses: Vec<Vec<usize>> = rows
        .iter()
        .map(|row| match &row.anchor {
            Some((uri, offset, _)) => containing_clause(&clauses, uri, *offset)
                .into_iter()
                .collect(),
            None => Vec::new(),
        })
        .collect();
    let row_of_entry: BTreeMap<u32, usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.entry_id != 0)
        .map(|(i, r)| (r.entry_id, i))
        .collect();
    for (i, row) in rows.iter().enumerate() {
        if row.members.is_empty() {
            continue;
        }
        // N statements that wire into one net are N clauses on one row — which
        // is the only way `merge` can arise at this hop: a row has one position,
        // so it can never fall inside two statements' spans.
        let mut cs: Vec<usize> = Vec::new();
        for m in &row.members {
            if let Some(ri) = row_of_entry.get(m) {
                for c in &row_clauses[*ri] {
                    if !cs.contains(c) {
                        cs.push(*c);
                    }
                }
            }
        }
        cs.sort_unstable();
        row_clauses[i] = cs;
    }

    // A clause owns the rows that only it contains; a row contained by several
    // is the subject of its own `merge` item instead.
    let mut owners: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (ri, cs) in row_clauses.iter().enumerate() {
        if cs.len() == 1 {
            owners.entry(cs[0]).or_default().push(ri);
        }
    }

    let mut sources = SourceText::new();
    let mut items: Vec<(u8, String, Value)> = Vec::new();
    let mut counts: BTreeMap<&'static str, usize> =
        SIX_WORDS.iter().map(|w| (*w, 0usize)).collect();
    let mut unanchored = 0usize;
    let mut unanchored_by_class: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut declarations_unmatched = 0usize;
    let mut func_scoped = 0usize;
    let mut header_scoped = 0usize;
    // Every row falls in exactly one of: inside a clause's span (one or several),
    // or in one of the four anchor buckets below. Publishing both halves lets a
    // reader *prove* an empty `synth` is empty rather than silently skipped —
    // a zero member class would otherwise be a branch no readout ever reaches.
    let mut rows_with_clause = 0usize;

    // Sub-hop 1: the AST clause against the Pass-1 record — "the parser would not
    // take this statement", the mismatch a reader can act on.
    let records = pass1_records();
    let mut claimed: BTreeSet<(String, usize)> = BTreeSet::new();
    let mut src_ast_mismatch = 0usize;
    for clause in &clauses {
        let Some(at) = clause.stmt_at else {
            continue;
        };
        if clause.pass1 && records.contains(&(clause.uri.clone(), at)) {
            claimed.insert((clause.uri.clone(), at));
        } else {
            src_ast_mismatch += 1;
        }
    }
    // Sub-hop 2: that record against the Pass-2 rows inside its span — "Pass 2
    // never wrote it", a different fault with a different owner. Counting the
    // records this walk does not reach instead would make the number depend on
    // the walk rather than on the pipeline.
    let mut ast_p2_mismatch = 0usize;
    for (ci, clause) in clauses.iter().enumerate() {
        let Some(at) = clause.stmt_at else {
            continue;
        };
        if !clause.pass1 || !claimed.contains(&(clause.uri.clone(), at)) {
            continue;
        }
        if owners.get(&ci).map(Vec::is_empty).unwrap_or(true) {
            ast_p2_mismatch += 1;
        }
    }

    for (ci, clause) in clauses.iter().enumerate() {
        let owned = owners.get(&ci).cloned().unwrap_or_default();
        let class = if clause.skip {
            "skip"
        } else if owned.is_empty() {
            if clause.declare {
                declarations_unmatched += 1;
                continue;
            }
            "drop"
        } else if owned.len() == 1 {
            "carry"
        } else {
            "expand"
        };
        counts.insert(class, counts[class] + 1);
        let why = match class {
            "skip" => SKIP_NOTE,
            // What was measured, and no more. "Downstream truly has none" would
            // over-claim: this build holds rows with no anchor at all, and a
            // statement whose rows are among them is indistinguishable here from
            // one that produced nothing. §5.2 hard constraint 3 keeps those apart, so the
            // note states the check, not a conclusion about the world.
            "drop" => DROP_NOTE,
            _ => "",
        };
        items.push((
            rank_of(class),
            format!(
                "{}\u{1}{:012}\u{1}{}",
                clause.uri, clause.start, clause.start
            ),
            json!({
                "class": class,
                "key": clause_key(clause, &mut sources),
                "text": clause.text,
                "loc": clause_loc(clause, &mut sources),
                "from": Value::Array(vec![]),
                "to": rows_value(&rows, &owned),
                "why": why,
            }),
        ));
    }

    for (ri, row) in rows.iter().enumerate() {
        let cs = &row_clauses[ri];
        if !cs.is_empty() {
            rows_with_clause += 1;
        }
        if cs.len() > 1 {
            counts.insert("merge", counts["merge"] + 1);
            items.push((
                rank_of("merge"),
                format!("{}\u{1}{}", "merge", row.sort),
                json!({
                    "class": "merge",
                    "key": row.key,
                    "loc": row.loc,
                    "from": Value::Array(cs.iter().map(|c| clause_key(&clauses[*c], &mut sources)).collect()),
                    "to": Value::Array(vec![row.key.clone()]),
                    "members": Value::Array(
                        row.member_paths.iter().map(|p| Value::String(p.clone())).collect()
                    ),
                    "why": format!("{} {}", cs.len(), MERGE_NOTE),
                }),
            ));
        } else if cs.is_empty() {
            if row.anchor.is_none() {
                // A key's defect, not an event (see the module note).
                unanchored += 1;
                *unanchored_by_class.entry(row.class).or_default() += 1;
            } else if in_func_span(&func_spans, row.anchor.as_ref().unwrap()) {
                // Positioned inside a `func` body, which this hop does not walk.
                // The row was written by the call site's statement, so there is
                // an upstream for it — just not one of these items. Reporting it
                // as `synth` would claim a generated object where the truth is
                // an excluded template (§5.2 hard constraint 3).
                func_scoped += 1;
            } else if in_func_span(&header_spans, row.anchor.as_ref().unwrap()) {
                // Declared on a module header: a port, not a generated object.
                header_scoped += 1;
            } else {
                counts.insert("synth", counts["synth"] + 1);
                items.push((
                    rank_of("synth"),
                    format!("{}\u{1}{}", "synth", row.sort),
                    json!({
                        "class": "synth",
                        "key": row.key,
                        "loc": row.loc,
                        "from": Value::Array(vec![]),
                        "to": Value::Array(vec![row.key.clone()]),
                        "why": SYNTH_NOTE,
                    }),
                ));
            }
        }
    }

    items.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));

    let mut counts_value = serde_json::Map::new();
    for w in SIX_WORDS {
        counts_value.insert((*w).into(), json!(counts[w]));
    }
    counts_value.insert("sub_hop_src_ast".into(), json!(src_ast_mismatch));
    counts_value.insert("sub_hop_ast_p2".into(), json!(ast_p2_mismatch));
    counts_value.insert("unanchored".into(), json!(unanchored));
    counts_value.insert(
        "unanchored_by_class".into(),
        Value::Object(
            unanchored_by_class
                .iter()
                .map(|(k, v)| ((*k).to_string(), json!(v)))
                .collect(),
        ),
    );
    counts_value.insert(
        "declarations_unmatched".into(),
        json!(declarations_unmatched),
    );
    counts_value.insert("func_scoped".into(), json!(func_scoped));
    counts_value.insert("header_scoped".into(), json!(header_scoped));
    counts_value.insert("bus_rows".into(), json!(bus_rows));
    counts_value.insert("diagnostics".into(), json!(diagnostics));
    counts_value.insert("rows_total".into(), json!(rows.len()));
    counts_value.insert("rows_with_clause".into(), json!(rows_with_clause));

    StageView::with_view(
        SRC_P2_VIEW,
        top,
        items.into_iter().map(|(_, _, v)| v).collect(),
        Value::Object(counts_value),
    )
}

fn rank_of(class: &str) -> u8 {
    GROUP_ORDER
        .iter()
        .position(|c| *c == class)
        .unwrap_or(GROUP_ORDER.len()) as u8
}

/// The `(uri, offset)` of every Pass-1 statement record in the loaded world.
///
/// `stmt_spans[i]` is parallel to `stmts[i]` (both pushed by `parse_body`), and
/// `start` is the offset that identifies the statement. Only module bodies have
/// such a record — a component body keeps no `stmts`.
fn pass1_records() -> BTreeSet<(String, usize)> {
    let mut out = BTreeSet::new();
    for entry in WORKSPACE.modules.iter() {
        let m = entry.value();
        for sp in &m.stmt_spans {
            out.insert((m.uri.clone(), sp.start));
        }
    }
    out
}

/// The clauses of every loaded source file, sorted by `(uri, offset)`.
///
/// Sorted because the source set lives in a `DashMap` whose iteration order is
/// unspecified, and an artifact whose row order depends on hash-table layout is
/// not comparable to the next run (build-design §3.7 discipline 0).
///
/// # Why a clause's end comes from its next sibling
///
/// A node's own `len` under-reports a statement: the last token is outside it,
/// so `USB.vin -> V5V::DC(5V)` ends at `5V` and a row written at the statement's
/// closing operand falls outside the span. Statements are siblings in source
/// order, so the next sibling's start is the honest end — and it makes the
/// clauses of a body tile it exactly, which is what lets a row be attributed to
/// the statement it was written in rather than to nothing.
fn in_scope_clauses() -> (
    Vec<Clause>,
    Vec<(String, usize, usize)>,
    Vec<(String, usize, usize)>,
) {
    let mut out = Vec::new();
    let mut func_spans: Vec<(String, usize, usize)> = Vec::new();
    let mut header_spans: Vec<(String, usize, usize)> = Vec::new();
    for entry in WORKSPACE.mcodes.iter() {
        let uri = entry.key().clone();
        let code = entry.value();
        let text = code.content.clone();
        let top: Vec<AstNode> = code.ast.iter().collect();
        for (i, node) in top.iter().enumerate() {
            let end = next_bound(&top, i, &text, text.len());
            let t = node.get_type();
            if t == MCAST_USE || t == MCAST_USE_PUB {
                out.push(clause_of(&uri, &text, node, end, None, false, false, true));
            } else if t == MCAST_MODULE || t == MCAST_COMPONENT {
                let in_module = t == MCAST_MODULE;
                let Some(children) = node.get_sub_node() else {
                    continue;
                };
                let Some(body) = children.iter().find(|c| c.is_type(MCAST_BODY)) else {
                    continue;
                };
                let Some(clauses) = body.get_sub_node() else {
                    continue;
                };
                let list: Vec<AstNode> = clauses.iter().collect();
                // The header: the declaration line down to the body's first
                // clause. A port declared there (`psnk dc{VDD_3V3, GND}::DC(3.3V)`)
                // produces rows, but it is a declaration and not a statement, so
                // those rows have no clause to land in.
                header_spans.push((
                    uri.clone(),
                    clause_start(node, &text),
                    line_start(&text, body.get_pos() as usize),
                ));
                let limit = (body.get_pos() as usize + body.get_len() as usize).min(text.len());
                for (j, cl) in list.iter().enumerate() {
                    let end = next_bound(&list, j, &text, limit);
                    if cl.is_type(MCAST_FUNCTION) {
                        // Not walked: a `func` is a wiring macro whose rows are
                        // produced at the call site. Recorded so that a row
                        // positioned inside one can be told apart from a row
                        // nothing upstream wrote.
                        func_spans.push((uri.clone(), cl.get_pos() as usize, end));
                        continue;
                    }
                    collect_body_clause(&mut out, &uri, &text, cl, end, in_module);
                }
            }
        }
    }
    out.sort_by(|a, b| (a.uri.as_str(), a.start).cmp(&(b.uri.as_str(), b.start)));
    (out, func_spans, header_spans)
}

/// Where a clause begins: the start of the line its node's position is on.
///
/// A node's `pos` is not the statement's first byte. `parse_body` records the
/// *sub-node's* position, and which sub-node that is varies by kind — for
/// `TP1::TP()` it is the call, for `((a -> b) + c)` it is `a`, for a `use` it is
/// the path. Every one of those is on the statement's own line and no statement
/// shares a line with another, so the line start is both the honest beginning
/// and stable across kinds. Without it a clause would begin mid-statement and
/// its text column would show a fragment.
fn clause_start(node: &AstNode, text: &str) -> usize {
    line_start(text, node.get_pos() as usize)
}

/// The first byte of the line containing `pos`.
fn line_start(text: &str, pos: usize) -> usize {
    text.get(..pos.min(text.len()))
        .and_then(|s| s.rfind('\n'))
        .map(|i| i + 1)
        .unwrap_or(0)
}

/// The newline at the end of the line containing `pos`, or the end of the text.
/// The counterpart of [`line_start`], and the reason a clause can be quoted
/// whole: the same whole-line rule that opens it closes it.
fn line_end(text: &str, pos: usize) -> usize {
    let p = pos.min(text.len());
    p + text
        .get(p..)
        .and_then(|s| s.find('\n'))
        .unwrap_or_else(|| text.len() - p)
}

/// The end of the clause at `i`: the line its next sibling starts on, or — for
/// the last one — the end of the line this node's own extent reaches, clamped to
/// the enclosing scope.
///
/// Both bounds are line boundaries, so a clause spans whole lines and its text
/// is whole statements. `len` is not trustworthy on its own either way: a
/// `module` body's first child carries the *body's* length, and believing it
/// would stretch that one clause over every statement after it and take their
/// rows; a plain statement under-reports its own, and believing that cuts the
/// text off mid-statement (`c2.Cap([VDD,` for `c2.Cap([VDD, GND])`). The
/// over-report is harmless here because it only ever belongs to the first child,
/// which always has a next sibling to bound it; the under-report is repaired by
/// rounding the end up to the end of its line.
fn next_bound(list: &[AstNode], i: usize, text: &str, limit: usize) -> usize {
    let start = clause_start(&list[i], text);
    match list.get(i + 1).map(|n| clause_start(n, text)) {
        Some(e) if e > start => e,
        _ => {
            // Round the reached position up to a line end, then let the scope
            // bound it — but never below the clause's own first line, or the
            // scope's own (equally untrustworthy) figure would cut the very
            // statement it is meant to contain.
            let own = line_end(text, start);
            line_end(text, start + list[i].get_len() as usize)
                .min(limit)
                .max(own)
                .max(start)
        }
    }
}

/// One clause of a module or component body, if it is a statement at all.
fn collect_body_clause(
    out: &mut Vec<Clause>,
    uri: &str,
    text: &str,
    cl: &AstNode,
    end: usize,
    in_module: bool,
) {
    let t = cl.get_type();
    if t == MCAST_NET || t == MCAST_DECLARE {
        let declare = t == MCAST_DECLARE
            || cl
                .get_sub_node()
                .map(|s| s.is_type(MCAST_DECLARE))
                .unwrap_or(false);
        let (stmt_at, pass1) = if declare {
            (None, false)
        } else {
            (cl.get_sub_node().map(|s| s.get_pos() as usize), in_module)
        };
        out.push(clause_of(
            uri, text, cl, end, stmt_at, declare, pass1, false,
        ));
    } else if is_skip_kind(t) {
        out.push(clause_of(uri, text, cl, end, None, false, false, true));
    }
    // Anything else in a body is a definition or a construct the chain does not
    // carry; it is not a statement and gets no item.
}

/// Non-participating by AST node kind. Structural, never a name test.
fn is_skip_kind(kind: u16) -> bool {
    matches!(
        kind,
        MCAST_USE
            | MCAST_USE_PUB
            | MCAST_NET_PORTS
            | MCAST_ATTRIBUTE
            | MCAST_ATTRIBUTE_PIN
            | MCAST_ATTRIBUTE_PINADD
            | MCAST_REF
            | MCAST_DOMAIN
            | MCAST_RAIL
    )
}

#[allow(clippy::too_many_arguments)]
fn clause_of(
    uri: &str,
    text: &str,
    node: &AstNode,
    end: usize,
    stmt_at: Option<usize>,
    declare: bool,
    pass1: bool,
    skip: bool,
) -> Clause {
    let start = clause_start(node, text);
    let end = end.max(start);
    Clause {
        uri: uri.to_string(),
        start,
        end,
        stmt_at,
        pass1,
        declare,
        skip,
        text: clause_text(text, start, end),
    }
}

/// The clause's source text, flattened onto one line.
///
/// The span runs to the next sibling, so its tail holds whatever separated the
/// two statements — blank lines and comments. Those are dropped, and a
/// multi-line statement is joined with a single space, because this is one cell
/// of a fixed-width table and a newline in it would break the columns.
fn clause_text(text: &str, start: usize, end: usize) -> String {
    let raw = text.get(start..end.min(text.len())).unwrap_or("");
    let mut lines: Vec<&str> = raw.lines().collect();
    while let Some(last) = lines.last() {
        let t = last.trim();
        if t.is_empty() || t.starts_with('#') {
            lines.pop();
        } else {
            break;
        }
    }
    lines.join(" ").trim().to_string()
}

/// The innermost clause whose span contains `offset`, if any.
fn containing_clause(clauses: &[Clause], uri: &str, offset: usize) -> Option<usize> {
    clauses
        .iter()
        .enumerate()
        .filter(|(_, c)| c.uri == uri && c.start <= offset && offset < c.end)
        .map(|(i, _)| i)
        .max_by_key(|i| clauses[*i].start)
}

/// Whether a row's anchor sits inside one of these spans.
fn in_func_span(spans: &[(String, usize, usize)], anchor: &(String, usize, bool)) -> bool {
    spans
        .iter()
        .any(|(uri, start, end)| *uri == anchor.0 && *start <= anchor.1 && anchor.1 < *end)
}

/// Every downstream row of the flat table, plus the count of bus rows, which
/// own no point and so are not chain objects at this hop (§2.4).
fn downstream_rows(table: &InstTable) -> (Vec<DRow>, usize) {
    let mut sources = SourceText::new();
    let mut rows: Vec<DRow> = Vec::new();
    let mut bus_rows = 0usize;
    for (_, entry) in table.iter() {
        let class = match entry.kind {
            InstKind::Module | InstKind::Component => "instance",
            InstKind::Pin | InstKind::Port => "point",
            InstKind::Bus => {
                bus_rows += 1;
                continue;
            }
            InstKind::Label => "label",
        };
        let key = match entry.kind {
            InstKind::Module | InstKind::Component => Value::String(format!("D{}", entry.id)),
            InstKind::Pin | InstKind::Port => entry
                .point
                .map(|p| Value::String(p.to_string()))
                .unwrap_or(Value::Null),
            _ => Value::Null,
        };
        // The wiring site wins; the declaration site is used only when there is
        // no wiring site, and the row remembers which one it was.
        let (win, via_declaration) = match (&entry.src_pos, &entry.fallback_pos) {
            (Some(p), _) => (Some(p), false),
            (None, Some(p)) => (Some(p), true),
            (None, None) => (None, false),
        };
        rows.push(DRow {
            class,
            key,
            loc: loc_of(win, &mut sources),
            anchor: win.map(|p| (p.uri.clone(), p.offset as usize, via_declaration)),
            entry_id: entry.id,
            members: Vec::new(),
            member_paths: Vec::new(),
            sort: format!("{}\u{1}{:08}", entry.path, entry.id),
        });
    }
    for net in table.get_nets() {
        let labeled = !crate::instant::mc_net::is_anon_net_name(&net.name);
        let mut paths: Vec<String> = net
            .points
            .iter()
            .filter_map(|p| table.get_entry(*p).map(|e| e.path.clone()))
            .collect();
        paths.sort();
        rows.push(DRow {
            class: "net",
            key: if labeled {
                Value::String(format!("net:{}", net.name))
            } else {
                Value::Null
            },
            // A net owns no position of its own: its anchor is its members'.
            loc: Value::Null,
            anchor: None,
            entry_id: 0,
            members: net.points.clone(),
            sort: format!(
                "{}\u{1}{}\u{1}{}",
                net.name,
                net.points.len(),
                paths.join(",")
            ),
            member_paths: paths,
        });
    }
    (rows, bus_rows)
}

/// A clause's key — its own coordinate, per §5.3 source-side item 5: a source position is
/// the only anchor that still points back after the next edit, so every item
/// carries it.
fn clause_key(clause: &Clause, sources: &mut SourceText) -> Value {
    let loc = clause_loc(clause, sources);
    let line = loc["line"].as_u64().unwrap_or(0);
    if line == 0 {
        Value::String(format!("{}:-", clause.uri))
    } else {
        Value::String(format!("{}:{}", clause.uri, line))
    }
}

fn clause_loc(clause: &Clause, sources: &mut SourceText) -> Value {
    let pos = SourcePos::new(clause.uri.clone(), clause.start as u32);
    loc_of(Some(&pos), sources)
}

fn rows_value(rows: &[DRow], idx: &[usize]) -> Value {
    Value::Array(idx.iter().map(|i| rows[*i].key.clone()).collect())
}

/// Render the `join.src->p2` text face from the *same* items the JSON face uses
/// (§5.3 ruling ③).
///
/// Grammar per §5.3: one item per line, the class in the second
/// whitespace-separated column, the `drop` group on top, a blank line between
/// groups, and only two prefixes (`!` for `drop`, `+` for `synth`, so `grep '^!'`
/// lands on the suspicious rows). No ANSI, no box drawing, no tabs; a missing
/// value prints as `-`.
pub fn render_join_text(view: &StageView) -> String {
    let mut out = vec![view.header_line(), sub_hop_line(view), six_word_line(view)];
    for (gi, class) in GROUP_ORDER.iter().enumerate() {
        let rows: Vec<Vec<String>> = view
            .items
            .iter()
            .filter(|i| i["class"] == *class)
            .map(|i| {
                let prefix = match *class {
                    "drop" => "!",
                    "synth" => "+",
                    _ => " ",
                };
                let (detail, tail) = match *class {
                    // The member set, not the key: a label is not an identity
                    // and two nets may share one (§5.3 six-class table).
                    "merge" => (
                        keys_cell(&i["members"]),
                        format!("← {}", keys_cell(&i["from"])),
                    ),
                    "synth" => (keys_cell(&i["to"]), String::new()),
                    _ => (detail_cell(i), keys_cell(&i["to"])),
                };
                let why = i["why"].as_str().unwrap_or("");
                vec![
                    format!("{prefix} {class}"),
                    loc_cell(&i["loc"]),
                    detail,
                    tail,
                    if why.is_empty() {
                        String::new()
                    } else {
                        format!("; {why}")
                    },
                ]
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        if gi > 0 {
            out.push(String::new());
        }
        render_table(&rows, &mut out);
    }
    out.join("\n")
}

/// The second line: the two sub-hops of `src -> p2`, counted apart, so a
/// statement the parser dropped can never be read as a Pass-2 loss.
fn sub_hop_line(view: &StageView) -> String {
    format!(
        "# {} src->ast {} {}   ast->p2 {} {}",
        SUBHOP_LABEL,
        MISMATCH_LABEL,
        view.counts["sub_hop_src_ast"].as_u64().unwrap_or(0),
        MISMATCH_LABEL,
        view.counts["sub_hop_ast_p2"].as_u64().unwrap_or(0)
    )
}

/// The third line: six words, always all of them, always with a cardinality —
/// so `merge` cannot be misread as N-1 losses (§5.3 ②).
fn six_word_line(view: &StageView) -> String {
    let words: Vec<String> = SIX_WORDS
        .iter()
        .map(|w| format!("{w} {}", view.counts[*w].as_u64().unwrap_or(0)))
        .collect();
    format!("# {}", words.join("  "))
}

/// The detail column: a clause's own source text, or the row's key when the item
/// is a downstream one.
fn detail_cell(item: &Value) -> String {
    item["text"]
        .as_str()
        .or_else(|| item["key"].as_str())
        .unwrap_or("-")
        .to_string()
}

fn keys_cell(v: &Value) -> String {
    match v.as_array() {
        Some(a) if !a.is_empty() => a
            .iter()
            .map(|k| k.as_str().unwrap_or("-").to_string())
            .collect::<Vec<_>>()
            .join(" "),
        _ => "-".to_string(),
    }
}
