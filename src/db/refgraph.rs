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
//! Coverage (U234 tiers ①–②): the five former bypassers all record (member
//! resolution and the consolidation/lapper span resolution via the locked
//! wrapper, the scan-based declare-class registration, the goto-def legs,
//! the re-entrant fallback), and tier ② closes the remaining gap: every
//! whitelisted ref entry entering a `RefDefMap` records its edge at the
//! `RefDefMap::insert` chokepoint (owner file × def file), so the
//! Inst/Label-kind references `references::find_at` collects are all
//! edge-backed and the who-uses face can prefilter on the graph without
//! dropping them (ruling D4). Still invisible: member-level references
//! (the edge shape has no member dimension).
//!
//! Purge is deliberately one-sided (ref-points only): a purged file's own
//! edges go and are re-recorded by its rebuild, while edges from other
//! files into it survive — they are as stale as those files' own maps,
//! which the read faces already tolerate, and over-approximation is free
//! because a prefiltered face post-filters on the exact def key.

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
    /// File-level projection of `rev` (U234 tier ②): def file → every file
    /// with at least one recorded ref-point into it. Prefilters read faces
    /// (who-uses) at file granularity, where ident mismatches between a
    /// def's per-file map entries cannot cause a miss — over-approximation
    /// is free because the face post-filters on the exact def key.
    rev_files: DashMap<String, Vec<String>>,
}

impl DefRefGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a resolution edge `from → to` (deduplicated). `from` is the
    /// referencing ref-point `(referenced-name, referencing-file)`, `to` the
    /// resolved definition `(def-name, defining-file)`. Also maintains the
    /// file-level projection (def file → referencing files).
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
        drop(r);
        let def_file = to.uri.as_uri().to_string();
        let ref_file = from.uri.as_uri().to_string();
        let mut rf = self.rev_files.entry(def_file).or_default();
        if !rf.contains(&ref_file) {
            rf.push(ref_file);
        }
    }

    /// Files with at least one recorded ref-point into `def_file` (the
    /// file-level projection, U234 tier ②) — the who-uses prefilter
    /// candidate set, minus the def file itself.
    pub fn dependent_files_of_file(&self, def_file: &str) -> Vec<String> {
        self.rev_files
            .get(def_file)
            .map(|v| v.clone())
            .unwrap_or_default()
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
        self.rev_files.clear();
    }

    /// Drop the ref-points of `uri` (U234) — the re-parse / file-removal
    /// purge. Only edges whose **from** side lives in the purged file go:
    /// ref-points in a re-parsed file may no longer resolve there, so their
    /// edges are re-recorded by the file's own rebuild. Edges *pointing
    /// into* the purged file are kept: the referencing files have not been
    /// rebuilt yet, and their own maps still hold the same (equally stale)
    /// refs — a graph-prefiltered read face must over-approximate, never
    /// under-approximate (D4). A referencing file's next rebuild (or its
    /// removal, which purges its ref-points) restores exactness.
    pub fn purge_file(&self, uri: &str) {
        self.purge_ref_points(&[uri.to_string()]);
    }

    /// Multi-file form of [`purge_file`] — the lib-unload sweep, which
    /// tombstones every ref-point under a uri set in one round.
    pub fn purge_files(&self, uris: &HashSet<String>) {
        let set: Vec<String> = uris.iter().cloned().collect();
        self.purge_ref_points(&set);
    }

    /// The ref-point-side sweep shared by both purge forms: drop out keys in
    /// the set, drop rev values in the set (empty rev buckets go too), drop
    /// the set's files from the file-level projection.
    fn purge_ref_points(&self, uris: &[String]) {
        self.out.retain(|key, _| !uris.contains(&key.uri.as_uri().to_string()));
        self.rev.retain(|_, froms| {
            froms.retain(|f| !uris.contains(&f.uri.as_uri().to_string()));
            !froms.is_empty()
        });
        self.rev_files.retain(|_, refs| {
            refs.retain(|f| !uris.contains(f));
            !refs.is_empty()
        });
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

    /// U234 tier ②: the purge is ref-point-side only. A purged file's own
    /// ref-points (and their rev entries, and its rows in the file
    /// projection) go; edges from other files *into* the purged file stay —
    /// the referencing files' maps are equally stale, and a prefiltered
    /// read face must over-approximate, never under-approximate (D4).
    #[test]
    fn def_refgraph__purge_file_drops_ref_points_keeps_incoming_edges() {
        let g = DefRefGraph::new();
        let from_a = sn("LED", "proj/a.mc");
        let from_b = sn("LED", "proj/b.mc");
        let to_led = sn("LED", "mcode/led.mc");
        let to_res = sn("RES", "mcode/res.mc");

        g.record(&from_a, &to_led);
        g.record(&from_a, &to_res);
        g.record(&from_b, &to_led);

        // Purge ref-point file a: a's edges go everywhere; b's edge into
        // mcode/led.mc survives untouched.
        g.purge_file("proj/a.mc");
        assert!(g.referenced(&from_a).is_empty(), "out key a is gone");
        assert_eq!(g.referenced(&from_b), vec![to_led.clone()]);
        assert_eq!(
            g.dependents(&to_led),
            vec![from_b.clone()],
            "rev[led] keeps only the surviving ref-point"
        );
        assert!(!g.has_dependents(&to_res), "no empty shell for res");
        assert_eq!(
            g.dependent_files_of_file("mcode/led.mc"),
            vec!["proj/b.mc".to_string()],
            "projection drops the purged ref file, keeps the survivor"
        );
        assert!(
            g.dependent_files_of_file("mcode/res.mc").is_empty(),
            "projection never keeps an empty bucket"
        );

        // Purge def file mcode/led.mc: b's edge INTO it stays (b has not
        // been rebuilt; its map still holds the same ref).
        g.purge_file("mcode/led.mc");
        assert_eq!(g.referenced(&from_b), vec![to_led.clone()], "incoming edge survives");
        assert_eq!(
            g.dependent_files_of_file("mcode/led.mc"),
            vec!["proj/b.mc".to_string()],
            "projection keeps the referencing file"
        );
    }

    /// U234: the lib-unload sweep shape — one call, a uri set, every
    /// ref-point under any of them gone, edges into the set preserved.
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

        // proj's ref-point into led.mc points INTO the purged set: kept.
        assert_eq!(g.referenced(&from_proj), vec![to_led.clone(), to_keep.clone()]);
        assert!(g.referenced(&from_lib).is_empty(), "lib-internal ref-point purged");
        assert_eq!(g.dependents(&to_keep), vec![from_proj]);
        assert_eq!(
            g.dependent_files_of_file("mcode/led.mc"),
            vec!["proj/a.mc".to_string()]
        );
    }

    /// U234 tier ②: the file-level projection records alongside the edges
    /// and dedups.
    #[test]
    fn def_refgraph__file_projection_records_and_dedups() {
        let g = DefRefGraph::new();
        let from_a = sn("LED", "proj/a.mc");
        let from_a2 = sn("LED2", "proj/a.mc");
        let to_led = sn("LED", "mcode/led.mc");

        g.record(&from_a, &to_led);
        g.record(&from_a2, &to_led);
        g.record(&from_a, &to_led);

        assert_eq!(
            g.dependent_files_of_file("mcode/led.mc"),
            vec!["proj/a.mc".to_string()],
            "one row per referencing file, however many ref-points"
        );
        assert!(g.dependent_files_of_file("mcode/other.mc").is_empty());
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
