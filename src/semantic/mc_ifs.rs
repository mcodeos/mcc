// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::sync::Arc;

use crate::semantic::basic::mc_conds::McConds;
use crate::{
    ast::{ast_node::AstNode, c_macros::*},
    semantic::{
        basic::mc_param::McParamDeclares, basic::mc_phrase::McPhrase, basic::mc_role::McRole,
        component::mc_attr::McAttributes, component::mc_pins::McPins, mc_func::HasFindInst,
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
    pub span: crate::ast::ast_semantic::Span, // ★ LSP: span for goto definition
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
        let span = crate::ast::ast_semantic::Span { start, end };

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

        //3. body: get subnode of body which contains the clauses
        if let Some(body_subnodes) = body_node.get_sub_node() {
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

            //5. parse pin definitions in the first conditional branch
            Self::parse_first_cond_pins(&mut ret.pins, &body_node);
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

    /// Parse pin definitions in the first conditional branch
    fn parse_first_cond_pins(pins: &mut McPins, body_node: &AstNode) {
        if let Some(subnodes) = body_node.get_sub_node() {
            for child in subnodes.iter() {
                let child_ref = &child;
                let child_type = child_ref.get_type();
                // Directly check if it's a COND_IF node
                if child_type == MCAST_COND_IF {
                    // Found COND_IF, parse its pins from the ELSE (default) branch.
                    // COND_IF structure may be: [cond_expr?, pins?, cond_block1?, COND_ELSE_IF*, COND_ELSE?]
                    if let Some(cond_subnodes) = child_ref.get_sub_node() {
                        // First pass: find the last COND_ELSE block (default branch).
                        // If no COND_ELSE, use the last COND_BLOCK.
                        let mut last_block: Option<AstNode> = None;
                        let mut direct_pins: Option<AstNode> = None;
                        for cond_child in cond_subnodes.iter() {
                            let cond_child_type = cond_child.get_type();
                            if cond_child_type == MCAST_COND_BLOCK
                                || cond_child_type == MCAST_COND_ELSE
                            {
                                last_block = Some(cond_child.clone());
                            } else if cond_child_type == MCAST_ATTRIBUTE_PIN
                                || cond_child_type == MCAST_ATTRIBUTE_PINADD
                            {
                                direct_pins = Some(cond_child.clone());
                            }
                        }
                        // Prefer the last block (ELSE/default), fallback to direct pins
                        if let Some(ref block) = last_block {
                            if let Some(block_subnodes) = block.get_sub_node() {
                                for block_child in block_subnodes.iter() {
                                    let block_child_type = block_child.get_type();
                                    if block_child_type == MCAST_ATTRIBUTE_PIN
                                        || block_child_type == MCAST_ATTRIBUTE_PINADD
                                    {
                                        pins.parse(&block_child);
                                    }
                                }
                            }
                        } else if let Some(ref dp) = direct_pins {
                            pins.parse(dp);
                        }
                    }
                    break;
                }
            }
        }
    }
}

// ============================================================================
// HasFindInst for McInterface — namespace lookup (Phase 4.5)
// ============================================================================

impl HasFindInst for McInterface {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

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

    fn uri(&self) -> &crate::McURI {
        &self.uri
    }

    fn parse_declare(&mut self, _node: &AstNode) -> Vec<McInstance> {
        Vec::new()
    }

    fn gen_anon_name(&mut self, _classname: &str) -> String {
        String::new()
    }
}

// ============================================================================
// Display implementation - compact format output
// ============================================================================

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

// ============================================================================
// Mc2Interface - Interface instance wrapper
// ============================================================================

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

    pub fn with_params(name: &str, base: Arc<McInterface>, params: Vec<McParamValue>) -> Self {
        let param_names = base.params.names();
        let param_tuples: Vec<(McIds, String)> = params
            .iter()
            .zip(param_names.iter())
            .filter_map(|(p, param_name)| {
                let s = format!("{p}");
                if s == "_" || s.is_empty() {
                    None
                } else {
                    Some((McIds::from(param_name.as_str()), s))
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
                if let Some(selected_block) = conds.evaluate(&param_tuples) {
                    inst.parsed_pins = Self::parse_pins_from_block(&selected_block);
                }
            }
        }

        inst
    }

    /// Create Mc2Interface with McIds name and params (for component pin parsing)
    pub fn with_ids_and_params(
        name: McIds,
        base: Arc<McInterface>,
        params: Vec<McParamValue>,
    ) -> Self {
        let param_names = base.params.names();

        let param_tuples: Vec<(McIds, String)> = params
            .iter()
            .zip(param_names.iter())
            .filter_map(|(p, param_name)| {
                let s = format!("{p}");
                if s == "_" || s.is_empty() {
                    None
                } else {
                    Some((McIds::from(param_name.as_str()), s))
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
                        if let Some(selected_block) = conds.evaluate(&param_tuples) {
                            inst.parsed_pins = Self::parse_pins_from_block(&selected_block);
                            break; // Found matching condition, stop searching
                        }
                    }
                }
            }
        }

        inst
    }

    fn parse_pins_from_block(block: &AstNode) -> Option<McPins> {
        let mut pins = McPins::new();

        // If the block itself is an ATTRIBUTE_PIN or ATTRIBUTE_PINADD, parse it directly
        if block.is_type(MCAST_ATTRIBUTE_PIN) || block.is_type(MCAST_ATTRIBUTE_PINADD) {
            pins.parse(block);
            return Some(pins);
        }

        // Otherwise, look for ATTRIBUTE_PIN or ATTRIBUTE_PINADD in subnodes
        if let Some(subnodes) = block.get_sub_node() {
            subnodes
                .iter()
                .filter(|x| x.is_type(MCAST_ATTRIBUTE_PIN) || x.is_type(MCAST_ATTRIBUTE_PINADD))
                .for_each(|x| {
                    pins.parse(&x);
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

    /// Get all pin names list (for merging)
    pub fn get_all_pin_ids(&self) -> Vec<String> {
        let mut pin_names: Vec<String> = self.base.pins.names_to_id.keys().cloned().collect();
        pin_names.sort();
        pin_names.dedup();
        pin_names
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
    /// Note: only updates registered_pins, doesn't modify base.pins.names_to_id (that's Interface definition)
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

// ============================================================================
// Debug implementation - simplified format output
// ============================================================================

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
