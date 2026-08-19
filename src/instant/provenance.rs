// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 instantiation — expansion provenance (logical expansion tree)
//!
//! Records every instantiation / expansion event produced by the Pass2
//! instantiator, so that downstream consumers (verify, `mcc show`, LSP) can
//! attribute products (components, sub-modules, connections) to the exact
//! call site and function body line that created them — without re-deriving
//! the call tree from flat physical structures.
//!
//! Design: `mcd/doc/expansion-provenance.md` (logical expansion tree).
//!
//! Each `McModuleInst` owns one `ExpansionLog`; record indices (`expansion_id`)
//! are **module-local** and must not be referenced across modules — cross-module
//! links go through `ExpansionRecord::sub_target` (sub-module instance path).

use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_mod::McModuleInst;
use crate::instant::mc_net::ConnectionInst;
use crate::McURI;

/// Kind of an expansion / instantiation event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionKind {
    /// `uC.i2c()` / `mcu.i2c()` — component / sub-module method (body expansion).
    InstanceMethod,
    /// Module-level `func` call — `instantiate_user_func` (body expansion).
    UserFunc,
    /// Module-level zero-arg `func` auto invocation (body expansion, call_site = None).
    AutoInvoke,
    /// `declare A` statement instantiation (leaf record, call_site = port_span).
    Declare,
    /// Bare `Class(...)` / `NAME::TYPE()` / declareb `C4::CAP()` (leaf record).
    ComponentCtor,
    /// Inline module call (parent-side leaf record; body expands inside sub-module).
    ModuleCall,
    /// `.Cap()` / `.Pullup()` / `.Pulldown()` (body expansion).
    BuiltinTwopin,
    /// Iterated call `cap[4:5]::CAP()` / iterated func (leaf for component arrays).
    Iterated,
}

impl ExpansionKind {
    /// Stable snake-case name used in output (JSON `kind` field, tree text).
    pub fn name(self) -> &'static str {
        match self {
            Self::InstanceMethod => "instance_method",
            Self::UserFunc => "user_func",
            Self::AutoInvoke => "auto_invoke",
            Self::Declare => "declare",
            Self::ComponentCtor => "component_ctor",
            Self::ModuleCall => "module_call",
            Self::BuiltinTwopin => "builtin_twopin",
            Self::Iterated => "iterated",
        }
    }
}

/// One expansion / instantiation event recorded during Pass2.
#[derive(Debug, Clone)]
pub struct ExpansionRecord {
    /// Kind of this expansion event.
    pub kind: ExpansionKind,
    /// `this` binding: full scope path of the caller instance (e.g. "mcu513",
    /// "mcu.uC"). None for auto-invoked module funcs.
    pub caller_inst: Option<String>,
    /// Called function name (last segment of a chained call like "uC.i2c").
    pub func_name: String,
    /// Call site as absolute byte offset: top-level call = statement span
    /// start; nested call = function-body line offset. None for auto-invoked
    /// module funcs (no user call statement).
    pub call_site: Option<(McURI, u32)>,
    /// Function definition site (source file + definition offset).
    pub def_site: Option<(McURI, u32)>,
    /// Nesting parent: None = top-level call (grouped into a statement node by
    /// `build_tree`); Some(idx) = a call re-issued during this expansion.
    pub parent: Option<usize>,
    /// Nested child record indices (expansions re-issued during this one).
    pub children: Vec<usize>,
    /// Deliberate empty-expansion marker (sub-module method P2-8 skip).
    pub skipped: bool,
    /// Cross-module sub-module method: sub-module instance path (§7.3).
    pub sub_target: Option<String>,
}

impl ExpansionRecord {
    /// Create a new expansion record.
    pub fn new(
        kind: ExpansionKind,
        caller_inst: Option<String>,
        func_name: String,
        call_site: Option<(McURI, u32)>,
        def_site: Option<(McURI, u32)>,
        parent: Option<usize>,
    ) -> Self {
        Self {
            kind,
            caller_inst,
            func_name,
            call_site,
            def_site,
            parent,
            children: Vec::new(),
            skipped: false,
            sub_target: None,
        }
    }
}

/// Call statement node: verify-stage aggregation unit.
///
/// Built by `ExpansionLog::build_tree` from top-level records sharing the same
/// `call_site` (statement span start, absolute offset). Records with
/// `call_site = None` (auto-invoke) are skipped and attach to the module node.
#[derive(Debug, Clone)]
pub struct StatementNode {
    /// Statement span start (absolute offset).
    pub call_site: (McURI, u32),
    /// Statement source text (for display; empty until filled by verify).
    pub text: String,
    /// Top-level expansion record indices belonging to this statement.
    pub expansions: Vec<usize>,
}

/// Expansion log owned by each `McModuleInst` (module-local id space).
#[derive(Debug, Clone, Default)]
pub struct ExpansionLog {
    /// Flat log of expansion records; `parent` / `children` link the tree.
    pub records: Vec<ExpansionRecord>,
    /// Stack of active expansion indices (top-level vs nested determination).
    stack: Vec<usize>,
}

impl ExpansionLog {
    /// Enter an expansion: link to the stack top (parent) and push onto the
    /// stack. Returns the record index used as `expansion_id` for products.
    pub fn begin(
        &mut self,
        kind: ExpansionKind,
        caller_inst: Option<String>,
        func_name: String,
        call_site: Option<(McURI, u32)>,
        def_site: Option<(McURI, u32)>,
    ) -> usize {
        let idx = self.records.len();
        let parent = self.stack.last().copied();
        self.records.push(ExpansionRecord::new(
            kind,
            caller_inst,
            func_name,
            call_site,
            def_site,
            parent,
        ));
        if let Some(p) = parent {
            self.records[p].children.push(idx);
        }
        self.stack.push(idx);
        idx
    }

    /// Exit an expansion: pop the stack. Product tagging already happened at
    /// push time via `current_id`, so nothing else needs to be rolled back.
    pub fn end(&mut self, idx: usize) {
        debug_assert_eq!(self.stack.last(), Some(&idx), "expansion end out of order");
        self.stack.pop();
    }

    /// Current active expansion id (for tagging products at push time).
    pub fn current_id(&self) -> Option<usize> {
        self.stack.last().copied()
    }

    /// Mark a record as deliberately empty (P2-8 skip; not a `no_expansion` bug).
    pub fn mark_skipped(&mut self, idx: usize) {
        if let Some(r) = self.records.get_mut(idx) {
            r.skipped = true;
        }
    }

    /// Link a record to a sub-module instance path (cross-module expansion).
    pub fn set_sub_target(&mut self, idx: usize, path: String) {
        if let Some(r) = self.records.get_mut(idx) {
            r.sub_target = Some(path);
        }
    }

    /// Build the statement-node view: group top-level records by `call_site`.
    ///
    /// Records with `call_site = None` (auto-invoke) are not statement-level;
    /// they attach to the module node and are excluded here. Nodes are sorted
    /// by `call_site` offset so the tree mirrors source order (§7.5).
    pub fn build_tree(&self) -> Vec<StatementNode> {
        let mut nodes: Vec<StatementNode> = Vec::new();
        for (i, r) in self.records.iter().enumerate() {
            if r.parent.is_some() {
                continue; // nested expansion, not a statement-level node
            }
            let Some(call_site) = r.call_site.clone() else {
                continue; // auto-invoke: no user call statement
            };
            match nodes.iter_mut().find(|n| n.call_site == call_site) {
                Some(n) => n.expansions.push(i),
                None => nodes.push(StatementNode {
                    call_site,
                    text: String::new(),
                    expansions: vec![i],
                }),
            }
        }
        nodes.sort_by_key(|n| (n.call_site.0.clone(), n.call_site.1));
        nodes
    }

    /// Group one module's products by expansion id (§5.4).
    ///
    /// Components / sub-modules / connections are bucketed by their
    /// `expansion_id`: `Some(k)` → the direct products of record `k`;
    /// `None` → module top-level statements (no active expansion).
    /// Nested products tag the innermost record, so each record's group holds
    /// only its own direct products — no range subtraction is needed.
    pub fn group_products(
        &self,
        components: &[McComponentInst],
        sub_modules: &[McModuleInst],
        connections: &[ConnectionInst],
    ) -> ProductGroups {
        let mut groups = ProductGroups {
            by_record: (0..self.records.len())
                .map(|_| ProductGroup::default())
                .collect(),
            top_level: ProductGroup::default(),
        };
        for (i, c) in components.iter().enumerate() {
            match c.expansion_id {
                Some(k) if k < groups.by_record.len() => groups.by_record[k].components.push(i),
                None => groups.top_level.components.push(i),
                Some(_) => {} // stale id (records never removed today); ignore
            }
        }
        for (i, m) in sub_modules.iter().enumerate() {
            match m.expansion_id {
                Some(k) if k < groups.by_record.len() => groups.by_record[k].sub_modules.push(i),
                None => groups.top_level.sub_modules.push(i),
                Some(_) => {}
            }
        }
        for (i, c) in connections.iter().enumerate() {
            match c.expansion_id {
                Some(k) if k < groups.by_record.len() => groups.by_record[k].connections.push(i),
                None => groups.top_level.connections.push(i),
                Some(_) => {}
            }
        }
        groups
    }
}

/// Direct product set of one expansion record: indices into the owning
/// module's `components` / `sub_modules` / `connections` whose `expansion_id`
/// equals the record's index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductGroup {
    /// Component instance indices (into `McModuleInst.components`).
    pub components: Vec<usize>,
    /// Sub-module instance indices (into `McModuleInst.sub_modules`).
    pub sub_modules: Vec<usize>,
    /// Connection indices (into `McModuleInst.connections`).
    pub connections: Vec<usize>,
}

/// Product attribution for one module (§5.4): per-record groups plus the
/// module-top-level bucket for products created outside any expansion.
#[derive(Debug, Clone, Default)]
pub struct ProductGroups {
    /// Per-record direct products, index-aligned with `ExpansionLog.records`.
    pub by_record: Vec<ProductGroup>,
    /// Products with `expansion_id = None` — module top-level statements.
    pub top_level: ProductGroup,
}
