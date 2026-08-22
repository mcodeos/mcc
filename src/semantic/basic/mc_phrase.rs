// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::super::{
    basic::mc_bus::McBus,
    basic::mc_closure::McClosure,
    basic::mc_endpoint::{McEndpoint, McInstanceRef},
    basic::mc_fcall::{McFuncCall, ReturnShape},
    basic::mc_group::McGroup,
    common::{representative, ConnDir, ConnOp, IOType, McCMIE, Shape},
    component::Mc2Component,
    mc_func::HasFindInst,
    mc_inst::McInstance,
    module::Mc2Module,
};
use crate::{
    ast::{ast_node::AstNode, c_macros::*, error::message::*},
    db::{
        context::DB,
        diagnostic::diagnostic::{dlog_error, dlog_trace, dlog_warning},
    },
    query::refs::{mcb_register_declare_class, mcb_register_instance_ref},
    refdef::types::SymbolKind,
    semantic::{
        basic::{mc_opd::McOpd, mc_param::McParamValue, opd_shape::OpdShape},
        component::mc_pins::McPinPort,
        context::resolve_cmie,
        instref::validate_inst_reference,
        mc_ifs::Mc2Interface,
    },
    McIds,
};

use std::ops::{Add, Shr};
use std::sync::Arc;

/// P5.1: a prefix identifier `_X` (e.g. `_OPEN`, `__CLR`) is treated as an **independent
/// operand** and does not resolve to any instance — per §1 it is really a named member
/// of an IDA index (`M[1:4][_OPEN,_CLOSE]`), not a wire `_`.
/// If the user intends a passthrough, they should write `_`. A declared `_X` label is
/// a valid label, so this path does not trigger (find_inst hits).
fn warn_prefix_id_as_wire(node: &AstNode, name: &str) {
    if name.len() > 1 && name.starts_with('_') {
        dlog_warning(
            crate::errcodes::LEAD_PREFIX_ID_AS_WIRE,
            node,
            &crate::errcodes::format_msg(crate::errcodes::LEAD_PREFIX_ID_AS_WIRE, &[&name]),
        );
    }
}

// ============================================================================
// McPhrase
// ============================================================================

#[derive(Debug, Clone)]
pub enum McPhrase {
    Lead,
    Endpoint(McEndpoint),
    Series(Vec<McPhrase>, ConnDir),
    Parallel(Vec<McPhrase>),
    Multiple(Vec<McPhrase>),
    Group(McGroup),
    Transposed(Box<McPhrase>),
    Closure(McClosure),
    FuncCall(McFuncCall),
    Member(Box<McPhrase>, McEndpoint),
}

impl McPhrase {
    /// Create endpoint
    pub fn ep(ref_: McInstanceRef) -> Self {
        McPhrase::Endpoint(McEndpoint::single(ref_))
    }

    /// Create label endpoint
    pub fn label(name: String) -> Self {
        McPhrase::ep(McInstanceRef::new(McInstance::Label(name)))
    }

    /// Create list endpoint
    pub fn ep_list(eps: Vec<McPhrase>) -> Self {
        McPhrase::Endpoint(McEndpoint::list(
            eps.into_iter().map(|p| p.into_endpoint()).collect(),
        ))
    }

    /// Create pair endpoint
    pub fn ep_node(input: Vec<McPhrase>, output: Vec<McPhrase>) -> Self {
        McPhrase::Endpoint(McEndpoint::node(
            input.into_iter().map(|p| p.into_endpoint()).collect(),
            output.into_iter().map(|p| p.into_endpoint()).collect(),
        ))
    }

    /// Series (auto-flatten)
    pub fn series(phrases: Vec<McPhrase>) -> Self {
        let mut flat = Vec::new();
        for p in phrases {
            match p {
                McPhrase::Series(items, _) => flat.extend(items),
                other => flat.push(other),
            }
        }
        match flat.len() {
            0 => McPhrase::Series(vec![], ConnDir::Undirected),
            1 => flat.into_iter().next().unwrap(),
            _ => McPhrase::Series(flat, ConnDir::Undirected),
        }
    }

    /// Parallel (auto-flatten)
    pub fn parallel(phrases: Vec<McPhrase>) -> Self {
        let mut flat = Vec::new();
        for p in phrases {
            match p {
                McPhrase::Parallel(items) => flat.extend(items),
                other => flat.push(other),
            }
        }
        match flat.len() {
            0 => McPhrase::Parallel(vec![]),
            1 => flat.into_iter().next().unwrap(),
            _ => McPhrase::Parallel(flat),
        }
    }

    /// Convert to endpoint
    pub fn into_endpoint(self) -> McEndpoint {
        match self {
            McPhrase::Endpoint(ep) => ep,
            McPhrase::Lead => {
                McEndpoint::single(McInstanceRef::new(McInstance::Label("(lead)".to_string())))
            }
            other => McEndpoint::single(McInstanceRef::new(McInstance::Label(other.to_string()))),
        }
    }

    pub(crate) fn new(node: &AstNode, context: &mut dyn HasFindInst) -> Option<Self> {
        use McPhrase::*;
        let scope = context.scope_name();
        let node_type = node.get_type();
        match node_type {
            MCAST_OPD_USCORE => Some(McPhrase::Lead),

            MCAST_OPD_THIS => {
                // this.X ≡ X (pins transparency) — resolve member through find_inst
                if let Some(nextnode) = node.get_next() {
                    let member_ids = McIds::new(&nextnode);
                    if let Some(member) = member_ids {
                        let member_str = member.to_string();
                        if let Some(inst) = context.find_inst(&member_str) {
                            return Some(McPhrase::Endpoint(McEndpoint::Single(
                                McInstanceRef::new(inst),
                            )));
                        }
                    }
                    // fallback: existing behavior
                    let mut this_ids = McIds::from("this");
                    this_ids.append(&nextnode);
                    context.add_label(this_ids.to_string())
                } else {
                    // bare "this" — keep as label (2-pin passthrough handled later)
                    context.add_label("this".to_string())
                }
            }

            MCAST_OPD => {
                // Handle MCAST_OPD node - extract subnode and process as McOpd
                let subnode = node.get_sub_node()?;
                // ── P2-11: Handle nested FuncCall inside MCAST_OPD ──
                let subnode_type = subnode.get_type();
                let subnode_str = subnode.to_string().unwrap_or_default();
                mcc_dbg!("sem::conds", 
                    "[P2-OPD-DBG] MCAST_OPD subnode_type={subnode_type} MCAST_OPD_FCALL={MCAST_OPD_FCALL} subnode_str={subnode_str:?}"
                );
                if subnode_type == MCAST_OPD_FCALL {
                    return McFuncCall::parse(&subnode, context);
                }
                if let Some(opdc) = McOpd::new(&subnode) {
                    // Convert McOpd to McPhrase
                    match opdc {
                        McOpd::Id(ids) => {
                            let ids_str = ids.to_string();
                            // eprintln!(
                            //     "[PHRASE_DEBUG] OPD Id: ids={:?}, is_curly={}",
                            //     ids_str, is_curly
                            // );
                            let mut items: Vec<Option<crate::McInstance>> =
                                vec![context.find_inst(&ids_str)];
                            // eprintln!(
                            //     "[PHRASE_DEBUG] OPD Id: find_inst result={:?}",
                            //     items[0].is_some()
                            // );
                            if let Some(ident) = items.remove(0) {
                                // ── P2-4: square bracket Interface → expand to Multiple ──
                                // When a square bracket list like `[VDD_3V3, GND]` is found
                                // as an McInstance::Interface in the symbol table, expand it
                                // to individual member labels. Otherwise the bracket list is
                                // returned as a single phrase, and get_left_points /
                                // get_right_points return empty for Interface, causing the
                                // entire body line connection to be silently dropped.
                                if ids.is_square_bracket()
                                    && matches!(ident, McInstance::Interface(_))
                                {
                                    let expanded = ids.expand();
                                    if expanded.len() >= 2 {
                                        let phrases: Vec<McPhrase> = expanded
                                            .into_iter()
                                            .map(|m| {
                                                context
                                                    .add_label(m.clone())
                                                    .unwrap_or_else(|| McPhrase::label(m))
                                            })
                                            .collect();
                                        return Some(McPhrase::Multiple(phrases));
                                    }
                                }
                                // ★ LSP: Register instance reference for MCAST_OPD path
                                let span = (subnode.get_pos() as usize)
                                    ..((subnode.get_pos() + subnode.get_len()) as usize);
                                if let Some(decl_id) = crate::query::refs::mcb_lookup_instance_decl(
                                    context.uri(),
                                    &ids.to_string(),
                                    scope.as_deref(),
                                ) {
                                    mcb_register_instance_ref(
                                        context.uri(),
                                        span,
                                        decl_id,
                                        scope.as_deref(),
                                    );
                                }
                                Some(ident.into())
                            } else if ids.is_curly_bracket() {
                                let bus_info = ids.as_bus();
                                let _comp_member = ids.as_component_member();
                                // eprintln!(
                                //     "[PHRASE_DEBUG] curly: ids={:?}, as_bus={:?}, as_comp_member={:?}",
                                //     ids_str, bus_info, comp_member
                                // );

                                if let Some(result) = validate_inst_reference(&ids, context, node) {
                                    // eprintln!(
                                    //     "[PHRASE_DEBUG] curly: validate_inst_reference -> Some"
                                    // );
                                    if bus_info.is_some() {
                                        let span = (subnode.get_pos() as usize)
                                            ..((subnode.get_pos()
                                                + subnode.get_sub_node()?.get_len())
                                                as usize);
                                        if let Some(decl_id) =
                                            crate::query::refs::mcb_lookup_instance_decl(
                                                context.uri(),
                                                &bus_info.unwrap().0,
                                                scope.as_deref(),
                                            )
                                        {
                                            mcb_register_instance_ref(
                                                context.uri(),
                                                span,
                                                decl_id,
                                                scope.as_deref(),
                                            );
                                        }
                                    }
                                    // TODO for comp_member
                                    // TODO as_bus and as_component_member are only simple workarounds
                                    // Need to be able to deal with more general / complicated patterns
                                    return Some(result);
                                }
                                // eprintln!("[PHRASE_DEBUG] curly: validate_inst_reference -> None");
                                if let Some((name, members)) = ids.as_bus() {
                                    if context.find_inst(&name).is_some() {
                                        dlog_error(
                                            crate::errcodes::BUS_NAME_ALREADY_INSTANCE,
                                            node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::BUS_NAME_ALREADY_INSTANCE,
                                                &[
                                                    &name as &dyn std::fmt::Display,
                                                    &members.join(", ") as &dyn std::fmt::Display,
                                                ],
                                            ),
                                        );
                                        return None;
                                    } else {
                                        let name_clone = name.clone();
                                        let members_clone = members.clone();
                                        context.upgrade_label_to_bus(&name);
                                        return context.add_bus(name_clone, members_clone);
                                    }
                                } else if let Some((component, interface, members)) =
                                    ids.as_component_member()
                                {
                                    let full_name = format!("{component}.{interface}");
                                    let found = context.find_inst(&component).is_some();
                                    let aim = if found {
                                        context.add_interface_member(
                                            &component,
                                            &interface,
                                            members.clone(),
                                        )
                                    } else {
                                        None
                                    };
                                    // eprintln!(
                                    //     "[P1-CURLY]   comp_member: comp={:?} iface={:?} members={:?} find_inst={} add_iface_member_some={}",
                                    //     component, interface, members, found, aim.is_some()
                                    // );
                                    if found {
                                        if let Some(result) = aim {
                                            return Some(result);
                                        }
                                        dlog_error(
                                            crate::errcodes::IFACE_MEMBER_NOT_FOUND,
                                            node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::IFACE_MEMBER_NOT_FOUND,
                                                &[&interface, &full_name, &component],
                                            ),
                                        );
                                    } else {
                                        dlog_error(
                                            crate::errcodes::IFACE_CURLY_MEMBER_INVALID,
                                            node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::IFACE_CURLY_MEMBER_INVALID,
                                                &[&component, &component, &interface],
                                            ),
                                        );
                                    }
                                    return None;
                                } else {
                                    return Some(
                                        context
                                            .add_label(ids.to_string())
                                            .unwrap_or_else(|| McPhrase::label(ids.to_string())),
                                    );
                                }
                            } else if ids.is_square_bracket() {
                                if let Some((name, members)) = ids.as_bus() {
                                    context.add_list(name, members)
                                } else {
                                    // ── Iter-11.2: pure square bracket [A, B] expand to Multiple ──
                                    //
                                    // Original code: context.add_label(ids.to_string())
                                    // treat `[VDD_3V3, GND]` as single Label string
                                    // "[VDD_3V3, GND]", causing:
                                    //   1. body line `[VDD_3V3, GND] -> dcdc{Vin, GND}`
                                    //      left is 1×1 scalar, right is 2×1 bus, dimension mismatch
                                    //   2. DC interface's positive and GND can't be separated into independent nets
                                    //   3. global GND bus can't form a net
                                    //
                                    // Fix: use ids.expand() to extract member list, generate Multiple
                                    // make each member independent NetPoint, support N×N pairwise connection.
                                    //
                                    // Example:
                                    //   `[VDD_3V3, GND]` → Multiple([Label("VDD_3V3"), Label("GND")])
                                    //   `[VDD_3V3, GND] -> dcdc{Vin, GND}`
                                    //   → 2×2 zip: VDD_3V3~dcdc.Vin, GND~dcdc.GND
                                    let expanded = ids.expand();
                                    if expanded.len() >= 2 {
                                        let phrases: Vec<McPhrase> = expanded
                                            .into_iter()
                                            .map(|m| {
                                                context
                                                    .add_label(m.clone())
                                                    .unwrap_or_else(|| McPhrase::label(m))
                                            })
                                            .collect();
                                        Some(McPhrase::Multiple(phrases))
                                    } else {
                                        // ── D6: DROPPED_STATEMENT detection ──────────────────────
                                        // When NAME[k] form (e.g. GPIO[2]) is used as an indexed
                                        // alias and the expanded name is not a known instance, the
                                        // statement will produce no nets/constraints.
                                        if ids.segments.len() >= 2 {
                                            if let Some(expanded_name) = expanded.first() {
                                                if context.find_inst(expanded_name).is_none()
                                                    && context.find_inst(&ids.to_string()).is_none()
                                                {
                                                    dlog_error(
                                                        crate::errcodes::NET_DROPPED_STATEMENT,
                                                        node,
                                                        &crate::errcodes::format_msg(
                                                            crate::errcodes::NET_DROPPED_STATEMENT,
                                                            &[&ids.to_string(), &expanded_name],
                                                        ),
                                                    );
                                                }
                                            }
                                        }
                                        context.add_label(ids.to_string())
                                    }
                                }
                            } else if let Some(chain) = ids.dot_chain_parts() {
                                // ★ Dot chain — structured segments straight from the
                                // AST (`uC.ADC.P` → ["uC", "ADC", "P"]), no text re-parsing.
                                //
                                // `uC.ADC.P` (3+ segments): component-bus member access.
                                if chain.len() >= 3 {
                                    let base = &chain[0];
                                    let bus_name = &chain[1];
                                    if context.is_component_bus(base, bus_name) {
                                        let pin_name = chain[2..].join(".");
                                        let full_name = format!("{base}.{bus_name}");
                                        let member_ref = McBus::member_ref(&full_name, pin_name);
                                        return Some(McPhrase::Endpoint(McEndpoint::Single(
                                            McInstanceRef::new(McInstance::Bus(member_ref)),
                                        )));
                                    }
                                }
                                // Single segment — plain name.
                                if chain.len() == 1 {
                                    warn_prefix_id_as_wire(&subnode, &ids.to_string());
                                    return Some(
                                        context
                                            .add_label(ids.to_string())
                                            .unwrap_or_else(|| McPhrase::label(ids.to_string())),
                                    );
                                }
                                // Two-segment dot access (`MIC.P`).
                                let base = &chain[0];
                                let rest = chain[1..].join(".");
                                if context.find_inst(base).is_none() {
                                    context.add_bus(base.to_string(), vec![rest.clone()]);
                                } else {
                                    // E1802: Check if base is a Component and rest is a valid pin
                                    if let Some(McInstance::Component(c)) = context.find_inst(base)
                                    {
                                        if c.find_pin(&rest).is_none() {
                                            let available: Vec<&str> = c
                                                .base
                                                .pins
                                                .names_to_id
                                                .keys()
                                                .map(|s| s.as_str())
                                                .collect();
                                            dlog_error(
                                                crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                                &subnode,
                                                &crate::errcodes::format_msg(
                                                    crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                                    &[
                                                        &rest as &dyn std::fmt::Display,
                                                        base as &dyn std::fmt::Display,
                                                        &available.join(", ")
                                                            as &dyn std::fmt::Display,
                                                    ],
                                                ),
                                            );
                                            return None;
                                        }
                                    }
                                    // ★ LSP: Register instance reference for dot-separated path
                                    let span = (subnode.get_pos() as usize)
                                        ..((subnode.get_pos() + subnode.get_len()) as usize);
                                    if let Some(decl_id) =
                                        crate::query::refs::mcb_lookup_instance_decl(
                                            context.uri(),
                                            base,
                                            scope.as_deref(),
                                        )
                                    {
                                        mcb_register_instance_ref(
                                            context.uri(),
                                            span,
                                            decl_id,
                                            scope.as_deref(),
                                        );
                                    }
                                    context.upgrade_label_to_bus(base);
                                    if let Some(McPhrase::Endpoint(McEndpoint::Single(
                                        McInstanceRef {
                                            base: McInstance::Bus(bus),
                                            ..
                                        },
                                    ))) = context.add_bus_member(base, rest.clone())
                                    {
                                        return Some(McPhrase::Endpoint(McEndpoint::Single(
                                            McInstanceRef::new(McInstance::Bus(bus)),
                                        )));
                                    }
                                }
                                let member_ref = McBus::member_ref(base, rest);
                                Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                    McInstance::Bus(member_ref),
                                ))))
                            } else {
                                warn_prefix_id_as_wire(&subnode, &ids.to_string());
                                context.add_label(ids.to_string())
                            }
                        }
                        McOpd::This(ids) => {
                            // this.X ≡ X (pins transparency)
                            if let Some((base, member)) = ids.as_dot_access() {
                                if base == "this" {
                                    if let Some(inst) = context.find_inst(&member) {
                                        return Some(McPhrase::Endpoint(McEndpoint::Single(
                                            McInstanceRef::new(inst),
                                        )));
                                    }
                                }
                            }
                            // fallback: bare "this" or unresolved member
                            context.add_label(ids.to_string())
                        }
                        McOpd::Pins(ids) => context.add_label(ids.to_string()),
                        McOpd::Uscore => Some(McPhrase::Lead),
                    }
                } else {
                    // McOpd::new failed - subnode might be MCAST_INT or another basic type
                    // Fall back to processing the subnode directly
                    Self::new(&subnode, context)
                }
            }

            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                let data = node.to_id_or_ida();
                if data.is_empty() {
                    dlog_error(
                        crate::errcodes::NAME_ID_EXTRACT_FAILED,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::NAME_ID_EXTRACT_FAILED, &[]),
                    );
                    return None;
                }

                let mut items: Vec<Option<crate::McInstance>> =
                    data.iter().map(|id| context.find_inst(id)).collect();

                if items.len() == 1 {
                    if let Some(ident) = items.remove(0) {
                        // ★ LSP: Register instance reference when found in symbol table
                        let span =
                            (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                        if let Some(decl_id) = crate::query::refs::mcb_lookup_instance_decl(
                            context.uri(),
                            &data[0],
                            scope.as_deref(),
                        ) {
                            mcb_register_instance_ref(
                                context.uri(),
                                span,
                                decl_id,
                                scope.as_deref(),
                            );
                        }
                        Some(ident.into())
                    } else {
                        let id = &data[0];
                        if let Some((base, member)) = id.split_once('.') {
                            let base_inst_opt = context.find_inst(base);
                            if base_inst_opt.is_none() {
                                // Base instance not found - create a new bus
                                context.add_bus(base.to_string(), vec![member.to_string()]);
                            } else {
                                // Base instance found - check if it's a Component
                                if let Some(McInstance::Component(c)) = base_inst_opt {
                                    // E1802: Check if the member is a valid pin in the component
                                    if c.find_pin(member).is_none() {
                                        let available: Vec<&str> = c
                                            .base
                                            .pins
                                            .names_to_id
                                            .keys()
                                            .map(|s| s.as_str())
                                            .collect();
                                        dlog_error(
                                            crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                            node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                                &[
                                                    &member as &dyn std::fmt::Display,
                                                    &base as &dyn std::fmt::Display,
                                                    &available.join(", ") as &dyn std::fmt::Display,
                                                ],
                                            ),
                                        );
                                        return None;
                                    }
                                }
                                // ★ LSP: Register instance reference for dot-separated path
                                let span = (node.get_pos() as usize)
                                    ..((node.get_pos() + node.get_len()) as usize);
                                if let Some(decl_id) = crate::query::refs::mcb_lookup_instance_decl(
                                    context.uri(),
                                    base,
                                    scope.as_deref(),
                                ) {
                                    mcb_register_instance_ref(
                                        context.uri(),
                                        span,
                                        decl_id,
                                        scope.as_deref(),
                                    );
                                }
                                context.upgrade_label_to_bus(base);
                                if let Some(McPhrase::Endpoint(McEndpoint::Single(
                                    McInstanceRef {
                                        base: McInstance::Bus(bus),
                                        ..
                                    },
                                ))) = context.add_bus_member(base, member.to_string())
                                {
                                    return Some(McPhrase::Endpoint(McEndpoint::Single(
                                        McInstanceRef::new(McInstance::Bus(bus)),
                                    )));
                                }
                            }
                            let member_ref = McBus::member_ref(base, member.to_string());
                            Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(member_ref),
                            ))))
                        } else {
                            warn_prefix_id_as_wire(node, id);
                            context.add_label(id.clone())
                        }
                    }
                } else {
                    // ── Iter-6 P0-2.mcid-merge ──────────────────────────────
                    // Root cause: extract_ida expands `MIC{P, N}` to ["MIC.P", "MIC.N"]
                    //       two independent id strings, then Multiple path turns each into
                    //       single-member Bus, phrase_to_members flattens to sibling phrases,
                    //       chain adjacency directly shorts MIC.P ~ MIC.N.
                    //
                    // If all ids are "base.rest" shape sharing same base, and base is
                    // declared instance, then treat as multi-member access to base, collapse to single Bus(base, [..]).
                    // (aligning with validate_inst_reference output, downstream goes through points.rs
                    //  P1-A4 owner=base path.)
                    let common = {
                        let mut base: Option<String> = None;
                        let mut members: Vec<String> = Vec::with_capacity(data.len());
                        let mut ok = true;
                        for id in &data {
                            match id.split_once('.') {
                                Some((b, rest)) => {
                                    if let Some(ref existing) = base {
                                        if existing != b {
                                            ok = false;
                                            break;
                                        }
                                    } else {
                                        base = Some(b.to_string());
                                    }
                                    members.push(rest.to_string());
                                }
                                None => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            base.filter(|b| context.find_inst(b).is_some())
                                .map(|b| (b, members))
                        } else {
                            None
                        }
                    };

                    if let Some((base_name, members)) = common {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(&base_name, members)),
                        ))));
                    }

                    // Original Multiple path preserved as fallback
                    Some(McPhrase::Multiple(
                        items
                            .into_iter()
                            .enumerate()
                            .filter_map(|(idx, ident)| {
                                if let Some(ident) = ident {
                                    Some(McPhrase::Endpoint(McEndpoint::Single(
                                        McInstanceRef::new(ident),
                                    )))
                                } else {
                                    let id = &data[idx];
                                    if let Some((base, member)) = id.split_once('.') {
                                        if context.find_inst(base).is_none() {
                                            context.add_bus(
                                                base.to_string(),
                                                vec![member.to_string()],
                                            );
                                        } else {
                                            context.upgrade_label_to_bus(base);
                                            if let Some(McPhrase::Endpoint(McEndpoint::Single(
                                                McInstanceRef {
                                                    base: McInstance::Bus(bus),
                                                    ..
                                                },
                                            ))) =
                                                context.add_bus_member(base, member.to_string())
                                            {
                                                return Some(McPhrase::Endpoint(
                                                    McEndpoint::Single(McInstanceRef::new(
                                                        McInstance::Bus(bus),
                                                    )),
                                                ));
                                            }
                                        }
                                        let member_ref =
                                            McBus::member_ref(base, member.to_string());
                                        Some(McPhrase::Endpoint(McEndpoint::Single(
                                            McInstanceRef::new(McInstance::Bus(member_ref)),
                                        )))
                                    } else {
                                        warn_prefix_id_as_wire(node, id);
                                        context.add_label(id.clone())
                                    }
                                }
                            })
                            .collect(),
                    ))
                }
            }

            MCAST_DECLARE => {
                // First check if DECLARE contains a DOT expression (e.g., ldo.VIN)
                if let Some(subnodes) = node.get_sub_node() {
                    for child in subnodes.iter() {
                        if child.get_type() == MCAST_OPD_DOT {
                            return Self::new(&child, context);
                        }
                    }
                }

                // ── R5a fix: `name[a:b]::TYPE(params)` in chain ──
                // If TYPE is known 2-pin passive (RES/CAP/IND/DIO…, naming::is_known_twopin_class),
                // monster falls to mc_inst.rs:1092 Label fallback → bare net short.
                // Build FuncCall{TYPE,params} for each expanded name, same path as anonymous `CAP(...)` in chain.
                // ★ Criterion must use is_known_twopin_class, not cmie.is_none():
                //   Net types like DC/GND also cmie-None, would be mistakenly created as @?DC_n 2-pin component, breaking power nets.
                if let Some(sub) = node.get_sub_node() {
                    let mut class_node: Option<AstNode> = None;
                    let mut names: Vec<String> = Vec::new();
                    // Source span of the instance ID node (e.g. `res[1:2]` / `C4`),
                    // reused for LSP declaration registration below.
                    let mut names_span: Option<std::ops::Range<usize>> = None;
                    // Source offset of the instance ID node, used to tell the
                    // `name::TYPE(...)` form (instance first) from a space-form
                    // `TYPE name` declaration (class first) — the two share the
                    // same AST shape (MCAST_CLASS + MCAST_INSTANCE).
                    let mut inst_pos: Option<u32> = None;
                    for c in sub.iter() {
                        let t = c.get_type();
                        if t == MCAST_CLASS && class_node.is_none() {
                            class_node = Some(c.clone());
                        } else if t == MCAST_INSTANCE {
                            inst_pos = Some(c.get_pos());
                            let inst_id_node = c.get_sub_node().unwrap_or_else(|| c.clone());
                            let ids_node = if inst_id_node.get_type() == MCAST_OPD {
                                inst_id_node
                                    .get_sub_node()
                                    .unwrap_or_else(|| inst_id_node.clone())
                            } else {
                                inst_id_node
                            };
                            if let Some(ids) = McIds::new(&ids_node) {
                                names.extend(ids.expand());
                                names_span = Some(
                                    (ids_node.get_pos() as usize)
                                        ..((ids_node.get_pos() + ids_node.get_len()) as usize),
                                );
                            }
                        }
                    }
                    if let Some(cls) = class_node {
                        if let Some(class_ids) = cls.get_sub_node().and_then(|cid| McIds::new(&cid))
                        {
                            let fname = class_ids.to_string();
                            // ★ LSP: Register class reference for goto-definition on ::
                            // syntax inside net expressions (e.g., res[1:2]::RES(0Ω)).
                            // This is needed because the 2-pin class code path below
                            // returns early, bypassing context.parse_declare() at the
                            // end of this MCAST_DECLARE handler.
                            if let Some(class_id_node) = cls.get_sub_node() {
                                let class_span = (class_id_node.get_pos() as usize)
                                    ..((class_id_node.get_pos() + class_id_node.get_len())
                                        as usize);
                                mcb_register_declare_class(context.uri(), &class_ids, class_span);
                            }
                            let is_twopin =
                                crate::vector::graph::naming::is_known_twopin_class(&fname);
                            // Keep the FuncCall path for 2-pin classes: Pass2 relies
                            // on it for transpose (`R442::RES(1MΩ)'`) and NC-param
                            // semantics. LSP classification of the declared names
                            // (C4/C5 → inst) is supplied separately via
                            // record_declareb_def below.
                            //
                            // Also take this path for any resolved COMPONENT class
                            // (`TP1::TEST_POINT()`): a named inline construction must
                            // render with `::CLASS(...)` rather than degrading to a
                            // bare instance name. Net types (DC/GND interfaces, which
                            // are not components) keep the parse_declare path below.
                            let is_component = matches!(
                                resolve_cmie(&DB, &class_ids, context.uri()),
                                Some(McCMIE::Component(_))
                            );
                            // Only the `name::TYPE(...)` form (instance name
                            // before the class, e.g. `TP1::TEST_POINT()`) is a
                            // named inline construction. A space-form `TYPE name`
                            // declaration inside a net chain (`_M U1 + GND`) is a
                            // plain instance and must keep the parse_declare path
                            // below so 3-pin plus/minus checks and normal display
                            // still apply. The grammar produces the same AST for
                            // both forms, so source order disambiguates them.
                            let inline_ctor = inst_pos.map(|i| i < cls.get_pos()).unwrap_or(false);
                            let build = is_twopin || (is_component && inline_ctor);
                            if build {
                                // ★ LSP: Register declareb instance declarations
                                // (`res[1:2]::RES(0Ω)` → res1/res2, `C4::CAP()` → C4).
                                // The twopin early-return below bypasses
                                // context.parse_declare(), so only the class ref above
                                // is registered while the instance names are not.
                                // Mirror declare_instance's store_port_span +
                                // add_declare_with_name pattern so goto-def /
                                // hover can locate these inline declared instances.
                                if let Some(span) = &names_span {
                                    for name in &names {
                                        context.store_inst_span(name, span.clone());
                                        // ★ Declareb inference rule (`idx::CLASS(...)`):
                                        // the class is a 2-pin component (twopin list),
                                        // so the declared name is an instance. Record
                                        // the hint so the lapper classifies it as
                                        // InstDef/InstRef (not LabelDef/PortRef) even
                                        // though it bypasses parse_declare. First
                                        // registration wins (declaration = first
                                        // typed occurrence).
                                        context.record_declareb_def(
                                            name,
                                            SymbolKind::InstDef,
                                            span.clone(),
                                        );
                                        // register under the real
                                        // scope key so the inline instance id
                                        // matches the lapper-time InstDef id.
                                        let _ = crate::db::cmie::tables::WORKSPACE
                                            .mcodes
                                            .get(context.uri())
                                            .and_then(|mcode| {
                                                mcode.symbols.lock().ok().map(|mut sem| {
                                                    crate::refdef::register::register_instance_decl_parse_time(
                                                        &mut sem,
                                                        context.uri(),
                                                        scope.as_deref(),
                                                        name,
                                                        span.clone(),
                                                    )
                                                })
                                            });
                                    }
                                }
                                let mut params: Vec<McParamValue> = Vec::new();
                                let mut cur = cls.get_sub_node();
                                while let Some(n) = cur {
                                    if n.get_type() == MCAST_PARAMS {
                                        if let Some(ps) = n.get_sub_node() {
                                            for p in ps.iter() {
                                                if let Some(v) =
                                                    crate::semantic::basic::mc_param::McParamValue::new(
                                                        &p, context,
                                                    )
                                                {
                                                    params.push(v);
                                                }
                                            }
                                        }
                                        break;
                                    }
                                    cur = n.get_next();
                                }
                                let left = vec![McBus::new(&format!("{fname}.in"))];
                                let right = vec![McBus::new(&format!("{fname}.out"))];
                                let total = if names.is_empty() { 1 } else { names.len() };
                                let mut fcs: Vec<McPhrase> = Vec::with_capacity(total);
                                for name in if names.is_empty() {
                                    vec![String::new()]
                                } else {
                                    names.clone()
                                } {
                                    let caller = if name.is_empty() {
                                        None
                                    } else {
                                        Some(Box::new(McPhrase::Endpoint(McEndpoint::Single(
                                            McInstanceRef::new(McInstance::Label(name.clone())),
                                        ))))
                                    };
                                    fcs.push(McPhrase::FuncCall(McFuncCall {
                                        id: 0,
                                        caller,
                                        func_name: class_ids.clone(),
                                        params: params.clone(),
                                        left: left.clone(),
                                        right: right.clone(),
                                        dot_member: None,
                                        resolved_return_shape: None,
                                        pre_closure: false,
                                        named_ctor: true,
                                    }));
                                }
                                return Some(if fcs.len() <= 1 {
                                    // Also handle empty names (e.g. `R1::RES(1M)` where
                                    // instance name is part of mc_ids via `::`)
                                    fcs.into_iter().next().unwrap_or_else(|| {
                                        McPhrase::FuncCall(McFuncCall {
                                            id: 0,
                                            caller: None,
                                            func_name: class_ids.clone(),
                                            params: params.clone(),
                                            left: left.clone(),
                                            right: right.clone(),
                                            dot_member: None,
                                            resolved_return_shape: None,
                                            pre_closure: false,
                                            named_ctor: true,
                                        })
                                    })
                                } else {
                                    McPhrase::Multiple(fcs)
                                });
                            }
                        }
                    }
                }

                let parsed_instances = context.parse_declare(node);
                let mut result: Vec<McPhrase> =
                    parsed_instances.into_iter().map(|x| x.into()).collect();

                if result.is_empty() {
                    // DECLARE class not found — may be a type annotation like V5V::DC(5V)
                    // or TP2::TEST_POINT() where the class has not been loaded.
                    // Try to extract instance names and create labels instead of failing.
                    if let Some(subnodes) = node.get_sub_node() {
                        // First check if subnodes contain a DOT expression
                        for each in subnodes.iter() {
                            if each.get_type() == MCAST_OPD_DOT {
                                return Self::new(&each, context);
                            }
                            // Also check nested DOT
                            if let Some(inner) = each.get_sub_node() {
                                if inner.get_type() == MCAST_OPD_DOT {
                                    return Self::new(&inner, context);
                                }
                            }
                        }

                        let mut labels: Vec<McPhrase> = Vec::new();
                        for each in subnodes.iter() {
                            if each.get_type() == MCAST_INSTANCE {
                                // Instance name nodes
                                if let Some(inner) = each.get_sub_node() {
                                    for name in inner.to_id_or_ida() {
                                        if let Some(label) = context.add_label(name.clone()) {
                                            labels.push(label);
                                        }
                                    }
                                }
                            }
                        }
                        if labels.len() == 1 {
                            return Some(labels.remove(0));
                        } else if labels.len() > 1 {
                            return Some(McPhrase::Multiple(labels));
                        }
                    }
                    // Still nothing found — log and return None
                    dlog_error(
                        crate::errcodes::NAME_DECLARE_PARSE_FAILED,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::NAME_DECLARE_PARSE_FAILED,
                            &[],
                        ),
                    );
                    None
                } else if result.len() == 1 {
                    match result.remove(0) {
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Component(c),
                            ..
                        })) => Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Component(c.clone()),
                        )))),
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Module(m),
                            ..
                        })) => Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Module(m.clone()),
                        )))),
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Bus(ne),
                            ..
                        })) => Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(ne.clone()),
                        )))),
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Label(label),
                            ..
                        })) => Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(label.clone()),
                        )))),
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Interface(declare),
                            ..
                        })) => Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Interface(declare.clone()),
                        )))),
                        other => Some(other),
                    }
                } else {
                    Some(Self::Multiple(result))
                }
            }

            MCAST_OPD_DOT => {
                let subnode1 = node.get_sub_node().expect(MISSING_SUBNODE);
                let subnode2 = subnode1.get_next().expect(MISSING_SUBNODE);

                let left_opd = Self::new(&subnode1, context)?;
                let right = subnode2.to_id_or_ida_or_num();

                let _left_kind = match &left_opd {
                    McPhrase::Endpoint(McEndpoint::Single(ir)) => match &ir.base {
                        McInstance::Label(s) => format!("Label('{s}')"),
                        McInstance::Bus(b) => format!("Bus('{}', mem={:?})", b.name, b.member),
                        McInstance::Component(c) => format!("Component('{}')", c.name),
                        McInstance::Module(m) => format!("Module('{}')", m.name),
                        McInstance::Interface(i) => format!("Interface('{}')", i.name),
                        McInstance::List(l) => format!("List('{}', mem={:?})", l.name, l.member),
                        McInstance::Unresolved { class_name } => format!("?{class_name}"),
                        McInstance::BusRef { component, bus } => {
                            format!("BusRef('{component}.{bus}')")
                        }
                        McInstance::Pins => "Pins".into(),
                        McInstance::PinId(id) => format!("PinId('{id}')"),
                        McInstance::Attr(a) => format!("Attr({a})"),
                        McInstance::Func(f) => format!("Func({})", f.name),
                        McInstance::EnumVal { value_name, .. } => format!("EnumVal({value_name})"),
                    },
                    _ => format!("{:?}", std::mem::discriminant(&left_opd)),
                };
                // eprintln!("[OPD-DOT] left_opd_kind={} right={:?}", left_kind, right);

                // Pins transparency: .pins is a pure organizational namespace.
                // `inst.pins.X ≡ inst.X` — skip the `pins` segment transparently.
                // Exception: numeric index access `pins[8]` is handled by the
                // bracket/index handler, not this DOT handler.
                if right.len() == 1 && right[0] == "pins" {
                    return Some(left_opd);
                }

                // Special case: if left is Label and right has one element,
                // combine them into a single label (e.g., usbsock.VBUS -> "usbsock.VBUS")
                if let McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Label(ref data),
                    ..
                })) = &left_opd
                {
                    if right.len() == 1 && !right[0].is_empty() {
                        // Create combined label: "left_name.right_name"
                        let combined_name = format!("{}.{}", data, right[0]);
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(&combined_name)),
                        ))));
                    }
                }

                // Also handle the case where left is Component or Module and dot_or_curly returns Multiple
                // In that case, we should combine them into a single qualified name
                if right.len() == 1 && !right[0].is_empty() {
                    if let McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Component(ref c),
                        ..
                    })) = &left_opd
                    {
                        let inst_name = c.name.to_string();
                        // Try dot_or_curly first with a clone
                        if let Some(result) = left_opd.clone().dot_or_curly(&right) {
                            if matches!(result, McPhrase::Multiple(_)) {
                                // dot_or_curly returned Multiple, which means some members not found
                                // E1802: pin not found in component
                                if right.len() == 1 {
                                    let member = &right[0];
                                    if c.find_pin(member).is_none() {
                                        let available: Vec<&str> = c
                                            .base
                                            .pins
                                            .names_to_id
                                            .keys()
                                            .map(|s| s.as_str())
                                            .collect();
                                        dlog_error(
                                            crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                            node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                                &[
                                                    member as &dyn std::fmt::Display,
                                                    &inst_name as &dyn std::fmt::Display,
                                                    &available.join(", ") as &dyn std::fmt::Display,
                                                ],
                                            ),
                                        );
                                        return None;
                                    }
                                }
                                // If at least one pin found, proceed with the result
                                return Some(result);
                            }
                            return Some(result);
                        } else {
                            // dot_or_curly returned None, meaning no pins found
                            // E1802: pin not found in component
                            if right.len() == 1 {
                                let member = &right[0];
                                let available: Vec<&str> =
                                    c.base.pins.names_to_id.keys().map(|s| s.as_str()).collect();
                                dlog_error(
                                    crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                    node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::COMPONENT_PIN_NOT_FOUND,
                                        &[
                                            member as &dyn std::fmt::Display,
                                            &inst_name as &dyn std::fmt::Display,
                                            &available.join(", ") as &dyn std::fmt::Display,
                                        ],
                                    ),
                                );
                                return None;
                            }
                        }
                    } else if let McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Module(ref m),
                        ..
                    })) = &left_opd
                    {
                        let inst_name = m.name.to_string();
                        // Try dot_or_curly first with a clone
                        if let Some(result) = left_opd.clone().dot_or_curly(&right) {
                            if matches!(result, McPhrase::Multiple(_)) {
                                // dot_or_curly returned Multiple, which means some members not found
                                // E1802: pin not found in component
                                if right.len() == 1 {
                                    let member = &right[0];
                                    if !m.base.insts.find_port(member).is_some() {
                                        let available: Vec<&str> = m
                                            .base
                                            .insts
                                            .iter_ports()
                                            .map(|(name, _)| name)
                                            .collect();
                                        dlog_error(
                                            crate::errcodes::MODULE_PORT_NOT_FOUND,
                                            node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::MODULE_PORT_NOT_FOUND,
                                                &[
                                                    member as &dyn std::fmt::Display,
                                                    &inst_name as &dyn std::fmt::Display,
                                                    &available.join(", ") as &dyn std::fmt::Display,
                                                ],
                                            ),
                                        );
                                        return None;
                                    }
                                }
                                // If at least one port found, proceed with the result
                                return Some(result);
                            }
                            return Some(result);
                        } else {
                            // dot_or_curly returned None, meaning no ports found
                            // E1803: port not found in module
                            if right.len() == 1 {
                                let member = &right[0];
                                let available: Vec<&str> =
                                    m.base.insts.iter_ports().map(|(name, _)| name).collect();
                                dlog_error(
                                    crate::errcodes::MODULE_PORT_NOT_FOUND,
                                    node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::MODULE_PORT_NOT_FOUND,
                                        &[
                                            member as &dyn std::fmt::Display,
                                            &inst_name as &dyn std::fmt::Display,
                                            &available.join(", ") as &dyn std::fmt::Display,
                                        ],
                                    ),
                                );
                                return None;
                            }
                        }
                    }
                }

                // Handle the case where left is FuncCall or other Phrase - create Member variant
                if right.len() == 1 && !right[0].is_empty() {
                    let member_ep =
                        McEndpoint::Single(McInstanceRef::new(McInstance::Label(right[0].clone())));
                    return Some(McPhrase::Member(Box::new(left_opd), member_ep));
                }

                left_opd.dot_or_curly(&right)
            }

            MCAST_OPD_CURLY => {
                let subnode1 = node.get_sub_node().expect(MISSING_SUBNODE);
                let subnode2 = subnode1.get_next().expect(MISSING_SUBNODE);

                let left_opd = Self::new(&subnode1, context)?;
                // Grammar now allows mc_phrase (expressions) inside curly braces,
                // not just IDAN. Extract string form for simple cases (identifiers/numbers);
                // complex expressions return empty vec and fall through gracefully.
                let right: Vec<String> = subnode2.to_id_or_ida_or_num();

                let _left_kind = match &left_opd {
                    McPhrase::Endpoint(McEndpoint::Single(ir)) => match &ir.base {
                        McInstance::Label(s) => format!("Label('{s}')"),
                        McInstance::Bus(b) => format!("Bus('{}', mem={:?})", b.name, b.member),
                        McInstance::Component(c) => format!("Component('{}')", c.name),
                        McInstance::Module(m) => format!("Module('{}')", m.name),
                        McInstance::Interface(i) => format!("Interface('{}')", i.name),
                        McInstance::List(l) => format!("List('{}', mem={:?})", l.name, l.member),
                        McInstance::Unresolved { class_name } => format!("?{class_name}"),
                        McInstance::BusRef { component, bus } => {
                            format!("BusRef('{component}.{bus}')")
                        }
                        McInstance::Pins => "Pins".into(),
                        McInstance::PinId(id) => format!("PinId('{id}')"),
                        McInstance::Attr(a) => format!("Attr({a})"),
                        McInstance::Func(f) => format!("Func({})", f.name),
                        McInstance::EnumVal { value_name, .. } => format!("EnumVal({value_name})"),
                    },
                    _ => format!("{:?}", std::mem::discriminant(&left_opd)),
                };

                // dot_or_curly success (hits find_pin) use directly; extract instance qualified name first
                let base_name: Option<String> = match &left_opd {
                    McPhrase::Endpoint(McEndpoint::Single(ir)) => match &ir.base {
                        McInstance::Component(c) => Some(c.name.to_string()),
                        McInstance::Module(m) => Some(m.name.to_string()),
                        McInstance::Interface(i) => Some(i.name.to_string()),
                        McInstance::Bus(b) => Some(b.name.clone()),
                        McInstance::Label(s) => Some(s.clone()),
                        _ => None,
                    },
                    _ => None,
                };

                if let Some(res) = left_opd.dot_or_curly(&right) {
                    return Some(res);
                }

                // ── P1/P6/speaker fix v2 ───────────────────────────────
                // Use single "Bus with members" as fallback, aligning mcu{DAC_OUT,SPK_MUTE} /
                // dot_or_curly hit (line 1650-1655) canonical form; previously used
                // Multiple([Bus,Bus]) wrong shape, dropped at is_connectable.
                if let Some(name) = base_name {
                    let members: Vec<String> =
                        right.into_iter().filter(|m| !m.is_empty()).collect();
                    if members.is_empty() {
                        return None;
                    }
                    return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new_with_members(&name, members)),
                    ))));
                }
                None
            }

            MCAST_OPD_SQUARE_VEC => {
                let first_subnode = node.get_sub_node().expect(MISSING_SUBNODE);
                let subnodes: Vec<AstNode> = first_subnode.iter().collect();

                // ── D6: DROPPED_STATEMENT detection ──────────────────────
                // When a single-element square bracket (e.g. [2] or [Unknown])
                // expands to a name that is not a known instance, the statement
                // may produce no meaningful nets or constraints.
                if subnodes.len() == 1 {
                    if let Some(ids) = McIds::new(&subnodes[0]) {
                        let expanded = ids.expand();
                        if let Some(expanded_name) = expanded.first() {
                            if context.find_inst(expanded_name).is_none()
                                && context.find_inst(&ids.to_string()).is_none()
                            {
                                dlog_error(
                                    crate::errcodes::NET_DROPPED_STATEMENT,
                                    node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::NET_DROPPED_STATEMENT,
                                        &[&ids.to_string(), &expanded_name],
                                    ),
                                );
                            }
                        }
                    }
                }

                Some(McPhrase::Multiple(
                    subnodes
                        .into_iter()
                        .map(|n| {
                            Some(McPhrase::new(&n, context)?.upgrade_new_label_or_bus(context))
                        })
                        .collect::<Option<Vec<_>>>()?,
                ))
            }

            MCAST_OPD_CURLY_MN => {
                let subnode1 = node.get_sub_node().expect(MISSING_SUBNODE);
                let subnode2 = subnode1.get_next().expect(MISSING_SUBNODE);
                let subnode3 = subnode2.get_next().expect(MISSING_SUBNODE);

                let left_opd = Self::new(&subnode1, context)?;

                // mca.y wraps each `mc_opds` inside curly braces in an
                // MCAST_OPDS node so the `|` boundary survives mc_value_link3's
                // next-chain flattening (otherwise the left option list absorbs
                // the right side's ids). Extract all id/ida/int elements by
                // walking each wrapper's own chain; complex expressions return
                // empty vec and fall through gracefully.
                //
                // Walk the chain with `get_next()` explicitly: `AstNodeIter`
                // (from `.iter()`) silently truncates the traversal when its
                // pointer-safety guard trips (e.g. a misaligned `next`), which
                // dropped trailing option members like `DAC_OUT, SPK_MUTE`.
                let mut right1: Vec<String> = Vec::new();
                let mut cur = subnode2.get_sub_node();
                while let Some(n) = cur {
                    right1.extend(n.to_id_or_ida_or_num());
                    cur = n.get_next();
                }
                let mut right2: Vec<String> = Vec::new();
                cur = subnode3.get_sub_node();
                while let Some(n) = cur {
                    right2.extend(n.to_id_or_ida_or_num());
                    cur = n.get_next();
                }

                match left_opd {
                    left_opd @ McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Component(_),
                        ..
                    }))
                    | left_opd @ McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Module(_),
                        ..
                    })) => left_opd.curly_mn(&right1, &right2),
                    _ => {
                        // ★ Fix: When left_opd is Bus or Label, try to resolve it as a
                        // Component or Module. This happens when the instance was not yet
                        // registered in the symbol table (e.g., due to parse order issues).
                        let name = match &left_opd {
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Bus(ref ne),
                                ..
                            })) if !ne.name.is_empty() => Some(ne.name.clone()),
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Label(label),
                                ..
                            })) => Some(label.clone()),
                            _ => None,
                        };

                        if let Some(ref name) = name {
                            // Try looking up in symbol table first
                            if let Some(ident) = context.find_inst(name) {
                                let resolved: McPhrase = ident.into();
                                if matches!(
                                    resolved,
                                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                        base: McInstance::Component(_),
                                        ..
                                    }))
                                ) || matches!(
                                    resolved,
                                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                        base: McInstance::Module(_),
                                        ..
                                    }))
                                ) {
                                    return resolved.curly_mn(&right1, &right2);
                                }
                            }
                            // Try global lookup
                            let ids = McIds::from(name.as_str());
                            if let Some(cmie) = resolve_cmie(&DB, &ids, context.uri()) {
                                match cmie {
                                    McCMIE::Component(comp_def) => {
                                        let mc2_comp = Mc2Component::new(name, comp_def);
                                        let phrase = McPhrase::Endpoint(McEndpoint::Single(
                                            McInstanceRef::new(McInstance::Component(Arc::new(
                                                mc2_comp,
                                            ))),
                                        ));
                                        return phrase.curly_mn(&right1, &right2);
                                    }
                                    McCMIE::Module(mod_def) => {
                                        let mc2_mod = Mc2Module::new(name, mod_def);
                                        let phrase = McPhrase::Endpoint(McEndpoint::Single(
                                            McInstanceRef::new(McInstance::Module(Arc::new(
                                                mc2_mod,
                                            ))),
                                        ));
                                        return phrase.curly_mn(&right1, &right2);
                                    }
                                    _ => {}
                                }
                            }

                            // ★ Fix: Graceful fallback - create Node from label names
                            // When the definition is not found (e.g., due to missing file),
                            // treat it as label-based port selection rather than hard error.
                            // This allows downstream processing to continue with partial info.
                            let left_members: Vec<McBus> = right1
                                .iter()
                                .map(|r| McBus::new(&format!("{name}.{r}")))
                                .collect();
                            let right_members: Vec<McBus> = right2
                                .iter()
                                .map(|r| McBus::new(&format!("{name}.{r}")))
                                .collect();

                            if !left_members.is_empty() || !right_members.is_empty() {
                                dlog_error(
                                    crate::errcodes::NAME_DEF_NOT_FOUND_LABEL_FALLBACK,
                                    node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::NAME_DEF_NOT_FOUND_LABEL_FALLBACK,
                                        &[&name],
                                    ),
                                );
                                return Some(McPhrase::Endpoint(McEndpoint::Node {
                                    input: left_members
                                        .iter()
                                        .map(|bus| {
                                            McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                                bus.clone(),
                                            )))
                                        })
                                        .collect(),
                                    output: right_members
                                        .iter()
                                        .map(|bus| {
                                            McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                                                bus.clone(),
                                            )))
                                        })
                                        .collect(),
                                }));
                            }
                        }
                        dlog_error(
                            crate::errcodes::CURLY_MN_WRONG_BASE,
                            node,
                            &crate::errcodes::format_msg(crate::errcodes::CURLY_MN_WRONG_BASE, &[]),
                        );
                        None
                    }
                }
            }

            MCAST_OPD_APOST => {
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                // Normalize: keep Series branches as-is (`(A - B)'` transposes the whole series chain)
                let opd1 = match McPhrase::new(&opd1_node, context)? {
                    McPhrase::Series(phrases, _) => McPhrase::Series(phrases, ConnDir::Undirected),
                    other => other,
                };
                // Structural rejection: Bus / Multiple / Label / nested Transposed cannot be transposed again
                if matches!(
                    &opd1,
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Bus(_),
                        ..
                    })) | McPhrase::Multiple(_)
                        | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Label(_),
                            ..
                        }))
                        | McPhrase::Transposed(_)
                ) {
                    dlog_error(
                        crate::errcodes::CONN_CANNOT_TRANSPOSE,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::CONN_CANNOT_TRANSPOSE, &[]),
                    );
                    return None;
                }
                // Pass1 safety rule (eval.md §5.5): shapes whose strict math
                // transpose has no connectable expression cannot be transposed.
                if let Err(rows) = check_transpose_allowed(&opd1, context) {
                    dlog_error(
                        crate::errcodes::SHAPE_TRANSPOSE_LIMIT,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::SHAPE_TRANSPOSE_LIMIT,
                            &[&rows],
                        ),
                    );
                    return None;
                }
                Some(McPhrase::Transposed(Box::new(opd1)))
            }

            MCAST_OPD_CARET => {
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                match McPhrase::new(&opd1_node, context)? {
                    McPhrase::Series(ref mut phrases, _) => {
                        phrases.reverse();
                        Some(McPhrase::Series(phrases.clone(), ConnDir::Undirected))
                    }
                    // ── P5: E2903 — reverse `^` on a vector operand is a no-op ──
                    // Parallel (`A + B`), transposed (`X'`) and parenthesized
                    // vector groupings (`(A + B)`) carry no order to reverse
                    // (eval.md §5.6 / examples L180: `...'^`).
                    opd1 if is_reverse_noop_operand(&opd1) => {
                        dlog_warning(
                            crate::errcodes::SHAPE_REVERSE_NOOP,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::SHAPE_REVERSE_NOOP,
                                &[&opd1],
                            ),
                        );
                        Some(opd1)
                    }
                    // §6.3: reversing a node swaps its left/right ports (the
                    // single-element Series wrapper below would be a no-op).
                    opd1 @ McPhrase::Endpoint(McEndpoint::Node { .. }) => {
                        let mut node = opd1;
                        node.reverse();
                        Some(node)
                    }
                    // §6.3: reversing a two-pin component (a 1*2 row vector)
                    // swaps its two pins — `R101^` presents pin 2 on the left
                    // and pin 1 on the right (vec-arch.md §5.2). The
                    // single-element Series wrapper below is a no-op for a lone
                    // component, so the swap is done here at the phrase level.
                    ref opd1 @ McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Component(ref c),
                        ..
                    })) => {
                        if matches!(shape_defaults(c).kind, PinShapeKind::TwoPin) {
                            let inst_name = c.name.to_string();
                            Some(McPhrase::Endpoint(McEndpoint::Node {
                                input: vec![McEndpoint::Single(McInstanceRef::new(
                                    McInstance::Bus(McBus::new(&format!("{inst_name}.2"))),
                                ))],
                                output: vec![McEndpoint::Single(McInstanceRef::new(
                                    McInstance::Bus(McBus::new(&format!("{inst_name}.1"))),
                                ))],
                            }))
                        } else {
                            let mut phrases = vec![opd1.clone()];
                            phrases.reverse();
                            Some(McPhrase::Series(phrases, ConnDir::Undirected))
                        }
                    }
                    opd1 => {
                        let mut phrases = vec![opd1];
                        phrases.reverse();
                        Some(McPhrase::Series(phrases, ConnDir::Undirected))
                    }
                }
            }

            MCAST_OPD_FCALL => {
                let result = McFuncCall::parse(node, context);
                if let Some(ref r) = result {
                    let fn_name = match r {
                        McPhrase::FuncCall(fc) => fc.func_name.to_string(),
                        _ => format!("{:?}", std::mem::discriminant(r)),
                    };
                    mcc_dbg!("sem::conds", "[P2-FCALL-RESULT] FCALL result={fn_name}");
                } else {
                    mcc_dbg!("sem::conds", "[P2-FCALL-RESULT] FCALL result=None");
                }
                result
            }

            MCAST_OPD_CLOSURE => McClosure::parse(node, context).map(McPhrase::Closure),

            MCAST_OPD_GROUP => {
                // Parse once via McGroup::parse, avoid duplicate parsing
                McGroup::parse(node, context).map(McPhrase::Group)
            }

            MCAST_OPD_PLUS => {
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                let opd2_node = opd1_node.get_next().expect(MISSING_SUBNODE);

                let opd1 = McPhrase::new(&opd1_node, context)?;
                let opd2 = McPhrase::new(&opd2_node, context)?;

                // Infer shapes and upgrade phrases before checking connectivity
                // ★ P4.3 single-port representative rule:
                //   - `+` / `-` / `<-`: op1 is the shape representative (left side dominant)
                //   - `->`: op2 is the shape representative (right side is destination)
                //   This is enforced through check_inst_plusminus for op1 and already
                //   implicit in how get_left/get_right shape inference works.
                let (opd1, opd2) = infer_shape_and_upgrade(opd1, opd2, context);

                // ── P1.3: inst 1*1/1*2 constraint for +/- ──
                if let Some((inst_name, pin_count)) = check_inst_plusminus(&opd1) {
                    dlog_error(
                        crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                            &[&inst_name, &pin_count],
                        ),
                    );
                    return None;
                }
                if let Some((inst_name, pin_count)) = check_inst_plusminus(&opd2) {
                    dlog_error(
                        crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                            &[&inst_name, &pin_count],
                        ),
                    );
                    return None;
                }

                // §5.1 parallel `+` is left-aligned: the left ports must always
                // match one-to-one. A transposed operand is first transposed to
                // its full-width column (its effective port is the merged inner
                // left + right element list), and the §5.1 left-alignment check
                // runs on that transposed result — there is no transpose
                // carve-out (opcheck is shared with Pass2).
                let opd1_shape = OpdShape::of(&opd1, context);
                let opd2_shape = OpdShape::of(&opd2, context);
                if !is_connectable(
                    ConnOp::Parallel,
                    ConnDir::Undirected,
                    &opd1_shape,
                    &opd2_shape,
                    crate::semantic::opcheck::ParallelAlign::Left,
                ) {
                    dlog_error(
                        crate::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    return None;
                }
                // The right ports only need to align when BOTH sides carry an
                // independent right port (row vector / node, left != right).
                // When only one side does (single node / column vector, left ==
                // right), the right side merges into the result without
                // alignment (vec-dianlu.md §5.1). A transposed operand is a
                // column after transposition (left == right), so it never
                // carries an independent right port.
                if opd1_shape.port_left() != opd1_shape.port_right()
                    && opd2_shape.port_left() != opd2_shape.port_right()
                    && !is_connectable(
                        ConnOp::Parallel,
                        ConnDir::Undirected,
                        &opd1_shape,
                        &opd2_shape,
                        crate::semantic::opcheck::ParallelAlign::Right,
                    )
                {
                    dlog_error(
                        crate::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    return None;
                }

                use McPhrase::*;
                match (opd1, opd2) {
                    (opd1 @ Transposed(_), opd2 @ Transposed(_)) => {
                        Some(Series(vec![opd1, opd2], ConnDir::Undirected))
                    }
                    (opd1 @ Transposed(_), opd2) => {
                        if opd2.get_left().iter().map(|e| e.size()).sum::<usize>() != 2 {
                            dlog_error(
                                crate::errcodes::CONN_TRANSPOSE_SIZE_MISMATCH,
                                node,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::CONN_TRANSPOSE_SIZE_MISMATCH,
                                    &[],
                                ),
                            );
                            return None;
                        }
                        let mut ret_line_members = vec![opd1];
                        if let Series(line, _) = opd2 {
                            ret_line_members.extend(line);
                        } else {
                            ret_line_members.push(opd2);
                        }
                        Some(Series(ret_line_members, ConnDir::Undirected))
                    }
                    (opd1, opd2 @ Transposed(_)) => {
                        if opd1.get_right().iter().map(|e| e.size()).sum::<usize>() != 2 {
                            dlog_error(
                                crate::errcodes::CONN_TRANSPOSE_SIZE_MISMATCH,
                                node,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::CONN_TRANSPOSE_SIZE_MISMATCH,
                                    &[],
                                ),
                            );
                            return None;
                        }
                        if let Series(mut line, _) = opd1 {
                            line.push(opd2);
                            Some(Series(line, ConnDir::Undirected))
                        } else {
                            Some(Series(vec![opd1, opd2], ConnDir::Undirected))
                        }
                    }
                    (opd1, opd2) => {
                        let mut ret_opds = match opd1 {
                            McPhrase::Parallel(opds) => opds,
                            _ => vec![opd1],
                        };
                        match opd2 {
                            McPhrase::Parallel(opds) => ret_opds.extend(opds),
                            _ => ret_opds.push(opd2),
                        }
                        Some(McPhrase::Parallel(ret_opds))
                    }
                }
            }

            MCAST_OPD_MINUS => {
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                let opd2_node = opd1_node.get_next().expect(MISSING_SUBNODE);

                let opd1 = McPhrase::new(&opd1_node, context)?;
                let opd2 = McPhrase::new(&opd2_node, context)?;
                let (opd1, opd2) = infer_shape_and_upgrade(opd1, opd2, context);

                // ── P1.3: inst 1*1/1*2 constraint for +/- ──
                if let Some((inst_name, pin_count)) = check_inst_plusminus(&opd1) {
                    dlog_error(
                        crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                            &[&inst_name, &pin_count],
                        ),
                    );
                    return None;
                }
                if let Some((inst_name, pin_count)) = check_inst_plusminus(&opd2) {
                    dlog_error(
                        crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::SHAPE_INST_3PIN_PLUSMINUS,
                            &[&inst_name, &pin_count],
                        ),
                    );
                    return None;
                }

                // §4.1 series evaluation: opd1.right ↔ opd2.left (- is Undirected, take op1).
                // A transposed operand is first transposed to its full-width
                // column, then the §5.2 check runs on the transposed result —
                // no transpose carve-out.
                if !is_connectable(
                    ConnOp::Series,
                    ConnDir::Undirected,
                    &OpdShape::of(&opd1, context),
                    &OpdShape::of(&opd2, context),
                    crate::semantic::opcheck::ParallelAlign::Left,
                ) {
                    dlog_error(
                        crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    return None;
                }

                // Only flatten a chain whose direction matches the operator's
                // `-` (Undirected). An embedded `->` chain keeps its own
                // direction (`A -> B - C` stays `A -> B - C`), otherwise the
                // inner direction would be lost when the outer Series flattens it.
                let mut ret_line: Vec<McPhrase> = match opd1 {
                    Series(line, ConnDir::Undirected) => line,
                    other => vec![other],
                };

                match opd2 {
                    Series(line2, ConnDir::Undirected) => ret_line.extend(line2),
                    other => ret_line.push(other),
                }
                Some(Series(ret_line, ConnDir::Undirected))
            }

            MCAST_OPD_RIGHTARROW => {
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                let opd2_node = opd1_node.get_next().expect(MISSING_SUBNODE);
                let opd1 = McPhrase::new(&opd1_node, context);
                let opd2 = McPhrase::new(&opd2_node, context);
                let opd1 = opd1?;
                let opd2 = opd2?;

                let (opd1, opd2) = infer_shape_and_upgrade(opd1, opd2, context);

                // §4.3 series evaluation: opd1.right ↔ opd2.left (-> is LtoR, take op2).
                // A transposed operand is first transposed to its full-width
                // column, then the §5.2 check runs on the transposed result —
                // no transpose carve-out.
                if !is_connectable(
                    ConnOp::Series,
                    ConnDir::LtoR,
                    &OpdShape::of(&opd1, context),
                    &OpdShape::of(&opd2, context),
                    crate::semantic::opcheck::ParallelAlign::Left,
                ) {
                    dlog_error(
                        crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    return None;
                }

                // ret_line: Vec<McPhrase> representing the accumulated line.
                // Only flatten a chain whose direction matches `->` (LtoR); an
                // embedded `-` chain keeps its own direction (`A - B -> C`
                // stays `A - B -> C`), otherwise the inner direction would be
                // lost when the outer Series flattens it.
                let mut ret_line: Vec<McPhrase> = match opd1 {
                    Series(phrases, ConnDir::LtoR) => phrases,
                    other => vec![other],
                };
                // Set right side of ret_line as output
                if let Some(last) = ret_line.last_mut() {
                    last.set_right_out();
                }

                // line2: the second operand
                let mut line2 = match opd2 {
                    Series(phrases, ConnDir::LtoR) => phrases,
                    other => vec![other],
                };
                // Set left side of line2 as input
                if let Some(first) = line2.first_mut() {
                    first.set_left_in();
                }

                ret_line.extend(line2);
                let result = Series(ret_line, ConnDir::LtoR);
                Some(result)
            }

            MCAST_OPD_LEFTARROW => {
                // Left arrow: opd1 <- opd2 means data flows from opd2 to opd1
                // i.e. opd2.right connects to opd1.left
                // Result line order: [opd2, opd1]
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                let opd2_node = opd1_node.get_next().expect(MISSING_SUBNODE);

                let opd1 = McPhrase::new(&opd1_node, context)?;
                let opd2 = McPhrase::new(&opd2_node, context)?;

                // Note: swap order here for shape inference, because data flow is opd2 -> opd1
                let (opd2, opd1) = infer_shape_and_upgrade(opd2, opd1, context);

                // §4.4 series evaluation (leftward): opd2.right ↔ opd1.left (<- is RtoL, take op1).
                // A transposed operand is first transposed to its full-width
                // column, then the §5.2 check runs on the transposed result —
                // no transpose carve-out.
                if !is_connectable(
                    ConnOp::Series,
                    ConnDir::RtoL,
                    &OpdShape::of(&opd2, context),
                    &OpdShape::of(&opd1, context),
                    crate::semantic::opcheck::ParallelAlign::Left,
                ) {
                    dlog_error(
                        crate::errcodes::CONN_LEFT_ARROW_SHAPE_MISMATCH,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::CONN_LEFT_ARROW_SHAPE_MISMATCH,
                            &[],
                        ),
                    );
                    return None;
                }

                // opd2 is source, its right is output.
                // Only flatten a chain whose direction matches `<-` (RtoL); an
                // embedded `-`/`->` chain keeps its own direction.
                let mut ret_line: Vec<McPhrase> = match opd2 {
                    Series(phrases, ConnDir::RtoL) => phrases,
                    other => vec![other],
                };
                if let Some(last) = ret_line.last_mut() {
                    last.set_right_out();
                }

                // opd1 is the target, its left side is input
                let mut line1: Vec<McPhrase> = match opd1 {
                    Series(phrases, ConnDir::RtoL) => phrases,
                    other => vec![other],
                };
                if let Some(first) = line1.first_mut() {
                    first.set_left_in();
                }

                // Connection: opd2 -> opd1
                ret_line.extend(line1);
                Some(Series(ret_line, ConnDir::RtoL))
            }

            // When MCAST_INSTANCE appears in an expression context (usually as a child node of MCAST_OPD
            // or as a leftover from split inline declarations), extract the instance name as an identifier reference.
            MCAST_INSTANCE => {
                if let Some(inner) = node.get_sub_node() {
                    // Check if inner is a DECLARE - if so, parse it via the DECLARE handling
                    if inner.get_type() == MCAST_DECLARE {
                        // Parse the DECLARE to get the phrase (which may contain DOT expressions)
                        return Self::new(&inner, context);
                    }
                    let names = inner.to_id_or_ida();
                    if names.len() == 1 {
                        if let Some(inst) = context.find_inst(&names[0]) {
                            return Some(inst.into());
                        }
                        let ids = McIds::from(names[0].as_str());
                        if resolve_cmie(&DB, &ids, context.uri()).is_some() {
                            return None;
                        }
                        return context.add_label(names[0].clone());
                    } else if names.len() > 1 {
                        let phrases: Vec<McPhrase> = names
                            .iter()
                            .filter_map(|name| {
                                if let Some(inst) = context.find_inst(name) {
                                    Some(McPhrase::Endpoint(McEndpoint::Single(
                                        McInstanceRef::new(inst),
                                    )))
                                } else {
                                    let ids = McIds::from(name.as_str());
                                    if resolve_cmie(&DB, &ids, context.uri()).is_some() {
                                        return None;
                                    }
                                    context.add_label(name.clone())
                                }
                            })
                            .collect();
                        return if phrases.len() == 1 {
                            Some(phrases.into_iter().next().unwrap())
                        } else {
                            Some(McPhrase::Multiple(phrases))
                        };
                    }
                }
                // When name cannot be extracted, fall back to to_string
                if let Some(s) = node.to_string() {
                    if let Some(inst) = context.find_inst(&s) {
                        return Some(inst.into());
                    }
                    let ids = McIds::from(s.as_str());
                    if resolve_cmie(&DB, &ids, context.uri()).is_some() {
                        return None;
                    }
                    return context.add_label(s);
                }
                dlog_error(
                    crate::errcodes::INST_EXPR_PARSE_FAILED,
                    node,
                    &crate::errcodes::format_msg(crate::errcodes::INST_EXPR_PARSE_FAILED, &[]),
                );
                None
            }

            // When MCAST_CLASS appears in an expression (e.g., a leftover from inline declaration V5V::DC(5V)),
            // extract the class name as an identifier reference
            MCAST_CLASS => {
                if let Some(inner) = node.get_sub_node() {
                    let names = inner.to_id_or_ida();
                    if names.len() == 1 {
                        if let Some(inst) = context.find_inst(&names[0]) {
                            // ★ LSP: Register instance reference for MCAST_CLASS path
                            let span = (node.get_pos() as usize)
                                ..((node.get_pos() + node.get_len()) as usize);
                            if let Some(decl_id) = crate::query::refs::mcb_lookup_instance_decl(
                                context.uri(),
                                &names[0],
                                scope.as_deref(),
                            ) {
                                mcb_register_instance_ref(
                                    context.uri(),
                                    span,
                                    decl_id,
                                    scope.as_deref(),
                                );
                            }
                            return Some(inst.into());
                        }
                        return context.add_label(names[0].clone());
                    }
                }
                if let Some(s) = node.to_string() {
                    return context.add_label(s);
                }
                None
            }

            // Skip intermediate grammar node types that shouldn't appear in phrase context
            // These are part of type/interface definitions, not expressions
            // ── Iter-5.A ────────────────────────────────────────────────
            // This body syntax like `V1V2 => CAP(...).Cap(_) -> [VDD_CORE, GND]`:
            // `=>` makes the left side V1V2 act as the chain's entry Endpoint, wrapped in AST as
            // MCAST_PARAMS_PRE(V1V2). The original code directly returned None here -> the entire body line
            // containing `=>` was lost due to early-return on `opd1?` in the upper RIGHTARROW branch ->
            // the CAP wiring starting with V1V2 was not generated at all (CAP_2/CAP_3 isolated symptoms).
            //
            // Fix: treat PARAMS_PRE as a transparent container, recursively parse its child nodes.
            // This way `V1V2 => X` in the body -> Series[V1V2, X] goes through adjacency normally.
            // MCAST_IOTYPE_RETURN is still kept as None (it is iotype syntax, not an expression).
            MCAST_PARAMS_PRE => {
                if let Some(inner) = node.get_sub_node() {
                    return McPhrase::new(&inner, context);
                }
                None
            }
            MCAST_IOTYPE_RETURN => None,

            // ── P1-1: arithmetic / range operators on connection stmts ──────
            // mca.y accepts `A * B` / `A / B` / `A ~ B` / `A : B` as mc_phrase,
            // but only `+` (parallel) and `-` / `->` (series) have connection
            // semantics. These four fall through to the generic E1110 today;
            // give an operator-specific diagnostic instead so the dropped stmt
            // is not confused with an AST shape bug.
            MCAST_OPD_MULTI | MCAST_OPD_DIVID | MCAST_OPD_TILDE | MCAST_OPD_COLON => {
                let op = match node.get_type() {
                    MCAST_OPD_MULTI => "*",
                    MCAST_OPD_DIVID => "/",
                    MCAST_OPD_TILDE => "~",
                    _ => ":",
                };
                dlog_error(
                    crate::errcodes::CONN_OPERATOR_UNSUPPORTED,
                    node,
                    &crate::errcodes::format_msg(
                        crate::errcodes::CONN_OPERATOR_UNSUPPORTED,
                        &[
                            &node.get_type() as &dyn std::fmt::Display,
                            &op as &dyn std::fmt::Display,
                        ],
                    ),
                );
                None
            }

            _ => {
                dlog_error(
                    crate::errcodes::PHRASE_AST_TYPE_UNEXPECTED,
                    node,
                    &crate::errcodes::format_msg(
                        crate::errcodes::PHRASE_AST_TYPE_UNEXPECTED,
                        &[
                            &node.get_type() as &dyn std::fmt::Display,
                            &node.get_type() as &dyn std::fmt::Display,
                        ],
                    ),
                );
                None
            }
        }
    }
}

// ============================================================================
// Shape defaults — eval.md §2 Pin shape default rules 1-4
// ============================================================================

/// Component pin shape descriptor, computed from component definition.
/// Maps to eval.md §2 rules 1-4.
struct CompPinShape {
    kind: PinShapeKind,
    /// Total static pin count (0 for dynamic-only pins)
    static_count: usize,
}

/// Shape kind per eval.md §2 rules.
enum PinShapeKind {
    /// Rule 1: 1-port label/pins → 1×1 (single bus, left=right=same name)
    Single,
    /// Rule 3: 2-pin device (RES/CAP/IND/DIO/LED/FUSE etc.) → 1×2
    ///   - left bus = "{inst}.1" (pin 1)
    ///   - right bus = "{inst}.2" (pin 2)
    TwoPin,
    /// Rule 4: 3+ pins → column vector
    ///   - left: input pins (or power[0] if no inputs)
    ///   - right: output pins (or power[1] if no outputs)
    MultiPort,
}

/// Compute the default pin shape for a component instance.
///
/// Implements eval.md §2 rules:
///   1. 1-port → Single (1×1)
///   2. (IDX expansion → handled at instantiation phase, not here)
///   3. 2-pin device (static or dynamic known 2-pin class) → TwoPin (1×2)
///   4. 3+ pins → MultiPort (N×1 column vector)
///
/// Dynamic pins with unknown class shape → defers to instantiation stage.
fn shape_defaults(c: &Mc2Component) -> CompPinShape {
    let static_count = c.base.pins.count();
    let has_dynamic = c.base.pins.has_dynamic_pins();
    let class_name = c.base.name.to_string();
    let is_known_2pin = crate::vector::graph::naming::is_known_twopin_class(&class_name);

    let kind = match static_count {
        // Rule 3: 0 static pins + dynamic + known 2-pin class → TwoPin
        // (anonymous instances @CAP, @RES etc. also treated as TwoPin)
        0 if has_dynamic && is_known_2pin => PinShapeKind::TwoPin,
        // Rule 1: 0 or 1 static pin, no matching 2-pin class → Single
        0 | 1 => PinShapeKind::Single,
        // Rule 3: exactly 2 static pins → TwoPin
        2 => PinShapeKind::TwoPin,
        // Rule 4: 3+ static pins → MultiPort
        _ => PinShapeKind::MultiPort,
    };

    // Dynamic pins with unknown shape (e.g. LPA/FLASH/SPEAKER):
    // treat as Single at phrase stage, defer to instantiation stage
    // for full shape determination.
    let kind = if has_dynamic && !is_known_2pin {
        PinShapeKind::Single
    } else {
        kind
    };

    CompPinShape { kind, static_count }
}

/// Whether a phrase carries no order to reverse: parallel groupings,
/// transposed operands and parenthesized vector groupings are vectors, so
/// `^` on them is a no-op (eval.md §5.6). Series chains are reversible and
/// are excluded here.
fn is_reverse_noop_operand(p: &McPhrase) -> bool {
    match p {
        McPhrase::Parallel(_) | McPhrase::Transposed(_) => true,
        McPhrase::Group(g) => g.opds.iter().any(is_reverse_noop_operand),
        _ => false,
    }
}

/// Check veccircuit.md constraint: instances with 3+ pins cannot directly
/// participate in `+` (Parallel) or `-` (Series Undirected) operations.
/// Returns `Some((name, pin_count))` for a rejected MultiPort instance,
/// `None` when the operand is allowed to participate.
fn check_inst_plusminus(opd: &McPhrase) -> Option<(String, usize)> {
    use McPhrase::*;
    match opd {
        Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Component(ref c),
            ..
        })) => {
            let shape = shape_defaults(c);
            if matches!(shape.kind, PinShapeKind::MultiPort) {
                Some((c.name.to_string(), shape.static_count))
            } else {
                None
            }
        }
        // Transposed, Group, FuncCall etc. — allowed (shape will be validated at Pass2)
        _ => None,
    }
}

// ============================================================================
// get_left / get_right
// ============================================================================

impl McPhrase {
    pub(crate) fn get_left(&self) -> Vec<McBus> {
        use IOType;
        match self {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(ref c),
                ..
            })) => {
                let inst_name = c.name.to_string();
                let shape = shape_defaults(c);
                match shape.kind {
                    PinShapeKind::TwoPin => vec![McBus::new(&format!("{inst_name}.1"))],
                    PinShapeKind::Single => vec![McBus::new(&inst_name)],
                    PinShapeKind::MultiPort => {
                        let in_pins = c.base.pins.get_pins_by_io(&IOType::In);
                        let out_pins = c.base.pins.get_pins_by_io(&IOType::Out);
                        let ps_pins = c.base.pins.get_pins_by_io(&IOType::Power);
                        if !in_pins.is_empty() && !out_pins.is_empty() {
                            in_pins
                                .iter()
                                .map(|p| McBus::new(&format!("{inst_name}.{p}")))
                                .collect()
                        } else if !ps_pins.is_empty() {
                            // A component with 3+ power pins and no in/out pins
                            // is a column vector (left == right == all power
                            // pins), not a folded single point (vec-arch.md
                            // §4.2). Expose every power pin so the Pass1 width
                            // matches the Pass2 lane expansion.
                            ps_pins
                                .iter()
                                .map(|p| McBus::new(&format!("{inst_name}.{p}")))
                                .collect()
                        } else {
                            vec![McBus::new(&inst_name)]
                        }
                    }
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(ref m),
                ..
            })) => {
                let inst_name = m.name.to_string();
                m.base
                    .insts
                    .get_all_inputs()
                    .iter()
                    .map(|p| p.to_node_element_with_prefix(&inst_name))
                    .collect()
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref data),
                ..
            })) => {
                // For a 2-member Bus (member.len() == 2), treat as pass-through: left=1, right=1
                // This means a 2-member bus can connect to anything single
                let total_members = data.member.len().max(1);
                if total_members == 2 {
                    // 2-member Bus: return single element with empty member (size=1)
                    vec![McBus {
                        name: data.name.clone(),
                        member: Vec::new(),
                        full_members: data.full_members.clone(),
                    }]
                } else {
                    Vec::from(data.clone())
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(ref label),
                ..
            })) => {
                vec![McBus::new(label)]
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(ref iface),
                ..
            })) => {
                vec![McBus::new(&iface.name.to_string())]
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(ref list),
                ..
            })) => {
                // A named list `Label[1,2]` is a column vector of its members
                // (vec-arch.md §4.2), not a single point named after the list.
                // Only a member-less list falls back to the bare name.
                if list.member.is_empty() && !list.name.is_empty() {
                    vec![McBus::new(list.name())]
                } else {
                    list.member.iter().map(|m| McBus::new(m)).collect()
                }
            }
            McPhrase::Parallel(opds) => {
                if opds.is_empty() {
                    vec![McBus::new("<error:empty_parallel>")]
                } else {
                    opds[0].get_left()
                }
            }
            McPhrase::Closure(ref c) => {
                // The left interface of a closure is the left interface of its body's first line
                if let Some(first_line) = c.body.first() {
                    first_line.get_left()
                } else {
                    vec![McBus::new("<closure:empty>")]
                }
            }
            McPhrase::Group(ref g) => g.get_left(),
            McPhrase::Series(ref phrases, _) => {
                if phrases.is_empty() {
                    vec![McBus::new("<error:empty_seq>")]
                } else {
                    phrases[0].get_left()
                }
            }
            McPhrase::Multiple(mc_opds) => mc_opds.iter().flat_map(|x| x.get_left()).collect(),
            McPhrase::Endpoint(McEndpoint::Node { ref input, .. }) => {
                input.iter().flat_map(|e| e.get_left()).collect()
            }
            McPhrase::Transposed(mc_line) => mc_line.get_right(),
            // ★ P4.1: consume the resolved return shape (eval.md §8.1).
            // `This` / unresolved → caller shape (f.left); `Label` → left is empty ([0|N]).
            McPhrase::FuncCall(ref f) => match &f.resolved_return_shape {
                Some(ReturnShape::Label { .. }) => Vec::new(),
                Some(ReturnShape::This) | None => f.left.clone(),
            },
            McPhrase::Lead => vec![McBus::new("(lead)")],
            McPhrase::Endpoint(ref ep) => ep.get_left(),
            McPhrase::Member(phrase, _) => phrase.get_left(),
        }
    }

    pub(crate) fn get_right(&self) -> Vec<McBus> {
        use IOType;
        match self {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(ref c),
                ..
            })) => {
                let inst_name = c.name.to_string();
                let shape = shape_defaults(c);
                match shape.kind {
                    PinShapeKind::TwoPin => vec![McBus::new(&format!("{inst_name}.2"))],
                    PinShapeKind::Single => vec![McBus::new(&inst_name)],
                    PinShapeKind::MultiPort => {
                        let in_pins = c.base.pins.get_pins_by_io(&IOType::In);
                        let out_pins = c.base.pins.get_pins_by_io(&IOType::Out);
                        let ps_pins = c.base.pins.get_pins_by_io(&IOType::Power);
                        if !in_pins.is_empty() && !out_pins.is_empty() {
                            out_pins
                                .iter()
                                .map(|p| McBus::new(&format!("{inst_name}.{p}")))
                                .collect()
                        } else if !ps_pins.is_empty() {
                            // See get_left: a component with only power pins is
                            // a column vector (left == right == all power pins).
                            ps_pins
                                .iter()
                                .map(|p| McBus::new(&format!("{inst_name}.{p}")))
                                .collect()
                        } else {
                            vec![McBus::new(&inst_name)]
                        }
                    }
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(ref m),
                ..
            })) => {
                let inst_name = m.name.to_string();
                m.base
                    .insts
                    .get_all_outputs()
                    .iter()
                    .map(|p| p.to_node_element_with_prefix(&inst_name))
                    .collect()
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref data),
                ..
            })) => {
                // For a 2-member Bus (member.len() == 2), treat as pass-through: left=1, right=1
                let total_members = data.member.len().max(1);
                if total_members == 2 {
                    vec![McBus {
                        name: data.name.clone(),
                        member: Vec::new(),
                        full_members: data.full_members.clone(),
                    }]
                } else {
                    Vec::from(data.clone())
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(ref label),
                ..
            })) => {
                vec![McBus::new(label)]
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(ref iface),
                ..
            })) => {
                vec![McBus::new(&iface.name.to_string())]
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(ref list),
                ..
            })) => {
                // A named list `Label[1,2]` is a column vector of its members
                // (vec-arch.md §4.2), not a single point named after the list.
                // Only a member-less list falls back to the bare name.
                if list.member.is_empty() && !list.name.is_empty() {
                    vec![McBus::new(list.name())]
                } else {
                    list.member.iter().map(|m| McBus::new(m)).collect()
                }
            }
            McPhrase::Parallel(opds) => {
                if opds.is_empty() {
                    vec![McBus::new("<error:empty_parallel>")]
                } else {
                    opds[0].get_right()
                }
            }
            McPhrase::Closure(ref c) => c.right.clone(),
            McPhrase::Group(ref g) => g.get_right(),
            McPhrase::Series(ref phrases, _) => {
                if phrases.is_empty() {
                    vec![McBus::new("<error:empty_seq>")]
                } else {
                    phrases.last().unwrap().get_right()
                }
            }
            McPhrase::Multiple(mc_opds) => mc_opds.iter().flat_map(|x| x.get_right()).collect(),
            McPhrase::Endpoint(McEndpoint::Node { ref output, .. }) => {
                output.iter().flat_map(|e| e.get_right()).collect()
            }
            McPhrase::Transposed(mc_line) => mc_line.get_left(),
            // ★ P4.1: consume the resolved return shape (eval.md §8.1).
            // `Label` → right = the return value's buses ([0|N]).
            McPhrase::FuncCall(ref f) => match &f.resolved_return_shape {
                Some(ReturnShape::Label { bus }) => bus.clone(),
                Some(ReturnShape::This) | None => f.right.clone(),
            },
            McPhrase::Lead => vec![McBus::new("(lead)")],
            McPhrase::Endpoint(ref ep) => ep.get_right(),
            McPhrase::Member(_, ep) => ep.get_right(),
        }
    }

    /// Set the left side as input
    pub(crate) fn set_left_in(&mut self) {
        match self {
            McPhrase::Series(ref mut phrases, _) => {
                // Series: set the first phrase's left side as input
                if let Some(first) = phrases.first_mut() {
                    first.set_left_in();
                }
            }
            McPhrase::Transposed(ref mut inner) => {
                // Transpose: swap left and right and then set
                inner.reverse();
                inner.set_right_out();
            }
            McPhrase::Parallel(ref mut opds) => {
                for opd in opds.iter_mut() {
                    opd.set_left_in();
                }
            }
            McPhrase::Closure(ref mut c) => {
                // Set the first line of the closure body left side as input
                if let Some(first) = c.body.first_mut() {
                    first.set_left_in();
                }
            }
            McPhrase::Group(ref mut g) => {
                for opd in g.opds.iter_mut() {
                    opd.set_left_in();
                }
            }
            McPhrase::Multiple(ref mut opds) => {
                if let Some(first) = opds.first_mut() {
                    first.set_left_in();
                }
            }
            _ => {}
        }
    }

    /// Set the right side as output
    pub(crate) fn set_right_out(&mut self) {
        match self {
            McPhrase::Series(ref mut phrases, _) => {
                // Series: set the last phrase's right side as output
                if let Some(last) = phrases.last_mut() {
                    last.set_right_out();
                }
            }
            McPhrase::Transposed(ref mut inner) => {
                // Transpose: set as output and then swap
                inner.set_right_out();
                inner.reverse();
            }
            McPhrase::Parallel(ref mut opds) => {
                for opd in opds.iter_mut() {
                    opd.set_right_out();
                }
            }
            McPhrase::Closure(ref mut c) => {
                // Set the last line of the closure body right side as output
                if let Some(last) = c.body.last_mut() {
                    last.set_right_out();
                }
            }
            McPhrase::Group(ref mut g) => {
                for opd in g.opds.iter_mut() {
                    opd.set_right_out();
                }
            }
            McPhrase::Multiple(ref mut opds) => {
                if let Some(last) = opds.last_mut() {
                    last.set_right_out();
                }
            }
            _ => {}
        }
    }

    /// Reverse the connection direction
    pub(crate) fn reverse(&mut self) {
        match self {
            McPhrase::Series(ref mut phrases, _) => phrases.reverse(),
            McPhrase::Transposed(ref mut inner) => {
                inner.reverse();
            }
            McPhrase::Parallel(ref mut opds) => {
                for opd in opds.iter_mut() {
                    opd.reverse();
                }
                opds.reverse();
            }
            McPhrase::Closure(ref mut c) => {
                for line in c.body.iter_mut() {
                    line.reverse();
                }
                c.body.reverse();
            }
            McPhrase::Group(ref mut g) => {
                for opd in g.opds.iter_mut() {
                    opd.reverse();
                }
            }
            McPhrase::Multiple(ref mut opds) => {
                for opd in opds.iter_mut() {
                    opd.reverse();
                }
            }
            McPhrase::Member(ref mut phrase, _) => {
                phrase.reverse();
            }
            // §6.3: reversing a node swaps its left/right ports.
            McPhrase::Endpoint(McEndpoint::Node { input, output }) => {
                std::mem::swap(input, output);
            }
            _ => {}
        }
    }

    fn dot_or_curly(self, member_names: &[String]) -> Option<McPhrase> {
        if member_names.is_empty() {
            return Some(self);
        }

        match self {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(c),
                ..
            })) => {
                let inst_name = c.name.to_string();

                // Combined name matching (e.g. ["VOUT", "Vout"] → "VOUT.Vout")
                if member_names.len() > 1 {
                    let combined = member_names.join(".");
                    if let Some(found) = c.find_pin(&combined) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(&format!("{inst_name}.{found}"))),
                        ))));
                    }
                }

                // ── Iter-6 P0-2 (narrow) ──────────────────────────────────
                // Only merge into single Bus with members when len>=2 and all find_pin hits.
                // len==1 or partial hits preserve the original Multiple path——to avoid breaking single-member
                // access like `ldo.VOUT` (the "Interface-as-pin" pattern): resolver treats
                // Bus(name='ldo.VOUT', member=[]) and Bus(name='ldo', member=['VOUT']) differently——
                // the latter triggers Interface sub-pin auto-expansion, causing VOUT.Vout and
                // VOUT.GND to be injected simultaneously, leading to a short circuit.
                if member_names.len() >= 2 {
                    let all_hit = member_names.iter().all(|id| c.find_pin(id).is_some());
                    if all_hit {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(
                                &inst_name,
                                member_names.to_vec(),
                            )),
                        ))));
                    }
                }

                // len==1 or partial hits: preserve the original Multiple path
                let mut result: Vec<McPhrase> = member_names
                    .iter()
                    .filter_map(|id| {
                        c.find_pin(id).map(|found| {
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(McBus::new(&format!("{inst_name}.{found}"))),
                            )))
                        })
                    })
                    .collect();

                if result.is_empty() {
                    dlog_trace(1162, &crate::errcodes::format_msg(1162, &[]));
                    None
                } else if result.len() == 1 {
                    Some(result.remove(0))
                } else {
                    Some(McPhrase::Multiple(result))
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(m),
                ..
            })) => {
                let inst_name = m.name.to_string();

                // ── [Iter-6 diag] ─────────────────────────────────────────
                // eprintln!("[DOTC-MOD] inst='{}' members={:?}", inst_name, member_names);
                // for id in member_names {
                //     eprintln!(
                //         "[DOTC-MOD]   find_port('{}') = {:?}",
                //         id,
                //         m.base.insts.find_port(id).map(|_| "Some").unwrap_or("None")
                //     );
                // }
                // ─────────────────────────────────────────────────────────

                // For multi-segment names like ["VOUT", "Vout"],
                // try combined dotted name first
                if member_names.len() > 1 {
                    let combined = member_names.join(".");
                    if m.base.insts.find_port(&combined).is_some() {
                        // Module port lookup found - create McPhrase from it
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(&inst_name, vec![combined])),
                        ))));
                    }
                }

                // Try individual port lookups
                let mut found_members: Vec<String> = Vec::new();
                let mut not_found_refs: Vec<(String, String)> = Vec::new();
                for (i, id) in member_names.iter().enumerate() {
                    if m.base.insts.find_port(id).is_some() {
                        found_members.push(id.clone());
                    } else {
                        not_found_refs.push((inst_name.clone(), member_names[i].clone()));
                    }
                }

                let mut final_results: Vec<McPhrase> = Vec::new();
                // Create single Bus with all found members
                if !found_members.is_empty() {
                    final_results.push(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new_with_members(&inst_name, found_members)),
                    ))));
                }
                // Add not found references as separate Buses with the member
                for (base, member) in not_found_refs {
                    final_results.push(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new_with_members(&base, vec![member])),
                    ))));
                }

                if final_results.is_empty() {
                    dlog_trace(1163, &crate::errcodes::format_msg(1163, &[]));
                    None
                } else if final_results.len() == 1 {
                    Some(final_results.remove(0))
                } else {
                    Some(McPhrase::Multiple(final_results))
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(i),
                ..
            })) => {
                let inst_name = i.name.to_string();

                if member_names.len() > 1 {
                    let combined = member_names.join(".");
                    if i.base.pins.find_pin(&combined).is_some() {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(&inst_name, vec![combined])),
                        ))));
                    }
                }

                // ── Iter-6 P0-2 (narrow) ──────────────────────────────────
                if member_names.len() >= 2 {
                    let all_hit = member_names
                        .iter()
                        .all(|id| i.base.pins.find_pin(id).is_some());
                    if all_hit {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(
                                &inst_name,
                                member_names.to_vec(),
                            )),
                        ))));
                    }
                }

                // len==1 or partial hits: preserve the original path
                let mut final_results: Vec<McPhrase> = Vec::new();
                for (idx, id) in member_names.iter().enumerate() {
                    if let Some(found) = i.base.pins.find_pin(id) {
                        final_results.push(McPhrase::Endpoint(McEndpoint::Single(
                            McInstanceRef::new(McInstance::Bus(McBus::new_with_members(
                                &inst_name,
                                vec![found],
                            ))),
                        )));
                    } else {
                        final_results.push(McPhrase::Endpoint(McEndpoint::Single(
                            McInstanceRef::new(McInstance::Bus(McBus::new(&format!(
                                "{}.{}",
                                inst_name, member_names[idx]
                            )))),
                        )));
                    }
                }

                if final_results.is_empty() {
                    dlog_trace(1164, &crate::errcodes::format_msg(1164, &[]));
                    None
                } else if final_results.len() == 1 {
                    Some(final_results.remove(0))
                } else {
                    Some(McPhrase::Multiple(final_results))
                }
            }
            // Inst<Component/Module> delegates to the unwrapped handlers
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref data),
                ..
            })) => match member_names.len() {
                1 => {
                    // Single member access: GPIO.A -> find A in the bus's members
                    let mut results: Vec<McBus> = Vec::new();
                    for m in &data.member {
                        if *m == member_names[0] {
                            results.push(McBus {
                                name: format!("{}.{}", data.name, m),
                                member: Vec::new(),
                                full_members: data.full_members.clone(),
                            });
                        }
                    }
                    if results.is_empty() {
                        None
                    } else {
                        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new_with_members(
                                &data.name,
                                member_names[0..1].to_vec(),
                            )),
                        ))))
                    }
                }
                _ => Some(McPhrase::Multiple(
                    member_names
                        .iter()
                        .filter_map(|id| {
                            let mut found = Vec::new();
                            for m in &data.member {
                                if *m == *id {
                                    found.push(McBus {
                                        name: format!("{}.{}", data.name, m),
                                        member: Vec::new(),
                                        full_members: data.full_members.clone(),
                                    });
                                }
                            }
                            if found.is_empty() {
                                None
                            } else {
                                Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                    McInstance::Bus(McBus::new_with_members(
                                        &data.name,
                                        found
                                            .into_iter()
                                            .map(|e| e.member.last().cloned().unwrap_or_default())
                                            .collect(),
                                    )),
                                ))))
                            }
                        })
                        .collect(),
                )),
            },
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(ref data),
                ..
            })) => {
                if member_names.is_empty() {
                    return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Label(data.clone()),
                    ))));
                }
                Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::Bus(McBus::new_with_members(data, member_names.to_vec())),
                ))))
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(mut list),
                ..
            })) => {
                member_names.iter().for_each(|x| list.add_member(x));
                Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::List(list),
                ))))
            }
            McPhrase::Parallel(opds) => {
                let results: Vec<McPhrase> = opds
                    .into_iter()
                    .map(|phrase| phrase.dot_or_curly(member_names))
                    .collect::<Option<Vec<_>>>()?;
                Some(McPhrase::Parallel(results))
            }
            McPhrase::Multiple(data) => Some(McPhrase::Multiple(
                data.into_iter()
                    .map(|x| x.dot_or_curly(member_names))
                    .collect::<Option<Vec<_>>>()?,
            )),
            McPhrase::Series(_, _) => {
                dlog_trace(1168, &crate::errcodes::format_msg(1168, &[]));
                None
            }
            McPhrase::Endpoint(McEndpoint::Node { .. }) => {
                dlog_trace(1169, &crate::errcodes::format_msg(1169, &[]));
                None
            }
            McPhrase::Transposed(_) => {
                dlog_trace(1170, &crate::errcodes::format_msg(1170, &[]));
                None
            }
            McPhrase::Lead => {
                dlog_trace(1171, &crate::errcodes::format_msg(1171, &[]));
                None
            }
            McPhrase::Group(_) => {
                dlog_trace(1172, &crate::errcodes::format_msg(1172, &[]));
                None
            }
            McPhrase::Closure(ref c) => {
                // Phase 3: access members of the closure's output interface
                if c.right.is_empty() {
                    dlog_trace(1173, &crate::errcodes::format_msg(1173, &[]));
                    return None;
                }
                Self::access_node_element_members(&c.right, member_names)
            }
            McPhrase::FuncCall(ref f) => {
                // Phase 3: access members of the FuncCall's output interface.
                // Enables chained module calls like: ModLDO(params){vin|vout}
                //   1. ModLDO(params) -> FuncCall with right = output port NodeElements
                //   2. {vin|vout} -> curly_mn -> dot_or_curly(["vin"]) / dot_or_curly(["vout"])
                //   3. Find matching port in the right interface
                if f.right.is_empty() {
                    dlog_trace(1174, &crate::errcodes::format_msg(1174, &[]));
                    return None;
                }
                Self::access_node_element_members(&f.right, member_names)
            }
            McPhrase::Endpoint(_ep) => {
                dlog_trace(1175, &crate::errcodes::format_msg(1175, &[]));
                None
            }
            McPhrase::Member(_, _) => {
                dlog_trace(1176, &crate::errcodes::format_msg(1176, &[]));
                None
            }
        }
    }

    /// Phase 3: Shared logic for accessing named members from a McBus interface list.
    ///
    /// Used by dot_or_curly for FuncCall and Closure.
    /// Searches for each member_name in the interface elements (name match or member match).
    /// For flat McBus structure where member: Vec<String>
    fn access_node_element_members(
        interface: &[McBus],
        member_names: &[String],
    ) -> Option<McPhrase> {
        let mut results = Vec::new();

        for member_name in member_names {
            let mut found = false;

            for elem in interface {
                // Direct name match (e.g. "vin" matches McBus { name: "ModLDO.vin" })
                if elem.name == *member_name || elem.name.ends_with(&format!(".{member_name}")) {
                    results.push(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new_with_members(&elem.name, elem.member.clone())),
                    ))));
                    found = true;
                    break;
                }

                // Member list match (elem.member is now Vec<String> in flat structure)
                if elem.member.contains(member_name) {
                    // Create a new McBus with the combined name
                    let combined_name = if elem.name.is_empty() {
                        member_name.clone()
                    } else {
                        format!("{}.{}", elem.name, member_name)
                    };
                    results.push(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new(&combined_name)),
                    ))));
                    found = true;
                    break;
                }
            }

            if !found {
                // Fallback: create a derived name from the first interface element
                if let Some(first) = interface.first() {
                    let base = first.name.split('.').next().unwrap_or(&first.name);
                    let derived_name = format!("{base}.{member_name}");
                    results.push(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(McBus::new(&derived_name)),
                    ))));
                } else {
                    dlog_trace(
                        crate::errcodes::PHRASE_IFACE_MEMBER_NOT_FOUND,
                        &crate::errcodes::format_msg(
                            crate::errcodes::PHRASE_IFACE_MEMBER_NOT_FOUND,
                            &[&member_name],
                        ),
                    );
                    return None;
                }
            }
        }

        if results.len() == 1 {
            Some(results.remove(0))
        } else {
            Some(McPhrase::Multiple(results))
        }
    }

    fn curly_mn(self, right1: &[String], right2: &[String]) -> Option<McPhrase> {
        if right1.is_empty() {
            dlog_trace(1197, &crate::errcodes::format_msg(1197, &[]));
            return None;
        }
        if right2.is_empty() {
            dlog_trace(1198, &crate::errcodes::format_msg(1198, &[]));
            return None;
        }

        fn opd_to_node_element_vec(phrase: McPhrase) -> Option<Vec<McBus>> {
            match phrase {
                McPhrase::Multiple(mc_opds) => {
                    let results: Option<Vec<Vec<McBus>>> =
                        mc_opds.into_iter().map(opd_to_node_element_vec).collect();
                    results.map(|v| v.into_iter().flatten().collect())
                }
                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Bus(ref node_elements),
                    ..
                })) => Some(Vec::from(node_elements)),
                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Label(ref name),
                    ..
                })) => Some(vec![McBus::new(name)]),
                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                    base: McInstance::Module(m),
                    members,
                })) => {
                    let name = m.name.to_string();
                    let members = members
                        .iter()
                        .flat_map(|ml| ml.expand())
                        .collect::<Vec<_>>();
                    if members.is_empty() {
                        Some(vec![McBus::new(&name)])
                    } else {
                        Some(vec![McBus::new_with_members(&name, members)])
                    }
                }
                _ => {
                    dlog_trace(1199, &crate::errcodes::format_msg(1199, &[]));
                    None
                }
            }
        }

        match self {
            McPhrase::Multiple(data) => Some(McPhrase::Multiple(
                data.into_iter()
                    .map(|x| x.curly_mn(right1, right2))
                    .collect::<Option<Vec<_>>>()?,
            )),
            _ => {
                let left = opd_to_node_element_vec(self.clone().dot_or_curly(right1)?)?;
                let right = opd_to_node_element_vec(self.dot_or_curly(right2)?)?;
                Some(McPhrase::Endpoint(McEndpoint::Node {
                    input: left
                        .iter()
                        .map(|bus| {
                            McEndpoint::Single(McInstanceRef::new(McInstance::Bus(bus.clone())))
                        })
                        .collect(),
                    output: right
                        .iter()
                        .map(|bus| {
                            McEndpoint::Single(McInstanceRef::new(McInstance::Bus(bus.clone())))
                        })
                        .collect(),
                }))
            }
        }
    }

    /// Helper function to recursively extract method name from AST nodes
    fn extract_method_name(node: &AstNode) -> Option<McIds> {
        // First check if this is a direct method call pattern
        let node_type = node.get_type();

        // For OPD_FCALL nodes, we need to look at the structure
        if node_type == MCAST_OPD_FCALL {
            // Check if there's a dot notation in the children
            for child in node.iter() {
                if child.get_type() == MCAST_OPD_DOT {
                    // For OPD_DOT, look for the method name in its children
                    for dot_child in child.iter() {
                        if dot_child.get_type() == MCAST_NAME {
                            let node_copy = dot_child.clone();
                            return McIds::new(&node_copy);
                        }
                    }
                }
            }
        }

        // For MCAST_NAME nodes, extract directly
        if node_type == MCAST_NAME {
            let node_copy = node.clone();
            return McIds::new(&node_copy);
        }

        // If that fails, check for OPD_DOT, DECLARE, or other nodes
        if node_type == MCAST_OPD_DOT || node_type == MCAST_DECLARE {
            for child in node.iter() {
                if let Some(name) = Self::extract_method_name(&child) {
                    return Some(name);
                }
            }
        }

        // For other nodes, check their subnodes
        if let Some(subnode) = node.get_sub_node() {
            return Self::extract_method_name(&subnode);
        }

        None
    }

    fn upgrade_new_label_or_bus(self, context: &mut dyn HasFindInst) -> McPhrase {
        match self {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(ref data),
                ..
            })) => context.add_label(data.clone()).unwrap_or(self),
            _ => self,
        }
    }
}

fn needs_paren_for_priority(phrase: &McPhrase) -> bool {
    match phrase {
        McPhrase::Parallel(_) => true,
        McPhrase::Transposed(_) => true,
        McPhrase::Multiple(_) => true,
        McPhrase::Series(phrases, _) => {
            if phrases.is_empty() {
                false
            } else {
                needs_paren_for_priority(&phrases[0])
            }
        }
        _ => false,
    }
}

fn needs_paren_for_series(phrase: &McPhrase) -> bool {
    match phrase {
        // `+` binds tighter than `->` / `-` / `<-`, so a Parallel item in a
        // Series re-parses as one item without parentheses
        // (`a + b -> c` is `(a + b) -> c`).
        McPhrase::Parallel(_) => false,
        // `Transposed` renders with self-delimiting parentheses (`({p})'`),
        // so extra parentheses are redundant (`(({p})')` -> `({p})'`).
        McPhrase::Transposed(_) => false,
        // `Multiple` renders with self-delimiting brackets (`[a, b]`),
        // so extra parentheses are redundant (`([a, b])` -> `[a, b]`).
        _ => false,
    }
}

fn format_series_item(phrase: &McPhrase) -> String {
    if needs_paren_for_series(phrase) {
        format!("({phrase})")
    } else {
        format!("{phrase}")
    }
}

impl std::fmt::Display for McPhrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McPhrase::Lead => write!(f, "_"),
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(c),
                ..
            })) => {
                if c.nc {
                    write!(f, "{}(NC)", c.name)
                } else {
                    write!(f, "{}", c.name)
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(m),
                ..
            })) => write!(f, "{}", m.name),
            McPhrase::Endpoint(McEndpoint::Single(ref_)) => write!(f, "{ref_}"),
            McPhrase::Series(phrases, dir) => {
                let sep = match dir {
                    ConnDir::LtoR => " -> ",
                    ConnDir::RtoL => " <- ",
                    ConnDir::Undirected => " - ",
                };
                // A pre-closure expansion target (`vin => CAP(10uF).Cap(_)`)
                // is joined with `=>` regardless of the series direction.
                let mut parts: Vec<String> = Vec::with_capacity(phrases.len() * 2 - 1);
                for (i, p) in phrases.iter().enumerate() {
                    if i > 0 {
                        let gap = match p {
                            McPhrase::FuncCall(fc) if fc.pre_closure => " => ",
                            _ => sep,
                        };
                        parts.push(gap.to_string());
                    }
                    parts.push(format_series_item(p));
                }
                write!(f, "{}", parts.concat())
            }
            McPhrase::Parallel(phrases) => {
                let items: Vec<String> = phrases.iter().map(|p| format!("{p}")).collect();
                write!(f, "{}", items.join(" + "))
            }
            McPhrase::Transposed(p) => {
                // A parenthesized operand (`(a -> b)'`) is a one-element Group
                // whose own Display keeps the parentheses; render it inside the
                // transpose's parentheses instead of doubling them.
                if let McPhrase::Group(g) = p.as_ref() {
                    if g.opds.len() == 1 {
                        return write!(f, "({})'", g.opds[0]);
                    }
                }
                write!(f, "({p})'")
            }
            McPhrase::Multiple(phrases) => {
                // Flatten nested `Multiple` so a source `[dio[1:2]::DIO(...)]`
                // (already expanded to a Multiple) is not rendered as `[[..]]`.
                fn flatten<'a>(p: &'a McPhrase, out: &mut Vec<String>) {
                    match p {
                        McPhrase::Multiple(inner) => {
                            for x in inner {
                                flatten(x, out);
                            }
                        }
                        other => out.push(format!("{other}")),
                    }
                }
                let mut flat: Vec<String> = Vec::new();
                for p in phrases {
                    flatten(p, &mut flat);
                }
                write!(f, "[{}]", flat.join(", "))
            }
            McPhrase::Closure(c) => write!(f, "=>({})", c.body.len()),
            McPhrase::FuncCall(fc) => {
                // Check if this is a pre-closure parameter pattern
                // Pattern: pre_param -> ClassName(params).MethodName(method_params)
                // where pre_param is the pre_closure parameter, and ClassName starts with uppercase
                let caller_is_pre_closure = if let Some(c) = &fc.caller {
                    if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                        let func_name_str = inner_fc.func_name.to_string();
                        func_name_str
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_uppercase())
                    } else if let McPhrase::Endpoint(ep) = c.as_ref() {
                        matches!(
                            ep,
                            McEndpoint::Single(McInstanceRef {
                                base: McInstance::Label(_),
                                ..
                            }) | McEndpoint::Single(McInstanceRef {
                                base: McInstance::Bus(_),
                                ..
                            })
                        )
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Print caller or pre-closure parameter.
                // Function calls bind with `.` (source: `CAP(x).Cap(_)`),
                // except declareb constructions (`C4::CAP()`) which bind with
                // `::` per `named_ctor`.
                let joiner = if fc.named_ctor { "::" } else { "." };
                if let Some(c) = &fc.caller {
                    if caller_is_pre_closure {
                        if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                            write!(f, "{}", inner_fc.func_name)?;
                            let inner_params: Vec<String> =
                                inner_fc.params.iter().map(|p| format!("{p}")).collect();
                            write!(f, "({})", inner_params.join(", "))?;
                        } else {
                            write!(f, "{c}")?;
                        }
                        write!(f, "{joiner}")?;
                    } else {
                        write!(f, "{c}")?;
                        write!(f, "{joiner}")?;
                    }
                }
                write!(f, "{}", fc.func_name)?;
                let param_strs: Vec<String> = fc.params.iter().map(|p| format!("{p}")).collect();
                write!(f, "({})", param_strs.join(", "))?;
                Ok(())
            }
            McPhrase::Member(phrase, ep) => {
                write!(f, "{phrase}.{ep}")
            }
            McPhrase::Group(g) => {
                if g.opds.len() == 1 {
                    // One-element Groups come from explicit source parentheses.
                    // Keep them for compound phrases so the output round-trips
                    // to the same structure (`(a -> b) + c`, `[x, (a -> b)]`);
                    // leaves (`a`, `a.b`, `CAP(10uF)`) render bare.
                    let inner = &g.opds[0];
                    if matches!(
                        inner,
                        McPhrase::Series(_, _)
                            | McPhrase::Parallel(_)
                            | McPhrase::Multiple(_)
                            | McPhrase::Group(_)
                    ) {
                        write!(f, "({inner})")
                    } else {
                        write!(f, "{inner}")
                    }
                } else {
                    let items: Vec<String> = g
                        .opds
                        .iter()
                        .map(|p| {
                            if needs_paren_for_priority(p) {
                                format!("({p})")
                            } else {
                                format!("{p}")
                            }
                        })
                        .collect();
                    write!(f, "({})", items.join(", "))
                }
            }
            McPhrase::Endpoint(ep) => write!(f, "{ep}"),
        }
    }
}

// ============================================================================
// Auxiliary functions
// ============================================================================

fn infer_shape_and_upgrade(
    opd1: McPhrase,
    opd2: McPhrase,
    context: &mut dyn HasFindInst,
) -> (McPhrase, McPhrase) {
    use McPhrase::*;
    match (opd1, opd2) {
        (
            Group(McGroup {
                opds: lhs,
                left_match: lhs_lm,
                right_match: lhs_rm,
            }),
            Group(McGroup {
                opds: rhs,
                left_match: rhs_lm,
                right_match: rhs_rm,
            }),
        ) => {
            if lhs.len() == rhs.len() {
                let mut left_results = Vec::new();
                let mut right_results = Vec::new();

                for (l, r) in lhs.into_iter().zip(rhs) {
                    let (left_result, right_result) = infer_shape_and_upgrade(l, r, context);
                    left_results.push(left_result);
                    right_results.push(right_result);
                }

                (
                    Group(McGroup {
                        opds: left_results,
                        left_match: lhs_lm,
                        right_match: lhs_rm,
                    }),
                    Group(McGroup {
                        opds: right_results,
                        left_match: rhs_lm,
                        right_match: rhs_rm,
                    }),
                )
            } else {
                dlog_trace(1220, &crate::errcodes::format_msg(1220, &[]));
                (
                    Group(McGroup {
                        opds: Vec::new(),
                        left_match: false,
                        right_match: false,
                    }),
                    Group(McGroup {
                        opds: Vec::new(),
                        left_match: false,
                        right_match: false,
                    }),
                )
            }
        }

        (
            Group(McGroup {
                opds,
                left_match,
                right_match,
            }),
            rhs_opd,
        ) => {
            let mut left_results = Vec::new();
            let mut rhs = rhs_opd;

            for branch in opds {
                let (left_result, right_result) =
                    infer_shape_and_upgrade(branch, rhs.clone(), context);
                left_results.push(left_result);
                rhs = right_result;
            }

            (
                Group(McGroup {
                    opds: left_results,
                    left_match,
                    right_match,
                }),
                rhs,
            )
        }

        (
            lhs_opd,
            Group(McGroup {
                opds,
                left_match,
                right_match,
            }),
        ) => {
            let mut right_results = Vec::new();
            let mut lhs = lhs_opd;

            for branch in opds {
                let (left_result, right_result) =
                    infer_shape_and_upgrade(lhs.clone(), branch, context);
                right_results.push(right_result);
                lhs = left_result;
            }

            (
                lhs,
                Group(McGroup {
                    opds: right_results,
                    left_match,
                    right_match,
                }),
            )
        }

        (
            Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(data1),
                ..
            })),
            Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(data2),
                ..
            })),
        ) => (
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
                data1,
            )))),
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
                data2,
            )))),
        ),

        (
            Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(data1),
                ..
            })),
            rhs,
        ) => (
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
                data1,
            )))),
            rhs,
        ),

        (
            lhs,
            Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(data2),
                ..
            })),
        ) => (
            lhs,
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
                data2,
            )))),
        ),

        (lhs, rhs) => (lhs, rhs),
    }
}

/// Infer the vector shape from a set of endpoints (Pass1 stage, eval.md §1/§2).
///
/// - Empty set → [`Shape::unknown`] (unresolved, e.g. a FuncCall return value);
/// - Each `McBus` element is one row; a bus with members (`RS485{A,B}`) counts as N rows;
/// - At the endpoint stage the column count is always 1: a 2-pin device's
///   `get_left/right` only exposes a single point, so the `1*2` row-vector shape is
///   invisible at the phrase layer and only fully expanded in Pass2.
fn shape_of_bus_list(elems: &[McBus]) -> Shape {
    if elems.is_empty() {
        return Shape::unknown();
    }
    let rows: usize = elems.iter().map(|e| e.size()).sum();
    Shape::new(rows.max(1))
}

/// Pass1 transpose safety guard (eval.md §5.5 / vec-arch.md §5.2): the operand
/// being transposed may only carry a shape whose strict math transpose has a
/// connectable expression — a column or node side wider than 2 rows has none.
///
/// - `OpdShape::Unknown` / empty sides (unresolved FuncCall return value) →
///   wildcard pass;
/// - Contains an `<error` placeholder marker → wildcard pass (reusing the
///   lenient semantics of [`is_connectable`]);
/// - `Column(N > 2)` or a `Node` side wider than 2 (e.g. `[A, B, C]'`) →
///   `Err(rows)`, reported by the caller as E2902.
fn check_transpose_allowed(opd: &McPhrase, context: &mut dyn HasFindInst) -> Result<(), usize> {
    let shape = OpdShape::of(opd, context);
    if shape.is_unknown() {
        return Ok(());
    }
    if shape
        .port_left()
        .iter()
        .chain(shape.port_right().iter())
        .any(|b| b.name.contains("<error"))
    {
        return Ok(());
    }
    shape.transpose().map(|_| ()).map_err(|rows| rows)
}

/// Member-count expansion of a multi-member `Mc2Interface` (e.g. a 2-pin
/// XTAL::XTAL port, or PDM[CLK, DATA] / {VDD, GND}): returns one bus element
/// per interface member, mirroring the Pass2 P2-4 point expansion
/// (mc_mod/points.rs). Single-member interfaces fall back to the plain
/// interface name so the Pass1 shape view matches the Pass2 point count.
fn interface_elems(iface: &Mc2Interface) -> Vec<McBus> {
    if let Some(members) = iface.name.list_members() {
        if members.len() >= 2 {
            return members.into_iter().map(|m| McBus::new(&m)).collect();
        }
    }
    if let Some((prefix, members)) = iface.name.as_bus() {
        if members.len() >= 2 {
            return members
                .into_iter()
                .map(|m| McBus::new(&format!("{prefix}.{m}")))
                .collect();
        }
    }
    let pin_names: Vec<String> = iface
        .base
        .pins
        .pins
        .values()
        .filter_map(|p| p.names.first().cloned())
        .collect();
    if pin_names.len() >= 2 {
        let port_name = iface.name.to_string();
        return pin_names
            .into_iter()
            .map(|m| McBus::new(&format!("{port_name}.{m}")))
            .collect();
    }
    vec![McBus::new(&iface.name.to_string())]
}

/// Expand a dotted component pin reference (`uC.UART0`, `ldo.VIN`) to its
/// member elements when the referenced pin is a registered multi-member port.
/// Pass2 expands such a port to one real point per member, so the Pass1
/// opcheck view must present the same width — otherwise a strict §5 row-count
/// check rejects a legal chain whose component side Pass2 will expand to N
/// points. Returns None when the reference is not a multi-member component
/// port (the caller falls back to its default single-point width).
fn component_port_elems(
    context: &mut dyn HasFindInst,
    base: &str,
    member: &str,
) -> Option<Vec<McBus>> {
    let c = match context.find_inst(base) {
        Some(McInstance::Component(c)) => c,
        _ => return None,
    };
    let port = c.base.pins.names_to_id.get(member)?;
    let elems: Vec<McBus> = match port {
        McPinPort::Bus(b) if b.member.len() >= 2 => b
            .member
            .iter()
            .map(|m| McBus::new(&format!("{base}.{member}.{m}")))
            .collect(),
        McPinPort::List(_, members) if members.len() >= 2 => members
            .iter()
            .map(|m| McBus::new(&format!("{base}.{member}.{m}")))
            .collect(),
        McPinPort::Multi(ids) if ids.len() >= 2 => ids
            .iter()
            .map(|m| McBus::new(&format!("{base}.{member}.{m}")))
            .collect(),
        McPinPort::Interface(iface) => {
            let elems = interface_elems(iface);
            if elems.len() >= 2 {
                elems
            } else {
                return None;
            }
        }
        _ => return None,
    };
    if elems.len() >= 2 {
        Some(elems)
    } else {
        None
    }
}

/// Base instance name of a phrase when it is, or resolves through, an
/// endpoint / method call: `X6` -> "X6", `X6.setup(...)` -> "X6".
/// Used by the Member / FuncCall branches to resolve a chained member
/// (`.XTAL`, `.I2C0`) against the port set of the base instance.
fn base_instance_name(phrase: &McPhrase) -> Option<String> {
    match phrase {
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef { base, .. })) => match base {
            McInstance::Component(c) => Some(c.name.base_name()),
            McInstance::Label(l) => Some(l.clone()),
            McInstance::Module(m) => Some(m.name.base_name()),
            _ => None,
        },
        McPhrase::FuncCall(fc) => fc.caller.as_deref().and_then(base_instance_name),
        _ => None,
    }
}

/// Expand a dotted module port reference (`usbsocket.vin`, `modldo.vout`) to
/// its member elements when the referenced port is a declared multi-member
/// interface port. Pass2 expands such a port to one real point per member
/// (mc_mod/points.rs expand_port_lanes), so the Pass1 opcheck view must present
/// the same width. Returns None when the reference is not a multi-member module
/// port (the caller falls back to its default single-point width).
fn module_port_elems(
    context: &mut dyn HasFindInst,
    base: &str,
    member: &str,
) -> Option<Vec<McBus>> {
    let m = match context.find_inst(base) {
        Some(McInstance::Module(m)) => m,
        _ => return None,
    };
    // Match by base name: a port may be stored under its curly / vector form
    // (`vin{POWER_SYS, GND}`, `[VDD_3V3, GND]`) while the reference uses the
    // plain name (`vin`). Take the port with the most effective members.
    let base_member = member
        .split(|c| c == '{' || c == '[')
        .next()
        .unwrap_or(member);
    let mut best: Option<Vec<McBus>> = None;
    let mut best_len = 0usize;
    for (name, port) in m.base.insts.iter() {
        let name_base = name.split(|c| c == '{' || c == '[').next().unwrap_or(name);
        if name_base != base_member {
            continue;
        }
        let elems: Vec<McBus> = match port {
            McInstance::Interface(iface) => interface_elems(iface),
            _ => {
                let bracket: Vec<String> = name
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .map(|s| {
                        s.split(',')
                            .map(|x| x.trim().to_string())
                            .filter(|x| !x.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                if bracket.len() >= 2 {
                    bracket.into_iter().map(|m| McBus::new(&m)).collect()
                } else {
                    Vec::new()
                }
            }
        };
        if elems.len() >= 2 && elems.len() > best_len {
            best_len = elems.len();
            best = Some(elems);
        }
    }
    best
}

/// Single-sided port bus list of `phrase` used for Pass1 operator
/// evaluation (vec-dianlu.md §5.3): a `Transposed` operand is first
/// transposed to its full-width column — the inner left and right elements
/// are merged in order and exposed on **both** ports (mirroring the Pass2
/// expansion in `mc_mod/points.rs`) — so the shape fed to `opcheck` is the
/// transposed result, not the pre-transpose single port. There is no
/// pair-by-min / lane-hang carve-out for transposed operands. Non-transposed
/// operands delegate to `get_left()` / `get_right()`, except that multi-member
/// interfaces and component port references present their full member count so
/// the Pass1 check sees the same width Pass2 will expand.
fn eval_port_elems(phrase: &McPhrase, right: bool, context: &mut dyn HasFindInst) -> Vec<McBus> {
    match phrase {
        // A transposed operand is first transposed via strict math transpose
        // (vec-arch.md §5.2 / §6.2): the inner shape is transposed, then the
        // requested side is read from the transposed shape. `Row` -> `Column`,
        // `Column(2)` -> `Row`, and a connectable `Node` / `Point` / `Unknown`
        // transpose to themselves. `check_transpose_allowed` (called in
        // MCAST_OPD_APOST before wrapping) already rejects shapes whose
        // transpose is unrepresentable, so `transpose()` succeeds here.
        McPhrase::Transposed(inner) => {
            let shape = OpdShape::of(inner, context);
            match shape.transpose() {
                Ok(t) => {
                    if right {
                        t.port_right()
                    } else {
                        t.port_left()
                    }
                }
                Err(_) => {
                    // Unreachable for a validated operand; fall back to the
                    // previous full-width column merge for robustness.
                    let mut all = inner.get_left();
                    all.extend(inner.get_right());
                    all
                }
            }
        }
        // A Series exposes the port of its head (left) / tail (right). Recurse so
        // a transposed head/tail is transposed to its full-width column first
        // (the pre-transpose single port of e.g. `A -> CAP'` must not be fed to
        // opcheck as the chain's right port).
        McPhrase::Series(phrases, _) => {
            let edge = if right {
                phrases.last()
            } else {
                phrases.first()
            };
            match edge {
                Some(e) => eval_port_elems(e, right, context),
                None => {
                    // empty series: mirror get_left/get_right sentinel
                    if right {
                        phrase.get_right()
                    } else {
                        phrase.get_left()
                    }
                }
            }
        }
        // A Parallel exposes the port of its first operand (mirroring
        // get_left/get_right and the Pass2 anchoring on opds[0]); recurse for a
        // transposed first operand.
        McPhrase::Parallel(opds) => match opds.first() {
            Some(first) => eval_port_elems(first, right, context),
            None => {
                if right {
                    phrase.get_right()
                } else {
                    phrase.get_left()
                }
            }
        },
        // A multi-member Interface (e.g. a 2-pin XTAL::XTAL port, or
        // PDM[CLK, DATA] / {VDD, GND}) presents its full member count, not the
        // 1*1 shorthand that get_left/get_right return for `McInstance::Interface`
        // (the interface name only). Pass2 expands such an interface to one real
        // point per member (mc_mod/points.rs P2-4), so the Pass1 opcheck view must
        // present the same width — otherwise a strict §5 row-count check rejects a
        // legal chain whose interface side Pass2 will expand to N points.
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Interface(ref iface),
            ..
        })) => interface_elems(iface),
        // A whole-module reference (e.g. `V3V3 -> moddcdc`) exposes the module's
        // input / output ports. Each port presents its declared width: a vector
        // port (`in [VDD_3V3, GND]`, or an Interface with multiple members)
        // contributes one element per member — Pass2 expands it to one lane per
        // member (mc_mod/points.rs expand_port_lanes) — while scalar ports keep
        // the single-point width that get_left/get_right expose. This keeps the
        // Pass1 opcheck view aligned with the Pass2 expansion count.
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Module(ref m),
            ..
        })) => {
            let ports = if right {
                m.base.insts.get_all_outputs()
            } else {
                m.base.insts.get_all_inputs()
            };
            let mut elems: Vec<McBus> = Vec::new();
            for port in ports {
                match port {
                    McInstance::Interface(iface) => {
                        let port_elems = interface_elems(iface);
                        if port_elems.len() >= 2 {
                            elems.extend(port_elems);
                        } else {
                            elems.push(McBus::new(&port.get_name()));
                        }
                    }
                    _ => {
                        // Bracket / curly vector name (`[A, B]`) carries the
                        // declared members; anything else is a scalar port.
                        let name = port.get_name();
                        let members: Vec<String> = name
                            .strip_prefix('[')
                            .and_then(|s| s.strip_suffix(']'))
                            .or_else(|| name.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
                            .map(|s| {
                                s.split(',')
                                    .map(|x| x.trim().to_string())
                                    .filter(|x| !x.is_empty())
                                    .collect()
                            })
                            .unwrap_or_default();
                        if members.len() >= 2 {
                            elems.extend(members.into_iter().map(|m| McBus::new(&m)));
                        } else {
                            elems.push(McBus::new(&name));
                        }
                    }
                }
            }
            elems
        }
        // `base.member` component port reference (e.g. `ldo.VIN`, `uC.UART0`):
        // Pass1 collapses these to a plain Label (1*1). When `base` resolves to
        // a known component instance and `member` is a registered multi-member
        // port, present the port's full member count — Pass2 expands the port to
        // one point per member — so the strict §5 check does not reject a legal
        // chain whose component side Pass2 will expand to N points.
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Label(ref label),
            ..
        })) => {
            if let Some((base, member)) = label.split_once('.') {
                if let Some(elems) = component_port_elems(context, base, member) {
                    return elems;
                }
                if let Some(elems) = module_port_elems(context, base, member) {
                    return elems;
                }
            }
            // §8.9.6.3 shape by use: a scalar-declared module port (e.g.
            // `io UART0`) is not shape-locked at the declaration site — its
            // width is inferred from the connection context and Pass2 expands
            // the port lanes accordingly. Present an empty (unknown-width)
            // shape so the strict §5 opcheck does not reject the statement
            // before Pass2 can upgrade the port. Internal labels and function
            // parameters keep their fixed 1*1 shape.
            if !label.contains('.') && context.is_declared_port(label) {
                return Vec::new();
            }
            vec![McBus::new(label)]
        }
        // Bypass the 2-member Bus pass-through in get_left/get_right (which
        // collapses `{A, B}` to a single element so the bus "can connect to
        // anything single"): under the strict §5 row-count rule a 2-member bus is
        // a 2*1 column — Pass2 expands it to two real points — so the Pass1
        // opcheck view must present the real member count, otherwise a transposed
        // 2*1 bridge is rejected against a falsely collapsed 1*1 bus.
        //
        // A dotted component pin reference can also arrive as a Bus — either
        // the member_ref form `Bus("uC", ["UART0"])` or the combined-name form
        // `Bus("uC.UART0", [])` — and must present the port's full member count
        // the same way the Label branch does.
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
            base: McInstance::Bus(ref bus),
            ..
        })) => {
            let base_member: Option<(&str, &str)> =
                if bus.member.len() == 1 && !bus.name.contains('.') {
                    Some((bus.name.as_str(), bus.member[0].as_str()))
                } else if bus.member.is_empty() {
                    bus.name.split_once('.')
                } else {
                    None
                };
            if let Some((base, member)) = base_member {
                if let Some(elems) = component_port_elems(context, base, member) {
                    return elems;
                }
                if let Some(elems) = module_port_elems(context, base, member) {
                    return elems;
                }
            }
            Vec::from(bus.clone())
        }
        // A chained member on a method call (`X6.setup(...).XTAL`,
        // `uC.i2c(...).I2C0`) that resolves to a multi-member port of the
        // caller component: Pass2 expands it to one point per member, so the
        // Pass1 opcheck view must present the same width.
        McPhrase::FuncCall(fc) => {
            if let Some(member) = &fc.dot_member {
                if let Some(base) = fc.caller.as_deref().and_then(base_instance_name) {
                    if let Some(elems) = component_port_elems(context, &base, member) {
                        return elems;
                    }
                }
            }
            let r = if right {
                fc.right.clone()
            } else {
                fc.left.clone()
            };
            if !r.is_empty() {
                r
            } else if right {
                phrase.get_right()
            } else {
                phrase.get_left()
            }
        }
        // A chained member on a composite phrase — `X6.setup(GND, NC).XTAL`
        // parses as Member(FuncCall, XTAL): the member names a port of the
        // base instance and Pass2 expands it to one point per member, so the
        // Pass1 opcheck view must present the same width.
        McPhrase::Member(inner, ep) => {
            let member = match ep {
                McEndpoint::Single(McInstanceRef {
                    base: McInstance::Label(l),
                    ..
                }) => Some(l.clone()),
                _ => None,
            };
            if let (Some(member), Some(base)) = (member, base_instance_name(inner)) {
                if let Some(elems) = component_port_elems(context, &base, &member) {
                    return elems;
                }
            }
            if right {
                phrase.get_right()
            } else {
                phrase.get_left()
            }
        }
        McPhrase::Endpoint(McEndpoint::Node {
            ref input,
            ref output,
        }) => {
            // Curly-mn port selection `m{vin|vout}` (MCAST_OPD_CURLY_MN) — the
            // input elements connect on the left, the output elements on the
            // right. Each element is an instance member reference (`m.vin`),
            // so it must present the same expanded width as the Bus branch:
            // a multi-member component / module port contributes one element
            // per member (Pass2 expands it to one point per member), otherwise
            // the strict §5 row-count check rejects a legal chain whose port
            // Pass2 will expand to N points.
            let side = if right { output } else { input };
            let mut elems: Vec<McBus> = Vec::new();
            for ep in side {
                match ep {
                    McEndpoint::Single(McInstanceRef {
                        base: McInstance::Bus(ref bus),
                        ..
                    }) => {
                        let base_member: Option<(&str, &str)> =
                            if bus.member.len() == 1 && !bus.name.contains('.') {
                                Some((bus.name.as_str(), bus.member[0].as_str()))
                            } else if bus.member.is_empty() {
                                bus.name.split_once('.')
                            } else {
                                None
                            };
                        if let Some((base, member)) = base_member {
                            if let Some(e) = component_port_elems(context, base, member) {
                                elems.extend(e);
                                continue;
                            }
                            if let Some(e) = module_port_elems(context, base, member) {
                                elems.extend(e);
                                continue;
                            }
                        }
                        elems.extend(Vec::from(bus.clone()));
                    }
                    other => {
                        if right {
                            elems.extend(other.get_right());
                        } else {
                            elems.extend(other.get_left());
                        }
                    }
                }
            }
            elems
        }
        _ => {
            let r = if right {
                phrase.get_right()
            } else {
                phrase.get_left()
            };
            r
        }
    }
}

impl OpdShape {
    /// Build the first-class operand shape for a phrase (vec-arch.md §4.2).
    ///
    /// The left and right ports are computed with the same width-aligned view
    /// as [`eval_port_elems`] (interface members, component/module port refs,
    /// shape-by-use, bus member bypass, chained members) and then classified
    /// into a single `OpdShape` via [`OpdShape::from_sides`].
    pub(crate) fn of(phrase: &McPhrase, context: &mut dyn HasFindInst) -> OpdShape {
        let left = eval_port_elems(phrase, false, context);
        let right = eval_port_elems(phrase, true, context);
        Self::from_sides(left, right)
    }

    /// Classify a pair of single-sided ports into an `OpdShape` (vec-arch.md
    /// §4.1 / §5.1). The classification preserves the row counts from
    /// `size_left` / `size_right`; the kind (Point/Row/Column/Node) is derived
    /// from the relationship between the two sides.
    pub(crate) fn from_sides(left: Vec<McBus>, right: Vec<McBus>) -> OpdShape {
        if left.is_empty() && right.is_empty() {
            // Both sides unknown (shape-by-use port / unresolved FuncCall).
            OpdShape::Unknown
        } else if left == right {
            match left.len() {
                1 => OpdShape::Point(left.into_iter().next().unwrap()),
                _ => OpdShape::Column(left),
            }
        } else if left.is_empty() {
            // `return <expr>` FuncCall: no left contact, right = return value.
            OpdShape::Node(Vec::new(), right)
        } else if right.is_empty() {
            OpdShape::Node(left, Vec::new())
        } else if left.len() == 1 && right.len() == 1 {
            // Two distinct single-element ports: a two-pin row vector.
            OpdShape::Row(
                left.into_iter().next().unwrap(),
                right.into_iter().next().unwrap(),
            )
        } else {
            OpdShape::Node(left, right)
        }
    }
}

/// Pass1 operator evaluation entry point (eval.md §3/§4): the shared
/// "connectable" check for the four operator branches `-`/`+`/`->`/`<-`.
/// Shape constraints follow the §5 legal-operation table via
/// [`opcheck`](crate::semantic::opcheck) (the same module Pass2 uses); the
/// single-port (1*1) representative side is determined by [`representative`]
/// per the §4 notes.
///
/// Wildcards kept:
/// - Empty set (unresolved FuncCall return value) → pass;
/// - `<error` placeholder marker → pass;
/// - Both sides 1*1 → pass (single-port representative rule).
/// - Series `1*1` vs `N*1` → **row-count mismatch** (single-point broadcast
///   `X -> [A, B]` / `[A, B] -> GND` is not a §5 operation; no broadcast
///   carve-out), reported by the operator handlers (E4007) unless the side
///   count is unknown.
/// - Parallel single point (1 row) vs N-row (N ≥ 2) → **row-count mismatch**,
///   reported by the operator handlers (E4005) — the §5.1 left-alignment rule.
///   A transposed operand is first transposed to its full-width column by the
///   caller ([`eval_port_elems`]) before this check runs, so there is no
///   transpose carve-out here.
fn is_connectable(
    op: ConnOp,
    dir: ConnDir,
    lhs: &OpdShape,
    rhs: &OpdShape,
    align: crate::semantic::opcheck::ParallelAlign,
) -> bool {
    // The contact side depends on the operator (vec-arch.md §5.3): series
    // touches `lhs.right x rhs.left`, parallel touches the `align`-selected
    // side. Selecting it here keeps the side-selection local to this call;
    // opcheck re-derives the same side from the full shape internally.
    let (lhs_side, rhs_side): (Vec<McBus>, Vec<McBus>) = match op {
        ConnOp::Series => (lhs.port_right(), rhs.port_left()),
        ConnOp::Parallel => match align {
            crate::semantic::opcheck::ParallelAlign::Left => (lhs.port_left(), rhs.port_left()),
            crate::semantic::opcheck::ParallelAlign::Right => (lhs.port_right(), rhs.port_right()),
        },
    };

    // Empty shape means "unknown/unresolved" (e.g. FuncCall return value), treated as connectable
    if lhs_side.is_empty() || rhs_side.is_empty() {
        return true;
    }

    // Shapes containing error/placeholder markers are also treated as connectable
    if lhs_side.iter().any(|b| b.name.contains("<error"))
        || rhs_side.iter().any(|b| b.name.contains("<error"))
    {
        return true;
    }

    let lhs_shape = shape_of_bus_list(&lhs_side);
    let rhs_shape = shape_of_bus_list(&rhs_side);

    // §4 single-port (1*1) representative rule (eval.md §4 note): a single-port
    // connection has no left/right distinction, so one representative is chosen —
    // `+`/`-`/`<-` take operand 1 (op1), `->` takes operand 2 (op2).
    // Consistent with the Pass2 anchoring: `+` anchors `wire_parallel_internal`
    // opd[0], `-` anchors the Series head opd1, `<-` anchors the RtoL chain tail
    // op1 (after the swap op1 lands at the tail), `->` anchors the LtoR chain tail op2.
    if lhs_shape.rows == 1 && rhs_shape.rows == 1 {
        mcc_dbg!(
            "sem::conds",
            "[vec] single-port representative: dir={dir:?} lhs={lhs_shape} rhs={rhs_shape} rep={rep}",
            rep = representative(op, dir, lhs_shape, rhs_shape)
        );
        return true;
    }

    // §5 legal-operation table (opcheck — shared with Pass2):
    // - Series (§5.2): lhs.right x rhs.left row counts must match; unequal
    //   rows (including `1*1` vs `N*1`) are rejected — no broadcast carve-out.
    // - Parallel (§5.1): the `align` side (left ports, or the right ports
    //   when both operands carry an independent right port) must have equal
    //   rows; no broadcast carve-out.
    match op {
        ConnOp::Series => {
            matches!(
                crate::semantic::opcheck::check_series(lhs, rhs),
                crate::semantic::opcheck::OpCheck::Legal(_)
            )
        }
        ConnOp::Parallel => {
            matches!(
                crate::semantic::opcheck::check_parallel(lhs, rhs, align),
                crate::semantic::opcheck::OpCheck::Legal(_)
            )
        }
    }
}

// ============================================================================
// Operator implementations
// ============================================================================

impl<R: Into<McPhrase>> Add<R> for McPhrase {
    type Output = McPhrase;
    fn add(self, other: R) -> Self::Output {
        McPhrase::parallel(vec![self, other.into()])
    }
}

impl<R: Into<McPhrase>> Shr<R> for McPhrase {
    type Output = McPhrase;
    fn shr(self, other: R) -> Self::Output {
        McPhrase::series(vec![self, other.into()])
    }
}

// Note: Shr is used for the `->` operator in Rust code constructing McPhrase.
// The `MCAST_OPD_RIGHTARROW` handler in `new()` is the primary path for parsing `->`
// from source code and correctly uses ConnDir::LtoR.
