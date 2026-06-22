// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_bus::McBus;
use super::mc_endpoint::{McEndpoint, McInstanceRef};
use super::mc_ids::McIds;
use super::mc_opd::McOpd;
use super::mc_param::McParamValue;
use super::mc_phrase::McPhrase;
use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::builder::diagnostic::dlog_error;
use crate::builder::mcb_get_cmie;
use crate::core::common::McCMIE;
use crate::core::component::Mc2Component;
use crate::core::mc_func::{HasFindInst, McFuncReturn};
use crate::core::mc_ifs::Mc2Interface;
use crate::core::mc_inst::McInstance;
use crate::core::module::Mc2Module;
use std::sync::Arc;

/// Function call
#[derive(Debug, Clone)]
pub struct McFuncCall {
    /// Caller operand
    pub caller: Option<Box<McPhrase>>,
    /// Function name
    pub func_name: McIds,
    /// Parameter list
    pub params: Vec<McParamValue>,
    /// Input interface (decided by caller)
    pub left: Vec<McBus>,
    /// Output interface (decided by function return value)
    pub right: Vec<McBus>,
    /// Chained member access (e.g., ".I2C0" in i2c(0x36).I2C0)
    pub dot_member: Option<String>,
}

impl McFuncCall {
    /// Parse function call from AST node
    pub fn parse(node: &AstNode, context: &mut dyn HasFindInst) -> Option<McPhrase> {
        Self::parse_internal(node, context, |n, ctx| McPhrase::new(n, ctx))
    }

    /// Internal parse function, uses callback to avoid circular dependency
    fn parse_internal<F>(
        node: &AstNode,
        context: &mut dyn HasFindInst,
        parse_phrase: F,
    ) -> Option<McPhrase>
    where
        F: Fn(&AstNode, &mut dyn HasFindInst) -> Option<McPhrase>,
    {
        let subnode = node
            .get_sub_node()
            .expect(crate::ast::error::message::MISSING_SUBNODE);

        let mut caller: Option<Box<McPhrase>> = None;
        let mut func_name: Option<McIds> = None;
        let mut params: Vec<McParamValue> = Vec::new();

        // === Handle pre-closure parameter (vin => CAP(10uF).Cap(_)) ===
        // Pattern: opd => ClassName(params).MethodName(method_params)
        // AST structure:
        //   MCAST_OPD_FCALL (outer)
        //     MCAST_PARAMS_PRE (22) -> vin (pre-closure param)
        //     MCAST_INSTANCE (29) -> Contains inner FCall for "ClassName(params)"
        //     MCAST_NAME (21) -> MethodName
        //     MCAST_PARAMS (23) -> method_params

        // First, check if this is a pre-closure pattern
        let mut pre_param_opt: Option<McParamValue> = None;
        let mut instance_name: Option<McIds> = None;
        let mut instance_params: Vec<McParamValue> = Vec::new();
        let mut method_name_opt: Option<McIds> = None;
        let mut method_params: Vec<McParamValue> = Vec::new();

        // eprintln!("[DEBUG] mc_fcall: parse_func_call, subnode type={}", subnode.get_type());
        // for (i, child) in subnode.iter().enumerate() {
        //     eprintln!("[DEBUG] mc_fcall: child[{}] type={}", i, child.get_type());
        // }

        // Check if first child is MCAST_PARAMS_PRE (pre-closure param)
        if let Some(first) = subnode.iter().next() {
            // Case 1: First child is MCAST_PARAMS_PRE directly (pattern: vin => ...)
            if first.get_type() == MCAST_PARAMS_PRE {
                if let Some(pre_inner) = first.get_sub_node() {
                    pre_param_opt = McParamValue::new(&pre_inner, context);
                }

                let mut name_params_pairs: Vec<(u16, AstNode)> = Vec::new();
                for each in subnode.iter().skip(1) {
                    let t = each.get_type();
                    if t == MCAST_NAME || t == MCAST_PARAMS {
                        name_params_pairs.push((t, each.clone()));
                    }
                }

                if name_params_pairs.len() >= 2 {
                    let (_, name_node) = &name_params_pairs[0];
                    if let Some(ids_node) = name_node.get_sub_node() {
                        instance_name = McIds::new(&ids_node);
                    }
                    let (_, params_node) = &name_params_pairs[1];
                    if let Some(params_sub) = params_node.get_sub_node() {
                        for p in params_sub.iter() {
                            if let Some(v) = McParamValue::new(&p, context) {
                                instance_params.push(v);
                            }
                        }
                    }

                    if name_params_pairs.len() >= 4 {
                        let (_, name_node2) = &name_params_pairs[2];
                        if let Some(ids_node2) = name_node2.get_sub_node() {
                            method_name_opt = McIds::new(&ids_node2);
                        }
                        let (_, params_node2) = &name_params_pairs[3];
                        if let Some(params_sub2) = params_node2.get_sub_node() {
                            for p in params_sub2.iter() {
                                if let Some(v) = McParamValue::new(&p, context) {
                                    method_params.push(v);
                                }
                            }
                        }
                    }
                }
            }
            // Case 2: First child is MCAST_OPD_FCALL containing pre-closure
            else if first.get_type() == MCAST_OPD_FCALL {
                if let Some(inner_sub) = first.get_sub_node() {
                    let inner_children: Vec<_> = inner_sub.iter().collect();

                    if let Some(inner_first) = inner_children.first() {
                        if inner_first.get_type() == MCAST_PARAMS_PRE {
                            if let Some(pre_inner) = inner_first.get_sub_node() {
                                pre_param_opt = McParamValue::new(&pre_inner, context);
                            }

                            let mut inner_pairs: Vec<(u16, AstNode)> = Vec::new();
                            for child in inner_children.iter() {
                                let t = child.get_type();
                                if t == MCAST_NAME || t == MCAST_PARAMS {
                                    inner_pairs.push((t, child.clone()));
                                }
                            }

                            if inner_pairs.len() >= 2 {
                                let (_, name_node) = &inner_pairs[0];
                                if let Some(ids_node) = name_node.get_sub_node() {
                                    instance_name = McIds::new(&ids_node);
                                }
                                let (_, params_node) = &inner_pairs[1];
                                if let Some(params_sub) = params_node.get_sub_node() {
                                    for p in params_sub.iter() {
                                        if let Some(v) = McParamValue::new(&p, context) {
                                            instance_params.push(v);
                                        }
                                    }
                                }
                            }

                            let mut outer_pairs: Vec<(u16, AstNode)> = Vec::new();
                            for child in subnode.iter().skip(1) {
                                let t = child.get_type();
                                if t == MCAST_NAME || t == MCAST_PARAMS {
                                    outer_pairs.push((t, child.clone()));
                                }
                            }

                            if outer_pairs.len() >= 2 {
                                let (_, name_node2) = &outer_pairs[0];
                                if let Some(ids_node2) = name_node2.get_sub_node() {
                                    method_name_opt = McIds::new(&ids_node2);
                                }
                                let (_, params_node2) = &outer_pairs[1];
                                if let Some(params_sub2) = params_node2.get_sub_node() {
                                    for p in params_sub2.iter() {
                                        if let Some(v) = McParamValue::new(&p, context) {
                                            method_params.push(v);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Case 3: First child is MCAST_INSTANCE containing pre-closure FCall
            // Pattern: MCAST_INSTANCE(inner FCall with pre-closure) + MCAST_NAME(method) + MCAST_PARAMS(params)
            else if first.get_type() == MCAST_INSTANCE {
                if let Some(inst_inner) = first.get_sub_node() {
                    if inst_inner.get_type() == MCAST_OPD_FCALL {
                        if let Some(inner_sub) = inst_inner.get_sub_node() {
                            let inner_children: Vec<_> = inner_sub.iter().collect();

                            if let Some(inner_first) = inner_children.first() {
                                if inner_first.get_type() == MCAST_PARAMS_PRE {
                                    if let Some(pre_inner) = inner_first.get_sub_node() {
                                        pre_param_opt = McParamValue::new(&pre_inner, context);
                                    }

                                    let mut inner_pairs: Vec<(u16, AstNode)> = Vec::new();
                                    for child in inner_children.iter() {
                                        let t = child.get_type();
                                        if t == MCAST_NAME || t == MCAST_PARAMS {
                                            inner_pairs.push((t, child.clone()));
                                        }
                                    }

                                    if inner_pairs.len() >= 2 {
                                        let (_, name_node) = &inner_pairs[0];
                                        if let Some(ids_node) = name_node.get_sub_node() {
                                            instance_name = McIds::new(&ids_node);
                                        }
                                        let (_, params_node) = &inner_pairs[1];
                                        if let Some(params_sub) = params_node.get_sub_node() {
                                            for p in params_sub.iter() {
                                                if let Some(v) = McParamValue::new(&p, context) {
                                                    instance_params.push(v);
                                                }
                                            }
                                        }
                                    }

                                    let mut outer_pairs: Vec<(u16, AstNode)> = Vec::new();
                                    for child in subnode.iter().skip(1) {
                                        let t = child.get_type();
                                        if t == MCAST_NAME || t == MCAST_PARAMS {
                                            outer_pairs.push((t, child.clone()));
                                        }
                                    }

                                    if outer_pairs.len() >= 2 {
                                        let (_, name_node2) = &outer_pairs[0];
                                        if let Some(ids_node2) = name_node2.get_sub_node() {
                                            method_name_opt = McIds::new(&ids_node2);
                                        }
                                        let (_, params_node2) = &outer_pairs[1];
                                        if let Some(params_sub2) = params_node2.get_sub_node() {
                                            for p in params_sub2.iter() {
                                                if let Some(v) = McParamValue::new(&p, context) {
                                                    method_params.push(v);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // If we found all parts of the pre-closure pattern
        if pre_param_opt.is_some() && instance_name.is_some() && method_name_opt.is_some() {
            let pre_param = pre_param_opt.unwrap();

            // Create pre_param as Label (for Series display: pre_param -> ...)
            let pre_label = McPhrase::label(pre_param.to_string());

            // ── R3: `=>` fold unified rules ───────────────────────────────────────
            let is_uscore = |p: &McParamValue| {
                matches!(p, McParamValue::NONE(_)) || matches!(p, McParamValue::Opd(McOpd::Uscore))
            };
            let m_last = method_name_opt
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_default();
            let m_last = m_last.rsplit('.').next().unwrap_or("").to_string();
            let is_twopin = m_last == "Cap"
                || m_last.eq_ignore_ascii_case("Pullup")
                || m_last.eq_ignore_ascii_case("Pulldown");
            let all_ph = !method_params.is_empty() && method_params.iter().all(&is_uscore);

            let all_method_params: Vec<McParamValue> = if is_twopin && all_ph {
                // (a) Pure `.Cap(_)`: don't fold, pre_param stays as chain head (pre_label) for shunt path
                method_params
            } else if is_twopin && method_params.iter().any(&is_uscore) {
                // (b) `.Pullup(_, VDD)`: pre_param replaces first `_` placeholder → [I2C0, VDD]
                let mut replaced = false;
                method_params
                    .into_iter()
                    .map(|p| {
                        if !replaced && is_uscore(&p) {
                            replaced = true;
                            pre_param.clone()
                        } else {
                            p
                        }
                    })
                    .collect()
            } else {
                // (c) Non-two-pin: keep original prepend
                let mut v = vec![pre_param.clone()];
                v.extend(method_params);
                v
            };

            // R4: don't instantiate at parse time. Built-in two-pin components handled by instantiation-phase process_member_internal
            // unified creation —— same P1-D branch-1 path as `->` form, consistent naming/wiring.

            // Create inner FuncCall: ClassName(instance_params)
            let inner_call = McFuncCall {
                caller: None,
                func_name: instance_name.unwrap(),
                params: instance_params,
                left: vec![],
                right: vec![],
                dot_member: None,
            };

            // Create outer FuncCall: ClassName(params).MethodName(all_method_params)
            let outer_call = McFuncCall {
                caller: Some(Box::new(McPhrase::FuncCall(inner_call))),
                func_name: method_name_opt.unwrap(),
                params: all_method_params,
                left: vec![],
                right: vec![],
                dot_member: None,
            };

            // Create Series: pre_param -> funcall
            return Some(McPhrase::Series(vec![
                pre_label,
                McPhrase::FuncCall(outer_call),
            ]));
        }

        // === Iter 2: detect DECLARE child node ===
        let declare_node = subnode.iter().find(|n| n.is_type(MCAST_DECLARE));
        if let Some(ref decl) = declare_node {
            let declared = context.parse_declare(decl);
            if let Some(first_inst) = declared.into_iter().next() {
                caller = Some(Box::new(first_inst.into()));
            }
        }

        // === Handle pre-closure parameter from MCAST_INSTANCE (e.g., ldo.VIN) ===
        if let Some(first_child) = subnode.iter().next() {
            if first_child.get_type() == MCAST_INSTANCE {
                if let Some(inner) = first_child.get_sub_node() {
                    if let Some(phrase) = parse_phrase(&inner, context) {
                        caller = Some(Box::new(phrase));
                    }
                } else if let Some(phrase) = parse_phrase(&first_child, context) {
                    caller = Some(Box::new(phrase));
                }
            }
        }

        // Special handling for method calls after DECLARE
        if declare_node.is_some() {
            for each in subnode.iter() {
                if each.get_type() == MCAST_NAME {
                    if let Some(name_subnode) = each.get_sub_node() {
                        if name_subnode.get_type() == MCAST_OPD_DOT {
                            let mut dot_children = name_subnode.iter();
                            dot_children.next();
                            if let Some(method_name_node) = dot_children.next() {
                                if method_name_node.get_type() == MCAST_ID
                                    || method_name_node.get_type() == MCAST_IDA
                                {
                                    let node_copy = method_name_node.clone();
                                    func_name = McIds::new(&node_copy);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        for each in subnode.iter() {
            match each.get_type() {
                MCAST_NAME => {
                    if let Some(ids_node) = each.get_sub_node() {
                        func_name = McIds::new(&ids_node);
                    }
                }

                MCAST_PARAMS => {
                    if let Some(param_nodes) = each.get_sub_node() {
                        for param_node in param_nodes.iter() {
                            if let Some(value) = McParamValue::new(&param_node, context) {
                                params.push(value);
                            }
                        }
                    }
                    // Extract instance params when we see MCAST_PARAMS in MCAST_INSTANCE context
                    if instance_name.is_some() && instance_params.is_empty() {
                        if let Some(param_nodes) = each.get_sub_node() {
                            for param_node in param_nodes.iter() {
                                if let Some(value) = McParamValue::new(&param_node, context) {
                                    instance_params.push(value);
                                }
                            }
                        }
                    }
                }

                MCAST_OPD_FCALL => {
                    if let Some(chain_subnode) = each.get_sub_node() {
                        let chain_elements: Vec<_> = chain_subnode.iter().collect();

                        // First element is the caller (e.g., CAP(...) or ldo)
                        if !chain_elements.is_empty() {
                            let caller_node = &chain_elements[0];
                            let caller_type = caller_node.get_type();

                            // If caller is DOT, this is not a function call, skip it
                            if caller_type == MCAST_OPD_DOT {
                                return None;
                            }

                            if caller.is_none() {
                                // First try: check if it's an existing instance (e.g., ldo.enable)
                                let names = caller_node.to_id_or_ida();
                                if !names.is_empty() {
                                    let inst_name = names[0].to_string();
                                    if let Some(existing_inst) = context.find_inst(&inst_name) {
                                        caller = Some(Box::new(McPhrase::from(existing_inst)));
                                    }
                                }

                                // Second try: if not found, check if it's a class with params (e.g., CAP(...))
                                if caller.is_none() {
                                    if let Some(inner) = caller_node.get_sub_node() {
                                        let names = inner.to_id_or_ida();
                                        if !names.is_empty() {
                                            let class_name = &names[0];
                                            let ids = McIds::from(class_name.as_str());
                                            let class_name_str = class_name.to_string();

                                            let anon_name = context.gen_anon_name(&class_name_str);

                                            // Check for NC parameter using instance_params
                                            let is_nc = instance_params
                                                .iter()
                                                .any(|p| matches!(p, McParamValue::NC(_)));

                                            if let Some(McCMIE::Component(comp_def)) =
                                                mcb_get_cmie(&ids, context.uri())
                                            {
                                                let component = Mc2Component::with_params(
                                                    &anon_name,
                                                    comp_def,
                                                    instance_params.clone(),
                                                );
                                                if let Some(phrase) =
                                                    context.add_component(anon_name, component)
                                                {
                                                    caller = Some(Box::new(phrase));
                                                }
                                            } else if let Some(McCMIE::Module(mod_def)) =
                                                mcb_get_cmie(&ids, context.uri())
                                            {
                                                let module = Mc2Module::new(&anon_name, mod_def);
                                                if let Some(phrase) =
                                                    context.add_module(anon_name, module)
                                                {
                                                    caller = Some(Box::new(phrase));
                                                }
                                            } else if let Some(McCMIE::Interface(iface_def)) =
                                                mcb_get_cmie(&ids, context.uri())
                                            {
                                                let iface = Mc2Interface::new_with_str(
                                                    &anon_name, iface_def,
                                                );
                                                let inst = McInstance::Interface(Arc::new(iface));
                                                caller = Some(Box::new(McPhrase::from(inst)));
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Second element is the method name (e.g., .Cap)
                        if chain_elements.len() >= 2 {
                            let method_node = &chain_elements[1];
                            if let MCAST_NAME = method_node.get_type() {
                                if let Some(ids_node) = method_node.get_sub_node() {
                                    func_name = McIds::new(&ids_node);
                                }
                            }
                        }
                    }
                }

                MCAST_DECLARE => {}

                MCAST_INSTANCE => {
                    // ── [INST-DIAG] one-time: dump res/cap/dio inline instance node substructure ──
                    if let Some(sub0) = node.get_sub_node() {
                        let nm = sub0.to_string().unwrap_or_default();
                        if nm.contains("res") || nm.contains("cap") || nm.contains("dio") {
                            eprintln!("[INST-DIAG] Direct child nodes of MCAST_INSTANCE '{nm}':");
                            for ch in sub0.iter() {
                                let gk: Vec<u16> = ch
                                    .get_sub_node()
                                    .map(|g| g.iter().map(|x| x.get_type()).collect())
                                    .unwrap_or_default();
                                eprintln!(
                                    "[INST-DIAG]   type={} str='{}' grandchild types={:?}",
                                    ch.get_type(),
                                    ch.to_string().unwrap_or_default(),
                                    gk
                                );
                            }
                        }
                    }
                    // Handle MCAST_INSTANCE as caller for method calls like ldo.enable() or CAP(...).Cap(...)
                    if caller.is_none() {
                        if let Some(inner) = each.get_sub_node() {
                            let names = inner.to_id_or_ida();
                            if !names.is_empty() {
                                let inst_name = names[0].to_string();

                                // First try: check if it's an existing instance (e.g., ldo.enable)
                                if let Some(existing_inst) = context.find_inst(&inst_name) {
                                    caller = Some(Box::new(McPhrase::from(existing_inst)));
                                } else {
                                    // Second try: it's a class definition, create anonymous instance
                                    let ids = McIds::from(inst_name.as_str());
                                    let anon_name = context.gen_anon_name(&inst_name);

                                    // Check for NC parameter
                                    let is_nc = instance_params
                                        .iter()
                                        .any(|p| matches!(p, McParamValue::NC(_)));

                                    if let Some(McCMIE::Component(comp_def)) =
                                        mcb_get_cmie(&ids, context.uri())
                                    {
                                        let component = Mc2Component::with_params(
                                            &anon_name,
                                            comp_def,
                                            instance_params.clone(),
                                        );
                                        if let Some(phrase) =
                                            context.add_component(anon_name, component)
                                        {
                                            caller = Some(Box::new(phrase));
                                        }
                                    } else if let Some(McCMIE::Module(mod_def)) =
                                        mcb_get_cmie(&ids, context.uri())
                                    {
                                        let module = Mc2Module::new(&anon_name, mod_def);
                                        if let Some(phrase) = context.add_module(anon_name, module)
                                        {
                                            caller = Some(Box::new(phrase));
                                        }
                                    } else if let Some(McCMIE::Interface(iface_def)) =
                                        mcb_get_cmie(&ids, context.uri())
                                    {
                                        let iface =
                                            Mc2Interface::new_with_str(&anon_name, iface_def);
                                        let inst = McInstance::Interface(Arc::new(iface));
                                        caller = Some(Box::new(McPhrase::from(inst)));
                                    }
                                }
                            }
                        }
                    }
                }

                _ => {
                    if caller.is_none() {
                        if let Some(caller_phrase) = parse_phrase(&each, context) {
                            caller = Some(Box::new(caller_phrase));
                        } else if let Some(inner) = each.get_sub_node() {
                            let names = inner.to_id_or_ida();
                            if !names.is_empty() {
                                let class_name = &names[0];
                                let ids = McIds::from(class_name.as_str());

                                let has_method_name = subnode.iter().any(|n| {
                                    n.get_type() == MCAST_NAME
                                        && !n
                                            .get_sub_node()
                                            .map(|s| s.to_id_or_ida())
                                            .unwrap_or_default()[0]
                                            .contains('.')
                                });

                                if has_method_name {
                                    let class_name_str = class_name.to_string();
                                    let existing_inst = context.find_inst(&class_name_str);

                                    if existing_inst.is_some() {
                                        if let Some(inst) = existing_inst {
                                            caller = Some(Box::new(McPhrase::from(inst)));
                                        }
                                    } else {
                                        let anon_name = context.gen_anon_name(class_name);

                                        // Check for NC parameter
                                        let is_nc = instance_params
                                            .iter()
                                            .any(|p| matches!(p, McParamValue::NC(_)));

                                        if let Some(McCMIE::Component(comp_def)) =
                                            mcb_get_cmie(&ids, context.uri())
                                        {
                                            let component = if is_nc {
                                                Mc2Component::with_nc(&anon_name, comp_def, true)
                                            } else {
                                                Mc2Component::new(&anon_name, comp_def)
                                            };
                                            if let Some(phrase) =
                                                context.add_component(anon_name, component)
                                            {
                                                caller = Some(Box::new(phrase));
                                            }
                                        } else if let Some(McCMIE::Module(mod_def)) =
                                            mcb_get_cmie(&ids, context.uri())
                                        {
                                            let module = Mc2Module::new(&anon_name, mod_def);
                                            if let Some(phrase) =
                                                context.add_module(anon_name, module)
                                            {
                                                caller = Some(Box::new(phrase));
                                            }
                                        } else if let Some(phrase) = context.add_label(anon_name) {
                                            caller = Some(Box::new(phrase));
                                        }
                                    }
                                } else {
                                    let anon_name = context.gen_anon_name(class_name);

                                    // Check for NC parameter
                                    let is_nc = instance_params
                                        .iter()
                                        .any(|p| matches!(p, McParamValue::NC(_)));

                                    if let Some(McCMIE::Component(comp_def)) =
                                        mcb_get_cmie(&ids, context.uri())
                                    {
                                        let component = if is_nc {
                                            Mc2Component::with_nc(&anon_name, comp_def, true)
                                        } else {
                                            Mc2Component::new(&anon_name, comp_def)
                                        };
                                        if let Some(phrase) =
                                            context.add_component(anon_name, component)
                                        {
                                            caller = Some(Box::new(phrase));
                                        }
                                    } else if let Some(McCMIE::Module(mod_def)) =
                                        mcb_get_cmie(&ids, context.uri())
                                    {
                                        let module = Mc2Module::new(&anon_name, mod_def);
                                        if let Some(phrase) = context.add_module(anon_name, module)
                                        {
                                            caller = Some(Box::new(phrase));
                                        }
                                    } else if let Some(phrase) = context.add_label(anon_name) {
                                        caller = Some(Box::new(phrase));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let func_name: McIds = match func_name {
            Some(name) => name,
            None => {
                if declare_node.is_some() {
                    if let Some(name) = Self::extract_method_name(node) {
                        name
                    } else {
                        McIds::from("enable")
                    }
                } else if caller.is_some() {
                    let has_fcall = subnode.iter().any(|n| n.is_type(MCAST_OPD_FCALL));
                    if has_fcall {
                        for each in subnode.iter() {
                            if each.is_type(MCAST_OPD_FCALL) {
                                if let Some(chain_subnode) = each.get_sub_node() {
                                    for chain_child in chain_subnode.iter() {
                                        if chain_child.is_type(MCAST_NAME) {
                                            if let Some(ids_node) = chain_child.get_sub_node() {
                                                if let Some(mc_opd) = McOpd::new(&ids_node) {
                                                    if let McOpd::Id(name) = mc_opd {
                                                        let left = caller.as_ref().map_or_else(
                                                            || vec![McBus::new("undefined.in")],
                                                            |phrase| phrase.get_left(),
                                                        );
                                                        let right = caller.as_ref().map_or_else(
                                                            || vec![McBus::new("undefined.out")],
                                                            |phrase| phrase.get_right(),
                                                        );
                                                        // chain validity: previous link must return `this`
                                                        Self::check_chain_validity(
                                                            &caller, &name, node, context,
                                                        );
                                                        return Some(McPhrase::FuncCall(
                                                            McFuncCall {
                                                                caller,
                                                                func_name: name,
                                                                params,
                                                                left,
                                                                right,
                                                                dot_member: None,
                                                            },
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(ref caller_opd) = caller {
                        match caller_opd.as_ref() {
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Component(c),
                                members: _,
                            })) => McIds::from(c.name.to_string().as_str()),
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Module(m),
                                members: _,
                            })) => McIds::from(m.name.to_string().as_str()),
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Bus(ne),
                                members: _,
                            })) => McIds::from(ne.name.as_str()),
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Label(label),
                                members: _,
                            })) => McIds::from(label.as_str()),
                            McPhrase::Multiple(opds) if !opds.is_empty() => match &opds[0] {
                                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                    base: McInstance::Bus(ne),
                                    members: _,
                                })) => McIds::from(ne.name.as_str()),
                                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                    base: McInstance::Label(label),
                                    members: _,
                                })) => McIds::from(label.as_str()),
                                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                    base: McInstance::Component(c),
                                    members: _,
                                })) => McIds::from(c.name.to_string().as_str()),
                                McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                    base: McInstance::Module(m),
                                    members: _,
                                })) => McIds::from(m.name.to_string().as_str()),
                                _ => {
                                    if declare_node.is_some() {
                                        if let Some(name) = Self::extract_method_name(node) {
                                            name
                                        } else {
                                            McIds::from("enable")
                                        }
                                    } else {
                                        dlog_error(
                                            1301,
                                            node,
                                            "Missing function name in function call",
                                        );
                                        return None;
                                    }
                                }
                            },
                            McPhrase::Series(_) => {
                                dlog_error(
                                    1301,
                                    node,
                                    "Missing function name in function call (seq context)",
                                );
                                return None;
                            }
                            _ => {
                                if declare_node.is_some() {
                                    if let Some(name) = Self::extract_method_name(node) {
                                        name
                                    } else {
                                        McIds::from("enable")
                                    }
                                } else {
                                    dlog_error(
                                        1301,
                                        node,
                                        "Missing function name in function call",
                                    );
                                    return None;
                                }
                            }
                        }
                    } else {
                        dlog_error(1301, node, "Missing function name in function call");
                        return None;
                    }
                } else if let Some(ref caller_opd) = caller {
                    match caller_opd.as_ref() {
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Component(c),
                            members: _,
                        })) => McIds::from(c.name.to_string().as_str()),
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Module(m),
                            members: _,
                        })) => McIds::from(m.name.to_string().as_str()),
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Bus(ne),
                            members: _,
                        })) => McIds::from(ne.name.as_str()),
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                            base: McInstance::Label(label),
                            members: _,
                        })) => McIds::from(label.as_str()),
                        McPhrase::Multiple(opds) if !opds.is_empty() => match &opds[0] {
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Bus(ne),
                                members: _,
                            })) => McIds::from(ne.name.as_str()),
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Label(label),
                                members: _,
                            })) => McIds::from(label.as_str()),
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Component(c),
                                members: _,
                            })) => McIds::from(c.name.to_string().as_str()),
                            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                                base: McInstance::Module(m),
                                members: _,
                            })) => McIds::from(m.name.to_string().as_str()),
                            _ => {
                                dlog_error(1301, node, "Missing function name in function call");
                                return None;
                            }
                        },
                        McPhrase::Series(_) => {
                            dlog_error(
                                1301,
                                node,
                                "Missing function name in function call (seq context)",
                            );
                            return None;
                        }
                        _ => {
                            dlog_error(1301, node, "Missing function name in function call");
                            return None;
                        }
                    }
                } else {
                    dlog_error(1301, node, "Missing function name in function call");
                    return None;
                }
            }
        };

        // === Iter 2: handle dot-notation func_name ===
        let func_name = if func_name.to_string().contains('.') {
            let name_str = func_name.to_string();
            if let Some(dot_pos) = name_str.find('.') {
                let inst_part = &name_str[..dot_pos];
                let method_part = &name_str[dot_pos + 1..];
                if caller.is_none() {
                    if let Some(ident) = context.find_inst(inst_part) {
                        caller = Some(Box::new(ident.into()));
                        McIds::from(method_part)
                    } else {
                        func_name
                    }
                } else {
                    McIds::from(method_part)
                }
            } else {
                func_name
            }
        } else {
            func_name
        };

        // Determine input interface (left side)
        let left = if let Some(ref caller_opd) = caller {
            caller_opd.as_ref().get_left()
        } else {
            vec![McBus::new(&format!("{func_name}.in"))]
        };

        // Determine output interface (right side)
        // The output interface of a function call is inherited from the caller's right (output) interface,
        // because a function call is a transformation of the caller, and its output usually preserves the caller's output shape.
        let right = if let Some(ref caller_opd) = caller {
            caller_opd.as_ref().get_right()
        } else {
            vec![McBus::new(&format!("{func_name}.out"))]
        };

        // Check if func_name is a Component or Module definition (function call form instantiation)
        // e.g., CAP(10uF, ...).Cap(...) - creates anonymous instance of CAP
        if caller.is_none() {
            let cmie_result = mcb_get_cmie(&func_name, context.uri()).is_some();
            // eprintln!("[FC-PARSE-BARE] func_name='{}' cmie_found={}", func_name, cmie_result);
            if let Some(cmie) = mcb_get_cmie(&func_name, context.uri()) {
                match cmie {
                    McCMIE::Component(comp_def) => {
                        let inst_name = context.gen_anon_name(&func_name.to_string());
                        // ── Iter-3.E fix ────────────────────────────────────
                        // When context is McComponent, gen_anon_name returns "",
                        // and add_component is also an empty implementation. If we wrap a component
                        // with name "" into Endpoint as-is, pass2 processing would produce
                        // ghost pins (empty owner) like `.1 : X ~ .1`.
                        //
                        // Correct approach: when inst_name is empty, **do not** take the Endpoint branch;
                        // fall through to the FuncCall construction below, letting pass2's auto_name
                        // in `instantiate_component_construction` generate the actual @RES1/@CAP1 names.
                        if !inst_name.is_empty() {
                            // Check for NC parameter
                            let is_nc = params.iter().any(|p| matches!(p, McParamValue::NC(_)));
                            // ── Iter-7.4 (with diag) ───────────────────────
                            let mc2_comp = if is_nc {
                                Mc2Component::with_nc(&inst_name, comp_def.clone(), true)
                            } else {
                                Mc2Component::with_params(
                                    &inst_name,
                                    comp_def.clone(),
                                    params.clone(),
                                )
                            };
                            context.add_component(inst_name.clone(), mc2_comp.clone());
                            return Some(McPhrase::Endpoint(McEndpoint::Single(
                                McInstanceRef::new(McInstance::Component(Arc::new(mc2_comp))),
                            )));
                        }
                        // else: fall through to FuncCall construction below
                    }
                    McCMIE::Module(mod_def) => {
                        let inst_name = context.gen_anon_name(&func_name.to_string());
                        // Same as Iter-3.E: only take the Endpoint branch when inst_name is non-empty
                        if !inst_name.is_empty() {
                            let mc2_mod = Mc2Module::new(&inst_name, mod_def.clone());
                            context.add_module(inst_name.clone(), mc2_mod);
                            return Some(McPhrase::Endpoint(McEndpoint::Single(
                                McInstanceRef::new(McInstance::Module(Arc::new(Mc2Module::new(
                                    &inst_name, mod_def,
                                )))),
                            )));
                        }
                        // else: fall through to FuncCall construction below
                    }
                    _ => {}
                }
            }
        }

        // eprintln!("[FC-PARSE] returning FuncCall: func_name='{}' caller_is_some={}",
        //       func_name, caller.is_some());

        // ── chain validity ────────────────────────────────────────────────
        // If the caller is itself a FuncCall, the previous link in the chain
        // must return `this` (or be Implicit). A function returning a bus /
        // label is an *endpoint* and cannot be chained off of.
        Self::check_chain_validity(&caller, &func_name, node, context);

        Some(McPhrase::FuncCall(McFuncCall {
            caller,
            func_name,
            params,
            left,
            right,
            dot_member: None,
        }))
    }

    /// Validate that the caller (if it's an inner [`McFuncCall`]) returns
    /// something chainable.
    ///
    /// Resolution strategy:
    ///   * Caller is not an inner FuncCall   → nothing to check.
    ///   * Inner has a receiver (`obj.f()`)  → walk the receiver chain down
    ///     to its root instance, then look up `inner.func_name` in that
    ///     class's `funcs` table.
    ///   * Inner has no receiver (bare `f()`) → fall back to the current
    ///     scope via `context.find_func_return`.
    ///   * No record found anywhere → silently skip (built-in, unknown
    ///     external function, etc. — we can't authoritatively say).
    ///   * Found `Endpoint(_)` return → emit error 1316.
    fn check_chain_validity(
        caller: &Option<Box<McPhrase>>,
        outer_method: &McIds,
        node: &AstNode,
        context: &mut dyn HasFindInst,
    ) {
        let Some(caller_box) = caller else { return };
        let McPhrase::FuncCall(inner_fc) = caller_box.as_ref() else {
            return;
        };

        let inner_name = inner_fc.func_name.to_string();

        // Walk the receiver chain to a concrete instance (Module/Component/...).
        let root = inner_fc
            .caller
            .as_ref()
            .and_then(|c| Self::root_receiver(c.as_ref()));

        let ret: Option<McFuncReturn> = match root {
            Some(McInstance::Module(arc_mod)) => arc_mod
                .base
                .funcs
                .find(&inner_name)
                .map(|f| f.returns.clone()),
            Some(McInstance::Component(arc_comp)) => arc_comp
                .base
                .funcs
                .find(&inner_name)
                .map(|f| f.returns.clone()),
            Some(_) => {
                // Receiver is Bus / Label / List / Interface — has no `funcs`
                // table to query. Cannot validate; skip.
                return;
            }
            None => {
                // Either the bare-call case (no receiver), or we couldn't
                // resolve a concrete root. Try the surrounding scope.
                context.find_func_return(&inner_name)
            }
        };

        let Some(ret) = ret else { return };
        if ret.is_chainable() {
            return;
        }

        debug_assert!(matches!(ret, McFuncReturn::Endpoint(_)));
        dlog_error(
            1316,
            node,
            &format!(
                "Cannot chain `.{outer_method}` after `{inner_name}(...)`: function `{inner_name}` returns a \
                 bus/label (endpoint), not `this`. Only functions that return \
                 `this` can be chained.",
            ),
        );
    }

    /// Walk a phrase down through chained `FuncCall`s to find the root
    /// receiver instance (the first non-FuncCall caller).
    ///
    /// For `mcu513.setup().capIt().i2c()`, calling this on the outer i2c's
    /// inner-FuncCall caller (i.e. capIt's FuncCall phrase) will recurse:
    /// capIt → setup → mcu513 endpoint, returning the `mcu513` instance.
    fn root_receiver(phrase: &McPhrase) -> Option<&McInstance> {
        match phrase {
            McPhrase::FuncCall(fc) => fc
                .caller
                .as_ref()
                .and_then(|c| Self::root_receiver(c.as_ref())),
            McPhrase::Endpoint(McEndpoint::Single(iref)) => Some(&iref.base),
            _ => None,
        }
    }

    /// Helper function to recursively extract method name from AST nodes
    fn extract_method_name(node: &AstNode) -> Option<McIds> {
        let node_type = node.get_type();

        if node_type == MCAST_OPD_FCALL {
            for child in node.iter() {
                if child.get_type() == MCAST_OPD_DOT {
                    for dot_child in child.iter() {
                        if dot_child.get_type() == MCAST_NAME {
                            let node_copy = dot_child.clone();
                            return McIds::new(&node_copy);
                        }
                    }
                }
            }
        }

        if node_type == MCAST_NAME {
            let node_copy = node.clone();
            return McIds::new(&node_copy);
        }

        if node_type == MCAST_OPD_DOT || node_type == MCAST_DECLARE {
            for child in node.iter() {
                if let Some(name) = Self::extract_method_name(&child) {
                    return Some(name);
                }
            }
        }

        if let Some(subnode) = node.get_sub_node() {
            return Self::extract_method_name(&subnode);
        }

        None
    }
}
