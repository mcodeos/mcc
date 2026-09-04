// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Net shape —— sidecar provenance
//!
//! ## Why sidecar instead of restructuring
//!
//! `McVecNet` / `McVec` already have hundreds of call sites, with the whole islands / sp_model /
//! ladder_model stack built on top. **Changing their shape = recompiling the whole library + invalidating every regression.**
//!
//! But only three pieces of information are really lost:
//!   1. **Which lane** this connection is in the source (the `for k in 0..max_w` loop in `visit.rs`)
//!   2. The **arrow direction** in the source (`->` / `<-` / `-` / `+`)
//!   3. **Which two-terminal device** this segment passes through
//!
//! So the approach is: add **one** `Option<NetShape>` field to `McVecNet`;
//! when `None`, all legacy code behaves bit-for-bit identically; when set, downstream can stop reverse-engineering heuristics.
//!
//! ```text
//! Surface of change:
//!   McVec           0 fields        ← untouched
//!   McVecNet        +1 Option       ← legacy constructors keep their signatures
//!   ConnPair        +3 fields       ← only 4 construction sites (visit.rs)
//!   Downstream      0 required changes ← use it if you want; otherwise it's as if it doesn't exist
//! ```
//!
//! ## Relation to `connection_type()`
//!
//! `McVecNet::connection_type()` reverse-engineers the shape from the **merged point pairs**;
//! it is a byproduct of net merging and misclassifies equipotential points as buses
//! (the comment in `fromblock.rs::is_real_bus` already acknowledges this).
//!
//! `NetShape` is the shape **written in the source**; the two have different origins.
//! Migration path: downstream reads `shape` first, and falls back to `connection_type()` when it is `None`.
//! Once `shape` coverage stabilizes at 95%+, mark `connection_type()` with `#[deprecated]`.

use std::fmt;

use crate::semantic::common::{ConnDir, ConnOp};

use super::trunk::{TrunkCtx, TrunkKind};

// ============================================================================
// ConnDir —— arrow direction in the source
// ============================================================================
//
// `ConnDir` (semantic/common.rs) is the single arrow-direction type. It was
// unified with the former vector-layer `ConnDir` (vec-dianlu.md §8.9.7-F);
// the two enums were structurally identical and only linked by
// `conn_dir_to_pair_dir`.
// - `->` directed series -> [`ConnDir::LtoR`]
// - `<-` reversed -> [`ConnDir::RtoL`] — a first-class mirror of `LtoR`:
//   the parser swaps operands so member/point order is source-first in both
//   directions; `NetShape::ltr_view`/`driver_load` recover the LTR render view.
// - `-` series / `+` parallel -> [`ConnDir::Undirected`]

// ============================================================================
// LaneRef —— which lane of the vector
// ============================================================================

/// Which lane of the vector a connection belongs to.
///
/// Source: the `k` and `member_name_opt` from the `for k in 0..max_w` loop in `visit.rs`.
/// Both values are **complete** in that loop; they are now flattened by `ConnPair`
/// and later guessed back by `connection.rs::build_star_topology` using frequency statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneRef {
    /// Which lane, starting from 0
    pub index: u16,
    /// Member name of this lane (`"P"` / `"VDD_3V3"` / `"SCLK"`); None when unavailable
    pub name: Option<String>,
}

impl LaneRef {
    pub fn new(index: u16, name: Option<String>) -> Self {
        Self { index, name }
    }
}

impl fmt::Display for LaneRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.name {
            Some(n) => write!(f, "[{}:{}]", self.index, n),
            None => write!(f, "[{}]", self.index),
        }
    }
}

// ============================================================================
// GroupRole —— what role a group of endpoints plays in the source
// ============================================================================

/// Role of each group in `McVecNet.nets`.
///
/// Note the difference from `ConnectionType`: `ConnectionType` is the **inferred** topology,
/// while this is the role **written in the source**.
///
/// `BusLanes` was removed in the §8.9.2-4 cleanup: production `build_net_shape`
/// only ever produced `Scalar`/`Broadcast` from the vec length, so the variant
/// was never created. Bus/interface identity and per-member pin2pin now live in
/// the coarse `Trunk` layer (vec-dianlu.md §8.9.4), not in the flat groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupRole {
    /// Single point: `GND`, `R1.1`
    Scalar,
    /// One-to-many fan: 1 source point reaching N points, or an N-member bus
    /// group (`MIC{P,N}`, `[VDD_3V3, GND]`) — width is N. **Post-hoc drawing
    /// role**: in the current model a scalar-vs-N fan is only ever the legal
    /// group/same-name-pad/DC-bus fan-out (vec-dianlu §5.3.2 / §7.3); the
    /// §5.3.1 single-point broadcast is abolished and never produces a net.
    Broadcast(usize),
}

impl GroupRole {
    pub fn width(self) -> usize {
        match self {
            GroupRole::Scalar => 1,
            GroupRole::Broadcast(n) => n,
        }
    }
}

impl fmt::Display for GroupRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GroupRole::Scalar => write!(f, "1"),
            GroupRole::Broadcast(n) => write!(f, "1→{n}"),
        }
    }
}

// ============================================================================
// NetShape —— the one Option hanging on McVecNet
// ============================================================================

/// The shape of a net **as written in the source**.
///
/// All fields are filled by `visit.rs` at the moment the `ConnPair` is built,
/// with no inference. Fields that can't be obtained are left empty, and the coverage
/// in the logs speaks for itself —— **never backfill with heuristics**, or it degenerates
/// into the current state: three layers of guesses fighting each other.
#[derive(Debug, Clone, Default)]
pub struct NetShape {
    /// Role of each group, in one-to-one order with `McVecNet.nets`
    pub groups: Vec<GroupRole>,

    /// Overall direction of the whole line (majority when segments on the same line disagree)
    pub dir: ConnDir,

    /// The lane this net belongs to (which lane of a bus); None for scalar nets
    pub lane: Option<LaneRef>,

    /// Two-terminal devices this net passes through in the source (**order is topological order**)
    ///
    /// Purpose:
    /// - The `M4` cut-set forced rule "passive device in the belt -> always Wire" reads this directly,
    ///   no longer reverse-engineering with the `rails.rs` `touches_passive` heuristic
    /// - `M3` quotient graph decides whether an edge is an SP belt or a direct belt
    pub series_chain: Vec<i64>,

    /// Connection operator that produced this net (`Series` for `-`/`->`/`<-`,
    /// `Parallel` for `+`); `None` when unknown. Copied from `ConnPair.op`,
    /// which is itself copied from `ConnectionInst.op` in visit.rs. Downstream
    /// can tell a series net from a parallel one without re-deriving it from
    /// the merged point pairs.
    pub op: Option<ConnOp>,

    /// Left-alignment anchor of a parallel connection: the first ordered
    /// endpoint, i.e. the left main operand (lopd[0]). `None` for series nets.
    /// Filled in `build_net_shape` via the shared `parallel_anchor` rule
    /// (vec-dianlu.md §8.9.4 step 4), so Pass1's `representative()`
    /// (`Parallel -> lhs`) and the vector layer can never drift.
    pub anchor: Option<i64>,

    /// Combination order: the endpoint sequence of the net in source order
    /// (left-to-right along the phrase chain, deduplicated). For a parallel
    /// `A + B + C` this is the left-aligned merge order of the members; for a
    /// series chain `P0 -> P1 -> P2` it is `[P0, P1, P2]`.
    pub order: Vec<i64>,
}

impl NetShape {
    /// Whether this net is one lane of a bus
    pub fn is_bus_lane(&self) -> bool {
        self.lane.is_some()
    }

    /// Bus width (widest of all groups); returns 1 for scalars
    pub fn vec_netshape_bus_width(&self) -> usize {
        self.groups.iter().map(|g| g.width()).max().unwrap_or(1)
    }

    /// Whether this net passes through a passive device (M4's forced-wire criterion)
    pub fn has_series_passive(&self) -> bool {
        !self.series_chain.is_empty()
    }

    /// Whether it carries any real information —— a fully empty shape is equivalent to None; don't store it.
    /// `order` is a derived projection of the pairs (any net has endpoints), so
    /// it does not participate; only the source-written `op` does.
    pub fn is_informative(&self) -> bool {
        !self.groups.is_empty()
            || self.dir.is_directed()
            || self.lane.is_some()
            || !self.series_chain.is_empty()
            || self.op.is_some()
    }

    /// Driver (source) and load (sink) endpoints of a **directed** net.
    ///
    /// `order` is source-first (the parser swapped `<-` operands and ConnPair
    /// points are source-first in both directions), so the driver is always
    /// `order[0]` and the load `order.last()`. Returns `None` for undirected
    /// nets or chains with a single endpoint.
    pub fn driver_load(&self) -> Option<(i64, i64)> {
        if self.dir.is_directed() && self.order.len() >= 2 {
            Some((self.order[0], *self.order.last().unwrap()))
        } else {
            None
        }
    }

    /// Left-to-right **render** view of a directed net: `(leftmost, rightmost,
    /// arrow-as-drawn)`.
    ///
    /// - `LtoR` already draws driver→load → `(driver, load, LtoR)`.
    /// - `RtoL` is flipped to the LTR orientation → `(load, driver, LtoR)`; the
    ///   operand/pair swap is exactly the case `ConnDir::flipped()` documents
    ///   ("reverse direction (used when swapping a pair's left/right)").
    /// - `Undirected` → `None`.
    pub fn ltr_view(&self) -> Option<(i64, i64, ConnDir)> {
        let (driver, load) = self.driver_load()?;
        match self.dir {
            ConnDir::LtoR => Some((driver, load, ConnDir::LtoR)),
            // The pair-swap mirror: re-reading an RtoL pair left-to-right is
            // exactly `ConnDir::flipped()`'s documented use.
            ConnDir::RtoL => Some((load, driver, ConnDir::RtoL.flipped())),
            ConnDir::Undirected => None,
        }
    }
}

impl fmt::Display for NetShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let g: Vec<String> = self.groups.iter().map(|x| x.to_string()).collect();
        write!(f, "{} {}", g.join(" "), self.dir)?;
        if let Some(op) = &self.op {
            let sym = match op {
                ConnOp::Series => "-",
                ConnOp::Parallel => "+",
            };
            write!(f, " {sym}")?;
        }
        if let Some(l) = &self.lane {
            write!(f, " lane{l}")?;
        }
        if !self.series_chain.is_empty() {
            write!(f, " via{:?}", self.series_chain)?;
        }
        if !self.order.is_empty() {
            write!(f, " order{:?}", self.order)?;
        }
        Ok(())
    }
}

// ============================================================================
// Fix suggestions —— P5.4
// ============================================================================

/// Generate a fix suggestion for a vector shape mismatch (eval.md §3 / §7).
///
/// Enriches diagnostics on the E4007 / E2904 path. A row-count mismatch is an
/// **error with no connection generated** (vec-dianlu §5.3.3: illegal ⇒ E4007,
/// no truncation / pair-by-min recovery) — the shape gate never "recovers by
/// truncation" anymore. This suggestion only guides the author toward a legal
/// equal-row form (write an explicit N-row list / group, or align widths with
/// `*` / `_`). Returns `None` when the row counts already agree (no mismatch
/// to fix).
pub fn suggest_shape_fix(lhs_rows: usize, rhs_rows: usize) -> Option<String> {
    if lhs_rows == rhs_rows {
        return None;
    }
    match (lhs_rows, rhs_rows) {
        (1, n) => Some(format!(
            "the scalar would have to reach {n} members; write it as an explicit equal-width \
             list (vec-dianlu §7.3), e.g. [GND, GND]"
        )),
        (n, 1) => Some(format!(
            "the scalar would have to reach {n} members; write it as an explicit equal-width \
             list (vec-dianlu §7.3), e.g. [GND, GND]"
        )),
        (l, r) => Some(format!(
            "row counts differ ({l}x1 vs {r}x1); use an explicit `*` expansion list \
             (eval.md §7 rule 3) or `_` placeholders to align the widths"
        )),
    }
}

// ============================================================================
// Coverage statistics —— the only acceptance metric for this change
// ============================================================================

/// Fill coverage of `shape`.
///
/// **The criterion for "done" is not "code written", but `from_source` share ≥ 90% in this table.**
/// Low coverage means some paths still take the old inference branch; those paths are the next batch to fix.
///
/// ★ v4: `coverage()` = `from_source / total_nets` (nets with a shape / total nets),
/// not `from_source / (from_source + inferred)` (that is always 100%).
#[derive(Debug, Default, Clone)]
pub struct ShapeStats {
    pub total: usize,
    pub total_nets: usize,
    pub from_source: usize,
    pub inferred: usize,
    pub dir_ltr: usize,
    pub dir_rtl: usize,
    pub dir_undirected: usize,
    pub bus_nets: usize,
    pub max_bus_width: usize,
    /// Names of nets without a shape, used to locate the next path to fix
    pub uncovered: Vec<String>,
}

impl ShapeStats {
    /// Observe one net's shape and its AST-layer group context.
    ///
    /// §8.9.6.7: the structured group kind (`trunk.kind != Plain`) is the
    /// authority for bus classification; the width heuristic
    /// (`is_bus_lane()` / `vec_netshape_bus_width() >= 2`) only applies when no group
    /// context is available. A net without a shape but with a bus group
    /// identity is still counted as a bus (not left "uncovered").
    pub fn observe(&mut self, name: &str, shape: Option<&NetShape>, group: Option<&TrunkCtx>) {
        self.total += 1;
        match shape {
            Some(s) => {
                self.from_source += 1;
                match s.dir {
                    ConnDir::LtoR => self.dir_ltr += 1,
                    ConnDir::RtoL => self.dir_rtl += 1,
                    ConnDir::Undirected => self.dir_undirected += 1,
                }
                let is_bus = match group {
                    Some(g) => g.kind != TrunkKind::Plain,
                    None => s.is_bus_lane() || s.vec_netshape_bus_width() >= 2,
                };
                if is_bus {
                    self.bus_nets += 1;
                    self.max_bus_width = self.max_bus_width.max(s.vec_netshape_bus_width());
                }
            }
            None => {
                self.inferred += 1;
                // No shape, but the group identity already says bus — classify
                // it as a bus instead of leaving it uncovered.
                if let Some(g) = group {
                    if g.kind != TrunkKind::Plain {
                        self.bus_nets += 1;
                        return;
                    }
                }
                if self.uncovered.len() < 32 {
                    self.uncovered.push(name.to_string());
                }
            }
        }
    }

    pub fn coverage(&self) -> f64 {
        if self.total_nets == 0 {
            return 1.0;
        }
        self.from_source as f64 / self.total_nets as f64
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "[vec] SHAPE: from_source={} inferred={} (coverage {:.0}% = {}/{})\n",
            self.from_source,
            self.inferred,
            self.coverage() * 100.0,
            self.from_source,
            self.total_nets
        ));
        s.push_str(&format!(
            "[vec] DIR:   ltr={} rtl={} undirected={}\n",
            self.dir_ltr, self.dir_rtl, self.dir_undirected
        ));
        s.push_str(&format!(
            "[vec] LANES: bus nets={} max_width={}\n",
            self.bus_nets, self.max_bus_width
        ));
        if !self.uncovered.is_empty() {
            s.push_str(&format!("[vec] UNCOVERED: {}\n", self.uncovered.join(" ")));
        }
        s
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec_netshape__dir_flip() {
        assert_eq!(ConnDir::LtoR.flipped(), ConnDir::RtoL);
        assert_eq!(ConnDir::Undirected.flipped(), ConnDir::Undirected);
        assert!(ConnDir::LtoR.is_directed());
        assert!(!ConnDir::Undirected.is_directed());
    }

    #[test]
    fn vec_netshape__empty_shape_is_not_informative() {
        // A fully empty shape should be stored as None; don't create an intermediate state of "has a shape but no information"
        assert!(!NetShape::default().is_informative());
        let s = NetShape {
            dir: ConnDir::LtoR,
            ..Default::default()
        };
        assert!(s.is_informative());
    }

    #[test]
    fn vec_netshape_bus_width() {
        let s = NetShape {
            groups: vec![GroupRole::Broadcast(2), GroupRole::Broadcast(2)],
            ..Default::default()
        };
        assert_eq!(s.vec_netshape_bus_width(), 2);

        let scalar = NetShape {
            groups: vec![GroupRole::Scalar, GroupRole::Scalar],
            ..Default::default()
        };
        assert_eq!(scalar.vec_netshape_bus_width(), 1);
    }

    #[test]
    fn vec_netshape__directed_accessors_ltr() {
        // LtoR: driver_load stays (order[0], order.last()); ltr_view unchanged.
        let s = NetShape {
            dir: ConnDir::LtoR,
            order: vec![3, 1],
            ..Default::default()
        };
        assert_eq!(s.driver_load(), Some((3, 1)));
        assert_eq!(s.ltr_view(), Some((3, 1, ConnDir::LtoR)));
    }

    #[test]
    fn vec_netshape__directed_accessors_rtl() {
        // RtoL: order is still source-first, so driver = order[0]; ltr_view
        // flips to the pair-swap mirror `(load, driver, LtoR)` (the case
        // `ConnDir::flipped()` documents).
        let s = NetShape {
            dir: ConnDir::RtoL,
            order: vec![9, 2],
            ..Default::default()
        };
        assert_eq!(s.driver_load(), Some((9, 2)));
        assert_eq!(s.ltr_view(), Some((2, 9, ConnDir::LtoR)));
    }

    #[test]
    fn vec_netshape__directed_accessors_undirected_none() {
        // Undirected has no driver/load; single-endpoint and empty orders are None too.
        let u = NetShape {
            dir: ConnDir::Undirected,
            order: vec![1, 3, 5],
            ..Default::default()
        };
        assert_eq!(u.driver_load(), None);
        assert_eq!(u.ltr_view(), None);

        let single = NetShape {
            dir: ConnDir::LtoR,
            order: vec![4],
            ..Default::default()
        };
        assert_eq!(single.driver_load(), None);
        assert_eq!(single.ltr_view(), None);

        assert_eq!(NetShape::default().driver_load(), None);
        assert_eq!(NetShape::default().ltr_view(), None);
    }

    #[test]
    fn vec_netshape__stats_coverage() {
        let mut st = ShapeStats::default();
        let s = NetShape {
            dir: ConnDir::LtoR,
            ..Default::default()
        };
        st.total_nets = 2;
        st.observe("a", Some(&s), None);
        st.observe("b", None, None);
        assert!((st.coverage() - 0.5).abs() < 1e-9);
        assert_eq!(st.uncovered, vec!["b".to_string()]);
    }

    // ── P5.4: shape fix suggestions ──

    #[test]
    fn vec_netshape__suggest_fix_none_when_counts_agree() {
        assert_eq!(suggest_shape_fix(2, 2), None);
        assert_eq!(suggest_shape_fix(1, 1), None);
    }

    #[test]
    fn vec_netshape__suggest_fix_expand_scalar_to_vector() {
        let s = suggest_shape_fix(1, 3).expect("scalar vs vector should suggest");
        assert!(s.contains("[GND, GND]"), "got: {s}");
        let s = suggest_shape_fix(4, 1).expect("vector vs scalar should suggest");
        assert!(s.contains("[GND, GND]"), "got: {s}");
    }

    #[test]
    fn vec_netshape__suggest_fix_explicit_star_for_named_vectors() {
        let s = suggest_shape_fix(3, 2).expect("N vs M should suggest");
        assert!(
            s.contains("`*`"),
            "explicit `*` expansion hint expected; got: {s}"
        );
    }
}
