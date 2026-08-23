// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! bus processing (Iteration 5)
//!
//! - `ensure_bus`               —— register or merge bus members incrementally
//! - `find_bus` / `is_bus`      —— bus lookup
//! - `expand_node_element`      —— McBus → multiple NetPoint
//! - `resolve_curly_mn_points`  —— resolve `m{a,b}` curly-mn
//! - `process_curly_mn_as_bus`  —— generate base_name-type NetPoint list

use super::McModuleInst;
use crate::instant::mc_bus::McBusInst;
use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::common::IOType;

impl McModuleInst {
    /// register or merge bus members incrementally
    ///
    /// ## Iter 3: from "equality check" to "incremental merge"
    ///
    /// In projects, access to the same bus is naturally cumulative:
    ///
    /// ```text
    /// uC.XTAL    - X6.2          # → ensure_bus("uC", ["XTAL"])
    /// uC.UART0   - cap4.1         # → ensure_bus("uC", ["UART0"])
    /// uC.pins[8:11] - SPI.SCLK    # → ensure_bus("uC", ["pins[8:11]"])
    /// ```
    ///
    /// ## Iter 4: incremental merge
    ///
    /// Old version did equality check, and WARN #921 and silently dropped new members.
    /// `InstTable` enumerates members in `bus_inst.members` enumeration.
    /// Registered member paths (`main.mcu.uC/UART0` etc.), once a member
    /// is lost, the downstream `resolve_netpoint` bus-member fallback fails,
    /// and the connection is silently removed from the graph.
    ///
    /// Delegates to the [`McBusInst::merge_members`]: existing members are
    /// skipped, new members are appended in incoming order. Therefore
    /// ensure_bus no longer produces WARN #921 — for the current project's
    /// code paths, bus definitions do **not** distinguish between "strict
    /// declaration vs. implicit access"; all calls come from access
    /// contexts; strict declaration validation should be handled in a later
    /// lint layer (Iter 5+), not here.
    pub(super) fn ensure_bus(&mut self, name: &str, members: &[String]) -> Result<(), InstError> {
        if let Some(existing) = self.buses.get_mut(name) {
            existing.merge_members(members);
        } else {
            // P2-6: deduplicate members before creating new bus.
            // Component pins like GND can appear multiple times (e.g. pin 21
            // has names ["GND", "ADC.GND"], both mapping to pin 21), causing
            // duplicate entries in the Multi port. Deduplication prevents
            // shape mismatches when labels expand to too many entries.
            let mut deduped: Vec<String> = Vec::new();
            for m in members {
                if !deduped.contains(m) {
                    deduped.push(m.clone());
                }
            }
            self.buses
                .insert(name.to_string(), McBusInst::new(name, deduped));
        }
        Ok(())
    }

    /// find bus definition
    pub(super) fn find_bus(&self, name: &str) -> Option<&McBusInst> {
        self.buses.get(name)
    }

    /// check if a bus is known
    pub(super) fn is_bus(&self, name: &str) -> bool {
        self.buses.contains_key(name)
    }

    /// Validate that `member` is declared on a typed interface port `owner`.
    ///
    /// The rule (user-confirmed): when a port's member set is **fixed by its
    /// declaration** — a typed interface port like `out vout::DC(3.3V)` →
    /// `{VCC, GND}`, or explicit curly/bracket members — referencing an
    /// undeclared member is a hard error: it would otherwise silently create a
    /// dangling net (e.g. `vout.VCC1V2` never ties to the rail). Bare ports /
    /// free labels / component sub-buses (member set not declared) stay
    /// lenient — they legitimately create new nets.
    ///
    /// Fires ONLY for module ports with a declared member set. Rail labels
    /// (`V3V3::DC(3.3V)`) and free labels (`K.45 <- ...` accumulation) are not
    /// module ports and are left lenient — validating against their
    /// incrementally-merged bus would false-positive (members accumulate
    /// access-by-access, e.g. `K` sees `["45"]` before `K.46` is merged).
    ///
    /// MUST be called **before** `ensure_bus` merges the reference into the
    /// member set, otherwise the undeclared member would already be present.
    pub(super) fn check_bus_member_ref(&self, owner: &str, member: &str) {
        if owner.is_empty() || member.is_empty() {
            return;
        }
        // Component/submodule instance pins expand separately — not buses.
        // Check the FIRST segment: `lpa.IN` is a component-pin sub-bus whose
        // members accumulate incrementally and must not be treated as a fixed
        // declaration; `modldo.vout` is a submodule port (validation deferred).
        let first = owner.split('.').next().unwrap_or(owner);
        if self.find_component(first).is_some() || self.find_submodule(first).is_some() {
            return;
        }
        // The only authoritative "declared" member set is the port's own
        // declaration (typed interface / curly `{A, B}` / bracket `[A, B]`).
        // The module port's bus_members is populated at instantiation from the
        // declaration — it is NOT the incrementally-merged bus (whose members
        // accumulate access-by-access and are only partial mid-body, e.g.
        // `dc{VDD_3V3, GND}` sees `["GND"]` before `dc.VDD_3V3` has been merged
        // yet — that reference is valid).
        let Some(declared) = self
            .ports
            .iter()
            .find(|p| super::phases::port_base_name(&p.name) == owner)
            .map(|p| &p.bus_members)
        else {
            // Not a module port (rail label / free label / connector instance):
            // no fixed declaration — lenient.
            return;
        };
        if declared.is_empty() {
            // Bare port without a declared member set — lenient.
            return;
        }
        if !declared.iter().any(|m| m == member) {
            let msg = crate::errcodes::format_msg(
                crate::errcodes::BUS_MEMBER_UNDECLARED,
                &[
                    &owner.to_string(),
                    &member.to_string(),
                    &format!("{:?}", declared),
                ],
            );
            let (uri, pos) = match (&self.current_func_span, &self.current_stmt_span) {
                (Some(sp), _) => (sp.uri.clone(), sp.offset),
                (None, Some(s)) => (s.uri.clone(), s.offset),
                (None, None) => (self.def_uri.clone(), 0),
            };
            crate::db::diagnostic::diagnostic::diagnostic_log_at(
                crate::errcodes::BUS_MEMBER_UNDECLARED,
                crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                uri,
                pos,
                1,
                &msg,
                &[],
            );
        }
    }

    /// expand McBus to multiple NetPoint points
    ///
    /// e.g. power{VCC, GND} -> [power.VCC, power.GND]
    /// flattened version: element.member is Vec<String>
    pub(super) fn expand_node_element(&mut self, element: &McBus) -> Vec<NetPoint> {
        if element.member.is_empty() {
            // ── Iter-8 ───────────────────────────────────────────────
            // Port N×1 bus expansion: when element's own member is empty,
            // check whether it corresponds to a port declared with ≥2
            // members; if so, expand according to the declaration.
            // See the Iter-8 comment above points.rs::expand_port_lanes
            // for details.
            if !element.name.is_empty() {
                if let Some(lanes) = self.expand_port_lanes(&element.name) {
                    return lanes;
                }
            }
            // Leaf node, directly convert
            vec![self.node_to_netpoint(element)]
        } else {
            // Recursively expand sub-members (members are Vec<String>)
            let mut points = Vec::new();
            for member_name in &element.member {
                let full_path = if element.name.is_empty() {
                    member_name.clone()
                } else {
                    format!("{}.{}", element.name, member_name)
                };

                let member_elem = McBus {
                    name: full_path,
                    member: Vec::new(),
                    full_members: Vec::new(),
                };

                // §8.9.6.7: stamp the structured lane member name on each
                // expanded point so downstream per-lane port-group identity
                // is structured (never a dotted-path string split).
                for p in self.expand_node_element(&member_elem) {
                    points.push(p.with_member_name(member_name));
                }
            }
            points
        }
    }

    /// expand McBus to multiple NetPoint points
    /// Handle nested McBus, recursively expand all members
    /// flattened version: element.member is Vec<String>
    pub(super) fn expand_node_element_to_points(
        &mut self,
        element: &McBus,
    ) -> Result<Vec<NetPoint>, InstError> {
        if element.member.is_empty() {
            // ── Iter-8 ───────────────────────────────────────────────
            // Port N×1 bus expansion (mirrored expand_node_element rename)。
            // Transposed `'`-delimited inner_line may still be a bare port ref (like
            // `XTAL + R442::RES'`'s XTAL), so we need to ensure transposed path ports
            // also expand as declared.
            if !element.name.is_empty() {
                if let Some(lanes) = self.expand_port_lanes(&element.name) {
                    return Ok(lanes);
                }
            }
            // Leaf node, directly convert
            Ok(vec![self.node_to_netpoint(element)])
        } else {
            // Recursively expand sub-members (member is Vec<String>)
            let mut points = Vec::new();
            for member_name in &element.member {
                let full_path = if element.name.is_empty() {
                    member_name.clone()
                } else {
                    format!("{}.{}", element.name, member_name)
                };

                let sub_elem = McBus {
                    name: full_path,
                    member: Vec::new(),
                    full_members: Vec::new(),
                };
                // §8.9.6.7: stamp the structured lane member name on each
                // expanded point (mirror of expand_node_element above).
                for p in self.expand_node_element_to_points(&sub_elem)? {
                    points.push(p.with_member_name(member_name));
                }
            }
            Ok(points)
        }
    }

    /// extract member names from McBus elements
    fn extract_member_names(elements: &[McBus]) -> Vec<String> {
        elements.iter().map(|e| e.name.clone()).collect()
    }

    /// resolve curly-mn points (left/right endpoints of Node)
    ///
    /// Node's left/right McBus (e.g. R1.1, sub1.clk)
    /// but need to look up components/sub_modules/buses to determine correct owner
    ///
    /// `is_left`: true for left endpoint, false for right endpoint
    pub(super) fn resolve_curly_mn_points(
        &mut self,
        left: &[McBus],
        right: &[McBus],
        is_left: bool,
    ) -> Result<Vec<NetPoint>, InstError> {
        let elements = if is_left { left } else { right };

        // ── P1-A3 ────────────────────────────────────────────────────────
        // Curly-mn such as `ldo{vin|vout}` / `mcu{MIC | DAC_OUT, SPK_MUTE}`
        // containing `|` will be assembled by the parser as { name: "ldo",
        // member: ["vin"] } — this "name is the instance, member is the port"
        // form. Note this is **not** an already-joined dotted path like
        // "ldo.vin". The previous split_once('.') branch did not apply
        // to "ldo", so it directly went to node_to_netpoint which ate
        // the member field, and all ports were directly mapped via
        // node_to_netpoint.
        if let Some(first) = elements.first() {
            if !first.member.is_empty() {
                let base = &first.name;
                if self.find_submodule(base).is_some() || self.find_component(base).is_some() {
                    let mut points = Vec::new();
                    for elem in elements {
                        if elem.member.is_empty() {
                            // Same batch with empty members: directly map to node_to_netpoint
                            points.push(self.node_to_netpoint(elem));
                        } else {
                            for m in &elem.member {
                                let path = format!("{}.{}", elem.name, m);
                                // ── P2-4: expand bus port to member lanes ──
                                // Submodule ports like MIC{P,N} are bus ports;
                                // expand_port_lanes decomposes them into MIC.P / MIC.N
                                // so the parent module sees individual member points
                                // instead of the bare bus name leaking into unrelated nets.
                                if let Some(lanes) = self.expand_port_lanes(&path) {
                                    points.extend(lanes);
                                } else {
                                    points.push(NetPoint::with_owner(
                                        &path,
                                        &elem.name,
                                        IOType::None,
                                    ));
                                }
                            }
                        }
                    }
                    return Ok(points);
                }
            }

            // Existing path: first element name like "R1.1"
            if let Some((base, _)) = first.name.split_once('.') {
                return self.process_curly_mn_as_bus(base, elements);
            }
        }

        // No path delimiter, directly map
        Ok(elements.iter().map(|e| self.node_to_netpoint(e)).collect())
    }

    /// Process Node structure, return multiple NetPoint points
    ///
    /// Node has three types:
    /// 1. Component pin access: R1{1,2} - base_name "R1" in components
    /// 2. Submodule port access: sub1{a,b} - base_name "sub1" in sub_modules
    /// 3. Bus definition: power{VCC, GND} - Register and lock bus name
    fn process_curly_mn_as_bus(
        &mut self,
        base_name: &str,
        elements: &[McBus],
    ) -> Result<Vec<NetPoint>, InstError> {
        // 1. Component pin access
        if self.find_component(base_name).is_some() {
            return Ok(elements
                .iter()
                .map(|e| {
                    // McBus.name may already be dotted (like "R1.1") or just (like "1")
                    let path = if e.name.contains('.') {
                        e.name.clone()
                    } else {
                        format!("{}.{}", base_name, e.name)
                    };
                    // ── P3-1: normalize alias to physical pin ID (e.g. uC.VDD → uC.5) ──
                    let path = self.normalize_one_inst_pin_path(&path).unwrap_or(path);
                    NetPoint::with_owner(&path, base_name, IOType::None)
                })
                .collect());
        }

        // 2. Submodule port access
        if self.find_submodule(base_name).is_some() {
            let mut pts = Vec::new();
            for e in elements {
                let path = if e.name.contains('.') {
                    e.name.clone()
                } else {
                    format!("{}.{}", base_name, e.name)
                };
                // ── P2: Port is bus (like VDD_3V3, GND /
                //    sub.vin{POWER_SYS, GND}) → expand to lanes
                if let Some(lanes) = self.expand_port_lanes(&path) {
                    pts.extend(lanes);
                } else {
                    pts.push(NetPoint::with_owner(&path, base_name, IOType::None));
                }
            }
            return Ok(pts);
        }

        // 3. Bus definition - Register and lock bus name
        let member_names = Self::extract_member_names(elements);
        self.ensure_bus(base_name, &member_names)?;

        Ok(elements
            .iter()
            .map(|e| {
                let path = if e.name.contains('.') {
                    e.name.clone()
                } else {
                    format!("{}.{}", base_name, e.name)
                };
                NetPoint::new(&path, IOType::None)
            })
            .collect())
    }
}
