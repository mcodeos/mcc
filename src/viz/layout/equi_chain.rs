// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ M8.1: chain analysis — a two-pin part's ORIENTATION, decided by the
//! netlist alone.
//!
//! ## Why this module exists
//!
//! Up to M7 orientation was a by-product of the row allocator:
//! `tap_role` compared the partner net's ROW against this net's ROW, and rows
//! were already frozen by `assign_rows`. Deciding the row first and reading the
//! direction off it afterwards leaves the direction no freedom at all — the M3
//! landing note records the consequence in one line: a true `Series` (two nets
//! on one row) is *structurally unreachable under the RowAllocator*. So every
//! two-pin part in the device layer
//! comes out VERTICAL, which wastes a row per part and stretches chains that
//! should read as one straight wire.
//!
//! M8 turns the dependency around:
//!
//! ```text
//!   M7:  assign_rows ──► tap_role (row delta) ──► place
//!   M8:  [chain analysis] ──► assign_rows (knows who shares a row) ──► place
//!             ^ this module
//! ```
//!
//! ## The rule
//!
//! Think of every anchor pin as growing a horizontal RUN outward: the parts
//! strung off that pin extend the pin's electrical identity, so the far end of
//! the run *acts as that pin*. Two such runs meeting through a part means that
//! part joins two different pins — it is a bridge and must be drawn ACROSS the
//! rows.
//!
//! ```text
//!   moddcdc, East side
//!     LX(3) ──[L1]── VCC_1V2 ── C3 ┐          L1 : Along  (LX's run)
//!                        │         ├─ GND     C3 : Shunt
//!                        │      C4 ┘          C4 : Shunt
//!                       [R2]                  R2 : Across (LX's run ↔ FB)
//!                        │
//!     FB(5) ─────────────┴──[R3]── GND        R3 : Shunt
//!        └──[C5]── LX                         C5 : Across (FB ↔ LX)
//! ```
//!
//! `VCC_1V2` carries only a label, so it has no pin identity of its own; it
//! inherits `LX`'s by being claimed by `LX`'s run. `R2` then sees a pin on one
//! side and a pin-plus-label on the other, and "two pins wins" makes it Across.
//!
//! **Which run claims a shared net is load-bearing.** `LX` and `FB` are both one
//! hop from `VCC_1V2`; if `FB` claimed it first, `R2` would come out horizontal
//! and `L1` vertical — the mirror image of the right answer. Runs therefore grow
//! from the strongest DRIVER first ([`NetView::anchor_pin`] rank: Power 4,
//! Output 3, Bidir 2, Input 1). A run is power/signal flowing out of a pin; a
//! sense input receives, it does not drive one.
//!
//! ## Contract
//!
//! Pure topology, like [`super::equi_column`]: no rect (`x/y/w/h`), no slot, no
//! lane is read or written. The A2 guard (layout ≡ render replay) therefore
//! cannot be affected by anything in here.

use std::collections::{BTreeMap, VecDeque};

/// How a two-pin part is drawn relative to the row (trunk) it hangs off.
///
/// The names are deliberately relative to the trunk rather than to the page: in
/// the M1 row model trunks are horizontal, so `Along` reads as "horizontal" and
/// `Across` as "vertical"; when a vertical-trunk axis arrives the whole picture
/// mirrors and the classification is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartOrientation {
    /// ALONG the trunk. The part extends one endpoint outward, so both of its
    /// nets sit on the SAME row and their trunk spans meet at its two pins —
    /// one straight wire with a component inserted in it. Body parallel to the
    /// trunk, pins facing along it.
    Along,
    /// ACROSS the rows. The part joins two different endpoint regions, so its
    /// nets are on different rows. Body perpendicular to the trunk.
    Across,
    /// A shunt to ground. Perpendicular, hanging toward the ground rail.
    Shunt,
}

/// One two-pin part, as the analyser sees it.
#[derive(Debug, Clone)]
pub struct PartView {
    pub box_id: i64,
    /// Indices into the `nets` slice of the two nets this part joins.
    pub nets: (usize, usize),
}

/// One net, as the analyser sees it.
#[derive(Debug, Clone, Default)]
pub struct NetView {
    /// `(io rank, pin id)` of the layer-anchor pin this net owns, if any.
    /// Rank: Power 4, Output 3, Bidir 2, Input 1, anything else 0 — higher
    /// drives harder and therefore grows its run first. `None` means the net
    /// does not touch the layer anchor.
    pub anchor_pin: Option<(u8, i64)>,
    /// Ground nets never form a region: a part touching one is a [`Shunt`].
    ///
    /// [`Shunt`]: PartOrientation::Shunt
    pub is_ground: bool,
    /// This net is an ENDPOINT in its own right: the netlist puts an explicit
    /// label / port box on it, or it is a power rail.
    ///
    /// ★ M8.6 — two jobs. It seeds nets that no pin can reach (an isolated
    /// island still gets a deterministic root), and it **stops a run**: an
    /// endpoint is where the wire is named and where the label glyph is drawn,
    /// so parts hanging off it are hanging off the END of the run, not
    /// extending it. See [`analyse`].
    ///
    /// Auto-named internal nodes (`_net7`) are NOT endpoints — they are just
    /// the wire between two parts, which is what lets a multi-part series chain
    /// stay one run.
    pub is_endpoint: bool,
}

/// The analysis result.
#[derive(Debug, Clone, Default)]
pub struct ChainPlan {
    /// net idx → the seed net index whose run claimed it. `None` for ground
    /// nets (they belong to the rail, not to a run).
    pub region: Vec<Option<usize>>,
    /// net idx → hops from its region's root (`0` = the root itself).
    /// The column allocator orders a row's nets by this: depth `d` sits
    /// strictly further out than depth `d - 1`.
    pub depth: Vec<usize>,
    /// part `box_id` → orientation.
    pub orientation: BTreeMap<i64, PartOrientation>,
}

impl ChainPlan {
    /// Do these two nets share a row? True exactly when one run claimed both.
    /// This is the predicate `assign_rows` needs (M8.2): an `Along` part's two
    /// nets are collinear, so they must land on the same band.
    pub fn shares_row(&self, a: usize, b: usize) -> bool {
        match (self.region.get(a), self.region.get(b)) {
            (Some(Some(ra)), Some(Some(rb))) => ra == rb,
            _ => false,
        }
    }

    /// Orientation of a part, defaulting to [`PartOrientation::Shunt`] for an
    /// unknown box (the conservative choice — it is the M7 behaviour).
    pub fn orientation_of(&self, box_id: i64) -> PartOrientation {
        self.orientation
            .get(&box_id)
            .copied()
            .unwrap_or(PartOrientation::Shunt)
    }
}

/// Classify every part. Deterministic: seeds are ordered by `(rank desc, pin id
/// asc)`, the fallback seeds by net index, and each BFS walks its parts in
/// `box_id` order.
pub fn analyse(nets: &[NetView], parts: &[PartView]) -> ChainPlan {
    let n = nets.len();
    let mut region: Vec<Option<usize>> = vec![None; n];
    let mut depth: Vec<usize> = vec![0; n];

    // net idx → the parts touching it, in box_id order (determinism).
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by_key(|&p| parts[p].box_id);
    for &p in &order {
        let (a, b) = parts[p].nets;
        if a < n {
            incident[a].push(p);
        }
        if b < n && b != a {
            incident[b].push(p);
        }
    }

    // ── Step 1: every pin net is its OWN region, claimed up front ────────────
    // Claiming before any BFS runs is what keeps a part between two pin nets an
    // `Across`: were the seeds claimed lazily, the first BFS would swallow the
    // next seed and `moddcdc`'s `R1` (Vin ↔ EN) would come out horizontal.
    let mut seeds: Vec<usize> = (0..n)
        .filter(|&i| !nets[i].is_ground && nets[i].anchor_pin.is_some())
        .collect();
    seeds.sort_by_key(|&i| {
        let (rank, pin) = nets[i].anchor_pin.unwrap_or((0, 0));
        (std::cmp::Reverse(rank), pin, i)
    });
    for &s in &seeds {
        region[s] = Some(s);
    }

    // ── Step 2: grow each run, strongest driver first ────────────────────────
    for &s in &seeds {
        let mut queue: VecDeque<usize> = VecDeque::new();
        queue.push_back(s);
        while let Some(cur) = queue.pop_front() {
            // ★ M8.6: a run STOPS when it ARRIVES at an endpoint. `VCC_1V2` is
            // reached through `L1` — so `L1` lies along the row — but the rail is
            // where the wire is named and where its label is drawn, so `C3`/`C4`
            // hang OFF that end rather than extending it further outward. The
            // test is "arrived at", not "is an endpoint": a run's own ROOT is
            // very often labelled too (`US_SPEAKER_MUTE` carries both `lpa.1`
            // and a bus label), and it must still grow.
            if cur != s && nets[cur].is_endpoint {
                continue;
            }
            for &p in &incident[cur] {
                let (a, b) = parts[p].nets;
                let other = if a == cur { b } else { a };
                if other >= n || other == cur {
                    continue;
                }
                if nets[other].is_ground || region[other].is_some() {
                    continue;
                }
                region[other] = Some(s);
                depth[other] = depth[cur] + 1;
                queue.push_back(other);
            }
        }
    }

    // ── Step 3: fallback roots for whatever no pin could reach ───────────────
    // Endpoint nets first (an island rail reads better rooted at its label),
    // then anything left, both in index order.
    let unclaimed = |region: &[Option<usize>], i: usize| !nets[i].is_ground && region[i].is_none();
    let roots: Vec<usize> = (0..n)
        .filter(|&i| unclaimed(&region, i) && nets[i].is_endpoint)
        .chain((0..n).filter(|&i| unclaimed(&region, i) && !nets[i].is_endpoint))
        .collect();
    for r in roots {
        if region[r].is_some() {
            continue;
        }
        region[r] = Some(r);
        let mut queue: VecDeque<usize> = VecDeque::new();
        queue.push_back(r);
        while let Some(cur) = queue.pop_front() {
            if cur != r && nets[cur].is_endpoint {
                continue;
            }
            for &p in &incident[cur] {
                let (a, b) = parts[p].nets;
                let other = if a == cur { b } else { a };
                if other >= n || other == cur || nets[other].is_ground {
                    continue;
                }
                if region[other].is_some() {
                    continue;
                }
                region[other] = Some(r);
                depth[other] = depth[cur] + 1;
                queue.push_back(other);
            }
        }
    }

    // ── Step 4: classify ─────────────────────────────────────────────────────
    let mut orientation: BTreeMap<i64, PartOrientation> = BTreeMap::new();
    for p in parts {
        let (a, b) = p.nets;
        let o = if a >= n || b >= n || nets[a].is_ground || nets[b].is_ground {
            PartOrientation::Shunt
        } else {
            match (region[a], region[b]) {
                // Same run — a tree edge, or a parallel sibling of one (a back
                // edge inside one region). Both read as one straight wire, so
                // both are Along; the column model gives the parallel pair the
                // same column span at different offsets, which is how a
                // parallel pair is drawn.
                (Some(ra), Some(rb)) if ra == rb => PartOrientation::Along,
                (Some(_), Some(_)) => PartOrientation::Across,
                _ => PartOrientation::Shunt,
            }
        };
        orientation.insert(p.box_id, o);
    }

    ChainPlan {
        region,
        depth,
        orientation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(rank: u8, id: i64) -> NetView {
        NetView {
            anchor_pin: Some((rank, id)),
            is_ground: false,
            is_endpoint: false,
        }
    }
    fn gnd() -> NetView {
        NetView {
            anchor_pin: None,
            is_ground: true,
            is_endpoint: false,
        }
    }
    fn label() -> NetView {
        NetView {
            anchor_pin: None,
            is_ground: false,
            is_endpoint: true,
        }
    }
    fn pin_labelled(rank: u8, id: i64) -> NetView {
        NetView {
            anchor_pin: Some((rank, id)),
            is_ground: false,
            is_endpoint: true,
        }
    }
    fn plain() -> NetView {
        NetView::default()
    }
    fn part(box_id: i64, a: usize, b: usize) -> PartView {
        PartView {
            box_id,
            nets: (a, b),
        }
    }

    /// The `moddcdc` East side, which is the whole reason this module exists.
    /// Every one of the six answers is pinned here.
    #[test]
    fn moddcdc_orientations() {
        //  0 VDD_3V3 (Vin, Power)   1 _net1 (EN, Input)    2 _net3 (LX, Output)
        //  3 _net5 (FB, Input)      4 VCC_1V2 (label)      5 GND
        let nets = vec![
            pin(4, 104),
            pin(1, 101),
            pin(3, 103),
            pin(1, 105),
            label(),
            gnd(),
        ];
        let parts = vec![
            part(21, 0, 1), // R1  Vin ↔ EN
            part(11, 0, 5), // C1  Vin ↔ GND
            part(12, 1, 5), // C2  EN  ↔ GND
            part(31, 2, 4), // L1  LX  ↔ VCC_1V2
            part(15, 2, 3), // C5  LX  ↔ FB
            part(22, 4, 3), // R2  VCC_1V2 ↔ FB
            part(23, 3, 5), // R3  FB  ↔ GND
            part(13, 4, 5), // C3  VCC_1V2 ↔ GND
            part(14, 4, 5), // C4  VCC_1V2 ↔ GND
        ];
        let plan = analyse(&nets, &parts);

        // L1 extends LX outward: the output rail IS the switch node past the
        // inductor, so VCC_1V2 joins LX's run and L1 lies along the row.
        assert_eq!(plan.orientation_of(31), PartOrientation::Along);
        assert!(plan.shares_row(2, 4));

        // Two pins on either side → across.
        assert_eq!(plan.orientation_of(21), PartOrientation::Across); // R1
        assert_eq!(plan.orientation_of(15), PartOrientation::Across); // C5
                                                                      // R2's far end is VCC_1V2, which now carries LX's pin identity.
        assert_eq!(plan.orientation_of(22), PartOrientation::Across);
        assert!(!plan.shares_row(3, 4));

        for c in [11, 12, 13, 14, 23] {
            assert_eq!(plan.orientation_of(c), PartOrientation::Shunt, "box {c}");
        }
    }

    /// The driver-first seed order is load-bearing: if the sense pin grew its
    /// run first the two answers would swap.
    #[test]
    fn stronger_driver_claims_the_shared_net() {
        let nets = vec![pin(3, 103), pin(1, 105), label()];
        let parts = vec![part(31, 0, 2), part(22, 2, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.region[2], Some(0), "the Output pin claims the rail");
        assert_eq!(plan.orientation_of(31), PartOrientation::Along);
        assert_eq!(plan.orientation_of(22), PartOrientation::Across);

        // Swap the ranks and the picture mirrors exactly.
        let nets = vec![pin(1, 103), pin(3, 105), label()];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.region[2], Some(1));
        assert_eq!(plan.orientation_of(31), PartOrientation::Across);
        assert_eq!(plan.orientation_of(22), PartOrientation::Along);
    }

    /// `A.1~R1.1 / R1.2~R2.1~R3.1 / R2.2~R3.2~R4.1` — series, then a parallel
    /// pair, then series again. All four lie along the row, and the parallel
    /// siblings share their column span.
    #[test]
    fn series_parallel_series_is_all_along() {
        let nets = vec![pin(3, 1), plain(), plain(), label()];
        let parts = vec![
            part(1, 0, 1), // R1
            part(2, 1, 2), // R2 ┐ parallel
            part(3, 1, 2), // R3 ┘
            part(4, 2, 3), // R4
        ];
        let plan = analyse(&nets, &parts);
        for b in 1..=4 {
            assert_eq!(plan.orientation_of(b), PartOrientation::Along, "box {b}");
        }
        assert_eq!(plan.depth, vec![0, 1, 2, 3]);
        assert!(plan.shares_row(0, 3), "the whole chain is one row");
    }

    /// An island with no pin at all still gets a deterministic root, and its
    /// parts stay Along rather than degenerating to Shunt.
    #[test]
    fn island_roots_at_its_label() {
        let nets = vec![plain(), label(), gnd()];
        let parts = vec![part(7, 0, 1), part(8, 0, 2)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.region[1], Some(1), "the labelled net is the root");
        assert_eq!(plan.region[0], Some(1));
        assert_eq!(plan.orientation_of(7), PartOrientation::Along);
        assert_eq!(plan.orientation_of(8), PartOrientation::Shunt);
    }

    /// ★ M8.6 `speaker`: `US_SPEAKER_MUTE ~ _R1.2 ~ lpa.1` and
    /// `VDD_3V3 ~ _R1.1`. One end of `R1` is a pin, the other a bare rail label
    /// with nothing else on it — the resistor extends the pin outward, so it
    /// lies ALONG the row. The pin net is itself labelled (it is a bus), which
    /// must not stop its own run.
    #[test]
    fn pin_to_bare_rail_is_along() {
        let nets = vec![pin_labelled(1, 101), label()];
        let parts = vec![part(1, 0, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert!(plan.shares_row(0, 1));
    }

    /// ★ M8.6: the run ends where the wire is named. `pin → L → rail` is Along,
    /// but a further part off the rail hangs OFF that end — it does not extend
    /// the run another hop outward.
    #[test]
    fn run_stops_at_an_endpoint() {
        //  0 pin net    1 rail (endpoint)    2 some other node
        let nets = vec![pin(3, 103), label(), plain()];
        let parts = vec![part(1, 0, 1), part(2, 1, 2)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert_eq!(
            plan.orientation_of(2),
            PartOrientation::Across,
            "the rail is where the run ends; the next part hangs off it"
        );
        assert!(!plan.shares_row(1, 2));
    }
}
