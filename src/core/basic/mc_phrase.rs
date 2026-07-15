// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::super::{
    basic::mc_bus::McBus,
    basic::mc_closure::McClosure,
    basic::mc_endpoint::{McEndpoint, McInstanceRef},
    basic::mc_fcall::McFuncCall,
    basic::mc_group::McGroup,
    common::{IOType, McCMIE},
    component::Mc2Component,
    mc_func::HasFindInst,
    mc_inst::McInstance,
    module::Mc2Module,
};
use crate::{
    ast::{ast_node::AstNode, c_macros::*, error::message::*},
    builder::{
        diagnostic::{
            dlog_error, dlog_trace, dlog_warning,
            message_templates::{CANNOT_TRANSPOSE, SHAPE_MISMATCH},
        },
        inst_ref_validator::validate_inst_reference,
        mcb_get_cmie, mcb_register_instance_ref,
    },
    core::basic::{mc_opd::McOpd, mc_param::McParamValue},
    McIds,
};

use std::ops::{Add, Shr};
use std::sync::Arc;

// ============================================================================
// McPhrase
// ============================================================================

#[derive(Debug, Clone)]
pub enum McPhrase {
    Lead,
    Endpoint(McEndpoint),
    Series(Vec<McPhrase>),
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
                McPhrase::Series(items) => flat.extend(items),
                other => flat.push(other),
            }
        }
        match flat.len() {
            0 => McPhrase::Series(vec![]),
            1 => flat.into_iter().next().unwrap(),
            _ => McPhrase::Series(flat),
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
        let _node_name = format!("{}", node_type);
        let _node_str = node.to_string();
        // eprintln!(
        //     "[PHRASE_DEBUG] new: type={}, str_repr={:?}",
        //     node_name, node_str
        // );
        match node_type {
            MCAST_OPD_USCORE => Some(McPhrase::Lead),

            MCAST_OPD_THIS => {
                let mut this_ids = McIds::from("this");
                if let Some(nextnode) = node.get_next() {
                    this_ids.append(&nextnode);
                }
                context.add_label(this_ids.to_string())
            }

            MCAST_OPD => {
                // Handle MCAST_OPD node - extract subnode and process as McOpd
                let subnode = node.get_sub_node()?;
                if let Some(opdc) = McOpd::new(&subnode) {
                    // Convert McOpd to McPhrase
                    match opdc {
                        McOpd::Id(ids) => {
                            let ids_str = ids.to_string();
                            let _is_curly = ids.is_curly_bracket();
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
                                // ★ LSP: Register instance reference for MCAST_OPD path
                                let span = (subnode.get_pos() as usize)
                                    ..((subnode.get_pos() + subnode.get_len()) as usize);
                                if let Some(decl_id) = crate::builder::mcb_lookup_instance_decl(
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
                                            crate::builder::mcb_lookup_instance_decl(
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
                                        dlog_error(1705, node, &format!("Name '{}' is already an instance, cannot create bus with members [{}]", name, members.join(", ")));
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
                                            1700,
                                            node,
                                            &format!(
                                                "Interface '{interface}.{full_name}' not found in component '{component}'"
                                            ),
                                        );
                                    } else {
                                        dlog_error(
                                            1702,
                                            node,
                                            &format!(
                                                "Component '{component}' not found for interface '{component}.{interface}'"
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
                                    //   1. body line `[VDD_3V3, GND] -> lp322dcdc{Vin, GND}`
                                    //      left is 1×1 scalar, right is 2×1 bus, dimension mismatch
                                    //   2. DC interface's positive and GND can't be separated into independent nets
                                    //   3. global GND bus can't form a net
                                    //
                                    // Fix: use ids.expand() to extract member list, generate Multiple
                                    // make each member independent NetPoint, support N×N pairwise connection.
                                    //
                                    // Example:
                                    //   `[VDD_3V3, GND]` → Multiple([Label("VDD_3V3"), Label("GND")])
                                    //   `[VDD_3V3, GND] -> lp322dcdc{Vin, GND}`
                                    //   → 2×2 zip: VDD_3V3~lp322dcdc.Vin, GND~lp322dcdc.GND
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
                                                        2006,
                                                        node,
                                                        &format!(
                                                            "DROPPED_STATEMENT: indexed alias '{}' expands to '{}' which is not a known instance. \
                                                             The statement may produce no nets or constraints.",
                                                            ids.to_string(),
                                                            expanded_name
                                                        ),
                                                    );
                                                }
                                            }
                                        }
                                        context.add_label(ids.to_string())
                                    }
                                }
                            } else {
                                let id_str = ids.to_string();
                                if let Some((base, rest)) = id_str.split_once('.') {
                                    if let Some((bus_name, pin_name)) = rest.split_once('.') {
                                        if context.is_component_bus(base, bus_name) {
                                            let full_name = format!("{base}.{bus_name}");
                                            let member_ref =
                                                McBus::member_ref(&full_name, pin_name.to_string());
                                            return Some(McPhrase::Endpoint(McEndpoint::Single(
                                                McInstanceRef::new(McInstance::Bus(member_ref)),
                                            )));
                                        }
                                    }
                                    if context.find_inst(base).is_none() {
                                        context.add_bus(base.to_string(), vec![rest.to_string()]);
                                    } else {
                                        // E1802: Check if base is a Component and rest is a valid pin
                                        if let Some(McInstance::Component(c)) =
                                            context.find_inst(base)
                                        {
                                            if c.base.pins.find_pin(&rest).is_none() {
                                                dlog_error(
                                                    1802,
                                                    &subnode,
                                                    &format!(
                                                        "Pin '{}' not found in component '{}'",
                                                        rest, base
                                                    ),
                                                );
                                                return None;
                                            }
                                        }
                                        // ★ LSP: Register instance reference for dot-separated path
                                        let span = (subnode.get_pos() as usize)
                                            ..((subnode.get_pos() + subnode.get_len()) as usize);
                                        if let Some(decl_id) =
                                            crate::builder::mcb_lookup_instance_decl(
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
                                        ))) = context.add_bus_member(base, rest.to_string())
                                        {
                                            return Some(McPhrase::Endpoint(McEndpoint::Single(
                                                McInstanceRef::new(McInstance::Bus(bus)),
                                            )));
                                        }
                                    }
                                    let member_ref = McBus::member_ref(base, rest.to_string());
                                    Some(McPhrase::Endpoint(McEndpoint::Single(
                                        McInstanceRef::new(McInstance::Bus(member_ref)),
                                    )))
                                } else {
                                    context.add_label(id_str)
                                }
                            }
                        }
                        McOpd::This(ids) => context.add_label(ids.to_string()),
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
                    dlog_error(1100, node, "Failed to extract ID/IDA data");
                    return None;
                }

                let mut items: Vec<Option<crate::McInstance>> =
                    data.iter().map(|id| context.find_inst(id)).collect();

                if items.len() == 1 {
                    if let Some(ident) = items.remove(0) {
                        // ★ LSP: Register instance reference when found in symbol table
                        let span =
                            (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                        if let Some(decl_id) = crate::builder::mcb_lookup_instance_decl(
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
                                    if c.base.pins.find_pin(member).is_none() {
                                        dlog_error(
                                            1802,
                                            node,
                                            &format!(
                                                "Pin '{}' not found in component '{}'",
                                                member, base
                                            ),
                                        );
                                        return None;
                                    }
                                }
                                // ★ LSP: Register instance reference for dot-separated path
                                let span = (node.get_pos() as usize)
                                    ..((node.get_pos() + node.get_len()) as usize);
                                if let Some(decl_id) = crate::builder::mcb_lookup_instance_decl(
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
                    for c in sub.iter() {
                        let t = c.get_type();
                        if t == MCAST_CLASS && class_node.is_none() {
                            class_node = Some(c.clone());
                        } else if t == MCAST_INSTANCE {
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
                            }
                        }
                    }
                    if let Some(cls) = class_node {
                        if let Some(class_ids) = cls.get_sub_node().and_then(|cid| McIds::new(&cid))
                        {
                            let fname = class_ids.to_string();
                            let build = names.is_empty()
                                && crate::vector::graph::naming::is_known_twopin_class(&fname);
                            if build {
                                let mut params: Vec<McParamValue> = Vec::new();
                                let mut cur = cls.get_sub_node();
                                while let Some(n) = cur {
                                    if n.get_type() == MCAST_PARAMS {
                                        if let Some(ps) = n.get_sub_node() {
                                            for p in ps.iter() {
                                                if let Some(v) =
                                                    crate::core::basic::mc_param::McParamValue::new(
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
                                let mut fcs: Vec<McPhrase> = Vec::with_capacity(names.len());
                                for _ in &names {
                                    fcs.push(McPhrase::FuncCall(McFuncCall {
                                        caller: None,
                                        func_name: class_ids.clone(),
                                        params: params.clone(),
                                        left: left.clone(),
                                        right: right.clone(),
                                        dot_member: None,
                                    }));
                                }
                                return Some(if fcs.len() == 1 {
                                    fcs.into_iter().next().unwrap()
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
                    dlog_error(1101, node, "Failed to parse DECLARE");
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
                        McInstance::BusRef {
                            component: _,
                            bus: _,
                        } => todo!(),
                    },
                    _ => format!("{:?}", std::mem::discriminant(&left_opd)),
                };
                // eprintln!("[OPD-DOT] left_opd_kind={} right={:?}", left_kind, right);

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
                                    if !c.base.pins.find_pin(member).is_some() {
                                        dlog_error(
                                            1802,
                                            node,
                                            &format!(
                                                "Pin '{}' not found in component '{}'",
                                                member, inst_name
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
                                dlog_error(
                                    1802,
                                    node,
                                    &format!(
                                        "Pin '{}' not found in component '{}'",
                                        member, inst_name
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
                                        dlog_error(
                                            1803,
                                            node,
                                            &format!(
                                                "Port '{}' not found in module '{}'",
                                                member, inst_name
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
                                dlog_error(
                                    1803,
                                    node,
                                    &format!(
                                        "Port '{}' not found in module '{}'",
                                        member, inst_name
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
                if !subnode2.is_type(MCAST_OPD_IDAN) {
                    dlog_error(1103, &subnode2, "Expected IDAN node");
                    return None;
                }
                let subnode2 = subnode2.get_sub_node().expect(MISSING_SUBNODE);
                let right: Vec<String> = subnode2
                    .iter()
                    .flat_map(|n| n.to_id_or_ida_or_num())
                    .collect();

                let _left_kind = match &left_opd {
                    McPhrase::Endpoint(McEndpoint::Single(ir)) => match &ir.base {
                        McInstance::Label(s) => format!("Label('{s}')"),
                        McInstance::Bus(b) => format!("Bus('{}', mem={:?})", b.name, b.member),
                        McInstance::Component(c) => format!("Component('{}')", c.name),
                        McInstance::Module(m) => format!("Module('{}')", m.name),
                        McInstance::Interface(i) => format!("Interface('{}')", i.name),
                        McInstance::List(l) => format!("List('{}', mem={:?})", l.name, l.member),
                        McInstance::Unresolved { class_name } => format!("?{class_name}"),
                        McInstance::BusRef {
                            component: _,
                            bus: _,
                        } => todo!(),
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
                // Use single "Bus with members" as fallback, aligning mcu513{DAC_OUT,SPK_MUTE} /
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
                                    2006,
                                    node,
                                    &format!(
                                        "DROPPED_STATEMENT: indexed alias '{}' expands to '{}' which is not a known instance. \
                                         The statement may produce no nets or constraints.",
                                        ids.to_string(),
                                        expanded_name
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

                // ★ Fix: Extract right1/right2 before match so both arms can use them
                if !subnode2.is_type(MCAST_OPD_IDAN) {
                    dlog_error(1005, &subnode2, "Expected IDAN");
                    return None;
                }
                let subnode2_inner = subnode2.get_sub_node().expect(MISSING_SUBNODE);
                let right1: Vec<String> = subnode2_inner
                    .iter()
                    .flat_map(|n| n.to_id_or_ida_or_num())
                    .collect();

                if !subnode3.is_type(MCAST_OPD_IDAN) {
                    dlog_error(1105, &subnode3, "Expected IDAN");
                    return None;
                }
                let subnode3_inner = subnode3.get_sub_node().expect(MISSING_SUBNODE);
                let right2: Vec<String> = subnode3_inner
                    .iter()
                    .flat_map(|n| n.to_id_or_ida_or_num())
                    .collect();

                // eprintln!("[CMN-DIAG] CURLY_MN left={} right1={:?} right2={:?}",
                //     match &left_opd {
                //         McPhrase::Endpoint(McEndpoint::Single(r)) =>
                //             format!("Endpoint({:?})", std::mem::discriminant(&r.base)),
                //         McPhrase::Member(..) => "Member".to_string(),
                //         McPhrase::Multiple(_) => "Multiple".to_string(),
                //         other => format!("{:?}", std::mem::discriminant(other)),
                //     }, right1, right2);

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
                            if let Some(cmie) = mcb_get_cmie(&ids, context.uri()) {
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
                                    1106,
                                    node,
                                    &format!(
                                        "CURLY_MN: '{name}' definition not found, using label fallback"
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
                        // eprintln!("[CMN-DIAG] → None(1006) name={:?}", name);
                        dlog_error(1006, node, "CURLY_MN requires Component or Module");
                        None
                    }
                }
            }

            MCAST_OPD_APOST => {
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                match McPhrase::new(&opd1_node, context)? {
                    McPhrase::Series(phrases) => {
                        Some(McPhrase::Transposed(Box::new(McPhrase::Series(phrases))))
                    }
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Bus(_),
                        ..
                    }))
                    | McPhrase::Multiple(_)
                    | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                        base: McInstance::Label(_),
                        ..
                    }))
                    | McPhrase::Transposed(_) => {
                        dlog_error(1150, node, CANNOT_TRANSPOSE);
                        None
                    }
                    opd1 => Some(McPhrase::Transposed(Box::new(opd1))),
                }
            }

            MCAST_OPD_CARET => {
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                match McPhrase::new(&opd1_node, context)? {
                    McPhrase::Series(ref mut phrases) => {
                        phrases.reverse();
                        Some(McPhrase::Series(phrases.clone()))
                    }
                    opd1 => {
                        let mut phrases = vec![opd1];
                        phrases.reverse();
                        Some(McPhrase::Series(phrases))
                    }
                }
            }

            MCAST_OPD_FCALL => McFuncCall::parse(node, context),

            MCAST_OPD_CLOSURE => McClosure::parse(node, context).map(McPhrase::Closure),

            MCAST_OPD_GROUP => {
                // Parse once via McGroup::parse, avoid duplicate parsing
                McGroup::parse(node, context).map(McPhrase::Group)
            }

            MCAST_OPD_PLUS => {
                // ── D8: AMBIGUOUS_PRECEDENCE detection ──────────────────────
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                let loc_node = first_expr_leaf(&opd1_node);
                check_ambiguous_precedence(node, &loc_node);

                let opd2_node = opd1_node.get_next().expect(MISSING_SUBNODE);

                let opd1 = McPhrase::new(&opd1_node, context)?;
                let opd2 = McPhrase::new(&opd2_node, context)?;

                // Infer shapes and upgrade phrases before checking connectivity
                let (opd1, opd2) = infer_shape_and_upgrade(opd1, opd2, context);

                if !is_connectable(&opd1.get_left(), &opd2.get_left())
                    || !is_connectable(&opd1.get_right(), &opd2.get_right())
                {
                    dlog_error(1151, node, "Shape mismatch in parallel connection");
                    return None;
                }

                use McPhrase::*;
                match (opd1, opd2) {
                    (opd1 @ Transposed(_), opd2 @ Transposed(_)) => Some(Series(vec![opd1, opd2])),
                    (opd1 @ Transposed(_), opd2) => {
                        if opd2.get_left().iter().map(|e| e.size()).sum::<usize>() != 2 {
                            dlog_error(1102, node, "Transposed connection size mismatch");
                            return None;
                        }
                        let mut ret_line_members = vec![opd1];
                        if let Series(line) = opd2 {
                            ret_line_members.extend(line);
                        } else {
                            ret_line_members.push(opd2);
                        }
                        Some(Series(ret_line_members))
                    }
                    (opd1, opd2 @ Transposed(_)) => {
                        if opd1.get_right().iter().map(|e| e.size()).sum::<usize>() != 2 {
                            dlog_error(1153, node, "Transposed connection size mismatch");
                            return None;
                        }
                        if let Series(mut line) = opd1 {
                            line.push(opd2);
                            Some(Series(line))
                        } else {
                            Some(Series(vec![opd1, opd2]))
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
                // ── D8: AMBIGUOUS_PRECEDENCE detection ──────────────────────
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                let loc_node = first_expr_leaf(&opd1_node);
                check_ambiguous_precedence(node, &loc_node);

                let opd2_node = opd1_node.get_next().expect(MISSING_SUBNODE);

                let opd1 = McPhrase::new(&opd1_node, context)?;
                let opd2 = McPhrase::new(&opd2_node, context)?;

                let (opd1, opd2) = infer_shape_and_upgrade(opd1, opd2, context);

                if !is_connectable(&opd1.get_right(), &opd2.get_left()) {
                    dlog_error(1154, node, SHAPE_MISMATCH);
                    return None;
                }

                let mut ret_line: Vec<McPhrase> = match opd1 {
                    Series(line) => line,
                    _ => vec![opd1],
                };

                match opd2 {
                    Series(line2) => ret_line.extend(line2),
                    _ => ret_line.push(opd2),
                }
                Some(Series(ret_line))
            }

            MCAST_OPD_RIGHTARROW => {
                // ── D8: AMBIGUOUS_PRECEDENCE detection ──────────────────────
                let opd1_node = node.get_sub_node().expect(MISSING_SUBNODE);
                let loc_node = first_expr_leaf(&opd1_node);
                check_ambiguous_precedence(node, &loc_node);

                let opd2_node = opd1_node.get_next().expect(MISSING_SUBNODE);
                let opd1 = McPhrase::new(&opd1_node, context);
                let opd2 = McPhrase::new(&opd2_node, context);
                // if opd1.is_none() || opd2.is_none() {
                //     eprintln!("[ARROW-NONE] L t={} str={:?} ok={} | R t={} str={:?} ok={}",
                //         opd1_node.get_type(), opd1_node.to_string(), opd1.is_some(),
                //         opd2_node.get_type(), opd2_node.to_string(), opd2.is_some());
                // }
                let opd1 = opd1?;
                let opd2 = opd2?;

                let (opd1, opd2) = infer_shape_and_upgrade(opd1, opd2, context);

                if !is_connectable(&opd1.get_right(), &opd2.get_left()) {
                    dlog_error(1155, node, "Shape mismatch in -> connection");
                    return None;
                }

                // ret_line: Vec<McPhrase> representing the accumulated line
                let mut ret_line: Vec<McPhrase> = match opd1 {
                    Series(phrases) => phrases,
                    _ => vec![opd1],
                };
                // Set right side of ret_line as output
                if let Some(last) = ret_line.last_mut() {
                    last.set_right_out();
                }

                // line2: the second operand
                let mut line2 = match opd2 {
                    Series(phrases) => phrases,
                    _ => vec![opd2],
                };
                // Set left side of line2 as input
                if let Some(first) = line2.first_mut() {
                    first.set_left_in();
                }

                ret_line.extend(line2);
                Some(Series(ret_line))
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

                // Check if opd2.right can connect to opd1.left
                if !is_connectable(&opd2.get_right(), &opd1.get_left()) {
                    dlog_error(1106, node, "Shape mismatch in <- connection");
                    return None;
                }

                // opd2 is source, its right is output
                let mut ret_line: Vec<McPhrase> = match opd2 {
                    Series(phrases) => phrases,
                    _ => vec![opd2],
                };
                if let Some(last) = ret_line.last_mut() {
                    last.set_right_out();
                }

                // opd1 is the target, its left side is input
                let mut line1: Vec<McPhrase> = match opd1 {
                    Series(phrases) => phrases,
                    _ => vec![opd1],
                };
                if let Some(first) = line1.first_mut() {
                    first.set_left_in();
                }

                // Connection: opd2 -> opd1
                ret_line.extend(line1);
                Some(Series(ret_line))
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
                        if mcb_get_cmie(&ids, context.uri()).is_some() {
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
                                    if mcb_get_cmie(&ids, context.uri()).is_some() {
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
                    if mcb_get_cmie(&ids, context.uri()).is_some() {
                        return None;
                    }
                    return context.add_label(s);
                }
                dlog_error(
                    1003,
                    node,
                    "Failed to parse MCAST_INSTANCE in expression context",
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
                            if let Some(decl_id) = crate::builder::mcb_lookup_instance_decl(
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

            _ => {
                dlog_error(
                    1110,
                    node,
                    &format!(
                        "node={} Unexpected AST node type {} in McPhrase::new",
                        node.get_type(),
                        node.get_type()
                    ),
                );
                None
            }
        }
    }

    pub(crate) fn get_left(&self) -> Vec<McBus> {
        use IOType;
        match self {
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(ref c),
                ..
            })) => {
                let inst_name = c.name.to_string();
                // ── Iter-7.5b ─────────────────────────────────────────────
                // Same as line.rs phrase_to_members: only treat system library 2-pin dynamic classes
                // (CAP/RES/IND/DIODE/LED/FUSE) or anonymous instances (@ prefix) as 2-pin,
                // multi-pin dynamic components (LPA/FLASH/SPEAKER etc.) go through the original path.
                //
                // ── ★ P0-2: list moved to naming::is_known_twopin_class ──────
                // Single source of truth, supports both dotted form (DIO.ESD) and aliases (ESD/ZENER/...).
                let class_name = c.base.name.to_string();
                let is_known_2pin_class =
                    crate::vector::graph::naming::is_known_twopin_class(&class_name);
                let is_anon_inst = inst_name.starts_with('@');
                let static_count = c.base.pins.count();
                let dyn_two_pin = static_count == 0
                    && c.base.pins.has_dynamic_pins()
                    && (is_known_2pin_class || is_anon_inst);

                match (static_count, dyn_two_pin) {
                    (2, _) | (_, true) => vec![McBus::new(&format!("{inst_name}.1"))],
                    (0, _) | (1, _) => vec![McBus::new(&inst_name)],
                    _ => {
                        let in_pins = c.base.pins.get_pins_by_io(&IOType::In);
                        let out_pins = c.base.pins.get_pins_by_io(&IOType::Out);
                        let ps_pins = c.base.pins.get_pins_by_io(&IOType::Power);

                        if !in_pins.is_empty() && !out_pins.is_empty() {
                            in_pins
                                .iter()
                                .map(|p| McBus::new(&format!("{inst_name}.{p}")))
                                .collect()
                        } else if !ps_pins.is_empty() {
                            vec![McBus::new(&format!("{}.{}", inst_name, ps_pins[0]))]
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
                if list.name.is_empty() {
                    list.member.iter().map(|m| McBus::new(m)).collect()
                } else {
                    vec![McBus::new(list.name())]
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
            McPhrase::Series(ref phrases) => {
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
            McPhrase::FuncCall(ref f) => f.left.clone(),
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
                // ── Iter-7.5b ─────────────────────────────────────────────
                // Same as get_left: dynamic-pins treated as 2-pin (return .2), but only
                // limited to known 2-pin class names or anonymous instances, not affecting multi-pin dynamic components.
                //
                // ── ★ P0-2: list moved to naming::is_known_twopin_class ──────
                let class_name = c.base.name.to_string();
                let is_known_2pin_class =
                    crate::vector::graph::naming::is_known_twopin_class(&class_name);
                let is_anon_inst = inst_name.starts_with('@');
                let static_count = c.base.pins.count();
                let dyn_two_pin = static_count == 0
                    && c.base.pins.has_dynamic_pins()
                    && (is_known_2pin_class || is_anon_inst);

                match (static_count, dyn_two_pin) {
                    (2, _) | (_, true) => vec![McBus::new(&format!("{inst_name}.2"))],
                    (0, _) | (1, _) => vec![McBus::new(&inst_name)],
                    _ => {
                        let in_pins = c.base.pins.get_pins_by_io(&IOType::In);
                        let out_pins = c.base.pins.get_pins_by_io(&IOType::Out);
                        let ps_pins = c.base.pins.get_pins_by_io(&IOType::Power);

                        if !in_pins.is_empty() && !out_pins.is_empty() {
                            out_pins
                                .iter()
                                .map(|p| McBus::new(&format!("{inst_name}.{p}")))
                                .collect()
                        } else if !ps_pins.is_empty() && ps_pins.len() >= 2 {
                            vec![McBus::new(&format!("{}.{}", inst_name, ps_pins[1]))]
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
                if list.name.is_empty() {
                    list.member.iter().map(|m| McBus::new(m)).collect()
                } else {
                    vec![McBus::new(list.name())]
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
            McPhrase::Series(ref phrases) => {
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
            McPhrase::FuncCall(ref f) => f.right.clone(),
            McPhrase::Lead => vec![McBus::new("(lead)")],
            McPhrase::Endpoint(ref ep) => ep.get_right(),
            McPhrase::Member(_, ep) => ep.get_right(),
        }
    }

    /// Set the left side as input
    pub(crate) fn set_left_in(&mut self) {
        match self {
            McPhrase::Series(ref mut phrases) => {
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
            McPhrase::Series(ref mut phrases) => {
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
            McPhrase::Series(ref mut phrases) => phrases.reverse(),
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
                    if let Some(found) = c.base.pins.find_pin(&combined) {
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
                    let all_hit = member_names
                        .iter()
                        .all(|id| c.base.pins.find_pin(id).is_some());
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
                        c.base.pins.find_pin(id).map(|found| {
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                                McInstance::Bus(McBus::new(&format!("{inst_name}.{found}"))),
                            )))
                        })
                    })
                    .collect();

                if result.is_empty() {
                    dlog_trace(1162, "No ports found in component");
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
                let mut not_found_refs: Vec<String> = Vec::new();
                for (i, id) in member_names.iter().enumerate() {
                    if m.base.insts.find_port(id).is_some() {
                        found_members.push(id.clone());
                    } else {
                        not_found_refs.push(format!("{}.{}", inst_name, member_names[i]));
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
                for ref_str in not_found_refs {
                    let parts: Vec<&str> = ref_str.split('.').collect();
                    if parts.len() == 2 {
                        final_results.push(McPhrase::Endpoint(McEndpoint::Single(
                            McInstanceRef::new(McInstance::Bus(McBus::new_with_members(
                                parts[0],
                                vec![parts[1].to_string()],
                            ))),
                        )));
                    } else {
                        final_results.push(McPhrase::Endpoint(McEndpoint::Single(
                            McInstanceRef::new(McInstance::Label(ref_str)),
                        )));
                    }
                }

                if final_results.is_empty() {
                    dlog_trace(1163, "No ports found in module");
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
                    dlog_trace(1164, "No ports found in interface");
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
                                            .map(|e| {
                                                e.name.split('.').next_back().unwrap().to_string()
                                            })
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
            McPhrase::Series(_) => {
                dlog_trace(1168, "Dot operator does not apply for Series");
                None
            }
            McPhrase::Endpoint(McEndpoint::Node { .. }) => {
                dlog_trace(1169, "Dot operator does not apply for Node");
                None
            }
            McPhrase::Transposed(_) => {
                dlog_trace(1170, "Dot operator does not apply for Transposed");
                None
            }
            McPhrase::Lead => {
                dlog_trace(1171, "Dot operator does not apply for Lead");
                None
            }
            McPhrase::Group(_) => {
                dlog_trace(1172, "Dot operator does not apply for Group");
                None
            }
            McPhrase::Closure(ref c) => {
                // Phase 3: access members of the closure's output interface
                if c.right.is_empty() {
                    dlog_trace(1173, "Closure has no output interface to access");
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
                    dlog_trace(1174, "FuncCall has no return interface to access");
                    return None;
                }
                Self::access_node_element_members(&f.right, member_names)
            }
            McPhrase::Endpoint(_ep) => {
                dlog_trace(1175, "Dot operator does not apply for Endpoint");
                None
            }
            McPhrase::Member(_, _) => {
                dlog_trace(1176, "Dot operator already applied for Member");
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
                        1175,
                        &format!("Member '{member_name}' not found in interface"),
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
            dlog_trace(1197, "curly_mn: left member list is empty");
            return None;
        }
        if right2.is_empty() {
            dlog_trace(1198, "curly_mn: right member list is empty");
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
                    dlog_trace(1199, "Cannot convert to McBus in curly_mn");
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
        McPhrase::Series(phrases) => {
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
        McPhrase::Parallel(_) => true,
        McPhrase::Transposed(_) => true,
        McPhrase::Multiple(_) => true,
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
            McPhrase::Series(phrases) => {
                let items: Vec<String> = phrases.iter().map(format_series_item).collect();
                write!(f, "{}", items.join(" -> "))
            }
            McPhrase::Parallel(phrases) => {
                let items: Vec<String> = phrases.iter().map(|p| format!("{p}")).collect();
                write!(f, "{}", items.join(" + "))
            }
            McPhrase::Transposed(p) => write!(f, "({p})'"),
            McPhrase::Multiple(phrases) => {
                let items: Vec<String> = phrases.iter().map(|p| format!("{p}")).collect();
                write!(f, "[{}]", items.join(", "))
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

                // Print caller or pre-closure parameter
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
                        write!(f, " -> ")?;
                    } else {
                        write!(f, "{c}")?;
                        write!(f, ".")?;
                    }
                }
                write!(f, "{}", fc.func_name)?;
                let param_strs: Vec<String> = fc.params.iter().map(|p| format!("{p}")).collect();
                // If in pre-closure mode, skip the leading "_" placeholder (it is shown via Series)
                let display_params =
                    if caller_is_pre_closure && param_strs.first() == Some(&"_".to_string()) {
                        &param_strs[1..]
                    } else {
                        &param_strs
                    };
                write!(f, "({})", display_params.join(", "))?;
                Ok(())
            }
            McPhrase::Member(phrase, ep) => {
                write!(f, "{phrase}.{ep}")
            }
            McPhrase::Group(g) => {
                if g.opds.len() == 1 {
                    write!(f, "{}", g.opds[0])
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

/// ── D8: AMBIGUOUS_PRECEDENCE detection ─────────────────────────────────
/// Walk the AST subtree to check if the expression mixes `+`, `-`, `->`
/// operators without explicit parentheses (Group), spanning more than 2
/// leaf components. If so, emit a warning because the intended grouping
/// may differ from the parser's precedence.
/// `loc_node` specifies the node for error location (use first operand to avoid trailing comments).
fn check_ambiguous_precedence(node: &AstNode, loc_node: &AstNode) {
    let (leaf_count, has_plus, has_minus, has_arrow) = analyze_expr_tree(node);
    let mixed = (has_plus as u8 + has_minus as u8 + has_arrow as u8) >= 2;
    if mixed && leaf_count > 2 {
        dlog_warning(
            2008,
            loc_node,
            &format!(
                "AMBIGUOUS_PRECEDENCE: expression mixes +,-,-> operators without parentheses \
                 and spans {leaf_count} components (>2). Consider adding explicit parentheses \
                 (Group) to clarify the intended grouping."
            ),
        );
    }
}

/// Drill down to the first non-op leaf node in an expression tree,
/// to avoid spanning multi-line comments in error locations.
fn first_expr_leaf(node: &AstNode) -> AstNode {
    let t = node.get_type();
    match t {
        MCAST_OPD_PLUS | MCAST_OPD_MINUS | MCAST_OPD_RIGHTARROW | MCAST_OPD_LEFTARROW => {
            if let Some(sub) = node.get_sub_node() {
                return first_expr_leaf(&sub);
            }
        }
        _ => {}
    }
    node.clone()
}

/// Walk the AST subtree and return (leaf_count, has_plus, has_minus, has_arrow).
fn analyze_expr_tree(node: &AstNode) -> (usize, bool, bool, bool) {
    let t = node.get_type();
    match t {
        MCAST_OPD_PLUS | MCAST_OPD_MINUS | MCAST_OPD_RIGHTARROW | MCAST_OPD_LEFTARROW => {
            let is_plus = t == MCAST_OPD_PLUS;
            let is_minus = t == MCAST_OPD_MINUS;
            let is_arrow = t == MCAST_OPD_RIGHTARROW || t == MCAST_OPD_LEFTARROW;
            let mut total = 0usize;
            let mut hp = is_plus;
            let mut hm = is_minus;
            let mut ha = is_arrow;
            if let Some(sub) = node.get_sub_node() {
                let (c, p, m, a) = analyze_expr_tree(&sub);
                total += c;
                hp |= p;
                hm |= m;
                ha |= a;
                if let Some(next) = sub.get_next() {
                    let (c, p, m, a) = analyze_expr_tree(&next);
                    total += c;
                    hp |= p;
                    hm |= m;
                    ha |= a;
                }
            }
            if total == 0 {
                total = 1; // degenerate case: count as at least 1
            }
            (total, hp, hm, ha)
        }
        MCAST_OPD_GROUP => {
            // Group (parentheses) resets the ambiguity scope — treat as a single leaf
            (1, false, false, false)
        }
        _ => {
            // Leaf component: count as 1, recursively check children for nested operators
            let mut total = 1usize;
            let mut hp = false;
            let mut hm = false;
            let mut ha = false;
            if let Some(sub) = node.get_sub_node() {
                let (c, p, m, a) = analyze_expr_tree(&sub);
                // Don't add to total for non-operator children (they're part of this leaf)
                hp |= p;
                hm |= m;
                ha |= a;
                let _ = c; // child count absorbed into this leaf
                           // Check siblings
                let mut next = sub.get_next();
                while let Some(n) = next {
                    let (c, p, m, a) = analyze_expr_tree(&n);
                    total += c;
                    hp |= p;
                    hm |= m;
                    ha |= a;
                    next = n.get_next();
                }
            }
            (total, hp, hm, ha)
        }
    }
}

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
                dlog_trace(1220, "Groups with different branch counts cannot connect");
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

fn is_connectable(lhs: &[McBus], rhs: &[McBus]) -> bool {
    // Empty shape means "unknown/unresolved" (e.g. FuncCall return value), treated as connectable
    if lhs.is_empty() || rhs.is_empty() {
        return true;
    }

    // Fast path: shape fully matches
    if is_same_shape(lhs, rhs) {
        return true;
    }

    let left_size: usize = lhs.iter().map(|each| each.size()).sum();
    let right_size: usize = rhs.iter().map(|each| each.size()).sum();

    // Compatible when sizes are equal
    if left_size == right_size {
        return true;
    }

    // Size <= 1 is treated as a wildcard: Label, single pin, unresolved reference, etc. at the phrase stage
    // have no determined shape yet; the actual shape is only determined at the instantiation stage,
    // and should not be blocked here.
    if left_size <= 1 || right_size <= 1 {
        return true;
    }

    // Shapes containing error/placeholder markers are also treated as connectable
    if lhs.iter().any(|b| b.name.contains("<error"))
        || rhs.iter().any(|b| b.name.contains("<error"))
    {
        return true;
    }

    false
}

fn is_same_shape(lhs: &[McBus], rhs: &[McBus]) -> bool {
    if lhs.len() != rhs.len() {
        return false;
    }

    // After flattening: compare whether member counts are the same
    // member.len() == 0 means single wire, member.len() > 0 means bus
    lhs.iter()
        .zip(rhs.iter())
        .all(|(l, r)| l.member.len() == r.member.len())
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
