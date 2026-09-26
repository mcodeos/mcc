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
//! - `substitute_param_value`       —— recursively substitute inside McParamValue (FuncCall nested
//! scenario)
//! - `substitute_phrase` / `substitute_stmt` —— substitute throughout the McPhrase tree

use super::expand::ExpansionContext;
use super::InstantiationBuilder;
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::opd_shape::OpdShape;
use crate::semantic::mc_func::ShapeCtx;
use crate::semantic::basic::mc_closure::McClosure;
use crate::semantic::basic::mc_ref::{McRef, McInstanceRef};
use crate::semantic::basic::mc_fcall::McFuncCall;
use crate::semantic::basic::mc_group::McGroup;
use crate::semantic::basic::mc_opd::McOpd;
use crate::semantic::basic::mc_param::{McParamBindings, McParamValue};
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::mc_inst::McInstance;
use crate::McIds;

impl InstantiationBuilder {
    // McParamValue / McOpd / McIds → Vec<McBus>

    /// Convert McParamValue to McBus(s)
    ///
    /// Transforms function actual-parameter values into node elements
    /// usable in connection stmts.
    pub(super) fn param_value_to_node_elements(
        value: &McParamValue,
        cx: &dyn ShapeCtx,
    ) -> Vec<McBus> {
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
                        synthetic: None,
                    }]
                }
            }
            McParamValue::Opd(opdc) => Self::opdc_to_node_elements(opdc),
            McParamValue::Const(c) => {
                vec![McBus {
                    name: format!("{c}"),
                    member: Vec::new(),
                    full_members: Vec::new(),
                    synthetic: None,
                }]
            }
            McParamValue::Set(values) => values
                .iter()
                .flat_map(|v| Self::param_value_to_node_elements(v, cx))
                .collect(),
            McParamValue::Phrase(phrase) => Self::phrase_to_node_elements(phrase, cx),
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
                    synthetic: None,
                }]
            }
        }
    }

    /// Convert a phrase actual to McBus(s).
    ///
    /// A net expression as parameter (e.g., `[dc.VDD_3V3 -> wm7121.VCC]`)
    /// contributes its left endpoint as the primary connection target; the
    /// `->` connection itself is wired by the normal instantiation path
    /// (method body / func-return face, unified-twopin v2.0).
    ///
    /// A parenthesized list `(a, b)` written in an argument table is one of
    /// the enumerating forms (param-prefix-design §3.1): it contributes one
    /// leaf per member, exactly like `X{a, b}` or `X[1:2]`. Taking the group's
    /// own face would report only `opds[0]` and drop the remaining members
    /// before the leaf count is made — a silent member loss.
    fn phrase_to_node_elements(phrase: &McPhrase, cx: &dyn ShapeCtx) -> Vec<McBus> {
        match phrase {
            McPhrase::Group(g) => g
                .opds
                .iter()
                .flat_map(|p| Self::phrase_to_node_elements(p, cx))
                .collect(),
            // U308 ruling B: what an operand presents on its **left** is a
            // shape question — asked of the value face's canonical
            // width-aligned view, not of the spelling side.
            _ => {
                let new = OpdShape::of(phrase, cx).port_left();
                let old = phrase.get_left();
                let names = |bs: &[McBus]| -> Vec<String> {
                    bs.iter().map(|b| b.name.clone()).collect()
                };
                if names(&old) != names(&new) {
                    eprintln!(
                        "[U308-TRACE] subst phrase_to_node_elements DIVERGE {:?} -> {:?} \
                         phrase={phrase}",
                        names(&old),
                        names(&new)
                    );
                }
                old
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

    // formal → actual substitution

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
    fn substitute_node_element(
        elem: &McBus,
        bindings: &McParamBindings,
        cx: &dyn ShapeCtx,
    ) -> Vec<McBus> {
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
                                    return Self::param_value_to_node_elements(v, cx);
                                }
                            }
                            if idx == 0 {
                                return Self::param_value_to_node_elements(value, cx);
                            }
                            return vec![McBus {
                                name: elem.name.clone(),
                                member: vec![],
                                full_members: vec![],
                                synthetic: elem.synthetic,
                            }];
                        }
                    }
                    return Self::param_value_to_node_elements(value, cx);
                } else {
                    // Parameter with members: dc24v.VCC -> my_dc.V1
                    // elem.member is now Vec<String>
                    let mut new_elems = Self::param_value_to_node_elements(value, cx);
                    if new_elems.len() == 1 {
                        let new_base = &mut new_elems[0];
                        let mut new_members: Vec<String> = Vec::new();
                        for child_name in &elem.member {
                            if let Some(member_val) = binding.get_member_value(child_name) {
                                // Substitute member value
                                let substituted =
                                    Self::param_value_to_node_elements(&member_val, cx);
                                for sub_elem in substituted {
                                    // If substituted element has members, use them; otherwise use
                                    // the name
                                    if sub_elem.member.is_empty() {
                                        new_members.push(sub_elem.name);
                                    } else {
                                        // Flatten: take the substituted member names
                                        new_members.push(sub_elem.name.clone());
                                        new_members.extend(sub_elem.member);
                                    }
                                }
                            }
                            // `get_member_value` projects the declared member to its
                            // bound actual member (e.g. `dc24v.VCC` -> `my_dc.V1`);
                            // a member with no matching binding stays as-is.
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
            synthetic: elem.synthetic,
        }]
    }

    /// Substitute parameters in a list of NodeElements
    fn substitute_node_elements(
        elements: &[McBus],
        bindings: &McParamBindings,
        cx: &dyn ShapeCtx,
    ) -> Vec<McBus> {
        elements
            .iter()
            .flat_map(|elem| Self::substitute_node_element(elem, bindings, cx))
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
        // Iter-3.B3
        // In the multi-element case, the previous logic
        // `name = elements[0].name; members = flat_map(e.member)`
        // **loses all bare McBus entries except the first one's name**. Example:
        //   V1V2 -> [McBus{"VCC_1V2",[]}, McBus{"GND",[]}]
        //   Old code: name="VCC_1V2", members=[], result is McBus{"VCC_1V2"} -- GND is lost
        // New code: all elements have empty .member -> treated as "anonymous bus, element names as
        // members"
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
                            // Multi-member formal (e.g. `[V3V3, GND]::DC(3.3V)`
                            // parsed as Single(Square([V3V3, GND]))): the name
                            // must map to its formal member slot, not to the
                            // whole actual value. McIds::match_name matches
                            // Square forms against their expanded members, so
                            // both `V3V3` and `GND` hit the same binding;
                            // returning the whole actual for every member would
                            // collapse `[V3V3, GND]` into `[V3V3, V3V3]`.
                            // Mirror substitute_node_element: member[0] takes
                            // the whole actual, later members keep their own
                            // name unless the actual is a Set that maps
                            // positionally.
                            let members = binding.declare.expand();
                            if members.len() > 1 {
                                if let Some(idx) = members.iter().position(|m| {
                                    m == &ids_str || m.rsplit('.').next() == Some(ids_str.as_str())
                                }) {
                                    if let McParamValue::Set(vals) = actual {
                                        if let Some(v) = vals.get(idx) {
                                            return Self::external_wrap(v);
                                        }
                                    }
                                    if idx == 0 {
                                        return Self::external_wrap(actual);
                                    }
                                    return value.clone();
                                }
                            }
                            return Self::external_wrap(actual);
                        }
                    }
                }
                value.clone()
            }
            McParamValue::Opd(McOpd::Id(ids)) => {
                // The `=>` parameter-prefixing fold of a bare name
                // (`V3V3 => CAP(..).Cap(_)` → `.Cap(V3V3)`, §1.2) wraps the
                // prefix in an Opd, not an Ids. Route single-segment names
                // through the Ids path so a formal DC rail pair substitutes
                // to its two bound members instead of staying a single scalar
                // point (E4176).
                //
                // Dotted names (e.g. `CAP.X5R` enum members, `PKG.R0402`
                // packages) and non-formal names must keep their Opd wrapper:
                // enum/package validation member-checks only plain Ids values
                // and treats Opd values as opaque positional fallbacks.
                if ids.segments.len() == 1 && bindings.find(&ids.to_string()).is_some() {
                    Self::substitute_param_value(&McParamValue::Ids(ids.clone()), bindings)
                } else {
                    value.clone()
                }
            }
            McParamValue::Set(values) => McParamValue::Set(
                values
                    .iter()
                    .map(|v| Self::substitute_param_value(v, bindings))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }

    // External-value protection

    /// Wrap a substituted actual value so the instance-prefix pass treats it
    /// as opaque external data.
    ///
    /// Substituted values name the **caller's** nets (e.g. `uC.power([VDD_3V3,
    /// GND], ...)` binds `V3V3` to the module lanes `[VDD_3V3, GND]`). They
    /// must not be re-qualified by the body's instance-prefix pass: a bare
    /// `GND` lane would otherwise become `uC.GND` even though it names the
    /// caller's ground, not `this.GND` (matching-rules-design.md §6). The
    /// `Phrase` wrapper is opaque to `prefix_param_value_with_skip` while
    /// `param_value_to_node_elements` / `phrase.get_left()` still recover the
    /// member buses downstream.
    fn external_wrap(value: &McParamValue) -> McParamValue {
        match value {
            // Already a phrase (net expression actual): opaque to prefixing,
            // keep the structure so Series params still wire their internal
            // chain (P2-13).
            McParamValue::Phrase(_) => value.clone(),
            _ => McParamValue::Phrase(Box::new(Self::param_value_to_phrase(value))),
        }
    }

    /// Rebuild an `McPhrase` tree that carries the same member lanes as a
    /// param value, for use inside `external_wrap`.
    fn param_value_to_phrase(value: &McParamValue) -> McPhrase {
        use crate::semantic::basic::mc_opd::McOpd;
        match value {
            McParamValue::Ids(ids) => McPhrase::label(ids.to_string()),
            McParamValue::Opd(McOpd::Id(ids)) => McPhrase::label(ids.to_string()),
            McParamValue::Opd(McOpd::This(ids)) => {
                if ids.is_empty() {
                    McPhrase::label("this".to_string())
                } else {
                    McPhrase::label(format!("this.{ids}"))
                }
            }
            McParamValue::Opd(McOpd::Pins(ids)) => McPhrase::label(ids.to_string()),
            McParamValue::Opd(McOpd::Uscore) => McPhrase::label("_".to_string()),
            McParamValue::Const(c) => McPhrase::label(format!("{c}")),
            McParamValue::Int(v) => McPhrase::label(v.to_string()),
            McParamValue::Hex(v) => McPhrase::label(v.to_string()),
            McParamValue::Float(v) => McPhrase::label(v.to_string()),
            McParamValue::String(v) => McPhrase::label(v.to_string()),
            McParamValue::UValue(v) => McPhrase::label(v.to_string()),
            McParamValue::NONE(name) | McParamValue::NC(name) => McPhrase::label(name.clone()),
            McParamValue::InlineAttrs(_) => McPhrase::label("_".to_string()),
            McParamValue::Set(vals) => {
                McPhrase::Multiple(vals.iter().map(Self::param_value_to_phrase).collect())
            }
            McParamValue::Phrase(p) => (**p).clone(),
        }
    }

    // McPhrase tree substitution

    /// Does this label spell a container self face (`this` / `pins`)?
    ///
    /// Both keywords name the same self face, each in its own spelling: a
    /// container body reads `this` and `pins` interchangeably, so the
    /// instantiation layer rewrites either one to the caller instance.
    fn self_ref_keyword(s: &str) -> Option<&'static str> {
        ["this", "pins"].into_iter().find(|kw| {
            s == *kw
                || s.strip_prefix(*kw).is_some_and(|rest| {
                    rest.starts_with('.') || (rest.starts_with('{') && rest.ends_with('}'))
                })
        })
    }

    /// Resolve a self-face reference to the caller instance bus.
    ///
    /// Supported forms (the keyword is either spelling):
    /// - `kw`       → `caller_inst_name`
    /// - `kw.xxx`   → `caller_inst_name.xxx`
    /// - `kw{a, b}` → `caller_inst_name{a, b}` (curly member access, e.g. `this{1}`)
    ///
    /// The curly split reads the shared text entry (`mc_ids::parse_display`,
    /// §3.1 late binding) and takes base + members from the trailing `Curly`
    /// segment — `|` pipes, `,` separators and numeric slices (`this{1:3}` →
    /// R12) expand structurally, with no `strip_prefix("this.")`-style text
    /// re-derivation of the member list.
    fn self_ref_to_bus(s: &str, ctx: &ExpansionContext) -> McBus {
        let inst_name = ctx.instance.name.as_str();
        let kw = Self::self_ref_keyword(s).unwrap_or("this");
        // Dotted / plain labels rewrite the self token; the suffix is a
        // literal bus name (it may carry its own group text later).
        if let Some(rest) = s.strip_prefix(kw).and_then(|r| r.strip_prefix('.')) {
            return McBus::new(&format!("{inst_name}.{rest}"));
        }
        if s == kw {
            return McBus::new(inst_name);
        }
        // Curly member access `kw{...}`: base is `kw` (guaranteed after the
        // dotted check above) and members come straight from the group.
        if let Some((base, members)) = crate::semantic::basic::mc_ids::curly_base_members(s) {
            if base == kw {
                return McBus::new_with_members(inst_name, members);
            }
            return McBus::new(s);
        }
        McBus::new(s)
    }

    /// Substitute formal parameters in an McPhrase
    fn substitute_phrase(
        phrase: &McPhrase,
        bindings: &McParamBindings,
        expansion_ctx: Option<&ExpansionContext>,
        cx: &dyn ShapeCtx,
    ) -> McPhrase {
        match phrase {
            McPhrase::Series(phrases, d) => McPhrase::Series(
                phrases
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx, cx))
                    .collect(),
                *d,
            ),
            McPhrase::Parallel(phrases) => McPhrase::Parallel(
                phrases
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx, cx))
                    .collect(),
            ),
            McPhrase::Closure(c) => McPhrase::Closure(McClosure {
                params: c.params.clone(),
                right: Self::substitute_node_elements(&c.right, bindings, cx),
                body: c
                    .body
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx, cx))
                    .collect(),
            }),
            McPhrase::Group(g) => McPhrase::Group(McGroup {
                opds: g
                    .opds
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx, cx))
                    .collect(),
                left_match: g.left_match,
                right_match: g.right_match,
            }),
            McPhrase::FuncCall(f) => McPhrase::FuncCall(McFuncCall {
                id: 0,
                caller: f.caller.as_ref().map(|c| {
                    Box::new(Self::substitute_phrase(c, bindings, expansion_ctx, cx))
                }),
                func_name: f.func_name.clone(),
                params: f
                    .params
                    .iter()
                    .map(|p| Self::substitute_param_value(p, bindings))
                    .collect(),
                left: Self::substitute_node_elements(&f.left, bindings, cx),
                right: Self::substitute_node_elements(&f.right, bindings, cx),
                dot_member: f.dot_member.clone(),
                resolved_return_shape: f.resolved_return_shape.clone(),
                pre_closure: f.pre_closure,
                named_ctor: f.named_ctor,
                receiver_is_ctor: f.receiver_is_ctor,
            }),
            McPhrase::Transposed(inner) => McPhrase::Transposed(Box::new(
                Self::substitute_phrase(inner, bindings, expansion_ctx, cx),
            )),
            McPhrase::Reversed(inner) => McPhrase::Reversed(Box::new(Self::substitute_phrase(
                inner,
                bindings,
                expansion_ctx,
                cx,
            ))),
            McPhrase::Lead(_) => phrase.clone(),
            // Iter-2.3
            // Returning Endpoint::Name(Label/Bus/List) as-is would leave the
            // V1V2 formal parameter in `V1V2 => CAP(...)` unsubstituted: the func
            // body could
            // connected to power".
            //
            // Fix: for Endpoint of Label/Bus/List types, try to run substitute_node_element.
            // Component/Module/Interface are "already declared concrete instances", formal params
            // should not override them, keep as-is.
            //
            // self-face substitution
            // Replace "this" / "pins" (and their `.xxx` / `{a, b}` tails) with the
            // caller instance bus ("caller_inst_name" / "caller_inst_name.xxx" /
            // "caller_inst_name{a, b}").
            McPhrase::Endpoint(McRef::Name(McInstanceRef {
                base: McInstance::Label(s),
                ..
            })) => {
                let is_self_ref = Self::self_ref_keyword(s).is_some();
                let mut elem = McBus::new(s);

                // BARE self face: the component's own default 1×2 face. In a body
                // chain `net1 - this - net2` the instance is vector-evaluated
                // against its pins (user rule): net1 → this.pin1, this.pin2 →
                // net2. A plain `McInstance::Bus(inst_name)` endpoint would make
                // get_left_points/get_right_points resolve to the instance NODE
                // (shorting net1 and net2 into one net); an
                // `McInstance::Component` reference resolves to the component's
                // default face (left pin1 / right pin2) instead.
                if Self::self_ref_keyword(s) == Some(s) {
                    if let Some(ctx) = expansion_ctx {
                        let comp = McInstance::Component(std::sync::Arc::new(
                            crate::semantic::component::Mc2Component::new(
                                &ctx.instance.name,
                                ctx.instance.def.clone(),
                            ),
                        ));
                        return McPhrase::Endpoint(McRef::Name(McInstanceRef::new(comp)));
                    }
                }

                // Check whether it's a self-face reference
                if let Some(ctx) = expansion_ctx {
                    if is_self_ref {
                        elem = Self::self_ref_to_bus(s, ctx);
                    }
                }

                let substituted = Self::substitute_node_element(&elem, bindings, cx);
                if substituted.is_empty() {
                    phrase.clone()
                } else if substituted.len() == 1
                    && substituted[0].name == elem.name
                    && substituted[0].member.is_empty()
                    && !is_self_ref
                {
                    // No substitution hit for a non-self label, return as-is
                    phrase.clone()
                } else {
                    // Substitution hit (or a self-face reference resolved to the
                    // caller instance bus): merge into a Bus endpoint.
                    let bus = Self::node_elements_to_bus(&substituted);
                    McPhrase::Endpoint(McRef::Name(McInstanceRef::new(McInstance::Bus(bus))))
                }
            }
            McPhrase::Endpoint(McRef::Name(McInstanceRef {
                base: McInstance::Bus(ref b),
                ..
            })) => {
                // Check whether the Bus name is a self-face reference
                let is_self_ref = Self::self_ref_keyword(&b.name).is_some();
                let mut bus_name = b.name.clone();
                let mut self_members: Option<Vec<String>> = None;
                if let Some(ctx) = expansion_ctx {
                    if is_self_ref {
                        let bus = Self::self_ref_to_bus(&b.name, ctx);
                        bus_name = bus.name;
                        self_members = Some(bus.member);
                    }
                }

                let elem = McBus::new_with_members(
                    &bus_name,
                    self_members.unwrap_or_else(|| b.member.clone()),
                );
                let substituted = Self::substitute_node_element(&elem, bindings, cx);
                if substituted.is_empty() {
                    phrase.clone()
                } else if substituted.len() == 1
                    && substituted[0].name == bus_name
                    && substituted[0].member == b.member
                    && !is_self_ref
                {
                    // No substitution hit for a non-self bus, return as-is
                    phrase.clone()
                } else {
                    let bus = Self::node_elements_to_bus(&substituted);
                    McPhrase::Endpoint(McRef::Name(McInstanceRef::new(McInstance::Bus(bus))))
                }
            }
            McPhrase::Endpoint(McRef::Name(McInstanceRef {
                base: McInstance::List(ref l),
                ..
            })) => {
                // List does not process this substitution (List form e.g. GPIO[1,2])
                let elem = McBus::new_with_members(&l.name, l.member.clone());
                let substituted = Self::substitute_node_element(&elem, bindings, cx);
                if substituted.len() == 1
                    && substituted[0].name == l.name
                    && substituted[0].member == l.member
                {
                    phrase.clone()
                } else if substituted.is_empty() {
                    phrase.clone()
                } else {
                    let bus = Self::node_elements_to_bus(&substituted);
                    McPhrase::Endpoint(McRef::Name(McInstanceRef::new(McInstance::Bus(bus))))
                }
            }
            McPhrase::Endpoint(McRef::Name(McInstanceRef {
                base: McInstance::Component(_),
                ..
            }))
            | McPhrase::Endpoint(McRef::Name(McInstanceRef {
                base: McInstance::Module(_),
                ..
            }))
            | McPhrase::Endpoint(McRef::Name(McInstanceRef {
                base: McInstance::Interface(_),
                ..
            })) => phrase.clone(),
            McPhrase::Multiple(phrases) => McPhrase::Multiple(
                phrases
                    .iter()
                    .map(|p| Self::substitute_phrase(p, bindings, expansion_ctx, cx))
                    .collect(),
            ),
            McPhrase::Endpoint(McRef::Ports {
                ref left,
                ref right,
                ..
            }) => {
                // U308 ruling B: `{a | b}`'s two sides are the **value** face's
                // port lists, not a per-member walk of the spelling side. The
                // old read drove every member through the cascade law
                // (`get_left`/`get_right`), a second, divergent copy of the
                // width-aligned view the shape layer already owns.
                let shape = OpdShape::of(phrase, cx);
                let new_l: Vec<McBus> = shape.port_left();
                let new_r: Vec<McBus> = shape.port_right();
                let left_elems: Vec<McBus> = left.iter().flat_map(|e| e.get_left()).collect();
                let right_elems: Vec<McBus> = right.iter().flat_map(|e| e.get_right()).collect();
                let names = |bs: &[McBus]| -> Vec<String> {
                    bs.iter().map(|b| b.name.clone()).collect()
                };
                if names(&left_elems) != names(&new_l) || names(&right_elems) != names(&new_r) {
                    eprintln!(
                        "[U308-TRACE] subst Ports DIVERGE L {:?} -> {:?} R {:?} -> {:?} \
                         phrase={phrase}",
                        names(&left_elems),
                        names(&new_l),
                        names(&right_elems),
                        names(&new_r)
                    );
                }
                // Iter-2.3
                // Also perform formal-parameter substitution on the Ports' left/right McBus
                let left_subst = Self::substitute_node_elements(&left_elems, bindings, cx);
                let right_subst = Self::substitute_node_elements(&right_elems, bindings, cx);
                if left_subst.is_empty() && right_subst.is_empty() {
                    McPhrase::Endpoint(McRef::Ports {
                        left: vec![],
                        right: vec![],
                    })
                } else if left_subst.is_empty() {
                    let right_bus = Self::node_elements_to_bus(&right_subst);
                    McPhrase::Endpoint(McRef::Ports {
                        left: vec![],
                        right: vec![McRef::Name(McInstanceRef::new(McInstance::Bus(
                            right_bus.clone(),
                        )))],
                    })
                } else if right_subst.is_empty() {
                    let left_bus = Self::node_elements_to_bus(&left_subst);
                    McPhrase::Endpoint(McRef::Ports {
                        left: vec![McRef::Name(McInstanceRef::new(McInstance::Bus(
                            left_bus.clone(),
                        )))],
                        right: vec![],
                    })
                } else {
                    let left_bus = Self::node_elements_to_bus(&left_subst);
                    let right_bus = Self::node_elements_to_bus(&right_subst);
                    McPhrase::Endpoint(McRef::Ports {
                        left: vec![McRef::Name(McInstanceRef::new(McInstance::Bus(
                            left_bus,
                        )))],
                        right: vec![McRef::Name(McInstanceRef::new(McInstance::Bus(
                            right_bus,
                        )))],
                    })
                }
            }
            McPhrase::Endpoint(ref ep) => McPhrase::Endpoint(ep.clone()),
            McPhrase::Member(phrase, ep) => McPhrase::Member(
                Box::new(Self::substitute_phrase(phrase, bindings, expansion_ctx, cx)),
                ep.clone(),
            ),
        }
    }

    /// Substitute formal parameters in an McPhrase (delegates to substitute_phrase)
    pub(super) fn substitute_stmt(
        phrase: &McPhrase,
        bindings: &McParamBindings,
        expansion_ctx: Option<&ExpansionContext>,
        cx: &dyn ShapeCtx,
    ) -> McPhrase {
        Self::substitute_phrase(phrase, bindings, expansion_ctx, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::basic::mc_literal::{McLiteral, McString};
    use crate::semantic::component::mc_attr::{McAttrVal, McAttribute};

    /// An empty shape scope. The `InlineAttrs` arm answers before any shape
    /// question is asked, so the context under test holds no instance — the
    /// cell would fail loudly if the conversion ever started reading one.
    #[derive(Default)]
    struct NoScope {
        uri: crate::McURI,
    }

    impl ShapeCtx for NoScope {
        fn find_inst(&self, _id: &str) -> Option<McInstance> {
            None
        }
        fn uri(&self) -> &crate::McURI {
            &self.uri
        }
    }

    /// P1-6 regression: an InlineAttrs argument (attribute block, e.g.
    /// `foo { key = value }`) must NOT be fabricated into a bogus text node
    /// name like `[key = value]`. Attributes are not net elements, so the
    /// conversion must yield no node while reporting the degradation.
    #[test]
    fn mat_subst__inline_attrs_param_value_is_not_degraded_to_text_node() {
        let attr = McAttribute {
            no: 0,
            id: McIds::from("color"),
            values: vec![McAttrVal::AttrLiteral(McLiteral::String(McString {
                value: "red".to_string(),
            }))],
            key_span: None,
            pins_ids: None,
        };
        let value = McParamValue::InlineAttrs(vec![attr]);
        let elems = InstantiationBuilder::param_value_to_node_elements(&value, &NoScope::default());
        assert!(
            elems.is_empty(),
            "InlineAttrs must not become a net node, got {elems:?}"
        );
    }
}
