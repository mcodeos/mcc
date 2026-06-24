// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Connection line processing
//!
//! - `process_line`: single line expansion + member/adjacent connection dispatch
//! - `phrase_to_members`: expand Series etc aggregate forms to member sequence
//! - `try_connect_adjacent`: adjacent member pairing connections
//! - `process_member_internal`: single member internal processing (FuncCall / Closure / Group …)

use super::funccall::FuncCallInst;
use super::McModuleInst;
use crate::core::basic::mc_bus::McBus;
use crate::core::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::core::basic::mc_opd::McOpd;
use crate::core::basic::mc_param::McParamValue;
use crate::core::basic::mc_phrase::McPhrase;
use crate::core::common::IOType;
use crate::core::mc_inst::McInstance;
use crate::instant::mc_net::{ConnectionInst, InstError, NetPoint};

impl McModuleInst {
    /// Process connection line - accepts McPhrase
    pub(super) fn process_line(&mut self, phrase: &McPhrase) -> Result<(), InstError> {
        let members = self.phrase_to_members(phrase);
        if members.is_empty() {
            return Ok(());
        }

        // recursively process nested structures — per-member fault-tolerant
        for member in &members {
            if let Err(e) = self.process_member_internal(member) {
                self.record_warning(911, format!("Member processing failed: {e}"));
            }
        }

        // ── Root cause C: `.Cap(_)` decoupling cap in chain is parallel shunt, not series ─────────
        // form like `[V3V3,GND] -> CAP(..).Cap(_) -> [VCC,VSS]`: cap should "bridge rails",
        // bus itself passes through (V3V3~VCC, GND~VSS). Old logic treated it as series element in chain
        // (adjacent connected right neighbor to pin2) + wire_builtin_twopin connected pin2 to GND → pin2
        // double-connected → rail short to ground (flash.VCC ~ GND).
        //
        // Only when this line actually contains `.Cap(_)` (params empty / all `_`) shunt, go through special
        // wiring below; lines without shunt fall through to original adjacency loop → zero impact on existing paths.
        let shunt: Vec<bool> = members.iter().map(Self::is_chain_cap_shunt).collect();
        if shunt.iter().any(|&s| s) {
            self.wire_chain_with_shunts(&members, &shunt);
            return Ok(());
        }

        // handle adjacent member connections — per-pair fault-tolerant
        for i in 0..members.len().saturating_sub(1) {
            let left_member = &members[i];
            let right_member = &members[i + 1];

            if let Err(e) = self.try_connect_adjacent(left_member, right_member) {
                self.record_warning(
                    912,
                    format!(
                        "Connection between members #{} and #{} failed: {}",
                        i,
                        i + 1,
                        e
                    ),
                );
            }
        }

        Ok(())
    }

    /// Root cause C: determine if a chain member is `.Cap(_)` form decoupling cap (parallel shunt).
    ///
    /// Hit conditions: is `Cap` in builtin two-pin function (case-sensitive, consistent with
    /// `is_builtin_twopin_net_fn` handling of `Cap`), and params empty or all `_`
    /// placeholder (`McParamValue::NONE`). This exactly matches `wire_builtin_twopin`
    /// `targets.is_empty()` branch "pin2 → GND, pin1 left for chain".
    ///
    /// Only recognizes `Cap`: `.Pullup(_, VDD)` `_` is in signal position not ground,
    /// `.Pullup` / `.Pulldown` still go original adjacency path, not special-cased here, avoid false hits.
    fn is_chain_cap_shunt(member: &McPhrase) -> bool {
        if let McPhrase::FuncCall(fc) = member {
            let fname = fc.func_name.to_string();
            let last = fname.rsplit('.').next().unwrap_or(fname.as_str());
            if last == "Cap" {
                return fc.params.iter().all(Self::is_uscore_param);
            }
        }
        false
    }

    fn is_uscore_param(p: &McParamValue) -> bool {
        matches!(p, McParamValue::NONE(_)) || matches!(p, McParamValue::Opd(McOpd::Uscore))
    }

    fn shunt_chain_points(&mut self, m: &McPhrase, right_side: bool) -> Vec<NetPoint> {
        if let McPhrase::Multiple(inner) = m {
            return inner
                .iter()
                .flat_map(|ip| self.shunt_chain_points(ip, right_side))
                .collect();
        }
        let pts = if right_side {
            self.get_right_points(m).unwrap_or_default()
        } else {
            self.get_left_points(m).unwrap_or_default()
        };
        if !pts.is_empty() {
            return pts;
        }
        let name = m.to_string();
        if !name.is_empty() && name != "_" {
            return vec![self.node_to_netpoint(&McBus::new(&name))];
        }
        pts
    }

    /// Root cause C: wire chain containing `.Cap(_)` shunt.
    ///
    /// Rules:
    ///   1. **Bus pass-through**: serialize all non-shunt members in original adjacency order (skip shunt),
    ///      i.e., `left_neighbor.right ~ right_neighbor.left`, width alignment handled by create_connection.
    ///      Example `[V3V3,GND] -> CAP.Cap(_) -> [VCC,VSS]` → V3V3~VCC, GND~VSS.
    ///   2. **Cap parallel**: each shunt cap pin1 ~ rail (first non-ground point in neighbor endpoints),
    ///      pin2 ~ GND (latter already connected by process_member_internal
    ///      wire_builtin_twopin `targets.is_empty()` branch).
    ///
    /// This way decoupling cap is truly "bridging rails", no longer treated as series element double-connecting pin2 to short.
    fn wire_chain_with_shunts(&mut self, members: &[McPhrase], shunt: &[bool]) {
        // 1. non-shunt members serialized by adjacency (skip shunt, pass-through)
        let non_shunt: Vec<&McPhrase> = members
            .iter()
            .zip(shunt.iter())
            .filter(|(_, &s)| !s)
            .map(|(m, _)| m)
            .collect();
        for pair in non_shunt.windows(2) {
            let _raw_lp = self.get_right_points(pair[0]).unwrap_or_default();
            let _raw_rp = self.get_left_points(pair[1]).unwrap_or_default();
            let _kind = |m: &McPhrase| -> String {
                match m {
                    McPhrase::Multiple(inner) => format!("Multiple(n={})", inner.len()),
                    McPhrase::Endpoint(McEndpoint::Single(ir)) => match &ir.base {
                        McInstance::Label(s) => format!("Label({s})"),
                        McInstance::Bus(b) => format!("Bus({}, mem={:?})", b.name, b.member),
                        _ => "Endpoint(other)".into(),
                    },
                    _ => format!("{:?}", std::mem::discriminant(m)),
                }
            };
            // eprintln!("[SHUNT-PT] L={} L_pts={:?} | R={} R_pts={:?}",
            //     kind(pair[0]),
            //     raw_lp.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
            //     kind(pair[1]),
            //     raw_rp.iter().map(|p| p.path.clone()).collect::<Vec<_>>());
            let lp = self.shunt_chain_points(pair[0], true);
            let rp = self.shunt_chain_points(pair[1], false);
            if let Err(e) = self.create_connection(lp, rp) {
                self.record_warning(
                    912,
                    format!("Pass-through across `.Cap(_)` shunt failed: {e}"),
                );
            }
        }

        // 2. each shunt cap: pin1 ~ rail
        for (k, m) in members.iter().enumerate() {
            if !shunt[k] {
                continue;
            }
            // get rail source: prefer nearest left neighbor right_points, otherwise nearest right neighbor left_points
            let rail_src: Vec<NetPoint> = {
                let mut left_pts: Option<Vec<NetPoint>> = None;
                for j in (0..k).rev() {
                    if !shunt[j] {
                        left_pts = self.get_right_points(&members[j]).ok();
                        break;
                    }
                }
                match left_pts {
                    Some(p) if !p.is_empty() => p,
                    _ => {
                        let mut right_pts: Vec<NetPoint> = Vec::new();
                        for j in (k + 1)..members.len() {
                            if !shunt[j] {
                                right_pts = self.get_left_points(&members[j]).unwrap_or_default();
                                break;
                            }
                        }
                        right_pts
                    }
                }
            };
            if rail_src.is_empty() {
                self.record_warning(
                    913,
                    "`.Cap(_)` shunt: no neighbor to derive a rail; only pin2 → GND wired"
                        .to_string(),
                );
                continue;
            }
            // rail = first non-ground point (all ground then fallback to first point)
            let rail = rail_src
                .iter()
                .find(|p| !lr_is_ground_name(lr_last_seg(&p.path)))
                .cloned()
                .unwrap_or_else(|| rail_src[0].clone());
            // cap pin1
            let pin1 = self.get_left_points(m).unwrap_or_default();
            if pin1.is_empty() {
                self.record_warning(
                    913,
                    "`.Cap(_)` shunt: cannot resolve pin1; only pin2 → GND wired".to_string(),
                );
                continue;
            }
            if let Err(e) = self.create_connection(pin1, vec![rail]) {
                self.record_warning(913, format!("`.Cap(_)` shunt pin1 → rail failed: {e}"));
            }
        }
    }

    /// Convert McPhrase to expanded McPhrase list
    /// Series is recursively expanded to individual member McPhrases
    pub(super) fn phrase_to_members(&self, phrase: &McPhrase) -> Vec<McPhrase> {
        match phrase {
            McPhrase::Series(phrases) => {
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
                //   - `[VDD_3V3, GND] -> lp322dcdc{Vin, GND}` (power.mc:101)
                //     → `Multiple([Label(VDD_3V3), Label(GND)])`, L_size=0
                //   - `MIC{P,N} -> cap[4:5]::CAP(1uF) -> uC.ADC{P,N}` (us513.mc:147)
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
                // Background: parser for `Name{a, b, ...}` in certain scenarios (especially line
                // start position + Name is io/out/in declared Label-type port) expansion
                // is inconsistent — expected to produce ONE Bus(Name, [a, b, ...]), actually
                // produces [Bus(Name, [a]), Bus(Name, [b]), ...] multiple adjacent
                // phrases entering Series.
                //
                // Verified case (us513.mc:147):
                //   `MIC{P,N} -> cap[4:5]::CAP(1uF) -> uC.ADC{P,N}`
                //   - line end `uC.ADC{P,N}` parsed correctly: Bus(uC.ADC, [P, N]) single
                //     phrase (variants log: Bus(name='uC.ADC' members=[P,N]))
                //   - line start `MIC{P,N}` parsed incorrectly: split into two phrases
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
                //     correctly handles line-end curly as single Bus(MIC, [P, N])): adjacent phrase
                //     name different (mic vs MIC), won't trigger.
                //   - `mcu513{ MIC | DAC_OUT, SPK_MUTE }` (Node form): not
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
                // If user explicitly wrote member access (e.g., `lp322dcdc{Vin, GND}` or
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
                // Bare module reference `V3V3 -> moddcdc -> V1V2`: need to split module into
                // Node (in side / out side), so `moddcdc` two sides don't get
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
                vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::Bus(data.clone()),
                )))]
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(label),
                ..
            })) => {
                vec![McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::Bus(McBus::new(label)),
                )))]
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
                if matches!(inner.as_ref(), McPhrase::FuncCall(_)) {
                    // keep the Member so adjacency resolves the named port via the Member branch
                    vec![McPhrase::Member(inner.clone(), member_ep.clone())]
                } else {
                    self.phrase_to_members(inner)
                }
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

    /// Try to connect adjacent members
    ///
    /// Helper method extracted from `process_line`, handling Group / normal
    /// member connection dispatch. On failure the caller `process_line` catches
    /// and records the diagnosis.
    fn try_connect_adjacent(
        &mut self,
        left_member: &McPhrase,
        right_member: &McPhrase,
    ) -> Result<(), InstError> {
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
        let left_is_group = matches!(left_member, McPhrase::Group { .. });
        let right_is_group = matches!(right_member, McPhrase::Group { .. });

        if right_is_group {
            let external_points = self.get_right_points(left_member)?;
            self.connect_to_group(external_points, right_member, true)?;
        } else if left_is_group {
            let external_points = self.get_left_points(right_member)?;
            self.connect_to_group(external_points, left_member, false)?;
        } else {
            let left_points = self.get_right_points(left_member)?;
            let right_points = self.get_left_points(right_member)?;
            // ── [P4-ADJ] temporary probe (commented)
            // if matches!(left_member, McPhrase::Parallel(_))
            //     || matches!(right_member, McPhrase::Parallel(_))
            // {
            //     let dl: Vec<String> = left_points.iter().map(|p| p.path.clone()).collect();
            //     let dr: Vec<String> = right_points.iter().map(|p| p.path.clone()).collect();
            //     eprintln!(
            //         "[P4-ADJ] L={} R={} | get_right(L)={:?} get_left(R)={:?}",
            //         l_kind, r_kind, dl, dr
            //     );
            // }
            let trans_involved = matches!(left_member, McPhrase::Transposed(_))
                || matches!(right_member, McPhrase::Transposed(_));
            if trans_involved
                && !left_points.is_empty()
                && !right_points.is_empty()
                && left_points.len() != right_points.len()
            {
                let n = left_points.len().min(right_points.len());
                for (l, r) in left_points
                    .into_iter()
                    .take(n)
                    .zip(right_points.into_iter().take(n))
                {
                    self.create_connection(vec![l], vec![r])?;
                }
            } else {
                self.create_connection(left_points, right_points)?;
            }
        }
        Ok(())
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
    /// - This method assumes `lines.len() >= 2`, please check before calling.
    /// - Single-end right net degradation avoidance: if all opds are
    ///   single-end (left/right paths equal), only generate left net, don't
    ///   repeat the right net (they have identical node sets).
    fn wire_parallel_internal(&mut self, lines: &[McPhrase]) -> Result<(), InstError> {
        // 1) Collect each opd's left/right endpoints
        let mut opd_lefts: Vec<Vec<NetPoint>> = Vec::with_capacity(lines.len());
        let mut opd_rights: Vec<Vec<NetPoint>> = Vec::with_capacity(lines.len());

        for (_idx, opd) in lines.iter().enumerate() {
            // ── Skip Lead placeholder ────────────────────────────────────────
            // In Parallel with `_` like `(_, A, B)`, `_` is parsed as Lead,
            // doesn't participate in parallel wiring.
            if matches!(opd, McPhrase::Lead) {
                opd_lefts.push(Vec::new());
                opd_rights.push(Vec::new());
                continue;
            }
            // ── Diagnostic: print opd phrase form ──────────────────────────────
            let _opd_kind = match opd {
                McPhrase::Endpoint(McEndpoint::Single(ir)) => match &ir.base {
                    McInstance::Label(s) => format!("Label('{s}')"),
                    McInstance::Bus(b) => format!("Bus('{}', mem={:?})", b.name, b.member),
                    McInstance::Component(c) => {
                        format!("Component('{}', class='{}')", c.name, c.base.name)
                    }
                    McInstance::Module(m) => format!("Module('{}')", m.name),
                    McInstance::Interface(i) => format!("Interface('{}')", i.name),
                    McInstance::List(l) => format!("List('{}')", l.name),
                    McInstance::BusRef { component, bus } => {
                        format!("BusRef(c={component},b={bus})")
                    }
                },
                McPhrase::Endpoint(McEndpoint::Node { input, output }) => {
                    format!("Node(in={},out={})", input.len(), output.len())
                }
                McPhrase::Endpoint(McEndpoint::List(_)) => "Endpoint(List)".to_string(),
                McPhrase::FuncCall(fc) => format!("FuncCall('{}')", fc.func_name),
                McPhrase::Parallel(v) => format!("Parallel(len={})", v.len()),
                McPhrase::Multiple(v) => format!("Multiple(len={})", v.len()),
                McPhrase::Group(g) => format!("Group(opds={})", g.opds.len()),
                McPhrase::Transposed(_) => "Transposed".to_string(),
                McPhrase::Closure(_) => "Closure".to_string(),
                McPhrase::Lead => "Lead".to_string(),
                McPhrase::Member(_, _) => "Member".to_string(),
                McPhrase::Series(_) => "Series".to_string(),
            };
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

        // [R2-DIAG2] Unconditionally print each parallel's opd form + point extraction
        {
            let _kinds: Vec<String> = lines
                .iter()
                .map(|o| match o {
                    McPhrase::Endpoint(_) => "Endpoint".to_string(),
                    McPhrase::FuncCall(fc) => format!("FuncCall({})", fc.func_name),
                    McPhrase::Parallel(v) => format!("Parallel({})", v.len()),
                    McPhrase::Multiple(v) => format!("Multiple({})", v.len()),
                    McPhrase::Group(g) => format!("Group({})", g.opds.len()),
                    McPhrase::Transposed(_) => "Transposed".to_string(),
                    McPhrase::Series(v) => format!("Series({})", v.len()),
                    McPhrase::Closure(_) => "Closure".to_string(),
                    McPhrase::Lead => "Lead".to_string(),
                    McPhrase::Member(_, _) => "Member".to_string(),
                })
                .collect();
        }

        // 2) Anchor operand 1 (opd[0]). If opd[0]'s endpoints are empty,
        //    find the next non-empty as the anchor (Lead-skip fallback).
        let anchor_idx = (0..lines.len()).find(|&i| !opd_lefts[i].is_empty());
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

        // 4) Whether dimension mismatch needs a zip-mismatch warning
        let mut dim_mismatch_warned = false;

        for i in 0..lines.len() {
            if i == anchor_idx {
                continue;
            }
            let lp = &opd_lefts[i];
            let rp = &opd_rights[i];
            if lp.is_empty() && rp.is_empty() {
                continue; // Lead or empty opd
            }

            let opd_single = is_single_ended(lp, rp);

            if opd_single {
                // Single-end opd: only attached to left net (operand 1's left end)
                // bugfix_report error 9 rule: "+ takes operand 1, single-end X connects to operand 1's left end"
                //
                // When the anchor is double-ended N wide, single-end point needs
                // to be "broadcast" to all N lanes (replicated N times), so that
                // subsequent zip splitting can correctly distribute it to each
                // lane's left end. When the anchor is 1 wide (single-end or 1 wide
                // double-end), just extend directly.
                if anchor_dim >= 2 && !is_single_ended(&anchor_left, &anchor_right) {
                    for _ in 0..anchor_dim {
                        left_net.extend(lp.iter().cloned());
                    }
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
                // practice, see us513.mc:82).
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
                // Dimension mismatch (double-end but different widths): degrade
                // to "merge all to left net" + warning
                if !dim_mismatch_warned {
                    self.record_warning(
                        921,
                        format!(
                            "Parallel '+' operand dimension mismatch (anchor={}x1, opd[{}]={}x1 left / {}x1 right): \
                             merging operand into anchor's left net.",
                            anchor_dim, i, lp.len(), rp.len()
                        ),
                    );
                    dim_mismatch_warned = true;
                }
                left_net.extend(lp.iter().cloned());
                left_net.extend(rp.iter().cloned());
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
                        self.connections.push(ConnectionInst::new(id, lane));
                    }
                }
            } else {
                // Anchor is 1 wide / indivisible (e.g. dimension mismatch degenerate path):
                // all endpoints in the same net
                let id = self.next_conn_id();
                self.connections
                    .push(ConnectionInst::new(id, left_net.clone()));
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
                        self.connections.push(ConnectionInst::new(id, lane));
                    }
                }
            } else {
                let id = self.next_conn_id();
                self.connections.push(ConnectionInst::new(id, right_net));
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

    fn process_series_branch_inplace(&mut self, elems: &[McPhrase]) -> Result<(), InstError> {
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
            if let Err(err) = self.try_connect_adjacent(lref, rref) {
                self.record_warning(
                    912,
                    format!(
                        "Group/Parallel-branch Series member #{}~#{} connect failed: {}",
                        k,
                        k + 1,
                        err
                    ),
                );
            }
        }
        Ok(())
    }

    pub(super) fn process_member_internal(&mut self, phrase: &McPhrase) -> Result<(), InstError> {
        match phrase {
            McPhrase::Parallel(lines) => {
                // ── P1-E1 ────────────────────────────────────────────────
                // Each item in Parallel is an independent "line". Previously
                // here uniformly went through `self.process_line(line)`, but
                // process_line first calls phrase_to_members to clone line, then
                // does process_member_internal on the cloned elements —— the
                // auto_inst_map's key falls on the cloned address.
                //
                // Later, in the adjacency phase, get_left_points / get_right_points
                // access through the **original** `&line` again, the key is
                // unequal, auto_inst_map can't find it, P0-4 stub / component
                // instances are all lost. Typical symptom is `[DIO.ESD(), DIO.ESD()]`
                // such anonymous 2-pin element column all collapses into bare
                // `DIO` label and merges into a giant net.
                //
                // For "leaves" (single FuncCall / Endpoint etc.) directly call
                // process_member_internal, keeping the address of `&line`
                // unchanged. For composite nodes (Series / Parallel nesting)
                // still use process_line, because they themselves need adjacency
                // processing, and usually don't contain anonymous construction
                // calls that would trigger the stub mechanism.
                for line in lines {
                    match line {
                        McPhrase::Series(elems) => {
                            // ── BUG4 fix (same as Group handler) ────────────────
                            // Originally process_line(clone) → FuncCall in Series
                            // is instantiated on the cloned pointer; but outer
                            // get_left_points(Parallel) → opds[0]=Series →
                            // get_left_points(&Series.elems[0]) uses original
                            // pointer to query auto_inst_map → MISS → RES.in leaks.
                            // (speaker periph.mc:97 `(RES(30kΩ)->lpa.VO1 + spk.N)`
                            //  where opds[0] is Series([RES_3, lpa.VO1]) this form.)
                            // Changed to in-place instantiate each element + internal
                            // adjacency (Label upgrade), keep FuncCall original pointer.
                            self.process_series_branch_inplace(elems)?;
                        }
                        McPhrase::Parallel(_) => {
                            self.process_line(line)?;
                        }
                        _ => {
                            self.process_member_internal(line)?;
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
                if lines.len() >= 2 {
                    self.wire_parallel_internal(lines)?;
                }
            }
            McPhrase::Group(ref g) => {
                // ── BUG4 fix ──────────────────────────────────────────────
                // Originally called process_line(p) for each branch. But the
                // first step of process_line, phrase_to_members, will clone the
                // branch (Group/Series/FuncCall all cloned), then do
                // process_member_internal on the cloned elements —— FuncCall's
                // auto_inst_map key falls on the **cloned pointer**.
                //
                // While the outer chain's adjacent wiring (try_connect_adjacent:
                // RES_5 -> Group) goes get_left_points(Group) → iterates
                // **this Group's g.opds[i]** (same as here), for the FuncCall
                // inside it uses g.opds[i]'s original pointer to query
                // auto_inst_map —— unequal to the cloned pointer above → MISS →
                // placeholder CAP.in/RES.in leaks as @_phantom (CLAUDE.md BUG4).
                //
                // Fix: no longer process_line(clone), but in-place process each
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
                        McPhrase::Series(elems) => {
                            // BUG4: in-place processing + Label upgrade
                            // (fix the unconnected GND in `(CAP+RES)->GND`,
                            // the internal series in `VBUS->USB_VBUS`).
                            self.process_series_branch_inplace(elems)?;
                        }
                        _ => {
                            self.process_member_internal(p)?;
                        }
                    }
                }
            }
            McPhrase::Transposed(inner) => {
                self.process_line(inner)?;
            }
            McPhrase::Closure(ref c) => {
                // Phase 3.3: Closure instantiation (closure parameter binding)
                for param_decl in c.params.iter() {
                    if let Some(name) = param_decl.get_primary_name() {
                        self.ensure_label(&name);
                    }
                }
                for p in &c.body {
                    self.process_line(p)?;
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
                            self.components.extend(new_components);
                            self.connections.extend(new_connections);
                        }
                        FuncCallInst::SubModule {
                            inst,
                            new_connections,
                        } => {
                            self.auto_inst_map.insert(key, inst.name.clone());
                            self.sub_modules.push(inst);
                            self.connections.extend(new_connections);
                        }
                        FuncCallInst::PassThrough => {}
                    }
                    return Ok(());
                }

                // ── Iter-1.3 ─────────────────────────────────────────────
                // Array-form caller pointing to already-declared instances:
                // for a call like `cap[4:5]::CAP(1uF)`, pass1 has already
                // registered cap4/cap5 as independent components in
                // self.components, but the net line's FuncCall caller is still
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
                // (Iter-2.2 instance method, P1-D builtin twopin). Reasons:
                //
                //   1. **Chained call semantics**: `obj.f1().f2().f3()` semantics
                //      is "apply f1/f2/f3 sequentially to the same obj", each
                //      level needs to independently expand body, can't skip
                //      inner just because outer early-exits in dispatch phase.
                //   2. **builtin twopin still depends on this**: when outer .Cap
                //      of `CAP(v).Cap(x)` goes through P1-D, it needs inner
                //      CAP(v) to have already written @CAP_N into auto_inst_map.
                //      Lifting to here doesn't affect this invariant.
                //   3. **Pointer stability (original Iter-3.F argument)**: use
                //      `process_member_internal` for single-member caller instead
                //      of `process_line`, keep `&**caller_line` address unchanged,
                //      making auto_inst_map's pointer key match reliable.
                //      Compound caller (Series/Parallel) still uses `process_line`
                //      for adjacency.
                //
                // Side effect tracking: after lifting, dispatch paths will see
                // an already-processed caller first. For Endpoint-form caller
                // (like mcu513 in `mcu513.setup()`) processing is no-op; for
                // FuncCall-form caller (like `.capIt()` after `setup()`) it
                // recursively expands the setup body —— which is exactly the
                // fix target.
                if let Some(caller_line) = &fc.caller {
                    match caller_line.as_ref() {
                        McPhrase::FuncCall(_)
                        | McPhrase::Endpoint(_)
                        | McPhrase::Transposed(_)
                        | McPhrase::Lead
                        | McPhrase::Member(_, _) => {
                            self.process_member_internal(caller_line.as_ref())?;
                        }
                        _ => {
                            self.process_line(caller_line.as_ref())?;
                        }
                    }
                }

                // ── Iter-2.2 ─────────────────────────────────────────────
                // Component instance method dispatch: forms like `uC.power(V3V3, V1V2)`.
                // funccall.rs::instantiate_funccall currently only checks
                // self.sub_modules, never dispatches methods on component instances
                // —— causing `func power()` / `func i2c()` in MCU.US513_20_F to
                // never expand.
                //
                // Here we do explicit dispatch before entering instantiate_funccall:
                //   1. Extract instance name from fc.caller (Endpoint::Single's base name)
                //   2. If hit self.components, look up the component def's funcs table
                //   3. If hit self.sub_modules, look up the module def's funcs table
                //   4. If corresponding func def found, call instantiate_instance_method
                // This path also covers the Iter-1 cap[4:5] scenario where
                // "caller is array but func is user method" extreme case
                // (although not in hbl).
                //
                // ── Iter-3.A ────────────────────────────────────────────
                // Important: must first exclude builtin 2-pin methods
                // (`.Cap/.Pullup/.Pulldown`), otherwise will incorrectly grab
                // the P1-D wire_builtin_twopin path below. Some components'
                // funcs tables may have empty-shell methods of the same name,
                // once entered will be treated as "Instance method has no
                // parsed lines", completely losing builtin wiring.
                if !Self::is_builtin_twopin_net_fn(&fc.func_name.to_string()) {
                    if let Some(caller_box) = &fc.caller {
                        if let Some(inst_name) = Self::extract_caller_inst_name(caller_box.as_ref())
                        {
                            let func_name_str = fc.func_name.to_string();

                            // Component instance method
                            let comp_func = self
                                .components
                                .iter()
                                .find(|c| c.name == inst_name)
                                .and_then(|c| c.def.funcs.find(&func_name_str).cloned());
                            if let Some(func_def) = comp_func {
                                let key = Self::member_key(phrase);
                                let result = self.instantiate_instance_method(
                                    &inst_name, &func_def, &fc.params, &fc.left, &fc.right,
                                )?;
                                if matches!(result, FuncCallInst::PassThrough) {
                                    self.auto_inst_map.insert(key, inst_name.clone());
                                }
                                return Ok(());
                            }

                            // Sub-module instance method
                            let sub_func = self
                                .sub_modules
                                .iter()
                                .find(|m| m.name == inst_name)
                                .and_then(|m| m.def.funcs.find(&func_name_str).cloned());
                            if let Some(func_def) = sub_func {
                                let key = Self::member_key(phrase);
                                let result = self.instantiate_instance_method(
                                    &inst_name, &func_def, &fc.params, &fc.left, &fc.right,
                                )?;
                                if matches!(result, FuncCallInst::PassThrough) {
                                    self.auto_inst_map.insert(key, inst_name.clone());
                                }
                                return Ok(());
                            }

                            // ── P1 fix: dotted scope-chain drill down ──────────────
                            // inst_name like "mcu513.uC" → look up
                            // components["uC"].funcs["i2c"] in sub_modules["mcu513"].
                            // This handles the dispatch path after `uC.i2c(0x36)` in
                            // func body is prefixed to `mcu513.uC.i2c(0x36)`.
                            if inst_name.contains('.') {
                                let segs: Vec<&str> = inst_name.split('.').collect();
                                if segs.len() >= 2 {
                                    // Try sub_modules[seg0].components[seg1].funcs[func]
                                    if let Some(sub) =
                                        self.sub_modules.iter().find(|m| m.name == segs[0])
                                    {
                                        let inner_comp_func = sub
                                            .components
                                            .iter()
                                            .find(|c| c.name == segs[1])
                                            .and_then(|c| {
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
                                            });
                                        if let Some(func_def) = inner_comp_func {
                                            let key = Self::member_key(phrase);
                                            let result = self.instantiate_instance_method(
                                                &inst_name, &func_def, &fc.params, &fc.left,
                                                &fc.right,
                                            )?;
                                            if matches!(result, FuncCallInst::PassThrough) {
                                                self.auto_inst_map.insert(key, inst_name.clone());
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
                                            .and_then(|m| {
                                                m.def.funcs.find(&func_name_str).cloned()
                                            });
                                        if let Some(func_def) = inner_sub_func {
                                            let key = Self::member_key(phrase);
                                            let result = self.instantiate_instance_method(
                                                &inst_name, &func_def, &fc.params, &fc.left,
                                                &fc.right,
                                            )?;
                                            if matches!(result, FuncCallInst::PassThrough) {
                                                self.auto_inst_map.insert(key, inst_name.clone());
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
                            // Typical scenario (hbl.mc:34):
                            //   `mcu513.setup(V3V3, V1V2).capIt().i2c().loadFlash(flash)`
                            // These 4 methods are currently not defined in the US513 module.
                            //
                            // Before fix: fall through to `instantiate_funccall` below,
                            //         treated as globally unknown class, generates
                            //         `@?capIt_1` style stubs, polluting components list
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
                            //   1. loadFlash chain's 4 layers each insert one
                            //      auto_inst_map[layer_phrase_addr] = "mcu513"
                            //   2. After that line's process_line returns, the 4 McPhrase
                            //      nodes' memory is freed
                            //   3. When next line `mic(V3V3).MIC -> ...` is parsed, new
                            //      McPhrase is allocated on the heap, at least one new
                            //      address happens to land on the just-freed old address
                            //   4. resolve_funccall_right(mic FuncCall) uses the new
                            //      address to query map, **hits stale entry** "mcu513"
                            //      → mic is incorrectly parsed as mcu513's output port
                            //   5. Eventually mic.MIC and mcu513's internal MIC/DAC_OUT/
                            //      SPK_MUTE three independent signals short into a 5-endpoint
                            //      super net
                            //
                            // Since the chain in hbl is actually an isolated line, the
                            // assumption in (b) doesn't happen; and outer's parsing in
                            // (a) actually comes from extract_caller_inst_name going
                            // through FuncCall recursion (Iter-6.S2) to derive along
                            // structure, no map needed.
                            //
                            // Fix: directly remove the insert. Chain layer fallback
                            // no longer writes to the map.
                            //
                            // Note: the pointer reuse risk from auto_inst_map being
                            // persistent across process_line is not further aggravated
                            // here, the root fix is Iter-6.S4.3 adding per-line clear in
                            // phases.rs's instantiate_lines_resilient.
                            let inst_is_component =
                                self.components.iter().any(|c| c.name == inst_name);
                            let inst_is_submodule =
                                self.sub_modules.iter().any(|m| m.name == inst_name);
                            if inst_is_component || inst_is_submodule {
                                let owner_kind = if inst_is_component {
                                    "component"
                                } else {
                                    "sub-module"
                                };
                                self.record_warning(
                                    940,
                                    format!(
                                        "Method '{func_name_str}' not defined in {owner_kind} '{inst_name}'; \
                                         chain link skipped, no body expanded."
                                    ),
                                );
                                // ── Iter-6.S4.2 ──
                                // No longer self.auto_inst_map.insert(...) —— see comment above
                                return Ok(());
                            }
                        }
                    }
                }

                // ── Iter-6.S4.1 ─────────────────────────────────────────
                // Caller chain recursion was originally placed here, after Iter-2.2
                // dispatch and before P1-D builtin twopin. But combined with
                // Iter-6.S4's "undefined method warning + early exit" logic, chained
                // calls like `mcu513.setup().capIt().i2c().loadFlash()` once outer
                // (loadFlash) hits early exit, can never reach here —— inner i2c /
                // capIt / setup three layers are silently skipped regardless of
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
                // Below follows P1-D builtin twopin.
                let key = Self::member_key(phrase);

                // ── P1-D ────────────────────────────────────────────────
                // Built-in chained wiring function: `.Cap(a, b)` / `.Pullup(a, b)` /
                // `.Pulldown(a, b)`
                // These are not global classes or user functions, but compile-time
                // built-ins —— semantics: "take the 2-pin element constructed by the
                // caller, connect its two pins out per args".
                //
                // Supported arg forms:
                //   1 arg, 1-wide: pin1 → arg   (pin2 left for outer chain to continue;
                //                                e.g. in `CAP(v).Cap(x) -> y`, y connects to pin2)
                //   1 arg, 2-wide: pin1 → arg[0], pin2 → arg[1]
                //                  (e.g. `.Cap(lp322dcdc{Vin, GND})` or `.Cap([V, G])`)
                //   2 args:        pin1 → args[0], pin2 → args[1]
                //   `.Cap(_)`:     all args are `_`, not wired here, passed to outer chain
                //
                // After processing, point the outer key to the caller's 2-pin instance,
                // so the chain `... -> CAP(v).Cap(x) -> ...` can find the component's
                // left_pin/right_pin in resolve_funccall_*_points and continue properly.
                if Self::is_builtin_twopin_net_fn(&fc.func_name.to_string()) {
                    if let Some(caller_box) = &fc.caller {
                        let caller_key = Self::member_key(caller_box.as_ref());
                        let map_hit = self.auto_inst_map.get(&caller_key).cloned();
                        if let Some(caller_inst_name) = map_hit {
                            self.wire_builtin_twopin(&caller_inst_name, &fc.params)?;
                            self.auto_inst_map.insert(key, caller_inst_name);
                            return Ok(());
                        }
                        // ── Iter-5.E (Part 1) ─────────────────────────────────
                        // In the case where auto_inst_map doesn't hit, if the caller
                        // is already an Endpoint(Component(name)) created at parse time
                        // and name is non-empty, directly use this name as
                        // caller_inst_name and run wire_builtin_twopin. Reason:
                        // Components created at parse time never went through
                        // process_member_internal's registration path (Iter-3.F
                        // recursive Endpoint arm is no-op), so auto_inst_map can
                        // never find them, but the component itself is already in
                        // self.components —— wire_builtin_twopin just uses the name
                        // to find in self.components.
                        if let McPhrase::Endpoint(McEndpoint::Single(ir)) = caller_box.as_ref() {
                            if let McInstance::Component(c) = &ir.base {
                                let caller_inst_name = c.name.to_string();
                                if !caller_inst_name.is_empty() {
                                    self.wire_builtin_twopin(&caller_inst_name, &fc.params)?;
                                    self.auto_inst_map.insert(key, caller_inst_name);
                                    return Ok(());
                                } else if !c.params.is_empty() {
                                    // Anonymous component with params - instantiate it first
                                    let result = self.instantiate_component_construction(
                                        c.base.clone(),
                                        &c.params,
                                        &fc.left,
                                        &fc.right,
                                    )?;
                                    if let FuncCallInst::Components {
                                        mut new_components,
                                        new_connections,
                                    } = result
                                    {
                                        if let Some(inst) = new_components.pop() {
                                            let inst_name = inst.name.clone();
                                            self.components.push(inst);
                                            self.connections.extend(new_connections);
                                            self.wire_builtin_twopin(&inst_name, &fc.params)?;
                                            self.auto_inst_map.insert(key, inst_name);
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }

                        // ── P1-D fallback: find last component of the expected type ─────────────
                        // If the caller phrase was transformed (e.g. by phrase_to_members turning FuncCall into Node),
                        // look for the most recent component of the appropriate type (CAP for Cap, RES for Pullup/Pulldown)
                        let func_name_str = fc.func_name.to_string();
                        let target_types: Vec<&str> = match func_name_str.rsplit('.').next() {
                            Some("Cap") => {
                                vec!["CAP", "CAP.CER", "CAP.ELE", "CAP.FILM", "CAP.TANT"]
                            }
                            Some(s)
                                if s.eq_ignore_ascii_case("Pullup")
                                    || s.eq_ignore_ascii_case("Pulldown") =>
                            {
                                vec!["RES"]
                            }
                            _ => vec![],
                        };
                        if !target_types.is_empty() {
                            if let Some(comp) = self.components.iter().rev().find(|c| {
                                let cls_name =
                                    c.def.name.to_string().replace('.', "_").to_uppercase();
                                target_types.iter().any(|&t| {
                                    let t_uppercase = t.replace('.', "_").to_uppercase();
                                    cls_name == t_uppercase || cls_name.starts_with(&t_uppercase)
                                })
                            }) {
                                let caller_inst_name = comp.name.clone();
                                self.wire_builtin_twopin(&caller_inst_name, &fc.params)?;
                                self.auto_inst_map.insert(key, caller_inst_name);
                                return Ok(());
                            }
                        }
                    }
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
                        self.components.extend(new_components);
                        self.connections.extend(new_connections);
                    }
                    FuncCallInst::SubModule {
                        inst,
                        new_connections,
                    } => {
                        self.auto_inst_map.insert(key, inst.name.clone());
                        self.sub_modules.push(inst);
                        self.connections.extend(new_connections);
                    }
                    FuncCallInst::PassThrough => {
                        // ── P2-2: check Endpoint return side channel ─────────────────
                        // instantiate_instance_method sets this when it detects
                        // McFuncReturn::Endpoint. Takes priority over P0-4 stub path.
                        let return_ep = super::funccall_inst::LAST_RETURN_ENDPOINT
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
                            let last_seg = class_name.rsplit('.').next().unwrap_or("");
                            let class_looking = class_name.contains('.')
                                || last_seg
                                    .chars()
                                    .next()
                                    .is_some_and(|c| c.is_ascii_uppercase());
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
                            let caller_looks_like_class = caller_name
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_uppercase());
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
                                // Symptom: hbl mcu513 module's 3 decoupling caps
                                //   `CAP_1` / `CAP_2` / `CAP_3` have already been
                                //   actually registered in self.components by
                                //   `instantiate_component_construction` via
                                //   `auto_name(safe_type)` (and written to InstTable),
                                //   but the same line's FuncCall dispatch through the
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
                                // created by inner —— equivalent to P1-D's
                                // `wire_builtin_twopin` map_hit path, just that P1-D
                                // uses pointer key match, we use class name match +
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
                                //     P1-D works properly, both inner and outer map to
                                //     the same "CAP_1". We want to replicate this
                                //     semantics, deliberately **not** use
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
                                    let stub = self.auto_name(&format!("@?{safe}"));
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
            McPhrase::Series(_) => {}
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

    /// Get the McPhrase's pointer address as a unique identifier
    ///
    /// Within the same `process_line` call scope, the address of the same
    /// McPhrase reference is stable, so it can be safely used as a HashMap
    /// key to associate process_member_internal with get_left/right_points.
    pub(super) fn member_key(member: &McPhrase) -> usize {
        member as *const McPhrase as usize
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
            McPhrase::Series(phrases) if phrases.len() == 1 => {
                Self::extract_caller_inst_name(&phrases[0])
            }
            // ── Iter-6.S2 ────────────────────────────────────────────────
            // Chained call support: caller is itself a FuncCall (e.g. `setup()`
            // in `mcu513.setup().capIt()` is capIt's caller).
            //
            // Semantically, each layer's "this" on the chain is the innermost
            // real instance. Therefore recurse inward along fc.caller until
            // hitting an Endpoint or returning None.
            //
            // Example:
            //   `mcu513.setup(V3V3, V1V2).capIt().i2c().loadFlash(flash)`
            // parsed as
            //   FuncCall { name=loadFlash, caller=
            //     FuncCall { name=i2c, caller=
            //       FuncCall { name=capIt, caller=
            //         FuncCall { name=setup, caller=Endpoint(Module(mcu513)) }}}}
            //
            // When taking loadFlash's caller_inst_name, this function drills
            // down layer by layer:
            //   loadFlash.caller (FuncCall i2c)
            //     → i2c.caller (FuncCall capIt)
            //       → capIt.caller (FuncCall setup)
            //         → setup.caller (Endpoint(Module(mcu513)))  ← end
            //           → returns "mcu513"
            //
            // Compatible rollback: if a middle caller in the chain is None
            // (shouldn't happen in theory, parser should treat empty caller
            // as Endpoint), recursion naturally returns None, degrading to
            // pre-fix behavior.
            McPhrase::FuncCall(fc) => fc
                .caller
                .as_deref()
                .and_then(Self::extract_caller_inst_name),
            _ => None,
        }
    }

    /// Recognize the "array-form caller pointing to a set of already-declared
    /// instances" form.
    ///
    /// For example, the caller of `cap[4:5]::CAP(1uF)` might be
    /// `Bus("cap[4:5]")` or `Label("cap[4:5]")`. We try:
    ///   1. Extract the caller's name string
    ///   2. Use `McIds::from(&name).expand()` to expand into a named list
    ///      (e.g. ["cap4","cap5"])
    ///   3. If all expanded names exist in self.components, consider it a hit
    ///
    /// Returns `Some(vec!["cap4", "cap5"])` on hit, otherwise None.
    ///
    /// ── Iter-3.D ───────────────────────────────────────────────────────
    /// Added fallback: if parser resolves `res[1:2]` to `Component(res1)`
    /// (taking the first existing instance of the array), we can also detect
    /// it: check if the caller name ends with a digit suffix, if so probe
    /// adjacent sibling instances like res2/res3 to assemble the array.
    pub(super) fn resolve_array_caller_to_existing(
        &self,
        phrase: &McPhrase,
    ) -> Option<Vec<String>> {
        use crate::core::basic::mc_ids::McIds;

        // First try to extract name from Label/Bus
        let name_with_bracket = match phrase {
            McPhrase::Endpoint(McEndpoint::Single(iref)) => match &iref.base {
                McInstance::Label(s) => Some(s.clone()),
                McInstance::Bus(b) if b.member.is_empty() => Some(b.name.clone()),
                _ => None,
            },
            _ => None,
        };

        if let Some(name) = name_with_bracket {
            if name.contains('[') {
                let ids = McIds::from(name.as_str());
                let expanded = ids.expand();
                if expanded.len() > 1 {
                    let all_exist = expanded
                        .iter()
                        .all(|n| self.components.iter().any(|c| &c.name == n));
                    if all_exist {
                        return Some(expanded);
                    }
                }
            }
        }

        // Iter-3.D added: Component(res1) form — parser only took the first
        // after expansion
        if let McPhrase::Endpoint(McEndpoint::Single(iref)) = phrase {
            if let McInstance::Component(c) = &iref.base {
                let cname = c.name.to_string();

                // ── Iter-6.S5.3 ────────────────────────────────────────
                // Exclude `@` prefix auto-named components (`@CAP1`, `@CAP2`, ...).
                //
                // Background: when pass1 parses inline component constructions
                // without user-explicit naming like `CAP(v).Cap(x)` / `RES(v).Pullup(x)` /
                // `LDO.SGM... ldo`, it auto-allocates names by `@<CLASS><N>`
                // counter. These names have **no array relationship** —— they are
                // **independent, completely unrelated components**, just happening
                // to share the auto-increment counter.
                //
                // The Iter-3.D heuristic's target case is: user writes
                // `res[1:2]::RES(0Ω)`, parser only expands to `res1`, we use
                // sibling probing to splice `res2` back. That case's user name
                // **will absolutely not start with `@`** (`@` is the pass1
                // auto-naming reserved prefix).
                //
                // Without this exclusion it leads to **fatal silent-return bug**:
                //   - Any two inline-created `@CAP1` / `@CAP2` in the same
                //     module will be treated by the heuristic as an array
                //     `[@CAP1, @CAP2]`
                //   - When `.Cap(...)` such builtin twopin method is called
                //     with caller of `Cap` class (i.e. `@CAP1`), Iter-1.3
                //     early-exits treating caller as array, **skips P1-D
                //     wire_builtin_twopin**, never connects pins 1/2, the
                //     component is isolated.
                //   - Verified foot-guns (power.mc:102 + modldo:65):
                //       `CAP(10uF).Cap(lp322dcdc{Vin, GND})`  → @CAP1 isolated
                //       `vin -> ldo.VIN => CAP(10uF).Cap(_)`  → ldo.VIN is
                //          also wrongly connected to both @CAP1.1 and @CAP2.1
                //          (because @@ARRAY encoding makes
                //          resolve_funccall_left_points return two points),
                //          then through @CAP2 internal wiring to vout, finally
                //          vin~vout short
                //
                // Fix: when Component name starts with `@`, directly return None,
                // letting subsequent P1-D path handle normally.
                if cname.starts_with('@') {
                    return None;
                }

                // Check if name ends with one or more digits
                let digit_start = cname
                    .char_indices()
                    .rev()
                    .take_while(|(_, ch)| ch.is_ascii_digit())
                    .last()
                    .map(|(i, _)| i);
                if let Some(idx) = digit_start {
                    let (base, num_str) = cname.split_at(idx);
                    if !base.is_empty() && !num_str.is_empty() {
                        if let Ok(start_num) = num_str.parse::<usize>() {
                            // Check if base+(start_num+1), base+(start_num+2), etc. are all in components
                            let mut collected = vec![cname.clone()];
                            let mut k = start_num + 1;
                            // Set upper bound 16 to prevent unbounded scan, actual array length usually small
                            while k < start_num + 16 {
                                let sibling = format!("{base}{k}");
                                if self.components.iter().any(|c| c.name == sibling) {
                                    collected.push(sibling);
                                    k += 1;
                                } else {
                                    break;
                                }
                            }
                            if collected.len() > 1 {
                                return Some(collected);
                            }
                        }
                    }
                }
            }
        }

        None
    }
}

/// Determine if it's a single-end connection: a and b have the same length and
/// each pair of points has equal paths
fn is_single_ended(a: &[NetPoint], b: &[NetPoint]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // Compare one by one after sorting by path
    let mut a_paths: Vec<&str> = a.iter().map(|p| p.path.as_str()).collect();
    let mut b_paths: Vec<&str> = b.iter().map(|p| p.path.as_str()).collect();
    a_paths.sort_unstable();
    b_paths.sort_unstable();
    a_paths == b_paths
}

// ── Root cause C helper (line.rs module private; same-name function in group.rs not in this module's scope) ──────
/// Get the last segment of a path (`mic.MIC.P` → `P`, `V3V3` → `V3V3`).
fn lr_last_seg(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// Determine if a name is ground. Consistent with `is_ground_name` in group.rs.
fn lr_is_ground_name(s: &str) -> bool {
    let u = s.to_uppercase();
    matches!(u.as_str(), "GND" | "VSS" | "AGND" | "DGND" | "PGND")
        || u.starts_with("GND")
        || u.starts_with("VSS")
}
