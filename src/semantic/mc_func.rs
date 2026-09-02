// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::{dlog_error, dlog_warning};
use crate::semantic::basic::mc_bus::{McBus, McList};
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::component::Mc2Component;
use crate::semantic::mc_inst::McInstance;
use crate::semantic::module::Mc2Module;
use crate::McIds;
use crate::McInstances;
use crate::{
    ast::error::message::*,
    ast::macros::*,
    ast::node::AstNode,
    semantic::basic::form::{classify, reference_parts, Form, RefVerdict},
    semantic::basic::mc_param::McParamDeclares,
};

// ============================================================================
// McFuncReturn — function return-value kind (parse-time)
// ============================================================================

/// Function return-value kind, decided at parse time from the body's `return`
/// statement (if any). Used by the call-site parser to validate method chains.
///
/// Chainability rules:
///   * `Implicit` — no explicit `return`; backwards-compatible default that
///     behaves like `return this`. Chainable.
///   * `This` — explicit `return this`. Chainable.
///   * `Endpoint(_)` — explicit `return <bus|label|expr>`. **Not** chainable;
///     the result is a value/endpoint, not the receiver, so `.next_method()`
///     after it is a hard error.
#[derive(Debug, Clone, Default)]
pub enum McFuncReturn {
    /// No explicit `return` statement.
    #[default]
    Implicit,
    /// Explicit `return this`.
    This,
    /// Explicit `return <expr>` where the expression resolves to a label/bus
    /// or any other non-`this` phrase.
    Endpoint(McPhrase),
}

impl McFuncReturn {
    /// Whether the return value supports continued method chaining.
    pub fn is_chainable(&self) -> bool {
        matches!(self, McFuncReturn::Implicit | McFuncReturn::This)
    }

    /// Short tag for diagnostics ("implicit"/"this"/"endpoint").
    pub fn kind_str(&self) -> &'static str {
        match self {
            McFuncReturn::Implicit => "implicit",
            McFuncReturn::This => "this",
            McFuncReturn::Endpoint(_) => "endpoint",
        }
    }
}

/// A Phase 1 entry-gate (resolve-gate-design.md §1.3) candidate: the base name of a
/// structured dot access was not declared in scope at parse time, so the ghost
/// bus was suppressed (the statement produced no net). The component-finish
/// recheck re-resolves the base against the final symbol table: a late-declared
/// instance dismisses the candidate (`resolved_late`), a still-missing name
/// errors with [`INSTANCE_REF_UNDECLARED`](crate::errcodes::INSTANCE_REF_UNDECLARED).
#[derive(Debug, Clone)]
pub struct GateCandidate {
    /// The undeclared base name (e.g. `uC` in `uC.ADC.P`).
    pub base: String,
    /// The full chain as written (e.g. `uC.ADC.P`).
    pub form: String,
    /// Source position of the failing reference (for the error span).
    pub pos: u32,
    pub len: u32,
}

/// Trait for types that can provide instance lookup for symbol resolution
pub trait HasFindInst {
    fn find_inst(&self, id: &str) -> Option<McInstance>;
    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance>;

    /// Read the ordered member set of a declared vector group
    /// (`c[1:2]` → `["c1","c2"]`), §11.2/§11.3 ③. The scope chain is the same
    /// as `find_inst` (pin 3): FuncBodyContext delegates to its parent,
    /// McFunction/McModule read their own `McInstances.vectors`. Returns
    /// `None` for a non-vector base or a scalar single-member base (contract E).
    fn get_vector_members(&self, base: &str) -> Option<Vec<String>> {
        let _ = base;
        None
    }

    /// Primary name lookup method: search by priority chain and return both the
    /// semantic instance and its source span (for LSP goto-definition).
    ///
    /// Default implementation delegates to [`find_inst`] with a `None` span.
    /// Implementors should override this to provide accurate source spans.
    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        self.find_inst(id).map(|inst| (inst, None))
    }
    /// Add a label, optionally recording its source span for LSP goto-def.
    fn add_label(&mut self, name: String) -> Option<McPhrase> {
        self.add_label_at(name, None)
    }
    /// Add a label with a known source span.
    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase>;
    fn add_component(
        &mut self,
        name: String,
        comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase>;
    fn add_module(
        &mut self,
        name: String,
        module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase>;
    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase>;
    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase>;
    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase>;
    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase>;
    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)>;
    fn is_component_bus(&self, base: &str, member: &str) -> bool;
    fn upgrade_label_to_bus(&mut self, name: &str) -> bool;
    fn uri(&self) -> &crate::McURI;
    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance>;
    fn gen_anon_name(&mut self, classname: &str) -> String;

    /// True when `name` is a declared port of the enclosing scope (an `io` /
    /// `in` / `out` declaration carrying a concrete IOType), as opposed to an
    /// internal label or function parameter.
    ///
    /// The Pass1 vector-circuit opcheck uses this to apply the shape-by-use
    /// rule (§8.9.6.3): a scalar-declared port is not shape-locked at the
    /// declaration site — its width is inferred from the connection context,
    /// so the port reference must present an unknown (empty) shape instead of
    /// a fixed 1*1. Internal labels keep their declared single-point shape.
    ///
    /// Hook invoked when a bare identifier in a net statement does not resolve
    /// to any instance in scope. Default: no-op. The func-body context
    /// overrides this to warn about floating labels.
    fn report_floating_label(&self, _name: &str, _node: &AstNode) {}

    /// Phase 1 entry-gate discriminator (resolve-gate-design.md §1.4/v1.17): true
    /// when `base` is an instance name established in the enclosing scope —
    /// a declared instance, func-local declare, or FuncCall caller/instance
    /// name — as opposed to a genuine miss (typo / forgotten declaration).
    /// The ghost-bus fallback consults this to decide pass (keep the ghost bus,
    /// defer to §3 materialization, no error) vs true miss (suppress + register
    /// candidate + error after the component-finish recheck).
    ///
    /// Default: `find_inst` only — correct for scopes that never see caller
    /// names or func-local declares (McInterface/McEnumDef). The func-body
    /// context additionally consults func-local declares and noted caller
    /// names; `McModule` consults module body caller names.
    fn is_declared_instance_name(&self, base: &str) -> bool {
        self.find_inst(base).is_some()
    }

    /// Note that `name` was established as an instance reference in the current
    /// body — a FuncCall caller / inline-constructed instance (e.g.
    /// `TTL.D dTrigger.Cap()` → `dTrigger`, `PL3085A(powerSupply) PL.Cap()` →
    /// `PL`). Such names are not registered in any insts table — they become
    /// caller Label phrases — yet they are legitimate instance references that
    /// must pass the Phase 1 gate. Default no-op.
    fn note_func_call_caller(&mut self, _name: &str) {}

    /// Register a Phase 1 gate candidate: `base` was the undeclared base of a
    /// structured dot access whose ghost-bus is inlined (relax-everything — the statement
    /// is kept). The component-finish recheck warns E3137 if the inline net is
    /// still referenced only once, and balances late-declared refs (§5 item 23:
    /// inlining is what can join two rails through a shared ghost net — the R03
    /// short check catches that). Default no-op.
    fn register_gate_candidate(&mut self, _base: &str, _form: &str, _pos: u32, _len: u32) {}

    /// Phase 2 entry — the single reference-resolution entry (§1.2②, plan step 3):
    /// classify the reference form, derive base/member, and run the Phase 1
    /// miss decision once for every gate site (mc_phrase.rs A/B/C/D). What it
    /// converges is the *miss action* — relax-everything: the phantom ghost-bus is kept
    /// and inlined (no E3182), the gate candidate is registered (for the finish
    /// recheck's E3137 single-use warning / late-resolution balance), and the
    /// caller adds the bus. Found-base handling (E1802 member validation,
    /// `add_bus_member`, LSP registration, member fall-through) and the
    /// `as_component_member` branch stay at each site.
    ///
    /// Default impl is scope-agnostic; it inherits the scope discriminator via
    /// the [`is_declared_instance_name`] override (FuncBodyContext / McModule
    /// add caller names and func-local declares).
    ///
    /// Site string is intentionally bare (`"gate undeclared base (E3182)"`, no
    /// `<path>:<line>` prefix): `normalize_site` passes it through unchanged, so
    /// goldens stay stable across code-line drift.
    fn resolve_reference(&mut self, ids: &McIds, pos: u32, len: u32) -> RefVerdict {
        let form = classify(ids);
        // Bare / List are the legitimate net cases (§1.3 ②/③) — the four gate
        // sites never hand them here (they classify to structured forms before
        // reaching the miss decision), so this is a defensive guard.
        if matches!(form, Form::Bare | Form::List) {
            return RefVerdict::Wire;
        }
        // ★ Vector arm (§11.3 ③): prefix+square references (`c[1:2]`, `res[4]`)
        // resolve against the declared vector groups / member set, so they no
        // longer fall to the literal `add_label` fall-through. Combinatorial:
        // the outer vector comes from `vectors[base]` (same scope chain as
        // `find_inst`, pin 3), and each member is a scalar entry.
        if matches!(form, Form::Array | Form::Indexed) {
            if let Some(expanded) = crate::semantic::basic::equivalent::member_set(ids) {
                if expanded.len() >= 2 {
                    let base = ids.base_name();
                    // (a) Declared vector group, an array alias whose members are
                    // all individually declared instances, or a func-local vector
                    // (members visible via `is_declared_instance_name` — func.insts
                    // is not on the body scope chain, §11.3 pin 3) →
                    // multi-member.
                    if self.get_vector_members(&base).is_some()
                        || expanded.iter().all(|m| self.find_inst(m).is_some())
                        || expanded.iter().all(|m| self.is_declared_instance_name(m))
                    {
                        return RefVerdict::ResolvedMany(expanded);
                    }
                } else if self.find_inst(&expanded[0]).is_some() {
                    // (b) Contract E: single-member range (`res[4]`) whose member
                    // is a declared scalar → scalar reference, not a vector.
                    return RefVerdict::Resolved;
                }
            }
            // True miss (array base declared nowhere) — fall through to the
            // existing scalar-miss decision (E3136/Wire twin, no sibling-probing).
        }
        let (base, member) = reference_parts(ids, form);
        if self.find_inst(&base).is_some() {
            return RefVerdict::Resolved;
        }
        if self.is_declared_instance_name(&base) {
            // B-family pass: base is a declared instance name in scope — keep
            // the ghost-bus, defer to §3 materialization (observable gate
            // behavior unchanged; only the dispatch moves here).
            return RefVerdict::Deferred;
        }
        // True miss (relax-everything): the base is declared nowhere. The ghost-bus is
        // inlined at the call site (no E3182; the net layer decides via the
        // E3137 single-use warning / R03 short check). Register the candidate
        // so the finish recheck can still balance late-declared refs and warn
        // on single-use inline nets.
        self.register_gate_candidate(&base, &ids.to_string(), pos, len);
        RefVerdict::UnresolvedRef { base, member }
    }

    /// Default implementation returns `false` (not a port).
    fn is_declared_port(&self, _name: &str) -> bool {
        false
    }

    /// Authoritative declared member set of a module io/out/in port named
    /// `base`, or `None` when `base` is not such a port.
    ///
    /// `Some(vec![])`      = a scalar-declared port (no members) — any
    ///                       member/lane access is E3183
    ///                       (BUS_MEMBER_ON_SCALAR_PORT).
    /// `Some(members)`     = a membered port (`io X{A, B}` / `io X[A, B]`);
    ///                       a referenced member outside this set is E3181
    ///                       (BUS_MEMBER_UNDECLARED).
    /// `None`              = internal net, func param, component pin, module
    ///                       instance, interface port, etc. — usage-defined
    ///                       (shape by use), never gated.
    ///
    /// Default: `None`. `McModule` reads its own `insts` IOType table (a module
    /// port carries a concrete `io`/`in`/`out` IOType); `FuncBodyContext`
    /// delegates to its parent so func params stay shape-by-use.
    fn declared_port_members(&self, _base: &str) -> Option<Vec<String>> {
        None
    }

    /// Authoritative declared-shape gate (E3183 / E3181): when `base` is a
    /// declared module io/out/in port, validate the member/lane access
    /// `members` against its declaration and emit exactly one error — E3183
    /// for a scalar-declared port, E3181 for an undeclared member on a
    /// membered port. Returns `true` iff `base` is such a declared port, so the
    /// caller can skip the usage auto-expansion (never widen a declared port).
    fn enforce_declared_port_shape(
        &self,
        base: &str,
        members: &[String],
        access_text: &str,
        node: &AstNode,
    ) -> bool {
        let Some(declared) = self.declared_port_members(base) else {
            return false;
        };
        if declared.is_empty() {
            crate::db::diagnostic::diagnostic::dlog_error(
                crate::errcodes::BUS_MEMBER_ON_SCALAR_PORT,
                node,
                &crate::errcodes::format_msg(
                    crate::errcodes::BUS_MEMBER_ON_SCALAR_PORT,
                    &[&base, &access_text],
                ),
            );
        } else {
            let missing: Vec<&String> = members
                .iter()
                .filter(|m| !declared.iter().any(|d| d == *m))
                .collect();
            if !missing.is_empty() {
                let missing_str = missing
                    .iter()
                    .map(|m| m.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                crate::db::diagnostic::diagnostic::dlog_error(
                    crate::errcodes::BUS_MEMBER_UNDECLARED,
                    node,
                    &crate::errcodes::format_msg(
                        crate::errcodes::BUS_MEMBER_UNDECLARED,
                        &[&base, &missing_str, &format!("{:?}", declared)],
                    ),
                );
            }
        }
        true
    }

    /// Member names of an interface-class module parameter whose base name
    /// matches `name` (e.g. `dc{VDD_3V3, GND}::DC(3.3V)` → `["VDD_3V3", "GND"]`
    /// for `name = "dc"`). Interface-class params are registered in the module
    /// param table only (not `insts`), so `find_inst` cannot see them; the
    /// Pass1 opcheck uses this to present the declared bus width of a bare
    /// param reference, mirroring Pass2's `expand_port_lanes` upgrade. Returns
    /// `None` when `name` is not such a param (or has fewer than 2 members).
    fn interface_param_members(&self, _name: &str) -> Option<Vec<String>> {
        None
    }

    /// Record the source span for an already-created instance.
    /// Used so diagnostics on anonymous instances (e.g. `@CAP2`) can point at
    /// the actual usage position instead of the enclosing module start.
    /// Default implementation is a no-op for contexts that don't track spans.
    fn store_inst_span(&mut self, _name: &str, _span: std::ops::Range<usize>) {}

    /// ★ Record a declareb kind hint (`idx::CLASS(...)` inference): a 2-pin
    /// declareb (`C4::CAP()`) is parsed as a FuncCall and bypasses
    /// `parse_declare`, so the name never enters `insts`. The hint carries the
    /// def kind inferred from the class (Component/Module → `InstDef`) and the
    /// declaration span; the lapper uses it to classify the name as an
    /// instance instead of a label. Default implementation is a no-op for
    /// contexts that don't track instances.
    fn record_declareb_def(
        &mut self,
        _name: &str,
        _kind: crate::refdef::types::SymbolKind,
        _span: std::ops::Range<usize>,
    ) {
    }

    /// Look up a user-defined function in the surrounding scope and report
    /// its return kind. Used by [`McFuncCall`] to validate method chains.
    ///
    /// The default implementation returns `None`, meaning "no function with
    /// that name is visible in this scope". `McModule` / `McComponent` should
    /// override this to delegate into their own `funcs` table, e.g.:
    /// ```ignore
    /// fn find_func_return(&self, name: &str) -> Option<McFuncReturn> {
    ///     self.funcs.find(name).map(|f| f.returns.clone())
    /// }
    /// ```
    fn find_func_return(&self, _name: &str) -> Option<McFuncReturn> {
        None
    }

    /// Return the enclosing scope name (module/component/function name),
    /// or None for file-level scope.
    fn scope_name(&self) -> Option<String> {
        None
    }
}

/// Composite context for func body parsing: first searches func params,
/// then falls back to the parent (module/component) for module-level instances.
struct FuncBodyContext<'a> {
    /// Single-name func params (exact-match scope for `find_inst`). Bracket
    /// members (`[net1, net2]`) are NOT included here — `get_primary_name`
    /// returns None for a Multiple — so they fall through to the label path
    /// and are filtered out again after the body loop via `self.params`.
    param_names: &'a [String],
    /// Bare identifiers that failed `find_inst` (name + source pos/len),
    /// collected during the body loop and filtered afterwards against the
    /// func's own `params` (member-aware) and `insts` (func-local declares).
    /// A `RefCell` avoids the borrow conflict with `self.insts.parse`
    /// mutating the same table mid-loop.
    pending_floating: &'a std::cell::RefCell<Vec<(String, u32, u32)>>,
    /// Names established as instance references in this func body: FuncCall
    /// caller names (`TTL.D dTrigger.Cap()` → `dTrigger`, noted via
    /// `note_func_call_caller`) plus func-local declares registered into
    /// `func.insts` during the loop (noted by `parse_body`). The Phase 1
    /// ghost-bus discriminator passes these (resolve-gate §1.4).
    seen_callers: &'a std::cell::RefCell<Vec<String>>,
    /// Phase 1 gate candidates: structured dot-access bases that were not
    /// declared in scope at parse time, so their phantom bus was suppressed.
    /// Drained into `McFunction.gate_candidates` after the body loop.
    gate_candidates: &'a std::cell::RefCell<Vec<GateCandidate>>,
    parent: &'a mut dyn HasFindInst,
}

impl<'a> HasFindInst for FuncBodyContext<'a> {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance> {
        self.parent.find_inst_mut(id)
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        // instance_chain (§3.4): P1 func params → P2 parent container chain.
        // Func params shadow the parent's same-named instances (param wins).
        crate::semantic::scope::instance_chain(self.param_names, &*self.parent)
            .resolve(id)
            .map(|r| (r.inst, r.span))
    }

    fn get_vector_members(&self, base: &str) -> Option<Vec<String>> {
        // Same scope chain as `find_inst`: the parent (module/function) holds
        // the vector groups; func-local declares live in the parent's `insts`.
        self.parent.get_vector_members(base)
    }

    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        self.parent.add_label_at(name, span)
    }

    fn add_component(
        &mut self,
        name: String,
        comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase> {
        self.parent.add_component(name, comp)
    }

    fn add_module(
        &mut self,
        name: String,
        module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        self.parent.add_module(name, module)
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        self.parent.add_bus(name, members)
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        self.parent.add_list(name, members)
    }

    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase> {
        self.parent.add_bus_member(base, member)
    }

    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase> {
        self.parent
            .add_interface_member(component, interface, members)
    }

    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)> {
        self.parent.check_bus_member(base, member)
    }

    fn is_component_bus(&self, base: &str, member: &str) -> bool {
        self.parent.is_component_bus(base, member)
    }

    fn upgrade_label_to_bus(&mut self, name: &str) -> bool {
        self.parent.upgrade_label_to_bus(name)
    }

    fn is_declared_port(&self, name: &str) -> bool {
        // Func params are bound at the call site — their net width is not
        // fixed at the definition site, so they follow the shape-by-use rule
        // the same way scalar module ports do (the Pass2 param binding
        // resolves the actual member width).
        self.param_names.iter().any(|p| p == name) || self.parent.is_declared_port(name)
    }

    fn declared_port_members(&self, base: &str) -> Option<Vec<String>> {
        // Deliberately NOT including func params: a param is bound at the call
        // site and stays shape-by-use (member width resolved at instantiation),
        // so it is never an authoritative declared port. Only a real module
        // io/out/in port on the parent chain is gated.
        self.parent.declared_port_members(base)
    }

    fn uri(&self) -> &crate::McURI {
        self.parent.uri()
    }

    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        self.parent.parse_declare(node)
    }

    fn gen_anon_name(&mut self, classname: &str) -> String {
        self.parent.gen_anon_name(classname)
    }

    fn store_inst_span(&mut self, name: &str, span: std::ops::Range<usize>) {
        self.parent.store_inst_span(name, span)
    }

    fn record_declareb_def(
        &mut self,
        name: &str,
        kind: crate::refdef::types::SymbolKind,
        span: std::ops::Range<usize>,
    ) {
        self.parent.record_declareb_def(name, kind, span)
    }

    fn find_func_return(&self, name: &str) -> Option<McFuncReturn> {
        self.parent.find_func_return(name)
    }

    fn scope_name(&self) -> Option<String> {
        self.parent.scope_name()
    }

    fn report_floating_label(&self, name: &str, node: &AstNode) {
        // Record every bare identifier that failed find_inst. Param members
        // and func-local declares are filtered out after the body loop
        // (`self.params` / `self.insts`) — the actual member width of a
        // bracket param is substituted at instantiation, and a func-local
        // declare (e.g. `RES R[1:2](5.1kΩ)` → R1) is a real instance.
        self.pending_floating
            .borrow_mut()
            .push((name.to_string(), node.get_pos(), node.get_len()));
    }

    fn is_declared_instance_name(&self, base: &str) -> bool {
        // Func params and the parent container chain (`find_inst`) plus the
        // func body's own established instance references: func-local declares
        // (registered into func.insts during the loop — the `instance_chain`
        // does NOT include func.insts, so `find_inst` alone cannot see them)
        // and FuncCall caller names.
        if self.param_names.iter().any(|p| p == base) {
            return true;
        }
        if self.seen_callers.borrow().iter().any(|s| s == base) {
            return true;
        }
        self.find_inst(base).is_some()
    }

    fn note_func_call_caller(&mut self, name: &str) {
        let mut s = self.seen_callers.borrow_mut();
        if !s.iter().any(|x| x == name) {
            s.push(name.to_string());
        }
    }

    fn register_gate_candidate(&mut self, base: &str, form: &str, pos: u32, len: u32) {
        self.gate_candidates.borrow_mut().push(GateCandidate {
            base: base.to_string(),
            form: form.to_string(),
            pos,
            len,
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct McFunctions {
    functions: Vec<McFunction>,
}

impl McFunctions {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
        }
    }

    /// Parse header + body with context (for symbol resolution)
    pub fn parse(&mut self, node: &AstNode, context: &mut dyn HasFindInst) {
        if let Some(mut new_node) = McFunction::new(node) {
            // Find and parse body
            if let Some(subnodes) = node.get_sub_node() {
                if let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) {
                    new_node.parse_body(context, &body);
                }
            }
            self.functions.push(new_node);
        }
    }

    /// Find by function name
    pub fn find(&self, name: &str) -> Option<&McFunction> {
        self.functions
            .iter()
            .find(|elem| elem.name.to_string() == name)
    }

    /// Find by function name (mutable reference)
    pub fn find_mut(&mut self, name: &str) -> Option<&mut McFunction> {
        self.functions
            .iter_mut()
            .find(|elem| elem.name.to_string() == name)
    }
}

impl std::ops::Deref for McFunctions {
    type Target = Vec<McFunction>;

    fn deref(&self) -> &Self::Target {
        &self.functions
    }
}

impl std::ops::DerefMut for McFunctions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.functions
    }
}

#[derive(Debug, Clone)]
pub struct McFunction {
    pub name: McIds,
    pub params: McParamDeclares,
    /// Return-value kind (Implicit / This / Endpoint).
    /// Set by [`parse_body`] when it encounters a `return` statement.
    pub returns: McFuncReturn,
    pub insts: McInstances,
    pub stmts: Vec<McPhrase>,
    /// Source byte offset of each body stmt, parallel to `stmts`. Used by
    /// Pass2 func-body expansion to attribute anonymous instance names and
    /// connection provenance to the exact source stmt of the construction.
    pub stmt_offsets: Vec<u32>,
    /// Conditional blocks (if/else if/else) parsed from the function body.
    /// Lines are pre-parsed into McPhrase; evaluated at instantiation time
    /// against actual parameter values.
    pub conds: Vec<crate::semantic::basic::mc_conds::McFuncConds>,
    /// Bare identifiers in the body that failed `find_inst` and are not a
    /// declared param member or func-local instance (name, pos, len). A
    /// post-parse check (E3136) counts how many times each name is referenced
    /// across all funcs of the component and warns for single-use dangling
    /// labels. Populated by [`McFunction::parse_body`].
    pub(crate) floating_candidates: Vec<(String, u32, u32)>,
    /// Names established as instance references in this func body (FuncCall
    /// caller names + func-local declares), for the Phase 1 discriminator and
    /// the component-finish recheck. Populated by [`McFunction::parse_body`].
    pub(crate) seen_callers: Vec<String>,
    /// Phase 1 gate candidates whose phantom bus was suppressed at parse time.
    /// The component-finish recheck re-resolves each against the final symbol
    /// table. Populated by [`McFunction::parse_body`].
    pub(crate) gate_candidates: Vec<GateCandidate>,
    /// Pre-parsed function body connections (needs to be called after McModule is built to fill parse_body)
    pub called_time: u32,
    anon_counter: usize,
    uri: Option<crate::McURI>,
    /// Source span in the definition file. Set by [`new`] from the AST node.
    pub span: Option<std::ops::Range<usize>>,
}

impl McFunction {
    /// The file this function was parsed from. `None` only if the body was
    /// never parsed, which means there are no body stmts to expand.
    pub fn source_uri(&self) -> Option<&crate::McURI> {
        self.uri.as_ref()
    }

    pub fn new(node: &AstNode) -> Option<Self> {
        // MCAST_FUNCTION
        // |- MCAST_NAME - MCAST_PARAM (option) - MCAST_BODY
        let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);

        //1. new — span from the function name (MCAST_NAME → MCAST_IDS), not the whole node
        let name_node = subnodes
            .iter()
            .find(|x: &AstNode| x.is_type(MCAST_NAME))
            .expect(MISSING_SUBNODE);
        let ids_node = name_node.get_sub_node().expect(MISSING_SUBNODE);
        // ids_node (MCAST_IDS) has the actual name span; MCAST_NAME covers the whole function
        let func_span =
            (ids_node.get_pos() as usize)..((ids_node.get_pos() + ids_node.get_len()) as usize);
        let mut ret = Self {
            name: McIds::new(&ids_node)?,
            params: McParamDeclares::new(),
            returns: McFuncReturn::Implicit,
            insts: McInstances::new(),
            stmts: Vec::new(),
            stmt_offsets: Vec::new(),
            conds: Vec::new(),
            floating_candidates: Vec::new(),
            seen_callers: Vec::new(),
            gate_candidates: Vec::new(),
            called_time: 0,
            anon_counter: 1,
            uri: None,
            span: Some(func_span),
        };

        //2. param
        let _ = &subnodes
            .iter()
            .find(|x: &AstNode| x.is_type(MCAST_PARAMS))
            .map(|param_node| ret.params.parse(&param_node));

        // ret.body
        //     .iter()
        //     .filter(|x| x.is_type(MCAST_ATTRIBUTE))
        //     .for_each(|x| ret.attrs.parse(x));

        Some(ret)
    }

    pub fn call_count_incr(&mut self) {
        self.called_time += 1;
    }

    /// Parse function body in McModule context
    ///
    /// Needs to be called after McModule is created (after symbol table is ready),
    /// because McOpd::new in function body needs symbol resolution through context.
    ///
    /// # Call timing
    /// In pass_defgen phase, all McModule members (components, submodules, labels, function declarations)
    /// after all parsed, iterate all functions calling this method:
    /// ```ignore
    /// for func in self.funcs.iter_mut() {
    ///     func.parse_body(self);
    /// }
    /// ```
    pub fn parse_body(&mut self, context: &mut dyn HasFindInst, body: &AstNode) {
        let uri = context.uri().clone();
        self.uri = Some(uri.clone());
        // ★ LSP: Set scope for instance registration with parent prefix
        let parent_scope = context.scope_name().unwrap_or_default();
        let full_scope = if parent_scope.is_empty() {
            self.name.to_string()
        } else {
            format!("{}.{}", parent_scope, self.name.to_string())
        };
        self.insts.scope = Some(full_scope);
        // ★ Fix: wrap context so func params are searchable by McPhrase::new
        let param_names: Vec<String> = self
            .params
            .iter()
            .filter_map(|p| p.get_primary_name())
            .collect();
        // Bare identifiers that failed find_inst are recorded here and
        // filtered after the body loop against the func's own params/insts.
        let pending_floating = std::cell::RefCell::new(Vec::new());
        // Phase 1: names established as instance references in this body
        // (caller names + func-local declares) and gate candidates whose
        // phantom bus was suppressed. Drained into `self` after the loop.
        let seen_callers = std::cell::RefCell::new(Vec::new());
        let gate_candidates = std::cell::RefCell::new(Vec::new());
        let mut wrapper = FuncBodyContext {
            param_names: &param_names,
            pending_floating: &pending_floating,
            seen_callers: &seen_callers,
            gate_candidates: &gate_candidates,
            parent: context,
        };
        if let Some(body_nodes) = body.get_sub_node() {
            let body_nodes: AstNode = body_nodes;
            // ── [BODY-RAW] read-only diagnostic ─────────────────────────────
            // Pure print, no behavior change. List each top-level node's type under body + its
            // child node type sequence, used to confirm that `MIC{P,N} -> cap[4:5]::CAP() -> uC.ADC{P,N}`
            // this statement appears in AST in what form (or doesn't appear at all).
            // get_type() returns u16; against macros.rs: NET=33, DECLARE=26,
            // OPD=52, OPD_RIGHTARROW=71, OPD_DBCOLON=77, INSTANCE=29, CLASS=28。
            {
                let mut idx = 0;
                for body_node in body_nodes.iter() {
                    let bt = body_node.get_type();
                    let child_types: Vec<u16> = body_node
                        .get_sub_node()
                        .map(|c| c.iter().map(|n| n.get_type()).collect())
                        .unwrap_or_default();
                    let node_str = body_node.to_string();
                    mcc_dbg!("sem::fcall", 
                        "[BODY-RAW] node[{}] type={} child_types={child_types:?} to_string={node_str:?}",
                        idx, bt
                    );
                    idx += 1;
                }
                mcc_dbg!(
                    "sem::fcall",
                    "[BODY-RAW] total {} top-level body nodes",
                    idx
                );
            }
            for body_node in body_nodes.iter() {
                match body_node.get_type() {
                    // MCAST_DECLARE: component/module instantiation
                    MCAST_DECLARE => {
                        self.parse_declare_note(&body_node, &uri, &seen_callers);
                    }

                    MCAST_NET => {
                        if let Some(subnode) = body_node.get_sub_node() {
                            // ── return-statement detection ──────────────────
                            // The parser may wrap `return X` either as
                            //   NET → IOTYPE_RETURN(→ X | sibling X)        (typical)
                            //   NET → IOTYPE_RETURN  (bare `return`)
                            // Sniff for the marker first; if found, divert
                            // the stmt into the return slot instead of pushing
                            // it onto `self.stmts`.
                            if Self::find_return_marker(&subnode).is_some() {
                                self.handle_return(&mut wrapper, &body_node, &subnode);
                                continue;
                            }

                            // MCAST_DECLARE inside MCAST_NET is a declaration - process it
                            if subnode.get_type() == MCAST_DECLARE {
                                self.parse_declare_note(&subnode, &uri, &seen_callers);
                                continue;
                            }

                            // ★ LSP: Record net refs for identifiers in func body
                            {
                                let scope = self
                                    .insts
                                    .scope
                                    .clone()
                                    .unwrap_or_else(|| self.name.to_string());
                                crate::semantic::module::McModule::collect_net_refs_in_node(
                                    &subnode,
                                    &mut self.insts,
                                    &mut self.params,
                                    &scope,
                                );
                            }
                            match McPhrase::new(&subnode, &mut wrapper) {
                                Some(net) => {
                                    // Keep the source byte offset of this body stmt
                                    // parallel to `stmts` so Pass2 can attribute
                                    // anonymous instances / connections to the exact stmt.
                                    self.stmt_offsets.push(subnode.get_pos() as u32);
                                    self.stmts.push(net);
                                }
                                None => {
                                    // ── P1 fix: no longer silently discarded ────────────────────
                                    // Previously `None => {}` silently swallowed unresolvable connection stmts,
                                    // causing whole stmt to disappear from netlist but errors=0/warnings=0
                                    // (typical: `MIC{P,N} -> cap[4:5]::CAP() -> uC.ADC{P,N}`).
                                    // Now upgraded to Warning (non-fatal, doesn't break errors=0 gate),
                                    // with reconstructed source text, making any "whole-stmt evaporation" immediately visible.
                                    let stmt_txt = subnode
                                        .to_string()
                                        .unwrap_or_else(|| "<unprintable>".to_string());
                                    dlog_warning(
                                        crate::errcodes::FUNC_STMT_DROPPED,
                                        &subnode,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::FUNC_STMT_DROPPED,
                                            &[&stmt_txt],
                                        ),
                                    );
                                }
                            }
                        } else {
                            dlog_error(
                                crate::errcodes::FUNC_EMPTY_NET,
                                &body_node,
                                &crate::errcodes::format_msg(crate::errcodes::FUNC_EMPTY_NET, &[]),
                            );
                        }
                    }

                    // ── return statement appearing as a top-level body node ──
                    // (defensive: some parser shapes may not wrap `return` in NET)
                    MCAST_IOTYPE_RETURN => {
                        self.handle_return(&mut wrapper, &body_node, &body_node);
                    }

                    MCAST_COND_IF => {
                        // Parse conditional blocks (if/else if/else)
                        // The body_node is a COND_IF node; use McConds to parse
                        // the condition structure, then convert to McFuncConds
                        // with pre-parsed McPhrase stmts.
                        use crate::semantic::basic::mc_conds::{McConds, McFuncConds};
                        if let Some(raw_conds) = McConds::new(&body_node) {
                            // ★ LSP: Record net refs inside if/else blocks so
                            // identifiers on conditional net stmts (e.g.
                            // `GPIO[2]`, `GND`) resolve for goto-definition.
                            // Top-level stmts are handled by the MCAST_NET
                            // branch above; conditional stmts were missing.
                            {
                                let scope = self
                                    .insts
                                    .scope
                                    .clone()
                                    .unwrap_or_else(|| self.name.to_string());
                                for cond in &raw_conds.if_blocks {
                                    crate::semantic::module::McModule::collect_net_refs_in_node(
                                        &cond.block,
                                        &mut self.insts,
                                        &mut self.params,
                                        &scope,
                                    );
                                }
                                if let Some(else_node) = &raw_conds.else_block {
                                    crate::semantic::module::McModule::collect_net_refs_in_node(
                                        else_node,
                                        &mut self.insts,
                                        &mut self.params,
                                        &scope,
                                    );
                                }
                            }
                            let parsed = McFuncConds::from_conds(&raw_conds, &mut wrapper);
                            if !parsed.if_blocks.is_empty() || !parsed.else_stmts.is_empty() {
                                mcc_dbg!(
                                    "sem::fcall",
                                    "[COND-PARSE] func={} if_blocks={} else_stmts={}",
                                    self.name,
                                    parsed.if_blocks.len(),
                                    parsed.else_stmts.len()
                                );
                                self.conds.push(parsed);
                            }
                        }
                    }
                    _ => {
                        dlog_error(
                            crate::errcodes::FUNC_BODY_INVALID,
                            &body_node,
                            &crate::errcodes::format_msg(crate::errcodes::FUNC_BODY_INVALID, &[]),
                        );
                    }
                }
            }

            // ── E3136 FUNC_FLOATING_LABEL ─────────────────────────────────────
            // A bare identifier that failed find_inst is a floating net
            // endpoint unless it is declared by this func body:
            //   * param member, incl. bracket form `[net1, net2]` — the actual
            //     member width is substituted at instantiation (params.find);
            //   * func-local declare (`RES R[1:2](5.1kΩ)` → R1,
            //     `CAP ccp(220nF, 25V)` → ccp) — lands in `self.insts` during
            //     the loop.
            // Anything still unresolved is recorded as a candidate for the
            // post-parse FloatingLabelCheck (E3136), which counts how many
            // times the name is referenced across all funcs of the component
            // and warns for single-use dangling labels (declarations in a
            // sibling func or a conditional block are resolved there via the
            // component's final instance table).
            drop(wrapper);
            // Phase 1: drain the body's caller names and gate candidates into
            // the func for the component-finish recheck.
            self.seen_callers = seen_callers.into_inner();
            self.gate_candidates = gate_candidates.into_inner();
            for (name, pos, len) in pending_floating.into_inner() {
                if self.params.find(&name).is_some() {
                    continue;
                }
                if self.insts.get(&name).is_some() {
                    continue;
                }
                self.floating_candidates.push((name, pos, len));
            }

            // ★ Smart Param (M5): Finalize after body parsed
            let func_name = self.name.to_string();
            // ★ Collect param references in function body for LSP goto-def
            crate::semantic::component::McComponent::collect_param_refs_in_body(
                body,
                &mut self.params,
                &func_name,
            );
            let diags = self.params.finalize(Some(body), &func_name);
            for d in &diags {
                crate::mcc_log_global_diag(d);
            }
        }
    }

    /// Parse a func-body DECLARE into `self.insts` and note any newly
    /// registered instance names into the Phase 1 `seen_callers` set (set
    /// difference, same shape as `McFunction::parse_declare`). Func-local
    /// declares (e.g. `SYS.Calendar timer`, `Transistor q`) live in
    /// `func.insts`, which the body context's `instance_chain` does NOT
    /// include — so without this note the ghost-bus discriminator would
    /// misclassify a legitimate `timer.I2C` / `q.g` reference as a true miss.
    fn parse_declare_note(
        &mut self,
        node: &AstNode,
        uri: &crate::McURI,
        seen_callers: &std::cell::RefCell<Vec<String>>,
    ) {
        let before: std::collections::HashSet<String> =
            self.insts.iter().map(|(n, _)| n.to_string()).collect();
        self.insts.parse(node, uri);
        for (name, _) in self.insts.iter() {
            if before.contains(name) {
                continue;
            }
            let mut sc = seen_callers.borrow_mut();
            if !sc.iter().any(|s| s == name) {
                sc.push(name.to_string());
            }
        }
    }

    // ========================================================================
    // return-statement helpers
    // ========================================================================

    /// Render the function body for display: plain connection stmts followed
    /// by conditional blocks (`if cond` / `else`, branch stmts indented).
    /// The model stores stmts and conds separately, so source interleaving is
    /// not preserved; conds are appended after the plain stmts.
    pub fn body_stmts_display(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.stmts.iter().map(|l| l.to_string()).collect();
        for c in &self.conds {
            for blk in &c.if_blocks {
                lines.push(format!("if {}", blk.condition));
                for l in &blk.stmts {
                    lines.push(format!("    {}", l));
                }
            }
            if !c.else_stmts.is_empty() {
                lines.push("else".to_string());
                for l in &c.else_stmts {
                    lines.push(format!("    {}", l));
                }
            }
        }
        lines
    }

    /// Locate a `MCAST_IOTYPE_RETURN` marker inside a NET subnode (or a body
    /// node that already is the marker).
    ///
    /// Two AST shapes are accepted:
    ///   * `node` itself is `MCAST_IOTYPE_RETURN`  → return `Some(node.clone())`
    ///   * `node`'s first child is `MCAST_IOTYPE_RETURN` → return that child
    ///     (allows the expression to live as a sibling at the NET layer)
    fn find_return_marker(node: &AstNode) -> Option<AstNode> {
        if node.get_type() == MCAST_IOTYPE_RETURN {
            return Some(node.clone());
        }
        if let Some(first) = node.get_sub_node() {
            if first.get_type() == MCAST_IOTYPE_RETURN {
                return Some(first);
            }
        }
        None
    }

    /// Handle a recognised `return` statement.
    ///
    /// `body_node` — the outer node, used for error position.
    /// `wrapper`   — the NET subnode (or body node) that contains the marker.
    fn handle_return(
        &mut self,
        context: &mut dyn HasFindInst,
        body_node: &AstNode,
        wrapper: &AstNode,
    ) {
        // 1. Reject multiple returns. A function may have at most one.
        if !matches!(self.returns, McFuncReturn::Implicit) {
            dlog_error(
                crate::errcodes::FUNC_MULTIPLE_RETURNS,
                body_node,
                &crate::errcodes::format_msg(crate::errcodes::FUNC_MULTIPLE_RETURNS, &[]),
            );
            return;
        }

        // 2. Locate the IOTYPE_RETURN marker.
        let Some(marker) = Self::find_return_marker(wrapper) else {
            dlog_error(
                crate::errcodes::FUNC_RETURN_MALFORMED,
                body_node,
                &crate::errcodes::format_msg(crate::errcodes::FUNC_RETURN_MALFORMED, &[]),
            );
            return;
        };

        // 3. Find the expression node — try `marker.sub_node` first (the
        //    common "tagged wrapper" shape), then fall back to the next
        //    sibling at the NET layer.
        let expr_node_opt = marker.get_sub_node().or_else(|| marker.get_next());

        let Some(expr_node) = expr_node_opt else {
            // Bare `return` with no expression — interpret as `return this`.
            self.returns = McFuncReturn::This;
            return;
        };

        // 4. Recognise `return this` first: it is the only chainable variant
        //    that needs explicit acknowledgement (we cannot represent `this`
        //    as a McPhrase, since `this` is the receiver itself).
        if Self::is_this_expr(&expr_node) {
            self.returns = McFuncReturn::This;
            return;
        }

        // 5. Otherwise treat the expression as a phrase. A successful parse
        //    means it's a label / bus / endpoint → non-chainable return.
        match McPhrase::new(&expr_node, context) {
            Some(phrase) => {
                self.returns = McFuncReturn::Endpoint(phrase);
            }
            None => {
                dlog_error(
                    crate::errcodes::FUNC_RETURN_EXPR_INVALID,
                    body_node,
                    &crate::errcodes::format_msg(crate::errcodes::FUNC_RETURN_EXPR_INVALID, &[]),
                );
            }
        }
    }

    /// Recognise `this` across the few plausible AST shapes.
    fn is_this_expr(node: &AstNode) -> bool {
        if node.get_type() == MCAST_OPD_THIS {
            return true;
        }
        if let Some(sub) = node.get_sub_node() {
            if sub.get_type() == MCAST_OPD_THIS {
                return true;
            }
        }
        if let Some(s) = node.to_string() {
            return s == "this";
        }
        false
    }
}

impl HasFindInst for McFunction {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.insts.get(id).cloned()
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut McInstance> {
        self.insts.get_mut(id)
    }

    fn get_vector_members(&self, base: &str) -> Option<Vec<String>> {
        self.insts
            .get_vector_members(base)
            .map(|members| members.to_vec())
    }

    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        if let Some(s) = span {
            self.insts.store_port_span(&name, s);
        }
        self.add_label(name)
    }

    fn add_label(&mut self, name: String) -> Option<McPhrase> {
        if let Some(existing_inst) = self.insts.get(&name) {
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                existing_inst.clone(),
            ))));
        }
        for (inst_name, inst) in self.insts.iter() {
            let is_anon = inst_name.starts_with('@')
                || (inst_name.starts_with('[') && inst_name.contains(','));
            if !is_anon {
                continue;
            }
            match inst {
                McInstance::List(list) => {
                    if list.member.contains(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                }
                McInstance::Bus(bus) => {
                    if bus.full_members.contains(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                }
                McInstance::Interface(iface) => {
                    if iface.base.pins.names_to_id.contains_key(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                    let iface_members = iface.name.expand();
                    if iface_members.len() > 1 && iface_members.contains(&name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(name.clone()),
                        ))));
                    }
                }
                _ => {}
            }
        }
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Label(name),
        ))))
    }

    fn add_component(&mut self, name: String, comp: Mc2Component) -> Option<McPhrase> {
        let inst = McInstance::Component(std::sync::Arc::new(comp));
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_module(&mut self, name: String, module: Mc2Module) -> Option<McPhrase> {
        let inst = McInstance::Module(std::sync::Arc::new(module));
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        // An inlined ghost-bus (resolve-gate relax-everything) is a statement-tree net
        // node, NOT a declaration — never register it into `insts`, or the
        // finish recheck (gate.rs `base_declared_by_finish`) would mistake the
        // base for a late-declared instance and skip E3137. Net joining in
        // pass2 is driven by the bus name in the statement tree.
        let inst = McInstance::Bus(McBus::new_with_members(&name, members));
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let inst = McInstance::List(McList::new_with_members(&name, members));
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_bus_member(&mut self, _base: &str, _member: String) -> Option<McPhrase> {
        None
    }

    fn add_interface_member(
        &mut self,
        _component: &str,
        _interface: &str,
        _members: Vec<String>,
    ) -> Option<McPhrase> {
        None
    }

    fn check_bus_member(&mut self, _base: &str, _member: &str) -> Option<(String, String)> {
        None
    }

    fn is_component_bus(&self, _base: &str, _member: &str) -> bool {
        false
    }

    fn uri(&self) -> &crate::McURI {
        self.uri
            .as_ref()
            .expect("McFunction.uri not set, call parse_body first")
    }

    /// ── Iter-7.4 (parser fix) ────────────────────────────────────────────
    /// Let `MCAST_DECLARE` embedded in func body chain (named instance `R442::RES(1MΩ)`
    /// or anonymous instance `CAP(1nF)` etc.) be correctly instantiated as Component / Module / Bus instance,
    /// instead of falling back to mc_phrase.rs:335 fallback to become label.
    ///
    /// Historical reason:
    ///   Original implementation returns `Vec::new()`, causing all DECLARE encountered in func body chain
    ///   nodes to get no instance, can only be fallback as label in mc_phrase.rs.
    ///   Symptoms (root cause of bugfix_report errors 5/6/8):
    ///     - In `XTAL + R442::RES(1MΩ)` R442 doesn't appear in components list,
    ///       netlist shows as `X6.R442 ~ CAP_3.1` (R442 used as node)
    ///     - In `(CAP(1nF) + RES(10kΩ)) -> GND` two anonymous components as bare scalar nodes
    ///       (`@CAP5 ~ GND.GND`) form, no .1/.2 pin distinction → flattened
    ///     - Crystal setup() body 18pF capacitor topology confused
    ///
    /// Fix strategy:
    ///   1. Call existing `McInstances::parse_declare` (mc_inst.rs:669) to register instance
    ///      into `self.insts` —— this 870-line monster method already handles
    ///      class lookup, CMIE, NC, nested params, array instances, construction args and all
    ///      corner cases, reuse is most stable.
    ///   2. Use set difference to get newly registered instances, clone and return `Vec<McInstance>`.
    ///      These instances are wrapped into phrase at mc_phrase.rs:330-333
    ///      (Endpoint(Component(Arc<Mc2Component>))), phrase itself carries component
    ///      all info (partno, pins etc.), instantiation phase uses directly.
    ///
    /// # Note
    /// - This method assumes `self.uri` is set by `parse_body`. `McPhrase::new` in
    ///   func body parse context, parse_body has already completed line 232's
    ///   `self.uri = Some(uri.clone())`, so the precondition is satisfied.
    /// - For a second `parse_declare` with the same `inst_name`,
    ///   `McInstances::parse_declare` internally uses `insert` to overwrite,
    ///   so the difference-derived "newly added" set will be empty — this
    ///   situation does not actually occur (declaring the same name twice in
    ///   one chain line is a user error) and is not handled.
    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        // No uri means we have to give up (parse_body hasn't finished yet?) — preserve old behavior
        let uri = match self.uri.clone() {
            Some(u) => u,
            None => return Vec::new(),
        };

        // 1) Record the instance name set before the call
        let before: std::collections::HashSet<String> =
            self.insts.iter().map(|(k, _)| k.to_string()).collect();

        // 2) Call McInstances::parse_declare to register the new instance
        //    iotype is None — inline instances in a chain are not port/power types
        self.insts
            .parse_declare(node, &uri, &crate::semantic::common::IOType::None);

        // 3) Extract newly added instances (clone — McInstance itself is an enum,
        //    internal Component/Module is Arc-wrapped, so clone is cheap)
        self.insts
            .iter()
            .filter(|(k, _)| !before.contains(*k))
            .map(|(_, inst)| inst.clone())
            .collect()
    }

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
    }

    fn gen_anon_name(&mut self, classname: &str) -> String {
        // ── P4-e: Sanitize '.' ──
        // `DIO.ESD` → `@DIO_ESD{n}`; otherwise `@DIO.ESD{n}` will be misjudged
        // by `node_to_netpoint`'s `split_once('.')` as owner=`@DIO` → multiple
        // anonymous calls share the `@DIO` label → short circuit.
        // Aligned with P0-2 (safe_type) of `pass2 instantiate_component_construction`.
        let safe = classname.replace('.', "_");
        let name = format!("@{}{}", safe, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn store_inst_span(&mut self, name: &str, span: std::ops::Range<usize>) {
        self.insts.store_port_span(name, span);
    }

    fn record_declareb_def(
        &mut self,
        name: &str,
        kind: crate::refdef::types::SymbolKind,
        span: std::ops::Range<usize>,
    ) {
        self.insts.record_declareb_def(name, kind, span);
    }

    fn scope_name(&self) -> Option<String> {
        Some(self.name.to_string())
    }
}
