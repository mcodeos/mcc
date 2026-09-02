// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::{macros::*, node::AstNode};
use crate::db::context::DB;
use crate::db::diagnostic::diagnostic::{dlog_error, dlog_warning};

use crate::query::refs::mcb_register_declare_class;
use crate::refdef::types::{ChainSegment, SymbolKind};
use crate::semantic::basic::mc_bus::{McBus, McList};
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_ida::McIda;
use crate::semantic::basic::mc_ids::{IdsSegment, McIds};
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue, ParamBindError};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::IOType;
use crate::semantic::component::Mc2Component;
use crate::semantic::context::resolve_cmie;
use crate::semantic::mc_func::HasFindInst;
use crate::semantic::mc_ifs::Mc2Interface;
use crate::semantic::module::Mc2Module;
use crate::McAttrVal;
use crate::McCMIE;
use crate::McFunction;
use crate::McURI;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::Arc;

/// ── P1: Collect constructor arguments from MCAST_INSTANCE (parenthesized arguments of mcu(V3V3,V1V2) / flash(V3V3)).
/// Arguments are attached inside the instance node, as the next sibling MCAST_PARAMS of id (mc_inst.rs:854 comment);
/// some forms are attached to the next sibling of the instance node, try both places, take the first non-empty.
/// Each argument is parsed by the canonical context-free value parser
/// (McParamValue::new_no_ctx, mc_param.rs) — the literal dispatch that used
/// to live here (INT / STRING / NC / UVALUE / identifier) is centralized there.
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
        let out: Vec<McParamValue> = psub
            .iter()
            .filter(|p| p.get_type() == MCAST_PARAM)
            .filter_map(|p| McParamValue::new_no_ctx(&p))
            .collect();
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

/// ★ §3.4.3 (rev): a named curly-bus definition, registered as a whole with
/// per-member precise spans. This is the "independent Rust class" the LSP
/// lookup model is built on: `MIC` resolves to the whole [`BusDef`], `MIC.P`
/// resolves to the member span. Populated in `parse_opd`, expanded into
/// `BusMemberDef` lapper entries by `lapper_module_ports`.
#[derive(Debug, Clone, Default)]
pub struct BusDef {
    /// Bus name (e.g. "MIC").
    pub name: String,
    /// Whole-bus span — covers the base identifier `MIC`, not the braces.
    pub span: Range<usize>,
    /// Member name → precise span at its declaration text (e.g. `P` in `{P,N}`).
    pub members: Vec<(String, Range<usize>)>,
}

impl BusDef {
    pub fn member_span(&self, member: &str) -> Option<Range<usize>> {
        self.members
            .iter()
            .find(|(name, _)| name == member)
            .map(|(_, span)| span.clone())
    }
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
    /// "pins" keyword — transparent under pins transparency rules,
    /// but preserved for explicit index-based access (e.g. `uC.pins[8]`).
    Pins,
    /// Physical pin ID — a key in the component's pin_id_to_names table,
    /// resolved during chain lookup (e.g. `uC.19`, `uC.W1`).
    PinId(String),
    /// Component attribute value (e.g. `partno`, `package`).
    Attr(McAttrVal),
    /// Function reference — returned by `find_inst` for func names.
    Func(Arc<McFunction>),
    /// Scoped enum value — returned when a component's family enum value
    /// is looked up by name (e.g. `X7R` from `enum CAP`).
    EnumVal {
        /// The enum definition name (e.g. "CAP").
        enum_name: String,
        /// The enum value name (e.g. "X7R").
        value_name: String,
        /// Source span of the value definition, for LSP goto-def.
        span: Option<Range<usize>>,
        /// RefDefMap class id of the enum class in its defining file, used to
        /// locate the value definition precisely (packed value id =
        /// class_id + value index). None when the class is not registered in
        /// the referencing file's RefDefMap.
        class_id: Option<u32>,
        /// URI of the file that defines the enum class (e.g. "CAP").
        def_uri: Option<String>,
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
            Pins => "pins".to_string(),
            PinId(id) => id.clone(),
            Attr(a) => a.to_string(),
            Func(f) => f.name.to_string(),
            EnumVal { value_name, .. } => value_name.clone(),
        }
    }

    /// Get member list (Bus/List member names; e.g. `MIC{P,N}` → `["P","N"]`).
    pub fn members(&self) -> Vec<String> {
        match self {
            McInstance::Bus(b) => b.member.clone(),
            McInstance::List(l) => l.member.clone(),
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
            McInstance::Pins => McBus::new("pins"),
            McInstance::PinId(id) => McBus::new(id),
            McInstance::Attr(a) => McBus::new(&a.to_string()),
            McInstance::Func(f) => McBus::new(&f.name.to_string()),
            McInstance::EnumVal { value_name, .. } => McBus::new(value_name),
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
            | McInstance::Unresolved { .. }
            | McInstance::Pins => true,
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
            McInstance::Pins => "Pins",
            McInstance::PinId(_) => "PinId",
            McInstance::Attr(_) => "Attr",
            McInstance::Func(_) => "Func",
            McInstance::EnumVal { .. } => "EnumVal",
        }
    }

    /// Unified member resolution across container types (Phase 3.1).
    ///
    /// Given a member name, dispatches to the container's [`HasFindInst::find_inst`]
    /// implementation. Supports Component (pin members), Interface (interface pins),
    /// Bus (bus members), and Module (inst members).
    ///
    /// Returns `None` if the member is not found or the container type doesn't
    /// support member resolution.
    pub fn resolve_member(&self, member: &str) -> Option<McInstance> {
        match self {
            McInstance::Component(c) => c.base.find_inst(member),
            McInstance::Module(m) => m.base.find_inst(member),
            McInstance::Interface(i) => i.base.find_inst(member),
            McInstance::Bus(b) => {
                // Bus members are resolved by name within the member list
                if b.member.iter().any(|m| m == member) {
                    Some(McInstance::Label(member.to_string()))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// McInstances - Symbol table for instances and ports within a module
///
/// Stores all identifiers within module: (IOType, McInstance) mapping
#[derive(Debug, Clone)]
pub struct McInstances {
    insts: BTreeMap<String, (IOType, McInstance)>,
    /// ★ §11.2 vector groups: vector base name -> ordered member names
    /// (`"c" -> ["c1","c2"]`), recorded at parse_declare expansion time so the
    /// declaration no longer erases vector information. Members still land in
    /// `insts` as ordinary entries; this is the grouping overlay. Scoped per
    /// `McInstances` (module vs function scope keep vector bases isolated).
    /// Contract E: a single-member range is NOT registered (it is a scalar).
    vectors: BTreeMap<String, Vec<String>>,
    /// Port spans for LSP goto-definition (name -> span ranges, multiple for DOT patterns)
    port_spans: HashMap<String, Vec<Range<usize>>>,
    /// LSP: spans in module body that reference port definitions (span, port_name)
    net_ref_spans: Vec<(Range<usize>, String, String)>, // (span, port_name, scope)
    /// LSP: AST-structured chain references (span, segments, scope).
    /// Used for cross-container member resolution without text-based re-parsing.
    chain_ref_spans: Vec<(Range<usize>, Vec<ChainSegment>, String)>,
    /// ★ LSP: Enclosing scope name (module/component/function name)
    pub(crate) scope: Option<String>,
    /// Label kind registry: tracks whether a label is Explicit (declared) or Inline (net phrase).
    label_kinds: HashMap<String, LabelKind>,
    /// ★ §3.4.3 (rev): named curly-bus definitions with per-member spans
    /// (bus name → whole bus + member spans). Single source of truth for
    /// member-level goto-def; expanded by lapper_module_ports.
    bus_defs: BTreeMap<String, BusDef>,
    /// ★ Declareb kind hints (`idx::CLASS(...)` inference rule): a 2-pin
    /// declareb (`C4::CAP()`, `R1::RES(1kOhm)`) bypasses `parse_declare`
    /// (it is parsed as a FuncCall for Pass2 transpose/NC semantics), so the
    /// name never enters `insts` and the lapper cannot classify it from the
    /// instance table. The parse-time hint records the def kind inferred from
    /// the class (Component/Module → `InstDef`; Interface → label/bus) and the
    /// declaration span; `lapper_module_ports` registers `InstDef` at that
    /// span and `resolve_net_ref_kind` answers `InstRef` for use sites.
    declareb_defs: HashMap<String, (SymbolKind, Range<usize>)>,
}

impl McInstances {
    pub(crate) fn new() -> Self {
        Self {
            insts: BTreeMap::new(),
            vectors: BTreeMap::new(),
            port_spans: HashMap::new(),
            net_ref_spans: Vec::new(),
            chain_ref_spans: Vec::new(),
            scope: None,
            label_kinds: HashMap::new(),
            bus_defs: BTreeMap::new(),
            declareb_defs: HashMap::new(),
        }
    }

    /// Read the ordered member set of a declared vector group, if any.
    /// Returns `None` for a non-vector (or scalar single-member) base name.
    pub fn get_vector_members(&self, base: &str) -> Option<&[String]> {
        self.vectors.get(base).map(Vec::as_slice)
    }

    /// §11.2: iterate all declared vector groups (base name → ordered member
    /// names) for pass2 `McVectorInst` materialization. Written order preserved.
    pub fn vector_groups(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.vectors.iter()
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

    /// Whether `name` is a declared port / label: header ports and `label`
    /// statements carry a real IOType (In/Out/InOut/Power/Analog/Return/
    /// Label), while names created inline by a connection phrase have
    /// IOType::None. `verify`/hierarchy use this to tell the declared origin
    /// of a label/bus/interface instance from the inline one.
    pub fn is_port_io_type(&self, name: &str) -> bool {
        matches!(
            self.insts.get(name).map(|(t, _)| t),
            Some(
                IOType::In
                    | IOType::Out
                    | IOType::InOut
                    | IOType::Power
                    | IOType::Analog
                    | IOType::Return
                    | IOType::Label
            )
        )
    }

    pub fn contains(&self, name: &str) -> bool {
        self.insts.contains_key(name)
    }

    /// Iterate all ports (instances with IOType != None/Return/NonCon)
    pub fn iter_ports(&self) -> impl Iterator<Item = (&str, &IOType)> {
        self.insts
            .iter()
            .filter(|(_, (io_type, _))| {
                !matches!(
                    io_type,
                    IOType::None | IOType::Return | IOType::NonCon | IOType::Label
                )
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
                !matches!(
                    io_type,
                    IOType::None | IOType::Return | IOType::NonCon | IOType::Label
                )
            })
            .filter_map(|(name, (_, inst))| match inst {
                McInstance::BusRef { .. }
                | McInstance::Component(_)
                | McInstance::Module(_)
                | McInstance::Unresolved { .. } => None,
                _ => Some(name),
            })
    }

    /// Access the raw insts table for diagnostics.
    pub fn insts(&self) -> &BTreeMap<String, (IOType, McInstance)> {
        &self.insts
    }

    /// §7.5: Unified idx-aware name resolution.
    /// Given a reference name (e.g. "GPIO1", "rs485.A", "DC1"),
    /// find the matching definition key (e.g. "GPIO[1:2]", "rs485{A,B}", "DC1{VDD,GND}").
    /// Returns None if no matching def key is found.
    pub fn resolve_idx(&self, ref_name: &str) -> Option<String> {
        if self.port_spans.contains_key(ref_name) {
            return Some(ref_name.to_string());
        }
        self.iter_instance_names()
            .find(|k| self.all_name_forms_for(k).contains(&ref_name.to_string()))
            .cloned()
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
                McInstance::Interface(iface) => {
                    // Square `[VDD_3V3, GND]::DC(3.3V)` members are referenced
                    // as bare members (`VDD_3V3`, `GND`); curly
                    // `vin{POWER_SYS, GND}::DC(5V)` as `vin.POWER_SYS`.
                    forms.extend(iface.name.expand());
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

    /// ★ §3.4.3 (rev): register a whole curly-bus definition with member spans.
    pub(crate) fn register_bus_def(
        &mut self,
        name: &str,
        span: Range<usize>,
        members: Vec<(String, Range<usize>)>,
    ) {
        self.bus_defs.insert(
            name.to_string(),
            BusDef {
                name: name.to_string(),
                span,
                members,
            },
        );
    }

    /// Get a registered bus def (whole + member spans).
    pub fn bus_def(&self, name: &str) -> Option<&BusDef> {
        self.bus_defs.get(name)
    }

    /// Iterate all registered bus defs.
    pub fn iter_bus_defs(&self) -> impl Iterator<Item = &BusDef> {
        self.bus_defs.values()
    }

    /// Record a declareb kind hint. First registration wins: the declaration
    /// is the first typed occurrence of the name; later same-name declareb is
    /// a use/ref, not a second def.
    pub(crate) fn record_declareb_def(&mut self, name: &str, kind: SymbolKind, span: Range<usize>) {
        self.declareb_defs
            .entry(name.to_string())
            .or_insert((kind, span));
    }

    /// Look up the declareb hint for a name (inferred def kind + declaration span).
    pub fn declareb_def(&self, name: &str) -> Option<(SymbolKind, Range<usize>)> {
        self.declareb_defs
            .get(name)
            .map(|(kind, span)| (*kind, span.clone()))
    }

    /// Iterate all declareb hints.
    pub fn iter_declareb_defs(
        &self,
    ) -> impl Iterator<Item = (&String, &(SymbolKind, Range<usize>))> {
        self.declareb_defs.iter()
    }

    /// Iterate all ports with their spans (multiple entries per key for DOT patterns)
    pub fn iter_ports_with_span(&self) -> impl Iterator<Item = (&str, &IOType, Range<usize>)> + '_ {
        self.insts
            .iter()
            .filter(|(_, (io_type, _))| {
                !matches!(
                    io_type,
                    IOType::None | IOType::Return | IOType::NonCon | IOType::Label
                )
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

    /// Iterate all labels (explicit and inline) with their spans in
    /// deterministic source order (span start, then name).
    /// Labels are instances with IOType::None that have stored port spans.
    ///
    /// `port_spans` is a HashMap; its iteration order varies run-to-run (each
    /// process seeds its own RandomState). Registration loops that mint fresh
    /// DeclareIds must not depend on that order, or symbol ids shuffle between
    /// runs.
    pub fn iter_labels_with_span(
        &self,
    ) -> impl Iterator<Item = (&str, LabelKind, Range<usize>)> + '_ {
        let mut items: Vec<(&str, LabelKind, Range<usize>)> = Vec::new();
        for (name, spans) in &self.port_spans {
            // Only include entries that are Label instances (not ports/buses/components)
            if matches!(
                self.insts.get(name).map(|(_, inst)| inst),
                Some(McInstance::Label(_))
            ) {
                let kind = self.get_label_kind(name);
                for span in spans {
                    items.push((name.as_str(), kind, span.clone()));
                }
            }
        }
        items.sort_by(|a, b| (a.2.start, a.0).cmp(&(b.2.start, b.0)));
        items.into_iter()
    }

    /// Iterate `port_spans` entries in deterministic source order (first span
    /// start, then name). Registration loops that register defs per
    /// port_spans key (e.g. lapper building) must iterate this instead of the
    /// raw HashMap so DeclareId allocation order is stable across runs.
    pub fn iter_port_spans_sorted(&self) -> impl Iterator<Item = (&str, &Vec<Range<usize>>)> + '_ {
        let mut items: Vec<(&str, &Vec<Range<usize>>)> = self
            .port_spans
            .iter()
            .map(|(name, spans)| (name.as_str(), spans))
            .collect();
        items.sort_by_key(|(name, spans)| {
            (spans.first().map(|s| s.start).unwrap_or(usize::MAX), *name)
        });
        items.into_iter()
    }

    /// Record a net-line reference to a port definition (for LSP goto-definition)
    pub(crate) fn record_net_ref(&mut self, span: Range<usize>, port_name: &str, scope: &str) {
        self.net_ref_spans
            .push((span, port_name.to_string(), scope.to_string()));
    }

    pub fn iter_net_refs(&self) -> impl Iterator<Item = &(Range<usize>, String, String)> {
        self.net_ref_spans.iter()
    }

    /// Record an AST-structured chain reference (for cross-container member resolution).
    pub(crate) fn record_chain_ref(
        &mut self,
        span: Range<usize>,
        segments: Vec<ChainSegment>,
        scope: &str,
    ) {
        self.chain_ref_spans
            .push((span, segments, scope.to_string()));
    }

    pub fn iter_chain_refs(
        &self,
    ) -> impl Iterator<Item = &(Range<usize>, Vec<ChainSegment>, String)> {
        self.chain_ref_spans.iter()
    }

    /// Find a name within comma-separated text and return its byte span
    /// relative to `base_offset`.
    fn find_name_in_text(text: &str, name: &str, base_offset: usize) -> Range<usize> {
        let text_start = text.as_ptr() as usize;
        let bytes = text.as_bytes();
        let mut pos: usize = 0;
        let n = bytes.len();
        while pos < n {
            // Skip leading whitespace
            while pos < n && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            // Find end of this identifier (comma or end)
            let start = pos;
            while pos < n && bytes[pos] != b',' {
                pos += 1;
            }
            let part = &text[start..pos];
            let trimmed = part.trim();
            // Extract base name (before { or [)
            let base = trimmed
                .split(|c: char| c == '{' || c == '[')
                .next()
                .unwrap_or(trimmed);
            if base == name {
                let base_start = base.as_ptr() as usize - text_start;
                return (base_offset + base_start)..(base_offset + base_start + base.len());
            }
            // Skip past comma
            if pos < n && bytes[pos] == b',' {
                pos += 1;
            }
        }
        // Fallback
        base_offset..base_offset + text.len()
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
                                    // parse_declare already stores per-instance spans for the
                                    // inserted keys, so no span is stored here. Only mark
                                    // explicit Label kind for `label ...` declares.
                                    let before: Vec<String> = self.insts.keys().cloned().collect();
                                    self.parse_declare(&child, uri, iotype_ref);
                                    let new_keys: Vec<String> = self
                                        .insts
                                        .keys()
                                        .filter(|k| !before.contains(k))
                                        .cloned()
                                        .collect();
                                    for k in new_keys {
                                        if matches!(iotype_ref, IOType::Label) {
                                            self.set_label_kind(&k, LabelKind::Explicit);
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
                                        // Compute per-name spans for precise F12 jump
                                        let opd_text = child.to_string().unwrap_or_default();
                                        for k in new_keys {
                                            let name_span =
                                                Self::find_name_in_text(&opd_text, &k, span.start);
                                            self.store_port_span(&k, name_span);
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
            dlog_error(
                crate::errcodes::INST_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(crate::errcodes::INST_MISSING_SUBNODE, &[]),
            );
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
                                        // Curly-bus port (e.g. `MIC{P, N}`): the port span
                                        // covers the base identifier `MIC` only, not the raw
                                        // OPD node span `MIC{P, N` (mc_value_link extends the
                                        // first child's len, dropping the closing brace). This
                                        // keeps F12 on the base landing on the name text.
                                        let port_span = if pname.is_curly_bracket() {
                                            opd_node
                                                .get_sub_node()
                                                .filter(|n| n.get_type() == MCAST_ID)
                                                .map(|n| {
                                                    let p = n.get_pos() as usize;
                                                    p..(p + busname.len())
                                                })
                                                .unwrap_or_else(|| {
                                                    let p = span.start;
                                                    p..(p + busname.len())
                                                })
                                        } else {
                                            span.clone()
                                        };
                                        self.store_port_span(&busname, port_span);
                                    }
                                    if pname.is_square_only() {
                                        let members = pname.expand();
                                        let next_node = opd_node.get_next();
                                        let mut interface_name: Option<McIds> = None;
                                        let mut interface_span: Option<std::ops::Range<usize>> =
                                            None;
                                        // Collect `::DC(3.3V)` ctor args from the PARAMS
                                        // sibling of the DBCOLON node, so the interface
                                        // carries its construction parameters.
                                        let mut ctor_params: Vec<McParamValue> = Vec::new();
                                        if let Some(n) = next_node {
                                            if n.get_type() == MCAST_OPD_DBCOLON {
                                                if let Some(sub) = n.get_sub_node() {
                                                    // McIds straight from the AST node — no
                                                    // flattened-string rebuild.
                                                    interface_name = McIds::new(&sub);
                                                    interface_span = Some(
                                                        (sub.get_pos() as usize)
                                                            ..((sub.get_pos() + sub.get_len())
                                                                as usize),
                                                    );
                                                }
                                                if let Some(pn) = n.get_next() {
                                                    if pn.get_type() == MCAST_PARAMS {
                                                        ctor_params = collect_ctor_params(&pn, &pn);
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(iface_ids) = &interface_name {
                                            // ★ LSP: Register class reference for goto-definition on
                                            // ::Interface() syntax in port declarations
                                            // (e.g., ps [VDD, GND]::DC(3.3V)).
                                            if let Some(ref span) = interface_span {
                                                mcb_register_declare_class(
                                                    uri,
                                                    iface_ids,
                                                    span.clone(),
                                                );
                                            }
                                            if let Some(McCMIE::Interface(iface_def)) =
                                                resolve_cmie(&DB, iface_ids, uri)
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
                                                let port_name = ids_name.to_string();
                                                let mc_inst = McInstance::Interface(Arc::new(
                                                    Mc2Interface::with_ids_and_params(
                                                        ids_name,
                                                        iface_def.clone(),
                                                        ctor_params,
                                                    ),
                                                ));
                                                self.insts.insert(
                                                    port_name.clone(),
                                                    (iotype.clone(), mc_inst),
                                                );
                                                self.store_port_span(&port_name, span);
                                            } else {
                                                dlog_error(
                                                    crate::errcodes::IFACE_BUS_NOT_FOUND,
                                                    &opd_node,
                                                    &crate::errcodes::format_msg(
                                                        crate::errcodes::IFACE_BUS_NOT_FOUND,
                                                        &[
                                                            &iface_ids as &dyn std::fmt::Display,
                                                            &pname as &dyn std::fmt::Display,
                                                            &members.to_vec().join(",")
                                                                as &dyn std::fmt::Display,
                                                        ],
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
                                                    crate::errcodes::PORT_NAME_COUNT_ERROR,
                                                    &opd_node,
                                                    &crate::errcodes::format_msg(
                                                        crate::errcodes::PORT_NAME_COUNT_ERROR,
                                                        &[],
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {
                                dlog_error(
                                    crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED,
                                    &opd_node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED,
                                        &[],
                                    ),
                                );
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
                                                            crate::errcodes::PORT_NAME_COUNT_ERROR,
                                                            &opd_node,
                                                            &crate::errcodes::format_msg(
                                                                crate::errcodes::PORT_NAME_COUNT_ERROR,
                                                                &[],
                                                            ),
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        dlog_error(
                                            crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED,
                                            &opd_node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED,
                                                &[],
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }
        } else {
            dlog_error(
                crate::errcodes::MALFORMED_IOTYPE,
                &subnode,
                &crate::errcodes::format_msg(crate::errcodes::MALFORMED_IOTYPE, &[]),
            );
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

    pub(crate) fn parse_declare(&mut self, node: &AstNode, uri: &McURI, iotype: &IOType) {
        // MCAST_DECLARE structure:
        // |- MCAST_CLASS (class_id, class_params)
        // |- MCAST_INSTANCE (instance_id, instance_params)
        // |- MCAST_INSTANCE (instance_id, instance_params) ... (multiple instances)
        let Some(sub) = node.get_sub_node() else {
            dlog_error(
                crate::errcodes::NAME_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(crate::errcodes::NAME_MISSING_SUBNODE, &[]),
            );
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
            dlog_error(
                crate::errcodes::INST_CLASS_NODE_MISSING,
                node,
                &crate::errcodes::format_msg(crate::errcodes::INST_CLASS_NODE_MISSING, &[]),
            );
            return;
        };
        if inst_nodes.is_empty() {
            dlog_error(
                crate::errcodes::INST_NODE_MISSING,
                node,
                &crate::errcodes::format_msg(crate::errcodes::INST_NODE_MISSING, &[]),
            );
            return;
        }

        // Parse class name
        let Some(class_id_node) = class_node.get_sub_node() else {
            dlog_error(
                crate::errcodes::INST_CLASS_ID_MISSING,
                node,
                &crate::errcodes::format_msg(crate::errcodes::INST_CLASS_ID_MISSING, &[]),
            );
            return;
        };
        let Some(class_ids) = McIds::new(&class_id_node) else {
            dlog_error(
                crate::errcodes::INST_CLASS_IDS_PARSE_FAILED,
                node,
                &crate::errcodes::format_msg(crate::errcodes::INST_CLASS_IDS_PARSE_FAILED, &[]),
            );
            return;
        };

        // Look up definition using mcb_get_cmie
        let cmie = resolve_cmie(&DB, &class_ids, uri);

        // ★ LSP: Register class reference for goto-definition
        let class_span = (class_id_node.get_pos() as usize)
            ..((class_id_node.get_pos() + class_id_node.get_len()) as usize);
        mcb_register_declare_class(uri, &class_ids, class_span);

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
            //   - interface bindings → never expand: `PWR_[VDD2, GND2]::DC(5V)` is a
            //     single named interface instance (`PWR_` with members VDD2/GND2), and
            //     the interface branch keys every expanded name by `base_name`, which
            //     would register the same span once per member (duplicate-instance
            //     diagnostics / duplicate LSP symbols).
            let expanded_names = inst_ids.expand();
            let inst_str = inst_ids.to_string();
            // AST-driven (§0): `inst_str` has a `[` exactly when some segment is a
            // square (outer Square or embedded Ida square), and a `{` exactly when
            // some outer segment is a Curly — so the old string predicate
            // `contains('[') && !contains('{')` is exactly `has_square() &&
            // !has_curly()`, read off the segment tree instead of display output.
            let has_square_range = inst_ids.has_square() && !inst_ids.has_curly();
            let is_interface = matches!(cmie, Some(McCMIE::Interface(_)));
            // A named base with a square range (`c[1:2]`, `res[4]`) — excludes
            // square-only lists (`[VDD,GND]::DC()`), curlies and interface binds.
            let is_named_square_range = !is_interface
                && !inst_ids.is_square_only()
                && !inst_ids.base_name().is_empty()
                && has_square_range;
            // Vector guard (contract E, §11.3): only ≥2 expanded members is a
            // vector group; a single-member range (`res[4]`) is a scalar member
            // (`res4`), materialized by the expanded name instead of a literal
            // `res[4]` instance (invariant B: no literal bracket path).
            let should_expand = is_named_square_range && expanded_names.len() >= 2;
            let names_to_create: Vec<String> = if should_expand {
                expanded_names
            } else if is_named_square_range && expanded_names.len() == 1 {
                expanded_names
            } else {
                vec![inst_str.clone()]
            };
            let base_name = inst_ids.base_name();
            if should_expand {
                self.vectors
                    .insert(base_name.to_string(), names_to_create.clone());
            }

            // ── P1: collect this instance's construction args ──
            let ctor_args = collect_ctor_params(inst_node, &inst_id_node);

            for inst_name_ref in &names_to_create {
                let inst_name = inst_name_ref.clone();

                // ★ LSP: Register instance declaration symbol
                // The ids node len may be unreliable: mc_value_link extends the
                // first child's len (e.g. `wm7121(NC` for `wm7121(NC)`), and for
                // curly buses the node may span `MIC{P, N` while the parsed name
                // text is longer. Clamp to the parsed text, and for curly buses
                // cover the base identifier only (decision: F12 on the base
                // jumps to the name text).
                let inst_span = if inst_ids.is_curly_bracket() {
                    let base = inst_ids.base_name();
                    ids_node
                        .get_sub_node()
                        .filter(|n| n.get_type() == MCAST_ID)
                        .map(|n| {
                            let p = n.get_pos() as usize;
                            p..(p + base.len())
                        })
                        .unwrap_or_else(|| {
                            let p = ids_node.get_pos() as usize;
                            p..(p + base.len())
                        })
                } else {
                    let inst_len = (ids_node.get_len() as usize).min(inst_str.len());
                    (ids_node.get_pos() as usize)..((ids_node.get_pos() as usize) + inst_len)
                };
                // The span for the inserted key is stored below (after the
                // instance kind is resolved), because the inserted key may
                // differ from inst_name (e.g. square interface `[VDD,GND]`).
                let scope = self.scope.as_deref();
                // ★ Try WORKSPACE first, fall back to direct register_def
                // register under the real (file_id, container_id,
                // func_id) scope — not the all-zero SourceLocation::from_span —
                // so the parse-time InstRef id equals the lapper-time InstDef id.
                let _ = crate::db::cmie::tables::WORKSPACE
                    .mcodes
                    .get(uri)
                    .and_then(|mcode| {
                        mcode.symbols.lock().ok().map(|mut sem| {
                            crate::refdef::register::register_instance_decl_parse_time(
                                &mut sem,
                                uri,
                                scope,
                                &inst_name,
                                inst_span.clone(),
                            )
                        })
                    });

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
                // Collect params from the params_node found during traversal;
                // each argument is parsed by the canonical context-free value
                // parser (McParamValue::new_no_ctx, mc_param.rs) so named
                // attribute blocks `{ cap = 1uF; volt = 50V }` and every other
                // literal form are handled uniformly.
                if let Some(params) = &params_node {
                    if let Some(params_sub) = params.get_sub_node() {
                        for p in params_sub.iter() {
                            if p.get_type() == MCAST_PARAM {
                                if let Some(val) = McParamValue::new_no_ctx(&p) {
                                    instance_params.push(val);
                                }
                            }
                        }
                    }
                }
                let (mc_inst, insert_key) = match &cmie {
                    Some(McCMIE::Component(comp_def)) => {
                        mcc_dbg!("sem::inst", 
                            "[P2-4-PARSE] inst='{inst_name}' class='{class_ids}' -> Component (cmie=Component)",
                        );
                        // ── P1: besides class-level value params (CAP(1uF…)), also merge instance-level construction args (flash(V3V3)) ──
                        let mut instance_params = instance_params;
                        instance_params.extend(ctor_args.clone());
                        // Report binding failures at the instance site: an
                        // orphan named argument (`{ nope = 5 }`), an excess
                        // positional argument, or a type mismatch hard-fail
                        // the signature bind. A missing required parameter is
                        // silent (Component-Spec Separation) — the value comes
                        // from spec / the BOM and the instance is still
                        // created with the supplied arguments.
                        if !instance_params.is_empty() {
                            if let Err(e) =
                                McParamBindings::bind(comp_def.bind_params(), &instance_params)
                            {
                                // Missing required parameters never block
                                // instance creation: circuit topology only
                                // needs pins, and the value comes from spec /
                                // the BOM. Silent in dev mode, reported as a
                                // warning (E4178) in strict mode.
                                // Written-but-wrong arguments (excess /
                                // unknown / type-mismatched) are errors
                                // (E4176).
                                if let ParamBindError::MissingRequired { name } = e {
                                    if crate::cli::strict_mode() {
                                        dlog_warning(
                                            crate::errcodes::INST_PARAM_MISSING_REQUIRED,
                                            inst_node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::INST_PARAM_MISSING_REQUIRED,
                                                &[&inst_name, &comp_def.name.to_string(), &name],
                                            ),
                                        );
                                    }
                                } else {
                                    dlog_error(
                                        crate::errcodes::INST_PARAM_BIND_FAILED,
                                        inst_node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::INST_PARAM_BIND_FAILED,
                                            &[
                                                &inst_name,
                                                &comp_def.name.to_string(),
                                                &format!("{e}"),
                                            ],
                                        ),
                                    );
                                }
                            }
                        }
                        let mc2_comp = Mc2Component::with_params(
                            &inst_name,
                            comp_def.clone(),
                            instance_params,
                        );
                        (McInstance::Component(Arc::new(mc2_comp)), inst_name)
                    }
                    Some(McCMIE::Module(mod_def)) => {
                        mcc_dbg!("sem::inst",
                            "[P2-4-PARSE] inst='{inst_name}' class='{class_ids}' -> Module (cmie=Module)",
                        );
                        (
                            // ── P1: bring construction args into module instance ──
                            McInstance::Module(Arc::new(Mc2Module::with_params(
                                &inst_name,
                                mod_def.clone(),
                                ctor_args.clone(),
                            ))),
                            inst_name,
                        )
                    }
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
                            let port_name = ids_name.to_string();
                            (
                                McInstance::Interface(Arc::new(Mc2Interface::with_ids_and_params(
                                    ids_name,
                                    iface_def.clone(),
                                    instance_params.clone(),
                                ))),
                                port_name,
                            )
                        } else {
                            // ★ LSP: register a whole BusDef for curly interface
                            // ports (`io vin{POWER_SYS, GND}::DC(5V)`) so member
                            // refs `vin.GND` / `vin.POWER_SYS` resolve to the
                            // member name text in THIS file, not the interface
                            // class definition (dc.mc). Mirrors the named-curly
                            // bus logic in `parse_opd`.
                            if let Some((busname, members)) = inst_ids.as_bus() {
                                if !members.is_empty() {
                                    let whole_span = ids_node
                                        .get_sub_node()
                                        .filter(|n| n.get_type() == MCAST_ID)
                                        .map(|n| {
                                            let p = n.get_pos() as usize;
                                            p..(p + busname.len())
                                        })
                                        .unwrap_or_else(|| {
                                            let p = ids_node.get_pos() as usize;
                                            p..(p + busname.len())
                                        });
                                    let mut member_spans: Vec<(String, Range<usize>)> = Vec::new();
                                    let mut cur = ids_node.get_sub_node();
                                    while let Some(child) = cur {
                                        if matches!(
                                            child.get_type(),
                                            MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN
                                        ) {
                                            let mut mc = child.get_sub_node();
                                            while let Some(m) = mc {
                                                if let Some(mname) = m.to_string() {
                                                    let mstart = m.get_pos() as usize;
                                                    let mlen = mname.len();
                                                    member_spans
                                                        .push((mname, mstart..(mstart + mlen)));
                                                }
                                                mc = m.get_next();
                                            }
                                        }
                                        cur = child.get_next();
                                    }
                                    if !member_spans.is_empty() {
                                        self.register_bus_def(&busname, whole_span, member_spans);
                                    }
                                }
                            }
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
                            // The `::DC(5V)` ctor args are passed through so the
                            // interface param count / conditional pins match the call site.
                            let new_interface = Mc2Interface::with_ids_and_params(
                                inst_ids.clone(),
                                iface_def.clone(),
                                instance_params.clone(),
                            );
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
                            crate::errcodes::INST_CLASS_UNRESOLVED,
                            &class_node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::INST_CLASS_UNRESOLVED,
                                &[&class_name],
                            ),
                        );
                        (
                            McInstance::Unresolved {
                                class_name: class_name.clone(),
                            },
                            inst_name,
                        )
                    }
                };
                self.insts
                    .insert(insert_key.clone(), (iotype.clone(), mc_inst));
                self.store_port_span(&insert_key, inst_span.clone());
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
                        // ★ §3.4.3 (rev): named curly bus `MIC{P,N}` registers a
                        //   whole BusDef with per-member precise spans, so lookup
                        //   `MIC` → whole bus, `MIC.P` → member text. Member node
                        //   pos is reliable; len is NOT (mc_value_link extends the
                        //   first child's len), so spans use pos + name length.
                        if pname.is_curly_bracket() {
                            // Whole-bus span covers the base identifier `MIC`
                            // (decision: F12 on the base jumps to the name text).
                            let whole_span = opd_node
                                .get_sub_node()
                                .filter(|n| n.get_type() == MCAST_ID)
                                .map(|n| {
                                    let p = n.get_pos() as usize;
                                    p..(p + busname.len())
                                })
                                .unwrap_or_else(|| {
                                    let p = opd_node.get_pos() as usize;
                                    p..(p + busname.len())
                                });
                            let mut member_spans: Vec<(String, Range<usize>)> = Vec::new();
                            let mut cur = opd_node.get_sub_node();
                            while let Some(child) = cur {
                                if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN)
                                {
                                    let mut mc = child.get_sub_node();
                                    while let Some(m) = mc {
                                        if let Some(mname) = m.to_string() {
                                            let mstart = m.get_pos() as usize;
                                            let mlen = mname.len();
                                            member_spans.push((mname, mstart..(mstart + mlen)));
                                        }
                                        mc = m.get_next();
                                    }
                                }
                                cur = child.get_next();
                            }
                            self.register_bus_def(&busname, whole_span, member_spans);
                        }
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
                                dlog_error(
                                    crate::errcodes::PORT_NAME_COUNT_ERROR,
                                    &opd_node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::PORT_NAME_COUNT_ERROR,
                                        &[],
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            _ => {
                dlog_error(
                    crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED,
                    &opd_node,
                    &crate::errcodes::format_msg(crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED, &[]),
                );
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
                                        dlog_error(
                                            crate::errcodes::PORT_NAME_COUNT_ERROR,
                                            &opd_node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::PORT_NAME_COUNT_ERROR,
                                                &[],
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        dlog_error(
                            crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED,
                            &opd_node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::PORT_NAME_TYPE_UNSUPPORTED,
                                &[],
                            ),
                        );
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
            McInstance::Unresolved { class_name } => write!(f, "Unresolved({class_name})"),
            McInstance::Pins => write!(f, "pins"),
            McInstance::PinId(id) => write!(f, "pin:{id}"),
            McInstance::Attr(a) => write!(f, "{a}"),
            McInstance::Func(func) => write!(f, "Func:{}", func.name),
            McInstance::EnumVal {
                enum_name,
                value_name,
                ..
            } => {
                write!(f, "{enum_name}.{value_name}")
            }
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
