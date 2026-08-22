// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Parameter substitution helpers (Iteration A)
//!
//! "Formal parameter -> actual argument" substitution chain for user functions / instance methods:
//!
//! - `param_value_to_node_elements` —— McParamValue -> Vec<McBus>
//! - `opdc_to_node_elements`        —— McOpd -> Vec<McBus>
//! - `ids_to_node_elements`         —— McIds -> Vec<McBus>
//! - `substitute_node_element(s)`   —— substitute formal with actual in a single McBus / McBus list
//! - `node_elements_to_bus`         —— Vec<McBus> -> single McBus (with members)
//! - `substitute_param_value`       —— recursively substitute inside McParamValue (FuncCall nested scenario)
//! - `substitute_phrase` / `substitute_stmt` —— substitute throughout the McPhrase tree

use super::expand::ExpansionContext;
use super::McModuleInst;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_closure::McClosure;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_fcall::McFuncCall;
use crate::semantic::basic::mc_group::McGroup;
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_inst::McInstance;
use crate::McIds;

impl McModuleInst {
    // ========================================================================
    // McParamValue / McOpd / McIds → Vec<McBus>
    // ========================================================================

    /// Convert McParamValue to McBus(s)
    ///
    /// Transforms function actual-parameter values into node elements
    /// usable in connection stmts.
    pub(super) fn param_value_to_node_elements(value: &McParamValue) -> Vec<McBus> {
        match value {
            McParamValue::Ids(ids) => {
                if ids.is_empty() {
                    vec![]
                } else {
                    // Use dot-joined name as flat node element
                    vec![McBus {
                        name: ids.to_string(),
                        member: Vec::new(),
                        full_members: Vec::new(),
                    }]
                }
            }
            McParamValue::Opd(opdc) => Self::opdc_to_node_elements(opdc),
            McParamValue::Const(c) => {
                vec![McBus {
                    name: format!("{c}"),
                    member: Vec::new(),
                    full_members: Vec::new(),
                }]
            }
            McParamValue::Set(values) => values
                .iter()
                .flat_map(Self::param_value_to_node_elements)
                .collect(),
            McParamValue::Phrase(phrase) => {
                // Net expression as parameter (e.g., [dc.VDD_3V3 -> wm7121.VCC])
                // Return the left endpoint as the primary connection target.
                // The -> connection itself is handled by wire_builtin_twopin.
                phrase.get_left()
            }
            McParamValue::InlineAttrs(attrs) => {
                // P1-6: Attribute blocks (`key = value`) are NOT net elements.
                // The previous `_ =>` fallback degraded them into a fabricated
                // text node name via format! (e.g. "[key = value]"), silently
                // losing the structured attribute info. Report the degradation
                // and produce no node instead.
                let shown: Vec<String> = attrs.iter().map(|a| format!("{a}")).collect();
                tracing::warn!(
                    "param_value_to_node_elements: InlineAttrs argument [{}] cannot be \
                     converted to a connection node; attributes are not net elements (P1-6)",
                    shown.join(", ")
                );
                vec![]
            }
            _ => {
                // Literals (Int/Hex/Float/String/UValue/NONE/NC) - use Display as node name.
                // This is the intended mechanism for value-bearing components such as
                // `CAP(100nF)` -> node name "100nF".
                let name = format!("{value}");
                vec![McBus {
                    name,
                    member: Vec::new(),
                    full_members: Vec::new(),
                }]
            }
        }
    }

    /// Convert McOpd to McBus list
    ///
    /// Converts parameter operands to connection-line node elements.
    /// McOpd is currently simplified to four variants (Id/This/Pins/Uscore);
    /// complex structures (DotId, Curly, Square, etc.) are internalized into McIds
    /// and handled through McIds::expand() and McIds::as_bus().
    fn opdc_to_node_elements(opdc: &McOpd) -> Vec<McBus> {
        match opdc {
            McOpd::Id(ids) | McOpd::This(ids) | McOpd::Pins(ids) => Self::ids_to_node_elements(ids),
            McOpd::Uscore => {
                // Underscore _ indicates no connection / placeholder
                vec![]
            }
        }
    }

    /// Convert McIds to a McBus list
    ///
    /// Handles three cases:
    /// 1. Bus form (e.g. `DC1{VDD, GND}`) -> McBus { name: "DC1", member: ["VDD", "GND"] }
    /// 2. Multi-value expansion (e.g. `GPIO[1:4]`) -> multiple independent McBus
    /// 3. Simple name (e.g. `R1`) -> single McBus
    fn ids_to_node_elements(ids: &McIds) -> Vec<McBus> {
        // Check Bus form first: DC1{VDD, GND} -> single McBus with members
        if let Some((base_name, members)) = ids.as_bus() {
            return vec![McBus::new_with_members(&base_name, members)];
        }

        // Non-Bus form: expand into independent nodes
        let expanded = ids.expand();
        if expanded.is_empty() {
            return vec![];
        }

        expanded.iter().map(|name| McBus::new(name)).collect()
    }

    // ========================================================================
    // formal → actual substitution
    // ========================================================================

    /// Substitute formal parameter references in a McBus.
    ///
    /// Two cases:
    /// 1. Simple: formal `sin` -> actual `S1`,
    ///    McBus{name:"sin"} -> McBus{name:"S1"}
    /// 2. With members: formal `dc24v[VCC,GND]` -> actual `my_dc[V1,G1]`,
    ///    McBus{name:"dc24v",member:["VCC"]}
    ///    -> McBus{name:"my_dc",member:["V1"]}
    ///
    /// Flattened version: elem.member is Vec<String>
    fn substitute_node_element(elem: &McBus, bindings: &McParamBindings) -> Vec<McBus> {
        // Check if node name matches a formal parameter
        if let Some(binding) = bindings.find(&elem.name) {
            if let Some(value) = binding.get_value() {
                if elem.member.is_empty() {
                    let members = binding.declare.expand();
                    if members.len() > 1 {
                        if let Some(idx) = members.iter().position(|m| {
                            m == &elem.name || m.rsplit('.').next() == Some(elem.name.as_str())
                        }) {
                            if let McParamValue::Set(vals) = value {
                                if let Some(v) = vals.get(idx) {
                                    return Self::param_value_to_node_elements(v);
                                }
                            }
                            if idx == 0 {
                                return Self::param_value_to_node_elements(value);
                            }
                            return vec![McBus {
                                name: elem.name.clone(),
                                member: vec![],
                                full_members: vec![],
                            }];
                        }
                    }
                    return Self::param_value_to_node_elements(value);
                } else {
                    // Parameter with members: dc24v.VCC -> my_dc.V1
                    // elem.member is now Vec<String>
                    let mut new_elems = Self::param_value_to_node_elements(value);
                    if new_elems.len() == 1 {
                        let new_base = &mut new_elems[0];
                        let mut new_members: Vec<String> = Vec::new();
                        for child_name in &elem.member {
                            if let Some(member_val) = binding.get_member_value(child_name) {
                                // Substitute member value
                                let substituted = Self::param_value_to_node_elements(&member_val);
                                for sub_elem in substituted {
                                    // If substituted element has members, use them; otherwise use the name
                                    if sub_elem.member.is_empty() {
                                        new_members.push(sub_elem.name);
                                    } else {
                                        // Flatten: take the substituted member names
                                        new_members.push(sub_elem.name.clone());
                                        new_members.extend(sub_elem.member);
                                    }
                                }
                            }
                            // `get_member_value` is a stub (always None, see eval.md §11.5
                            // design gap A), so member-level substitution does not run and
                            // the member name stays as-is for now.
                        }
                        new_base.member = new_members;
                    }
                    return new_elems;
                }
            }
        }

        // No match -> return element unchanged (with flat string members)
        vec![McBus {
            name: elem.name.clone(),
            member: elem.member.clone(),
            full_members: elem.full_members.clone(),
        }]
    }

    /// Substitute parameters in a list of NodeElements
    fn substitute_node_elements(elements: &[McBus], bindings: &McParamBindings) -> Vec<McBus> {
        elements
            .iter()
            .flat_map(|elem| Self::substitute_node_element(elem, bindings))
            .collect()
    }

    /// Convert Vec<McBus> back to McBus
    pub(super) fn node_elements_to_bus(elements: &[McBus]) -> McBus {
        if elements.is_empty() {
            return McBus::new("<empty>");
        }
        if elements.len() == 1 {
            return McBus::new_with_members(&elements[0].name, elements[0].member.clone());
        }
        // --- Iter-3.B3 ----------------------------------------------------
        // In the multi-element case, the previous logic `name = elements[0].name; members = flat_map(e.member)`
        // **loses all bare McBus entries except the first one's name**. Example:
        //   V1V2 -> [McBus{"VCC_1V2",[]}, McBus{"GND",[]}]
        //   Old code: name="VCC_1V2", members=[], result is McBus{"VCC_1V2"} -- GND is lost
        //   New code: all elements have empty .member -> treated as "anonymous bus, element names as members"
        //             result is McBus{name:"", member:["VCC_1V2", "GND"]}, downstream P1-A4
        //             can correctly expand into two NetPoints.
        let all_empty_members = elements.iter().all(|e| e.member.is_empty());
        if all_empty_members {
            let members: Vec<String> = elements.iter().map(|e| e.name.clone()).collect();
            return McBus::new_with_members("", members);
        }
        // Mixed form (both name and member present): keep the original logic as fallback
        let name = &elements[0].name;
        let members: Vec<String> = elements.iter().flat_map(|e| e.member.clone()).collect();
        McBus::new_with_members(name, members)
    }

    /// Substitute formal parameter references inside McParamValue.
    ///
    /// Handles cases like `func f(pwr) { Cap(pwr) }` where Cap's
    /// param `pwr` needs to be replaced with the actual argument.
    fn substitute_param_value(value: &McParamValue, bindings: &McParamBindings) -> McParamValue {
        match value {
            McParamValue::Ids(ids) => {
                // Convert to string to handle all cases uniformly
                let ids_str = ids.to_string();

                // Check if it's a single segment or multi-segment
                if let Some((first_seg, _)) = ids_str.split_once(".") {
                    // Multi-segment case
                    if let Some(binding) = bindings.find(first_seg) {
                        if let Some(actual) = binding.get_value() {
                            let actual_str = actual.to_string();
                            let new_str = format!(
                                "{}.{}",
                                actual_str,
                                ids_str
                                    .split_once(".")
                                    .map(|(_, s)| s)
                                    .unwrap_or(ids_str.as_str())
                            );
                            let new_opdc = McIds::from(new_str.as_str());
                            return McParamValue::Ids(new_opdc);
                        }
                    }
                } else {
                    // Single segment case
                    if let Some(binding) = bindings.find(&ids_str) {
                        if let Some(actual) = binding.get_value() {
                            return actual.clone();
                        }
                    }
                }
                value.clone()
            }
            McParamValue::Set(values) => McParamValue::Set(
                values
                    .iter()
                    .map(|v| Self::substitute_param_value(v, bindings))
                    .collect(),
            ),
            /*McParamValue::FuncCall(fc) => {
                let new_params: Vec<McParamValue> = fc
                    .params
                    .iter()
                    .map(|p| Self::substitute_param_value(p, bindings))
                    .collect();
                McParamValue::FuncCall(Box::new(crate::semantic::basic::mc_param::McParamFuncCall {
                    caller: fc.caller.clone(),
                    name: fc.name.clone(),
                    params: new_params,
                    chain: fc.chain.clone(),
                }))
            }*/
            _ => value.clone(),
        }
    }

    // ========================================================================
    // McPhrase tree substitution
    // ========================================================================

    /// Resolve a `this` reference to the caller instance bus.
    ///
    /// Supported forms:
    /// - `this`       → `caller_inst_name`
    /// - `this.xxx`   → `caller_inst_name.xxx`
    /// - `this{a, b}` → `caller_inst_name{a, b}` (curly member access, e.g. `this{1}`)
    fn this_ref_to_bus(s: &str, ctx: &ExpansionContext) -> McBus {
        let this_name = ctx.instance.name.as_str();
        if s == "this" {
            return McBus::new(this_name);
        }
        if let Some(rest) = s.strip_prefix("this.") {
            return McBus::new(&format!("{this_name}.{rest}"));
        }
        if let Some(rest) = s.strip_prefix("this{") {
            if let Some(inner) = rest.strip_suffix('}') {
                let members: Vec<String> = inner
                    .split(',')
                    .map(|m| m.trim().to_string())
                    .filter(|m| !m.is_empty())
                    .collect();
                return McBus::new_with_members(this_name, members);
            }
        }
        McBus::new(s)
    }

    /// Substitute formal parameters in an McPhrase
    fn substitute_phrase(
        phrase: &McPhrase,
        bindings: &McParamBindings,
        expansion_ctx: Option<&ExpansionContext>,
    ) -> McPhrase {
        match phrase {
            McPhrase::Series(phrases, d) => McPhrase::Series(
                phrases
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx))
                    .collect(),
                *d,
            ),
            McPhrase::Parallel(phrases) => McPhrase::Parallel(
                phrases
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx))
                    .collect(),
            ),
            McPhrase::Closure(c) => McPhrase::Closure(McClosure {
                params: c.params.clone(),
                right: Self::substitute_node_elements(&c.right, bindings),
                body: c
                    .body
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx))
                    .collect(),
            }),
            McPhrase::Group(g) => McPhrase::Group(McGroup {
                opds: g
                    .opds
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx))
                    .collect(),
                left_match: g.left_match,
                right_match: g.right_match,
            }),
            McPhrase::FuncCall(f) => McPhrase::FuncCall(McFuncCall {
                id: 0,
                caller: f
                    .caller
                    .as_ref()
                    .map(|c| Box::new(Self::substitute_phrase(c, bindings, expansion_ctx))),
                func_name: f.func_name.clone(),
                params: f
                    .params
                    .iter()
                    .map(|p| Self::substitute_param_value(p, bindings))
                    .collect(),
                left: Self::substitute_node_elements(&f.left, bindings),
                right: Self::substitute_node_elements(&f.right, bindings),
                dot_member: f.dot_member.clone(),
                resolved_return_shape: f.resolved_return_shape.clone(),
                pre_closure: f.pre_closure,
                named_ctor: f.named_ctor,
            }),
            McPhrase::Transposed(inner) => McPhrase::Transposed(Box::new(Self::substitute_phrase(
                inner,
                bindings,
                expansion_ctx,
            ))),
            McPhrase::Lead => phrase.clone(),
            // --- Iter-2.3 ------------------------------------------------
            // Previously Endpoint::Single(Label/Bus/List) was returned as-is -- as a result
            // the V1V2 formal parameter in `V1V2 => CAP(...)` was never substituted, the func body could
            // only lay out an isolated "V1V2" label, and the user would see "decoupling cap not connected to power".
            //
            // Fix: for Endpoint of Label/Bus/List types, try to run substitute_node_element.
            // Component/Module/Interface are "already declared concrete instances", formal params should not override them, keep as-is.
            //
            // --- this substitution -------------------------------------------
            // Replace "this" / "this.xxx" / "this{a, b}" with the caller instance
            // bus ("caller_inst_name" / "caller_inst_name.xxx" / "caller_inst_name{a, b}").
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(s),
                ..
            })) => {
                let is_this_ref = s == "this"
                    || s.starts_with("this.")
                    || (s.starts_with("this{") && s.ends_with('}'));
                let mut elem = McBus::new(s);

                // Check whether it's a this reference
                if let Some(ctx) = expansion_ctx {
                    if is_this_ref {
                        elem = Self::this_ref_to_bus(s, ctx);
                    }
                }

                let substituted = Self::substitute_node_element(&elem, bindings);
                if substituted.is_empty() {
                    phrase.clone()
                } else if substituted.len() == 1
                    && substituted[0].name == elem.name
                    && substituted[0].member.is_empty()
                    && !is_this_ref
                {
                    // No substitution hit for a non-this label, return as-is
                    phrase.clone()
                } else {
                    // Substitution hit (or a this-reference resolved to the
                    // caller instance bus): merge into a Bus endpoint.
                    let bus = Self::node_elements_to_bus(&substituted);
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(bus))))
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(ref b),
                ..
            })) => {
                // Check whether the Bus name is a this reference
                let is_this_ref = b.name == "this"
                    || b.name.starts_with("this.")
                    || (b.name.starts_with("this{") && b.name.ends_with('}'));
                let mut bus_name = b.name.clone();
                let mut this_members: Option<Vec<String>> = None;
                if let Some(ctx) = expansion_ctx {
                    if is_this_ref {
                        let bus = Self::this_ref_to_bus(&b.name, ctx);
                        bus_name = bus.name;
                        this_members = Some(bus.member);
                    }
                }

                let elem = McBus::new_with_members(
                    &bus_name,
                    this_members.unwrap_or_else(|| b.member.clone()),
                );
                let substituted = Self::substitute_node_element(&elem, bindings);
                if substituted.is_empty() {
                    phrase.clone()
                } else if substituted.len() == 1
                    && substituted[0].name == bus_name
                    && substituted[0].member == b.member
                    && !is_this_ref
                {
                    // No substitution hit for a non-this bus, return as-is
                    phrase.clone()
                } else {
                    let bus = Self::node_elements_to_bus(&substituted);
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(bus))))
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(ref l),
                ..
            })) => {
                // List does not process this substitution (List form e.g. GPIO[1,2])
                let elem = McBus::new_with_members(&l.name, l.member.clone());
                let substituted = Self::substitute_node_element(&elem, bindings);
                if substituted.len() == 1
                    && substituted[0].name == l.name
                    && substituted[0].member == l.member
                {
                    phrase.clone()
                } else if substituted.is_empty() {
                    phrase.clone()
                } else {
                    let bus = Self::node_elements_to_bus(&substituted);
                    McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(bus))))
                }
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Component(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Module(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(_),
                ..
            })) => phrase.clone(),
            McPhrase::Multiple(phrases) => McPhrase::Multiple(
                phrases
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx))
                    .collect(),
            ),
            McPhrase::Endpoint(McEndpoint::Node {
                ref input,
                ref output,
                ..
            }) => {
                let left_elems: Vec<McBus> = input.iter().flat_map(|e| e.get_left()).collect();
                let right_elems: Vec<McBus> = output.iter().flat_map(|e| e.get_right()).collect();
                // --- Iter-2.3 ---------------------------------------------
                // Also perform formal-parameter substitution on the Node's left/right McBus
                let left_subst = Self::substitute_node_elements(&left_elems, bindings);
                let right_subst = Self::substitute_node_elements(&right_elems, bindings);
                if left_subst.is_empty() && right_subst.is_empty() {
                    McPhrase::Endpoint(McEndpoint::Node {
                        input: vec![],
                        output: vec![],
                    })
                } else if left_subst.is_empty() {
                    let right_bus = Self::node_elements_to_bus(&right_subst);
                    McPhrase::Endpoint(McEndpoint::Node {
                        input: vec![],
                        output: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            right_bus.clone(),
                        )))],
                    })
                } else if right_subst.is_empty() {
                    let left_bus = Self::node_elements_to_bus(&left_subst);
                    McPhrase::Endpoint(McEndpoint::Node {
                        input: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            left_bus.clone(),
                        )))],
                        output: vec![],
                    })
                } else {
                    let left_bus = Self::node_elements_to_bus(&left_subst);
                    let right_bus = Self::node_elements_to_bus(&right_subst);
                    McPhrase::Endpoint(McEndpoint::Node {
                        input: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            left_bus,
                        )))],
                        output: vec![McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            right_bus,
                        )))],
                    })
                }
            }
            McPhrase::Endpoint(ref ep) => McPhrase::Endpoint(ep.clone()),
            McPhrase::Member(phrase, ep) => McPhrase::Member(
                Box::new(Self::substitute_phrase(phrase, bindings, expansion_ctx)),
                ep.clone(),
            ),
        }
    }

    /// Substitute formal parameters in an McPhrase (delegates to substitute_phrase)
    pub(super) fn substitute_stmt(
        phrase: &McPhrase,
        bindings: &McParamBindings,
        expansion_ctx: Option<&ExpansionContext>,
    ) -> McPhrase {
        Self::substitute_phrase(phrase, bindings, expansion_ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::basic::mc_literal::{McLiteral, McString};
    use crate::semantic::component::mc_attr::{McAttrVal, McAttribute};

    /// P1-6 regression: an InlineAttrs argument (attribute block, e.g.
    /// `foo { key = value }`) must NOT be fabricated into a bogus text node
    /// name like `[key = value]`. Attributes are not net elements, so the
    /// conversion must yield no node while reporting the degradation.
    #[test]
    fn inline_attrs_param_value_is_not_degraded_to_text_node() {
        let attr = McAttribute {
            no: 0,
            id: McIds::from("color"),
            values: vec![McAttrVal::AttrLiteral(McLiteral::String(McString {
                value: "red".to_string(),
            }))],
            key_span: None,
        };
        let value = McParamValue::InlineAttrs(vec![attr]);
        let elems = McModuleInst::param_value_to_node_elements(&value);
        assert!(
            elems.is_empty(),
            "InlineAttrs must not become a net node, got {elems:?}"
        );
    }
}
