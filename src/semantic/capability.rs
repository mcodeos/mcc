// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `capability` container — a role circuit recipe (abstract-variant-capability-plan §3).
//!
//! A capability is a *declaration-only* container: a set of **signal
//! declarations** (`ps [VCC,GND]`, `io uart[RO,DI]::UART.TTL(DTE)`, … — the
//! module-port family grammar, minus physical pin numbers) plus **funcs** that
//! are written against those declared signals (contract §3.2). It is not
//! placeable, has no partno/package/spec, no construction params, no
//! derivation (§3.1 — the grammar simply has no slots for them).
//!
//! A component adopts a capability with `:: Cap`; its funcs then become
//! effective methods of the adopter (§5), expanded in the adopting instance's
//! scope at call time. Capability func bodies are parsed once here, at load
//! time, resolved against the capability's *own* declared signals — body-name
//! resolution is lexical against the defining def's members (§3.2) — so the
//! def is self-contained and needs no AST retention.
//!
//! The signal table is a full [`McInstances`] (module-port machinery), so
//! declared signals carry real IOType (ps → Power, io → InOut, in/out → …)
//! and membered groups expand exactly as module ports do.

use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::semantic::basic::mc_bus::{McBus, McList};
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_ids::McIds;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::component::Mc2Component;
use crate::semantic::mc_func::{HasFindInst, McFunctions};
use crate::semantic::mc_inst::{McInstance, McInstances};
use crate::McURI;

/// A capability definition (`capability DecoupledPower { … }`).
#[derive(Debug, Clone)]
pub struct McCapability {
    pub name: McIds,
    /// File the capability was declared in (LSP / diagnostics anchor).
    pub uri: McURI,
    /// Declared signals, parsed with the module-port machinery
    /// (net_ports / declare clauses → IOType-bearing entries + vector groups).
    pub signals: McInstances,
    /// Role funcs, parsed once at load time against `signals` (self-consistent,
    /// contract §3.2 — no adopter needed to resolve the body).
    pub funcs: McFunctions,
    /// Source span for LSP goto-definition (byte range in `uri`).
    pub span: crate::ast::sem::Span,
    /// Counter for anonymous-instance names (`@{classname}{counter}`) in func
    /// bodies, mirroring `McComponent::anon_counter` (never materialized —
    /// dormant until an adopter instantiates the func).
    pub anon_counter: usize,
}

impl McCapability {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCAST_CAPABILITY
        // |- MCAST_NAME - MCAST_BODY
        let subnodes = node.get_sub_node()?;

        //1. name — same read as component (mc_class_name = ids [+ .int dot])
        let cap_name = McIds::new_with_dot(
            &subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))?
                .get_sub_node()?,
        )?;

        // Span from the capability name (MCAST_NAME → MCAST_IDS), not the whole node
        let name_node = subnodes.iter().find(|x| x.is_type(MCAST_NAME))?;
        let ids_node = name_node.get_sub_node()?;
        let start = ids_node.get_pos() as usize;
        let end = start + ids_node.get_len() as usize;

        let mut cap = Self {
            name: cap_name.clone(),
            uri: uri.clone(),
            signals: McInstances::new(),
            funcs: McFunctions::new(),
            span: crate::ast::sem::Span { start, end },
            anon_counter: 1,
        };
        // ★ LSP: enclosing scope name for instance registration (module sets
        // this at parse_body; capability func clauses are parsed below).
        cap.signals.scope = Some(cap_name.to_string());

        //2. body — signals first, then funcs (a func body resolves against the
        //   *full* declared signal set, so funcs must see every signal clause
        //   regardless of textual order).
        let mut func_nodes: Vec<AstNode> = Vec::new();
        if let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) {
            if let Some(body_nodes) = body.get_sub_node() {
                for clause in body_nodes.iter() {
                    match clause.get_type() {
                        // Signal declaration (`ps …`, `io …`, `in …`): module-port family.
                        MCAST_NET_PORTS => {
                            cap.signals.parse(&clause, uri);
                        }
                        // Role func.
                        MCAST_FUNCTION => {
                            func_nodes.push(clause);
                        }
                        // Capability bodies may contain *only* signal declarations
                        // and funcs (§3.1): no attrs (partno/package/spec), no
                        // pins=[…], no net/connection stmts, no declares, no role,
                        // no conditionals.
                        _ => {
                            let msg = crate::errcodes::format_msg(
                                crate::errcodes::CAPABILITY_BODY_INVALID,
                                &[&cap_name.to_string()],
                            );
                            crate::db::diagnostic::diagnostic::dlog_error(
                                crate::errcodes::CAPABILITY_BODY_INVALID,
                                &clause,
                                &msg,
                            );
                        }
                    }
                }
            }
        }

        //3. funcs (parse header + body with the capability as scope)
        let context = unsafe { &mut *(&mut cap as *mut McCapability) as &mut dyn HasFindInst };
        for x in func_nodes {
            // ★ LSP: register interface class refs from the func header
            // (`func Bypass(c::CAP(100nF,10V))` → `CAP`) so goto-def / hover
            // resolve them (same path as component funcs).
            crate::query::refs::register_func_header_iface_refs(&x, &cap.uri);
            cap.funcs.parse(&x, context);
        }

        //4. §3.2 self-consistency: a capability func body must reference only
        //   declared signals, its own params, and func-local instances — the
        //   capability's *full* name set is its signal table, resolved
        //   lexically against the def (§3.2), so the def must be self-consistent
        //   at load. `floating_candidates` holds every bare name that survived
        //   the param / func-local filter; component/module defer these to the
        //   E3136 finish-time recheck, but a capability is never instantiated
        //   or finished, so the violation is reported here. One diagnostic per
        //   distinct name (first occurrence).
        for f in cap.funcs.iter() {
            let mut reported: std::collections::HashSet<String> = Default::default();
            for (name, pos, len) in &f.floating_candidates {
                if !reported.insert(name.clone()) {
                    continue;
                }
                let msg = crate::errcodes::format_msg(
                    crate::errcodes::CAPABILITY_FUNC_UNRESOLVED_REF,
                    &[name],
                );
                crate::db::diagnostic::diagnostic::dlog_error_at(
                    crate::errcodes::CAPABILITY_FUNC_UNRESOLVED_REF,
                    *pos,
                    *len,
                    &msg,
                );
            }
        }

        Some(cap)
    }
}

// ============================================================================
// HasFindInst for McCapability — func-body name scope (§3.2)
// ============================================================================
//
// The body scope is the capability's own declared signals (module-port family),
// reached through the module-style category chain. `add_*` / `parse_declare`
// mirror `McComponent` so capability func bodies support the same statement
// shapes as component func bodies (chained subinstance declares register into
// `signals` with IOType::None — never a declared *port*, so the §4.2
// consistency surface, which reads the declared signal set, is unpolluted).

impl HasFindInst for McCapability {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    fn find_inst_mut(&mut self, _id: &str) -> Option<&mut crate::McInstance> {
        None
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        crate::semantic::scope::capability_scope(self)
            .resolve(id)
            .map(|r| (r.inst, r.span))
    }

    fn add_label_at(
        &mut self,
        name: String,
        _span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Label(name),
        ))))
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Bus(McBus::new_with_members(&name, members)),
        ))))
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::List(McList::new_with_members(&name, members)),
        ))))
    }

    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase> {
        self.add_bus(base.to_string(), vec![member])
    }

    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase> {
        self.add_bus(format!("{component}.{interface}"), members)
    }

    fn check_bus_member(&mut self, _base: &str, _member: &str) -> Option<(String, String)> {
        None
    }

    fn is_component_bus(&self, _base: &str, _member: &str) -> bool {
        false
    }

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
    }

    fn uri(&self) -> &McURI {
        &self.uri
    }

    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        // Mirror McComponent::parse_declare (component/mod.rs:589): register a
        // chained func-body declare into the signal table with IOType::None so
        // its receiver endpoint resolves. Never a declared port (consistency
        // reads declared signals only, §4.2).
        let before: std::collections::HashSet<String> =
            self.signals.iter().map(|(k, _)| k.to_string()).collect();
        self.signals
            .parse_declare(node, &self.uri, &crate::semantic::common::IOType::None);
        self.signals
            .iter()
            .filter(|(k, _)| !before.contains(*k))
            .map(|(_, inst)| inst.clone())
            .collect()
    }

    fn add_component(&mut self, name: String, comp: Mc2Component) -> Option<McPhrase> {
        // Mirror McComponent::add_component: `@`-prefixed anonymous names are
        // created inline in connection stmts and are not re-declared.
        let inst = McInstance::Component(std::sync::Arc::new(comp));
        if !name.starts_with('@') {
            self.signals.create_inst(&name, inst.clone());
        }
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_module(
        &mut self,
        _name: String,
        _module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        None
    }

    fn gen_anon_name(&mut self, classname: &str) -> String {
        // Mirror McComponent::gen_anon_name: `@` marks inline anonymous
        // instances; dormant until an adopter instantiates the func.
        let name = format!("@{}{}", classname, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn record_declareb_def(
        &mut self,
        name: &str,
        kind: crate::refdef::types::SymbolKind,
        span: std::ops::Range<usize>,
    ) {
        self.signals.record_declareb_def(name, kind, span);
    }

    fn scope_name(&self) -> Option<String> {
        Some(self.name.to_string())
    }
}

// ============================================================================
// Display implementation - concise format output
// ============================================================================

impl std::fmt::Display for McCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n_signals = self.signals.iter_ports().count();
        writeln!(
            f,
            "Capability {} ({} signals, {} funcs)",
            self.name,
            n_signals,
            self.funcs.len()
        )
    }
}
