// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW —— EntryPoint filling (Step 1 + P06 two rounds)
//!
//! ## Motivation
//! Before this, `McVecBox.entry_points` was always empty, causing router to degenerate to
//! "evenly distribute on box four sides, guess direction" approximation. All N pins on the same IC are mapped
//! to the midpoint of the box edge → all wires exit squeezed together.
//!
//! This module solves this problem: in layout phase, assign each box's all pins
//! an `EntrySide` + `offset`, router gets the exact pin position.
//!
//! ## ★ P06 (S5) two-round scheduling
//! Step 1's `assign_entry_points` only looks at pin **name**, doesn't know where neighbors will ultimately be placed ——
//! A Generic pin with name `"5"` would be evenly distributed to Left, but its real neighbor is on the right,
//! router has to draw "U-shaped path back".
//!
//! P06 changes the flow to **two rounds**:
//!
//! ```text
//! ┌─────────────────────┐
//! │ assign_default_sizes │
//! └─────────┬───────────┘
//!           ▼
//! ┌─────────────────────────────────┐    ← Round 1: coarse
//! │ assign_entry_points_coarse      │      Only look at pin name / IoDirection
//! │   (= old assign_entry_points)   │      Used by layout for size calculation
//! └─────────┬───────────────────────┘
//!           ▼
//! ┌─────────────────────┐
//! │  Layouter calculate coordinates │
//! └─────────┬───────────┘
//!           ▼
//! ┌─────────────────────────────────┐    ← Round 2: refine
//! │ assign_entry_points_refine      │      Look at neighbor final (x,y) rearrange pin side
//! │   - Only move Passive/Bidir/Unknown │      Power/Ground/Input/Output stay
//! │   - Redistribute offset on same side          │
//! └─────────┬───────────────────────┘
//!           ▼
//! ┌─────────────────────────────────┐    ← (optional) size recalculate
//! │ recompute_sizes_with_pin_count  │      pin side count changed → box grows taller
//! └─────────────────────────────────┘
//! ```
//!
//! `assign_entry_points` remains public API, equals `assign_entry_points_coarse`,
//! callers (HierarchicalLayouter / RadialLayouter) gradually migrate to two-round scheduling.
//!
//! ## Assignment rules (coarse round, same as original)
//! Name heuristics (don't depend on InstTable):
//! - `VCC` / `VDD` / `V3V3` / `V5V` ... → **Top**
//! - `GND` / `VSS` / `AGND` ...        → **Bottom**
//! - `*_IN` / `RX` / `MOSI` / `CLK` / `RST` ... → **Left**  (input)
//! - `*_OUT` / `TX` / `MISO` / `INT` ...        → **Right** (output)
//! - Others (including pure numeric pin numbers)            → Evenly distributed between **Left** / **Right**
//!
//! N pins on the same side have evenly distributed offset: `offset_i = (i + 0.5) / N`
//!
//! ## Refinement rules (P06 new)
//! For each box's each pin:
//! 1. If `io_type ∈ {Power, Ground, Input, Output}` → **skip** (semantics take priority)
//! 2. Otherwise (Passive / Bidir / Unknown): look at geometric center of opposite endpoint on net
//! 3. Use `pick_side_by_direction(dx, dy)` to decide new side
//! 4. Add hysteresis (new direction must be ≥ 1.2x current direction to switch, prevent oscillation)
//! 5. Redistribute offset evenly for same-side pins, avoid stacking

use std::collections::{HashMap, HashSet};

use crate::vector::graph::box_def::BoxPin;
use crate::vector::graph::net_def::IoDirection;
use crate::vector::graph::{BoxKind, EntryPoint, EntrySide, McVecBox, McVecGraph};

// ============================================================================
// Main API
// ============================================================================

/// **(compatible alias)** —— Fill `entry_points` field for each box in graph (recursive subgraphs)
///
/// Since P06 (S5), behavior equals [`assign_entry_points_coarse`].
/// Callers (Layouter) should switch to explicit two-round scheduling:
/// ```ignore
/// assign_entry_points_coarse(graph);   // before calculating coordinates
/// // ... layouter calculates coordinates ...
/// assign_entry_points_refine(graph);   // after calculating coordinates, look at neighbors to adjust pin side
/// ```
///
/// Layouter that didn't change goes single-round compatibility path, visual behavior identical to pre-P06.
pub fn assign_entry_points(graph: &mut McVecGraph) {
    assign_entry_points_coarse(graph)
}

/// ★ P06 (S5) — coarse round: only look at name/IoDirection to assign pin side
///
/// Same behavior as old `assign_entry_points`, just renamed to highlight "this is the first round".
/// Must be called **before** layouter calculates coordinates (`box_size_v2` uses entry_points to calculate height).
pub fn assign_entry_points_coarse(graph: &mut McVecGraph) {
    // Step 1: Collect all (pin_id, pin_name) used by each box —— only "connected" pins
    let pins_per_box = collect_pins_per_box(graph);

    // Step 2: Calculate entry points for each box
    //
    // ★ Key fix: merge "connected pins" (from net) with "box physical pins" (from mcode, b.pins).
    // Previously only used pins from net, causing boxes with no connections to have zero entry points →
    // Can't draw pins, size degenerates to minimum. After merging, boxes without connections can also draw pins normally.
    for b in &mut graph.boxes {
        let empty = Vec::new();
        let net_pins = pins_per_box.get(&b.id).unwrap_or(&empty);
        // ★ Connected pins set (pin_id that appeared in nets) —— for compute_entry_points to
        //   place "connected pins" on left/right of core, "unconnected pins" on top/bottom waste area to distinguish.
        let connected: HashSet<i64> = net_pins.iter().map(|(id, _)| *id).collect();
        let merged = merge_box_pins(net_pins, &b.pins);
        b.entry_points = compute_entry_points(b, &merged, &connected);
    }

    // Step 3: Recursive subgraphs
    for sub in &mut graph.sub_graphs {
        assign_entry_points_coarse(sub);
    }
}

/// Merge "connected pins" (from net) with "box physical pins" (from mcode), deduplicate by `pin_id`.
///
/// - Net pins are placed first —— maintain existing wire routing position / order unchanged, already wired graphs unaffected;
/// - Physical pins on box **not appearing in net** (i.e., unconnected pins) are appended at the end,
///   ensure "unconnected pins can also be drawn" (pin number / name complete);
/// - Skip physical pins with `id <= 0` (shouldn't exist theoretically, defensive handling).
///
/// net endpoint pin_id and BoxPin.id come from same source (both are that pin's InstEntry id), so when a
/// connected physical pin appears on both sides, it will be correctly deduplicated into one entry point.
fn merge_box_pins(net_pins: &[(i64, String)], box_pins: &[BoxPin]) -> Vec<(i64, String)> {
    // Placeholder pins (when typed-chip hasn't registered Pin sub-items, from_block synthesizes pins to "hold shape", id ≥ this base)
    // Once box already has real named pins from net (like flash's VCC/VSS/SPI), placeholder pins are pure noise ——
    // Skip them, avoid VCC/VSS/SPI having 1/2/3/4/5 appear next to them. Only keep placeholder pins when box has no connections (net_pins empty),
    // otherwise box can't draw any pins.
    const PLACEHOLDER_BASE: i64 = 8_000_000_000;
    let has_net_pins = !net_pins.is_empty();

    let mut seen: HashSet<i64> = net_pins.iter().map(|(id, _)| *id).collect();
    let mut merged: Vec<(i64, String)> = net_pins.to_vec();
    for p in box_pins {
        if p.id <= 0 || !seen.insert(p.id) {
            continue;
        }
        if has_net_pins && p.id >= PLACEHOLDER_BASE {
            continue; // Already has real named pins, drop placeholder pins
        }
        // This string is only used for side classification (classify_pin → which side) and size estimation; the label actually drawn on
        //   the graph is taken by render_pin via find_pin (common name + description). Classification using functional
        //   description is more accurate (GND→bottom / VCC→top / TX→right...); if no description, fall back to common name.
        let label = if !p.description.is_empty() {
            p.description.clone()
        } else {
            p.pin_id.clone()
        };
        merged.push((p.id, label));
    }
    merged
}

/// ★ FIX (subgraph iteration) — promote "synthetic endpoints" to **independent** pins on boxes
///
/// ## Background: tree-root collapse + no enlargement root cause
/// rail-synth / same-name paired nets in `from_block` create endpoints with `pin_id = -1`,
/// `pin_name = "(rail)"` (see `synthesize_rail_nets`). If a box connects to N
/// such nets (N different rails/same-name signals), its N synthetic endpoints **all have pin_id = -1**.
/// This causes:
/// 1. [`collect_pins_per_box`] deduplicates by `pin_id` → N endpoints collapse to **1** entry
///    point → box thinks it has only 1 pin → size logic doesn't enlarge;
/// 2. Routing `compute_exit_for_pin(-1)` can't find corresponding entry → all degenerate to box edge
///    **same midpoint** → N wires collapse to "tree root" connection into box.
///
/// ## What this pass does
/// Assign each `pin_id <= 0` endpoint a **globally unique** positive id (high base, no collision with real
/// point id / flag id), and replace placeholder name `"(rail)"` with net name. Result:
/// - Multiple synthetic connections on same box get different ids → no longer deduplicated into one → each becomes independent
///   pin, box enlarges by real connection count;
/// - Pin names become real rail/signal names (GND / VCC_1V2 / ...) → [`compute_entry_points`]
///   `classify_pin` distributes to appropriate side (ground→bottom, power→top, signal→left/right), labels also meaningful;
/// - Routing `find_entry` can hit each pin → each wire connects to its own point on box (fan-out).
///
/// Must be called **before** [`assign_entry_points_coarse`] and `assign_default_sizes`
/// (both work with pin_id produced here). Recursive subgraphs.
pub fn promote_synthetic_pins(graph: &mut McVecGraph) {
    /// Starting base for synthetic pin_id:
    /// - Higher than common real point id (InstTable sequential id, much lower)
    /// - Lower than rails.rs `FLAG_ID_BASE` (9e9) / `STUB_NET_ID_BASE` (9.5e9)
    /// pin_id and box_id are different namespaces, here only need to ensure "same box + with real pin" don't collide.
    const SYNTH_PIN_BASE: i64 = 3_000_000_000;

    fn go(graph: &mut McVecGraph, counter: &mut i64) {
        for net in &mut graph.nets {
            let net_name = net.name.clone();
            for ep in &mut net.endpoints {
                if ep.pin_id <= 0 {
                    ep.pin_id = SYNTH_PIN_BASE + *counter;
                    *counter += 1;
                    if !net_name.is_empty() && (ep.pin_name.is_empty() || ep.pin_name == "(rail)") {
                        ep.pin_name = net_name.clone();
                    }
                }
            }
        }
        for sub in &mut graph.sub_graphs {
            go(sub, counter);
        }
    }

    let mut counter = 0i64;
    go(graph, &mut counter);
}

// ============================================================================
// ★ FIX (split shared pins) — one pin connects to multiple nets → one independent pin per net
// ============================================================================

/// Split "one pin connected to multiple nets" into "one independent pin per net".
///
/// ## Background: root cause of "multiple wires fan out from one point"
/// Composite / bundle ports (like `[VDD_3V3, VCC_1V2]`, `[X, GND]`) flatten to **one**
/// `pin_id`, but may appear in **multiple different nets** simultaneously (3V3 net + 1V2 net, or GND net + signal net).
/// Renderer draws one pin per `pin_id` → multiple wires connect at **one point**, visually multiple wires fan out from one
/// point on component (user feedback: mcu513's 3V3/1V2, flash's left/right sides).
///
/// Note: This is different from offset collision — here component has only **one** entry point, rearranging offset
/// can't help, must **split this pin into multiple independent pins**.
///
/// ## What it does
/// Count how many **different nets** reference each `(box_id, pin_id)`:
/// - Referenced by 1 net → normal pin, don't touch (most pins are like this, so this pass has minimal side effects).
/// - Referenced by K (>1) nets → keep first net with original `pin_id`, each of the other K-1 nets gets a **brand new
///   `pin_id`** (high base, no collision with real / synthetic / flag id), rewrite this endpoint's
///   `pin_id` in those nets (`pin_name` preserved, classification / label still follows electrical name).
///
/// Result: [`collect_pins_per_box`] sees K different pins → [`compute_entry_points`] gives them K
/// different entry points → routing `compute_exit_for_pin` hits each point → each net connects to component
/// **its own pin** then wires out, no more "fanning out from one point".
///
/// Must be called **before** [`assign_entry_points_coarse`] / `assign_default_sizes` (they work with
/// `pin_id` produced here), typically right after [`promote_synthetic_pins`]. Recursive subgraphs.
pub fn split_shared_pins(graph: &mut McVecGraph) {
    /// Starting base for split pin:
    /// - Higher than real point id and `promote_synthetic_pins` `SYNTH_PIN_BASE` (3e9)
    /// - Lower than `rails.rs` `FLAG_ID_BASE` (9e9)
    const SPLIT_PIN_BASE: i64 = 4_000_000_000;

    fn go(graph: &mut McVecGraph, counter: &mut i64) {
        // ★ Safety guard: two-pin passives (TwoPin, resistor/capacitor/inductor/diode) **don't split**.
        //   Their pin rendering (ep_for_two_pin) only takes first 2 pins, split 3rd+ pins get
        //   truncated → that net's endpoint can't find entry → degenerate to box edge midpoint → component looks disconnected.
        //   Also two-pin passive pins shouldn't share across nets (sharing = electrical short), skipping them has zero side effects.
        let two_pin_ids: HashSet<i64> = graph
            .boxes
            .iter()
            .filter(|b| matches!(b.kind, BoxKind::TwoPin))
            .map(|b| b.id)
            .collect();

        // 1. Count each (box_id, pin_id) → nets referencing it (deduplicate within same net,
        //    avoid one net having same pin twice being mistakenly judged as "multi-net sharing")
        let mut net_refs: std::collections::BTreeMap<(i64, i64), Vec<usize>> =
            std::collections::BTreeMap::new();
        for (ni, net) in graph.nets.iter().enumerate() {
            let mut seen: HashSet<(i64, i64)> = HashSet::new();
            for ep in &net.endpoints {
                if ep.pin_id <= 0 {
                    continue; // synthetic endpoints handled by promote_synthetic_pins
                }
                if two_pin_ids.contains(&ep.box_id) {
                    continue; // two-pin passives don't split (see above)
                }
                let key = (ep.box_id, ep.pin_id);
                if seen.insert(key) {
                    net_refs.entry(key).or_default().push(ni);
                }
            }
        }

        // 2. Generate rewrite plan: (net_index, box_id, old_pin, new_pin)
        //    Keep first net of each shared pin with original id, each remaining net gets a new id.
        let mut plan: Vec<(usize, i64, i64, i64)> = Vec::new();
        for ((box_id, pin_id), nets) in &net_refs {
            if nets.len() <= 1 {
                continue;
            }
            for &ni in nets.iter().skip(1) {
                let new_pid = SPLIT_PIN_BASE + *counter;
                *counter += 1;
                plan.push((ni, *box_id, *pin_id, new_pid));
            }
        }

        // 3. Apply rewrite
        let n_split = plan.len();
        for (ni, box_id, old_pin, new_pin) in plan {
            if let Some(ep) = graph.nets[ni]
                .endpoints
                .iter_mut()
                .find(|e| e.box_id == box_id && e.pin_id == old_pin)
            {
                ep.pin_id = new_pin;
            }
        }
        if n_split > 0 {
            crate::vlog!(
                "[layout::split_shared] graph '{}' bid={}: split {} shared pin-connection(s) into distinct pins",
                graph.name, graph.bid, n_split
            );
        }

        for sub in &mut graph.sub_graphs {
            go(sub, counter);
        }
    }

    let mut counter = 0i64;
    go(graph, &mut counter);
}

// ============================================================================
// ★ FIX (deduplication fallback) — ensure pin exit points on each side don't overlap and have minimum spacing
// ============================================================================

/// **Minimum pixel spacing** between two pins on same side. Below this value is judged as "too close / overlapping",
/// triggers re-spreading for that side.
pub const MIN_PIN_GAP_PX: f64 = 18.0;

/// Ensure pin exit points on same side of each box don't overlap, and adjacent spacing ≥ [`MIN_PIN_GAP_PX`].
///
/// ## Why needed (flash "two wires not separated" root cause)
/// `order_pins_by_neighbor` only rearranges offset for pins **with opposite neighbor** (`(rank+1)/(n+1)`),
/// pins **without neighbor** (e.g., pins only connected to GND flag / power flag) keep old offset. Result: on same side,
/// "rearranged pins" and "unrearranged pins" may fall to **same offset** → two wires fan out from **same point** on box edge. `align_hub_to_spokes` aligning two pins to same opposite Y also causes offset collision.
///
/// `split_shared_pins` solves "one pin_id across multiple nets"; this solves "**two different pin_id colliding at same offset**", complementary.
///
/// ## Rules (try not to disturb already arranged sides)
/// For each side of each box:
/// 1. Sort by current offset.
/// 2. All adjacent pins have pixel spacing ≥ MIN_PIN_GAP_PX → side not crowded, **keep original offset**
///    (preserve positions already arranged by align_hub_to_spokes / order_pins_by_neighbor).
/// 3. Otherwise re-spread that side: prefer centered by MIN_PIN_GAP_PX spacing; if doesn't fit → evenly distribute across side.
///
/// Only changes `entry_points[*].offset`, **doesn't move any boxes** → never introduces new box collision. Recursive subgraphs.
/// Must be called after layouter **completes all offset rearrangements** (at layout end).
pub fn enforce_unique_offsets(graph: &mut McVecGraph) {
    for b in &mut graph.boxes {
        let (bw, bh) = (b.w, b.h);
        let edge_len_of = |side: &EntrySide| -> f64 {
            match side {
                EntrySide::Top | EntrySide::Bottom => bw,
                EntrySide::Left | EntrySide::Right => bh,
            }
        };

        let mut by_side: HashMap<EntrySide, Vec<usize>> = HashMap::new();
        for (i, ep) in b.entry_points.iter().enumerate() {
            by_side.entry(ep.side.clone()).or_default().push(i);
        }

        for (side, mut idxs) in by_side {
            if idxs.len() < 2 {
                continue;
            }
            let len = edge_len_of(&side).max(1.0);

            idxs.sort_by(|&a, &c| {
                b.entry_points[a]
                    .offset
                    .partial_cmp(&b.entry_points[c].offset)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let crowded = idxs.windows(2).any(|w| {
                let d = (b.entry_points[w[1]].offset - b.entry_points[w[0]].offset).abs() * len;
                d < MIN_PIN_GAP_PX
            });
            if !crowded {
                continue; // side not crowded → keep original offset
            }

            let n = idxs.len();
            let want_span = MIN_PIN_GAP_PX * (n as f64 - 1.0);
            if want_span <= len {
                let start = (len - want_span) / 2.0;
                for (rank, &idx) in idxs.iter().enumerate() {
                    let pos = start + MIN_PIN_GAP_PX * rank as f64;
                    b.entry_points[idx].offset = (pos / len).clamp(0.02, 0.98);
                }
            } else {
                for (rank, &idx) in idxs.iter().enumerate() {
                    b.entry_points[idx].offset = (rank as f64 + 1.0) / (n as f64 + 1.0);
                }
            }
        }
    }

    for sub in &mut graph.sub_graphs {
        enforce_unique_offsets(sub);
    }
}

/// ★ P06 (S5) — refine round: use final coordinates to rearrange "semantic-less" pins to neighbor's nearest side
///
/// Must be called after layouter **completes coordinate calculation**. Read `box.x/y/w/h` to infer neighbor direction.
///
/// ## Rules
/// 1. Only move pins with `IoDirection ∈ {Passive, Bidir, Unknown}`
///    (Power/Ground/Input/Output keep coarse results, these pins' sides are semantics-determined)
/// 2. Look at geometric center average of all "opposite boxes" on this pin's net, decide direction
/// 3. Apply hysteresis (new direction must clearly dominate, then switch, prevent oscillation)
/// 4. After switching sides, rearrange same-side pins offset, avoid stacking
///
/// io_type looked up by querying `graph.nets` inversely (not stored in EntryPoint, avoid schema change).
pub fn assign_entry_points_refine(graph: &mut McVecGraph) {
    // 1. Build (box_id, pin_id) → IoDirection mapping in one pass (look up from nets)
    let pin_io = collect_pin_io_types(graph);

    // 2. Opposite box list for each pin
    let pin_neighbors = collect_pin_neighbors(graph);

    // 3. Center coordinates of each box
    let box_centers = collect_box_centers(graph);

    // 3b. ★ Each box's rectangle (for collision detection: does straight line towards neighbor pass through other boxes)
    let box_rects = collect_box_rects(graph);

    // 4. Refine each pin for each box
    let mut reassigned_total = 0usize;
    for b in &mut graph.boxes {
        let bcx = b.x + b.w / 2.0;
        let bcy = b.y + b.h / 2.0;
        let mut reassigned_in_box = 0usize;

        for ep in &mut b.entry_points {
            let io = pin_io
                .get(&(b.id, ep.pin_id))
                .copied()
                .unwrap_or(IoDirection::Unknown);

            // Rule 1: semantic pins don't move
            if !is_repinnable(io) {
                continue;
            }

            // Rule 2: find neighbor direction
            let nbrs = pin_neighbors
                .get(&(b.id, ep.pin_id))
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            if nbrs.is_empty() {
                continue; // isolated pin
            }

            let (sum_x, sum_y, n) = nbrs
                .iter()
                .filter_map(|nb| box_centers.get(nb))
                .fold((0.0_f64, 0.0_f64, 0usize), |(sx, sy, k), &(x, y)| {
                    (sx + x, sy + y, k + 1)
                });
            if n == 0 {
                continue;
            }
            let (ncx, ncy) = (sum_x / n as f64, sum_y / n as f64);

            let dx = ncx - bcx;
            let dy = ncy - bcy;

            let preferred = pick_side_by_direction(dx, dy);

            // ★ Collision-aware: if straight line "towards neighbor" (box center→neighbor center) passes through other boxes,
            //   don't face it (will hit head-on, e.g., dc straight down towards speaker passes through moddcdc);
            //   change to perpendicular side with more empty space, let router route around.
            let mut exclude: HashSet<i64> = nbrs.iter().copied().collect();
            exclude.insert(b.id);
            let blocked = path_blocked((bcx, bcy), (ncx, ncy), &box_rects, &exclude);
            let (target, forced) = if blocked {
                match open_perpendicular_side((b.x, b.y, b.w, b.h), &preferred, &box_rects, b.id) {
                    Some(s) => (s, true), // found more empty perpendicular side → forced switch (collision avoidance is hard constraint)
                    None => (preferred, false), // all sides blocked, no choice, keep orientation
                }
            } else {
                (preferred, false)
            };

            // ★ User requirement: component pins **only on left/right sides**. Refinement allows left/right swap (pins face neighbor's horizontal side),
            //   but never flip pins to top/bottom — project any "top/bottom" result back to left/right by neighbor's horizontal direction.
            //   collision avoidance handled by router, not by "flipping pins to top/bottom" (that would break professional IC symbol left/right layout).
            let target = match target {
                EntrySide::Top | EntrySide::Bottom => {
                    if dx >= 0.0 {
                        EntrySide::Right
                    } else {
                        EntrySide::Left
                    }
                }
                s => s,
            };

            if target == ep.side {
                continue;
            }

            // Rule 3: hysteresis — only needed for pure direction preference (avoid dx≈dy oscillating);
            //   collision-avoidance forced switches don't go through hysteresis (hard constraint, not preference).
            if !forced && !sides_warrant_switch(&ep.side, &target, dx, dy) {
                continue;
            }

            if std::env::var("MC_VIZ_REFINE_DUMP").is_ok() {
                crate::vlog!(
                    "[entry_refine] box '{}' (id={}) pin {} '{}': {:?} → {:?} \
                     (nbr_dir=({:+.0},{:+.0})){}",
                    b.name,
                    b.id,
                    ep.pin_id,
                    ep.pin_name,
                    ep.side,
                    target,
                    dx,
                    dy,
                    if forced { " [forced]" } else { "" }
                );
            }

            ep.side = target;
            reassigned_in_box += 1;
        }

        if reassigned_in_box > 0 {
            normalize_offsets_per_side(b);
            reassigned_total += reassigned_in_box;
        }
    }

    crate::vlog!(
        "[entry_refine] graph '{}' bid={}: {} pins reassigned across {} boxes",
        graph.name,
        graph.bid,
        reassigned_total,
        graph.boxes.len()
    );

    // 5. Recursive subgraphs
    for sub in &mut graph.sub_graphs {
        assign_entry_points_refine(sub);
    }
}

// ============================================================================
// ★ P06 helpers
// ============================================================================

/// Determine if a pin with io_type can be rearranged in refinement round
///
/// - Power / Ground / Input / Output → **don't move** (semantics decide)
/// - Passive (directionless passive) / Bidir / Unknown → can rearrange
fn is_repinnable(io: IoDirection) -> bool {
    matches!(
        io,
        IoDirection::Passive | IoDirection::Bidir | IoDirection::Unknown
    )
}

/// Pick side based on neighbor direction (dx, dy)
///
/// Use abs(dx) vs abs(dy) to decide horizontal or vertical axis, then look at sign for specific side.
fn pick_side_by_direction(dx: f64, dy: f64) -> EntrySide {
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            EntrySide::Right
        } else {
            EntrySide::Left
        }
    } else if dy >= 0.0 {
        EntrySide::Bottom
    } else {
        EntrySide::Top
    }
}

/// hysteresis: is new side worth switching to (avoid dx ≈ dy oscillating)
///
/// Only switch when **target direction** dominates **current direction** by 1.2x or more.
/// This way if a pin was on Left and neighbor is slightly right (but dy close to dx size),
/// won't casually switch to Right; neighbor must be clearly on right to switch.
fn sides_warrant_switch(cur: &EntrySide, new: &EntrySide, dx: f64, dy: f64) -> bool {
    if cur == new {
        return false;
    }
    let new_axis_strength = match new {
        EntrySide::Left | EntrySide::Right => dx.abs(),
        EntrySide::Top | EntrySide::Bottom => dy.abs(),
    };
    let cur_axis_strength = match cur {
        EntrySide::Left | EntrySide::Right => dx.abs(),
        EntrySide::Top | EntrySide::Bottom => dy.abs(),
    };
    if axis_of(cur) == axis_of(new) {
        return true;
    }
    new_axis_strength >= cur_axis_strength * 1.2
}

#[derive(PartialEq, Eq)]
enum Axis {
    H, // Left / Right
    V, // Top / Bottom
}
fn axis_of(s: &EntrySide) -> Axis {
    match s {
        EntrySide::Left | EntrySide::Right => Axis::H,
        EntrySide::Top | EntrySide::Bottom => Axis::V,
    }
}

/// Redistribute offsets evenly for multiple pins on same side, avoid stacking
///
/// When to call: after refine switches pin side, to prevent pins newly added to a side from stacking at one place.
fn normalize_offsets_per_side(b: &mut McVecBox) {
    let mut by_side: HashMap<EntrySide, Vec<usize>> = HashMap::new();
    for (i, ep) in b.entry_points.iter().enumerate() {
        by_side.entry(ep.side.clone()).or_default().push(i);
    }
    for (_side, mut indices) in by_side {
        indices.sort_by(|&a, &c| {
            b.entry_points[a]
                .offset
                .partial_cmp(&b.entry_points[c].offset)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let n = indices.len();
        for (rank, &idx) in indices.iter().enumerate() {
            b.entry_points[idx].offset = (rank as f64 + 0.5) / n as f64;
        }
    }
}

/// (box_id, pin_id) → IoDirection (look up from graph.nets)
fn collect_pin_io_types(graph: &McVecGraph) -> HashMap<(i64, i64), IoDirection> {
    let mut out: HashMap<(i64, i64), IoDirection> = HashMap::new();
    for net in &graph.nets {
        for ep in &net.endpoints {
            if ep.pin_id <= 0 {
                continue; // synthetic endpoints have no io_type concept
            }
            // if same pin appears on multiple nets, take first non-Unknown
            out.entry((ep.box_id, ep.pin_id))
                .and_modify(|cur| {
                    if matches!(cur, IoDirection::Unknown)
                        && !matches!(ep.io_type, IoDirection::Unknown)
                    {
                        *cur = ep.io_type;
                    }
                })
                .or_insert(ep.io_type);
        }
    }
    out
}

/// (box_id, pin_id) → [opposite box_id, ...]
fn collect_pin_neighbors(graph: &McVecGraph) -> HashMap<(i64, i64), Vec<i64>> {
    let mut out: HashMap<(i64, i64), Vec<i64>> = HashMap::new();
    for net in &graph.nets {
        // all box_id involved in this net
        let mut net_box_ids: Vec<i64> = net.endpoints.iter().map(|e| e.box_id).collect();
        net_box_ids.sort();
        net_box_ids.dedup();

        for ep in &net.endpoints {
            if ep.pin_id <= 0 {
                continue;
            }
            let key = (ep.box_id, ep.pin_id);
            let entry = out.entry(key).or_default();
            for &other_box in &net_box_ids {
                if other_box != ep.box_id && !entry.contains(&other_box) {
                    entry.push(other_box);
                }
            }
        }
    }
    out
}

/// box_id → (cx, cy) geometric center
fn collect_box_centers(graph: &McVecGraph) -> HashMap<i64, (f64, f64)> {
    graph
        .boxes
        .iter()
        .map(|b| (b.id, (b.x + b.w / 2.0, b.y + b.h / 2.0)))
        .collect()
}

// ============================================================================
// ★ Collision-aware pin assignment: make pins "that would hit other boxes" flip to empty side
// ============================================================================

const RECT_PAD: f64 = 8.0; // box inset, only catch "actual hits" (corner cuts / through small passives don't count)
const OPEN_FAR: f64 = 1.0e6; // no obstruction in direction = extremely empty
const MIN_CLEAR: f64 = 60.0; // perpendicular side must have this much clearance to be worth routing around
const MIN_BLOCKER_DIM: f64 = 70.0; // only boxes with both dimensions ≥ this value are "big boxes" (IC/submodule) counted as obstacles,
                                   // small passives don't block → dense subgraph areas don't flip randomly

/// box_id → (x, y, w, h) rectangle
fn collect_box_rects(graph: &McVecGraph) -> HashMap<i64, (f64, f64, f64, f64)> {
    graph
        .boxes
        .iter()
        .map(|b| (b.id, (b.x, b.y, b.w, b.h)))
        .collect()
}

/// Does segment (x0,y0)→(x1,y1) pass through any **big box not in exclude** (inset RECT_PAD)
fn path_blocked(
    from: (f64, f64),
    to: (f64, f64),
    rects: &HashMap<i64, (f64, f64, f64, f64)>,
    exclude: &HashSet<i64>,
) -> bool {
    for (id, &(x, y, w, h)) in rects {
        if exclude.contains(id) {
            continue;
        }
        if w < MIN_BLOCKER_DIM || h < MIN_BLOCKER_DIM {
            continue; // small devices don't count as obstacles
        }
        if seg_intersects_rect(from, to, x, y, w, h) {
            return true;
        }
    }
    false
}

/// Segment and AABB intersection check (Liang-Barsky clipping; box inset RECT_PAD tolerates edge-touching)
fn seg_intersects_rect(
    from: (f64, f64),
    to: (f64, f64),
    rx: f64,
    ry: f64,
    rw: f64,
    rh: f64,
) -> bool {
    let (x0, y0) = from;
    let (x1, y1) = to;
    let dx = x1 - x0;
    let dy = y1 - y0;
    let (xmin, xmax) = (rx + RECT_PAD, rx + rw - RECT_PAD);
    let (ymin, ymax) = (ry + RECT_PAD, ry + rh - RECT_PAD);
    if xmax <= xmin || ymax <= ymin {
        return false; // box too small, treat as no obstruction
    }
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    let checks = [
        (-dx, x0 - xmin),
        (dx, xmax - x0),
        (-dy, y0 - ymin),
        (dy, ymax - y0),
    ];
    for (p, q) in checks {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return false; // parallel to this edge and outside box
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return false;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return false;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }
    t0 <= t1
}

/// Pick the most empty side among the two edges **perpendicular to preferred** main axis.
///
/// If neither side has enough clearance, return None (routing around won't help, keep original orientation).
fn open_perpendicular_side(
    b: (f64, f64, f64, f64),
    preferred: &EntrySide,
    rects: &HashMap<i64, (f64, f64, f64, f64)>,
    self_id: i64,
) -> Option<EntrySide> {
    let perp = match axis_of(preferred) {
        Axis::V => [EntrySide::Left, EntrySide::Right], // vertical main axis blocked → route horizontally
        Axis::H => [EntrySide::Top, EntrySide::Bottom], // horizontal main axis blocked → route vertically
    };
    let mut best: Option<(EntrySide, f64)> = None;
    for s in perp {
        let clr = side_clearance(b, &s, rects, self_id);
        if best.as_ref().is_none_or(|(_, c)| clr > *c) {
            best = Some((s, clr));
        }
    }
    best.filter(|(_, c)| *c >= MIN_CLEAR).map(|(s, _)| s)
}

/// Clear width from a box's edge outward to "nearest blocking box" (that box must overlap vertically with this box).
///
/// No blocking box → OPEN_FAR.
fn side_clearance(
    b: (f64, f64, f64, f64),
    side: &EntrySide,
    rects: &HashMap<i64, (f64, f64, f64, f64)>,
    self_id: i64,
) -> f64 {
    let (bx, by, bw, bh) = b;
    let mut gap = OPEN_FAR;
    for (id, &(x, y, w, h)) in rects {
        if *id == self_id {
            continue;
        }
        let g = match side {
            EntrySide::Right => {
                if y < by + bh && y + h > by && x >= bx + bw {
                    x - (bx + bw)
                } else {
                    continue;
                }
            }
            EntrySide::Left => {
                if y < by + bh && y + h > by && x + w <= bx {
                    bx - (x + w)
                } else {
                    continue;
                }
            }
            EntrySide::Bottom => {
                if x < bx + bw && x + w > bx && y >= by + bh {
                    y - (by + bh)
                } else {
                    continue;
                }
            }
            EntrySide::Top => {
                if x < bx + bw && x + w > bx && y + h <= by {
                    by - (y + h)
                } else {
                    continue;
                }
            }
        };
        if g < gap {
            gap = g;
        }
    }
    gap
}

// ============================================================================
// Internal: look up which pins each box uses from nets
// ============================================================================

/// Scan `graph.nets` once, get box_id → [(pin_id, pin_name)]
///
/// Same pin appearing in multiple nets counts only once. Synthetic "rail" endpoints with pin_id <= 0
/// (`pin_id: -1`) don't count (they have no real pins).
///
/// ## ★ P03 (S1) changes
/// Previously read both `graph.edges` (old binary) + `graph.nets` (new), P03 removed edges path,
/// now only traverses nets. `McVecEdge` field kept but no longer populated, this function no longer scans it.
fn collect_pins_per_box(graph: &McVecGraph) -> HashMap<i64, Vec<(i64, String)>> {
    let mut out: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
    let mut seen: HashMap<i64, HashSet<i64>> = HashMap::new();

    for net in &graph.nets {
        for ep in &net.endpoints {
            // skip synthetic endpoints (rail-synth produced pin_id=-1) and invalid pins
            if ep.pin_id <= 0 {
                continue;
            }
            let set = seen.entry(ep.box_id).or_default();
            if set.insert(ep.pin_id) {
                out.entry(ep.box_id)
                    .or_default()
                    .push((ep.pin_id, ep.pin_name.clone()));
            }
        }
    }

    out
}

// ============================================================================
// Internal: dispatch by BoxKind
// ============================================================================

fn compute_entry_points(
    b: &McVecBox,
    pins: &[(i64, String)],
    connected: &HashSet<i64>,
) -> Vec<EntryPoint> {
    // ★ Reserved interface ①: if component gives explicit layout hint, use it (builder doesn't fill today → None → skip).
    if let Some(layout) = &b.layout_hint {
        if !layout.is_empty() {
            return ep_from_layout(b, pins, layout);
        }
    }
    match b.kind {
        BoxKind::PowerLabel => ep_for_power_label(b),
        BoxKind::TwoPin => ep_for_two_pin(pins),
        BoxKind::MultiPin => ep_for_multi_pin(pins, connected),
        BoxKind::SubModule => ep_for_sub_module(pins, connected),
    }
}

/// ★ Reserved interface ① consumer: distribute pins to four sides according to component's explicit layout.
///
/// Matching rules: for each pin, use its `BoxPin.pin_id` (number) or `description` (function name) to look up side in `layout`
/// ([`PinLayout::side_of`]). Matched pins are evenly distributed on that side following layout order;
/// unmatched pins fall back to original heuristic (by kind), ensuring no pins are missed.
fn ep_from_layout(
    b: &McVecBox,
    pins: &[(i64, String)],
    layout: &crate::vector::graph::box_def::PinLayout,
) -> Vec<EntryPoint> {
    use std::collections::BTreeMap;

    // Validate: warn about layout entries that don't match any actual pin on this box
    for side_entries in [
        ("left", &layout.left),
        ("right", &layout.right),
        ("top", &layout.top),
        ("bottom", &layout.bottom),
    ] {
        for entry in side_entries.1 {
            let matched = b
                .pins
                .iter()
                .any(|p| &p.pin_id == entry || &p.description == entry);
            if !matched {
                eprintln!(
                    "[layout] WARN: box '{}' layout {} references pin '{}' which does not exist on this box — ignored",
                    b.name, side_entries.0, entry
                );
            }
        }
    }

    // side → pins on that side (in layout order), encode side as 0/1/2/3 for BTreeMap stable sorting
    let mut by_side: BTreeMap<u8, Vec<(i64, String)>> = BTreeMap::new();
    let mut leftover: Vec<(i64, String)> = Vec::new();

    let side_key = |s: &EntrySide| -> u8 {
        match s {
            EntrySide::Top => 0,
            EntrySide::Right => 1,
            EntrySide::Bottom => 2,
            EntrySide::Left => 3,
        }
    };

    for (id, label) in pins {
        // Two matching keys for this pin: number (pin_id) and function name (description)
        let matched_side = b.find_pin(*id).and_then(|p| {
            layout
                .side_of(&p.pin_id)
                .or_else(|| layout.side_of(&p.description))
        });
        match matched_side {
            Some(s) => by_side
                .entry(side_key(&s))
                .or_default()
                .push((*id, label.clone())),
            None => leftover.push((*id, label.clone())),
        }
    }

    let side_from_key = |k: u8| -> EntrySide {
        match k {
            0 => EntrySide::Top,
            1 => EntrySide::Right,
            2 => EntrySide::Bottom,
            _ => EntrySide::Left,
        }
    };

    let mut out = Vec::new();
    for (k, list) in &by_side {
        let side = side_from_key(*k);
        let n = list.len();
        for (i, (id, label)) in list.iter().enumerate() {
            out.push(EntryPoint {
                pin_id: *id,
                pin_name: label.clone(),
                side: side.clone(),
                offset: (i as f64 + 1.0) / (n as f64 + 1.0),
            });
        }
    }

    // Pins not covered by layout: fall back to heuristic, avoid missed drawing. (User explicit layout path; treat remaining pins as
    //  normally connected — pass empty connection set → degraded protection handles as "all connected".)
    if !leftover.is_empty() {
        let no_conn: HashSet<i64> = HashSet::new();
        let fallback = match b.kind {
            BoxKind::TwoPin => ep_for_two_pin(&leftover),
            BoxKind::SubModule => ep_for_sub_module(&leftover, &no_conn),
            _ => ep_for_multi_pin(&leftover, &no_conn),
        };
        out.extend(fallback);
    }
    out
}

// ============================================================================
// Pin role classification (by name)
//
// **P04 (S1)**: implementation migrated to `super::naming::pin_role`
// This module's `PinRole` is for entry-side allocation with 4 categories (Power/Ground/Input/Output/Generic),
// `naming::NameRole` has more granular 7 categories (plus Clock/Reset). The two convert via `from_name_role`.
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PinRole {
    Power,   // VCC/VDD/V3V3 ... → Top
    Ground,  // GND/VSS ...      → Bottom
    Input,   // *_IN/RX/MOSI/SCL/CLK/RST ... → Left
    Output,  // *_OUT/TX/MISO/INT ...        → Right
    Generic, // digital / unclassifiable  → evenly distribute Left/Right
}

fn classify_pin(name: &str) -> PinRole {
    use crate::vector::graph::naming::NameRole;
    match crate::vector::graph::naming::pin_role(name) {
        NameRole::Power => PinRole::Power,
        NameRole::Ground => PinRole::Ground,
        // Clock / Reset / Input all treated as input side
        NameRole::Input | NameRole::Clock | NameRole::Reset => PinRole::Input,
        NameRole::Output => PinRole::Output,
        NameRole::Generic => PinRole::Generic,
    }
}

// ============================================================================
// Allocation strategies by BoxKind
// ============================================================================

/// PowerLabel: single exit point
///
/// - Name contains GND/VSS → exit point at **top** (rail label hangs below circuit, wire enters from top)
/// - Otherwise (VCC/VDD/...) → exit point at **bottom** (rail label hangs above circuit, wire enters from bottom)
fn ep_for_power_label(b: &McVecBox) -> Vec<EntryPoint> {
    let u = b.name.to_uppercase();
    let is_ground = u.contains("GND") || u.contains("VSS");
    let side = if is_ground {
        EntrySide::Top
    } else {
        EntrySide::Bottom
    };
    vec![EntryPoint {
        pin_id: b.id,
        pin_name: b.name.clone(),
        side,
        offset: 0.5,
    }]
}

/// TwoPin (R/C/L/D etc): two pins on left/right
///
/// No real "input side / output side" concept, simply one pin on each side.
fn ep_for_two_pin(pins: &[(i64, String)]) -> Vec<EntryPoint> {
    // Stable sort by name (avoid HashMap source randomness)
    let mut sorted = pins.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    for (i, (pid, pname)) in sorted.iter().take(2).enumerate() {
        let side = if i == 0 {
            EntrySide::Left
        } else {
            EntrySide::Right
        };
        out.push(EntryPoint {
            pin_id: *pid,
            pin_name: pname.clone(),
            side,
            offset: 0.5,
        });
    }
    out
}

/// MultiPin (multi-pin IC): distribute pins to four sides by pin role
///
/// 1. Power → Top, Ground → Bottom
/// 2. Input → Left, Output → Right
/// 3. Generic → evenly distribute between Left / Right (odd index goes Right)
/// 4. On same side, N pins evenly distributed with `offset = (i + 0.5) / N`
fn ep_for_multi_pin(pins: &[(i64, String)], connected: &HashSet<i64>) -> Vec<EntryPoint> {
    // Professional IC symbol layout: pins **all on left/right sides**, evenly spread, not stacked on top/bottom.
    //   - Top/bottom edges in auto mode **no pins** (reserved for power/ground flags by place_flags, and future overflow).
    //     Previously "throwing unconnected pins to top/bottom" caused long-named ports to crowd at narrow box's top/bottom edges, overlapping —
    //     exactly the mcu513 top/bottom cluster in the screenshot. After changing to all-left/right, pins spread along height direction and won't crowd.
    //   - Connected pins placed at each side's **front** (core connection position), unconnected pins at each side's **end** (secondary position),
    //     but both on left/right, no conflict or waste.
    //   - Left/right count strictly balanced (alternating distribution), prevent same-direction clustering.
    // Degraded protection: box has no connections at all (standalone component, like attr01) → treat all as connected, spread normally on left/right.
    let any_connected = pins.iter().any(|p| connected.contains(&p.0));
    let is_conn = |id: i64| !any_connected || connected.contains(&id);

    let mut conn: Vec<&(i64, String)> = Vec::new();
    let mut dead: Vec<&(i64, String)> = Vec::new();
    for p in pins {
        if is_conn(p.0) {
            conn.push(p);
        } else {
            dead.push(p);
        }
    }

    // Connected pins sorted by (role order, pin_id): power/input first (upper part of each side), output/ground later.
    //   Only determines **same-side vertical order**, doesn't affect "all on left/right".
    fn role_rank(name: &str) -> u8 {
        match classify_pin(name) {
            PinRole::Power => 0,
            PinRole::Input => 1,
            PinRole::Generic => 2,
            PinRole::Output => 3,
            PinRole::Ground => 4,
        }
    }
    conn.sort_by(|a, b| role_rank(&a.1).cmp(&role_rank(&b.1)).then(a.0.cmp(&b.0)));
    dead.sort_by(|a, b| a.0.cmp(&b.0));

    // Alternating left/right distribution: connected first (occupying each side's front), then unconnected (each side's end).
    // No longer rearrange per side — preserve "connected in front / unconnected behind" vertical order.
    let mut left: Vec<&(i64, String)> = Vec::new();
    let mut right: Vec<&(i64, String)> = Vec::new();
    for (i, p) in conn.iter().chain(dead.iter()).enumerate() {
        if i % 2 == 0 {
            left.push(p);
        } else {
            right.push(p);
        }
    }

    let mut out = Vec::new();
    push_side(&mut out, &left, EntrySide::Left);
    push_side(&mut out, &right, EntrySide::Right);
    out
}

/// SubModule: ports use same strategy as MultiPin (aligned with main module, no longer different allocation logic).
fn ep_for_sub_module(pins: &[(i64, String)], connected: &HashSet<i64>) -> Vec<EntryPoint> {
    ep_for_multi_pin(pins, connected)
}

// ============================================================================
// Evenly distribute pins on same side
// ============================================================================

fn push_side(out: &mut Vec<EntryPoint>, pins: &[&(i64, String)], side: EntrySide) {
    let n = pins.len();
    if n == 0 {
        return;
    }
    for (i, p) in pins.iter().enumerate() {
        let offset = (i as f64 + 0.5) / n as f64;
        out.push(EntryPoint {
            pin_id: p.0,
            pin_name: p.1.clone(),
            side: side.clone(),
            offset,
        });
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_power() {
        assert_eq!(classify_pin("VCC"), PinRole::Power);
        assert_eq!(classify_pin("VDD"), PinRole::Power);
        assert_eq!(classify_pin("V3V3"), PinRole::Power);
        assert_eq!(classify_pin("V5V0"), PinRole::Power);
        assert_eq!(classify_pin("VBUS"), PinRole::Power);
    }

    #[test]
    fn test_classify_ground() {
        assert_eq!(classify_pin("GND"), PinRole::Ground);
        assert_eq!(classify_pin("VSS"), PinRole::Ground);
        assert_eq!(classify_pin("AGND"), PinRole::Ground);
        assert_eq!(classify_pin("DGND"), PinRole::Ground);
    }

    #[test]
    fn test_classify_io() {
        assert_eq!(classify_pin("MOSI"), PinRole::Input);
        assert_eq!(classify_pin("MISO"), PinRole::Output);
        assert_eq!(classify_pin("RX"), PinRole::Input);
        assert_eq!(classify_pin("TX"), PinRole::Output);
        assert_eq!(classify_pin("CLK"), PinRole::Input);
        assert_eq!(classify_pin("INT"), PinRole::Output);
        assert_eq!(classify_pin("DATA_IN"), PinRole::Input);
        assert_eq!(classify_pin("ADDR_OUT"), PinRole::Output);
    }

    #[test]
    fn test_classify_generic() {
        // pure numbers (pin numbers)
        assert_eq!(classify_pin("1"), PinRole::Generic);
        assert_eq!(classify_pin("14"), PinRole::Generic);
        // generic GPIO names
        assert_eq!(classify_pin("PA0"), PinRole::Generic);
        assert_eq!(classify_pin("GPIO5"), PinRole::Generic);
    }

    #[test]
    fn test_offset_distribution() {
        // 4 generic pins → 2 left, 2 right
        let pins = vec![
            (1, "1".to_string()),
            (2, "2".to_string()),
            (3, "3".to_string()),
            (4, "4".to_string()),
        ];
        let connected: HashSet<i64> = [1, 2, 3, 4].into_iter().collect();
        let eps = ep_for_multi_pin(&pins, &connected);
        assert_eq!(eps.len(), 4);

        let left: Vec<&EntryPoint> = eps.iter().filter(|e| e.side == EntrySide::Left).collect();
        let right: Vec<&EntryPoint> = eps.iter().filter(|e| e.side == EntrySide::Right).collect();
        assert_eq!(left.len(), 2);
        assert_eq!(right.len(), 2);

        // same-side offsets should be 0.25 and 0.75
        let left_offsets: Vec<f64> = left.iter().map(|e| e.offset).collect();
        assert!(left_offsets.contains(&0.25));
        assert!(left_offsets.contains(&0.75));
    }

    #[test]
    fn test_mixed_roles() {
        // simulate MCU: VCC + GND + RX + TX + 2 GPIO
        let pins = vec![
            (1, "VCC".to_string()),
            (2, "GND".to_string()),
            (3, "RX".to_string()),
            (4, "TX".to_string()),
            (5, "PA0".to_string()),
            (6, "PA1".to_string()),
        ];
        let connected: HashSet<i64> = [1, 2, 3, 4, 5, 6].into_iter().collect();
        let eps = ep_for_multi_pin(&pins, &connected);
        assert_eq!(eps.len(), 6);

        let by_id: HashMap<i64, &EntryPoint> = eps.iter().map(|e| (e.pin_id, e)).collect();
        // Auto MultiPin layout keeps all pins on left/right edges; power/ground
        // role only affects same-side ordering.
        assert!(matches!(by_id[&1].side, EntrySide::Left | EntrySide::Right)); // VCC
        assert!(matches!(by_id[&2].side, EntrySide::Left | EntrySide::Right)); // GND
        assert!(matches!(by_id[&3].side, EntrySide::Left | EntrySide::Right)); // RX
        assert!(matches!(by_id[&4].side, EntrySide::Left | EntrySide::Right)); // TX
        assert!(matches!(by_id[&5].side, EntrySide::Left | EntrySide::Right)); // GPIO
        assert!(matches!(by_id[&6].side, EntrySide::Left | EntrySide::Right)); // GPIO

        let left_count = eps.iter().filter(|e| e.side == EntrySide::Left).count();
        let right_count = eps.iter().filter(|e| e.side == EntrySide::Right).count();
        assert_eq!(left_count, 3);
        assert_eq!(right_count, 3);
    }

    #[test]
    fn test_power_label() {
        let vcc_box = McVecBox::new(
            10,
            "VCC".into(),
            String::new(),
            BoxKind::PowerLabel,
            0,
            crate::vector::graph::IoSummary::new(),
        );
        let gnd_box = McVecBox::new(
            11,
            "GND".into(),
            String::new(),
            BoxKind::PowerLabel,
            0,
            crate::vector::graph::IoSummary::new(),
        );
        let vcc_eps = ep_for_power_label(&vcc_box);
        let gnd_eps = ep_for_power_label(&gnd_box);
        assert_eq!(vcc_eps[0].side, EntrySide::Bottom);
        assert_eq!(gnd_eps[0].side, EntrySide::Top);
    }

    // ========================================================================
    // ★ P06 (S5) refinement round tests
    // ========================================================================

    use crate::vector::graph::net_def::IoDirection;
    use crate::vector::graph::{EndpointRef, NetKind, VizNet};

    /// Tool: create a box with coordinates and fill in an entry_point
    fn mk_box_with_ep(
        id: i64,
        name: &str,
        x: f64,
        y: f64,
        ep_pin: i64,
        ep_side: EntrySide,
    ) -> McVecBox {
        let mut b = McVecBox::new(
            id,
            name.into(),
            String::new(),
            BoxKind::MultiPin,
            1,
            crate::vector::graph::IoSummary::new(),
        );
        b.x = x;
        b.y = y;
        b.w = 100.0;
        b.h = 60.0;
        b.entry_points.push(EntryPoint {
            pin_id: ep_pin,
            pin_name: format!("p{}", ep_pin),
            side: ep_side,
            offset: 0.5,
        });
        b
    }

    #[test]
    fn layout_hint_assigns_sides() {
        use crate::vector::graph::box_def::{BoxPin, PinLayout};
        let mut b = McVecBox::new(
            1,
            "u8".into(),
            "ps8_transistor".into(),
            BoxKind::MultiPin,
            4,
            crate::vector::graph::IoSummary::new(),
        );
        b.set_pins(vec![
            BoxPin {
                id: 1,
                pin_id: "B".into(),
                description: "Base".into(),
                io: IoDirection::Bidir,
            },
            BoxPin {
                id: 2,
                pin_id: "C".into(),
                description: "Collector".into(),
                io: IoDirection::Bidir,
            },
            BoxPin {
                id: 3,
                pin_id: "E".into(),
                description: "Emmiter".into(),
                io: IoDirection::Bidir,
            },
            BoxPin {
                id: 4,
                pin_id: "4".into(),
                description: String::new(),
                io: IoDirection::Unknown,
            },
        ]);
        b.set_layout_hint(PinLayout {
            left: vec!["B".into()],
            right: vec!["Collector".into()], // match by function name (description)
            top: vec!["E".into()],
            bottom: vec![],
        });

        let pins = vec![
            (1, "Base".to_string()),
            (2, "Collector".to_string()),
            (3, "Emmiter".to_string()),
            (4, "4".to_string()),
        ];
        // layout_hint takes priority, connected not used in this branch; pass empty set.
        let connected: HashSet<i64> = HashSet::new();
        let eps = compute_entry_points(&b, &pins, &connected);
        let side_of = |pid: i64| eps.iter().find(|e| e.pin_id == pid).map(|e| e.side.clone());

        assert_eq!(side_of(1), Some(EntrySide::Left)); // by pin_id "B"
        assert_eq!(side_of(2), Some(EntrySide::Right)); // by description "Collector"
        assert_eq!(side_of(3), Some(EntrySide::Top)); // by pin_id "E"
        assert!(
            side_of(4).is_some(),
            "pins not in layout should also be drawn (fallback heuristic)"
        );
    }

    #[test]
    fn empty_layout_hint_falls_back() {
        // empty layout set still results in None → use heuristic, behavior unchanged.
        use crate::vector::graph::box_def::PinLayout;
        let mut b = McVecBox::new(
            1,
            "u".into(),
            "c".into(),
            BoxKind::MultiPin,
            0,
            crate::vector::graph::IoSummary::new(),
        );
        b.set_layout_hint(PinLayout::default());
        assert!(b.layout_hint.is_none());
    }

    /// Tool: create a net connecting two pins, with specified io_type for endpoints
    fn mk_net_with_io(
        nid: i64,
        a_box: i64,
        a_pin: i64,
        a_io: IoDirection,
        b_box: i64,
        b_pin: i64,
        b_io: IoDirection,
    ) -> VizNet {
        VizNet::new(
            nid,
            format!("n{}", nid),
            NetKind::Signal,
            vec![
                EndpointRef::with_io(a_box, a_pin, format!("p{}", a_pin), a_io),
                EndpointRef::with_io(b_box, b_pin, format!("p{}", b_pin), b_io),
            ],
        )
    }

    #[test]
    fn p06_refine_moves_passive_pin_toward_neighbor() {
        // box 1 on left, box 2 on right; box 1's pin 5 (Passive, coarse assigned to Left)
        // should be switched to Right by refine
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes
            .push(mk_box_with_ep(1, "L", 0.0, 0.0, 5, EntrySide::Left));
        g.boxes
            .push(mk_box_with_ep(2, "R", 300.0, 0.0, 7, EntrySide::Left));
        g.nets.push(mk_net_with_io(
            10,
            1,
            5,
            IoDirection::Passive,
            2,
            7,
            IoDirection::Passive,
        ));

        assign_entry_points_refine(&mut g);

        let b1 = g.boxes.iter().find(|b| b.id == 1).unwrap();
        assert_eq!(
            b1.entry_points[0].side,
            EntrySide::Right,
            "Passive pin should re-pin to face neighbor on the right"
        );
    }

    #[test]
    fn p06_refine_does_not_move_power_pin() {
        // Power pin should keep Top from coarse even if neighbor is on right
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes
            .push(mk_box_with_ep(1, "src", 0.0, 0.0, 5, EntrySide::Top));
        g.boxes
            .push(mk_box_with_ep(2, "dst", 300.0, 0.0, 7, EntrySide::Left));
        g.nets.push(mk_net_with_io(
            10,
            1,
            5,
            IoDirection::Power,
            2,
            7,
            IoDirection::Passive,
        ));

        assign_entry_points_refine(&mut g);
        let b1 = g.boxes.iter().find(|b| b.id == 1).unwrap();
        assert_eq!(
            b1.entry_points[0].side,
            EntrySide::Top,
            "Power pin should NOT move regardless of neighbor"
        );
    }

    #[test]
    fn p06_refine_does_not_move_input_output_pins() {
        // Input/Output also keep coarse-assigned side (semantics take priority)
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes
            .push(mk_box_with_ep(1, "src", 0.0, 0.0, 5, EntrySide::Left));
        g.boxes
            .push(mk_box_with_ep(2, "dst", 300.0, 0.0, 7, EntrySide::Right));
        g.nets.push(mk_net_with_io(
            10,
            1,
            5,
            IoDirection::Input,
            2,
            7,
            IoDirection::Output,
        ));

        assign_entry_points_refine(&mut g);
        let b1 = g.boxes.iter().find(|b| b.id == 1).unwrap();
        let b2 = g.boxes.iter().find(|b| b.id == 2).unwrap();
        assert_eq!(b1.entry_points[0].side, EntrySide::Left);
        assert_eq!(b2.entry_points[0].side, EntrySide::Right);
    }

    #[test]
    fn p06_refine_moves_bidir_pin() {
        // Bidir also allows reassignment
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes
            .push(mk_box_with_ep(1, "L", 0.0, 0.0, 5, EntrySide::Left));
        g.boxes
            .push(mk_box_with_ep(2, "R", 300.0, 0.0, 7, EntrySide::Left));
        g.nets.push(mk_net_with_io(
            10,
            1,
            5,
            IoDirection::Bidir,
            2,
            7,
            IoDirection::Bidir,
        ));

        assign_entry_points_refine(&mut g);
        let b1 = g.boxes.iter().find(|b| b.id == 1).unwrap();
        assert_eq!(b1.entry_points[0].side, EntrySide::Right);
    }

    #[test]
    fn p06_refine_isolated_pin_no_change() {
        // pins without neighbors don't move (Passive but isolated)
        let mut g = McVecGraph::new(0, "test".into());
        g.boxes
            .push(mk_box_with_ep(1, "lonely", 0.0, 0.0, 5, EntrySide::Left));
        // no net

        assign_entry_points_refine(&mut g);
        let b1 = g.boxes.iter().find(|b| b.id == 1).unwrap();
        assert_eq!(b1.entry_points[0].side, EntrySide::Left);
    }

    #[test]
    fn p06_refine_normalizes_offsets() {
        // a box has 3 Passive pins, all on Left before (offset 0.25/0.5/0.75)
        // refine switches 2 to Right, remaining Left should have offset=0.5, Right should 0.25/0.75
        let mut g = McVecGraph::new(0, "test".into());
        let mut src = McVecBox::new(
            1,
            "src".into(),
            String::new(),
            BoxKind::MultiPin,
            3,
            crate::vector::graph::IoSummary::new(),
        );
        src.x = 0.0;
        src.y = 0.0;
        src.w = 100.0;
        src.h = 100.0;
        src.entry_points = vec![
            EntryPoint {
                pin_id: 1,
                pin_name: "p1".into(),
                side: EntrySide::Left,
                offset: 0.25,
            },
            EntryPoint {
                pin_id: 2,
                pin_name: "p2".into(),
                side: EntrySide::Left,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 3,
                pin_name: "p3".into(),
                side: EntrySide::Left,
                offset: 0.75,
            },
        ];
        g.boxes.push(src);
        // 1 dst on right
        let mut dst = McVecBox::new(
            2,
            "dst".into(),
            String::new(),
            BoxKind::MultiPin,
            2,
            crate::vector::graph::IoSummary::new(),
        );
        dst.x = 300.0;
        dst.y = 0.0;
        dst.w = 80.0;
        dst.h = 80.0;
        dst.entry_points = vec![
            EntryPoint {
                pin_id: 11,
                pin_name: "q1".into(),
                side: EntrySide::Left,
                offset: 0.5,
            },
            EntryPoint {
                pin_id: 12,
                pin_name: "q2".into(),
                side: EntrySide::Left,
                offset: 0.5,
            },
        ];
        g.boxes.push(dst);

        // pin 2 / pin 3 both connect to dst, pin 1 stays with self (no neighbor)
        g.nets.push(mk_net_with_io(
            100,
            1,
            2,
            IoDirection::Passive,
            2,
            11,
            IoDirection::Passive,
        ));
        g.nets.push(mk_net_with_io(
            101,
            1,
            3,
            IoDirection::Passive,
            2,
            12,
            IoDirection::Passive,
        ));

        assign_entry_points_refine(&mut g);

        let src = g.boxes.iter().find(|b| b.id == 1).unwrap();
        let by_id: HashMap<i64, &EntryPoint> =
            src.entry_points.iter().map(|e| (e.pin_id, e)).collect();
        // pin 1: no neighbor, keep Left
        assert_eq!(by_id[&1].side, EntrySide::Left);
        // pin 2, 3: switch to Right
        assert_eq!(by_id[&2].side, EntrySide::Right);
        assert_eq!(by_id[&3].side, EntrySide::Right);
        // same-side offset normalization: Left now has only 1 → offset = 0.5
        assert!((by_id[&1].offset - 0.5).abs() < 1e-6);
        // Right has 2 → offset = 0.25 + 0.75
        let mut right_offsets: Vec<f64> = src
            .entry_points
            .iter()
            .filter(|e| e.side == EntrySide::Right)
            .map(|e| e.offset)
            .collect();
        right_offsets.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!((right_offsets[0] - 0.25).abs() < 1e-6);
        assert!((right_offsets[1] - 0.75).abs() < 1e-6);
    }

    #[test]
    fn p06_pick_side_by_direction_axis_choice() {
        // dx larger → horizontal axis
        assert_eq!(pick_side_by_direction(100.0, 5.0), EntrySide::Right);
        assert_eq!(pick_side_by_direction(-100.0, 5.0), EntrySide::Left);
        // dy larger → vertical axis
        assert_eq!(pick_side_by_direction(5.0, 100.0), EntrySide::Bottom);
        assert_eq!(pick_side_by_direction(5.0, -100.0), EntrySide::Top);
        // tie favors horizontal axis (dx.abs >= dy.abs)
        assert_eq!(pick_side_by_direction(50.0, 50.0), EntrySide::Right);
    }

    #[test]
    fn p06_hysteresis_prevents_borderline_switch() {
        // current Left, neighbor at right 200, below 220 (dy dominates)
        // pick_side will return Bottom, but cross-axis switch requires new_axis >= 1.2 * cur_axis
        // cur (Left) axis = dx = 200, new (Bottom) axis = dy = 220
        // 220 / 200 = 1.1 < 1.2 → no switch
        assert!(!sides_warrant_switch(
            &EntrySide::Left,
            &EntrySide::Bottom,
            200.0,
            220.0
        ));
        // 220 vs 200, dy dominates but not strong enough → no switch (note pick_side_by_direction returns Bottom)
        // strengthen dy → should switch
        assert!(sides_warrant_switch(
            &EntrySide::Left,
            &EntrySide::Bottom,
            200.0,
            300.0 // 300 / 200 = 1.5 > 1.2
        ));
    }

    #[test]
    fn p06_same_axis_switch_always_allowed() {
        // switch from Left to Right (same H axis, just opposite direction) → always allowed
        assert!(sides_warrant_switch(
            &EntrySide::Left,
            &EntrySide::Right,
            -10.0, // dx tiny sign change
            5.0
        ));
        assert!(sides_warrant_switch(
            &EntrySide::Top,
            &EntrySide::Bottom,
            5.0,
            10.0
        ));
    }

    #[test]
    fn p06_is_repinnable_table() {
        assert!(!is_repinnable(IoDirection::Power));
        assert!(!is_repinnable(IoDirection::Ground));
        assert!(!is_repinnable(IoDirection::Input));
        assert!(!is_repinnable(IoDirection::Output));
        assert!(is_repinnable(IoDirection::Passive));
        assert!(is_repinnable(IoDirection::Bidir));
        assert!(is_repinnable(IoDirection::Unknown));
    }
}
