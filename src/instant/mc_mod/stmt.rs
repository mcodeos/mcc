// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection statement processing
//!
//! - `process_stmt`: single stmt expansion + member/adjacent connection dispatch
//! - `phrase_to_members`: expand Series etc aggregate forms to member sequence
//! - `connect_adjacent_pair`: adjacent member pairing connections
//! - `process_member_internal`: single member internal processing (FuncCall / Closure / Group …)

use super::funccall::FuncCallInst;
use super::{AutoInst, InstantiationBuilder};
use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{ConnDir, IOType};
use crate::semantic::component::mc_pins::{McPinPort, PwrDir};
use crate::semantic::mc_inst::McInstance;
use crate::vector::model::trunk::TrunkKind;
use std::collections::HashSet;

// ── M11.4: lane item for position-aware bridge pin collection ──
pub(super) enum LaneItem<'a> {
    Series(&'a McPhrase),
    Bridge(NetPoint),
}

/// PWR-10 (6028) judged-position expectation for a power terminal in its own
/// connection chain: a `Out` position is the supply/OUT end (a source must lead),
/// an `In` position the load/IN end (a sink must trail). Members are stored in
/// written source order (R0 §2.4.5 corollary), so which end a member is comes
/// from the direction of the gap touching it — see [`McModInst::chain_end_role`].
#[derive(Clone, Copy, PartialEq, Debug)]
enum DirExpect {
    Out,
    In,
}

/// Base name of a port row, stripping a trailing `{…}` member group
/// (`vin{V5V, GND}` → `vin`); used to compare a reference's owner/token against
/// port-row names that may keep their written member group.
fn brace_plain(s: &str) -> &str {
    s.split_once('{').map(|(h, _)| h).unwrap_or(s)
}

impl InstantiationBuilder {
    /// Process connection stmt - accepts McPhrase
    pub(super) fn process_stmt(&mut self, phrase: &McPhrase) -> Result<(), InstError> {
        // ── §10.6: a `(,)` group is a STATEMENT LIST, not a shape — expand it
        // into the standalone statements it stands for before anything else
        // (`R101 - (s1, s2) + R106` ≡ `R101 - s1 + R106` / `R101 - s2 + R106`,
        // group-inner parentheses do not survive). Expanded branches carry no
        // group anymore, so the recursion terminates.
        if let Some(expanded) = phrase.expand_group_statements() {
            for stmt in expanded {
                self.process_stmt(&stmt)?;
            }
            return Ok(());
        }

        // ── G4: Skip stmts referencing failed components ──
        // If any FuncCall in the phrase references a class whose instantiation
        // previously failed, skip the entire stmt to avoid ghost pins.
        if !self.failed_classes.is_empty()
            && Self::phrase_contains_failed_class(phrase, &self.failed_classes)
        {
            self.record_warning(
                crate::errcodes::INST_STMT_SKIP_FAILED_CLASS,
                crate::errcodes::format_msg(crate::errcodes::INST_STMT_SKIP_FAILED_CLASS, &[]),
            );
            return Ok(());
        }

        // ── P0.4 follow-up: assign stable IDs before phrase_to_members clones ──
        // assign_phrase_ids was defined but never called, causing all FuncCall.id
        // to remain 0. Since auto_inst_map is keyed by member_key(f.id), all
        // FuncCalls shared key=0, overwriting each other's entries.

        let mut phrase = phrase.clone();
        Self::assign_phrase_ids(&mut phrase, &mut self.next_phrase_id);
        // ★ M-1'-A (edge-level): `members` + `gaps` come from one gapped flatten.
        // `gaps[i]` is the operator direction connecting `members[i]`~`members[i+1]`
        // (nested-Series directions preserved; non-Series phrases are Undirected).
        let (members, gaps) = self.phrase_to_members_gapped(&phrase);
        if members.is_empty() {
            return Ok(());
        }
        debug_assert_eq!(
            gaps.len(),
            members.len() - 1,
            "edge-level gap vector must align with member boundaries: {} members, {} gaps",
            members.len(),
            gaps.len()
        );

        // ── PWR-10 (6028): direction-word terminal at the wrong end of its own
        // connection chain (power-intent-design.md §5.3.2). Members are stored
        // in **written source order** (R0 §2.4.5 corollary — the reversal lives
        // in `ConnDir` alone), so which end is the supply/OUT end is read off
        // the gap direction at that end, not off the member index: `->` leads
        // left-to-right, `<-` is the mirror. Only directed ops are adjudicated
        // (`-`/`+` parallel joins claim no flow); psbi and direction-word-less
        // terminals never warn; a module wiring its own body into its own
        // exported power rows is internal and exempt. The direction word stays
        // authoritative for the semantic rules — this Warning only tells the
        // author the chain disagrees with the declared direction contract.
        self.audit_dc_binding_dir(&members, &gaps);

        // unified-twopin-no-builtin v2.0 §2.4: no chain-shunt special-case. A
        // `.Cap([a, b])` member is an ordinary FuncCall whose connection face
        // comes from the library func's return; the normal member loop +
        // adjacent pairing wire the pass-through lanes. `[2×1] -> CAP(1×2) ->
        // [2×1]` is a shape error reported by the series-row check.
        self.process_series_members(&members, &gaps)
    }

    // ── PWR-10 (6028): arrow/direction-word consistency audit ──
    // power-intent-design.md §5.3.2. A direction-word power terminal (module
    // power-port row or leaf-component power pin) must sit at the end of its own
    // connection chain its direction word claims: a source (psrc) face leads the
    // chain / the right of a `{L|R}` through, a sink (psnk) trails it / sits on
    // the left. Warning only — the direction word stays authoritative for the
    // semantic rules (6011/6019/6021/pwrflow).

    /// Walk the flattened member/gap vectors and flag direction-word terminals
    /// on the wrong side of their chain.
    pub(super) fn audit_dc_binding_dir(&mut self, members: &[McPhrase], gaps: &[ConnDir]) {
        let n = members.len();
        if n < 2 {
            return;
        }
        // Members are stored in **written source order** (R0 §2.4.5 corollary),
        // so a chain end's role is read off the direction of the gap touching
        // it, never assumed from its index: `->` (`LtoR`) leads left-to-right
        // (left member = OUT/supply end, right member = IN/load end), `<-`
        // (`RtoL`) is the mirror.
        if gaps[0].is_directed() {
            let (expect, position) = Self::chain_end_role(gaps[0], true);
            self.judge_dc_terms(&members[0], expect, position);
        }
        if gaps[n - 2].is_directed() {
            let (expect, position) = Self::chain_end_role(gaps[n - 2], false);
            self.judge_dc_terms(&members[n - 1], expect, position);
        }
        // Interior `{L|R}` through members: the upstream face is the DC-in side
        // (expects a sink), the downstream face the DC-out side (expects a
        // source). Upstream is the side the surrounding flow *arrives* from, so
        // it is derived from the directed flank's direction rather than fixed
        // to the left face. Judged when the chain asserts a flow around the
        // through (a directed op on either flank).
        for i in 1..n - 1 {
            let upstream_is_left = match (gaps[i - 1].is_directed(), gaps[i].is_directed()) {
                // The left flank carries the flow: `->` feeds forward (left
                // face upstream), `<-` feeds backward (right face upstream).
                (true, _) => gaps[i - 1] != ConnDir::RtoL,
                // Only the right flank is directed: `->` leaves through the
                // right face (left face upstream), `<-` the mirror.
                (false, true) => gaps[i] == ConnDir::RtoL,
                (false, false) => continue,
            };
            let McPhrase::Endpoint(McEndpoint::Node { input, output }) = &members[i] else {
                continue;
            };
            let (upstream, downstream, up_pos, down_pos) = if upstream_is_left {
                (
                    input,
                    output,
                    "the input (left) face of a {L|R} through",
                    "the output (right) face of a {L|R} through",
                )
            } else {
                (
                    output,
                    input,
                    "the output (right) face of a {L|R} through",
                    "the input (left) face of a {L|R} through",
                )
            };
            self.judge_dc_face(upstream, DirExpect::In, up_pos);
            self.judge_dc_face(downstream, DirExpect::Out, down_pos);
        }
    }

    /// Role of the chain end touching `gap` — `head` selects `members[0]`
    /// (the head) over `members[n-1]` (the tail). `LtoR` makes the left member
    /// the OUT end and the right member the IN end; `RtoL` is the mirror.
    /// `Undirected` never reaches here (callers guard with `is_directed`).
    fn chain_end_role(gap: ConnDir, head: bool) -> (DirExpect, &'static str) {
        let out = match gap {
            ConnDir::RtoL => !head,
            _ => head,
        };
        match (head, out) {
            (true, true) => (DirExpect::Out, "the head (the source / OUT end)"),
            (true, false) => (DirExpect::In, "the head (the sink / IN end)"),
            (false, true) => (DirExpect::Out, "the tail (the source / OUT end)"),
            (false, false) => (DirExpect::In, "the tail (the sink / IN end)"),
        }
    }

    /// Judge the single-face member at a chain end (head/tail).
    fn judge_dc_terms(&mut self, member: &McPhrase, expect: DirExpect, position: &str) {
        let mut refs = Vec::new();
        Self::member_refs(member, &mut refs);
        for (owner, tokens) in refs {
            self.warn_conflicting_terms(&owner, &tokens, expect, position);
        }
    }

    /// Judge one face of an interior `{L|R}` through.
    fn judge_dc_face(&mut self, face: &[McEndpoint], expect: DirExpect, position: &str) {
        for ep in face {
            let McEndpoint::Single(iref) = ep else {
                continue;
            };
            let Some((owner, tokens)) = Self::iref_tokens(iref) else {
                continue;
            };
            // Only a *written* through face (member tokens present) is a
            // direction claim. A bare module reference the net builder split
            // into a Node (P1-A2) has member-less, path-dotted buses and no
            // direction contract of its own — stay silent there.
            if tokens.is_empty() {
                continue;
            }
            self.warn_conflicting_terms(&owner, &tokens, expect, position);
        }
    }

    /// Resolve each written terminal of a reference and warn on those whose
    /// direction word contradicts the judged position.
    fn warn_conflicting_terms(
        &mut self,
        owner: &str,
        tokens: &[String],
        expect: DirExpect,
        position: &str,
    ) {
        for token in tokens {
            let Some((label, dir)) = self.pwr_dir_of_ref(owner, token) else {
                continue; // exempt self-wiring, psbi-unsupported shape, or net label
            };
            let conflict = match expect {
                DirExpect::Out => dir == PwrDir::Snk, // a sink cannot lead the chain
                DirExpect::In => dir == PwrDir::Src,  // a source cannot trail it
            };
            if !conflict {
                continue;
            }
            let word = match dir {
                PwrDir::Src => "psrc",
                PwrDir::Snk => "psnk",
                PwrDir::Bi => "psbi",
            };
            let args: Vec<&dyn std::fmt::Display> = vec![&label, &word, &position];
            let msg = crate::errcodes::format_msg(crate::errcodes::DC_BINDING_DIR_MISMATCH, &args);
            self.log_global_diag(
                crate::errcodes::DC_BINDING_DIR_MISMATCH,
                crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                msg,
            );
        }
    }

    /// Direction word a connection reference names, when the reference resolves
    /// to an authoritative non-exempt power terminal in *this* module's scope:
    /// - a leaf component pin under `owner` (def pwr-row hot pin) — always judged;
    /// - a child submodule power-port row (`owner.<port>`, matched by the
    ///   port's written members against `def.pi.pwr_ports`) — judged;
    /// - this module's own exported power-port row — a module wiring its own
    ///   body into its own power face is internal implementation (the direction
    ///   word is the contract to the *parent* frame): exempt (`None`).
    /// A plain net label / unresolved owner resolves to `None` too (never judged).
    fn pwr_dir_of_ref(&self, owner: &str, token: &str) -> Option<(String, PwrDir)> {
        // 1. Leaf component under the module being built. Power rows capture the
        // hot member *verbatim* — dotted for a named group (`psrc VOUT{Vout, GND}`
        // → hot `VOUT.Vout`, matching the pin net), bare for an anonymous pair
        // (`[1,2]=[IN,GND]` → hot `IN`). The judged token is that same spelling,
        // so compare whole tokens, never a last-segment split.
        if let Some(comp) = self.find_component(owner) {
            if let Some(row) = comp.def.pins.pwr.iter().find(|r| r.hot == token) {
                return Some((format!("{owner}.{token}"), row.dir));
            }
            return None;
        }
        // 2. Child submodule power-port row.
        if let Some(sub) = self.find_submodule(owner) {
            let port = sub.ports.iter().find(|p| {
                p.iotype == IOType::Power && brace_plain(&p.name) == brace_plain(token)
            })?;
            let bm = &port.bus_members;
            if bm.is_empty() {
                return None;
            }
            let row = sub.def.pi.pwr_ports.iter().find(|r| {
                r.hot == bm[0]
                    && match (&r.ret, bm.get(1)) {
                        (None, _) => true,
                        (Some(rh), Some(rm)) => rh == rm,
                        (Some(_), None) => false,
                    }
            })?;
            return Some((format!("{owner}.{token}"), row.dir));
        }
        // 3. Self's own exported port row → exempt.
        if self
            .ports
            .iter()
            .any(|p| brace_plain(&p.name) == brace_plain(owner))
        {
            return None;
        }
        None
    }

    /// Collect every `(owner, member-tokens)` reference reachable in a judged
    /// end member (through nodes at chain ends are not terminals and are
    /// intentionally not descended).
    fn member_refs<'x>(member: &'x McPhrase, out: &mut Vec<(String, Vec<String>)>) {
        match member {
            McPhrase::Endpoint(ep) => Self::endpoint_refs(ep, out),
            McPhrase::Multiple(inner) | McPhrase::Series(inner, _) | McPhrase::Parallel(inner) => {
                for p in inner {
                    Self::member_refs(p, out);
                }
            }
            _ => {}
        }
    }

    fn endpoint_refs<'x>(ep: &'x McEndpoint, out: &mut Vec<(String, Vec<String>)>) {
        match ep {
            McEndpoint::Single(iref) => {
                if let Some(t) = Self::iref_tokens(iref) {
                    out.push(t);
                }
            }
            McEndpoint::List(items) => {
                for e in items {
                    Self::endpoint_refs(e, out);
                }
            }
            McEndpoint::Node { .. } => {} // two-face members judged at the face level
        }
    }

    /// A single instance reference and its member tokens. A member-ful bus keeps
    /// its tokens (`USB{vin}` → "USB", ["vin"]); a bare bus splits a trailing
    /// dotted path (`USB.vin` → "USB", ["vin"]); a plain net label has neither
    /// members nor a dot → `None`.
    fn iref_tokens(iref: &McInstanceRef) -> Option<(String, Vec<String>)> {
        let (owner, mut tokens) = match &iref.base {
            McInstance::Bus(bus) => (bus.name().to_string(), bus.get_full_members().clone()),
            _ => return None,
        };
        // Member tokens sometimes ride the instance-ref member lists instead of
        // the bus (parse-path dependent) — merge both.
        for ml in &iref.members {
            tokens.extend(ml.expand());
        }
        if tokens.is_empty() {
            if let Some((o, m)) = owner.rsplit_once('.') {
                return Some((o.to_string(), vec![m.to_string()]));
            }
            return None; // plain net label — no owner, nothing to adjudicate
        }
        Some((owner, tokens))
    }

    /// Process a flattened series' members: P2-5 expansion, normal member loop,
    /// lane-by-lane wiring, and adjacent pairing.
    /// `gaps[i]` = operator direction connecting `members[i]`~`members[i+1]`
    /// (edge-level; nested-Series directions already threaded by the gapped flatten).
    fn process_series_members(
        &mut self,
        members: &[McPhrase],
        gaps: &[ConnDir],
    ) -> Result<(), InstError> {
        debug_assert_eq!(gaps.len(), members.len().saturating_sub(1));
        // ── P2-5: Expand builtin twopin calls adjacent to multi-member buses ──
        // When a builtin twopin (Pullup/Pulldown) is on the RIGHT side of a Multiple
        // with N > 1 members, iterate the FuncCall N times to create N components.
        // e.g. I2C0 => RES(10kΩ).Pullup(_, VDD) should create 2 resistors (SCL, SDA).
        //
        // Only expand when Multiple is on the LEFT (signal side). When Multiple is on
        // the RIGHT (e.g. Cap(_) -> [VDD, GND]), the Multiple represents the component's
        // own pins, NOT independent signals — do NOT expand.
        let mut i: usize = 0;
        // Track which member indices were consumed by P2-5 expansion, so the
        // downstream connect_adjacent_pair loop doesn't re-process them and
        // create shorting connections (e.g. SCL-SDA bridge).
        let mut p25_consumed: std::collections::HashSet<usize> = std::collections::HashSet::new();
        while i < members.len() {
            let (should_expand, n_items, fc_is_left) = match &members[i] {
                McPhrase::Multiple(inner) if inner.len() > 1 => {
                    if i + 1 < members.len() {
                        // ── P2-5 §8: arg-based detection — the FuncCall's
                        // folded Set arg carries this bus as a MEMBER (a `_`
                        // placeholder in the Set filled by the `=>` prefix).
                        // No method-name list: any method whose bus actual
                        // lane-expands is handled here.
                        let bus_actual = match &members[i + 1] {
                            McPhrase::FuncCall(fc) => match Self::multiple_base_bus(inner) {
                                Some(base_bus) => {
                                    Self::fc_params_reference_bus_in_set(fc, &base_bus)
                                }
                                None => false,
                            },
                            _ => false,
                        };
                        if bus_actual {
                            (true, inner.len(), false) // Multiple left, FuncCall right
                        } else {
                            (false, 0, false)
                        }
                    } else {
                        (false, 0, false)
                    }
                }
                _ => {
                    // P2-5 fix: do NOT expand when FuncCall is on the left and Multiple
                    // is on the right. This case (e.g. Cap(_) -> [VDD, GND]) means the
                    // Multiple is the component's own pins, not independent signals.
                    (false, 0, false)
                }
            };

            if should_expand {
                let multiple_idx = if fc_is_left { i + 1 } else { i };
                let fc_idx = if fc_is_left { i } else { i + 1 };
                let inner = match &members[multiple_idx] {
                    McPhrase::Multiple(inner) => inner.clone(),
                    _ => unreachable!(),
                };
                let fc = members[fc_idx].clone();
                mcc_dbg!("inst::mod", 
                    "[P2-5-EXPAND] module='{}' expanding builtin twopin: n_items={}, fc_is_left={fc_is_left}, fc={fc:?}",
                    self.name, n_items
                );

                // Mark these indices as consumed so they won't be re-processed
                // by the downstream connect_adjacent_pair loop.
                p25_consumed.insert(multiple_idx);
                p25_consumed.insert(fc_idx);

                for item in &inner {
                    let mut fc_clone = fc.clone();
                    // ── P2-5 fix: reset FuncCall IDs so each expanded pair
                    // gets fresh unique IDs from assign_phrase_ids. Without this,
                    // all pairs share the same ID, and P2-9 dedup incorrectly
                    // skips the second (and subsequent) builtin twopin
                    // instantiations (e.g. I2C0 SCL+SDA Pullup only creates 1 RES).
                    Self::reset_phrase_ids(&mut fc_clone);
                    // ── P2-5 fix: substitute the lane into the folded params so
                    // each expanded call binds cleanly. `I2C0 => RES(10kΩ).Pullup([_, VDD])`
                    // folds to `.Pullup([I2C0, VDD])`; per lane the bus `uC.I2C0`
                    // inside the Set must become `uC.I2C0.SCL` (§5 lane expansion)
                    // so the Pullup body wires pin2→VDD instead of leaving it
                    // dangling on a failed bind.
                    if let McPhrase::FuncCall(fc_ref) = &mut fc_clone {
                        if let Some((base_bus, lane_name)) = Self::bus_lane_of(item) {
                            Self::substitute_bus_in_fc_params(fc_ref, &base_bus, &lane_name);
                        }
                    }
                    // The Multiple~FuncCall pair sits at members[i]~members[i+1];
                    // their boundary operator direction is `gaps[i]` (edge-level).
                    let gap_dir = gaps[i];
                    let pair = if fc_is_left {
                        McPhrase::Series(vec![fc_clone, item.clone()], gap_dir)
                    } else {
                        McPhrase::Series(vec![item.clone(), fc_clone], gap_dir)
                    };
                    if let Err(e) = self.process_stmt(&pair) {
                        self.record_warning(
                            crate::errcodes::INST_BUILTIN_TWOPIN_EXPAND_FAILED,
                            crate::errcodes::format_msg(
                                crate::errcodes::INST_BUILTIN_TWOPIN_EXPAND_FAILED,
                                &[&e],
                            ),
                        );
                    }
                }
                i += 2; // skip both the Multiple and the FuncCall
                continue;
            }

            // Normal processing for non-expanded members
            if let Err(e) = self.process_member_internal(&members[i]) {
                self.record_warning(
                    crate::errcodes::INST_MEMBER_PROCESS_FAILED,
                    crate::errcodes::format_msg(crate::errcodes::INST_MEMBER_PROCESS_FAILED, &[&e]),
                );
            }
            i += 1;
        }

        // unified-twopin-no-builtin v2.0 §2.4: chain members are wired by the
        // normal lane-by-lane / adjacent paths only — no `wire_chain_with_shunts`
        // special-case. A `.Cap([a, b])` member's pass-through comes from its
        // func return face; a genuinely mis-shaped `[2×1] -> CAP(1×2) -> [2×1]`
        // chain is reported by the series-row check below.

        // ── M11.1 / M11.4: Lane-by-lane wiring ────────────────────────────
        // Use lane-by-lane wiring when the chain contains:
        // - Lead (_) pass-through elements (e.g. [RES, _])
        // - Standalone Transposed bridge passives (e.g. CAP')
        // - Parallel with Lead or Transposed (e.g. [RES, _] + CAP')
        let needs_lane_by_lane = members
            .iter()
            .any(|m| Self::member_contains_lead(m) || matches!(m, McPhrase::Transposed(_)));
        if needs_lane_by_lane {
            // §8.9.6.7: the lane-by-lane path bypasses connect_adjacent_pair,
            // so the AST-layer group context is never established there.
            // Extract it from the chain members (driver side first, then the
            // far side) and wire inside it, so bus member lanes carry their
            // trunk identity and render as a trunk. The written-pair rule
            // applies here too (§5.2, same as connect_adjacent_pair): a chain
            // that spells its DC pair at an end (`usb.vin -> [F1::FUSE(), _]
            // -> [VBUS_RAW, GND]`) is named by that pair, not by the instance
            // name the far-side terminal would otherwise leak.
            let tail: Vec<&McPhrase> = members[1..].iter().collect();
            let head: Vec<&McPhrase> = members[..members.len() - 1].iter().collect();
            let trunk = self
                .end_pair_trunk(&members[0], &tail)
                .or_else(|| self.end_pair_trunk(&members[members.len() - 1], &head))
                .or_else(|| Self::extract_trunk_group(&members[0]))
                .or_else(|| members.iter().rev().find_map(Self::extract_trunk_group));
            let trunk_kind = Self::extract_trunk_kind(&members[0])
                .or_else(|| members.iter().rev().find_map(Self::extract_trunk_kind))
                .or_else(|| trunk.as_ref().map(|_| TrunkKind::Plain));
            let trunk_iface = self.extract_trunk_iface(&members[0]).or_else(|| {
                members
                    .iter()
                    .rev()
                    .find_map(|m| self.extract_trunk_iface(m))
            });
            return self.with_trunk(trunk, trunk_kind, trunk_iface, |this| {
                this.vexpr_lane_chain(&members, gaps).map(|_| ())
            });
        }

        // handle adjacent member connections — per-pair fault-tolerant
        for i in 0..members.len().saturating_sub(1) {
            // Skip pairs where either member was consumed by P2-5 expansion
            if p25_consumed.contains(&i) || p25_consumed.contains(&(i + 1)) {
                continue;
            }
            let left_member = &members[i];
            let right_member = &members[i + 1];

            // Edge-level: this pair's operator is the gap at the left index.
            let gap_dir = gaps[i];
            if let Err(e) = self.connect_adjacent_pair(left_member, right_member, gap_dir) {
                self.record_warning(
                    crate::errcodes::INST_ADJACENT_CONNECT_FAILED,
                    crate::errcodes::format_msg(
                        crate::errcodes::INST_ADJACENT_CONNECT_FAILED,
                        &[
                            &i as &dyn std::fmt::Display,
                            &(i + 1) as &dyn std::fmt::Display,
                            &e as &dyn std::fmt::Display,
                        ],
                    ),
                );
            }
        }

        Ok(())
    }

    /// True when every actual parameter is a `_` placeholder (possibly nested
    /// in a Set/list), i.e. the call carries no explicit network endpoint
    /// (§11.6). `.Cap(_)` / `.Cap(_, _)` / `.Cap([_, _])` all qualify. These
    /// must not dispatch as a method — binding `_` to a formal would emit
    /// garbage nets. The folded chain-shunt form (`[A,B] => CAP(..).Cap(_)`)
    /// has already been rewritten to `.Cap([A, B])` (non-placeholder) and
    /// dispatches normally.
    fn is_all_placeholder_params(params: &[McParamValue]) -> bool {
        !params.is_empty() && params.iter().all(Self::is_placeholder_param)
    }

    fn is_placeholder_param(p: &McParamValue) -> bool {
        match p {
            McParamValue::NONE(_) => true,
            McParamValue::Opd(McOpd::Uscore) => true,
            McParamValue::Set(vals) => {
                !vals.is_empty() && vals.iter().all(Self::is_placeholder_param)
            }
            _ => false,
        }
    }

    // ── M11.2: check if a member contains Lead (recursively into Parallel) ──
    pub(super) fn phrase_contains_transposed(phrase: &McPhrase) -> bool {
        match phrase {
            McPhrase::Transposed(_) => true,
            McPhrase::Series(elems, _) => elems.iter().any(|e| Self::phrase_contains_transposed(e)),
            McPhrase::Multiple(inner) => inner.iter().any(|e| Self::phrase_contains_transposed(e)),
            McPhrase::Parallel(stmts) => stmts.iter().any(|l| Self::phrase_contains_transposed(l)),
            McPhrase::Group(g) => g.opds.iter().any(|e| Self::phrase_contains_transposed(e)),
            _ => false,
        }
    }

    pub(super) fn member_contains_lead(member: &McPhrase) -> bool {
        match member {
            McPhrase::Multiple(inner) => inner.iter().any(|p| matches!(p, McPhrase::Lead)),
            McPhrase::Parallel(stmts) => stmts.iter().any(|l| Self::member_contains_lead(l)),
            _ => false,
        }
    }

    /// ── P2-5 §8: arg-based lane-expansion detection (no builtin-name lists) ──
    /// The decision to lane-expand a FuncCall rests entirely on its actuals:
    /// the multi-member bus is present as a Set MEMBER (a `_` placeholder the
    /// `=>` prefix filled), never on what the method is called.
    ///
    /// Base bus of a multi-member `Multiple` — derived from the first lane's
    /// dotted name (`uC.I2C0.SCL` → `uC.I2C0`). The FuncCall is expanded iff
    /// that bus shows up inside one of its Set actuals.
    fn multiple_base_bus(inner: &[McPhrase]) -> Option<String> {
        Self::bus_lane_of(inner.first()?).map(|(base, _)| base)
    }

    fn fc_params_reference_bus_in_set(
        fc: &crate::semantic::basic::mc_fcall::McFuncCall,
        base_bus: &str,
    ) -> bool {
        fc.params
            .iter()
            .any(|p| Self::param_references_bus_in_set(p, base_bus))
    }

    /// True when `base_bus` appears as a Set member (not as a whole top-level
    /// value). A whole-value bus (e.g. `Cap(V3V3)` where the bus fills the
    /// entire Set formal) is handled by `align_vector_bindings` instead, and
    /// must NOT trigger lane expansion here.
    fn param_references_bus_in_set(p: &McParamValue, base_bus: &str) -> bool {
        match p {
            McParamValue::Set(vs) => vs.iter().any(|v| match v {
                McParamValue::Opd(McOpd::Id(ids)) => ids.to_string() == base_bus,
                McParamValue::Set(_) => Self::param_references_bus_in_set(v, base_bus),
                _ => false,
            }),
            _ => false,
        }
    }

    /// ── P2-5 lane substitution ────────────────────────────────────────────
    /// Extract (base_bus, lane_name) from a lane endpoint phrase. P2-5's
    /// `inner` Multiple holds per-member bus endpoints produced by
    /// `expand_multi_member_buses` (e.g. `Bus("uC.I2C0.SCL")` →
    /// `("uC.I2C0", "SCL")`). Returns None for anything that is not such a
    /// dotted bus lane.
    fn bus_lane_of(phrase: &McPhrase) -> Option<(String, String)> {
        if let McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Bus(bus),
            ..
        })) = phrase
        {
            let name = &bus.name;
            let dot = name.rfind('.')?;
            let (base, lane) = name.split_at(dot);
            let lane = &lane[1..];
            if !base.is_empty() && !lane.is_empty() {
                return Some((base.to_string(), lane.to_string()));
            }
        }
        None
    }

    /// Replace every `base_bus` reference in the FuncCall's params with the
    /// lane endpoint `base_bus.lane_name` (e.g. `uC.I2C0` → `uC.I2C0.SCL`),
    /// recursing into Sets so a folded `[I2C0, VDD]` actual becomes
    /// `[I2C0.SCL, VDD]` for the per-lane expanded call.
    fn substitute_bus_in_fc_params(
        fc: &mut crate::semantic::basic::mc_fcall::McFuncCall,
        base_bus: &str,
        lane_name: &str,
    ) {
        let lane_ids =
            crate::semantic::basic::mc_ids::McIds::from(format!("{base_bus}.{lane_name}").as_str());
        for p in fc.params.iter_mut() {
            Self::substitute_bus_in_param_value(p, base_bus, &lane_ids);
        }
    }

    fn substitute_bus_in_param_value(
        p: &mut McParamValue,
        base_bus: &str,
        lane_ids: &crate::semantic::basic::mc_ids::McIds,
    ) {
        match p {
            McParamValue::Opd(McOpd::Id(ids)) => {
                if ids.to_string() == base_bus {
                    *ids = lane_ids.clone();
                }
            }
            McParamValue::Set(vs) => {
                for v in vs.iter_mut() {
                    Self::substitute_bus_in_param_value(v, base_bus, lane_ids);
                }
            }
            _ => {}
        }
    }

    // ── M11.2: determine lane count for a chain member ──
    // Lane-wiring only (the `num_lanes` loop in `vexpr_lane_chain`):
    // how many independent parallel lanes a member spans. NOT a §5 port-width
    // source — width legality now goes through the unified
    // `get_left_points`/`get_right_points` → `Shape::vvec` → `check_series_rows`
    // chain (vec-arch.md stage D). A bare port label here is `1` lane, which is
    // correct for lane wiring even when the port declares multiple members.
    pub(super) fn member_lane_width(&self, member: &McPhrase) -> usize {
        match member {
            McPhrase::Multiple(inner) => inner.len(),
            McPhrase::Parallel(stmts) => stmts
                .iter()
                .map(|l| self.member_lane_width(l))
                .max()
                .unwrap_or(1),
            McPhrase::Transposed(_) => {
                // Transposed 2-pin components expose each pin as a lane
                2
            }
            // `^` is a view of the same expression, so it presents exactly
            // the lanes its operand does.
            McPhrase::Reversed(inner) => self.member_lane_width(inner),
            // ── M11.5: handle Bus with multiple members as multi-lane ──
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref bus),
                ..
            })) if !bus.member.is_empty() => bus.member.len(),
            // ── model A §5.3 whole-DC-pair curly face ──
            // A Node (`ldo{VIN | VOUT}`) spans its input face's [hot, ret]
            // lanes (collect_one_lane_item mirrors this), so a lane chain
            // containing one is sized by the face width, not 1.
            McPhrase::Endpoint(McEndpoint::Node { input, .. }) => Self::node_face_lanes(input),
            _ => 1,
        }
    }

    /// Number of lane slots a curly-face Node exposes on one side. A face is a
    /// list of bus refs; a whole-DC-pair face (`bk{VIN | VOUT}`) is usually ONE
    /// ref whose Bus carries the port's members (`Bus(bk{VIN.Vin, VIN.GND})`),
    /// so the count is the sum of each ref's bus-member count — matching the
    /// points `get_left_points`/`get_right_points` resolve for the face.
    fn node_face_lanes(face: &[McEndpoint]) -> usize {
        let mut n = 0usize;
        for f in face {
            match f {
                McEndpoint::Single(McInstanceRef {
                    base: McInstance::Bus(ref bus),
                    ..
                }) if !bus.member.is_empty() => n += bus.member.len(),
                _ => n += 1,
            }
        }
        n.max(1)
    }

    /// Pick the lane-specific point from a list of points.
    /// For multi-pin endpoints (e.g. XTAL interface with 2 pins),
    /// get_left_points/get_right_points returns all points. This helper
    /// picks the point at index `lane`, falling back to index 0 if the
    /// lane index is out of bounds.
    pub(super) fn pick_lane_point(pts: &[NetPoint], lane: usize) -> Option<NetPoint> {
        if pts.is_empty() {
            None
        } else if lane < pts.len() {
            Some(pts[lane].clone())
        } else {
            Some(pts[0].clone())
        }
    }

    // ── M11.4: collect lane items (series elements + bridge pins) preserving order ──
    // Returns each item tagged with the **member index** that produced it, so
    // the caller can map a lane element back to its member-boundary gap dir
    // (a member may be skipped on a lane — e.g. a Lead `_` — so the element
    // index within a lane is not the member index).
    pub(super) fn collect_lane_items<'a>(
        &mut self,
        members: &'a [McPhrase],
        lane: usize,
    ) -> Vec<(usize, LaneItem<'a>)> {
        let mut items: Vec<(usize, LaneItem<'a>)> = Vec::new();
        for (member_idx, member) in members.iter().enumerate() {
            self.collect_one_lane_item(member, member_idx, lane, &mut items);
        }
        items
    }

    fn collect_one_lane_item<'a>(
        &mut self,
        member: &'a McPhrase,
        member_idx: usize,
        lane: usize,
        items: &mut Vec<(usize, LaneItem<'a>)>,
    ) {
        match member {
            McPhrase::Multiple(inner) => {
                if lane < inner.len() {
                    let p = &inner[lane];
                    if !matches!(p, McPhrase::Lead) {
                        items.push((member_idx, LaneItem::Series(p)));
                    }
                }
            }
            McPhrase::Parallel(stmts) => {
                for (stmt_idx, stmt) in stmts.iter().enumerate() {
                    match stmt {
                        McPhrase::Multiple(inner) => {
                            if lane < inner.len() {
                                let p = &inner[lane];
                                if !matches!(p, McPhrase::Lead) {
                                    items.push((member_idx, LaneItem::Series(p)));
                                }
                            }
                        }
                        McPhrase::Transposed(inner) => {
                            if let Some(pin) = self.get_transposed_lane_pin(stmt, lane) {
                                items.push((member_idx, LaneItem::Bridge(pin)));
                            }
                            self.try_record_bridge_passive(inner);
                        }
                        _ => {
                            // ── P2-7-XTAL fix: assign each Parallel stmt to its matching lane ──
                            // In lane-by-lane wiring, a Parallel group like [CAP1, CAP2]
                            // provides one element per lane. Previously, lane==0 captured
                            // all stmts, causing duplicate component creation and leaving
                            // other lanes without their assigned elements.
                            if lane == stmt_idx {
                                items.push((member_idx, LaneItem::Series(stmt)));
                            }
                        }
                    }
                }
            }
            McPhrase::Transposed(inner) => {
                // M11.4: standalone Transposed in chain acts as bridge passive
                if let Some(pin) = self.get_transposed_lane_pin(member, lane) {
                    items.push((member_idx, LaneItem::Bridge(pin)));
                }
                self.try_record_bridge_passive(inner);
            }
            McPhrase::Group(g) => {
                // M11.4: expand Group's opds per lane, same as Multiple.
                // Each opd is a lane item (e.g. (RES(),RES()) gives RES1 to lane 0,
                // RES2 to lane 1). Lead (_) elements are skipped.
                if let Some(p) = g.opds.get(lane) {
                    if !matches!(p, McPhrase::Lead) {
                        items.push((member_idx, LaneItem::Series(p)));
                    }
                }
            }
            // ── whole-DC-pair curly face (model A §5.3) ──────────────────
            // `ldo{VIN | VOUT}` / `buck{VIN | LX}` expands to a Node whose
            // input/output faces are the port's [hot, ret] member refs,
            // spanning `node_face_lanes` lanes. The default arm below would
            // place it on lane 0 only, so a lane-series right side
            // (`- [IND(2.2uH), _] ->`, golden main.mc buck12) wired the hot
            // lane but silently dropped the return member — the device's GND
            // pin never reached the return net (4116, zero explicit error).
            // A Node must appear on every lane of its face: per-lane left /
            // right points are the input / output face members in order
            // (get_left_points / get_right_points → resolve_curly_mn_points),
            // so the shared return participates on the return lane.
            McPhrase::Endpoint(McEndpoint::Node { input, .. }) => {
                if lane < Self::node_face_lanes(input) {
                    items.push((member_idx, LaneItem::Series(member)));
                }
            }
            // ── P2-7: bus endpoint (e.g. XTAL interface with 2 pins) ──
            // Treat as multi-lane series element: each lane gets its own pin.
            // The lane-specific pin is picked in vexpr_lane_chain via
            // pick_lane_point.
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref bus),
                ..
            })) if !bus.member.is_empty() => {
                if lane < bus.member.len() {
                    items.push((member_idx, LaneItem::Series(member)));
                }
            }
            _ => {
                if lane == 0 {
                    items.push((member_idx, LaneItem::Series(member)));
                }
            }
        }
    }

    // ── M11.2: get the pin for a specific lane from a Transposed member ──
    fn get_transposed_lane_pin(&mut self, member: &McPhrase, lane: usize) -> Option<NetPoint> {
        let pts = self.get_left_points(member).unwrap_or_default();
        if lane < pts.len() {
            Some(pts[lane].clone())
        } else {
            None
        }
    }

    /// Record bridge passive instance names from a Transposed inner phrase.
    /// Same logic as the Transposed branch in process_member_internal (lines 1582-1589).
    fn try_record_bridge_passive(&mut self, inner: &McPhrase) {
        // For Endpoint(Component(c)), the component was instantiated during Pass1
        // and c.name is already the instance name (e.g. "@CAP_2"). Use it directly
        // instead of auto_inst_map (which is only populated for FuncCall paths).
        if let McPhrase::Endpoint(McEndpoint::Single(iref)) = inner {
            if let McInstance::Component(c) = &iref.base {
                let inst_name = c.name.to_string();
                self.bridge_passive_names.insert(inst_name);
                return;
            }
        }
        // Fallback: try auto_inst_map (for FuncCall inner, though this shouldn't
        // normally happen since Transposed with FuncCall inner is handled
        // separately in process_member_internal).
        let key = Self::member_key(inner);
        if let Some(auto) = self.auto_inst_map.get(&key).cloned() {
            let names: Vec<String> = auto.instance_names().map(str::to_string).collect();
            self.bridge_passive_names.extend(names);
        }
    }

    /// Is `name` (of the form `owner.port`) a same-name component pin group?
    ///
    /// A same-name group is a bus port whose resolved pins ALL carry the same
    /// member name — either all empty (`spk{GND}`) or all explicitly identical
    /// (`ps [19,32,48,64] = VDD` → every pad named "VDD"). In vector circuits a
    /// same-name pin is taken once, not once per pad (same-name-pin-group.md §2):
    /// it is a single logical net and counts as ONE lane in shape computation
    /// (vec-dianlu.md §5.2). Distinct-member buses (SPI.CS / SPI.SCLK / …) are
    /// NOT same-name groups and stay multi-lane. `name` that isn't a component
    /// port at all returns false.
    fn is_same_name_component_group(&self, name: &str) -> bool {
        let Some((owner, port)) = name.split_once('.') else {
            return false;
        };
        if port.contains('.') {
            return false;
        }
        let Some(comp) = self.find_component(owner) else {
            return false;
        };
        let Some(pids) = comp.find_bus_port_pin_ids(port) else {
            return false;
        };
        if pids.len() < 2 {
            return false;
        }
        let first = &pids[0].0;
        pids.iter().all(|(m, _)| m == first)
    }

    /// Convert McPhrase to expanded McPhrase list (flat member projection).
    /// Series is recursively expanded to individual member McPhrases.
    /// Directions are dropped: this is the member-only view used by the
    /// ~13 non-`process_stmt` call sites. Edge-level directions live in
    /// `phrase_to_members_gapped` (Series arm).
    pub(super) fn phrase_to_members(&self, phrase: &McPhrase) -> Vec<McPhrase> {
        self.phrase_to_members_gapped(phrase).0
    }

    /// Convert McPhrase to expanded McPhrase list **with edge-level directions**.
    ///
    /// Returns `(members, gaps)` where `gaps[i]` is the operator direction
    /// connecting `members[i]`~`members[i+1]` (members in **written source
    /// order**, R0 §2.4.5 corollary; a nested Series whose direction differs
    /// from its parent contributes its own internal gaps, see the Series arm
    /// below). For any non-Series phrase there is no serial operator direction,
    /// so members carry `ConnDir::Undirected` gaps (single-member → empty).
    pub(super) fn phrase_to_members_gapped(
        &self,
        phrase: &McPhrase,
    ) -> (Vec<McPhrase>, Vec<ConnDir>) {
        let disc = std::mem::discriminant(phrase);
        mcc_dbg!(
            "inst::mod",
            "[P2-5-PTM-ENTRY] module='{}' phrase_to_members: disc={disc:?}",
            self.name
        );
        match phrase {
            McPhrase::Series(phrases, d) => {
                // ── P1-B ────────────────────────────────────────────────
                // Don't flatten Multiple inside Series into chain — that would
                // turn `MIC{P,N} -> cap[4:5] -> uC.ADC{P,N}` "both ends N-wide,
                // middle N parallel branches" pattern, incorrectly into cap4→cap5
                // serial chain. Keep Multiple as **single chain member**,
                // its get_left/get_right aggregates all branch endpoints as
                // multi-point side, handled by create_connection N-to-N paired wiring.
                //
                // ── Iter-6.S5.2 P0-2 (B + C) ───────────────────────────
                // But **just keeping Multiple shell isn't enough** — inner phrase is still
                // parser raw AST form (`Single(Component)` / `Single(Label)`
                // / `Single(Interface)` …). These forms in points.rs
                // `get_left_points` directly fall to line 286-290 fallback:
                //
                //     | McInstance::Label / List / Interface / Component
                //     | / Module => Ok(vec![]),
                //
                // returns **empty NetPoint list**, causing entire chain adjacency at Multiple
                // side size=0, connections swallowed.
                //
                // Verified hit cases (from 5.2-diag):
                //   - `[VDD_3V3, GND] -> dcdc{Vin, GND}` (power.mc:101)
                //     → `Multiple([Label(VDD_3V3), Label(GND)])`, L_size=0
                //   - `MIC{P,N} -> cap[4:5]::CAP(1uF) -> uC.ADC{P,N}` (main.mc:147)
                //     → `Multiple([Component(@CAPx), Component(@CAPy)])`,
                //     L_size=0 / R_size=0 → cap4/cap5 isolated
                //   - `RES(10kΩ) -> [lpa.EN, US_SPEAKER_MUTE]` (periph.mc:104)
                //     → similar, only reaches first inner
                //
                // Fix: after entering Multiple, recursively call `self.phrase_to_members`
                // to standardize each inner item (Component → Node form,
                // Label → Bus form, Interface → Bus form…), then **still wrap whole
                // back into Multiple**, preserving P1-B wide-vs-narrow chain semantics.
                //
                // Note: phrase_to_members for inner may return multiple phrases
                // (e.g., inner is Series gets flattened), so use `flat_map`
                // to collect — this is exactly what we want (flattened to several phrases sharing
                // same Multiple wrapper).
                // ── edge-level direction gaps ────────────────────────────
                // `gaps[i]` is the operator direction connecting
                // `result[i]`~`result[i+1]`. Each top-level child is one
                // operand: the boundary between consecutive operands carries
                // this Series' own direction `d`. A `Multiple` child counts as
                // **one** member (no internal gap — its lanes are parallel, not
                // serial, P1-B above). A nested `Series` child (parser invariant:
                // only appears when its direction differs from `d`) contributes
                // its own internal gaps through the gapped recursion.
                let mut result = Vec::new();
                let mut gaps: Vec<ConnDir> = Vec::new();
                for p in phrases {
                    match p {
                        McPhrase::Multiple(inner) => {
                            let transformed_inner: Vec<McPhrase> =
                                self.normalize_multiple_lanes(inner);
                            if !result.is_empty() {
                                gaps.push(*d);
                            }
                            result.push(McPhrase::Multiple(transformed_inner));
                        }
                        _ => {
                            let (sub, sub_gaps) = self.phrase_to_members_gapped(p);
                            if !sub.is_empty() {
                                if !result.is_empty() {
                                    gaps.push(*d);
                                }
                                result.extend(sub);
                                gaps.extend(sub_gaps);
                            }
                        }
                    }
                }

                // ── Iter-6.S5.1 P0-2 scenario C: the "curly-split" fix-up ──
                // A pre-pass (`merge_adjacent_curly_split`) used to live here. It
                // merged two adjacent same-name single-member Bus phrases into
                // one, to repair a parser defect where `MIC{P,N}` at statement
                // start came out as `[Bus(MIC,[P]), Bus(MIC,[N])]` — which the
                // adjacency wiring then shorted together.
                //
                // It has been **retired** (R0 A6, `vec-dianlu.md` §2.4, ban 2:
                // an operator is encoded, never rewritten).
                // Two reasons:
                //
                // * It rewrote the tree without looking at `gaps`, so whenever the
                //   user *did* write an operator between two same-name
                //   single-member Buses the operator was erased together with the
                //   boundary — `R101.1 -> R101.2` and `vout.VCC -> vout.GND` were
                //   silently merged into one Bus and the `->` disappeared.
                // * The parser defect it repaired no longer reproduces: probing
                //   every real board (hbl / pwrint / hs / hbl1) found **no**
                //   adjacent same-name Bus pair at all, and no synthetic
                //   `Name{a,b}` form splits either. Multi-member Buses are built
                //   upstream and reach the `Endpoint(Bus)` arms below (and the
                //   Label arms) intact.
                //
                // So the surgical fix-up outlived its defect and all that was left
                // was the operator erasure. `R101.1 -> R101.2` is now an ordinary
                // chain again.

                // ── M11.5: expand multi-member Buses to Multiple ──
                // Buses like dc{VDD_3V3, GND}
                // may have multiple members.  Expand them to Multiple so
                // lane-by-lane wiring can handle each lane independently.
                // Count-neutral (remove+insert one member), so gap indices stay aligned.
                Self::expand_multi_member_buses(&mut result);

                debug_assert_eq!(gaps.len(), result.len().saturating_sub(1));
                (result, gaps)
            }
            McPhrase::Parallel(phrases) => (vec![McPhrase::Parallel(phrases.clone())], Vec::new()),
            McPhrase::Closure(c) => (vec![McPhrase::Closure(c.clone())], Vec::new()),
            McPhrase::FuncCall(f) => (vec![McPhrase::FuncCall(f.clone())], Vec::new()),
            McPhrase::Group(g) => (vec![McPhrase::Group(g.clone())], Vec::new()),
            McPhrase::Transposed(inner) => (
                vec![McPhrase::Transposed(Box::new((**inner).clone()))],
                Vec::new(),
            ),
            // §2.4.5: `^` is a view, not a tree rewrite. Walk the operand's
            // chain the other way — the member list reverses and every
            // directed gap flips — and re-wrap each member so its own faces
            // swap too (get_left_points / get_right_points read through the
            // wrapper). That re-wrap is what lets a **single**-member operand
            // such as `mcu{A, B | C, D}^` reverse at all; a no-op operand
            // (parallel / transposed) passes through unchanged because the
            // wrapper's face accessors are no-ops for it.
            McPhrase::Reversed(inner) => {
                let (members, gaps) = self.phrase_to_members_gapped(inner);
                let members: Vec<McPhrase> = members
                    .into_iter()
                    .rev()
                    .map(|m| McPhrase::Reversed(Box::new(m)))
                    .collect();
                let gaps: Vec<ConnDir> = gaps.into_iter().rev().map(ConnDir::flipped).collect();
                (members, gaps)
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(c),
                members,
            })) => {
                let inst_name = c.name.to_string();

                // ── P0-1 fix ──────────────────────────────────────────────
                // If user explicitly wrote member access (e.g., `dcdc{Vin, GND}` or
                // `wm7121{2,3}`), expand these members into Bus.member, letting downstream
                // get_left_points / get_right_points expand bus-to-bus.
                //
                // Otherwise (bare component reference like `R1` / `C1`), still use pin count heuristic:
                //   0/1 pin → single-point Bus
                //   2 pin   → 2-pin Node (left=.1, right=.2)
                //   multi-pin → single-point Bus (fallback, pin handling delegated to FuncCall/declaration)
                let expanded: Vec<String> = members.iter().flat_map(|ml| ml.expand()).collect();
                if !expanded.is_empty() {
                    return (
                        vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(&inst_name, expanded)),
                        )))],
                        Vec::new(),
                    );
                }

                // ── Two-pin determination (def-driven) ───────────────────
                // Decide from the class's *real* pin count — static pins plus
                // any dynamic range resolved against this instance's bound
                // params — instead of the old class-name whitelist / @-anon
                // guess (system CAP/RES used to be dynamic-pin and needed the
                // name list; they are static now and hit `(2, _)` directly).
                // Static count 2 is authoritative; a dynamic-pin class is
                // two-pin only when it really resolves to 2 (this also stops
                // forcing every @-anonymous dynamic instance into a 2-pin
                // Node); a dynamic count that can't be bound here is *not*
                // guessed as two-pin by name — it falls to the single-point
                // Bus, where the component's real pins are wired later.
                let static_count = c.base.pins.count();
                let two_pin = match c.resolved_pin_count() {
                    Some(2) => true,
                    Some(_) => false,
                    None => false,
                };

                match (static_count, two_pin) {
                    (2, _) | (_, true) => (
                        vec![McPhrase::Endpoint(McEndpoint::Node {
                            input: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                McBus::new(&format!("{inst_name}.1")),
                            )))],
                            output: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                McBus::new(&format!("{inst_name}.2")),
                            )))],
                        })],
                        Vec::new(),
                    ),
                    _ => (
                        vec![McPhrase::from(McInstance::Bus(McBus::new(&inst_name)))],
                        Vec::new(),
                    ),
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(m),
                members,
            })) => {
                let inst_name = m.name.to_string();

                // ── P1-A1b ───────────────────────────────────────────────
                // User explicit member access `speaker{DAC_IN, US_SPEAKER_MUTE}`:
                // Note **cannot** directly return `Bus(name, members)` — `get_left_points`
                // Bus branch `Vec::from(mcbus)` in member.len()==2 special path
                // would clear `member` field, resulting in `speaker{DAC_IN, US_SPEAKER_MUTE}`
                // collapsed back to scalar `speaker` sharing the chain's other-side net.
                //
                // Changed to return `Endpoint::Node`, which in `get_left_points` goes through
                // resolve_curly_mn_points, that path stably returns `speaker.DAC_IN` /
                // `speaker.US_SPEAKER_MUTE` as independent NetPoints with owner.
                //
                // Port iotype looked up from declared submodule instance `self.sub_modules`:
                //   - In / InOut  → input  side
                //   - Out / InOut → output side
                // Members not found (e.g., module not declared or pass2 not yet instantiated), put on
                // input side as fallback.
                let expanded: Vec<String> = members.iter().flat_map(|ml| ml.expand()).collect();
                if !expanded.is_empty() {
                    let sub_opt = self.find_submodule(&inst_name);
                    let mut input: Vec<McEndpoint> = Vec::new();
                    let mut output: Vec<McEndpoint> = Vec::new();
                    for m_name in &expanded {
                        let path = format!("{inst_name}.{m_name}");
                        let ep = McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            McBus::new(&path),
                        )));
                        let iotype = sub_opt
                            .as_ref()
                            .and_then(|s| s.ports.iter().find(|p| p.name == *m_name))
                            .map(|p| p.iotype.clone())
                            .unwrap_or(IOType::None);
                        match iotype {
                            IOType::In => input.push(ep),
                            IOType::Out => output.push(ep),
                            IOType::InOut => {
                                input.push(ep.clone());
                                output.push(ep);
                            }
                            _ => input.push(ep),
                        }
                    }
                    return (
                        vec![McPhrase::Endpoint(McEndpoint::Node { input, output })],
                        Vec::new(),
                    );
                }

                // ── P1-A2 ────────────────────────────────────────────────
                // Bare module reference `V3V3 -> dcdc -> V1V2`: need to split module into
                // Node (in side / out side), so `dcdc` two sides don't get
                // union-find merged into one big net.
                //
                // Prefer declared submodule instance ports (pass2 reliable data),
                // m.base.insts is empty on some parse paths, can't rely on it.
                let (left, right): (Vec<McBus>, Vec<McBus>) =
                    if let Some(sub) = self.find_submodule(&inst_name) {
                        let lp: Vec<McBus> = sub
                            .ports
                            .iter()
                            .filter(|p| matches!(p.iotype, IOType::In | IOType::InOut))
                            .map(|p| McBus::new(&format!("{}.{}", inst_name, p.name)))
                            .collect();
                        let rp: Vec<McBus> = sub
                            .ports
                            .iter()
                            .filter(|p| matches!(p.iotype, IOType::Out | IOType::InOut))
                            .map(|p| McBus::new(&format!("{}.{}", inst_name, p.name)))
                            .collect();
                        (lp, rp)
                    } else {
                        let l: Vec<McBus> = m
                            .base
                            .insts
                            .get_all_inputs()
                            .iter()
                            .map(|p| p.to_node_element_with_prefix(&inst_name))
                            .collect();
                        let r: Vec<McBus> = m
                            .base
                            .insts
                            .get_all_outputs()
                            .iter()
                            .map(|p| p.to_node_element_with_prefix(&inst_name))
                            .collect();
                        (l, r)
                    };

                if left.is_empty() && right.is_empty() {
                    (
                        vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(&inst_name)),
                        )))],
                        Vec::new(),
                    )
                } else {
                    (
                        vec![McPhrase::Endpoint(McEndpoint::Node {
                            input: left
                                .iter()
                                .map(|bus| {
                                    McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                        bus.clone(),
                                    )))
                                })
                                .collect(),
                            output: right
                                .iter()
                                .map(|bus| {
                                    McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                        bus.clone(),
                                    )))
                                })
                                .collect(),
                        })],
                        Vec::new(),
                    )
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(i),
                members,
            })) => {
                let inst_name = i.name.to_string();

                // ── P0-2 fix ──────────────────────────────────────────────
                // Interface class label defaults to "single net label" handling (same as Label).
                // No longer auto-expand to `.1/.2` just because "interface has 2 pins" — that breaks
                // `V5V::DC(5V)` "attach interface type to label" top-level usage.
                //
                // Only expand when user **explicitly** uses `{m1, m2}` syntax to access certain members.
                let expanded: Vec<String> = members.iter().flat_map(|ml| ml.expand()).collect();
                if !expanded.is_empty() {
                    return (
                        vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(&inst_name, expanded)),
                        )))],
                        Vec::new(),
                    );
                }

                (
                    vec![McPhrase::from(McInstance::Bus(McBus::new(&inst_name)))],
                    Vec::new(),
                )
            }
            McPhrase::Lead => (vec![McPhrase::Lead], Vec::new()),
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref data),
                ..
            })) => {
                mcc_dbg!(
                    "inst::mod",
                    "[P2-5-BUS-ENTRY] module='{}' phrase_to_members Bus: name='{}', member={:?}",
                    self.name,
                    data.name,
                    data.member
                );
                // ── M11.5: expand multi-member Bus to Multiple ──────────────
                // When a Bus has multiple members (e.g. dc{VDD_3V3, GND}),
                // expand to Multiple so lane-by-lane wiring can handle each
                // lane independently.  Single-member buses stay as-is.
                //
                // ── P2-5: also check bus table for named buses (e.g. I2C0) ──
                // When data.member is empty but the bus table has members,
                // expand using the bus table members.
                let members: Vec<String> = if data.member.len() > 1 {
                    data.member.clone()
                } else if data.member.is_empty() && !data.name.is_empty() {
                    let from_bus = self
                        .buses
                        .get(&data.name)
                        .map(|b| b.members.clone())
                        .unwrap_or_default();
                    mcc_dbg!("inst::mod", 
                        "[P2-5-BUS-LOOKUP] module='{}' bus='{}' data.member={:?} from_bus_table={:?}",
                        self.name, data.name, data.member, from_bus
                    );
                    from_bus
                } else {
                    Vec::new()
                };

                if members.len() > 1 {
                    // ── Same-name component pin group: ONE lane, not N ──
                    // `U1B.VDD` with members [19,32,48,64] are the physical pads
                    // of a same-name pin group (`ps [19,32,48,64] = VDD`): every
                    // pad carries the same member name "VDD". In vector circuits
                    // a same-name pin is taken once, not once per pad
                    // (same-name-pin-group.md §2) — that is the basic rule for
                    // shape computation (vec-dianlu.md §5.2). Do NOT expand to a
                    // Multiple of pin lanes; keep the bare Bus so point resolution
                    // routes it through expand_port_lanes, which collapses the
                    // group to a single logical point carrying the pads.
                    if self.is_same_name_component_group(&data.name) {
                        return (
                            vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(McBus::new(&data.name)),
                            )))],
                            Vec::new(),
                        );
                    }
                    mcc_dbg!(
                        "inst::mod",
                        "[P2-5-BUS] module='{}' expanding bus '{}' to Multiple with members {:?}",
                        self.name,
                        data.name,
                        members
                    );
                    let inner: Vec<McPhrase> = members
                        .iter()
                        .map(|m| {
                            // P2-6: when bus name is empty (anonymous DC bus),
                            // use member name directly without dot prefix.
                            // e.g. [VDD_3V3,GND]::DC() → VDD_3V3, GND (not .VDD_3V3, .GND)
                            let path = if data.name.is_empty() {
                                m.clone()
                            } else {
                                format!("{}.{}", data.name, m)
                            };
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(McBus::new(&path)),
                            )))
                        })
                        .collect();
                    (vec![McPhrase::Multiple(inner)], Vec::new())
                } else {
                    (
                        vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(data.clone()),
                        )))],
                        Vec::new(),
                    )
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(label),
                ..
            })) => {
                // ── P2-5 fix: parse curly bracket members from label name ──
                // When a label like `USB_VBUS_1{VDD_3V, GND}` is parsed,
                // the curly bracket members are part of the label name.
                // Extract them and create a Multiple so lane-by-lane wiring
                // can handle each lane independently.
                if let Some(open) = label.find('{') {
                    if label.ends_with('}') {
                        let base = &label[..open];
                        let members_str = &label[open + 1..label.len() - 1];
                        let members: Vec<String> = members_str
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if members.len() > 1 {
                            // Preserve declaration order (no sorting) to match
                            // the order used by component/interface bus members.
                            let inner: Vec<McPhrase> = members
                                .iter()
                                .map(|m| {
                                    let path = format!("{}.{}", base, m);
                                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                        McInstance::Bus(McBus::new(&path)),
                                    )))
                                })
                                .collect();
                            return (vec![McPhrase::Multiple(inner)], Vec::new());
                        }
                    }
                }
                // ── P2-5: expand Label to Multiple when bus table has members ──
                let from_bus = self
                    .buses
                    .get(label)
                    .map(|b| b.members.clone())
                    .unwrap_or_default();
                if from_bus.len() > 1 {
                    mcc_dbg!("inst::mod", 
                        "[P2-5-BUS-LABEL] module='{}' expanding Label '{}' to Multiple with members {:?}",
                        self.name, label, from_bus
                    );
                    let inner: Vec<McPhrase> = from_bus
                        .iter()
                        .map(|m| {
                            let path = format!("{}.{}", label, m);
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(McBus::new(&path)),
                            )))
                        })
                        .collect();
                    (vec![McPhrase::Multiple(inner)], Vec::new())
                } else {
                    (
                        vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(label)),
                        )))],
                        Vec::new(),
                    )
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(list),
                ..
            })) => (
                vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::Bus(McBus::new_with_members(&list.name, list.member.clone())),
                )))],
                Vec::new(),
            ),
            McPhrase::Multiple(inner) => {
                // Top-level Multiple (not a Series child — those are handled by the
                // Series arm as a single member): flatten to members. A bare
                // Multiple statement has no serial operator, so every boundary is
                // Undirected (matches the pre-fix chain-level default for a
                // non-Series phrase).
                let result = self.normalize_multiple_lanes(inner);
                let gaps = vec![ConnDir::Undirected; result.len().saturating_sub(1)];
                (result, gaps)
            }
            McPhrase::Endpoint(ref ep) => {
                // ── §11.3 lane-structured List: N independent member lanes ──────
                // `cap[4:5]` resolves at pass1 to
                // `Endpoint(List([Single(cap4), Single(cap5), ...]))`. Do NOT
                // collapse through get_left/get_right — those take only the
                // first/last lane and turn the parallel lane group into a serial
                // `cap4 → cap5` Node (the flatten-before-zip pitfall). Pass
                // the List through so the
                // array-form re-link (resolve_array_caller_to_existing) and the
                // get_left/get_right_points List handlers consume the lanes
                // structurally.
                if matches!(ep, McEndpoint::List(_)) {
                    return (vec![McPhrase::Endpoint(ep.clone())], Vec::new());
                }
                mcc_dbg!("inst::mod",
                    "[P2-5-BUS-CATCHALL] module='{}' phrase_to_members Endpoint catch-all: ep={ep:?}",
                    self.name
                );
                let left = ep.get_left();
                let right = ep.get_right();
                if left.is_empty() && right.is_empty() {
                    (vec![McPhrase::Endpoint(ep.clone())], Vec::new())
                } else if left.len() == 1 && right.len() == 1 {
                    (
                        vec![McPhrase::Endpoint(McEndpoint::Node {
                            input: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                left[0].clone(),
                            )))],
                            output: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                right[0].clone(),
                            )))],
                        })],
                        Vec::new(),
                    )
                } else {
                    (vec![McPhrase::Endpoint(ep.clone())], Vec::new())
                }
            }
            McPhrase::Member(inner, member_ep) => {
                // ── P2-4 fix: keep Member for ALL cases, not just FuncCall ──
                // Previously only FuncCall inners kept the Member wrapper (e.g.
                // `X6.setup(GND, NC).XTAL`), while non-FuncCall inners like
                // `uC.XTAL` (Member(Endpoint(Component(uC)), "XTAL")) were stripped,
                // losing the XTAL member name and causing all XTAL pins to merge
                // into one net instead of lane-by-lane matching.
                (
                    vec![McPhrase::Member(inner.clone(), member_ep.clone())],
                    Vec::new(),
                )
            }
        }
    }

    /// Normalize the inner phrases (**lanes**) of a `Multiple`.
    ///
    /// Every lane normalizes to a single member, except two cases:
    ///
    /// * a nested `Multiple` concatenates into the lane list — a lane list of
    ///   lane lists is the same lane list (`get_left_points` of a `Multiple`
    ///   already recurses that way);
    /// * a `Series` lane stays **one lane**, recursively normalized. Flattening
    ///   it would be information destruction: `get_left_points` /
    ///   `get_right_points` read a chain as its first / last member
    ///   (`points.rs`), so a chain is a *single* lane with one face on each
    ///   side. Flattening would expose every member as its own lane face and,
    ///   with `gaps` dropped, erase the chain's internal operator directions
    ///   (R0 `vec-dianlu.md` §2.4.5 corollary — the member list is the written
    ///   order). A lane list is parallel, so there is no gap *between* lanes.
    fn normalize_multiple_lanes(&self, inner: &[McPhrase]) -> Vec<McPhrase> {
        let mut out = Vec::new();
        for ip in inner {
            match ip {
                McPhrase::Series(members, d) => {
                    out.push(McPhrase::Series(self.normalize_multiple_lanes(members), *d));
                }
                _ => out.extend(self.phrase_to_members(ip)),
            }
        }
        out
    }

    /// ── M11.5: expand multi-member Buses into Multiple ──────────────────
    /// Buses may have multiple members (e.g. `dc{VDD_3V3, GND}`).  Expand them
    /// to Multiple so lane-by-lane
    /// wiring can handle each lane independently.
    fn expand_multi_member_buses(members: &mut Vec<McPhrase>) {
        let mut i = 0;
        while i < members.len() {
            let should_expand = match &members[i] {
                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Bus(bus),
                    members,
                })) => {
                    let n = bus.member.len().max(bus.full_members.len());
                    n > 1 && members.is_empty()
                }
                _ => false,
            };
            if should_expand {
                let old = members.remove(i);
                if let McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Bus(bus),
                    ..
                })) = old
                {
                    let names: Vec<String> = if !bus.member.is_empty() {
                        bus.member.clone()
                    } else {
                        bus.full_members.clone()
                    };
                    let inner: Vec<McPhrase> = names
                        .iter()
                        .map(|m| {
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(McBus::member_ref(&bus.name, m.clone())),
                            )))
                        })
                        .collect();
                    members.insert(i, McPhrase::Multiple(inner));
                }
            }
            i += 1;
        }
    }

    /// ★ P9-A2: Extract the trunk group name from a McPhrase.
    ///
    /// For `flash.SPI` or `mic.MIC`, the trunk group is the Interface/Bus name
    /// (e.g., "SPI", "MIC"). Returns `None` for non-port-group phrases.
    fn extract_trunk_group(phrase: &McPhrase) -> Option<String> {
        let r = Self::extract_trunk_group_inner(phrase);
        mcc_dbg!(
            "inst::mod",
            "[PG-DBG] module={} phrase={:?} -> {:?}",
            "?",
            phrase,
            r
        );
        r
    }

    fn extract_trunk_group_inner(phrase: &McPhrase) -> Option<String> {
        match phrase {
            McPhrase::Endpoint(McEndpoint::Single(ref ir)) => {
                // For Endpoint, only use Interface/Bus base name or member name.
                // Do NOT use Label fallback — Label just means the instance name
                // (e.g. "speaker"), not a trunk group.
                Self::extract_pg_from_iref(ir, false)
            }
            // ★ P9-A2: McPhrase::Member(base, member) — e.g. mcu513.DAC_OUT
            // The member endpoint carries the trunk group name. Use Label fallback
            // because the member is stored as Label("DAC_OUT").
            McPhrase::Member(_base, McEndpoint::Single(ref ir)) => {
                Self::extract_pg_from_iref(ir, true)
            }
            // §8.9.6.7: `MIC{P,N}` expands (M11.5 expand_multi_member_buses or
            // the parser's dot_or_curly) into a Multiple of per-member bus
            // endpoints. Two shapes arrive here:
            //   Form A — member carried separately: Bus{name:"MIC", member:["P"]}
            //   Form B — flattened dotted path:      Bus{name:"MIC.P", member:[]}
            // The group is the shared base name ("MIC"); both forms reduce to
            // it (form B by dropping the last dot segment — §8.9.6.3 form 1
            // member access). Require a consistent group across all endpoints
            // so a mixed net does not get a false trunk.
            McPhrase::Multiple(items) => {
                let groups: Vec<String> = items
                    .iter()
                    .filter_map(|it| match it {
                        McPhrase::Endpoint(McEndpoint::Single(ir)) => {
                            Self::extract_pg_from_multiple_endpoint(ir)
                        }
                        _ => None,
                    })
                    .collect();
                let first = groups.first()?;
                if groups.iter().all(|g| g == first) {
                    Some(first.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// §8.9.6.7: group identity of one endpoint inside a `Multiple`.
    /// An interface endpoint uses its interface name; a bus endpoint
    /// contributes when it carries member(s) (form A) or is a flattened
    /// dotted member path (form B). Labels, funcalls and bare scalar buses
    /// contribute nothing.
    fn extract_pg_from_multiple_endpoint(
        ir: &crate::semantic::basic::mc_endpoint::McInstanceRef,
    ) -> Option<String> {
        match &ir.base {
            McInstance::Interface(_) => Self::extract_pg_from_iref(ir, false),
            McInstance::Bus(b) => {
                if !b.member.is_empty() || !b.full_members.is_empty() {
                    Some(b.name().to_string())
                } else {
                    b.name()
                        .rsplit_once('.')
                        .map(|(base, _member)| base.to_string())
                }
            }
            _ => None,
        }
    }

    /// Extract trunk group name from an McInstanceRef.
    /// `use_label_fallback`: if true, fall back to Label name when no Interface/Bus/member.
    fn extract_pg_from_iref(
        ir: &crate::semantic::basic::mc_endpoint::McInstanceRef,
        use_label_fallback: bool,
    ) -> Option<String> {
        // First check if the base is an Interface or Bus
        let base_name = match &ir.base {
            McInstance::Interface(i) => i.name.segments.first().and_then(|seg| match seg {
                crate::semantic::basic::mc_ids::IdsSegment::Ida(ida) => Some(ida.to_string()),
                crate::semantic::basic::mc_ids::IdsSegment::DotIda(ida) => Some(ida.to_string()),
                _ => None,
            }),
            McInstance::Bus(b) => {
                // §8.9.6.3 form 1: a curly bus `MIC{P,N}` groups by its bus
                // NAME; the members are lanes (stamped per-lane downstream,
                // see group.rs::refine_lane_trunk). The old join("_")
                // conflated members into one pseudo-name and lost the group
                // identity.
                Some(b.name().to_string())
            }
            _ => None,
        };
        if base_name.is_some() {
            return base_name;
        }
        // For Module/Component endpoints like `mcu513.MIC`,
        // use the first member name as the trunk group.
        if let Some(ml) = ir.members.first() {
            if let Some(m) = ml.items.first() {
                if let crate::semantic::basic::mc_endpoint::McMember::Single(s) = m {
                    return Some(s.clone());
                }
            }
        }
        // Fallback: if members is empty, use the base label name
        // (e.g. McPhrase::Member(_, Label("DAC_OUT")) → "DAC_OUT")
        if use_label_fallback {
            if let McInstance::Label(s) = &ir.base {
                return Some(s.clone());
            }
        }
        None
    }

    /// §5.2 / §8.9.6.7: the literal spelling of a *written scalar pair list*
    /// (`[V5V, GND]`), when the member is exactly that — at least two distinct
    /// bare net names and no bus / interface group identity.
    ///
    /// The spelling is rebuilt from the AST names (never from a rendered
    /// display string) so it matches the source: `[V5V, GND]`.
    fn written_pair_name(member: &McPhrase) -> Option<String> {
        let McPhrase::Multiple(items) = member else {
            return None;
        };
        if items.len() < 2 {
            return None;
        }
        let mut names: Vec<String> = Vec::with_capacity(items.len());
        for it in items {
            let McPhrase::Endpoint(McEndpoint::Single(ir)) = it else {
                return None;
            };
            // Bare scalar only: a member-carrying bus (`MIC{P,N}`) or a dotted
            // path carries group identity of its own and is not a plain pair.
            let name = match &ir.base {
                McInstance::Bus(b)
                    if b.member.is_empty()
                        && b.full_members.is_empty()
                        && !b.name().contains('.') =>
                {
                    b.name().to_string()
                }
                McInstance::Label(label) if !label.contains('.') => label.clone(),
                _ => return None,
            };
            names.push(name);
        }
        // Two *distinct* nets: a repeated member is a lane list, not a pair.
        let mut distinct = names.clone();
        distinct.sort();
        distinct.dedup();
        if distinct.len() < 2 {
            return None;
        }
        Some(format!("[{}]", names.join(", ")))
    }

    /// The written DC pair at one end of a chain names that chain's trunk,
    /// provided some member of `others` reaches an authoritative power
    /// terminal (`[V5V, GND]` beside `USB.vin`). `None` when the end is not a
    /// plain pair or the far end is not a supply — both callers then fall back
    /// to the bus / interface group derivation.
    fn end_pair_trunk(&self, end: &McPhrase, others: &[&McPhrase]) -> Option<String> {
        let pair = Self::written_pair_name(end)?;
        others
            .iter()
            .any(|m| self.has_power_terminal(m))
            .then_some(pair)
    }

    /// Does this end member reach an authoritative power terminal — a module
    /// power port or a leaf component power pin in *this* module's scope? Used
    /// to tell a DC supply pair from an arbitrary net list. Two-face `Node`
    /// members are descended: `LDO{vin | vout}` names its terminals on the
    /// faces, not on the member itself.
    fn has_power_terminal(&self, member: &McPhrase) -> bool {
        let mut refs: Vec<(String, Vec<String>)> = Vec::new();
        Self::member_refs_deep(member, &mut refs);
        refs.iter().any(|(owner, tokens)| {
            tokens
                .iter()
                .any(|t| self.pwr_dir_of_ref(owner, t).is_some())
        })
    }

    /// `member_refs` plus two-face `Node` descent — `member_refs` stops at
    /// nodes because the 6028 audit judges those faces separately.
    fn member_refs_deep<'x>(member: &'x McPhrase, out: &mut Vec<(String, Vec<String>)>) {
        match member {
            McPhrase::Endpoint(ep) => Self::endpoint_refs_deep(ep, out),
            McPhrase::Multiple(inner) | McPhrase::Series(inner, _) | McPhrase::Parallel(inner) => {
                for p in inner {
                    Self::member_refs_deep(p, out);
                }
            }
            _ => {}
        }
    }

    fn endpoint_refs_deep<'x>(ep: &'x McEndpoint, out: &mut Vec<(String, Vec<String>)>) {
        match ep {
            McEndpoint::Single(iref) => {
                if let Some(t) = Self::iref_tokens(iref) {
                    out.push(t);
                }
            }
            McEndpoint::List(items) => {
                for e in items {
                    Self::endpoint_refs_deep(e, out);
                }
            }
            McEndpoint::Node { input, output } => {
                for e in input.iter().chain(output.iter()) {
                    Self::endpoint_refs_deep(e, out);
                }
            }
        }
    }

    /// ★ §8.9.4: Extract the coarse `TrunkKind` of a trunk group phrase, mirroring
    /// `extract_trunk_group`'s traversal so `Trunk.kind` never needs to be
    /// re-derived downstream.
    fn extract_trunk_kind(phrase: &McPhrase) -> Option<TrunkKind> {
        // §8.9.6.7: mirror of extract_trunk_group — `MIC{P,N}` appears as a
        // Multiple of member-carrying bus endpoints, so a Multiple contributes
        // a Bus/Interface kind only when at least one endpoint carries one
        // (the same member-carrying rule as the group extractor).
        if let McPhrase::Multiple(items) = phrase {
            return items.iter().find_map(|it| match it {
                McPhrase::Endpoint(McEndpoint::Single(ir)) => match &ir.base {
                    McInstance::Interface(_) => Some(TrunkKind::Interface),
                    McInstance::Bus(b) => {
                        if !b.member.is_empty()
                            || !b.full_members.is_empty()
                            || b.name().contains('.')
                        {
                            Some(TrunkKind::Bus)
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                _ => None,
            });
        }
        let ir = match phrase {
            McPhrase::Endpoint(McEndpoint::Single(ir)) => ir,
            McPhrase::Member(_base, McEndpoint::Single(ir)) => ir,
            _ => return None,
        };
        match &ir.base {
            McInstance::Interface(_) => Some(TrunkKind::Interface),
            McInstance::Bus(_) => Some(TrunkKind::Bus),
            _ => {
                // Member access (`mcu513.MIC`) is a bracket/list member; a bare
                // label fallback has no coarse identity.
                if !ir.members.is_empty() {
                    Some(TrunkKind::List)
                } else {
                    Some(TrunkKind::Plain)
                }
            }
        }
    }

    /// ★ §8.9.4: Extract the standardized interface class (e.g. `UART.TTL`)
    /// of an interface trunk group phrase, mirroring `extract_trunk_kind`'s
    /// traversal so `TrunkEnd.iface_class` never needs to be re-derived
    /// downstream. Non-interface phrases (bus / list / plain) yield `None`.
    ///
    /// Interface member lanes arrive flattened as a bus whose dotted member
    /// keeps the interface port name (e.g. `U_MCU{UART0.TX}`); the owner
    /// component's pin table resolves that port name back to the bound
    /// interface class (§8.9.4 data flow: `McPinPort::Interface` → name).
    fn extract_trunk_iface(&self, phrase: &McPhrase) -> Option<String> {
        if let McPhrase::Multiple(items) = phrase {
            return items.iter().find_map(|it| match it {
                McPhrase::Endpoint(McEndpoint::Single(ir)) => self.iface_class_of(ir),
                _ => None,
            });
        }
        let ir = match phrase {
            McPhrase::Endpoint(McEndpoint::Single(ir)) => ir,
            McPhrase::Member(_base, McEndpoint::Single(ir)) => ir,
            _ => return None,
        };
        self.iface_class_of(ir)
    }

    /// Interface class of one endpoint ref: a direct `McInstance::Interface`
    /// yields its bound class; a flattened bus member lane (`U_MCU{UART0.TX}`)
    /// resolves the dotted member's port segment against the owner
    /// component's pin table.
    fn iface_class_of(&self, ir: &McInstanceRef) -> Option<String> {
        match &ir.base {
            McInstance::Interface(i) => Some(i.base_name()),
            McInstance::Bus(b) => {
                let member = b.member.first().or_else(|| b.full_members.first())?;
                let port_name = member.split('.').next()?;
                if port_name.is_empty() {
                    return None;
                }
                let comp = self.find_component(b.name())?;
                match comp.def.pins.names_to_id.get(port_name) {
                    Some(McPinPort::Interface(iface)) => Some(iface.base_name()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Wire one adjacent pair of chain members (unified-core §7.3 L4).
    ///
    /// The `->` leg itself is no longer implemented here: the operand faces and
    /// the §5.2 pairing come from the fold
    /// ([`InstantiationBuilder::vexpr_fold_member`] /
    /// [`InstantiationBuilder::vexpr_step`]), which is the single
    /// implementation of the leg. What stays here is production
    /// **orchestration** — the trunk context and the `Group`-as-chain-member
    /// dispatch:
    ///
    /// - **Trunk.** Prefer the left (written-first) member, fall back to the
    ///   right (written-second). Members are in written source order and the
    ///   pairing is positional (R0), so the tie-break follows the written order
    ///   rather than any flow direction. RAII (§7.11(2)): the group is restored
    ///   on every exit path (including early `Err` returns), so it can never
    ///   leak into the next connection.
    ///
    ///   Declaration face first (§5.2): a written DC pair `[V5V, GND]` beside a
    ///   declared power terminal is itself the trunk — its literal spelling
    ///   names it. Without this the instance name on the *other* side leaks in
    ///   as the trunk (the parser carries `USB.vin` as `Bus{name:"USB",
    ///   member:["vin"]}`, so the port token is not the group identity, the
    ///   owner instance name is). A bus / interface group on the far side still
    ///   wins, and only the *name* is overridden.
    /// - **`Group`.** The fold deliberately has no `Group` arm: the law for
    ///   "Group as a chain member" is still an open semantic item (unified-core
    ///   §7.6 step 0 (3)), so that one shape is delegated to
    ///   [`InstantiationBuilder::connect_to_group`].
    ///
    /// Also re-links bracket-form array instance references (`cap[4:5] -> ...`)
    /// to the already-declared instances; see the re-link block in the body.
    fn connect_adjacent_pair(
        &mut self,
        left_member: &McPhrase,
        right_member: &McPhrase,
        dir: ConnDir,
    ) -> Result<(), InstError> {
        let trunk = self
            .end_pair_trunk(left_member, &[right_member])
            .or_else(|| self.end_pair_trunk(right_member, &[left_member]))
            .or_else(|| Self::extract_trunk_group(left_member))
            .or_else(|| Self::extract_trunk_group(right_member));
        let trunk_kind = Self::extract_trunk_kind(left_member)
            .or_else(|| Self::extract_trunk_kind(right_member))
            .or_else(|| trunk.as_ref().map(|_| TrunkKind::Plain));
        let trunk_iface = self
            .extract_trunk_iface(left_member)
            .or_else(|| self.extract_trunk_iface(right_member));
        self.with_trunk(trunk, trunk_kind, trunk_iface, |this| {
            // ── Array-form operands fall through to the general row gate below ──
            // Whole declared arrays in plain Series statements (`cap[4:5] -> PWR{VCC,GND}`,
            // `cap[4] -> GND`) are NOT re-linked member-by-member here. Each member is a
            // row of the array node, so the statement goes through the shared §5 row check
            // (get_right_points/get_left_points → check_series_rows → create_connection)
            // like any other operand: equal rows zip row-aligned; a member column against
            // a single point (or any unequal-row pair) is E4007 with no connection. The
            // old per-member re-link silently turned these into the abolished 1:N single-
            // point broadcast (member pins collapsed onto one rail, no diagnostic).
            // `resolve_array_caller_to_existing` is retained solely for the FuncCall
            // dispatch (`@@ARRAY`, below), where per-member invocation is the legal
            // iterated layer (vec-dianlu §7.6).
            if matches!(right_member, McPhrase::Group { .. }) {
                let external_points = this.get_right_points(left_member)?;
                this.connect_to_group(external_points, right_member, true, dir)?;
            } else if matches!(left_member, McPhrase::Group { .. }) {
                let external_points = this.get_left_points(right_member)?;
                this.connect_to_group(external_points, left_member, false, dir)?;
            } else {
                let left_opd = this.vexpr_fold_member(left_member)?;
                let right_opd = this.vexpr_fold_member(right_member)?;
                this.vexpr_step(&left_opd, &right_opd, dir)?;
            }
            Ok(())
        })
    }

    /// BUG4 helper: in-place process a Series in Group/Parallel branches ——
    /// keeps the FuncCall's original pointer (for auto_inst_map hit), and
    /// also does phrase_to_members Label→Bus upgrade for Label/List/Interface
    /// endpoints (otherwise get_*_points returns empty for bare Label →
    /// create_connection doesn't connect due to one side being empty, e.g.
    /// `GND` in `(CAP+RES) -> GND`, `VBUS -> USB_VBUS`).
    ///
    /// Key: cannot do whole-segment phrase_to_members (would clone FuncCall
    /// and change pointer). Here we judge element by element: FuncCall/
    /// Parallel/Group/Node use the **original reference**; Label/List/
    /// Interface use the upgraded **owned copy** (they resolve by name, not
    /// dependent on pointer).
    fn normalize_branch_elem(&self, e: &McPhrase) -> Option<McPhrase> {
        match e {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(_),
                ..
            })) => self.phrase_to_members(e).into_iter().next(),
            _ => None,
        }
    }

    pub(super) fn process_series_branch_inplace(
        &mut self,
        elems: &[McPhrase],
        dir: ConnDir,
    ) -> Result<(), InstError> {
        // 1) In-place instantiate each element (FuncCall registers in auto_inst_map on the original pointer)
        for e in elems {
            self.process_member_internal(e)?;
        }
        // 2) Adjacent wiring: for each pair, Label types use upgraded copy, others use original reference
        for k in 0..elems.len().saturating_sub(1) {
            let ln = self.normalize_branch_elem(&elems[k]);
            let rn = self.normalize_branch_elem(&elems[k + 1]);
            let lref: &McPhrase = ln.as_ref().unwrap_or(&elems[k]);
            let rref: &McPhrase = rn.as_ref().unwrap_or(&elems[k + 1]);
            if let Err(err) = self.connect_adjacent_pair(lref, rref, dir) {
                self.record_warning(
                    crate::errcodes::INST_ADJACENT_CONNECT_FAILED,
                    crate::errcodes::format_msg(
                        crate::errcodes::INST_ADJACENT_CONNECT_FAILED,
                        &[
                            &k as &dyn std::fmt::Display,
                            &(k + 1) as &dyn std::fmt::Display,
                            &err as &dyn std::fmt::Display,
                        ],
                    ),
                );
            }
        }
        Ok(())
    }

    /// Store the result of a PassThrough method expansion in auto_inst_map.
    /// `instantiate_instance_method` encodes the func's return value into
    /// LAST_RETURN_ENDPOINT (`@@RETURN_EP:` / `@@RETURN_NETS:`) when the func
    /// returns an endpoint; consume it here so later chain members resolve
    /// their face from the func's return value. Without this the plain
    /// instance name is stored and the return face degenerates to the
    /// instance's own pins (e.g. `XTAL4.Setup(...) -> [U2.XIN, U2.XOUT]`
    /// would see only the NC pin instead of the 2-lane XTAL{X1,X2} return).
    fn stash_pass_through(&mut self, key: u32, inst_name: &str) {
        let return_ep =
            super::fcallinst::LAST_RETURN_ENDPOINT.with(|cell| cell.borrow_mut().take());
        let entry = return_ep.unwrap_or_else(|| AutoInst::Name(inst_name.to_string()));
        self.auto_inst_map.insert(key, entry);
    }

    pub(super) fn process_member_internal(&mut self, phrase: &McPhrase) -> Result<(), InstError> {
        match phrase {
            McPhrase::Parallel(stmts) => {
                // ── P1-E1 ────────────────────────────────────────────────
                // Each item in Parallel is an independent stmt. Previously
                // here uniformly went through `self.process_stmt(stmt)`, but
                // process_stmt first calls phrase_to_members to clone stmt, then
                // does process_member_internal on the cloned elements —— the
                // auto_inst_map's key falls on the cloned address.
                //
                // Later, in the adjacency phase, get_left_points / get_right_points
                // access through the **original** `&stmt` again, the key is
                // unequal, auto_inst_map can't find it, P0-4 stub / component
                // instances are all lost. Typical symptom is `[DIO.ESD(), DIO.ESD()]`
                // such anonymous 2-pin element column all collapses into bare
                // `DIO` label and merges into a giant net.
                //
                // For "leaves" (single FuncCall / Endpoint etc.) directly call
                // process_member_internal, keeping the address of `&stmt`
                // unchanged. For composite nodes (Series / Parallel nesting)
                // still use process_stmt, because they themselves need adjacency
                // processing, and usually don't contain anonymous construction
                // calls that would trigger the stub mechanism.
                for stmt in stmts {
                    match stmt {
                        McPhrase::Series(elems, d) => {
                            // ── BUG4 fix (same as Group handler) ────────────────
                            // Originally process_stmt(clone) → FuncCall in Series
                            // is instantiated on the cloned pointer; but outer
                            // get_left_points(Parallel) → opds[0]=Series →
                            // get_left_points(&Series.elems[0]) uses original
                            // pointer to query auto_inst_map → MISS → RES.in leaks.
                            // (speaker periph.mc:97 `(RES(30kΩ)->lpa.VO1 + spk.N)`
                            //  where opds[0] is Series([RES_3, lpa.VO1]) this form.)
                            // Changed to in-place instantiate each element + internal
                            // adjacency (Label upgrade), keep FuncCall original pointer.
                            self.process_series_branch_inplace(elems, *d)?;
                        }
                        McPhrase::Parallel(_) => {
                            self.process_stmt(stmt)?;
                        }
                        _ => {
                            self.process_member_internal(stmt)?;
                        }
                    }
                }

                // ── §5.1 `+` internal wiring ────────────────────────────
                // `A + B + C` generates the nets that tie the written ends
                // together (rules §10.1): the left ends into the chain-entry
                // net and the right ends into the chain-exit net, split per
                // lane when the anchor is wider than one (`XTAL{X1, X2}` +
                // `R442::RES'` is a 2-lane zip). The `+` arm in points.rs exposes
                // only `opds[0]` to the outer chain (rules §10.1), so these
                // nets are the operand's internal attachment points, not its
                // external face.
                //
                // The law now lives in the fold (`vexpr/fold.rs`
                // `fold_parallel_chain`, driven by `vexpr_wire_parallel`) — the
                // same module that folds the `+` external face, so the operator
                // has one implementation for both halves.
                if stmts.len() >= 2 {
                    self.vexpr_wire_parallel(stmts)?;
                }
            }
            McPhrase::Group(ref g) => {
                // ── BUG4 fix ──────────────────────────────────────────────
                // Originally called process_stmt(p) for each branch. But the
                // first step of process_stmt, phrase_to_members, will clone the
                // branch (Group/Series/FuncCall all cloned), then do
                // process_member_internal on the cloned elements —— FuncCall's
                // auto_inst_map key falls on the **cloned pointer**.
                //
                // While the outer chain's adjacent wiring (connect_adjacent_pair:
                // RES_5 -> Group) goes get_left_points(Group) → iterates
                // **this Group's g.opds[i]** (same as here), for the FuncCall
                // inside it uses g.opds[i]'s original pointer to query
                // auto_inst_map —— unequal to the cloned pointer above → MISS →
                // placeholder CAP.in/RES.in leaks as @_phantom.
                //
                // Fix: no longer process_stmt(clone), but in-place process each
                // branch, keeping g.opds[i] sub-pointer unchanged (same strategy
                // as Parallel/Multiple handler):
                //   - Series branch: process_member_internal(&series[k])
                //     element by element (FuncCall instantiated on original
                //     pointer), then use the same batch of original pointers for
                //     internal adjacent connect_adjacent_pair.
                //   - Non-Series branch (FuncCall/Parallel/Endpoint etc.): directly
                //     process_member_internal(branch), pointer is g.opds[i] itself.
                // This way outer get_left_points(g.opds[i]) querying auto_inst_map
                // must hit, getting the real @?TYPE_n pins.
                for p in &g.opds {
                    match p {
                        McPhrase::Series(elems, d) => {
                            // BUG4: in-place processing + Label upgrade
                            // (fix the unconnected GND in `(CAP+RES)->GND`,
                            // the internal series in `VBUS->USB_VBUS`).
                            self.process_series_branch_inplace(elems, *d)?;
                        }
                        _ => {
                            self.process_member_internal(p)?;
                        }
                    }
                }
            }
            McPhrase::Transposed(inner) => {
                // ── P0 fix (Transposed auto_inst_map pointer mismatch) ──────
                // Originally process_stmt(inner) cloned the FuncCall via
                // phrase_to_members, causing the auto_inst_map key to land on
                // the cloned pointer. Later get_left_points / get_right_points
                // on the outer Transposed member use the original pointer to
                // query auto_inst_map → MISS → pins not resolved.
                // Fix: in-place process, keeping the original pointer (same
                // pattern as the caller chain dispatch at line 1380-1393).
                match inner.as_ref() {
                    McPhrase::Series(elems, d) => {
                        self.process_series_branch_inplace(elems, *d)?;
                    }
                    McPhrase::FuncCall(_)
                    | McPhrase::Endpoint(_)
                    | McPhrase::Transposed(_)
                    | McPhrase::Lead
                    | McPhrase::Member(_, _) => {
                        self.process_member_internal(inner)?;
                        // ★ M11.3: record bridge passive instance names from Transposed
                        let key = Self::member_key(inner);
                        if let Some(auto) = self.auto_inst_map.get(&key).cloned() {
                            let names: Vec<String> =
                                auto.instance_names().map(str::to_string).collect();
                            self.bridge_passive_names.extend(names);
                        }
                    }
                    _ => {
                        self.process_stmt(inner)?;
                    }
                }
            }
            // §2.4.5: `^` is a view over the same expression, so instantiate
            // the operand in place — keeping its original pointer in
            // auto_inst_map — exactly as the `Transposed` arm above does.
            McPhrase::Reversed(inner) => match inner.as_ref() {
                McPhrase::Series(elems, d) => {
                    self.process_series_branch_inplace(elems, *d)?;
                }
                McPhrase::FuncCall(_)
                | McPhrase::Endpoint(_)
                | McPhrase::Transposed(_)
                | McPhrase::Reversed(_)
                | McPhrase::Lead
                | McPhrase::Member(_, _) => {
                    self.process_member_internal(inner)?;
                }
                _ => {
                    self.process_stmt(inner)?;
                }
            },
            McPhrase::Closure(ref c) => {
                // Phase 3.3: Closure instantiation (closure parameter binding)
                for param_decl in c.params.iter() {
                    if let Some(name) = param_decl.get_primary_name() {
                        self.ensure_label(&name);
                    }
                }
                for p in &c.body {
                    self.process_stmt(p)?;
                }
                for elem in &c.right {
                    if !elem.name.is_empty() {
                        self.ensure_label(&elem.name);
                    }
                }
            }
            McPhrase::FuncCall(ref fc) => {
                // First check if it's an iterated call
                if let Some(iterated_result) = self.check_and_expand_iterated_call(
                    &fc.caller,
                    &fc.func_name,
                    &fc.params,
                    &fc.left,
                    &fc.right,
                )? {
                    let key = Self::member_key(phrase);
                    match iterated_result {
                        FuncCallInst::Components {
                            new_components,
                            new_connections,
                        } => {
                            // ── Iter-1.2 ───────────────────────────────────
                            // When iterated calls produce multiple components
                            // (e.g. `cap[4:5]::CAP()`), record them as an
                            // `AutoInst::Array` so the face resolver returns
                            // all instances' corresponding pins, letting
                            // `MIC{P,N} -> cap[4:5] -> uC.ADC{P,N}` go
                            // through the positional 2×1 vs 2×1 connection
                            // rather than being collapsed.
                            if let Some(entry) = AutoInst::from_instance_names(
                                new_components.iter().map(|c| c.name.clone()).collect(),
                            ) {
                                self.auto_inst_map.insert(key, entry);
                            }
                            // §7.9: batch-extended iterated products must not
                            // bypass the factory — push through it so any
                            // product that was not explicitly tagged by its
                            // construction record still gets the current
                            // expansion id (pre-tagged ids are preserved).
                            for comp in new_components {
                                self.add_component(comp);
                            }
                            for conn in new_connections {
                                self.add_connection(conn);
                            }
                        }
                        FuncCallInst::SubModule {
                            inst,
                            new_connections,
                        } => {
                            self.auto_inst_map
                                .insert(key, AutoInst::Name(inst.name.clone()));
                            self.add_submodule(inst);
                            for conn in new_connections {
                                self.add_connection(conn);
                            }
                        }
                        FuncCallInst::PassThrough => {
                            // Iterated call produced nothing (every item degraded to
                            // pass-through, warnings 944 already emitted per item by
                            // instantiate_funccall). Log the call for troubleshooting
                            // so a dropped iterated connection is traceable.
                            crate::db::diagnostic::diagnostic::dlog_trace(
                                944,
                                &format!(
                                    "stmt: iterated call '{}' → all pass-through, iterated connection dropped (module='{}')",
                                    fc.func_name,
                                    self.name,
                                ),
                            );
                        }
                    }
                    return Ok(());
                }

                // ── Iter-1.3 ─────────────────────────────────────────────
                // Array-form caller pointing to already-declared instances:
                // for a call like `cap[4:5]::CAP(1uF)`, pass1 has already
                // registered cap4/cap5 as independent components in
                // self.components, but the net stmt's FuncCall caller is still
                // the unexpanded "cap[4:5]" form. If we naively go through
                // instantiate_funccall, it would treat CAP as a class
                // constructor and create another @CAP?, misaligned with the
                // existing cap4/cap5.
                //
                // Here we recognize this form: caller is Bus/Label and the
                // name contains `[N:M]` / `[a,b]`, each name after expansion
                // can be found in self.components. On hit, record the resolved
                // members directly in auto_inst_map, skipping construction.
                if let Some(caller_box) = &fc.caller {
                    if let Some(array_names) =
                        self.resolve_array_caller_to_existing(caller_box.as_ref())
                    {
                        let key = Self::member_key(phrase);
                        if let Some(entry) = AutoInst::from_instance_names(array_names) {
                            self.auto_inst_map.insert(key, entry);
                        }
                        return Ok(());
                    }
                }

                // ── Iter-6.S4.1 ─────────────────────────────────────────────
                // **Caller chain recursion (lifted from original Iter-3.F position)**
                //
                // Must process the inner caller once before all dispatch paths
                // (Iter-2.2 instance-method dispatch, generic FuncCall path). Reasons:
                //
                //   1. **Chained call semantics**: `obj.f1().f2().f3()` semantics
                //      is "apply f1/f2/f3 sequentially to the same obj", each
                //      level needs to independently expand body, can't skip
                //      inner just because outer early-exits in dispatch phase.
                //   2. **method dispatch depends on this**: when outer `.Cap`
                //      of `CAP(v).Cap(x)` dispatches, it needs inner
                //      CAP(v) to have already written @CAP_N into auto_inst_map.
                //      Lifting to here doesn't affect this invariant.
                //   3. **Pointer stability (original Iter-3.F argument)**: use
                //      `process_member_internal` for single-member caller instead
                //      of `process_stmt`, keep `&**caller_stmt` address unchanged,
                //      making auto_inst_map's pointer key match reliable.
                //      Compound caller (Series/Parallel) still uses `process_stmt`
                //      for adjacency.
                //
                // Side effect tracking: after lifting, dispatch paths will see
                // an already-processed caller first. For Endpoint-form caller
                // (like mcu in `mcu.setup()`) processing is no-op; for
                // FuncCall-form caller (like `.add_caps()` after `setup()`) it
                // recursively expands the setup body —— which is exactly the
                // fix target.
                if let Some(caller_stmt) = &fc.caller {
                    match caller_stmt.as_ref() {
                        McPhrase::FuncCall(_)
                        | McPhrase::Endpoint(_)
                        | McPhrase::Transposed(_)
                        | McPhrase::Lead
                        | McPhrase::Member(_, _) => {
                            self.process_member_internal(caller_stmt.as_ref())?;
                        }
                        _ => {
                            self.process_stmt(caller_stmt.as_ref())?;
                        }
                    }
                }

                // ── Iter-2.2 ─────────────────────────────────────────────
                // Component instance method dispatch: forms like `uC.power(V3V3, V1V2)`.
                // funccall.rs::instantiate_funccall currently only checks
                // self.sub_modules, never dispatches methods on component instances
                // —— causing `func power()` / `func i2c()` in comp.sub to
                // never expand.
                //
                // Here we do explicit dispatch before entering instantiate_funccall:
                //   1. Extract instance name from fc.caller (Endpoint::Single's base name)
                //   2. If hit self.components, look up the component def's funcs table
                //   3. If hit self.sub_modules, look up the module def's funcs table
                //   4. If corresponding func def found, call instantiate_instance_method
                // This path also covers the Iter-1 cap[4:5] scenario where
                // "caller is array but func is user method" extreme case
                // (although not in the example project).
                //
                // ── Iter-3.A ────────────────────────────────────────────
                // `.Cap/.Pullup/.Pulldown` must reach Iter-2.2 dispatch below
                // so the library func is the wiring source (unified-twopin-
                // no-builtin v2.0) — never get grabbed earlier as a component
                // instance method with an empty-shell body ("Instance method
                // has no parsed stmts"), which would silently drop the call.
                // ── All-`_` placeholder twopin calls ─────────────────────
                // `.Cap(_)` / `.Cap([_, _])` carry no explicit endpoint;
                // dispatching them would bind `_` to a Multiple formal and
                // emit garbage nets. §11.6: placeholders do not implicitly
                // connect to GND. (The folded chain-shunt form `[A,B] =>
                // CAP(..).Cap(_)` has already become `.Cap([A, B])` and
                // dispatches normally.)
                if Self::is_all_placeholder_params(&fc.params) {
                    let reason = format!(
                        "'{}' has only `_` placeholder arguments and no network \
                         endpoints; placeholders do not implicitly connect to \
                         GND (§11.6)",
                        fc.func_name
                    );
                    crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::INST_PARAM_BIND_FAILED,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                        0,
                        0,
                        &crate::errcodes::format_msg(
                            crate::errcodes::INST_PARAM_BIND_FAILED,
                            &[
                                &fc.func_name.to_string(),
                                &fc.func_name.to_string(),
                                &reason,
                            ],
                        ),
                        &[],
                    );
                    return Ok(());
                }

                // ── Iter-2.2: ordinary instance-method dispatch ──────────
                // Runs for ALL method calls including `.Cap/.Pullup/.Pulldown`
                // — the library func (`func Cap([net1, net2])` etc.) is now the
                // only implementation (unified-twopin-no-builtin v2.0). If the
                // caller's component/sub-module def declares the func, dispatch
                // via instantiate_instance_method. The auto_inst_map caller
                // fallback below resolves `CAP(100nF)` constructions whose
                // instance is registered under the construction's own key.
                if let Some(caller_box) = &fc.caller {
                    // ── Iter-2.2 (Finding-A): auto_inst_map caller fallback ──
                    // `extract_caller_inst_name` on a caller-less construction
                    // FuncCall (`mic(V3V3)`, `DIO.ESD(5V)`) returns the *class*
                    // name ("mic", "DIO.ESD"), which is not a declared instance
                    // (construction created `_MIC1` / `_DIO_ESD1`). The caller
                    // chain recursion above already registered the created
                    // instance under member_key(caller); look it up so methods
                    // dispatch onto the constructed instance.
                    let mut inst_name = Self::extract_caller_inst_name(caller_box.as_ref());
                    if let Some(nm) = &inst_name {
                        let known =
                            self.find_component(nm).is_some() || self.find_submodule(nm).is_some();
                        if !known {
                            if let McPhrase::FuncCall(caller_fc) = caller_box.as_ref() {
                                // Only a plain instance name is dispatchable; an
                                // array group or a func-return face is not an
                                // instance this method can land on.
                                if let Some(AutoInst::Name(real)) =
                                    self.auto_inst_map.get(&caller_fc.id)
                                {
                                    inst_name = Some(real.clone());
                                }
                            }
                        }
                    }
                    if let Some(inst_name) = inst_name {
                        let func_name_str = fc.func_name.to_string();

                        // Component instance method (§5 effective method set:
                        // own func, else adopted-capability func).
                        let comp_func = self.find_component(&inst_name).and_then(|c| {
                            crate::db::defregistry::effective_method(&c.def, &func_name_str)
                        });
                        if let Some(func_def) = comp_func {
                            // arity guard: only dispatch when formals and
                            // actuals agree (mirrors the dotted-chain guard
                            // below). A no-arg method called with args would
                            // otherwise silently drop the args and wrongly
                            // expand the body.
                            let func_arity = func_def.params.iter().count();
                            let call_arity = fc.params.len();
                            if (func_arity > 0 && call_arity > 0)
                                || (func_arity == 0 && call_arity == 0)
                            {
                                let key = Self::member_key(phrase);
                                let result = self.instantiate_instance_method(
                                    &inst_name, &func_def, &fc.params, &fc.left, &fc.right,
                                )?;
                                if matches!(result, FuncCallInst::PassThrough) {
                                    self.stash_pass_through(key, &inst_name);
                                }
                                return Ok(());
                            }
                        }

                        // Sub-module instance method
                        let sub_func = self
                            .find_submodule(&inst_name)
                            .and_then(|m| m.def.funcs.find(&func_name_str).cloned());
                        if let Some(func_def) = sub_func {
                            // arity guard (mirrors the component-method and
                            // dotted-chain guards): don't dispatch a no-arg
                            // method called with args.
                            let func_arity = func_def.params.iter().count();
                            let call_arity = fc.params.len();
                            if (func_arity > 0 && call_arity > 0)
                                || (func_arity == 0 && call_arity == 0)
                            {
                                let key = Self::member_key(phrase);
                                let result = self.instantiate_instance_method(
                                    &inst_name, &func_def, &fc.params, &fc.left, &fc.right,
                                )?;
                                if matches!(result, FuncCallInst::PassThrough) {
                                    self.stash_pass_through(key, &inst_name);
                                }
                                return Ok(());
                            }
                        }

                        // ── P1 fix: dotted scope-chain drill down ──────────────
                        // inst_name like "mcu.uC" → look up
                        // components["uC"].funcs["i2c"] in sub_modules["mcu"].
                        // This handles the dispatch path after `uC.i2c(0x36)` in
                        // func body is prefixed to `mcu.uC.i2c(0x36)`.
                        if inst_name.contains('.') {
                            let segs: Vec<&str> = inst_name.split('.').collect();
                            if segs.len() >= 2 {
                                // Try sub_modules[seg0].components[seg1].funcs[func]
                                if let Some(sub) = self.find_submodule(segs[0]) {
                                    let inner_comp_func =
                                        self.component_in(&sub, segs[1]).and_then(|c| {
                                            let f = crate::db::defregistry::effective_method(
                                                &c.def,
                                                &func_name_str,
                                            )?;
                                            // arity guard
                                            let func_arity = f.params.iter().count();
                                            let call_arity = fc.params.len();
                                            if func_arity > 0 && call_arity > 0
                                                || func_arity == 0 && call_arity == 0
                                            {
                                                Some(f)
                                            } else {
                                                None
                                            }
                                        });
                                    if let Some(func_def) = inner_comp_func {
                                        let key = Self::member_key(phrase);
                                        let result = self.instantiate_instance_method(
                                            &inst_name, &func_def, &fc.params, &fc.left, &fc.right,
                                        )?;
                                        if matches!(result, FuncCallInst::PassThrough) {
                                            self.stash_pass_through(key, &inst_name);
                                        }
                                        return Ok(());
                                    }
                                }

                                // Try sub_modules[seg0].sub_modules[seg1].funcs[func]
                                if let Some(sub) = self.find_submodule(segs[0]) {
                                    let inner_sub_func = self
                                        .submodule_in(&sub, segs[1])
                                        .and_then(|m| m.def.funcs.find(&func_name_str).cloned());
                                    if let Some(func_def) = inner_sub_func {
                                        let key = Self::member_key(phrase);
                                        let result = self.instantiate_instance_method(
                                            &inst_name, &func_def, &fc.params, &fc.left, &fc.right,
                                        )?;
                                        if matches!(result, FuncCallInst::PassThrough) {
                                            self.stash_pass_through(key, &inst_name);
                                        }
                                        return Ok(());
                                    }
                                }
                            }
                        }

                        // ── Iter-6.S4 ────────────────────────────────────
                        // Chained call fallback: caller has been successfully
                        // resolved as some known instance (component / sub_module),
                        // but the called method does not **exist** in that
                        // instance type's funcs table.
                        //
                        // Typical scenario (main.mc:34):
                        //   `mcu.setup(V3V3, V1V2).add_caps().i2c().do_flash(flash)`
                        // These 4 methods are currently not defined in the module.
                        //
                        // Before fix: fall through to `instantiate_funccall` below,
                        //         treated as globally unknown class, generates
                        //         `@?add_caps_1` style stubs, polluting components list
                        //         + silently swallowing errors (iter6 P0-1).
                        // After fix: explicit warning + skip.
                        //   - Don't construct stub, don't call instantiate_funccall;
                        //   - **Don't** write auto_inst_map (see Iter-6.S4.2 fix note).
                        //
                        // Each layer on the chain will individually fall to here
                        // (4 warnings), letting the author immediately see the
                        // complete "undefined method" list.
                        //
                        // ── Iter-6.S4.2 removed the original auto_inst_map.insert ────────
                        // Originally there was a line here
                        // `self.auto_inst_map.insert(key, inst_name)`, intent was
                        // "in case this chain isn't an isolated line but participates
                        // in adjacency, get_left/right_points can also resolve ports
                        // from inst_name".
                        //
                        // Tests found this insert triggers a **stale entry bug from
                        // pointer reuse**:
                        //   1. do_flash chain's 4 layers each insert one
                        //      auto_inst_map[layer_phrase_addr] = "mcu"
                        //   2. After that line's process_stmt returns, the 4 McPhrase
                        //      nodes' memory is freed
                        //   3. When next line `mic(V3V3).MIC -> ...` is parsed, new
                        //      McPhrase is allocated on the heap, at least one new
                        //      address happens to land on the just-freed old address
                        //   4. resolve_funccall_right(mic FuncCall) uses the new
                        //      address to query map, **hits stale entry** "mcu"
                        //      → mic is incorrectly parsed as mcu's output port
                        //   5. Eventually mic.MIC and mcu's internal MIC/DAC_OUT/
                        //      SPK_MUTE three independent signals short into a 5-endpoint
                        //      super net
                        //
                        // Since the chain in the example project is actually an isolated line, the
                        // assumption in (b) doesn't happen; and outer's parsing in
                        // (a) actually comes from extract_caller_inst_name going
                        // through FuncCall recursion (Iter-6.S2) to derive along
                        // structure, no map needed.
                        //
                        // Fix: directly remove the insert. Chain layer fallback
                        // no longer writes to the map.
                        //
                        // Note: the pointer reuse risk from auto_inst_map being
                        // persistent across process_stmt is not further aggravated
                        // here, the root fix is Iter-6.S4.3 adding per-line clear in
                        // phases.rs's instantiate_stmts_resilient.
                        let inst_is_component = self.find_component(&inst_name).is_some();
                        let inst_is_submodule = self.find_submodule(&inst_name).is_some();
                        if inst_is_component || inst_is_submodule {
                            let owner_kind = if inst_is_component {
                                "component"
                            } else {
                                "sub-module"
                            };
                            self.record_warning(
                                crate::errcodes::INST_CHAIN_LINK_SKIPPED,
                                crate::errcodes::format_msg(
                                    crate::errcodes::INST_CHAIN_LINK_SKIPPED,
                                    &[&func_name_str, &owner_kind, &inst_name],
                                ),
                            );
                            // ── Iter-6.S4.2 ──
                            // No longer self.auto_inst_map.insert(...) —— see comment above
                            return Ok(());
                        }
                    }
                }

                // ── Iter-6.S4.1 ─────────────────────────────────────────
                // Caller chain recursion was originally placed here, after Iter-2.2
                // dispatch and before the generic FuncCall path. But combined with
                // Iter-6.S4's "undefined method warning + early exit" logic, chained
                // calls like `mcu.setup().add_caps().i2c().do_flash()` once outer
                // (do_flash) hits early exit, can never reach here —— inner i2c /
                // add_caps / setup three layers are silently skipped regardless of
                // whether defined.
                //
                // Fix: lift the entire recursion before Iter-2.2 dispatch (see above),
                // so inner chain layers are always processed once before outer:
                //   - If inner method is defined → each expands body (fixes the
                //     potential "outer dispatched, inner body lost" hidden bug)
                //   - If inner method is undefined → each falls to Iter-6.S4 fallback,
                //     each layer reports warning #940, author gets the complete
                //     missing list at once
                //
                // This position is kept as a placeholder note, semantics are lifted.
                // Below follows the generic FuncCall instantiation path
                // (unified-twopin-no-builtin v2.0: no P1-D builtin twopin
                // fallback — `.Cap/.Pullup/.Pulldown` either dispatch through
                // method dispatch above or fall through to the generic path).
                let key = Self::member_key(phrase);

                // ── P2-9: prevent duplicate component creation ──────────────
                // When lane-by-lane wiring re-processes the same FuncCall
                // elements that were already instantiated by the normal
                // process_member_internal loop, auto_inst_map already has
                // the entry. Skip re-instantiation to avoid creating
                // duplicate components (e.g. CAP_6/CAP_7 alongside CAP_4/CAP_5
                // in XTAL setup).
                if self.auto_inst_map.contains_key(&key) {
                    return Ok(());
                }

                let result = self.instantiate_funccall(
                    &fc.func_name,
                    &fc.params,
                    &fc.left,
                    &fc.right,
                    fc.caller.as_deref(),
                )?;
                match result {
                    FuncCallInst::Components {
                        new_components,
                        new_connections,
                    } => {
                        if let Some(comp) = new_components.first() {
                            self.auto_inst_map
                                .insert(key, AutoInst::Name(comp.name.clone()));
                        }
                        // §7.9: push through the factories so untagged
                        // products still receive the current expansion id.
                        for comp in new_components {
                            self.add_component(comp);
                        }
                        for conn in new_connections {
                            self.add_connection(conn);
                        }
                    }
                    FuncCallInst::SubModule {
                        inst,
                        new_connections,
                    } => {
                        self.auto_inst_map
                            .insert(key, AutoInst::Name(inst.name.clone()));
                        self.add_submodule(inst);
                        for conn in new_connections {
                            self.add_connection(conn);
                        }
                    }
                    FuncCallInst::PassThrough => {
                        // ── P2-2: check Endpoint return side channel ─────────────────
                        // instantiate_instance_method sets this when it detects
                        // McFuncReturn::Endpoint. Takes priority over P0-4 stub path.
                        let return_ep = super::fcallinst::LAST_RETURN_ENDPOINT
                            .with(|cell| cell.borrow_mut().take());
                        if let Some(entry) = return_ep {
                            self.auto_inst_map.insert(key, entry);
                        } else {
                            // ── P0-4 fix (enhanced) ───────────────────────────────
                            // Unrecognized FuncCall → register a unique stub name for
                            // each call in `auto_inst_map`, to avoid class names leaking
                            // as Labels and causing shorts.
                            //
                            // ── P0-4 naming unification ──────────────────────────
                            // Unify type string normalization: `.Cap(...)` and
                            // `CAP(...)` both use the canonical class name (all caps)
                            // for auto_name, no longer one using function name and
                            // the other using class name.
                            // `instantiate_component_construction` uses `comp_def.name`
                            // (all caps, e.g. "CAP"); P0-4 stub also normalizes to
                            // the same namespace.
                            let class_name = fc.func_name.to_string();
                            // ── P2-7-XTAL: strict full-name case-sensitive class
                            // check (replaces first-letter-uppercase + contains('.')
                            // heuristic). `Cap`/`Reset` are method names, not
                            // classes → not class-looking. `CAP`, `DIO.ESD` are
                            // registered classes → class-looking (stub/reuse).
                            let class_looking = Self::is_registered_class_name(&class_name);
                            let caller_name = match &fc.caller {
                                None => String::new(),
                                Some(caller_box) => match caller_box.as_ref() {
                                    McPhrase::Endpoint(McEndpoint::Single(iref)) => {
                                        match &iref.base {
                                            McInstance::Label(s) => s.clone(),
                                            McInstance::Bus(b) => b.name.clone(),
                                            _ => String::new(),
                                        }
                                    }
                                    _ => String::new(),
                                },
                            };
                            // ── P2-7-XTAL: strict full-name class check — an
                            // instance name (Y2, R442) is never a registered
                            // class, so the old uppercase-first + no-digit
                            // heuristic is replaced by the exact CMIE lookup.
                            let caller_looks_like_class =
                                Self::is_registered_class_name(&caller_name);
                            let caller_unknown = caller_name.is_empty()
                                || caller_looks_like_class
                                || (!self.is_port(&caller_name)
                                    && self.find_component(&caller_name).is_none()
                                    && self.find_submodule(&caller_name).is_none()
                                    && !self.is_bus(&caller_name));

                            if class_looking && caller_unknown {
                                // ── P0-4 naming unification ──────────────────────
                                // Normalize type name: replace '.' with '_', then
                                // uppercase so `@?Cap_1` and `@CAP_1` normalize to
                                // `@?CAP_1`
                                //
                                // ── ★ P0-2 alias normalization ─────────────────────────────
                                // Further convert shorthand to the canonical class name
                                // actually present in CMIE:
                                //   `Esd(...)`   → canonical name `DIO.ESD`  → stub `@?DIO_ESD_N`
                                //   `Zener(...)` → canonical name `DIO.ZENER`→ stub `@?DIO_ZENER_N`
                                // This way: (a) the same physical type no longer produces
                                // two different stub namespaces; (b) even with this stub
                                // fallback, it's consistent with the safe_type used by
                                // downstream instantiate_component_construction, no longer
                                // "@?ESD vs @DIO_ESD" parallel orphan. (Root fix is in
                                // funccall.rs the alias fallback before CMIE lookup,
                                // that path lets ESD(...) directly go through real
                                // component construction; this is just a fallback.)
                                let canonical_class =
                                    crate::vector::graph::naming::canonicalize_class_alias(
                                        &class_name,
                                    )
                                    .unwrap_or_else(|| class_name.clone());
                                let safe = canonical_class.replace('.', "_").to_ascii_uppercase();

                                // ── ★ ITER-1 P0 fix: reuse real component name, eliminate @? mismatch ──────────
                                //
                                // Symptom: the example mcu module's 3 decoupling caps
                                //   `CAP_1` / `CAP_2` / `CAP_3` have already been
                                //   actually registered in self.components by
                                //   `instantiate_component_construction` via
                                //   `auto_name(safe_type)` (and written to InstTable),
                                //   but the same stmt's FuncCall dispatch through the
                                //   dispatcher path returns PassThrough, falling to
                                //   this P0-4 branch, which separately generates
                                //   stub names like `@?CAP_1` via the `@?CAP` counter
                                //   and writes them into auto_inst_map.
                                // Consequence: when pass2 parses connection nets, it
                                //   gets `@?CAP_1` from auto_inst_map, looks up
                                //   `@?CAP_1.1` in InstTable, the entire net is lost
                                //   (`[NET] fully lost: failed: ["@?CAP_1.1"]`), 8/9
                                //   dropped nets are all this single bug.
                                //
                                // Fix strategy: before going through the P0-4 stub, first
                                // check if self.components already has a real component
                                // with def.name matching `safe`. If yes, directly
                                // "claim" this real component name (reverse find =
                                // take the most recently created instance), letting this
                                // outer FuncCall share the real component already
                                // created by inner —— equivalent to the Iter-2.2
                                // auto_inst_map caller fallback of ordinary method
                                // dispatch, just that here we use class name match +
                                // most recent instance as fallback.
                                //
                                // Safety argument:
                                //   - Only enter this branch when `class_looking && caller_unknown`
                                //     (which is already the P0-4 stub trigger condition),
                                //     won't damage other paths.
                                //   - Take the **most recently created** component of the
                                //     same class (rev find): inner FuncCall is always
                                //     processed by process_member_internal recursively
                                //     before the outer caller (Iter-6.S4.1), so the end
                                //     of components is the inner paired with this outer.
                                //   - Multiple auto_inst_map keys pointing to the same
                                //     real inst.name is **expected behavior** —— when
                                //     method dispatch works properly, both inner and
                                //     outer map to the same "CAP_1". We want to
                                //     replicate this semantics, deliberately **not** use
                                //     `auto_inst_map.values()` to exclude already
                                //     referenced instances, otherwise when inner has
                                //     already registered "CAP_1", outer's P0-4 reuse
                                //     can never find anything to claim, directly falls
                                //     back to stub, bug not fixed.
                                //   - Use `def.name` (after replacing '.' → '_') for
                                //     comparison instead of `inst.name`, to avoid mixing
                                //     same-name instances (CAP_1) with same-class
                                //     different instances (RES_1).
                                //   - If no matching-class real component found, fall
                                //     back to old stub path —— this is the boundary case
                                //     without inner real construction (e.g. truly unknown
                                //     class), keeping original behavior.
                                let reusable = self
                                    .components_view()
                                    .iter()
                                    .rev() // Most recently created takes priority (matches AST processing order)
                                    .find(|c| {
                                        let cls_safe = c
                                            .def
                                            .name
                                            .to_string()
                                            .replace('.', "_")
                                            .to_ascii_uppercase();
                                        cls_safe == safe
                                    })
                                    .map(|c| c.name.clone());

                                if let Some(real_name) = reusable {
                                    self.auto_inst_map.insert(key, AutoInst::Name(real_name));
                                } else {
                                    // Genuine unresolved construction: the class is
                                    // registered (`class_looking`) but no real instance
                                    // was ever materialized for it, so the outer call
                                    // degrades to an `@?` stub that produces no nets
                                    // or parts. That is a user-facing error — surface
                                    // it instead of silently absorbing the construction
                                    // (unlike the ITER-1 reuse branch above, which is
                                    // the healthy same-class sharing path).
                                    let diag = crate::errcodes::format_msg(
                                        crate::errcodes::UNRESOLVED_CLASS_STUB,
                                        &[&class_name],
                                    );
                                    let (stub, _, _) =
                                        self.auto_name(super::AutoNameKind::Stub, &safe);
                                    self.auto_inst_map.insert(key, AutoInst::Name(stub));
                                    self.log_global_diag(
                                        crate::errcodes::UNRESOLVED_CLASS_STUB,
                                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                                        diag,
                                    );
                                }
                            }
                        } // ← P2-2 else close
                    }
                }
            }
            // Basic types need no special handling
            McPhrase::Lead
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Node { .. })
            | McPhrase::Endpoint(_) => {}
            McPhrase::Multiple(inner) => {
                // ── P1-B2 ────────────────────────────────────────────────
                // Cooperates with P1-B's "keep Multiple inside Series" rule.
                // Previously phrase_to_members would flatten Multiple away,
                // process_member_internal would never encounter Multiple, so
                // here was originally no-op. After P1-B changed to keep it, if
                // here still does nothing, inner FuncCalls (like the iterated
                // call `cap[4:5]::CAP(1uF)`, or member list
                // `[CAP(10uF).Cap(...), RES(1k).Pullup(...)]`) won't be
                // instantiated, auto_inst_map won't have corresponding keys,
                // downstream get_left_points/get_right_points can only go
                // through fallback, expanding pins as bare labels, and the
                // actual wiring of the chain's upstream/downstream **entirely
                // disappears**.
                //
                // Fix: recursively process each phrase inside Multiple, so
                // their declarations/constructions also walk into their
                // respective FuncCall / Bus / Label branches.
                for p in inner {
                    self.process_member_internal(p)?;
                }
            }
            McPhrase::Series(_, _) => {}
            // ── Iter-12.1c: recursively process Member's inner phrase ──────────────
            //
            // Original code: `McPhrase::Member(_, _) => {}` (no-op)
            //
            // Problem: `uC.i2c(0x36).I2C0 -> I2C0` is parsed as
            //   Member(FuncCall(uC.i2c), Label("I2C0"))
            // Member's no-op causes the inner FuncCall to never be dispatched:
            //   - uC.i2c() method body not expanded
            //   - auto_inst_map has no entry
            //   - get_right_points degrades to uC's generic right pin (pin 21 GND)
            //
            // Fix: recursively call process_member_internal to handle the inner
            // phrase, so FuncCall properly goes through the method dispatch path.
            McPhrase::Member(inner_phrase, _) => {
                self.process_member_internal(inner_phrase)?;
            }
        }
        Ok(())
    }

    /// Assign stable IDs to all `McFuncCall` nodes in a phrase tree.
    /// IDs survive cloning, replacing the fragile pointer-based auto_inst_map key.
    pub(super) fn assign_phrase_ids(phrase: &mut McPhrase, next_id: &mut u32) {
        match phrase {
            McPhrase::FuncCall(ref mut f) => {
                if f.id == 0 {
                    *next_id += 1;
                    f.id = *next_id;
                }
                if let Some(ref mut caller) = f.caller {
                    Self::assign_phrase_ids(caller, next_id);
                }
            }
            McPhrase::Series(elems, _) | McPhrase::Parallel(elems) | McPhrase::Multiple(elems) => {
                for p in elems {
                    Self::assign_phrase_ids(p, next_id);
                }
            }
            McPhrase::Group(ref mut g) => {
                for p in &mut g.opds {
                    Self::assign_phrase_ids(p, next_id);
                }
            }
            McPhrase::Transposed(ref mut inner) => {
                Self::assign_phrase_ids(inner, next_id);
            }
            McPhrase::Reversed(ref mut inner) => {
                Self::assign_phrase_ids(inner, next_id);
            }
            McPhrase::Closure(ref mut c) => {
                for p in &mut c.body {
                    Self::assign_phrase_ids(p, next_id);
                }
            }
            McPhrase::Member(ref mut inner, _) => {
                Self::assign_phrase_ids(inner, next_id);
            }
            McPhrase::Lead | McPhrase::Endpoint(_) => {}
        }
    }

    /// Reset all FuncCall IDs in a phrase to 0.
    /// Used by P2-5 expansion so that each expanded pair gets fresh unique IDs
    /// from assign_phrase_ids, preventing P2-9 dedup from incorrectly skipping
    /// the second (and subsequent) builtin twopin instantiations.
    fn reset_phrase_ids(phrase: &mut McPhrase) {
        match phrase {
            McPhrase::FuncCall(ref mut f) => {
                f.id = 0;
                if let Some(ref mut caller) = f.caller {
                    Self::reset_phrase_ids(caller);
                }
            }
            McPhrase::Series(elems, _) | McPhrase::Parallel(elems) | McPhrase::Multiple(elems) => {
                for p in elems {
                    Self::reset_phrase_ids(p);
                }
            }
            McPhrase::Group(ref mut g) => {
                for p in &mut g.opds {
                    Self::reset_phrase_ids(p);
                }
            }
            McPhrase::Transposed(ref mut inner) => {
                Self::reset_phrase_ids(inner);
            }
            McPhrase::Reversed(ref mut inner) => {
                Self::reset_phrase_ids(inner);
            }
            McPhrase::Closure(ref mut c) => {
                for p in &mut c.body {
                    Self::reset_phrase_ids(p);
                }
            }
            McPhrase::Member(ref mut inner, _) => {
                Self::reset_phrase_ids(inner);
            }
            _ => {}
        }
    }

    /// Get the stable ID for a FuncCall phrase.
    /// Returns 0 for non-FuncCall phrases (they never use auto_inst_map).
    pub(super) fn member_key(member: &McPhrase) -> u32 {
        match member {
            McPhrase::FuncCall(f) => f.id,
            // `^` wraps without changing which call it refers to, so the key
            // must still find the operand's entry in auto_inst_map.
            McPhrase::Reversed(inner) => Self::member_key(inner),
            _ => 0,
        }
    }

    // ────────────────────────────────────────────────────────────────────────
    // Iter-1/2 helper functions
    // ────────────────────────────────────────────────────────────────────────

    /// Extract the "caller's instance name" from McPhrase.
    ///
    /// Used to identify the component/sub-module instance name pointed to by
    /// the caller side in syntax like `uC.power(...)` / `flash.init(...)`.
    ///
    /// Supports the following forms:
    ///   - `Endpoint::Single(Bus("uC"))`        → "uC"
    ///   - `Endpoint::Single(Label("flash"))`   → "flash"
    ///   - `Endpoint::Single(Component(c))`     → c.name
    ///   - `Endpoint::Single(Module(m))`        → m.name
    ///   - `FuncCall(...)` (Iter-6.S2)          → recursively inward along caller chain
    ///
    /// Returns None to indicate the caller is not a single instance reference.
    pub(super) fn extract_caller_inst_name(phrase: &McPhrase) -> Option<String> {
        match phrase {
            McPhrase::Endpoint(McEndpoint::Single(iref)) => match &iref.base {
                McInstance::Label(s) => Some(s.clone()),
                McInstance::Bus(b) => {
                    // Bare Bus (member empty) is treated as instance reference
                    if b.member.is_empty() {
                        Some(b.name.clone())
                    } else {
                        None
                    }
                }
                McInstance::Component(c) => Some(c.name.to_string()),
                McInstance::Module(m) => Some(m.name.to_string()),
                _ => None,
            },
            // Series[Endpoint] fallback: parser occasionally wraps a single instance in Series
            McPhrase::Series(phrases, _) if phrases.len() == 1 => {
                Self::extract_caller_inst_name(&phrases[0])
            }
            // ── Iter-6.S2 ────────────────────────────────────────────────
            // Chained call support: caller is itself a FuncCall (e.g. `setup()`
            // in `mcu.setup().add_caps()` is add_caps's caller).
            //
            // Semantically, each layer's "this" on the chain is the innermost
            // real instance. Therefore recurse inward along fc.caller until
            // hitting an Endpoint or returning None.
            //
            // Example:
            //   `mcu.setup(V3V3, V1V2).add_caps().i2c().do_flash(flash)`
            // parsed as
            //   FuncCall { name=do_flash, caller=
            //     FuncCall { name=i2c, caller=
            //       FuncCall { name=add_caps, caller=
            //         FuncCall { name=setup, caller=Endpoint(Module(mcu)) }}}}
            //
            // When taking do_flash's caller_inst_name, this function drills
            // down layer by layer:
            //   do_flash.caller (FuncCall i2c)
            //     → i2c.caller (FuncCall add_caps)
            //       → add_caps.caller (FuncCall setup)
            //         → setup.caller (Endpoint(Module(mcu)))  ← end
            //           → returns "mcu"
            //
            // Compatible rollback: if a middle caller in the chain is None
            // (shouldn't happen in theory, parser should treat empty caller
            // as Endpoint), recursion naturally returns None, degrading to
            // pre-fix behavior.
            McPhrase::FuncCall(fc) => fc
                .caller
                .as_deref()
                .and_then(Self::extract_caller_inst_name)
                // Caller-less instance creation (`mic(V3V3)`, `CAP(...)`) names
                // the created instance after the class, so a chained member
                // (`mic(V3V3).MIC`) resolves against that instance name.
                .or_else(|| {
                    let name = fc.func_name.to_string();
                    (!name.is_empty()).then_some(name)
                }),
            _ => None,
        }
    }

    /// Recognize the "array-form caller pointing to a set of already-declared
    /// instances" form.
    ///
    /// Two structural arms, no bracket-string re-parse (AST-driven guideline):
    ///   1. `Endpoint(List([...]))` — pass1's vector arm (§11.3 ③) resolves a
    ///      declared array to one lane per ordered member; extract the member
    ///      instance names structurally.
    ///   2. `Endpoint(Single(Component(res1)))` — pass1 resolving a bracket to
    ///      a single member (contract E scalar); matched against the declared
    ///      vector group's physical member id list.
    ///
    /// Returns `Some(vec!["cap4", "cap5"])` on hit, otherwise None.
    ///
    /// The old arms are gone: the bare-bracket `McIds::from(&name).expand()`
    /// synthesis (fires for `Bus("cap[4:5]")` / `Label("cap[4:5]")` callers)
    /// and the digit-suffix sibling-probing fallback (Iter-3.D). Declared
    /// arrays reach here as `Endpoint::List` (arm 1); an undeclared array base
    /// falls to the scalar-miss decision like any other undeclared name, never
    /// re-assembled from name patterns.
    pub(super) fn resolve_array_caller_to_existing(
        &self,
        phrase: &McPhrase,
    ) -> Option<Vec<String>> {
        // ── §11.3 lane-structured List (Phase 1.3) ──────────────────────────
        // `cap[4:5]` in a connection operand resolves at pass1 to
        // `Endpoint(List([Single(Component cap4), Single(Component cap5)]))`
        // (module scope → find_inst hits → Component). Extract the member
        // instance names **structurally** from the lanes — no bracket-string
        // re-parse (AST-driven guideline). Guarded by the all_exist check, so
        // phantom/auto-named lanes never re-link.
        if let McPhrase::Endpoint(McEndpoint::List(eps)) = phrase {
            let mut names = Vec::new();
            for ep in eps {
                match ep {
                    McEndpoint::Single(iref) => match &iref.base {
                        McInstance::Component(c) => names.push(c.name.to_string()),
                        McInstance::Module(m) => names.push(m.name.to_string()),
                        McInstance::Label(s) => names.push(s.clone()),
                        McInstance::Bus(b) if b.member.is_empty() => names.push(b.name.clone()),
                        _ => return None,
                    },
                    _ => return None,
                }
            }
            if names.len() > 1 && names.iter().all(|n| self.find_component(n).is_some()) {
                return Some(names);
            }
            return None;
        }

        // ── §11.3/1.6: a single `Component` caller is NEVER re-linked to its
        // group ─────────────────────────────────────────────────────────────
        // The Iter-3.D sibling-probing heuristic is gone (cffa52c): it probed
        // base+digit siblings (`res1` → res2, res3 ...) to reassemble an array
        // after pass1 expanded only the first member. Its successor here used
        // to match a single `Component` caller against every vector group's
        // `member_ids` and hand back the WHOLE group.
        //
        // That successor was wrong, and cffa52c's own claim — "contract-E
        // scalars are not in `vectors`, so single-member references never
        // re-link as arrays" — is false as written: `member_ids` holds exactly
        // the contract-E scalar member names (`res1`, `res2`). So a scalar
        // member used as a *FuncCall caller* (`res[2].Pullup(...)`,
        // `res1.Pullup(...)`) matched, producer B registered `@@ARRAY:res1,
        // res2` and returned early — **before** the Iter-2.2 method dispatch —
        // silently dropping the call. The old lock
        // (`member_scalar__single_index_ref_connects_only_itself`) only covers
        // the connection form (`res[2] -> GND`), never the caller form.
        //
        // Arm 1 above already owns the legal array caller: pass1 resolves a
        // declared bracket to a lane-structured `Endpoint(List)` (§11.3 ③,
        // `vector_lane_pass1.rs`), so the whole-group re-link is reachable only
        // through the single-`Component` form — which contract E defines as a
        // **scalar member reference**, never an array. No legal trigger
        // remains, so the arm is deleted rather than guarded (same precedent
        // as the digit-suffix branch it replaced).

        None
    }

    /// Recursively scan a McPhrase for FuncCall nodes referencing a failed component class.
    pub(super) fn phrase_contains_failed_class(
        phrase: &McPhrase,
        failed: &HashSet<String>,
    ) -> bool {
        match phrase {
            McPhrase::FuncCall(fc) => {
                let name = fc.func_name.to_string();
                // Check both the full name and the base class name (strip after last '.')
                if failed.contains(&name) {
                    return true;
                }
                if let Some(base) = name.rsplit('.').next() {
                    if failed.contains(base) {
                        return true;
                    }
                }
                // Also check caller
                if let Some(ref caller) = fc.caller {
                    if Self::phrase_contains_failed_class(caller, failed) {
                        return true;
                    }
                }
                false
            }
            McPhrase::Series(elems, _) | McPhrase::Parallel(elems) | McPhrase::Multiple(elems) => {
                elems
                    .iter()
                    .any(|e| Self::phrase_contains_failed_class(e, failed))
            }
            McPhrase::Group(g) => g
                .opds
                .iter()
                .any(|opd| Self::phrase_contains_failed_class(opd, failed)),
            McPhrase::Transposed(inner) | McPhrase::Reversed(inner) => {
                Self::phrase_contains_failed_class(inner, failed)
            }
            McPhrase::Member(inner, _) => Self::phrase_contains_failed_class(inner, failed),
            _ => false,
        }
    }
}
