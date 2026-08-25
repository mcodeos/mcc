// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ M0 — Equipotential-tree layout observatory (zero behaviour change)
//!
//! ## Why this file exists
//! The row/column rework (M1..M6) touches `assign_regions`, `resolve_lanes`,
//! `place_members_for_topo` and `realize` in sequence. Verifying each step by
//! eyeballing the SVG is not a verification — a wrong `col` and a wrong `row_y`
//! produce equally plausible-looking pictures. So before changing anything we
//! freeze:
//!
//!   1. a **stable text projection** of the layout model ([`LayoutView`]), whose
//!      *schema does not change* across M0..M6 — later milestones only fill in
//!      cells that currently print `—`. Diffing two dumps is then meaningful.
//!   2. an **invariant harness** ([`audit_equi_tree`]) carrying all eight
//!      assertions A1..A8 from day one, each gated on the [`Milestone`] at which
//!      it is supposed to go green. A check may be computed and reported red
//!      long before it is enforced.
//!
//! ## Contract
//! This module is **read-only**: it never touches `x/y/w/h`, `slots`,
//! `entry_points` or `lane`. It may be called at any point after
//! `place_by_topology` without perturbing the result.
//!
//! ## Known-red at M0 (expected, do not "fix" here)
//! * **A2 / A2b** — lane / anchor equality with the render replay. Green since
//!   M1's dependency-order lane resolution and kept green through M2/M2.5:
//!   `assign_rows` is a geometry-free pure function, so layout and render derive
//!   identical rows; A2 is the guard that would go red the moment a row pass
//!   reads a rect.
//! * **A4** — passive orientation. Green on `moddcdc` (M1's Series rewrite
//!   routes Series members through `assign_shunt_slots`, opposite-edge pins),
//!   but it stays due M3 — `Stub`/`Sink` still call `assign_pin_slots` (one
//!   side for every pin) and `inner_side` is geometrically wrong under the row
//!   model, so a fixture with Stub/Sink two-pin passives would go red.
//! * **A7 / A8** — wire hygiene. Goes green at M5. At M2.5, A7 is down to the
//!   named cross-side Series (Class A) residuals.
//!
//! ## Green from M2/M2.5 (the M2 regression class, all closed)
//! * **A10** — same-side row exclusivity. The `505/506` collision was an
//!   occupancy-table hole (a free East net colliding with the South rail),
//!   fixed by M2.5 Step 6; both fixtures are green.
//! * **A13** — no overlapping pin slots (multi-pin IC nets + NC pins).
//! * **A14** — pin labels fit inside the box (collapsed-row height regression).
//! * **A15** — all Ground glyphs share one band (M2.5 Step 7).
//!
//! ## Green at M0 (guarded from day one)
//! * **A3** — no dangling segment endpoints.
//! * **A9** — one ground glyph per ground net (the five `GND` nets stay five).
//!
//! ## Table note (M2.5 Step 8)
//! The NETS table swapped the old `row_assigned` bool for a `src` column
//! (`side` / `rail` / `p:<nid>` / `—` for terminal-only) — a column
//! replacement, not an addition, so the schema-freeze convention holds.
//!
//! ## Two defects this harness surfaces on the `moddcdc` fixture
//! * `select_anchor_deterministic` hands 5 of 10 topologies an anchor that is a
//!   **two-pin passive** (no IC pin on the net → every candidate has pin_count 1
//!   → the `max_by` chain falls through to `box_id`). M2 resolves this into
//!   three terminal-only nets (502/503/504, no trunk) and two free nets
//!   (501/506, rows inherited from a partner), so `anchor_placed` and
//!   `row_assigned` now diverge from `is_layer_anchor` as designed.
//! * `find_net` used to resolve by **name** — `moddcdc` carries five distinct
//!   nets all named `GND`, so `direct_region` / `anchor_pin_io` read the first
//!   one for all five. M2 switched it to a nid lookup (`NetTopology.nid`),
//!   which is unique.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::vector::graph::{BoxKind, EntrySide, McVecBox, McVecGraph, NetKind};

use super::equipotential_tree::{
    assign_regions, assign_rows, build_topology, envelop_lanes, is_w_e_opposite, layer_anchor_id,
    member_pin_point, net_corridor_demand, partner_info, point_on_segment, realize_all,
    resolve_lanes, segment_hits_box, slot_of, slot_point, tap_role, EquiTree, Lane, NetTopology,
    PinGroup, Region, RowSource, TapRole, Terminal, TreeSymbol, TreeSymbolKind, LABEL_CHAR_W,
    LABEL_PAD, ROW_CLEAR, SYMBOL_DROP, TOOTH_GAP,
};

// ============================================================================
// Milestone gating
// ============================================================================

/// Which milestone an invariant is expected to hold from.
///
/// `audit_equi_tree` always *computes* every check; `assert_clean_through`
/// only *enforces* the ones whose `since <= current`. Advancing a milestone is
/// therefore a one-line change in the test, and a check that goes green early
/// is visible in the report immediately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Milestone {
    /// Observatory only — no layout change.
    M0,
    /// W/E trunks become horizontal rows + dependency-order lane resolution.
    M1,
    /// `assign_rows` (pure row allocation + pin-offset ownership inversion),
    /// free-net row inheritance, terminal-only nets without a trunk.
    M2,
    /// `TapRole` replaces `MemberRole`.
    M3,
    /// Rendering-overlap patch: R1–R4 + A17/A18 (label text side, edge-label
    /// spacing, tooth gap, rail/lead spacing).
    M3_5,
    /// Column model (`resolve_columns` / `rank.rs`).
    M4,
    /// Typography + terminals.
    M5,
    /// Regression + fallbacks.
    M6,
    /// ★ M11: the row END BUDGET (A31) and the label stub (A32). The fixtures
    /// still call `assert_clean_through(M6)`, so these two are REPORTED and not
    /// enforced; advancing a fixture to `M7` is the one-line change that turns
    /// them on.
    M7,
}

impl fmt::Display for Milestone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

// ============================================================================
// Layer 0: the read model — schema frozen at M0
// ============================================================================

/// Geometric orientation of a placed box, derived from `w` vs `h`.
///
/// This is the *observable* the A4 invariant is written against: a two-pin
/// passive drawn horizontally must carry Left/Right pins, one drawn vertically
/// must carry Top/Bottom pins. Anything else means the symbol renders rotated
/// relative to its own leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orient {
    Horizontal,
    Vertical,
    /// Square or degenerate (`w == h`, or not placed at all).
    Undecided,
}

impl Orient {
    fn of(b: &McVecBox) -> Orient {
        if b.w > b.h + 0.5 {
            Orient::Horizontal
        } else if b.h > b.w + 0.5 {
            Orient::Vertical
        } else {
            Orient::Undecided
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Orient::Horizontal => "H",
            Orient::Vertical => "V",
            Orient::Undecided => "·",
        }
    }
}

/// One attachment point of a net: an anchor pin group, a member box, or a
/// terminal symbol.
///
/// `role` / `col` are `None` before M3 / M4 respectively and print as `—`.
/// The field order here is the column order of the TAPS table and must not be
/// reordered — golden diffs depend on it.
#[derive(Debug, Clone)]
pub struct TapView {
    pub box_id: i64,
    pub box_name: String,
    pub is_anchor: bool,
    /// `TapRole` as a short string. `None` until M3.
    pub role: Option<String>,
    /// Column rank. `None` until M4.
    pub col: Option<i32>,
    /// The point at which this tap meets the trunk, as `realize` computes it.
    pub tap: (f64, f64),
    /// Placed rect of the box.
    pub rect: (f64, f64, f64, f64),
    pub orient: Orient,
    /// Sides carried by this box's pin slots, in slot order.
    pub pin_sides: Vec<EntrySide>,
    /// This box has been placed by a layout pass (as opposed to still sitting
    /// at the origin with a fallback rect).
    pub placed: bool,
}

/// One net's slice of the layout model.
#[derive(Debug, Clone)]
pub struct NetView {
    pub nid: i64,
    pub net_name: String,
    pub kind: NetKind,
    pub region: Region,
    /// Trunk orientation. M1 makes it an **all-horizontal row model**: every
    /// trunk is a horizontal row, so this column is constant `H`. It is read
    /// off `Lane::horizontal` (added at M1 so M4's column model does not
    /// re-derive direction from the Region; the ladder style stays in
    /// FlowLayouter, which never runs on device layers).
    pub trunk_horizontal: bool,
    pub anchor_id: i64,
    pub anchor_name: String,
    pub is_layer_anchor: bool,
    /// Whether the anchor box was already placed when `resolve_lanes` read it.
    /// `false` here means the lane was computed from the `(0, 0, 120, 60)`
    /// fallback rect — see the module docs.
    pub anchor_placed: bool,
    /// How this net's row was produced (M2.5 B1 fix). `IslandFallback` is the
    /// only source A1 treats as `rows_fallback`.
    pub(crate) row_source: RowSource,
    /// Row coordinate. `None` until M2; pre-M2 this mirrors `axis` when the
    /// trunk is horizontal so the column is not empty.
    pub row: Option<f64>,
    pub axis: f64,
    pub span: (f64, f64),
    pub taps: Vec<TapView>,
    pub terminals: Vec<String>,
    /// Net has exactly one real group and does not touch the layer anchor —
    /// the "terminal only" shape that M2 stops giving a trunk to.
    pub terminal_only: bool,
}

/// The whole layer, projected to text.
#[derive(Debug, Clone)]
pub struct LayoutView {
    pub layer_anchor_id: i64,
    pub layer_anchor_name: String,
    pub nets: Vec<NetView>,
}

/// Project `(graph, topos)` into the stable read model.
///
/// Deterministic: nets are ordered by `(net_name, nid)`, taps keep topology
/// order (anchor first, members by `box_id`) with terminals appended.
pub fn build_view(graph: &McVecGraph, topos: &[NetTopology]) -> LayoutView {
    let anchor_id = layer_anchor_id(topos);
    let anchor_name = box_name(graph, anchor_id);

    let mut nets: Vec<NetView> = topos
        .iter()
        .map(|topo| build_net_view(graph, topos, topo, anchor_id))
        .collect();
    nets.sort_by(|a, b| a.net_name.cmp(&b.net_name).then_with(|| a.nid.cmp(&b.nid)));

    LayoutView {
        layer_anchor_id: anchor_id,
        layer_anchor_name: anchor_name,
        nets,
    }
}

fn build_net_view(
    graph: &McVecGraph,
    _topos: &[NetTopology],
    topo: &NetTopology,
    layer_anchor: i64,
) -> NetView {
    let lane: Lane = topo.lane;
    // M1 row model: every trunk is horizontal, read off `lane.horizontal` (the
    // field exists so M4's column model does not re-derive direction from the
    // Region). Pre-M1 this was derived as `!region.axis_vertical()`.
    let trunk_horizontal = lane.horizontal;

    let mut taps: Vec<TapView> = Vec::new();
    let topo_idx = _topos.iter().position(|t| t.nid == topo.nid);
    for (i, group) in topo.groups.iter().enumerate() {
        let Some(b) = graph.boxes.iter().find(|b| b.id == group.box_id) else {
            continue;
        };
        let is_anchor = i == 0;
        let tap = if is_anchor {
            anchor_tap_point(b, group, lane, trunk_horizontal)
        } else {
            member_pin_point(b, group)
        };
        // M3.3: the TAPS `role` column is filled from the member's TapRole
        // (anchor taps carry no role and keep printing `—`).
        let role = if is_anchor {
            None
        } else {
            topo_idx.map(|idx| {
                let p = super::equipotential_tree::partner_info(_topos, idx, group);
                super::equipotential_tree::tap_role(b, &_topos[idx], p, layer_anchor)
                    .short()
                    .to_string()
            })
        };
        taps.push(TapView {
            box_id: b.id,
            box_name: b.name.clone(),
            is_anchor,
            role,
            col: None, // M4
            tap,
            rect: (b.x, b.y, b.w, b.h),
            orient: Orient::of(b),
            pin_sides: b.slots.iter().map(|s| s.side).collect(),
            placed: b.geom_locked,
        });
    }

    // M2: the topology carries `terminal_only` (single real group — no trunk);
    // the observatory used to re-derive it as `groups.len() == 1 && anchor !=
    // layer_anchor`.
    let terminal_only = topo.terminal_only;

    NetView {
        nid: topo.nid,
        net_name: topo.net_name.clone(),
        kind: topo.net_kind.clone(),
        region: lane.region,
        trunk_horizontal,
        anchor_id: topo.anchor,
        anchor_name: box_name(graph, topo.anchor),
        is_layer_anchor: topo.anchor == layer_anchor,
        // Whether the anchor box was already placed when this net's lane was
        // resolved. `place_by_topology` resolves lanes in **dependency order**
        // (P3 only touches nets whose anchor is placed), so this is true for
        // every net that ever got a lane — and false for one whose anchor was
        // never placed. Pre-M1, before the dependency-order pipeline, this was
        // a tautology of `is_layer_anchor`; they diverge from M1 on.
        anchor_placed: topo.anchor_placed,
        row_source: topo.row_source,
        row: if trunk_horizontal {
            Some(lane.axis)
        } else {
            None
        },
        axis: lane.axis,
        span: lane.span,
        taps,
        terminals: topo.terminals.iter().map(terminal_label).collect(),
        terminal_only,
    }
}

/// Where the anchor group meets its trunk. Mirrors `realize`'s tooth geometry:
/// the tooth runs perpendicular from the pin to the trunk axis.
fn anchor_tap_point(
    b: &McVecBox,
    group: &PinGroup,
    lane: Lane,
    trunk_horizontal: bool,
) -> (f64, f64) {
    let pt = group
        .pin_ids
        .first()
        .and_then(|&pid| slot_of(b, pid))
        .map(|s| slot_point(b, s));
    match pt {
        Some((px, py)) => {
            if trunk_horizontal {
                (px, lane.axis)
            } else {
                (lane.axis, py)
            }
        }
        None => (b.x + b.w / 2.0, b.y + b.h / 2.0),
    }
}

fn terminal_label(t: &Terminal) -> String {
    match t {
        Terminal::Ground => "GND⏚".to_string(),
        Terminal::NetLabel(n) => format!("○{n}"),
        Terminal::Port { name } => format!("▷{name}"),
    }
}

fn box_name(graph: &McVecGraph, id: i64) -> String {
    graph
        .boxes
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.name.clone())
        .unwrap_or_else(|| format!("<missing id={id}>"))
}

// ============================================================================
// Text projection
// ============================================================================

fn f(v: f64) -> String {
    format!("{v:.0}")
}

fn opt_f(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.0}")).unwrap_or_else(|| "—".into())
}

fn opt_i(v: Option<i32>) -> String {
    v.map(|x| x.to_string()).unwrap_or_else(|| "—".into())
}

fn opt_s(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| "—".into())
}

fn side_glyph(s: EntrySide) -> char {
    match s {
        EntrySide::Top => 'T',
        EntrySide::Right => 'R',
        EntrySide::Bottom => 'B',
        EntrySide::Left => 'L',
    }
}

/// M2.5 Step 8: compact `src` column — where the net's row came from.
fn row_src_glyph(r: &RowSource) -> String {
    match r {
        RowSource::SidePin => "side".into(),
        RowSource::EdgeRail => "rail".into(),
        RowSource::Partner(n) => format!("p:{n}"),
        RowSource::IslandFallback => "island".into(),
    }
}

impl fmt::Display for LayoutView {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            out,
            "== LAYOUT MODEL ==  layer_anchor = {} (id={})  nets={}",
            self.layer_anchor_name,
            self.layer_anchor_id,
            self.nets.len()
        )?;
        writeln!(out)?;

        // ── NETS ──
        writeln!(
            out,
            "{:<5} {:<11} {:<7} {:<6} {:<4} {:<13} {:<6} {:<7} {:<8} {:<17} {:<5} {}",
            "nid",
            "net",
            "kind",
            "region",
            "ori",
            "anchor",
            "plcd",
            "src",
            "row",
            "span",
            "term",
            "terminals",
        )?;
        writeln!(out, "{}", "-".repeat(110))?;
        for n in &self.nets {
            let row_glyph = if n.terminal_only {
                String::new()
            } else {
                row_src_glyph(&n.row_source)
            };
            writeln!(
                out,
                "{:<5} {:<11} {:<7} {:<6} {:<4} {:<13} {:<6} {:<7} {:<8} {:<17} {:<5} {}",
                n.nid,
                truncate(&n.net_name, 11),
                format!("{:?}", n.kind),
                format!("{:?}", n.region),
                if n.trunk_horizontal { "H" } else { "V" },
                truncate(
                    &format!(
                        "{}{}",
                        n.anchor_name,
                        if n.is_layer_anchor { "*" } else { "" }
                    ),
                    13
                ),
                if n.anchor_placed { "yes" } else { "NO" },
                if n.terminal_only { "—" } else { &row_glyph },
                opt_f(n.row),
                format!("({}, {})", f(n.span.0), f(n.span.1)),
                if n.terminal_only { "only" } else { "" },
                n.terminals.join(" "),
            )?;
        }

        // ── TAPS ──
        writeln!(out)?;
        writeln!(
            out,
            "{:<5} {:<11} {:<10} {:<3} {:<8} {:<4} {:<15} {:<24} {:<4} {:<6} {}",
            "nid",
            "net",
            "box",
            "anc",
            "role",
            "col",
            "tap(x,y)",
            "rect(x,y,w,h)",
            "ori",
            "pins",
            "plcd",
        )?;
        writeln!(out, "{}", "-".repeat(110))?;
        for n in &self.nets {
            for t in &n.taps {
                writeln!(
                    out,
                    "{:<5} {:<11} {:<10} {:<3} {:<8} {:<4} {:<15} {:<24} {:<4} {:<6} {}",
                    n.nid,
                    truncate(&n.net_name, 11),
                    truncate(&t.box_name, 10),
                    if t.is_anchor { "*" } else { "" },
                    opt_s(&t.role),
                    opt_i(t.col),
                    format!("({}, {})", f(t.tap.0), f(t.tap.1)),
                    format!(
                        "({}, {}, {}, {})",
                        f(t.rect.0),
                        f(t.rect.1),
                        f(t.rect.2),
                        f(t.rect.3)
                    ),
                    t.orient.glyph(),
                    t.pin_sides
                        .iter()
                        .map(|&s| side_glyph(s))
                        .collect::<String>(),
                    if t.placed { "yes" } else { "NO" },
                )?;
            }
        }
        Ok(())
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// Convenience: project and render in one call.
pub fn dump_layout_model(graph: &McVecGraph, topos: &[NetTopology]) -> String {
    build_view(graph, topos).to_string()
}

// ============================================================================
// Layer 1: invariants A1..A8
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Fail,
    /// Not computable yet — the data it needs arrives at `since`.
    Skipped,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub id: &'static str,
    pub name: &'static str,
    /// Milestone from which this check is enforced.
    pub since: Milestone,
    pub status: CheckStatus,
    pub details: Vec<String>,
}

impl Check {
    fn new(id: &'static str, name: &'static str, since: Milestone) -> Check {
        Check {
            id,
            name,
            since,
            status: CheckStatus::Pass,
            details: Vec::new(),
        }
    }

    fn skipped(id: &'static str, name: &'static str, since: Milestone, why: &str) -> Check {
        Check {
            id,
            name,
            since,
            status: CheckStatus::Skipped,
            details: vec![why.to_string()],
        }
    }

    fn fail(&mut self, detail: String) {
        self.status = CheckStatus::Fail;
        // Cap the detail list — a systematically broken invariant would
        // otherwise bury the report.
        if self.details.len() < 24 {
            self.details.push(detail);
        }
    }
}

#[derive(Debug, Clone)]
pub struct EquiAudit {
    pub checks: Vec<Check>,
}

impl EquiAudit {
    pub fn failures(&self) -> Vec<&Check> {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .collect()
    }

    /// Failures that are *enforced* at `current`.
    pub fn blocking(&self, current: Milestone) -> Vec<&Check> {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail && c.since <= current)
            .collect()
    }

    /// Panic if any invariant due at `current` is red. Checks not yet due are
    /// reported but tolerated.
    pub fn assert_clean_through(&self, current: Milestone) {
        let blocking = self.blocking(current);
        if blocking.is_empty() {
            return;
        }
        let mut msg = format!("equi-tree audit failed at {current}:\n");
        for c in blocking {
            msg.push_str(&format!("  [{}] {} ({})\n", c.id, c.name, c.since));
            for d in &c.details {
                msg.push_str(&format!("        {d}\n"));
            }
        }
        panic!("{msg}");
    }
}

impl fmt::Display for EquiAudit {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(out, "== EQUI-TREE AUDIT ==")?;
        for c in &self.checks {
            let mark = match c.status {
                CheckStatus::Pass => "PASS",
                CheckStatus::Fail => "FAIL",
                CheckStatus::Skipped => "skip",
            };
            writeln!(out, "  {mark}  [{}] {:<40} due:{}", c.id, c.name, c.since)?;
            for d in &c.details {
                writeln!(out, "          {d}")?;
            }
        }
        Ok(())
    }
}

/// Run every invariant against a **placed** graph and its layout-phase topologies.
///
/// `layout_topos` must be the same slice that `place_by_topology` mutated —
/// A2 compares it against a fresh render-side replay.
pub fn audit_equi_tree(graph: &McVecGraph, layout_topos: &[NetTopology]) -> EquiAudit {
    let trees: Vec<EquiTree> = realize_all(layout_topos, graph);

    let checks = vec![
        check_a1_rows(layout_topos),
        check_a2_lane_replay(graph, layout_topos),
        check_a2b_anchor_replay(graph, layout_topos),
        check_a3_dangling(graph, layout_topos, &trees),
        check_a4_passive_orientation(graph),
        check_a5_col_unique(),
        check_a6_bridge_same_col(),
        check_a7_wire_through_box(graph, layout_topos, &trees),
        check_a8_junction_present(layout_topos, &trees),
        check_a9_ground_glyphs(layout_topos, &trees),
        check_a10_same_side_rows(graph, layout_topos),
        check_a11_same_row_opposite(layout_topos),
        check_a12_row_band_overlap(graph, layout_topos),
        check_a13_pin_overlap(graph),
        check_a14_label_fit(graph),
        check_a15_ground_band(&trees),
        check_a16_ground_count_conservation(graph, layout_topos, &trees),
        check_a17_text_overlap(graph, &trees),
        check_a18_wire_collinear_edge(graph, &trees),
        check_a21_members_do_not_overlap(graph, layout_topos),
        check_a22_spanning_member_in_span(graph, layout_topos),
        check_a23_shunt_near_anchor_pin(graph, layout_topos),
        check_a24_no_wire_crossings(graph, layout_topos),
        check_a25_label_clear_of_members(graph, layout_topos, &trees),
        check_a26_shunt_balance(graph, layout_topos),
        check_a27_pin_on_its_row(graph, layout_topos),
        check_a28_along_is_collinear(graph, layout_topos),
        check_a29_run_spans_disjoint(layout_topos),
        check_a30_satellite_pins_on_rows(graph, layout_topos),
        check_a34_every_pin_on_its_row(graph, layout_topos),
        check_a31_row_end_budget(graph, layout_topos, &trees),
        check_a32_label_has_a_stub(layout_topos, &trees),
    ];

    EquiAudit { checks }
}

// ── A1 ──────────────────────────────────────────────────────────────────────

/// A1: no trunk-bearing net fell back to an island row — `rows_fallback == 0`.
///
/// `assign_rows` (P0) is the single authority for trunk y. A free net that
/// found no partner falls back to `RowSource::IslandFallback`; that is the only
/// source this check treats as a fallback row. Terminal-only nets carry no
/// trunk and are exempt. (B1 fix: the island fallback used to also set
/// `row_assigned`, making this a tautology again.)
fn check_a1_rows(layout_topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A1", "rows_fallback == 0", Milestone::M2);
    let fallback: Vec<&NetTopology> = layout_topos
        .iter()
        .filter(|t| !t.terminal_only && t.row_source == RowSource::IslandFallback)
        .collect();
    if !fallback.is_empty() {
        for t in fallback {
            c.fail(format!(
                "net '{}' (nid={}) fell back to an island row (no partner)",
                t.net_name, t.nid
            ));
        }
    }
    c
}

// ── A2 ──────────────────────────────────────────────────────────────────────

/// Replay the render-side pipeline (`assign_regions` → `assign_rows` →
/// `resolve_lanes` → `envelop_lanes`) on the **fully placed** graph. The render
/// phase (`build_all_trees`) does exactly this, so this helper is the render
/// side of the A2/A2b comparison.
fn render_replay(graph: &McVecGraph) -> Vec<NetTopology> {
    let mut replay = build_topology(graph);
    assign_regions(graph, &mut replay);
    let layer_anchor = layer_anchor_id(&replay);
    assign_rows(graph, &mut replay, layer_anchor);
    resolve_lanes(graph, &mut replay);
    envelop_lanes(graph, &mut replay);
    replay
}

/// The layout phase and the render phase must derive identical lanes. Both run
/// the same `resolve_lanes`; the only drift source is `anchor_box_rect` — a
/// lane computed while its anchor box is still unplaced (fallback rect) differs
/// from one computed after placement.
///
/// Matched on `nid`, not `net_name` — `moddcdc` has five nets named `GND`.
///
/// **Due M2, not M0** — `place_by_topology` runs `resolve_lanes` with only the
/// layer anchor placed, while the render side runs it on the fully placed
/// graph. For any net whose anchor is a two-pin passive (M0-finding #1) the
/// layout side reads the `(0,0,120,60)` fallback rect and the replay reads the
/// real rect, so the lanes legitimately differ. The fix is lane resolution in
/// dependency order (a net's lane is computed only once its anchor box is
/// placed); until then this check is computed and reported, not enforced.
fn check_a2_lane_replay(graph: &McVecGraph, layout_topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A2", "lane(layout) == lane(render replay)", Milestone::M2);

    let replay = render_replay(graph);
    if replay.len() != layout_topos.len() {
        c.fail(format!(
            "topology count drift: layout={} render={}",
            layout_topos.len(),
            replay.len()
        ));
        return c;
    }

    let by_nid: BTreeMap<i64, &NetTopology> = replay.iter().map(|t| (t.nid, t)).collect();
    for lt in layout_topos {
        let Some(rt) = by_nid.get(&lt.nid) else {
            c.fail(format!(
                "net '{}' (nid={}) missing on the render side",
                lt.net_name, lt.nid
            ));
            continue;
        };
        if lt.lane != rt.lane {
            c.fail(format!(
                "net '{}' (nid={}) lane drift: layout={:?} render={:?}",
                lt.net_name, lt.nid, lt.lane, rt.lane
            ));
        }
    }
    c
}

/// Anchor selection is coordinate-independent (`select_anchor_deterministic`
/// reads only pin counts / degrees / ids), so it must never drift between the
/// layout and render phases. Split out of A2 so a lane drift does not hide a
/// (worse) anchor drift, and vice versa.
fn check_a2b_anchor_replay(graph: &McVecGraph, layout_topos: &[NetTopology]) -> Check {
    let mut c = Check::new(
        "A2b",
        "anchor(layout) == anchor(render replay)",
        Milestone::M2,
    );

    let replay = render_replay(graph);
    if replay.len() != layout_topos.len() {
        return c;
    }

    let by_nid: BTreeMap<i64, &NetTopology> = replay.iter().map(|t| (t.nid, t)).collect();
    for lt in layout_topos {
        let Some(rt) = by_nid.get(&lt.nid) else {
            continue;
        };
        if lt.anchor != rt.anchor {
            c.fail(format!(
                "net '{}' (nid={}) anchor drift: layout={} render={}",
                lt.net_name, lt.nid, lt.anchor, rt.anchor
            ));
        }
    }
    c
}

// ── A3 ──────────────────────────────────────────────────────────────────────

fn check_a3_dangling(graph: &McVecGraph, topos: &[NetTopology], trees: &[EquiTree]) -> Check {
    let mut c = Check::new("A3", "no dangling segment endpoints", Milestone::M0);
    for (topo, tree) in topos.iter().zip(trees.iter()) {
        let d = dangling_segments(topo, tree, graph);
        if !d.is_empty() {
            c.fail(format!(
                "net '{}' (nid={}) dangling segment idx {:?}",
                topo.net_name, topo.nid, d
            ));
        }
    }
    c
}

/// Segment indices with an endpoint that is neither a net pin point, nor a
/// terminal symbol, nor a point on another segment of the same net.
///
/// Lifted verbatim out of `equipotential_tree::tests` so production code and
/// tests share one definition.
pub fn dangling_segments(topo: &NetTopology, tree: &EquiTree, graph: &McVecGraph) -> Vec<usize> {
    let mut pin_points: Vec<(f64, f64)> = Vec::new();
    for grp in &topo.groups {
        if let Some(b) = graph.boxes.iter().find(|b| b.id == grp.box_id) {
            for &pid in &grp.pin_ids {
                if let Some(s) = slot_of(b, pid) {
                    pin_points.push(slot_point(b, s));
                }
            }
            // ★ M10.3: a two-pin member is a WIRE-CONTINUATION (Along/Series).
            // Its far pin is owned by the PARTNER net, yet this net's trunk
            // legitimately reaches it when the wire runs on past the member
            // (a Series cap whose row still carries further members beyond).
            // A segment ending there is connected through the body, not dangling.
            if b.pins.len() == 2 {
                for p in &b.pins {
                    if !grp.pin_ids.contains(&p.id) {
                        if let Some(s) = slot_of(b, p.id) {
                            pin_points.push(slot_point(b, s));
                        }
                    }
                }
            }
        }
    }
    let sym_points: Vec<(f64, f64)> = tree.symbols.iter().map(|s| (s.x, s.y)).collect();
    let on_point = |ex: f64, ey: f64, pts: &[(f64, f64)], tol: f64| -> bool {
        pts.iter()
            .any(|&(px, py)| (px - ex).abs() <= tol && (py - ey).abs() <= tol)
    };

    let mut dangling = Vec::new();
    for (idx, seg) in tree.segments.iter().enumerate() {
        for (ex, ey) in [(seg.x1, seg.y1), (seg.x2, seg.y2)] {
            let ok = on_point(ex, ey, &pin_points, 1.0)
                || on_point(ex, ey, &sym_points, 16.0)
                || tree
                    .segments
                    .iter()
                    .enumerate()
                    .any(|(j, other)| j != idx && point_on_segment(ex, ey, other));
            if !ok {
                dangling.push(idx);
                break;
            }
        }
    }
    dangling
}

// ── A4 ──────────────────────────────────────────────────────────────────────

/// A two-pin passive drawn horizontally (`w > h`) must carry Left/Right pins;
/// drawn vertically (`h > w`) it must carry Top/Bottom pins. A mismatch means
/// the symbol renders rotated relative to its own leads.
///
/// Beyond that, a two-pin passive must carry **exactly two slots on opposite
/// edges**. `assign_pin_slots` stuffs every pin on one side (the Series / Stub /
/// Sink branches all call it), so a passive mis-classified as Series lands with
/// both feet on the same edge — e.g. `LL`. The plain `all(Left|Right)` check
/// lets `LL` through (both are Left), which is a false green; the opposite-edge
/// rule catches it.
///
/// Due M3; on `moddcdc` it is already green after M1's Series rewrite routed
/// Series members through `assign_shunt_slots` (opposite-edge pins).
fn check_a4_passive_orientation(graph: &McVecGraph) -> Check {
    let mut c = Check::new(
        "A4",
        "two-pin passive orientation matches pins",
        Milestone::M3,
    );
    for b in &graph.boxes {
        if b.kind != BoxKind::TwoPin || !b.is_two_pin_passive() {
            continue;
        }
        if b.slots.is_empty() {
            c.fail(format!("'{}' (id={}) has no pin slots", b.name, b.id));
            continue;
        }
        let sides: Vec<EntrySide> = b.slots.iter().map(|s| s.side).collect();
        if sides.len() != 2 {
            c.fail(format!(
                "'{}' (id={}) has {} pin slots, expected 2",
                b.name,
                b.id,
                sides.len()
            ));
            continue;
        }
        let opposite = matches!(
            (sides[0], sides[1]),
            (EntrySide::Left, EntrySide::Right)
                | (EntrySide::Right, EntrySide::Left)
                | (EntrySide::Top, EntrySide::Bottom)
                | (EntrySide::Bottom, EntrySide::Top)
        );
        if !opposite {
            c.fail(format!(
                "'{}' (id={}) pins are not on opposite edges: {:?}",
                b.name, b.id, sides
            ));
            continue;
        }
        let horiz = matches!(sides[0], EntrySide::Left | EntrySide::Right);
        let vert = !horiz;
        match Orient::of(b) {
            Orient::Horizontal if !horiz => c.fail(format!(
                "'{}' (id={}) is w>h but pins are {:?}",
                b.name, b.id, sides
            )),
            Orient::Vertical if !vert => c.fail(format!(
                "'{}' (id={}) is h>w but pins are {:?}",
                b.name, b.id, sides
            )),
            Orient::Undecided => c.fail(format!(
                "'{}' (id={}) has no orientation (w={:.0} h={:.0})",
                b.name, b.id, b.w, b.h
            )),
            _ => {}
        }
    }
    c
}

// ── A5 / A6 ─────────────────────────────────────────────────────────────────

fn check_a5_col_unique() -> Check {
    Check::skipped(
        "A5",
        "cols unique within a row",
        Milestone::M4,
        "column model does not exist yet",
    )
}

fn check_a6_bridge_same_col() -> Check {
    Check::skipped(
        "A6",
        "bridge endpoints share a column",
        Milestone::M4,
        "column model does not exist yet",
    )
}

// ── A7 ──────────────────────────────────────────────────────────────────────

/// No segment may cross a box that is not one of its own net's members.
/// Label-kind boxes are excluded — they render as tree symbols, not rects.
///
/// M3.5: `since` moved M5 → M3 (it was computed but never gated, so the
/// R4-class edge-pressures slipped through). The ONE tolerated class at M3.5 is
/// a wire grazing a **cross-side spanning member** (two owners on W/E- or
/// N/S-opposite sides) — those are reported as `(tolerated)` details and are
/// cleared by M4's column model, per the M3/M3.5 criteria.
fn check_a7_wire_through_box(
    graph: &McVecGraph,
    topos: &[NetTopology],
    trees: &[EquiTree],
) -> Check {
    let mut c = Check::new("A7", "no wire passes through a foreign box", Milestone::M3);
    for (topo, tree) in topos.iter().zip(trees.iter()) {
        let own: Vec<i64> = topo.groups.iter().map(|g| g.box_id).collect();
        for (i, seg) in tree.segments.iter().enumerate() {
            for b in &graph.boxes {
                if own.contains(&b.id) || b.is_container_box() {
                    continue;
                }
                if matches!(
                    b.kind,
                    BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
                ) {
                    continue;
                }
                if b.w <= 0.0 || b.h <= 0.0 {
                    continue;
                }
                if segment_hits_box(seg.x1, seg.y1, seg.x2, seg.y2, b.x, b.y, b.w, b.h) {
                    // Report the pair (SeriesHi, SeriesLo) a crossed member
                    // belongs to, and decide whether it is the tolerated
                    // cross-side class or a real defect.
                    let owners: Vec<&NetTopology> = topos
                        .iter()
                        .filter(|t| t.groups.iter().any(|g| g.box_id == b.id))
                        .collect();
                    let pair = if owners.len() == 2 {
                        let lo = owners.iter().map(|t| t.lane.axis).fold(f64::MAX, f64::min);
                        let hi = owners.iter().map(|t| t.lane.axis).fold(f64::MIN, f64::max);
                        format!(
                            " (SeriesHi={}:{}, SeriesLo={}:{})",
                            owners
                                .iter()
                                .find(|t| (t.lane.axis - hi).abs() < 0.5)
                                .map_or("?", |t| t.net_name.as_str()),
                            owners
                                .iter()
                                .find(|t| (t.lane.axis - hi).abs() < 0.5)
                                .map_or(0, |t| t.nid),
                            owners
                                .iter()
                                .find(|t| (t.lane.axis - lo).abs() < 0.5)
                                .map_or("?", |t| t.net_name.as_str()),
                            owners
                                .iter()
                                .find(|t| (t.lane.axis - lo).abs() < 0.5)
                                .map_or(0, |t| t.nid)
                        )
                    } else {
                        String::new()
                    };
                    let msg = format!(
                        "net '{}' (nid={}) seg#{i} crosses '{}' (id={}){pair}",
                        topo.net_name, topo.nid, b.name, b.id
                    );
                    let cross_side = owners.len() == 2
                        && is_opposite_sides(owners[0].lane.region, owners[1].lane.region);
                    if cross_side {
                        // M3.5: tolerated — M4's column model clears this class.
                        if c.details.len() < 24 {
                            c.details.push(format!("(tolerated) {msg}"));
                        }
                    } else {
                        c.fail(msg);
                    }
                }
            }
        }
    }
    c
}

/// Opposite-side region pair: W↔E or N↔S (the cross-side member class that
/// A7 tolerates until M4's column model).
fn is_opposite_sides(a: Region, b: Region) -> bool {
    match (a, b) {
        (Region::West, Region::East)
        | (Region::East, Region::West)
        | (Region::North, Region::South)
        | (Region::South, Region::North) => true,
        _ => false,
    }
}

// ── A8 ──────────────────────────────────────────────────────────────────────

/// A net with three or more tap points is a comb and must show at least one
/// junction dot; without one it has degenerated into disconnected strokes.
///
/// M3.5: taps count the DRAWN structure only — `tree.symbols.len()` instead of
/// `topo.terminals.len()`, because a terminal whose label box already exists
/// (`has_label_box`) is suppressed and never drawn, so counting it inflated the
/// tap count (ldo VCC: 3 "taps" but actually a 2-point VOUT→CAP_2 line, no
/// junction needed).
fn check_a8_junction_present(topos: &[NetTopology], trees: &[EquiTree]) -> Check {
    let mut c = Check::new("A8", "multi-tap nets carry a junction dot", Milestone::M5);
    for (topo, tree) in topos.iter().zip(trees.iter()) {
        let taps: usize =
            topo.groups.iter().map(|g| g.pin_ids.len()).sum::<usize>() + tree.symbols.len();
        if taps >= 3 && tree.junction_dots.is_empty() && tree.segments.len() > 1 {
            c.fail(format!(
                "net '{}' (nid={}) has {taps} taps, {} segments, 0 junction dots",
                topo.net_name,
                topo.nid,
                tree.segments.len()
            ));
        }
    }
    c
}

// ── A9 ──────────────────────────────────────────────────────────────────────

/// A9: one ground glyph per ground net.
///
/// The projection layer explodes a driverless ground rail into one net per
/// consumer (`rails.rs` R-1, and `coalesce.rs` keeps Power/Ground out of the
/// union-find on purpose). `moddcdc` therefore carries five separate `GND`
/// nets, nid 501..505, and must draw five ground symbols. Anything that
/// coalesces them, or de-dups the glyph across nets, is a regression.
///
/// Counted per net, not per endpoint: a ground net with several members shares
/// one trunk and one glyph. This is a deliberate divergence from `rails.rs`
/// R-1, which specifies one in-place glyph per endpoint and no edges at all —
/// that rule governs `rail_decorations` on non-device layers, not the
/// device-layer equipotential tree (see `equipotential_tree.rs` module docs).
///
/// Known non-violations this check deliberately does not chase: a GND net whose
/// endpoints are all label-kind boxes (`build_one_topology` drops it as it has
/// no real box to anchor) draws no glyph at all, which is correct — there is
/// nothing to ground. And `build_symbols`'s `has_label_box` suppression only
/// targets the `NetLabel` branch, so it never eats a Ground glyph.
fn check_a9_ground_glyphs(topos: &[NetTopology], trees: &[EquiTree]) -> Check {
    let mut c = Check::new("A9", "one ground glyph per ground net", Milestone::M0);
    for (topo, tree) in topos.iter().zip(trees.iter()) {
        if topo.net_kind != NetKind::Ground {
            continue;
        }
        let n = tree
            .symbols
            .iter()
            .filter(|s| matches!(s.kind, TreeSymbolKind::Ground))
            .count();
        if n != 1 {
            c.fail(format!(
                "net '{}' (nid={}) draws {n} ground glyphs, expected 1",
                topo.net_name, topo.nid
            ));
        }
    }
    c
}

// ── A10 ─────────────────────────────────────────────────────────────────────

/// A10: same-side row exclusivity + no member invades a foreign row band.
///
/// Two trunk-bearing nets whose horizontal spans intersect (same side of the
/// layer anchor) must occupy different rows — otherwise their trunks overlap
/// and a member hanging from one row crosses the other's trunk (Class B, the
/// arithmetic `PIN_PITCH < TWO_PIN_SYMBOL_W` collision). And a trunk-bearing
/// net's row must not run through a foreign member's body. This is the M2-level
/// witness for the same cause A7 (M5) reports as wire hygiene, three milestones
/// earlier.
fn check_a10_same_side_rows(graph: &McVecGraph, layout_topos: &[NetTopology]) -> Check {
    let mut c = Check::new(
        "A10",
        "same-side rows exclusive, no foreign member",
        Milestone::M2,
    );

    let trunk: Vec<&NetTopology> = layout_topos.iter().filter(|t| !t.terminal_only).collect();

    // Part 1: intersecting spans must not share a row.
    for (i, a) in trunk.iter().enumerate() {
        for b in trunk.iter().skip(i + 1) {
            let span_overlap = a.lane.span.0 < b.lane.span.1 && b.lane.span.0 < a.lane.span.1;
            if span_overlap && (a.lane.axis - b.lane.axis).abs() < 0.5 {
                c.fail(format!(
                    "'{}' (nid={}) and '{}' (nid={}) share row {:.0} with intersecting spans {:?} / {:?}",
                    a.net_name, a.nid, b.net_name, b.nid, a.lane.axis, a.lane.span, b.lane.span
                ));
            }
        }
    }

    // Part 2: a trunk-bearing net's row must not run through a foreign member's
    // body (a member of another net hangs across this net's trunk line).
    for a in &trunk {
        let own: Vec<i64> = a.groups.iter().map(|g| g.box_id).collect();
        for b in &graph.boxes {
            if own.contains(&b.id) || b.is_container_box() {
                continue;
            }
            if matches!(
                b.kind,
                BoxKind::PowerLabel | BoxKind::Dot | BoxKind::PortTerminal
            ) {
                continue;
            }
            if b.w <= 0.0 || b.h <= 0.0 {
                continue;
            }
            let y = a.lane.axis;
            let within_y = y > b.y + 0.5 && y < b.y + b.h - 0.5;
            let overlap_x = a.lane.span.0 < b.x + b.w && b.x < a.lane.span.1;
            if within_y && overlap_x {
                c.fail(format!(
                    "net '{}' (nid={}) row {:.0} runs through foreign box '{}' (id={})",
                    a.net_name, a.nid, y, b.name, b.id
                ));
            }
        }
    }
    c
}

// ── A11 ─────────────────────────────────────────────────────────────────────

/// A11: two trunk-bearing nets on the same row must be W/E-opposite.
///
/// The M3.2 RowAllocator builds rows so a band is shared only by a W/E pair
/// ("two taps share a row ⟺ regions are W/E-opposite"), so this is green by
/// construction — the check exists to catch any future allocator change that
/// lets two same-side nets collide on one trunk y.
fn check_a11_same_row_opposite(layout_topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A11", "same row implies W/E opposite sides", Milestone::M3);
    let trunk: Vec<&NetTopology> = layout_topos.iter().filter(|t| !t.terminal_only).collect();
    for (i, a) in trunk.iter().enumerate() {
        for b in trunk.iter().skip(i + 1) {
            // ★ M8.2: a RUN is collinear by construction — all of its nets share
            // one row on one side, which is what lets the parts between them lie
            // ALONG it. Same row is legal for a W/E-opposite pair (M3) OR for two
            // nets of the same run.
            if (a.lane.axis - b.lane.axis).abs() < 0.5
                && !is_w_e_opposite(a.lane.region, b.lane.region)
                && a.run_root != b.run_root
            {
                c.fail(format!(
                    "'{}' (nid={}) and '{}' (nid={}) share row {:.0} but are {:?}/{:?}",
                    a.net_name, a.nid, b.net_name, b.nid, a.lane.axis, a.lane.region, b.lane.region
                ));
            }
        }
    }
    c
}

// ── A12 ─────────────────────────────────────────────────────────────────────

/// A12: consecutive row bands do not overlap — `y[k] + down(k) + ROW_CLEAR ≤
/// y[k+1]` and `y[k+1] - up(k+1) - ROW_CLEAR ≥ y[k]`, a pure RowPlan self-check
/// (no rect reads; the plan's sum-form, `y[k]+down+ROW_CLEAR ≤ y[k+1]-up`, is
/// unreachable under the `max` pitch that keeps the IC within the height
/// target — documented deviation in the M3 notes).
///
/// The band sequence is the RowAllocator's side + free bands; North/South rails
/// sit off the sequence (they hug the box edge) and islands have no band.
fn check_a12_row_band_overlap(graph: &McVecGraph, layout_topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A12", "row bands do not overlap", Milestone::M3);
    let mut banded: Vec<(usize, f64)> = layout_topos
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            !t.terminal_only && matches!(t.row_source, RowSource::SidePin | RowSource::Partner(_))
        })
        .map(|(idx, t)| (idx, t.lane.axis))
        .collect();
    banded.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

    // Group by distinct row y, then compute the band's demand as the max over
    // its nets' corridor demands.
    let mut rows: Vec<(f64, f64, f64)> = Vec::new(); // (y, down, up)
    for (idx, y) in banded {
        let (up, down) = net_corridor_demand(graph, layout_topos, idx);
        match rows.last_mut() {
            Some((ry, rd, ru)) if (*ry - y).abs() < 0.5 => {
                *rd = rd.max(down);
                *ru = ru.max(up);
            }
            _ => rows.push((y, down, up)),
        }
    }

    for w in rows.windows(2) {
        let (y0, d0, _) = w[0];
        let (y1, _, u1) = w[1];
        if y0 + d0 + ROW_CLEAR > y1 + 0.01 {
            c.fail(format!(
                "band at y={y0:.0} (down {d0:.0}) does not clear the next row y={y1:.0}"
            ));
        }
        if y1 - u1 - ROW_CLEAR < y0 - 0.01 {
            c.fail(format!(
                "band at y={y1:.0} (up {u1:.0}) does not clear the previous row y={y0:.0}"
            ));
        }
    }
    c
}

// ── A13 ─────────────────────────────────────────────────────────────────────

/// A13: no two pin slots of the same box collapse onto the same point.
///
/// M2's per-net row allocation used to give every IC pin of one net the same
/// row, so a multi-pin net's slots overlapped exactly (`moddcdc` never hit it,
/// `ldo` does: VIN+CE on POWER_SYS); the NC pin used to pile on the box middle.
/// Any two slots closer than 1.0 in both axes is a regression.
fn check_a13_pin_overlap(graph: &McVecGraph) -> Check {
    let mut c = Check::new("A13", "no overlapping pin slots", Milestone::M2);
    for b in &graph.boxes {
        for (i, s1) in b.slots.iter().enumerate() {
            for s2 in b.slots.iter().skip(i + 1) {
                let (x1, y1) = slot_point(b, s1);
                let (x2, y2) = slot_point(b, s2);
                if (x1 - x2).abs() < 1.0 && (y1 - y2).abs() < 1.0 {
                    c.fail(format!(
                        "box '{}' (id={}): pins '{}'(id={}) and '{}'(id={}) overlap at ({:.1},{:.1})",
                        b.name,
                        b.id,
                        s1.name,
                        s1.pin_id,
                        s2.name,
                        s2.pin_id,
                        x1,
                        y1
                    ));
                }
            }
        }
    }
    c
}

// ── A14 ─────────────────────────────────────────────────────────────────────

/// A14: pin labels fit inside the box. Left label max width + right label max
/// width + `2*LABEL_PAD` must be ≤ `b.w` — M2's collapsed row span used to
/// shrink the IC into a strip that spilled `VIN.Vin` / `VOUT.Vout` past the
/// edge.
fn check_a14_label_fit(graph: &McVecGraph) -> Check {
    let mut c = Check::new("A14", "pin labels fit inside the box", Milestone::M2);
    for b in &graph.boxes {
        if b.slots.is_empty() {
            continue;
        }
        let mut left = 0usize;
        let mut right = 0usize;
        let mut top = 0usize;
        let mut bottom = 0usize;
        let mut top_n = 0usize;
        let mut bottom_n = 0usize;
        for s in &b.slots {
            let n = s.name.chars().count();
            match s.side {
                EntrySide::Left => left = left.max(n),
                EntrySide::Right => right = right.max(n),
                EntrySide::Top => {
                    top = top.max(n);
                    top_n += 1;
                }
                EntrySide::Bottom => {
                    bottom = bottom.max(n);
                    bottom_n += 1;
                }
            }
        }
        // L/R: the box must fit the two longest side labels plus padding.
        // (A box with no Left/Right pins — a vertical two-pin passive — has no
        // side labels to fit, so this check is skipped for it.)
        if left > 0 || right > 0 {
            let need = (left + right) as f64 * LABEL_CHAR_W + 2.0 * LABEL_PAD;
            if need > b.w {
                c.fail(format!(
                    "box '{}' (id={}) needs width {:.0} for its labels, box is {:.0}",
                    b.name, b.id, need, b.w
                ));
            }
        }
        // M3.5 (R2): top/bottom pins are spread along the box width at
        // `(i+1)/(n+1)`; the slot spacing must fit the widest edge label, else
        // adjacent labels overlap (`SHIELD3SHIELD4` on a 5-pin bottom edge).
        for (n, widest, edge) in [(top_n, top, "top"), (bottom_n, bottom, "bottom")] {
            if n > 1 && widest > 0 {
                let spacing = b.w / (n as f64 + 1.0);
                let need = widest as f64 * LABEL_CHAR_W;
                if spacing < need {
                    c.fail(format!(
                        "box '{}' (id={}) {edge} edge: {} pins, widest label {widest} chars needs {need:.0}px/slot, spacing is {spacing:.0}",
                        b.name, b.id, n
                    ));
                }
            }
        }
    }
    c
}

// ── A15 ─────────────────────────────────────────────────────────────────────

/// A15: all Ground glyphs of the layer hang at the same y — the shared ground
/// band (M2.5 Step 7). Before the fix, terminal-only grounds each picked a free
/// stub direction locally, so two grounds on one layer sat half a page apart.
fn check_a15_ground_band(trees: &[EquiTree]) -> Check {
    let mut c = Check::new("A15", "ground stub is short", Milestone::M2);
    // ★ M7.5: the rule flipped. Up to M6 every ground glyph was pinned to one
    // shared band (`max(South row) + SYMBOL_DROP`) and this check enforced that
    // alignment — but on a layer whose free ground net sits far below the IC the
    // shared band is far below everything, so the connecting vertical grew until
    // it ran off the canvas. Short wire beats aligned glyph: a ground now hangs
    // one SYMBOL_DROP off its own trunk in the first free direction, and this
    // check enforces THAT instead.
    for tree in trees {
        for sym in tree.symbols.iter() {
            if sym.kind != TreeSymbolKind::Ground {
                continue;
            }
            // The stub is the segment that ends on the glyph. No such segment
            // means the glyph sits directly on its attach node (the degenerate
            // `pick_stub_dir` == None path) — length 0, trivially fine.
            let stub = tree
                .segments
                .iter()
                .filter(|g| {
                    (g.x1 - sym.x).abs() < 1.0 && (g.y1 - sym.y).abs() < 1.0
                        || (g.x2 - sym.x).abs() < 1.0 && (g.y2 - sym.y).abs() < 1.0
                })
                .map(|g| (g.x2 - g.x1).abs() + (g.y2 - g.y1).abs())
                .fold(f64::MAX, f64::min);
            if stub < f64::MAX && stub > SYMBOL_DROP + 1.0 {
                c.fail(format!(
                    "ground glyph of '{}' at ({:.0},{:.0}) hangs on a {stub:.0}px wire (max {:.0})",
                    tree.net_name, sym.x, sym.y, SYMBOL_DROP
                ));
            }
        }
    }
    c
}

// ── A16 ─────────────────────────────────────────────────────────────────────

/// A16: ground-net count conservation across the whole pipeline —
/// `(ground nets in the graph) == (ground topologies) == (ground glyphs drawn)`.
/// This is the M6 end-to-end witness that `coalesce_equipotential_nets` (and the
/// rail synthesis) never folds distinct per-consumer ground nets into one, which
/// would short the decoupling caps and drop ground glyphs. `coalesce` is what
/// the plan/m6.md M6.0 guards; the fixture here bypasses coalesce, so A16 locks
/// the *downstream* invariant (topo == glyph) on top of it.
fn check_a16_ground_count_conservation(
    graph: &McVecGraph,
    topos: &[NetTopology],
    trees: &[EquiTree],
) -> Check {
    let mut c = Check::new("A16", "ground net count conserved", Milestone::M6);
    let graph_gnd = graph
        .nets
        .iter()
        .filter(|n| n.kind == NetKind::Ground)
        .count();
    let topo_gnd = topos
        .iter()
        .filter(|t| t.net_kind == NetKind::Ground)
        .count();
    let glyph_gnd = trees
        .iter()
        .flat_map(|t| t.symbols.iter())
        .filter(|s| matches!(s.kind, TreeSymbolKind::Ground))
        .count();
    if graph_gnd != topo_gnd {
        c.fail(format!(
            "ground nets in graph ({graph_gnd}) != ground topologies ({topo_gnd})"
        ));
    }
    if topo_gnd != glyph_gnd {
        c.fail(format!(
            "ground topologies ({topo_gnd}) != ground glyphs drawn ({glyph_gnd})"
        ));
    }
    c
}

// ── A17 / A18 ───────────────────────────────────────────────────────────────

/// M3.5 (A17): a terminal symbol's label TEXT must not overlap any box, any
/// wire of a DIFFERENT net, nor another symbol's label text. The text bbox is
/// estimated by [`symbol_text_bbox`] (shared by every sub-check so it cannot
/// drift from the renderer).
fn check_a17_text_overlap(graph: &McVecGraph, trees: &[EquiTree]) -> Check {
    let mut c = Check::new(
        "A17",
        "symbol text overlaps no box / foreign wire",
        Milestone::M3_5,
    );
    for (tidx, tree) in trees.iter().enumerate() {
        for sym in &tree.symbols {
            if sym.label.is_empty() {
                continue;
            }
            let (bx, by, w, h) = symbol_text_bbox(sym);
            for b in &graph.boxes {
                if b.w <= 0.0 || b.h <= 0.0 {
                    continue;
                }
                if rects_overlap(bx, by, w, h, b.x, b.y, b.w, b.h) {
                    c.fail(format!(
                        "net '{}' label '{}' text box ({bx:.0},{by:.0} {w:.0}x{h:.0}) overlaps box '{}' (id={})",
                        tree.net_name, sym.label, b.name, b.id
                    ));
                }
            }
            for (tid2, tree2) in trees.iter().enumerate() {
                if tid2 == tidx {
                    continue;
                }
                for seg in &tree2.segments {
                    if segment_hits_box(seg.x1, seg.y1, seg.x2, seg.y2, bx, by, w, h) {
                        c.fail(format!(
                            "net '{}' label '{}' text box overlaps a wire of net '{}'",
                            tree.net_name, sym.label, tree2.net_name
                        ));
                    }
                }
            }
        }
    }
    // M3.5: text vs text — two TreeSymbol labels pressed together (A14 only
    // covers pin labels, not symbol labels). Compare every labelled symbol
    // against every later one, including same-net pairs (a net carries at most
    // one label, so a same-net hit is still a real duplicate).
    let labeled: Vec<(usize, &TreeSymbol, &EquiTree)> = trees
        .iter()
        .enumerate()
        .flat_map(|(ti, t)| {
            t.symbols
                .iter()
                .filter(|s| !s.label.is_empty())
                .map(move |s| (ti, s, t))
        })
        .collect();
    for (k, (_, sym, tree)) in labeled.iter().enumerate() {
        let (bx, by, w, h) = symbol_text_bbox(sym);
        for (_, sym2, tree2) in labeled.iter().skip(k + 1) {
            let (bx2, by2, w2, h2) = symbol_text_bbox(sym2);
            if rects_overlap(bx, by, w, h, bx2, by2, w2, h2) {
                c.fail(format!(
                    "net '{}' label '{}' text overlaps net '{}' label '{}'",
                    tree.net_name, sym.label, tree2.net_name, sym2.label
                ));
            }
        }
    }
    c
}

/// Estimated text bounding box of a terminal symbol, matching the renderer
/// (font 10px, ~0.6×char width, anchored by `text_side`: `-1` → "end" at x-4
/// extends left, `+1` → "start" at x+4 extends right; y centred).
fn symbol_text_bbox(sym: &TreeSymbol) -> (f64, f64, f64, f64) {
    let font_size = 10.0;
    let w = sym.label.chars().count() as f64 * font_size * 0.6;
    let h = font_size;
    // ★ M8.7: a vertical label is rotated -90 deg, so its span is a column — a
    // vertical run of ~width reading UPWARD off `sym.y`, horizontal extent ~ one
    // glyph height. Share this shape with the renderer / content bbox so A17
    // neither false-positives nor misses a vertical glyph.
    if sym.vertical {
        return (sym.x - font_size / 2.0, sym.y - w, font_size, w);
    }
    let (bx, by) = if sym.text_side < 0.0 {
        (sym.x - 4.0 - w, sym.y - h / 2.0)
    } else {
        (sym.x + 4.0, sym.y - h / 2.0)
    };
    (bx, by, w, h)
}

/// M3.5 (A18): no wire may run COLLINEAR with a box edge for a significant
/// distance — e.g. a West pin's tooth used to coincide with `x = box.x` and
/// draw a wire on top of the box border for the whole span (R3). `TOOTH_GAP`
/// moves anchor teeth outward; this check pins the invariant. Only segments
/// whose collinear overlap with the edge EXCEEDS `TOOTH_GAP` are flagged — the
/// short `TOOTH_GAP` pin leads and a terminal symbol's brief stub connection
/// are intentional, not edge-running.
fn check_a18_wire_collinear_edge(graph: &McVecGraph, trees: &[EquiTree]) -> Check {
    let mut c = Check::new(
        "A18",
        "no wire runs collinear with a box edge",
        Milestone::M3_5,
    );
    let eps = 1.0;
    for tree in trees {
        for seg in &tree.segments {
            for b in &graph.boxes {
                if b.w <= 0.0 || b.h <= 0.0 {
                    continue;
                }
                if (seg.x1 - seg.x2).abs() < eps {
                    // vertical segment: collinear with the box's left/right edge
                    for ex in [b.x, b.x + b.w] {
                        if (seg.x1 - ex).abs() < eps {
                            let ys = seg.y1.min(seg.y2);
                            let ye = seg.y1.max(seg.y2);
                            let overlap = ye.min(b.y + b.h) - ys.max(b.y);
                            if overlap > TOOTH_GAP {
                                c.fail(format!(
                                    "net '{}' segment ({:.0},{:.0})-({:.0},{:.0}) runs {overlap:.0}px along the vertical edge x={ex:.0} of box '{}' (id={})",
                                    tree.net_name, seg.x1, seg.y1, seg.x2, seg.y2, b.name, b.id
                                ));
                            }
                        }
                    }
                }
                if (seg.y1 - seg.y2).abs() < eps {
                    // horizontal segment: collinear with the box's top/bottom edge
                    for ey in [b.y, b.y + b.h] {
                        if (seg.y1 - ey).abs() < eps {
                            let xs = seg.x1.min(seg.x2);
                            let xe = seg.x1.max(seg.x2);
                            let overlap = xe.min(b.x + b.w) - xs.max(b.x);
                            if overlap > TOOTH_GAP {
                                c.fail(format!(
                                    "net '{}' segment ({:.0},{:.0})-({:.0},{:.0}) runs {overlap:.0}px along the horizontal edge y={ey:.0} of box '{}' (id={})",
                                    tree.net_name, seg.x1, seg.y1, seg.x2, seg.y2, b.name, b.id
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    c
}

/// Two axis-aligned rects overlap (inclusive).
fn rects_overlap(x0: f64, y0: f64, w0: f64, h0: f64, x1: f64, y1: f64, w1: f64, h1: f64) -> bool {
    x0 < x1 + w1 && x1 < x0 + w0 && y0 < y1 + h1 && y1 < y0 + h0
}

// ── A21 / A22 / A23 ─────────────────────────────────────────────────────────

/// M4.0 (A21): no two member boxes may occupy the same place — horizontal AND
/// vertical overlap (both > 4px) on the same row is a column collision. This is
/// the *baseline* check for M4's column allocator: it is expected to be red
/// before M4 (frac placement `(i+1)/(n+1)` can stack two members at one x) and
/// green after `place_members_by_columns`. Not gated until M4.
fn check_a21_members_do_not_overlap(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A21", "same-row members do not collide", Milestone::M4);
    let member_ids: BTreeSet<i64> = topos
        .iter()
        .flat_map(|t| t.groups.iter().skip(1).map(|g| g.box_id))
        .collect();
    let boxes: Vec<&McVecBox> = graph
        .boxes
        .iter()
        .filter(|b| member_ids.contains(&b.id) && b.geom_locked && b.w > 0.0 && b.h > 0.0)
        .collect();
    let eps = 4.0;
    for (k, a) in boxes.iter().enumerate() {
        for b in boxes.iter().skip(k + 1) {
            if a.id == b.id {
                continue;
            }
            let wx = a.x.max(a.x + a.w).min(b.x.max(b.x + b.w))
                - a.x.min(a.x + a.w).max(b.x.min(b.x + b.w));
            let wy = a.y.max(a.y + a.h).min(b.y.max(b.y + b.h))
                - a.y.min(a.y + a.h).max(b.y.min(b.y + b.h));
            if wx > eps && wy > eps {
                c.fail(format!(
                    "members '{}' (id={}) and '{}' (id={}) collide: {}x{} overlap",
                    a.name, a.id, b.name, b.id, wx as i64, wy as i64
                ));
            }
        }
    }
    c
}

/// M4.0 (A22): a cross-row (vertical) member — a two-pin box shared by two nets
/// on different rows — must sit inside BOTH rows' trunk spans. This is the
/// baseline for M4's cross-row column alignment: before M4 the two rows are enveloped
/// independently and a vertical member can drift outside one of them. Not gated
/// until M4.
fn check_a22_spanning_member_in_span(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let mut c = Check::new(
        "A22",
        "cross-row member sits in both trunk spans",
        Milestone::M4,
    );
    // box_id → (nid, span_lo, span_hi, row_axis) for every net that owns it.
    let mut owner: BTreeMap<i64, Vec<(i64, f64, f64, f64)>> = BTreeMap::new();
    for t in topos {
        if t.terminal_only || t.groups.len() < 2 {
            continue;
        }
        let (lo, hi) = t.lane.span;
        for g in t.groups.iter().skip(1) {
            owner
                .entry(g.box_id)
                .or_default()
                .push((t.nid, lo, hi, t.lane.axis));
        }
    }
    for (box_id, owners) in owner {
        // a two-pin member shared by two nets = a spanning part.
        if owners.len() < 2 {
            continue;
        }
        let Some(b) = graph.boxes.iter().find(|b| b.id == box_id) else {
            continue;
        };
        // ★ M7.6: A22's contract is a TWO-PIN spanning part sitting in both
        // trunk spans. A multi-pin Sink (connector / second IC) has its tap pin
        // at `(i+1)/(n+1)` along a wide body — off-centre by construction — so
        // requiring its CENTRE inside the short trunk span is a false positive
        // (`speaker` `spk` centre x=240 vs trunk span [200,218]).
        if b.pins.len() != 2 {
            continue;
        }
        let xc = b.x + b.w / 2.0;
        let dist = owners
            .iter()
            .filter_map(|&(nid, lo, hi, _)| {
                if nid == 0 || (lo - hi).abs() < 1e-6 {
                    // a degenerate/span-only net in one of the two; check x vs
                    // the one real span only.
                    None
                } else {
                    Some((lo, hi))
                }
            })
            .collect::<Vec<(f64, f64)>>();
        if dist.len() < 2 {
            continue;
        }
        for (lo, hi) in &dist {
            let (slo, shi) = if lo < hi { (*lo, *hi) } else { (*hi, *lo) };
            if !(xc >= slo - 4.0 && xc <= shi + 4.0) {
                c.fail(format!(
                    "cross-row member '{}' (id={}) centre x={xc:.0} outside a span [{slo:.0},{shi:.0}]",
                    b.name, box_id
                ));
            }
        }
    }
    c
}

/// M4.0 (A23): a decoupling ("Shunt") member — a two-pin vertical hang whose
/// partner is Ground or terminal-only — must sit within `2 * MEMBER_GAP` of the
/// anchor pin it decouples. Baseline for M4's pin-hugging shunt placement. Not gated until M4.
fn check_a23_shunt_near_anchor_pin(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A23", "shunt sits next to its decoupled pin", Milestone::M4);
    const MEMBER_GAP_LOCAL: f64 = 60.0;
    let limit = 2.0 * MEMBER_GAP_LOCAL;
    for t in topos {
        if t.terminal_only || t.groups.len() < 2 {
            continue;
        }
        let Some(anchor_box) = graph.boxes.iter().find(|b| b.id == t.anchor) else {
            continue;
        };
        // anchor tap pin x = first pin slot of the anchor group.
        let anchor_pin_x = t
            .groups
            .first()
            .and_then(|g| g.pin_ids.first())
            .and_then(|&pid| slot_of(anchor_box, pid))
            .map(|s| slot_point(anchor_box, s).0);
        let Some(pin_x) = anchor_pin_x else {
            continue;
        };
        // A decoupling member: two-pin vertical hang on this net.
        for g in t.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|b| b.id == g.box_id) else {
                continue;
            };
            if g.pin_ids.len() != 2 {
                continue;
            }
            let xc = b.x + b.w / 2.0;
            if (xc - pin_x).abs() > limit {
                c.fail(format!(
                    "member '{}' (id={}) centre x={xc:.0} is {:.0}px from anchor pin {pin_x:.0} (> {limit:.0})",
                    b.name, b.id, (xc - pin_x).abs()
                ));
            }
        }
    }
    c
}

// ── M5.0: A24 / A25 / A26 ─────────────────────────────────────────────────

/// M5.0 (A24): two members on the SAME side must not cross. If net A's anchor
/// tap sits left of net B's anchor tap but A's member column lands right of
/// B's column, the two connecting teeth cross into an X on the trunk. So for
/// every pair (i, j) on the same side where `anchor_pin_x[i] < anchor_pin_x[j]`
/// we require `member_centre_x[i] <= member_centre_x[j]`.
///
/// `anchor_pin_x` is the net's anchor-edge x (`b.x` for West, `b.x + b.w` for
/// East) — the exact base the column allocator grows members from, so ordering
/// by it reproduces the allocator's decision order. InlineEnd members sit AT
/// their tap and are exempt; their lateral offset is ornamental, not a tooth.
fn check_a24_no_wire_crossings(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let layer_anchor = layer_anchor_id(topos);
    let mut c = Check::new("A24", "same-side members do not cross", Milestone::M5);
    // (region, anchor_edge_x, member_centre_x, box_id, box_name)
    let mut entries: Vec<(Region, f64, f64, i64, String)> = Vec::new();
    for (idx, topo) in topos.iter().enumerate() {
        if topo.terminal_only {
            continue;
        }
        match topo.lane.region {
            Region::West | Region::East => {}
            _ => continue,
        }
        let anchor_edge = {
            let ab = graph.boxes.iter().find(|b| b.id == topo.anchor);
            match topo.lane.region {
                Region::West => ab.map(|b| b.x).unwrap_or(0.0),
                _ => ab.map(|b| b.x + b.w).unwrap_or(0.0),
            }
        };
        for group in topo.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|bb| bb.id == group.box_id) else {
                continue;
            };
            let role = tap_role(b, topo, partner_info(topos, idx, group), layer_anchor);
            if matches!(role, TapRole::InlineEnd) {
                continue;
            }
            entries.push((
                topo.lane.region,
                anchor_edge,
                b.x + b.w / 2.0,
                b.id,
                b.name.clone(),
            ));
        }
    }
    for region in [Region::West, Region::East] {
        let mut side: Vec<&(Region, f64, f64, i64, String)> =
            entries.iter().filter(|e| e.0 == region).collect();
        // Order by anchor tap x — the allocator's decision order on this side.
        side.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap().then(a.3.cmp(&b.3)));
        for w in side.windows(2) {
            let a = w[0];
            let b = w[1];
            // Same anchor edge: same column start; not a crossing.
            if (a.1 - b.1).abs() < 0.5 {
                continue;
            }
            if a.2 > b.2 + 0.5 {
                c.fail(format!(
                    "crossing: '{}' (id={}) anchor_x={:.0} col_x={:.0} ; '{}' (id={}) anchor_x={:.0} col_x={:.0}",
                    a.4, a.3, a.1, a.2, b.4, b.3, b.1, b.2
                ));
            }
        }
    }
    c
}

/// M5.0 (A25): a terminal symbol's label TEXT must not overlap a MEMBER box of
/// a DIFFERENT net. A17 already covers text-vs-IC-box and text-vs-foreign-wire;
/// A25 is the complementary member-box check (the M5.2 nudger's target).
fn check_a25_label_clear_of_members(
    graph: &McVecGraph,
    topos: &[NetTopology],
    trees: &[EquiTree],
) -> Check {
    let mut c = Check::new(
        "A25",
        "label text clears foreign member boxes",
        Milestone::M5,
    );
    // box_id -> set of owning nets from the NON-anchor (member) groups.
    let mut member_owner: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for t in topos {
        for g in t.groups.iter().skip(1) {
            member_owner.entry(g.box_id).or_default().insert(t.nid);
        }
    }
    // (net_name, symbol) for every labelled symbol, in tree order.
    let labelled: Vec<(&str, &TreeSymbol)> = trees
        .iter()
        .flat_map(|t| {
            t.symbols
                .iter()
                .filter(|s| !s.label.is_empty())
                .map(|s| (t.net_name.as_str(), s))
        })
        .collect();
    for (net_name, sym) in &labelled {
        let (bx, by, w, h) = symbol_text_bbox(sym);
        for (&bid, owners) in &member_owner {
            if owners.contains(&sym.net_id) {
                continue; // own net's member box — allowed
            }
            let Some(b) = graph.boxes.iter().find(|bb| bb.id == bid) else {
                continue;
            };
            if rects_overlap(bx, by, w, h, b.x, b.y, b.w, b.h) {
                c.fail(format!(
                    "net '{}' label '{}' text box overlaps foreign member '{}' (id={})",
                    net_name, sym.label, b.name, b.id
                ));
            }
        }
    }
    c
}

/// M5.0 (A26): on any single trunk row with >= 2 shunt (Drop) members, the
/// number hanging UP vs DOWN must differ by at most 1. Guards against a row's
/// decoupling caps piling all on one side of the trunk.
///
/// ★ M7.3: only **free** Drops (`dir == 0.0`) are counted. A Drop into a Ground
/// net is pinned DOWN by `tap_role` — the ground rails and the shared ground
/// band are always below the side rows, so hanging it up for cosmetic balance
/// forces the ground tooth back over the member's own body. Counting pinned
/// Drops here would demand exactly that (it is what flipped `modldo` `_C2` and
/// `moddcdc` `_C2` upward), so the check now measures only the freedom the
/// placer actually has.
fn check_a26_shunt_balance(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let layer_anchor = layer_anchor_id(topos);
    let mut c = Check::new("A26", "shunt up/down balance on a row", Milestone::M5);
    // row-key(axis*10) -> (up_count, down_count)
    let mut per_row: BTreeMap<i64, (usize, usize)> = BTreeMap::new();
    for (idx, topo) in topos.iter().enumerate() {
        if topo.terminal_only {
            continue;
        }
        let row_key = (topo.lane.axis * 10.0).round() as i64;
        for group in topo.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|bb| bb.id == group.box_id) else {
                continue;
            };
            let role = tap_role(b, topo, partner_info(topos, idx, group), layer_anchor);
            let TapRole::Drop { dir } = role else {
                continue;
            };
            if dir != 0.0 {
                continue; // pinned by electrics, not free to balance
            }
            let cy = b.y + b.h / 2.0;
            let entry = per_row.entry(row_key).or_default();
            if cy < topo.lane.axis {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
    }
    for (row_key, (up, down)) in per_row {
        let total = up + down;
        if total >= 2 && (up as isize - down as isize).abs() > 1 {
            c.fail(format!(
                "row y={:.0} has {up} up / {down} down shunts (imbalanced)",
                row_key as f64 / 10.0
            ));
        }
    }
    c
}

// ── M7.4: A27 ──────────────────────────────────────────────

/// ★ M7.4 (A27): a West/East net's trunk row sits ON one of its layer-anchor
/// pins.
///
/// This is the check whose absence let the row desync ship. A2 compares the
/// layout lane against the render replay — both were wrong in the SAME way, so
/// A2 stayed green while every pin sat a constant offset above its own trunk and
/// `realize` drew a long tooth from the pin down to the row, through whatever
/// member hung in between. Nothing in A1..A26 relates a placed pin's y back to
/// `lane.axis`; A27 does exactly that and nothing else.
///
/// A net with several pins on one side legitimately reaches the extra ones
/// through a tooth (`ldo` POWER_SYS = VIN + CE), so the requirement is that at
/// least ONE of its anchor pins lands on the row — not all of them.
fn check_a27_pin_on_its_row(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A27", "IC side pin sits on its net's row", Milestone::M6);
    let layer_anchor = layer_anchor_id(topos);
    let Some(ab) = graph.boxes.iter().find(|b| b.id == layer_anchor) else {
        return c;
    };
    for topo in topos {
        if topo.terminal_only {
            continue;
        }
        if !matches!(topo.lane.region, Region::West | Region::East) {
            continue;
        }
        let Some(g) = topo.groups.first() else {
            continue;
        };
        if g.box_id != layer_anchor {
            continue;
        }
        let mut best = f64::MAX;
        for &pid in &g.pin_ids {
            let Some(slot) = slot_of(ab, pid) else {
                continue;
            };
            if !matches!(slot.side, EntrySide::Left | EntrySide::Right) {
                continue;
            }
            let (_px, py) = slot_point(ab, slot);
            best = best.min((py - topo.lane.axis).abs());
        }
        if best < f64::MAX && best > 1.0 {
            c.fail(format!(
                "net '{}' (nid={}) row y={:.0} but its nearest anchor pin is {best:.0}px away",
                topo.net_name, topo.nid, topo.lane.axis
            ));
        }
    }
    c
}

// ── M8: A28 / A29 ───────────────────────────────────────────

/// ★ M8 (A28): the two nets of an ALONG part are collinear.
///
/// An Along part is drawn IN the wire, so both of its nets must sit on the same
/// row; otherwise the "one straight line with a component inserted" reading
/// breaks and the part's two pins point at two different rows.
fn check_a28_along_is_collinear(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let layer_anchor = layer_anchor_id(topos);
    let mut c = Check::new("A28", "Along part's two nets share a row", Milestone::M6);
    for (idx, topo) in topos.iter().enumerate() {
        if topo.terminal_only {
            continue;
        }
        for group in topo.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|bb| bb.id == group.box_id) else {
                continue;
            };
            let p = partner_info(topos, idx, group);
            if !matches!(tap_role(b, topo, p, layer_anchor), TapRole::Series { .. }) {
                continue;
            }
            let other = topos
                .iter()
                .enumerate()
                .find(|(j, o)| *j != idx && o.groups.iter().any(|g| g.box_id == group.box_id));
            let Some((_, other)) = other else { continue };
            // ★ M10.3: a TERMINAL-ONLY partner owns no row of its own — its
            // glyph hangs off this part's far pin, so there is no second row to
            // be collinear with, and its `lane.axis` is the untouched default
            // (`resolve_lane_for_topo` returns early for terminal-only nets).
            // Comparing it was harmless only while no fixture had a Series into
            // one; M8.6 (`speaker` VDD_3V3) and M10.3 (every ground adopted as a
            // run's outer end) both do. Widening an assertion is worth a second
            // look — this is the one relaxation in the round.
            if other.terminal_only {
                continue;
            }
            // ★ M12.1: a ground COLUMN is a node, not a row. Its arms are
            // horizontal on their OWN rows and meet on the column's vertical, so
            // collinearity with the node's row is not the contract here — A31's
            // end budget is.
            if other.ground_column {
                continue;
            }
            if (other.lane.axis - topo.lane.axis).abs() > 1.0 {
                c.fail(format!(
                    "'{}' lies along the wire between '{}' (row {:.0}) and '{}' (row {:.0}) — not collinear",
                    b.name, topo.net_name, topo.lane.axis, other.net_name, other.lane.axis
                ));
            }
        }
    }
    c
}

/// ★ M8 (A29): the trunks of one RUN tile the row — they never overlap.
///
/// Two nets of a run are separated by the part between them, so their spans meet
/// on its pins rather than running through each other. An overlap means the
/// prefix sum in `chain_origins` under-reserved and one net's members are
/// sitting on its neighbour's wire.
fn check_a29_run_spans_disjoint(topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A29", "run trunks do not overlap", Milestone::M6);
    for (i, a) in topos.iter().enumerate() {
        if a.terminal_only {
            continue;
        }
        for b in topos.iter().skip(i + 1) {
            if b.terminal_only || a.run_root != b.run_root || a.nid == b.nid {
                continue;
            }
            let alo = a.lane.span.0.min(a.lane.span.1);
            let ahi = a.lane.span.0.max(a.lane.span.1);
            let blo = b.lane.span.0.min(b.lane.span.1);
            let bhi = b.lane.span.0.max(b.lane.span.1);
            if alo.max(blo) + 1.0 < ahi.min(bhi) {
                c.fail(format!(
                    "'{}' [{alo:.0},{ahi:.0}] and '{}' [{blo:.0},{bhi:.0}] are one run but their trunks overlap",
                    a.net_name, b.net_name
                ));
            }
        }
    }
    c
}

// ── M9: A30 ─────────────────────────────────────────────────────────────────

/// ★ M9 (A30): a SATELLITE component's facing pin sits on its net's row.
///
/// The point of placing a component beside the anchor, instead of hanging it off
/// one row, is that every shared net becomes a straight horizontal wire. That
/// only holds if the pin is ON the row; a pin one band off drags the wire around
/// the box, which is the `speaker` `_net9` defect this milestone exists to fix.
///
/// Satellites are exactly the non-anchor multi-pin boxes with Left/Right slots:
/// the M7.6 `Sink` shape puts its pins on Top/Bottom, so the two never collide.
fn check_a30_satellite_pins_on_rows(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A30", "satellite facing pin sits on its row", Milestone::M6);
    let layer_anchor = layer_anchor_id(topos);
    for b in &graph.boxes {
        if b.id == layer_anchor || b.pins.len() < 3 {
            continue;
        }
        for slot in b.slots.iter() {
            if !matches!(slot.side, EntrySide::Left | EntrySide::Right) {
                continue;
            }
            let owner = topos.iter().find(|t| {
                t.groups
                    .iter()
                    .any(|g| g.box_id == b.id && g.pin_ids.contains(&slot.pin_id))
            });
            let Some(t) = owner.filter(|t| t.lane.horizontal && !t.terminal_only) else {
                continue;
            };
            let py = b.y + b.h * slot.offset;
            if (py - t.lane.axis).abs() > 1.0 {
                c.fail(format!(
                    "'{}' pin {} is at y={py:.0} but its net '{}' runs at {:.0}",
                    b.name, slot.name, t.net_name, t.lane.axis
                ));
            }
        }
    }
    c
}

/// ★ M15.4 (A34): EVERY pin of a net lies on that net's row and inside its span
/// — not just the anchor group's.
///
/// A27 checks the anchor group only, which was fine while the anchor group was
/// always the layer anchor. It is not: on `mic` the interesting nets are anchored
/// on a SATELLITE, and every defect of the last three rounds showed up as a pin
/// stub on one edge of that satellite with its trunk somewhere else — a shape
/// A27 structurally could not see.
///
/// A multi-pin box hanging off a row as a `Sink` is exempt: its pins face the
/// trunk from above or below by design, and A30 owns that case.
fn check_a34_every_pin_on_its_row(graph: &McVecGraph, topos: &[NetTopology]) -> Check {
    let mut c = Check::new("A34", "every pin lies on its net's row", Milestone::M7);
    for topo in topos.iter() {
        if topo.terminal_only || !topo.lane.horizontal {
            continue;
        }
        let (lo, hi) = (
            topo.lane.span.0.min(topo.lane.span.1),
            topo.lane.span.0.max(topo.lane.span.1),
        );
        for (gi, group) in topo.groups.iter().enumerate() {
            let Some(b) = graph.boxes.iter().find(|bb| bb.id == group.box_id) else {
                continue;
            };
            // Two-pin members and Sinks hang OFF the row on purpose.
            if gi > 0 && b.pins.len() < 3 {
                continue;
            }
            if gi > 0
                && !b
                    .slots
                    .iter()
                    .any(|s| matches!(s.side, EntrySide::Left | EntrySide::Right))
            {
                continue; // a Sink: pins on Top/Bottom, A30's business
            }
            for &pid in &group.pin_ids {
                let Some(s) = slot_of(b, pid) else { continue };
                let (px, py) = slot_point(b, s);
                if (py - topo.lane.axis).abs() > 1.0 {
                    c.fail(format!(
                        "'{}' pin {} on '{}' sits at y={:.0} but the row is {:.0}",
                        topo.net_name, pid, b.name, py, topo.lane.axis
                    ));
                } else if px < lo - TOOTH_GAP - 1.0 || px > hi + TOOTH_GAP + 1.0 {
                    c.fail(format!(
                        "'{}' pin {} on '{}' sits at x={:.0}, outside the span ({:.0},{:.0})",
                        topo.net_name, pid, b.name, px, lo, hi
                    ));
                }
            }
        }
    }
    c
}

// ── M11: A31 / A32 ──────────────────────────────────────────────────────────

/// ★ M11 (A31): a row has at most TWO horizontal ends.
///
/// > an equipotential point has exactly TWO horizontal things on it: a start
/// > (pin / label / component) and an end (pin / label / component).
///
/// The subtlety is WHAT gets counted. Not parts — **directions**. A parallel
/// bundle (several parts joining the SAME pair of nets) leaves the row once, in
/// one direction, stacked in y over one column interval, so it is ONE end:
///
/// ```text
///   lpa.1 ─[R1]─ ┬─[R2]─┬─ VCC       R1, R2, R3 are all horizontal,
///                └─[R3]─┘            but the middle net has 2 ends:
///                                    inner = R1, outer = the {R2,R3} bundle
/// ```
///
/// So the count is: the net's own anchor pin (the inner end, at most one), plus
/// one per DISTINCT partner net reached through an `Along` part, plus a
/// satellite component sitting on this row (M9 — "ends at a component"), plus
/// every terminal glyph drawn horizontally (`|dir.0| > 0`).
///
/// Three or more means two things want the same end of the same wire. That is
/// the fan-out `equi_chain`'s end budget prevents, and the shape a netlist LOOP
/// still produces: `pin ─[R1]─ X ─[R2]─ Y` plus a part straight from `pin` to
/// `Y` is one run whose chord has nowhere vertical to go, both of its nets being
/// on this row. That case is a known gap — this report is how it stays visible
/// instead of quietly drawing a wire over a part.
fn check_a31_row_end_budget(
    graph: &McVecGraph,
    topos: &[NetTopology],
    trees: &[EquiTree],
) -> Check {
    let mut c = Check::new(
        "A31",
        "a row has at most two horizontal ends",
        Milestone::M7,
    );
    let layer_anchor = layer_anchor_id(topos);
    for (idx, topo) in topos.iter().enumerate() {
        if topo.terminal_only || !matches!(topo.lane.region, Region::West | Region::East) {
            continue;
        }
        let mut ends: Vec<String> = Vec::new();
        if topo.groups.iter().any(|g| g.box_id == layer_anchor) {
            ends.push("its anchor pin".to_string());
        }
        let mut partners: BTreeSet<i64> = BTreeSet::new();
        for group in topo.groups.iter().skip(1) {
            let Some(b) = graph.boxes.iter().find(|bb| bb.id == group.box_id) else {
                continue;
            };
            if b.pins.len() >= 3 {
                // A satellite is a non-anchor multi-pin box with Left/Right
                // slots; the M7.6 `Sink` shape puts its pins on Top/Bottom, so
                // the two never collide (same test as A30).
                if b.id != layer_anchor
                    && b.slots
                        .iter()
                        .any(|s| matches!(s.side, EntrySide::Left | EntrySide::Right))
                {
                    ends.push(format!("component '{}'", b.name));
                }
                continue;
            }
            let p = partner_info(topos, idx, group);
            if !matches!(
                tap_role(b, topo, p.clone(), layer_anchor),
                TapRole::Series { .. }
            ) {
                continue;
            }
            let Some(p) = p else { continue };
            // One entry per PARTNER NET: a parallel bundle is one end.
            if partners.insert(topos[p.topo_idx].nid) {
                ends.push(format!("part '{}'", b.name));
            }
        }
        for sym in trees.get(idx).map(|t| t.symbols.as_slice()).unwrap_or(&[]) {
            if sym.dir.0.abs() > 0.5 {
                ends.push(match sym.kind {
                    TreeSymbolKind::Ground => "a ground glyph".to_string(),
                    _ => format!("label '{}'", sym.label),
                });
            }
        }
        if ends.len() > 2 {
            c.fail(format!(
                "'{}' has {} horizontal ends on one row: {}",
                topo.net_name,
                ends.len(),
                ends.join(", ")
            ));
        }
    }
    c
}

/// ★ M11 (A32): a name is pulled OFF its wire on a stub.
///
/// A `NetLabel` / `BusLabel` / `PortLabel` glyph sitting ON its own trunk is the
/// `speaker` `US_SPEAKER_MUTE` defect: M8.7 parked it 4px off the row, so the
/// bus circle landed on the junction dot and the name read as text painted onto
/// the wire. Either the glyph is a full `SYMBOL_DROP` off the row, or it is out
/// past the end of the span — a name written along the wire beyond its last
/// member is the normal horizontal case and is fine.
fn check_a32_label_has_a_stub(topos: &[NetTopology], trees: &[EquiTree]) -> Check {
    let mut c = Check::new("A32", "a label is pulled off its wire", Milestone::M7);
    for (idx, topo) in topos.iter().enumerate() {
        if topo.terminal_only || !topo.lane.horizontal {
            continue;
        }
        let (lo, hi) = (
            topo.lane.span.0.min(topo.lane.span.1),
            topo.lane.span.0.max(topo.lane.span.1),
        );
        for sym in trees.get(idx).map(|t| t.symbols.as_slice()).unwrap_or(&[]) {
            if !matches!(
                sym.kind,
                TreeSymbolKind::NetLabel | TreeSymbolKind::BusLabel | TreeSymbolKind::PortLabel
            ) {
                continue;
            }
            let off_the_row = (sym.y - topo.lane.axis).abs() >= SYMBOL_DROP - 1.0;
            let past_the_end = sym.x < lo - 0.5 || sym.x > hi + 0.5;
            if !off_the_row && !past_the_end {
                c.fail(format!(
                    "label '{}' of '{}' sits on its own trunk at ({:.0},{:.0}), row {:.0}",
                    sym.label, topo.net_name, sym.x, sym.y, topo.lane.axis
                ));
            }
        }
    }
    c
}

// ============================================================================
// Fixtures
// ============================================================================

/// `moddcdc` — the LP3220 buck reference used as the M0..M6 golden.
///
/// Netlist as reported by the module dump (10 nets, 20 connections). Note the
/// **five separate `GND` nets**: `rails.rs` explodes ground into per-consumer
/// flags on purpose, and the target schematic draws five independent ground
/// symbols. Nothing downstream may coalesce them. `501` is the SHARED ground
/// `GND ~ C1.2 ~ C2.2` — M12 turns it into a COLUMN (two horizontal arms).
///
/// ID plan (stable — golden diffs depend on it):
/// ```text
///   1        lp322dcdc      pins 101..105  = EN, GND, LX, VIN, FB
///   11..15   CAP_1..CAP_5   pins 1n1 / 1n2
///   21..23   RES_1..RES_3   pins 2n1 / 2n2
///   31       IND_1          pins 311 / 312
///   41..45   GND labels     pins 411..451
///   46       VCC_1V2 label  pin 461
///   47       VDD_3V3 label  pin 471
///   501      GND @ CAP_1 + CAP_2 (shared → M12 column)
///   502..504 GND @ CAP_3/4/RES_3   505 GND @ lp322dcdc.2
/// ```
#[cfg(test)]
pub(crate) mod fixture {
    use crate::vector::graph::boxdef::{BoxPin, IoSummary, PortDir};
    use crate::vector::graph::kinds::{BoxKind, NetKind};
    use crate::vector::graph::netdef::{EndpointRef, IoDirection, NetRole};
    use crate::vector::graph::symbol::Symbol;
    use crate::vector::graph::{LayerStyle, McVecBox, McVecGraph, VizNet};

    pub(crate) fn mk_box(
        id: i64,
        name: &str,
        class: &str,
        kind: BoxKind,
        symbol: Symbol,
        pins: &[(i64, &str, &str, IoDirection)],
    ) -> McVecBox {
        let mut b = McVecBox::new_v2(
            id,
            name.into(),
            class.into(),
            kind,
            symbol,
            None,
            None,
            pins.len(),
            IoSummary::new(),
            name.into(),
            vec![],
        );
        for (pid, number, desc, io) in pins {
            b.pins.push(BoxPin {
                id: *pid,
                pin_id: (*number).into(),
                description: (*desc).into(),
                io: *io,
                port_dir: PortDir::None,
            });
        }
        b
    }

    pub(crate) fn two_pin(
        id: i64,
        name: &str,
        class: &str,
        symbol: Symbol,
        a: i64,
        b: i64,
    ) -> McVecBox {
        mk_box(
            id,
            name,
            class,
            BoxKind::TwoPin,
            symbol,
            &[
                (a, "1", "", IoDirection::Passive),
                (b, "2", "", IoDirection::Passive),
            ],
        )
    }

    fn label(id: i64, name: &str, pin: i64, is_ground: bool) -> McVecBox {
        mk_box(
            id,
            name,
            "",
            BoxKind::PowerLabel,
            Symbol::PowerRail { is_ground },
            &[(pin, "1", "", IoDirection::Passive)],
        )
    }

    pub(crate) fn net(nid: i64, name: &str, kind: NetKind, eps: &[(i64, i64)]) -> VizNet {
        VizNet::new(
            nid,
            name.into(),
            kind,
            NetRole::Signal,
            eps.iter()
                .map(|&(b, p)| EndpointRef::new(b, p, ""))
                .collect(),
        )
    }

    pub(crate) fn build_moddcdc_graph() -> McVecGraph {
        let mut g = McVecGraph::new(1000, "moddcdc".into());
        g.layer_style = LayerStyle::Device;

        g.boxes.push(mk_box(
            1,
            "lp322dcdc",
            "LP3220AB5F",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (101, "1", "EN", IoDirection::Input),
                (102, "2", "GND", IoDirection::Ground),
                (103, "3", "LX", IoDirection::Output),
                (104, "4", "VIN", IoDirection::Power),
                (105, "5", "FB", IoDirection::Input),
            ],
        ));

        for (id, name, a, b) in [
            (11, "CAP_1", 111, 112),
            (12, "CAP_2", 121, 122),
            (13, "CAP_3", 131, 132),
            (14, "CAP_4", 141, 142),
            (15, "CAP_5", 151, 152),
        ] {
            g.boxes
                .push(two_pin(id, name, "CAP", Symbol::Capacitor, a, b));
        }
        for (id, name, a, b) in [
            (21, "RES_1", 211, 212),
            (22, "RES_2", 221, 222),
            (23, "RES_3", 231, 232),
        ] {
            g.boxes
                .push(two_pin(id, name, "RES", Symbol::Resistor, a, b));
        }
        g.boxes
            .push(two_pin(31, "IND_1", "IND", Symbol::Inductor, 311, 312));

        for (id, pin) in [(41, 411), (42, 421), (43, 431), (44, 441), (45, 451)] {
            g.boxes.push(label(id, "GND", pin, true));
        }
        g.boxes.push(label(46, "VCC_1V2", 461, false));
        g.boxes.push(label(47, "VDD_3V3", 471, false));

        // ★ M12.1: a SHARED ground node — `GND ~ CAP_1.2 ~ CAP_2.2`. Two caps
        // into one ground: M10.3 adopted it as the EN run's outer end, and M12
        // turns it into a COLUMN so BOTH caps lie horizontal on their own rows
        // (`VDD_3V3` and `_net1`) and stop at the node's x.
        // GND ~ CAP_1.2 ~ CAP_2.2
        g.nets.push(net(
            501,
            "GND",
            NetKind::Ground,
            &[(41, 411), (11, 112), (12, 122)],
        ));
        // GND ~ CAP_3.2
        g.nets
            .push(net(502, "GND", NetKind::Ground, &[(42, 421), (13, 132)]));
        // GND ~ CAP_4.2
        g.nets
            .push(net(503, "GND", NetKind::Ground, &[(43, 431), (14, 142)]));
        // GND ~ RES_3.2
        g.nets
            .push(net(504, "GND", NetKind::Ground, &[(44, 441), (23, 232)]));
        // GND ~ lp322dcdc.2
        g.nets
            .push(net(505, "GND", NetKind::Ground, &[(45, 451), (1, 102)]));
        // IND_1.2 ~ VCC_1V2 ~ CAP_3.1 ~ CAP_4.1 ~ RES_2.1
        g.nets.push(net(
            506,
            "VCC_1V2",
            NetKind::Power,
            &[(31, 312), (46, 461), (13, 131), (14, 141), (22, 221)],
        ));
        // lp322dcdc.4 ~ RES_1.1 ~ VDD_3V3 ~ CAP_1.1
        g.nets.push(net(
            507,
            "VDD_3V3",
            NetKind::Power,
            &[(1, 104), (21, 211), (47, 471), (11, 111)],
        ));
        // RES_1.2 ~ lp322dcdc.1 ~ CAP_2.1
        g.nets.push(net(
            508,
            "__net_1",
            NetKind::Signal,
            &[(21, 212), (1, 101), (12, 121)],
        ));
        // lp322dcdc.3 ~ IND_1.1 ~ CAP_5.2
        g.nets.push(net(
            509,
            "__net_3",
            NetKind::Signal,
            &[(1, 103), (31, 311), (15, 152)],
        ));
        // RES_2.2 ~ lp322dcdc.5 ~ RES_3.1 ~ CAP_5.1
        g.nets.push(net(
            510,
            "__net_5",
            NetKind::Signal,
            &[(22, 222), (1, 105), (23, 231), (15, 151)],
        ));

        g
    }

    /// `ldo` — the LDO.SGM2019 single-rail regulator used as the M2.5 regression
    /// witness. M2 broke its rendering in four machine-checkable ways, all
    /// reproduced here:
    ///   * **two IC pins on one net** — POWER_SYS carries both ldo.101 (VIN) and
    ///     ldo.103 (CE), so the anchor group is 2 pins → they used to collapse
    ///     onto the same row/offset (B2);
    ///   * **an unconnected pin** — ldo.104 (FB) belongs to no net → it used to
    ///     pile onto the box middle at offset 0.5 (B3);
    ///   * **box collapse / label overflow** — pin 4 is NC so the side rows are
    ///     sparse and the box height used to follow the row span (B4/R3);
    ///   * **two GND symbols at different y** (D5).
    ///
    /// Pins: 1=VIN(Power) 2=GND 3=CE(Input) 4=FB(unconnected) 5=VOUT(Output).
    pub(crate) fn build_ldo_graph() -> McVecGraph {
        let mut g = McVecGraph::new(2000, "ldo".into());
        g.layer_style = LayerStyle::Device;

        g.boxes.push(mk_box(
            1,
            "ldo",
            "LDO.SGM2019_33YN5G_TR",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (101, "1", "VIN", IoDirection::Power),
                (102, "2", "GND", IoDirection::Ground),
                (103, "3", "CE", IoDirection::Input),
                (104, "4", "FB", IoDirection::Input),
                (105, "5", "VOUT", IoDirection::Output),
            ],
        ));
        // Input cap: POWER_SYS → GND.
        g.boxes
            .push(two_pin(11, "CAP_1", "CAP", Symbol::Capacitor, 111, 112));
        // Output cap: VCC → GND.
        g.boxes
            .push(two_pin(12, "CAP_2", "CAP", Symbol::Capacitor, 121, 122));
        // Port labels.
        g.boxes.push(label(21, "POWER_SYS", 211, false));
        g.boxes.push(label(22, "VCC", 221, false));
        // Two ground labels (per-consumer grounds stay separate, like moddcdc).
        g.boxes.push(label(23, "GND", 231, true));
        g.boxes.push(label(24, "GND", 241, true));

        // POWER_SYS: ldo.101 (VIN) AND ldo.103 (CE) on one net — the anchor group
        // has two IC pins, reproducing the B2 overlap.
        g.nets.push(net(
            301,
            "POWER_SYS",
            NetKind::Power,
            &[(1, 101), (1, 103), (11, 111), (21, 211)],
        ));
        // VCC: ldo.105 (VOUT) + output cap + label.
        g.nets.push(net(
            302,
            "VCC",
            NetKind::Power,
            &[(1, 105), (12, 121), (22, 221)],
        ));
        // GND_A: ldo.102 + input cap + label.
        g.nets.push(net(
            303,
            "GND",
            NetKind::Ground,
            &[(1, 102), (11, 112), (23, 231)],
        ));
        // GND_B: output cap + label.
        g.nets
            .push(net(304, "GND", NetKind::Ground, &[(12, 122), (24, 241)]));
        // ldo.104 (FB) is deliberately in NO net → the NC pin.

        g
    }

    /// `series_bridge_shunt` — the M4.5 buck-converter witness for the column
    /// model (A21 / A22 / A23).
    ///
    /// A switching buck exercises the column allocator's two reachable
    /// two-pin roles side by side on one picture:
    ///   * **Drop (shunt)** — `CAP_IN` (VIN→GND) and `CAP_OUT` (VOUT→GND)
    ///     hug their decoupled anchor pin (A23);
    ///   * **Bridge** — `IND` (SW→VOUT, free East net on a lower row) and
    ///     `R_FB` (VOUT↔FB) span two trunk rows and must sit inside BOTH
    ///     trunks' spans (A22);
    /// and A21 asserts none of them collide — the exact regression the
    /// per-net frac placement produced. A true `Series` (same-row partner)
    /// is structurally unreachable in the RowAllocator (a free net always
    /// lands strictly below its partner band), so Bridge/Drop are the
    /// exhaustive vertical cases the allocator must handle.
    ///
    /// ID plan:
    /// ```text
    ///   1        conv          pins 101..105 = VIN, GND, SW, FB, EN
    ///   11       CAP_IN        pins 111 / 112
    ///   12       CAP_EN        pins 121 / 122
    ///   13       CAP_OUT       pins 131 / 132
    ///   21       R_FB          pins 211 / 212
    ///   31       IND           pins 311 / 312
    /// ```
    pub(crate) fn build_series_bridge_graph() -> McVecGraph {
        let mut g = McVecGraph::new(3000, "series_bridge_shunt".into());
        g.layer_style = LayerStyle::Device;

        // A 5-pin buck (EN, GND, SW, FB, VIN) mirrors moddcdc's proven-clean
        // side balance: three West rows (EN / FB / VIN) + one East (SW), with
        // VIN placed at the BOTTOM West slot so its long input-shunt tooth
        // hangs below the other West labels (A17). Row order is pin order.
        g.boxes.push(mk_box(
            1,
            "conv",
            "BUCK",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (101, "1", "EN", IoDirection::Input),
                (102, "2", "GND", IoDirection::Ground),
                (103, "3", "SW", IoDirection::Output),
                (104, "4", "FB", IoDirection::Input),
                (105, "5", "VIN", IoDirection::Power),
            ],
        ));
        // Input shunt: VIN → GND.
        g.boxes
            .push(two_pin(11, "CAP_IN", "CAP", Symbol::Capacitor, 111, 112));
        // Enable shunt: EN → GND.
        g.boxes
            .push(two_pin(12, "CAP_EN", "CAP", Symbol::Capacitor, 121, 122));
        // Output shunt: VOUT → GND.
        g.boxes
            .push(two_pin(13, "CAP_OUT", "CAP", Symbol::Capacitor, 131, 132));
        // Feedback divider: VOUT → FB.
        g.boxes
            .push(two_pin(21, "R_FB", "RES", Symbol::Resistor, 211, 212));
        // Output inductor: SW → VOUT (the bridge across two rows).
        g.boxes
            .push(two_pin(31, "IND", "IND", Symbol::Inductor, 311, 312));
        // Port labels so the power nets carry a visible terminal.
        g.boxes.push(label(41, "VDD", 411, false));
        g.boxes.push(label(42, "GND", 421, true));

        // GND (input): conv.102 + VIN/EN shunts + label. Per-consumer ground,
        // like moddcdc — the output cap's ground is a SEPARATE net so VOUT
        // inherits East (from SW) instead of South, mirroring a real buck.
        g.nets.push(net(
            601,
            "GND",
            NetKind::Ground,
            &[(1, 102), (11, 112), (12, 122), (42, 421)],
        ));
        // GND_B (output): output shunt alone → terminal-only per-consumer ground.
        g.nets.push(net(606, "GND", NetKind::Ground, &[(13, 132)]));
        // VIN (Power, West): conv.105 + input shunt + label.
        g.nets.push(net(
            602,
            "VIN",
            NetKind::Power,
            &[(1, 105), (11, 111), (41, 411)],
        ));
        // EN (Signal, West): conv.101 + enable shunt.
        g.nets
            .push(net(607, "EN", NetKind::Signal, &[(1, 101), (12, 121)]));
        // SW (Signal, East): conv.103 + inductor.
        g.nets
            .push(net(603, "SW", NetKind::Signal, &[(1, 103), (31, 311)]));
        // VOUT (free, East): inductor + output shunt + feedback divider.
        g.nets.push(net(
            604,
            "VOUT",
            NetKind::Power,
            &[(31, 312), (13, 131), (21, 211)],
        ));
        // FB (Signal, West): conv.104 + feedback divider.
        g.nets
            .push(net(605, "FB", NetKind::Signal, &[(1, 104), (21, 212)]));

        g
    }

    /// ★ M7.6 `two_anchor` — a layer with TWO multi-pin components.
    ///
    /// Every fixture up to M7 had exactly one: `lp322dcdc`, `ldo`, the buck.
    /// So `assign_anchor_slots` (label-driven size, pins over four sides,
    /// `connected` from the topology) was exercised, and the `TapRole::Sink`
    /// path that every OTHER multi-pin box takes never was. The real `speaker`
    /// layer has an amplifier AND a speaker connector, and the connector came
    /// out an 89×20 box with four pins 18px apart, two "GND" labels printed on
    /// top of each other and every pin marked NC — with A14 fully capable of
    /// catching it and no fixture to run it on. This is that fixture.
    ///
    /// Trimmed from `speaker`: `lpa` drives `spk` through its two outputs, the
    /// connector's other two pins are ground.
    ///
    /// ```text
    ///   1      lpa   pins 1..4     = VDD, GND, VO1, VO2
    ///   2      spk   pins 101..104 = 1, 2, GND, GND
    ///   11     CAP_1 pins 111 / 112
    ///   41..43 GND labels     51 VDD_3V3 label
    /// ```
    pub(crate) fn build_two_anchor_graph() -> McVecGraph {
        let mut g = McVecGraph::new(3000, "speaker".into());
        g.layer_style = LayerStyle::Device;

        // The AMPLIFIER holds pin numbers 1..4 (the LOW end). `lpa` and `spk`
        // both connect the one-pin nets `__net_8`/`__net_9`, and the layer-anchor
        // tie-break is "lowest source line wins" (`select_anchor_deterministic`),
        // so `lpa` must carry the lower numbers to stay the layer anchor and
        // leave `spk` as the `TapRole::Sink` under test.
        g.boxes.push(mk_box(
            1,
            "lpa",
            "LPA4871",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (1, "1", "VDD", IoDirection::Power),
                (2, "2", "GND", IoDirection::Ground),
                (3, "3", "VO1", IoDirection::Output),
                (4, "4", "VO2", IoDirection::Output),
                // A second ground pin: a real power amp carries one, and it puts
                // `lpa` one net-endpoint ahead of `spk` so the layer-anchor
                // degree tie-break keeps the amplifier (not the connector) as the
                // layer anchor; `spk` then stays the `TapRole::Sink` under test.
                (5, "5", "GND", IoDirection::Ground),
            ],
        ));
        // The connector: two signal pins, two ground pins with the SAME label.
        // Four pins on one edge at `(i+1)/(n+1)` is what used to overlap.
        g.boxes.push(mk_box(
            2,
            "spk",
            "SPEAKER.PHB2AWB",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (101, "1", "OUTP", IoDirection::Passive),
                (102, "2", "OUTN", IoDirection::Passive),
                (103, "3", "GND", IoDirection::Ground),
                (104, "4", "GND", IoDirection::Ground),
            ],
        ));
        g.boxes
            .push(two_pin(11, "CAP_1", "CAP", Symbol::Capacitor, 111, 112));
        for (id, pin) in [(41, 411), (42, 421), (43, 431)] {
            g.boxes.push(label(id, "GND", pin, true));
        }
        g.boxes.push(label(51, "VDD_3V3", 511, false));

        // lpa.VDD ~ CAP_1.1 ~ VDD_3V3
        g.nets.push(net(
            701,
            "VDD_3V3",
            NetKind::Power,
            &[(1, 1), (11, 111), (51, 511)],
        ));
        // lpa.VO1 ~ spk.1   /   lpa.VO2 ~ spk.2
        // Single-underscore `_net8`/`_net9` (like the real speaker) so the
        // auto-name guard keeps them label-less — a NetLabel would sit on the
        // satellite's facing pin.
        g.nets
            .push(net(702, "_net8", NetKind::Signal, &[(1, 3), (2, 101)]));
        g.nets
            .push(net(703, "_net9", NetKind::Signal, &[(1, 4), (2, 102)]));
        // Grounds, per consumer.
        g.nets.push(net(
            704,
            "GND",
            NetKind::Ground,
            &[(41, 411), (1, 2), (1, 5)],
        ));
        g.nets.push(net(
            705,
            "GND",
            NetKind::Ground,
            &[(42, 421), (2, 103), (2, 104)],
        ));
        g.nets
            .push(net(706, "GND", NetKind::Ground, &[(43, 431), (11, 112)]));

        g
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::equipotential_tree::{place_by_topology, MIN_BOX_W, MIN_SINK_H};
    use super::fixture::{
        build_ldo_graph, build_moddcdc_graph, build_series_bridge_graph, build_two_anchor_graph,
    };
    use super::*;

    /// Run the pipeline once and hand back everything the observatory needs.
    fn placed() -> (McVecGraph, Vec<NetTopology>) {
        let mut g = build_moddcdc_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);
        (g, topos)
    }

    /// Write the dump next to the build so it can be diffed between milestones:
    ///
    /// ```sh
    /// cargo test -p <crate> equi_audit -- --nocapture
    /// cp target/equi_dump/moddcdc.M0.txt tests/golden/equi/moddcdc.M0.txt
    /// cp target/equi_dump/ldo.M2.txt tests/golden/equi/ldo.M2.txt
    /// ```
    ///
    /// The golden file is **generated, not authored** — commit the first run as
    /// the baseline and diff every later milestone against it.
    fn write_dump(tag: &str, body: &str) {
        let dir = std::path::Path::new("target/equi_dump");
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join(format!("{tag}.txt")), body);
    }

    #[test]
    fn moddcdc_topology_shape() {
        let (g, topos) = placed();

        // 10 nets in, 10 topologies out — nothing may be dropped or split.
        assert_eq!(
            topos.len(),
            10,
            "expected 10 topologies, got {}\n{}",
            topos.len(),
            dump_layout_model(&g, &topos)
        );

        // The five GND nets stay five. If this ever reads 1, something
        // coalesced the ground flags.
        let gnd = topos.iter().filter(|t| t.net_name == "GND").count();
        assert_eq!(gnd, 5, "the five GND nets must stay separate");

        // nids survive the topology build (added at M0 so the five GNDs are
        // distinguishable).
        let mut nids: Vec<i64> = topos.iter().map(|t| t.nid).collect();
        nids.sort();
        assert_eq!(nids, (501..=510).collect::<Vec<_>>());
    }

    /// M2.5 Step 8: A1 must be falsifiable — it counts `RowSource::IslandFallback`,
    /// not "the dependency iteration converged". Force a free net into the
    /// island fallback and A1 must go red; a clean run must stay green.
    #[test]
    fn a1_is_falsifiable() {
        // Green baseline: every free net found a partner.
        let (g, topos) = placed();
        let clean = check_a1_rows(&topos);
        assert!(
            matches!(clean.status, CheckStatus::Pass),
            "A1 must be green when no island fallback"
        );

        // Force 506 (the free net that normally inherits a partner row) to look
        // like an island fallback — A1 must flag it as rows_fallback. (501 is
        // terminal-only now and carries no row, so it cannot exercise the flag.)
        let mut g2 = g;
        let mut topos2 = build_topology(&g2);
        place_by_topology(&mut g2, &mut topos2);
        let idx = topos2.iter().position(|t| t.nid == 506).unwrap();
        topos2[idx].row_source = RowSource::IslandFallback;
        let red = check_a1_rows(&topos2);
        assert!(
            matches!(red.status, CheckStatus::Fail),
            "A1 must flag an island-fallback net"
        );
    }

    /// ★ M0 finding — M2 resolved.
    ///
    /// `select_anchor_deterministic` gives 5 of 10 nets an anchor that is a
    /// two-pin passive. At M2 those split into two classes:
    ///   * 3 terminal-only nets (502/503/504) — single real group + Ground
    ///     glyph, no trunk, never given an IC anchor (there is no IC pin on
    ///     their net, so `passive_anchored → 0` is unreachable);
    ///   * 2 free nets (501/506) — multi-group nets whose anchor is a passive
    ///     placed by another net; their row is inherited from a partner
    ///     (`501←507`, `506←510`), not from where the anchor happened to land.
    #[test]
    fn moddcdc_anchor_baseline() {
        let (g, topos) = placed();
        let view = build_view(&g, &topos);

        assert_eq!(
            view.layer_anchor_name, "lp322dcdc",
            "layer anchor must be the IC"
        );

        // 3 single-group GND nets (502/503/504) are terminal-only: no trunk.
        // (501 is the SHARED ground `GND ~ C1.2 ~ C2.2` — two real groups, so
        // not terminal-only; M12 makes it a COLUMN. 505 is the IC ground rail.)
        let terminal_only = view.nets.iter().filter(|n| n.terminal_only).count();
        assert_eq!(
            terminal_only, 3,
            "M2: 502/503/504 are terminal-only (no trunk), got {terminal_only}:\n{view}"
        );

        // The 2 multi-group free nets (501/506) are NOT "passive-anchored
        // adrift": every one must have inherited a row from a partner
        // (`501←507`, `506←510`) instead of the accidental y of wherever its
        // anchor landed or an island fallback below the IC.
        let free: Vec<&NetView> = view
            .nets
            .iter()
            .filter(|n| !n.terminal_only && !n.is_layer_anchor)
            .collect();
        assert_eq!(
            free.len(),
            2,
            "M2: free nets are 501/506, got {}:\n{view}",
            free.len()
        );
        let free_net_passive_anchored = free
            .iter()
            .filter(|n| !matches!(n.row_source, RowSource::Partner(_)))
            .count();
        assert_eq!(
            free_net_passive_anchored, 0,
            "M2: every free net must inherit a partner row:\n{view}"
        );

        // ★ Ungated ground rule: every Ground net starts South, and only the
        // two ADOPTED ones leave it — 501 rides the EN run as a COLUMN (West,
        // M12.1) and 504 rides the FB run (East). 502/503 stay South and 505
        // is the IC's own ground rail.
        let south = view
            .nets
            .iter()
            .filter(|n| n.region == Region::South)
            .count();
        assert_eq!(
            south, 3,
            "three GND nets must hang South (502/503/505), got {south}:\n{view}"
        );
    }

    #[test]
    fn moddcdc_m0_audit() {
        let (g, topos) = placed();
        let view = build_view(&g, &topos);
        let audit = audit_equi_tree(&g, &topos);

        let body = format!("{view}\n\n{audit}\n");
        write_dump("moddcdc.M0", &body);
        eprintln!("{body}");

        // Enforced through M3_5: A1/A2/A2b/A10-A15, A4 and A17/A18. M4 adds
        // A21/A22/A23 (column model). M5 adds A24/A25/A26. M6 adds A16 (ground
        // count conservation) — all green on moddcdc.
        audit.assert_clean_through(Milestone::M6);
    }

    /// M3 fixture assertions (plan M3 completion criteria).
    ///
    /// * `lp322dcdc` fits within the 200px height target (the M2.5 deviation
    ///   fixed by the M3.2 RowAllocator's `ROW_CLEAR` pitch).
    /// * `RES_1` and `CAP_1..4` are vertical (h > w).
    /// * `IND_1` is vertical too — a documented deviation from the plan's
    ///   `IND_1.w > IND_1.h`: the output inductor spans LX (`__net_3`, row 100)
    ///   down to `VCC_1V2` (row 460), i.e. a `Bridge`; making it horizontal
    ///   would force two same-side East nets onto one row, violating A11.
    #[test]
    fn moddcdc_m3_fixture_assertions() {
        let (g, topos) = placed();
        let _audit = audit_equi_tree(&g, &topos);

        let anchor = g
            .boxes
            .iter()
            .find(|b| b.id == layer_anchor_id(&topos))
            .expect("layer anchor box");
        // M3.5 (R4): `LEAD` 0 → 20 raised the corridor demand (`LEAD + h`), so
        // the IC grew from 200 to 240 — the price of the visible lead-wire
        // segments (the M3.2 `<= 200` win is partially traded back, accepted in
        // the R4 plan for the correct Bridge/Drop geometry).
        //
        // ★ M7.1: the netlist coupling moved FB from West to East, so the IC
        // dropped from 3 West bands to 2 (West VIN/EN, East LX/FB) and the box
        // shrank from 240 to 140. The bound is tightened to lock that in.
        assert!(
            anchor.h <= 160.0,
            "lp322dcdc height {:.0} exceeds the M7.1 bound (160, 2 bands)",
            anchor.h
        );

        let b = |name: &str| {
            g.boxes
                .iter()
                .find(|b| b.name == name)
                .unwrap_or_else(|| panic!("missing box {name}"))
        };
        assert!(
            b("RES_1").h > b("RES_1").w,
            "RES_1 must be vertical (Bridge)"
        );
        // ★ M12.1: CAP_1 and CAP_2 are now horizontal — they are the two arms
        // of the shared ground COLUMN `GND ~ C1.2 ~ C2.2`, each lying along its
        // own row (Vin / EN) and stopping at the column's x.
        for cap in ["CAP_1", "CAP_2"] {
            let c = b(cap);
            assert!(
                c.w > c.h,
                "{cap} must be horizontal (ground column arm), got w={} h={}",
                c.w,
                c.h
            );
        }
        // M10.3: CAP_1 and CAP_2 share the same cold x (the column node).
        assert!(
            (b("CAP_1").x - b("CAP_2").x).abs() < 1.0,
            "CAP_1 x={} and CAP_2 x={} must line up on the ground column",
            b("CAP_1").x,
            b("CAP_2").x
        );
        // CAP_3/CAP_4 keep private grounds → stay vertical Drops off VCC_1V2.
        for cap in ["CAP_3", "CAP_4"] {
            let c = b(cap);
            assert!(c.h > c.w, "{cap} must be vertical, got w={} h={}", c.w, c.h);
        }
        let ind = b("IND_1");
        assert!(
            ind.w > ind.h,
            "IND_1 must be horizontal (Along: LX → VCC_1V2), got w={} h={}",
            ind.w,
            ind.h
        );
        let net3 = topos.iter().find(|t| t.net_name == "__net_3").unwrap();
        let vcc = topos.iter().find(|t| t.net_name == "VCC_1V2").unwrap();
        assert_eq!(net3.run_root, vcc.run_root, "LX and VCC_1V2 are one run");
        assert!(
            (net3.lane.axis - vcc.lane.axis).abs() < 1.0,
            "a run is collinear: {} vs {}",
            net3.lane.axis,
            vcc.lane.axis
        );

        // A4 is green on this fixture (structural: every 2-pin passive got
        // opposite-side slots matching its orientation).
        let a4 = check_a4_passive_orientation(&g);
        assert!(
            matches!(a4.status, CheckStatus::Pass),
            "A4 must be structurally green:\n{}",
            dump_layout_model(&g, &topos)
        );
    }

    /// M3.4: a 2-pin member whose two nets end up on the SAME row is a
    /// `Series` — it must be placed HORIZONTALLY (w > h) with Left/Right pins,
    /// and A4 must stay green. Constructed by two IC nets on opposite sides
    /// (West IN / East OUT) which the M3.2 RowAllocator puts on one band.
    #[test]
    fn series_member_is_horizontal() {
        use super::fixture::{mk_box, net, two_pin};
        use crate::vector::graph::netdef::IoDirection;
        use crate::vector::graph::symbol::Symbol;
        use crate::vector::graph::LayerStyle;

        let mut g = McVecGraph::new(3000, "series".into());
        g.layer_style = LayerStyle::Device;
        // The two-pin passive is id 1, the IC id 2: `select_anchor_deterministic`
        // breaks pin-count ties by larger box id, so the IC anchors both nets and
        // CAP_1 stays a member (the thing we want to classify as Series).
        //
        // ★ M7.1: NET_A and NET_B share CAP_1, so the coupling pass would pull
        // the weaker net (IN) across to the OUT side and CAP_1 would become a
        // Bridge. NET_B carries two anchor pins, so the post-move imbalance
        // stays above `SIDE_IMBALANCE_MAX`, the move is refused, and the two
        // nets stay on opposite sides on the same band — the one path where a
        // genuine Series (and its horizontal orientation) is still reachable.
        // The long pin labels keep the IC wider than its 2-row height.
        g.boxes
            .push(two_pin(1, "CAP_1", "CAP", Symbol::Capacitor, 11, 12));
        g.boxes.push(mk_box(
            2,
            "ic",
            "IC",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (21, "1", "INPUT_A", IoDirection::Input),
                (22, "2", "OUTPUT_B", IoDirection::Output),
                (23, "3", "OUTPUT_C", IoDirection::Output),
            ],
        ));
        g.nets
            .push(net(301, "NET_A", NetKind::Signal, &[(2, 21), (1, 11)]));
        g.nets.push(net(
            302,
            "NET_B",
            NetKind::Signal,
            &[(2, 22), (2, 23), (1, 12)],
        ));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        // The coupled IN net stays West and the OUT net stays East on the same
        // band → the member is a Series.
        let tap_role_of = |box_name: &str| -> String {
            let b = g.boxes.iter().find(|b| b.name == box_name).unwrap();
            let (idx, topo) = topos
                .iter()
                .enumerate()
                .find(|(_, t)| t.groups.iter().skip(1).any(|gr| gr.box_id == b.id))
                .expect("box should be a member of a net");
            let group = topo
                .groups
                .iter()
                .skip(1)
                .find(|gr| gr.box_id == b.id)
                .unwrap();
            super::super::equipotential_tree::tap_role(
                b,
                topo,
                super::super::equipotential_tree::partner_info(&topos, idx, group),
                super::super::equipotential_tree::layer_anchor_id(&topos),
            )
            .short()
            .to_string()
        };
        assert_eq!(tap_role_of("CAP_1"), "Series");

        let cap = g.boxes.iter().find(|b| b.id == 2).unwrap();
        assert!(
            cap.w > cap.h,
            "Series member must be horizontal, got w={} h={}",
            cap.w,
            cap.h
        );
        let a4 = check_a4_passive_orientation(&g);
        assert!(
            matches!(a4.status, CheckStatus::Pass),
            "A4 must be structurally green for a Series member:\n{}",
            dump_layout_model(&g, &topos)
        );
    }

    /// M3.5 (R2): long bottom-edge pin labels must not overlap. A 5-pin South
    /// edge with 7-char labels used to get `box_w = 5*PIN_PITCH + 40 = 240` →
    /// slot spacing `240/6 = 40 < 49` (`SHIELD3`), so the labels overlapped and
    /// A14 (which only checked Left/Right) stayed silent. The box must now
    /// widen to fit the edge labels.
    #[test]
    fn bottom_labels_do_not_overlap() {
        use super::fixture::{mk_box, net};
        use crate::vector::graph::netdef::IoDirection;
        use crate::vector::graph::symbol::Symbol;
        use crate::vector::graph::LayerStyle;

        let mut g = McVecGraph::new(4000, "shield".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_box(
            1,
            "usbsock",
            "USB",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (1, "SHIELD1", "", IoDirection::Ground),
                (2, "SHIELD2", "", IoDirection::Ground),
                (3, "SHIELD3", "", IoDirection::Ground),
                (4, "SHIELD4", "", IoDirection::Ground),
                (5, "SHIELD5", "", IoDirection::Ground),
            ],
        ));
        // Each Ground pin on its own net → five South rails.
        for i in 1..=5 {
            g.nets
                .push(net(500 + i as i64, "GND", NetKind::Ground, &[(1, i)]));
        }

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let a14 = check_a14_label_fit(&g);
        assert!(
            matches!(a14.status, CheckStatus::Pass),
            "A14 must pass with widened bottom slots:\n{}",
            dump_layout_model(&g, &topos)
        );
    }

    /// M3.5 (R3): a multi-pin net's same-side pins must land on ADJACENT row
    /// slots. IC pins 1=A 2=B 3=A (all West) interleave A's pins with B's; the
    /// R3 grouping in `assign_pin_order` makes A's two pins consecutive so the
    /// stray pin's tooth does not span another net's row.
    #[test]
    fn same_net_pins_are_adjacent() {
        use super::fixture::{mk_box, net, two_pin};
        use crate::vector::graph::netdef::IoDirection;
        use crate::vector::graph::symbol::Symbol;
        use crate::vector::graph::LayerStyle;

        let mut g = McVecGraph::new(5000, "adj".into());
        g.layer_style = LayerStyle::Device;
        g.boxes.push(mk_box(
            1,
            "ic",
            "IC",
            BoxKind::MultiPin,
            Symbol::Ic,
            &[
                (1, "1", "A1", IoDirection::Input),
                (2, "2", "B1", IoDirection::Input),
                (3, "3", "A2", IoDirection::Input),
            ],
        ));
        g.boxes
            .push(two_pin(2, "CAP_1", "CAP", Symbol::Capacitor, 21, 22));
        g.nets.push(net(
            301,
            "NET_A",
            NetKind::Signal,
            &[(1, 1), (1, 3), (2, 21)],
        ));
        g.nets
            .push(net(302, "NET_B", NetKind::Signal, &[(1, 2), (2, 22)]));

        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let ic = g.boxes.iter().find(|b| b.id == 1).unwrap();
        let mut west: Vec<(i64, f64)> = ic
            .slots
            .iter()
            .filter(|s| s.side == EntrySide::Left)
            .map(|s| (s.pin_id, slot_point(ic, s).1))
            .collect();
        west.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let p1 = west.iter().position(|(pid, _)| *pid == 1).unwrap();
        let p3 = west.iter().position(|(pid, _)| *pid == 3).unwrap();
        assert!(
            (p1 as i64 - p3 as i64).abs() == 1,
            "NET_A's two West pins must be on adjacent rows, got positions {p1} and {p3} in {:?}",
            west
        );
    }

    /// M2.5 Step 1: freeze the `ldo` regression state as a golden BEFORE any
    /// fix lands, so every later step can diff against it. At this point the
    /// dump is expected to show the broken geometry (two IC pins on one row,
    /// the NC pin on the box middle, labels spilling out).
    #[test]
    fn ldo_audit() {
        let mut g = build_ldo_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);
        let view = build_view(&g, &topos);
        let audit = audit_equi_tree(&g, &topos);

        let body = format!("{view}\n\n{audit}\n");
        write_dump("ldo.M2", &body);
        eprintln!("{body}");

        // M6.3 final acceptance: the whole audit through M6 (A16-A26).
        audit.assert_clean_through(Milestone::M6);
    }

    /// M4.5: the `series_bridge_shunt` fixture is the dedicated witness for the
    /// column model. It combines both reachable two-pin roles — `Drop` decoupling
    /// shunts (`CAP_IN`, `CAP_OUT`) and cross-row `Bridge` members (`IND`, `R_FB`)
    /// — on one picture, and asserts the full M4 audit (A21 no-overlap, A22
    /// in-both-spans, A23 shunt-near-pin, plus every earlier invariant) is clean.
    #[test]
    fn series_bridge_shunt_fixture() {
        let mut g = build_series_bridge_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);
        let audit = audit_equi_tree(&g, &topos);

        // ★ M4.5 final acceptance: the whole audit is now enforced through M4.
        audit.assert_clean_through(Milestone::M4);

        // The bridge members must be >w (vertical) and the shunts must be near
        // their anchor pin — a cross-check that the fixture really puts the
        // column model under load (A22 / A23 are non-vacuous).
        // ★ M8: the output inductor extends SW outward into VOUT, so it lies ALONG
        // the row (w>h); the feedback divider joins VOUT ↔ FB across two rows
        // and stays a vertical Bridge (h>w). Both are load-bearing for the
        // column model (A22 / A23 are non-vacuous).
        let b = |name: &str| {
            g.boxes
                .iter()
                .find(|b| b.name == name)
                .unwrap_or_else(|| panic!("missing box {name}"))
        };
        let ind = b("IND");
        assert!(
            ind.w > ind.h,
            "IND must be Along (w>h, SW run continues into VOUT), got w={} h={}",
            ind.w,
            ind.h
        );
        let rfb = b("R_FB");
        assert!(
            rfb.h > rfb.w,
            "R_FB must be a vertical Bridge (h>w), got w={} h={}",
            rfb.w,
            rfb.h
        );
        // Every member centre drifts at most COL_STEP (20) from its column
        // slot: two members sharing a column would collide. The A21 PASS above
        // is the ground truth; these orientational checks make it legible.
        let shunts = ["CAP_IN", "CAP_OUT"];
        for s in shunts {
            let bb = b(s);
            assert!(
                bb.h > bb.w,
                "{s} must be a vertical Drop shunt (h>w), got w={} h={}",
                bb.w,
                bb.h
            );
        }
    }

    /// ★ M7.6: the second multi-pin component must be drawn like a component.
    ///
    /// Guards the four things that were broken on `speaker`: a real box size, a
    /// pin distribution that keeps the labels apart, ground pins on the edge the
    /// rail is on, and `connected` taken from the topology rather than from the
    /// (empty) `entry_points`.
    #[test]
    fn two_anchor_fixture() {
        let mut g = build_two_anchor_graph();
        let mut topos = build_topology(&g);
        place_by_topology(&mut g, &mut topos);

        let anchor = layer_anchor_id(&topos);
        assert_eq!(anchor, 1, "the amplifier is the layer anchor");

        let spk = g.boxes.iter().find(|b| b.id == 2).expect("spk placed");
        assert!(
            spk.w >= MIN_BOX_W && spk.h >= MIN_SINK_H,
            "spk collapsed to {:.0}x{:.0}",
            spk.w,
            spk.h
        );
        assert_eq!(spk.slots.len(), 4, "every pin gets a slot");
        assert!(
            spk.slots.iter().all(|s| s.connected),
            "all four pins are in a net — none may render as NC"
        );
        // ★ M9: `spk` is a SATELLITE, not a Sink. The two pins it shares with `lpa`
        // (101/102) sit on ONE edge facing `lpa`; spk's own ground pins (103/104)
        // sit on the OPPOSITE edge, away from the connection — NOT crammed into
        // the gap between the two components.
        let side_of = |pid: i64| {
            spk.slots
                .iter()
                .find(|s| s.pin_id == pid)
                .unwrap_or_else(|| panic!("spk pin {pid} has a slot"))
                .side
        };
        let facing_edge = side_of(101);
        assert!(
            matches!(facing_edge, EntrySide::Left | EntrySide::Right),
            "the shared pins must face lpa on a W/E edge, got {facing_edge:?}"
        );
        assert_eq!(side_of(102), facing_edge, "both shared pins share one edge");
        let away_edge = side_of(103);
        assert!(
            away_edge != facing_edge && matches!(away_edge, EntrySide::Left | EntrySide::Right),
            "spk's own ground must sit on the far edge, got facing={facing_edge:?} away={away_edge:?}"
        );
        assert_eq!(side_of(104), away_edge, "ground pins share the far edge");
        // Each shared net is a straight wire: the pin is ON the row.
        for pid in [101, 102] {
            let s = spk.slots.iter().find(|s| s.pin_id == pid).unwrap();
            let t = topos
                .iter()
                .find(|t| {
                    t.groups
                        .iter()
                        .any(|g| g.box_id == 2 && g.pin_ids.contains(&pid))
                })
                .unwrap();
            let py = spk.y + spk.h * s.offset;
            assert!(
                (py - t.lane.axis).abs() < 1.0,
                "pin {pid} at {py:.0} is off its row {:.0}",
                t.lane.axis
            );
        }

        let audit = audit_equi_tree(&g, &topos);
        audit.assert_clean_through(Milestone::M6);
    }

    /// The dump must be a pure function of the placed graph: two projections of
    /// the same state are byte-identical. Without this, golden diffs are noise.
    #[test]
    fn dump_is_deterministic() {
        let (g, topos) = placed();
        assert_eq!(
            dump_layout_model(&g, &topos),
            dump_layout_model(&g, &topos),
            "dump is not deterministic"
        );
    }

    /// Sanity: the observatory is read-only.
    #[test]
    fn audit_does_not_mutate() {
        let (g, topos) = placed();
        let before: Vec<(i64, f64, f64, f64, f64)> =
            g.boxes.iter().map(|b| (b.id, b.x, b.y, b.w, b.h)).collect();

        let _ = audit_equi_tree(&g, &topos);
        let _ = dump_layout_model(&g, &topos);

        let after: Vec<(i64, f64, f64, f64, f64)> =
            g.boxes.iter().map(|b| (b.id, b.x, b.y, b.w, b.h)).collect();
        assert_eq!(before, after, "audit/dump mutated box geometry");
    }

    /// M6.2 — structural regression snapshot (R1–R5). NOT pixel comparison: it
    /// pins the semantic structure of the "good" layout so any future change
    /// has to consciously re-generate this baseline. Adapted from plan/m6.md to
    /// the real graph/topo model (`layout.ic_pins` etc. do not exist).
    #[test]
    fn m6_regression_structure() {
        let (g, topos) = placed();
        let trees: Vec<EquiTree> = realize_all(&topos, &g);

        // R1: every IC pin slot sits inside the IC box bounds (tolerance 1.0).
        let ic = g
            .boxes
            .iter()
            .find(|b| b.id == layer_anchor_id(&topos))
            .expect("layer anchor box");
        for slot in &ic.slots {
            let (px, py) = slot_point(ic, slot);
            assert!(
                px >= ic.x - 1.0 && px <= ic.x + ic.w + 1.0,
                "IC pin {} x={:.1} outside box x [{:.1},{:.1}]",
                slot.pin_id,
                px,
                ic.x,
                ic.x + ic.w
            );
            assert!(
                py >= ic.y - 1.0 && py <= ic.y + ic.h + 1.0,
                "IC pin {} y={:.1} outside box y [{:.1},{:.1}]",
                slot.pin_id,
                py,
                ic.y,
                ic.y + ic.h
            );
        }

        // R2: every member centre is within 3 * MEMBER_GAP of the NEAREST trunk that
        // owns it. A two-pin passive is a member of two nets (the net it hangs
        // from and the one it decouples/bridges to); geometrically it sits at
        // the row of the net that PLACED it, which is one of its owners — so
        // "nearest owner trunk" is the meaningful bound (a cap hanging off the
        // VDD row is 250px from the GND rail by design).
        let tol = 3.0 * 60.0;
        // box_id -> owner trunk ys (non-terminal owners only).
        let mut owner_rows: BTreeMap<i64, Vec<f64>> = BTreeMap::new();
        for topo in &topos {
            if topo.terminal_only {
                continue;
            }
            for group in topo.groups.iter().skip(1) {
                owner_rows
                    .entry(group.box_id)
                    .or_default()
                    .push(topo.lane.axis);
            }
        }
        for (&box_id, rows) in &owner_rows {
            let Some(b) = g.boxes.iter().find(|bb| bb.id == box_id) else {
                continue;
            };
            let cy = b.y + b.h / 2.0;
            let delta = rows
                .iter()
                .map(|&y| (cy - y).abs())
                .fold(f64::MAX, f64::min);
            assert!(
                delta <= tol + 0.5,
                "member '{}' (id={}) is {delta:.0}px (>{tol:.0}) from every owner trunk {:?}",
                b.name,
                b.id,
                rows
            );
        }

        // R3: the IC box does not overlap any member box.
        for topo in &topos {
            for group in topo.groups.iter().skip(1) {
                let Some(b) = g.boxes.iter().find(|bb| bb.id == group.box_id) else {
                    continue;
                };
                let overlap =
                    ic.x < b.x + b.w && ic.x + ic.w > b.x && ic.y < b.y + b.h && ic.y + ic.h > b.y;
                assert!(
                    !overlap,
                    "IC box overlaps member '{}' (id={})",
                    b.name, b.id
                );
            }
        }

        // R4: IC box is not degenerate (labels would clip / pins would stack).
        assert!(ic.w >= 80.0, "IC box width {:.0} too narrow", ic.w);
        assert!(ic.h >= 40.0, "IC box height {:.0} too short", ic.h);

        // R5: the rendered SVG is well-formed — no NaN/Infinity, sane size, and
        // non-degenerate (positive width/height) after fit-to-content.
        let mut svg = String::from("<svg>");
        use crate::viz::render::equipotential_tree_render::render_equi_tree;
        for tree in &trees {
            svg.push_str(&render_equi_tree(tree));
        }
        svg.push_str("</svg>");
        assert!(!svg.contains("NaN"), "NaN in SVG coordinates");
        assert!(!svg.contains("Infinity"), "Infinity in SVG coordinates");
        assert!(!svg.contains("-inf") && !svg.contains("inf"), "inf in SVG");
        // Content extent (boxes + segments) must be a sane schematic size.
        let mut xs: Vec<f64> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for b in &g.boxes {
            xs.push(b.x);
            xs.push(b.x + b.w);
            ys.push(b.y);
            ys.push(b.y + b.h);
        }
        for t in &trees {
            for s in &t.segments {
                xs.push(s.x1);
                xs.push(s.x2);
                ys.push(s.y1);
                ys.push(s.y2);
            }
        }
        let (x0, x1) = (
            xs.iter().cloned().fold(f64::MAX, f64::min),
            xs.iter().cloned().fold(f64::MIN, f64::max),
        );
        let (y0, y1) = (
            ys.iter().cloned().fold(f64::MAX, f64::min),
            ys.iter().cloned().fold(f64::MIN, f64::max),
        );
        let w = x1 - x0;
        let h = y1 - y0;
        assert!(
            w >= 200.0 && w <= 5000.0,
            "layout width {w:.0} out of sane range"
        );
        assert!(
            h >= 200.0 && h <= 5000.0,
            "layout height {h:.0} out of sane range"
        );
    }
}
