// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::collections::HashSet;
use std::sync::Arc;

use crate::semantic::basic::mc_conds::{CondParam, McConds};
use crate::{
    ast::{macros::*, node::AstNode},
    semantic::{
        basic::mc_param::McParamDeclares, basic::mc_phrase::McPhrase, basic::mc_role::McRole,
        component::mc_attr::McAttributes, component::mc_pins::McPins, mc_func::{HasFindInst, ShapeCtx},
    },
    McIds, McInstance, McURI,
};

#[derive(Debug, Clone)]
pub struct McInterface {
    pub name: McIds,
    pub params: McParamDeclares,
    pub attrs: McAttributes,
    pub pins: McPins,
    pub roles: Vec<McRole>,
    pub body: AstNode,
    pub uri: McURI,
    pub span: crate::ast::sem::Span, // ★ LSP: span for goto definition
}

impl McInterface {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_COMPONENT
        // |- MCAST_NAME - MCAST_PARAM (option) - MCAST_BODY
        let subnodes = node.get_sub_node()?;
        let body_node = subnodes.iter().find(|x| x.is_type(MCAST_BODY))?;
        let name_node = subnodes.iter().find(|x| x.is_type(MCAST_NAME))?;
        // ★ LSP: Span from the interface name (MCAST_NAME → MCAST_IDS)
        let ids_node = name_node.get_sub_node()?;
        let start = ids_node.get_pos() as usize;
        let end = start + ids_node.get_len() as usize;
        let span = crate::ast::sem::Span { start, end };

        let mut ret = Self {
            name: McIds::new_with_dot(&name_node.get_sub_node()?)?,
            params: McParamDeclares::new(),
            attrs: McAttributes::new(),
            pins: McPins::new(),
            roles: Vec::new(),
            body: body_node.clone(),
            uri: uri.clone(),
            span,
        };

        //2. param
        let _ = &subnodes
            .iter()
            .find(|x| x.is_type(MCAST_PARAMS))
            .map(|param_node| ret.params.parse(&param_node));

        //3. body: the clauses the body holds, an in-body partition made
        // transparent (`block`'s contents belong to the enclosing body).
        let body_subnodes = body_node.clause_list();
        if !body_subnodes.is_empty() {
            //3. attributes
            body_subnodes
                .iter()
                .filter(|x| x.is_type(MCAST_ATTRIBUTE))
                .for_each(|x| ret.attrs.parse(&x));

            //3.5. roles
            for child in body_subnodes.iter().filter(|x| x.is_type(MCAST_ROLE)) {
                if let Some(role) = McRole::new(&child) {
                    ret.roles.push(role);
                }
            }

            //4. pins - parse pin definitions without conditions
            body_subnodes
                .iter()
                .filter(|x| x.is_type(MCAST_ATTRIBUTE_PIN) || x.is_type(MCAST_ATTRIBUTE_PINADD))
                .for_each(|x| ret.pins.parse(&x));

            //5. conditional chains — every branch of every chain materializes
            // into the pin table (U346 ②): the face used to read a single
            // branch, so chains contributed nothing (or a silent subset).
            for child in body_subnodes.iter().filter(|x| x.is_type(MCAST_COND_IF)) {
                Self::parse_cond_chain_pins(&mut ret.pins, child);
            }

            //6. U346 ①: `mc_body` is shared with component/module bodies, so
            // any clause kind is grammar-legal here, but only attrs/roles/
            // pins (and the conditional chains above) carry interface
            // semantics. Every other clause was dropped silently — report it.
            for child in body_subnodes.iter() {
                if !Self::is_interface_clause(child.get_type()) {
                    crate::db::diagnostic::diagnostic::dlog_error(
                        crate::errcodes::INTERFACE_CLAUSE_UNSUPPORTED,
                        child,
                        "this clause is not accepted in an interface body — an interface \
                         body carries attributes, roles, pin tables, and conditional pin \
                         chains only",
                    );
                }
            }
        }

        // ★ LSP: Scan body for references to interface parameters
        let ifs_name = ret.name.to_string();
        crate::semantic::component::McComponent::collect_param_refs_in_body(
            &body_node,
            &mut ret.params,
            &ifs_name,
        );

        // ★ Smart Param (M5): Finalize after body parsed
        let diags = ret.params.finalize(Some(&body_node), &ifs_name);
        for d in &diags {
            crate::mcc_log_global_diag(d);
        }

        Some(ret)
    }

    /// Whether a body clause kind carries interface semantics (U346 ①):
    /// attributes (plain and `+=`), roles, pin tables, and the conditional
    /// chains materialized above. Anything else is diagnosed, not dropped.
    fn is_interface_clause(t: u16) -> bool {
        t == MCAST_ATTRIBUTE
            || t == MCAST_ATTRIBUTE_ADD
            || t == MCAST_ROLE
            || t == MCAST_ATTRIBUTE_PIN
            || t == MCAST_ATTRIBUTE_PINADD
            || t == MCAST_COND_IF
    }

    /// Parse pin rows from EVERY branch of one conditional chain (U346 ②).
    ///
    /// The declaration-time pin table is the union shape of the interface:
    /// the adoption site (`Mc2Interface::with_params`) picks the branch its
    /// parameter bindings select, so the declaration must have registered
    /// them all. Branch blocks arrive either as a bare pin row node or as a
    /// block/body whose direct children are pin rows (same shapes the
    /// component face's conditional fold reads).
    ///
    /// The default (`else`) branch materializes FIRST: it is the branch the
    /// def face alone used to register, so its rows keep the member
    /// declaration order they always had (`decl_order`); the conditional
    /// arms then extend the union. Name-expression rows that cannot resolve
    /// against the declaration (`"VCC" + canon(volt)` with `volt` unbound)
    /// ride the dynamic path and register at adoption, as before.
    fn parse_cond_chain_pins(pins: &mut McPins, chain: &AstNode) {
        let Some(conds) = McConds::new(chain) else {
            return;
        };
        if let Some(block) = &conds.else_block {
            Self::parse_cond_branch_pins(pins, block);
        }
        for cond in &conds.if_blocks {
            Self::parse_cond_branch_pins(pins, &cond.block);
        }
    }

    /// Parse the pin rows of one conditional branch block.
    fn parse_cond_branch_pins(pins: &mut McPins, block: &AstNode) {
        let block_type = block.get_type();
        if block_type == MCAST_ATTRIBUTE_PIN || block_type == MCAST_ATTRIBUTE_PINADD {
            pins.parse(block);
            return;
        }
        if block_type == MCAST_BODY || block_type == MCAST_COND_BLOCK {
            if let Some(sub) = block.get_sub_node() {
                for inner in sub.iter() {
                    let t = inner.get_type();
                    if t == MCAST_ATTRIBUTE_PIN || t == MCAST_ATTRIBUTE_PINADD {
                        pins.parse(&inner);
                    }
                }
            }
        }
    }
}

// HasFindInst for McInterface — namespace lookup (Phase 4.5)

impl ShapeCtx for McInterface {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    fn uri(&self) -> &crate::McURI {
        &self.uri
    }
}

impl HasFindInst for McInterface {
    fn find_inst_mut(&mut self, _id: &str) -> Option<&mut crate::McInstance> {
        None // Interface body has no mutable net statements at Pass1
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        // Interface category chain (§3.3): ① params → ② pin names.
        crate::semantic::scope::interface_scope(self)
            .resolve(id)
            .map(|r| (r.inst, r.span))
    }

    fn add_label_at(
        &mut self,
        _name: String,
        _span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        None // No-ops for interface body (no net statements)
    }

    fn add_component(
        &mut self,
        _name: String,
        _comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase> {
        None
    }

    fn add_module(
        &mut self,
        _name: String,
        _module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        None
    }

    fn add_bus(&mut self, _name: String, _members: Vec<String>) -> Option<McPhrase> {
        None
    }

    fn add_list(&mut self, _name: String, _members: Vec<String>) -> Option<McPhrase> {
        None
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

    fn upgrade_label_to_bus(&mut self, _name: &str) -> bool {
        false
    }

    fn parse_declare(&mut self, _node: &AstNode) -> Vec<McInstance> {
        Vec::new()
    }

    fn gen_anon_name(&mut self, _classname: &str) -> String {
        String::new()
    }
}

// Display implementation - compact format output

impl std::fmt::Display for McInterface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Interface {}", self.name)?;
        write!(f, "{}", self.pins)?;

        // Display roles
        if !self.roles.is_empty() {
            writeln!(f, "  Roles:")?;
            for role in &self.roles {
                writeln!(f, "    role {}:", role.name)?;
                write!(f, "{}", role.pins)?;
            }
        }

        Ok(())
    }
}

// Mc2Interface - Interface instance wrapper

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::errcodes;
use crate::semantic::basic::mc_param::McParamValue;
use crate::semantic::mc_inst::McInst;

#[derive(Clone)]
pub struct Mc2Interface {
    pub base: Arc<McInterface>,
    pub name: McIds,
    pub params: Vec<McParamValue>,
    pub insts: Vec<McInst>,
    pub registered_pins: Vec<String>, // List of registered chip pin IDs
    pub parsed_pins: Option<McPins>,  // Parameterized pin definitions
    pub pin_name_mapping: Vec<String>, // Pin name mapping (e.g., [Vin, GND])
}

impl Mc2Interface {
    pub fn new(name: McIds, base: Arc<McInterface>) -> Self {
        Self {
            name,
            base,
            params: Vec::new(),
            insts: Vec::new(),
            registered_pins: Vec::new(),
            parsed_pins: None,
            pin_name_mapping: Vec::new(),
        }
    }

    pub fn new_with_str(name: &str, base: Arc<McInterface>) -> Self {
        Self {
            name: McIds::from(name),
            base,
            params: Vec::new(),
            insts: Vec::new(),
            registered_pins: Vec::new(),
            parsed_pins: None,
            pin_name_mapping: Vec::new(),
        }
    }

    /// The member names of this interface instance in connection-ordinal
    /// order (§11.1 / CIMP §1 U150): the declared role's own table when the
    /// instantiation names a role, else the interface base pin table — both
    /// read through `McPins::member_names` (declaration order; an anonymous
    /// conductor-view pin reads as `_(<pinid>)` and still occupies its
    /// sequence slot). Never the BTreeMap pinid key order. Written member
    /// spellings (`SPI{CS,..}` / `[A,B]`) are the author's own local view and
    /// are consumed by the callers before this fallback.
    pub fn ordinal_member_names(&self) -> Vec<String> {
        if let Some(McParamValue::Ids(role_ids)) = self.params.first() {
            let role_name = role_ids.to_string();
            for role in &self.base.roles {
                if role.name.to_string() == role_name {
                    let names = role.pins.member_names();
                    if !names.is_empty() {
                        return names;
                    }
                    break;
                }
            }
        }
        self.base.pins.member_names()
    }

    /// `anchor` is the consumer's own syntax, used to position a condition that
    /// cannot be evaluated; see [`McConds::evaluate`]. It is used only when the
    /// arguments are literal-complete (see [`Self::args_are_literals`]).
    pub fn with_params(
        name: &str,
        base: Arc<McInterface>,
        params: Vec<McParamValue>,
        anchor: Option<&AstNode>,
    ) -> Self {
        let param_names = base.params.names();
        Self::report_arg_excess(name, &params, param_names.len(), anchor);
        let raw_anchor = anchor;
        let anchor = anchor.filter(|_| Self::args_are_literals(&params, param_names.len()));
        let param_tuples: Vec<CondParam> = params
            .iter()
            .zip(param_names.iter())
            .filter_map(|(p, param_name)| {
                let s = format!("{p}");
                if s == "_" || s.is_empty() {
                    None
                } else {
                    Some(CondParam {
                        name: McIds::from(param_name.as_str()),
                        text: s,
                        family: p.cond_family(),
                    })
                }
            })
            .collect();

        let mut inst = Self {
            name: McIds::from(name),
            base: base.clone(),
            params: params.clone(),
            insts: Vec::new(),
            registered_pins: Vec::new(),
            parsed_pins: None,
            pin_name_mapping: Vec::new(),
        };

        if let Some(ref cond_block) = inst.base.body.get_sub_node() {
            if let Some(conds) = McConds::new(cond_block) {
                if let Some(selected_block) = conds.evaluate(&param_tuples, None, anchor) {
                    // U216: the environment rides down, so a computed name in
                    // the selected branch (`"VCC" + canon(volt)`) materializes
                    // here, in declaration order.
                    let values: Vec<(String, String)> = param_tuples
                        .iter()
                        .map(|p| (p.name.to_string(), p.text.clone()))
                        .collect();
                    inst.parsed_pins =
                        Self::parse_pins_from_block(&inst.base.uri, &selected_block, &values);
                }
            }
        }
        Self::report_unbound_rows(
            name,
            base.as_ref(),
            &param_names,
            &params,
            match &inst.parsed_pins {
                Some(parsed) => parsed,
                None => &base.pins,
            },
            raw_anchor,
        );

        inst
    }

    /// Create Mc2Interface with McIds name and params (for component pin parsing)
    ///
    /// `anchor` is the consumer's own syntax, used to position a condition that
    /// cannot be evaluated; see [`McConds::evaluate`]. It is used only when the
    /// arguments are literal-complete (see [`Self::args_are_literals`]).
    pub fn with_ids_and_params(
        name: McIds,
        base: Arc<McInterface>,
        params: Vec<McParamValue>,
        anchor: Option<&AstNode>,
    ) -> Self {
        let param_names = base.params.names();
        let display_name = name.to_string();
        Self::report_arg_excess(&display_name, &params, param_names.len(), anchor);
        let raw_anchor = anchor;
        let anchor = anchor.filter(|_| Self::args_are_literals(&params, param_names.len()));

        let param_tuples: Vec<CondParam> = params
            .iter()
            .zip(param_names.iter())
            .filter_map(|(p, param_name)| {
                let s = format!("{p}");
                if s == "_" || s.is_empty() {
                    None
                } else {
                    Some(CondParam {
                        name: McIds::from(param_name.as_str()),
                        text: s,
                        family: p.cond_family(),
                    })
                }
            })
            .collect();

        let mut inst = Self {
            name,
            base: base.clone(),
            params: params.clone(),
            insts: Vec::new(),
            registered_pins: Vec::new(),
            parsed_pins: None,
            pin_name_mapping: Vec::new(),
        };

        // Iterate through all children of body to find conditional blocks
        if let Some(body_subnodes) = inst.base.body.get_sub_node() {
            for child in body_subnodes.iter() {
                let child_type = child.get_type();

                if child_type == MCAST_COND_IF {
                    if let Some(conds) = McConds::new(&child) {
                        if let Some(selected_block) = conds.evaluate(&param_tuples, None, anchor) {
                            // U216: the environment rides down, so a computed
                            // name in the selected branch materializes here.
                            let values: Vec<(String, String)> = param_tuples
                                .iter()
                                .map(|p| (p.name.to_string(), p.text.clone()))
                                .collect();
                            inst.parsed_pins = Self::parse_pins_from_block(
                                &inst.base.uri,
                                &selected_block,
                                &values,
                            );
                            break; // Found matching condition, stop searching
                        }
                    }
                }
            }
        }
        Self::report_unbound_rows(
            &display_name,
            base.as_ref(),
            &param_names,
            &params,
            match &inst.parsed_pins {
                Some(parsed) => parsed,
                None => &base.pins,
            },
            raw_anchor,
        );

        inst
    }

    /// Whether the construction arguments are literal-complete: exactly one
    /// value literal per declared parameter, no fewer.
    ///
    /// A symbolic argument (`::DC(volt)`) or an absent one (`::DC()`) leaves the
    /// received parameter unreduced, so a condition over it is undecided — the
    /// value may still arrive from the instance or the spec — and must not be
    /// reported as an operator error.
    ///
    /// A **window** literal (`::DC(2.5V~5.5V)`, `5V±5%`) is decided as to its
    /// endpoints but undecided as to any single compare over it: the value
    /// layer keeps window forms undecoded by design (doc/eval), so `volt ==
    /// 1.2V` against a window has no truth value — the argument is the
    /// consumer's honest input-range declaration, not an operator error, and
    /// branch selection falls through to the interface's `else` exactly as a
    /// scalar outside every tested class does.
    fn args_are_literals(params: &[McParamValue], arity: usize) -> bool {
        params.len() == arity
            && params.iter().all(|p| {
                p.is_literal()
                    && !matches!(p, McParamValue::UValue(uv) if uv.is_range_or_plusminus())
            })
    }

    /// E3190 (U347 C1): constructor arguments past the declared formal table
    /// are dropped by the cond-environment zip — report the surplus instead of
    /// letting it vanish.
    fn report_arg_excess(
        name: &str,
        params: &[McParamValue],
        arity: usize,
        anchor: Option<&AstNode>,
    ) {
        let Some(node) = anchor else {
            return;
        };
        if params.len() <= arity {
            return;
        }
        let passed = params.len().to_string();
        let declared = arity.to_string();
        dlog_error(
            errcodes::IFACE_ARG_EXCESS,
            node,
            &errcodes::format_msg(
                errcodes::IFACE_ARG_EXCESS,
                &[&name, &declared, &passed],
            ),
        );
    }

    /// E3191 (U347 C2): a computed pin-row name in the effective view (the
    /// selected cond branch, else the base table) resolves against this
    /// adoption's arguments; a row whose parameter no argument binds lands in
    /// the table's dynamic pins, which this face never resolves — the row
    /// would vanish with no pin and no word. A symbolic argument
    /// (`::DC(volt)`) is a binding as written and stays quiet; a declared
    /// default covers its parameter.
    fn report_unbound_rows(
        name: &str,
        base: &McInterface,
        param_names: &[String],
        params: &[McParamValue],
        effective: &McPins,
        anchor: Option<&AstNode>,
    ) {
        if effective.dynamic_pins.is_empty() {
            return;
        }
        let Some(node) = anchor else {
            return;
        };
        let defaults: HashSet<String> = base
            .params
            .get_params_with_defaults()
            .into_iter()
            .filter_map(|(name, _)| name.get_primary_name())
            .collect();
        let unbound: Vec<String> = param_names
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                if defaults.contains(*n) {
                    return false;
                }
                params
                    .get(*i)
                    .map(|p| {
                        let s = format!("{p}");
                        s.is_empty() || s == "_"
                    })
                    .unwrap_or(true)
            })
            .map(|(_, n)| n.clone())
            .collect();
        if unbound.is_empty() {
            return;
        }
        let list = unbound.join(", ");
        dlog_error(
            errcodes::IFACE_ROW_UNBOUND_PARAM,
            node,
            &errcodes::format_msg(errcodes::IFACE_ROW_UNBOUND_PARAM, &[&name, &list]),
        );
    }

    /// Parse an interface body's pin block. `uri` is the file that owns the
    /// block, not the file being parsed: the interface may be declared in
    /// another file (or a library) while the current file is only the one
    /// instantiating it, and every diagnostic this raises belongs to the
    /// declaration's own syntax (CIMP U39).
    fn parse_pins_from_block(uri: &McURI, block: &AstNode, values: &[(String, String)]) -> Option<McPins> {
        let _uri = crate::current_uri::UriGuard::new(uri);

        let mut pins = McPins::new();

        // If the block itself is an ATTRIBUTE_PIN or ATTRIBUTE_PINADD, parse it directly
        if block.is_type(MCAST_ATTRIBUTE_PIN) || block.is_type(MCAST_ATTRIBUTE_PINADD) {
            pins.parse_with_values(block, values);
            return Some(pins);
        }

        // Otherwise, look for ATTRIBUTE_PIN or ATTRIBUTE_PINADD in subnodes
        if let Some(subnodes) = block.get_sub_node() {
            subnodes
                .iter()
                .filter(|x| x.is_type(MCAST_ATTRIBUTE_PIN) || x.is_type(MCAST_ATTRIBUTE_PINADD))
                .for_each(|x| {
                    pins.parse_with_values(&x, values);
                });
            Some(pins)
        } else {
            None
        }
    }

    /// Get the number of interface pins
    pub fn pin_count(&self) -> usize {
        self.base.pins.names_to_id.len()
    }

    /// Get base interface name (for matching same-type interfaces)
    pub fn base_name(&self) -> String {
        self.base.name.to_string()
    }

    /// Merge two interfaces' pins (used for merging same-type interfaces)
    /// Return a new Mc2Interface containing merged pins
    pub fn merge_with(&self, other: &Mc2Interface) -> Self {
        // If base interface names differ, cannot merge
        if self.base_name() != other.base_name() {
            return self.clone();
        }

        // Merge pin info - directly add all pins, regardless of whether name already exists
        let mut new_pins = self.base.pins.clone();

        for (name, pin) in &other.base.pins.names_to_id {
            // Check if this pin already exists (by comparing pin info)
            let pin_exists =
                new_pins
                    .names_to_id
                    .values()
                    .any(|existing_pin| match (existing_pin, pin) {
                        (
                            crate::semantic::component::mc_pins::McPinPort::Single(e),
                            crate::semantic::component::mc_pins::McPinPort::Single(p),
                        ) => e == p,
                        _ => false,
                    });

            if !pin_exists {
                // Create a unique name (based on pin value)
                let new_name = match pin {
                    crate::semantic::component::mc_pins::McPinPort::Single(pid) => pid.clone(),
                    crate::semantic::component::mc_pins::McPinPort::Multi(pids) => pids.join(","),
                    _ => name.clone(),
                };
                new_pins.names_to_id.insert(new_name, pin.clone());
            }
        }

        // Create a new base interface
        let mut new_base = (*self.base).clone();
        new_base.pins = new_pins;

        Self {
            base: Arc::new(new_base),
            name: self.name.clone(),
            params: self.params.clone(),
            insts: self.insts.clone(),
            registered_pins: self.registered_pins.clone(),
            parsed_pins: self.parsed_pins.clone(),
            pin_name_mapping: self.pin_name_mapping.clone(),
        }
    }

    /// Merge pin number list into interface
    /// Used to merge pin numbers from multiple GPIO instances into same GPIO interface
    /// Note: only updates registered_pins, doesn't modify base.pins.names_to_id (that's Interface
    /// definition)
    pub fn merge_pins_with(&self, pins: &[String]) -> Self {
        // No longer modify base.pins.names_to_id, only update registered_pins
        // base.pins.names_to_id should remain as Interface definition (e.g. {IO})

        // Update registered_pins
        let mut new_registered = self.registered_pins.clone();
        for pin_id in pins {
            if !new_registered.contains(pin_id) {
                new_registered.push(pin_id.clone());
            }
        }

        Self {
            base: self.base.clone(),
            name: self.name.clone(),
            params: self.params.clone(),
            insts: self.insts.clone(),
            registered_pins: new_registered,
            parsed_pins: self.parsed_pins.clone(),
            pin_name_mapping: self.pin_name_mapping.clone(),
        }
    }
}

// Debug implementation - simplified format output

impl std::fmt::Debug for Mc2Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format parameters (if any)
        let params_str = if self.params.is_empty() {
            String::new()
        } else {
            format!(
                "({})",
                self.params
                    .iter()
                    .map(|p| format!("{p:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        // Format interface instance name
        // If pure Square form (e.g. [LX, GND]), convert to Curly form ({LX,GND})
        let name_str = if self.name.is_list() {
            if let Some(members) = self.name.list_members() {
                format!("{{{}}}", members.join(","))
            } else {
                format!("{}", self.name)
            }
        } else {
            format!("{}", self.name)
        };

        // Output format: "{LX,GND}::DC(5V) or VIN{Vin,GND}::DC(5V)
        write!(f, "{}::{}{}", name_str, self.base.name, params_str)
    }
}
