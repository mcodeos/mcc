// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ Equipotential net coalescing — turn "one connection = one net" into real nodes.
//!
//! ## The problem this file solves
//! `fromblock.rs` builds **one `VizNet` per block net** ("Original: one VizNet per net",
//! `fromblock.rs:1280`). Whether two block nets that touch the *same physical pin* were
//! already merged upstream depends on which builder path produced them:
//!
//! * `mc_net.rs::NetTable` — real union-find, merges them. c07's netlist arrives merged
//!   (`N1 : RES1.2 ~ RES3.1 ~ CAP2.1`, three points on one net).
//! * `visit.rs::build_nets_from_connections` — groups **by net name**, and its FIX-B
//!   cross-net merge only fires for `InstKind::Pin` endpoints. Anonymous components
//!   (`@RES1.1`) reach `ConnPair` through owner-backfill as a *component* id, so FIX-B
//!   deliberately skips them.
//!
//! Result on the SP golden netlist: nine 2-point nets where `@RES1.1` sits on **both**
//! `__net_5` and `__net_7`. Every topology model downstream (`sp_model`, `ladder_model`,
//! `chain`, `trunk_tap`) assumes "net == equipotential node" and therefore mis-reads the
//! graph — `sp_model` sees `RES1` touching 3 nets and bails `PassiveNetCount{nets:3}`.
//!
//! ## What this pass does
//! Union-find over net indices keyed on `(box_id, pin_id)`, then **rewrites `graph.nets`**
//! so each equipotential node is a single multi-endpoint `VizNet`. Electrically a no-op;
//! structurally it restores the invariant the whole layout stack is written against.
//!
//! Doing it as a real rewrite (instead of a private map inside `sp_model`) matters:
//! `dispatch.rs:303` decides "already routed?" **per net**, so a node split across three
//! nets can only ever get one of them routed by a deterministic placer — the other two get
//! drawn again by the generic router (duplicate wires), and handing them an empty `Route`
//! trips `feedback.rs:232` `EmptyRoute / Hard`. One node, one net, one route.
//!
//! ## Guards (learned from `visit.rs` FIX-B's catastrophic 18-net merge)
//! * synthesized endpoints (`pin_id == -1`, see `netdef.rs:200`) never key a merge;
//! * endpoints on rail / `PowerLabel` boxes never key a merge — `rails.rs` explodes those
//!   into per-consumer flags on purpose and this pass must not fight it;
//! * endpoints whose `box_id` is not a live box in this layer never key a merge.
//!
//! Nets that share no keyed endpoint are left byte-identical, so a netlist that already
//! arrived merged (c07) passes through untouched.

use std::collections::HashMap;

use crate::vector::graph::netdef::{EndpointRef, VizNet};
use crate::vector::graph::{BoxKind, McVecGraph, NetKind};

use super::rails::is_rail_box;

// ============================================================================
// Public entry
// ============================================================================

/// Merge nets that share a physical pin into one multi-endpoint net.
///
/// Returns the number of nets that disappeared (`0` = nothing to do). Idempotent:
/// running it twice is a no-op the second time.
pub fn coalesce_equipotential_nets(graph: &mut McVecGraph) -> usize {
    let before = graph.nets.len();
    if before < 2 {
        return 0;
    }

    // ── which boxes may key a merge ─────────────────────────────────────────
    let mergeable_box: HashMap<i64, bool> = graph
        .boxes
        .iter()
        .map(|b| (b.id, !is_rail_box(b) && b.kind != BoxKind::PowerLabel))
        .collect();
    let can_key = |e: &EndpointRef| -> bool {
        e.pin_id >= 0 && *mergeable_box.get(&e.box_id).unwrap_or(&false)
    };

    // ── union-find over net indices, keyed on (box_id, pin_id) ──────────────
    let mut dsu = Dsu::new(before);
    let mut first_seen: HashMap<(i64, i64), usize> = HashMap::new();
    for (ni, net) in graph.nets.iter().enumerate() {
        for e in &net.endpoints {
            if !can_key(e) {
                continue;
            }
            match first_seen.get(&(e.box_id, e.pin_id)) {
                Some(&other) => dsu.union(other, ni),
                None => {
                    first_seen.insert((e.box_id, e.pin_id), ni);
                }
            }
        }
    }

    // ── group members by root, keeping first-appearance order stable ────────
    let mut order: Vec<usize> = Vec::new(); // roots, in order of first member
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for ni in 0..before {
        let r = dsu.find(ni);
        let slot = members.entry(r).or_default();
        if slot.is_empty() {
            order.push(r);
        }
        slot.push(ni);
    }
    if order.len() == before {
        return 0; // nothing shares a pin — leave the graph byte-identical
    }

    // ── rebuild ─────────────────────────────────────────────────────────────
    let old = std::mem::take(&mut graph.nets);
    let mut out: Vec<VizNet> = Vec::with_capacity(order.len());
    for root in order {
        let idxs = &members[&root];
        if idxs.len() == 1 {
            out.push(old[idxs[0]].clone());
            continue;
        }

        // endpoints: union, deduped on (box_id, pin_id), first occurrence wins
        let mut endpoints: Vec<EndpointRef> = Vec::new();
        for &i in idxs {
            for e in &old[i].endpoints {
                if !endpoints
                    .iter()
                    .any(|k| k.box_id == e.box_id && k.pin_id == e.pin_id)
                {
                    endpoints.push(e.clone());
                }
            }
        }

        // name: the most informative one (a real signal name beats `__net_N`);
        // ties break on the lowest member index so the result is deterministic.
        let name = idxs
            .iter()
            .map(|&i| &old[i].name)
            .find(|n| is_informative(n))
            .cloned()
            .unwrap_or_else(|| old[idxs[0]].name.clone());

        // kind: a rail/bus classification outranks plain Signal.
        let kind = idxs
            .iter()
            .map(|&i| old[i].kind.clone())
            .find(|k| !matches!(k, NetKind::Signal))
            .unwrap_or(NetKind::Signal);

        let nid = idxs.iter().map(|&i| old[i].nid).min().unwrap_or(0);
        let mut merged = VizNet::new(nid, name, kind, endpoints);
        merged.src_pos = idxs.iter().find_map(|&i| old[i].src_pos);

        crate::vlog!(
            "[coalesce] node '{}' ← {} net(s): {:?}",
            merged.name,
            idxs.len(),
            idxs.iter()
                .map(|&i| old[i].name.as_str())
                .collect::<Vec<_>>()
        );
        out.push(merged);
    }

    graph.nets = out;
    let removed = before - graph.nets.len();
    crate::vlog!(
        "[coalesce] layer '{}' bid={}: {} net(s) → {} equipotential node(s)",
        graph.name,
        graph.bid,
        before,
        graph.nets.len()
    );
    removed
}

/// A name carries information when it is not an auto-generated `__net_N`.
fn is_informative(name: &str) -> bool {
    !name.starts_with("__net") && !name.is_empty()
}

// ============================================================================
// Union-find
// ============================================================================

struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // lower index wins so the surviving root matches first-appearance order
            if ra < rb {
                self.parent[rb] = ra;
            } else {
                self.parent[ra] = rb;
            }
        }
    }
}

// ============================================================================
// Tests — the RAW netlist, exactly as the builder emits it
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::graph::boxdef::IoSummary;
    use crate::vector::graph::netdef::{EndpointRef, VizNet};
    use crate::vector::graph::{McVecBox, McVecGraph, NetKind, Symbol};
    use crate::viz::layout::sp_model::build_sp_model;

    fn term(id: i64, name: &str, outputs: usize) -> McVecBox {
        let mut io = IoSummary::new();
        io.outputs = outputs;
        McVecBox::new_v2(
            id,
            name.into(),
            "".into(),
            BoxKind::TwoPin,
            Symbol::Ic,
            None,
            None,
            1,
            io,
        )
    }
    fn passive(id: i64, name: &str, class: &str, sym: Symbol) -> McVecBox {
        McVecBox::new_v2(
            id,
            name.into(),
            class.into(),
            BoxKind::TwoPin,
            sym,
            Some(name.into()),
            None,
            2,
            IoSummary::new(),
        )
    }
    fn net(nid: i64, name: &str, eps: &[(i64, i64)]) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            NetKind::Signal,
            eps.iter()
                .map(|&(b, p)| EndpointRef::new(b, p, format!("p{p}")))
                .collect(),
        )
    }

    /// The netlist **as dumped by the real pipeline**: 9 connections → 9 two-point nets.
    /// pin ids follow `box_id * 10 + terminal`, so `RES1.1` = (1, 11).
    fn raw_golden() -> McVecGraph {
        let mut g = McVecGraph::new(1, "main".into());
        g.boxes.push(passive(1, "R1", "RES", Symbol::Resistor));
        g.boxes.push(passive(2, "C2", "CAP", Symbol::Capacitor));
        g.boxes.push(passive(3, "R3", "RES", Symbol::Resistor));
        g.boxes.push(passive(4, "R4", "RES", Symbol::Resistor));
        g.boxes.push(passive(5, "C5", "CAP", Symbol::Capacitor));
        g.boxes.push(passive(6, "R6", "RES", Symbol::Resistor));
        g.boxes.push(term(101, "u1", 1));
        g.boxes.push(term(102, "u2", 0));

        g.nets.push(net(0, "__net_0", &[(1, 12), (2, 21)])); // RES1.2 ~ CAP2.1
        g.nets.push(net(1, "__net_1", &[(4, 42), (5, 51)])); // RES4.2 ~ CAP5.1
        g.nets.push(net(2, "__net_2", &[(4, 41), (6, 61)])); // RES4.1 ~ RES6.1
        g.nets.push(net(3, "__net_3", &[(5, 52), (6, 62)])); // CAP5.2 ~ RES6.2
        g.nets.push(net(4, "__net_4", &[(3, 32), (4, 41)])); // RES3.2 ~ RES4.1
        g.nets.push(net(5, "__net_5", &[(1, 11), (3, 31)])); // RES1.1 ~ RES3.1
        g.nets.push(net(6, "__net_6", &[(2, 22), (5, 52)])); // CAP2.2 ~ CAP5.2
        g.nets.push(net(7, "__net_7", &[(101, 6), (1, 11)])); // u1.6   ~ RES1.1
        g.nets.push(net(8, "__net_8", &[(2, 22), (102, 6)])); // CAP2.2 ~ u2.6
        g
    }

    #[test]
    fn raw_golden_coalesces_to_five_nodes() {
        let mut g = raw_golden();
        let removed = coalesce_equipotential_nets(&mut g);
        assert_eq!(removed, 4, "9 nets → 5 nodes");
        assert_eq!(g.nets.len(), 5);

        // node A = {__net_5, __net_7} : u1.6, R1.1, R3.1
        let node_of = |bid: i64, pid: i64| -> usize {
            g.nets
                .iter()
                .position(|n| {
                    n.endpoints
                        .iter()
                        .any(|e| e.box_id == bid && e.pin_id == pid)
                })
                .expect("pin must live on some node")
        };
        assert_eq!(
            node_of(101, 6),
            node_of(1, 11),
            "u1.6 and R1.1 are one node"
        );
        assert_eq!(node_of(1, 11), node_of(3, 31), "R3.1 joins the same node");
        assert_eq!(
            node_of(2, 22),
            node_of(102, 6),
            "C2.2 and u2.6 are one node"
        );
        assert_eq!(node_of(2, 22), node_of(5, 52), "C5.2 joins the same node");
        assert_eq!(node_of(4, 41), node_of(3, 32), "R4.1 and R3.2 are one node");
        // and no endpoint got lost
        let total: usize = g.nets.iter().map(|n| n.endpoints.len()).sum();
        assert_eq!(total, 14, "6 passives × 2 + 2 terminals");
    }

    /// ★ The regression that matters: the RAW netlist must reach the golden SP tree.
    /// Before this pass it bailed `PassiveNetCount{ box_id: 1, nets: 3 }`.
    #[test]
    fn raw_golden_builds_the_sp_model_after_coalescing() {
        let mut g = raw_golden();
        assert!(
            build_sp_model(&g).is_err(),
            "raw (uncoalesced) input is expected to bail — that is the bug being fixed"
        );
        coalesce_equipotential_nets(&mut g);
        let m = build_sp_model(&g).expect("coalesced input must be series-parallel");
        assert_eq!(m.root.expr(), "(R1 + C2) ∥ (R3 + ((R4 + C5) ∥ R6))");
        assert_eq!(m.root.size(), (3.0, 3.0));
        assert_eq!(m.left_box, 101);
        assert_eq!(m.right_box, 102);
    }

    #[test]
    fn already_merged_netlist_is_untouched() {
        // c07-shaped input: nets already carry 3 points, nothing shares a pin across nets
        let mut g = McVecGraph::new(1, "main".into());
        g.boxes.push(passive(1, "R1", "RES", Symbol::Resistor));
        g.boxes.push(passive(2, "C2", "CAP", Symbol::Capacitor));
        g.boxes.push(term(101, "u1", 1));
        g.nets.push(net(0, "N0", &[(101, 6), (1, 11)]));
        g.nets.push(net(1, "N1", &[(1, 12), (2, 21)]));
        let before = g.nets.len();
        assert_eq!(coalesce_equipotential_nets(&mut g), 0);
        assert_eq!(g.nets.len(), before);
    }

    #[test]
    fn idempotent() {
        let mut g = raw_golden();
        coalesce_equipotential_nets(&mut g);
        let n = g.nets.len();
        assert_eq!(
            coalesce_equipotential_nets(&mut g),
            0,
            "second run is a no-op"
        );
        assert_eq!(g.nets.len(), n);
    }
}
