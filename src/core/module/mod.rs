// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::{
    basic::mc_bus::{McBus, McList},
    basic::mc_endpoint::{McEndpoint, McInstanceRef},
    basic::mc_phrase::McPhrase,
    mc_func::{HasFindInst, McFunctions},
    mc_inst::{McInst, McInstance, McInstances},
};
use crate::builder::mcb_get_cmie;
use crate::core::component::Mc2Component;
use crate::core::mc_func::McFuncReturn;
use crate::{
    ast::{ast_node::AstNode, c_macros::*, error::message::*},
    builder::diagnostic::dlog_error,
    core::basic::mc_param::McParamDeclares,
    IOType, McCMIE, McIds, McParamValue, McURI,
};
use std::sync::Arc;
// ============================================================================
// McModule - Module definition
// ============================================================================

#[derive(Debug, Clone)]
pub struct McModule {
    pub name: McIds,
    pub params: McParamDeclares,
    pub insts: McInstances,
    pub lines: Vec<McPhrase>,
    pub funcs: McFunctions,
    pub uri: McURI,
    anon_counter: usize,
}

impl McModule {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_MODULE
        // |- MCAST_NAME - MCAST_PARAM (option) - MCAST_BODY
        if let Some(subnodes) = node.get_sub_node() {
            let module_name = subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))
                .and_then(|n| n.get_sub_node())
                .and_then(|n| McIds::new(&n));

            let body = subnodes
                .iter()
                .find(|x| x.is_type(MCAST_BODY))
                .expect(MISSING_SUBNODE);

            let module_name = module_name?;

            let mut module = Self {
                name: module_name,
                params: McParamDeclares::new(),
                funcs: McFunctions::new(),
                insts: McInstances::new(),
                lines: Vec::new(),
                uri: uri.clone(),
                anon_counter: 1,
            };

            // 2. Parse parameters
            if let Some(param_node) = subnodes.iter().find(|x| x.is_type(MCAST_PARAMS)) {
                module.parse_params(&param_node);
            }

            // 3. Parse body
            module.parse_body(&body);

            Some(module)
        } else {
            dlog_error(804, node, MISSING_SUBNODE);
            None
        }
    }

    pub(crate) fn parse_params(&mut self, decl_node: &AstNode) {
        // Parameters divided into 2 categories: data and inst, each parsed separately
        // MCAST_PARAMS
        //   |- MCAST_PARAM
        //      |- MCAST_ROLE               : parse as params: McParamDeclares
        //      |- MCAST_IDS                : parse as params: McParamDeclares
        //      |- MCAST_SQUARE_VEC         : parse as params: McParamDeclares
        //      |- MCAST_DECLARE_UV         : parse as params: McParamDeclares

        //      |- MCAST_OPD                : parse as insts: McInstances
        //      |- MCAST_OPD_SQUARE_VEC     : parse as insts: McInstances
        //      |- MCAST_DECLARE            : parse as insts: McInstances

        if let Some(subnodes) = decl_node.get_sub_node() {
            for param_node in subnodes.iter() {
                // Each MCAST_PARAM child node determines its type
                let Some(subnode) = param_node.get_sub_node() else {
                    continue;
                };

                match subnode.get_type() {
                    // Data parameter -> params
                    MCAST_ROLE | MCAST_IDS | MCAST_SQUARE_VEC | MCAST_DECLARE_UV => {
                        self.params.parse(&param_node);
                    }
                    // Reference parameter -> insts (treated as port)
                    MCAST_OPD | MCAST_OPD_SQUARE_VEC => {
                        self.insts.parse(&subnode, &self.uri);
                    }
                    // Instance parameter -> insts
                    MCAST_DECLARE => {
                        self.insts.parse(&subnode, &self.uri);
                    }
                    // IOTYPE-prefix parameter -> insts (e.g. ps dc24v, in GPIO[1:2])
                    // Need to pass complete param_node, let McInstances::parse handle IOTYPE and operands
                    MCAST_IOTYPE => {
                        self.insts.parse(&param_node, &self.uri);
                    }
                    _ => {
                        // Unknown type, try to parse as data parameter
                        dlog_error(803, &subnode, "Unexpected type in module param");
                    }
                }
            }
        }
    }

    pub(crate) fn parse_body(&mut self, body: &AstNode) {
        // ★ LSP: Set scope for instance registration
        self.insts.scope = Some(self.name.to_string());
        if let Some(clauses) = body.get_sub_node() {
            for clause in clauses.iter() {
                match clause.get_type() {
                    MCAST_NET_PORTS => {
                        self.insts.parse(&clause, &self.uri);
                    }

                    MCAST_NET => {
                        if let Some(subnode) = clause.get_sub_node() {
                            if subnode.get_type() == MCAST_DECLARE {
                                self.insts.parse(&subnode, &self.uri);
                                continue;
                            }
                            // Collect port reference spans before parsing the net
                            Self::collect_port_refs_in_node(
                                &subnode,
                                &mut self.insts,
                                &mut self.params,
                            );
                            match McPhrase::new(&subnode, self) {
                                Some(net) => {
                                    self.lines.push(net);
                                }
                                None => {
                                    dlog_error(1301, &clause, "connection line failed to parse");
                                }
                            }
                        } else {
                            dlog_error(1300, &clause, "Empty NET");
                        }
                    }

                    MCAST_FUNCTION => {
                        let context = unsafe { &mut *(self as *mut McModule) };
                        self.funcs.parse(&clause, context);
                    }

                    MCAST_DECLARE => {
                        self.insts.parse(&clause, &self.uri);
                    }

                    MCAST_ROLE => {
                        dlog_error(801, &clause, "Module does not support role definition.");
                    }
                    MCAST_ATTRIBUTE_PIN | MCAST_ATTRIBUTE_PINADD => {
                        dlog_error(
                            801,
                            &clause,
                            "Module does not support PINS directly. Use in/out/io declarations.",
                        );
                    }
                    _ => {
                        dlog_error(1402, &clause, "Unexpected clause type in module body");
                    }
                }
            }

            // ★ Smart Param (M5): Finalize after body parsed
            let mod_name = self.name.to_string();
            let diags = self.params.finalize(Some(body), &mod_name);
            for d in &diags {
                mcc::mcc_record_param_diag(&format!("[{:?}] {}", d.kind, d.message));
            }
        }
    }

    /// Find instance
    pub(crate) fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.insts.get(id).cloned()
    }

    /// Add label to symbol table
    /// If instance exists, return reference to existing instance
    /// If not found, check members in anonymous List/Bus
    pub(crate) fn add_label(&mut self, name: String) -> McPhrase {
        if let Some(existing_inst) = self.insts.get(&name) {
            return McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                existing_inst.clone(),
            )));
        }
        if let Some(member_ref) = self.find_member_in_anon_insts(&name) {
            return member_ref;
        }
        self.insts
            .create_inst(&name, McInstance::Label(name.clone()));
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
            name,
        ))))
    }

    /// Find member in anonymous List/Bus/Interface
    /// Anonymous instance: name starts with @, or [member1, member2] format (no total name)
    fn find_member_in_anon_insts(&self, member_name: &str) -> Option<McPhrase> {
        for (inst_name, inst) in self.insts.iter() {
            let is_anon = inst_name.starts_with('@')
                || (inst_name.starts_with('[') && inst_name.contains(','));
            if !is_anon {
                continue;
            }
            match inst {
                McInstance::List(list) => {
                    if list.member.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                McInstance::Bus(bus) => {
                    if bus.full_members.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                McInstance::Interface(iface) => {
                    if iface.base.pins.names_to_id.contains_key(member_name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                    let iface_members = iface.name.expand();
                    if iface_members.len() > 1 && iface_members.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Add component instance to symbol table
    pub(crate) fn add_component(&mut self, name: String, comp: Mc2Component) -> McPhrase {
        let inst = McInstance::Component(Arc::new(comp));
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Add module instance to symbol table
    pub(crate) fn add_module(&mut self, name: String, module: Mc2Module) -> McPhrase {
        let inst = McInstance::Module(Arc::new(module));
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Add bus to symbol table
    pub(crate) fn add_bus(&mut self, name: String, members: Vec<String>) -> McPhrase {
        let bus = McBus::new_with_members(&name, members);
        let inst = McInstance::Bus(bus);
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Add list to symbol table
    pub(crate) fn add_list(&mut self, name: String, members: Vec<String>) -> McPhrase {
        let list = McList::new_with_members(&name, members);
        let inst = McInstance::List(list);
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Get all input ports' McBus
    pub fn get_input_elements(&self) -> Vec<McBus> {
        self.insts
            .get_all_inputs()
            .iter()
            .map(|p| p.to_node_element())
            .collect()
    }

    /// Get all output ports' McBus
    pub fn get_output_elements(&self) -> Vec<McBus> {
        self.insts
            .get_all_outputs()
            .iter()
            .map(|p| p.to_node_element())
            .collect()
    }
}

impl HasFindInst for McModule {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.insts.get(id).cloned()
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance> {
        self.insts.get_mut(id)
    }

    fn add_label(&mut self, name: String) -> Option<McPhrase> {
        Some(self.add_label(name))
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let bus = McBus::new_with_members(&name, members);
        let inst = McInstance::Bus(bus);
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let list = McList::new_with_members(&name, members);
        let inst = McInstance::List(list);
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase> {
        let is_component_with_bus = self
            .insts
            .get(base)
            .map(|inst| {
                if let McInstance::Component(comp) = inst {
                    comp.base.pins.is_bus(&member)
                } else {
                    false
                }
            })
            .unwrap_or(false);

        if is_component_with_bus {
            let full_name = format!("{base}.{member}");
            if !self.insts.contains(&full_name) {
                let members = if let Some(inst) = self.insts.get(base) {
                    if let McInstance::Component(comp) = inst {
                        comp.base.pins.get_bus_members(&member).unwrap_or_default()
                    } else {
                        vec![member.clone()]
                    }
                } else {
                    vec![member.clone()]
                };
                let mut new_bus = McBus::new_with_members(&full_name, members);
                new_bus.add_member(&member);
                self.insts.create_inst(&full_name, McInstance::Bus(new_bus));
            } else if let Some(existing_inst) = self.insts.get_mut(&full_name) {
                if let McInstance::Bus(bus) = existing_inst {
                    if !bus.full_members.iter().any(|m| m == &member) {
                        bus.add_member(&member);
                    }
                }
            }
            let member_ref = McBus::member_ref(&full_name, member);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(member_ref),
            ))));
        }

        if let Some(inst) = self.insts.get_mut(base) {
            if let McInstance::Bus(bus) = inst {
                let fn_base = base.to_string();
                bus.add_member(&member);
                let full_members_clone = bus.full_members.clone();
                if !self.insts.contains(&fn_base) {
                    let bus_to_add = McBus::new_with_members(&fn_base, full_members_clone);
                    self.insts
                        .create_inst(&fn_base, McInstance::Bus(bus_to_add));
                }
                let member_ref = McBus::member_ref(&fn_base, member);
                return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::Bus(member_ref),
                ))));
            }
        }

        let bus = McBus::new_with_members(base, vec![member.clone()]);
        let inst = McInstance::Bus(bus);
        self.insts.create_inst(base, inst.clone());
        let member_ref = McBus::member_ref(base, member);
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Bus(member_ref),
        ))))
    }

    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase> {
        let full_name = format!("{component}.{interface}");
        if let Some(comp_inst) = self.insts.get(component) {
            if let McInstance::Component(comp) = comp_inst {
                if comp.base.pins.is_interface(interface) {
                    let iface_ref = McBus::new_with_members(&full_name, members);
                    return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(iface_ref),
                    ))));
                }
            }
        }
        if let Some(McCMIE::Interface(_)) = mcb_get_cmie(&McIds::from("ADC.DIFF"), self.uri()) {
            let iface_ref = McBus::new_with_members(&full_name, members);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(iface_ref),
            ))));
        }
        if let Some(McCMIE::Interface(_)) = mcb_get_cmie(
            &McIds::from(&format!("{component}.{interface}") as &str),
            self.uri(),
        ) {
            let iface_ref = McBus::new_with_members(&full_name, members);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(iface_ref),
            ))));
        }
        None
    }

    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)> {
        if let Some(inst) = self.insts.get(base) {
            if let McInstance::Component(comp) = inst {
                if comp.base.pins.is_bus(member) {
                    return Some((format!("{base}.{member}"), member.to_string()));
                }
            }
        }
        None
    }

    fn is_component_bus(&self, base: &str, member: &str) -> bool {
        if let Some(inst) = self.insts.get(base) {
            if let McInstance::Component(comp) = inst {
                return comp.base.pins.is_bus(member);
            }
        }
        false
    }

    fn uri(&self) -> &McURI {
        &self.uri
    }

    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        let before: Vec<String> = self.insts.get_all_names();
        self.insts.parse(node, &self.uri);
        // Collect newly created instances to return to callers (mc_phrase.rs, mc_fcall.rs)
        self.insts
            .get_all_names()
            .into_iter()
            .filter(|k| !before.contains(k))
            .filter_map(|k| self.insts.get(&k).cloned())
            .collect()
    }

    fn add_component(
        &mut self,
        name: String,
        comp: crate::core::component::Mc2Component,
    ) -> Option<McPhrase> {
        Some(self.add_component(name, comp))
    }

    fn add_module(
        &mut self,
        name: String,
        module: crate::core::module::Mc2Module,
    ) -> Option<McPhrase> {
        Some(self.add_module(name, module))
    }

    fn gen_anon_name(&mut self, classname: &str) -> String {
        let name = format!("@{}{}", classname, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn upgrade_label_to_bus(&mut self, name: &str) -> bool {
        if let Some(inst) = self.insts.get_mut(name) {
            if matches!(inst, McInstance::Label(_)) {
                let new_bus = McBus::new(name);
                *inst = McInstance::Bus(new_bus);
                return true;
            }
        }
        false
    }

    fn find_func_return(&self, name: &str) -> Option<McFuncReturn> {
        self.funcs.find(name).map(|f| f.returns.clone())
    }

    fn scope_name(&self) -> Option<String> {
        Some(self.name.to_string())
    }
}

impl McModule {
    /// Recursively scan AST nodes in a net expression for identifiers that match
    /// known port names (both from body insts and params), and record their spans for LSP goto-definition.
    fn collect_port_refs_in_node(
        node: &AstNode,
        insts: &mut McInstances,
        params: &mut McParamDeclares,
    ) {
        // Check this node (only leaf identifier nodes for precise spans)
        match node.get_type() {
            MCAST_ID | MCAST_IDA => {
                if let Some(text) = node.to_string() {
                    if insts.contains(&text) {
                        let span =
                            (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                        insts.record_port_ref(span, &text);
                    } else if params.contains(&text) {
                        let span =
                            (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                        params.record_port_ref(span, &text);
                    }
                }
            }
            _ => {}
        }
        // Walk sub-node chain (children)
        if let Some(sub) = node.get_sub_node() {
            let mut current = sub;
            loop {
                Self::collect_port_refs_in_node(&current, insts, params);
                match current.get_next() {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
    }
}

// ============================================================================
// Mc2Module - Module instance wrapper
// ============================================================================

#[derive(Debug, Clone)]
pub struct Mc2Module {
    pub base: Arc<McModule>,
    pub name: McIds,
    pub args: Vec<McParamValue>,
    pub insts: Vec<McInst>,
}

impl Mc2Module {
    pub fn new(name: &str, base: Arc<McModule>) -> Self {
        Self {
            base,
            name: McIds::from(name),
            args: Vec::new(),
            insts: Vec::new(),
        }
    }

    pub fn with_params(name: &str, base: Arc<McModule>, args: Vec<McParamValue>) -> Self {
        Self {
            base,
            name: McIds::from(name),
            args,
            insts: Vec::new(),
        }
    }

    /// Find externally exposed ports
    pub fn find_port(&self, id: &str) -> Option<McPhrase> {
        // 1. Find in interface definitions
        if let Some(_port) = self.base.insts.find_port(id) {
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(McBus::new_with_members(
                    &self.name.to_string(),
                    vec![id.to_string()],
                )),
            ))));
        }

        // 2. Support dot-path lookup (e.g. "in.data")
        if let Some((first, rest)) = id.split_once('.') {
            if let Some((_iotype, port)) = self.base.insts.get_with_iotype(first) {
                // Find in port's sub-members
                for member_name in port.members() {
                    if member_name == rest {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(&format!(
                                "{}.{}.{}",
                                self.name, first, rest
                            ))),
                        ))));
                    }
                }
            }
        }

        // 3. Find in functions (supports method calls)
        // TODO: phase 2 implementation

        None
    }

    /// Get all input ports
    pub fn get_input_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_inputs()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }

    /// Get all output ports
    pub fn get_output_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_outputs()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }

    /// Get all ports
    pub fn get_all_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_ports()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }
}

// ============================================================================
// Display implementation - concise format output
// ============================================================================

impl std::fmt::Display for McModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Module {}", self.name)?;
        writeln!(f, "  Insts:")?;

        // Collect inst data for alignment calculation
        #[derive(Debug)]
        struct InstRow {
            io: String,
            name: String,
            inst: String,
            inst_type: String,
            has_io: bool,
            type_order: u8, // 0=Component/Module, 1=Interface, 2=Label, 3=Bus, 4=other
        }

        let mut rows: Vec<InstRow> = Vec::new();
        for (name, (io, inst)) in self.insts.iter_with_iotype() {
            let has_io = !matches!(*io, IOType::None);
            let io_str = if has_io {
                format!("{io:?}")
            } else {
                String::new()
            };
            // Strip type prefixes from instance display and collect type separately
            let (inst_str, type_str, type_order) = match inst {
                McInstance::Component(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Component:").to_string(),
                        "Component".to_string(),
                        0,
                    )
                }
                McInstance::Module(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Module:").to_string(),
                        "Module".to_string(),
                        0,
                    )
                }
                McInstance::Label(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("L:").to_string(),
                        "Label".to_string(),
                        2,
                    )
                }
                McInstance::Interface(_) => (inst.to_string(), "Interface".to_string(), 1),
                McInstance::Bus(_) => (inst.to_string(), "Bus".to_string(), 3),
                McInstance::BusRef { .. } => (inst.to_string(), "Ref".to_string(), 4),
                McInstance::List(_) => (inst.to_string(), "List".to_string(), 4),
            };
            rows.push(InstRow {
                io: io_str,
                name: name.to_string(),
                inst: inst_str,
                inst_type: type_str,
                has_io,
                type_order,
            });
        }

        // Sort: 1. has_io=true first, 2. type_order, 3. name
        rows.sort_by(|a, b| {
            let io_cmp = b.has_io.cmp(&a.has_io);
            if io_cmp != std::cmp::Ordering::Equal {
                return io_cmp;
            }
            let type_cmp = a.type_order.cmp(&b.type_order);
            if type_cmp != std::cmp::Ordering::Equal {
                return type_cmp;
            }
            a.name.cmp(&b.name)
        });

        // Calculate column widths
        let io_width = rows.iter().map(|r| r.io.len()).max().unwrap_or(0);
        let name_width = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
        let inst_width = rows.iter().map(|r| r.inst.len()).max().unwrap_or(0);

        // Output with alignment
        for row in &rows {
            if row.io.is_empty() {
                if row.inst_type.is_empty() {
                    writeln!(
                        f,
                        "    {:<width$} {:<name_width$} = {:<inst_width$}",
                        "",
                        row.name,
                        row.inst,
                        width = io_width,
                        name_width = name_width,
                        inst_width = inst_width
                    )?;
                } else {
                    writeln!(
                        f,
                        "    {:<width$} {:<name_width$} = {:<inst_width$}  {}",
                        "",
                        row.name,
                        row.inst,
                        row.inst_type,
                        width = io_width,
                        name_width = name_width,
                        inst_width = inst_width
                    )?;
                }
            } else if row.inst_type.is_empty() {
                writeln!(
                    f,
                    "    {:<width$} {:<name_width$} = {:<inst_width$}",
                    row.io,
                    row.name,
                    row.inst,
                    width = io_width,
                    name_width = name_width,
                    inst_width = inst_width
                )?;
            } else {
                writeln!(
                    f,
                    "    {:<width$} {:<name_width$} = {:<inst_width$}  {}",
                    row.io,
                    row.name,
                    row.inst,
                    row.inst_type,
                    width = io_width,
                    name_width = name_width,
                    inst_width = inst_width
                )?;
            }
        }

        writeln!(f, "  Lines:")?;
        for line in &self.lines {
            writeln!(f, "    {line}")?;
        }
        Ok(())
    }
}
