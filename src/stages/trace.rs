// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `trace` — one object, followed along the whole chain.
//!
//! Design: `mcd/doc/pipeline/stage-readout-design.md` §5.3 ③. `join` walks a
//! whole hop and classifies it; `trace` takes one key and answers "what is this
//! thing at every stage, and where did it come from?". §5.3 source-side item 4
//! fixes the backwards direction as this command's own: `join src p2` and
//! `join p2 src` are not the same hop, and rather than growing the command
//! surface into an N×N matrix the reverse walk is `trace`.
//!
//! ```text
//! stmt(loc)  --join-->  stage.p2  --join-->  stage.vec  --join-->  stage.viz
//! ```
//!
//! ## Four key forms, told apart by content
//!
//! §5.3 ③ fixes the set — an in-domain handle, an instance's canonical path, a
//! def's canonical key, and a source position — and requires the form be read
//! off the key itself rather than from a `--kind` flag: the user holds a key and
//! should not have to know which of the four it is. Nothing here invents a fifth
//! spelling ([`parse_key`]).
//!
//! ## The walk is over the hop readouts, not over a second derivation
//!
//! Each hop is read through [`crate::stages::join`]'s own builders, which hand
//! back both the hop's classification and the two segments' views it was built
//! from. So the class a `trace` row prints is the very class `mcc join` prints
//! for that object, and the key it prints is the key `mcc show stage <seg>`
//! prints for it — three commands, one build, no third implementation to drift
//! (§5.3 ruling ③).
//!
//! ## Two things this command does not do
//!
//! - **It does not fall back to a name.** A key that resolves to no object is
//!   reported, not searched for by the name it resembles (§5.2 hard constraint 2).
//! - **It does not synthesise a key for an object that owns none.** A def is a
//!   template and not an object on the chain, so its row says so and lists what
//!   instantiates it rather than minting a handle it does not have (§2.4).

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::join::HopSides;
use super::{loc_value, render_table, SourceText, StageView};

/// The `view` value of this readout.
pub const TRACE_VIEW: &str = "trace";

/// The stages a row may name, in chain order. Print order is the chain's own:
/// the point of the readout is to read it top to bottom as `src -> viz`.
pub const STAGES: &[&str] = &["src", "ast", "p2", "vec", "viz"];

/// The four key forms of §5.3 ③.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyForm {
    /// An in-domain handle: `N<node>:<member>` (a point) or `N<node>` (its node).
    Domain,
    /// An instance's canonical path: `top.u1.vin`.
    Canon,
    /// A def's canonical key: `<uri>::<ident>`.
    Def,
    /// A source position: `<uri>:<line>[:<col>]`.
    Loc,
}

impl KeyForm {
    /// The word the header and the items spell this form with.
    pub fn as_str(self) -> &'static str {
        match self {
            KeyForm::Domain => "domain",
            KeyForm::Canon => "canon",
            KeyForm::Def => "def",
            KeyForm::Loc => "loc",
        }
    }
}

/// Read the key's form off its content (§5.3 ③).
///
/// The tests are ordered because the forms overlap in their characters:
/// `lib/power.mc::LDO` is dotted like a path, and `mcu.mc:23` contains a colon
/// like a point handle. `::` can only be a def; a leading `N` with digits can
/// only be in-domain; a trailing colon-number can only be a position. Each test
/// is a fact about the string, and what is left is the canonical path — which is
/// exactly the form that has no distinguishing punctuation of its own.
pub fn parse_key(key: &str) -> Option<KeyForm> {
    if key.is_empty() {
        return None;
    }
    if key.contains("::") {
        return Some(KeyForm::Def);
    }
    if let Some(rest) = key.strip_prefix('N') {
        let domain = match rest.split_once(':') {
            Some((n, m)) => is_digits(n) && is_digits(m),
            None => is_digits(rest),
        };
        if domain {
            return Some(KeyForm::Domain);
        }
    }
    if let Some((head, tail)) = key.rsplit_once(':') {
        if !head.is_empty() && is_digits(tail) {
            return Some(KeyForm::Loc);
        }
    }
    Some(KeyForm::Canon)
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// An object of `stage.p2`, as that view holds it.
///
/// Not every object of the view holds a join key: an instance holds `D<id>`, a
/// point holds its `PointId`, but a bus or a label holds only its canonical path
/// — it is an *off-hop* row, which the view prints and no hop matches. Carrying
/// that distinction here is what lets the readout say "this object has no key"
/// instead of printing a `-` and letting the reader guess.
struct Target {
    /// The join key at `stage.p2`, when the object holds one.
    key: Option<String>,
    /// The canonical path, which every object of the view holds.
    path: String,
    /// The object's own class at `stage.p2`.
    class: String,
}

impl Target {
    /// The string the key column prints: the join key when there is one, and the
    /// canonical path otherwise — never a fabricated handle (§2.4).
    fn label(&self) -> &str {
        self.key.as_deref().unwrap_or(&self.path)
    }

    /// The hop-walk seed. Empty for an off-hop object, and an empty seed names
    /// no row, so the walk is skipped rather than matched against `""`.
    fn seed(&self) -> &str {
        self.key.as_deref().unwrap_or("")
    }
}

/// Where the walk starts. Each variant yields the hop-0 rows to walk from and
/// the key the `p2` row prints.
enum Start {
    /// A key that names one object of `stage.p2`.
    Object(Target),
    /// A source position: the statement itself, whose hop-0 row is the clause.
    Clause { index: usize },
    /// A def: not an object on the chain, but the objects instantiating it.
    Def { keys: Vec<String> },
}

/// Follow `key` along the chain, over a build the caller already made.
///
/// The hop views are taken as they are: each carries its own two sides, so the
/// four segments' items are the ones the rest of the readout family publishes,
/// and no segment is built twice.
pub fn build_trace(
    key: &str,
    src_p2: &StageView,
    p2_vec: &HopSides,
    vec_viz: &HopSides,
    top: &str,
) -> Result<StageView, String> {
    let Some(form) = parse_key(key) else {
        return Err(unrecognised(key));
    };
    let clauses = crate::stages::join::clause_spans();
    let mut sources = SourceText::new();

    let start = match form {
        KeyForm::Domain | KeyForm::Canon => Start::Object(resolve_keyed(key, &p2_vec.left)?),
        KeyForm::Loc => Start::Clause {
            index: resolve_position(key, &clauses, &mut sources)?,
        },
        KeyForm::Def => Start::Def {
            keys: resolve_def(key, &p2_vec.left)?,
        },
    };

    let rows = match &start {
        Start::Object(t) => object_rows(t, src_p2, p2_vec, vec_viz, &clauses, &mut sources),
        Start::Clause { index } => clause_rows(
            &clauses[*index],
            *index,
            src_p2,
            p2_vec,
            vec_viz,
            &mut sources,
        ),
        Start::Def { keys } => def_rows(keys),
    };
    // `stages` is the length of the chain and is the same on every run; it is
    // printed so that a row going missing is visible. `reached` is how many of
    // those stages named the key at all — a walk that stopped at `p2` is a
    // reading, and the two numbers are what tell it apart from a walk that was
    // not performed.
    let reached = rows.iter().filter(|r| !r["key"].is_null()).count();
    let counts = json!({
        "form": form.as_str(),
        "stages": rows.len(),
        "reached": reached,
    });
    Ok(StageView::with_view(TRACE_VIEW, top, rows, counts))
}

// ── The five rows of a keyed object ──

/// `src` / `ast` / `p2` / `vec` / `viz` for an object the `stage.p2` view holds.
fn object_rows(
    t: &Target,
    src_p2: &StageView,
    p2_vec: &HopSides,
    vec_viz: &HopSides,
    clauses: &[(String, usize, usize, String)],
    sources: &mut SourceText,
) -> Vec<Value> {
    let mut rows = Vec::with_capacity(STAGES.len());
    let key = t.label();
    let seed = t.seed();
    let hop0 = rows_naming(&src_p2.items, seed);

    // The statement that wrote it. Three ways for there to be none, and each
    // gets its own sentence: an object with no join key is off the hop
    // altogether, an object no hop row names has no statement by construction,
    // and a merged net has several candidate statements but no single one. The
    // head of the chain is a statement, so this row never picks one of several.
    let owner = if seed.is_empty() || hop0.is_empty() {
        None
    } else {
        owning_clause(src_p2, clauses, seed, sources)
    };
    match owner {
        Some(i) => {
            rows.push(clause_row(&clauses[i], i, sources, "src"));
            rows.push(clause_row(&clauses[i], i, sources, "ast"));
        }
        None => {
            let why = if seed.is_empty() {
                "this object holds no join key at stage.p2, so no statement can be tied to it"
            } else if hop0.is_empty() {
                "no statement owns this object: no row of the src->p2 hop names it"
            } else {
                "no single statement wrote this object"
            };
            rows.push(missing("src", why));
            rows.push(missing("ast", why));
        }
    }

    // The three circuit-side segments, each read off the hop that produced it.
    // The `p2` row's class is the object's own class when nothing hop-wise named
    // it, which is what distinguishes an off-hop row from a lost one.
    //
    // The `p2` row carries no detail, and that is deliberate: the three ways in
    // (§5.3 ③ — a source position, a handle, a canonical path) have to print the
    // same walk, and only a walk entered *at* the object has a single object to
    // describe — entered at a statement, the row stands for as many objects as
    // the statement wrote, which the key column already lists. The design's own
    // face sample draws the row this way too. What the object *is* is in the
    // class column and the key column; where it came from is the `src` row.
    rows.push(circuit_row(
        "p2",
        key,
        if hop0.is_empty() {
            t.class.clone()
        } else {
            hop_class(&hop0)
        },
        String::new(),
        Value::Null,
        &note(hop0.len(), 0, seed.is_empty()),
    ));

    let hop1 = rows_naming(&p2_vec.join.items, seed);
    let mid = downstream(&hop1, true);
    rows.push(circuit_row(
        "vec",
        &mid.join(" "),
        hop_class(&hop1),
        detail_of(&p2_vec.right, &mid.join(" ")),
        Value::Null,
        &note(hop1.len(), 1, seed.is_empty()),
    ));

    let hop2: Vec<&Value> = vec_viz
        .join
        .items
        .iter()
        .filter(|r| mid.iter().any(|m| names(r, m)))
        .collect();
    let last = downstream(&hop2, true);
    rows.push(circuit_row(
        "viz",
        &last.join(" "),
        hop_class(&hop2),
        detail_of(&vec_viz.right, &last.join(" ")),
        Value::Null,
        &note(hop2.len(), 2, seed.is_empty()),
    ));
    rows
}

/// The same five rows for a source position: the statement *is* the object, so
/// `src` / `ast` describe it and the three circuit rows are what it wrote.
///
/// This is the reading that makes the form useful — the question a position
/// answers is "this line, what did it become?". A `drop` or a `skip` therefore
/// shows up as an empty downstream with its class still printed, the same shape
/// `mcc join src p2` gives it.
fn clause_rows(
    clause: &(String, usize, usize, String),
    index: usize,
    src_p2: &StageView,
    p2_vec: &HopSides,
    vec_viz: &HopSides,
    sources: &mut SourceText,
) -> Vec<Value> {
    let src = clause_row(clause, index, sources, "src");
    let key = src["key"].as_str().unwrap_or_default().to_string();
    let hop0: Vec<&Value> = src_p2
        .items
        .iter()
        .filter(|i| i["key"].as_str() == Some(&key))
        .collect();
    let start = downstream(&hop0, false);

    let hop1: Vec<&Value> = p2_vec
        .join
        .items
        .iter()
        .filter(|r| start.iter().any(|s| names(r, s)))
        .collect();
    let mid = downstream(&hop1, true);
    let hop2: Vec<&Value> = vec_viz
        .join
        .items
        .iter()
        .filter(|r| mid.iter().any(|m| names(r, m)))
        .collect();
    let last = downstream(&hop2, true);

    vec![
        src,
        clause_row(clause, index, sources, "ast"),
        circuit_row(
            "p2",
            &start.join(" "),
            hop_class(&hop0),
            String::new(),
            Value::Null,
            &note(hop0.len(), 0, false),
        ),
        circuit_row(
            "vec",
            &mid.join(" "),
            hop_class(&hop1),
            detail_of(&p2_vec.right, &mid.join(" ")),
            Value::Null,
            &note(hop1.len(), 1, false),
        ),
        circuit_row(
            "viz",
            &last.join(" "),
            hop_class(&hop2),
            detail_of(&vec_viz.right, &last.join(" ")),
            Value::Null,
            &note(hop2.len(), 2, false),
        ),
    ]
}

/// A def's row set: the template is not on the chain, so `src` and `ast` say
/// why, `p2` lists what instantiates it, and the two drawing stages are empty
/// because a def is not drawn — its instances are.
///
/// The alternative reading — expand each instance into a full five-row walk —
/// is a *different command's* output (`join` over a def-shaped selection), and
/// printing it here would make one row of the table stand for many objects. The
/// `p2` row names them, so the follow-up is one copy-paste away.
fn def_rows(keys: &[String]) -> Vec<Value> {
    vec![
        missing("src", "a def is a definition, not a statement"),
        missing("ast", "a def is a definition, not a statement"),
        circuit_row(
            "p2",
            &keys.join(" "),
            "expand".to_string(),
            String::new(),
            Value::Null,
            &format!("{} objects instantiating this def", keys.len()),
        ),
        missing(
            "vec",
            "a def is a template; its instances are the drawn objects",
        ),
        missing(
            "viz",
            "a def is a template; its instances are the drawn objects",
        ),
    ]
}

// ── Resolution ──

/// Resolve an in-domain or canonical-path key to the object `stage.p2` holds.
///
/// The two in-domain spellings are not the same lookup. `N<node>:<member>` is a
/// point's own key and is found by it. A bare `N<node>` is the identity
/// registry's node number, while the flat table's `D<id>` is its own row number
/// — on the fixture they differ (`D54` and `N10` are both `main.DCDC._C1`), so
/// the node is resolved through the points that carry it: the owner of
/// `N<node>:*` is the instance the caller means. That is a read of the build's
/// own data, not a second index (O13).
fn resolve_keyed(key: &str, left: &StageView) -> Result<Target, String> {
    if let Some(i) = left.items.iter().find(|i| i["key"].as_str() == Some(key)) {
        return Ok(target_of(i));
    }
    if let Some(i) = left
        .items
        .iter()
        .find(|i| i["canon_key"]["path"].as_str() == Some(key))
    {
        return Ok(target_of(i));
    }
    if let Some(node) = key.strip_prefix('N').filter(|n| is_digits(n)) {
        let prefix = format!("N{node}:");
        let owner = left
            .items
            .iter()
            .filter(|i| i["class"] == "point")
            .find(|i| i["key"].as_str().is_some_and(|k| k.starts_with(&prefix)))
            .and_then(|i| i["path"].as_str())
            .and_then(|p| p.rsplit_once('.').map(|(owner, _)| owner.to_string()));
        if let Some(owner) = owner {
            if let Some(i) = left
                .items
                .iter()
                .find(|i| i["class"] == "instance" && i["path"].as_str() == Some(&owner))
            {
                return Ok(target_of(i));
            }
        }
    }
    Err(format!(
        "no object in this build holds the key '{key}'\n\
         hint: the key names an object of stage.p2, so a file name or a class name will not do"
    ))
}

/// One item of `stage.p2`, read as the walk's target.
fn target_of(item: &Value) -> Target {
    Target {
        key: item["key"].as_str().map(str::to_string),
        path: item["canon_key"]["path"]
            .as_str()
            .or_else(|| item["path"].as_str())
            .unwrap_or_default()
            .to_string(),
        class: item["class"].as_str().unwrap_or("-").to_string(),
    }
}

/// Resolve `file:line[:col]` to the clause a reader is pointing at.
///
/// The line becomes a byte offset and the test is the clause's own half-open
/// span, `start <= at < end`. Comparing *lines* instead would be off by one at
/// every clause end: a clause's `end` offset is the byte after its last one,
/// which the newline before the next statement puts on the next line — so
/// asking for line 22 would hand back the clause that ends there, which starts
/// on line 21. A reader points at the line they can see, which for a statement
/// that wraps is inside the clause, and the clause list is sorted, so the first
/// match is a function of the input (build-design §3.7 discipline 0).
fn resolve_position(
    key: &str,
    clauses: &[(String, usize, usize, String)],
    sources: &mut SourceText,
) -> Result<usize, String> {
    let (uri, rest) = key
        .split_once(':')
        .ok_or_else(|| format!("'{key}' is not a source position"))?;
    let line: u32 = rest
        .split(':')
        .next()
        .unwrap_or(rest)
        .parse()
        .map_err(|_| format!("'{key}' is not a source position"))?;
    clauses
        .iter()
        .position(|(u, start, end, _)| u == uri && contains(sources, u, *start, *end, line))
        .or_else(|| {
            clauses.iter().position(|(u, start, end, _)| {
                same_file(u, uri) && contains(sources, u, *start, *end, line)
            })
        })
        .ok_or_else(|| {
            format!(
                "no statement spans {uri}:{line}\n\
                 hint: the position must fall inside a statement of the loaded source set"
            )
        })
}

/// Whether line `line` of `uri` falls inside the span, in bytes. See
/// [`byte_of_line`] for the conversion.
fn contains(sources: &mut SourceText, uri: &str, start: usize, end: usize, line: u32) -> bool {
    let text = sources.text(uri).unwrap_or_default().to_string();
    let at = byte_of_line(&text, line);
    start <= at && at < end
}

/// Whether two spellings name the same file.
///
/// A key's `uri` rarely matches a clause's byte for byte: the clause list
/// carries the U274 display form (`src/hbl.mc`), while the reader types the
/// path they passed to `-F` (`/private/tmp/…/twice.mc`) or the one the
/// design's sample uses (`lib/power.mc`). Each spelling is resolved the way
/// the loader resolves a `use` path — absolute as-is, relative against the
/// project root, then the system root ([`crate::viz::srcuri::resolve`]) —
/// before the comparison. It is on the **path**, never on the file's name:
/// `a/power.mc` and `b/power.mc` are two files and stay two — a name-only
/// match is the fallback §5.2 hard constraint 2 forbids.
fn same_file(a: &str, b: &str) -> bool {
    let project = crate::db::infra::init::mcb_get_project_root();
    let res = |s: &str| crate::viz::srcuri::resolve(s, &project);
    match (res(a).canonicalize(), res(b).canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The byte offset of the first byte of a 1-based line: the offset just past the
/// `line - 1`th newline. Past the end of the text it is the text's length, which
/// is inside no clause — the same answer an unknown line should get.
fn byte_of_line(text: &str, line: u32) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut seen = 1u32;
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            seen += 1;
            if seen == line {
                return i + 1;
            }
        }
    }
    text.len()
}

/// Resolve a def's canonical key to the objects that instantiate it.
///
/// The def half of the key is `(uri, ident)` — a *file* and a name (§2.4) — so
/// the uri is matched by [`same_file`], not by its spelling: the design's own
/// sample writes `lib/power.mc::LDO`, which is how the file is spelled in the
/// project while the loaded def carries the resolved path.
fn resolve_def(key: &str, left: &StageView) -> Result<Vec<String>, String> {
    let (uri, ident) = key
        .split_once("::")
        .ok_or_else(|| format!("'{key}' is not a def key"))?;
    let def_of = |i: &Value| -> Option<String> {
        let d = &i["canon_key"]["def"];
        let d_uri = d["uri"].as_str()?;
        if d["ident"].as_str() != Some(ident) || !(d_uri == uri || same_file(d_uri, uri)) {
            return None;
        }
        i["key"].as_str().map(str::to_string)
    };
    let keys: Vec<String> = left.items.iter().filter_map(def_of).collect();
    if keys.is_empty() {
        return Err(format!(
            "no object in this build instantiates '{key}'\n\
             hint: a def key names a definition, so a build that does not use it has nothing to show"
        ));
    }
    Ok(keys)
}

/// The clause that wrote an object: the hop-0 row that lists it as its
/// downstream. A merged net has no single owning statement, so this is `None`
/// for it — and the row says so rather than picking one of the several.
fn owning_clause(
    src_p2: &StageView,
    clauses: &[(String, usize, usize, String)],
    key: &str,
    sources: &mut SourceText,
) -> Option<usize> {
    let clause = src_p2
        .items
        .iter()
        .find(|i| i["text"].is_string() && holds(i, "to", key))?;
    let k = clause["key"].as_str()?;
    // The hop's statement key is the U274 display form; the clause list carries
    // the path the loader resolved. The file is matched as a file (the same
    // `use`-path resolution a reader's spelling gets), the text is read under
    // the clause's own spelling.
    let (uri, line) = k.rsplit_once(':')?;
    let line: u32 = line.parse().ok()?;
    clauses.iter().position(|(u, start, _, _)| {
        same_file(u, uri) && {
            let text = sources.text(u).unwrap_or_default().to_string();
            crate::hierarchy::line_of_byte(&text, *start) == line
        }
    })
}

// ── Rows ──

/// An item for a stage the object does not reach, with the structural reason.
/// §5.3's rule for a missing value is `-`, and a reason is what keeps `-` from
/// reading as "not implemented".
fn missing(stage: &str, why: &str) -> Value {
    json!({
        "stage": stage,
        "form": "-",
        "key": Value::Null,
        "loc": Value::Null,
        "class": "-",
        "detail": Value::Null,
        "why": why,
    })
}

fn unrecognised(key: &str) -> String {
    format!(
        "'{key}' is not one of the four key forms\n\
         expected one of:\n  \
         N12:3               an in-domain point handle\n  \
         top.u1.vin          an instance's canonical path\n  \
         lib/power.mc::LDO   a def's canonical key\n  \
         mcu.mc:23           a source position"
    )
}

/// A `src` / `ast` row: the clause as the chain's head.
///
/// `phrase#<n>` is the clause's ordinal in the sorted in-scope clause list — a
/// derived ordinal, recomputed every build, never persisted and never promoted
/// to an identity (O13). It is a position in *this* build, so it is no more
/// comparable across builds than a `NodeId` is.
fn clause_row(
    clause: &(String, usize, usize, String),
    index: usize,
    sources: &mut SourceText,
    stage: &str,
) -> Value {
    let (uri, start, end, text) = clause;
    let body = sources.text(uri).unwrap_or_default().to_string();
    let loc = loc_value(Some(uri), Some(*start as u32), Some(&body));
    let line = loc["line"].as_u64().unwrap_or(0);
    let (form, key, detail) = match stage {
        "src" => (
            "loc",
            // The key spelling is the U274 display form — the same one the
            // hop's statement keys stamp — so the two sides compare equal.
            format!("{}:{line}", crate::viz::srcuri::display(uri)),
            format!("\"{}\"", text.trim()),
        ),
        _ => (
            "phrase",
            format!("phrase#{}", index + 1),
            format!(
                "span {uri}:{a}..{b}",
                a = crate::hierarchy::line_of_byte(&body, *start),
                b = crate::hierarchy::line_of_byte(&body, *end),
            ),
        ),
    };
    json!({
        "stage": stage,
        "form": form,
        "key": key,
        "loc": loc,
        "class": "-",
        "detail": detail,
        "why": "",
    })
}

/// A circuit-side row: the object as that segment sees it.
///
/// `key` may be several keys, space-separated, because one row stands for one
/// *object of the walk*, which at a hop whose cardinality is not (1,1) is more
/// than one object of the segment. The count note in `why` says how many hop
/// rows produced it, so a cardinality is never read off the number of keys.
fn circuit_row(
    stage: &str,
    key: &str,
    class: String,
    detail: String,
    loc: Value,
    why: &str,
) -> Value {
    json!({
        "stage": stage,
        "form": if key.is_empty() { "-" } else { "key" },
        "key": if key.is_empty() { Value::Null } else { Value::String(key.to_string()) },
        "loc": loc,
        "class": class,
        "detail": if detail.is_empty() { Value::Null } else { Value::String(detail) },
        "why": why,
    })
}

/// The `why` note of a circuit row: how many hop rows named the object.
///
/// A number is what keeps a tie visible. Zero is printed, not omitted: an object
/// the hop did not name is a finding, and a blank would read as a rendering bug.
/// An off-hop object's zero is not a finding but a property, so it is named —
/// otherwise "no row matched" would read as "something was lost".
fn note(n: usize, hop: usize, offhop: bool) -> String {
    const HOPS: [&str; 3] = ["src->p2", "p2->vec", "vec->viz"];
    let at = format!("{n} hop row(s) at {}", HOPS[hop]);
    if offhop {
        format!("off-hop: no join key, so no hop can name it; {at}")
    } else {
        at
    }
}

/// The hop rows that name an object: the row is anchored on it, or it is one of
/// the handles the row matched.
///
/// A handle at a circuit hop is spelled `<stage class>:<key>` (§5.3 ②) because
/// one key can name objects of two classes; at the source hop it is the raw key.
/// Comparing by suffix with a colon is what keeps `D1` from matching `D11`.
fn names(row: &Value, key: &str) -> bool {
    row["key"].as_str() == Some(key) || holds(row, "from", key) || holds(row, "to", key)
}

fn holds(row: &Value, field: &str, key: &str) -> bool {
    row[field].as_array().is_some_and(|a| {
        a.iter().any(|h| {
            h.as_str().is_some_and(|h| {
                h == key
                    || h.strip_suffix(key)
                        .is_some_and(|prefix| prefix.ends_with(':'))
            })
        })
    })
}

fn rows_naming<'a>(items: &'a [Value], key: &str) -> Vec<&'a Value> {
    if key.is_empty() {
        return Vec::new();
    }
    items.iter().filter(|r| names(r, key)).collect()
}

/// The handles a set of hop rows matched to, as bare keys, deduplicated and
/// sorted — the next hop's seeds, and the row's own answer.
///
/// `prefixed` says whether the handles carry a class prefix. At a circuit hop
/// they do — `<stage class>:<key>`, one key naming objects of two classes is
/// exactly why (§5.3 ②) — and the class half is dropped so the next hop is
/// seeded with the key both views agree on. At the source hop they do not: the
/// handle is the row's raw key, and a point's key contains a colon of its own
/// (`N1:0`), so splitting there would turn a point into its node number.
fn downstream(rows: &[&Value], prefixed: bool) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for r in rows {
        if let Some(a) = r["to"].as_array() {
            for h in a {
                if let Some(h) = h.as_str().filter(|h| !h.is_empty()) {
                    out.insert(if prefixed { bare(h) } else { h.to_string() });
                }
            }
        }
    }
    out.into_iter().collect()
}

/// A circuit-hop handle without its class prefix: `point:N12:3` -> `N12:3`.
fn bare(handle: &str) -> String {
    match handle.split_once(':') {
        Some((_, k)) => k.to_string(),
        None => handle.to_string(),
    }
}

/// The class column of a circuit-side row: the words the hop gave the object.
///
/// One word for a single hop row, and the sorted set joined by `+` when the
/// object reached several — which is what an `expand` looks like from the
/// downstream side. Never a guess: an object the hop did not name prints `-`.
fn hop_class(rows: &[&Value]) -> String {
    let mut set: BTreeSet<&str> = BTreeSet::new();
    for r in rows {
        set.insert(r["class"].as_str().unwrap_or("-"));
    }
    if set.is_empty() {
        "-".to_string()
    } else {
        set.into_iter().collect::<Vec<_>>().join("+")
    }
}

/// The detail column of a circuit-side row: the class the object's own segment
/// gives it plus its spelling there — `endpoint u1.vin`, `box u1` — so the row
/// says *which* segment is speaking and not only which key matched.
fn detail_of(view: &StageView, key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    let Some(item) = view.items.iter().find(|i| {
        key.split_whitespace()
            .any(|k| i["key"].as_str() == Some(k) || i["path"].as_str() == Some(k))
    }) else {
        return String::new();
    };
    let stage_class = item["class"].as_str().unwrap_or("-");
    let own = item["path"]
        .as_str()
        .filter(|p| !p.is_empty())
        .or_else(|| item["name"].as_str())
        .or_else(|| item["key"].as_str())
        .unwrap_or("-");
    format!("{stage_class} {own}")
}

// ── Text face ──

/// How many keys a text-face row prints before eliding the rest. One statement
/// can write dozens of objects — a `loc` key on the fixture reaches 57 — and one
/// undivided cell of 57 keys is not a line anyone reads. The JSON face always
/// carries the whole list, and the elision is marked with its own count, so
/// nothing is silently dropped.
const KEY_LIMIT: usize = 8;

/// Render the text face from the *same* items the JSON face uses (§5.3 ruling ③).
///
/// Columns: stage, form, key, class, detail — the design's own sample (§5.3 ③).
/// There is no `loc` column because the key already carries it wherever it
/// exists: a source-side row's key **is** its source position (§5.3 source-side
/// item 2), and the circuit rows' positions are in the JSON items. The row order
/// is the chain order, so the readout is read top to bottom as `src -> viz`; the
/// source line the chain starts at is appended verbatim, because that is the one
/// line a reader would otherwise have to open an editor for.
pub fn render_trace_text(view: &StageView) -> String {
    let mut out = vec![view.header_line(), form_line(view)];
    let rows: Vec<Vec<String>> = view
        .items
        .iter()
        .map(|i| {
            vec![
                i["stage"].as_str().unwrap_or("-").to_string(),
                i["form"].as_str().unwrap_or("-").to_string(),
                key_cell(i["key"].as_str().unwrap_or("-")),
                i["class"].as_str().unwrap_or("-").to_string(),
                detail_cell(i),
            ]
        })
        .collect();
    render_table(&rows, &mut out);
    if let Some(src) = source_line(view) {
        out.push(src);
    }
    out.join("\n")
}

fn form_line(view: &StageView) -> String {
    format!(
        "# form={}  stages={}  reached={}",
        view.counts["form"].as_str().unwrap_or("-"),
        view.counts["stages"].as_u64().unwrap_or(0),
        view.counts["reached"].as_u64().unwrap_or(0)
    )
}

/// The `key` column text: the keys as they are, up to [`KEY_LIMIT`] of them.
///
/// The marker says how many were not printed rather than trailing off, because
/// the one thing a readout must never do is look complete when it is not.
fn key_cell(key: &str) -> String {
    let keys: Vec<&str> = key.split_whitespace().collect();
    if keys.len() <= KEY_LIMIT {
        return key.to_string();
    }
    format!(
        "{} +{} more",
        keys[..KEY_LIMIT].join(" "),
        keys.len() - KEY_LIMIT
    )
}

fn detail_cell(item: &Value) -> String {
    let detail = item["detail"].as_str().unwrap_or("");
    let why = item["why"].as_str().unwrap_or("");
    match (detail.is_empty(), why.is_empty()) {
        (true, true) => "-".to_string(),
        (false, true) => detail.to_string(),
        (true, false) => format!("; {why}"),
        (false, false) => format!("{detail}  ; {why}"),
    }
}

/// The chain's first line, verbatim, read back from the file rather than
/// re-rendered from the clause: the point of printing it is that it is the bytes
/// a reader would see in their editor.
fn source_line(view: &StageView) -> Option<String> {
    let src = view.items.iter().find(|i| i["stage"] == "src")?;
    let uri = src["loc"]["uri"].as_str()?;
    let line = src["loc"]["line"].as_u64()?;
    if line == 0 {
        return None;
    }
    let mut sources = SourceText::new();
    let body = sources.text(uri)?.lines().nth(line as usize - 1)?.trim();
    Some(format!("source {uri}:{line}  {body}"))
}
