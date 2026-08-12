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
use crate::db::context::DB;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};
use crate::semantic::component::Mc2Component;
use crate::semantic::context::resolve_cmie;
use crate::semantic::mc_func::McFuncReturn;
use crate::{
    ast::{ast_node::AstNode, c_macros::*, error::message::*},
    semantic::basic::mc_param::McParamDeclares,
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
    /// Source span for each connection line in `lines` (parallel array).
    /// Used for diagnostic position reporting during instantiation.
    pub line_spans: Vec<crate::ast::ast_semantic::Span>,
    pub funcs: McFunctions,
    pub uri: McURI,
    /// Source span for LSP goto-definition (byte range in `uri`).
    pub span: crate::ast::ast_semantic::Span,
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

            let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) else {
                dlog_error(804, node, MISSING_SUBNODE);
                return None;
            };

            let module_name = module_name?;

            // Span from the module name (MCAST_NAME → MCAST_IDS), not the whole node
            let ids_node = subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))
                .and_then(|n| n.get_sub_node())?;
            let start = ids_node.get_pos() as usize;
            let end = start + ids_node.get_len() as usize;
            let mut module = Self {
                name: module_name,
                params: McParamDeclares::new(),
                funcs: McFunctions::new(),
                insts: McInstances::new(),
                lines: Vec::new(),
                line_spans: Vec::new(),
                uri: uri.clone(),
                span: crate::ast::ast_semantic::Span { start, end },
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
                    // Instance parameter -> insts, or enum-class/interface data param
                    MCAST_DECLARE => {
                        // Check if CLASS is an enum → data param (B5/B6)
                        let is_enum = McParamType::extract_class_name_from_declare(&subnode)
                            .map(|cn| crate::db::cmie::cmie::is_enum_class_name(&cn))
                            .unwrap_or(false);
                        // Check if CLASS is an interface → port param (A3/A4)
                        // e.g., USB_VBUS_1{VDD_3V, GND}::DC(3.3V) has name prefix
                        // and is parsed as MCAST_DECLARE (not MCAST_SQUARE_VEC).
                        let pt = McParamType::from_ast(&subnode);
                        let is_interface = matches!(
                            pt.kind,
                            McParamTypeKind::Interface { .. }
                                | McParamTypeKind::InterfaceWithRole { .. }
                        );
                        if is_enum || is_interface {
                            self.params.parse(&param_node);
                        } else {
                            self.insts.parse(&subnode, &self.uri);
                        }
                    }
                    // IOTYPE-prefix parameter -> insts + params (e.g. ps dc24v, in GPIO[1:2])
                    MCAST_IOTYPE => {
                        self.insts.parse(&param_node, &self.uri);
                        self.params.parse(&param_node); // also register for unused detection
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
                let ct = clause.get_type();
                match ct {
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
                            let scope = self.name.to_string();
                            Self::collect_net_refs_in_node(
                                &subnode,
                                &mut self.insts,
                                &mut self.params,
                                &scope,
                            );
                            match McPhrase::new(&subnode, self) {
                                Some(net) => {
                                    // Store definition spans + LSP lapper entries for inline ports
                                    Self::collect_net_def_spans(
                                        &subnode,
                                        &mut self.insts,
                                        &self.uri,
                                        &self.name.to_string(),
                                    );
                                    // Track source span for diagnostic position reporting
                                    let line_start = subnode.get_pos() as usize;
                                    let line_end = line_start + subnode.get_len() as usize;
                                    self.line_spans.push(crate::ast::ast_semantic::Span {
                                        start: line_start,
                                        end: line_end,
                                    });
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

            // ★ Smart Param (M5): Check both formal params and body ports.
            let mod_name = self.name.to_string();
            let diags = self.params.finalize(Some(body), &mod_name);
            let warned: std::collections::HashSet<String> =
                diags.iter().map(|d| d.param_name.clone()).collect();
            for d in diags {
                crate::mcc_log_global_diag(&d);
            }
            for port_name in self.insts.iter_port_names() {
                if warned.contains(port_name) {
                    continue;
                }
                let mut span = self
                    .insts
                    .port_spans()
                    .get(port_name)
                    .and_then(|s| s.first().cloned())
                    .unwrap_or(0..1);
                // Fallback for IDX members stored individually without span:
                // try base name (strip trailing digits) in port_spans.
                if span.start == 0 && span.end == 1 {
                    // Try sibling labels with same base (GPIO2 → find GPIO1's span)
                    if let Some(base) = McInstances::strip_trailing_digits(port_name) {
                        for other in self.insts.iter_port_names() {
                            if other == port_name {
                                continue;
                            }
                            if let Some(other_base) = McInstances::strip_trailing_digits(other) {
                                if other_base == base {
                                    if let Some(s) =
                                        self.insts.port_spans().get(other).and_then(|v| v.first())
                                    {
                                        span = s.clone();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                let all_forms = self.insts.all_name_forms_for(port_name);
                let has_recorded_ref = self
                    .insts
                    .iter_net_refs()
                    .any(|(_, name, _)| all_forms.iter().any(|form| form == name));
                let has_ast_usage = all_forms.iter().any(|form| {
                    crate::semantic::basic::mc_param_infer::collect_usages(form, body)
                        .iter()
                        .any(|usage| usage.pos != span.start)
                });
                if !has_recorded_ref && !has_ast_usage {
                    crate::db::diagnostic::diagnostic::diagnostic_log(
                        1405,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                        span.start as u32,
                        (span.end - span.start) as u32,
                        &format!(
                            "Port '{}' in '{}' is declared but never used in any net connection.",
                            port_name, mod_name
                        ),
                        &[],
                    );
                }
            }
        }
    }
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
        self.insts
            .set_label_kind(&name, crate::semantic::mc_inst::LabelKind::Inline);
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
        // ── P2-10: anonymous components (names starting with @) are created
        // inline in connection lines. They must NOT be stored in insts,
        // otherwise instantiate_declarations_resilient will create them as
        // declarations with no connections, duplicating the line-created ones.
        if !name.starts_with('@') {
            self.insts.create_inst(&name, inst.clone());
        }
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
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance> {
        self.insts.get_mut(id)
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        // P1: param ports (IO params) — highest priority
        for (name, span) in self.params.iter_ports_with_span() {
            if name == id {
                return Some((McInstance::Label(id.to_string()), Some(span)));
            }
        }
        // P2: param defs (non-port params)
        for (name, span) in self.params.iter_defs_with_span() {
            if name == id {
                return Some((McInstance::Label(id.to_string()), Some(span)));
            }
        }
        // P3: ports (instances with IOType ≠ None, e.g. In, Out, Power)
        if let Some((iotype, inst)) = self.insts.get_with_iotype(id) {
            if !matches!(iotype, IOType::None) {
                let span = self.insts.get_port_span(id);
                return Some((inst.clone(), span));
            }
        }
        // P4: explicit labels (McInstance::Label entries in insts)
        for (name, _kind, span) in self.insts.iter_labels_with_span() {
            if name == id {
                return Some((McInstance::Label(id.to_string()), Some(span)));
            }
        }
        // P5: remaining non-port, non-label insts (Component/Module/Interface/Bus/List)
        if let Some((iotype, inst)) = self.insts.get_with_iotype(id) {
            if matches!(iotype, IOType::None) && !matches!(inst, McInstance::Label(_)) {
                let span = self.insts.get_port_span(id);
                return Some((inst.clone(), span));
            }
        }
        // P6: funcs
        if let Some(func) = self.funcs.find(id) {
            return Some((McInstance::Func(Arc::new(func.clone())), None));
        }
        None
    }

    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        if let Some(s) = span {
            self.insts.store_port_span(&name, s);
        }
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
        if let Some(McCMIE::Interface(_)) = resolve_cmie(&DB, &McIds::from("ADC.DIFF"), self.uri())
        {
            let iface_ref = McBus::new_with_members(&full_name, members);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(iface_ref),
            ))));
        }
        if let Some(McCMIE::Interface(_)) = resolve_cmie(
            &DB,
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
        comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase> {
        Some(self.add_component(name, comp))
    }

    fn add_module(
        &mut self,
        name: String,
        module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        Some(self.add_module(name, module))
    }

    /// Generate an anonymous instance name: `@{classname}{counter}` (e.g. `@RES1`, `@CAP2`).
    ///
    /// # Design rule
    /// Anonymous instances are created inline in connection statements
    /// (e.g. `-> RES(10kΩ) ->`). Their declaration position **is** their usage
    /// position — they exist solely as part of a connection chain and do not
    /// need to be referenced from elsewhere.
    ///
    /// Diagnostics that check for "unused" ports/instances must skip names
    /// produced by this function. See [`McInstances::iter_port_names`].
    fn gen_anon_name(&mut self, classname: &str) -> String {
        let name = format!("@{}{}", classname, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn store_inst_span(&mut self, name: &str, span: std::ops::Range<usize>) {
        self.insts.store_port_span(name, span);
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
    /// Walk AST nodes in a net phrase and store definition spans + LSP lapper
    /// entries for any identifier that becomes an inline port instance.
    fn collect_net_def_spans(node: &AstNode, insts: &mut McInstances, uri: &McURI, scope: &str) {
        match node.get_type() {
            MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC
            | MCAST_OPD_CURLY => {
                if let Some(text) = node.to_string() {
                    let span =
                        (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                    let key = insts.resolve_idx(&text).unwrap_or(text);
                    if insts.get(&key).is_some() && insts.port_spans().get(&key).is_none() {
                        insts.store_port_span(&key, span.clone());
                        // Register in name_to_declare_id so goto-def can find this inline port
                        if let Some(mcode) = crate::db::cmie::tables::WORKSPACE.mcodes.get(uri) {
                            if let Ok(mut sem) = mcode.symbols.lock() {
                                sem.local_table.add_declare_with_name(
                                    uri,
                                    crate::ast::ast_semantic::SourceLocation::from_span(&span),
                                    Some(key),
                                    Some(scope),
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if let Some(sub) = node.get_sub_node() {
            let mut cur = sub;
            loop {
                Self::collect_net_def_spans(&cur, insts, uri, scope);
                match cur.get_next() {
                    Some(next) => cur = next,
                    None => break,
                }
            }
        }
    }

    pub(crate) fn collect_net_refs_in_node(
        node: &AstNode,
        insts: &mut McInstances,
        params: &mut McParamDeclares,
        scope: &str,
    ) {
        let handled = match node.get_type() {
            MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN => {
                Self::record_scoped_net_ref(node, insts, params, scope);
                true
            }
            // ★ SQUARE_VEC / OPD_SQUARE_VEC (e.g. [VDD_3V3,GND]):
            //   text starts with `[` so split-by-`[` gives empty base.
            //   Iterate members and look up each individually — matching how
            //   McParamDeclares::parse stores them as individual keys in def_spans.
            MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC => {
                tracing::info!(target: "mcc::lsp",
                    "SQUARE_VEC_REF node_type={} pos={} len={}",
                    node.get_type(),
                    node.get_pos(),
                    node.get_len()
                );
                let mut current = node.get_sub_node();
                while let Some(phrase_node) = current {
                    let ids_node = phrase_node
                        .get_sub_node()
                        .unwrap_or_else(|| phrase_node.clone());
                    if let Some(ids) = crate::semantic::basic::mc_ids::McIds::new(&ids_node) {
                        let name = ids.to_string();
                        let member_span = (ids_node.get_pos() as usize)
                            ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                        let in_insts = insts.port_spans().contains_key(&name);
                        let in_params = params.is_defined(&name);
                        tracing::info!(
                            "SQUARE_VEC_REF member='{name}' span=[{},{}] in_insts={in_insts} in_params={in_params} scope='{scope}'",
                            member_span.start, member_span.end
                        );
                        if in_insts {
                            insts.record_net_ref(member_span, &name, scope);
                        } else if in_params {
                            params.record_net_ref(member_span, &name, scope);
                        }
                    }
                    current = phrase_node.get_next();
                }
                true
            }
            MCAST_OPD => {
                if let Some(sub) = node.get_sub_node() {
                    let inner_type = sub.get_type();
                    if matches!(
                        inner_type,
                        MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN
                    ) {
                        Self::record_scoped_net_ref(&sub, insts, params, scope);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        };

        if !handled {
            let Some(sub) = node.get_sub_node() else {
                return;
            };
            let mut current = sub;
            loop {
                Self::collect_net_refs_in_node(&current, insts, params, scope);
                match current.get_next() {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
    }

    fn record_scoped_net_ref(
        node: &AstNode,
        insts: &mut McInstances,
        params: &mut McParamDeclares,
        scope: &str,
    ) {
        let Some(text) = node.to_string() else {
            return;
        };
        let ids = McIds::new(node);
        let root = ids.as_ref().and_then(McIds::root_name).unwrap_or_else(|| {
            text.split(|c: char| c == '.' || c == '{' || c == '[')
                .next()
                .unwrap_or(&text)
                .to_string()
        });
        if root.is_empty() {
            return;
        }

        let start = node.get_pos() as usize;
        let span = start..(start + root.len().min(node.get_len() as usize));
        let matched_key = if insts.contains(&root) {
            Some(root.clone())
        } else {
            insts
                .resolve_idx(&text)
                .or_else(|| insts.resolve_idx(&root))
        };

        if let Some(key) = matched_key {
            insts.record_net_ref(span, &key, scope);
        } else if params.is_defined(&root) {
            params.record_net_ref(span, &root, scope);
        } else {
            insts.record_net_ref(span, &root, scope);
        }

        // ★ §3.4.3 (rev): per-segment member refs — curly-bus members and
        // dot members get their own refs so F12 lands on the member text.
        // Member node pos is reliable; len is not (mc_value_link extension).
        //
        // Two curly forms are handled:
        //   - `MIC{P,N}`        — McIds parses the whole bus (`as_bus` hits);
        //   - `U_MCU{I2C0.SCL}` — McIds only parses the leading segment, so the
        //     curly child carries the members; base = first segment (`root`).
        if let Some(ids) = ids {
            let base_from_bus = ids.as_bus().map(|(b, _members)| b);
            let has_curly_child = node.get_sub_node().map_or(false, |sub| {
                let mut cur = Some(sub);
                loop {
                    let Some(child) = cur else { break false };
                    if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                        break true;
                    }
                    cur = child.get_next();
                }
            });
            if base_from_bus.is_some() || has_curly_child {
                // Register each curly member as `<base>.<member>` so F12 lands
                // on the member text (`MIC.P`, `U_MCU.I2C0.SCL`).
                let bus = base_from_bus.unwrap_or_else(|| root.clone());
                let mut cur = node.get_sub_node();
                while let Some(child) = cur {
                    if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                        let mut mc = child.get_sub_node();
                        while let Some(m) = mc {
                            if let Some(mname) = m.to_string() {
                                let mstart = m.get_pos() as usize;
                                insts.record_net_ref(
                                    mstart..(mstart + mname.len()),
                                    &format!("{bus}.{mname}"),
                                    scope,
                                );
                            }
                            mc = m.get_next();
                        }
                    }
                    cur = child.get_next();
                }
            } else if ids.count() >= 2 {
                // `MIC.P`: register the member segment (text after the dot).
                let member_start = start + root.len() + 1;
                let member_len = text.len().saturating_sub(root.len() + 1);
                if member_len > 0 {
                    insts.record_net_ref(member_start..(member_start + member_len), &text, scope);
                }
            } else {
                // ★ Dot chain: `MIC.P`, `U_MCU.I2C0.SCL`. `node.to_string()`
                // returns only the base segment (`U_MCU`), while `ids.to_string()`
                // carries the full chain. Register the member segment(s) after
                // the first dot so F12 lands on the member text and lapper can
                // resolve it via the member chain (Phase 3).
                let full = ids.to_string();
                if !ids.is_square_only() && full.contains('.') {
                    let member_start = start + root.len() + 1;
                    let member_end = start + full.len();
                    if member_end > member_start {
                        insts.record_net_ref(member_start..member_end, &full, scope);
                    }
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
                McInstance::List(_) | McInstance::Unresolved { .. } => {
                    (inst.to_string(), "Unresolved".to_string(), 5)
                }
                McInstance::Pins => ("pins".to_string(), "Pins".to_string(), 6),
                McInstance::Attr(_) => (inst.to_string(), "Attr".to_string(), 7),
                McInstance::Func(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Func:").to_string(),
                        "Func".to_string(),
                        8,
                    )
                }
                McInstance::EnumVal { .. } => (inst.to_string(), "EnumVal".to_string(), 9),
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
