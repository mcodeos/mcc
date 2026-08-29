// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Group / Transposed processing + connection generation
//!
//! - `get_group_branch_count` / `check_group_broadcast`
//! - `connect_to_group`             —— Connection strategy between Group and external points
//! - `create_connection`            —— Generic N×M connection generation (1:1 / 1:N / N:1 / truncation)

use super::expand::expand_match;
use super::McModuleInst;
use crate::db::diagnostic::diagnostic::{diagnostic_log, DiagnosticLevel};
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{ConnDir, ConnOp};
use crate::vector::model::trunk::{TrunkCtx, TrunkKind};

/// D5 BUS_ORDER_MISMATCH: process-level count of mismatched bus bits.
/// When all pairs in a bus connection have mismatched member names, D5 fires and
/// sets this to the bus width. The metrics module uses this to compute
/// `bus_bits_paired_ok = bus_bits_total - BUS_BITS_MISMATCHED`.
pub(crate) static BUS_BITS_MISMATCHED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

impl McModuleInst {
    // ========================================================================
    // Group processing (Iteration 6)
    // ========================================================================

    /// Get the branch count of a Group
    fn get_group_branch_count(member: &McPhrase) -> usize {
        match member {
            McPhrase::Group(ref g) => g.opds.len(),
            _ => 1,
        }
    }

    /// Check whether a Group can safely perform broadcast connections
    ///
    /// Returns (left_broadcastable, right_broadcastable)
    fn check_group_broadcast(member: &McPhrase) -> (bool, bool) {
        match member {
            McPhrase::Group(ref g) => (g.left_match, g.right_match),
            _ => (true, true),
        }
    }

    /// Handle connections between a Group and external elements
    ///
    /// Scenario examples:
    /// - `VCC -> (a, b, c)`: broadcast VCC to each branch's left port
    /// - `(a, b, c) -> GND`: all branches' right ports connect to GND
    /// - `[x, y, z] -> (a, b, c)`: per-branch corresponding connection (requires matching count)
    pub(super) fn connect_to_group(
        &mut self,
        external_points: Vec<NetPoint>,
        group_member: &McPhrase,
        external_is_left: bool, // true: external -> group, false: group -> external
        dir: ConnDir,
    ) -> Result<(), InstError> {
        let (left_match, right_match) = Self::check_group_broadcast(group_member);

        let group_points = if external_is_left {
            // external -> group: get group's left endpoints
            self.get_left_points(group_member)?
        } else {
            // group -> external: get group's right endpoints
            self.get_right_points(group_member)?
        };

        let external_size = external_points.len();
        let group_size = group_points.len();
        let branch_count = Self::get_group_branch_count(group_member);

        // Check whether connection can be made
        if external_size == 1 {
            // Single point broadcasts to all branches
            self.create_connection(external_points, group_points, dir, None)?;
        } else if external_size == branch_count {
            // External point count equals branch count, per-branch connection
            // This needs special handling: each external point connects to its corresponding branch
            if external_is_left {
                let shape_ok = left_match;
                if !shape_ok {
                    mcc_dbg!(
                        "inst::mod",
                        "Warning: Group left shapes inconsistent, connection may be incorrect"
                    );
                }
            } else {
                let shape_ok = right_match;
                if !shape_ok {
                    mcc_dbg!(
                        "inst::mod",
                        "Warning: Group right shapes inconsistent, connection may be incorrect"
                    );
                }
            }
            self.create_connection(external_points, group_points, dir, None)?;
        } else if external_size == group_size {
            // Point counts match exactly, connect one-to-one
            self.create_connection(external_points, group_points, dir, None)?;
        } else {
            // ★ Degraded to warning: connect as much as possible, truncate by min
            self.record_warning(
                crate::errcodes::CONN_GROUP_SHAPE_MISMATCH,
                crate::errcodes::format_msg(
                    crate::errcodes::CONN_GROUP_SHAPE_MISMATCH,
                    &[
                        &external_size as &dyn std::fmt::Display,
                        &group_size as &dyn std::fmt::Display,
                        &branch_count as &dyn std::fmt::Display,
                    ],
                ),
            );
            let min_size = external_size.min(group_size);
            let ext_trunc: Vec<NetPoint> = external_points.into_iter().take(min_size).collect();
            let grp_trunc: Vec<NetPoint> = group_points.into_iter().take(min_size).collect();
            self.create_connection(ext_trunc, grp_trunc, dir, None)?;
        }

        Ok(())
    }

    // ========================================================================
    // Generic connection generation
    // ========================================================================

    /// Generic connection generation (1:1 / 1:N / N:1 / N:N + truncation)
    pub(super) fn create_connection(
        &mut self,
        left_points: Vec<NetPoint>,
        right_points: Vec<NetPoint>,
        dir: ConnDir,
        lane: Option<u16>,
    ) -> Result<(), InstError> {
        let left_size = left_points.len();
        let right_size = right_points.len();
        if left_size == 0 || right_size == 0 {
            return Ok(());
        }

        // ── §3 shape-match check (eval.md) ────────────────────────────────
        // Endpoint-layer shape is N×1 (one NetPoint per row). Same row count
        // → §3 allows 1:1 pairing (by-name / sorted zip); different row count
        // → §3 rejects, handled by the recovery branch below: 1:N broadcast
        // (group / DC bus / interface expansion semantics) or
        // N:M truncation (genuine misalignment → E4007 diagnostic).

        // ★ P9-A2: compute source_span and trunk once for this connection
        // Decision A (§7.1): source_span carries a **byte offset**, not a line
        // number; display layers convert offset → line via the owning file.
        let source_span: Option<crate::semantic::common::SourcePos> =
            match (&self.current_func_span, &self.current_stmt_span) {
                // Func-body expansion context (func may live in another file)
                (Some(spos), _) => Some(spos.clone()),
                (None, Some(s)) => Some(crate::semantic::common::SourcePos::new(
                    self.def_uri.clone(),
                    s.offset,
                )),
                (None, None) => None,
            };
        // ★ §8.9.6: structured group context. Prefer `current_trunk`
        // (set from source code context), fall back to `trunk_from_points`;
        // the coarse kind rides along inside `TrunkCtx`.
        let trunk: Option<TrunkCtx> = self
            .current_trunk
            .clone()
            .map(|g| {
                TrunkCtx::from_group_member(
                    &g,
                    self.current_trunk_kind,
                    self.current_trunk_iface.clone(),
                )
            })
            .or_else(|| {
                let mut all_pts: Vec<&NetPoint> = Vec::new();
                all_pts.extend(left_points.iter());
                all_pts.extend(right_points.iter());
                trunk_from_points(&all_pts).map(|g| TrunkCtx::from_group_member(&g, None, None))
            });

        // Helper to create ConnectionInst with consistent lane+dir+op+provenance.
        // `create_connection` is the series-entry (all callers connect adjacent
        // phrase members with `-`/`->`/`<-`); `+` goes through
        // `wire_parallel_internal` (stmt.rs), which tags Parallel explicitly.
        let mk_conn = |id, pts: Vec<NetPoint>, dir: ConnDir, lane: Option<u16>| -> ConnectionInst {
            let mut conn = ConnectionInst::new(id, pts)
                .with_dir(dir)
                .with_op(ConnOp::Series);
            if let Some(l) = lane {
                conn = conn.with_lane(l);
            }
            if let Some(pos) = &source_span {
                conn = conn.with_source_span(pos.clone());
            }
            if let Some(ref pg) = trunk {
                // §8.9.6.7: refine the connection-level context into the
                // per-lane identity (member from the point's structured
                // member name), so bus member lanes render as a trunk.
                if let Some(refined) = refine_lane_trunk(Some(pg.clone()), &conn.points) {
                    conn = conn.with_trunk(refined);
                }
            }
            conn
        };

        // ── §5: logical-net uniqueness check (same-name multi-pin group) ─
        // A same-name group (`3 = GND; 4 = GND`) is ONE logical net whose
        // physical pads share the (owner, member_name) identity. Referencing
        // the group in two slots (`spk{GND, GND}`) re-emits every pad, so the
        // same logical net ends up referenced more than once in one pairing.
        // Per same-name-pin-group.md §5 that is either redundant (every
        // reference pairs to the same peer net) or a short (distinct peer nets
        // get tied together through the group's pads) — both non-blocking
        // warnings, so the connection is still built.
        let net_key = |p: &NetPoint| -> (String, String) {
            match (&p.owner, &p.member_name) {
                (Some(o), Some(m)) if !m.is_empty() => (o.clone(), m.clone()),
                // Ports / labels / unexpanded single pins carry no member
                // identity — they are unique by path.
                _ => (String::new(), p.path.clone()),
            }
        };

        // Phase 1: which logical nets are referenced more than once on each
        // side? A repeated slot shows up as the same logical net key appearing
        // twice (`spk{GND, GND}` → two slots of (spk, GND)). Only same-name
        // group slots (points that carry physical pads) count — a plain pin
        // repeated verbatim (`[A, A]`) keeps no pads and stays a MERGED_SHORT
        // defect handled by the D3 check below.
        let repeated_nets = |pts: &[NetPoint]| -> std::collections::HashSet<(String, String)> {
            let mut by_key: std::collections::HashMap<(String, String), Vec<&NetPoint>> =
                std::collections::HashMap::new();
            for p in pts {
                let k = net_key(p);
                by_key.entry(k).or_default().push(p);
            }
            by_key
                .into_iter()
                .filter_map(|(k, ps)| {
                    let any_group_slot = ps.iter().any(|p| !p.same_name_pads.is_empty());
                    (any_group_slot && ps.len() >= 2).then_some(k)
                })
                .collect()
        };
        let left_repeated = repeated_nets(&left_points);
        let right_repeated = repeated_nets(&right_points);

        // ── D3: MERGED_SHORT detection ──────────────────────────────────
        // A merged short is a genuine defect only when the *same connection
        // pair* (same left point + same right point) is created more than once,
        // e.g. `[A, A] -> GND` produces (A, GND) twice. Fan-out such as
        // `[P1, P2] -> [G, G]` produces the distinct pairs (P1, G) and (P2, G)
        // and is legitimate (multiple pins merging onto one net) — do not flag it.
        {
            // Pair model mirrors the connections created below: 1:N broadcast,
            // N:1 broadcast, N:M zip.
            let pairs: Vec<(&NetPoint, &NetPoint)> = match (left_size, right_size) {
                (1, _) => left_points
                    .iter()
                    .flat_map(|l| right_points.iter().map(move |r| (l, r)))
                    .collect(),
                (_, 1) => right_points
                    .iter()
                    .flat_map(|r| left_points.iter().map(move |l| (l, r)))
                    .collect(),
                _ => left_points
                    .iter()
                    .zip(right_points.iter())
                    .map(|(l, r)| (l, r))
                    .collect(),
            };
            let mut seen: std::collections::HashSet<(&str, &str)> =
                std::collections::HashSet::new();
            for (l, r) in &pairs {
                // A repeated same-name group re-emits the same physical pair;
                // that is the §5 redundancy/short classified below, not a
                // merged short — skip it here so it stays warning-level.
                if left_repeated.contains(&net_key(l)) || right_repeated.contains(&net_key(r)) {
                    continue;
                }
                if !seen.insert((l.path.as_str(), r.path.as_str())) {
                    // Use the NetPoint's src_pos for accurate error location;
                    // fall back to the current connection line's span, then the
                    // module definition's span start, so the diagnostic points
                    // near the actual source rather than (1,1).
                    let fallback = self
                        .current_stmt_span
                        .as_ref()
                        .map(|s| s.offset as i32)
                        .unwrap_or(self.def.span.start as i32);
                    let pos = left_points
                        .first()
                        .and_then(|p| p.src_pos.as_ref().map(|s| s.offset))
                        .unwrap_or(fallback as u32);
                    let len = l.path.len() as u32 + r.path.len() as u32 + 1;
                    let msg = format!(
                        "MERGED_SHORT: duplicate connection pair '{}' ↔ '{}' in \
                         connection. The same two points are connected more than once, \
                         merging into a short.",
                        l.path, r.path
                    );
                    diagnostic_log(
                        crate::errcodes::NET_MERGED_SHORT,
                        DiagnosticLevel::Error,
                        pos,
                        len,
                        &msg,
                        &[],
                    );
                    break;
                }
            }

            // ── §5 warning: classify the repeated references ─────────────
            // A repeated logical net whose paired peers are all the same net is
            // redundant; peers that differ are shorted together through the
            // group's pads. Report at most one warning per connection, with a
            // short taking precedence over mere redundancy.
            if !left_repeated.is_empty() || !right_repeated.is_empty() {
                let mut left_targets: std::collections::HashMap<
                    (String, String),
                    Vec<(String, String)>,
                > = std::collections::HashMap::new();
                let mut right_targets: std::collections::HashMap<
                    (String, String),
                    Vec<(String, String)>,
                > = std::collections::HashMap::new();
                for (l, r) in &pairs {
                    let lk = net_key(l);
                    let rk = net_key(r);
                    if left_repeated.contains(&lk) {
                        left_targets.entry(lk.clone()).or_default().push(rk.clone());
                    }
                    if right_repeated.contains(&rk) {
                        right_targets.entry(rk.clone()).or_default().push(lk);
                    }
                }
                let display = |k: &(String, String)| -> String {
                    if k.0.is_empty() {
                        k.1.clone()
                    } else {
                        format!("{}.{}", k.0, k.1)
                    }
                };
                let mut short: Option<(String, String)> = None;
                let mut redundant: Option<(String, String)> = None;
                for (net, peers) in left_targets.iter().chain(right_targets.iter()) {
                    let all_same = peers.windows(2).all(|w| w[0] == w[1]);
                    let net_disp = display(net);
                    if all_same {
                        if redundant.is_none() {
                            redundant = Some((net_disp, display(&peers[0])));
                        }
                    } else if short.is_none() {
                        // Distinct peer nets, order-preserving.
                        let mut seen: std::collections::HashSet<String> =
                            std::collections::HashSet::new();
                        let list: Vec<String> = peers
                            .iter()
                            .map(display)
                            .filter(|d| seen.insert(d.clone()))
                            .collect();
                        short = Some((net_disp, list.join(", ")));
                    }
                }
                if short.is_some() || redundant.is_some() {
                    let fallback = self
                        .current_stmt_span
                        .as_ref()
                        .map(|s| s.offset as i32)
                        .unwrap_or(self.def.span.start as i32);
                    let pos = left_points
                        .first()
                        .and_then(|p| p.src_pos.as_ref().map(|s| s.offset))
                        .unwrap_or(fallback as u32);
                    if let Some((net, peers)) = short {
                        let len = net.len() as u32;
                        let msg = format!(
                            "SHORT_REF: logical net '{net}' is referenced more than once and \
                             pairs to different nets [{peers}]. Those nets are shorted together \
                             through the same-name group's pads; review the connection."
                        );
                        diagnostic_log(
                            crate::errcodes::NET_SHORT_REF,
                            DiagnosticLevel::Warning,
                            pos,
                            len,
                            &msg,
                            &[],
                        );
                    } else if let Some((net, peer)) = redundant {
                        let len = net.len() as u32;
                        let msg = format!(
                            "DUPLICATE_REF: logical net '{net}' is referenced more than once \
                             and always pairs to the same net '{peer}'. The result is identical \
                             to '{net} -> {peer}'; simplify the redundant reference."
                        );
                        diagnostic_log(
                            crate::errcodes::NET_DUPLICATE_REF,
                            DiagnosticLevel::Warning,
                            pos,
                            len,
                            &msg,
                            &[],
                        );
                    }
                }
            }
        }

        if let Some(m) = expand_match(&left_points, &right_points) {
            // ── P4.2: §7 vector expansion matching (eval.md §7) ──────────────
            // The pure function expand_match replaces the old
            // try_match_by_member_name + sorted zip:
            //   Rule 1 layer correspondence — both sides have unique non-empty
            //              member names that can be paired one-to-one →
            //              match by name (keep lhs order, deterministic);
            //   Rule 2 total correspondence — equal counts → zip after stable
            //              sort by name, also producing the D5 signal;
            //   Rule 3 count mismatch → None (implicit auto-expansion is
            //              forbidden, falls into the recovery branch below).
            // Shape matching has already passed here (equal counts and both
            // sides non-empty), so expand_match is necessarily Some.
            mcc_dbg!(
                "inst::mod",
                "[P4.2-CONN] create_connection: left_size={left_size}, right_size={right_size}, \
                 expand pairs={}, all_members_mismatched={}",
                m.pairs.len(),
                m.all_members_mismatched,
            );

            // ── D5: BUS_ORDER_MISMATCH ─────────────────────────────────────
            // Multi-point 1:1 connection on both sides, and after the sorted
            // zip all pair member names are mutually different → the bus member
            // order may be misaligned (e.g. SPI SCLK↔MOSI). Not reported for a
            // single pair: for a scalar connection (e.g. VCC→VDD) differing
            // names are normal, not a bus misalignment. Same-name group slots
            // (`spk{GND, GND}`) are not a bus — pairing one against distinct
            // peer members is the §5 short case (NET_SHORT_REF), classified by
            // the repeated-net check above, never a bus-order mismatch.
            if m.pairs.len() >= 2
                && m.all_members_mismatched
                && !m
                    .pairs
                    .iter()
                    .any(|(l, r)| !l.same_name_pads.is_empty() || !r.same_name_pads.is_empty())
            {
                BUS_BITS_MISMATCHED.store(m.pairs.len(), std::sync::atomic::Ordering::Relaxed);
                let mismatches: Vec<String> = m
                    .pairs
                    .iter()
                    .enumerate()
                    .map(|(i, (l, r))| {
                        format!(
                            "#{i}: {}↔{}",
                            l.member_name.as_deref().unwrap_or(&l.path),
                            r.member_name.as_deref().unwrap_or(&r.path),
                        )
                    })
                    .collect();
                // Use the first left point's src_pos for error location;
                // fall back to the current line's span, then the module's.
                let fallback = self
                    .current_stmt_span
                    .as_ref()
                    .map(|s| s.offset as i32)
                    .unwrap_or(self.def.span.start as i32);
                let pos = left_points
                    .first()
                    .and_then(|p| p.src_pos.as_ref().map(|s| s.offset))
                    .unwrap_or(fallback as u32);
                let len = left_points
                    .first()
                    .map(|p| p.path.len() as u32)
                    .unwrap_or(0);
                let msg = format!(
                    "BUS_ORDER_MISMATCH: all {} pairs have mismatched member names: [{}]. \
                     This may indicate bus member order misalignment between the two sides.",
                    m.pairs.len(),
                    mismatches.join(", "),
                );
                diagnostic_log(
                    crate::errcodes::NET_BUS_ORDER_MISMATCH,
                    DiagnosticLevel::Info,
                    pos,
                    len,
                    &msg,
                    &[],
                );
            }

            for (l, r) in m.pairs {
                let conn = mk_conn(self.next_conn_id(), vec![l, r], dir, lane);
                self.add_connection(conn);
            }
        } else if left_size == 1 {
            let l = left_points
                .into_iter()
                .next()
                .ok_or_else(|| InstError::Other("expected 1 left point".into()))?;
            // ── P2: scalar ↔ DC bus → role-aligned, no broadcast (prevent power-to-ground short) ──
            if Self::is_dc_power_bus(&right_points) {
                self.connect_scalar_to_dc_bus(&l, &right_points);
            } else if let Some(expanded) = self.try_member_passthrough_scalar(&l, &right_points) {
                // ── P2/A2: bare submodule port expanded by peer member then per-bit zip ──
                for (le, r) in expanded.into_iter().zip(right_points.into_iter()) {
                    let conn = mk_conn(self.next_conn_id(), vec![le, r], dir, lane);
                    self.add_connection(conn);
                }
            } else {
                for r in right_points {
                    let conn = mk_conn(self.next_conn_id(), vec![l.clone(), r], dir, lane);
                    self.add_connection(conn);
                }
            }
        } else if right_size == 1 {
            let r = right_points
                .into_iter()
                .next()
                .ok_or_else(|| InstError::Other("expected 1 right point".into()))?;
            if Self::is_dc_power_bus(&left_points) {
                self.connect_scalar_to_dc_bus(&r, &left_points);
            } else if let Some(expanded) = self.try_member_passthrough_scalar(&r, &left_points) {
                // ── P2/A2: same as above, scalar on the right ──
                for (l, re) in left_points.into_iter().zip(expanded.into_iter()) {
                    let conn = mk_conn(self.next_conn_id(), vec![l, re], dir, lane);
                    self.add_connection(conn);
                }
            } else {
                for l in left_points {
                    let conn = mk_conn(self.next_conn_id(), vec![l, r.clone()], dir, lane);
                    self.add_connection(conn);
                }
            }
        } else {
            // §3 row count mismatch (N×1 vs M×1, N, M ≥ 2): a genuine vector
            // alignment error. Pass1 checks the phrase-layer shape, but dynamic
            // pins / FuncCall returns / interface expansion can still surface a
            // mismatch here — upgraded from a truncation warning to E4007
            // (vec-dianlu.md §5.1 left-alignment). Still paired by min so the
            // netlist stays buildable.
            self.record_error(
                crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                crate::errcodes::format_msg(crate::errcodes::CONN_SERIES_SHAPE_MISMATCH, &[]),
            );
            // ── P5: E2904 (expand dim mismatch, eval.md §7 rule 3) ─────────
            // When both sides carry named members, the mismatch is a
            // bus-member expansion problem: implicit auto-expansion is
            // forbidden, so a named N×1 vs M×1 pair needs an explicit `*`
            // expansion list or `_` placeholders. Attach the P5.4 fix
            // suggestion to the message.
            if left_points
                .iter()
                .chain(right_points.iter())
                .any(|p| p.member_name.as_deref().is_some_and(|n| !n.is_empty()))
            {
                let suggestion =
                    crate::vector::model::netshape::suggest_shape_fix(left_size, right_size);
                self.record_warning(
                    crate::errcodes::SHAPE_EXPAND_DIM_MISMATCH,
                    crate::errcodes::format_msg(
                        crate::errcodes::SHAPE_EXPAND_DIM_MISMATCH,
                        &[
                            &left_size as &dyn std::fmt::Display,
                            &right_size as &dyn std::fmt::Display,
                            &suggestion.as_deref().unwrap_or("") as &dyn std::fmt::Display,
                        ],
                    ),
                );
            }
            let min_size = left_size.min(right_size);
            for (l, r) in left_points
                .into_iter()
                .zip(right_points.into_iter())
                .take(min_size)
            {
                let conn = mk_conn(self.next_conn_id(), vec![l, r], dir, lane);
                self.add_connection(conn);
            }
        }

        Ok(())
    }

    /// ★ P9-A2: Create a ConnectionInst with provenance (source_span + trunk)
    /// from the current context.
    ///
    /// `source_span` is derived from `current_stmt_span` (set by phases.rs before
    /// processing each source line). `trunk` is extracted from the common
    /// parent segment of the dot-separated point paths.
    ///
    /// This is the canonical factory for ConnectionInst — call sites that directly
    /// use `ConnectionInst::new` will miss provenance and cause R-M edge merge to
    /// degrade.
    pub(super) fn make_conn_with_provenance(
        &self,
        id: u32,
        points: Vec<NetPoint>,
        dir: ConnDir,
        lane: Option<u16>,
    ) -> ConnectionInst {
        // Decision A (§7.1): source_span carries a byte offset (see the other
        // construction site in this file).
        let source_span: Option<crate::semantic::common::SourcePos> =
            match (&self.current_func_span, &self.current_stmt_span) {
                // Func-body expansion context (func may live in another file)
                (Some(spos), _) => Some(spos.clone()),
                (None, Some(s)) => Some(crate::semantic::common::SourcePos::new(
                    self.def_uri.clone(),
                    s.offset,
                )),
                (None, None) => None,
            };
        // ★ §8.9.6: structured group context. Prefer `current_trunk`
        // (set from source code context), fall back to `trunk_from_points`
        // (extracted from point paths); the coarse kind rides along.
        let trunk: Option<TrunkCtx> = self
            .current_trunk
            .clone()
            .map(|g| {
                TrunkCtx::from_group_member(
                    &g,
                    self.current_trunk_kind,
                    self.current_trunk_iface.clone(),
                )
            })
            .or_else(|| {
                let pts: Vec<&NetPoint> = points.iter().collect();
                trunk_from_points(&pts).map(|g| TrunkCtx::from_group_member(&g, None, None))
            });
        let mut conn = ConnectionInst::new(id, points).with_dir(dir);
        if let Some(l) = lane {
            conn = conn.with_lane(l);
        }
        if let Some(pos) = &source_span {
            conn = conn.with_source_span(pos.clone());
        }
        if let Some(ref pg) = trunk {
            // §8.9.6.7: refine into the per-lane identity (mirror of
            // create_connection's mk_conn).
            if let Some(refined) = refine_lane_trunk(Some(pg.clone()), &conn.points) {
                conn = conn.with_trunk(refined);
            }
        }
        conn
    }

    /// ── P2: connect a scalar net to a DC bus with role alignment ──
    /// Power-rail members ← scalar (representing that power net); ground members ← global GND.
    /// Covers `usbsocket.vin -> V5V`: V5V~vin.POWER_SYS, vin.GND~GND (no short).
    fn connect_scalar_to_dc_bus(&mut self, scalar: &NetPoint, bus: &[NetPoint]) {
        let scalar_is_ground = is_ground_point(scalar);
        for p in bus {
            // Prefer member_name for role detection: interface member points carry
            // the member (e.g. ldo.VOUT.GND → member_name "GND") while the path is
            // a physical pin id (e.g. "ldo.2") that name heuristics cannot classify.
            let last = p
                .member_name
                .as_deref()
                .or_else(|| Some(p.path.rsplit('.').next().unwrap_or("")))
                .unwrap_or("");
            let id = self.next_conn_id();
            if is_ground_name(last) {
                if scalar_is_ground {
                    // Ground scalar (bare `GND` or `s.GND` → pid `s.2` with
                    // member_name "GND") lands on the bus ground member — wiring
                    // it to the power member would short the rail to ground.
                    self.add_connection(self.make_conn_with_provenance(
                        id,
                        vec![scalar.clone(), p.clone()],
                        ConnDir::Undirected,
                        None,
                    ));
                } else {
                    // Strict DC rail identity: the bus ground member belongs to
                    // the scalar rail (`{scalar}.GND`), not the module's bare
                    // `GND` label. Different rails keep distinct grounds until
                    // real wiring ties them together.
                    let gnd = self.rail_ground_point(scalar, last);
                    self.add_connection(self.make_conn_with_provenance(
                        id,
                        vec![p.clone(), gnd],
                        ConnDir::Undirected,
                        None,
                    ));
                }
            } else if !scalar_is_ground {
                self.add_connection(self.make_conn_with_provenance(
                    id,
                    vec![scalar.clone(), p.clone()],
                    ConnDir::Undirected,
                    None,
                ));
            }
        }
    }

    /// ── P2: check whether a set of points constitutes a DC power bus ──
    /// i.e. it contains both power-rail members and ground members.
    fn is_dc_power_bus(points: &[NetPoint]) -> bool {
        is_dc_power_bus_points(points)
    }

    /// ── P2/A2: boundary member passthrough (fallback) ─────────────────────────────────────
    /// When **one side is N(≥2) lanes of the same owner in `X.<member>` form**, and the other side
    /// is some submodule's **bare scalar port** (`sub.port`, whose `bus_members` is empty in the
    /// submodule and whose port name is neither power nor ground), expand the scalar port by the
    /// peer's member names into `sub.port.<member_i>`, returning the lanes aligned to the peer
    /// (order matches `others`). Any miss returns None.
    ///
    /// Sole target scenario: `mic.MIC -> mcu.MIC` (main.mc:38). The left `mic.MIC` has been
    /// expanded per mic's `out MIC{P,N}` into `[mic.P, mic.N]`; the right `mcu.MIC` keeps
    /// scalar because the MIC chain inside mcu (main.mc:155) never emits → port `bus_members`
    /// is still empty, so it gets broadcast to both P/N and **shorts the differential pair**.
    /// Here we expand `mcu.MIC` into `mcu.MIC.P` / `mcu.MIC.N` and zip with the left,
    /// so the boundary nets become the expected `mic.MIC.P ~ mcu.MIC.P` /
    /// `mic.MIC.N ~ mcu.MIC.N`.
    ///
    /// The guard stays narrow (must be a real submodule bare port hit by find_submodule + peer
    /// ≥2 lanes, common prefix, distinct members): it does not affect `flash.SPI~mcu.spi`
    /// (1-vs-1), DC bus (power/ground guard), or component pins. The only relaxation is on
    /// "peer-lane segment count" — accepting both `owner.member` (2 segments) and
    /// `owner.port.member` (3 segments, e.g. `mic.MIC.P`); any multi-hit case is still a
    /// "multi-lane port vs bare port on both sides" scenario which **should** zip, so replacing
    /// broadcast with zip is a fix, not a regression.
    ///
    /// ── S1 Bug A extension (2026-06) ─────────────────────────────────────
    /// Additionally supports scalar boundary formals inside a submodule's **internal body**
    /// (e.g. `spi` inside `do_flash(spi) { spi + uC.SPI }` body). Here `spi` is a boundary
    /// formal, treated as a bare label (1 point) in the submodule's Phase A body; the peer
    /// `uC.SPI` expands into 4 lanes (uC.8..11). The current implementation only recognizes
    /// the `sub.port` (2-segment) form, so bare `spi` (1 segment, a label) misses → falls
    /// back to broadcast → all 4 uC SPI pins get shorted into the same net (S1 body side).
    ///
    /// Fix: when scalar.path contains no '.', treat scalar as a "boundary formal of the
    /// current submodule", look up self.ports for one with the same name and a non-empty
    /// bus_members (a declared interface port), and use its bus_members to expand into
    /// `[<formal>.<member_i>]` then zip with the peer. Case mismatch between formal name
    /// and port name also falls back to eq_ignore_ascii_case.
    ///
    /// Note: this is the P2 round-2 **boundary fallback (A2)**, fixing the parent-level
    /// `mic.MIC -> mcu.MIC` differential-pair short; it does not fix the missing
    /// `mcu.MIC.{P,N} -> cap[4:5] -> uC.ADC.{P,N}` chain inside mcu (that's the
    /// array instance at main.mc:155 not being materialized in the middle of the chain,
    /// root cause C).
    fn try_member_passthrough_scalar(
        &self,
        scalar: &NetPoint,
        others: &[NetPoint],
    ) -> Option<Vec<NetPoint>> {
        if others.len() < 2 {
            return None;
        }
        // Peer N lanes: must all share the **same prefix** `<prefix>.<member>`, with distinct members.
        let mut members: Vec<String> = Vec::with_capacity(others.len());
        let mut prefix0: Option<&str> = None;
        for o in others {
            let (oprefix, omember) = o.path.rsplit_once('.')?;
            match prefix0 {
                None => prefix0 = Some(oprefix),
                Some(w) if w != oprefix => return None,
                _ => {}
            }
            if members.iter().any(|m| m.as_str() == omember) {
                return None; // duplicate member → not a clean N×1 bus, give up
            }
            members.push(omember.to_string());
        }

        // ── P2-2: extract member names from others for pin ID lookup ──
        // Prefer the member_name field (set by P2-1 bus port expansion);
        // fall back to the last path segment for bare pin IDs.
        let peer_member_names: Vec<&str> = others
            .iter()
            .map(|o| {
                o.member_name
                    .as_deref()
                    .unwrap_or_else(|| o.path.rsplit('.').next().unwrap_or(&o.path))
            })
            .collect();

        // ── Form 1: scalar = `sub.port` (2 segments) — original P2/A2 path ─────
        if let Some((sub, port)) = scalar.path.split_once('.') {
            if !port.contains('.') && !is_power_rail_name(port) && !is_ground_name(port) {
                if let Some(submod) = self.find_submodule(sub) {
                    if submod
                        .ports
                        .iter()
                        .any(|p| p.name == port && p.bus_members.is_empty())
                    {
                        // ── P2-2: try physical pin ID lookup from submodule's components ──
                        // When the submodule's port has empty bus_members, look for a
                        // component inside the submodule that has a same-named bus port,
                        // and use its physical pin IDs (e.g. mcu.10 instead of mcu.SPI.1).
                        // First try member-name matching, then fall back to positional.
                        let pin_ids: Option<Vec<String>> =
                            submod.components.iter().find_map(|comp| {
                                comp.find_bus_port_pin_ids(port)
                                    .map(|pairs| pairs.into_iter().map(|(_, pid)| pid).collect())
                            });

                        if let Some(ref pids) = pin_ids {
                            if pids.len() == members.len() {
                                // Try member-name matching first
                                let pin_map: std::collections::HashMap<&str, &str> = {
                                    // Build map from peer_member_names → pids, but since
                                    // member names may be empty, fall back to positional
                                    let mut map = std::collections::HashMap::new();
                                    for (i, m) in peer_member_names.iter().enumerate() {
                                        if !m.is_empty() && i < pids.len() {
                                            map.insert(*m, pids[i].as_str());
                                        }
                                    }
                                    map
                                };

                                let lanes: Vec<NetPoint> = if !pin_map.is_empty() {
                                    peer_member_names
                                        .iter()
                                        .filter_map(|m| {
                                            pin_map.get(m).map(|pid| {
                                                NetPoint::with_owner(
                                                    &format!("{sub}.{pid}"),
                                                    sub,
                                                    scalar.iotype.clone(),
                                                )
                                            })
                                        })
                                        .collect()
                                } else {
                                    // Fallback: positional zip
                                    members
                                        .iter()
                                        .enumerate()
                                        .map(|(i, _m)| {
                                            NetPoint::with_owner(
                                                &format!("{sub}.{}", pids[i]),
                                                sub,
                                                scalar.iotype.clone(),
                                            )
                                        })
                                        .collect()
                                };

                                if lanes.len() == members.len() {
                                    return Some(lanes);
                                }
                            }
                            // Pin count mismatch → fall through to original behavior
                        }

                        // Original behavior: use member names as suffix
                        let lanes: Vec<NetPoint> = members
                            .iter()
                            .map(|m| {
                                NetPoint::with_owner(
                                    &format!("{sub}.{port}.{m}"),
                                    sub,
                                    scalar.iotype.clone(),
                                )
                            })
                            .collect();
                        return Some(lanes);
                    }
                }
            }
        }

        // ── Form 2: scalar is a bare label (1 segment) — S1 Bug A extension ──────
        // Current scope is some submodule's body; `scalar.path = "spi"` is a boundary formal.
        // self.ports has a same-named declared interface port (`SPI`, with non-empty bus_members);
        // use its bus_members to expand into `[spi.<member_i>]` and zip with the peer.
        if !scalar.path.contains('.') {
            let formal = scalar.path.as_str();
            // Power/ground handled by connect_scalar_to_dc_bus
            if is_power_rail_name(formal) || is_ground_name(formal) {
                return None;
            }
            // Prefer exact match, then case-insensitive fallback (same fix as Bug D)
            let bus_members: Vec<String> = self
                .ports
                .iter()
                .find(|p| p.name == formal && !p.bus_members.is_empty())
                .or_else(|| {
                    self.ports
                        .iter()
                        .find(|p| p.name.eq_ignore_ascii_case(formal) && !p.bus_members.is_empty())
                })
                .map(|p| p.bus_members.clone())?;
            if bus_members.len() != members.len() {
                // Lane count mismatch → degrade, do not force zip (avoid misalignment)
                return None;
            }
            let lanes: Vec<NetPoint> = bus_members
                .iter()
                .map(|m| {
                    NetPoint::with_owner(&format!("{formal}.{m}"), formal, scalar.iotype.clone())
                })
                .collect();
            return Some(lanes);
        }
        None
    }
}

fn is_ground_name(s: &str) -> bool {
    let u = s.to_uppercase();
    matches!(u.as_str(), "GND" | "VSS" | "AGND" | "DGND" | "PGND")
        || u.starts_with("GND")
        || u.starts_with("VSS")
}

/// Ground check for a NetPoint that prefers `member_name` over the path leaf.
/// Component pin alias paths are unified to pin-id paths at construction
/// (`s.GND` → `s.2`), so the path leaf loses the rail name while
/// `member_name` keeps it (`Some("GND")`).
fn is_ground_point(p: &NetPoint) -> bool {
    let name = p
        .member_name
        .as_deref()
        .unwrap_or_else(|| p.path.rsplit('.').next().unwrap_or(&p.path));
    is_ground_name(name)
}

/// Extract the common port group from a set of NetPoint paths.
///
/// For paths like `mcu513.SPI.SCLK` and `flash.SPI.SCLK`, the common
/// parent segment is `SPI`. Returns `None` when paths have fewer than
/// 3 segments or the common parent cannot be determined.
///
/// This is NOT a heuristic guess — the path segments come directly from
/// the source code's dot-separated identifiers.
pub(super) fn trunk_from_points(points: &[&NetPoint]) -> Option<String> {
    if points.len() < 2 {
        return None;
    }

    let candidates: Vec<Option<&str>> = points
        .iter()
        .map(|p| {
            let segs: Vec<&str> = p.path.split('.').collect();
            match segs.len() {
                0 | 1 => None,
                2 => {
                    // Two-segment path like "mcu513.DAC_OUT": use the last segment.
                    // But skip if the last segment looks like a pin number (all digits).
                    let last = segs[1];
                    if last.chars().all(|c| c.is_ascii_digit()) {
                        None
                    } else {
                        Some(last)
                    }
                }
                _ => {
                    // Three+ segment path like "mic.MIC.N": use second-to-last segment.
                    Some(segs[segs.len() - 2])
                }
            }
        })
        .collect();

    let first = candidates.first()?;
    if candidates.iter().all(|c| *c == *first) {
        first.map(|s| s.to_string())
    } else {
        None
    }
}

/// §8.9.6.7: refine a connection-level group context into the per-lane
/// identity for bus member lanes. The group name/kind come from the AST-layer
/// context; the lane member is taken from the first point stamped with a
/// structured member name (set by bus expansion). When no point carries one
/// (flattened member paths like `MIC{P,N}` → points "MIC.P"/"MIC.N"), fall
/// back to the point-path suffix anchored on the group name — it only fires
/// when the point provably belongs to the group (path starts with
/// `"<group>."`), never a blind last-segment split. Plain connections keep
/// their context untouched.
pub(super) fn refine_lane_trunk(ctx: Option<TrunkCtx>, points: &[NetPoint]) -> Option<TrunkCtx> {
    let mut pg = ctx?;
    if pg.kind == TrunkKind::Plain {
        return Some(pg);
    }
    if let Some(member) = points.iter().find_map(|p| p.member_name.clone()) {
        pg.member = Some(member);
    } else if let Some(name) = pg.name.as_deref() {
        let prefix = format!("{name}.");
        if let Some(member) = points
            .iter()
            .find_map(|p| p.path.strip_prefix(&prefix).map(|s| s.to_string()))
        {
            pg.member = Some(member);
        }
    }
    Some(pg)
}

fn is_power_rail_name(s: &str) -> bool {
    let u = s.to_uppercase();
    const EXACT: &[&str] = &["VCC", "VDD", "VBUS", "VPP", "AVDD", "POWER_SYS"];
    if EXACT.contains(&u.as_str()) {
        return true;
    }
    if ["VCC", "VDD", "V3V", "V5V", "V1V", "VIN", "VOUT"]
        .iter()
        .any(|p| u.starts_with(p))
    {
        return true;
    }
    // Voltage patterns like 3V3 / 5V0 / 1V2
    let b = u.as_bytes();
    b.iter().enumerate().any(|(i, &c)| {
        c == b'V'
            && i > 0
            && i + 1 < b.len()
            && b[i - 1].is_ascii_digit()
            && b[i + 1].is_ascii_digit()
    })
}

/// Whether a set of endpoints constitutes a DC power bus (containing both power-rail members and ground members).
/// Used by create_connection to determine whether broadcasting would short power to ground.
fn is_dc_power_bus_points(points: &[NetPoint]) -> bool {
    // Prefer member_name (interface member points carry it; e.g. ldo.VIN member
    // "Vin"/"GND") and fall back to the path's last segment for plain labels.
    fn role_name(p: &NetPoint) -> &str {
        p.member_name
            .as_deref()
            .unwrap_or_else(|| p.path.rsplit('.').next().unwrap_or(&p.path))
    }
    let has_pwr = points.iter().any(|p| is_power_rail_name(role_name(p)));
    let has_gnd = points.iter().any(|p| is_ground_name(role_name(p)));
    has_pwr && has_gnd
}
