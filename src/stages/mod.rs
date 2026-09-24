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

pub mod corercview;
pub mod diagview;
pub mod join;
pub mod netlistview;
pub mod p2;
pub mod projmodel;
pub mod payload;
pub mod read;
pub mod slice;
pub mod stage_diff;
pub mod top_ver;
pub mod trace;
pub mod vec;
pub mod viz;
pub mod vizdiff;
pub mod world_ver;

use serde_json::{json, Value};

use crate::instant::insttab::InstTable;
use crate::semantic::common::SourcePos;

/// The projection envelope's schema version, spelled in exactly one place.
///
/// Per `projection-schema-design.md` §1 the rule is "increment on a breaking
/// change": the version describes the shape of `items` and the keys around it. A
/// view whose item shape breaks *is* a breaking change of this envelope, which
/// is why the view model does not carry a second version number of its own.
///
/// `1.0` → `1.1` (b3557/b3558): no key was removed and no type changed, but two
/// things a cached reading cannot survive did. Every row gained `loc_all` /
/// `decl_loc` / `via`, and `loc`'s value moved for a row whose first wiring site
/// is not the site it used to report — so an old reading and a new one under the
/// same token differ in cells that did not exist before. And a statement's class
/// changed meaning (`drop`/`carry`/`expand` now count what the statement
/// *reaches*), so the same token over `join.src->p2` would compare two different
/// questions. The token's job is to make exactly those comparisons impossible.
pub const PROJ_SCHEMA_VERSION: &str = "proj.1.1";

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

/// `mcc show org-units` names its own vocabulary (a definitions-space census,
/// not a pipeline stage), and until now that name existed only as a literal at
/// its two producers. Named here so the registry below lists every published
/// `view` value from one place.
pub const ORG_UNITS_VIEW: &str = "org-units";

/// Every `view` value any read face publishes today, the whole inventory in
/// one place. The vocabulary ruling (b3907, CIMP U280) makes the six words of
/// `schema/projection.cddl`'s `view-name` the canonical read projections; a
/// face may publish one only once its serde payload group has landed — the
/// carried words so far are `diagnostics` ([`diagview`]), `netlist`
/// ([`netlistview`]), `project-model` ([`projmodel`]) and `core-erc`
/// ([`corercview`]), the other two are still v1 reservations, and
/// publishing one of those would be impersonating a projection that does not
/// exist. The lock `tests/shard7/view_vocabulary.rs` reads the canonical
/// words from the CDDL, holds every published word that equals a canonical
/// word to the carried set, and holds this list equal to what the producers
/// actually stamp — a new face adds its name here; no face invents a name
/// anywhere else.
pub fn published_views() -> Vec<&'static str> {
    vec![
        StageSeg::P1.view_name(),
        StageSeg::P2.view_name(),
        StageSeg::Vec.view_name(),
        StageSeg::Viz.view_name(),
        join::SRC_P2_VIEW,
        join::P2_VEC_VIEW,
        join::VEC_VIZ_VIEW,
        trace::TRACE_VIEW,
        ORG_UNITS_VIEW,
        diagview::DIAGNOSTICS_VIEW,
        netlistview::NETLIST_VIEW,
        projmodel::PROJECT_MODEL_VIEW,
        corercview::CORE_ERC_VIEW,
        stage_diff::DIFF_P2_VIEW,
        stage_diff::DIFF_VEC_VIEW,
        stage_diff::DIFF_VIZ_VIEW,
    ]
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
    /// The same material restricted to this view's top: "did *this* top
    /// change?" (`projection-schema-design.md` §1.1). `None` under the same
    /// conditions as [`world_ver`](Self::world_ver), and for a `top` no single
    /// project module defines — see [`top_ver::top_ver`].
    pub top_ver: Option<String>,
    /// `mcc` version that produced this view.
    pub mcc_version: String,
    /// Drawing contract versions, and only on the view that draws.
    ///
    /// These are **declared**, where the two tokens above are **derived**: a
    /// token must change when the world does, a contract version changes only
    /// when someone decides the contract changed. `stage.viz` is the sole
    /// producer of the drawing face (ruling b3477), so these three are `None`
    /// on every other view — they would be meaningless there, and printing a
    /// version for a contract a view does not honour is worse than printing
    /// nothing. Attached by [`StageView::carrying_drawing_contract`].
    pub layout_version: Option<String>,
    /// See [`layout_version`](Self::layout_version).
    pub render_version: Option<String>,
    /// See [`layout_version`](Self::layout_version).
    pub metric_schema_version: Option<String>,
    /// The alignment key table this view's items are comparable under, or
    /// `None` where the segment has no law.
    ///
    /// A difference is well defined only under **one** key table, and a saved
    /// reading has to state which one it was written under or it cannot be
    /// compared with a reading another build produced (CIMP §1 U96: the archive
    /// operand). The value is the law's, **declared** there; this field is the
    /// reading saying so, the same way the drawing contract's three versions are
    /// attached by [`StageView::carrying_drawing_contract`]. `stage.p1` and the
    /// `join.*` / `trace` vocabularies state nothing, because no law covers
    /// them — a version for a table that does not exist is worse than none.
    pub key_table: Option<String>,
    /// `stage.p1` | `stage.p2` | `stage.vec` | `stage.viz`, or one of the
    /// vocabularies assembled by [`StageView::with_view`] (`join` / `trace` /
    /// `org-units` / `diagnostics` / `netlist`), which name what they read
    /// rather than a pipeline stage.
    pub view: &'static str,
    /// What this view is scoped to.
    pub top: String,
    /// The items, in the **view's** order — never the traversal's.
    ///
    /// Which order is the view's to state: [`StageView::new`] sorts its items
    /// by `(class, key)` here, while a vocabulary assembled by
    /// [`StageView::with_view`] orders its own and documents its own comparator
    /// (`org-units` by `(kind, canonical key)`, `join` and `trace` by their
    /// rules). What this field promises is the part every view owes the law:
    /// the order is a function of the world, so two readings of one world
    /// serialize alike.
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
        let mut view = Self::assemble(seg.view_name(), top, items, counts);
        // Attached here rather than in `assemble` because only a segment has a
        // law: `join` and `trace` publish vocabularies of their own through
        // `with_view`, and their items are not comparable across builds at all.
        view.key_table = crate::stages::stage_diff::law_for(seg).map(|l| l.key_table.to_string());
        view
    }

    /// Assemble a view whose vocabulary is its own.
    ///
    /// `join`, `trace`, `org-units`, `diagnostics` and `netlist` publish their own `view`
    /// name and their own set of count words — a `join` groups by a rule the design fixes for
    /// it (the `drop` group on top), and the organization directory counts the
    /// units a definition space holds rather than the items of a pipeline
    /// stage — so none of them can go through [`StageView::new`], whose counts
    /// carry the diagnostic base. Same envelope shape either way: this is law
    /// B's "one `items`, two faces", not a second view format.
    pub fn with_view(view: &'static str, top: &str, items: Vec<Value>, counts: Value) -> Self {
        Self::assemble(view, top, items, counts)
    }

    /// The one place the envelope identity fields are filled in, so `new` and
    /// `with_view` cannot drift apart on them.
    fn assemble(view: &'static str, top: &str, items: Vec<Value>, counts: Value) -> Self {
        let (world_ver, top_ver) = revision_tokens(top);
        Self {
            schema_version: PROJ_SCHEMA_VERSION,
            world_ver,
            top_ver,
            mcc_version: format!("{}.{}", crate::buildinfo::VERSION, crate::buildinfo::BUILD),
            layout_version: None,
            render_version: None,
            metric_schema_version: None,
            key_table: None,
            view,
            top: top.to_string(),
            items,
            counts,
        }
    }

    /// Attach the drawing contract, making this view the drawing face.
    ///
    /// Not folded into [`StageView::assemble`]: three of the segments do not
    /// draw, and the assembler cannot tell whether its caller does — the caller
    /// knows. Called by `stage.viz` alone, which is the sole producer of the
    /// drawing face.
    pub fn carrying_drawing_contract(mut self) -> Self {
        self.layout_version = Some(crate::viz::layout::LAYOUT_VERSION.to_string());
        self.render_version = Some(crate::viz::render::RENDER_VERSION.to_string());
        self.metric_schema_version = Some(crate::viz::metrics::METRIC_SCHEMA_VERSION.to_string());
        self
    }

    /// The text face's first line: the envelope header. One line, fixed field
    /// order, `world_ver` printed in full (it is a token meant to be compared,
    /// so truncating it would invite two different worlds to look alike).
    pub fn header_line(&self) -> String {
        let wv = self.world_ver.as_deref().unwrap_or("-");
        let tv = self.top_ver.as_deref().unwrap_or("-");
        let mut line = format!(
            "# {}  top={}  world_ver={}  top_ver={}  mcc {}",
            self.view, self.top, wv, tv, self.mcc_version
        );
        // The drawing contract, when there is one. Every field is printed with
        // the same `-` fallback, so a view that draws and one that does not
        // stay the same shape rather than gaining a whole clause.
        if let Some(layout) = &self.layout_version {
            line.push_str(&format!(
                "  layout={}  render={}  metrics={}",
                layout,
                self.render_version.as_deref().unwrap_or("-"),
                self.metric_schema_version.as_deref().unwrap_or("-"),
            ));
        }
        line
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

/// The two revision tokens of `top`, derived from one scan of the world.
///
/// They are siblings, not neighbours: `top_ver` is `world_ver`'s material
/// restricted to `top`'s dependency closure, so when that closure covers the
/// whole loaded world the two digests are the same number and only the prefix
/// differs (`projection-schema-design.md` §1.1). Deriving them together is what
/// the schema asks for — both tokens from one pass, no second scan — and it is
/// also the only way the two can be *seen* to agree, since a second scan could
/// collect a different world.
fn revision_tokens(top: &str) -> (Option<String>, Option<String>) {
    let pairs = world_ver::source_pairs();
    match pairs.as_deref() {
        Some(pairs) => (
            Some(world_ver::root_from(pairs)),
            top_ver::top_ver_from(pairs, top),
        ),
        None => (None, None),
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
pub fn loc_value(uri: Option<&str>, offset: Option<u32>, content: Option<&str>) -> Value {
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
/// one — [`InstTable::class_def_of`], the one owner of that walk (the
/// inst-list rows and the reverse index read the same method). Walking
/// `parent_id` is a read of a value the build already computed, not a second
/// identity system.
pub fn def_of(table: &InstTable, id: u32) -> Option<Value> {
    table.class_def_of(id).map(|sn| {
        json!({
            "uri": sn.uri.as_uri().to_string(),
            "ident": sn.ident.to_string(),
        })
    })
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

/// Where a net's name came from, which is what decides whether the name may
/// stand as a key.
///
/// Three families, and only the first is the *source's*:
///
/// - `Source` — the name is written in the `.mc` (a label, a port). Two builds
///   of the same source agree on it, so it can key an item.
/// - `Segment` — the builder split one chain and minted `<base>~<k>` for the
///   pieces ([`crate::vector::builder::visit`]'s `segment_net_name`, which
///   states the name is "unique across the chain and never collides with the
///   original whole-chain name"). Unique to *this* build's segmentation, so
///   keying on it would claim a stability the name does not have.
/// - `Anonymous` — `_net<k>`, minted per build by a counter.
///
/// A `Segment` or `Anonymous` net is not left unidentifiable: its `members` list
/// is the recomputable handle §2.4 prescribes for objects that own no key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetOrigin {
    Source,
    Segment,
    Anonymous,
}

impl NetOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            NetOrigin::Source => "source",
            NetOrigin::Segment => "segment",
            NetOrigin::Anonymous => "anonymous",
        }
    }
}

/// Classify a net name. `~` cannot occur in an MCode identifier, so its presence
/// means the builder minted the name rather than the source writing it.
pub fn net_origin(name: &str) -> NetOrigin {
    if crate::instant::mc_net::is_anon_net_name(name) {
        NetOrigin::Anonymous
    } else if name.contains('~') {
        NetOrigin::Segment
    } else {
        NetOrigin::Source
    }
}

/// A net's canonical key, or `None` when the name it carries is not the
/// source's.
///
/// Spelled here for the same reason [`canon_instance`] is: two views name the
/// same net. `stage.vec` keys a `net` item with it, and `stage.viz` records it
/// on every pin it draws, so a second copy of the rule would let one net be
/// keyed two ways — and a consumer joining the two would be joining two
/// vocabularies that happen to look alike.
pub fn net_key(name: &str) -> Option<String> {
    if net_origin(name) == NetOrigin::Source {
        Some(format!("net:{name}"))
    } else {
        None
    }
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
