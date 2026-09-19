// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The rows of the organization directory — the six def kinds and the three
//! units that are not standalone defs (CIMP §1 U120, 2026-09-19).
//!
//! | unit | why it needs a row of its own | its key |
//! |---|---|---|
//! | `component` / `module` / `interface` / `enum` / `define` / `capability` | a def, listed by `all_defs` | its name, in its file |
//! | `func` | a member of its host (design §12.1) | `(host, name)` |
//! | `bus` | carries no `DefId` (T12) | its name, in its host |
//! | `clause` | no declaration object at all (§1) | its position `(uri, start)` |
//!
//! ⚠ The rows are built **here once** and consumed by every face — `list
//! func|bus|clause`, `query --kind func|bus|clause`, the `org-units`
//! projection, and its RPC method — so no two of them can come to describe one
//! unit two ways. Each row carries its own `key` string and a `loc`
//! (`{uri, line, span}`, design §3), and `name` is the field the
//! `--filter name=` / matcher paths read: the unit's own name where it has
//! one, and the **host** name for a clause, which has none.
//!
//! The directory as a whole ([`org_unit_items`]) is the one read of §8.4: the
//! def kinds come from [`crate::DefinitionSpace::all_defs`] and the three unit
//! kinds from [`unit_rows`], and the two halves are joined in one place so the
//! envelope, the text face and the RPC method cannot disagree about what is in
//! the directory or in what order.
//!
//! It holds nothing: every call re-reads the live definition space and the
//! live parse, so the compliance five conditions of design §0 hold by
//! construction — no counter, nothing persisted, nothing promoted to
//! identity, and no key that is a two-space id.

use crate::db::defspace::definition_space;
use crate::stages::{loc_value, SourceText};
use crate::DefValue;
use serde_json::{json, Map, Value};

/// Which unit a row set describes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Func,
    Bus,
    Clause,
}

impl UnitKind {
    /// The word this kind answers to on every command face.
    pub fn word(self) -> &'static str {
        match self {
            UnitKind::Func => "func",
            UnitKind::Bus => "bus",
            UnitKind::Clause => "clause",
        }
    }

    /// The kind's plural word, for a count line or a payload key.
    ///
    /// Spelled per variant rather than by appending an `s`: `bus` → `buses`,
    /// and a suffix rule would print `buss`.
    pub fn plural(self) -> &'static str {
        match self {
            UnitKind::Func => "funcs",
            UnitKind::Bus => "buses",
            UnitKind::Clause => "clauses",
        }
    }

    /// The three unit kinds, in display order. One list, so the directory's
    /// order and the count line cannot disagree about which kinds there are.
    pub const ALL: [UnitKind; 3] = [UnitKind::Func, UnitKind::Bus, UnitKind::Clause];
}

/// One directory row: the JSON every face emits, plus the name the filter and
/// matcher paths match on.
pub struct UnitRow {
    /// The name `--filter name=<pat>` and `query <pat>` match against. Not a
    /// key: a clause's is its host's name, because it has none of its own.
    pub name: String,
    /// The canonical key, `uri#<the unit's own key>`. Unique across the whole
    /// space, which is what a projection's ordering needs; the unit's own
    /// `key` field stays the spelling a user addresses it by.
    ///
    /// ⚠ For a clause this is a *spelling* of the key, not the key: the key is
    /// the position `(uri, start)` and the path writes it `uri@start`, which
    /// does **not** sort in position order as text (`@1012` < `@53`). The
    /// clause class is therefore ordered by the pair, which is the order
    /// [`crate::mcb_iter_clauses`] already returns.
    pub canon: String,
    pub json: Value,
}

/// All rows of one unit kind, in the iterator's order — that is, already
/// sorted by the unit's key, never by traversal order.
pub fn unit_rows(kind: UnitKind) -> Vec<UnitRow> {
    let mut sources = SourceText::new();
    match kind {
        UnitKind::Func => crate::mcb_iter_funcs()
            .into_iter()
            .map(|r| {
                let key = format!("{}.{}", r.host, r.func);
                UnitRow {
                    name: r.func.clone(),
                    canon: format!("{}#{key}", r.uri),
                    json: json!({
                        "key": key,
                        "kind": "func",
                        "host": r.host,
                        "host_kind": r.host_kind,
                        "func": r.func,
                        "uri": r.uri,
                        "loc": loc(&r.uri, r.offset, &mut sources),
                    }),
                }
            })
            .collect(),
        UnitKind::Bus => crate::mcb_iter_buses()
            .into_iter()
            .map(|r| {
                let key = format!("{}.{}", r.host, r.bus);
                UnitRow {
                    name: r.bus.clone(),
                    canon: format!("{}#{key}", r.uri),
                    json: json!({
                        "key": key,
                        "kind": "bus",
                        "host": r.host,
                        "host_kind": r.host_kind,
                        "bus": r.bus,
                        "members": r.members,
                        "uri": r.uri,
                        "loc": loc(&r.uri, Some(r.offset), &mut sources),
                    }),
                }
            })
            .collect(),
        UnitKind::Clause => crate::mcb_iter_clauses()
            .into_iter()
            .map(|r| {
                let key = format!("{}@{}", r.uri, r.start);
                UnitRow {
                    // A clause has no name of its own (§1), so the filter path
                    // and the matcher read the host — the only name a row holds.
                    name: r.host.clone(),
                    canon: key.clone(),
                    json: json!({
                        "key": key,
                        "kind": "clause",
                        "host": r.host,
                        "host_kind": r.host_kind,
                        "owner": r.owner,
                        "uri": r.uri,
                        "start": r.start,
                        "end": r.end,
                        "loc": loc(&r.uri, Some(r.start), &mut sources),
                    }),
                }
            })
            .collect(),
    }
}

/// The `loc` of one row: `{uri, line, span}` (design §3). `line` is 1-based,
/// derived from the row's byte offset; an offset the parse did not record
/// renders as line `0` rather than as a guessed one.
fn loc(uri: &str, offset: Option<usize>, sources: &mut SourceText) -> Value {
    let text = sources.text(uri).map(str::to_string);
    loc_value(Some(uri), offset.map(|o| o as u32), text.as_deref())
}

// ── The whole directory: the six def kinds plus the three unit kinds ──

/// Every row of the organization directory, in display order — the def kinds
/// first (in the registry's own enumeration order), then the three unit kinds,
/// each kind in its own run and each run in canonical-key order.
///
/// ★ CIMP §1 U120 (2026-09-19): this is the **one read** the directory was
/// missing. It is read-only, issues no id and holds nothing, so design §0's
/// compliance five conditions hold by construction — the only things it
/// touches are the live definition space and the live parse.
///
/// Each row carries a `class` word, its own `key`, a `canon_key` path (`uri#…`)
/// unique across the space, and a `loc`. The `key` is the spelling a user
/// addresses the row by; `canon_key` is what the ordering uses, because two
/// hosts may hold members of one name and a clause's key is its position.
pub fn org_unit_items() -> Vec<Value> {
    let mut sources = SourceText::new();
    let mut items: Vec<Value> = Vec::new();

    // The six def kinds, in the registry's own enumeration order.
    for (kind, sn, data) in definition_space().all_defs() {
        let uri = crate::uri_resolve(sn.uri).to_string();
        let name = sn.ident.to_string();
        let offset = def_span_start(&data);
        let text = sources.text(&uri).map(str::to_string);
        items.push(json!({
            "class": kind.word(),
            "key": name,
            "canon_key": { "path": format!("{uri}#{name}") },
            "name": name,
            "uri": uri,
            "funcs": host_func_names(&data),
            "loc": loc_value(Some(&uri), offset, text.as_deref()),
        }));
    }
    // Then the three units that are not standalone defs, each on its own key
    // and each in its own run: the order is (kind, canonical key), so the
    // three kinds are grouped rather than interleaved by name — `unit_rows`
    // already returns each kind in canonical-key order.
    for kind in UnitKind::ALL {
        for row in unit_rows(kind) {
            let mut item = row.json;
            item["class"] = json!(kind.word());
            item["canon_key"] = json!({ "path": row.canon });
            items.push(item);
        }
    }
    items
}

/// The count words of the directory, one per **group** — the class word made
/// plural, so the count line reads `modules 2  buses 3  clauses 4`.
///
/// The nine words are the six def kinds then the three unit kinds. There is
/// deliberately **no** `diagnostics` word: this view reads the definition
/// space, which carries no diagnostics, so a `diagnostics 0` line would read
/// as a measurement rather than as "not applicable".
pub fn org_unit_counts(items: &[Value]) -> Value {
    let count_of = |class: &str| items.iter().filter(|i| i["class"] == class).count();
    let mut counts = Map::new();
    for kind in crate::DEF_KIND_ORDER {
        counts.insert(kind.group().to_string(), json!(count_of(kind.word())));
    }
    for kind in UnitKind::ALL {
        counts.insert(kind.plural().to_string(), json!(count_of(kind.word())));
    }
    Value::Object(counts)
}

/// The func member names of a def that hosts them, in the host's own order.
/// Empty for the kinds that cannot host func members (interface / enum /
/// define) — the three kinds `register_host_funcs` does not walk either.
pub fn host_func_names(data: &DefValue) -> Vec<String> {
    match data {
        DefValue::Module(m) => m.funcs.iter().map(|f| f.name.to_string()).collect(),
        DefValue::Component(c) => c.funcs.iter().map(|f| f.name.to_string()).collect(),
        DefValue::Capability(c) => c.funcs.iter().map(|f| f.name.to_string()).collect(),
        _ => Vec::new(),
    }
}

/// The byte offset a def's own declaration starts at, when its value records a
/// span. `Func` never reaches this: a func row is not a def row (design §12.1).
fn def_span_start(data: &DefValue) -> Option<u32> {
    let start = |s: &std::ops::Range<usize>| Some(s.start as u32);
    match data {
        DefValue::Module(m) => start(&m.span),
        DefValue::Component(c) => start(&c.span),
        DefValue::Interface(i) => start(&i.span),
        DefValue::Enum(e) => Some(e.span[0]),
        DefValue::Define(d) => start(&d.span),
        DefValue::Capability(c) => start(&c.span),
        DefValue::Func(_) => None,
    }
}
