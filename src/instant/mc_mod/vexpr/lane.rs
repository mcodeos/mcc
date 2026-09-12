// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Lane production for the unified connection core (unified-core §7.7 S4b).
//!
//! A chain whose members contain a `_` lead or a standalone `'` transpose is
//! wired **lane by lane**, one connection per lane, each tagged `Some(lane)`.
//! That tag is not cosmetic: `ConnectionInst.lane` is the sole input to the
//! vector layer's `NetShape.lane` / `MemberLane.lane` / `TrunkRef.lane`, and
//! the plain adjacent leg always passes `None` — so an evaluator that
//! reproduces only the adjacent path silently collapses the lane layer
//! (unified-core §4.6 C-4).
//!
//! This module owns the lane **loop** — the part the design says the engine
//! owns ("the truth is the loop counter of `for lane in 0..num_lanes`").
//! Everything structural
//! is **reused** from production, never copied:
//!
//! | need | reused from |
//! |---|---|
//! | how many lanes a member spans | `member_lane_width` |
//! | which items a lane holds, in order | `collect_lane_items` / `LaneItem` |
//! | the lane's point within a face | `pick_lane_point` |
//! | the transposed member's lane pin | `collect_lane_items` (`get_transposed_lane_pin`) |
//! | connections | `create_connection` / `make_conn_with_provenance` / `add_connection` |
//! | width legality | `check_series_rows` (shared `opcheck` rule) |
//!
//! Only the loop, the leading / gap / trailing bridge placement and the
//! edge-direction clamping are written here.
//!
//! # Trunk context
//!
//! Trunk context is **not** established here. Production's caller
//! ([`super::super::stmt::process_series_members`]) wraps the whole lane chain
//! in `with_trunk(...)` (§8.9.6.7) so bus lanes carry their trunk identity;
//! the wiring below runs inside that wrapper, exactly as the former inline
//! lane loop did. Trunk is orthogonal to the net list — it only aggregates
//! identity after `create_connection` (§7.1).

use super::eval::LaneOutcome;
use super::ConcreteOpd;
use crate::instant::mc_mod::builder::InstantiationBuilder;
use crate::instant::mc_mod::stmt::LaneItem;
use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{ConnDir, Shape};
use crate::semantic::opcheck::{check_series_rows, OpCheck};

impl InstantiationBuilder {
    /// §7.7 S4b: wire a lane chain, one lane at a time.
    ///
    /// Returns one [`LaneOutcome`] per lane, in lane order; each carries the
    /// lane's external faces and the connections that lane emitted. The
    /// connections are already emitted into the builder — the return value is
    /// for callers that need the per-lane faces.
    pub(in crate::instant::mc_mod) fn vexpr_lane_chain(
        &mut self,
        members: &[McPhrase],
        gaps: &[ConnDir],
    ) -> Result<Vec<LaneOutcome>, InstError> {
        debug_assert_eq!(gaps.len(), members.len().saturating_sub(1));
        let num_lanes = members
            .iter()
            .map(|m| self.member_lane_width(m))
            .max()
            .unwrap_or(0);
        if num_lanes == 0 {
            return Ok(Vec::new());
        }

        // A Transposed member's inner FuncCall is instantiated by the normal
        // member pre-pass, which lane wiring skips; without it the transposed
        // lane pins resolve to nothing. Reuse production's pre-pass, and report
        // a failure the same way the former inline lane loop did.
        for member in members {
            if matches!(member, McPhrase::Transposed(_)) {
                if let Err(e) = self.process_member_internal(member) {
                    self.record_warning(
                        crate::errcodes::INST_LANE_TRANSPOSED_FAILED,
                        crate::errcodes::format_msg(
                            crate::errcodes::INST_LANE_TRANSPOSED_FAILED,
                            &[&e],
                        ),
                    );
                }
            }
        }

        // Strict §5.2 transpose-bridge legality: a transposed operand is first
        // transposed to its full-width column (`get_left_points` /
        // `get_right_points` already merge the inner faces), then the shared row
        // check runs on adjacent pairs. A mismatch reports E4007 and wires
        // nothing — the whole chain is withheld.
        if members.iter().any(|m| Self::phrase_contains_transposed(m)) {
            for i in 0..members.len().saturating_sub(1) {
                let lpts = self.get_right_points(&members[i])?;
                let rpts = self.get_left_points(&members[i + 1])?;
                if lpts.is_empty() || rpts.is_empty() {
                    continue;
                }
                let verdict = check_series_rows(Shape::vvec(lpts.len()), Shape::vvec(rpts.len()));
                if !matches!(verdict, OpCheck::Legal(_)) {
                    self.record_error(
                        crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                        crate::errcodes::format_msg(
                            crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    return Ok(Vec::new());
                }
            }
        }

        let mut out = Vec::new();
        for lane in 0..num_lanes {
            out.push(self.vexpr_one_lane(members, gaps, lane)?);
        }
        Ok(out)
    }

    /// Wire one lane: its series elements in order, with each bridge's pins
    /// attached at the position the chain writes them.
    fn vexpr_one_lane(
        &mut self,
        members: &[McPhrase],
        gaps: &[ConnDir],
        lane: usize,
    ) -> Result<LaneOutcome, InstError> {
        let start = self.connections.len();

        // Split the lane's items into series elements and the bridge pins
        // pending before each of them; `origins[i]` is the member index that
        // produced `series[i]`, which is what maps an element gap back to its
        // member-boundary direction (a member may be skipped on a lane, so the
        // element index is not the member index).
        let items = self.collect_lane_items(members, lane);
        let mut series: Vec<&McPhrase> = Vec::new();
        let mut origins: Vec<usize> = Vec::new();
        let mut bridges_at: Vec<Vec<NetPoint>> = Vec::new();
        let mut lead_before: Vec<bool> = Vec::new();
        let mut pending: Vec<NetPoint> = Vec::new();
        let mut pending_lead = false;
        for (origin, item) in items {
            match item {
                LaneItem::Series(elem) => {
                    series.push(elem);
                    origins.push(origin);
                    bridges_at.push(std::mem::take(&mut pending));
                    lead_before.push(std::mem::replace(&mut pending_lead, false));
                }
                LaneItem::Bridge(pin) => pending.push(pin),
                LaneItem::Lead => pending_lead = true,
            }
        }

        // Uniform chains reproduce the pre-fix single `dir` exactly; interior
        // element gaps use the operator leaving `origins[i]`; chain-head /
        // chain-tail artifact nets use the nearest real boundary.
        let lane_gap_dir = |g: usize| -> ConnDir {
            if gaps.is_empty() {
                ConnDir::Undirected
            } else {
                gaps[g.min(gaps.len() - 1)]
            }
        };

        // FuncCall elements are instantiated by the normal member loop that lane
        // wiring skips, so do it here before resolving their points; a failure
        // is reported the same way the former inline lane loop did.
        for elem in &series {
            if matches!(elem, McPhrase::FuncCall(_)) {
                if let Err(e) = self.process_member_internal(elem) {
                    self.record_warning(
                        crate::errcodes::INST_LANE_FUNCCALL_FAILED,
                        crate::errcodes::format_msg(
                            crate::errcodes::INST_LANE_FUNCCALL_FAILED,
                            &[&e],
                        ),
                    );
                }
            }
        }

        let lane_id = Some(lane as u16);
        let n = series.len();

        // §11 strict vector order: a chain-head bridge (`CAP' -> A`) is written
        // before the first element, so its pins precede that element's left
        // points.
        if let (Some(leading), Some(first)) = (bridges_at.first(), series.first()) {
            if !leading.is_empty() {
                let first_left = self.get_left_points(first).unwrap_or_default();
                if let Some(lp) = Self::pick_lane_point(&first_left, lane) {
                    let mut points = leading.clone();
                    points.push(lp);
                    if points.len() >= 2 {
                        let id = self.next_conn_id();
                        let dir = lane_gap_dir(origins[0].saturating_sub(1));
                        let conn = self.make_conn_with_provenance(id, points, dir, lane_id);
                        self.add_connection(conn);
                    }
                }
            }
        }

        // The element gaps. Bridge pins collected before element `i + 1` belong
        // between `series[i]` and `series[i + 1]`, in the gap.
        for i in 0..n.saturating_sub(1) {
            let left_pts = self.get_right_points(series[i]).unwrap_or_default();
            let right_pts = self.get_left_points(series[i + 1]).unwrap_or_default();
            if left_pts.is_empty() || right_pts.is_empty() {
                continue;
            }
            let Some(bridge_pins) = bridges_at.get(i + 1) else {
                continue;
            };
            let (Some(lp), Some(rp)) = (
                Self::pick_lane_point(&left_pts, lane),
                Self::pick_lane_point(&right_pts, lane),
            ) else {
                continue;
            };
            let dir = lane_gap_dir(origins[i]);
            if lead_before.get(i + 1).copied().unwrap_or(false) {
                self.warn_lead_crossnet(&lp, &rp);
            }
            if bridge_pins.is_empty() {
                self.create_connection(vec![lp], vec![rp], dir, lane_id)?;
            } else {
                let mut points = vec![lp];
                points.extend(bridge_pins.iter().cloned());
                points.push(rp);
                let id = self.next_conn_id();
                let conn = self.make_conn_with_provenance(id, points, dir, lane_id);
                self.add_connection(conn);
            }
        }

        // §11 strict vector order: a chain-tail bridge (`A -> CAP'`) is written
        // after the last element, so its pins follow that element's right
        // points — not into the last gap.
        if !pending.is_empty() {
            if let Some(last) = series.last() {
                let last_right = self.get_right_points(last).unwrap_or_default();
                if let Some(rp) = Self::pick_lane_point(&last_right, lane) {
                    let mut points = vec![rp];
                    points.extend(pending.iter().cloned());
                    if points.len() >= 2 {
                        let id = self.next_conn_id();
                        let dir = origins
                            .last()
                            .map_or(ConnDir::Undirected, |&o| lane_gap_dir(o));
                        let conn = self.make_conn_with_provenance(id, points, dir, lane_id);
                        self.add_connection(conn);
                    }
                }
            }
        }

        // The lane's own external faces: its first element's left point and its
        // last element's right point. A lane with no element has no faces.
        let left = self.lane_face(series.first().copied(), lane, true);
        let right = self.lane_face(series.last().copied(), lane, false);
        let opd = ConcreteOpd {
            lane: lane_id,
            ..ConcreteOpd::from_sides(left, right)
        };
        Ok(LaneOutcome {
            opd,
            emitted: start..self.connections.len(),
        })
    }

    /// §5.4 lead-body check: the lane just wired `lp` to `rp` through a `_`
    /// whose placeholder the lane loop dropped. A lead is an **ideal wire** (a
    /// body, vec-dianlu §5.4) — its two ends are meant to be one net, which is
    /// what makes a lane's left and right ends one member. Two **different
    /// bare nets** under one lead means the wire shorts them at zero
    /// impedance: warn, never error — the author may have written the jumper
    /// on purpose.
    ///
    /// Only bare nets (no `owner`) are compared. A pin (`R101.1`) or a
    /// sub-module port (`sub1.CLK`) carries an owner, and a lead between two
    /// of those is ordinary wiring, not a named-net merge; comparing path
    /// strings across identities would invent a short that no net has.
    fn warn_lead_crossnet(&mut self, lp: &NetPoint, rp: &NetPoint) {
        if lp.owner.is_some() || rp.owner.is_some() {
            return;
        }
        if lp.is_lead_placeholder() || rp.is_lead_placeholder() {
            return;
        }
        if crate::instant::mc_net::canonicalize_path(&lp.path)
            == crate::instant::mc_net::canonicalize_path(&rp.path)
        {
            return;
        }
        self.record_warning(
            crate::errcodes::CONN_LEAD_CROSSNET,
            crate::errcodes::format_msg(crate::errcodes::CONN_LEAD_CROSSNET, &[&lp.path, &rp.path]),
        );
    }

    /// One lane's face point as a single-row face list (empty when the lane has
    /// no element there).
    fn lane_face(&mut self, elem: Option<&McPhrase>, lane: usize, left: bool) -> Vec<NetPoint> {
        let Some(elem) = elem else {
            return Vec::new();
        };
        let points = if left {
            self.get_left_points(elem)
        } else {
            self.get_right_points(elem)
        }
        .unwrap_or_default();
        Self::pick_lane_point(&points, lane)
            .map(|p| vec![p])
            .unwrap_or_default()
    }
}
