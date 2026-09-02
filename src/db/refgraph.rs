// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! DefRefGraph — def resolution edges (defspace D14).
//!
//! Every class-name resolution that hits records an edge: `out` says "this
//! ref-point (name, file) resolved to that def", `rev` is the reverse
//! (dependents). Edges are the natural byproduct of pass1 resolution —
//! recorded at the single resolution bridge (`mcb_get_cmie_with_uri`), so no
//! separate pass is needed and the edge set is complete by construction.
//!
//! The current file-level `reverse_deps` stays as its coarse-grained subset
//! ("who uses this file"); this graph is the def-level granularity ("who
//! references this def"), feeding goto-def (out), who-uses (rev), and — once
//! the instance layer lands — the def→circuits invalidation index (which is
//! NOT held here, per §12.6).
//!
//! Edge granularity (honest boundary, T5): the `from` side is a ref-point
//! `(referenced-name, referencing-file)`, not the enclosing def — pass1's
//! resolution bridge (`unified_lookup`) is keyed by file, and threading the
//! enclosing def through every resolution call site is deferred until a
//! def→def consumer (def-level invalidation, §12.6 / T6) needs it. The `to`
//! side is a resolved def; D15.3 queries below answer def-scoped questions
//! (`dependents_of(DefId)`) through one registry hop, so consumers hold
//! ids, never text.

use crate::db::defregistry::{def_id as registry_def_id, kind_of, live_entry_by_id, DefId};
use crate::McSpaceName;
use dashmap::DashMap;

/// Per-world def resolution graph (D14). Nodes are canonical `(ident, uri)`
/// keys — the same identity the registry uses — so project and system-lib
/// defs mix freely and cross-world comparison is by canonical key.
#[derive(Debug, Clone, Default)]
pub struct DefRefGraph {
    out: DashMap<McSpaceName, Vec<McSpaceName>>,
    rev: DashMap<McSpaceName, Vec<McSpaceName>>,
}

impl DefRefGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a resolution edge `from → to` (deduplicated). `from` is the
    /// referencing ref-point `(referenced-name, referencing-file)`, `to` the
    /// resolved definition `(def-name, defining-file)`.
    pub fn record(&self, from: &McSpaceName, to: &McSpaceName) {
        let mut o = self.out.entry(from.clone()).or_default();
        if !o.contains(to) {
            o.push(to.clone());
        }
        drop(o);
        let mut r = self.rev.entry(to.clone()).or_default();
        if !r.contains(from) {
            r.push(from.clone());
        }
    }

    /// Defs that `from` resolved to (out edges) — the goto-def answer.
    #[allow(dead_code)] // D14 query API; unit-tested, wired by goto-def in a later phase
    pub fn referenced(&self, from: &McSpaceName) -> Vec<McSpaceName> {
        self.out.get(from).map(|v| v.clone()).unwrap_or_default()
    }

    /// Defs/ref-points that reference `to` (rev edges, dependents) — the
    /// who-uses / invalidation answer.
    #[allow(dead_code)] // D14 query API; unit-tested, wired by who-uses in a later phase
    pub fn dependents(&self, to: &McSpaceName) -> Vec<McSpaceName> {
        self.rev.get(to).map(|v| v.clone()).unwrap_or_default()
    }

    /// Whether `to` has any recorded dependents.
    #[allow(dead_code)] // D14 query API; unit-tested, wired by who-uses in a later phase
    pub fn has_dependents(&self, to: &McSpaceName) -> bool {
        self.rev.get(to).is_some_and(|v| !v.is_empty())
    }

    /// The registry [`DefId`] of a resolved def node (D15.3: a graph hit
    /// carries the def id — consumers hold ids, never text). One hop through
    /// the registry's canonical-key index; `None` when the def was removed
    /// (the graph may outlive a `remove_by_uri`).
    #[allow(dead_code)] // D14 query API; wired by goto-def / who-uses in a later phase
    pub fn def_id_of(&self, to: &McSpaceName) -> Option<DefId> {
        let kind = kind_of(to)?;
        registry_def_id(to, kind)
    }

    /// Def-scoped who-uses (rev): every ref-point that resolved to the def
    /// identified by `id`. `live_entry_by_id` maps the id back to the def's
    /// canonical key, then the rev side answers — one id → dependents
    /// without a text-keyed registry round trip on the caller's side.
    #[allow(dead_code)] // D14 query API; wired by who-uses in a later phase
    pub fn dependents_of(&self, id: DefId) -> Vec<McSpaceName> {
        let Some((sn, _)) = live_entry_by_id(id) else {
            return Vec::new();
        };
        self.dependents(&sn)
    }

    /// Def-scoped who-uses predicate — whether the def `id` has dependents
    /// (def-level invalidation domain: "does anything reference this def").
    #[allow(dead_code)] // D14 query API; wired by invalidation in a later phase
    pub fn has_dependents_of(&self, id: DefId) -> bool {
        let Some((sn, _)) = live_entry_by_id(id) else {
            return false;
        };
        self.has_dependents(&sn)
    }

    /// All out edges as `(from, targets)` pairs — used to rebuild a restored
    /// world's graph (record() reconstructs the rev side).
    pub fn out_pairs(&self) -> Vec<(McSpaceName, Vec<McSpaceName>)> {
        self.out
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    pub fn clear(&self) {
        self.out.clear();
        self.rev.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::McIds;

    fn sn(name: &str, uri: &str) -> McSpaceName {
        McSpaceName {
            ident: McIds::from(name),
            uri: crate::semantic::common::uri_intern(uri),
        }
    }

    #[test]
    fn records_out_and_rev_edges() {
        let g = DefRefGraph::new();
        let from = sn("LED", "proj/a.mc");
        let to = sn("LED", "mcode/led.mc");

        g.record(&from, &to);
        // Duplicate record is deduplicated.
        g.record(&from, &to);

        assert_eq!(g.referenced(&from), vec![to.clone()]);
        assert_eq!(g.dependents(&to), vec![from.clone()]);
        assert!(g.has_dependents(&to));
        assert!(!g.has_dependents(&from));

        // out_pairs round-trips through clear + record.
        let pairs = g.out_pairs();
        g.clear();
        assert!(!g.has_dependents(&to));
        for (f, ts) in pairs {
            for t in ts {
                g.record(&f, &t);
            }
        }
        assert_eq!(g.dependents(&to), vec![from]);
    }

    /// D15.3: a graph hit carries the registry [`DefId`] — one id answers
    /// the rev (who-uses) queries without a text-keyed registry round trip
    /// on the caller's side. Serializes on the crate-wide parse lock (the
    /// registry is process-wide) and uses a unique uri so parallel tests
    /// are never disturbed; the entry is removed afterwards.
    #[test]
    fn def_id_queries_answer_through_the_registry() {
        use crate::db::defregistry::{insert, remove_by_uri, DefValue, LoadDomain};
        use crate::db::infra::init::MCC_TEST_PARSE_LOCK;
        use crate::semantic::mc_enum::McEnumDef;

        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
        const NAME: &str = "REFGRAPH_DEF_ID_ENUM";
        const URI: &str = "/sys/refgraph_defid.mc";
        let to = McSpaceName {
            ident: crate::McIds::from(NAME),
            uri: crate::semantic::common::uri_intern(URI),
        };
        insert(
            &to,
            LoadDomain::SystemLib("mcode".into()),
            DefValue::Enum(std::sync::Arc::new(McEnumDef {
                name: to.ident.clone(),
                span: [0, 3],
                values: Vec::new(),
                uri: URI.to_string(),
            })),
        );

        let g = DefRefGraph::new();
        let from = sn("LED", "proj/a.mc");
        g.record(&from, &to);

        // One hop: resolved def node → registry DefId.
        let Some(id) = g.def_id_of(&to) else {
            panic!("a resolved def node must carry a registry DefId");
        };
        // Def-scoped who-uses through the id, not the text key.
        assert_eq!(g.dependents_of(id), vec![from.clone()]);
        assert!(g.has_dependents_of(id));

        // A dead id answers empty rather than panicking.
        assert!(g.dependents_of(u32::MAX).is_empty());
        assert!(!g.has_dependents_of(u32::MAX));

        // Leave no residue for parallel tests.
        remove_by_uri(URI);
    }
}
