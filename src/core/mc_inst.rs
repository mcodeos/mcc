// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::{ast_node::AstNode, c_macros::*};
use crate::builder::diagnostic::{dlog_error, dlog_warning};
use crate::builder::mcb_get_cmie;
use crate::builder::mcb_register_declare_class;
use crate::builder::mcb_register_instance_decl;
use crate::core::basic::mc_bus::{McBus, McList};
use crate::core::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::core::basic::mc_ida::McIda;
use crate::core::basic::mc_ids::{IdsSegment, McIds};
use crate::core::basic::mc_literal::{McInt, McString};
use crate::core::basic::mc_param::McParamValue;
use crate::core::basic::mc_phrase::McPhrase;
use crate::core::basic::mc_uval::McUnitValue;
use crate::core::common::IOType;
use crate::core::component::Mc2Component;
use crate::core::mc_ifs::Mc2Interface;
use crate::core::module::Mc2Module;
use crate::message::MISSING_SUBNODE;
use crate::McCMIE;
use crate::McURI;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

/// ── P1: Robustly extract identifier from a MCAST_PARAM child node (V3V3 / V1V2 / [VDD,GND] / flash.SPI).
/// Don't guess a single node type: try McIds::new on current node, if failed, unpack downwards (OPD/PARAM wrapper) and retry,
/// up to 4 layers. Consistent with parse_declare's MCAST_OPD unpacking for instance name parsing.
fn extract_param_ids(node: &AstNode) -> Option<McIds> {
    let mut cur = node.clone();
    for _ in 0..4 {
        if let Some(ids) = McIds::new(&cur) {
            if !ids.is_empty() {
                return Some(ids);
            }
        }
        match cur.get_sub_node() {
            Some(sub) => cur = sub,
            None => break,
        }
    }
    None
}

/// ── P1: Collect constructor arguments from MCAST_INSTANCE (parenthesized arguments of mcu513(V3V3,V1V2) / flash(V3V3)).
/// Arguments are attached inside the instance node, as the next sibling MCAST_PARAMS of id (mc_inst.rs:854 comment);
/// some forms are attached to the next sibling of the instance node, try both places, take the first non-empty.
fn collect_ctor_params(inst_node: &AstNode, inst_id_node: &AstNode) -> Vec<McParamValue> {
    for cand in [inst_id_node.get_next(), inst_node.get_next()] {
        let Some(n) = cand else {
            continue;
        };
        if n.get_type() != MCAST_PARAMS {
            continue;
        }
        let Some(psub) = n.get_sub_node() else {
            continue;
        };
        let mut out: Vec<McParamValue> = Vec::new();
        for p in psub.iter() {
            if p.get_type() != MCAST_PARAM {
                continue;
            }
            let Some(sub) = p.get_sub_node() else {
                continue;
            };
            match sub.get_type() {
                MCAST_INT => {
                    if let Some(v) = McInt::new(&sub) {
                        out.push(McParamValue::Int(v));
                    }
                }
                MCAST_STRING => {
                    let val = sub.to_string().unwrap_or_default();
                    let clean_val = if val.starts_with('"') && val.ends_with('"') && val.len() >= 2
                    {
                        val[1..val.len() - 1].to_string()
                    } else {
                        val
                    };
                    out.push(McParamValue::String(McString::from(clean_val.as_str())));
                }
                MCAST_OPD_NC => out.push(McParamValue::NC(String::from("NC"))),
                MCAST_UVALUE => {
                    if let Some(uval) = McUnitValue::new(&sub) {
                        out.push(McParamValue::UValue(uval));
                    }
                }
                // net-ref / identifier (V3V3, V1V2, [VDD,GND], flash.SPI ...) —— robust extraction
                _ => {
                    if let Some(ids) = extract_param_ids(&sub) {
                        if !ids.is_empty() {
                            out.push(McParamValue::Ids(ids));
                        }
                    }
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    Vec::new()
}

/// Instance information
#[derive(Debug, Clone)]
pub struct McInst {
    pub id: McIds,
    pub params: Vec<McParamValue>,
}

/// Whether a label is explicitly declared or defined inline in a net phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    /// Explicitly declared in parameter list or port section.
    Explicit,
    /// Defined on-the-fly in a net connection (e.g. `res1 - A` where `A` is not a port).
    Inline,
}

/// Identifier types within a module
///
/// Used in symbol table to store various declared entities
#[derive(Debug, Clone)]
pub enum McInstance {
    Label(String),
    List(McList),
    Bus(McBus),
    BusRef {
        component: String,
        bus: String,
    },
    Interface(Arc<Mc2Interface>),
    Component(Arc<Mc2Component>),
    Module(Arc<Mc2Module>),
    /// Unresolved component/module reference — class definition not found in
    /// loaded scope (e.g. library not loaded). Stored as a named instance so
    /// net connections still resolve, but flagged for diagnostics.
    Unresolved {
        class_name: String,
    },
}

impl McInstance {
    /// Get the identifier's name
    pub fn get_name(&self) -> String {
        use McInstance::*;
        match self {
            Label(s) => s.clone(),
            Bus(b) => b.name.clone(),
            BusRef { component, bus } => format!("{component}.{bus}"),
            List(l) => l.name.clone(),
            Interface(i) => i.name.to_string(),
            Component(c) => c.name.to_string(),
            Module(m) => m.name.to_string(),
            Unresolved { class_name } => class_name.clone(),
        }
    }

    /// Get member list
    pub fn members(&self) -> Vec<String> {
        match self {
            McInstance::Bus(b) => b.member.clone(),
            _ => Vec::new(),
        }
    }

    /// Get full member list (including full_members)
    pub fn full_members(&self) -> Vec<String> {
        match self {
            McInstance::Bus(b) => b.full_members.clone(),
            _ => Vec::new(),
        }
    }

    /// Convert to node element with prefix
    pub fn to_node_element_with_prefix(&self, prefix: &str) -> McBus {
        let name = self.get_name();
        McBus {
            name: format!("{prefix}.{name}"),
            member: Vec::new(),
            full_members: Vec::new(),
        }
    }

    /// Convert to McBus
    pub fn to_node_element(&self) -> McBus {
        match self {
            McInstance::Label(s) => McBus::new(s),
            McInstance::List(l) => McBus::new_with_members(&l.name, l.member.clone()),
            McInstance::Bus(b) => b.clone(),
            McInstance::BusRef { component, bus } => {
                McBus::new_with_members(&format!("{component}.{bus}"), vec![])
            }
            McInstance::Interface(i) => McBus::new(&i.name.to_string()),
            McInstance::Component(c) => McBus::new(&c.name.to_string()),
            McInstance::Module(m) => McBus::new(&m.name.to_string()),
            McInstance::Unresolved { class_name } => McBus::new(class_name),
        }
    }

    /// Check if it's a component
    pub fn is_component(&self) -> bool {
        match self {
            McInstance::Component(_) => true,
            _ => false,
        }
    }

    /// Check if it's a module
    pub fn is_module(&self) -> bool {
        match self {
            McInstance::Module(_) => true,
            _ => false,
        }
    }

    /// Check if it's a label or bus
    pub fn is_label_or_bus(&self) -> bool {
        match self {
            McInstance::Label(_)
            | McInstance::Bus(_)
            | McInstance::BusRef { .. }
            | McInstance::List(_)
            | McInstance::Unresolved { .. } => true,
            _ => false,
        }
    }

    /// Get type name string
    pub fn type_name(&self) -> &'static str {
        match self {
            McInstance::Label(_) => "Label",
            McInstance::Bus(_) => "Bus",
            McInstance::BusRef { .. } => "Ref",
            McInstance::List(_) => "List",
            McInstance::Interface(_) => "Interface",
            McInstance::Component(_) => "Component",
            McInstance::Module(_) => "Module",
            McInstance::Unresolved { .. } => "Unresolved",
        }
    }
}

/// McInstances - Symbol table for instances and ports within a module
///
/// Stores all identifiers within module: (IOType, McInstance) mapping
#[derive(Debug, Clone)]
pub struct McInstances {
    insts: BTreeMap<String, (IOType, McInstance)>,
    /// Port spans for LSP goto-definition (name -> span ranges, multiple for DOT patterns)
    port_spans: HashMap<String, Vec<Range<usize>>>,
    /// LSP: spans in module body that reference port definitions (span, port_name)
    port_ref_spans: Vec<(Range<usize>, String, String)>, // (span, port_name, scope)
    /// ★ LSP: Enclosing scope name (module/component/function name)
    pub(crate) scope: Option<String>,
    /// Label kind registry: tracks whether a label is Explicit (declared) or Inline (net phrase).
    label_kinds: HashMap<String, LabelKind>,
}

impl McInstances {
    pub(crate) fn new() -> Self {
        Self {
            insts: BTreeMap::new(),
            port_spans: HashMap::new(),
            port_ref_spans: Vec::new(),
            scope: None,
            label_kinds: HashMap::new(),
        }
    }

    /// Record a label's kind. Idempotent: Explicit takes precedence over Inline.
    pub fn set_label_kind(&mut self, name: &str, kind: LabelKind) {
        match self.label_kinds.get(name) {
            Some(LabelKind::Explicit) => {} // Explicit overrides Inline
            _ => {
                self.label_kinds.insert(name.to_string(), kind);
            }
        }
    }

    /// Get a label's kind. Defaults to Explicit if not recorded.
    pub fn get_label_kind(&self, name: &str) -> LabelKind {
        self.label_kinds
            .get(name)
            .copied()
            .unwrap_or(LabelKind::Explicit)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.insts.contains_key(name)
    }

    /// Iterate all ports (instances with IOType != None/Return/NonCon)
    pub fn iter_ports(&self) -> impl Iterator<Item = (&str, &IOType)> {
        self.insts
            .iter()
            .filter(|(_, (io_type, _))| {
                !matches!(io_type, IOType::None | IOType::Return | IOType::NonCon | IOType::Label)
            })
            .map(|(name, (io_type, _))| (name.as_str(), io_type))
    }

    /// Get port span by name (returns first span if multiple)
    pub fn get_port_span(&self, name: &str) -> Option<Range<usize>> {
        self.port_spans.get(name).and_then(|v| v.first().cloned())
    }

    /// Access port_spans for diagnostic purposes (all ports, no IOType filter).
    pub fn port_spans(&self) -> &HashMap<String, Vec<Range<usize>>> {
        &self.port_spans
    }

    /// Iterate all instance names (for unused-port diagnostics).
    pub fn iter_instance_names(&self) -> impl Iterator<Item = &String> {
        self.insts.keys()
    }

    /// Iterate only port-declaration names — skips auto-generated Component/Module
    /// instances (e.g. `@RES1`, `@CAP1`) that are not user-declared ports.
    pub fn iter_port_names(&self) -> impl Iterator<Item = &String> {
        self.insts
            .iter()
            .filter(|(_, (io_type, _))| {
                // Labels are not ports — they have no IO direction
                !matches!(io_type, IOType::Label)
            })
            .filter_map(|(name, (_, inst))| match inst {
                McInstance::Component(_)
                | McInstance::Module(_)
                | McInstance::Unresolved { .. } => None,
                _ => Some(name),
            })
    }

    /// Access the raw insts table for diagnostics.
    pub fn insts(&self) -> &BTreeMap<String, (IOType, McInstance)> {
        &self.insts
    }

    /// Return all possible name forms that could reference this port at a usage site.
    pub fn all_name_forms_for(&self, key: &str) -> Vec<String> {
        let mut forms = vec![key.to_string()];
        if let Some((_, inst)) = self.insts.get(key) {
            match inst {
                McInstance::Bus(bus) => {
                    // Dot-member forms only; bare member names are NOT valid
                    // references (per IDX expansion strategy).
                    for m in &bus.member {
                        forms.push(format!("{}.{}", bus.name, m));
                    }
                }
                McInstance::List(list) => {
                    // Square-indexed: generate GPIO1, GPIO[1], GPIO2, GPIO[2], 1, 2
                    for m in &list.member {
                        forms.push(format!("{}[{}]", list.name, m));
                        forms.push(format!("{}{}", list.name, m)); // GPIO1, GPIO2
                        forms.push(m.clone()); // bare "1", "2"
                    }
                }
                McInstance::Label(label) => {
                    forms.push(label.clone());
                    if let Some(base) = Self::strip_trailing_digits(label) {
                        if let Some(num) = label.strip_prefix(&base) {
                            forms.push(base.clone());
                            forms.push(format!("{}[{}]", base, num));
                        }
                    }
                    // "DC1{VDD,GND}" → also "DC1"
                    if let Some(pos) = label.find('{') {
                        forms.push(label[..pos].to_string());
                    }
                }
                _ => {}
            }
        }
        forms
    }

    /// Store port span when a port is inserted
    pub(crate) fn store_port_span(&mut self, name: &str, span: Range<usize>) {
        self.port_spans
            .entry(name.to_string())
            .or_default()
            .push(span);
    }

    /// Iterate all ports with their spans (multiple entries per key for DOT patterns)
    pub fn iter_ports_with_span(&self) -> impl Iterator<Item = (&str, &IOType, Range<usize>)> + '_ {
        self.insts
            .iter()
            .filter(|(_, (io_type, _))| {
                !matches!(io_type, IOType::None | IOType::Return | IOType::NonCon | IOType::Label)
            })
            .filter_map(|(name, (io_type, _))| {
                self.port_spans
                    .get(name)
                    .map(|spans| (name.as_str(), io_type, spans))
            })
            .flat_map(|(name, iotype, spans)| {
                spans.iter().map(move |span| (name, iotype, span.clone()))
            })
    }

    /// Iterate all labels (explicit and inline) with their spans.
    /// Labels are instances with IOType::None that have stored port spans.
    pub fn iter_labels_with_span(&self) -> impl Iterator<Item = (&str, LabelKind, Range<usize>)> + '_ {
        self.port_spans
            .iter()
            .filter(|(name, _spans)| {
                // Only include entries that are Label instances (not ports/buses/components)
                matches!(
                    self.insts.get(*name).map(|(_, inst)| inst),
                    Some(McInstance::Label(_))
                )
            })
            .flat_map(|(name, spans)| {
                let kind = self.get_label_kind(name);
                spans.iter().map(move |span| (name.as_str(), kind, span.clone()))
            })
    }

    /// Record a net-line reference to a port definition (for LSP goto-definition)
    pub(crate) fn record_port_ref(&mut self, span: Range<usize>, port_name: &str, scope: &str) {
        self.port_ref_spans
            .push((span, port_name.to_string(), scope.to_string()));
    }

    pub fn iter_port_refs(&self) -> impl Iterator<Item = &(Range<usize>, String, String)> {
        self.port_ref_spans.iter()
    }

    pub fn parse(&mut self, node: &AstNode, uri: &McURI) {
        // Handle MCAST_NET_PORTS specially - extract spans for port definitions
        if node.get_type() == MCAST_NET_PORTS {
            if let Some(subnode) = node.get_sub_node() {
                // First child is IOTYPE (ps, io, in, out, label)
                if let Some(first) = subnode.iter().next() {
                    if let Some(iotype) = IOType::new(&first) {
                        let iotype_ref = &iotype;
                        // Process remaining children as operands
                        for child in first.iter().skip(1) {
                            let ctype = child.get_type();
                            match ctype {
                                MCAST_DECLARE => {
                                    let before: Vec<String> = self.insts.keys().cloned().collect();
                                    self.parse_declare(&child, uri, iotype_ref);
                                    let new_keys: Vec<String> = self
                                        .insts
                                        .keys()
                                        .filter(|k| !before.contains(k))
                                        .cloned()
                                        .collect();
                                    if new_keys.len() == 1 {
                                        // Use instance ID position from the DECLARE's child
                                        let inst_span = Self::find_instance_span(&child);
                                        for k in new_keys {
                                            self.store_port_span(&k, inst_span.clone());
                                            // ★ Label: set explicit kind
                                            if matches!(iotype_ref, IOType::Label) {
                                                self.set_label_kind(&k, LabelKind::Explicit);
                                            }
                                        }
                                    }
                                }
                                MCAST_OPD => {
                                    let span = (child.get_pos() as usize)
                                        ..((child.get_pos() + child.get_len()) as usize);
                                    // Detect DOT pattern (DC2.VDD, label1.sub) before parse
                                    let dot_base = child.get_sub_node().and_then(|opd| {
                                        let first = opd.get_sub_node()?;
                                        let next = first.get_next()?;
                                        if next.get_type() == MCAST_OPD_DOT {
                                            first.to_string()
                                        } else {
                                            None
                                        }
                                    });
                                    if let Some(ref base) = dot_base {
                                        // DOT pattern: key always exists (or is created) in insts as `base`
                                        self.parse_opd(&child, iotype_ref.clone());
                                        self.store_port_span(base, span);
                                    } else {
                                        // Non-DOT: snapshot existing keys, then store spans for new ones
                                        let before_keys: std::collections::HashSet<String> =
                                            self.insts.keys().cloned().collect();
                                        self.parse_opd(&child, iotype_ref.clone());
                                        let new_keys: Vec<String> = self
                                            .insts
                                            .keys()
                                            .filter(|k| !before_keys.contains(*k))
                                            .cloned()
                                            .collect();
                                        for k in new_keys {
                                            self.store_port_span(&k, span.clone());
                                            // ★ Label: set explicit kind
                                            if matches!(iotype_ref, IOType::Label) {
                                                self.set_label_kind(&k, LabelKind::Explicit);
                                            }
                                        }
                                    }
                                }
                                MCAST_OPD_SQUARE_VEC => {
                                    let span = (child.get_pos() as usize)
                                        ..((child.get_pos() + child.get_len()) as usize);
                                    // Store span before parse to capture the @N index used by parse_opd_square_vec
                                    let port_key = format!("@{}", self.insts.len());
                                    self.parse_opd_square_vec(&child, iotype_ref.clone());
                                    self.store_port_span(&port_key, span);
                                }
                                _ => {}
                            }
                        }
                        return;
                    }
                }
            }
        }

        // Handle MCAST_DECLARE directly (e.g., "RES res1, res2" without IOTYPE)
        if node.get_type() == MCAST_DECLARE {
            self.parse_declare(node, uri, &IOType::None);
            return;
        }

        // Handle MCAST_OPD directly (reference parameters like &dc24v, &GPIO[1:2])
        // when called from parse_params without IOType prefix
        if node.get_type() == MCAST_OPD {
            self.parse_opd(node, IOType::Power);
            return;
        }

        // Handle MCAST_OPD_SQUARE_VEC directly (reference set like &[VDD1, GND1])
        if node.get_type() == MCAST_OPD_SQUARE_VEC {
            let span = (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
            let port_key = format!("@{}", self.insts.len());
            self.parse_opd_square_vec(node, IOType::Power);
            self.store_port_span(&port_key, span);
            return;
        }

        let Some(subnode) = node.get_sub_node() else {
            dlog_error(1001, node, MISSING_SUBNODE);
            return;
        };

        // first node is IOTYPE
        if let Some(iotype) = IOType::new(&subnode) {
            for each in subnode.iter().skip(1) {
                match each.get_type() {
                    MCAST_DECLARE => {
                        self.parse_declare(&each, uri, &iotype);
                    }

                    MCAST_OPD => {
                        // Single port operand (e.g., DC1{VDD, GND} or GPIO[1:2])
                        let Some(opd_node) = each.get_sub_node() else {
                            continue;
                        };

                        // Compute span for this operand (used for LSP port_definition)
                        let span =
                            (each.get_pos() as usize)..((each.get_pos() + each.get_len()) as usize);

                        // Check if this is a DOT pattern (DC2.VDD)
                        let child = opd_node.get_sub_node();
                        let mut is_dot_pattern = false;
                        let mut base_name = String::new();
                        let mut dot_member = String::new();

                        if let Some(first) = child {
                            if first.get_type() == MCAST_ID {
                                base_name = first.to_string().unwrap_or_default();
                                if let Some(second) = first.get_next() {
                                    if second.get_type() == MCAST_OPD_DOT {
                                        is_dot_pattern = true;
                                        if let Some(member_node) = second.get_sub_node() {
                                            dot_member =
                                                member_node.to_string().unwrap_or_default();
                                        }
                                    }
                                }
                            }
                        }

                        if is_dot_pattern {
                            // DC2.VDD - dot access pattern
                            if let Some((existing_iotype, existing_port)) =
                                self.insts.get(&base_name)
                            {
                                if let McInstance::Bus(bus) = existing_port {
                                    let mut new_members = bus.member.clone();
                                    if !new_members.contains(&dot_member) {
                                        new_members.push(dot_member.clone());
                                    }
                                    self.insts.insert(
                                        base_name.clone(),
                                        (
                                            existing_iotype.clone(),
                                            McInstance::Bus(McBus::new_with_members(
                                                &base_name,
                                                new_members,
                                            )),
                                        ),
                                    );
                                    self.store_port_span(&base_name, span.clone());
                                    let full_name = format!("{}.{}", base_name, dot_member);
                                    self.store_port_span(&full_name, span);
                                    continue;
                                }
                            }
                            let dot_member_clone = dot_member.clone();
                            let members = vec![dot_member];
                            self.insts.insert(
                                base_name.clone(),
                                (
                                    iotype.clone(),
                                    McInstance::Bus(McBus::new_with_members(&base_name, members)),
                                ),
                            );
                            self.store_port_span(&base_name, span.clone());
                            let full_name = format!("{}.{}", base_name, dot_member_clone);
                            self.store_port_span(&full_name, span);
                            continue;
                        }

                        // Normal IDS pattern handling
                        match opd_node.get_type() {
                            MCAST_IDS => {
                                if let Some(pname) = McIds::new(&opd_node) {
                                    if let Some((busname, members)) = pname.as_bus() {
                                        let inst = if pname.is_curly_bracket() {
                                            McInstance::Bus(McBus::new_with_members(
                                                &busname, members,
                                            ))
                                        } else {
                                            McInstance::List(McList::new_with_members(
                                                &busname, members,
                                            ))
                                        };
                                        self.insts.insert(busname.clone(), (iotype.clone(), inst));
                                        self.store_port_span(&busname, span.clone());
                                    }
                                    if pname.is_square_only() {
                                        let members = pname.expand();
                                        let next_node = opd_node.get_next();
                                        let mut interface_name = None;
                                        if let Some(n) = next_node {
                                            if n.get_type() == MCAST_OPD_DBCOLON {
                                                if let Some(sub) = n.get_sub_node() {
                                                    interface_name =
                                                        Some(sub.to_string().unwrap_or_default());
                                                }
                                            }
                                        }
                                        if let Some(iface_str) = interface_name {
                                            if let Some(McCMIE::Interface(iface_def)) =
                                                mcb_get_cmie(&McIds::from(iface_str.as_str()), uri)
                                            {
                                                let members_ids: Vec<IdsSegment> = members
                                                    .iter()
                                                    .map(|m| {
                                                        IdsSegment::Ida(Box::new(McIda::from(
                                                            m.as_str(),
                                                        )))
                                                    })
                                                    .collect();
                                                let ids_name = McIds {
                                                    segments: vec![IdsSegment::Square(members_ids)],
                                                };
                                                let port_name =
                                                    format!("[{}]", members.to_vec().join(","));
                                                let mc_inst = McInstance::Interface(Arc::new(
                                                    Mc2Interface::new(ids_name, iface_def.clone()),
                                                ));
                                                self.insts.insert(
                                                    port_name.clone(),
                                                    (iotype.clone(), mc_inst),
                                                );
                                                self.store_port_span(&port_name, span);
                                            } else {
                                                dlog_error(
                                                    1703,
                                                    &opd_node,
                                                    &format!(
                                                        "Interface '{}' not found for bus '{}[{}]'",
                                                        iface_str,
                                                        pname,
                                                        members.to_vec().join(",")
                                                    ),
                                                );
                                                let port_name = format!("@{}", self.insts.len());
                                                self.insts.insert(
                                                    port_name.clone(),
                                                    (
                                                        iotype.clone(),
                                                        McInstance::List(McList::new_with_members(
                                                            &port_name, members,
                                                        )),
                                                    ),
                                                );
                                                self.store_port_span(&port_name, span);
                                            }
                                        } else {
                                            let port_name = format!("@{}", self.insts.len());
                                            self.insts.insert(
                                                port_name.clone(),
                                                (
                                                    iotype.clone(),
                                                    McInstance::List(McList::new_with_members(
                                                        &port_name, members,
                                                    )),
                                                ),
                                            );
                                            self.store_port_span(&port_name, span);
                                        }
                                    } else {
                                        match pname.count() {
                                            1 => {
                                                self.insts.insert(
                                                    pname.to_string(),
                                                    (
                                                        iotype.clone(),
                                                        McInstance::Label(pname.to_string()),
                                                    ),
                                                );
                                                self.store_port_span(&pname.to_string(), span);
                                            }
                                            2.. => {
                                                // Check if contains curly or square bracket syntax (register as Bus as a whole)
                                                if pname.is_curly_bracket()
                                                    || pname.is_square_bracket()
                                                {
                                                    // Register as Bus as a whole, not register members separately
                                                    if let Some((busname, members)) = pname.as_bus()
                                                    {
                                                        let inst = if pname.is_curly_bracket() {
                                                            McInstance::Bus(
                                                                McBus::new_with_members(
                                                                    &busname, members,
                                                                ),
                                                            )
                                                        } else {
                                                            McInstance::Bus(
                                                                McBus::new_with_members(
                                                                    &busname, members,
                                                                ),
                                                            )
                                                        };
                                                        self.insts.insert(
                                                            busname.clone(),
                                                            (iotype.clone(), inst),
                                                        );
                                                        self.store_port_span(&busname, span);
                                                    } else {
                                                        // If as_bus() returns None, try manual parsing
                                                        let base = pname.base_name();
                                                        let members = pname.expand();
                                                        if !base.is_empty() && !members.is_empty() {
                                                            let inst = if pname.is_curly_bracket() {
                                                                McInstance::Bus(
                                                                    McBus::new_with_members(
                                                                        &base, members,
                                                                    ),
                                                                )
                                                            } else {
                                                                McInstance::Bus(
                                                                    McBus::new_with_members(
                                                                        &base, members,
                                                                    ),
                                                                )
                                                            };
                                                            self.insts.insert(
                                                                base.clone(),
                                                                (iotype.clone(), inst),
                                                            );
                                                            self.store_port_span(&base, span);
                                                        }
                                                    }
                                                } else {
                                                    // No curly or square brackets, register each member separately
                                                    let members = pname.expand();
                                                    for member in &members {
                                                        self.insts.insert(
                                                            member.clone(),
                                                            (
                                                                iotype.clone(),
                                                                McInstance::Label(member.clone()),
                                                            ),
                                                        );
                                                    }
                                                    if !members.is_empty() {
                                                        self.store_port_span(&members[0], span);
                                                    }
                                                }
                                            }
                                            _ => {
                                                dlog_error(
                                                    1202,
                                                    &opd_node,
                                                    "Port name count error",
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {
                                dlog_error(1200, &opd_node, "Port name not support type");
                            }
                        }
                    }

                    // Handle direct MCAST_IDS (e.g., for "ps dc24v" where dc24v is MCAST_IDS)
                    MCAST_IDS => {
                        if let Some(pname) = McIds::new(&each) {
                            // Compute span for this operand (used for LSP port_definition)
                            let span = (each.get_pos() as usize)
                                ..((each.get_pos() + each.get_len()) as usize);

                            // Check for DOT access pattern (e.g., DC2.VDD)
                            if let Some((base_name, dot_member)) = pname.as_dot_access() {
                                // DOT pattern: DC2.VDD - add member to existing bus or create new
                                if let Some((existing_iotype, existing_port)) =
                                    self.insts.get(&base_name)
                                {
                                    if let McInstance::Bus(bus) = existing_port {
                                        let mut new_members = bus.member.clone();
                                        if !new_members.contains(&dot_member) {
                                            new_members.push(dot_member.clone());
                                        }
                                        self.insts.insert(
                                            base_name.clone(),
                                            (
                                                existing_iotype.clone(),
                                                McInstance::Bus(McBus::new_with_members(
                                                    &base_name,
                                                    new_members,
                                                )),
                                            ),
                                        );
                                        // Don't overwrite existing span
                                        continue;
                                    }
                                }
                                // No existing bus, create new one
                                let members = vec![dot_member];
                                self.insts.insert(
                                    base_name.clone(),
                                    (
                                        iotype.clone(),
                                        McInstance::Bus(McBus::new_with_members(
                                            &base_name, members,
                                        )),
                                    ),
                                );
                                self.store_port_span(&base_name, span);
                                continue;
                            }

                            if let Some((busname, members)) = pname.as_bus() {
                                let inst = if pname.is_curly_bracket() {
                                    McInstance::Bus(McBus::new_with_members(&busname, members))
                                } else {
                                    McInstance::List(McList::new_with_members(&busname, members))
                                };
                                self.insts.insert(busname.clone(), (iotype.clone(), inst));
                                self.store_port_span(&busname, span);
                            } else if pname.is_square_only() {
                                let members = pname.expand();
                                let port_name = format!("@{}", self.insts.len());
                                self.insts.insert(
                                    port_name.clone(),
                                    (
                                        iotype.clone(),
                                        McInstance::List(McList::new_with_members(
                                            &port_name, members,
                                        )),
                                    ),
                                );
                                self.store_port_span(&port_name, span);
                            } else {
                                match pname.count() {
                                    1 => {
                                        self.insts.insert(
                                            pname.to_string(),
                                            (iotype.clone(), McInstance::Label(pname.to_string())),
                                        );
                                        self.store_port_span(&pname.to_string(), span);
                                    }
                                    2.. => {
                                        let members = pname.expand();
                                        for member in &members {
                                            self.insts.insert(
                                                member.clone(),
                                                (iotype.clone(), McInstance::Label(member.clone())),
                                            );
                                        }
                                        // Store port span for the base name (used for goto-def lookup)
                                        if !members.is_empty() {
                                            self.store_port_span(&members[0], span.clone());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }

                    MCAST_OPD_SQUARE_VEC => {
                        // Compute span for the entire square vector operand
                        let span =
                            (each.get_pos() as usize)..((each.get_pos() + each.get_len()) as usize);

                        let mut children: Vec<AstNode> = Vec::new();
                        let mut child = each.get_sub_node();
                        while let Some(c) = child {
                            children.push(c.clone());
                            child = c.get_next();
                        }

                        let mut members: Vec<String> = Vec::new();
                        let mut is_simple_bus = true;

                        for child_node in &children {
                            let actual_node =
                                child_node.get_sub_node().unwrap_or(child_node.clone());
                            if let Some(pname) = McIds::new(&actual_node) {
                                if pname.count() == 1 && !pname.is_square_only() {
                                    members.push(pname.to_string());
                                } else {
                                    is_simple_bus = false;
                                    break;
                                }
                            } else {
                                is_simple_bus = false;
                                break;
                            }
                        }

                        if is_simple_bus && members.len() >= 2 {
                            let port_name = format!("@{}", self.insts.len());
                            self.insts.insert(
                                port_name.clone(),
                                (
                                    iotype.clone(),
                                    McInstance::List(McList::new_with_members(&port_name, members)),
                                ),
                            );
                            self.store_port_span(&port_name, span);
                        } else {
                            for child_node in &children {
                                let Some(opd_node) = child_node.get_sub_node() else {
                                    continue;
                                };
                                // Compute span for individual child operand
                                let child_span = (opd_node.get_pos() as usize)
                                    ..((opd_node.get_pos() + opd_node.get_len()) as usize);
                                match opd_node.get_type() {
                                    MCAST_IDS => {
                                        if let Some(pname) = McIds::new(&opd_node) {
                                            if let Some((busname, bus_members)) = pname.as_bus() {
                                                self.insts.insert(
                                                    busname.clone(),
                                                    (
                                                        iotype.clone(),
                                                        McInstance::Bus(McBus::new_with_members(
                                                            &busname,
                                                            bus_members,
                                                        )),
                                                    ),
                                                );
                                                self.store_port_span(&busname, child_span);
                                            } else {
                                                match pname.count() {
                                                    1 => {
                                                        self.insts.insert(
                                                            pname.to_string(),
                                                            (
                                                                iotype.clone(),
                                                                McInstance::Label(
                                                                    pname.to_string(),
                                                                ),
                                                            ),
                                                        );
                                                        self.store_port_span(
                                                            &pname.to_string(),
                                                            child_span,
                                                        );
                                                    }
                                                    2.. => {
                                                        let exp_members = pname.expand();
                                                        for member in &exp_members {
                                                            self.insts.insert(
                                                                member.clone(),
                                                                (
                                                                    iotype.clone(),
                                                                    McInstance::Label(
                                                                        member.clone(),
                                                                    ),
                                                                ),
                                                            );
                                                        }
                                                        if !exp_members.is_empty() {
                                                            self.store_port_span(
                                                                &exp_members[0],
                                                                child_span,
                                                            );
                                                        }
                                                    }
                                                    _ => {
                                                        dlog_error(
                                                            1203,
                                                            &opd_node,
                                                            "Port name count error",
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        dlog_error(1200, &opd_node, "Port name not support type");
                                    }
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }
        } else {
            dlog_error(1002, &subnode, "Malformed IOTYPE node");
        }
    }

    /// Parse a MCAST_DECLARE node directly and create McInstance variants
    /// Strip trailing digits from a name like "GPIO1" → Some("GPIO"), "VDD1" → Some("VDD").
    /// Returns None if no trailing digits or name is all digits.
    pub fn strip_trailing_digits(s: &str) -> Option<String> {
        let base_end = s.trim_end_matches(|c: char| c.is_ascii_digit());
        if base_end.is_empty() || base_end.len() == s.len() {
            None
        } else {
            Some(base_end.to_string())
        }
    }

    /// Extract the instance identifier span from a MCAST_DECLARE node.
    /// Returns (pos, len) of the first MCAST_INSTANCE child's identifier.
    fn find_instance_span(node: &AstNode) -> std::ops::Range<usize> {
        if let Some(sub) = node.get_sub_node() {
            for child in sub.iter() {
                if child.get_type() == MCAST_INSTANCE {
                    if let Some(inst_sub) = child.get_sub_node() {
                        let ids_node = if inst_sub.get_type() == MCAST_OPD {
                            inst_sub.get_sub_node().unwrap_or(inst_sub)
                        } else {
                            inst_sub
                        };
                        let start = ids_node.get_pos() as usize;
                        let end = start + ids_node.get_len() as usize;
                        return start..end;
                    }
                }
            }
        }
        // Fallback to DECLARE node position
        (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize)
    }

    pub(crate) fn parse_declare(&mut self, node: &AstNode, uri: &McURI, iotype: &IOType) {
        // MCAST_DECLARE structure:
        // |- MCAST_CLASS (class_id, class_params)
        // |- MCAST_INSTANCE (instance_id, instance_params)
        // |- MCAST_INSTANCE (instance_id, instance_params) ... (multiple instances)
        let Some(sub) = node.get_sub_node() else {
            dlog_error(1101, node, "Missing sub node");
            return;
        };

        let mut class_node: Option<AstNode> = None;
        let mut inst_nodes: Vec<AstNode> = Vec::new();
        let mut params_node: Option<AstNode> = None;

        // Iterate linked list, find MCAST_CLASS, MCAST_PARAMS and all MCAST_INSTANCE
        for child in sub.iter() {
            let child_type = child.get_type();
            match child_type {
                MCAST_CLASS => {
                    class_node = Some(child.clone());
                    // CLASS node linked list structure:
                    // CLASS -> next -> INSTANCE (A)
                    // CLASS.sub -> IDS "HDR_SINGLE" -> next -> PARAMS (6)
                    // So need to iterate CLASS.sub linked list to find PARAMS
                    let mut current = child.get_sub_node();
                    while let Some(sub) = current {
                        let sub_type = sub.get_type();
                        if sub_type == MCAST_PARAMS && params_node.is_none() {
                            params_node = Some(sub.clone());
                        }
                        current = sub.get_next();
                    }
                }
                MCAST_PARAMS => {
                    params_node = Some(child);
                }
                MCAST_INSTANCE => inst_nodes.push(child),
                _ => {}
            }
        }

        let Some(class_node) = class_node else {
            dlog_error(1600, node, "No class node found");
            return;
        };
        if inst_nodes.is_empty() {
            dlog_error(1601, node, "No instance node found");
            return;
        }

        // Parse class name
        let Some(class_id_node) = class_node.get_sub_node() else {
            dlog_error(1602, node, "Missing class id node");
            return;
        };
        let Some(class_ids) = McIds::new(&class_id_node) else {
            dlog_error(1603, node, "Failed to parse class ids");
            return;
        };

        // Look up definition using mcb_get_cmie
        let cmie = mcb_get_cmie(&class_ids, uri);

        // ★ LSP: Register class reference for goto-definition
        let class_name = class_ids.to_string();
        let class_span = (class_id_node.get_pos() as usize)
            ..((class_id_node.get_pos() + class_id_node.get_len()) as usize);
        mcb_register_declare_class(uri, &class_name, class_span);

        // Parse all instances
        for inst_node in &inst_nodes {
            // MCAST_INSTANCE may have no children (e.g. "HDR_SINGLE A"), then instance name is the node's own content
            let inst_id_node = if let Some(sub) = inst_node.get_sub_node() {
                sub
            } else {
                inst_node.clone()
            };

            // If inst_id_node is MCAST_OPD, get its subnode for ids
            let ids_node = if inst_id_node.get_type() == MCAST_OPD {
                inst_id_node.get_sub_node().unwrap_or(inst_id_node.clone())
            } else {
                inst_id_node.clone()
            };
            let Some(inst_ids) = McIds::new(&ids_node) else {
                continue;
            };
            // ── P1 fix: array name expansion (with guard) ───────────────────────────
            // `cap[4:5]::CAP(1uF)`'s inst_ids is "cap[4:5]".
            // expand() expands to ["cap4", "cap5"]. Create a separate instance for each expanded name.
            //
            // Guard: only expand "array range with base prefix", exclude:
            //   - `[VDD_3V3, GND]::DC()` → is_square_only=true → not expand
            //   - `vin{POWER_SYS, GND}::DC()` → base="vin" but with curly brace → not expand
            //   - `MIC{P, N}::ADC.DIFF()` → same as above
            let expanded_names = inst_ids.expand();
            let inst_str = inst_ids.to_string();
            let has_square_range = inst_str.contains('[') && !inst_str.contains('{');
            let should_expand = !inst_ids.is_square_only()
                && !inst_ids.base_name().is_empty()
                && has_square_range
                && expanded_names.len() >= 2;
            let names_to_create: Vec<String> = if should_expand {
                expanded_names
            } else {
                vec![inst_str.clone()]
            };
            let base_name = inst_ids.base_name();

            // ── P1: collect this instance's construction args ──
            let ctor_args = collect_ctor_params(inst_node, &inst_id_node);
            // eprintln!(
            //     "[P1-ctor] inst='{}' ctor_args={} (id.next type={:?}, node.next type={:?})",
            //     inst_str,
            //     ctor_args.len(),
            //     inst_id_node.get_next().map(|n| n.get_type()),
            //     inst_node.get_next().map(|n| n.get_type()),
            // );

            for inst_name_ref in &names_to_create {
                let inst_name = inst_name_ref.clone();

                // ★ LSP: Register instance declaration symbol
                // Get the span of the instance name from ids_node
                let inst_span = (ids_node.get_pos() as usize)
                    ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                self.store_port_span(inst_name_ref, inst_span.clone());
                let scope = self.scope.as_deref();
                let decl_id = mcb_register_instance_decl(
                    uri,
                    inst_span.clone(),
                    Some(inst_name.clone()),
                    scope,
                );
                if let Some(id) = decl_id {
                    tracing::info!(target: "mcc::lsp", "Registered instance decl: {} at {:?} -> id={:?}", inst_name, inst_span, id);
                } else {
                    tracing::warn!(target: "mcc::lsp", "Failed to register instance decl: {}", inst_name);
                }

                // Check for NC parameter
                // MCAST_INSTANCE structure: instance_id (MCAST_PARAMS)?
                // MCAST_PARAMS children are MCAST_PARAM, MCAST_PARAM children may be MCAST_OPD_NC
                // Note: For instances without sub node, check if inst_node itself has next sibling
                let _is_nc = if let Some(next_sibling) = inst_node.get_next() {
                    if next_sibling.get_type() == MCAST_PARAMS {
                        if let Some(params_node) = next_sibling.get_sub_node() {
                            params_node.iter().any(|p| {
                                if p.get_type() == MCAST_PARAM {
                                    if let Some(param_child) = p.get_sub_node() {
                                        return param_child.get_type() == MCAST_OPD_NC;
                                    }
                                }
                                false
                            })
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Create McInstance based on definition
                // For Interface with curly bracket or square bracket syntax (e.g., DC4{VDD, GND}::DC(5V) or [VDD, GND]::DC(3.3V)),
                // use the appropriate name format for the instance
                // Collect instance parameters first (used by both Component and Module)
                let mut instance_params: Vec<McParamValue> = Vec::new();
                // Collect params from the params_node found during traversal
                if let Some(params) = &params_node {
                    if let Some(params_sub) = params.get_sub_node() {
                        for p in params_sub.iter() {
                            if p.get_type() == MCAST_PARAM {
                                if let Some(sub) = p.get_sub_node() {
                                    match sub.get_type() {
                                        MCAST_INT => {
                                            if let Some(int_val) = McInt::new(&sub) {
                                                instance_params.push(McParamValue::Int(int_val));
                                            }
                                        }
                                        MCAST_STRING => {
                                            let val = sub.to_string().unwrap_or_default();
                                            let clean_val = if val.starts_with('"')
                                                && val.ends_with('"')
                                                && val.len() >= 2
                                            {
                                                val[1..val.len() - 1].to_string()
                                            } else {
                                                val
                                            };
                                            instance_params.push(McParamValue::String(
                                                McString::from(clean_val.as_str()),
                                            ));
                                        }
                                        MCAST_OPD_NC => {
                                            instance_params
                                                .push(McParamValue::NC(String::from("NC")));
                                        }
                                        MCAST_ID | MCAST_IDA | MCAST_IDS => {
                                            if let Some(ids) = McIds::new(&sub) {
                                                if !ids.is_empty() {
                                                    instance_params.push(McParamValue::Ids(ids));
                                                }
                                            }
                                        }
                                        MCAST_OPD => {
                                            if let Some(inner) = sub.get_sub_node() {
                                                if let Some(ids) = McIds::new(&inner) {
                                                    if !ids.is_empty() {
                                                        instance_params
                                                            .push(McParamValue::Ids(ids));
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
                let (mc_inst, insert_key) = match &cmie {
                    Some(McCMIE::Component(comp_def)) => {
                        // ── P1: besides class-level value params (CAP(1uF…)), also merge instance-level construction args (flash(V3V3)) ──
                        let mut instance_params = instance_params;
                        instance_params.extend(ctor_args.clone());
                        let mc2_comp = Mc2Component::with_params(
                            &inst_name,
                            comp_def.clone(),
                            instance_params,
                        );
                        (McInstance::Component(Arc::new(mc2_comp)), inst_name)
                    }
                    Some(McCMIE::Module(mod_def)) => (
                        // ── P1: bring construction args into module instance ──
                        McInstance::Module(Arc::new(Mc2Module::with_params(
                            &inst_name,
                            mod_def.clone(),
                            ctor_args.clone(),
                        ))),
                        inst_name,
                    ),
                    Some(McCMIE::Interface(iface_def)) => {
                        // For Interface with square bracket syntax (e.g., [VDD, GND]::DC(3.3V)),
                        // create McIds with Square segment
                        if inst_ids.is_square_only() {
                            let members = inst_ids.expand();
                            let members_ids: Vec<IdsSegment> = members
                                .iter()
                                .map(|m| IdsSegment::Ida(Box::new(McIda::from(m.as_str()))))
                                .collect();
                            let ids_name = McIds {
                                segments: vec![IdsSegment::Square(members_ids)],
                            };
                            let port_name = format!("[{}]", members.to_vec().join(","));
                            (
                                McInstance::Interface(Arc::new(Mc2Interface::new(
                                    ids_name,
                                    iface_def.clone(),
                                ))),
                                port_name,
                            )
                        } else {
                            // Use base_name for Interface if it's a curly bracket expression
                            let iface_name = if base_name.is_empty() {
                                inst_name.clone()
                            } else {
                                base_name.clone()
                            };

                            // ★ FIX (Issue #1804/#1803):
                            // For `MIC{P, N}::ADC.DIFF()` style curly bracket interface
                            // port declaration, use `Mc2Interface::new(inst_ids.clone(), ...)` to preserve
                            // `{P, N}` user-declared bus members (in `name: McIds`).
                            // Previously used `new_with_str(&iface_name, ...)` only preserving base name "MIC",
                            // losing {P,N}, causing subsequent `MIC{P,N}` references
                            // validate_interface_member_ref can't find P/N in base.pins (empty).
                            let new_interface =
                                Mc2Interface::new(inst_ids.clone(), iface_def.clone());
                            if new_interface.pin_count() == 1 {
                                // Single-pin interface, check if same-name Interface already exists
                                if let Some((_existing_iotype, existing_inst)) =
                                    self.insts.get(&iface_name)
                                {
                                    if let McInstance::Interface(existing_iface) = existing_inst {
                                        if existing_iface.base_name() == new_interface.base_name() {
                                            // Merge into existing Interface
                                            let merged = existing_iface.merge_with(&new_interface);
                                            (McInstance::Interface(Arc::new(merged)), iface_name)
                                        } else {
                                            // Base interface name differs, register directly
                                            (
                                                McInstance::Interface(Arc::new(new_interface)),
                                                iface_name,
                                            )
                                        }
                                    } else {
                                        // Existing is not Interface, register directly
                                        (McInstance::Interface(Arc::new(new_interface)), iface_name)
                                    }
                                } else {
                                    // Same-name doesn't exist, register directly
                                    (McInstance::Interface(Arc::new(new_interface)), iface_name)
                                }
                            } else {
                                // Multi-pin interface, don't merge
                                (McInstance::Interface(Arc::new(new_interface)), iface_name)
                            }
                        }
                    }
                    _ => {
                        // Class definition not found in loaded scope (e.g. library not loaded).
                        // Keep as a named instance rather than downgrading to a plain label.
                        let class_name = class_ids.to_string();
                        dlog_warning(
                            1401,
                            &class_node,
                            &format!("unresolved class '{class_name}' — library not loaded?"),
                        );
                        (
                            McInstance::Unresolved {
                                class_name: class_name.clone(),
                            },
                            inst_name,
                        )
                    }
                };
                self.insts.insert(insert_key, (iotype.clone(), mc_inst));
            } // end for inst_name_ref in names_to_create
        }
    }

    /// Parse a single MCAST_OPD node (reference parameter like &dc24v, &GPIO[1:2])
    pub(crate) fn parse_opd(&mut self, node: &AstNode, iotype: IOType) {
        let Some(opd_node) = node.get_sub_node() else {
            return;
        };

        // Check if this is a DOT pattern (DC2.VDD)
        let child = opd_node.get_sub_node();
        let mut is_dot_pattern = false;
        let mut base_name = String::new();
        let mut dot_member = String::new();

        if let Some(first) = child {
            if first.get_type() == MCAST_ID {
                base_name = first.to_string().unwrap_or_default();
                if let Some(second) = first.get_next() {
                    if second.get_type() == MCAST_OPD_DOT {
                        is_dot_pattern = true;
                        if let Some(member_node) = second.get_sub_node() {
                            dot_member = member_node.to_string().unwrap_or_default();
                        }
                    }
                }
            }
        }

        if is_dot_pattern {
            // DC2.VDD - dot access pattern
            if let Some((existing_iotype, existing_port)) = self.insts.get(&base_name) {
                if let McInstance::Bus(bus) = existing_port {
                    let mut new_members = bus.member.clone();
                    if !new_members.contains(&dot_member) {
                        new_members.push(dot_member.clone());
                    }
                    self.insts.insert(
                        base_name.clone(),
                        (
                            existing_iotype.clone(),
                            McInstance::Bus(McBus::new_with_members(&base_name, new_members)),
                        ),
                    );
                    return;
                }
            }
            let members = vec![dot_member];
            self.insts.insert(
                base_name.clone(),
                (
                    iotype.clone(),
                    McInstance::Bus(McBus::new_with_members(&base_name, members)),
                ),
            );
            return;
        }

        // Normal IDS pattern handling
        match opd_node.get_type() {
            MCAST_IDS => {
                if let Some(pname) = McIds::new(&opd_node) {
                    if let Some((busname, members)) = pname.as_bus() {
                        let inst = if pname.is_curly_bracket() {
                            McInstance::Bus(McBus::new_with_members(&busname, members))
                        } else {
                            McInstance::List(McList::new_with_members(&busname, members))
                        };
                        self.insts.insert(busname.clone(), (iotype.clone(), inst));
                    } else if pname.is_square_only() {
                        let members = pname.expand();
                        let port_name = format!("@{}", self.insts.len());
                        self.insts.insert(
                            port_name.clone(),
                            (
                                iotype.clone(),
                                McInstance::List(McList::new_with_members(&port_name, members)),
                            ),
                        );
                    } else {
                        match pname.count() {
                            1 => {
                                self.insts.insert(
                                    pname.to_string(),
                                    (iotype.clone(), McInstance::Label(pname.to_string())),
                                );
                            }
                            2.. => {
                                let members = pname.expand();
                                for member in members {
                                    self.insts.insert(
                                        member.clone(),
                                        (iotype.clone(), McInstance::Label(member)),
                                    );
                                }
                            }
                            _ => {
                                dlog_error(1202, &opd_node, "Port name count error");
                            }
                        }
                    }
                }
            }
            _ => {
                dlog_error(1200, &opd_node, "Port name not support type");
            }
        }
    }

    /// Parse MCAST_OPD_SQUARE_VEC node (reference set like &[VDD1, GND1])
    pub(crate) fn parse_opd_square_vec(&mut self, node: &AstNode, iotype: IOType) {
        let mut children: Vec<AstNode> = Vec::new();
        let mut child = node.get_sub_node();
        while let Some(c) = child {
            children.push(c.clone());
            child = c.get_next();
        }

        let mut members: Vec<String> = Vec::new();
        let mut is_simple_bus = true;

        for child_node in &children {
            let actual_node = child_node
                .get_sub_node()
                .unwrap_or_else(|| child_node.clone());
            if let Some(pname) = McIds::new(&actual_node) {
                if pname.count() == 1 && !pname.is_square_only() {
                    members.push(pname.to_string());
                } else {
                    is_simple_bus = false;
                    break;
                }
            } else {
                is_simple_bus = false;
                break;
            }
        }

        if is_simple_bus && members.len() >= 2 {
            let port_name = format!("@{}", self.insts.len());
            self.insts.insert(
                port_name.clone(),
                (
                    iotype.clone(),
                    McInstance::List(McList::new_with_members(&port_name, members)),
                ),
            );
        } else {
            for child_node in &children {
                let Some(opd_node) = child_node.get_sub_node() else {
                    continue;
                };
                match opd_node.get_type() {
                    MCAST_IDS => {
                        if let Some(pname) = McIds::new(&opd_node) {
                            if let Some((busname, bus_members)) = pname.as_bus() {
                                self.insts.insert(
                                    busname.clone(),
                                    (
                                        iotype.clone(),
                                        McInstance::Bus(McBus::new_with_members(
                                            &busname,
                                            bus_members,
                                        )),
                                    ),
                                );
                            } else {
                                match pname.count() {
                                    1 => {
                                        self.insts.insert(
                                            pname.to_string(),
                                            (iotype.clone(), McInstance::Label(pname.to_string())),
                                        );
                                    }
                                    2.. => {
                                        let exp_members = pname.expand();
                                        for member in exp_members {
                                            self.insts.insert(
                                                member.clone(),
                                                (iotype.clone(), McInstance::Label(member)),
                                            );
                                        }
                                    }
                                    _ => {
                                        dlog_error(1203, &opd_node, "Port name count error");
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        dlog_error(1200, &opd_node, "Port name not support type");
                    }
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&McInstance> {
        self.insts.get(id).map(|(_, inst)| inst)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut McInstance> {
        self.insts.get_mut(id).map(|(_, inst)| inst)
    }

    pub fn get_with_iotype(&self, id: &str) -> Option<&(IOType, McInstance)> {
        self.insts.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &McInstance)> {
        self.insts.iter().map(|(k, (_, v))| (k.as_str(), v))
    }

    pub fn iter_with_iotype(&self) -> impl Iterator<Item = (&str, &(IOType, McInstance))> {
        self.insts.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub(crate) fn create(&mut self, id: &str, iotype: IOType, inst: McInstance) {
        if let Some(_existing) = self.insts.get(id) {
            return;
        }
        self.insts.insert(id.to_string(), (iotype, inst));
    }

    pub(crate) fn create_inst(&mut self, id: &str, inst: McInstance) {
        // Default to None IOType for internal instances
        self.create(id, IOType::None, inst);
    }

    /// Find port by name
    pub fn find_port(&self, name: &str) -> Option<&McInstance> {
        self.insts.get(name).map(|(_, port)| port)
    }

    /// Get all input ports
    pub fn inputs(&self) -> Vec<&McInstance> {
        self.insts
            .values()
            .filter(|(io, _)| matches!(io, IOType::In))
            .map(|(_, p)| p)
            .collect()
    }

    /// Get all output ports
    pub fn outputs(&self) -> Vec<&McInstance> {
        self.insts
            .values()
            .filter(|(io, _)| matches!(io, IOType::Out))
            .map(|(_, p)| p)
            .collect()
    }

    /// Get all bidirectional ports
    pub fn bidirs(&self) -> Vec<&McInstance> {
        self.insts
            .values()
            .filter(|(io, _)| matches!(io, IOType::InOut))
            .map(|(_, p)| p)
            .collect()
    }

    /// Get all input ports (including bidirectional)
    pub fn get_all_inputs(&self) -> Vec<&McInstance> {
        self.insts
            .values()
            .filter(|(io, _)| matches!(io, IOType::In) || matches!(io, IOType::InOut))
            .map(|(_, p)| p)
            .collect()
    }

    /// Get all output ports (including bidirectional)
    pub fn get_all_outputs(&self) -> Vec<&McInstance> {
        self.insts
            .values()
            .filter(|(io, _)| matches!(io, IOType::Out) || matches!(io, IOType::InOut))
            .map(|(_, p)| p)
            .collect()
    }

    /// Get all ports
    pub fn get_all_ports(&self) -> Vec<&McInstance> {
        self.insts.values().map(|(_, port)| port).collect()
    }

    /// Check if empty interface
    pub fn is_empty(&self) -> bool {
        self.insts.is_empty()
    }

    /// Get all input ports, return (name, port) pair
    pub fn inputs_with_name(&self) -> Vec<(&str, &McInstance)> {
        self.insts
            .iter()
            .filter(|(_, (io, _))| matches!(io, IOType::In))
            .map(|(name, (_, port))| (name.as_str(), port))
            .collect()
    }

    /// Get all output ports, return (name, port) pair
    pub fn outputs_with_name(&self) -> Vec<(&str, &McInstance)> {
        self.insts
            .iter()
            .filter(|(_, (io, _))| matches!(io, IOType::Out))
            .map(|(name, (_, port))| (name.as_str(), port))
            .collect()
    }

    /// Get all bidirectional ports, return (name, port) pair
    pub fn bidirs_with_name(&self) -> Vec<(&str, &McInstance)> {
        self.insts
            .iter()
            .filter(|(_, (io, _))| matches!(io, IOType::InOut))
            .map(|(name, (_, port))| (name.as_str(), port))
            .collect()
    }

    /// Get all power ports, return (name, port) pair
    pub fn powers_with_name(&self) -> Vec<(&str, &McInstance)> {
        self.insts
            .iter()
            .filter(|(_, (io, _))| matches!(io, IOType::Power))
            .map(|(name, (_, port))| (name.as_str(), port))
            .collect()
    }

    /// Get port's IOType
    pub fn get_iotype(&self, name: &str) -> Option<&IOType> {
        self.insts.get(name).map(|(io, _)| io)
    }

    /// Get all instance names
    pub fn get_all_names(&self) -> Vec<String> {
        self.insts.keys().cloned().collect()
    }
}

impl From<McInstance> for McPhrase {
    fn from(value: McInstance) -> Self {
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(value)))
    }
}

// ============================================================================
// Display implementation - concise format output
// ============================================================================

impl std::fmt::Display for McInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McInstance::Component(c) => write!(f, "Component:{c}"),
            McInstance::Module(m) => write!(f, "Module:{}", m.name),
            McInstance::Label(name) => write!(f, "{name}"),
            McInstance::Bus(bus) => {
                if bus.full_members.is_empty() {
                    write!(f, "{}", bus.name)
                } else {
                    let members = bus.full_members.to_vec().join(",");
                    write!(f, "{}{{{}}}", bus.name, members)
                }
            }
            McInstance::BusRef { component, bus } => {
                write!(f, "{component}.{bus}")
            }
            McInstance::List(list) => {
                let members = list.member.to_vec().join(",");
                write!(f, "{}[{}]", list.name, members)
            }
            McInstance::Interface(i) => write!(f, "{i:?}"),
            McInstance::Unresolved { class_name } => write!(f, "?{class_name}"),
        }
    }
}

impl std::fmt::Display for McInstances {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let instances: Vec<String> = self
            .insts
            .iter()
            .map(|(name, (_, inst))| format!("{name}:{inst}"))
            .collect();
        write!(f, "{}", instances.join(", "))
    }
}
