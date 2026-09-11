// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection generation
//!
//! - `create_connection`            —— Generic connection generation (1:1 / 1:N / N:1 / N:M)
//!
//! `Group` as a connection shape needs no strategy of its own: a multi-statement
//! group is expanded into statements before the wiring runs, and a one-element
//! group is see-through, so the fold handles the surviving shape directly
//! (unified-core §7.6 step 0 (3), settled 2026-09-11).

use super::expand::expand_match;
use super::InstantiationBuilder;
use crate::db::diagnostic::diagnostic::{diagnostic_log, DiagnosticLevel};
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint};
use crate::semantic::common::{ConnDir, ConnOp};
use crate::vector::model::trunk::{TrunkCtx, TrunkKind};

/// D5 BUS_ORDER_MISMATCH: process-level count of mismatched bus bits.
/// When all pairs in a bus connection have mismatched member names, D5 fires and
/// sets this to the bus width. The metrics module uses this to compute
/// `bus_bits_paired_ok = bus_bits_total - BUS_BITS_MISMATCHED`.
pub(crate) static BUS_BITS_MISMATCHED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

impl InstantiationBuilder {
    // ========================================================================
    // Generic connection generation
    // ========================================================================

    /// Generic connection generation (1:1 / 1:N / N:1 / N:N).
    ///
    /// Legal paths only: equal-row 1:1 pairing (by-name / sorted zip), the
    /// declared scalar-port member passthrough, and the §7.3 fan whose N side
    /// is ONE logical net (same-name multi-pin group). A single point against
    /// N ≥ 2 DISTINCT logical nets — a bare label meeting a DC [hot, ret]
    /// pair or a signal bus — is the abolished §5.3.1 single-point broadcast
    /// and is E4007 with no connection and no synthesized return, whether the
    /// bus is power or not. A genuine N:M row mismatch (N, M ≥ 2) is likewise
    /// E4007 with no connection — never truncated into a partial pair-by-min
    /// pairing (vec-dianlu.md §5.3.1/§5.3.3).
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

        // ── §5.3 shape-match check (vec-dianlu.md) ───────────────────────
        // Endpoint-layer shape is N×1 (one NetPoint per row). Same row count
        // → legal 1:1 pairing (by-name / sorted zip). Different row count is
        // handled below under the UNIFIED vector rule (model A): a single
        // point against N ≥ 2 points is legal only when (a) the scalar side
        // is a declared port that member-passes through to the N lanes
        // (`mic.MIC -> mcu.MIC`), or (b) the N side is ONE logical net whose
        // pads merge onto the scalar net (same-name multi-pin fan, §7.3).
        // Every other single-point ↔ bus pair — a bare label meeting a DC
        // [hot, ret] pair or a signal bus — is the abolished §5.3.1 single-
        // point broadcast (no DC role alignment, no synthesized return, no
        // silent drop) and is E4007 generating NO connection, whether the
        // bus is power or not. N:M with both sides ≥ 2 likewise E4007 — no
        // truncation, no pair-by-min recovery (§5.3.1/§5.3.3).

        // ★ P9-A2: compute source_span and trunk once for this connection
        // Decision A (§7.1): source_span carries a **byte offset**, not a line
        // number; display layers convert offset → line via the owning file.
        //
        // ★ Library-function expansion: when `current_func_span` points into a
        // DIFFERENT file than this module (e.g. the `Cap` method body in
        // `cap.mc`), that span is the library author's code, not the user's.
        // Attribute the connection to the user's statement that triggered the
        // call (`current_stmt_span`) so per-statement grouping (ground split)
        // and diagnostics see the real source line.
        let source_span: Option<crate::semantic::common::SourcePos> =
            match (&self.current_func_span, &self.current_stmt_span) {
                (Some(spos), Some(s)) if spos.uri != self.def_uri => Some(
                    crate::semantic::common::SourcePos::new(self.def_uri.clone(), s.offset),
                ),
                // Func-body expansion context (func in this module's file)
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
        // `vexpr_wire_parallel` (vexpr/eval.rs), which tags Parallel explicitly.
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
            // Pair model mirrors the connections created below: scalar→N
            // member fan (1:N), N member→scalar fan (N:1), N:M zip.
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
            // ── Unified vector rule (model A): scalar vs N ≥ 2 side ──────────
            // Only two shapes pass: a declared scalar port that member-passes
            // through to the N lanes (try_member_passthrough_scalar), or an N
            // side that is ONE logical net (same-name multi-pin group / the
            // same net repeated) whose pads legally merge onto the scalar net.
            // Everything else — a bare label meeting a DC [hot, ret] pair or a
            // distinct-lane signal bus — is the abolished §5.3.1 single-point
            // broadcast. No DC role alignment, no synthesized `{label}.GND`
            // return, no silent drop: report E4007 and generate NO connection
            // (power and non-power judged by the same structural rule).
            if let Some(expanded) = self.try_member_passthrough_scalar(&l, &right_points) {
                // ── P2/A2: bare submodule port expanded by peer member then per-bit zip ──
                for (le, r) in expanded.into_iter().zip(right_points.into_iter()) {
                    let conn = mk_conn(self.next_conn_id(), vec![le, r], dir, lane);
                    self.add_connection(conn);
                }
            } else if right_points
                .iter()
                .all(|p| net_key(p) == net_key(&right_points[0]))
            {
                // One logical net on the N side (§7.3 same-name fan): every pad
                // shares the (owner, member) net identity → merge onto scalar.
                for r in right_points {
                    let conn = mk_conn(self.next_conn_id(), vec![l.clone(), r], dir, lane);
                    self.add_connection(conn);
                }
            } else {
                self.record_error(
                    crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                    crate::errcodes::format_msg(crate::errcodes::CONN_SERIES_SHAPE_MISMATCH, &[]),
                );
            }
        } else if right_size == 1 {
            let r = right_points
                .into_iter()
                .next()
                .ok_or_else(|| InstError::Other("expected 1 right point".into()))?;
            // ── Unified vector rule (model A): mirror of the left_size==1 case ──
            if let Some(expanded) = self.try_member_passthrough_scalar(&r, &left_points) {
                // ── P2/A2: same as above, scalar on the right ──
                for (l, re) in left_points.into_iter().zip(expanded.into_iter()) {
                    let conn = mk_conn(self.next_conn_id(), vec![l, re], dir, lane);
                    self.add_connection(conn);
                }
            } else if left_points
                .iter()
                .all(|p| net_key(p) == net_key(&left_points[0]))
            {
                // One logical net on the N side → legal merge onto scalar.
                for l in left_points {
                    let conn = mk_conn(self.next_conn_id(), vec![l, r.clone()], dir, lane);
                    self.add_connection(conn);
                }
            } else {
                self.record_error(
                    crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                    crate::errcodes::format_msg(crate::errcodes::CONN_SERIES_SHAPE_MISMATCH, &[]),
                );
            }
        } else {
            // §5.3.3 row count mismatch (N×1 vs M×1, N, M ≥ 2): a genuine
            // vector alignment error. Pass1 checks the phrase-layer shape, but
            // dynamic pins / FuncCall returns / interface expansion can still
            // surface a mismatch here. Report E4007 and generate NO connection
            // — the row mismatch is not truncated into a partial pair-by-min
            // pairing (vec-dianlu.md §5.3.3: illegal ⇒ error + no recovery).
            self.record_error(
                crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                crate::errcodes::format_msg(crate::errcodes::CONN_SERIES_SHAPE_MISMATCH, &[]),
            );
            // ── P5: E2904 (expand dim mismatch, eval.md §7 rule 3) ─────────
            // When both sides carry named members, the mismatch is a
            // bus-member expansion problem: implicit auto-expansion is
            // forbidden, so a named N×1 vs M×1 pair needs an explicit `*`
            // expansion list or `_` placeholders. Attach the P5.4 fix
            // suggestion to the message. Informational only — no connection
            // is generated regardless.
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
        // construction site in this file). Library-function expansion
        // (`current_func_span` in a different file, e.g. the `Cap` body in
        // `cap.mc`) is attributed to the user's statement that triggered it.
        let source_span: Option<crate::semantic::common::SourcePos> =
            match (&self.current_func_span, &self.current_stmt_span) {
                (Some(spos), Some(s)) if spos.uri != self.def_uri => Some(
                    crate::semantic::common::SourcePos::new(self.def_uri.clone(), s.offset),
                ),
                // Func-body expansion context (func in this module's file)
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
    /// is still empty, so it stays scalar against both P/N and **shorts the differential pair**.
    /// Here we expand `mcu.MIC` into `mcu.MIC.P` / `mcu.MIC.N` and zip with the left,
    /// so the boundary nets become the expected `mic.MIC.P ~ mcu.MIC.P` /
    /// `mic.MIC.N ~ mcu.MIC.N`.
    ///
    /// The guard stays narrow (must be a real submodule bare port hit by find_submodule + peer
    /// ≥2 lanes, common prefix, distinct members): it does not affect `flash.SPI~mcu.spi`
    /// (1-vs-1), DC bus (power/ground guard), or component pins. The only relaxation is on
    /// "peer-lane segment count" — accepting both `owner.member` (2 segments) and
    /// `owner.port.member` (3 segments, e.g. `mic.MIC.P`); any multi-hit case is still a
    /// "multi-lane port vs bare port on both sides" scenario which **should** zip, so the
    /// per-member zip here is a fix, not a regression.
    ///
    /// ── S1 Bug A extension (2026-06) ─────────────────────────────────────
    /// Additionally supports scalar boundary formals inside a submodule's **internal body**
    /// (e.g. `spi` inside `do_flash(spi) { spi + uC.SPI }` body). Here `spi` is a boundary
    /// formal, treated as a bare label (1 point) in the submodule's Phase A body; the peer
    /// `uC.SPI` expands into 4 lanes (uC.8..11). The current implementation only recognizes
    /// the `sub.port` (2-segment) form, so bare `spi` (1 segment, a label) misses → falls
    /// back to scalar fan-out → all 4 uC SPI pins get shorted into the same net (S1 body side).
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
                        let pin_ids: Option<Vec<String>> = self
                            .components_of(submod.node_id?)
                            .into_iter()
                            .find_map(|comp| {
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
            // A power/ground-named bare label is a single conductor, not a declared
            // member column: never member-expand it. A scalar power/ground net
            // against an N-lane bus falls to the unified vector rule below
            // (same-net fan legal, distinct-net bus E4007) — no role alignment.
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
