// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Group / Transposed processing + connection generation
//!
//! - `get_group_branch_count` / `check_group_broadcast` / `analyze_group_shapes`
//! - `connect_to_group`             —— Connection strategy between Group and external points
//! - `get_transposed_shape` / `is_transposed` / `get_original_shape_before_transpose`
//! - `create_connection`            —— Generic N×M connection generation (1:1 / 1:N / N:1 / truncation)

use super::McModuleInst;
use crate::db::diagnostic::diagnostic::{diagnostic_log, DiagnosticLevel};
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::ConnDir;

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

    /// Get endpoint-count statistics for each branch inside a Group
    ///
    /// Returns (left_sizes, right_sizes)
    #[allow(dead_code)]
    pub(super) fn analyze_group_shapes(&mut self, member: &McPhrase) -> (Vec<usize>, Vec<usize>) {
        match member {
            McPhrase::Group(ref g) => {
                let mut left_sizes = Vec::new();
                let mut right_sizes = Vec::new();

                for phrase in &g.opds {
                    if let Ok(left_pts) = self.get_left_points_from_phrase(phrase) {
                        left_sizes.push(left_pts.len());
                    }
                    if let Ok(right_pts) = self.get_right_points_from_phrase(phrase) {
                        right_sizes.push(right_pts.len());
                    }
                }

                (left_sizes, right_sizes)
            }
            _ => (vec![1], vec![1]),
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
            self.create_connection(external_points, group_points, ConnDir::Undirected)?;
        } else if external_size == branch_count {
            // External point count equals branch count, per-branch connection
            // This needs special handling: each external point connects to its corresponding branch
            if external_is_left {
                let shape_ok = left_match;
                if !shape_ok {
                    eprintln!(
                        "Warning: Group left shapes inconsistent, connection may be incorrect"
                    );
                }
            } else {
                let shape_ok = right_match;
                if !shape_ok {
                    eprintln!(
                        "Warning: Group right shapes inconsistent, connection may be incorrect"
                    );
                }
            }
            self.create_connection(external_points, group_points, ConnDir::Undirected)?;
        } else if external_size == group_size {
            // Point counts match exactly, connect one-to-one
            self.create_connection(external_points, group_points, ConnDir::Undirected)?;
        } else {
            // ★ Degraded to warning: connect as much as possible, truncate by min
            self.record_warning(
                922,
                format!(
                    "Group shape mismatch: {external_size} external points vs {group_size} group points ({branch_count} branches), truncating"
                ),
            );
            let min_size = external_size.min(group_size);
            let ext_trunc: Vec<NetPoint> = external_points.into_iter().take(min_size).collect();
            let grp_trunc: Vec<NetPoint> = group_points.into_iter().take(min_size).collect();
            self.create_connection(ext_trunc, grp_trunc, ConnDir::Undirected)?;
        }

        Ok(())
    }

    // ========================================================================
    // Transpose and reverse processing (Iteration 7)
    // ========================================================================

    /// Compute the transposed shape
    ///
    /// Original shape: (left_count, right_count)
    /// After transpose: (left_count + right_count, left_count + right_count)
    ///
    /// Transposing makes all ports of the element exposed on both sides
    #[allow(dead_code)]
    fn get_transposed_shape(inner_line: &McPhrase) -> (usize, usize) {
        let left_count = inner_line.get_left().len();
        let right_count = inner_line.get_right().len();
        let total = left_count + right_count;
        (total, total)
    }

    /// Check whether this is a transposed McPhrase
    #[allow(dead_code)]
    fn is_transposed(member: &McPhrase) -> bool {
        matches!(member, McPhrase::Transposed(_))
    }

    /// Get the original shape before transposition
    #[allow(dead_code)]
    fn get_original_shape_before_transpose(member: &McPhrase) -> Option<(usize, usize)> {
        match member {
            McPhrase::Transposed(inner_line) => {
                let left_count = inner_line.get_left().len();
                let right_count = inner_line.get_right().len();
                Some((left_count, right_count))
            }
            _ => None,
        }
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
    ) -> Result<(), InstError> {
        let left_size = left_points.len();
        let right_size = right_points.len();
        if left_size == 0 || right_size == 0 {
            return Ok(());
        }

        // ── D3: MERGED_SHORT detection ──────────────────────────────────
        // Check for duplicate paths in left or right points (bracket expansion
        // like [A, A] creates duplicate connections that merge onto the same node).
        {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for p in left_points.iter().chain(right_points.iter()) {
                if !seen.insert(&p.path) {
                    // Use the NetPoint's src_pos for accurate error location;
                    // fall back to the current connection line's span, then the
                    // module definition's span start, so the diagnostic points
                    // near the actual source rather than (1,1).
                    let fallback = self
                        .current_line_span
                        .as_ref()
                        .map(|s| s.start as i32)
                        .unwrap_or(self.def.span.start as i32);
                    let pos = p.src_pos.unwrap_or(fallback) as u32;
                    let len = p.path.len() as u32;
                    let msg = format!(
                        "node=0 MERGED_SHORT: duplicate path '{}' in connection. \
                         A bracket expansion may contain duplicate entries causing signal merging.",
                        p.path
                    );
                    diagnostic_log(2003, DiagnosticLevel::Error, pos, len, &msg, &[]);
                    break;
                }
            }
        }

        if left_size == right_size {
            // ── P2-1: BUS BY NAME matching ──────────────────────────────────
            // When both sides have member_name set (from bus port expansion),
            // match by member name instead of position. This fixes SPI misalignment
            // where flash.SPI(CS,MISO,MOSI,SCLK) was zipped positionally against
            // uC.SPI(SCLK,MOSI,CSN,MISO) and got SCLK→MOSI etc.
            let matched_by_name = if left_size >= 2 {
                Self::try_match_by_member_name(&left_points, &right_points)
            } else {
                None
            };

            if let Some(pairs) = matched_by_name {
                for (l, r) in pairs {
                    let conn = ConnectionInst::new(self.next_conn_id(), vec![l, r]).with_dir(dir);
                    self.connections.push(conn);
                }
            } else {
                // ── D5: BUS_ORDER_MISMATCH detection ──────────────────────────────
                // When two multi-point lists are connected 1:1, compare the last
                // segment of each path at the same index. If they differ, the bus
                // member order may be misaligned.
                if left_size >= 2 {
                    let mut mismatches: Vec<String> = Vec::new();
                    for (i, (l, r)) in left_points.iter().zip(right_points.iter()).enumerate() {
                        let l_name = l.path.rsplit('.').next().unwrap_or(&l.path);
                        let r_name = r.path.rsplit('.').next().unwrap_or(&r.path);
                        if l_name != r_name {
                            mismatches.push(format!("#{}: {}↔{}", i, l_name, r_name));
                        }
                    }
                    if !mismatches.is_empty() && mismatches.len() == left_size {
                        BUS_BITS_MISMATCHED.store(left_size, std::sync::atomic::Ordering::Relaxed);
                        // Use the first left point's src_pos for error location;
                        // fall back to the current line's span, then the module's.
                        let fallback = self
                            .current_line_span
                            .as_ref()
                            .map(|s| s.start as i32)
                            .unwrap_or(self.def.span.start as i32);
                        let pos = left_points
                            .first()
                            .and_then(|p| p.src_pos)
                            .unwrap_or(fallback) as u32;
                        let len = left_points
                            .first()
                            .map(|p| p.path.len() as u32)
                            .unwrap_or(0);
                        let msg = format!(
                            "node=0 BUS_ORDER_MISMATCH: all {} pairs have mismatched member names: [{}]. \
                             This may indicate bus member order misalignment between the two sides.",
                            left_size, mismatches.join(", ")
                        );
                        diagnostic_log(2005, DiagnosticLevel::Error, pos, len, &msg, &[]);
                    }
                }
                for (l, r) in left_points.into_iter().zip(right_points.into_iter()) {
                    let conn = ConnectionInst::new(self.next_conn_id(), vec![l, r]).with_dir(dir);
                    self.connections.push(conn);
                }
            }
        } else if left_size == 1 {
            let l = left_points
                .into_iter()
                .next()
                .ok_or_else(|| InstError::Other("expected 1 left point".into()))?;
            // ── P2: scalar ↔ DC bus → role-aligned, no broadcast (prevent power-to-ground short) ──
            if Self::is_dc_power_bus(&right_points) && !is_ground_name(last_seg(&l.path)) {
                self.connect_scalar_to_dc_bus(&l, &right_points);
            } else if let Some(expanded) = self.try_member_passthrough_scalar(&l, &right_points) {
                // ── P2/A2: bare submodule port expanded by peer member then per-bit zip ──
                for (le, r) in expanded.into_iter().zip(right_points.into_iter()) {
                    let conn = ConnectionInst::new(self.next_conn_id(), vec![le, r]).with_dir(dir);
                    self.connections.push(conn);
                }
            } else {
                for r in right_points {
                    let conn =
                        ConnectionInst::new(self.next_conn_id(), vec![l.clone(), r]).with_dir(dir);
                    self.connections.push(conn);
                }
            }
        } else if right_size == 1 {
            let r = right_points
                .into_iter()
                .next()
                .ok_or_else(|| InstError::Other("expected 1 right point".into()))?;
            if Self::is_dc_power_bus(&left_points) && !is_ground_name(last_seg(&r.path)) {
                self.connect_scalar_to_dc_bus(&r, &left_points);
            } else if let Some(expanded) = self.try_member_passthrough_scalar(&r, &left_points) {
                // ── P2/A2: same as above, scalar on the right ──
                for (l, re) in left_points.into_iter().zip(expanded.into_iter()) {
                    let conn = ConnectionInst::new(self.next_conn_id(), vec![l, re]).with_dir(dir);
                    self.connections.push(conn);
                }
            } else {
                for l in left_points {
                    let conn =
                        ConnectionInst::new(self.next_conn_id(), vec![l, r.clone()]).with_dir(dir);
                    self.connections.push(conn);
                }
            }
        } else {
            // ★ Degraded to warning: do not abort, truncate connection by min(left, right)
            self.record_warning(
                920,
                format!(
                    "Shape mismatch: left={}, right={}, truncating to min({})",
                    left_size,
                    right_size,
                    left_size.min(right_size)
                ),
            );
            let min_size = left_size.min(right_size);
            for (l, r) in left_points
                .into_iter()
                .zip(right_points.into_iter())
                .take(min_size)
            {
                let conn = ConnectionInst::new(self.next_conn_id(), vec![l, r]).with_dir(dir);
                self.connections.push(conn);
            }
        }

        Ok(())
    }

    /// ── P2: connect a scalar net to a DC bus with role alignment ──
    /// Power-rail members ← scalar (representing that power net); ground members ← global GND.
    /// Covers `usbsocket.vin -> V5V`: V5V~vin.POWER_SYS, vin.GND~GND (no short).
    fn connect_scalar_to_dc_bus(&mut self, scalar: &NetPoint, bus: &[NetPoint]) {
        for p in bus {
            let last = p.path.rsplit('.').next().unwrap_or("");
            let id = self.next_conn_id();
            if is_ground_name(last) {
                let gnd = self.node_to_netpoint(&McBus::new("GND"));
                self.connections
                    .push(ConnectionInst::new(id, vec![p.clone(), gnd]));
            } else {
                self.connections
                    .push(ConnectionInst::new(id, vec![scalar.clone(), p.clone()]));
            }
        }
    }

    /// ── P2: check whether a set of points constitutes a DC power bus ──
    /// i.e. it contains both power-rail members and ground members.
    fn is_dc_power_bus(points: &[NetPoint]) -> bool {
        is_dc_power_bus_points(points)
    }

    /// ── P2-1: try to match two point lists by member name instead of position ──
    ///
    /// Returns `Some(reordered_pairs)` if both sides have member_name set and
    /// a one-to-one name match can be found. Returns `None` if matching fails
    /// (missing names, duplicate names, or incomplete match).
    fn try_match_by_member_name(
        left: &[NetPoint],
        right: &[NetPoint],
    ) -> Option<Vec<(NetPoint, NetPoint)>> {
        use std::collections::HashMap;

        // Build: member_name → left point
        let mut left_by_name: HashMap<&str, &NetPoint> = HashMap::new();
        for l in left {
            if let Some(ref name) = l.member_name {
                if name.is_empty() {
                    return None; // empty member name → fallback to position
                }
                if left_by_name.contains_key(name.as_str()) {
                    return None; // duplicate member name → fallback to position
                }
                left_by_name.insert(name.as_str(), l);
            } else {
                return None; // missing member_name → fallback to position
            }
        }

        // Build: member_name → right point
        let mut right_by_name: HashMap<&str, &NetPoint> = HashMap::new();
        for r in right {
            if let Some(ref name) = r.member_name {
                if name.is_empty() {
                    return None;
                }
                if right_by_name.contains_key(name.as_str()) {
                    return None;
                }
                right_by_name.insert(name.as_str(), r);
            } else {
                return None;
            }
        }

        // Match: for each left name, find right name
        let mut pairs: Vec<(NetPoint, NetPoint)> = Vec::new();
        for (name, l) in &left_by_name {
            if let Some(r) = right_by_name.get(name) {
                pairs.push(((*l).clone(), (*r).clone()));
            } else {
                return None; // name not found on right side → fallback to position
            }
        }

        if pairs.len() != left.len() || pairs.len() != right.len() {
            return None; // incomplete match → fallback to position
        }

        Some(pairs)
    }

    /// ── P2/A2: boundary member passthrough (fallback) ─────────────────────────────────────
    /// When **one side is N(≥2) lanes of the same owner in `X.<member>` form**, and the other side
    /// is some submodule's **bare scalar port** (`sub.port`, whose `bus_members` is empty in the
    /// submodule and whose port name is neither power nor ground), expand the scalar port by the
    /// peer's member names into `sub.port.<member_i>`, returning the lanes aligned to the peer
    /// (order matches `others`). Any miss returns None.
    ///
    /// Sole target scenario: `mic.MIC -> mcu513.MIC` (hbl:38). The left `mic.MIC` has been
    /// expanded per mic's `out MIC{P,N}` into `[mic.P, mic.N]`; the right `mcu513.MIC` keeps
    /// scalar because the MIC chain inside mcu513 (us513:155) never emits → port `bus_members`
    /// is still empty, so it gets broadcast to both P/N and **shorts the differential pair**.
    /// Here we expand `mcu513.MIC` into `mcu513.MIC.P` / `mcu513.MIC.N` and zip with the left,
    /// so the boundary nets become the expected `mic.MIC.P ~ mcu513.MIC.P` /
    /// `mic.MIC.N ~ mcu513.MIC.N`.
    ///
    /// The guard stays narrow (must be a real submodule bare port hit by find_submodule + peer
    /// ≥2 lanes, common prefix, distinct members): it does not affect `flash.SPI~mcu513.spi`
    /// (1-vs-1), DC bus (power/ground guard), or component pins. The only relaxation is on
    /// "peer-lane segment count" — accepting both `owner.member` (2 segments) and
    /// `owner.port.member` (3 segments, e.g. `mic.MIC.P`); any multi-hit case is still a
    /// "multi-lane port vs bare port on both sides" scenario which **should** zip, so replacing
    /// broadcast with zip is a fix, not a regression.
    ///
    /// ── S1 Bug A extension (2026-06) ─────────────────────────────────────
    /// Additionally supports scalar boundary formals inside a submodule's **internal body**
    /// (e.g. `spi` inside `loadFlash(spi) { spi + uC.SPI }` body). Here `spi` is a boundary
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
    /// `mic.MIC -> mcu513.MIC` differential-pair short; it does not fix the missing
    /// `mcu513.MIC.{P,N} -> cap[4:5] -> uC.ADC.{P,N}` chain inside mcu513 (that's the
    /// array instance at us513:155 not being materialized in the middle of the chain,
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
                        // and use its physical pin IDs (e.g. mcu513.10 instead of mcu513.SPI.1).
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

fn last_seg(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

fn is_ground_name(s: &str) -> bool {
    let u = s.to_uppercase();
    matches!(u.as_str(), "GND" | "VSS" | "AGND" | "DGND" | "PGND")
        || u.starts_with("GND")
        || u.starts_with("VSS")
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
    let has_pwr = points.iter().any(|p| is_power_rail_name(last_seg(&p.path)));
    let has_gnd = points.iter().any(|p| is_ground_name(last_seg(&p.path)));
    has_pwr && has_gnd
}
