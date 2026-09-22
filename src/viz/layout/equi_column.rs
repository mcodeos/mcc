// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M4.1: pure column allocator — replaces frac-based member x.
//!
//! ## Layering (M4)
//!
//!   M3: TapRole -> place_members_for_topo (frac x) -> envelop_lanes
//!   M4: TapRole -> [ColumnAllocator] -> place_members_by_columns -> envelop_lanes
//!                      ^ this module
//!
//! This module is PURE COMPUTE: it never reads a box rect (`x/y/w/h`), never
//! writes a member field, and keeps `TapRole` untouched. Members are described
//! to it as a flat [`MemberView`] list (role + dimensions + row y), which the
//! caller fills from the topology and placed anchor — the A2 guard (assign_rows
//! / assign_regions read no rect) therefore stays intact.
//!
//! The goal is a **single, one-way dependency chain** that kills the D4 cycle:
//!
//!   member width (from `assign_members`, runs first)
//!     -> column x values (width + clearance constants)
//!     -> trunk span (derived from the outermost columns)
//!
//! Nothing depends on the trunk, and the trunk is derived from the columns, so
//! the old `(member_count+1)*MEMBER_GAP` seed is gone.

use super::equipotential_tree::{TapRole, MEMBER_GAP};

/// Distance from the trunk's outer end to the first/last member centre.
///
/// ★ M6.5: this must exceed the DRAWN glyph half-width of a two-pin component
/// (resistor zigzag ~54px, inductor ~60px — half ~27..30), not just the layout's
/// `TWO_PIN_SYMBOL_H` box width (20). Otherwise a West member's body pokes into
/// the IC edge even though the occupancy table (which uses the box width) thinks
/// it clears.
pub const COL_MARGIN: f64 = 40.0;
/// Clearance between two member boxes (members are narrower than the old 60px
/// `MEMBER_GAP`, which inflated the trunk).
pub const COL_CLEAR: f64 = 16.0;
/// Side-allocation step: how far a member steps out (away from the anchor)
/// when its preferred column collides with an already-placed member.
pub const COL_STEP: f64 = 20.0;

/// One allocated column slot.
#[derive(Debug, Clone)]
pub struct ColSlot {
    pub col_idx: usize,
    pub member_idx: usize,
    pub width: f64,
    pub height: f64,
    /// The row range this column spans: `(y_lo, y_hi)` for a Bridge member,
    /// or a single row `(y, y)` for everyone else.
    pub row_span: (f64, f64),
}

/// The computed column layout for one net's members.
#[derive(Debug, Clone, Default)]
pub struct ColumnPlan {
    /// Slots, one per member (index = member_idx).
    pub slots: Vec<ColSlot>,
    /// `col_idx -> absolute x` of that column's centreline.
    pub x_values: Vec<f64>,
    /// Trunk span derived from the columns — NOT from a fake seed.
    pub span_lo: f64,
    pub span_hi: f64,
}

/// A member as seen by the allocator. `w`/`h` are the placed dimensions
/// (from `assign_members`); `row_y` is the member's own row y and `partner_y`
/// the other row a Bridge spans.
#[derive(Debug, Clone)]
pub struct MemberView {
    pub role: TapRole,
    pub w: f64,
    pub h: f64,
    pub row_y: f64,
    pub partner_y: Option<f64>,
}

/// Cross-row spanning member (Bridge): occupies two rows.
fn is_spanning(m: &MemberView) -> bool {
    matches!(m.role, TapRole::Bridge { .. })
}

/// Vertical single-row hang (decoupling/shunt).
fn is_shunt(m: &MemberView) -> bool {
    matches!(m.role, TapRole::Drop { .. })
}

/// Allocate integer columns for a net's members, then convert to absolute x.
///
/// `side` is the x growth direction: `-1` for West (members extend left of the
/// anchor tap), `+1` for East. `anchor_tap_pin_x` is the anchor pin's x (already
/// placed by `assign_anchor_slots`).
///
/// Greedy order: spanning (Bridge) members claim the low (anchor-adjacent)
/// columns first, shunt (vertical hang) members next, the rest fill the
/// remainder — all in deterministic input order. `x` is computed once by
/// packing member widths with `COL_CLEAR`; the trunk span is derived from the
/// outermost columns (the D4-free one-way chain).
pub fn allocate_columns(members: &[MemberView], anchor_tap_pin_x: f64, side: f64) -> ColumnPlan {
    let dir = if side < 0.0 { -1.0 } else { 1.0 };
    let n = members.len();
    if n == 0 {
        return ColumnPlan {
            slots: Vec::new(),
            x_values: Vec::new(),
            span_lo: anchor_tap_pin_x,
            span_hi: anchor_tap_pin_x,
        };
    }

    // Priority order: (priority, input idx). Bridge=0 (anchor-near), shunt=1,
    // everything else=2. Columns 0..n-1 are handed out in this order, so a
    // Bridge lands at the anchor-adjacent columns.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| {
        let prio = if is_spanning(&members[i]) {
            0
        } else if is_shunt(&members[i]) {
            1
        } else {
            2
        };
        (prio, i)
    });
    let mut col_of = vec![0usize; n];
    for (k, &i) in order.iter().enumerate() {
        col_of[i] = k;
    }

    // Pack x: col 0 starts one COL_MARGIN from the anchor pin, every further
    // column adds the previous member's half-width + COL_CLEAR + its own half.
    let mut x_values = vec![0.0f64; order.len()];
    let mut acc = 0.0;
    let mut prev_half = 0.0;
    for (k, &mi) in order.iter().enumerate() {
        let half = members[mi].w / 2.0;
        acc += if k == 0 {
            COL_MARGIN + half
        } else {
            prev_half + COL_CLEAR + half
        };
        x_values[k] = anchor_tap_pin_x + dir * acc;
        prev_half = half;
    }

    // Slots, indexed by member.
    let mut slots = Vec::with_capacity(n);
    for (i, m) in members.iter().enumerate() {
        let c = col_of[i];
        slots.push(ColSlot {
            col_idx: c,
            member_idx: i,
            width: m.w,
            height: m.h,
            row_span: match m.partner_y {
                Some(py) if (py - m.row_y).abs() > 1e-6 => (m.row_y.min(py), m.row_y.max(py)),
                _ => (m.row_y, m.row_y),
            },
        });
    }

    // Trunk span: one COL_MARGIN beyond the outermost member edges.
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for (i, m) in members.iter().enumerate() {
        let cx = x_values[col_of[i]];
        lo = lo.min(cx - m.w / 2.0 - COL_MARGIN);
        hi = hi.max(cx + m.w / 2.0 + COL_MARGIN);
    }

    ColumnPlan {
        slots,
        x_values,
        span_lo: lo,
        span_hi: hi,
    }
}

// M4.2b: global per-side column allocation

/// One member participating in a side-wide column allocation. `idx` is the
/// caller's opaque key (topo_idx, member_idx) packed by the caller.
#[derive(Debug, Clone)]
pub struct SideMember {
    pub idx: Option<(usize, usize)>,
    pub role: TapRole,
    pub w: f64,
    pub h: f64,
    pub row_y: f64,
    /// The net's anchor tap pin x this member grows from.
    pub anchor_pin_x: f64,
    /// ★ M18: the partner net's lane (the far end of this member's vertical
    /// tooth). `Some` ⇒ the allocator reserves the column strip from `row_y`
    /// to `partner_y` so no other member's body lands where this tooth runs —
    /// a cap's GND hang reaches the rail below, a bridge's tooth spans two
    /// rows. `None` = no tooth strip to reserve.
    pub partner_y: Option<f64>,
    /// ★ M18: x-intervals this member's tap must not fall inside. On those
    /// spans the net's trunk is DEFLECTED (a foreign body sits on the row), so
    /// a tooth hung there would run through the foreign glyph instead of
    /// reaching the row. Mirrors the carve's `foreign`/`must_deflect` set.
    pub blocked: Vec<(f64, f64)>,
    /// ★ M21 (U162①): the member's box position in its OWN net's endpoint
    /// list — the R0 reading order the signal_flow metric judges by. Members
    /// of one net sharing a tap origin are handed to the greedy in this
    /// order, so the part the author named first sits further west (east on
    /// a West side). `usize::MAX` = unknown (keep the old input order).
    pub read_rank: usize,
    /// ★ M22 (U162 residual): rank-window edges from ALREADY-PLACED
    /// co-endpoints of every net this box belongs to, INTERSECTED (a shared
    /// member must satisfy all its nets' R0 orders; conflicting windows fall
    /// back to the owning topo's own window). A co-endpoint ranked BEFORE
    /// this member bounds it from the west (its east edge); one ranked AFTER
    /// bounds it from the east (its west edge). Applied only when the band's
    /// default walk would park the member outside the window (see the gate in
    /// [`allocate_columns_for_side`]). `None` = unbounded.
    pub min_edge: Option<f64>,
    pub max_edge: Option<f64>,
}

/// Half-width of the tooth strip the allocator reserves for a vertical tooth.
/// The tooth is drawn ~1px wide; reserving 1px around its x is enough to catch
/// a foreign body whose interior the tooth would cross (bodies overlapping the
/// line by a full pixel are exactly the ones that cross it).
pub const TOOTH_EPS: f64 = 0.5;

/// Priority for side allocation: spanning/series first (anchor-near), then
/// vertical shunts, then the rest.
fn side_priority(role: &TapRole) -> u8 {
    match role {
        TapRole::Bridge { .. } | TapRole::Series { .. } => 0,
        TapRole::Drop { .. } => 1,
        _ => 2,
    }
}

/// Allocate columns for ALL members of one side at once. This is the correct
/// granularity: per-net calls each start from the same anchor x and pile all
/// col0 onto one x (the A21 collision). Here every member is placed against a
/// shared occupancy table, stepping `COL_STEP` along `dir` until it clears
/// every already-placed member in both axes.
///
/// Then M5.1 ([`reduce_crossings`]) re-orders the allocated columns so members
/// whose anchor taps are left of another's sit left of it too — killing the X
/// crossings between two nets' teeth (A24). The swap never introduces a new
/// collision (A21 stays green).
///
/// Returns `Vec<(idx, x)>` — the absolute centreline x for each member.
pub fn allocate_columns_for_side(members: &[SideMember], dir: f64) -> Vec<(usize, usize, f64)> {
    let dir = if dir < 0.0 { -1.0 } else { 1.0 };
    // ★ M22: the rank window is a reading-order constraint, not an allocation
    // origin — it fires only when the band's own walk would park the member on
    // the wrong side of an already-placed co-endpoint. A window the walk
    // already satisfies stays ignored: the member also rides other nets whose
    // reading order the unconstrained placement was chosen for, and dragging
    // it against those (e.g. a power symbol shared by the rail net and a
    // chain) is a regression this gate specifically closed.
    let members: Vec<SideMember> = members
        .iter()
        .map(|e| {
            let mut m = e.clone();
            if m.min_edge.is_some() || m.max_edge.is_some() {
                let half_w = m.w / 2.0 + COL_CLEAR / 2.0;
                let walk_cx = m.anchor_pin_x + dir * COL_MARGIN;
                if m.min_edge.is_some_and(|lo| walk_cx >= lo + half_w) {
                    m.min_edge = None;
                }
                if m.max_edge.is_some_and(|hi| walk_cx <= hi - half_w) {
                    m.max_edge = None;
                }
            }
            m
        })
        .collect();
    // Greedy order: priority, then anchor x, then R0 reading rank (stable).
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by(|&a, &b| {
        side_priority(&members[a].role)
            .cmp(&side_priority(&members[b].role))
            .then(
                members[a]
                    .anchor_pin_x
                    .partial_cmp(&members[b].anchor_pin_x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(members[a].read_rank.cmp(&members[b].read_rank))
            .then(a.cmp(&b))
    });
    // occupancy entry: (member_idx, x_lo, x_hi, y_lo, y_hi) — the member idx is
    // kept so `reduce_crossings` can exclude the two swaps' own slots.
    let mut occupied: Vec<(usize, f64, f64, f64, f64)> = Vec::new();
    // ★ M18: placed vertical teeth, kept SEPARATE from `occupied` so
    // `reduce_crossings` (which reasons about body boxes) is untouched.
    let mut teeth: Vec<(usize, f64, f64, f64, f64)> = Vec::new();
    let mut result: Vec<f64> = vec![0.0; members.len()];
    for &i in &order {
        let e = &members[i];
        let half_w = e.w / 2.0 + COL_CLEAR / 2.0;
        // vertical extent: conservative — allow room for an up or down hang.
        let y_lo = e.row_y - MEMBER_GAP - e.h;
        let y_hi = e.row_y + MEMBER_GAP + e.h;
        // ★ M18: the member's own tooth strip — the column from this row down
        // (or up) to its partner's row.
        let tooth_y = e.partner_y.map(|py| {
            let (tlo, thi) = (e.row_y.min(py), e.row_y.max(py));
            (tlo, thi)
        });
        // Step outward (away from the anchor, along `dir`) until a candidate
        // clears every already-placed member in both axes.
        let mut k = 0usize;
        loop {
            // First member sits one COL_MARGIN outside the anchor edge; every
            // further step moves COL_STEP outward. Without the margin the first
            // East member lands exactly on the IC edge and its tooth runs
            // collinear with the IC border (A18).
            //
            // ★ M22: a ranked member allocates from its window edge instead —
            // east of a lower-rank placed co-endpoint, west of a higher-rank
            // one — because the R0 reading order outranks the band's walk
            // direction (a last-rank member on a West band belongs between the
            // outermost joint and the anchor, not further west).
            let (base, step) = if let Some(lo) = e.min_edge {
                (lo + half_w, 1.0)
            } else if let Some(hi) = e.max_edge {
                (hi - half_w, -1.0)
            } else {
                (e.anchor_pin_x + dir * COL_MARGIN, dir)
            };
            let cx = base + step * (k as f64 * COL_STEP);
            let x_lo = cx - half_w;
            let x_hi = cx + half_w;
            // ★ M18: never hang a member where its net's trunk is deflected —
            // the tooth there runs through the foreign body, not to the row.
            let deflected = e.blocked.iter().any(|&(blo, bhi)| cx > blo && cx < bhi);
            let collides = deflected
                || occupied.iter().any(|&(_, oxl, oxh, oyl, oyh)| {
                    // candidate body vs placed body.
                    if x_lo < oxh && x_hi > oxl && y_lo < oyh && y_hi > oyl {
                        return true;
                    }
                    // candidate tooth vs placed body (a cap hung below a
                    // divider whose body the tooth would cross).
                    tooth_y.map_or(false, |(tlo, thi)| {
                        cx - TOOTH_EPS < oxh && cx + TOOTH_EPS > oxl && tlo < oyh && thi > oyl
                    })
                })
                || teeth.iter().any(|&(_, oxl, oxh, oyl, oyh)| {
                    // candidate body vs placed tooth (a member whose body a
                    // tooth from an earlier pass would cross).
                    x_lo < oxh && x_hi > oxl && y_lo < oyh && y_hi > oyl
                });
            if !collides {
                occupied.push((i, x_lo, x_hi, y_lo, y_hi));
                if let Some((tlo, thi)) = tooth_y {
                    teeth.push((i, cx - TOOTH_EPS, cx + TOOTH_EPS, tlo, thi));
                }
                result[i] = cx;
                break;
            }
            k += 1;
            if k > 64 {
                // Degenerate: force the current spot and move on.
                occupied.push((i, x_lo, x_hi, y_lo, y_hi));
                if let Some((tlo, thi)) = tooth_y {
                    teeth.push((i, cx - TOOTH_EPS, cx + TOOTH_EPS, tlo, thi));
                }
                result[i] = cx;
                break;
            }
        }
    }
    reduce_crossings(&members, dir, &mut result, &mut occupied);
    members
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.idx.map(|id| (id.0, id.1, result[i])))
        .collect()
}

/// M5.1: post-hoc crossing reduction. After the greedy allocation, sort the
/// member indices by their ANCHOR tap x (the allocator's order); where two
/// adjacent anchors sit one left of the other but their allocated columns came
/// out the opposite way, swap the two columns — if the swap introduces no new
/// collision with any OTHER member (and the two don't collide with each other)
/// or a foreign wire) it is accepted. The membership index on every occupancy
/// entry lets us skip the two members' own slots while checking.
fn reduce_crossings(
    members: &[SideMember],
    dir: f64,
    result: &mut [f64],
    occupied: &mut [(usize, f64, f64, f64, f64)],
) {
    let dir = if dir < 0.0 { -1.0 } else { 1.0 };
    let n = members.len();
    // Members whose anchor taps must stay in left-to-right column order.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        members[a]
            .anchor_pin_x
            .partial_cmp(&members[b].anchor_pin_x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    // One bubble pass down the anchor-ordered chain.
    for i in 0..order.len().saturating_sub(1) {
        let ia = order[i];
        let ib = order[i + 1];
        // Same anchor edge: no forced order, no swap.
        if (members[ia].anchor_pin_x - members[ib].anchor_pin_x).abs() < 0.5 {
            continue;
        }
        // ★ M22: a windowed member's column is pinned by its rank against
        // already-placed co-endpoints — a swap would violate the window.
        if members[ia].min_edge.is_some()
            || members[ia].max_edge.is_some()
            || members[ib].min_edge.is_some()
            || members[ib].max_edge.is_some()
        {
            continue;
        }
        // "Right of" in boost steps means toward +x; two members are inverted
        // when the one with the smaller anchor ends up further along `dir`.
        let xa = result[ia];
        let xb = result[ib];
        let inverted = (xa > xb && dir > 0.0) || (xa < xb && dir < 0.0);
        if !inverted {
            continue;
        }
        let ha = members[ia].w / 2.0 + COL_CLEAR / 2.0;
        let hb = members[ib].w / 2.0 + COL_CLEAR / 2.0;
        let (ya_lo, ya_hi) = box_y_extent(&members[ia]);
        let (yb_lo, yb_hi) = box_y_extent(&members[ib]);
        // candidate positions (swapped)
        let new_xa = xb;
        let new_xb = xa;
        // mutual clearance after the swap.
        if rects_overlap(
            (new_xa - ha, ya_lo, 2.0 * ha, ya_hi - ya_lo),
            (new_xb - hb, yb_lo, 2.0 * hb, yb_hi - yb_lo),
        ) {
            continue;
        }
        // clearance against every other member.
        let mutual_clear = |c_x: f64, half: f64, y_lo: f64, y_hi: f64| -> bool {
            occupied.iter().all(|&(om, oxl, oxh, oyl, oyh)| {
                if om == ia || om == ib {
                    return true;
                }
                let x_lo = c_x - half;
                let x_hi = c_x + half;
                !(x_lo < oxh && x_hi > oxl && y_lo < oyh && y_hi > oyl)
            })
        };
        if mutual_clear(new_xa, ha, ya_lo, ya_hi) && mutual_clear(new_xb, hb, yb_lo, yb_hi) {
            result[ia] = new_xa;
            result[ib] = new_xb;
            for o in occupied.iter_mut() {
                if o.0 == ia {
                    *o = (ia, new_xa - ha, new_xa + ha, ya_lo, ya_hi);
                } else if o.0 == ib {
                    *o = (ib, new_xb - hb, new_xb + hb, yb_lo, yb_hi);
                }
            }
        }
    }
}

fn box_y_extent(m: &SideMember) -> (f64, f64) {
    (m.row_y - MEMBER_GAP - m.h, m.row_y + MEMBER_GAP + m.h)
}

fn rects_overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(role: TapRole, w: f64, h: f64, row_y: f64, partner_y: Option<f64>) -> MemberView {
        MemberView {
            role,
            w,
            h,
            row_y,
            partner_y,
        }
    }

    #[test]
    fn same_anchor_members_do_not_collide() {
        // Two West nets, each with one vertical shunt on its own row, both
        // anchored at the same IC edge x=100. Per-net calls would give both
        // col0 at the same x; the side-wide allocator must separate them.
        let members = vec![
            SideMember {
                idx: Some((0, 0)),
                role: TapRole::Drop { dir: 1.0 },
                w: 20.0,
                h: 60.0,
                row_y: 100.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
            SideMember {
                idx: Some((1, 0)),
                role: TapRole::Drop { dir: 1.0 },
                w: 20.0,
                h: 60.0,
                row_y: 200.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
        ];
        let out = allocate_columns_for_side(&members, -1.0);
        assert_eq!(out.len(), 2);
        let (_, _, x0) = out[0];
        let (_, _, x1) = out[1];
        assert!(
            (x0 - x1).abs() > 1.0,
            "same-anchor members must not share x"
        );
    }

    #[test]
    fn x_monotonic_and_span_covers_first_last() {
        // A Bridge spanning rows 100/200 plus one shunt on row 100, West side.
        let members = vec![
            mv(
                TapRole::Bridge {
                    partner: 1,
                    dir: 1.0,
                },
                20.0,
                60.0,
                100.0,
                Some(200.0),
            ),
            mv(TapRole::Drop { dir: 1.0 }, 60.0, 20.0, 100.0, None),
        ];
        let plan = allocate_columns(&members, 80.0, -1.0);
        assert_eq!(plan.slots.len(), 2);
        // Bridge spans two rows in its row_span.
        let b = plan.slots.iter().find(|s| s.member_idx == 0).unwrap();
        assert!(b.row_span.0 < b.row_span.1);
        // span covers the outermost member edges.
        let min_edge = plan
            .slots
            .iter()
            .map(|s| plan.x_values[s.col_idx] - s.width / 2.0)
            .fold(f64::MAX, f64::min);
        let max_edge = plan
            .slots
            .iter()
            .map(|s| plan.x_values[s.col_idx] + s.width / 2.0)
            .fold(f64::MIN, f64::max);
        assert!(
            plan.span_lo <= min_edge,
            "span_lo {} > min edge {}",
            plan.span_lo,
            min_edge
        );
        assert!(
            plan.span_hi >= max_edge,
            "span_hi {} < max edge {}",
            plan.span_hi,
            max_edge
        );
    }

    #[test]
    fn columns_are_distinct() {
        let members = vec![
            mv(TapRole::Drop { dir: 1.0 }, 60.0, 20.0, 100.0, None),
            mv(TapRole::Drop { dir: 1.0 }, 60.0, 20.0, 100.0, None),
            mv(TapRole::Drop { dir: 1.0 }, 60.0, 20.0, 100.0, None),
        ];
        let plan = allocate_columns(&members, 80.0, 1.0);
        let mut cols: Vec<usize> = plan.slots.iter().map(|s| s.col_idx).collect();
        cols.sort();
        cols.dedup();
        assert_eq!(cols.len(), 3, "three shunts get three distinct columns");
    }

    #[test]
    fn empty_is_safe() {
        let plan = allocate_columns(&[], 80.0, 1.0);
        assert!(plan.slots.is_empty());
        assert_eq!(plan.span_lo, 80.0);
    }

    /// M5.1 (positive): `reduce_crossings` re-orders two East members whose
    /// allocated columns came out inverted vs their anchor order. a anchors at
    /// 100 (left) but landed farther out (244) than b (anchor 200, landed 194);
    /// the pass swaps them back to (194, 244) so larger anchor ⇒ larger x.
    #[test]
    fn reduce_crossings_fixes_inverted_anchor_order() {
        let members = vec![
            SideMember {
                idx: Some((0, 0)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 100.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
            SideMember {
                idx: Some((0, 1)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 200.0,
                anchor_pin_x: 200.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
        ];
        // Simulate the inverted greedy result: a(100) out at 300, b(200) in at 200.
        let mut result = vec![300.0, 200.0];
        let mut occupied = vec![
            (
                0usize,
                300.0 - 21.0,
                300.0 + 21.0,
                100.0 - 120.0,
                100.0 + 120.0,
            ),
            (
                1usize,
                200.0 - 21.0,
                200.0 + 21.0,
                200.0 - 120.0,
                200.0 + 120.0,
            ),
        ];
        reduce_crossings(&members, 1.0, &mut result, &mut occupied);
        assert!(
            result[0] < result[1],
            "crossing not fixed: a={} should sit left of b={}",
            result[0],
            result[1]
        );
        // The swap must not collapse the two into the same spot (A21 guard).
        assert!(
            (result[0] - result[1]).abs() > 1.0,
            "members collided after crossing fix: {result:?}"
        );
    }

    /// M5.1 (negative/guard): `reduce_crossings` is a NO-OP when the columns
    /// already follow the anchor order — it must not touch a clean layout.
    #[test]
    fn reduce_crossings_keeps_clean_order_untouched() {
        let members = vec![
            SideMember {
                idx: Some((0, 0)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 100.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
            SideMember {
                idx: Some((0, 1)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 200.0,
                anchor_pin_x: 200.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
        ];
        let mut result = vec![194.0, 244.0]; // already in anchor order
        let mut occupied = vec![
            (
                0usize,
                194.0 - 21.0,
                194.0 + 21.0,
                100.0 - 120.0,
                100.0 + 120.0,
            ),
            (
                1usize,
                244.0 - 21.0,
                244.0 + 21.0,
                200.0 - 120.0,
                200.0 + 120.0,
            ),
        ];
        reduce_crossings(&members, 1.0, &mut result, &mut occupied);
        assert_eq!(result[0], 194.0);
        assert_eq!(result[1], 244.0);
    }

    /// M21 (U162①): members of one net sharing a tap origin are placed in
    /// the net's R0 reading order — the greedy tiebreak is `read_rank`, so
    /// the endpoint the author stated first sits further west on an East
    /// side even when the input order came out reversed.
    #[test]
    fn same_anchor_members_follow_reading_rank() {
        // Input order reversed vs reading order: rank 1 arrives first.
        let members = vec![
            SideMember {
                idx: Some((0, 0)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 100.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: 1,
                min_edge: None,
                max_edge: None,
            },
            SideMember {
                idx: Some((0, 1)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 200.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: 0,
                min_edge: None,
                max_edge: None,
            },
        ];
        let out = allocate_columns_for_side(&members, 1.0);
        let x_of = |id: (usize, usize)| {
            out.iter()
                .find(|o| (o.0, o.1) == id)
                .map(|o| o.2)
                .unwrap()
        };
        let x_first = x_of((0, 1)); // rank 0: stated first
        let x_second = x_of((0, 0)); // rank 1: stated second
        assert!(
            x_first < x_second,
            "reading order not respected on East side: rank0 x={x_first}, rank1 x={x_second}"
        );
    }

    /// M22 (U162 residual, positive): a member whose read_rank comes after an
    /// ALREADY-PLACED co-endpoint allocates EAST of that endpoint's east edge
    /// even on a West band — the R0 reading order outranks the band's walk
    /// direction. X6's load cap `_C4` (last rank of its net) hung west of the
    /// crystal run's R442 joint before the window existed.
    #[test]
    fn min_edge_window_places_member_east_of_placed_joint_on_west_band() {
        let members = vec![SideMember {
            idx: Some((0, 0)),
            role: TapRole::Bridge {
                partner: 1,
                dir: -1.0,
            },
            w: 40.0,
            h: 60.0,
            row_y: 100.0,
            // Unbounded, the band walk would put this west of the tap.
            anchor_pin_x: 100.0,
            partner_y: Some(200.0),
            blocked: vec![],
            read_rank: 2,
            // The rank-1 joint sits at centre x -6 with a 60-wide body: its
            // east edge is 24.
            min_edge: Some(24.0),
            max_edge: None,
        }];
        let out = allocate_columns_for_side(&members, -1.0);
        let x = out[0].2;
        assert!(
            x > 24.0,
            "windowed member went west of the placed joint's east edge: x={x}"
        );
    }

    /// M22 (mirror): a member whose read_rank comes before a placed
    /// co-endpoint stays WEST of that endpoint's west edge (SPK `_R2`).
    #[test]
    fn max_edge_window_places_member_west_of_placed_endpoint() {
        let members = vec![SideMember {
            idx: Some((0, 0)),
            role: TapRole::Drop { dir: 1.0 },
            w: 40.0,
            h: 60.0,
            row_y: 100.0,
            anchor_pin_x: 300.0,
            partner_y: None,
            blocked: vec![],
            read_rank: 0,
            min_edge: None,
            max_edge: Some(200.0),
        }];
        let out = allocate_columns_for_side(&members, 1.0);
        let x = out[0].2;
        assert!(
            x < 200.0,
            "windowed member went east of the placed endpoint's west edge: x={x}"
        );
    }

    /// M22 (guard): `reduce_crossings` never swaps a windowed member — its
    /// column is pinned by rank against placed co-endpoints, and a swap would
    /// silently move it back across the window edge.
    #[test]
    fn reduce_crossings_never_swaps_windowed_member() {
        let members = vec![
            SideMember {
                idx: Some((0, 0)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 100.0,
                // Anchor tap west of the second member's: without the window
                // the pass would swap them.
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: 0,
                min_edge: Some(150.0),
                max_edge: None,
            },
            SideMember {
                idx: Some((0, 1)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 200.0,
                anchor_pin_x: 300.0,
                partner_y: None,
                blocked: vec![],
                read_rank: 1,
                min_edge: None,
                max_edge: None,
            },
        ];
        let mut result = vec![180.0, 260.0];
        let mut occupied = vec![
            (0usize, 140.0, 220.0, 20.0, 180.0),
            (1usize, 220.0, 300.0, 120.0, 280.0),
        ];
        reduce_crossings(&members, 1.0, &mut result, &mut occupied);
        assert_eq!(
            result[0], 180.0,
            "windowed member was swapped off its window spot"
        );
        assert_eq!(result[1], 260.0);
    }

    /// M21 (guard): an unknown rank (`usize::MAX`) ties with another unknown
    /// rank and keeps the input order — the stable fallback for members whose
    /// net's endpoint list is unavailable.
    #[test]
    fn unknown_ranks_keep_input_order() {
        let members = vec![
            SideMember {
                idx: Some((0, 0)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 100.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
            SideMember {
                idx: Some((0, 1)),
                role: TapRole::Drop { dir: 1.0 },
                w: 40.0,
                h: 60.0,
                row_y: 200.0,
                anchor_pin_x: 100.0,
                partner_y: None,
                blocked: vec![],
                read_rank: usize::MAX,
                min_edge: None,
                max_edge: None,
            },
        ];
        let out = allocate_columns_for_side(&members, 1.0);
        let x_of = |id: (usize, usize)| {
            out.iter()
                .find(|o| (o.0, o.1) == id)
                .map(|o| o.2)
                .unwrap()
        };
        assert!(
            x_of((0, 0)) < x_of((0, 1)),
            "unknown ranks must keep input order: first-input x={}, second-input x={}",
            x_of((0, 0)),
            x_of((0, 1))
        );
    }

    /// M22 (gate): a rank window the band's own walk already satisfies must
    /// stay ignored — the member also rides other nets whose reading order
    /// the unconstrained placement was chosen for. Regressed shape: a power
    /// symbol last-ranked on the rail net would be dragged east against the
    /// chain net it also feeds.
    #[test]
    fn satisfied_rank_window_does_not_move_the_member() {
        // West band (dir=-1): the walk parks the member at 80-40=40, well
        // west of a ceiling at 120 — the window is satisfied, so the result
        // must equal the unwindowed allocation (40), not the window origin
        // (120-18=102).
        let members = vec![SideMember {
            idx: Some((0, 0)),
            role: TapRole::Drop { dir: 1.0 },
            w: 20.0,
            h: 60.0,
            row_y: 100.0,
            anchor_pin_x: 80.0,
            partner_y: None,
            blocked: vec![],
            read_rank: 0,
            min_edge: None,
            max_edge: Some(120.0),
        }];
        let out = allocate_columns_for_side(&members, -1.0);
        assert_eq!(
            out[0].2, 40.0,
            "a satisfied window was applied as an allocation origin"
        );
    }

    /// M22 (the U162 residual shape): a last-rank member on a West band would
    /// walk west of an already-placed lower-rank joint — the window fires and
    /// allocates it east of the floor instead, back in reading order.
    #[test]
    fn violated_rank_window_clamps_to_the_reading_side() {
        // West band: walk parks at 160-40=120, west of the floor 240+18=258
        // set by a placed co-endpoint ranked before this member.
        let members = vec![SideMember {
            idx: Some((0, 0)),
            role: TapRole::Drop { dir: 1.0 },
            w: 20.0,
            h: 60.0,
            row_y: 100.0,
            anchor_pin_x: 160.0,
            partner_y: None,
            blocked: vec![],
            read_rank: 1,
            min_edge: Some(240.0),
            max_edge: None,
        }];
        let out = allocate_columns_for_side(&members, -1.0);
        assert_eq!(
            out[0].2, 258.0,
            "a violated window must clamp the member to its reading side"
        );
    }
}
