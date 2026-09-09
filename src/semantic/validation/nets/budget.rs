// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PWR-4 budget, root-scope aggregation (rail-contract-design.md §8.5 — S-set
//! step 1, copper/module-boundary-transparent). The §8.2 kernel adjudicated one
//! net in isolation: a capacity-bearing supply root (domain-rail face or a
//! `psrc`/`psbi` hot pin carrying `capacity`) budgeted the `amp` sinks hanging
//! on *that same net*.
//!
//! This leaf raises 6021 from net-local to **supply-root scope**: a budget root
//! governs every downstream net separated from it only by current-transparent
//! copper (two-pin elements with no DC rows) and module-boundary junctions, so
//! a ferrite/fuse/inductor leg no longer escapes its upstream rail's budget.
//!
//! Classification of one flat net (mirrors window.rs `resolve_rootless`):
//! 1. agreed capacity → the net is its own root (never ascended past — a rail
//!    is not re-rooted upstream toward the converter that feeds it);
//! 2. a supply face (rail guarantee or psrc/psbi hot pin) without an agreed
//!    capacity → source boundary: no oracle, opaque to walkers (push-up / the
//!    single-source-mode budget stay deferred);
//! 3. rootless → ascend: first copper pass device forwards its other net's
//!    outcome; module-boundary co-segments forward a resolved root only.
//! The demand of each net is then accumulated onto its resolved root's bucket
//! and 6021 fires once per over-capacity root at the first contributing sink.

use super::NetCheckResult;
use crate::instant::insttab::{InstKind, InstTable, NetEntry};
use crate::semantic::common::IOType;
use crate::semantic::module::pi::decode_pwr_pin;
use std::collections::{HashMap, HashSet};

/// Budget-root outcome for one flat net.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BudgetRoot {
    /// The net itself carries one agreed capacity — its own root. `cap` is the
    /// governing budget; downstream rootless nets may be attributed to it, but
    /// it is never ascended past toward an upstream source.
    Root { cap: f64 },
    /// A rootless net governed by an upstream self-root (`root` net id, `cap`),
    /// reached through transparent copper or a module-boundary co-segment.
    Reached { root: u32, cap: f64 },
    /// No budget oracle — a source/rail face without capacity (boundary), an
    /// ambiguous (disagreeing) capacity, an unresolvable rootless net, or a
    /// recursion cycle. Silent and opaque to walkers.
    Opaque,
}

/// Recursive root-of-net engine (module-level note). One engine per flat run,
/// memoized and cycle-guarded like the window.rs `WindowDeriv` engine — the
/// shared `PowerScan` face maps stay in the parent module.
struct BudgetScan<'a> {
    table: &'a InstTable,
    scan: super::PowerScan,
    memo: HashMap<u32, BudgetRoot>,
    stack: HashSet<u32>,
}

impl<'a> BudgetScan<'a> {
    fn new(table: &'a InstTable) -> BudgetScan<'a> {
        BudgetScan {
            table,
            scan: super::PowerScan::build(table),
            memo: HashMap::new(),
            stack: HashSet::new(),
        }
    }

    /// Budget-root state of a net — memoized, cycle-guarded. Re-entering a net
    /// already on this stack is a cycle → `Opaque` (never fabricate a root).
    fn root_of(&mut self, net_id: u32) -> BudgetRoot {
        if let Some(r) = self.memo.get(&net_id) {
            return *r;
        }
        if !self.stack.insert(net_id) {
            return BudgetRoot::Opaque;
        }
        let root = match self.table.get_net(net_id) {
            None => BudgetRoot::Opaque,
            Some(net) => self.resolve_net(net),
        };
        self.stack.remove(&net_id);
        self.memo.insert(net_id, root);
        root
    }

    /// §8.5 priority resolution for one net.
    fn resolve_net(&mut self, net: &NetEntry) -> BudgetRoot {
        // 1. an agreed capacity on the net (rail face first, then source pins)
        //    makes the net its own root. Disagreeing roots are the ambiguous
        //    6010/6013 scope → an opaque source boundary, never adjudicated and
        //    never crossed.
        let caps = self.scan.capacity_roots(self.table, net);
        if let Some(&cap) = caps.first() {
            if caps.iter().any(|c| (*c - cap).abs() > 1e-9) {
                return BudgetRoot::Opaque;
            }
            return BudgetRoot::Root { cap };
        }

        // 2. no capacity: a net that itself generates or guarantees supply is a
        //    source boundary (no oracle — push-up and the single-source-mode
        //    combine budget stay deferred), opaque to the copper ascent.
        if self.has_supply_face(net) {
            return BudgetRoot::Opaque;
        }

        // 3. rootless — walk transparent copper, then module boundaries.
        self.resolve_rootless(net)
    }

    /// A rail guarantee face or a psrc/psbi hot pin on the net — the §8.5
    /// source-boundary test (a net that supplies without declaring capacity is
    /// not a budget root and not a pass-through).
    fn has_supply_face(&self, net: &NetEntry) -> bool {
        if self.scan.rail_face(net).is_some() {
            return true;
        }
        for &pid in &net.points {
            let Some(entry) = self.table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(cid) = entry.parent_id else {
                continue;
            };
            let Some(def) = self.scan.def_of(cid) else {
                continue;
            };
            if super::source_contract_for(def, entry).is_some() {
                return true;
            }
        }
        false
    }

    /// §8.5 priority 3 — no capacity, no supply face: copper pass-through or a
    /// module-boundary root, mirroring window.rs `resolve_rootless` (§6.3/§6.6).
    fn resolve_rootless(&mut self, net: &NetEntry) -> BudgetRoot {
        // Copper arm (§6.3): a two-pin element with no DC rows on this net
        // (fuse/inductor/ferrite) forwards its other net's outcome verbatim —
        // current-transparent, so the budget root governs both sides.
        for &pid in &net.points {
            let Some(entry) = self.table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) {
                continue;
            }
            let Some(cid) = entry.parent_id else {
                continue;
            };
            let Some(def) = self.scan.def_arc(cid) else {
                continue;
            };
            if !def.pins.pwr.is_empty() {
                continue; // has DC rows → a power face, not raw copper
            }
            // The other pin of this pass device lands on a different net.
            let mut other: Option<u32> = None;
            for pin in self.table.get_pins_of(cid) {
                let Some(pnet) = self.table.get_net_of(pin.id) else {
                    continue;
                };
                if pnet.id != net.id {
                    other = Some(pnet.id);
                    break;
                }
            }
            if let Some(onet) = other {
                return match self.root_of(onet) {
                    BudgetRoot::Root { cap } => BudgetRoot::Reached { root: onet, cap },
                    // a transparent chain keeps the same governing root; an
                    // opaque/boundary upstream stops the walk.
                    r => r,
                };
            }
        }
        // Module-boundary arm (§6.6): the SAME physical copper across a
        // submodule port is a second NetEntry (both share the junction point id,
        // `module` differs). Forward only a genuinely resolved root from that
        // co-segment; a rootless/boundary child side stays Opaque (cycle-safe —
        // a child recursing back here hits the in_progress guard).
        if let Some(m) = net.module {
            for &pid in &net.points {
                for &cid in self.table.nets_of(pid) {
                    if cid == net.id {
                        continue;
                    }
                    let Some(co) = self.table.get_net(cid) else {
                        continue;
                    };
                    if co.module.is_none() || co.module == Some(m) {
                        continue; // same-scope segment, not a module boundary
                    }
                    match self.root_of(cid) {
                        BudgetRoot::Root { cap } => return BudgetRoot::Reached { root: cid, cap },
                        r @ BudgetRoot::Reached { .. } => return r,
                        BudgetRoot::Opaque => {}
                    }
                }
            }
        }
        BudgetRoot::Opaque
    }
}

/// PWR-4 budget, supply-root scope (rail-contract-design.md §8.5). Replaces the
/// §8.2 net-local kernel: every net's declared `amp` demand (§8.1, sink-exclusive
/// opt-in) is accumulated onto the budget root governing that net — the net
/// itself when it carries an agreed capacity, otherwise the upstream self-root
/// reached through current-transparent copper / module boundaries. 6021 fires
/// once per over-capacity root (`Σ amp ≤ capacity`, else over) at the first
/// contributing sink's entry, named after the *root* net.
///
/// An isolated self-root net (no rootless net re-routed onto it) accumulates
/// exactly its own demand, so its fire/no-fire output is byte-identical to the
/// §8.2 kernel. Nets with no oracle — a supply face without capacity, an
/// ambiguous capacity, an unresolvable or cyclic rootless net — stay silent.
pub(crate) fn check_net_budget(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let mut eng = BudgetScan::new(table);

    /// Aggregated demand of one budget root, opened on its first contributing
    /// amp sink (in net-visit order — the order the §8.2 kernel reported nets).
    #[derive(Default)]
    struct Bucket {
        cap: f64,
        demand: f64,
        count: usize,
        witness: Option<(u32, String)>,
    }
    let mut order: Vec<u32> = Vec::new();
    let mut buckets: HashMap<u32, Bucket> = HashMap::new();

    for net in table.get_nets() {
        // The root governing this net (itself for a self-root, else the reached
        // upstream self-root). Opaque nets contribute nothing — no oracle.
        let (root, cap) = match eng.root_of(net.id) {
            BudgetRoot::Root { cap } => (net.id, cap),
            BudgetRoot::Reached { root, cap } => (root, cap),
            BudgetRoot::Opaque => continue,
        };

        // ── This net's own declared amp demand: every decodable psnk sink on
        //    the net that declares amp (the §8.2 gather, unchanged). ──
        let mut demand: f64 = 0.0;
        let mut count: usize = 0;
        let mut witness: Option<(u32, String)> = None;
        for &pid in &net.points {
            let Some(entry) = table.get_entry(pid) else {
                continue;
            };
            if !matches!(entry.kind, InstKind::Pin) || !matches!(entry.io_type, IOType::Power) {
                continue;
            }
            let Some(comp_id) = entry.parent_id else {
                continue;
            };
            let Some(def) = eng.scan.def_of(comp_id) else {
                continue;
            };
            let Some(contract) = super::sink_contract_for(def, entry) else {
                continue;
            };
            let dec = decode_pwr_pin(contract);
            let Some(a) = dec.amp else {
                continue; // sink draws unknown current — not counted (opt-in)
            };
            demand += a;
            count += 1;
            if witness.is_none() {
                witness = Some(super::entry_pos(entry));
            }
        }
        if count == 0 {
            continue;
        }

        let bucket = buckets.entry(root).or_insert_with(|| {
            order.push(root);
            Bucket {
                cap,
                demand: 0.0,
                count: 0,
                witness: None,
            }
        });
        bucket.demand += demand;
        bucket.count += count;
        if bucket.witness.is_none() {
            bucket.witness = witness;
        }
    }

    // Report once per over-capacity root, in bucket-open order, anchored at the
    // first contributing sink across all of the root's nets.
    for root in order {
        let b = &buckets[&root];
        if b.demand <= b.cap + 1e-9 {
            continue;
        }
        let root_name = table
            .get_net(root)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let (pos, uri) = b.witness.clone().unwrap_or((0, String::new()));
        results.push(NetCheckResult {
            check: "net-budget-exceeded",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::NET_BUDGET_EXCEEDED,
                &[
                    &root_name,
                    &super::fmt_amps(b.demand),
                    &b.count.to_string(),
                    &super::fmt_amps(b.cap),
                ],
            ),
            net_name: root_name,
            code: crate::errcodes::NET_BUDGET_EXCEEDED,
            pos,
            uri,
        });
    }
}
