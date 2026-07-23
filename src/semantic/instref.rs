// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_error;
use crate::{
    ast::ast_node::AstNode,
    semantic::{
        basic::mc_bus::McBus,
        basic::mc_endpoint::{McEndpoint, McInstanceRef},
        basic::mc_ids::McIds,
        basic::mc_phrase::McPhrase,
        mc_func::HasFindInst,
        mc_inst::McInstance,
    },
};

/// Extract member aliases from a composite pin name.
///
/// Supports `[Vin, GND]` / `{VDD, GND}` / `DC{Vin, GND}` / `IVCC5I[VCC5I,GNDP]`。
/// Skips segments with containing colon (`:`) or underscore (`_`).
fn extract_bracket_members(key: &str) -> Vec<String> {
    let (open, close) = if key.contains('{') {
        ('{', '}')
    } else if key.contains('[') {
        ('[', ']')
    } else {
        return Vec::new();
    };
    let start = match key.find(open) {
        Some(i) => i + 1,
        None => return Vec::new(),
    };
    let end = match key.rfind(close) {
        Some(i) if i > start => i,
        _ => return Vec::new(),
    };
    key[start..end]
        .split(',')
        .map(|s| s.trim())
        .filter(|s| {
            !s.is_empty()
                && !s.contains(':')
                && s.chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_')
        })
        .map(|s| s.to_string())
        .collect()
}

fn validate_inst_member_ref(
    base_name: &str,
    members: &[String],
    context: &mut dyn HasFindInst,
    node: &AstNode,
) -> Option<McPhrase> {
    if let Some(inst) = context.find_inst(base_name) {
        match &inst {
            McInstance::Component(comp) => {
                return validate_component_pin_ref(base_name, members, comp, context, node);
            }
            McInstance::Module(module) => {
                return validate_module_port_ref(base_name, members, module, context, node);
            }
            McInstance::Bus(_)
            | McInstance::Label(_)
            | McInstance::List(_)
            | McInstance::Unresolved { .. } => {
                let phrases: Vec<McPhrase> = members
                    .iter()
                    .map(|m| {
                        let member_ref = McBus::member_ref(base_name, m.clone());
                        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Bus(
                            member_ref,
                        ))))
                    })
                    .collect();

                return Some(McPhrase::series(phrases));
            }
            McInstance::Interface(iface) => {
                return validate_interface_member_ref(base_name, members, iface, context, node);
            }
            McInstance::BusRef { .. } => {
                dlog_error(
                    1702,
                    node,
                    &format!(
                        "Cannot access members on interface '{base_name}' using curly bracket syntax"
                    ),
                );
                return None;
            }
        }
    }

    None
}

fn validate_component_pin_ref(
    base_name: &str,
    members: &[String],
    comp: &crate::semantic::component::Mc2Component,
    context: &mut dyn HasFindInst,
    node: &AstNode,
) -> Option<McPhrase> {
    let valid_pin_names: Vec<&String> = comp.base.pins.names_to_id.keys().collect();
    let pin_id_to_names = &comp.base.pins.pin_id_to_names;
    // ★ FIX (Issue #1801):
    // Accepts pin ids from raw `comp.base.pins.pins` table, so that `mic{1, 2}` can be accessed if `pin_id_to_names`
    // is not filled yet.
    let pins_map = &comp.base.pins.pins;

    // ── P0-1 (Iter-10/12): Extract user aliases from Interface declaration ──────────
    //
    // When `[4,2] = [Vin, GND]::DC(2.5V~5.5V)`, init_pins may not have registered
    // interface pins yet, leading to empty `iface_pins` list.
    // This results in `derive_interface_subnames` returning empty list,
    // and `register_pin` never being called.
    // Thus, "Vin"/"GND" are not found in `names_to_id`, `pin_id_to_names`, or `pins`.
    //
    // Last line of defense: Scan all Interface entries in names_to_id,
    // extract bus/list members from `name` field (user aliases),
    // build { user alias → Interface entry key } map.
    // When a match is found, convert user alias to "Interface entry key.member" form,
    // so that connection generation can hit registered paths.
    use crate::semantic::component::mc_pins::McPinPort;
    let mut iface_alias_to_key: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (key, port) in comp.base.pins.names_to_id.iter() {
        if let McPinPort::Interface(iface) = port {
            // Extract user aliases from Interface declaration: curly form DC{Vin, GND} or list form [Vin, GND]
            if let Some((_, user_members)) = iface.name.as_bus() {
                for m in &user_members {
                    iface_alias_to_key.insert(m.clone(), key.clone());
                }
            } else if iface.name.is_list() {
                let expanded = iface.name.expand();
                for m in &expanded {
                    iface_alias_to_key.insert(m.clone(), key.clone());
                }
            }
            // Also try to extract aliases from registered_pins
            for rp_name in &iface.registered_pins {
                // registered_pins values are in "InterfaceName.PinName" form
                // Extract last segment as alias
                if let Some(dot_pos) = rp_name.rfind('.') {
                    let alias = &rp_name[dot_pos + 1..];
                    if !alias.is_empty() {
                        iface_alias_to_key
                            .entry(alias.to_string())
                            .or_insert_with(|| key.clone());
                    }
                }
            }
        }
    }

    // ── P0-2: Extract member aliases from composite KEY names ──
    for key in comp.base.pins.names_to_id.keys() {
        for alias in extract_bracket_members(key) {
            iface_alias_to_key
                .entry(alias)
                .or_insert_with(|| key.clone());
        }
    }

    // ── D1: SORT_HAZARD detection ──────────────────────────────────────────
    // Check if members match a Bus entry's full_members in names_to_id.
    // When pin numbers are non-monotonic (e.g. [5,2]=VOUT{Vout,GND}),
    // the sorted BTreeMap may cause member→pin mapping to be incorrect.
    let mut bus_member_hit: Option<&String> = None;
    for (bus_name, port) in comp.base.pins.names_to_id.iter() {
        if let McPinPort::Bus(bus) = port {
            let member_set: std::collections::BTreeSet<&String> = members.iter().collect();
            let full_set: std::collections::BTreeSet<&String> = bus.full_members.iter().collect();
            if member_set == full_set {
                bus_member_hit = Some(bus_name);
                // Look up pin numbers for each member in declaration order
                let mut pin_ids: Vec<String> = Vec::new();
                for member in members {
                    let full_name = format!("{bus_name}.{member}");
                    if let Some(McPinPort::Single(pid)) = comp.base.pins.names_to_id.get(&full_name)
                    {
                        pin_ids.push(pid.clone());
                    }
                }
                // Check if pin numbers are non-monotonic (not in ascending numeric order)
                if pin_ids.len() == members.len() && pin_ids.len() >= 2 {
                    let sorted: Vec<String> = {
                        let mut s = pin_ids.clone();
                        s.sort_by(|a, b| {
                            let na: i64 = a.parse().unwrap_or(0);
                            let nb: i64 = b.parse().unwrap_or(0);
                            na.cmp(&nb)
                        });
                        s
                    };
                    if pin_ids != sorted {
                        let binding: Vec<String> = members
                            .iter()
                            .zip(pin_ids.iter())
                            .map(|(m, pid)| format!("{m}→pin{pid}"))
                            .collect();
                        dlog_error(
                            2001,
                            node,
                            &format!(
                                "SORT_HAZARD: pin numbers in component '{}' bus '{}' are non-monotonic. \
                                 Member→pin binding: [{}]. Pin declaration order differs from member order, \
                                 which may cause incorrect mapping after sorting.",
                                base_name, bus_name, binding.join(", ")
                            ),
                        );
                    }
                }
                break;
            }
        }
    }

    // Whether all pin information is completely unavailable (three maps are empty).
    // This usually happens when CMIE parsing fails / system library is not loaded,
    // leading to empty base stub.
    // In this case, we treat `Bus | Label | List` pins loosely to avoid triggering 1801/1803 double errors.
    let pins_unavailable = valid_pin_names.is_empty()
        && pin_id_to_names.is_empty()
        && pins_map.is_empty()
        && iface_alias_to_key.is_empty();
    let mut valid_members: Vec<String> = Vec::new();
    let mut invalid_members: Vec<String> = Vec::new();

    for member in members {
        if comp.find_pin(member).is_some() {
            valid_members.push(member.clone());
        } else if member == "NC" {
            valid_members.push(member.clone());
        } else if pin_id_to_names.contains_key(member) {
            valid_members.push(member.clone());
        } else if pins_map.contains_key(member) {
            // Direct hit on raw pin id BTreeMap, e.g. `mic{1, 2}` -> pins["1"], pins["2"]
            valid_members.push(member.clone());
        } else if iface_alias_to_key.contains_key(member) {
            // ── P0-1: Interface user alias hit ──
            // E.g. "Vin" is alias in DC declaration `[Vin, GND]::DC()`
            valid_members.push(member.clone());
        } else if bus_member_hit.is_some() {
            // ── D1: Bus member hit ──
            // Members match a Bus entry's full_members (e.g. LDO{Vout,GND} matching VOUT{Vout,GND})
            valid_members.push(member.clone());
        } else if pins_unavailable {
            // When all pin information is unavailable, treat `Bus | Label | List` pins loosely.
            valid_members.push(member.clone());
        } else {
            invalid_members.push(member.clone());
        }
    }

    if !invalid_members.is_empty() {
        let bus_members: Vec<&str> = bus_member_hit
            .and_then(|bn| comp.base.pins.names_to_id.get(bn))
            .and_then(|p| match p {
                McPinPort::Bus(b) => Some(
                    b.full_members
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let all_valid: Vec<&str> = valid_pin_names
            .iter()
            .map(|s| s.as_str())
            .chain(pin_id_to_names.keys().map(|s| s.as_str()))
            .chain(pins_map.keys().map(|s| s.as_str()))
            .chain(iface_alias_to_key.keys().map(|s| s.as_str()))
            .chain(bus_members.into_iter())
            .collect();
        let all_valid_str = all_valid.join(", ");
        dlog_error(
            1801,
            node,
            &format!(
                "Pin(s) '{}' not found in component '{}'. Available pins: [{}]",
                invalid_members.join(", "),
                base_name,
                all_valid_str
            ),
        );
        if valid_members.is_empty() {
            return None;
        }
    }

    if valid_members.len() == 1 {
        let full_name = format!("{}.{}", base_name, valid_members[0]);
        let member_ref = McBus::member_ref(base_name, valid_members[0].clone());

        if let Some(existing_inst) = context.find_inst(&full_name) {
            if matches!(existing_inst, McInstance::Bus(_)) {
                return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    existing_inst.clone(),
                ))));
            }
        }

        let inst_ref = McInstanceRef {
            base: McInstance::Bus(member_ref),
            members: Vec::new(),
        };

        return Some(McPhrase::Endpoint(McEndpoint::Single(inst_ref)));
    }

    let inst_ref = McInstanceRef {
        base: McInstance::Bus(McBus::new_with_members(base_name, valid_members.clone())),
        members: Vec::new(),
    };

    Some(McPhrase::Endpoint(McEndpoint::Single(inst_ref)))
}

fn validate_module_port_ref(
    base_name: &str,
    members: &[String],
    module: &crate::semantic::module::Mc2Module,
    _context: &mut dyn HasFindInst,
    node: &AstNode,
) -> Option<McPhrase> {
    let port_names = module.base.insts.get_all_names();

    // eprintln!("[VAL-MOD] inst='{}' requested={:?} all_ports={:?}",
    //       base_name, members, port_names);

    let valid_port_names: Vec<&String> = port_names.iter().collect();
    let mut valid_members: Vec<String> = Vec::new();
    let mut invalid_members: Vec<String> = Vec::new();

    for member in members {
        if valid_port_names.contains(&member) {
            valid_members.push(member.clone());
        } else {
            invalid_members.push(member.clone());
        }
    }

    if !invalid_members.is_empty() {
        let all_valid = port_names
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        dlog_error(
            1704,
            node,
            &format!(
                "Port(s) '{}' not found in module '{}'. Available ports: [{}]",
                invalid_members.join(", "),
                base_name,
                all_valid
            ),
        );
        if valid_members.is_empty() {
            return None;
        }
    }

    if valid_members.len() == 1 {
        let member_ref = McBus::member_ref(base_name, valid_members[0].clone());
        let inst_ref = McInstanceRef {
            base: McInstance::Bus(member_ref),
            members: Vec::new(),
        };

        return Some(McPhrase::Endpoint(McEndpoint::Single(inst_ref)));
    }

    let inst_ref = McInstanceRef {
        base: McInstance::Bus(McBus::new_with_members(base_name, valid_members.clone())),
        members: Vec::new(),
    };

    Some(McPhrase::Endpoint(McEndpoint::Single(inst_ref)))
}

fn validate_interface_member_ref(
    base_name: &str,
    members: &[String],
    iface: &crate::semantic::mc_ifs::Mc2Interface,
    _context: &mut dyn HasFindInst,
    node: &AstNode,
) -> Option<McPhrase> {
    // ★ FIX (Issue #1804):
    // For port declarations like `MIC{P, N}::ADC.DIFF()`, the user explicitly
    // lists the bus member names (P, N) used in actual wiring in `MIC{P, N}`. These member names
    // are stored in `iface.name: McIds`'s curly bracket segment, and do not exist in `iface.base.pins`
    // (the latter are pins defined by the interface type `ADC.DIFF` itself — may be empty, or use different naming).
    //
    // Therefore when validating references like `MIC{P, N}`, we must accept both the pin names
    // defined by the underlying Interface and the bus member names explicitly declared by the user
    // in the port declaration; the union of both is the set of actually accessible members for this port.
    let mut pin_names: Vec<String> = iface.base.pins.names_to_id.keys().cloned().collect();
    if let Some((_, declared_members)) = iface.name.as_bus() {
        for m in declared_members {
            if !pin_names.contains(&m) {
                pin_names.push(m);
            }
        }
    }
    // The entire Interface has no pin info at all (both base and user-declared bus are empty).
    // In this stub situation, be lenient and fall back to Bus member reference, avoiding 1804/1803 double error.
    let pins_unavailable = pin_names.is_empty();
    let valid_pin_names: Vec<&String> = pin_names.iter().collect();
    let mut valid_members: Vec<String> = Vec::new();
    let mut invalid_members: Vec<String> = Vec::new();

    for member in members {
        if valid_pin_names.contains(&member) {
            valid_members.push(member.clone());
        } else if pins_unavailable {
            valid_members.push(member.clone());
        } else {
            invalid_members.push(member.clone());
        }
    }

    if !invalid_members.is_empty() {
        let all_valid = pin_names
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        dlog_error(
            1706,
            node,
            &format!(
                "Pin(s) '{}' not found in interface '{}'. Available pins: [{}]",
                invalid_members.join(", "),
                base_name,
                all_valid
            ),
        );
        if valid_members.is_empty() {
            return None;
        }
    }

    if valid_members.len() == 1 {
        let member_ref = McBus::member_ref(base_name, valid_members[0].clone());
        let inst_ref = McInstanceRef {
            base: McInstance::Bus(member_ref),
            members: Vec::new(),
        };

        return Some(McPhrase::Endpoint(McEndpoint::Single(inst_ref)));
    }

    let inst_ref = McInstanceRef {
        base: McInstance::Bus(McBus::new_with_members(base_name, valid_members.clone())),
        members: Vec::new(),
    };

    Some(McPhrase::Endpoint(McEndpoint::Single(inst_ref)))
}

pub fn validate_inst_reference(
    ids: &McIds,
    context: &mut dyn HasFindInst,
    node: &AstNode,
) -> Option<McPhrase> {
    if !ids.is_curly_bracket() {
        return None;
    }

    // ── P1 fix: 3-segment form `component.interface{members}` (e.g. uC.ADC{P,N}) ──
    // as_bus() only recognizes 2-segment `name{members}`, returns None for 3-segment, so this reference
    // previously fell into mc_phrase.rs's as_component_member branch → called
    // McModule::add_interface_member → returned None → entire line (including MIC->cap->ADC)
    // was dropped. Here we intercept the 3-segment form before as_bus, using the component's own pin table
    // to resolve interface sub-pins (ADC.P / ADC.N), going through the same
    // validate_* system as mic{1,2}/MIC{P,N}, not depending on module-side add_interface_member.
    if ids.as_bus().is_none() {
        if let Some((component, interface, members)) = ids.as_component_member() {
            if context.find_inst(&component).is_some() {
                return validate_component_interface_ref(
                    &component, &interface, &members, context, node,
                );
            }
        }
    }

    let (base_name, members) = match ids.as_bus() {
        Some((name, members)) => (name, members),
        None => return None,
    };

    let _kind = match context.find_inst(&base_name) {
        Some(crate::semantic::mc_inst::McInstance::Module(_)) => "Module",
        Some(crate::semantic::mc_inst::McInstance::Component(_)) => "Component",
        Some(crate::semantic::mc_inst::McInstance::Label(_)) => "Label",
        Some(crate::semantic::mc_inst::McInstance::Bus(_)) => "Bus",
        Some(crate::semantic::mc_inst::McInstance::Interface(_)) => "Interface",
        Some(crate::semantic::mc_inst::McInstance::List(_)) => "List",
        Some(crate::semantic::mc_inst::McInstance::Unresolved { .. }) => "Unresolved",
        Some(_) => "Other",
        None => "NotFound",
    };
    // eprintln!("[VAL-INST] base='{}' members={:?} kind={}", base_name, members, kind);

    context.find_inst(&base_name)?;

    // eprintln!("[VAL-INST]   result_is_some={}", result.is_some());
    validate_inst_member_ref(&base_name, &members, context, node)
}

/// P1: Resolve `component.interface{members}` (e.g. `uC.ADC{P, N}`).
///
/// Same system as `validate_component_pin_ref`, but member names must be resolved under
/// the interface namespace: for each member `m`, first try combined pin name `interface.m` (e.g. "ADC.P").
///
/// ★ Output form (critical): each member produces a **full-path, empty-member** Bus
/// (`Bus(name="uC.ADC.P", member=[])`), wrapped in `Multiple`. This way downstream
/// `points.rs::node_to_netpoint` will split_once('.') on `uC.ADC.P` →
/// find_component("uC") → P7 block (points.rs:881) using component pin name table to
/// resolve "ADC.P"/"P" into physical pin id (uC.16).
///
/// Cannot use `Bus(name="uC.ADC", member=["P","N"])`: "uC.ADC" is neither a component nor
/// a port, would be ensure_bus'd into a fake bus by get_member_points's is_owned=false branch,
/// and "uC.ADC.P" wouldn't go through node_to_netpoint → P7, can't resolve to physical pin.
/// Cannot use `Bus(name="uC", member=["ADC.P","ADC.N"])` either: is_owned=true branch
/// (points.rs:471) directly with_owner bypasses node_to_netpoint, also skipping P7.
fn validate_component_interface_ref(
    component: &str,
    interface: &str,
    members: &[String],
    context: &mut dyn HasFindInst,
    node: &AstNode,
) -> Option<McPhrase> {
    let inst = context.find_inst(component)?;
    let comp = match &inst {
        McInstance::Component(c) => c.clone(),
        // Theoretically only Component has interface pins; other types keep old behavior (return to caller)
        _ => return None,
    };

    let valid_pin_names: Vec<&String> = comp.base.pins.names_to_id.keys().collect();
    let pin_id_to_names = &comp.base.pins.pin_id_to_names;
    let pins_map = &comp.base.pins.pins;

    // Whether component pin info is completely missing (system lib not loaded / CMIE failed stub).
    let pins_unavailable =
        valid_pin_names.is_empty() && pin_id_to_names.is_empty() && pins_map.is_empty();

    // Whether the interface pin itself is in the pin table (e.g. "ADC" registered as ADC.DIFF interface pin).
    // Interface type (ADC.DIFF) sub-pins P/N may not be individually registered (when interface definition not loaded),
    // but as long as the interface pin ADC is in the table (whole name "ADC", or sub-pins with "ADC." prefix),
    // consider ADC{P,N} members valid — downstream node_to_netpoint's P7 will try its best to resolve,
    // and if unresolvable, degrades to stable "uC.ADC.P" path, connection still works.
    let iface_dot_prefix = format!("{interface}.");
    let interface_pin_known = valid_pin_names.iter().any(|pn| pn.as_str() == interface)
        || valid_pin_names
            .iter()
            .any(|pn| pn.starts_with(&iface_dot_prefix))
        || pins_map
            .keys()
            .any(|k| k.as_str() == interface || k.starts_with(&iface_dot_prefix))
        || pin_id_to_names
            .keys()
            .any(|k| k.as_str() == interface || k.starts_with(&iface_dot_prefix));

    let mut valid_members: Vec<String> = Vec::new();
    let mut invalid_members: Vec<String> = Vec::new();

    for m in members {
        // Combined pin name: interface.member (e.g. "ADC.P")
        let combined = format!("{interface}.{m}");
        let hit = valid_pin_names.iter().any(|pn| pn.as_str() == combined)
            || pin_id_to_names.contains_key(&combined)
            || pins_map.contains_key(&combined)
            // Also accept bare member name (some interfaces register sub-pins as P/N directly rather than ADC.P)
            || valid_pin_names.iter().any(|pn| pn.as_str() == m.as_str())
            || pin_id_to_names.contains_key(m)
            || pins_map.contains_key(m)
            || m == "NC";
        if hit || interface_pin_known || pins_unavailable {
            valid_members.push(m.clone());
        } else {
            invalid_members.push(m.clone());
        }
    }

    if !invalid_members.is_empty() {
        let all_valid: Vec<&str> = valid_pin_names
            .iter()
            .map(|s| s.as_str())
            .chain(pin_id_to_names.keys().map(|s| s.as_str()))
            .chain(pins_map.keys().map(|s| s.as_str()))
            .collect();
        dlog_error(
            1801,
            node,
            &format!(
                "Pin(s) '{}' not found in interface '{}.{}'. Available pins: [{}]",
                invalid_members.join(", "),
                component,
                interface,
                all_valid.join(", ")
            ),
        );
        if valid_members.is_empty() {
            return None;
        }
    }

    // Each member → full-path empty-member Bus, goes through node_to_netpoint's P7 physical pin resolution.
    let phrases: Vec<McPhrase> = valid_members
        .iter()
        .map(|m| {
            let full_path = format!("{component}.{interface}.{m}");
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Bus(McBus::new(&full_path)),
                members: Vec::new(),
            }))
        })
        .collect();

    if phrases.len() == 1 {
        return Some(phrases.into_iter().next().unwrap());
    }
    Some(McPhrase::Multiple(phrases))
}
