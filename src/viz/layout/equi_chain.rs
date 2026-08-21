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
//! two-pin part in the device layer came out VERTICAL, which wastes a row per
//! part and stretches chains that should read as one straight wire.
//!
//! M8 turns the dependency around:
//!
//! ```text
//!   M7:  assign_rows ──► tap_role (row delta) ──► place
//!   M8:  [chain analysis] ──► assign_rows (knows who shares a row) ──► place
//!             ^ this module
//! ```
//!
//! ## The rule (M8)
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
//!     FB(5) ─────────────┴──[R3]── GND        R3 : Along  (M10.3, adopted)
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
//! ## ★ M11: the END BUDGET
//!
//! M8 grew a run with a BFS, so one net could hand its identity to *every*
//! unclaimed neighbour at once. That makes a run a TREE, and a tree cannot be
//! drawn on one row: two Along parts off the same net both want to lie on that
//! net's wire, heading the same way. `chain_origins` papered over it by
//! linearising the branches in `(depth, nid)` order — "not pretty, but never
//! overlapping" — which is how a fan-out ended up as a queue of parts marching
//! off the side of the page.
//!
//! The netlist rule that fixes it is the one a person draws by:
//!
//! > **an equipotential point has exactly TWO horizontal things on it: a start
//! > (pin / label / component) and an end (pin / label / component).**
//!
//! So every net gets a two-slot budget, [`NetEnds`]:
//!
//! ```text
//!   LPA.1 ──[R1]── ┬── VIN          net A = LPA.1 ~ R1.1
//!                  ├──[R2]──…       net B = R1.2 ~ R2.1 ~ R3.1 ~ VIN
//!                  └──[R3]──…
//!
//!   net A : inner = AnchorPin(LPA.1)   outer = Part(R1)   ← R1 is horizontal
//!   net B : inner = Part(R1)           outer = Name(VIN)  ← R1 is B's START,
//!                                                           VIN is its END
//!           ⇒ R2 and R3 find B full, and hang off it vertically.
//! ```
//!
//! A run is therefore a PATH, not a tree: one extension per net, chosen by
//! [`best_extension`]. Everything else on that net is a branch and goes
//! vertical. The three other things that can spend an end are all handled the
//! same way — a NAME ([`NetView::is_endpoint`]), a neighbouring COMPONENT
//! ([`NetView::ends_at_component`], the M9 satellite's facing pin), and an
//! adopted GROUND glyph ([`NetView::ground_adoptable`], M10.3).
//!
//! ### What is counted is DIRECTIONS, not parts
//!
//! A net can carry three horizontal parts and still be inside budget. This one
//! does:
//!
//! ```text
//!   lpa.1 ~ r1.1
//!   r1.2 ~ r2.1 ~ r3.1          ← this net touches R1, R2 and R3
//!   r2.2 ~ r3.2 ~ vcc
//!
//!   lpa.1 ──[R1]── ┬──[R2]──┬── VCC        R1, R2, R3 all horizontal
//!                  └──[R3]──┘
//! ```
//!
//! `R2` and `R3` join the SAME pair of nets. They are a **parallel bundle**:
//! one gap between two collinear trunks, spanned by two bodies stacked in y
//! over one column interval. The wire leaves that net ONCE, to the right — so
//! the bundle is one end, and the middle net's budget reads
//! `inner = Part(R1)`, `outer = Part(R2 bundle)`. Full, and correct.
//!
//! The general statement, which covers every case in one line:
//!
//! > Contract each set of parallel parts into a single edge. In the resulting
//! > simple net-graph, the HORIZONTAL edges must form disjoint PATHS — every net
//! > has horizontal degree ≤ 2, counting its anchor pin, its name, and any
//! > satellite on it as one unit of degree each.
//!
//! Which is exactly "one start, one end". [`analyse`] is the greedy that picks
//! those paths (strongest driver first, then the branch that reaches a name),
//! and `equi_audit`'s A31 is the same statement written as a check.
//!
//! ## Contract
//!
//! Pure topology, like [`super::equi_column`]: no rect (`x/y/w/h`), no slot, no
//! lane is read or written. The A2 guard (layout ≡ render replay) therefore
//! cannot be affected by anything in here.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

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

/// ★ M11: what occupies one of a net's two horizontal ends.
///
/// The point of naming the occupant rather than counting a `bool` is that three
/// different passes need to ask three different questions of the same slot:
/// `analyse` asks "may a run extend here", ground adoption asks "is the outer
/// end still free", and the render side asks "may the label glyph lie along the
/// wire, or must it rise off it on a stub" ([`EndUse::blocks_glyph`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndUse {
    /// Nothing claimed it.
    #[default]
    Free,
    /// This net's own pin on the layer anchor — always the INNER end.
    AnchorPin,
    /// An `Along` part (`box_id`): the wire continues through a component.
    Part(i64),
    /// A terminal glyph drawn along the wire — a rail / bus / port label, or a
    /// ground symbol adopted by M10.3.
    Name,
    /// A neighbouring multi-pin COMPONENT's facing pin (M9 satellite). This is
    /// the "ends at a component" branch: the row ends at another box, not at a glyph.
    Component,
}

impl EndUse {
    pub fn is_free(self) -> bool {
        matches!(self, EndUse::Free)
    }

    /// Does this end deny a horizontal terminal glyph? A `Part` or a
    /// `Component` physically occupies the end of the wire, so a label there
    /// would be painted over it — it has to leave on a vertical stub instead
    /// (M10.1). `AnchorPin` blocks nothing: it is the INNER end, and glyphs
    /// hang off the OUTER one.
    pub fn blocks_glyph(self) -> bool {
        matches!(self, EndUse::Part(_) | EndUse::Component)
    }
}

/// ★ M11: the two horizontal ends of one net (equipotential point).
///
/// `inner` faces the layer anchor, `outer` faces away from it. On a West row the
/// inner end is the right-hand one; on an East row the left-hand one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NetEnds {
    pub inner: EndUse,
    pub outer: EndUse,
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
    /// A ground net. It never seeds a region; a part touching one is a
    /// [`Shunt`] unless the ground is ADOPTED as a run's outer end (M10.3).
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
    /// ★ M10.3: this GROUND net may be adopted as a run's outer end, i.e. it is
    /// allowed to behave like a label. The caller clears it for a ground net
    /// that owns a real GND pin on the layer anchor — such a net belongs to the
    /// South rail and its shunts stay vertical. See [`analyse`] step 3.5.
    pub ground_adoptable: bool,
    /// ★ M11.2: this net reaches a SATELLITE component (a non-anchor multi-pin
    /// box, per `equi_place::plan_satellites`). The satellite's facing pin sits
    /// at the outer end of this row (M9.2 pushes it clear of every member), so
    /// the row's outer end is spent before any part gets a look at it.
    ///
    /// This is the "may end at a component" branch of the end rule, and it is
    /// what stops a run from trying to continue horizontally THROUGH a component.
    pub ends_at_component: bool,
}

/// The analysis result.
#[derive(Debug, Clone, Default)]
pub struct ChainPlan {
    /// net idx → the seed net index whose run claimed it. `None` for ground
    /// nets that no run adopted (they belong to the rail, not to a run).
    pub region: Vec<Option<usize>>,
    /// net idx → hops from its region's root (`0` = the root itself).
    /// The column allocator orders a row's nets by this: depth `d` sits
    /// strictly further out than depth `d - 1`.
    pub depth: Vec<usize>,
    /// ★ M11: net idx → what owns each of its two horizontal ends.
    pub ends: Vec<NetEnds>,
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

    /// ★ M11.3: is this net's OUTER end already occupied by something physical
    /// (a part or a neighbouring component)? The render side asks this before
    /// laying a label along the wire — see `equipotential_tree::realize`.
    pub fn outer_end_taken(&self, i: usize) -> bool {
        self.ends.get(i).is_some_and(|e| e.outer.blocks_glyph())
    }

    /// The two ends of a net, or a fully-free pair for an unknown index.
    pub fn ends_of(&self, i: usize) -> NetEnds {
        self.ends.get(i).copied().unwrap_or_default()
    }
}

/// Classify every part. Deterministic throughout: seeds are ordered by
/// `(rank desc, pin id asc)`, fallback seeds by `(is_endpoint desc, index)`,
/// and every choice inside a run breaks ties on `box_id`.
pub fn analyse(nets: &[NetView], parts: &[PartView]) -> ChainPlan {
    let n = nets.len();
    let mut region: Vec<Option<usize>> = vec![None; n];
    let mut depth: Vec<usize> = vec![0; n];
    let mut ends: Vec<NetEnds> = vec![NetEnds::default(); n];

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

    // ── Step 0 (★ M11): the ends the NETLIST fixes, before any run grows ─────
    // An anchor pin is always the inner end of its own net; a satellite always
    // sits at the outer end of the rows it shares with the anchor (M9.2 pushes
    // it past every member on that side).
    for i in 0..n {
        if nets[i].anchor_pin.is_some() {
            ends[i].inner = EndUse::AnchorPin;
        }
        if nets[i].ends_at_component {
            ends[i].outer = EndUse::Component;
        }
    }

    // ── Step 1: every pin net is its OWN region, claimed up front ────────────
    // Claiming before any run grows is what keeps a part between two pin nets an
    // `Across`: were the seeds claimed lazily, the first run would swallow the
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
        grow(
            s,
            nets,
            parts,
            &incident,
            &mut region,
            &mut depth,
            &mut ends,
        );
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
        grow(
            r,
            nets,
            parts,
            &incident,
            &mut region,
            &mut depth,
            &mut ends,
        );
    }

    // ── Step 3.5 (★ M10.3 / M11.2): a run may ADOPT one ground as its END ────
    //
    // "Treat GND as a kind of label" is the whole rule. A decoupling cap between
    // a pin and a ground glyph is topologically the same shape as a resistor
    // between a pin and a rail label — and M8.6 already draws THAT along the row
    // (`pin_to_bare_rail_is_along`). The only reason ground came out vertical was
    // the blanket `is_ground => Shunt` in step 4, not anything in the netlist.
    //
    // What keeps the classic vertical shunt alive is the end budget:
    //
    //   * G1 `ground_adoptable` — the caller's veto for a ground that owns a
    //     real GND pin on the IC. That net belongs to the South rail.
    //   * G2 a run that already carries a NAME (or ends at a component) has
    //     spent its outer end on that; its grounds keep dropping. This holds
    //     `moddcdc` `C1` (VDD_3V3 is a labelled rail) and `C3`/`C4` (VCC_1V2 is
    //     labelled) vertical.
    //   * G3 the adoption happens at the run's TIP and only if the tip's outer
    //     end is still free — ★ M11: adopting in the MIDDLE of a run would put
    //     two horizontal parts on one net, which is the exact fan-out this
    //     milestone exists to stop. One ground per run, one run per ground.
    //
    // Runs after step 3 so an ISLAND run can adopt too (M10 ran it before, which
    // silently excluded them). Ground nets are never claimed by step 3, so the
    // move cannot change any existing answer.
    //
    // ── KNOBS, in order of how much they give back ──────────────────────────
    //   (a) turn the whole feature off:  `ground_adoptable: false` in
    //       `chain_plan_for` — every ground goes back to a vertical Drop.
    //   (b) only a ground hanging DIRECTLY off a pin may be adopted: add
    //       `if depth[t] != 0 { continue; }` below. Classic decoupling caps
    //       stay horizontal, a ground at the end of a long chain does not.
    //   (c) only SENSE-side runs may adopt: require the root's `anchor_pin`
    //       rank to be <= 2 (Input / Bidir), so a power pin's cap keeps
    //       dropping while an EN pull-down goes horizontal.
    // Each is one line, none of them touches anything downstream.
    let mut run_spent: BTreeMap<usize, bool> = BTreeMap::new();
    let mut tip: BTreeMap<usize, usize> = BTreeMap::new();
    for i in 0..n {
        let Some(r) = region[i] else { continue };
        *run_spent.entry(r).or_insert(false) |= nets[i].is_endpoint || nets[i].ends_at_component;
        let t = tip.entry(r).or_insert(i);
        if (depth[i], i) > (depth[*t], *t) {
            *t = i;
        }
    }
    let mut ground_taken: BTreeSet<usize> = BTreeSet::new();
    for (&root, &t) in &tip {
        if run_spent.get(&root).copied().unwrap_or(false) {
            continue; // G2
        }
        if !ends[t].outer.is_free() {
            continue; // G3 — the tip already ends in something
        }
        let mut pick: Option<(i64, usize)> = None;
        for &p in &incident[t] {
            let (a, b) = parts[p].nets;
            let other = if a == t { b } else { a };
            if other >= n || other == t {
                continue;
            }
            if !nets[other].is_ground || !nets[other].ground_adoptable {
                continue; // G1
            }
            if region[other].is_some() || ground_taken.contains(&other) {
                continue; // one run per ground
            }
            if pick.map_or(true, |(id, _)| parts[p].box_id < id) {
                pick = Some((parts[p].box_id, other));
            }
        }
        let Some((box_id, g)) = pick else { continue };
        region[g] = Some(root);
        depth[g] = depth[t] + 1;
        ends[t].outer = EndUse::Part(box_id);
        ends[g].inner = EndUse::Part(box_id);
        ends[g].outer = EndUse::Name; // the ⏚ glyph IS the row's outer terminal
        ground_taken.insert(g);
    }

    // ── Step 3.9 (★ M11.3): names take whatever outer ends are left ──────────
    // A name that finds its end already spent does NOT lose its glyph — it is
    // drawn on a vertical stub instead (M10.1). `outer_end_taken` is how the
    // render side is told which case it is in, so the decision is topological
    // and not "did the text happen to overlap something".
    for i in 0..n {
        if nets[i].is_endpoint && ends[i].outer.is_free() {
            ends[i].outer = EndUse::Name;
        }
    }

    // ── Step 4: classify ─────────────────────────────────────────────────────
    let mut orientation: BTreeMap<i64, PartOrientation> = BTreeMap::new();
    for p in parts {
        let (a, b) = p.nets;
        // ★ M10.3: the `is_ground` short-circuit lives INSIDE the match. A
        // ground adopted onto this very run (step 3.5) is the run's outer end,
        // so the part into it lies ALONG the row and terminates in a horizontal
        // ground glyph; a ground claimed by a DIFFERENT run, or by none, is
        // still a Shunt.
        let o = if a >= n || b >= n {
            PartOrientation::Shunt
        } else {
            match (region[a], region[b]) {
                // Same run — a tree edge, or a parallel sibling of one (a back
                // edge inside one region). Both read as one straight wire, so
                // both are Along; the column model gives the parallel pair the
                // same column span at different offsets, which is how a
                // parallel pair is drawn. A parallel sibling does NOT spend a
                // second end: the pair leaves the net in one direction.
                (Some(ra), Some(rb)) if ra == rb => PartOrientation::Along,
                (Some(_), Some(_)) if !nets[a].is_ground && !nets[b].is_ground => {
                    PartOrientation::Across
                }
                _ => PartOrientation::Shunt,
            }
        };
        orientation.insert(p.box_id, o);
    }

    ChainPlan {
        region,
        depth,
        ends,
        orientation,
    }
}

/// ★ M11: extend one run outward, one net at a time, until its wire runs out of
/// ends. A run is a PATH — see the module note.
///
/// The loop stops on any of:
///   * the current net's outer end is already spent (a satellite, or the part
///     that a stronger run put there);
///   * the current net is NAMED and is not this run's own root — the label owns
///     the outer end, and parts hanging off it hang off the END of the run.
///     The root is exempt because a root is very often labelled as well
///     (`speaker`'s `US_SPEAKER_MUTE` carries both `lpa.1` and a bus label) and
///     a run that could never leave its root would make the whole milestone a
///     no-op; its name goes vertical instead (M10.1);
///   * no unclaimed non-ground neighbour is left.
fn grow(
    root: usize,
    nets: &[NetView],
    parts: &[PartView],
    incident: &[Vec<usize>],
    region: &mut [Option<usize>],
    depth: &mut [usize],
    ends: &mut [NetEnds],
) {
    let mut cur = root;
    loop {
        if !ends[cur].outer.is_free() {
            return;
        }
        if cur != root && nets[cur].is_endpoint {
            ends[cur].outer = EndUse::Name;
            return;
        }
        let Some((p, next)) = best_extension(cur, nets, parts, incident, region) else {
            return;
        };
        region[next] = region[cur];
        depth[next] = depth[cur] + 1;
        ends[cur].outer = EndUse::Part(parts[p].box_id);
        ends[next].inner = EndUse::Part(parts[p].box_id);
        cur = next;
    }
}

/// ★ M11: which ONE part gets this net's outer end.
///
/// Ranked, highest first:
///   1. the branch that reaches a proper END — a name or a component. A run
///      wants to terminate at something the reader can name, so the branch that
///      finds one is the main line and the rest are stubs off it;
///   2. the branch with the most nets still unclaimed behind it — between two
///      anonymous branches the longer one is the wire and the shorter one is a
///      decoration;
///   3. the smallest `box_id`, which makes the choice reproducible.
///
/// `reach_score` walks the unclaimed sub-graph, so it is `O(V·E)` across a whole
/// layer. At the scale of a device layer (tens of nets) that is free, and it
/// buys a decision that does not depend on the order boxes happen to be in.
fn best_extension(
    cur: usize,
    nets: &[NetView],
    parts: &[PartView],
    incident: &[Vec<usize>],
    region: &[Option<usize>],
) -> Option<(usize, usize)> {
    let n = nets.len();
    let mut best: Option<((bool, usize, std::cmp::Reverse<i64>), (usize, usize))> = None;
    for &p in &incident[cur] {
        let (a, b) = parts[p].nets;
        let other = if a == cur { b } else { a };
        if other >= n || other == cur {
            continue;
        }
        // Ground is never an extension — it is ADOPTED in step 3.5, after every
        // run has finished, so a cap can never outrank a real signal branch.
        if nets[other].is_ground || region[other].is_some() {
            continue;
        }
        let (named, size) = reach_score(other, nets, parts, incident, region);
        let key = (named, size, std::cmp::Reverse(parts[p].box_id));
        if best.as_ref().map_or(true, |(k, _)| key > *k) {
            best = Some((key, (p, other)));
        }
    }
    best.map(|(_, v)| v)
}

/// `(does this branch reach a proper end, how many nets are behind it)`.
///
/// The walk stops at a named net or at a component for the same reason `grow`
/// does: that is where the run would end, so nothing past it is reachable
/// horizontally.
fn reach_score(
    start: usize,
    nets: &[NetView],
    parts: &[PartView],
    incident: &[Vec<usize>],
    region: &[Option<usize>],
) -> (bool, usize) {
    let n = nets.len();
    let mut seen = vec![false; n];
    let mut queue: VecDeque<usize> = VecDeque::new();
    seen[start] = true;
    queue.push_back(start);
    let (mut named, mut size) = (false, 0usize);
    while let Some(c) = queue.pop_front() {
        size += 1;
        if nets[c].is_endpoint || nets[c].ends_at_component {
            named = true;
            continue;
        }
        for &p in &incident[c] {
            let (a, b) = parts[p].nets;
            let other = if a == c { b } else { a };
            if other >= n || other == c || seen[other] {
                continue;
            }
            if nets[other].is_ground || region[other].is_some() {
                continue;
            }
            seen[other] = true;
            queue.push_back(other);
        }
    }
    (named, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(rank: u8, id: i64) -> NetView {
        NetView {
            anchor_pin: Some((rank, id)),
            ..NetView::default()
        }
    }
    fn gnd() -> NetView {
        NetView {
            is_ground: true,
            ..NetView::default()
        }
    }
    /// ★ M10.3: a ground net with no GND pin on the IC — adoptable.
    fn gnd_free() -> NetView {
        NetView {
            is_ground: true,
            ground_adoptable: true,
            ..NetView::default()
        }
    }
    fn label() -> NetView {
        NetView {
            is_endpoint: true,
            ..NetView::default()
        }
    }
    fn pin_labelled(rank: u8, id: i64) -> NetView {
        NetView {
            anchor_pin: Some((rank, id)),
            is_endpoint: true,
            ..NetView::default()
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
    /// Every one of the nine answers is pinned here.
    ///
    /// Note the FIVE separate ground nets: `rails.rs` explodes ground into
    /// per-consumer flags on purpose, and that is what lets `C2` and `R3` each
    /// adopt one of their own (M10.3) — a single shared GND would give the
    /// first run the glyph and leave the second one vertical.
    #[test]
    fn moddcdc_orientations() {
        //  0 VDD_3V3 (Vin, Power ⇒ endpoint)   1 _net1 (EN)   2 _net3 (LX)
        //  3 _net5 (FB)   4 VCC_1V2 (label)    5..9 the five grounds
        let nets = vec![
            pin_labelled(4, 104),
            pin(1, 101),
            pin(3, 103),
            pin(1, 105),
            label(),
            gnd_free(), // 5 @C1
            gnd_free(), // 6 @C2
            gnd_free(), // 7 @C3
            gnd_free(), // 8 @C4
            gnd_free(), // 9 @R3
        ];
        let parts = vec![
            part(21, 0, 1), // R1  Vin ↔ EN
            part(11, 0, 5), // C1  Vin ↔ GND
            part(12, 1, 6), // C2  EN  ↔ GND
            part(31, 2, 4), // L1  LX  ↔ VCC_1V2
            part(15, 2, 3), // C5  LX  ↔ FB
            part(22, 4, 3), // R2  VCC_1V2 ↔ FB
            part(23, 3, 9), // R3  FB  ↔ GND
            part(13, 4, 7), // C3  VCC_1V2 ↔ GND
            part(14, 4, 8), // C4  VCC_1V2 ↔ GND
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

        // ★ M10.3: a NAMED run keeps its ground vertical, an anonymous one
        // spends its outer end on the glyph.
        assert_eq!(plan.orientation_of(11), PartOrientation::Shunt); // VDD_3V3 named
        assert_eq!(plan.orientation_of(13), PartOrientation::Shunt); // VCC_1V2 named
        assert_eq!(plan.orientation_of(14), PartOrientation::Shunt);
        assert_eq!(plan.orientation_of(12), PartOrientation::Along); // EN anonymous
        assert_eq!(plan.orientation_of(23), PartOrientation::Along); // FB anonymous
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
    /// siblings share their column span without spending a second end.
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
        assert_eq!(
            plan.ends_of(1).outer,
            EndUse::Part(2),
            "the pair is one end"
        );
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
    /// must not stop its own run; ★ M11 records that its own name lost the
    /// outer end, which is what turns that glyph vertical.
    #[test]
    fn pin_to_bare_rail_is_along() {
        let nets = vec![pin_labelled(1, 101), label()];
        let parts = vec![part(1, 0, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert!(plan.shares_row(0, 1));
        assert!(
            plan.outer_end_taken(0),
            "the bus label must leave vertically"
        );
        assert!(!plan.outer_end_taken(1), "the rail glyph owns the far end");
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

    /// ★ M10.3 `speaker`: `lpa.BYPASS ~ lpa.IN_P ~ C.1` and `C.2 ~ GND`. The
    /// signal node carries no name, so its row's outer end is free and the
    /// ground takes it: the cap lies ALONG the row and the glyph is drawn
    /// horizontally past it. This is the "GND is just a label" case.
    #[test]
    fn unnamed_run_adopts_its_ground() {
        let nets = vec![pin(2, 102), gnd_free()];
        let parts = vec![part(8, 0, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(8), PartOrientation::Along);
        assert!(plan.shares_row(0, 1), "the ground sits on the run's row");
        assert_eq!(plan.depth[1], 1);
        assert_eq!(plan.ends_of(1).outer, EndUse::Name);
    }

    /// The name owns the outer end. `moddcdc` `C1` hangs off `VDD_3V3`, which is
    /// a labelled rail — one horizontal end, already spent, so the cap drops.
    #[test]
    fn named_run_keeps_its_ground_vertical() {
        let nets = vec![pin_labelled(4, 104), gnd_free()];
        let parts = vec![part(11, 0, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(11), PartOrientation::Shunt);
        assert!(!plan.shares_row(0, 1));
    }

    /// A name anywhere on the run counts, not just at the root: `moddcdc`
    /// `LX ──[L1]── VCC_1V2` is one run and `VCC_1V2` is where it is named, so
    /// `C3`/`C4` stay vertical.
    #[test]
    fn a_name_deeper_in_the_run_still_spends_the_end() {
        let nets = vec![pin(3, 103), label(), gnd_free()];
        let parts = vec![part(31, 0, 1), part(13, 1, 2), part(14, 1, 2)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(31), PartOrientation::Along);
        assert_eq!(plan.orientation_of(13), PartOrientation::Shunt);
        assert_eq!(plan.orientation_of(14), PartOrientation::Shunt);
    }

    /// An IC with a real GND pin vetoes adoption (`ground_adoptable == false`),
    /// which is what keeps the `shunt_cap_hangs_vertical` fixture vertical.
    #[test]
    fn ic_ground_pin_vetoes_adoption() {
        let nets = vec![pin(4, 104), gnd()];
        let parts = vec![part(11, 0, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(11), PartOrientation::Shunt);
    }

    /// One run, one ground: a second cap into the SAME ground net is a parallel
    /// sibling of the first, so it lies along the row beside it rather than
    /// claiming a second end.
    #[test]
    fn only_one_ground_per_run() {
        let nets = vec![pin(2, 102), gnd_free()];
        let parts = vec![part(8, 0, 1), part(9, 0, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(8), PartOrientation::Along);
        assert_eq!(plan.orientation_of(9), PartOrientation::Along);
    }

    // ── ★ M11 ───────────────────────────────────────────────────────────────

    /// **The end budget, stated by the netlist you gave.**
    ///
    /// ```text
    ///   LPA.1 ~ R1.1                      net 0 — a pin at one end, R1 at the other
    ///   R1.2 ~ R2.1 ~ R3.1 ~ VIN          net 1 — 4 points, VIN is the name
    /// ```
    ///
    /// `R1` is decided by net 0 alone: a pin reaching straight out into a part,
    /// so it is horizontal. That makes it net 1's START; `VIN` is net 1's END;
    /// and `R2` / `R3` therefore have no horizontal end left and hang off the
    /// row vertically.
    #[test]
    fn a_part_is_the_start_of_the_next_net() {
        let nets = vec![pin(2, 101), label(), pin(1, 110), plain()];
        let parts = vec![part(1, 0, 1), part(2, 1, 2), part(3, 1, 3)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert_eq!(plan.ends_of(0).inner, EndUse::AnchorPin);
        assert_eq!(plan.ends_of(0).outer, EndUse::Part(1));
        assert_eq!(plan.ends_of(1).inner, EndUse::Part(1), "R1 is the start");
        assert_eq!(plan.ends_of(1).outer, EndUse::Name, "VIN is the end");
        assert_eq!(plan.orientation_of(2), PartOrientation::Across);
        assert_eq!(plan.orientation_of(3), PartOrientation::Across);
    }

    /// A net hands its wire to exactly ONE part. Under M8's BFS both branches
    /// were claimed and both came out Along, so two horizontal parts sat on one
    /// row heading the same way and `chain_origins` queued them nose-to-tail.
    /// The branch that reaches a name wins; the other one goes vertical.
    #[test]
    fn one_horizontal_extension_per_net() {
        //  0 pin ─[R1]─ 1 ─[R2]─ 2 (named)
        //                └─[R3]─ 3 (anonymous dead end)
        let nets = vec![pin(3, 101), plain(), label(), plain()];
        let parts = vec![part(1, 0, 1), part(2, 1, 2), part(3, 1, 3)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert_eq!(plan.orientation_of(2), PartOrientation::Along);
        assert_eq!(
            plan.orientation_of(3),
            PartOrientation::Across,
            "net 1's outer end went to the branch that reaches a name"
        );
        assert_eq!(plan.ends_of(1).outer, EndUse::Part(2));
    }

    /// Between two anonymous branches, the longer one is the wire.
    #[test]
    fn the_longer_branch_is_the_wire() {
        //  0 pin ─[R1]─ 1 ─[R9]─ 2 ─[R8]─ 3       (long, high box ids)
        //                └─[R2]─ 4                (short, low box id)
        let nets = vec![pin(3, 101), plain(), plain(), plain(), plain()];
        let parts = vec![part(1, 0, 1), part(9, 1, 2), part(8, 2, 3), part(2, 1, 4)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(9), PartOrientation::Along);
        assert_eq!(plan.orientation_of(2), PartOrientation::Across);
    }

    /// ★ M11.2 — "may end at a component". A row that already ends at a satellite
    /// component has no outer end to give away, so a part on it bridges rows
    /// instead of extending the wire into the box.
    #[test]
    fn a_component_owns_the_outer_end() {
        let nets = vec![
            NetView {
                anchor_pin: Some((3, 105)),
                ends_at_component: true,
                ..NetView::default()
            },
            plain(),
        ];
        let parts = vec![part(1, 0, 1)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Across);
        assert_eq!(plan.ends_of(0).outer, EndUse::Component);
        assert!(plan.outer_end_taken(0));
    }

    /// **A net may carry THREE horizontal parts and still be in budget.**
    ///
    /// ```text
    ///   lpa.1 ~ r1.1
    ///   r1.2 ~ r2.1 ~ r3.1
    ///   r2.2 ~ r3.2 ~ vcc
    /// ```
    ///
    /// `R2` and `R3` are a parallel BUNDLE — one gap, two bodies — so the middle
    /// net spends one end on the pair, not one each. This is the case that shows
    /// why the budget counts directions and not parts.
    #[test]
    fn a_parallel_bundle_is_one_end() {
        //  0 lpa.1~r1.1      1 r1.2~r2.1~r3.1      2 r2.2~r3.2~vcc
        let nets = vec![pin(2, 101), plain(), label()];
        let parts = vec![part(1, 0, 1), part(2, 1, 2), part(3, 1, 2)];
        let plan = analyse(&nets, &parts);
        for b in [1, 2, 3] {
            assert_eq!(plan.orientation_of(b), PartOrientation::Along, "box {b}");
        }
        assert_eq!(plan.ends_of(1).inner, EndUse::Part(1));
        assert_eq!(
            plan.ends_of(1).outer,
            EndUse::Part(2),
            "the bundle leaves once — R3 rides on R2's end"
        );
        assert_eq!(plan.ends_of(2).outer, EndUse::Name, "vcc is the far end");
        assert_eq!(plan.depth, vec![0, 1, 2]);
    }

    /// The bundle only counts once when it really is a bundle. Two parts off one
    /// net going to two DIFFERENT nets are a fan-out, and only one of them may
    /// be horizontal.
    #[test]
    fn a_fan_out_is_not_a_bundle() {
        //  0 pin ─[R1]─ 1 ─[R2]─ 2 (named)
        //                └─[R3]─ 3 (also named, but a different net)
        let nets = vec![pin(2, 101), plain(), label(), label()];
        let parts = vec![part(1, 0, 1), part(2, 1, 2), part(3, 1, 3)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert_eq!(plan.orientation_of(2), PartOrientation::Along);
        assert_eq!(plan.orientation_of(3), PartOrientation::Across);
    }

    /// ★ M11.2 — a ground is adopted at the run's TIP, never in its middle.
    /// M10 walked the parts in `box_id` order and adopted at the first ground
    /// it saw, which on `pin ─[R1]─ X` + `pin ─[C]─ GND` put a second
    /// horizontal part on the pin's row, pointing the same way as `R1`.
    #[test]
    fn ground_is_adopted_only_at_the_tip() {
        // The ground hangs off the ROOT, whose end R1 already took.
        let nets = vec![pin(2, 102), plain(), gnd_free()];
        let parts = vec![part(1, 0, 1), part(9, 0, 2)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert_eq!(plan.orientation_of(9), PartOrientation::Shunt);

        // Move it to the tip and it is adopted: pin ─[R1]─ X ─[C]─ ⏚.
        let parts = vec![part(1, 0, 1), part(9, 1, 2)];
        let plan = analyse(&nets, &parts);
        assert_eq!(plan.orientation_of(1), PartOrientation::Along);
        assert_eq!(plan.orientation_of(9), PartOrientation::Along);
        assert_eq!(plan.depth[2], 2);
    }
}
