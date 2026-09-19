// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The reverse index: a definition-space **name** → the circuit-side rows it
//! lands on (design `doc/arch/space/organization-units-design.md` §9).
//!
//! Build is a one-way mirror (design §9.1): the instance object carries its
//! def's coordinates, and every downstream read of it is a read of that same
//! field — so the direction **definition → circuit** never needed an index.
//! The opposite question ("I typed `RES_10K`; where is it on the board?") had
//! no answer at all, and looking one up by scanning is O(n) per keystroke,
//! which an editor's hot query cannot pay (§9.4).
//!
//! This index answers it by being **derived, per build, from the flat table** —
//! the same rows the forward leg publishes (`export::instlist`). It is the
//! shape `Overlays::point_index` already had (design §9.4), applied in the
//! other direction:
//!
//! - the **keys are names**, never ids: the def's own name as the author
//!   spelled it, plus the canonical pair `(uri, ident)` for a lookup that has
//!   to tell two same-named defs apart;
//! - it is **recomputed** by every projection and holds no counter, so no
//!   ordering of arrivals can reach it;
//! - it is **never persisted** and **never an identity**: nothing outside this
//!   process can hold one of its keys as a reference.
//!
//! The five conditions are the discipline `doc/pipeline/build-design.md` §3.7
//! states for a derived index (compliance = derived, not authoritative). What
//! is forbidden is the *stored bridge* — a table giving O(1) between a
//! `UriId` and a `PointId`, which is the violation this index does **not**
//! commit: a key here is a name or a canonical pair, both of which survive a
//! rebuild by being re-derived, and neither is an id of either space.
//!
//! **What is out of scope here, deliberately.** The value set is the forward
//! leg's rows: instance-level (`Module` / `Component`) and point-level (`Pin` /
//! `Port`). A label or a bus row names no def of its own, and the interface a
//! port is bound to lives in the description layer (Phase G) rather than in
//! `class_def` — so those two readings are not keys here yet.

use crate::instant::insttab::{InstKind, InstTable};
use crate::semantic::common::{McSpaceName, SourcePos};
use std::collections::BTreeMap;

/// One circuit-side row a key lands on: the entry's two-space identity, with
/// the kind tag that says which family of row it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Which family of row — the forward leg's instance level or point level.
    pub kind: InstKind,
    /// The flat entry id (an intra-build ordinal; it never leaves the build).
    pub id: u32,
    /// The canonical path the circuit knows this object by (`main.ldo.R1`).
    pub path: String,
    /// The arena node, when the row owns one.
    pub node: Option<u32>,
    /// The point this row names, when it names one (`Pin` / `Port`).
    pub point: Option<String>,
    /// The def the row belongs to: its own `class_def`, or the nearest
    /// ancestor's ([`InstTable::class_def_of`]).
    pub class: McSpaceName,
    /// Where the row is written: the wiring site when it has one, else the
    /// declaration site.
    pub pos: Option<SourcePos>,
}

impl Hit {
    /// The class's canonical pair `(uri, ident)`, both resolved to strings.
    pub fn class_pair(&self) -> (String, String) {
        (
            self.class.uri.as_uri().to_string(),
            self.class.ident.to_string(),
        )
    }
}

/// The per-build reverse index (design §9.6). Two key families, each a map to
/// the same rows:
///
/// - [`Self::names`] — the typed name: the class's `ident` exactly as the
///   author spelled it. One row of this map per distinct def name, so a
///   partial name is answered by matching over *keys*, never by scanning the
///   rows (design §9.4).
///
///   A qualified name gets **no** second key family. The only dotted identity
///   the def space mints today is a func's display label
///   (`db/defregistry.rs:763`), and its own comment says it is never a
///   resolution key — so a "bare last segment" family would hold no row at
///   all: a branch with no input, which answers nothing and hides that it
///   answers nothing. Partial and qualified typing both go through the key
///   matcher instead ([`crate::query::reverse`]), which is where a spelling
///   question belongs.
/// - [`Self::canon`] — the canonical key `(uri, ident)` kept **as a pair**, so
///   a qualified lookup distinguishes two defs that share a name. The pair is
///   never flattened into one string: its two coordinates are what the design
///   calls the key, and joining them would invent a third spelling.
///
/// `BTreeMap`, not `HashMap`: iteration order is part of what this index owes
/// the law (two readings of one world serialize alike), and a `HashMap` would
/// make the serialization depend on insertion history.
#[derive(Debug, Clone, Default)]
pub struct ReverseIndex {
    names: BTreeMap<String, Vec<Hit>>,
    canon: BTreeMap<(String, String), Vec<Hit>>,
}

impl ReverseIndex {
    /// Derive the index from the flat table, in the table's own walk order.
    ///
    /// Every row in the forward leg's value set is registered under each of its
    /// name keys and under its canonical pair, so one typed name reaches both
    /// an instance-level row and the point-level rows beneath it — which is the
    /// whole point: typing a component's class highlights the part *and* its
    /// pins.
    ///
    /// A row whose def cannot be resolved is skipped rather than keyed by
    /// something else: an unresolvable def is not a name, and inventing a key
    /// for it would put a row under a name the author never wrote.
    pub fn derive(table: &InstTable) -> Self {
        let mut names: BTreeMap<String, Vec<Hit>> = BTreeMap::new();
        let mut canon: BTreeMap<(String, String), Vec<Hit>> = BTreeMap::new();

        for (_, e) in table.iter() {
            // The forward leg's two sets: instance level then point level
            // (`export::instlist::build_inst_list`). Anything else in the table
            // is not a row this index answers with.
            if !matches!(
                e.kind,
                InstKind::Module | InstKind::Component | InstKind::Pin | InstKind::Port
            ) {
                continue;
            }
            let Some(class) = table.class_def_of(e.id) else {
                continue;
            };
            let hit = Hit {
                kind: e.kind.clone(),
                id: e.id,
                path: e.path.clone(),
                node: e.node_id.map(|n| n.0),
                point: e.point.map(|p| p.to_string()),
                class: class.clone(),
                pos: e.src_pos.clone().or_else(|| e.fallback_pos.clone()),
            };

            let (uri, ident) = hit.class_pair();
            canon
                .entry((uri, ident.clone()))
                .or_default()
                .push(hit.clone());
            if ident.is_empty() {
                continue;
            }
            names.entry(ident.clone()).or_default().push(hit);
        }

        ReverseIndex { names, canon }
    }

    /// Every name key, with its rows, in key order.
    pub fn names(&self) -> impl Iterator<Item = (&String, &Vec<Hit>)> {
        self.names.iter()
    }

    /// The rows registered under exactly this typed name.
    pub fn lookup(&self, name: &str) -> &[Hit] {
        self.names.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The rows registered under exactly this canonical pair.
    pub fn lookup_canon(&self, uri: &str, ident: &str) -> &[Hit] {
        self.canon
            .get(&(uri.to_string(), ident.to_string()))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Whether the index holds no key at all (an empty circuit, or one whose
    /// every row names no def).
    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.canon.is_empty()
    }

    /// How many name keys the index holds.
    pub fn len(&self) -> usize {
        self.names.len()
    }
}
