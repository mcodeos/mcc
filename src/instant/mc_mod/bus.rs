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
use crate::core::basic::mc_bus::McBus;
use crate::core::common::IOType;
use crate::instant::mc_bus::McBusInst;
use crate::instant::mc_net::{InstError, NetPoint};

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
    /// Registered member paths (`main.mcu513.uC/UART0` etc.), once a member
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
            self.buses
                .insert(name.to_string(), McBusInst::new(name, members.to_vec()));
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

                // Recursively expand sub-members (member is Vec<String>)
                points.extend(self.expand_node_element(&member_elem));
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
                points.extend(self.expand_node_element_to_points(&sub_elem)?);
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
        // Curly-mn such as `modldo{vin|vout}` / `mcu513{MIC | DAC_OUT, SPK_MUTE}`
        // containing `|` will be assembled by the parser as { name: "modldo",
        // member: ["vin"] } — this "name is the instance, member is the port"
        // form. Note this is **not** an already-joined dotted path like
        // "modldo.vin". The previous split_once('.') branch did not apply
        // to "modldo", so it directly went to node_to_netpoint which ate
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
                                points.push(NetPoint::with_owner(&path, &elem.name, IOType::None));
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
