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
}
