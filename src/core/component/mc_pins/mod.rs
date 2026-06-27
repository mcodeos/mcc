// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

pub mod dynamic;

use crate::builder::diagnostic::dlog_trace;
use crate::builder::mcb_get_cmie;
use crate::core::basic::mc_bus::McBus;
use crate::core::basic::mc_ida::IdaSegment;
use crate::core::basic::mc_ids::IdsSegment;
use crate::core::component::mc_attr::{McAttrVal, McAttribute};
use crate::core::mc_ifs::Mc2Interface;
use crate::{
    ast::ast_node::AstNode,
    ast::c_macros::*,
    ast::error::message::*,
    builder::diagnostic::{dlog_error, dlog_warning},
    core::basic::mc_expr::McExpression,
    core::basic::mc_param::McParamValue,
    core::common::IOType,
};
use crate::{McCMIE, McIds, McInt};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum McPinPort {
    NC,                           // Not Connect
    Single(String),               // single pinid
    Multi(Vec<String>),           // multiple pinids, e.g. [1,2] after GPIO[1,2] expansion
    MultiGroup(Vec<Vec<String>>), // multi-group multi-pinids
    List(String, Vec<String>),    // List, e.g. PDM[CLK, DATA], stores complete name and members
    Bus(McBus),                   // Bus, e.g. DC{VDD, GND}, members have structural relationship
    Interface(Arc<Mc2Interface>), // interface
}

#[derive(Debug, Clone)]
pub struct McPin {
    pub iotype: IOType,
    pub id: String,
    pub names: Vec<String>,
    pub values: Arc<Vec<McAttrVal>>,
}

/// McPins definition
#[derive(Debug, Clone)]
pub struct McPins {
    pub pins: BTreeMap<String, McPin>, // all single pins table, <pinid, McPin> btreemap

    pub names_to_id: BTreeMap<String, McPinPort>, // all registered/exported names to pin/pins/bus/ifs table, <name, McPinPort> btreemap

    // pin -> multiple function name/alias mapping (supports multi-option like I2C0 | GPIO)
    // e.g.: "1" -> ["GPIO3", "I2C0.SCL"], "2" -> ["GPIO4", "I2C0.SDA"]
    pub pin_id_to_names: BTreeMap<String, Vec<String>>,

    // 1. label
    // case1.1: 1 = NC                              -> <NC, Single(1)>
    // case1.2: 2 = NC                              -> <NC, Multi(1,2)> same-name merge, Single(1) becomes Ids(1,2)
    // case2: 1 = EN                                -> <EN, Single(1)>
    // case3: 1:2 = GND                             -> <GND, Multi(1, 2)>

    // 2. labels
    // case4: 1:4 = GPIO[1:4]                       -> ida-type GPIO[1:4] expands, <GPIO1, Single(1)>, <GPIO2, Single(2)>... <GPIO4, Single(4)>
    // case5: [7,8] = [VDD, GND]                    -> <VDD, Single(7)>, <GND, Single(8)>

    // 3. bus
    // case6: 7:8 = DC2{VDD, GND}                   -> first record Bus<DC2, Bus(McBus(DC2{VDD, GND}))>, then register each Bus member <DC2.VDD, Single(7)>, <DC2.GND, Single(8)>
    // case7.1: 7 = DC2.VDD                         -> for separate write, first record Bus<DC2, McBus(DC2{VDD}))>, then register pin <DC2.VDD, Single(7)>
    // case7.2: 8 = DC2.GND                         ->  then add/update Bus<DC2, McBus(DC2{VDD,GND}))>, then register pin <DC2.GND, Single(8)>

    // 4. inteface
    // case8: [7,8] = DC1::DC()                     -> for ifs without child members, first record Interface<DC1, Mc2Interface(DC(7,8))>, then find DC() interface definition, find member names, use member default names, register each pin <DC1.member1, Single(7)>, <DC1.member2, Single(8)>
    // case9: [7:8] = [VDD, GND]::DC()              -> for ifs without interface instance name, directly register pins <VDD, Single(7)>, <GND, Single(8)>
    // case10: 13:14 = DC2{VDD, GND}::DC()          -> ifs with instance name / with child members, first record Interface<DC1, Mc2Interface(DC(13,14))>, then each pin <DC2.VDD, Single(13)>, <DC2.GND, Single(14)>

    // 5. group vs labels/bus/interface
    // case11: [[7,8],[9,10]] = [VDD, GND]          -> <VDD, Multi(7,8)>, <GND, Multi(9,10)>
    // case12: [[7,8],[9,10]] = [VDD, GND]::DC()    -> <VDD, Multi(7,8)>, <GND, Multi(9,10)>
    // case13: [[7,8],[9,10]] = DC2{VDD, GND}       -> unsupported, can't determine which DC2 instance
    values_pool: Vec<Arc<Vec<McAttrVal>>>, // unique values pool to avoid duplication

    // Dynamic pin definitions (contain parameter references, need to be resolved at instantiation)
    // e.g.: 1:cols = 1:cols, where cols is component parameter
    pub dynamic_pins: Vec<dynamic::DynamicPinLine>,
}

impl Default for McPins {
    fn default() -> Self {
        Self::new()
    }
}

impl McPins {
    pub fn new() -> Self {
        Self {
            pins: BTreeMap::new(),
            names_to_id: BTreeMap::new(),
            pin_id_to_names: BTreeMap::new(),
            values_pool: Vec::new(),
            dynamic_pins: Vec::new(),
        }
    }

    pub fn has_dynamic_pins(&self) -> bool {
        !self.dynamic_pins.is_empty()
    }

    pub fn resolve_dynamic_pins(
        &self,
        param_bindings: &[(String, i64)],
    ) -> Vec<(i64, String, IOType)> {
        let mut results = Vec::new();

        for dyn_pin in &self.dynamic_pins {
            let resolved = dyn_pin.resolve(param_bindings);
            for (pin_id, pin_name) in resolved {
                results.push((pin_id, pin_name, dyn_pin.iotype.clone()));
            }
        }

        results
    }

    fn parse_dynamic_pin_line(
        &self,
        node: &AstNode,
        iotype: IOType,
        values: &[McAttrVal],
    ) -> Option<dynamic::DynamicPinLine> {
        let subnodes = node.get_sub_node()?;

        let mut pin_id_expr: Option<dynamic::DynamicPinExpr> = None;
        let mut pin_name_expr: Option<dynamic::DynamicPinExpr> = None;

        for subnode in subnodes.iter() {
            match subnode.get_type() {
                MCAST_PIN_ID => {
                    if let Some(pid_node) = subnode.get_sub_node() {
                        if let Some(expr) = dynamic::DynamicPinExpr::from_ast(&pid_node) {
                            pin_id_expr = Some(expr);
                        }
                    }
                }
                MCAST_PIN_NAMES => {
                    if let Some(names_node) = subnode.get_sub_node() {
                        for name_option in
                            names_node.iter().filter(|n| n.get_type() == MCAST_PIN_NAME)
                        {
                            if let Some(name_sub) = name_option.get_sub_node() {
                                if let Some(expr) = dynamic::DynamicPinExpr::from_ast(&name_sub) {
                                    pin_name_expr = Some(expr);
                                    break;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let pin_id_expr = pin_id_expr?;

        let mut line = dynamic::DynamicPinLine::new()
            .with_iotype(iotype)
            .with_values(values.to_vec())
            .with_pin_id(pin_id_expr);

        if let Some(name_expr) = pin_name_expr {
            line = line.with_pin_name(name_expr);
        }

        Some(line)
    }

    pub fn is_bus(&self, name: &str) -> bool {
        if let Some(port) = self.names_to_id.get(name) {
            matches!(port, McPinPort::Bus(_))
        } else {
            false
        }
    }

    pub fn is_interface(&self, name: &str) -> bool {
        if let Some(port) = self.names_to_id.get(name) {
            matches!(port, McPinPort::Interface(_))
        } else {
            false
        }
    }

    pub fn get_bus_members(&self, name: &str) -> Option<Vec<String>> {
        if let Some(port) = self.names_to_id.get(name) {
            match port {
                McPinPort::Bus(bus) => Some(bus.full_members.clone()),
                _ => None,
            }
        } else {
            None
        }
    }

    /// ── S3 fix: get a port's bus_members (supports both Bus and Interface forms) ──
    /// For Interface port, collect members via "PORT.MEMBER" form dot alias
    /// in Component's pin_id_to_names (sorted by pinid numeric ascending, deduped).
    pub fn get_bus_members_for_port(&self, name: &str) -> Vec<String> {
        if let Some(port) = self.names_to_id.get(name) {
            match port {
                McPinPort::Bus(bus) => return bus.full_members.clone(),
                McPinPort::Interface(_iface_arc) => {
                    // ── S3 fix: extract "PORT.MEMBER" form from pin_id_to_names
                    // member name. Order by pinid numeric ascending, dedup by pid.
                    let prefix = format!("{name}.");
                    let mut members: Vec<String> = Vec::new();
                    let mut seen_pids: BTreeSet<String> = BTreeSet::new();
                    let mut sorted_keys: Vec<&String> = self.pins.keys().collect();
                    sorted_keys.sort_by(|a, b| {
                        let na: i64 = a.parse().unwrap_or(0);
                        let nb: i64 = b.parse().unwrap_or(0);
                        na.cmp(&nb)
                    });
                    for pid in sorted_keys {
                        if seen_pids.contains(pid) {
                            continue;
                        }
                        if let Some(names) = self.pin_id_to_names.get(pid) {
                            for n in names {
                                if let Some(rest) = n.strip_prefix(&prefix) {
                                    members.push(rest.to_string());
                                    seen_pids.insert(pid.clone());
                                    break;
                                }
                            }
                        }
                    }
                    return members;
                }
                _ => return Vec::new(),
            }
        }
        Vec::new()
    }

    pub fn parse(&mut self, node: &AstNode) {
        // MCAST_ATTRIBUTE_PIN / MCAST_ATTRIBUTE_PINADD
        // |-*  MCAST_PIN_LINE *
        let Some(plinenodes) = node.get_sub_node() else {
            dlog_error(1001, node, MISSING_SUBNODE);
            return;
        };

        for pnode in plinenodes.iter().filter(|n| n.get_type() == MCAST_PIN_LINE) {
            // MCAST_PIN_LINE
            // |-MCAST_IOTYPE (option) - MCAST_PIN_ID - MCAST_PIN_NAMES - MCAST_ATT_VALUES (option)
            let subnodes = pnode.get_sub_node().expect(MISSING_SUBNODE);

            let mut iotype: Option<IOType> = None;
            let mut pinids: Option<McPinPort> = None;
            let mut pinnames: Option<McPinNames> = None;
            let mut values: Option<Vec<McAttrVal>> = None;
            let mut pin_name_has_param_ref = false;

            for subnode in subnodes.iter() {
                match subnode.get_type() {
                    MCAST_IOTYPE => {
                        iotype = IOType::new(&subnode);
                    }
                    MCAST_PIN_ID => {
                        pinids = McPins::parse_pinid(&subnode);
                        // Note: We don't check pin_id_has_param_ref for pin IDs because
                        // pin IDs are always fixed (numbers or letters), never parameter references.
                        // Dynamic pins are handled via pin_name_has_param_ref instead.
                    }
                    MCAST_PIN_NAMES => {
                        pinnames = McPinNames::new(&subnode);
                        if let Some(names) = &pinnames {
                            pin_name_has_param_ref = names.has_param_ref();
                        }
                    }
                    MCAST_ATT_VALUES => {
                        values = McAttribute::new_attr_values(&subnode);
                    }
                    _ => {
                        dlog_error(1204, &subnode, TYPE_MISMATCH);
                    }
                }
            }

            let iotype = iotype.unwrap_or(IOType::None);
            let names = pinnames.unwrap_or_default();
            let values = values.unwrap_or_default();

            if pin_name_has_param_ref {
                if let Some(dyn_line) = self.parse_dynamic_pin_line(&pnode, iotype, &values) {
                    self.dynamic_pins.push(dyn_line);
                }
                continue;
            }
            // (placeholder; probes below to be removed)

            // Only check pinids when there's no parameter reference
            let Some(pinids) = pinids else {
                continue;
            };

            for optname in &names.options {
                match optname {
                    McPinPort::NC => {
                        // Register corresponding pinid as NC
                        match &pinids {
                            McPinPort::Single(s) => {
                                self.register_pin(iotype.clone(), s, &["NC".to_string()], &values);
                            }
                            McPinPort::Multi(pids) => {
                                for pid in pids {
                                    self.register_pin(
                                        iotype.clone(),
                                        pid,
                                        &["NC".to_string()],
                                        &values,
                                    );
                                }
                            }
                            _ => {
                                dlog_trace(1103, "Pin ID and name not match");
                            }
                        };
                    }

                    McPinPort::Single(name) => {
                        if let Some((bus_name, member_name)) = name.split_once('.') {
                            // Handle dot-separated pin names like "IN.P" -> create Bus "IN" with member "P"
                            match &pinids {
                                McPinPort::Single(s) => {
                                    self.register_pin(iotype.clone(), s, &[name.clone()], &values);
                                }
                                McPinPort::Multi(pids) => {
                                    for pid in pids {
                                        self.register_pin(
                                            iotype.clone(),
                                            pid,
                                            &[name.clone()],
                                            &values,
                                        );
                                    }
                                }
                                _ => {}
                            }
                            match self.names_to_id.get_mut(bus_name) {
                                Some(existing) => {
                                    if let McPinPort::Bus(bus) = existing {
                                        if !bus.full_members.contains(&member_name.to_string()) {
                                            bus.member.push(member_name.to_string());
                                            bus.full_members.push(member_name.to_string());
                                        }
                                    } else {
                                        let mut new_bus = McBus::new(bus_name);
                                        new_bus.member.push(member_name.to_string());
                                        new_bus.full_members.push(member_name.to_string());
                                        *existing = McPinPort::Bus(new_bus);
                                    }
                                }
                                None => {
                                    let mut new_bus = McBus::new(bus_name);
                                    new_bus.member.push(member_name.to_string());
                                    new_bus.full_members.push(member_name.to_string());
                                    self.names_to_id
                                        .insert(bus_name.to_string(), McPinPort::Bus(new_bus));
                                }
                            }
                        } else {
                            match &pinids {
                                // 1 pid vs 1 name
                                McPinPort::Single(s) => {
                                    self.register_pin(iotype.clone(), s, &[name.clone()], &values);
                                }
                                // n pids vs 1 name, all pinids registered with same name
                                McPinPort::Multi(pids) => {
                                    for pid in pids {
                                        self.register_pin(
                                            iotype.clone(),
                                            pid,
                                            &[name.clone()],
                                            &values,
                                        );
                                    }
                                }
                                // MultiGroup vs Single name: [A1, Y1] = VCC -> A1->VCC, Y1->VCC
                                McPinPort::MultiGroup(groups) => {
                                    for grp in groups.iter() {
                                        for pid in grp.iter() {
                                            self.register_pin(
                                                iotype.clone(),
                                                pid,
                                                &[name.clone()],
                                                &values,
                                            );
                                        }
                                    }
                                }
                                _ => {
                                    dlog_trace(1103, "Pin ID and name not match");
                                }
                            };
                        }
                    }

                    McPinPort::Multi(names_vec) => {
                        match &pinids {
                            // 1 pid vs n names: register the same pinid with all names (aliases)
                            McPinPort::Single(s) => {
                                self.register_pin(iotype.clone(), s, names_vec, &values);
                            }
                            // n pids vs n name, pinid and name count should be 1:1, register 1:1
                            McPinPort::Multi(pids) => {
                                let names_cycle = names_vec.iter().cycle();
                                for (pid, name) in pids.iter().zip(names_cycle) {
                                    self.register_pin(
                                        iotype.clone(),
                                        pid,
                                        &[name.clone()],
                                        &values,
                                    );
                                }
                                // Check if names share a common base (like GPIO1, GPIO2 sharing "GPIO")
                                // If so, only register the base as Multi if there's no existing Interface with the same name
                                if let Some(base_name) = self.extract_common_base(names_vec) {
                                    // Skip if an Interface with this base name already exists
                                    if self
                                        .names_to_id
                                        .get(&base_name)
                                        .is_some_and(|p| matches!(p, McPinPort::Interface(_)))
                                    {
                                        // Interface already exists, don't create duplicate Multi
                                    } else {
                                        match self.names_to_id.get_mut(&base_name) {
                                            Some(existing) => {
                                                // Merge into existing Multi
                                                if let McPinPort::Multi(existing_pids) = existing {
                                                    for pid in pids {
                                                        if !existing_pids.contains(pid) {
                                                            existing_pids.push(pid.clone());
                                                        }
                                                    }
                                                } else {
                                                    // Convert to Multi
                                                    *existing = McPinPort::Multi(pids.clone());
                                                }
                                            }
                                            None => {
                                                self.names_to_id.insert(
                                                    base_name.clone(),
                                                    McPinPort::Multi(pids.clone()),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            // MultiGroup vs Multi names: [[20,21],[22,23]] = [VDD, GND]
                            // Each group cycles through the names independently: 20->VDD, 21->GND, 22->VDD, 23->GND
                            McPinPort::MultiGroup(groups) => {
                                for grp in groups.iter() {
                                    let names_cycle = names_vec.iter().cycle();
                                    for (pid, name) in grp.iter().zip(names_cycle) {
                                        self.register_pin(
                                            iotype.clone(),
                                            pid,
                                            &[name.clone()],
                                            &values,
                                        );
                                    }
                                }
                            }
                            _ => {
                                dlog_trace(1103, "Pin ID and name not match");
                            }
                        };
                    }

                    McPinPort::List(list_name, members) => {
                        // PDM[CLK, DATA] -> register list_name and each member
                        match &pinids {
                            McPinPort::Multi(pids) => {
                                // register each member to corresponding pin
                                let names_cycle = members.iter().cycle();
                                for (pid, name) in pids.iter().zip(names_cycle) {
                                    self.register_pin(
                                        iotype.clone(),
                                        pid,
                                        &[name.clone()],
                                        &values,
                                    );
                                }
                                // also register list_name itself to names_to_id
                                self.names_to_id
                                    .insert(list_name.clone(), McPinPort::Multi(pids.clone()));
                            }
                            _ => {
                                dlog_trace(1103, "Pin ID and name not match");
                            }
                        };
                    }

                    McPinPort::Bus(bus) => {
                        match &pinids {
                            // n pids vs 1 bus, pinid and bus member count should be 1:1, register 1:1
                            // also need to add bus name as a Multi containing all bus members
                            McPinPort::Multi(pids) => {
                                let mut all_pin_ids = Vec::new();
                                for (pid, member_name) in pids.iter().zip(bus.member.iter()) {
                                    // For numeric members like GPIO[1:2], concatenate directly (GPIO1, GPIO2)
                                    // For named members like DC1{VDD, GND}, use dot separator (DC1.VDD, DC1.GND)
                                    let full_name = if member_name.parse::<i64>().is_ok() {
                                        format!("{}{}", bus.name, member_name)
                                    } else {
                                        format!("{}.{}", bus.name, member_name)
                                    };
                                    self.register_pin(iotype.clone(), pid, &[full_name], &values);
                                    all_pin_ids.push(pid.clone());
                                }
                                // Also register bus name as a Bus containing all bus members
                                if !bus.member.is_empty() {
                                    self.names_to_id
                                        .insert(bus.name.clone(), McPinPort::Bus(bus.clone()));
                                }
                            }
                            _ => {
                                dlog_trace(1103, "Pin ID and name not match");
                            }
                        };
                    }

                    McPinPort::Interface(declare) => {
                        // Get pin names from interface definition
                        // First try: get pins from the specified role parameter (e.g., UART.TTL(DCE))
                        let mut iface_pins: Vec<String> = Vec::new();

                        // Look up role name from params (e.g., "DCE" from UART.TTL(DCE))
                        if let Some(McParamValue::Ids(role_ids)) = declare.params.first() {
                            let role_name = role_ids.to_string();
                            // Find the matching role in base.roles
                            for role in &declare.base.roles {
                                if role.name.to_string() == role_name {
                                    // Key: use pins (BTreeMap sorted by pinid) order
                                    //   - numeric pinid (1, 2, 3...) BTreeMap order = original declaration order
                                    //   - alphabetic pinid (A, B, AB) BTreeMap order ≠ original order,
                                    //     but closer to original than names_to_id.keys() (alphabetic name order)
                                    // for each pin, take names[0] as member name.
                                    iface_pins = role
                                        .pins
                                        .pins
                                        .values()
                                        .filter_map(|p| p.names.first().cloned())
                                        .collect();
                                    break;
                                }
                            }
                        }

                        // Fallback: get pins from interface top-level definition
                        if iface_pins.is_empty() {
                            iface_pins = declare
                                .base
                                .pins
                                .pins
                                .values()
                                .filter_map(|p| p.names.first().cloned())
                                .collect();
                        }

                        // 1305: interface top-level has no pin definitions (all pins are in role, e.g. UART.X),
                        //      and no role specified or role has no pin definitions.
                        //      then mcc can't fit physical pins onto interface members — that line silently
                        //      produces 0 pin registrations. Emit warn to inform user.
                        if iface_pins.is_empty() {
                            dlog_warning(
                                1305,
                                &pnode,
                                &format!(
                                    "Interface '{}' has no top-level pins (all pins are inside `role` blocks, e.g. UART.X); \
                                     no pin-to-member mapping will be created. If you want the role-specific pins \
                                     registered, list them explicitly (e.g. `pins = TX, RX, GND`).",
                                    declare.name,
                                ),
                            );
                        }

                        // 1303: check pin count matches. If interface top-level declares n pins (e.g. SPI=4),
                        //      but LHS only gives m pin IDs, should error instead of silently misaligning/losing members.
                        //      exception: iface_pins empty (pins in role, e.g. UART.X) skip check.
                        let declared_count: Option<usize> = match &pinids {
                            McPinPort::Single(_) => Some(1),
                            McPinPort::Multi(pids) => Some(pids.len()),
                            McPinPort::MultiGroup(groups) => {
                                Some(groups.iter().map(|g| g.len()).sum())
                            }
                            _ => None,
                        };
                        if let Some(dc) = declared_count {
                            if !iface_pins.is_empty() && dc != iface_pins.len() {
                                dlog_error(
                                    1303,
                                    &pnode,
                                    &format!(
                                        "Interface '{}' declares {} pin(s) (members: {:?}) \
                                         but {} pin ID(s) given; the counts must match. \
                                         Use a range like `a:b` to declare exactly {} pin(s).",
                                        declare.name,
                                        iface_pins.len(),
                                        iface_pins,
                                        dc,
                                        iface_pins.len(),
                                    ),
                                );
                            }
                        }

                        let _expanded_inst_names: Vec<String> = declare.name.expand();
                        let subname = derive_interface_subnames(&declare.name, &iface_pins);

                        match &pinids {
                            // n pids vs n interface pins, pinid and interface member count 1:1, register 1:1
                            McPinPort::Multi(pids) => {
                                // cycle subname, 1:1 with pids
                                let names_cycle = subname.iter().cycle();
                                for (pid, name) in pids.iter().zip(names_cycle) {
                                    self.register_pin(
                                        iotype.clone(),
                                        pid,
                                        &[name.clone()],
                                        &values,
                                    );
                                }

                                // check if need to merge same-type single-pin interfaces
                                let _base_iface_name = declare.base_name();
                                let pin_cnt = declare.base.pins.names_to_id.len();
                                let inst_pins: Vec<String> = pids.to_vec();

                                if pin_cnt == 1 {
                                    // for single-pin interface, use declare.name as key (e.g. [LX, GND])
                                    let iface_name = declare.name.to_string();

                                    // check if same-name Interface already exists
                                    if let Some(existing_port) = self.names_to_id.get(&iface_name) {
                                        if let McPinPort::Interface(existing_iface) = existing_port
                                        {
                                            // Merge into existing Interface
                                            let merged = existing_iface.merge_pins_with(&inst_pins);
                                            self.names_to_id.insert(
                                                iface_name.clone(),
                                                McPinPort::Interface(Arc::new(merged)),
                                            );
                                        } else {
                                            // existing is not Interface, create new and merge
                                            let new_iface = Mc2Interface {
                                                base: declare.base.clone(),
                                                name: declare.name.clone(),
                                                params: declare.params.clone(),
                                                insts: declare.insts.clone(),
                                                registered_pins: Vec::new(),
                                                parsed_pins: None,
                                                pin_name_mapping: Vec::new(),
                                            };
                                            let merged = new_iface.merge_pins_with(&inst_pins);
                                            self.names_to_id.insert(
                                                iface_name,
                                                McPinPort::Interface(Arc::new(merged)),
                                            );
                                        }
                                    } else {
                                        // Does not exist, create new and merge
                                        let new_iface = Mc2Interface {
                                            base: declare.base.clone(),
                                            name: declare.name.clone(),
                                            params: declare.params.clone(),
                                            insts: declare.insts.clone(),
                                            registered_pins: Vec::new(),
                                            parsed_pins: None,
                                            pin_name_mapping: Vec::new(),
                                        };
                                        let merged = new_iface.merge_pins_with(&inst_pins);
                                        self.names_to_id.insert(
                                            iface_name,
                                            McPinPort::Interface(Arc::new(merged)),
                                        );
                                    }
                                } else {
                                    // multi-pin interface, also need to update registered_pins
                                    if !subname.is_empty() {
                                        // Interface registration keys in names_to_id:
                                        //   - bus form `NAME{m1,m2}::IFACE()` → use bus name `NAME`
                                        //   - list form `[m1,m2]::IFACE()` → use declare.name.to_string()
                                        //   - normal `INST::IFACE()` → use declare.name.to_string()
                                        let iface_name = if declare.name.is_bus() {
                                            declare
                                                .name
                                                .as_bus()
                                                .map(|(busname, _)| busname)
                                                .unwrap_or_else(|| declare.name.to_string())
                                        } else {
                                            // for list form (e.g. [LX, GND]) and normal form, directly use declare.name
                                            declare.name.to_string()
                                        };
                                        // use inst_pins to call merge_pins_with update registered_pins
                                        let merged = declare.merge_pins_with(&inst_pins);
                                        // dlog_trace(1203, &format!("Bus interface insert: base={}, inst_name={}, iface_name={}, registered_pins={:?}",
                                        //     base_iface_name, declare.name, iface_name, merged.registered_pins));
                                        self.names_to_id.insert(
                                            iface_name,
                                            McPinPort::Interface(Arc::new(merged)),
                                        );
                                    }
                                }
                            }
                            // [[9,10], [11,12]] vs [VDD, GND] -> 9->VDD, 10->GND, 11->VDD, 12->GND
                            // Each pin in each group cycles through names independently
                            McPinPort::MultiGroup(groups) => {
                                for grp in groups.iter() {
                                    let names_cycle = subname.iter().cycle();
                                    for (pid, name) in grp.iter().zip(names_cycle) {
                                        self.register_pin(
                                            iotype.clone(),
                                            pid,
                                            &[name.clone()],
                                            &values,
                                        );
                                    }
                                }
                            }
                            // single pin interface (e.g. GPIO, PWM): single pinid -> use first subname
                            McPinPort::Single(pid) => {
                                let name = subname.first().cloned().unwrap_or_else(|| pid.clone());
                                self.register_pin(iotype.clone(), pid, &[name], &values);
                            }
                            _ => {
                                dlog_trace(1103, "Pin ID and name not match");
                            }
                        };
                    }
                    _ => {}
                }
            }

            // clean duplicate Multi: if Multi name starts with Interface base_name prefix, delete Multi
            let interface_names: BTreeSet<String> = self
                .names_to_id
                .iter()
                .filter_map(|(_, port)| {
                    if let McPinPort::Interface(iface) = port {
                        Some(iface.base.name.to_string())
                    } else {
                        None
                    }
                })
                .collect();

            if !interface_names.is_empty() {
                let multi_keys_to_remove: Vec<String> = self
                    .names_to_id
                    .iter()
                    .filter_map(|(name, port)| {
                        if let McPinPort::Multi(_) = port {
                            // check if Multi name starts with some Interface base_name prefix
                            for iface_name in &interface_names {
                                if name.starts_with(&iface_name.to_string())
                                    && name.len() > iface_name.len()
                                {
                                    return Some(name.clone());
                                }
                            }
                            None
                        } else {
                            None
                        }
                    })
                    .collect();

                for key in multi_keys_to_remove {
                    self.names_to_id.remove(&key);
                }
            }
        }
    }

    fn parse_pinid(node: &AstNode) -> Option<McPinPort> {
        // mc_pin_idn:
        // | mc_int
        // | mc_ida
        // | mc_phrase

        // case 1: single pin               -> McPinPort::Single(String), single pinid
        // case 2: 1 group, multi pins      -> McPinPort::Multi(Vec<String>), multiple pinids
        // case 3: n groups, multi pins     -> McPinPort::MultiGroup(Vec<Vec<String>>), multi-group multi-pinids

        if let Some(pid_node) = node.get_sub_node() {
            match pid_node.get_type() {
                MCAST_INT => McInt::new(&pid_node).map(|pid| McPinPort::Single(pid.to_string())),

                MCAST_ID | MCAST_IDA => {
                    if let Some(pid) = McIds::new(&pid_node) {
                        // check if matrix definition, e.g. R[1:2]C[1:7]
                        // matrix definition may contain multiple square bracket segments in one IdsSegment::Ida
                        let mut square_count = 0;
                        let mut rows = 1;
                        let mut cols = 1;

                        for seg in &pid.segments {
                            match seg {
                                IdsSegment::Ida(ida) => {
                                    // check count of square bracket segments in Ida
                                    let ida_squares = ida
                                        .segments
                                        .iter()
                                        .filter(|s| matches!(s, IdaSegment::Square(_)))
                                        .count();
                                    square_count += ida_squares;

                                    // if only one Ida segment with multiple brackets, also matrix definition
                                    if ida.segments.len() > 1 && ida_squares > 1 {
                                        // calculate rows and cols
                                        // e.g. R[1:2]C[1:7]: R is prefix, [1:2] is row def, [1:7] is col def
                                        let mut found_first_square = false;
                                        for ida_seg in &ida.segments {
                                            if let IdaSegment::Square(items) = ida_seg {
                                                if !found_first_square {
                                                    // first square segment defines row count
                                                    rows = items.len();
                                                    found_first_square = true;
                                                } else {
                                                    // last square segment defines col count
                                                    cols = items.len();
                                                }
                                            }
                                        }
                                    }
                                }
                                IdsSegment::Square(_) => {
                                    square_count += 1;
                                }
                                _ => {}
                            }
                        }

                        let has_multiple_squares = square_count > 1;

                        if has_multiple_squares {
                            // for matrix definition, try parse as MultiGroup
                            // e.g. R[1:2]C[1:7] should parse as [[R1C1, R1C2, ..., R1C7], [R2C1, R2C2, ..., R2C7]]
                            let expanded = pid.expand();
                            let total = expanded.len();

                            // if row/col counts unknown, calculate from element total
                            if rows == 1 || cols == 1 {
                                // try to back-derive row/col counts from expansion result
                                // for R[1:2]C[1:7], expansion has 14 elements
                                // if first square segment is [1:2], row count is 2
                                // if second square segment is [1:7], col count is 7
                                // 14 = 2 * 7, so rows = 2, cols = 7
                            }

                            if total > 0 {
                                // calculate rows/cols from total elements and bracket segment count
                                // if multiple bracket segments, total elements should be rows * cols
                                // try to factor total elements
                                let mut best_rows = 1;
                                let mut best_cols = total;

                                // iterate possible row counts, find best fit decomposition
                                for r in 1..=total {
                                    if total % r == 0 && square_count >= 2 {
                                        let c = total / r;
                                        if c > best_cols / c {
                                            // choose decomposition closer to square
                                            best_rows = r;
                                            best_cols = c;
                                        }
                                    }
                                }

                                // Use the best row/column decomposition
                                rows = best_rows;
                                cols = best_cols;

                                if rows > 1 && cols > 1 && total == rows * cols {
                                    // Group the expansion result by column count
                                    let mut groups = Vec::new();
                                    let mut current_group = Vec::new();

                                    for (i, item) in expanded.iter().enumerate() {
                                        current_group.push(item.clone());
                                        if (i + 1) % cols == 0 {
                                            groups.push(current_group);
                                            current_group = Vec::new();
                                        }
                                    }

                                    if !current_group.is_empty() {
                                        groups.push(current_group);
                                    }

                                    if groups.len() > 1 {
                                        return Some(McPinPort::MultiGroup(groups));
                                    }
                                }
                            }
                        }

                        // normal processing
                        match pid.count() {
                            1 => Some(McPinPort::Single(pid.to_string())),
                            2.. => Some(McPinPort::Multi(pid.expand())),
                            _ => {
                                dlog_error(1103, &pid_node, "Pin id count error");
                                None
                            }
                        }
                    } else {
                        None
                    }
                }

                MCAST_EXPRESSION => {
                    if let Some(exp_node) = pid_node.get_sub_node() {
                        match exp_node.get_type() {
                            MCAST_OPD_COLON => {
                                // Expand `13:14` => ["13","14"]
                                if let Some(expr) = McExpression::new(&exp_node) {
                                    // check if contains parameter reference, skip static parsing if so
                                    if dynamic::DynamicPinExpr::check_param_ref(&expr) {
                                        return None;
                                    }

                                    match expr {
                                        McExpression::Slice(_, _) => {
                                            let ids = expr.expand();
                                            match ids.len() {
                                                1 => Some(McPinPort::Single(ids[0].clone())),
                                                2.. => Some(McPinPort::Multi(ids)),
                                                _ => {
                                                    dlog_error(
                                                        1103,
                                                        &pid_node,
                                                        "Pin id count error",
                                                    );
                                                    None
                                                }
                                            }
                                        }
                                        _ => {
                                            dlog_error(1204, &exp_node, TYPE_MISMATCH);
                                            None
                                        }
                                    }
                                } else {
                                    None
                                }
                            }

                            // Handle OPD_SQUARE_VEC nodes (e.g., [1,2,3] format)
                            MCAST_OPD_SQUARE_VEC => {
                                // Expand `[6,7]` into `["6","7"]`.
                                if let Some(expr) = McExpression::new(&exp_node) {
                                    match expr {
                                        McExpression::Set(items) => {
                                            // Support two-level nesting: `[[9,10],[11,12]]`
                                            let mut flat_ids = Vec::<String>::new();
                                            let mut groups = Vec::<Vec<String>>::new();

                                            for item in items {
                                                match item {
                                                    McExpression::Set(inner_items) => {
                                                        // nested Set, e.g. [[20,21],[22,23]]
                                                        // need to recursively handle each nested Set
                                                        let mut grp = Vec::<String>::new();
                                                        for inner in inner_items {
                                                            match inner {
                                                                McExpression::Set(deep_items) => {
                                                                    // deeper nesting, recursively handle
                                                                    for deep in deep_items {
                                                                        match deep {
                                                                            McExpression::Variable(_) => {
                                                                                // For Variable (like A[20,21]), use expand() to get separate elements
                                                                                grp.extend(deep.expand());
                                                                            }
                                                                            _ => {
                                                                                if let Some(s) = deep.evaluate() {
                                                                                    grp.push(s);
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                _ => {
                                                                    match inner {
                                                                        McExpression::Variable(
                                                                            _,
                                                                        ) => {
                                                                            // For Variable (like A[20,21]), use expand() to get separate elements
                                                                            grp.extend(
                                                                                inner.expand(),
                                                                            );
                                                                        }
                                                                        _ => {
                                                                            if let Some(s) =
                                                                                inner.evaluate()
                                                                            {
                                                                                grp.push(s);
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        if !grp.is_empty() {
                                                            groups.push(grp);
                                                        }
                                                    }
                                                    _ => {
                                                        // Handle Slice expressions like 16:17
                                                        if let McExpression::Slice(_, _) = item {
                                                            let ids = item.expand();
                                                            match ids.len() {
                                                                1 => flat_ids.push(ids[0].clone()),
                                                                2.. => flat_ids.extend(ids),
                                                                _ => {}
                                                            }
                                                        } else if let McExpression::Variable(_) =
                                                            item
                                                        {
                                                            // Each Variable (like A[20,21]) creates its own group
                                                            // This preserves nesting: [A[20,21],A[22:23]] -> [[A20,A21],[A22,A23]]
                                                            let expanded = item.expand();
                                                            if !expanded.is_empty() {
                                                                groups.push(expanded);
                                                            }
                                                        } else if let Some(s) = item.evaluate() {
                                                            flat_ids.push(s);
                                                        }
                                                    }
                                                }
                                            }

                                            if !groups.is_empty() {
                                                Some(McPinPort::MultiGroup(groups))
                                            } else if flat_ids.len() == 1 {
                                                return Some(McPinPort::Single(
                                                    flat_ids[0].clone(),
                                                ));
                                            } else if !flat_ids.is_empty() {
                                                return Some(McPinPort::Multi(flat_ids));
                                            } else {
                                                return None;
                                            }
                                        }
                                        _ => {
                                            // Handle Slice expressions like 16:17
                                            if let McExpression::Slice(_, _) = expr {
                                                let ids = expr.expand();
                                                match ids.len() {
                                                    1 => Some(McPinPort::Single(ids[0].clone())),
                                                    2.. => Some(McPinPort::Multi(ids)),
                                                    _ => {
                                                        dlog_error(
                                                            1103,
                                                            &pid_node,
                                                            "Pin id count error",
                                                        );
                                                        None
                                                    }
                                                }
                                            } else if let Some(s) = expr.evaluate() {
                                                return Some(McPinPort::Single(s));
                                            } else {
                                                return None;
                                            }
                                        }
                                    }
                                } else {
                                    None
                                }
                            }

                            // Handle arithmetic expressions: + - * /
                            MCAST_OPD_MULTI | MCAST_OPD_DIVID | MCAST_OPD_PLUS
                            | MCAST_OPD_MINUS => {
                                if let Some(expr) = McExpression::new(&exp_node) {
                                    // Check for parameter references - if found, skip static parsing
                                    if dynamic::DynamicPinExpr::check_param_ref(&expr) {
                                        return None;
                                    }

                                    // For arithmetic expressions, try to evaluate to a single value
                                    expr.evaluate().map(McPinPort::Single)
                                } else {
                                    None
                                }
                            }

                            _ => {
                                dlog_error(1205, &exp_node, TYPE_MISMATCH);
                                None
                            }
                        }
                    } else {
                        None
                    }
                }

                _ => {
                    dlog_error(1204, &pid_node, TYPE_MISMATCH);
                    None
                }
            }
        } else {
            dlog_error(1204, node, TYPE_MISMATCH);
            None
        }
    }

    /// Insert values into the pool, returning an Arc reference
    fn insert_values(&mut self, values: &[McAttrVal]) -> Arc<Vec<McAttrVal>> {
        let arc: Arc<Vec<McAttrVal>> = values.to_vec().into();
        self.values_pool.push(arc.clone());
        arc
    }

    fn register_pin(
        &mut self,
        iotype: IOType,
        pinid: &String,
        names: &[String],
        values: &[McAttrVal],
    ) {
        let values_arc = self.insert_values(values);
        // If pinid already exists, append names instead of overwriting
        if let Some(existing) = self.pins.get_mut(pinid) {
            for name in names {
                if !existing.names.contains(name) {
                    existing.names.push(name.clone());
                }
            }
        } else {
            self.pins.insert(
                pinid.to_string(),
                McPin {
                    iotype,
                    id: pinid.to_string(),
                    names: names.to_vec(),
                    values: values_arc,
                },
            );
        }

        // Allow lookup by pin "name" to a pin-id.
        for name in names {
            // update pin_id_to_names: pin -> names mapping
            self.pin_id_to_names
                .entry(pinid.clone())
                .or_default()
                .push(name.clone());

            match self.names_to_id.get_mut(name) {
                Some(existing) => {
                    // Name already exists, convert Single to Multi or push to existing Multi
                    match existing {
                        McPinPort::Single(old_pid) => {
                            // Convert Single to Multi
                            *existing = McPinPort::Multi(vec![old_pid.clone(), pinid.clone()]);
                        }
                        McPinPort::Multi(pids) => {
                            // Push to existing Multi
                            pids.push(pinid.clone());
                        }
                        _ => {
                            // For other types (Bus, Interface, etc.), replace or skip
                            *existing = McPinPort::Single(pinid.clone());
                        }
                    }
                }
                None => {
                    // Name doesn't exist, insert new Single
                    self.names_to_id
                        .insert(name.clone(), McPinPort::Single(pinid.clone()));
                }
            }

            // Handle qualified names like DC2.VDD - check if base name (DC2) exists
            // If so, merge into Multi; if not, create base entry
            if let Some(dot_pos) = name.find('.') {
                let base_name = &name[..dot_pos];
                self.merge_into_base(pinid, base_name);
            }
        }
    }

    fn merge_into_base(&mut self, pinid: &String, base_name: &str) {
        match self.names_to_id.get_mut(base_name) {
            Some(existing) => {
                // Base exists, merge this pin into it
                match existing {
                    McPinPort::Single(_old_pid) => {
                        // Don't convert to Multi, leave as Single for now
                        // The Bus creation logic will handle converting to Bus
                    }
                    McPinPort::Multi(pids) => {
                        // Don't convert to Single, leave as Multi
                        if !pids.contains(pinid) {
                            pids.push(pinid.clone());
                        }
                    }
                    McPinPort::Bus(_) => {
                        // Bus already exists, don't modify
                    }
                    _ => {
                        // For other types, don't modify
                    }
                }
            }
            None => {
                // Base doesn't exist, create new Single
                self.names_to_id
                    .insert(base_name.to_string(), McPinPort::Single(pinid.clone()));
            }
        }
    }

    /// Extract common base name from a list of names like ["GPIO1", "GPIO2"]
    /// Returns Some("GPIO") if all names share the same base, None otherwise
    fn extract_common_base(&self, names: &[String]) -> Option<String> {
        if names.is_empty() {
            return None;
        }
        let first_name = &names[0];
        // Find the last non-digit character position (end of base prefix)
        let base_end = first_name.rfind(|c: char| !c.is_ascii_digit())? + 1;
        let base = &first_name[..base_end];
        // Check if all names start with this base
        for name in names.iter().skip(1) {
            if !name.starts_with(base) {
                return None;
            }
        }
        Some(base.to_string())
    }

    pub(crate) fn find_pin(&self, id: &str) -> Option<String> {
        //1.num 2.id 3.bus 4.spi.clk
        if self.names_to_id.contains_key(id) {
            Some(id.to_string())
        } else {
            None
        }
    }

    pub fn get_all_pins(&self) -> BTreeSet<String> {
        self.pins.keys().cloned().collect()
    }

    pub fn count(&self) -> usize {
        self.pins.len()
    }

    pub fn get_pins_by_io(&self, io_type: &IOType) -> Vec<String> {
        // IOType doesn't implement `PartialEq`, so compare by discriminant.
        self.pins
            .iter()
            .filter_map(|(pin_id, rec)| {
                if std::mem::discriminant(&rec.iotype) == std::mem::discriminant(io_type) {
                    Some(pin_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get single pin's IO type by pin_id (Step 1)
    ///
    /// Find from `self.pins: BTreeMap<String, McPin>`.
    /// Return None if not found.
    pub fn get_pin_io(&self, pin_id: &str) -> Option<IOType> {
        self.pins.get(pin_id).map(|pin| pin.iotype.clone())
    }
}

// ============================================================================
// Display implementation - concise format output (sort by pin ID, merge same-pin multiple function names)
// ============================================================================

impl std::fmt::Display for McPins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // show dynamic pin info (if any)
        if !self.dynamic_pins.is_empty() {
            for dyn_pin in &self.dynamic_pins {
                writeln!(f, "  {dyn_pin}")?;
            }
        }

        if self.pin_id_to_names.is_empty() {
            // compatible with old logic
            let mut entries: Vec<(&String, &McPinPort)> = self.names_to_id.iter().collect();
            entries.sort_by(|a, b| {
                let id_a = match a.1 {
                    McPinPort::Single(pid) => pid.clone(),
                    McPinPort::Multi(pids) => pids.first().cloned().unwrap_or_default(),
                    McPinPort::NC => "0".to_string(),
                    _ => a.0.clone(),
                };
                let id_b = match b.1 {
                    McPinPort::Single(pid) => pid.clone(),
                    McPinPort::Multi(pids) => pids.first().cloned().unwrap_or_default(),
                    McPinPort::NC => "0".to_string(),
                    _ => b.0.clone(),
                };
                let num_a: i64 = id_a.parse().unwrap_or(0);
                let num_b: i64 = id_b.parse().unwrap_or(0);
                num_a.cmp(&num_b)
            });

            for (name, pin) in entries {
                match pin {
                    McPinPort::Single(pid) => {
                        writeln!(f, "  {pid} = {name}")?;
                    }
                    McPinPort::Multi(pids) => {
                        writeln!(f, "  {} = {}", pids.join(","), name)?;
                    }
                    McPinPort::NC => {
                        writeln!(f, "  NC = {name}")?;
                    }
                    _ => {
                        writeln!(f, "  {name:?} = {pin:?}")?;
                    }
                }
            }
        } else {
            // use pin_id_to_names to merge display multiple function names of same pin
            // first output items with pin ID
            let mut pin_ids: Vec<&String> = self.pin_id_to_names.keys().collect();
            pin_ids.sort_by(|a, b| {
                let num_a: i64 = a.parse().unwrap_or(0);
                let num_b: i64 = b.parse().unwrap_or(0);
                num_a.cmp(&num_b)
            });

            for pin_id in pin_ids {
                if let Some(names) = self.pin_id_to_names.get(pin_id) {
                    let names_str = names
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    writeln!(f, "  {pin_id:<6} = {names_str}")?;
                }
            }

            // then output Interface, Bus etc without pin ID
            let mut other_entries: Vec<(&String, &McPinPort)> = self
                .names_to_id
                .iter()
                .filter(|(_, port)| {
                    matches!(
                        port,
                        McPinPort::Interface(_)
                            | McPinPort::Bus(_)
                            | McPinPort::Multi(_)
                            | McPinPort::List(..)
                    )
                })
                .collect();
            other_entries.sort_by(|a, b| a.0.cmp(b.0));

            // Output each Interface separately
            for (name, port) in &other_entries {
                if let McPinPort::Interface(iface) = port {
                    let ifc_name = iface.name.to_string();
                    let base_name = iface.base.name.to_string();
                    let _name_str = if name.starts_with('[') && name.ends_with(']') {
                        let inner = &name[1..name.len() - 1];
                        format!("{{{}}}", inner.replace(", ", ","))
                    } else {
                        name.to_string()
                    };
                    let registered_str = if iface.registered_pins.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", iface.registered_pins.join(","))
                    };
                    writeln!(f, "Interface: {ifc_name}{registered_str} :: {base_name}")?;
                }
            }

            // Output other types of items (Bus, Multi, List, etc.)
            for (name, port) in &other_entries {
                match port {
                    McPinPort::Interface(_) => {} // Already processed
                    McPinPort::Bus(bus) => {
                        let members_str = if bus.member.is_empty() {
                            String::new()
                        } else {
                            format!("{{{}}}", bus.member.join(", "))
                        };
                        writeln!(f, "Bus: {}{} -> {}", name, members_str, bus.name)?;
                    }
                    McPinPort::Multi(pids) => {
                        writeln!(f, "Multi: {} [{}]", name, pids.join(",").replace(",", ", "))?;
                    }
                    McPinPort::List(list_name, members) => {
                        writeln!(
                            f,
                            "List: {} {{{}}} -> {}",
                            list_name,
                            members.join(", "),
                            name
                        )?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct McPinNames {
    pub options: Vec<McPinPort>,
    pub has_param_ref: bool,
}

impl McPinNames {
    pub fn has_param_ref(&self) -> bool {
        if self.has_param_ref {
            return true;
        }

        for opt in &self.options {
            if let McPinPort::Single(name) = opt {
                let ids = McIds::from(name.as_str());
                // Use McIds::has_param_ref to check whether it contains parameter references
                // This checks whether there are non-numeric parameters (e.g. rows, cols) in the square bracket range
                if ids.has_param_ref() {
                    return true;
                }
            }
        }
        false
    }

    pub fn new(node: &AstNode) -> Option<Self> {
        // MCAST_PIN_NAMES
        //  |- MCAST_PIN_NAME *

        if node.get_type() != MCAST_PIN_NAMES {
            return None;
        }
        let mut cur = node.get_sub_node();
        while let Some(c) = cur {
            cur = c.get_next();
        }
        let Some(name_nodes) = node.get_sub_node() else {
            return None;
        };

        let mut myself = Self {
            options: Vec::new(),
            has_param_ref: false,
        };

        for each_name_option in
            name_nodes.iter().filter(|n| n.get_type() == MCAST_PIN_NAME)
        {
            // mc_pins_name:
            // | mc_nc
            // | mc_int
            // | mc_opd
            // | mc_phrase

            let Some(sub_node) = each_name_option.get_sub_node() else {
                continue;
            };
            match sub_node.get_type() {
                MCAST_OPD_NC => {
                    myself.options.push(McPinPort::NC);
                }

                MCAST_INT => {
                    if let Some(pname) = McInt::new(&sub_node) {
                        myself.options.push(McPinPort::Single(pname.to_string()));
                    }
                }

                MCAST_OPD => {
                    let Some(opd_node) = sub_node.get_sub_node() else {
                        continue;
                    };
                    match opd_node.get_type() {
                        MCAST_IDS => {
                            if let Some(pname) = McIds::new(&opd_node) {
                                // check if contains parameter reference (e.g. R[1:rows]C[1:cols])
                                // if contains parameter reference, mark as dynamic pin, handle by dynamic logic
                                if pname.has_param_ref() {
                                    myself.has_param_ref = true;
                                    continue;
                                }

                                if let Some((busname, members)) = pname.as_bus() {
                                    // DC1{VDD, GND} -> Bus (curly form)
                                    myself.options.push(McPinPort::Bus(McBus::new_with_members(
                                        &busname, members,
                                    )));
                                } else if pname.is_list() {
                                    // PDM[CLK, DATA] -> List (square bracket form)
                                    // use list_members() to get members
                                    let full_name = pname.to_string();
                                    if let Some(members) = pname.list_members() {
                                        myself.options.push(McPinPort::List(full_name, members));
                                    }
                                } else if pname.segments.len() == 2
                                    && matches!(pname.segments[0], IdsSegment::Ida(_))
                                    && matches!(pname.segments[1], IdsSegment::Ida(_))
                                {
                                    // INST::CLASS syntax (no brackets, e.g. `I0::I2C`)
                                    // segments[0] = inst name, segments[1] = class name.
                                    // pname.expand() in this case returns ["I0I2C"] (length 1, Cartesian product
                                    // without separator), upper layer treats as Multi(["I0I2C"]), all register branches
                                    // all require Multi length >= 2 → pin_count: 0.
                                    // fix: here recognize INST::CLASS pattern, route to mcb_get_cmie.
                                    let class_ida = match &pname.segments[1] {
                                        IdsSegment::Ida(ida) => ida.clone(),
                                        _ => unreachable!(),
                                    };
                                    let inst_ida = match &pname.segments[0] {
                                        IdsSegment::Ida(ida) => ida.clone(),
                                        _ => unreachable!(),
                                    };
                                    let class_id = McIds {
                                        segments: vec![IdsSegment::Ida(class_ida)],
                                    };
                                    let inst_id = McIds {
                                        segments: vec![IdsSegment::Ida(inst_ida)],
                                    };
                                    let lookup_uri =
                                        crate::current_uri::try_get().unwrap_or_default();
                                    if let Some(McCMIE::Interface(iface_def)) =
                                        mcb_get_cmie(&class_id, &lookup_uri)
                                    {
                                        let mc2_iface = Mc2Interface::new(inst_id, iface_def);
                                        myself
                                            .options
                                            .push(McPinPort::Interface(Arc::new(mc2_iface)));
                                    } else {
                                        // W1304: same as DECLARE_UV branch
                                        let class_str = class_id.to_string();
                                        let inst_str = inst_id.to_string();
                                        let is_likely_alias =
                                            class_str.starts_with('_') || class_str == inst_str;
                                        if !is_likely_alias {
                                            dlog_warning(
                                                1304,
                                                &opd_node,
                                                &format!(
                                                    "'{inst_str}::{class_str}' lookup failed; \
                                                     treating '{inst_str}' as plain pin alias. \
                                                     If you intended an interface binding, check \
                                                     that '{class_str}' is defined (and `use`d, \
                                                     if from a library).",
                                                ),
                                            );
                                        }
                                        myself.options.push(McPinPort::Single(inst_str));
                                    }
                                } else {
                                    match pname.count() {
                                        1 => myself
                                            .options
                                            .push(McPinPort::Single(pname.to_string())),
                                        2.. => {
                                            myself.options.push(McPinPort::Multi(pname.expand()))
                                        }
                                        _ => {
                                            dlog_error(1203, &opd_node, "Pin name count error");
                                        }
                                    }
                                }
                            }
                        }
                        // OPD_GROUP: `A | B | ...` is function reuse / alias syntax,
                        // each side of pipe is "one usage of this pin group" (e.g.
                        // `I2C0::I2C(Master) | GPIO[3,4]::GPIO()`, or single-pin alias `_WP | IO2`),
                        // each side should **occupy entire pinid group**, not by position.
                        //
                        // old code packs both sides into single Multi, falls into pin line "n pins vs n names" branch as 1:1
                        // positional zip (I2C0→pin1, GPIO[3,4]→pin2), compresses 2-wire interface to 1 pin →
                        // find_bus_port_pin_ids not found → `uC.i2c(...).I2C0` degrades to default right pin
                        // (GND), i.e. I2C0 net mistakenly merged with uC.21.
                        //
                        // change to push each name as Single option: pin line lets each name register entire group
                        // pinids register —— single pin: register_pin accumulates "same-pin multi-alias", multi-pin:
                        // accumulates Multi[entire group] (each function occupies whole group). Position list `[VDD,GND]`
                        // goes MCAST_OPD_SQUARE_VEC, not this branch.
                        MCAST_OPD_GROUP => {
                            let mut cur = opd_node.get_sub_node();
                            while let Some(child) = cur {
                                // handle _CS, SO, SI, SCLK alias forms
                                // or SO | IO1 embedded OPD_GROUP (each member is independent alias)
                                match child.get_type() {
                                    MCAST_IDS => {
                                        if let Some(ids) = McIds::new(&child) {
                                            myself.options.push(McPinPort::Single(ids.to_string()));
                                        }
                                    }
                                    MCAST_OPD_GROUP => {
                                        // Nested: SO | IO1 — split each member as an independent alias
                                        let mut sub_cur = child.get_sub_node();
                                        while let Some(sub) = sub_cur {
                                            if sub.get_type() == MCAST_IDS {
                                                if let Some(ids) = McIds::new(&sub) {
                                                    myself
                                                        .options
                                                        .push(McPinPort::Single(ids.to_string()));
                                                }
                                            }
                                            sub_cur = sub.get_next();
                                        }
                                    }
                                    _ => {}
                                }
                                cur = child.get_next();
                            }
                        }
                        _ => {
                            dlog_error(1201, &opd_node, "Pin name not support type");
                        }
                    }
                }

                MCAST_EXPRESSION => {
                    let Some(exp_node) = sub_node.get_sub_node() else {
                        continue;
                    };
                    match exp_node.get_type() {
                        MCAST_OPD_COLON => {
                            // use McExpression to get range, expand range to Vec<String>
                            if let Some(expr) = McExpression::new(&exp_node) {
                                // check if contains parameter reference, skip static parsing if so
                                if dynamic::DynamicPinExpr::check_param_ref(&expr) {
                                    myself.has_param_ref = true;
                                    continue;
                                }

                                let pname = expr.expand();
                                match pname.len() {
                                    1 => {
                                        myself.options.push(McPinPort::Single(pname[0].clone()))
                                    }
                                    2.. => myself.options.push(McPinPort::Multi(pname)),
                                    _ => {
                                        dlog_error(1203, &exp_node, "Pin name count error");
                                    }
                                }
                            }
                        }
                        MCAST_OPD_SQUARE_VEC => {
                            // Expand `[VDD, GND]` -> Multi(["VDD","GND"])
                            if let Some(expr) = McExpression::new(&exp_node) {
                                if let McExpression::Set(items) = expr {
                                    let mut out = Vec::<String>::new();
                                    for item in items {
                                        if let Some(s) = item.evaluate() {
                                            out.push(s);
                                        }
                                    }
                                    if out.len() == 1 {
                                        myself.options.push(McPinPort::Single(out[0].clone()));
                                    } else if !out.is_empty() {
                                        myself.options.push(McPinPort::Multi(out));
                                    }
                                }
                            }
                        }
                        // Handle arithmetic expressions in pin names: + - * /
                        MCAST_OPD_MULTI | MCAST_OPD_DIVID | MCAST_OPD_PLUS | MCAST_OPD_MINUS => {
                            if let Some(expr) = McExpression::new(&exp_node) {
                                // Check for parameter references
                                if dynamic::DynamicPinExpr::check_param_ref(&expr) {
                                    myself.has_param_ref = true;
                                    continue;
                                }

                                // For arithmetic expressions in pin names, try to evaluate
                                if let Some(s) = expr.evaluate() {
                                    myself.options.push(McPinPort::Single(s));
                                }
                            }
                        }
                        MCAST_DECLARE | MCAST_DECLARE_UV => {
                            // Parse MCAST_DECLARE directly to get class and instance names
                            // MCAST_DECLARE: inst_name::class_name() (no parameters)
                            // MCAST_DECLARE_UV: inst_name::class_name(params) (with parameters like Master)
                            //
                            // Also handles pin name aliases: `_CS | CS` is parsed as
                            // MCAST_DECLARE with class=_CS, instance=CS
                            let mut class_name: Option<McIds> = None;
                            let mut inst_ids: Option<McIds> = None;
                            let mut params: Vec<McParamValue> = Vec::new();

                            // Iterate over linked list structure
                            let mut current = exp_node.get_sub_node();
                            while let Some(node) = current {
                                match node.get_type() {
                                    MCAST_CLASS => {
                                        // Get class name from first sub-node (which should be MCAST_IDS)
                                        // Structure: MCAST_IDS contains the full class name (e.g., "UART.TTL")
                                        // Then traverse linked list inside CLASS to find MCAST_PARAMS
                                        if let Some(class_id_node) = node.get_sub_node() {
                                            class_name = McIds::new(&class_id_node);
                                        }

                                        // Also traverse the linked list from MCAST_CLASS to find params
                                        // Structure: CLASS -> IDS (subnode) -> next -> PARAMS -> ...
                                        let mut class_child = node.get_sub_node();
                                        while let Some(cc) = class_child {
                                            if cc.get_type() == MCAST_PARAMS {
                                                // MCAST_PARAMS -> MCAST_PARAM -> actual value
                                                if let Some(param_node) = cc.get_sub_node() {
                                                    if param_node.get_type() == MCAST_PARAM {
                                                        // McIds::new handles MCAST_PARAM by recursing into its sub-node
                                                        if let Some(ids) = McIds::new(&param_node) {
                                                            if !ids.is_empty() {
                                                                params.push(McParamValue::Ids(ids));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            class_child = cc.get_next();
                                        }
                                    }
                                    MCAST_INSTANCE => {
                                        if let Some(inst_id_node) = node.get_sub_node() {
                                            inst_ids = McIds::new(&inst_id_node);
                                        }
                                    }
                                    _ => {}
                                }

                                current = node.get_next();
                            }

                            let (Some(ref cn), Some(ref iname)) = (&class_name, &inst_ids) else {
                                continue;
                            };

                            let class_name = cn.clone();
                            let inst_name = iname.clone();

                            let lookup_uri = crate::current_uri::try_get().unwrap_or_default();
                            let lookup_result = mcb_get_cmie(&class_name, &lookup_uri);
                            if let Some(McCMIE::Interface(iface_def)) = lookup_result {
                                // Pass params to Mc2Interface (e.g., role parameter "DCE")
                                let mc2_iface =
                                    Mc2Interface::with_ids_and_params(inst_name, iface_def, params);
                                myself
                                    .options
                                    .push(McPinPort::Interface(Arc::new(mc2_iface)));
                            } else {
                                // 1304: mcb_get_cmie did not find the interface corresponding to class_name.
                                //      Still fall back to Single alias (compatible with alias mode like `_CS | CS`),
                                //      but emit a dlog_warning to inform the user that this is not a real interface
                                //      binding — if interface was intended, the class name is likely misspelled or not `use`d.
                                let class_str = class_name.to_string();
                                let inst_str = inst_name.to_string();
                                let is_likely_alias =
                                    class_str.starts_with('_') || class_str == inst_str;
                                if !is_likely_alias {
                                    dlog_warning(
                                        1304,
                                        &exp_node,
                                        &format!(
                                            "'{inst_str}::{class_str}(...)' lookup failed; \
                                             treating '{inst_str}' as plain pin alias. \
                                             If you intended an interface binding, check that \
                                             '{class_str}' is defined (and `use`d, if from a library).",
                                        ),
                                    );
                                }
                                myself
                                    .options
                                    .push(McPinPort::Single(inst_name.to_string()));
                            }
                        }
                        MCAST_OPD_FCALL => {
                            // Handle function call syntax like I2C(Master)
                            // This is used for parameterized interfaces like I2C0::I2C(Master)
                            // MCAST_OPD_FCALL structure: inst_name::CLASS(params)
                            let mut class_name: Option<McIds> = None;
                            let mut inst_ids: Option<McIds> = None;

                            // Iterate over linked list structure
                            let mut current = exp_node.get_sub_node();
                            while let Some(node) = current {
                                match node.get_type() {
                                    MCAST_CLASS => {
                                        // Get class name from first sub-node (which should be MCAST_IDS)
                                        if let Some(class_id_node) = node.get_sub_node() {
                                            class_name = McIds::new(&class_id_node);
                                        }
                                    }
                                    MCAST_INSTANCE => {
                                        if let Some(inst_id_node) = node.get_sub_node() {
                                            inst_ids = McIds::new(&inst_id_node);
                                        }
                                    }
                                    _ => {}
                                }

                                current = node.get_next();
                            }

                            let (Some(ref cn), Some(ref iname)) = (&class_name, &inst_ids) else {
                                continue;
                            };

                            let class_name = cn.clone();
                            let inst_name = iname.clone();

                            let lookup_uri = crate::current_uri::try_get().unwrap_or_default();
                            if let Some(McCMIE::Interface(iface_def)) =
                                mcb_get_cmie(&class_name, &lookup_uri)
                            {
                                let mc2_iface = Mc2Interface::new(inst_name, iface_def);
                                myself
                                    .options
                                    .push(McPinPort::Interface(Arc::new(mc2_iface)));
                            } else {
                                dlog_error(
                                    1707,
                                    &exp_node,
                                    &format!(
                                        "Interface '{class_name}' not found (looked up from '{lookup_uri}'); \
                                         check that it is defined and imported via `use`.",
                                    ),
                                );
                            }
                        }
                        _ => {
                            dlog_error(1204, &sub_node, TYPE_MISMATCH);
                        }
                    }
                }

                MCAST_OPD_USCORE | _ => {
                    dlog_error(1201, &sub_node, "Pin name not support type");
                }
            }
        }

        // only for **single PIN_NAME node** (square bracket position list `[A, B, ...]`) multiple Single does
        // merge → positional Multi, 1:1 zip with pins.
        //
        // top-level `|` or operation parses as **multiple PIN_NAME nodes** (name_nodes > 1), each is one
        // **candidate function** (e.g. `I2C0::I2C(Master) | GPIO[3,4]::GPIO()`), each should occupy entire group
        // pins —— never merge into positional Multi, otherwise zipped as "one pin each" (I2C0→pin1,
        // GPIO[3,4]→pin2), 2-wire interface compressed to 1 pin → find_bus_port_pin_ids not found →
        // `uC.i2c(...).I2C0` degrades to default pin (GND), i.e. I2C0 net mistakenly merges uC.21 (#4).
        let name_node_count = name_nodes
            .iter()
            .filter(|n| n.get_type() == MCAST_PIN_NAME)
            .count();
        if name_node_count == 1 && myself.options.len() > 1 {
            let all_singles: Vec<String> = myself
                .options
                .iter()
                .filter_map(|p| {
                    if let McPinPort::Single(name) = p {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .collect();

            if all_singles.len() > 1 {
                // Replace all Single entries with one Multi entry
                myself
                    .options
                    .retain(|p| !matches!(p, McPinPort::Single(_)));
                myself.options.push(McPinPort::Multi(all_singles));
            }
        }

        Some(myself)
    }
}

// ============================================================================
// Interface subname derivation —— extracted from `parse()` Interface branch, for unit testing
// ============================================================================

/// Determine pin subname list based on interface instance name `inst_name` form.
///
/// Three AST forms need separate handling, can't use one-size-fits-all:
///
/// | Source syntax                | `inst_name` form | Expected subname      |
/// |---------------------------|-----------------|-------------------|
/// | `DC1::DC()`               | plain Ida       | `DC1.<iface_pin>` Cartesian (expanded per interface prototype) |
/// | `[VDD,GND]::DC()`         | list (Square)   | `["VDD","GND"]` (user gave members explicitly, as-is) |
/// | `DC2{VDD,GND}::DC()`      | bus (Curly)     | `["DC2.VDD","DC2.GND"]` (bus name + members) |
/// | `XTAL{X1,X2}::XTAL()`     | bus (Curly)     | `["XTAL.X1","XTAL.X2"]` |
///
/// Previously `parse()` uniformly used `expanded × iface_pins` Cartesian for all three forms, causing
/// bus form concatenated to `XTAL.X1.X1 / XTAL.X1.X2 / XTAL.X2.X1 / XTAL.X2.X2`,
/// list form concatenated to `VDD.VDD / VDD.GND / ...`, all wrong.
pub(crate) fn derive_interface_subnames(inst_name: &McIds, iface_pins: &[String]) -> Vec<String> {
    if inst_name.is_bus() {
        if let Some((busname, members)) = inst_name.as_bus() {
            return members.iter().map(|m| format!("{busname}.{m}")).collect();
        }
    }
    if inst_name.is_list() {
        if let Some(members) = inst_name.list_members() {
            return members;
        }
    }

    // plain: instance name × interface default pin name
    let expanded = inst_name.expand();
    expanded
        .iter()
        .flat_map(|inst| {
            iface_pins
                .iter()
                .map(|p| format!("{inst}.{p}"))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod subname_tests {
    use super::*;
    use crate::core::basic::mc_ida::{IdaSegment, McIda};
    use crate::core::basic::mc_ids::IdsSegment;

    /// Construct a plain Ida (like `DC1`)
    fn ida(name: &str) -> McIda {
        McIda {
            segments: vec![IdaSegment::Id(name.to_string())],
        }
    }

    /// Construct `PREFIX{m1, m2, ...}` form McIds (bus form, corresponds to Curly segment)
    fn bus_ids(prefix: &str, members: &[&str]) -> McIds {
        McIds {
            segments: vec![
                IdsSegment::Ida(Box::new(ida(prefix))),
                IdsSegment::Curly(
                    members
                        .iter()
                        .map(|m| IdsSegment::Ida(Box::new(ida(m))))
                        .collect(),
                ),
            ],
        }
    }

    /// Construct `[m1, m2, ...]` form McIds (list form, corresponds to Square segment)
    /// Note: McIds::is_list() requires at least 2 segments (Ida prefix + Square),
    /// so here give `PREFIX[m1, m2]` form, then take prefix into members.
    fn list_ids(prefix: &str, members: &[&str]) -> McIds {
        McIds {
            segments: vec![
                IdsSegment::Ida(Box::new(ida(prefix))),
                IdsSegment::Square(
                    members
                        .iter()
                        .map(|m| IdsSegment::Ida(Box::new(ida(m))))
                        .collect(),
                ),
            ],
        }
    }

    /// Construct plain `NAME` form
    fn plain_ids(name: &str) -> McIds {
        McIds {
            segments: vec![IdsSegment::Ida(Box::new(ida(name)))],
        }
    }

    /// User-reported original: `XTAL{X1,X2}::XTAL()`
    ///
    /// Before regression (Cartesian) produced `["XTAL.X1.X1","XTAL.X1.X2","XTAL.X2.X1","XTAL.X2.X2"],
    /// first pin was registered as "XTAL.X1.X1", completely wrong.
    #[test]
    fn bus_form_xtal_regression() {
        let inst = bus_ids("XTAL", &["X1", "X2"]);
        let iface_pins = vec!["X1".to_string(), "X2".to_string()];
        let got = derive_interface_subnames(&inst, &iface_pins);
        assert_eq!(got, vec!["XTAL.X1", "XTAL.X2"]);
    }

    /// case10: `DC2{VDD,GND}::DC()`
    #[test]
    fn bus_form_dc2() {
        let inst = bus_ids("DC2", &["VDD", "GND"]);
        let iface_pins = vec!["VDD".to_string(), "GND".to_string()];
        let got = derive_interface_subnames(&inst, &iface_pins);
        assert_eq!(got, vec!["DC2.VDD", "DC2.GND"]);
    }

    /// case9: `[VDD, GND]::DC()` —— square bracket form, no instance prefix
    #[test]
    fn list_form_no_prefix() {
        let inst = list_ids("_", &["VDD", "GND"]);
        let iface_pins = vec!["VDD".to_string(), "GND".to_string()];
        let got = derive_interface_subnames(&inst, &iface_pins);
        assert_eq!(got, vec!["VDD", "GND"]);
    }

    /// case8: `DC1::DC()` —— plain form, expand per interface prototype
    #[test]
    fn plain_form_crosses_with_iface_pins() {
        let inst = plain_ids("DC1");
        let iface_pins = vec!["VDD".to_string(), "GND".to_string()];
        let got = derive_interface_subnames(&inst, &iface_pins);
        assert_eq!(got, vec!["DC1.VDD", "DC1.GND"]);
    }

    /// Edge case: plain name + empty interface pin list — expect empty list, not panic.
    #[test]
    fn plain_form_empty_iface_pins() {
        let inst = plain_ids("DC1");
        let got = derive_interface_subnames(&inst, &[]);
        assert!(got.is_empty());
    }
}
