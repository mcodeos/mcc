// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The reverse index's outward rows (design `organization-units-design.md`
//! §9.6), built once and read by both mouths: the CLI (`mcc refs --circuit`)
//! and the RPC method (`defs.reverse`).
//!
//! Three readings, three shapes, one builder each:
//!
//! - **by pattern** ([`matched_rows`]): the keys a typed name matches and the
//!   rows they land on — the reading an editor's filter asks for, and the one
//!   that never touches the rows to answer (design §9.4).
//! - **by key** ([`hit_rows`]): the circuit-side rows that one key lands on. The
//!   row is the forward leg's row — node, path, class, point, loc — plus the
//!   `kind` tag and the key it was reached by, so a consumer holding only the
//!   rows can still say which name found them.
//! - **the key index** ([`key_rows`]): every key with what it holds. This is
//!   the reading that answers "what can I type at all", and it is where the
//!   key discipline (§9.6: no key is an id) is checkable from the outside.

use crate::instant::reverse::{Hit, ReverseIndex};
use crate::stages::{loc_of, SourceText};
use serde_json::{json, Value};

/// The rows one key lands on.
///
/// `key` is the spelling the caller matched — a typed name, or the canonical
/// pair written as `uri#ident` for a canonical lookup. The pair itself is
/// spelled out in every row's `class`, so this field is the *question*, not a
/// second copy of the answer.
pub fn hit_rows(key: &str, hits: &[Hit], sources: &mut SourceText) -> Vec<Value> {
    hits.iter()
        .map(|h| {
            let (uri, ident) = h.class_pair();
            json!({
                "key": key,
                "kind": h.kind.word(),
                "path": h.path,
                "node": h.node,
                "point": h.point,
                "class": { "uri": uri, "ident": ident },
                "loc": loc_of(h.pos.as_ref(), sources),
            })
        })
        .collect()
}

/// The keys matching `pattern`, and the rows they land on, both in key order.
///
/// The match is the engine's default one — case-insensitive substring, the
/// same matcher `defs.search` and `query --kind instance` use
/// ([`crate::query::search::build_matcher`] with both flags false). It runs
/// over the **keys**, of which there is one per distinct def name, so a
/// keystroke costs the number of distinct names and not the number of objects
/// on the board — which is the whole reason the index exists (design §9.4).
///
/// The keys are returned beside the rows because they are the answer to a
/// second question the caller has to be able to ask: whether anything matched
/// at all. An empty key list is "this build has no such name", and no row
/// spelling can say that.
pub fn matched_rows(
    index: &ReverseIndex,
    pattern: &str,
    sources: &mut SourceText,
) -> (Vec<String>, Vec<Value>) {
    // The substring arm cannot fail; the `Result` exists for the regex arm.
    let Some(matcher) = crate::query::search::build_matcher(pattern, false, false).ok() else {
        return (Vec::new(), Vec::new());
    };
    let mut keys: Vec<String> = Vec::new();
    let mut rows: Vec<Value> = Vec::new();
    for (key, hits) in index.names() {
        if !matcher(key) {
            continue;
        }
        keys.push(key.clone());
        rows.extend(hit_rows(key, hits, sources));
    }
    (keys, rows)
}

/// The key index: one row per name key, in key order.
///
/// `kinds` lists the row families the key holds, in the order they were
/// walked (instance level before point level), so an empty list is impossible
/// and the reader can tell a def that named a part from one that named a port
/// column without counting rows.
pub fn key_rows(index: &ReverseIndex) -> Vec<Value> {
    index
        .names()
        .map(|(key, hits)| {
            let mut kinds: Vec<&str> = Vec::new();
            for h in hits {
                let w = h.kind.word();
                if !kinds.contains(&w) {
                    kinds.push(w);
                }
            }
            json!({ "name": key, "count": hits.len(), "kinds": kinds })
        })
        .collect()
}
