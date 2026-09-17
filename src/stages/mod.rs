// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Stage readout — one segment of the compile pipeline, as data.
//!
//! Design: `mcd/doc/pipeline/stage-readout-design.md` §3 / §5.3. A *stage view*
//! is a read-side projection of one point on the pipeline chain
//!
//! ```text
//! stmt --join--> stage.p2 --join--> stage.vec --join--> stage.viz
//! ```
//!
//! It is the readout a consumer can rely on. The `MC_VEC_DUMP` / `MC_VIZ_DUMP`
//! switches are the other face of the same segments: debug prose on stderr,
//! gated behind an env var *and* `-d`, interleaved with `[CARVE]` / `[SEG]`
//! noise, and **unsorted**, so comparing two of them is a bet on iteration
//! order (design §1.2 ③). They keep working.
//!
//! Four rules this module holds to, each with its reason:
//!
//! - **One `items`, two faces** (§5.3 ruling ③). Every view builds its items once
//!   and renders text and JSON from that one structure, the same shape
//!   [`crate::export::build_payload`] returns. Two traversals would drift.
//! - **Sorted by key, never by traversal order** (§3). The envelope is a
//!   persisted artifact, so consumers must not have to re-sort it.
//! - **A view is a readout, not a verdict** (law C). Diagnostics are *counted*
//!   and shown; they never flip an exit code.
//! - **A key is either in-domain or canonical, and says which.** `PointId` /
//!   `NodeId` are join handles valid only within this build (a `NodeId` is the
//!   ordinal of first interning, so inserting one instance shifts every later
//!   one); the canonical key is the only thing that survives a rebuild (§2,
//!   §2.4, §3.7 discipline 2/3). Every item therefore carries **both**.

pub mod p2;
pub mod vec;
pub mod viz;
pub mod world_ver;

use serde_json::{json, Value};

use crate::instant::insttab::InstTable;
use crate::semantic::common::SourcePos;

/// Which segment of the chain a view reads. `p1` is a placeholder: the command
/// surface is fixed now so that the Pass1 view (design §8 O2/O3) can land
/// without reshaping the command family (§5.3 ① ⚠).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageSeg {
    P1,
    P2,
    Vec,
    Viz,
}

impl StageSeg {
    /// Parse the `name` argument of `mcc show stage <seg>`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "p1" => Some(StageSeg::P1),
            "p2" => Some(StageSeg::P2),
            "vec" => Some(StageSeg::Vec),
            "viz" => Some(StageSeg::Viz),
            _ => None,
        }
    }

    /// The `view` value this segment publishes, listed beside the six existing
    /// projections (`project-model` / `netlist` / …). See design §3: `stage.*`
    /// are pipeline stages, not read-side projections of one frozen world, so
    /// they must not impersonate an existing view name.
    pub fn view_name(self) -> &'static str {
        match self {
            StageSeg::P1 => "stage.p1",
            StageSeg::P2 => "stage.p2",
            StageSeg::Vec => "stage.vec",
            StageSeg::Viz => "stage.viz",
        }
    }

    /// The three count words of the text face's second line, in print order.
    /// Fixed per segment so two runs cannot differ by which words appear.
    pub fn count_words(self) -> &'static [&'static str] {
        match self {
            StageSeg::P1 => &["defs", "stmts", "diagnostics"],
            StageSeg::P2 => &["instances", "nets", "points", "diagnostics"],
            StageSeg::Vec => &["layers", "boxes", "nets", "endpoints", "diagnostics"],
            StageSeg::Viz => &["layers", "boxes", "pins", "segments", "diagnostics"],
        }
    }
}

/// The projection envelope, field names per
/// `mcd/doc/world/projection-schema-design.md` §1.
///
/// This is deliberately a *minimal* envelope and not a new schema family: it
/// rides the existing [`crate::output::Envelope`] channel as one more sibling
/// key of [`crate::output::CommandResult`], which is what law B asks for — a new
/// `view` value on an existing envelope, not a second envelope format.
#[derive(Debug, Clone)]
pub struct StageView {
    /// Envelope schema version, spelled as the schema doc spells it.
    pub schema_version: &'static str,
    /// Root token of this projection: the loaded world's source set as a
    /// deterministic hash. `None` when the world cannot be fingerprinted —
    /// see [`world_ver::world_ver`].
    pub world_ver: Option<String>,
    /// `mcc` version that produced this view.
    pub mcc_version: String,
    /// `stage.p1` | `stage.p2` | `stage.vec` | `stage.viz`.
    pub view: &'static str,
    /// What this view is scoped to.
    pub top: String,
    /// The items, sorted by `(class, key)`.
    pub items: Vec<Value>,
    /// Per-class item counts plus the diagnostic base.
    pub counts: Value,
}

impl StageView {
    /// Assemble a view. `items` is sorted here so no caller can forget to, and
    /// the counts are computed from the *sorted* items so the header and the
    /// rows cannot disagree.
    pub fn new(seg: StageSeg, top: &str, mut items: Vec<Value>, diagnostics: usize) -> Self {
        sort_items(&mut items);
        let counts = counts(seg, &items, diagnostics);
        Self {
            schema_version: "proj.1.0",
            world_ver: world_ver::world_ver(),
            mcc_version: format!("{}.{}", crate::buildinfo::VERSION, crate::buildinfo::BUILD),
            view: seg.view_name(),
            top: top.to_string(),
            items,
            counts,
        }
    }

    /// The text face's first line: the envelope header. One line, fixed field
    /// order, `world_ver` printed in full (it is a token meant to be compared,
    /// so truncating it would invite two different worlds to look alike).
    pub fn header_line(&self) -> String {
        let wv = self.world_ver.as_deref().unwrap_or("-");
        format!(
            "# {}  top={}  world_ver={}  mcc {}",
            self.view, self.top, wv, self.mcc_version
        )
    }

    /// The text face's second line: the count words of this segment, in the
    /// order [`StageSeg::count_words`] fixes. Zero counts are printed, not
    /// omitted — an absent line would read as "not implemented".
    pub fn counts_line(&self, seg: StageSeg) -> String {
        let words: Vec<String> = seg
            .count_words()
            .iter()
            .map(|w| format!("{w} {}", self.counts[*w].as_u64().unwrap_or(0)))
            .collect();
        format!("# {}", words.join("  "))
    }
}

/// Per-class item counts plus the diagnostic base. Every word is printed even
/// when zero (design §5.3: a missing value is `-`, an absent category is `0`) —
/// an absent line reads as "not implemented" rather than "none".
fn counts(seg: StageSeg, items: &[Value], diagnostics: usize) -> Value {
    let n_of = |class: &str| items.iter().filter(|i| i["class"] == class).count();
    let mut map = serde_json::Map::new();
    match seg {
        StageSeg::P1 => {
            map.insert("defs".into(), json!(n_of("def")));
            map.insert("stmts".into(), json!(n_of("stmt")));
        }
        StageSeg::P2 => {
            map.insert("instances".into(), json!(n_of("instance")));
            map.insert("nets".into(), json!(n_of("net")));
            map.insert("points".into(), json!(n_of("point")));
        }
        StageSeg::Vec => {
            map.insert("layers".into(), json!(n_of("layer")));
            map.insert("boxes".into(), json!(n_of("box")));
            map.insert("nets".into(), json!(n_of("net")));
            map.insert("endpoints".into(), json!(n_of("endpoint")));
        }
        StageSeg::Viz => {
            map.insert("layers".into(), json!(n_of("layer")));
            map.insert("boxes".into(), json!(n_of("box")));
            map.insert("pins".into(), json!(n_of("pin")));
            map.insert("segments".into(), json!(n_of("segment")));
        }
    }
    map.insert("diagnostics".into(), json!(diagnostics));
    Value::Object(map)
}

/// Sort items by `(class, key)`, where `key` is the item's own `key` string
/// (or the canonical path when a class has no key, per §2.4).
///
/// Sorting *here*, once, at assembly, is the whole point: the design moved this
/// discipline from "the comparator sorts before comparing" to "the artifact is
/// already sorted" (§3), so every consumer gets it for free.
fn sort_items(items: &mut [Value]) {
    items.sort_by(|a, b| {
        let ka = (a["class"].as_str().unwrap_or(""), sort_key(a));
        let kb = (b["class"].as_str().unwrap_or(""), sort_key(b));
        ka.cmp(&kb)
    });
}

/// A sortable string for one item: the **canonical** key first, then the
/// handles that may disambiguate it.
///
/// Why canonical and not the run-local `key`: the ruling (design §7 O15) fixes
/// the sort key as the canonical key, and the reason is that the *sequence*
/// has to survive a rebuild as well as the keys do. A `NodeId` is the ordinal
/// of first interning, so ordering by it means inserting one instance reshuffles
/// the whole artifact — the diff a reader sees would be pure noise. Ordering by
/// canonical path keeps the sequence stable and, as a side effect, prints a
/// layer's rows in the order a human reads them.
///
/// The trailing components only break ties (one pin appears once per net), so a
/// tie is settled by data rather than by which item happened to be pushed
/// first; the sort is stable regardless.
fn sort_key(item: &Value) -> String {
    let s = item["canon_key"]["path"]
        .as_str()
        .or_else(|| item["path"].as_str())
        .or_else(|| item["key"].as_str())
        .unwrap_or("");
    format!(
        "{s}\u{1}{}\u{1}{}\u{1}{}",
        item["key"].as_str().unwrap_or(""),
        item["path"].as_str().unwrap_or(""),
        item["net"].as_str().unwrap_or("")
    )
}

/// Render one item's `loc` field: the source site a reader can jump back to.
///
/// Shape is `{uri, line, span}` (design §3). `line` is 1-based and derived from
/// the entry's byte offset; `span` stays `null` for the flat table, whose
/// positions carry an offset and no extent — filling it with a made-up end
/// would be inventing data.
pub fn loc_value(
    uri: Option<&str>,
    offset: Option<u32>,
    content: Option<&str>,
) -> Value {
    let Some(uri) = uri else {
        return Value::Null;
    };
    let line = match (content, offset) {
        (Some(c), Some(o)) => u64::from(crate::hierarchy::line_of_byte(c, o as usize)),
        _ => 0,
    };
    json!({
        "uri": uri,
        "line": line,
        "span": Value::Null,
    })
}

/// The `loc` column text: `uri:line`, or `-` when unknown (design §5.3: a
/// missing value prints as `-`, never as an empty cell).
pub fn loc_cell(loc: &Value) -> String {
    let Some(uri) = loc["uri"].as_str() else {
        return "-".to_string();
    };
    let line = loc["line"].as_u64().unwrap_or(0);
    if line == 0 {
        return format!("{uri}:-");
    }
    format!("{uri}:{line}")
}

/// Render rows as a fixed-width table: double-space separated columns, each
/// widened to its own longest cell.
///
/// Design §5.3 forbids tabs, ANSI colour and box drawing in the text face, and
/// requires the layout be *computed from the data* rather than padded by hand.
///
/// **The grammar is "columns are separated by two or more spaces", and that is
/// only a grammar if no cell contains two in a row.** Cell content comes from
/// the source — `main.MCU513.[VCC_1V2, GND]` is a real bus-member path — so the
/// guarantee is manufactured by [`cell`] rather than assumed here. A single
/// space inside a cell is fine and stays; it is the run that would be mistaken
/// for a separator.
pub fn render_table(rows: &[Vec<String>], out: &mut Vec<String>) {
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| r.iter().map(|c| cell(c)).collect())
        .collect();
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut width = vec![0usize; cols];
    for row in &rows {
        for (i, c) in row.iter().enumerate() {
            width[i] = width[i].max(c.chars().count());
        }
    }
    for row in &rows {
        let mut line = String::new();
        for (i, c) in row.iter().enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            line.push_str(c);
            // No padding after the final column: trailing spaces are invisible
            // but they are bytes, and the artifact is compared byte for byte.
            if i + 1 < row.len() {
                for _ in c.chars().count()..width[i] {
                    line.push(' ');
                }
            }
        }
        out.push(line);
    }
}

/// One cell, with every whitespace run collapsed to a single space and the ends
/// trimmed — so a row can always be split on a run of two or more spaces, and a
/// reader splitting on whitespace gets the same columns.
///
/// The cell is a key, a path, a class name or `uri:line`: an identifier, never
/// prose, so collapsing costs no information. Newlines are collapsed too, which
/// is what stops a cell from silently turning one item into two rows.
fn cell(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The def half of an object's canonical key, spelled `{uri, ident}`.
///
/// A component / module row carries its own `class_def`; a pin, port or net
/// row has none, so it is recovered from the nearest ancestor that declares
/// one. Walking `parent_id` is a read of a value the build already computed,
/// not a second identity system — `parent_id` strictly decreases towards the
/// root, so the walk terminates.
pub fn def_of(table: &InstTable, id: u32) -> Option<Value> {
    let mut cur = Some(id);
    while let Some(cid) = cur {
        let e = table.get_entry(cid)?;
        if let Some(sn) = &e.class_def {
            return Some(json!({
                "uri": sn.uri.as_uri().to_string(),
                "ident": sn.ident.to_string(),
            }));
        }
        cur = e.parent_id;
    }
    None
}

/// The canonical key of an instance, from its `InstTable` row: the canonical
/// path plus the def it instantiates.
///
/// This is the one form that survives a rebuild, so every class that names an
/// instance — a Pass2 row, a vec box — spells it through here and the two views
/// cannot drift apart.
pub fn canon_instance(table: &InstTable, id: u32) -> Value {
    let path = table
        .get_entry(id)
        .map(|e| e.path.clone())
        .unwrap_or_default();
    json!({ "path": path, "def": def_of(table, id) })
}

/// One `loc` value from a source position, with the line number resolved from
/// the source text. `null` when there is no position — a readout prints `-`
/// rather than a plausible-looking wrong line.
pub fn loc_of(pos: Option<&SourcePos>, sources: &mut SourceText) -> Value {
    let Some(p) = pos else {
        return Value::Null;
    };
    let text = sources.text(&p.uri).map(|t| t.to_string());
    loc_value(Some(p.uri.as_str()), Some(p.offset), text.as_deref())
}

/// Source text per URI, read at most once.
///
/// Needed to turn a byte offset into the line number a text face prints. An
/// unreadable file yields no text, and the line renders as unknown rather than
/// as a wrong number — the same choice [`world_ver`] makes, for the same
/// reason.
#[derive(Default)]
pub struct SourceText {
    cache: std::collections::HashMap<String, Option<String>>,
}

impl SourceText {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&mut self, uri: &str) -> Option<&str> {
        if !self.cache.contains_key(uri) {
            // In-memory content first (a source loaded from a string was parsed
            // from exactly this text); a project loaded from disk leaves it
            // empty, so the filesystem read is the normal path here too.
            let from_workspace = crate::db::cmie::tables::WORKSPACE
                .mcodes
                .get(uri)
                .map(|c| c.content.clone())
                .filter(|c| !c.is_empty());
            let text = from_workspace.or_else(|| std::fs::read_to_string(uri).ok());
            self.cache.insert(uri.to_string(), text);
        }
        self.cache.get(uri).and_then(|t| t.as_deref())
    }
}
