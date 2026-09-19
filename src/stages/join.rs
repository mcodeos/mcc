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
//! cardinality of every mismatch. This module holds all three hops: the first
//! matches **statements** against the rows they wrote, and the two inner ones
//! match **objects** against each other.
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
//! design's own table — a clause is classified by **how many rows it reaches**,
//! a row by how many clauses contain it:
//!
//! | shape | class | what it says |
//! |---|---|---|
//! | clause, 1 row | `carry` | one statement reaches exactly one row |
//! | clause, N rows | `expand` | a declaration plus its pins; a two-endpoint `Cap` |
//! | row, N clauses | `merge` | N statements wired into one net |
//! | clause, 0 rows | `drop` | suspicious (a statement) |
//! | any cardinality | `skip` | as-designed (a kind, not a shape) |
//! | row, 0 clauses | `synth` | nothing upstream wrote it |
//!
//! "Reaches" is the whole criterion, and it counts **shared** rows: a statement
//! that wired a row another statement also wired did get something, and asking
//! exclusively — which is what "owns" means — reported exactly those
//! statements as having produced nothing. How many **nets** a statement
//! produced is a different fact and is published as `layer`, a parallel
//! reading beside the class, never as the class.
//!
//! `skip` is decided by **AST node kind**, never by name or text (§5.3 source-side item 2):
//! whether an item participates in modelling is a structural fact, so `skip` is
//! orthogonal to the cardinality axis and shares no cell with `drop`. It stays
//! `skip` even when rows do land inside its span — a `pins = [...]` block is
//! skipped *as a construct*, and the rows it declares are listed on it rather
//! than reclassifying it as an expansion. The second line counts those rows
//! apart, so `skip N` on the summary line cannot read as N statements that
//! produced nothing.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::ast::macros::{
    MCAST_ATTRIBUTE, MCAST_ATTRIBUTE_PIN, MCAST_ATTRIBUTE_PINADD, MCAST_BODY, MCAST_COMPONENT,
    MCAST_DECLARE, MCAST_DOMAIN, MCAST_FUNCTION, MCAST_MODULE, MCAST_NET, MCAST_NET_PORTS,
    MCAST_RAIL, MCAST_REF, MCAST_USE, MCAST_USE_PUB,
};
use crate::ast::node::AstNode;
use crate::db::cmie::tables::WORKSPACE;
use crate::instant::insttab::{InstEntry, InstKind, InstTable};
use crate::instant::world::member_overlap;
use crate::semantic::common::SourcePos;
use crate::vector::graph::graphdef::McVecGraph;
use crate::viz::api::RenderedLayer;
use crate::viz::metrics::SchematicQualityReport;
use crate::viz::project::ProjectionLog;

use super::{loc_cell, loc_of, p2, render_table, vec, viz, SourceText, StageView};

/// The `view` value of this readout. Spelled with `->` rather than the doc's
/// `→`: a `view` value is an identifier a consumer greps for, and the arrow is
/// there to be read by a human.
pub const SRC_P2_VIEW: &str = "join.src->p2";

/// The two inner hops, spelled the same way. The pair is in the name because
/// `join` is not symmetric: `join p2 vec` reads p2 as upstream and vec as
/// downstream, and the counts mean different things in the other order.
pub const P2_VEC_VIEW: &str = "join.p2->vec";
pub const VEC_VIZ_VIEW: &str = "join.vec->viz";

/// The six class words of the summary line, in print order. Fixed, so two runs
/// cannot differ by which words appear, and printed even at zero, so an absent
/// word cannot read as "not implemented" (§5.3).
pub const SIX_WORDS: &[&str] = &["carry", "expand", "merge", "drop", "synth", "skip"];

/// The two states the class column prints that are **not** classes: the ways
/// this readout declines to classify. §5.3 puts them in the class column and
/// the summary line counts only the six, so they are counted apart, printed
/// after the six groups, and — being statements about *this join* rather than
/// about the object — never written back into a `stage.*` item (O9 / O10).
pub const DIAG_WORDS: &[&str] = &["branch", "ambiguous"];

/// Whether a word may be passed to `--only`: the six classes and the two
/// diagnostic states. A filtered readout cannot ask for a word the class column
/// can never print.
pub fn is_class_word(word: &str) -> bool {
    SIX_WORDS.contains(&word) || DIAG_WORDS.contains(&word)
}

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
/// The `skip` clauses that do own rows, on the same line and in the same
/// "label then count" shape as the two sub-hop labels. `skip` is a verdict on
/// the construct's kind, not on its cardinality, so the count of members on the
/// summary line is a total only this split can read: the rest took no part in
/// modelling at all. Printed even at zero, so a `skip` that never owns a row
/// cannot hide behind a branch the readout never reached.
const WITH_DOWNSTREAM_LABEL: &str = "\u{5e26}\u{4e0b}\u{6e38}";
/// The two labels of the inner hops' second line: the objects no declared kind
/// admits, and the objects of a declared kind that hold no key. Both are counts
/// the summary line would otherwise absorb — and both are proof that an empty
/// class is empty rather than a branch the readout never reached.
const OFFHOP_LABEL: &str = "\u{4e0d}\u{53c2}\u{4e0e}";
const KEYLESS_LABEL: &str = "\u{65e0}\u{952e}";
/// The `drop` / `synth` annotations of the inner hops, one pair per match rule,
/// so the note says *which check* ran: an equal key, a shared member, an equal
/// end pair. The three are not interchangeable — a `drop` under the member rule
/// means "no member in common", not "no such key".
const DROP_KEY: &str = "\u{4e0b}\u{6e38}\u{65e0}\u{540c}\u{952e}\u{9879}";
const SYNTH_KEY: &str = "\u{4e0a}\u{6e38}\u{65e0}\u{540c}\u{952e}\u{9879}";
const DROP_MEMBER: &str = "\u{4e0b}\u{6e38}\u{65e0}\u{6210}\u{5458}\u{4ea4}\u{96c6}";
const SYNTH_MEMBER: &str = "\u{4e0a}\u{6e38}\u{65e0}\u{6210}\u{5458}\u{4ea4}\u{96c6}";
const DROP_ENDS: &str = "\u{4e0b}\u{6e38}\u{65e0}\u{540c}\u{7aef}\u{5bf9}";
const SYNTH_ENDS: &str = "\u{4e0a}\u{6e38}\u{65e0}\u{540c}\u{7aef}\u{5bf9}";
/// Prefixed by the size of the tie — the count of objects that claim one
/// another — read from the side that holds the tie. A row refused because the
/// *other* side is tied would otherwise print "1 tied", which says nothing.
const AMBIGUOUS_NOTE: &str = "\u{9879}\u{5e76}\u{5217}\u{ff0c}\u{4e0d}\u{731c}";
const BRANCH_NOTE: &str = "\u{4e24}\u{7aef}\u{4e0d}\u{5168}\u{6709}\u{952e}";

/// Group order of the text face: the `drop` group on top, because it is the
/// debug entry point (§5.3 ②), then `synth` (the other flagged class), then the
/// rest in count-word order. Also the sort order of the JSON `items`, so the two
/// faces present the same sequence and not merely the same set.
const GROUP_ORDER: &[&str] = &["drop", "synth", "carry", "expand", "merge", "skip"];

/// Every class the class column can print, in print order: the six classes and
/// then the two diagnostic states. One list, so the sort order of the items and
/// the group order of the text face cannot drift.
fn print_order() -> impl Iterator<Item = &'static str> {
    GROUP_ORDER.iter().chain(DIAG_WORDS).copied()
}

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

/// Which position state anchored a row.
///
/// Three values and not a boolean: a net row owns no position at all, and a
/// boolean cannot say that — it would have to call "nothing" a declaration.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Via {
    /// A wiring site — a statement named this object.
    Wired,
    /// The declaration site, and nothing wired it.
    Declared,
    /// Neither: the object owns no position of its own.
    None,
}

impl Via {
    fn word(self) -> &'static str {
        match self {
            Via::Wired => "wired",
            Via::Declared => "declared",
            Via::None => "none",
        }
    }
}

/// The positions a row was written at.
///
/// A wiring site is **not** unique: two statements may name the same endpoint
/// and both are facts about it. Every one is kept, in walk order, and the
/// clause attribution reads them all — one position per row is what made a
/// statement's second site invisible, and what made a shared row unownable.
struct RowAnchor {
    /// Every wiring site, in walk order.
    sites: Vec<SourcePos>,
    /// The declaration site, when the entity has one.
    decl: Option<SourcePos>,
    /// Which of the two states anchored the row.
    via: Via,
}

impl RowAnchor {
    /// The anchor of a flat-table entry: its wiring sites and its declaration
    /// site, both recorded, with the state that anchored read off them.
    fn of(entry: &InstEntry) -> RowAnchor {
        let sites: Vec<SourcePos> = entry.src_pos.iter().cloned().collect();
        let decl = entry.fallback_pos.clone();
        let via = if !sites.is_empty() {
            Via::Wired
        } else if decl.is_some() {
            Via::Declared
        } else {
            Via::None
        };
        RowAnchor { sites, decl, via }
    }

    /// Every position that may attribute this row to a clause: all wiring
    /// sites, and the declaration site **only when nothing wired it** (a
    /// declaration says where the object was written, not where it was used).
    fn attribution(&self) -> impl Iterator<Item = &SourcePos> {
        let decl = if self.sites.is_empty() {
            self.decl.as_ref()
        } else {
            None
        };
        self.sites.iter().chain(decl)
    }
}

/// A downstream row: one line of the flat table.
struct DRow {
    class: &'static str,
    /// The row's key, spelled exactly as `stage.p2` spells it (`D<id>` for an
    /// instance, the `PointId` for a point, `net:<label>` for a labelled net,
    /// `null` for the objects that own no key).
    key: Value,
    loc: Value,
    /// Every position the row was written at, and the state that anchored it.
    /// `None` for a net row, which owns no position of its own — the readout
    /// says so rather than inventing one for it.
    anchor: Option<RowAnchor>,
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
    // acquired a clause. **Every** wiring site is asked, not just the first:
    // one statement may wire an endpoint a second time, and a row whose second
    // site falls in that statement's span does belong to it.
    let mut row_clauses: Vec<Vec<usize>> = rows
        .iter()
        .map(|row| match &row.anchor {
            Some(anchor) => {
                let mut cs: Vec<usize> = Vec::new();
                for at in anchor.attribution() {
                    if let Some(c) = containing_clause(&clauses, &at.uri, at.offset as usize) {
                        if !cs.contains(&c) {
                            cs.push(c);
                        }
                    }
                }
                cs.sort_unstable();
                cs
            }
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

    // A clause **reaches** every row whose clause set holds it, shared or not.
    // This is the clause axis' own relation, and both questions on it are asked
    // through it: the class ("did this statement get anything at all") and
    // sub-hop 2 ("did Pass 2 write a row inside this span"). Neither is a
    // question about exclusivity, and asking them exclusively made a statement
    // that wired a row another statement also wired read as having produced
    // nothing. Rows keep their order, so a clause's list is in row order.
    let mut reached: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (ri, cs) in row_clauses.iter().enumerate() {
        for c in cs {
            reached.entry(*c).or_default().push(ri);
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
    // A `skip` clause may still own rows: its kind takes no part in modelling,
    // but the construct can declare rows all the same (a `pins = [...]` block).
    // Counted apart so the summary line's `skip N` cannot read as N statements
    // that produced nothing — the split is what makes the word and the
    // cardinality legible as the two different things they are.
    let mut skip_with_downstream = 0usize;
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
    //
    // "Inside its span" is `reached`, not exclusive ownership. `owners` is a
    // strictly narrower relation — it excludes a row this clause shares with
    // another — and using it here would answer "Pass 2 wrote nothing only for
    // this statement", which is not the claim and which a set-valued anchor
    // makes rare for no reason.
    let mut ast_p2_mismatch = 0usize;
    for (ci, clause) in clauses.iter().enumerate() {
        let Some(at) = clause.stmt_at else {
            continue;
        };
        if !clause.pass1 || !claimed.contains(&(clause.uri.clone(), at)) {
            continue;
        }
        if reached.get(&ci).map(Vec::is_empty).unwrap_or(true) {
            ast_p2_mismatch += 1;
        }
    }

    for (ci, clause) in clauses.iter().enumerate() {
        let hit = reached.get(&ci).cloned().unwrap_or_default();
        // The class states **how many rows the statement reaches**: none, one,
        // or more. The count is of rows, never of exclusively-owned rows, and
        // never of layer members — how many nets the statement produced is
        // published beside it as `layer`, a parallel reading, not the criterion.
        let class = if clause.skip {
            "skip"
        } else if hit.is_empty() {
            if clause.declare {
                declarations_unmatched += 1;
                continue;
            }
            "drop"
        } else if hit.len() == 1 {
            "carry"
        } else {
            "expand"
        };
        // The layer this statement produced, as a count: the net rows it
        // reaches. Published **beside** the class and never as the class — the
        // class counts rows, and a layer of one net is a different fact from a
        // reach of one row.
        let layer = hit.iter().filter(|ri| rows[**ri].class == "net").count();
        counts.insert(class, counts[class] + 1);
        if class == "skip" && !hit.is_empty() {
            skip_with_downstream += 1;
        }
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
                "to": rows_value(&rows, &hit),
                "layer": layer,
                "why": why,
            }),
        ));
    }

    for (ri, row) in rows.iter().enumerate() {
        let cs = &row_clauses[ri];
        let pos_keys = position_keys(row, &mut sources);
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
                    "loc_all": pos_keys.0,
                    "decl_loc": pos_keys.1,
                    "via": pos_keys.2,
                    "from": Value::Array(cs.iter().map(|c| clause_key(&clauses[*c], &mut sources)).collect()),
                    "to": Value::Array(vec![row.key.clone()]),
                    "members": Value::Array(
                        row.member_paths.iter().map(|p| Value::String(p.clone())).collect()
                    ),
                    "why": format!("{} {}", cs.len(), MERGE_NOTE),
                }),
            ));
        } else if cs.is_empty() {
            // The position that anchored the row: the first wiring site, else
            // the declaration site. Both span buckets below ask "where was this
            // written", and that is exactly the anchoring position.
            let anchored_at = row
                .anchor
                .as_ref()
                .and_then(|a| a.sites.first().or(a.decl.as_ref()));
            if anchored_at.is_none() {
                // A key's defect, not an event (see the module note).
                unanchored += 1;
                *unanchored_by_class.entry(row.class).or_default() += 1;
            } else if in_func_span(&func_spans, anchored_at.unwrap()) {
                // Positioned inside a `func` body, which this hop does not walk.
                // The row was written by the call site's statement, so there is
                // an upstream for it — just not one of these items. Reporting it
                // as `synth` would claim a generated object where the truth is
                // an excluded template (§5.2 hard constraint 3).
                func_scoped += 1;
            } else if in_func_span(&header_spans, anchored_at.unwrap()) {
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
                        "loc_all": pos_keys.0,
                        "decl_loc": pos_keys.1,
                        "via": pos_keys.2,
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
    counts_value.insert("skip_with_downstream".into(), json!(skip_with_downstream));
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
    print_order()
        .position(|c| c == class)
        .unwrap_or_else(|| print_order().count()) as u8
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
/// Every in-scope source clause as `(uri, start, end, text)`, sorted by
/// `(uri, start)`.
///
/// The chain's head is a statement, so a caller that starts from one — `trace`
/// does, on a source-position key — needs the spans themselves, which the hop
/// readout keeps only as an item's `loc`. Sorting is not cosmetic: the walk
/// that builds these follows the workspace's own file map, so the sequence has
/// to be imposed here for a derived ordinal (`phrase#<n>`) to be a function of
/// the input rather than of iteration order (build-design §3.7 discipline 0).
pub fn clause_spans() -> Vec<(String, usize, usize, String)> {
    let (mut clauses, _, _) = in_scope_clauses();
    clauses.sort_by(|a, b| (&a.uri, a.start).cmp(&(&b.uri, b.start)));
    clauses
        .into_iter()
        .map(|c| (c.uri, c.start, c.end, c.text))
        .collect()
}

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
                if body.get_sub_node().is_none() {
                    continue;
                }
                // An in-body partition (`block`) is transparent here too: its
                // clauses are read as if written directly in this body, so a
                // row anchored inside one lands in the statement that wrote it
                // rather than in no clause at all.
                let list: Vec<AstNode> = body.clause_list();
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

/// Whether a position sits inside one of these spans.
fn in_func_span(spans: &[(String, usize, usize)], at: &SourcePos) -> bool {
    let offset = at.offset as usize;
    spans
        .iter()
        .any(|(uri, start, end)| at.uri == *uri && *start <= offset && offset < *end)
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
        // The first wiring site anchors the row; the declaration site is used
        // only when nothing wired it. Which one it was is published as `via`.
        let anchor = RowAnchor::of(entry);
        rows.push(DRow {
            class,
            key,
            loc: loc_of(entry.anchor_pos(), &mut sources),
            anchor: Some(anchor),
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

/// The three position keys every **row** item carries: `loc_all` (every wiring
/// site, always an array — a one-element array rather than a bare position, so
/// the shape never depends on the data), `decl_loc` (the declaration site), and
/// `via` (which state anchored the row).
///
/// A net row owns no position of its own, so it reports the empty array and
/// `via: "none"` — which is why `via` has three values: a boolean could not
/// tell "declared" from "there is nothing here at all".
fn position_keys(row: &DRow, sources: &mut SourceText) -> (Value, Value, &'static str) {
    match &row.anchor {
        Some(anchor) => (
            Value::Array(
                anchor
                    .sites
                    .iter()
                    .map(|p| loc_of(Some(p), sources))
                    .collect(),
            ),
            anchor
                .decl
                .as_ref()
                .map(|p| loc_of(Some(p), sources))
                .unwrap_or(Value::Null),
            anchor.via.word(),
        ),
        None => (Value::Array(vec![]), Value::Null, Via::None.word()),
    }
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
    let mut out = vec![view.header_line(), second_line(view), six_word_line(view)];
    for (gi, class) in print_order().enumerate() {
        let rows: Vec<Vec<String>> = view
            .items
            .iter()
            .filter(|i| i["class"] == class)
            .map(|i| {
                let prefix = match class {
                    "drop" => "!",
                    "synth" => "+",
                    _ => " ",
                };
                let (detail, tail) = match class {
                    // The member set, not the key: a label is not an identity
                    // and two nets may share one (§5.3 six-class table). A row
                    // that is a single object rather than a net — an endpoint
                    // two statements both name — has no member set, and names
                    // itself instead: the cell must be able to say which shape
                    // it is holding, or that row has no identity on the face.
                    "merge" => (
                        match i["members"].as_array() {
                            Some(m) if !m.is_empty() => keys_cell(&i["members"]),
                            _ => detail_cell(i),
                        },
                        format!("← {}", keys_cell(&i["from"])),
                    ),
                    // The object the downstream segment has and no upstream
                    // object accounts for: named as that segment names it, which
                    // for the source hop is exactly its key.
                    "synth" => (detail_cell(i), String::new()),
                    // A tie is reported by showing what tied, and which side the
                    // candidates are on decides which column holds them: a
                    // left-side row names its rights, a right-side row names its
                    // lefts. Printing only one of the two would leave half the
                    // ties looking like a plain non-match.
                    "ambiguous" => (
                        detail_cell(i),
                        if i["to"].as_array().is_some_and(|a| !a.is_empty()) {
                            keys_cell(&i["to"])
                        } else {
                            format!("← {}", keys_cell(&i["from"]))
                        },
                    ),
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

/// The second line, which says which hop this is: the two sub-hops of `src -> p2`
/// counted apart, or the two kinds of object the inner hops keep out of the join
/// counted apart. Both exist so a number the summary line would otherwise absorb
/// — "the parser dropped it" versus "Pass 2 never wrote it"; "not an object of
/// this hop" versus "an object with no key" — is legible in the row. The source
/// hop carries one more of the same kind: how many of its `skip` clauses do own
/// rows, which is the only way to read `skip`'s count as the kind verdict it is.
fn second_line(view: &StageView) -> String {
    match view.view {
        SRC_P2_VIEW => format!(
            "# {} src->ast {} {}   ast->p2 {} {}   skip {} {}",
            SUBHOP_LABEL,
            MISMATCH_LABEL,
            view.counts["sub_hop_src_ast"].as_u64().unwrap_or(0),
            MISMATCH_LABEL,
            view.counts["sub_hop_ast_p2"].as_u64().unwrap_or(0),
            WITH_DOWNSTREAM_LABEL,
            view.counts["skip_with_downstream"].as_u64().unwrap_or(0)
        ),
        _ => format!(
            "# {} {}   {} {}   branch {}   ambiguous {}",
            OFFHOP_LABEL,
            view.counts["offhop_total"].as_u64().unwrap_or(0),
            KEYLESS_LABEL,
            view.counts["keyless_total"].as_u64().unwrap_or(0),
            view.counts["branch"].as_u64().unwrap_or(0),
            view.counts["ambiguous"].as_u64().unwrap_or(0)
        ),
    }
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

// ── The two inner hops ──
//
// `src -> p2` matched a statement against the rows it wrote by *position*, and
// both sides of that hop are tables of the same kind. The two inner hops match
// objects instead, one kind of object at a time, each kind by the key §2.4 gives
// it:
//
// | hop | kind | left classes | right classes | matched on |
// |---|---|---|---|---|
// | p2 -> vec | `instance` | `instance` | `box` / `layer` | the `D<id>` key |
// | p2 -> vec | `point` | `point` | `endpoint` | the `PointId` |
// | p2 -> vec | `net` | `net` | `net` | the member set |
// | vec -> viz | `instance` | `box` / `layer` | `box` / `layer` | the `D<id>` key |
// | vec -> viz | `point` | `endpoint` | `pin` | the `PointId` |
// | vec -> viz | `path` | `trunk` | `segment` | the ordered end pair |
//
// Three things these hops do **not** do, each a ruling rather than an omission:
//
// - **They do not fall back to a name.** A net's label is not an identity — two
//   modules may each declare a `GND`, and on the fixture they do (7 of the 59 net
//   items in p2 carry a label a second net item already carries) — so nets are
//   matched by their member set and never by their label. The same holds for a
//   trunk's name, which a drawn segment also carries and with which it disagrees.
// - **They do not classify what they cannot match.** An object of a declared kind
//   that holds no key is counted apart instead of reported as `drop`; a path
//   whose two ends are not both named is `branch` (O9). Both are numbers in the
//   second line, so an empty class is proved empty rather than assumed.
// - **They do not guess at a key that names more than one object.** A key names
//   whatever carries it, so one key with several objects on **both** sides says
//   the two sides correspond without saying which pairs with which — the
//   projection publishing both the collapsed box of a module and the layer of its
//   interior is that shape. Pairing those by class or by name would be a
//   fallback, so the object is `ambiguous`. `merge`, the other half of the card,
//   needs one object downstream of several and neither hop has one; `by_kind`
//   publishes the per-kind relation counts, so those zeros are read rather than
//   asserted.

/// Build `join p2->vec`: the flat instance table against the vector graph.
pub fn build_join_p2_vec(
    graph: &McVecGraph,
    log: &ProjectionLog,
    table: &InstTable,
    top: &str,
    diagnostics: usize,
) -> StageView {
    build_join_p2_vec_with_sides(graph, log, table, top, diagnostics).join
}

/// The same hop, handing back the two sides it was built from.
///
/// `trace` follows one object rather than classifying a whole hop, so it needs
/// the objects themselves and not only the matching — the canonical key a
/// canonical-path lookup resolves against lives on the side's own item, which
/// [`build_hop`] would otherwise drop. Returning them here rather than building
/// each side a second time is what keeps `trace` and `join` readings of *one*
/// build (§5.3 ruling ③).
pub fn build_join_p2_vec_with_sides(
    graph: &McVecGraph,
    log: &ProjectionLog,
    table: &InstTable,
    top: &str,
    diagnostics: usize,
) -> HopSides {
    // Both sides are the segments' own builders, read back as items — never a
    // second derivation of the same objects. Otherwise `join` and `mcc show
    // stage p2` would drift apart one edit at a time (§5.3 ruling ③).
    let left = p2::build_p2(table, top, diagnostics);
    let right = vec::build_vec(graph, log, table, top, diagnostics);
    let join = build_hop(&P2_VEC, &left, &right);
    HopSides { join, left, right }
}

/// Build `join vec->viz`: the vector graph against the laid-out drawing.
///
/// Both sides are built here, in pipeline order — the vec view is read off the
/// graph, then the graph is rendered and the viz view is read off what the
/// renderer consumed. Taking the graph **by value** is that order rather than a
/// convenience: the renderer consumes it, and rendering a copy to keep one
/// around would make this a second reading of a different value.
pub fn build_join_vec_viz(
    graph: McVecGraph,
    log: &ProjectionLog,
    table: &InstTable,
    top: &str,
    diagnostics: usize,
) -> StageView {
    build_join_vec_viz_with_sides(graph, log, table, top, diagnostics).join
}

/// The same hop, handing back the two sides it was built from — see
/// [`build_join_p2_vec_with_sides`].
pub fn build_join_vec_viz_with_sides(
    graph: McVecGraph,
    log: &ProjectionLog,
    table: &InstTable,
    top: &str,
    diagnostics: usize,
) -> HopSides {
    let left = vec::build_vec(&graph, log, table, top, diagnostics);
    let mut layers: Vec<RenderedLayer> = Vec::new();
    let (_doc, metrics) = crate::viz::api::render_with_metrics_and_sink(
        graph,
        crate::viz::api::RenderOpts::default(),
        Some(&mut layers),
    );
    let quality: SchematicQualityReport = metrics.finish_quality(None);
    let right = viz::build_viz(&layers, &quality, table, top, diagnostics);
    let join = build_hop(&VEC_VIZ, &left, &right);
    HopSides { join, left, right }
}

/// One hop: its readout, plus the two sides it was built from.
///
/// The sides are the segments' own views, so a caller that follows an object
/// across hops (rather than classifying a hop) reads the *same* items `mcc show
/// stage <seg>` publishes, and a key one of them holds is a key the other two
/// commands agree on by construction.
pub struct HopSides {
    /// The hop readout: six words, both directions of every match.
    pub join: StageView,
    /// The upstream segment's view.
    pub left: StageView,
    /// The downstream segment's view.
    pub right: StageView,
}

/// How the two sides of one kind are matched.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MatchRule {
    /// The object's own §2.4 key: `D<id>` for an instance, `PointId` for a pin.
    Key,
    /// Member-set overlap, for the kind that owns no key (O10).
    MemberSet,
    /// The ordered pair of both ends' canonical keys, for a path (O9).
    EndPair,
}

impl MatchRule {
    /// The `drop` / `synth` annotations. The note says which check ran, so a
    /// reader can tell "no object with this key" from "no member in common".
    fn notes(self) -> (&'static str, &'static str) {
        match self {
            MatchRule::Key => (DROP_KEY, SYNTH_KEY),
            MatchRule::MemberSet => (DROP_MEMBER, SYNTH_MEMBER),
            MatchRule::EndPair => (DROP_ENDS, SYNTH_ENDS),
        }
    }
}

/// One kind of object at a hop: the classes that carry it on each side, and how
/// the two sides are matched.
struct HopKind {
    kind: &'static str,
    left: &'static [&'static str],
    right: &'static [&'static str],
    rule: MatchRule,
}

/// One hop: which segments it joins and which kinds of object it knows.
struct Hop {
    view: &'static str,
    left: &'static str,
    right: &'static str,
    kinds: &'static [HopKind],
}

const P2_VEC: Hop = Hop {
    view: P2_VEC_VIEW,
    left: "p2",
    right: "vec",
    kinds: &[
        // A module instance becomes a box, and a module that owns a layer
        // becomes that layer too — hence N>1 here, which is what `expand` is.
        HopKind {
            kind: "instance",
            left: &["instance"],
            right: &["box", "layer"],
            rule: MatchRule::Key,
        },
        HopKind {
            kind: "point",
            left: &["point"],
            right: &["endpoint"],
            rule: MatchRule::Key,
        },
        HopKind {
            kind: "net",
            left: &["net"],
            right: &["net"],
            rule: MatchRule::MemberSet,
        },
    ],
};

const VEC_VIZ: Hop = Hop {
    view: VEC_VIZ_VIEW,
    left: "vec",
    right: "viz",
    kinds: &[
        // Both sides publish a module's `D<id>` twice — once as the collapsed box
        // and once as the layer of its interior — so this kind is where a key
        // names more than one object per side, and what the join makes of that is
        // the diagnostic state rather than a pairing.
        HopKind {
            kind: "instance",
            left: &["box", "layer"],
            right: &["box", "layer"],
            rule: MatchRule::Key,
        },
        HopKind {
            kind: "point",
            left: &["endpoint"],
            right: &["pin"],
            rule: MatchRule::Key,
        },
        // A trunk and a segment are the same kind of object at two segments —
        // a path between two endpoints (§2.4) — so their handle is the pair of
        // ends and nothing is issued for either.
        HopKind {
            kind: "path",
            left: &["trunk"],
            right: &["segment"],
            rule: MatchRule::EndPair,
        },
    ],
};

/// One object of a declared kind, ready to be matched.
struct Obj {
    kind: &'static str,
    /// Which segment's view published it, so a row can say where it lives.
    seg: &'static str,
    /// The class it has in its own view (`box`, `layer`, `pin`), which is not
    /// the class this readout prints: the class column is the six words.
    stage_class: String,
    /// The §2.4 key, when the kind has one. `null` for a kind matched by a
    /// derived criterion, whose item therefore carries no key at all.
    key: Option<String>,
    /// The member set a net is matched on (O10). Empty for the other kinds.
    members: Vec<String>,
    /// How the object is spelled in the `from` / `to` columns: its `stage_class`
    /// then its key, because one key can name objects of **two** classes — a
    /// module's `D<id>` is both its box and the layer of its interior — and two
    /// handles reading `D39 D39` would say nothing about which is which.
    handle: String,
    /// The detail column.
    text: String,
    loc: Value,
    /// Deterministic tie-break inside a class: the canonical spelling.
    sort: String,
}

/// An object of a declared kind that cannot enter the join, and why.
enum Unnamed {
    /// It holds no key at all (`key: null`): §2.4 says a label and a bus member
    /// own no physical point, so it is not a chain object here.
    Keyless,
    /// A path whose two ends are not both named (O9), and a path has no key to
    /// fall back on.
    Branch,
}

fn obj_of(item: &Value, hk: &HopKind, seg: &'static str) -> Result<Obj, Unnamed> {
    let stage_class = item["class"].as_str().unwrap_or("-").to_string();
    let canon = item["canon_key"]["path"].as_str();
    let base = Obj {
        kind: hk.kind,
        seg,
        stage_class,
        key: None,
        members: Vec::new(),
        handle: String::new(),
        text: String::new(),
        loc: item["loc"].clone(),
        sort: String::new(),
    };
    let mut o = base;
    match hk.rule {
        MatchRule::Key => {
            let Some(key) = item["key"].as_str() else {
                return Err(Unnamed::Keyless);
            };
            o.key = Some(key.to_string());
            o.handle = format!("{}:{}", o.stage_class, key);
            o.text = obj_text(item, key);
            o.sort = canon.unwrap_or(key).to_string();
        }
        MatchRule::MemberSet => {
            // The label is read for the row and for the handle, never for the
            // match: §2.4 makes the member set the criterion (O10).
            let name = item["net"]
                .as_str()
                .or_else(|| item["name"].as_str())
                .unwrap_or("-");
            o.members = item["members"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            // The member count is in the text on purpose: two nets of one layer
            // may share a name on the fixture, and the criterion is the set.
            o.text = format!("{name} members={}", o.members.len());
            o.handle = format!("{}:{}", o.stage_class, name);
            o.sort = format!("{}\u{1}{}", name, o.members.join(","));
        }
        MatchRule::EndPair => {
            let Some(pair) = end_pair(item) else {
                return Err(Unnamed::Branch);
            };
            // The pair is the object's handle (§2.4) and is published as its
            // key; the row itself prints the view's own spelling for it, because
            // a pair of nine-lane ends is a paragraph and a reader scans names.
            o.key = Some(pair.clone());
            o.handle = format!("{}:{}", o.stage_class, pair);
            o.text = own_text(item);
            o.sort = pair;
        }
    }
    Ok(o)
}

/// The detail column of an object that owns a key: its own path, or the key when
/// it has no path (a net has neither).
fn obj_text(item: &Value, key: &str) -> String {
    item["path"]
        .as_str()
        .filter(|p| !p.is_empty())
        .unwrap_or(key)
        .to_string()
}

/// A path's handle: the ordered pair of both ends' canonical keys, each end a
/// **sorted** list so a multi-lane run cannot depend on lane order (O9).
///
/// `None` when either end is not named — a routed `wire` segment records
/// coordinates and no ends, and an end whose path is `null` is one the renderer
/// invented (a rail-synthesised endpoint), which owns no canonical key. There is
/// then no pair to compute rather than a pair to guess, and O9 calls that
/// `branch`.
fn end_pair(item: &Value) -> Option<String> {
    let (left, right) = match item["class"].as_str().unwrap_or("") {
        // A trunk carries its ends lane by lane; a segment side by side.
        "trunk" => (lanes(item, "left")?, lanes(item, "right")?),
        _ => (ends(item, "from")?, ends(item, "to")?),
    };
    Some(format!("{}->{}", left.join(","), right.join(",")))
}

fn lanes(item: &Value, side: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = item["lanes"]
        .as_array()?
        .iter()
        .map(|l| l[side].as_str().map(str::to_string))
        .collect::<Option<Vec<String>>>()?;
    if out.is_empty() {
        return None;
    }
    out.sort();
    Some(out)
}

fn ends(item: &Value, side: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = item[side]
        .as_array()?
        .iter()
        .map(|e| e["path"].as_str().map(str::to_string))
        .collect::<Option<Vec<String>>>()?;
    if out.is_empty() {
        return None;
    }
    out.sort();
    Some(out)
}

/// The relation one kind produced, and the class counts inside it.
///
/// The totals are the counts of the objects `partition` handed over; the numbers
/// of the kind's objects that stayed outside the join (no key, or a path whose
/// ends are not both named) are counted there, so a kind's whole population is
/// `joined + kept_out` and an empty class cannot hide a non-empty bucket.
struct KindJoin {
    joined_left: usize,
    joined_right: usize,
    classes: BTreeMap<&'static str, usize>,
}

/// Join one kind's two sides, by the rule the kind declared.
///
/// **`Key` and `EndPair` go through the component card** ([`join_by_key`]):
/// one key names one component, and the shape of that component *is* the answer.
///
/// **`MemberSet` cannot.** A member set names nothing (§2.4, O10), so the
/// relation it induces is not transitive and has no components — the only thing
/// to read is which pair is the best on both sides, and a tie on a member set is
/// two objects that are indistinguishable rather than one object seen twice.
///
/// Every object is classified **exactly once** either way, so the summary line is
/// a reading of the hop and not a restatement of the table.
fn join_kind(hk: &HopKind, left: &[Obj], right: &[Obj]) -> (Vec<(u8, String, Value)>, KindJoin) {
    let (out, classes) = match hk.rule {
        MatchRule::MemberSet => join_by_member_set(hk, left, right),
        MatchRule::Key | MatchRule::EndPair => join_by_key(hk, left, right),
    };
    (
        out,
        KindJoin {
            joined_left: left.len(),
            joined_right: right.len(),
            classes,
        },
    )
}

/// Join a kind whose pairs are decided by a key: one key is one component.
///
/// Every object carrying one key is in the same connected component of the
/// relation, which is then read off by [`classify_components`].
fn join_by_key(
    hk: &HopKind,
    left: &[Obj],
    right: &[Obj],
) -> (Vec<(u8, String, Value)>, BTreeMap<&'static str, usize>) {
    let mut groups: BTreeMap<&str, (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for (i, o) in left.iter().enumerate() {
        if let Some(k) = o.key.as_deref() {
            groups.entry(k).or_default().0.push(i);
        }
    }
    for (j, o) in right.iter().enumerate() {
        if let Some(k) = o.key.as_deref() {
            groups.entry(k).or_default().1.push(j);
        }
    }
    classify_components(hk, left, right, groups.into_values().collect())
}

/// Read the card off the components of a relation (§5).
///
/// One component is the set of objects the relation ties together, and the card
/// is read off its two sides: (1,1) `carry`, (1,N) `expand`, (N,1) `merge`,
/// (1,0) `drop`, (0,1) `synth`.
///
/// A component with several objects on **both** sides is not one of the six
/// words: the relation says the two sides correspond, not which object pairs
/// with which. It is the diagnostic state, reported from both sides rather than
/// guessed — calling it `expand` would report a fan-out that nothing measured,
/// and pairing across the split by class or by name would be the fallback the
/// design forbids. On the fixture this is what a module's `D<id>` looks like
/// where the projection published both its collapsed box and the layer of its
/// interior: one key, two objects per side.
///
/// A `merge` row is anchored on the one downstream object with the upstream ones
/// in `from`, the shape `join src p2` gives it.
fn classify_components(
    hk: &HopKind,
    left: &[Obj],
    right: &[Obj],
    groups: Vec<(Vec<usize>, Vec<usize>)>,
) -> (Vec<(u8, String, Value)>, BTreeMap<&'static str, usize>) {
    let (drop_note, synth_note) = hk.rule.notes();
    let mut out: Vec<(u8, String, Value)> = Vec::new();
    let mut classes = zero_classes();
    for (ls, rs) in &groups {
        let (ls, rs) = (ls.as_slice(), rs.as_slice());
        let to: Vec<String> = rs.iter().map(|j| right[*j].handle.clone()).collect();
        let from: Vec<String> = ls.iter().map(|i| left[*i].handle.clone()).collect();
        match (ls.len(), rs.len()) {
            (_, 0) => {
                for i in ls {
                    bump(&mut classes, "drop");
                    push_item(&mut out, "drop", &left[*i], &[], &[], drop_note.to_string());
                }
            }
            (0, _) => {
                for j in rs {
                    bump(&mut classes, "synth");
                    let o = &right[*j];
                    push_item(
                        &mut out,
                        "synth",
                        o,
                        &[],
                        &[o.handle.clone()],
                        synth_note.to_string(),
                    );
                }
            }
            (1, 1) => {
                bump(&mut classes, "carry");
                push_item(&mut out, "carry", &left[ls[0]], &[], &to, String::new());
            }
            (1, _) => {
                bump(&mut classes, "expand");
                push_item(&mut out, "expand", &left[ls[0]], &[], &to, String::new());
            }
            (_, 1) => {
                bump(&mut classes, "merge");
                push_item(
                    &mut out,
                    "merge",
                    &right[rs[0]],
                    &from,
                    &to,
                    format!("{} {}", from.len(), MERGE_NOTE),
                );
            }
            (_, _) => {
                for i in ls {
                    bump(&mut classes, "ambiguous");
                    push_item(
                        &mut out,
                        "ambiguous",
                        &left[*i],
                        &[],
                        &to,
                        format!("{} {}", to.len(), AMBIGUOUS_NOTE),
                    );
                }
                for j in rs {
                    bump(&mut classes, "ambiguous");
                    push_item(
                        &mut out,
                        "ambiguous",
                        &right[*j],
                        &from,
                        &[],
                        format!("{} {}", from.len(), AMBIGUOUS_NOTE),
                    );
                }
            }
        }
    }
    (out, classes)
}

/// Join a kind matched by member-set overlap (O10).
///
/// The same card as [`join_by_key`], read off a relation built without a key:
/// two objects are related when their member sets overlap **and** that overlap
/// is the best score either of them reaches. A score of zero is no evidence of
/// sameness, so it is not an edge — without that rule every object would be
/// related to every other one it shares nothing with, and the two segments
/// would collapse into a single component.
///
/// The relation is the **union** of the two directions' best sets: an edge
/// stands if the pair is best seen from the left **or** from the right. Keeping
/// only pairs that are best from both sides would hide a real fan-in — two
/// upstream nets whose best downstream net is one and the same are one
/// component, and the card calls that `merge`, not two separate carries.
///
/// A tie is never broken. That is O10's ruling, and the one place this differs
/// from [`crate::instant::world::net_deltas`], the overlap function's other
/// caller: a diff must produce exactly one answer, so it settles its ties.
fn join_by_member_set(
    hk: &HopKind,
    left: &[Obj],
    right: &[Obj],
) -> (Vec<(u8, String, Value)>, BTreeMap<&'static str, usize>) {
    let (l, r) = (left.len(), right.len());
    let score = |i: usize, j: usize| member_overlap(&left[i].members, &right[j].members);
    let best_l: Vec<usize> = (0..l)
        .map(|i| (0..r).map(|j| score(i, j)).max().unwrap_or(0))
        .collect();
    let best_r: Vec<usize> = (0..r)
        .map(|j| (0..l).map(|i| score(i, j)).max().unwrap_or(0))
        .collect();

    // Union-find over `left ++ right`, so an edge read in either direction lands
    // in one component. Every loop below scans ascending indices, so the
    // partition is a function of the input and not of any iteration order
    // (build-design 3.7, discipline 0).
    let mut parent: Vec<usize> = (0..l + r).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for i in 0..l {
        for j in 0..r {
            let s = score(i, j);
            if s > 0 && (s == best_l[i] || s == best_r[j]) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, l + j));
                if a != b {
                    parent[a] = b;
                }
            }
        }
    }

    // Components come out in the order of their first member, lefts first, so
    // the grouping is as stable as the segment views it is built from.
    let mut groups: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();
    let mut slot: BTreeMap<usize, usize> = BTreeMap::new();
    for node in 0..l + r {
        let root = find(&mut parent, node);
        let idx = *slot.entry(root).or_insert_with(|| {
            groups.push((Vec::new(), Vec::new()));
            groups.len() - 1
        });
        if node < l {
            groups[idx].0.push(node);
        } else {
            groups[idx].1.push(node - l);
        }
    }
    classify_components(hk, left, right, groups)
}

/// The six words plus the two diagnostic states, all at zero.
///
/// The words are always all present so a summary line can never omit a class by
/// having none of it; the diagnostic states are carried along so a hop with
/// nothing to refuse reads as zero rather than as an absent key.
fn zero_classes() -> BTreeMap<&'static str, usize> {
    SIX_WORDS
        .iter()
        .chain(DIAG_WORDS)
        .map(|w| (*w, 0))
        .collect()
}

/// Count one class in a per-kind tally.
fn bump(classes: &mut BTreeMap<&'static str, usize>, class: &'static str) {
    *classes.entry(class).or_default() += 1;
}

/// Split one segment's items into the objects of one kind and the leftovers.
///
/// Two kinds of leftover, counted apart because they are different statements:
/// an object of a declared kind that holds no key (counted here), and an object
/// of a class no declaration names (counted by [`build_hop`], which sees both
/// sides). The first is about the object, the second about the hop.
fn partition(
    items: &[Value],
    declared: &[&'static str],
    hk: &HopKind,
    seg: &'static str,
    keyless: &mut BTreeMap<String, usize>,
    out: &mut Vec<(u8, String, Value)>,
) -> (Vec<Obj>, usize) {
    let mut objs = Vec::new();
    let mut total = 0usize;
    for item in items {
        let Some(class) = item["class"].as_str() else {
            continue;
        };
        if !declared.contains(&class) {
            continue;
        }
        total += 1;
        match obj_of(item, hk, seg) {
            Ok(o) => objs.push(o),
            Err(Unnamed::Keyless) => *keyless.entry(format!("{seg}.{class}")).or_default() += 1,
            Err(Unnamed::Branch) => {
                // O9: a path whose two ends are not both named does not enter
                // the join. It is reported where it stands rather than as a
                // loss — nothing downstream is missing, a key is.
                let o = branch_obj(item, hk, seg);
                push_item(out, "branch", &o, &[], &[], BRANCH_NOTE.to_string());
            }
        }
    }
    (objs, total)
}

/// The object a `branch` row describes: one that exists and has no handle, so
/// everything a key would have supplied is absent and the row says so.
fn branch_obj(item: &Value, hk: &HopKind, seg: &'static str) -> Obj {
    Obj {
        kind: hk.kind,
        seg,
        stage_class: item["class"].as_str().unwrap_or("-").to_string(),
        key: None,
        members: Vec::new(),
        handle: String::new(),
        text: own_text(item),
        loc: item["loc"].clone(),
        sort: own_text(item),
    }
}

/// The object's own spelling: the path its view gave it when it has one, else
/// the name — with the within-net position for a drawn segment, which is one run
/// of a net and whose label alone is shared with every other run of it.
///
/// Used for the objects whose handle is not a name: a path (§2.4) and a `branch`
/// (no handle at all). A row therefore prints what the object is called where it
/// lives, and the handle stays in the `key` and `to` cells.
fn own_text(item: &Value) -> String {
    let base = ["path", "net", "name"]
        .iter()
        .find_map(|k| item[*k].as_str())
        .unwrap_or("-");
    match item["index"].as_u64() {
        Some(i) => format!("{base}#{i}"),
        None => base.to_string(),
    }
}

/// Join a hop's two views: one traversal of each, one item per object.
fn build_hop(hop: &Hop, left: &StageView, right: &StageView) -> StageView {
    let mut items: Vec<(u8, String, Value)> = Vec::new();
    let mut classes: BTreeMap<&'static str, usize> = SIX_WORDS
        .iter()
        .chain(DIAG_WORDS)
        .map(|w| (*w, 0))
        .collect();
    let mut keyless: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_kind = serde_json::Map::new();

    // Classes no kind on that side declares. Counted per class so the second
    // line's number can always be read back off the rows around it: on the
    // fixture these are `bus` and `label` in p2 (they own no physical point,
    // §2.4), `projection` in vec (the projection's own log) and `metrics` in viz
    // (the quality reports) — containers, logs and reports, none of them a chain
    // object at these hops.
    let mut offhop: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_class: BTreeMap<(bool, String), usize> = BTreeMap::new();
    for (side, view) in [(false, left), (true, right)] {
        for item in &view.items {
            let class = item["class"].as_str().unwrap_or("-").to_string();
            let declared = hop
                .kinds
                .iter()
                .any(|hk| if side { hk.right } else { hk.left }.contains(&class.as_str()));
            *by_class.entry((side, class.clone())).or_default() += 1;
            if !declared {
                let seg = if side { hop.right } else { hop.left };
                *offhop.entry(format!("{seg}.{class}")).or_default() += 1;
            }
        }
    }
    let tally = |m: &BTreeMap<(bool, String), usize>, right: bool| -> Value {
        Value::Object(
            m.iter()
                .filter(|((s, _), _)| *s == right)
                .map(|((_, c), n)| (c.clone(), json!(n)))
                .collect(),
        )
    };

    for hk in hop.kinds {
        let (l, lt) = partition(&left.items, hk.left, hk, hop.left, &mut keyless, &mut items);
        let (r, rt) = partition(
            &right.items,
            hk.right,
            hk,
            hop.right,
            &mut keyless,
            &mut items,
        );
        let (kind_items, kj) = join_kind(hk, &l, &r);
        items.extend(kind_items);
        for (word, n) in &kj.classes {
            *classes.entry(word).or_default() += n;
        }
        by_kind.insert(
            hk.kind.to_string(),
            json!({
                "left": lt,
                "right": rt,
                "joined_left": kj.joined_left,
                "joined_right": kj.joined_right,
                "classes": Value::Object(
                    kj.classes
                        .iter()
                        .filter(|(_, n)| **n > 0)
                        .map(|(w, n)| ((*w).to_string(), json!(n)))
                        .collect(),
                ),
            }),
        );
    }

    items.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));

    let mut counts = serde_json::Map::new();
    for word in print_order() {
        counts.insert(word.to_string(), json!(classes[word]));
    }
    counts.insert("by_kind".into(), Value::Object(by_kind));
    counts.insert("offhop".into(), string_tally(&offhop));
    counts.insert("keyless".into(), string_tally(&keyless));
    counts.insert("offhop_total".into(), json!(offhop.values().sum::<usize>()));
    counts.insert(
        "keyless_total".into(),
        json!(keyless.values().sum::<usize>()),
    );
    counts.insert("by_class_left".into(), tally(&by_class, false));
    counts.insert("by_class_right".into(), tally(&by_class, true));
    counts.insert("left_total".into(), json!(left.items.len()));
    counts.insert("right_total".into(), json!(right.items.len()));

    StageView::with_view(
        hop.view,
        &left.top,
        items.into_iter().map(|(_, _, v)| v).collect(),
        Value::Object(counts),
    )
}

fn string_tally(m: &BTreeMap<String, usize>) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
}

/// One row per object.
///
/// `from` and `to` are the matched objects' handles, so a row always names what
/// it matched: a `merge` its member set, an `ambiguous` the tie it refused to
/// break. A keyed kind carries its key; a kind matched by a derived criterion
/// carries none, because it has none (§2.4).
#[allow(clippy::too_many_arguments)]
fn push_item(
    out: &mut Vec<(u8, String, Value)>,
    class: &'static str,
    o: &Obj,
    from: &[String],
    to: &[String],
    why: String,
) {
    let handles = |v: &[String]| Value::Array(v.iter().cloned().map(Value::String).collect());
    out.push((
        rank_of(class),
        format!("{}\u{1}{}", o.kind, o.sort),
        json!({
            "class": class,
            "kind": o.kind,
            "side": o.seg,
            "stage_class": o.stage_class,
            "key": o.key.clone().map(Value::String).unwrap_or(Value::Null),
            "text": o.text.clone(),
            "loc": o.loc,
            "from": handles(from),
            "to": handles(to),
            "why": why,
        }),
    ));
}
