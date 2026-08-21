// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ M9.1: multi-component placement — which side each OTHER component sits on,
//! and which of its pins face inward.
//!
//! ## Why
//!
//! Up to M8 a layer had exactly one real component: the layer anchor. Every
//! other multi-pin box fell through to `TapRole::Sink` and was hung off ONE
//! net's row like an oversized capacitor. M7.6 at least gave it a sane size and
//! a sane pin split, but the shape was still wrong: `speaker`'s `spk` connects
//! to `lpa` through TWO nets on TWO different rows (`_net8` at `VO1`, `_net9` at
//! `VO2`), and a box hung off one row has to drag the other net all the way down
//! and around itself.
//!
//! The right shape is the one a person draws:
//!
//! ```text
//!      away            facing        facing            away
//!        │                │            │                 │
//!    ┌───┴────┐           │        ┌───┴──────────────┐  │
//!  ──┤3  spk  1├──────────┼────────┤5      lpa      1 ├──┘
//!  ──┤4       2├──────────┴────────┤8                 │
//!    └────────┘                    └──────────────────┘
//!       GND                     the two shared nets run straight
//!    (spk-only,                 across, one per row
//!     pushed out)
//! ```
//!
//! i.e. **pins that talk to another component go BETWEEN the two components;
//! pins that talk to nobody else get pushed out to the far side.** `spk.3/4`
//! carry a ground net that touches nothing but `spk`, so they belong on the
//! outside; `spk.1/2` carry the shared nets, so they belong on the side facing
//! `lpa`, each on its own net's row, and each wire is then a straight line.
//!
//! ## The walk
//!
//! Components, not nets, are the nodes: BFS out from the layer anchor, depth 0 →
//! 1 → 2. A component's side comes from the REGION of the nets it shares with
//! its parent, so the existing W/E/N/S split of the anchor keeps driving the
//! whole picture outward.
//!
//! **Ground nets are not edges.** Every component shares ground with every other
//! one, so letting ground link the graph would make the component graph complete
//! and the BFS order meaningless.
//!
//! ## Contract
//!
//! Pure topology, like [`super::equi_column`] and [`super::equi_chain`]: no
//! rect, no slot, no lane. The caller turns a [`Satellite`] into geometry.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// One multi-pin component as the placer sees it.
#[derive(Debug, Clone)]
pub struct CompView {
    pub box_id: i64,
    /// `(pin id, net index)` for every pin that is in a net, in physical pin
    /// order. Pins in no net are simply absent — they end up in [`Satellite::away`].
    pub pins: Vec<(i64, usize)>,
}

/// A component placed relative to a shallower one.
#[derive(Debug, Clone)]
pub struct Satellite {
    pub box_id: i64,
    /// Hops from the layer anchor (the anchor itself is not a satellite, so
    /// this is always >= 1).
    pub depth: usize,
    /// The shallower component this one hangs off.
    pub parent: i64,
    /// Net indices shared with `parent`, ascending.
    pub shared: Vec<usize>,
    /// Pins on a shared net — or, ★ M14.1, on a net BRIDGED to one through a
    /// two-pin part. They face `parent`, each on its own net's row.
    pub facing: Vec<i64>,
    /// Everything else, pushed to the far side: ground pins, pins on nets that
    /// reach no other component, and unconnected pins.
    pub away: Vec<i64>,
    /// ★ M14.1: the facing nets that are NOT in `shared` — reached only through
    /// a bridge. Their rows and their members belong in the gap between the two
    /// components, so the caller needs to tell them apart from `shared`.
    pub bridged: Vec<usize>,
}

/// BFS the component graph out from `anchor`.
///
/// Deterministic throughout: neighbours are visited in `box_id` order and nets
/// in index order, so two runs over the same netlist produce the same plan.
///
/// ★ M14.1: `bridged` is the two-pin relation — `(net a, net b)` for every
/// two-pin part joining two distinct non-ground nets. A satellite pin whose net
/// is reachable from a SHARED net through that relation faces the parent too:
///
/// ```text
///   mic.1 (MIC.P) ── wm7121.1        shared  → facing
///   mic.2 (MIC.N) ──[C1]── MIC.P     bridged → facing   ★ M14.1
///   mic.3 (_net2) ──[D1]── ⏚        neither → away
/// ```
///
/// Without it `mic.2` is "not shared", so the differential pair is split across
/// the microphone and `C1` — the one part that ties the two halves together —
/// has to reach around the box to close.
pub fn plan_satellites(
    comps: &[CompView],
    net_is_ground: &[bool],
    bridged: &[(usize, usize)],
    anchor: i64,
) -> Vec<Satellite> {
    let is_ground = |n: usize| net_is_ground.get(n).copied().unwrap_or(false);

    // net index → the components carrying it (ground nets excluded: they touch
    // everything and would make the graph complete).
    let mut by_net: BTreeMap<usize, BTreeSet<i64>> = BTreeMap::new();
    for c in comps {
        for &(_, n) in &c.pins {
            if !is_ground(n) {
                by_net.entry(n).or_default().insert(c.box_id);
            }
        }
    }
    let comp_of = |id: i64| comps.iter().find(|c| c.box_id == id);

    let mut depth: BTreeMap<i64, usize> = BTreeMap::new();
    depth.insert(anchor, 0);
    let mut out: Vec<Satellite> = Vec::new();
    let mut queue: VecDeque<i64> = VecDeque::new();
    queue.push_back(anchor);

    while let Some(cur) = queue.pop_front() {
        let d = depth[&cur];
        let Some(cc) = comp_of(cur) else { continue };

        // Neighbours through non-ground nets, in box_id order.
        let mut nets: Vec<usize> = cc.pins.iter().map(|&(_, n)| n).collect();
        nets.sort_unstable();
        nets.dedup();
        let mut neighbours: BTreeSet<i64> = BTreeSet::new();
        for &n in &nets {
            if is_ground(n) {
                continue;
            }
            if let Some(set) = by_net.get(&n) {
                for &other in set {
                    if other != cur && !depth.contains_key(&other) {
                        neighbours.insert(other);
                    }
                }
            }
        }

        for other in neighbours {
            if depth.contains_key(&other) {
                continue; // claimed by an earlier neighbour in this same round
            }
            let Some(oc) = comp_of(other) else { continue };
            let shared: Vec<usize> = {
                let mut s: Vec<usize> = oc
                    .pins
                    .iter()
                    .map(|&(_, n)| n)
                    .filter(|&n| !is_ground(n) && nets.binary_search(&n).is_ok())
                    .collect();
                s.sort_unstable();
                s.dedup();
                s
            };
            if shared.is_empty() {
                continue;
            }
            // ★ M14.1: close the shared set under the two-pin relation, but only
            // over nets THIS component actually carries — a bridge two hops away
            // on some third box says nothing about where these pins belong.
            let mine: BTreeSet<usize> = oc.pins.iter().map(|&(_, n)| n).collect();
            let mut reach: BTreeSet<usize> = shared.iter().copied().collect();
            loop {
                let before = reach.len();
                for &(a, b) in bridged {
                    if is_ground(a) || is_ground(b) {
                        continue;
                    }
                    if reach.contains(&a) && mine.contains(&b) {
                        reach.insert(b);
                    }
                    if reach.contains(&b) && mine.contains(&a) {
                        reach.insert(a);
                    }
                }
                if reach.len() == before {
                    break;
                }
            }
            let mut facing: Vec<i64> = Vec::new();
            let mut away: Vec<i64> = Vec::new();
            for &(pid, n) in &oc.pins {
                if reach.contains(&n) && !is_ground(n) {
                    facing.push(pid);
                } else {
                    away.push(pid);
                }
            }
            let bridged_nets: Vec<usize> = reach
                .iter()
                .copied()
                .filter(|n| !shared.contains(n))
                .collect();
            depth.insert(other, d + 1);
            out.push(Satellite {
                box_id: other,
                depth: d + 1,
                parent: cur,
                shared,
                facing,
                away,
                bridged: bridged_nets,
            });
            queue.push_back(other);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(box_id: i64, pins: &[(i64, usize)]) -> CompView {
        CompView {
            box_id,
            pins: pins.to_vec(),
        }
    }

    /// The `speaker` case this module exists for.
    ///
    /// ```text
    ///   lpa (1)  pins 105 → _net8, 108 → _net9, 106 → VDD, 107 → GND, ...
    ///   spk (2)  pins 201 → _net9, 202 → _net8, 203/204 → GND
    /// ```
    ///
    /// `spk` must land next to `lpa` with 201/202 facing it and the two ground
    /// pins pushed out — NOT hung off one row with the other net dragged around.
    #[test]
    fn speaker_spk_faces_lpa() {
        // nets: 0 = _net8, 1 = _net9, 2 = VDD, 3 = GND(lpa), 4 = GND(spk)
        let ground = [false, false, false, true, true];
        let comps = vec![
            comp(1, &[(105, 0), (108, 1), (106, 2), (107, 3)]),
            comp(2, &[(201, 1), (202, 0), (203, 4), (204, 4)]),
        ];
        let sats = plan_satellites(&comps, &ground, &[], 1);

        assert_eq!(sats.len(), 1);
        let s = &sats[0];
        assert_eq!((s.box_id, s.depth, s.parent), (2, 1, 1));
        assert_eq!(s.shared, vec![0, 1], "the two signal nets link them");
        assert_eq!(s.facing, vec![201, 202], "shared pins face lpa");
        assert_eq!(s.away, vec![203, 204], "spk's own ground goes outward");
    }

    /// ★ M14.1 `mic`: the microphone hangs off `wm7121` through `MIC.P` alone,
    /// but `mic.2` is bridged to `mic.1` by `C1`. Both pins therefore face the
    /// parent, and the bridged net is reported separately so the caller can put
    /// its wire in the GAP rather than on the shared row.
    ///
    /// ```text
    ///   nets: 0 = MIC.P (shared)   1 = MIC.N (bridged via C1)
    ///         2 = _net2            3 = _net3            4 = ground
    /// ```
    #[test]
    fn mic_bridged_pin_faces_the_parent() {
        let ground = [false, false, false, false, true];
        let comps = vec![
            comp(1, &[(101, 0), (102, 4), (103, 4), (104, 5)]), // wm7121 (anchor)
            comp(2, &[(201, 0), (202, 1), (203, 2), (204, 3)]), // mic
        ];
        let bridged = [(0usize, 1usize)]; // C1 between MIC.P and MIC.N

        let plain = plan_satellites(&comps, &ground, &[], 1);
        assert_eq!(plain[0].facing, vec![201], "without the bridge, only MIC.P");
        assert_eq!(plain[0].away, vec![202, 203, 204]);

        let sats = plan_satellites(&comps, &ground, &bridged, 1);
        assert_eq!(sats.len(), 1);
        let s = &sats[0];
        assert_eq!(s.shared, vec![0], "only MIC.P is literally shared");
        assert_eq!(s.facing, vec![201, 202], "★ MIC.N comes along through C1");
        assert_eq!(s.away, vec![203, 204], "the two ESD legs stay outward");
        assert_eq!(s.bridged, vec![1], "MIC.N is reached, not shared");
    }

    /// A bridge to a net this component does NOT carry proves nothing about
    /// where its pins go.
    #[test]
    fn a_bridge_elsewhere_does_not_pull_a_pin() {
        let ground = [false, false, false, true];
        let comps = vec![
            comp(1, &[(101, 0), (102, 3)]),
            comp(2, &[(201, 0), (202, 1), (203, 3)]),
        ];
        // net 1 is bridged to net 2, and comp 2 carries neither 2 nor a path to it.
        let sats = plan_satellites(&comps, &ground, &[(1, 2)], 1);
        assert_eq!(sats[0].facing, vec![201]);
        assert_eq!(sats[0].away, vec![202, 203]);
    }

    /// Ground must not be an edge: two components that share NOTHING but ground
    /// are not neighbours, otherwise every component on the layer would be a
    /// depth-1 satellite of the anchor.
    #[test]
    fn ground_is_not_an_edge() {
        let ground = [true];
        let comps = vec![comp(1, &[(101, 0)]), comp(2, &[(201, 0)])];
        assert!(plan_satellites(&comps, &ground, &[], 1).is_empty());
    }

    /// Depth 2: `u3` reaches the anchor only through `u2`, so it hangs off `u2`.
    #[test]
    fn walks_outward_by_depth() {
        // nets: 0 links u1-u2, 1 links u2-u3
        let ground = [false, false];
        let comps = vec![
            comp(1, &[(101, 0)]),
            comp(2, &[(201, 0), (202, 1)]),
            comp(3, &[(301, 1)]),
        ];
        let sats = plan_satellites(&comps, &ground, &[], 1);
        assert_eq!(sats.len(), 2);
        assert_eq!((sats[0].box_id, sats[0].depth, sats[0].parent), (2, 1, 1));
        assert_eq!((sats[1].box_id, sats[1].depth, sats[1].parent), (3, 2, 2));
        // u2's pin toward u3 is not "facing" u1 — it is on the far side.
        assert_eq!(sats[0].facing, vec![201]);
        assert_eq!(sats[0].away, vec![202]);
    }

    /// A component reachable two ways keeps the FIRST (lowest box_id) parent, so
    /// the plan does not depend on net ordering.
    #[test]
    fn shared_component_takes_one_parent() {
        let ground = [false, false, false];
        let comps = vec![
            comp(1, &[(101, 0), (102, 1)]),
            comp(2, &[(201, 0), (202, 2)]),
            comp(3, &[(301, 1), (302, 2)]),
        ];
        let sats = plan_satellites(&comps, &ground, &[], 1);
        assert_eq!(sats.len(), 2);
        for s in &sats {
            assert_eq!(s.parent, 1, "both hang off the anchor, not off each other");
            assert_eq!(s.depth, 1);
        }
    }
}
