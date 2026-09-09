// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PWR-1 / PWR-2 supply reach across transparent copper and module boundaries
//! (net-island-attribution-design.md §7 L4 — the *nominal* face of the S-set
//! step that budget.rs opened for capacity, rail-contract-design.md §8.5).
//!
//! 6019 (no-source kernel) and 6011 (mandatory nominal) adjudicate one flat net
//! in isolation: a net with no handwritten supply root on itself is an orphan
//! only when it also draws current, and both kernels documented the "copper
//! pass-through feed = later S-set step" gap. This leaf closes it for the
//! presence/nominal checks: a root-less sink net that is **fed** through
//! current-transparent copper (a two-pin element with no DC rows —
//! fuse/inductor/ferrite) or a module-boundary junction to an upstream supply
//! root is not a PWR-1 orphan (6019 stays silent) and is adjudicated against
//! that upstream root's nominal by 6011.
//!
//! Classification of one flat net (mirrors budget.rs `resolve_rootless` + the
//! window.rs §6.3/§6.6 walks):
//! 1. a rail guarantee face or a decodable `psrc`/`psbi` hot pin → the net is
//!    its own supply root — `has_supply`, nominal `net_nominal` (never ascended
//!    past, byte-identical to the net-local kernels);
//! 2. **return/reference copper** (island `Ret`/`Reference` role) at the seed
//!    *or* at any ascent hop → NOT fed, never climbed (a decoupling cap to GND
//!    is not a feed — this is the constraint that keeps the engine from
//!    false-silencing a genuinely undriven net through the shared return net);
//! 3. rootless → OR across every transparent copper leg and every module-boundary
//!    co-segment (unlike budget.rs's first-wins, a net is fed if *any* leg
//!    reaches a root; an unfed or cyclic leg just yields the next). No fed leg
//!    → NOT fed (6019 fires on a demand-carrying net, 6011 skips it).
//! The result is two-boolean-plus-nominal: 6011 consumes the agreed nominal,
//! 6019 consumes `has_supply` alone.

use crate::instant::insttab::{InstKind, InstTable, NetEntry};
use crate::instant::island::{NetIslandIndex, NetRole};
use std::collections::{HashMap, HashSet};

/// Supply-reach outcome for one flat net.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reach {
    /// A supply root governs the net — directly on it, or reachable through
    /// transparent copper / a module-boundary junction.
    pub has_supply: bool,
    /// The governing root's agreed nominal (§4.3 `net_nominal`, verbatim).
    /// `None` with `has_supply` = the root's own nominal is un-adjudicated
    /// (source contention — 6010/6013 territory), so 6011 skips as today.
    pub nominal: Option<(f64, String)>,
}

impl Reach {
    const NOT_FED: Reach = Reach {
        has_supply: false,
        nominal: None,
    };
}

/// Reach engine over the flat table (mirrors budget.rs `BudgetScan`). One
/// engine per flat-run owner; the shared `PowerScan` face maps stay in the
/// parent module and the island index is built once here (the §7 L4 role map
/// that this batch is the first genuine consumer of).
pub(crate) struct ReachScan<'a> {
    table: &'a InstTable,
    /// The parent module's face scan — kept `pub(crate)` so the 6011/6019
    /// owner fire loops in `nets/mod.rs` still call `scan.def_of` on it.
    pub(crate) scan: super::PowerScan,
    /// Per-net island attribution (net → `Ret`/`Reference`/`Hot`/… role).
    idx: NetIslandIndex,
    memo: HashMap<u32, Reach>,
    stack: HashSet<u32>,
    /// Frame-local cycle sentinel: set when a recursion re-enters a net already
    /// on the stack. Gates whether an *unfed* verdict may be memoized.
    cycle: bool,
}

impl<'a> ReachScan<'a> {
    pub(crate) fn new(table: &'a InstTable) -> ReachScan<'a> {
        ReachScan {
            table,
            scan: super::PowerScan::build(table),
            idx: NetIslandIndex::build(table),
            memo: HashMap::new(),
            stack: HashSet::new(),
            cycle: false,
        }
    }

    /// 6019's fed test: is this net governed by a supply root (own or reached)?
    pub(crate) fn has_supply_of(&mut self, net_id: u32) -> bool {
        self.reach_of(net_id).has_supply
    }

    /// 6011's S(net) nominal: the governing root's agreed nominal. `None` when
    /// the net has no governing root *or* the root's nominal is un-adjudicated
    /// — the reach replacement for `net_nominal`'s "intermediate (S-set later)"
    /// skip.
    pub(crate) fn nominal_of(&mut self, net_id: u32) -> Option<(f64, String)> {
        let r = self.reach_of(net_id);
        if r.has_supply {
            r.nominal
        } else {
            None
        }
    }

    /// Reach state of a net — memoized, cycle-guarded. Re-entering a net
    /// already on this stack is a cycle: that *edge* is NOT fed (never fabricate
    /// a feed from a loop), and resolution continues with the net's other legs.
    fn reach_of(&mut self, net_id: u32) -> Reach {
        if let Some(r) = self.memo.get(&net_id) {
            return r.clone();
        }
        if !self.stack.insert(net_id) {
            self.cycle = true;
            return Reach::NOT_FED;
        }
        let outer_cycle = self.cycle;
        self.cycle = false; // this frame's own cycle count
        let r = match self.table.get_net(net_id) {
            None => Reach::NOT_FED,
            Some(net) => self.resolve_net(net),
        };
        self.stack.remove(&net_id);
        let saw_cycle = self.cycle;
        self.cycle = outer_cycle || saw_cycle;
        // A *fed* verdict is final (resolution exhausts every leg — the cycle
        // guard only marks an edge, never short-circuits the net's own walk) and
        // is always memoized. An *unfed* verdict is provisional when this frame
        // saw a cycle: the net may be reachable-fed through a net still on the
        // stack above it (whose fed-ness is decided only after this frame
        // closes), so it must not be cached — a later query re-resolves it.
        if r.has_supply || !saw_cycle {
            self.memo.insert(net_id, r.clone());
        }
        r
    }

    /// §7 L4 priority resolution for one net.
    fn resolve_net(&mut self, net: &NetEntry) -> Reach {
        // 1. a handwritten supply root on the net — rail guarantee face or a
        //    decodable psrc/psbi hot pin — makes the net its own root. Nominal
        //    is the §4.3 derivation verbatim (disagreeing faces → None = 6010/
        //    6013 contention, never adjudicated here).
        if self.scan.rail_face(net).is_some() || self.scan.has_source_root(self.table, net) {
            return Reach {
                has_supply: true,
                nominal: self.scan.net_nominal(self.table, net),
            };
        }
        // 2. rootless — walk transparent copper, then module boundaries.
        self.resolve_rootless(net)
    }

    /// §7 L4 priority 2 — no supply root on the net: transparent-copper or
    /// module-boundary reach to an upstream root, OR-across every leg.
    fn resolve_rootless(&mut self, net: &NetEntry) -> Reach {
        // Seed: return/reference copper never feeds and is never climbed — the
        // §7 L4 distinction that keeps a decoupling cap (structurally identical
        // to a feed ferrite at the flat layer) from masquerading as a source. A
        // demand sink on such copper is a miswire and stays 6019-visible.
        if self.role_excluded(net.id) {
            return Reach::NOT_FED;
        }

        // Copper arm (mirrors budget.rs §6.3): every two-pin element with no DC
        // rows on this net (fuse/inductor/ferrite/decoupling cap) forwards its
        // other net's reach. Return/reference hops are skipped — a cap to GND
        // is not a feed. Unlike budget's first-wins, all legs are tried and the
        // first *fed* leg governs (its nominal rides along for 6011).
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
            let Some(onet) = other else {
                continue;
            };
            if self.role_excluded(onet) {
                continue;
            }
            let r = self.reach_of(onet);
            if r.has_supply {
                return r;
            }
        }

        // Module-boundary arm (mirrors budget.rs §6.6): the SAME physical copper
        // across a submodule port is a second NetEntry (both share the junction
        // point id, `module` differs). A fed co-segment feeds this side too —
        // cycle-safe, since a child recursing back here hits the stack guard.
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
                    if self.role_excluded(cid) {
                        continue;
                    }
                    let r = self.reach_of(cid);
                    if r.has_supply {
                        return r;
                    }
                }
            }
        }
        Reach::NOT_FED
    }

    /// Island-role exclusion: `Ret` (a rail's declared return copper) and
    /// `Reference` (a declared conduit no rail returns to) never supply current,
    /// so reach neither starts on them nor ascends through them.
    fn role_excluded(&self, net_id: u32) -> bool {
        self.idx
            .get(net_id)
            .is_some_and(|a| matches!(a.role, NetRole::Ret | NetRole::Reference))
    }
}
