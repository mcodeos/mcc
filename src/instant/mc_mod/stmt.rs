// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection statement processing
//!
//! - `process_stmt`: single stmt expansion + member/adjacent connection dispatch
//! - `phrase_to_members`: expand Series etc aggregate forms to member sequence
//! - `try_connect_adjacent`: adjacent member pairing connections
//! - `process_member_internal`: single member internal processing (FuncCall / Closure / Group …)

use super::funccall::FuncCallInst;
use super::InstantiationBuilder;
use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::{ConnDir, ConnOp, IOType, Shape};
use crate::semantic::component::mc_pins::McPinPort;
use crate::semantic::mc_inst::McInstance;
use crate::vector::model::trunk::TrunkKind;
use std::collections::HashSet;

// ── M11.4: lane item for position-aware bridge pin collection ──
enum LaneItem<'a> {
    Series(&'a McPhrase),
    Bridge(NetPoint),
}

impl InstantiationBuilder {
    /// Process connection stmt - accepts McPhrase
    pub(super) fn process_stmt(&mut self, phrase: &McPhrase) -> Result<(), InstError> {
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

        // ★ M-1'-A: extract direction from the source phrase before flattening
        let dir = match &phrase {
            McPhrase::Series(_, d) => *d,
            _ => ConnDir::Undirected,
        };

        let mut phrase = phrase.clone();
        Self::assign_phrase_ids(&mut phrase, &mut self.next_phrase_id);
        let members = self.phrase_to_members(&phrase);
        if members.is_empty() {
            return Ok(());
        }

        // unified-twopin-no-builtin v2.0 §2.4: no chain-shunt special-case. A
        // `.Cap([a, b])` member is an ordinary FuncCall whose connection face
        // comes from the library func's return; the normal member loop +
        // adjacent pairing wire the pass-through lanes. `[2×1] -> CAP(1×2) ->
        // [2×1]` is a shape error reported by the series-row check.
        self.process_series_members(&members, dir)
    }

    /// Process a flattened series' members: P2-5 expansion, normal member loop,
    /// lane-by-lane wiring, and adjacent pairing.
    fn process_series_members(
        &mut self,
        members: &[McPhrase],
        dir: ConnDir,
    ) -> Result<(), InstError> {
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
        // downstream try_connect_adjacent loop doesn't re-process them and
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
                // by the downstream try_connect_adjacent loop.
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
                    let pair = if fc_is_left {
                        McPhrase::Series(vec![fc_clone, item.clone()], dir)
                    } else {
                        McPhrase::Series(vec![item.clone(), fc_clone], dir)
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
            // §8.9.6.7: the lane-by-lane path bypasses try_connect_adjacent,
            // so the AST-layer group context is never established there.
            // Extract it from the chain members (driver side first, then the
            // far side) and wire inside it, so bus member lanes carry their
            // trunk identity and render as a trunk.
            let trunk = Self::extract_trunk_group(&members[0])
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
                this.wire_chain_lane_by_lane(&members, dir)
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

            if let Err(e) = self.try_connect_adjacent(left_member, right_member, dir) {
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

    fn member_contains_lead(member: &McPhrase) -> bool {
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
    // Lane-wiring only (the `num_lanes` loop in `wire_chain_lane_by_lane`):
    // how many independent parallel lanes a member spans. NOT a §5 port-width
    // source — width legality now goes through the unified
    // `get_left_points`/`get_right_points` → `Shape::vvec` → `check_series_rows`
    // chain (vec-arch.md stage D). A bare port label here is `1` lane, which is
    // correct for lane wiring even when the port declares multiple members.
    fn member_lane_width(&self, member: &McPhrase) -> usize {
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
            // ── M11.5: handle Bus with multiple members as multi-lane ──
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref bus),
                ..
            })) if !bus.member.is_empty() => bus.member.len(),
            _ => 1,
        }
    }

    // ── M11.1 / M11.2 / M11.4: Lane-by-lane wiring for chains containing
    // pass-through (_) or bridge passives (CAP').
    //
    // Each lane is wired independently. Lead elements are pass-through identity.
    // Transposed elements (standalone or inside Parallel) act as bridge passives:
    // each pin is attached to its corresponding lane net at the correct position
    // in the chain.
    //
    // Example: [a.P, a.N] -> [RES, _] -> [b.P, b.N]
    //   Lane 0: a.P → RES.1 → RES.2 → b.P
    //   Lane 1: a.N → b.N  (pass-through, _ skipped)
    //
    // Example: [a.P, a.N] -> [RES, _] + CAP' -> [b.P, b.N]
    //   Lane 0: a.P → RES.1 → RES.2 → b.P, CAP.1 on RES.2~b.P net
    //   Lane 1: a.N → b.N, CAP.2 on a.N~b.N net
    //
    // Example: [a.P, a.N] -> [RES, _] -> CAP' -> [RES, RES] -> [b.P, b.N]
    //   Lane 0: a.P → RES.1 → RES.2 → b.P, CAP.1 on RES.2~RES net
    //   Lane 1: a.N → RES → b.N, CAP.2 on a.N~RES net
    fn wire_chain_lane_by_lane(
        &mut self,
        members: &[McPhrase],
        dir: ConnDir,
    ) -> Result<(), InstError> {
        let num_lanes = members
            .iter()
            .map(|m| self.member_lane_width(m))
            .max()
            .unwrap_or(0);
        crate::vlog!("[lane-by-lane] num_lanes={num_lanes}");
        if num_lanes == 0 {
            return Ok(());
        }

        // Pre-instantiate FuncCalls inside Transposed members.
        // In normal flow, process_member_internal handles Transposed by
        // instantiating its inner FuncCall. But lane-by-lane wiring skips
        // the normal adjacency loop, so Transposed-inner FuncCalls never
        // get instantiated → get_transposed_lane_pin returns empty.
        for member in members {
            if let McPhrase::Transposed(_) = member {
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

        // ── Strict §5 transpose-bridge legality (vec-dianlu.md §5.3) ──
        // A transposed operand is first transposed to its full-width column —
        // `get_left_points` / `get_right_points` merge the inner left + right
        // pins, which is already the transposed result — and the §5.2 series
        // check runs on that result. There is no pair-by-min / lane-hang
        // carve-out: each adjacent pair must span the same width. Any width
        // mismatch is an illegal operation (E4007) and the chain generates no
        // connections.
        //
        // Unified width source (vec-arch.md stage D): this check uses the same
        // chain as `try_connect_adjacent` — `get_left_points` / `get_right_points`
        // (internally unified via `expand_port_lanes`, which resolves a bare
        // port label against the module's declared port members) → `Shape::vvec`
        // → `check_series_rows` (opcheck, shared Pass1/Pass2). The previous
        // `member_port_width` / `member_lane_width` self-computed widths were a
        // third, drifting source: `member_lane_width` returned 1 for a bare
        // port label (`Endpoint(Single(Bus))` with empty `bus.member`) even
        // when the port declares 2 members, causing a false E4007 on
        // `dc -> [RES, _] + CAP' -> ...` (periph.mc). A zero width (unresolved
        // side / empty expansion) skips the pair.
        let has_transposed = members.iter().any(|m| Self::phrase_contains_transposed(m));
        if has_transposed {
            for i in 0..members.len().saturating_sub(1) {
                // §5.2 contact side: left member's right port vs right member's
                // left port.
                let lpts = self.get_right_points(&members[i])?;
                let rpts = self.get_left_points(&members[i + 1])?;
                if lpts.is_empty() || rpts.is_empty() {
                    continue;
                }
                let lhs_shape = Shape::vvec(lpts.len());
                let rhs_shape = Shape::vvec(rpts.len());
                let verdict = crate::semantic::opcheck::check_series_rows(lhs_shape, rhs_shape);
                if !matches!(verdict, crate::semantic::opcheck::OpCheck::Legal(_)) {
                    self.record_error(
                        crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                        crate::errcodes::format_msg(
                            crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    return Ok(());
                }
            }
        }

        for lane in 0..num_lanes {
            // Collect lane items: series elements and bridge pins in order.
            let items = self.collect_lane_items(members, lane);

            // Extract series elements and their bridge pins.
            // bridges_at[i] = bridge pins to attach to the net between series[i] and series[i+1].
            let mut series_elems: Vec<&McPhrase> = Vec::new();
            let mut bridges_at: Vec<Vec<NetPoint>> = Vec::new();
            let mut pending_bridges: Vec<NetPoint> = Vec::new();

            for item in &items {
                match item {
                    LaneItem::Series(elem) => {
                        series_elems.push(elem);
                        // Bridge pins collected before this series element belong to
                        // the gap between the previous series element and this one.
                        bridges_at.push(std::mem::take(&mut pending_bridges));
                    }
                    LaneItem::Bridge(pin) => {
                        pending_bridges.push(pin.clone());
                    }
                }
            }
            // Trailing bridges (collected after the last series element) stay
            // in `pending_bridges`; §11 strict vector order: a chain-tail
            // bridge (`A -> CAP'`) is written after the last series element,
            // so its pins attach after that element's right points — not into
            // the last gap.

            // Instantiate FuncCall elements before resolving points.
            // When lane-by-lane wiring skips the normal process_member_internal
            // loop, FuncCall elements (e.g. CAP(18pF) in setup chains) are
            // never instantiated → get_left_points/get_right_points return
            // empty because auto_inst_map has no entries.
            for elem in &series_elems {
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
                    // P2-7 debug: trace FuncCall instantiation in lane-by-lane
                    if let McPhrase::FuncCall(fc) = elem {
                        let key = Self::member_key(elem);
                        let inst_name = self.auto_inst_map.get(&key).cloned();
                        let left_pts = self.get_left_points(elem).unwrap_or_default();
                        mcc_dbg!("inst::mod", 
                            "[LL-DBG] module={} lane={lane} FuncCall(fn={}, id={}) key={key:?} inst={inst_name:?} left_pts={left_pts:?}",
                            self.name, fc.func_name, fc.id
                        );
                    }
                }
            }

            // Single series element (or none): the main gap loop below is
            // `0..n-1`, so it naturally no-ops; chain-head / chain-tail
            // bridges are still handled by the leading / trailing branches,
            // keeping their semantic position (§11 strict vector order).

            // Wire series elements in order. Bridge pins at position i+1 are
            // attached to the net between series[i] and series[i+1].
            // bridges_at[0] = leading bridges, bridges_at[k] = gap between series[k-1] and series[k]
            let n = series_elems.len();

            // ── P2-7: handle leading bridges (bridges_at[0]) ──
            // Attach leading bridges to the first series element's left points.
            // §11 strict vector order: a chain-head bridge (`CAP' -> A`) is
            // written before A, so its pins come first: `CAP.1 -> A.left`,
            // not `A.left -> CAP.1`.
            if let Some(leading) = bridges_at.first() {
                if !leading.is_empty() {
                    let first_left = self.get_left_points(series_elems[0]).unwrap_or_default();
                    if let Some(lp) = Self::pick_lane_point(&first_left, lane) {
                        let mut all_pts = Vec::with_capacity(leading.len() + 1);
                        all_pts.extend(leading.iter().cloned());
                        all_pts.push(lp);
                        if all_pts.len() >= 2 {
                            let id = self.next_conn_id();
                            self.add_connection(self.make_conn_with_provenance(
                                id,
                                all_pts,
                                dir,
                                Some(lane as u16),
                            ));
                        }
                    }
                }
            }

            for i in 0..n.saturating_sub(1) {
                let left_pts = self.get_right_points(series_elems[i]).unwrap_or_default();
                let right_pts = self
                    .get_left_points(series_elems[i + 1])
                    .unwrap_or_default();
                if left_pts.is_empty() || right_pts.is_empty() {
                    continue;
                }

                let bridge_pins = if i + 1 < bridges_at.len() {
                    &bridges_at[i + 1]
                } else {
                    continue;
                };

                let lp = match Self::pick_lane_point(&left_pts, lane) {
                    Some(p) => p,
                    None => continue,
                };
                let rp = match Self::pick_lane_point(&right_pts, lane) {
                    Some(p) => p,
                    None => continue,
                };

                if !bridge_pins.is_empty() {
                    // §11 strict vector order: the bridge is a series element
                    // between the left and right elements, so its pins belong
                    // in the gap — the expanded point order must match the
                    // chain evaluation order
                    // (`A -> CAP' -> B` → `A.1, CAP.1, B.1`, not `A.1, B.1, CAP.1`).
                    let mut all_pts = vec![lp.clone()];
                    all_pts.extend(bridge_pins.iter().cloned());
                    all_pts.push(rp.clone());
                    let id = self.next_conn_id();
                    self.add_connection(self.make_conn_with_provenance(
                        id,
                        all_pts,
                        dir,
                        Some(lane as u16),
                    ));
                } else {
                    self.create_connection(vec![lp], vec![rp], dir, Some(lane as u16))?;
                }
            }

            // ── M11.4: handle trailing bridges (after the last series element) ──
            // §11 strict vector order: a chain-tail bridge (`A -> CAP'`) is
            // written after the last series element, so its pins follow that
            // element's right points: `B.right -> CAP.1`.
            if !pending_bridges.is_empty() {
                if let Some(last_elem) = series_elems.last() {
                    let last_right = self.get_right_points(last_elem).unwrap_or_default();
                    if let Some(rp) = Self::pick_lane_point(&last_right, lane) {
                        let mut all_pts = Vec::with_capacity(pending_bridges.len() + 1);
                        all_pts.push(rp);
                        all_pts.extend(pending_bridges.iter().cloned());
                        if all_pts.len() >= 2 {
                            let id = self.next_conn_id();
                            self.add_connection(self.make_conn_with_provenance(
                                id,
                                all_pts,
                                dir,
                                Some(lane as u16),
                            ));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Pick the lane-specific point from a list of points.
    /// For multi-pin endpoints (e.g. XTAL interface with 2 pins),
    /// get_left_points/get_right_points returns all points. This helper
    /// picks the point at index `lane`, falling back to index 0 if the
    /// lane index is out of bounds.
    fn pick_lane_point(pts: &[NetPoint], lane: usize) -> Option<NetPoint> {
        if pts.is_empty() {
            None
        } else if lane < pts.len() {
            Some(pts[lane].clone())
        } else {
            Some(pts[0].clone())
        }
    }

    // ── M11.4: collect lane items (series elements + bridge pins) preserving order ──
    fn collect_lane_items<'a>(
        &mut self,
        members: &'a [McPhrase],
        lane: usize,
    ) -> Vec<LaneItem<'a>> {
        let mut items: Vec<LaneItem<'a>> = Vec::new();
        for member in members {
            self.collect_one_lane_item(member, lane, &mut items);
        }
        items
    }

    fn collect_one_lane_item<'a>(
        &mut self,
        member: &'a McPhrase,
        lane: usize,
        items: &mut Vec<LaneItem<'a>>,
    ) {
        match member {
            McPhrase::Multiple(inner) => {
                if lane < inner.len() {
                    let p = &inner[lane];
                    if !matches!(p, McPhrase::Lead) {
                        items.push(LaneItem::Series(p));
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
                                    items.push(LaneItem::Series(p));
                                }
                            }
                        }
                        McPhrase::Transposed(inner) => {
                            if let Some(pin) = self.get_transposed_lane_pin(stmt, lane) {
                                items.push(LaneItem::Bridge(pin));
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
                                items.push(LaneItem::Series(stmt));
                            }
                        }
                    }
                }
            }
            McPhrase::Transposed(inner) => {
                // M11.4: standalone Transposed in chain acts as bridge passive
                if let Some(pin) = self.get_transposed_lane_pin(member, lane) {
                    items.push(LaneItem::Bridge(pin));
                }
                self.try_record_bridge_passive(inner);
            }
            McPhrase::Group(g) => {
                // M11.4: expand Group's opds per lane, same as Multiple.
                // Each opd is a lane item (e.g. (RES(),RES()) gives RES1 to lane 0,
                // RES2 to lane 1). Lead (_) elements are skipped.
                if let Some(p) = g.opds.get(lane) {
                    if !matches!(p, McPhrase::Lead) {
                        items.push(LaneItem::Series(p));
                    }
                }
            }
            // ── P2-7: bus endpoint (e.g. XTAL interface with 2 pins) ──
            // Treat as multi-lane series element: each lane gets its own pin.
            // The lane-specific pin is picked in wire_chain_lane_by_lane via
            // pick_lane_point.
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref bus),
                ..
            })) if !bus.member.is_empty() => {
                if lane < bus.member.len() {
                    items.push(LaneItem::Series(member));
                }
            }
            _ => {
                if lane == 0 {
                    items.push(LaneItem::Series(member));
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
        if let Some(inst_name) = self.auto_inst_map.get(&key).cloned() {
            if let Some(stripped) = inst_name.strip_prefix("@@ARRAY:") {
                for name in stripped.split(',') {
                    self.bridge_passive_names.insert(name.to_string());
                }
            } else {
                self.bridge_passive_names.insert(inst_name);
            }
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

    /// Convert McPhrase to expanded McPhrase list
    /// Series is recursively expanded to individual member McPhrases
    pub(super) fn phrase_to_members(&self, phrase: &McPhrase) -> Vec<McPhrase> {
        let disc = std::mem::discriminant(phrase);
        mcc_dbg!(
            "inst::mod",
            "[P2-5-PTM-ENTRY] module='{}' phrase_to_members: disc={disc:?}",
            self.name
        );
        match phrase {
            McPhrase::Series(phrases, _) => {
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
                let mut result = Vec::new();
                for p in phrases {
                    match p {
                        McPhrase::Multiple(inner) => {
                            let transformed_inner: Vec<McPhrase> = inner
                                .iter()
                                .flat_map(|ip| self.phrase_to_members(ip))
                                .collect();
                            result.push(McPhrase::Multiple(transformed_inner));
                        }
                        _ => result.extend(self.phrase_to_members(p)),
                    }
                }

                // ── Iter-6.S5.1 P0-2 scenario C ─────────────────────────
                // merge adjacent same-name single-member Bus phrases.
                //
                // Background: parser for `Name{a, b, ...}` in certain scenarios (especially stmt
                // start position + Name is io/out/in declared Label-type port) expansion
                // is inconsistent — expected to produce ONE Bus(Name, [a, b, ...]), actually
                // produces [Bus(Name, [a]), Bus(Name, [b]), ...] multiple adjacent
                // phrases entering Series.
                //
                // Verified case (main.mc:147):
                //   `MIC{P,N} -> cap[4:5]::CAP(1uF) -> uC.ADC{P,N}`
                //   - stmt end `uC.ADC{P,N}` parsed correctly: Bus(uC.ADC, [P, N]) single
                //     phrase (variants log: Bus(name='uC.ADC' members=[P,N]))
                //   - stmt start `MIC{P,N}` parsed incorrectly: split into two phrases
                //     [Bus(MIC, [P]), Bus(MIC, [N])]
                //   - chain total members from expected 3 becomes 4
                //   - adjacency wiring rules treat chain[0] = MIC.P ↔ chain[1] = MIC.N
                //     as "normal pair", **shorting P and N together**
                //     (Net Table: `MIC.P : MIC.P ~ MIC.N`)
                //
                // Fix: after phrase_to_members flattens result Series, do
                // one pass fix-up — only for **fully recognizable parser split traces**:
                //   prev and curr are both Endpoint::Single(Bus(_)) and outer
                //   members empty, same name, curr exactly 1 member, prev at least
                //   1 member (allows cascading accumulation).
                //
                // This rule **won't** falsely hit legitimate cases:
                //   - `MIC.P -> MIC.N` (dot access): names are "MIC.P" / "MIC.N"
                //     different names, won't trigger.
                //   - `mic{1,2} -> CAP(_).Cap(_) -> MIC{P,N}` (parser already
                //     correctly handles stmt-end curly as single Bus(MIC, [P, N])): adjacent phrase
                //     name different (mic vs MIC), won't trigger.
                //   - `mcu{ MIC | DAC_OUT, SPK_MUTE }` (Node form): not
                //     Single(Bus), won't trigger.
                //   - `[VDD_3V3, GND]` (List/Multiple form): not Single(Bus),
                //     won't trigger.
                //
                // **Only possible false hit**: user writes `MIC{P} -> MIC{N}` wanting P direct-connect N.
                // This would be merged into Bus(MIC, [P, N]) single phrase, losing P↔N adjacency.
                // This notation is virtually non-existent in engineering practice — standard
                // notation for P↔N direct connection is `MIC.P -> MIC.N` (dot not curly), latter won't trigger
                // this rule.
                //
                // Note: the long-term correct fix is to fix parser/`dot_or_curly` for Label/Port
                // handling consistency (mc_phrase.rs:1462-1470). But parser chain involves
                // upstream AST input and symbol table interaction, large change surface; doing fix-up
                // at phrase_to_members layer is surgical and can be rolled back cost-free after parser fix.
                Self::merge_adjacent_curly_split(&mut result);

                // ── M11.5: expand merged multi-member Buses to Multiple ──
                // After merge_adjacent_curly_split, Buses like dc{VDD_3V3, GND}
                // may have multiple members.  Expand them to Multiple so
                // lane-by-lane wiring can handle each lane independently.
                Self::expand_multi_member_buses(&mut result);

                result
            }
            McPhrase::Parallel(phrases) => {
                vec![McPhrase::Parallel(phrases.clone())]
            }
            McPhrase::Closure(c) => {
                vec![McPhrase::Closure(c.clone())]
            }
            McPhrase::FuncCall(f) => {
                vec![McPhrase::FuncCall(f.clone())]
            }
            McPhrase::Group(g) => {
                vec![McPhrase::Group(g.clone())]
            }
            McPhrase::Transposed(inner) => {
                vec![McPhrase::Transposed(Box::new((**inner).clone()))]
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
                    return vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new_with_members(&inst_name, expanded)),
                    )))];
                }

                // ── Iter-7.5b ────────────────────────────────────────────
                // System library CAP/RES/IND/DIODE 2-pin classes use dynamic_pins to declare
                // pins, class def c.base.pins static pins HashMap is empty,
                // count() returns 0, but actually 2-pin.
                // Tighten criteria: class name whitelist OR anonymous @ prefix, avoid false-hitting lpa/flash
                // multi-pin dynamic components (they also satisfy has_dynamic_pins but aren't 2-pin).
                //
                // ── ★ P0-2: list moved to naming::is_known_twopin_class (single source of truth) ──
                let class_name = c.base.name.to_string();
                let is_known_2pin_class =
                    crate::vector::graph::naming::is_known_twopin_class(&class_name);
                let is_anon_inst = inst_name.starts_with('@');
                let static_count = c.base.pins.count();
                let dyn_two_pin = static_count == 0
                    && c.base.pins.has_dynamic_pins()
                    && (is_known_2pin_class || is_anon_inst);

                match (static_count, dyn_two_pin) {
                    (2, _) | (_, true) => vec![McPhrase::Endpoint(McEndpoint::Node {
                        input: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            McBus::new(&format!("{inst_name}.1")),
                        )))],
                        output: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            McBus::new(&format!("{inst_name}.2")),
                        )))],
                    })],
                    _ => vec![McPhrase::from(McInstance::Bus(McBus::new(&inst_name)))],
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
                // collapsed back to single-point "speaker" broadcast to same net as chain other side.
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
                    let sub_opt = self.sub_modules.iter().find(|s| s.name == inst_name);
                    let mut input: Vec<McEndpoint> = Vec::new();
                    let mut output: Vec<McEndpoint> = Vec::new();
                    for m_name in &expanded {
                        let path = format!("{inst_name}.{m_name}");
                        let ep = McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            McBus::new(&path),
                        )));
                        let iotype = sub_opt
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
                    return vec![McPhrase::Endpoint(McEndpoint::Node { input, output })];
                }

                // ── P1-A2 ────────────────────────────────────────────────
                // Bare module reference `V3V3 -> dcdc -> V1V2`: need to split module into
                // Node (in side / out side), so `dcdc` two sides don't get
                // union-find merged into one big net.
                //
                // Prefer declared submodule instance ports (pass2 reliable data),
                // m.base.insts is empty on some parse paths, can't rely on it.
                let (left, right): (Vec<McBus>, Vec<McBus>) =
                    if let Some(sub) = self.sub_modules.iter().find(|s| s.name == inst_name) {
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
                    vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new(&inst_name)),
                    )))]
                } else {
                    vec![McPhrase::Endpoint(McEndpoint::Node {
                        input: left
                            .iter()
                            .map(|bus| {
                                McEndpoint::Single(McInstanceRef::new(McInstance::Bus(bus.clone())))
                            })
                            .collect(),
                        output: right
                            .iter()
                            .map(|bus| {
                                McEndpoint::Single(McInstanceRef::new(McInstance::Bus(bus.clone())))
                            })
                            .collect(),
                    })]
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
                    return vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new_with_members(&inst_name, expanded)),
                    )))];
                }

                vec![McPhrase::from(McInstance::Bus(McBus::new(&inst_name)))]
            }
            McPhrase::Lead => vec![McPhrase::Lead],
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
                        return vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(&data.name)),
                        )))];
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
                    vec![McPhrase::Multiple(inner)]
                } else {
                    vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(data.clone()),
                    )))]
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
                            return vec![McPhrase::Multiple(inner)];
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
                    vec![McPhrase::Multiple(inner)]
                } else {
                    vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new(label)),
                    )))]
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(list),
                ..
            })) => vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(McBus::new_with_members(&list.name, list.member.clone())),
            )))],
            McPhrase::Multiple(inner) => {
                let mut result = Vec::new();
                for p in inner {
                    result.extend(self.phrase_to_members(p));
                }
                result
            }
            McPhrase::Endpoint(ref ep) => {
                // ── §11.3 lane-structured List: N independent member lanes ──────
                // `cap[4:5]` resolves at pass1 to
                // `Endpoint(List([Single(cap4), Single(cap5), ...]))`. Do NOT
                // collapse through get_left/get_right — those take only the
                // first/last lane and turn the parallel lane group into a serial
                // `cap4 → cap5` Node (the flatten-before-broadcast pitfall). Pass
                // the List through so the
                // array-form re-link (resolve_array_caller_to_existing) and the
                // get_left/get_right_points List handlers consume the lanes
                // structurally.
                if matches!(ep, McEndpoint::List(_)) {
                    return vec![McPhrase::Endpoint(ep.clone())];
                }
                mcc_dbg!("inst::mod",
                    "[P2-5-BUS-CATCHALL] module='{}' phrase_to_members Endpoint catch-all: ep={ep:?}",
                    self.name
                );
                let left = ep.get_left();
                let right = ep.get_right();
                if left.is_empty() && right.is_empty() {
                    vec![McPhrase::Endpoint(ep.clone())]
                } else if left.len() == 1 && right.len() == 1 {
                    vec![McPhrase::Endpoint(McEndpoint::Node {
                        input: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            left[0].clone(),
                        )))],
                        output: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            right[0].clone(),
                        )))],
                    })]
                } else {
                    vec![McPhrase::Endpoint(ep.clone())]
                }
            }
            McPhrase::Member(inner, member_ep) => {
                // ── P2-4 fix: keep Member for ALL cases, not just FuncCall ──
                // Previously only FuncCall inners kept the Member wrapper (e.g.
                // `X6.setup(GND, NC).XTAL`), while non-FuncCall inners like
                // `uC.XTAL` (Member(Endpoint(Component(uC)), "XTAL")) were stripped,
                // losing the XTAL member name and causing all XTAL pins to merge
                // into one net instead of lane-by-lane matching.
                let result = vec![McPhrase::Member(inner.clone(), member_ep.clone())];
                result
            }
        }
    }

    /// ── Iter-6.S5.1 helper ─────────────────────────────────────────────
    /// Merge adjacent same-name single-member Bus phrases. See `phrase_to_members` Series branch
    /// Iter-6.S5.1 comment block for details.
    ///
    /// Trigger conditions (all must be satisfied):
    ///   1. prev and curr are both `Endpoint::Single(Bus(_))`;
    ///   2. McInstanceRef outer `members` field both empty (no additional outer
    ///      member modifier);
    ///   3. prev_bus and curr_bus same name;
    ///   4. curr_bus exactly 1 member (this is parser split trace fingerprint);
    ///   5. prev_bus at least 1 member (allows cascading accumulation: 1-1 → 2, 2-1 → 3, ...).
    ///
    /// Behavior: merge curr_bus member into prev_bus, delete curr. Continue from same
    /// index position forward, achieving chain accumulation.
    fn merge_adjacent_curly_split(members: &mut Vec<McPhrase>) {
        if members.len() < 2 {
            return;
        }
        let mut i = 1;
        while i < members.len() {
            // immutable borrow scope: extract data to be merged from curr to prev
            let merge_data = {
                let prev = &members[i - 1];
                let curr = &members[i];
                Self::extract_curly_split_merge_data(prev, curr)
            };
            if let Some((mem, full)) = merge_data {
                // Now do mutable borrow, merge into prev
                if let McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Bus(prev_bus_mut),
                    ..
                })) = &mut members[i - 1]
                {
                    prev_bus_mut.member.extend(mem);
                    prev_bus_mut.full_members.extend(full);
                }
                members.remove(i);
                // Don't increment i, allow cascading merge (the new members[i]
                // will be compared again with the extended members[i-1])
            } else {
                i += 1;
            }
        }
    }

    /// Pure check + data extraction part of `merge_adjacent_curly_split`.
    /// Returns `Some((curr.member.clone(), curr.full_members.clone()))` to
    /// indicate should merge; `None` to indicate should not merge.
    fn extract_curly_split_merge_data(
        prev: &McPhrase,
        curr: &McPhrase,
    ) -> Option<(Vec<String>, Vec<String>)> {
        let (prev_bus, prev_outer) = match prev {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(b),
                members,
            })) => (b, members),
            _ => return None,
        };
        let (curr_bus, curr_outer) = match curr {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(b),
                members,
            })) => (b, members),
            _ => return None,
        };
        if prev_outer.is_empty()
            && curr_outer.is_empty()
            && !prev_bus.member.is_empty()
            && curr_bus.member.len() == 1
            && prev_bus.name == curr_bus.name
        {
            Some((curr_bus.member.clone(), curr_bus.full_members.clone()))
        } else {
            None
        }
    }

    /// ── M11.5: expand multi-member Buses into Multiple ──────────────────
    /// After merge_adjacent_curly_split, Buses may have multiple members
    /// (e.g. dc{VDD_3V3, GND}).  Expand them to Multiple so lane-by-lane
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

    /// Try to connect adjacent members
    ///
    /// Helper method extracted from `process_stmt`, handling Group / normal
    /// member connection dispatch. On failure the caller `process_stmt` catches
    /// and records the diagnosis.
    ///
    /// Also re-links bracket-form array instance references (`cap[4:5] -> ...`)
    /// to the already-declared instances; see the re-link block in the body.
    fn try_connect_adjacent(
        &mut self,
        left_member: &McPhrase,
        right_member: &McPhrase,
        dir: ConnDir,
    ) -> Result<(), InstError> {
        // ★ P9-A2: extract trunk from source code context.
        // Prefer the left member (driver side), fall back to the right member.
        // RAII (§7.11(2)): the group is restored on every exit path (including
        // early `Err` returns), so it can never leak into the next connection.
        let trunk = Self::extract_trunk_group(left_member)
            .or_else(|| Self::extract_trunk_group(right_member));
        let trunk_kind = Self::extract_trunk_kind(left_member)
            .or_else(|| Self::extract_trunk_kind(right_member))
            .or_else(|| trunk.as_ref().map(|_| TrunkKind::Plain));
        let trunk_iface = self
            .extract_trunk_iface(left_member)
            .or_else(|| self.extract_trunk_iface(right_member));
        self.with_trunk(trunk, trunk_kind, trunk_iface, |this| {
            // ── P1-diag: detailed adjacent wiring diagnostic ─────────────────────────────────
            let _l_kind = match left_member {
                McPhrase::FuncCall(f) => format!(
                    "FuncCall(fn={}, caller={}, right_n={})",
                    f.func_name,
                    f.caller
                        .as_ref()
                        .map(|c| format!("{:?}", std::mem::discriminant(c.as_ref())))
                        .unwrap_or("None".into()),
                    f.right.len()
                ),
                McPhrase::Endpoint(e) => format!("Endpoint({e:?})"),
                McPhrase::Parallel(v) => format!("Parallel(len={})", v.len()),
                McPhrase::Group(g) => format!("Group(opds={})", g.opds.len()),
                _ => format!("{:?}", std::mem::discriminant(left_member)),
            };
            let _r_kind = match right_member {
                McPhrase::FuncCall(f) => {
                    format!("FuncCall(fn={}, right_n={})", f.func_name, f.right.len())
                }
                McPhrase::Endpoint(e) => format!("Endpoint({e:?})"),
                _ => format!("{:?}", std::mem::discriminant(right_member)),
            };

            // ── Array-form instance reference re-link ─────────────────────────
            // Plain Series statements (`cap[4:5] -> PWR{VCC, GND}` / `cap[4] -> GND`)
            // do not pass through the FuncCall path (stmt.rs resolve_array_caller_to_existing),
            // so the bracket literal used to fall straight into node_to_netpoint and get
            // quarantined as `@_phantom_N`, silently dropping the reference to the
            // already-declared array instances. Re-link the bracket form to the
            // existing instances here and connect each one to the other side.
            if let Some(array_names) = this.resolve_array_caller_to_existing(left_member) {
                let right_points = this.get_left_points(right_member)?;
                for name in array_names {
                    let left_points = match this.find_component(&name) {
                        Some(comp) => comp.get_right_pin().map(|p| vec![p]).unwrap_or_default(),
                        None => Vec::new(),
                    };
                    if left_points.is_empty() {
                        continue;
                    }
                    this.create_connection(left_points, right_points.clone(), dir, None)?;
                }
                return Ok(());
            }
            if let Some(array_names) = this.resolve_array_caller_to_existing(right_member) {
                let left_points = this.get_right_points(left_member)?;
                for name in array_names {
                    let right_points = match this.find_component(&name) {
                        Some(comp) => comp.get_left_pin().map(|p| vec![p]).unwrap_or_default(),
                        None => Vec::new(),
                    };
                    if right_points.is_empty() {
                        continue;
                    }
                    this.create_connection(left_points.clone(), right_points, dir, None)?;
                }
                return Ok(());
            }

            let left_is_group = matches!(left_member, McPhrase::Group { .. });
            let right_is_group = matches!(right_member, McPhrase::Group { .. });

            if right_is_group {
                let external_points = this.get_right_points(left_member)?;
                this.connect_to_group(external_points, right_member, true, dir)?;
            } else if left_is_group {
                let external_points = this.get_left_points(right_member)?;
                this.connect_to_group(external_points, left_member, false, dir)?;
            } else {
                let left_points = this.get_right_points(left_member)?;
                let right_points = this.get_left_points(right_member)?;
                // Explicit empty-port guard: a single-ended member or an
                // `<error` endpoint expands to an empty point list. This is not
                // an "unknown shape" to wildcard through opcheck — there is
                // simply nothing to connect (`create_connection` no-ops on an
                // empty side). Return early so `Shape::vvec` only receives
                // `len >= 1` and never relies on the `Shape::vvec(0) ==
                // Shape::unknown` coincidence as an implicit wildcard.
                if left_points.is_empty() || right_points.is_empty() {
                    return Ok(());
                }
                let lhs_shape = Shape::vvec(left_points.len());
                let rhs_shape = Shape::vvec(right_points.len());
                // ── §5.2 series legality (vec-dianlu.md): unified check shared
                // with Pass1 (`opcheck`), so the two passes can never drift.
                // A transposed member is first transposed to its full-width
                // column — its point list (`get_right_points` /
                // `get_left_points` merge the inner left + right pins) is
                // already the transposed result — so this check runs on the
                // transposed result with no pair-by-min / lane-hang carve-out.
                // Legal: equal rows only. Illegal: unequal rows — including the
                // single-point broadcast `1*1` vs `N*1` (`X -> [A, B]`) and any
                // transposed mismatch — report the error and generate no
                // connection statement (no truncation / pair-by-min recovery).
                let verdict = crate::semantic::opcheck::check_series_rows(lhs_shape, rhs_shape);
                if matches!(verdict, crate::semantic::opcheck::OpCheck::Legal(_)) {
                    // Row counts match: pair the whole group (create_connection
                    // does 1:1 / interface expansion internally).
                    this.create_connection(left_points, right_points, dir, None)?;
                } else {
                    // Illegal §5.2 operation (unequal rows — including the
                    // single-point broadcast `1*1` vs `N*1` and transposed
                    // mismatches): report the error and generate no statement —
                    // the row mismatch is not truncated into a partial pairing.
                    this.record_error(
                        crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                        crate::errcodes::format_msg(
                            crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                }
            }
            Ok(())
        })
    }

    /// Iter-7.1: make the internal parallel wiring of `A + B + C + ...` explicit
    ///
    /// # Semantics (summary of bugfix_report errors 5/9/10/12)
    ///
    /// `+` is "take operand 1's parallel connection", but **sensitive to
    /// operand k (k≥2) endpoint width**:
    ///
    /// 1. **Use opd[0] as anchor**: opd[0]'s left_points as "left net" seed,
    ///    right_points as "right net" seed.
    /// 2. **Operand k is double-ended** (same dimension as opd[0], e.g. `R101 + R102`,
    ///    `XTAL{X1,X2} + R442::RES'`): k.left zipped to left net, k.right zipped
    ///    to right net (position-corresponding).
    /// 3. **Operand k is single-ended** (left == right, or left.len==right.len==1
    ///    and paths are equal, e.g. IN.P in `lpa.BYPASS + lpa.IN.P`, or
    ///    spk.N in `R30k -> lpa.VO1 + spk.N`): **only attached to left net**
    ///    (i.e. opd[0]'s left end). This is consistent with bugfix_report
    ///    error 9: "§10.1 take operand 1, spk.N should connect to R30k's left end".
    /// 4. **Dimension mismatch** (e.g. opd[0] is 1 wide, opd[1] is 2 wide):
    ///    degrade to single-ended rule —— merge all opd[k] endpoints into the
    ///    left net, with warning.
    ///
    /// # Test case verification
    ///
    /// | Source snippet | Anchor (opd[0]) left/right | opd[k] form | Result |
    /// |---|---|---|---|
    /// | `(VBUS -> USB_VBUS) + TP1` | left=VBUS / right=USB_VBUS | TP1 single-end | TP1 → left net (VBUS) |
    /// | `lpa.BYPASS + lpa.IN.P` | both bare labels (single-end) | IN.P single-end | IN.P → left net (BYPASS) |
    /// | `(CAP1nF + R10k) -> GND` | CAP.1 / CAP.2 | R10k double-end | R10k.1→left, R10k.2→right |
    /// | `XTAL + R442::RES'` | XTAL.X1, X2 (2 wide) | R442' also 2 wide | X1↔R442.1, X2↔R442.2 |
    /// | `R30k -> lpa.VO1 + spk.N` | R30k.1 / lpa.VO1 | spk.N single-end | spk.N → left net (R30k.1) |
    ///
    /// # Notes
    /// - This method assumes `stmts.len() >= 2`, please check before calling.
    /// - Single-end right net degradation avoidance: if all opds are
    ///   single-end (left/right paths equal), only generate left net, don't
    ///   repeat the right net (they have identical node sets).
    fn wire_parallel_internal(&mut self, stmts: &[McPhrase]) -> Result<(), InstError> {
        // 1) Collect each opd's left/right endpoints
        let mut opd_lefts: Vec<Vec<NetPoint>> = Vec::with_capacity(stmts.len());
        let mut opd_rights: Vec<Vec<NetPoint>> = Vec::with_capacity(stmts.len());

        for (_idx, opd) in stmts.iter().enumerate() {
            // ── Skip Lead placeholder ────────────────────────────────────────
            // In Parallel with `_` like `(_, A, B)`, `_` is parsed as Lead,
            // doesn't participate in parallel wiring.
            if matches!(opd, McPhrase::Lead) {
                opd_lefts.push(Vec::new());
                opd_rights.push(Vec::new());
                continue;
            }
            // ── Use the same rule as try_connect_adjacent to get endpoints ───────────
            // i.e. call self.get_left_points / get_right_points (top-level version,
            // going through auto_inst_map), not _from_phrase, so that stubs /
            // already-instantiated anonymous 2-pin elements can be correctly resolved.
            //
            // Note: now points.rs::Parallel is changed to only take opds[0], so
            // nested Parallel here will also fall into the correct semantics
            // (recursively take the first branch).
            //
            // ── Iter-7.5d ────────────────────────────────────────────
            // Component endpoint form (like `@CAP5`/`@RES6` embedded in chain)
            // has no dedicated branch in points.rs::get_left_points, falls to
            // fallback returning empty. This causes wire_parallel_internal
            // to early-exit without getting endpoints when paralleling inline
            // anonymous 2-pin elements like (CAP + RES), losing the internal net.
            //
            // Fix: before taking endpoints, use phrase_to_members to normalize opd,
            // it does 7.5b judgment for Component (known 2-pin classes like
            // CAP/RES/IND or anonymous instances) → outputs Endpoint::Node{.1, .2},
            // points.rs can recognize the Node branch. Multi-pin user components
            // (lpa/flash etc.) phrase_to_members degenerates to single-point Bus,
            // behavior consistent with original, no impact.
            //
            // phrase_to_members usually returns 1 element (not Series); just take
            // the first.
            //
            // auto_inst_map is indexed by pointer address (member_key); phrase_to_members
            // will clone FuncCall → new address → resolve_funccall_*_points can't
            // find the registered @?TYPE_n → falls back to TYPE.in/TYPE.out
            // placeholders (= P3 leak). FuncCall must use the original opd address
            // to take points; other forms (Component endpoint etc.) still go through
            // Iter-7.5d normalization.
            let (lps, rps) = match opd {
                McPhrase::FuncCall(_) => (
                    self.get_left_points(opd).unwrap_or_default(),
                    self.get_right_points(opd).unwrap_or_default(),
                ),
                _ => {
                    // ── BUG4 fix (in conjunction with Group/Parallel handler in-place instantiation) ──
                    // FuncCall in branches like Series is now instantiated on the
                    // **original opd pointer** (see process_member_internal::Parallel/Group).
                    // First use the original opd to take points (get_left_points will
                    // recurse into FuncCall to query auto_inst_map, original pointer
                    // hits the real @?TYPE_n); if hit use it. Only when the original
                    // pointer can't get points (pure Endpoint(Component)/Label etc.
                    // forms that don't enter auto_inst_map) fall back to
                    // phrase_to_members normalization path (Iter-7.5d: Component endpoint → Node).
                    let lp0 = self.get_left_points(opd).unwrap_or_default();
                    let rp0 = self.get_right_points(opd).unwrap_or_default();
                    if !lp0.is_empty() || !rp0.is_empty() {
                        (lp0, rp0)
                    } else {
                        let normalized_opds = self.phrase_to_members(opd);
                        let p: &McPhrase = normalized_opds.first().unwrap_or(opd);
                        (
                            self.get_left_points(p).unwrap_or_default(),
                            self.get_right_points(p).unwrap_or_default(),
                        )
                    }
                }
            };
            opd_lefts.push(lps);
            opd_rights.push(rps);
        }

        // 2) Anchor operand 1 (opd[0]). If opd[0]'s endpoints are empty,
        //    find the next non-empty as the anchor (Lead-skip fallback).
        let anchor_idx = (0..stmts.len()).find(|&i| !opd_lefts[i].is_empty());
        let anchor_idx = match anchor_idx {
            Some(i) => i,
            None => {
                return Ok(());
            }
        };

        let anchor_left = opd_lefts[anchor_idx].clone();
        let anchor_right = opd_rights[anchor_idx].clone();
        let anchor_dim = anchor_left.len();

        // 3) Accumulate non-anchor opd endpoints into left/right net
        let mut left_net: Vec<NetPoint> = anchor_left.clone();
        let mut right_net: Vec<NetPoint> = anchor_right.clone();

        // Single-ended: left/right length equal and paths exactly equal
        // (typical: bare label, e.g. TP1, BYPASS)
        // Note: cannot simply use "left.len() == 1" to judge, because a 1-wide
        // double-end component may also have left.len()=1 (but left[0].path != right[0].path)
        let is_single_ended = |l: &[NetPoint], r: &[NetPoint]| -> bool {
            l.len() == r.len() && l.iter().zip(r.iter()).all(|(a, b)| a.path == b.path)
        };

        // 4) Whether dimension mismatch needs a zip-mismatch error
        let mut dim_mismatch_reported = false;

        for i in 0..stmts.len() {
            if i == anchor_idx {
                continue;
            }
            let lp = &opd_lefts[i];
            let rp = &opd_rights[i];
            if lp.is_empty() && rp.is_empty() {
                continue; // Lead or empty opd
            }

            let opd_single = is_single_ended(lp, rp);

            // ── P1 fix (Transposed not single-ended) ───────────────────────
            // Transposed merges left+right into both lp and rp, so
            // is_single_ended always returns true. This causes the opd to
            // be broadcast to every lane instead of distributed lane-by-lane
            // as a bridge/shunt element. Detect Transposed explicitly and
            // push lp as-is to left_net (rp == lp, so skip right_net).
            let is_transposed = matches!(stmts[i], McPhrase::Transposed(_));

            if is_transposed {
                // Transposed bridge element: lp already contains all pins
                // (left+right merged). Push to left_net for lane-slice
                // distribution; do NOT push to right_net (rp == lp).
                left_net.extend(lp.iter().cloned());
            } else if opd_single {
                // Single-end opd (a bare label / test point / single-pin net
                // node, e.g. TP1 in `(VBUS -> USB_VBUS) + TP1`, spk.N in
                // `R30k -> lpa.VO1 + spk.N`).
                //
                // Attach to the anchor's OUTPUT side (right net), not the left.
                // A chain's result is its right port (design rule: `A -> B`
                // evaluates to B, the last endpoint), so a single-end label
                // parallels the chain EXIT, not the entry. This matches the
                // golden netlists (POWER_USB: TP1 on USB_VBUS; SPEAKER_M:
                // spk.N on VO1). When the anchor is single-ended (left == right,
                // e.g. `BYPASS + IN.P`) the two nets are identical, so attaching
                // to left is equivalent — keep the left branch for that case.
                //
                // When the anchor is double-ended N wide, the single-end point
                // needs to be "broadcast" to all N lanes (replicated N times),
                // so subsequent zip splitting correctly distributes it to each
                // lane's right end.
                let anchor_is_chain = !is_single_ended(&anchor_left, &anchor_right);
                if anchor_dim >= 2 && anchor_is_chain {
                    for _ in 0..anchor_dim {
                        right_net.extend(lp.iter().cloned());
                    }
                } else if anchor_is_chain {
                    right_net.extend(lp.iter().cloned());
                } else {
                    left_net.extend(lp.iter().cloned());
                }
            } else if anchor_dim >= 2 && lp.len() + rp.len() == anchor_dim {
                // ── Iter-7.3 ─────────────────────────────────────────────
                // Implicit transpose: anchor is N wide (like bus port `XTAL{X1, X2}`
                // or real double-end list), opd's (left + right) total point count
                // exactly equals anchor width. This is the user's writing
                // **without explicit `'` transpose** in scenarios like
                // `XTAL + R442::RES(1MΩ)` (the canonical syntax per rules §10.5
                // is `XTAL + R442::RES(1MΩ)'`, but engineers often omit it in
                // practice, see main.mc:82).
                //
                // Handling: treat opd's left ++ right as N×1 view and zip with
                // anchor. Equivalent to the compiler automatically adding `'`.
                //
                // Example anchor_dim=2:
                //   opd = R442 (Component, lp=[R442.1], rp=[R442.2])
                //   → concatenated into [R442.1, R442.2] this 2-wide view
                //   → zipped with [X1, X2] → {X1, R442.1} + {X2, R442.2}
                //
                // Trigger conditions **only check anchor_dim >= 2 and lp+rp == anchor_dim**:
                //   - anchor_dim >= 2 excludes regular `R101 + R102 + R103` (anchor=1)
                //   - lp+rp == anchor_dim strict match, to avoid false hits on other forms
                //   - **Don't** check whether anchor is single-ended: XTAL such
                //     bus port, although left==right (the port itself is a net
                //     label, no .1/.2 concept), still needs to split X1/X2
                //     into independent nets by lane.
                //
                // Since opd's left half (lp) connects to anchor's left lane,
                // right half (rp) connects to anchor's right lane, push lp+rp
                // as a whole into left_net (it will naturally be distributed
                // to each lane via lane splitting), right_net **does not
                // increase** (this opd is essentially equivalent to an
                // implicitly transposed element, its "two ends" are already
                // placed in the left lane).
                left_net.extend(lp.iter().cloned());
                left_net.extend(rp.iter().cloned());
            } else if lp.len() == anchor_dim && rp.len() == opd_rights[anchor_idx].len() {
                // Double-end opd same dimension as anchor: zip to left/right net
                left_net.extend(lp.iter().cloned());
                right_net.extend(rp.iter().cloned());
            } else {
                // Dimension mismatch (double-end but different widths): a
                // row-count mismatch that slipped past Pass1 (e.g. dynamic
                // pins / interface expansion). Report the error
                // (vec-dianlu.md §5.1 left-alignment) and generate no
                // connection for this operand — it is not merged into the
                // left/right net, so no statement is emitted for it.
                if !dim_mismatch_reported {
                    self.record_error(
                        crate::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,
                        crate::errcodes::format_msg(
                            crate::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    dim_mismatch_reported = true;
                }
                // skip: do not extend left_net / right_net with this operand
            }
        }

        // 5) Write left net (anchor + all non-anchor opd's left endpoints / single-end points)
        //
        // Splitting principle: only look at anchor_dim.
        //   - anchor_dim >= 2 and left_net length divisible → slice by lane
        //   - Otherwise → all endpoints in the same net
        // Note: whether anchor is single-ended does not affect left net slice
        // decision —— XTAL such N-wide bus port has left == right but still
        // needs to be sliced by N lanes. is_single_ended is only used in the
        // right net decision (when anchor is single-ended, right net has the
        // same node set as left net, skip).
        if left_net.len() >= 2 {
            if anchor_dim >= 2 && left_net.len() % anchor_dim == 0 {
                // Slice N lanes by position
                let lanes = left_net.len() / anchor_dim;
                for i in 0..anchor_dim {
                    let lane: Vec<NetPoint> = (0..lanes)
                        .map(|j| left_net[j * anchor_dim + i].clone())
                        .collect();
                    if lane.len() >= 2 {
                        let id = self.next_conn_id();
                        self.add_connection(
                            self.make_conn_with_provenance(id, lane, ConnDir::Undirected, None)
                                .with_op(ConnOp::Parallel),
                        );
                    }
                }
            } else {
                // Anchor is 1 wide / indivisible (e.g. dimension mismatch degenerate path):
                // all endpoints in the same net
                let id = self.next_conn_id();
                self.add_connection(
                    self.make_conn_with_provenance(id, left_net.clone(), ConnDir::Undirected, None)
                        .with_op(ConnOp::Parallel),
                );
            }
        }

        // 6) Write right net
        //    - When anchor is single-ended (XTAL bare port, bare label etc.),
        //      right_net has the same node set as left_net, don't repeat.
        //    - Single-end opds don't appear in right_net (they only attach to left net).
        //    - Anchor double-end + same-dimension opd double-end: go through lane splitting.
        let anchor_is_single = is_single_ended(&anchor_left, &anchor_right);
        if right_net.len() >= 2 && !anchor_is_single && !is_single_ended(&right_net, &left_net) {
            let right_dim = opd_rights[anchor_idx].len();
            if right_dim >= 2 && right_net.len() % right_dim == 0 {
                let lanes = right_net.len() / right_dim;
                for i in 0..right_dim {
                    let lane: Vec<NetPoint> = (0..lanes)
                        .map(|j| right_net[j * right_dim + i].clone())
                        .collect();
                    if lane.len() >= 2 {
                        let id = self.next_conn_id();
                        self.add_connection(
                            self.make_conn_with_provenance(id, lane, ConnDir::Undirected, None)
                                .with_op(ConnOp::Parallel),
                        );
                    }
                }
            } else {
                let id = self.next_conn_id();
                self.add_connection(
                    self.make_conn_with_provenance(id, right_net, ConnDir::Undirected, None)
                        .with_op(ConnOp::Parallel),
                );
            }
        }

        Ok(())
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
            if let Err(err) = self.try_connect_adjacent(lref, rref, dir) {
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
        if let Some(encoded) = return_ep {
            self.auto_inst_map.insert(key, encoded);
        } else {
            self.auto_inst_map.insert(key, inst_name.to_string());
        }
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

                // ── Iter-7.1 ────────────────────────────────────────────
                // Internal parallel wiring: rules §10.1 `A + B + C` should generate
                // two nets, shorting all opd's left ends and right ends (take
                // operand 1 mode):
                //   - net_l: A.left ~ B.left ~ C.left  (chain entry is also the
                //            internal pin1 collection point)
                //   - net_r: A.right ~ B.right ~ C.right (chain exit is also the
                //            internal pin2 collection point)
                //
                // If each opd's endpoint dimensions are consistent (e.g.
                // XTAL{X1,X2} + R442::RES' are both 2 points wide), go zip:
                // i-th left with i-th left, i-th right with i-th right → generate
                // 2N nets.
                //
                // Historically this part of wiring relied on `points.rs::Parallel`
                // "happening to" spit all opd endpoints out to the outer chain,
                // side effects see points.rs::Parallel comment. Iter-7.1 lifts
                // this part here, explicitly generates internal nets, and changes
                // points.rs::Parallel back to only expose opds[0] endpoints
                // (consistent with rules §10.1).
                if stmts.len() >= 2 {
                    self.wire_parallel_internal(stmts)?;
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
                // While the outer chain's adjacent wiring (try_connect_adjacent:
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
                //     internal adjacent try_connect_adjacent.
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
                        if let Some(inst_name) = self.auto_inst_map.get(&key).cloned() {
                            if let Some(stripped) = inst_name.strip_prefix("@@ARRAY:") {
                                for name in stripped.split(',') {
                                    self.bridge_passive_names.insert(name.to_string());
                                }
                            } else {
                                self.bridge_passive_names.insert(inst_name);
                            }
                        }
                    }
                    _ => {
                        self.process_stmt(inner)?;
                    }
                }
            }
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
                            // (e.g. `cap[4:5]::CAP()`), use the
                            // `@@ARRAY:name1,name2` prefix to encode all
                            // instance names into auto_inst_map's value
                            // —— resolve_funccall_*_points after seeing the
                            // `@@ARRAY:` prefix will return all instances'
                            // corresponding pins, allowing
                            // `MIC{P,N} -> cap[4:5] -> uC.ADC{P,N}` to go
                            // through the positional 2×1 vs 2×1 connection
                            // rather than being collapsed.
                            let encoded = if new_components.len() > 1 {
                                let names: Vec<String> =
                                    new_components.iter().map(|c| c.name.clone()).collect();
                                format!("@@ARRAY:{}", names.join(","))
                            } else if let Some(comp) = new_components.first() {
                                comp.name.clone()
                            } else {
                                String::new()
                            };
                            if !encoded.is_empty() {
                                self.auto_inst_map.insert(key, encoded);
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
                            self.auto_inst_map.insert(key, inst.name.clone());
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
                // can be found in self.components. On hit, use @@ARRAY: encoding
                // to directly register auto_inst_map, skipping construction.
                if let Some(caller_box) = &fc.caller {
                    if let Some(array_names) =
                        self.resolve_array_caller_to_existing(caller_box.as_ref())
                    {
                        let key = Self::member_key(phrase);
                        let encoded = if array_names.len() > 1 {
                            format!("@@ARRAY:{}", array_names.join(","))
                        } else {
                            array_names.first().cloned().unwrap_or_default()
                        };
                        if !encoded.is_empty() {
                            self.auto_inst_map.insert(key, encoded);
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
                        let known = self.components.iter().any(|c| c.name == *nm)
                            || self.sub_modules.iter().any(|m| m.name == *nm);
                        if !known {
                            if let McPhrase::FuncCall(caller_fc) = caller_box.as_ref() {
                                if let Some(real) = self.auto_inst_map.get(&caller_fc.id).cloned() {
                                    if !real.starts_with("@@") {
                                        inst_name = Some(real);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(inst_name) = inst_name {
                        let func_name_str = fc.func_name.to_string();

                        // Component instance method
                        let comp_func = self
                            .components
                            .iter()
                            .find(|c| c.name == inst_name)
                            .and_then(|c| c.def.funcs.find(&func_name_str).cloned());
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
                            .sub_modules
                            .iter()
                            .find(|m| m.name == inst_name)
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
                                if let Some(sub) =
                                    self.sub_modules.iter().find(|m| m.name == segs[0])
                                {
                                    let inner_comp_func =
                                        sub.components.iter().find(|c| c.name == segs[1]).and_then(
                                            |c| {
                                                let f = c.def.funcs.find(&func_name_str)?;
                                                // arity guard
                                                let func_arity = f.params.iter().count();
                                                let call_arity = fc.params.len();
                                                if func_arity > 0 && call_arity > 0
                                                    || func_arity == 0 && call_arity == 0
                                                {
                                                    Some(f.clone())
                                                } else {
                                                    None
                                                }
                                            },
                                        );
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
                                if let Some(sub) =
                                    self.sub_modules.iter().find(|m| m.name == segs[0])
                                {
                                    let inner_sub_func = sub
                                        .sub_modules
                                        .iter()
                                        .find(|m| m.name == segs[1])
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
                        let inst_is_component = self.components.iter().any(|c| c.name == inst_name);
                        let inst_is_submodule =
                            self.sub_modules.iter().any(|m| m.name == inst_name);
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
                            self.auto_inst_map.insert(key, comp.name.clone());
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
                        self.auto_inst_map.insert(key, inst.name.clone());
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
                        if let Some(encoded) = return_ep {
                            self.auto_inst_map.insert(key, encoded);
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
                                    .components
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
                                    self.auto_inst_map.insert(key, real_name);
                                } else {
                                    let (stub, _) =
                                        self.auto_name(super::AutoNameKind::Stub, &safe);
                                    self.auto_inst_map.insert(key, stub);
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
            if names.len() > 1
                && names
                    .iter()
                    .all(|n| self.components.iter().any(|c| &c.name == n))
            {
                return Some(names);
            }
            return None;
        }

        // ── §11.3/1.6: direct vector-node lookup (was Iter-3.D sibling-probing) ──
        // The old heuristic probed base+digit siblings (`res1` → res2, res3 ...)
        // to reassemble an array after pass1 only expanded the first member —
        // a digit-suffix name scan with an artificial 16-sibling bound and an
        // `@` auto-named exclusion for its false positives.
        //
        // The declared member set is now a first-class modeling-layer coordinate
        // (`self.vectors`, §11.2): a `Component(res1)` caller (pass1 resolving
        // a bracket to a single member) is matched against the physical member
        // id list of every vector group. Auto-named components (`@CAP1`) are
        // never in a declared group, so the lookup simply misses — the old `@`
        // exclusion is structurally unnecessary. Contract E single-member
        // scalars are not in `vectors`, so they never re-link as arrays.
        if let McPhrase::Endpoint(McEndpoint::Single(iref)) = phrase {
            if let McInstance::Component(c) = &iref.base {
                let cname = c.name.to_string();
                for v in &self.vectors {
                    if v.member_ids.iter().any(|id| id == &cname) {
                        return Some(v.member_ids.clone());
                    }
                }
            }
        }

        None
    }

    /// Recursively scan a McPhrase for FuncCall nodes referencing a failed component class.
    fn phrase_contains_failed_class(phrase: &McPhrase, failed: &HashSet<String>) -> bool {
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
            McPhrase::Transposed(inner) => Self::phrase_contains_failed_class(inner, failed),
            McPhrase::Member(inner, _) => Self::phrase_contains_failed_class(inner, failed),
            _ => false,
        }
    }
}
