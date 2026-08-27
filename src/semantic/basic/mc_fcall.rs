// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_bus::McBus;
use super::mc_endpoint::{McEndpoint, McInstanceRef};
use super::mc_ids::McIds;
use super::mc_opd::McOpd;
use super::mc_param::{McParamBindings, McParamValue, ParamBindError};
use super::mc_phrase::McPhrase;
use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::db::context::DB;
use crate::db::diagnostic::diagnostic::{dlog_error, dlog_warning};
use crate::query::refs::mcb_register_declare_class;
use crate::semantic::common::ConnDir;
use crate::semantic::common::McCMIE;
use crate::semantic::component::Mc2Component;
use crate::semantic::context::resolve_cmie;
use crate::semantic::mc_func::{HasFindInst, McFuncReturn};
use crate::semantic::mc_ifs::Mc2Interface;
use crate::semantic::mc_inst::McInstance;
use crate::semantic::module::Mc2Module;
use std::sync::Arc;

/// Function call
#[derive(Debug, Clone)]
pub struct McFuncCall {
    /// Stable ID for auto_inst_map (replaces pointer-based key).
    /// Assigned during instantiation; 0 = unassigned. Clone-safe since Copy.
    pub id: u32,
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
    /// ★ P4.1: Resolved return shape after Pass1b.
    /// - `ReturnShape::This` → caller's left/right preserved (for `return this` / implicit)
    /// - `ReturnShape::Label` → left empty, right = return value (for `return <expr>`)
    pub resolved_return_shape: Option<ReturnShape>,
    /// Set when this call is the expansion target of a `=>` pre-closure
    /// (`vin => CAP(10uF).Cap(_)`): the series element before this call is
    /// the pre-closure input and the display joins them with `=>`.
    pub pre_closure: bool,
    /// Set when this call came from a `name::CLASS(params)` declareb
    /// declaration (e.g. `C4::CAP()`, `dio[1:2]::DIO.ESD(...)`). The caller
    /// is the declared instance name and display joins them with `::`
    /// (instead of the `.` used for `CLASS(x).Method(y)` calls).
    pub named_ctor: bool,
}

/// ★ P4.1: Fcall return shape resolved from McFunction.returns.
#[derive(Debug, Clone)]
pub enum ReturnShape {
    /// `return this` or implicit → caller shape preserved. The shape is read
    /// live from `McFuncCall.left`/`right` at use time, so substitutions and
    /// prefixing done during instantiation stay reflected.
    This,
    /// `return <label/bus/expr>` → left empty, right = return value as bus vector
    Label { bus: Vec<McBus> },
}

/// ★ P4.1: Extract right-side buses from a McPhrase (for Endpoint return shape).
/// Uses phrase's own `right` representation; for labels/buses, returns a descriptive bus.
/// `pub(crate)`: instantiate_instance_method reuses it to encode the substituted
/// return bus into `@@RETURN_NETS:` (func-return-design §7).
pub(crate) fn get_right_bus_from_phrase(phrase: &McPhrase) -> Vec<McBus> {
    match phrase {
        McPhrase::Endpoint(ep) => {
            // For endpoint return: derive bus from the endpoint type
            match ep {
                McEndpoint::Single(ir) => {
                    // A multi-member bus return (`return XTAL{X1, X2}`) is a
                    // vector of N lanes — one per member (Pass2 emits one
                    // `@@RETURN_EP` per member pin). Expand to per-member buses
                    // so the Pass1 `->` opcheck counts N rows, not the 1*1
                    // shorthand of the collapsed bus name.
                    let bus = ir.to_bus();
                    if bus.member.len() >= 2 {
                        return bus
                            .member
                            .iter()
                            .map(|m| McBus::new(&format!("{}.{}", bus.name, m)))
                            .collect();
                    }
                    let name = ir.to_string();
                    vec![McBus::new(&name)]
                }
                McEndpoint::List(eps) => eps
                    .iter()
                    .flat_map(|ep| get_right_bus_from_phrase(&McPhrase::Endpoint(ep.clone())))
                    .collect(),
                McEndpoint::Node { input: _, output } => output
                    .iter()
                    .flat_map(|ep| get_right_bus_from_phrase(&McPhrase::Endpoint(ep.clone())))
                    .collect(),
            }
        }
        McPhrase::Lead => vec![McBus::new("lead")],
        McPhrase::Multiple(phrases) => phrases
            .iter()
            .flat_map(|p| get_right_bus_from_phrase(p))
            .collect(),
        other => {
            // For other phrase types, derive bus from the display name
            let name = format!("{}", other);
            vec![McBus::new(&name)]
        }
    }
}

/// Build the chain-head phrase for a `=>` parameter prefix (§1.2).
///
/// Converts the prefix `McParamValue` into the chain's left port
/// **structurally** — never by stringifying and re-parsing. The original ids
/// structure must be preserved:
///   - a `Set` prefix `[a, b]` stays a `Multiple` of its members (the chain's
///     left port resolves to the vector's member lanes);
///   - a `Phrase` value stays the phrase itself (a DC parameter reference
///     `[V3V3, GND]` arriving as a Phrase must not collapse into a single
///     label named `[V3V3, GND]`, which instance-prefixing turns into
///     `flash.[V3V3, GND]`);
///   - an `Ids` / `Opd` becomes a label of the ids (a bare id / dot-member /
///     DC-bus name expands through the existing port/bus resolution);
///   - only scalar literals (Const/Int/...) fall back to a value label.
fn pre_param_to_label(v: &McParamValue) -> McPhrase {
    match v {
        McParamValue::Set(values) => {
            McPhrase::Multiple(values.iter().map(pre_param_to_label).collect())
        }
        McParamValue::Phrase(p) => (**p).clone(),
        McParamValue::Ids(ids) => McPhrase::label(ids.to_string()),
        McParamValue::Opd(opd) => match opd {
            McOpd::Id(ids) | McOpd::This(ids) | McOpd::Pins(ids) => {
                McPhrase::label(ids.to_string())
            }
            McOpd::Uscore => McPhrase::label("_".to_string()),
        },
        other => McPhrase::label(other.to_string()),
    }
}

/// Does a param value contain a `_` placeholder, either bare or inside a
/// Set (`[_, VDD]`)? Used to decide whether the `=>` prefix can fold into the
/// placeholder position (§1: prefix fills the leading `_`).
fn param_contains_uscore(p: &McParamValue) -> bool {
    match p {
        McParamValue::NONE(_) | McParamValue::Opd(McOpd::Uscore) => true,
        McParamValue::Set(vs) => vs.iter().any(param_contains_uscore),
        _ => false,
    }
}

/// Replace the leading `_` placeholder in `p` with `prefix`, recursing into
/// Sets so `[_, VDD]` + `I2C0` → `[I2C0, VDD]`. Returns (new_value, replaced).
/// Only the FIRST placeholder in the parameter list is replaced — a later
/// `_` (e.g. `.Pullup(VDD, _)`) keeps its open-slot meaning.
fn fold_prefix_into_uscore(p: &McParamValue, prefix: &McParamValue) -> (McParamValue, bool) {
    match p {
        McParamValue::NONE(_) => (prefix.clone(), true),
        McParamValue::Opd(McOpd::Uscore) => (prefix.clone(), true),
        McParamValue::Set(vs) => {
            let mut new_vs = Vec::with_capacity(vs.len());
            let mut done = false;
            for v in vs {
                if done {
                    new_vs.push(v.clone());
                } else {
                    let (nv, rep) = fold_prefix_into_uscore(v, prefix);
                    new_vs.push(nv);
                    done = rep;
                }
            }
            (McParamValue::Set(new_vs), done)
        }
        _ => (p.clone(), false),
    }
}

/// Report E4176 when an inline component construction argument list does not
/// match the class signature (chain-path counterpart of the declare-path
/// check in mc_inst.rs).
///
/// Mirrors `McParamBindings::bind` semantics: NC occupies no slot and never
/// covers a missing required parameter; unknown named / excess /
/// type-mismatched arguments are hard errors (e.g. a package value `C0402` /
/// `PKG.C0402` passed to a class that declares no enum-class parameter for
/// it). A missing required parameter is silent — the value comes from spec /
/// the BOM and the instance is created with the supplied arguments. The
/// instance is always created with the raw arguments so downstream wiring
/// stays intact; the diagnostic points the author at the offending argument
/// list.
pub(crate) fn check_ctor_bind(
    inst_name: &str,
    comp_def: &crate::semantic::component::McComponent,
    params: &[McParamValue],
    node: &AstNode,
) {
    if params.is_empty() {
        return;
    }
    if let Err(e) = McParamBindings::bind(comp_def.bind_params(), params) {
        // Component-Spec Separation: a missing required parameter never
        // blocks instance creation — circuit topology only needs pins, and
        // the parameter value is supplied later via spec or the BOM. It is
        // silent in dev mode and reported as a warning (E4178) in strict
        // mode. Written-but-wrong arguments (excess / unknown /
        // type-mismatched) are hard errors (E4176).
        if let ParamBindError::MissingRequired { name } = e {
            if crate::cli::strict_mode() {
                dlog_warning(
                    crate::errcodes::INST_PARAM_MISSING_REQUIRED,
                    node,
                    &crate::errcodes::format_msg(
                        crate::errcodes::INST_PARAM_MISSING_REQUIRED,
                        &[&inst_name, &comp_def.name.to_string(), &name],
                    ),
                );
            }
        } else {
            dlog_error(
                crate::errcodes::INST_PARAM_BIND_FAILED,
                node,
                &crate::errcodes::format_msg(
                    crate::errcodes::INST_PARAM_BIND_FAILED,
                    &[&inst_name, &comp_def.name.to_string(), &format!("{e}")],
                ),
            );
        }
    }
}

impl McFuncCall {
    /// Parse function call from AST node
    pub fn parse(node: &AstNode, context: &mut dyn HasFindInst) -> Option<McPhrase> {
        // ★ Register class_ref for F12 goto-def on inline constructors.
        // opd_fcall AST forms:
        //   CAP(10uF)          → { name: "CAP" }                              — class, no instance
        //   CAP.CER(10uF)      → { instance: "CAP", name: "CER" }             — dotted class
        //   uC.i2c(0x36)       → { instance: "uC", name: "i2c" }              — method call
        //   mic(V3V3)          → { name: "mic" }                              — instance constructor (has no instance child, but name IS a known instance)
        //   RES(10kΩ).Pullup() → { instance: opd_fcall, name: "Pullup" }      — chained method call
        // Distinction: if `instance` segment is a known instance (find_inst),
        // it's a method call → skip. Otherwise it's a class → register.
        if let Some(subnodes) = node.get_sub_node() {
            let mut has_instance = false;
            let mut inst_name: Option<McIds> = None;
            let mut inst_span: Option<std::ops::Range<usize>> = None;
            let mut class_name: Option<(McIds, std::ops::Range<usize>)> = None;
            for child in subnodes.iter() {
                match child.get_type() {
                    MCAST_INSTANCE => {
                        has_instance = true;
                        inst_span = Some(
                            (child.get_pos() as usize)
                                ..((child.get_pos() + child.get_len()) as usize),
                        );
                        if let Some(inner) = child.get_sub_node() {
                            if inner.get_type() == MCAST_OPD {
                                if let Some(opd_sub) = inner.get_sub_node() {
                                    if let Some(ids) = McIds::new(&opd_sub) {
                                        inst_name = Some(ids);
                                    }
                                }
                            }
                        }
                    }
                    MCAST_NAME => {
                        if let Some(ids_node) = child.get_sub_node() {
                            if let Some(ids) = McIds::new(&ids_node) {
                                let span = (ids_node.get_pos() as usize)
                                    ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                                class_name = Some((ids, span));
                            }
                        }
                    }
                    _ => {}
                }
            }
            // Method call: has instance AND either (a) instance is known name
            // in scope, or (b) instance is nested (e.g. RES(10kΩ).Pullup)
            let is_method_call = has_instance
                && (inst_name
                    .as_ref()
                    .is_some_and(|n| context.find_inst(&n.to_string()).is_some())
                    || inst_name.is_none());
            // Instance constructor: no instance child but name IS known instance
            // (e.g. mic(V3V3) — name="mic" is a declared instance)
            let is_instance_constructor = !has_instance
                && class_name
                    .as_ref()
                    .is_some_and(|(n, _)| context.find_inst(&n.to_string()).is_some());
            if !is_method_call && !is_instance_constructor {
                if let Some((name, span)) = class_name {
                    // The dotted class name spans two AST children (instance +
                    // name); rebuild the canonical single-Ida form for lookup.
                    let full_name_ids = match &inst_name {
                        Some(inst) => McIds::from(&format!("{inst}.{name}")),
                        None => name,
                    };
                    // ★ Dotted class (`DIO.ESD`): the name child only spans
                    // `ESD`, but mcb_register_declare_class rebuilds the ref
                    // span from the flattened name length (raw.start +
                    // name.len()). Passing the `ESD` span yields
                    // `ESD("ES` (bleeding into the string arg). Pass the
                    // whole `DIO.ESD` text span instead.
                    let full_span = match (&inst_name, &inst_span) {
                        (Some(_), Some(is)) if is.start <= span.start => is.start..span.end,
                        _ => span.clone(),
                    };
                    mcb_register_declare_class(context.uri(), &full_name_ids, full_span);
                }
            } else if is_method_call && inst_name.is_none() {
                // ★ Chained call: RES(100kΩ).Pullup() — register inner class name
                for child in subnodes.iter() {
                    if child.get_type() == MCAST_INSTANCE {
                        if let Some(inner) = child.get_sub_node() {
                            let fcall_node = if inner.get_type() == MCAST_OPD {
                                inner.get_sub_node()
                            } else {
                                Some(inner)
                            };
                            if let Some(fc) = fcall_node {
                                // ★ Fix: fc.iter() walks the `next` sibling list, but MCAST_NAME
                                // is a child of the fcall node (via `sub`). We need to iterate
                                // the children of the fcall, not its siblings.
                                if let Some(fc_sub) = fc.get_sub_node() {
                                    for fc_child in fc_sub.iter() {
                                        if fc_child.get_type() == MCAST_NAME {
                                            if let Some(ids_node) = fc_child.get_sub_node() {
                                                if let Some(ids) = McIds::new(&ids_node) {
                                                    let span = (ids_node.get_pos() as usize)
                                                        ..((ids_node.get_pos() + ids_node.get_len())
                                                            as usize);
                                                    mcb_register_declare_class(
                                                        context.uri(),
                                                        &ids,
                                                        span,
                                                    );
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

        // ── P2-12-DEBUG: print children types ──
        {
            let children: Vec<(u16, String)> = subnode
                .iter()
                .map(|c| (c.get_type(), c.to_string().unwrap_or_default()))
                .collect();
            let func_name_from_ast = subnode
                .iter()
                .find(|c| c.get_type() == MCAST_NAME)
                .and_then(|c| c.get_sub_node())
                .and_then(|n| McIds::new(&n))
                .map(|ids| ids.to_string())
                .unwrap_or_default();
            if func_name_from_ast.contains("Cap")
                || func_name_from_ast.contains("Pullup")
                || func_name_from_ast.contains("Pulldown")
            {
                mcc_dbg!(
                    "sem::fcall",
                    "[FCALL-PARSE-DBG] func={func_name_from_ast} children={children:?}",
                );
            }
        }

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

            // Create pre_param as Label (for Series display: pre_param -> ...).
            // A Set prefix `[V3V3, GND]` becomes a Multiple of member labels so
            // the chain's left port expands to the vector lanes (§1.2).
            let pre_label = pre_param_to_label(&pre_param);

            // ── R3: `=>` fold unified rules (§1) ──────────────────────────────
            // One rule, no method-name list: the `=>` prefix is an actual that
            // fills the leading `_` placeholder position of the right-hand
            // method call. All-placeholder → the prefix is the whole actual;
            // a leading `_` (bare or inside a Set) → replace it in place; no
            // placeholder → prepend (legacy keep).
            let is_uscore = |p: &McParamValue| {
                matches!(p, McParamValue::NONE(_)) || matches!(p, McParamValue::Opd(McOpd::Uscore))
            };
            let all_ph = !method_params.is_empty() && method_params.iter().all(&is_uscore);

            let all_method_params: Vec<McParamValue> = if all_ph {
                // (a) `.Cap(_)` + `=>` prefix → fold the prefix into the
                // placeholder position (parameter prefixing, §1.2).
                // `[V3V3, GND] => CAP(..).Cap(_)` → `.Cap([V3V3, GND])`,
                // `V3V3 => CAP(..).Cap(_)` → `.Cap(V3V3)`,
                // `vin -> ldo.VIN => CAP(..).Cap(_)` → `.Cap(ldo.VIN)`
                // (pre_param is already the last / right endpoint of the
                // prefix chain, per the vector circuit algebra). The single
                // vector fills both endpoint positions positionally at wiring
                // time (member[0] → pin1, member[1] → pin2, §11.6); a scalar
                // prefix folds to `.Cap(SIG)` which is E4176 (strict arity).
                vec![pre_param.clone()]
            } else if method_params.iter().any(&param_contains_uscore) {
                // (b) `(_ , VDD)` / `([_, VDD])` / `(x, _)` on any method:
                // The `=>` prefix fills the LEADING `_` placeholder (§1). A bare
                // `_` is replaced outright; a `_` inside a Set is replaced in
                // place so the folded call keeps ONE Set actual `[I2C0, VDD]`
                // that binds whole to the Set formal (strict arity — actuals
                // bind to formals, no auto-split). P2-5 then substitutes each
                // bus lane into the folded Set for the per-lane calls.
                let mut folded: Vec<McParamValue> = Vec::with_capacity(method_params.len());
                let mut done = false;
                for p in &method_params {
                    if done {
                        folded.push(p.clone());
                    } else {
                        let (np, rep) = fold_prefix_into_uscore(p, &pre_param);
                        folded.push(np);
                        done = rep;
                    }
                }
                folded
            } else {
                // (c) No placeholder: keep original prepend
                let mut v = vec![pre_param.clone()];
                v.extend(method_params);
                v
            };

            // R4: don't instantiate at parse time. Built-in two-pin components handled by instantiation-phase process_member_internal
            // unified creation —— same P1-D branch-1 path as `->` form, consistent naming/wiring.

            // Create inner FuncCall: ClassName(instance_params)
            let inner_call = McFuncCall {
                id: 0,
                caller: None,
                func_name: instance_name.unwrap(),
                params: instance_params,
                left: vec![],
                right: vec![],
                dot_member: None,
                resolved_return_shape: None,
                pre_closure: false,
                named_ctor: false,
            };

            // Create outer FuncCall: ClassName(params).MethodName(all_method_params)
            let outer_call = McFuncCall {
                id: 0,
                caller: Some(Box::new(McPhrase::FuncCall(inner_call))),
                func_name: method_name_opt.unwrap(),
                params: all_method_params,
                left: vec![],
                right: vec![],
                dot_member: None,
                resolved_return_shape: None,
                pre_closure: true,
                named_ctor: false,
            };

            // Create Series: pre_param -> funcall
            return Some(McPhrase::Series(
                vec![pre_label, McPhrase::FuncCall(outer_call)],
                ConnDir::Undirected,
            ));
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
                    let inner_type = inner.get_type();
                    mcc_dbg!(
                        "sem::fcall",
                        "[FCALL-CALLER-DBG] MCAST_INSTANCE inner_type={inner_type}"
                    );
                    // ── P2-12: Handle MCAST_DECLARE as FuncCall caller ──
                    // When CAP(10uF,10V) is wrapped in MCAST_INSTANCE as MCAST_DECLARE
                    // (e.g. CAP(10uF,10V).Cap(...)), parse_phrase creates a component
                    // at parse time and returns Multiple (due to auto-instance names).
                    // Instead, build a FuncCall directly to preserve the parameters.
                    if inner_type == MCAST_DECLARE {
                        mcc_dbg!(
                            "sem::fcall",
                            "[FCALL-CALLER-DBG] MCAST_DECLARE handler entered"
                        );
                        if let Some(sub) = inner.get_sub_node() {
                            let mut class_node: Option<AstNode> = None;
                            for c in sub.iter() {
                                if c.get_type() == MCAST_CLASS && class_node.is_none() {
                                    class_node = Some(c.clone());
                                }
                            }
                            mcc_dbg!(
                                "sem::fcall",
                                "[FCALL-CALLER-DBG] class_node={}",
                                class_node.is_some()
                            );
                            if let Some(cls) = class_node {
                                if let Some(class_ids) =
                                    cls.get_sub_node().and_then(|cid| McIds::new(&cid))
                                {
                                    let fname = class_ids.to_string();
                                    let is_twopin =
                                        crate::vector::graph::naming::is_known_twopin_class(&fname);
                                    mcc_dbg!(
                                        "sem::fcall",
                                        "[FCALL-CALLER-DBG] fname={fname} is_twopin={is_twopin}"
                                    );
                                    if is_twopin {
                                        let mut params: Vec<McParamValue> = Vec::new();
                                        let mut cur = cls.get_sub_node();
                                        while let Some(n) = cur {
                                            if n.get_type() == MCAST_PARAMS {
                                                if let Some(ps) = n.get_sub_node() {
                                                    for p in ps.iter() {
                                                        if let Some(v) =
                                                            McParamValue::new(&p, context)
                                                        {
                                                            params.push(v);
                                                        }
                                                    }
                                                }
                                                break;
                                            }
                                            cur = n.get_next();
                                        }
                                        caller = Some(Box::new(McPhrase::FuncCall(McFuncCall {
                                            id: 0,
                                            caller: None,
                                            func_name: class_ids,
                                            params,
                                            left: vec![],
                                            right: vec![],
                                            dot_member: None,
                                            resolved_return_shape: None,
                                            pre_closure: false,
                                            named_ctor: false,
                                        })));
                                    }
                                }
                            }
                        }
                    }
                    if caller.is_none() {
                        // For MCAST_OPD_FCALL inner two-pin classes, try direct FuncCall first
                        if inner_type == MCAST_OPD_FCALL {
                            caller = Self::try_parse_inner_fcall(&inner, context);
                        }
                        // ── P2-7: bare class name as caller ──
                        // `DIO.ESD(...)` parses the caller operand `DIO` as a plain id.
                        // It is a component class (dio.mc defines `component DIO`), not
                        // an instance, so `parse_phrase`'s plain-id fallback would
                        // register it via add_label, creating a bogus `DIO : ilabel`
                        // instance-table entry. When the name resolves to a
                        // component/module class and is not an existing instance, keep
                        // it as a non-registered Label phrase; instantiation decides
                        // via the dotted-name rule whether the label is part of the
                        // class name (e.g. `DIO.ESD`) or a user-specified instance name.
                        if caller.is_none() && inner_type == MCAST_OPD {
                            // A nested FuncCall caller (MCAST_OPD wrapping
                            // MCAST_OPD_FCALL, e.g. `CAP(x).Cap(_)`) must keep the
                            // parse_phrase path below — do not treat it as a class name.
                            let is_nested_fcall = inner
                                .get_sub_node()
                                .map(|sn| sn.get_type() == MCAST_OPD_FCALL)
                                .unwrap_or(false);
                            if !is_nested_fcall {
                                let names = inner.to_id_or_ida();
                                if names.len() == 1 {
                                    let inst_name = &names[0];
                                    if context.find_inst(inst_name).is_none() {
                                        let ids = McIds::from(inst_name.as_str());
                                        if let Some(cmie) = resolve_cmie(&DB, &ids, context.uri()) {
                                            match cmie {
                                                McCMIE::Component(_) | McCMIE::Module(_) => {
                                                    caller = Some(Box::new(McPhrase::label(
                                                        inst_name.clone(),
                                                    )));
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if caller.is_none() {
                            if let Some(phrase) = parse_phrase(&inner, context) {
                                let phrase_desc = match &phrase {
                                    McPhrase::FuncCall(fc) => format!("FuncCall({})", fc.func_name),
                                    McPhrase::Endpoint(_) => "Endpoint".into(),
                                    _ => format!("{:?}", std::mem::discriminant(&phrase)),
                                };
                                mcc_dbg!("sem::fcall", "[FCALL-CALLER-DBG] parse_phrase inner_type={inner_type} returned: {phrase_desc}");
                                caller = Some(Box::new(phrase));
                            }
                        }
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
                                            // Store the source span so diagnostics on this
                                            // anonymous instance point at its usage position.
                                            let inst_span = (caller_node.get_pos() as usize)
                                                ..((caller_node.get_pos() + caller_node.get_len())
                                                    as usize);
                                            context.store_inst_span(&anon_name, inst_span);

                                            if let Some(McCMIE::Component(comp_def)) =
                                                resolve_cmie(&DB, &ids, context.uri())
                                            {
                                                check_ctor_bind(
                                                    &anon_name,
                                                    &comp_def,
                                                    &instance_params,
                                                    caller_node,
                                                );
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
                                                resolve_cmie(&DB, &ids, context.uri())
                                            {
                                                let module = Mc2Module::new(&anon_name, mod_def);
                                                if let Some(phrase) =
                                                    context.add_module(anon_name, module)
                                                {
                                                    caller = Some(Box::new(phrase));
                                                }
                                            } else if let Some(McCMIE::Interface(iface_def)) =
                                                resolve_cmie(&DB, &ids, context.uri())
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
                    // Handle MCAST_INSTANCE as caller for method calls like ldo.enable() or CAP(...).Cap(...)
                    mcc_dbg!(
                        "sem::fcall",
                        "[FCALL-INST-DBG] MCAST_INSTANCE reached, caller.is_none()={}",
                        caller.is_none()
                    );
                    if caller.is_none() {
                        mcc_dbg!(
                            "sem::fcall",
                            "[FCALL-INST-DBG] each.get_sub_node()={:?}",
                            each.get_sub_node().map(|n| (n.get_type(), n.to_string()))
                        );
                        if let Some(inner) = each.get_sub_node() {
                            let inner_type = inner.get_type();
                            mcc_dbg!(
                                "sem::fcall",
                                "[FCALL-INST-DBG] inner_type={inner_type} MCAST_OPD_FCALL={}",
                                crate::ast::c_macros::MCAST_OPD_FCALL
                            );
                            // ── P2-12: Handle nested FuncCall inside MCAST_INSTANCE ──
                            // When a FuncCall like CAP(10uF,10V) is wrapped in MCAST_INSTANCE
                            // (e.g. as the instance of CAP(10uF,10V).Cap(...)),
                            // parse it as a FuncCall to preserve the parameters.
                            // For two-pin classes (CAP/RES/IND/...), build FuncCall directly
                            // to avoid premature anonymous component instantiation.
                            if inner_type == crate::ast::c_macros::MCAST_OPD_FCALL {
                                mcc_dbg!(
                                    "sem::fcall",
                                    "[FCALL-INST-DBG] nested FuncCall, parsing via parse_phrase"
                                );
                                caller = Self::try_parse_inner_fcall(&inner, context);
                                if caller.is_none() {
                                    if let Some(phrase) = parse_phrase(&inner, context) {
                                        caller = Some(Box::new(phrase));
                                    }
                                }
                            } else if inner_type == crate::ast::c_macros::MCAST_OPD {
                                // MCAST_OPD may wrap MCAST_OPD_FCALL
                                if let Some(opd_inner) = inner.get_sub_node() {
                                    if opd_inner.get_type() == crate::ast::c_macros::MCAST_OPD_FCALL
                                    {
                                        mcc_dbg!("sem::fcall", "[FCALL-INST-DBG] MCAST_OPD wraps FuncCall, parsing via parse_phrase");
                                        caller = Self::try_parse_inner_fcall(&opd_inner, context);
                                        if caller.is_none() {
                                            if let Some(phrase) = parse_phrase(&opd_inner, context)
                                            {
                                                caller = Some(Box::new(phrase));
                                            }
                                        }
                                    }
                                }
                            }
                            if caller.is_some() {
                                // caller was set by the nested FuncCall handling above
                            } else {
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
                                        // Store the source span for diagnostics on this anonymous instance.
                                        let inst_span = (each.get_pos() as usize)
                                            ..((each.get_pos() + each.get_len()) as usize);
                                        context.store_inst_span(&anon_name, inst_span);

                                        if let Some(McCMIE::Component(comp_def)) =
                                            resolve_cmie(&DB, &ids, context.uri())
                                        {
                                            check_ctor_bind(
                                                &anon_name,
                                                &comp_def,
                                                &instance_params,
                                                &each,
                                            );
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
                                            resolve_cmie(&DB, &ids, context.uri())
                                        {
                                            let module = Mc2Module::new(&anon_name, mod_def);
                                            if let Some(phrase) =
                                                context.add_module(anon_name, module)
                                            {
                                                caller = Some(Box::new(phrase));
                                            }
                                        } else if let Some(McCMIE::Interface(iface_def)) =
                                            resolve_cmie(&DB, &ids, context.uri())
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
                                        // Store the source span for diagnostics on this anonymous instance.
                                        let inst_span = (each.get_pos() as usize)
                                            ..((each.get_pos() + each.get_len()) as usize);
                                        context.store_inst_span(&anon_name, inst_span);

                                        if let Some(McCMIE::Component(comp_def)) =
                                            resolve_cmie(&DB, &ids, context.uri())
                                        {
                                            check_ctor_bind(
                                                &anon_name,
                                                &comp_def,
                                                &instance_params,
                                                &each,
                                            );
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
                                            resolve_cmie(&DB, &ids, context.uri())
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
                                    // Store the source span for diagnostics on this anonymous instance.
                                    let inst_span = (each.get_pos() as usize)
                                        ..((each.get_pos() + each.get_len()) as usize);
                                    context.store_inst_span(&anon_name, inst_span);

                                    if let Some(McCMIE::Component(comp_def)) =
                                        resolve_cmie(&DB, &ids, context.uri())
                                    {
                                        check_ctor_bind(
                                            &anon_name,
                                            &comp_def,
                                            &instance_params,
                                            &each,
                                        );
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
                                        resolve_cmie(&DB, &ids, context.uri())
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
                                                                id: 0,
                                                                caller,
                                                                func_name: name,
                                                                params,
                                                                left,
                                                                right,
                                                                dot_member: None,
                                                                resolved_return_shape: None,
                                                                pre_closure: false,
                                                                named_ctor: false,
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
                                            crate::errcodes::FUNC_CALL_MISSING_NAME,
                                            node,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::FUNC_CALL_MISSING_NAME,
                                                &[],
                                            ),
                                        );
                                        return None;
                                    }
                                }
                            },
                            McPhrase::Series(_, _) => {
                                dlog_error(
                                    crate::errcodes::FUNC_CALL_MISSING_NAME,
                                    node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::FUNC_CALL_MISSING_NAME,
                                        &[],
                                    ),
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
                                        crate::errcodes::FUNC_CALL_MISSING_NAME,
                                        node,
                                        &crate::errcodes::format_msg(
                                            crate::errcodes::FUNC_CALL_MISSING_NAME,
                                            &[],
                                        ),
                                    );
                                    return None;
                                }
                            }
                        }
                    } else {
                        dlog_error(
                            crate::errcodes::FUNC_CALL_MISSING_NAME,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::FUNC_CALL_MISSING_NAME,
                                &[],
                            ),
                        );
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
                                dlog_error(
                                    crate::errcodes::FUNC_CALL_MISSING_NAME,
                                    node,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::FUNC_CALL_MISSING_NAME,
                                        &[],
                                    ),
                                );
                                return None;
                            }
                        },
                        McPhrase::Series(_, _) => {
                            dlog_error(
                                crate::errcodes::FUNC_CALL_MISSING_NAME,
                                node,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::FUNC_CALL_MISSING_NAME,
                                    &[],
                                ),
                            );
                            return None;
                        }
                        _ => {
                            dlog_error(
                                crate::errcodes::FUNC_CALL_MISSING_NAME,
                                node,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::FUNC_CALL_MISSING_NAME,
                                    &[],
                                ),
                            );
                            return None;
                        }
                    }
                } else {
                    dlog_error(
                        crate::errcodes::FUNC_CALL_MISSING_NAME,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::FUNC_CALL_MISSING_NAME, &[]),
                    );
                    return None;
                }
            }
        };

        // === Iter 2: handle dot-notation func_name ===
        // Structured segment extraction (`obj.method` → ["obj", "method"]),
        // no `to_string()` + `trim_start_matches('.')` text re-processing.
        // Non-plain chains (curly/square/array segments) keep the original name.
        let func_name = if let Some(parts) = func_name.dot_chain_parts() {
            if parts.len() > 1 {
                let first = parts[0].clone();
                // Rebuild the method name from the remaining segments.
                let rest: String = parts[1..].join(".");
                if caller.is_none() {
                    if let Some(ident) = context.find_inst(&first) {
                        caller = Some(Box::new(ident.into()));
                        McIds::from(rest.as_str())
                    } else {
                        func_name
                    }
                } else {
                    McIds::from(rest.as_str())
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
        //
        // ── P2-12: For known two-pin classes (CAP/RES/IND/...), do NOT create
        // anonymous components at parse time. Instead, preserve as FuncCall so that
        // the instantiation phase can properly auto-name and wire them.
        if caller.is_none() {
            let _cmie_result = resolve_cmie(&DB, &func_name, context.uri()).is_some();
            let func_name_str = func_name.to_string();
            let is_twopin = crate::vector::graph::naming::is_known_twopin_class(&func_name_str);
            if !is_twopin {
                if let Some(cmie) = resolve_cmie(&DB, &func_name, context.uri()) {
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
                                // Store the source span for diagnostics on this anonymous instance.
                                let inst_span = (node.get_pos() as usize)
                                    ..((node.get_pos() + node.get_len()) as usize);
                                context.store_inst_span(&inst_name, inst_span);
                                // NC is an instance modifier: with_params keeps
                                // every argument (NC included), sets nc=true and
                                // binds the rest at instantiation time.
                                check_ctor_bind(&inst_name, &comp_def, &params, node);
                                let mc2_comp = Mc2Component::with_params(
                                    &inst_name,
                                    comp_def.clone(),
                                    params.clone(),
                                );
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
                                // Store the source span for diagnostics on this anonymous instance.
                                let inst_span = (node.get_pos() as usize)
                                    ..((node.get_pos() + node.get_len()) as usize);
                                context.store_inst_span(&inst_name, inst_span);
                                let mc2_mod = Mc2Module::new(&inst_name, mod_def.clone());
                                context.add_module(inst_name.clone(), mc2_mod);
                                return Some(McPhrase::Endpoint(McEndpoint::Single(
                                    McInstanceRef::new(McInstance::Module(Arc::new(
                                        Mc2Module::new(&inst_name, mod_def),
                                    ))),
                                )));
                            }
                            // else: fall through to FuncCall construction below
                        }
                        _ => {}
                    }
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

        let caller_desc = match &caller {
            Some(c) => match c.as_ref() {
                McPhrase::FuncCall(fc) => format!("FuncCall({})", fc.func_name),
                McPhrase::Endpoint(_) => "Endpoint".into(),
                _ => format!("{:?}", std::mem::discriminant(c.as_ref())),
            },
            None => "None".into(),
        };
        let fn_str = func_name.to_string();
        if fn_str == "Cap" || fn_str == "Pullup" || fn_str == "Pulldown" {
            mcc_dbg!(
                "sem::fcall",
                "[FCALL-FINAL] func={fn_str} caller={caller_desc} params={:?}",
                params
                    .iter()
                    .map(|p| format!("{:?}", p))
                    .collect::<Vec<_>>()
            );
        }

        // ── Construction-arg bind check for forms that skip the
        // `caller.is_none()` Endpoint branch above ──────────────────────
        // (1) a bare two-pin class keeps a Label caller (`DIO.ESD(...)` →
        //     caller=Label("DIO")), (2) a declareb keeps an Endpoint caller
        //     (`D1::DIO.ESD(...)`), and (3) a two-pin class with no caller
        //     (`CAP(100nF)`) falls straight through — none of them reach
        //     check_ctor_bind. Reconstruct the dotted class name (Label head
        //     + func_name) and run the same signature bind here, so a bad
        //     construction argument (missing / excess / unknown / NC outside
        //     a construction) is reported as E4176 at the construction site.
        //     Chained heads (`CAP(x).Cap(y)`) are already checked by
        //     try_parse_inner_fcall, and instance method calls (`uC.i2c()`,
        //     `C4.Cap(a, b)`) never resolve to a component class — both are
        //     skipped by the resolution below.
        let head_label = caller.as_deref().and_then(|c| match c {
            McPhrase::Endpoint(McEndpoint::Single(iref)) => match &iref.base {
                McInstance::Label(s) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        });
        let mut ctor_comp: Option<Arc<crate::semantic::component::McComponent>> = None;
        let mut ctor_class_name = String::new();
        // Candidate 1: func_name alone (`CAP(100nF)`, `D1::DIO.ESD(...)`).
        if let Some(McCMIE::Component(c)) = resolve_cmie(&DB, &func_name, context.uri()) {
            ctor_comp = Some(c);
            ctor_class_name = fn_str.clone();
        }
        // Candidate 2: dotted label.func_name (bare `DIO.ESD(...)` →
        // caller=Label("DIO") gives func_name="ESD").
        if ctor_comp.is_none() {
            if let Some(head) = &head_label {
                let dotted = format!("{head}.{fn_str}");
                let dotted_ids = McIds::from(dotted.as_str());
                if let Some(McCMIE::Component(c)) = resolve_cmie(&DB, &dotted_ids, context.uri()) {
                    ctor_comp = Some(c);
                    ctor_class_name = dotted;
                }
            }
        }
        // Candidate 3: bare-name alias (`ESD(...)` → `DIO.ESD`), mirroring
        // the pass2 fallback in funccall.rs.
        if ctor_comp.is_none() {
            if let Some(canon) = crate::vector::graph::naming::canonicalize_class_alias(&fn_str) {
                let canon_ids = McIds::from(canon.as_str());
                if let Some(McCMIE::Component(c)) = resolve_cmie(&DB, &canon_ids, context.uri()) {
                    ctor_comp = Some(c);
                    ctor_class_name = canon;
                }
            }
        }
        if let Some(comp_def) = ctor_comp {
            check_ctor_bind(&ctor_class_name, &comp_def, &params, node);
        }

        Some(McPhrase::FuncCall(McFuncCall {
            id: 0,
            caller,
            func_name,
            params,
            left,
            right,
            dot_member: None,
            resolved_return_shape: None,
            pre_closure: false,
            named_ctor: false,
        }))
    }

    /// ★ P4.1: Resolve return shape from McFunction.returns.
    /// Called after the enclosing scope is fully parsed ("Pass1b" hook), or
    /// directly by [`Self::fill_return_shape`].
    ///
    /// # Three-state rules (per eval.md §8.1):
    /// - `Implicit` / `This` → `ReturnShape::This` — preserves caller shape
    /// - `Endpoint(ref phrase)` → `ReturnShape::Label { bus }` — left empty, right = phrase's right interface
    pub fn resolve_return_shape(&mut self, func_returns: &McFuncReturn) {
        match func_returns {
            McFuncReturn::Implicit | McFuncReturn::This => {
                self.resolved_return_shape = Some(ReturnShape::This);
            }
            McFuncReturn::Endpoint(phrase) => {
                // Endpoint return: derive bus from the returned phrase's right side
                let bus = get_right_bus_from_phrase(phrase);
                self.resolved_return_shape = Some(ReturnShape::Label { bus });
            }
        }
    }

    /// ★ P4.1: Resolve the call's return shape (eval.md §8.1) from the called
    /// function's `McFuncReturn`, and store it on `self`.
    ///
    /// Lookup priority:
    ///   1. Instance method — walk the caller chain down to its root instance
    ///      and query that type's `funcs` table (same walk as `check_chain_validity`).
    ///   2. Scope function — bare call resolved via `scope.find_func_return`.
    ///
    /// Unknown / unresolvable calls keep `resolved_return_shape = None`; the
    /// phrase-level fallback then preserves the parse-time shape (no change).
    pub fn fill_return_shape(&mut self, scope: &dyn HasFindInst) {
        if self.resolved_return_shape.is_some() {
            return;
        }
        if let Some(ret) = Self::lookup_func_returns(&self.caller, &self.func_name, scope) {
            self.resolve_return_shape(&ret);
        }
    }

    /// Look up the `McFuncReturn` of a call. Mirrors the receiver-chain walk of
    /// [`Self::check_chain_validity`]: instance methods resolve via the root
    /// receiver's `funcs` table, bare calls via the surrounding scope.
    fn lookup_func_returns(
        caller: &Option<Box<McPhrase>>,
        func_name: &McIds,
        scope: &dyn HasFindInst,
    ) -> Option<McFuncReturn> {
        let name = func_name.to_string();
        let root = caller
            .as_ref()
            .and_then(|c| Self::root_receiver(c.as_ref()));
        match root {
            Some(McInstance::Module(arc_mod)) => {
                arc_mod.base.funcs.find(&name).map(|f| f.returns.clone())
            }
            Some(McInstance::Component(arc_comp)) => {
                arc_comp.base.funcs.find(&name).map(|f| f.returns.clone())
            }
            // Receiver is Bus / Label / List / Interface — has no `funcs` table.
            Some(_) => None,
            // Bare call (no receiver) or unresolvable root → surrounding scope.
            None => scope.find_func_return(&name),
        }
    }

    /// ★ P4.1: Recursively walk a phrase and fill `resolved_return_shape` on
    /// every nested `FuncCall`. Called after the enclosing body has been fully
    /// parsed ("Pass1b" hook), so the scope's funcs tables are complete.
    pub fn fill_return_shapes(phrase: &mut McPhrase, scope: &dyn HasFindInst) {
        match phrase {
            McPhrase::FuncCall(f) => {
                f.fill_return_shape(scope);
                // Chained calls: the inner FuncCall lives in `caller`
                // (e.g. `CT.config_a(V5V).config_b(GND)`), so recurse into it.
                if let Some(c) = &mut f.caller {
                    Self::fill_return_shapes(c, scope);
                }
            }
            McPhrase::Series(elems, _) => {
                for e in elems {
                    Self::fill_return_shapes(e, scope);
                }
            }
            McPhrase::Parallel(v) | McPhrase::Multiple(v) => {
                for e in v {
                    Self::fill_return_shapes(e, scope);
                }
            }
            McPhrase::Group(g) => {
                for o in &mut g.opds {
                    Self::fill_return_shapes(o, scope);
                }
            }
            McPhrase::Transposed(inner) => Self::fill_return_shapes(inner, scope),
            McPhrase::Closure(c) => {
                for line in &mut c.body {
                    Self::fill_return_shapes(line, scope);
                }
            }
            McPhrase::Member(p, _) => Self::fill_return_shapes(p, scope),
            McPhrase::Lead | McPhrase::Endpoint(_) => {}
        }
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
        let ret: Option<McFuncReturn> =
            Self::lookup_func_returns(&inner_fc.caller, &inner_fc.func_name, context);

        let Some(ret) = ret else { return };
        if ret.is_chainable() {
            return;
        }

        debug_assert!(matches!(ret, McFuncReturn::Endpoint(_)));
        dlog_error(
            crate::errcodes::FCALL_PARSE_FAILED,
            node,
            &crate::errcodes::format_msg(
                crate::errcodes::FCALL_PARSE_FAILED,
                &[&outer_method, &inner_name, &inner_name],
            ),
        );
    }

    /// Walk a phrase down through chained `FuncCall`s to find the root
    /// receiver instance (the first non-FuncCall caller).
    ///
    /// For `mcu.setup().add_caps().i2c()`, calling this on the outer i2c's
    /// inner-FuncCall caller (i.e. add_caps's FuncCall phrase) will recurse:
    /// add_caps → setup → mcu endpoint, returning the `mcu` instance.
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

    /// Try to parse an inner MCAST_OPD_FCALL as a FuncCall for two-pin classes.
    /// This avoids premature anonymous component instantiation when the inner
    /// FuncCall is the caller of a chained method (e.g. CAP(10uF).Cap(...)).
    /// Returns None if the inner FuncCall is not a known two-pin class.
    fn try_parse_inner_fcall(
        node: &AstNode,
        context: &mut dyn HasFindInst,
    ) -> Option<Box<McPhrase>> {
        let subnode = node.get_sub_node()?;
        let mut func_name: Option<McIds> = None;
        let mut params: Vec<McParamValue> = Vec::new();

        for child in subnode.iter() {
            match child.get_type() {
                MCAST_NAME => {
                    if let Some(ids_node) = child.get_sub_node() {
                        func_name = McIds::new(&ids_node);
                    }
                }
                MCAST_PARAMS => {
                    if let Some(param_nodes) = child.get_sub_node() {
                        for param_node in param_nodes.iter() {
                            if let Some(value) = McParamValue::new(&param_node, context) {
                                params.push(value);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let fname = func_name?;
        let fname_str = fname.to_string();
        if crate::vector::graph::naming::is_known_twopin_class(&fname_str) {
            // ── Construction-arg bind check (two-pin path) ────────────────
            // `CAP(...).Cap(...)` bypasses the with_params creation points
            // above (the caller is kept as a bare FuncCall), so bind the
            // construction arguments here: an argument that does not match
            // the class signature — e.g. a package value `C0402` / `PKG.C0402`
            // passed to a class that declares no enum-class parameter for it —
            // is reported as E4176 at the construction site. NC occupies no
            // slot: it is stripped before binding and does not cover a
            // missing required parameter.
            if let Some(cmie) = resolve_cmie(&DB, &fname, context.uri()) {
                if let McCMIE::Component(comp_def) = cmie {
                    check_ctor_bind(&fname_str, &comp_def, &params, node);
                }
            }
            mcc_dbg!(
                "sem::fcall",
                "[FCALL-INST-DBG] inner two-pin class: {fname_str}, building FuncCall directly"
            );
            Some(Box::new(McPhrase::FuncCall(McFuncCall {
                id: 0,
                caller: None,
                func_name: fname,
                params,
                left: vec![],
                right: vec![],
                dot_member: None,
                resolved_return_shape: None,
                pre_closure: false,
                named_ctor: false,
            })))
        } else {
            None
        }
    }
}
