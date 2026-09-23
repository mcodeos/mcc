// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! DefRefGraph — def resolution edges (defspace D14).
//!
//! Every class-name resolution that hits records an edge: `out` says "this
//! ref-point (name, file) resolved to that def", `rev` is the reverse
//! (dependents). Edges are the natural byproduct of resolution — recorded
//! inside the resolution policy itself (`Resolver::resolve_class` /
//! `resolve_class_locked` wrappers, `policy.rs::record_resolution_edge`) plus
//! the `mcb_get_cmie_with_uri` bridge, so no separate pass is needed and the
//! edge set is complete by construction. `record` dedups, so a resolution
//! flowing through several layers stays a single edge.
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
//!
//! Coverage (U234 tier ①): the five former bypassers all record now —
//! member resolution (`db/resolve/member.rs` via the locked wrapper), the
//! consolidation/lapper span resolution (`mc_code.rs` via the locked
//! wrapper), the scan-based declare-class registration (`query/refs.rs`,
//! own record at Step 3), the goto-def legs (`lsp/gotodef.rs`, own records
//! — the raw name-only path has no referencing file and stays unrecorded),
//! and the re-entrant fallback (`cmie.rs`, own record). Still invisible:
//! member-level references (the edge shape has no member dimension) and
//! Inst/Label-kind references collected by `references::find_at` — a read
//! face that pre-filters by graph hits must not drop those (ruling D4).

use crate::db::defregistry::{def_id as registry_def_id, kind_of, live_entry_by_id, DefId};
use crate::McSpaceName;
use std::collections::HashSet;
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
    /// (the graph may outlive a `remove_by_uri`). Consumed by the def-level
    /// who-uses RPC (`defs.dependents`, T5).
    pub fn def_id_of(&self, to: &McSpaceName) -> Option<DefId> {
        let kind = kind_of(to)?;
        registry_def_id(to, kind)
    }

    /// Def-scoped who-uses (rev): every ref-point that resolved to the def
    /// identified by `id`. `live_entry_by_id` maps the id back to the def's
    /// canonical key, then the rev side answers — one id → dependents
    /// without a text-keyed registry round trip on the caller's side.
    /// Consumed by the def-level who-uses RPC (`defs.dependents`, T5).
    pub fn dependents_of(&self, id: DefId) -> Vec<McSpaceName> {
        let Some((sn, _)) = live_entry_by_id(id) else {
            return Vec::new();
        };
        self.dependents(&sn)
    }

    /// Def-scoped who-uses predicate — whether the def `id` has dependents
    /// (def-level invalidation domain: "does anything reference this def").
    /// Consumed by the def-level who-uses RPC (`defs.dependents`, T5).
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

    /// Drop every edge touching `uri` (U234) — the re-parse / file-removal
    /// purge. An edge is stale when either end's file changed: ref-points in
    /// the edited file may no longer resolve there, and defs in it may have
    /// changed identity or disappeared. Both faces are swept symmetrically
    /// (out keys are ref-points / rev keys are defs; each face's values are
    /// the other end), and empty buckets are removed — the graph never keeps
    /// shells. Without this, `dependents` answers from edges a re-parse
    /// already invalidated.
    pub fn purge_file(&self, uri: &str) {
        purge_side(&self.out, |u| u == uri);
        purge_side(&self.rev, |u| u == uri);
    }

    /// Multi-file form of [`purge_file`] — the lib-unload sweep, which
    /// tombstones every def under a uri set in one round.
    pub fn purge_files(&self, uris: &HashSet<String>) {
        purge_side(&self.out, |u| uris.contains(u));
        purge_side(&self.rev, |u| uris.contains(u));
    }
}

/// One face of the purge: drop keys whose own file matches, drop value
/// entries whose other end's file matches, drop keys whose values went
/// empty. Run over both faces (`out` and `rev`) so an edge with either end
/// in the purged set is gone from both sides.
fn purge_side(map: &DashMap<McSpaceName, Vec<McSpaceName>>, matches: impl Fn(&str) -> bool) {
    map.retain(|key, targets| {
        if matches(key.uri.as_uri().as_ref()) {
            return false;
        }
        targets.retain(|t| !matches(t.uri.as_uri().as_ref()));
        !targets.is_empty()
    });
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
    fn def_refgraph__records_out_and_rev_edges() {
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

    /// U234: the re-parse / file-removal purge drops every edge touching the
    /// uri — as either end — and never leaves an empty bucket behind.
    #[test]
    fn def_refgraph__purge_file_drops_edges_touching_the_uri() {
        let g = DefRefGraph::new();
        let from_a = sn("LED", "proj/a.mc");
        let from_b = sn("LED", "proj/b.mc");
        let to_led = sn("LED", "mcode/led.mc");
        let to_res = sn("RES", "mcode/res.mc");

        g.record(&from_a, &to_led);
        g.record(&from_a, &to_res);
        g.record(&from_b, &to_led);

        // Purge by ref-point file: a's out bucket and every rev entry that
        // names it go; b's edges survive untouched.
        g.purge_file("proj/a.mc");
        assert!(g.referenced(&from_a).is_empty(), "out key a is gone");
        assert_eq!(g.referenced(&from_b), vec![to_led.clone()]);
        assert_eq!(
            g.dependents(&to_led),
            vec![from_b.clone()],
            "rev[led] keeps only the surviving ref-point"
        );
        // res lost its only ref-point: the bucket is dropped, not emptied.
        assert!(!g.has_dependents(&to_res), "no empty shell for res");

        // Purge by def file: the target side sweeps symmetrically.
        g.purge_file("mcode/led.mc");
        assert!(g.referenced(&from_b).is_empty(), "out key b emptied and dropped");
        assert!(!g.has_dependents(&to_led));
        assert_eq!(g.dependents(&to_res), Vec::<McSpaceName>::new());
    }

    /// U234: the lib-unload sweep shape — one call, a uri set, every edge
    /// touching any of them gone, everything else preserved.
    #[test]
    fn def_refgraph__purge_files_matches_the_lib_sweep_shape() {
        let g = DefRefGraph::new();
        let from_proj = sn("LED", "proj/a.mc");
        let from_lib = sn("SUB", "mclibs/sub.mc");
        let to_led = sn("LED", "mcode/led.mc");
        let to_sub = sn("SUB", "mcode/sub.mc");
        let to_keep = sn("RES", "mcode/res.mc");

        g.record(&from_proj, &to_led);
        g.record(&from_proj, &to_keep);
        g.record(&from_lib, &to_sub);

        let uris: HashSet<String> = ["mcode/led.mc", "mclibs/sub.mc"]
            .into_iter()
            .map(String::from)
            .collect();
        g.purge_files(&uris);

        assert_eq!(g.referenced(&from_proj), vec![to_keep.clone()]);
        assert!(g.referenced(&from_lib).is_empty());
        assert!(!g.has_dependents(&to_led));
        assert!(!g.has_dependents(&to_sub));
        assert_eq!(g.dependents(&to_keep), vec![from_proj]);
    }

    /// D15.3: a graph hit carries the registry [`DefId`] — one id answers
    /// the rev (who-uses) queries without a text-keyed registry round trip
    /// on the caller's side. Serializes on the crate-wide parse lock (the
    /// registry is process-wide) and uses a unique uri so parallel tests
    /// are never disturbed; the entry is removed afterwards.
    #[test]
    fn def_refgraph__def_id_queries_answer_through_the_registry() {
        use crate::db::cmie::tables::WORKSPACE;
        use crate::db::defregistry::{remove_by_uri, DefValue, LoadDomain};
        use crate::db::infra::init::MCC_TEST_PARSE_LOCK;
        use crate::semantic::mc_enum::McEnumDef;

        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
        const NAME: &str = "REFGRAPH_DEF_ID_ENUM";
        const URI: &str = "/sys/refgraph_defid.mc";
        let to = McSpaceName {
            ident: crate::McIds::from(NAME),
            uri: crate::semantic::common::uri_intern(URI),
        };
        WORKSPACE.insert_def(
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
