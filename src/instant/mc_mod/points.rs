// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Endpoint extraction
//!
//! Convert McPhrase to NetPoint list for `create_connection`.
//!
//! - `get_left_points` / `get_right_points`         —— single member left/right endpoints
//! - `get_left_points_from_phrase` / `_from_line`   —— entire line left/right endpoints
//! - `node_to_netpoint`                              —— single McBus → NetPoint
//! - `is_port` / `find_component` / `find_submodule` / `ensure_label` —— lookup helpers

use super::McModuleInst;
use crate::instant::mc_comp::McComponentInst;
use crate::instant::mc_net::{InstError, NetPoint};
use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::basic::mc_endpoint::{McEndpoint, McInstanceRef};
use crate::semantic::basic::mc_ids::McIds;
use crate::semantic::basic::mc_phrase::McPhrase;
use crate::semantic::common::IOType;
use crate::semantic::mc_inst::McInstance;

// ────────────────────────────────────────────────────────────────────────────
// Iter-1.1: member string IDA expansion
//
// Syntax like `uC.pins[8:11]` / `cap[4:5]` / `ADC{P,N}` results in
// `elements.member = ["pins[8:11]"]` **single literal member**.
// `expand_member_ida` expands these bracket range/list member strings into
// separate string lists (e.g., `pins[8:11]` → `["8","9","10","11"]`).
//
// Key conventions:
//   1. `pins[N:M]` / `pins.N` prefix `pins` keyword is stripped, keeping only index — because
//      component `init_pins()` registers pin path as `<inst>.<N>`, not `<inst>.pins.<N>`.
//   2. For non-pins-prefixed but bracket-range members (e.g., `X<1:2>`, `[A,B]`),
//      directly call `McIds::expand()` to expand.
//   3. for "no bracket" simple member names (e.g., `P`, `GND`), return `vec![m]` as-is.
// ────────────────────────────────────────────────────────────────────────────

/// Parse `wm7121{VCC}` → ("wm7121", "VCC"); single member curly selection.
/// Multi-member (`{P,N}`) not handled here (goes to upper bus expansion), returns None.
fn parse_curly_select(name: &str) -> Option<(String, String)> {
    let open = name.find('{')?;
    let close = name.find('}')?;
    if close <= open + 1 {
        return None;
    }
    let base = name[..open].to_string();
    let inner = &name[open + 1..close];
    if base.is_empty() || inner.contains(',') {
        return None;
    } // multi-member handed to bus expansion
    Some((base, inner.trim().to_string()))
}

fn expand_member_ida(member: &str) -> Vec<String> {
    // no brackets, return directly
    if !member.contains('[') {
        return vec![member.to_string()];
    }

    // try parsing and expanding with McIds
    let ids = McIds::from(member);
    let expanded = ids.expand();
    if expanded.is_empty() {
        return vec![member.to_string()];
    }

    // strip "pins" prefix (if present) — component pin paths use bare pin id
    use crate::instant::mc_net::normalize_pin_segments;
    expanded
        .into_iter()
        .map(|s| normalize_pin_segments(&s)) // P7: same rules as canonicalize_path
        .collect()
}

/// ── P2: bare bus/interface member alias → physical pid ────────────────────
/// **Bare** aliases like `ldo.Vout` / `ldo.GND` are not registered as top-level
/// `Single` entries in `names_to_id` (register_pin only records the dotted form
/// `"VOUT.Vout"`), so it and `ldo.5` become two different canonical strings and
/// the union-find never merges them (output cap floating / ground not shared /
/// EN floating).
///
/// Here we reverse-look up `pin_id_to_names` (pid -> [dotted name…], authoritative
/// and guaranteed to contain "VOUT.Vout"): find a pid whose "full name or last
/// segment" equals that alias.
fn resolve_bare_member_pid(
    pins: &crate::semantic::component::mc_pins::McPins,
    rest: &str,
    last: &str,
) -> Option<String> {
    let mut hits: Vec<String> = Vec::new();
    for (pid, names) in pins.pin_id_to_names.iter() {
        let matched = names.iter().any(|n| {
            let seg = n.rsplit('.').next().unwrap_or(n);
            n == rest || n == last || seg == rest || seg == last
        });
        if matched && !hits.contains(pid) {
            hits.push(pid.clone());
        }
    }
    match hits.len() {
        0 => None,
        1 => Some(hits.remove(0)),
        _ => {
            // ── [P2-AMBIG-PROBE] delete after verification: same alias lands on multiple pids ──
            // Indicates multiple buses in the source share a same-named member but different
            // physical pins (e.g., two GNDs). Take the smallest pid and print for manual
            // review to see if a more specific interface-segment spelling is needed.
            eprintln!(
                "[P2-AMBIG] alias rest={rest:?} last={last:?} -> pids {hits:?} (take smallest)"
            );
            hits.sort_by(|a, b| {
                a.parse::<i64>()
                    .unwrap_or(0)
                    .cmp(&b.parse::<i64>().unwrap_or(0))
            });
            Some(hits.remove(0))
        }
    }
}

impl McModuleInst {
    pub(super) fn get_left_points(
        &mut self,
        phrase: &McPhrase,
    ) -> Result<Vec<NetPoint>, InstError> {
        match phrase {
            McPhrase::Lead => {
                let name = format!("(lead)_{:x}", phrase as *const McPhrase as usize);
                Ok(vec![NetPoint::new(&name, IOType::None)])
            }

            McPhrase::Endpoint(McEndpoint::Single(iref))
                if matches!(iref.base, McInstance::Bus(_)) =>
            {
                let merged: McBus = iref.to_bus();
                let elements = &merged;

                if !elements.member.is_empty() {
                    // ── Iter-1.1 ─────────────────────────────────────────
                    // expand bracket-containing member literals (e.g., "pins[8:11]") to separate member list
                    let expanded_members: Vec<String> = elements
                        .member
                        .iter()
                        .flat_map(|m| expand_member_ida(m))
                        .collect();
                    let is_owned = !elements.name.is_empty()
                        && (self.find_submodule(&elements.name).is_some()
                            || self.find_component(&elements.name).is_some());
                    // ── Bug ② fix ───────────────────────────────────────
                    // only register as bus when name is not known component/submodule.
                    // if `uC{XTAL, ...}` "component instance + pin member" form gets
                    // ensure_bus into self.buses, downstream inst_table.rs step 4
                    // expands component pins to `uC/11` / `uC/XTAL` form Label.
                    if !elements.name.is_empty() && !is_owned {
                        self.ensure_bus(&elements.name, &expanded_members)?;
                    }
                    let mut points = Vec::new();
                    for m in &expanded_members {
                        let path = if elements.name.is_empty() {
                            m.clone()
                        } else {
                            format!("{}.{}", elements.name, m)
                        };
                        // ── P2: component pin alias → pid (normalize at construction so union sees pid) ──
                        let path = self.normalize_one_inst_pin_path(&path).unwrap_or(path);
                        // ── P2: if <owner>.<member> itself is bus port (members need further
                        //    expansion, e.g., usbsocket.vin{POWER_SYS,GND}), callback
                        //    expand_port_lanes to expand to lanes; component pins etc return None,
                        //    keep original single point unchanged.
                        if let Some(lanes) = self.expand_port_lanes(&path) {
                            points.extend(lanes);
                        } else if is_owned {
                            points.push(NetPoint::with_owner(&path, &elements.name, IOType::None));
                        } else {
                            points.push(NetPoint::new(&path, IOType::None));
                        }
                    }
                    return Ok(points);
                }

                // ── Iter-8 ───────────────────────────────────────────────
                // Port N×1 bus expansion: when phrase's own member is empty, check if
                // elements.name corresponds to N×1 port with declared members
                // (e.g., `UART0`, `mic.MIC`). If hit, expand by declared members to N
                // independent NetPoints, letting upper create_connection / process_member_internal
                // go through rule doc §10.4 "[N×1] vs [N×1]" zip per-element branch.
                //
                // No hit (scalar port / bare label / component pin) → fallthrough to
                // original node_to_netpoint path.
                if !elements.name.is_empty() {
                    if let Some(lanes) = self.expand_port_lanes(&elements.name) {
                        return Ok(lanes);
                    }
                }

                // handle McBus possibly with member
                let mut points = Vec::new();
                let node_elements: Vec<McBus> = Vec::from(elements.clone());
                for elem in node_elements {
                    if elem.member.is_empty() {
                        // no sub-members, direct conversion
                        points.push(self.node_to_netpoint(&elem));
                    } else {
                        // has sub-members, register as bus and expand (flattened: member is string list)
                        // ── Iter-1.1 ─────────────────────────────────────
                        let expanded_members: Vec<String> = elem
                            .member
                            .iter()
                            .flat_map(|m| expand_member_ida(m))
                            .collect();

                        // ── Bug ② fix ───────────────────────────────
                        // component/submodule instance names not registered as bus (see same-name fix above);
                        // on hit, endpoints generated in owner form.
                        let elem_owned = !elem.name.is_empty()
                            && (self.find_submodule(&elem.name).is_some()
                                || self.find_component(&elem.name).is_some());
                        if !elem.name.is_empty() && !elem_owned {
                            self.ensure_bus(&elem.name, &expanded_members)?;
                        }

                        // expand to each member's NetPoint
                        for m in &expanded_members {
                            let path = if elem.name.is_empty() {
                                m.clone()
                            } else {
                                format!("{}.{}", elem.name, m)
                            };
                            // ── P2: component pin alias → pid (normalize at construction so union sees pid) ──
                            let path = self.normalize_one_inst_pin_path(&path).unwrap_or(path);
                            if elem_owned {
                                points.push(NetPoint::with_owner(&path, &elem.name, IOType::None));
                            } else {
                                points.push(NetPoint::new(&path, IOType::None));
                            }
                        }
                    }
                }
                Ok(points)
            }

            McPhrase::Endpoint(McEndpoint::Node {
                ref input,
                ref output,
                ..
            }) => {
                let left_elems: Vec<McBus> = input.iter().flat_map(|e| e.get_left()).collect();
                let right_elems: Vec<McBus> = output.iter().flat_map(|e| e.get_right()).collect();
                self.resolve_curly_mn_points(&left_elems, &right_elems, true)
            }

            McPhrase::Parallel(phrases) => {
                // ── Iter-7.1 ──────────────────────────────────────────────
                // Rule §10.1: `+` takes operand 1. `A + B + C` exposed to outer chain
                // endpoints should only be opds[0]'s endpoints, **not all opds output**.
                //
                // Historically (Iter-5.C / refined) all opd left points were collected
                // into same list as "workaround to let non-first branches connect to upstream GND/power",
                // but side effects:
                //   1. (A + B) -> RES -> C, + incorrectly "attached to next branch"
                //      (bugfix_report error 12: TP1 connected to @RES1 instead of USB_VBUS)
                //   2. (C1nF + R10k) -> GND, C1nF/R10k each output .2 endpoint,
                //      relying on GND single-point broadcast coincidentally forming 2 nets, but
                //      middle node (operand 1's .1) has no proper internal net (errors 5, 9, 10)
                //   3. lpa.BYPASS + lpa.IN.P -> CAP -> GND directly shorts BYPASS
                //      / IN.P / CAP / GND all together (error 10)
                //
                // Fix: align with mc_phrase.rs::Parallel.get_left() (=opds[0].get_left()),
                // only expose opds[0] endpoints. **Internal pin1/pin2 pairing connections**
                // generated directly by line.rs::process_member_internal::Parallel
                // during instantiate phase (Iter-7.1 companion change C).
                if let Some(first) = phrases.first() {
                    match first {
                        McPhrase::FuncCall(_) => self.get_left_points(first),
                        _ => {
                            // ── BUG4 fix ──────────────────────────────────
                            // first may be Series([RES(30kΩ), lpa.VO1]) compound branch
                            // containing FuncCall (speaker periph.mc:97).
                            // _from_phrase for internal FuncCall reads bare RES.in placeholder →
                            // leaks @_phantom. prefer get_left_points (FuncCall goes through
                            // resolve_funccall_left_points querying auto_inst_map, paired with
                            // line.rs Parallel handler pointer-keeping fix hits);
                            // if empty, fallback to _from_phrase (Endpoint(Component) form).
                            let pts = self.get_left_points(first)?;
                            if pts.is_empty() {
                                self.get_left_points_from_phrase(first)
                            } else {
                                Ok(pts)
                            }
                        }
                    }
                } else {
                    Ok(Vec::new())
                }
            }

            McPhrase::Series(phrases) => {
                // P1-E2 companion: Parallel inner may nest Series (rare).
                // `get_left_points` originally returned empty for Series, changed to return **first**
                // sub phrase's left points (chain start), consistent with `_from_phrase`
                // semantics for Series.
                if let Some(first) = phrases.first() {
                    self.get_left_points(first)
                } else {
                    Ok(Vec::new())
                }
            }

            McPhrase::Transposed(inner_line) => {
                // ── P1 fix: Transposed(FuncCall) special handling ─────────────
                // FuncCall left/right are class name placeholders (e.g., "CAP.in"/"CAP.out"),
                // direct reading goes through node_to_netpoint → @_phantom_ path.
                // Must delegate to FuncCall resolve methods, which query auto_inst_map
                // to find real instance names (e.g., "@?CAP_3"), and filter class name placeholders through P0-4.B.
                if let McPhrase::FuncCall(ref f) = **inner_line {
                    let mut left_pts = self.resolve_funccall_left_points(inner_line, &f.left)?;
                    let right_pts = self.resolve_funccall_right_points(inner_line, &f.right)?;
                    // Transposed: merge left + right (all pins exposed on both sides)
                    left_pts.extend(right_pts);
                    return Ok(left_pts);
                }

                // non-FuncCall Transposed: original logic
                let original_left = inner_line.get_left();
                let original_right = inner_line.get_right();

                let mut all_points = Vec::new();
                for elem in &original_left {
                    all_points.extend(self.expand_node_element_to_points(elem)?);
                }
                for elem in &original_right {
                    all_points.extend(self.expand_node_element_to_points(elem)?);
                }
                Ok(all_points)
            }

            McPhrase::FuncCall(ref f) => self.resolve_funccall_left_points(phrase, &f.left),

            McPhrase::Closure(ref c) => {
                // Phase 3.3: closure's left interface is its parameter declaration
                // upstream member output ports passed into closure body via these parameter names
                // example: upstream => |dac, mute| { dac -> DAC; mute -> ctrl }
                //   upstream outputs [dac_out, mute_out] → closure parameters [dac, mute]
                if !c.params.is_empty() {
                    let points = c
                        .params
                        .iter()
                        .map(|p| {
                            let name = p.get_primary_name().unwrap_or_default();
                            self.ensure_label(&name);
                            NetPoint::new(&name, IOType::In)
                        })
                        .collect();
                    Ok(points)
                } else if let Some(first_line) = c.body.first() {
                    self.get_left_points_from_phrase(first_line)
                } else {
                    Ok(vec![])
                }
            }

            McPhrase::Group(ref g) => {
                // Group left endpoint handling
                // 1. recursively collect all branch left endpoints
                // 2. if left_match=true, all branches same shape, safe to broadcast
                // 3. if left_match=false, shapes differ, need warning

                if !g.left_match && g.opds.len() > 1 {
                    eprintln!(
                        "Warning: Group has inconsistent left shapes across branches, \
                        connection may not work as expected"
                    );
                }

                let mut points = Vec::new();
                for phrase in &g.opds {
                    // ── BUG4 fix (companion to line.rs Group handler pointer-keeping fix) ──
                    // prefer get_left_points: FuncCall in branches goes through
                    // resolve_funccall_left_points querying auto_inst_map. paired with line.rs
                    // "Group branch instantiated in-place (not cloned)", pointer matches → hits
                    // real @?TYPE_n pins, no longer leaks CAP.in/RES.in placeholders.
                    // if empty, fallback to _from_phrase: handles parse-time
                    // Endpoint(Component) (@CAP5_ep form, get_left_points returns empty,
                    // see this file Component branch + _from_phrase line comment).
                    let pts = self.get_left_points(phrase)?;
                    if pts.is_empty() {
                        points.extend(self.get_left_points_from_phrase(phrase)?);
                    } else {
                        points.extend(pts);
                    }
                }
                Ok(points)
            }

            // Def needs to be instantiated first, handled in process_member_internal
            //
            // ── Iter-10.D (bucket D) ─────────────────────────────────────────
            // However: this catch-all mixes "declare new instance (RES(100kΩ).Cap(...))" and
            // "reference existing instance (uC.UART0)" two syntax forms together. Former
            // indeed needs to instantiate first then associate endpoints via auto_inst_map; latter
            // is reference to existing component bus port, should immediately expand to N physical pin
            // lane.
            //
            // Fix here: after base=Component / Module hits, first try using iref
            // to_bus() to get candidate path, call expand_port_lanes — hit
            // (means component/submodule bus port reference) return expanded lane;
            // if not hit, fall back to original `Ok(vec![])` (Def path).
            //
            // Added diagnostic log [Iter-10.D-LP] outputs path / iref.left / iref.right
            // form, helps locate what specific phrase walked into this branch.
            McPhrase::Endpoint(McEndpoint::Single(
                iref @ McInstanceRef {
                    base: McInstance::Component(_),
                    ..
                },
            ))
            | McPhrase::Endpoint(McEndpoint::Single(
                iref @ McInstanceRef {
                    base: McInstance::Module(_),
                    ..
                },
            )) => {
                let bus = iref.to_bus();
                if !bus.name.is_empty() {
                    if let Some(lanes) = self.expand_port_lanes(&bus.name) {
                        return Ok(lanes);
                    }
                    if let Some(comp) = self.find_component(&bus.name) {
                        if let Some(pin) = comp.get_left_pin() {
                            return Ok(vec![pin]);
                        }
                    }
                }
                Ok(vec![])
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(_),
                ..
            })) => Ok(vec![]),
            McPhrase::Multiple(inner) => {
                let mut points = Vec::new();
                for p in inner {
                    points.extend(self.get_left_points(p)?);
                }
                Ok(points)
            }
            McPhrase::Endpoint(ref ep) => {
                let left = ep.get_left();
                let mut points = Vec::new();
                for bus in left {
                    points.push(self.node_to_netpoint(&bus));
                }
                Ok(points)
            }
            // ── Iter-12.1 (D-class fix): Member variant interface parsing ──────────
            //
            // Original code: `McPhrase::Member(phrase, _) => self.get_left_points(phrase)`
            // completely ignored member name — for `uC.i2c(0x36).I2C0` chain, `.I2C0`
            // was dropped, resulting in degradation to uC's generic left/right pin (pin 21 GND).
            //
            // Fix: extract member name, look up corresponding bus port through caller component,
            // return that interface's actual pin endpoints.
            //
            // ── Iter-12.1b: cross-module component lookup ──
            // after instantiate_instance_method prefixes function body, caller becomes
            // "mcu513.uC" (with module prefix). self (main) has no component named "mcu513.uC"
            // component. Need to split into sub_name="mcu513" + comp_name="uC",
            // look up in submodule's components.
            McPhrase::Member(phrase, member_ep) => {
                // try to extract member name
                let member_name = match member_ep {
                    McEndpoint::Single(ir) => match &ir.base {
                        McInstance::Label(s) => Some(s.clone()),
                        McInstance::Bus(b) if b.member.is_empty() => Some(b.name.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                // try to extract caller component name from inner phrase
                if let Some(ref mname) = member_name {
                    if let Some(caller) = Self::extract_caller_inst_name(phrase) {
                        // (A) direct component lookup (same module)
                        if let Some(comp) = self.find_component(&caller) {
                            if let Some(pids) = comp.find_bus_port_pin_ids(mname) {
                                return Ok(pids
                                    .iter()
                                    .map(|pid| {
                                        NetPoint::with_owner(
                                            &format!("{caller}.{pid}"),
                                            &caller,
                                            IOType::None,
                                        )
                                    })
                                    .collect());
                            }
                        }
                        // (B) cross-module component lookup: caller="mcu513.uC" → sub="mcu513", comp="uC"
                        if let Some((sub_name, comp_name)) = caller.split_once('.') {
                            if let Some(sub) = self.find_submodule(sub_name) {
                                if let Some(comp) =
                                    sub.components.iter().find(|c| c.name == comp_name)
                                {
                                    if let Some(pids) = comp.find_bus_port_pin_ids(mname) {
                                        return Ok(pids
                                            .iter()
                                            .map(|pid| {
                                                NetPoint::with_owner(
                                                    &format!("{caller}.{pid}"),
                                                    sub_name,
                                                    IOType::None,
                                                )
                                            })
                                            .collect());
                                    }
                                }
                            }
                        }
                        // (C) expand_port_lanes fallback
                        let qualified = format!("{caller}.{mname}");
                        if let Some(lanes) = self.expand_port_lanes(&qualified) {
                            return Ok(lanes);
                        }
                    }
                }
                // Fallback: delegate to inner phrase
                self.get_left_points(phrase)
            }
        }
    }

    pub(super) fn get_right_points(
        &mut self,
        member: &McPhrase,
    ) -> Result<Vec<NetPoint>, InstError> {
        match member {
            McPhrase::Lead => {
                let name = format!("(lead)_{:x}", member as *const McPhrase as usize);
                Ok(vec![NetPoint::new(&name, IOType::None)])
            }

            McPhrase::Endpoint(McEndpoint::Single(iref))
                if matches!(iref.base, McInstance::Bus(_)) =>
            {
                let merged: McBus = iref.to_bus();
                let elements = &merged;

                if !elements.member.is_empty() {
                    // ── Iter-1.1 ─────────────────────────────────────────
                    let expanded_members: Vec<String> = elements
                        .member
                        .iter()
                        .flat_map(|m| expand_member_ida(m))
                        .collect();
                    let is_owned = !elements.name.is_empty()
                        && (self.find_submodule(&elements.name).is_some()
                            || self.find_component(&elements.name).is_some());
                    // ── Bug ② fix (mirror get_left_points) ────────────────
                    // component/submodule instance names not registered as bus, avoids downstream inst_table
                    // expanding component pins to `<comp>/<pid>` form Label.
                    if !elements.name.is_empty() && !is_owned {
                        self.ensure_bus(&elements.name, &expanded_members)?;
                    }
                    let mut points = Vec::new();
                    for m in &expanded_members {
                        let path = if elements.name.is_empty() {
                            m.clone()
                        } else {
                            format!("{}.{}", elements.name, m)
                        };
                        // ── P2: component pin alias → pid (normalize at construction so union sees pid) ──
                        let path = self.normalize_one_inst_pin_path(&path).unwrap_or(path);
                        // ── P2: if <owner>.<member> itself is bus port (members need further
                        //    expansion, e.g., usbsocket.vin{POWER_SYS,GND}), callback
                        //    expand_port_lanes to expand to lanes; component pins etc return None,
                        //    keep original single point unchanged.
                        if let Some(lanes) = self.expand_port_lanes(&path) {
                            points.extend(lanes);
                        } else if is_owned {
                            points.push(NetPoint::with_owner(&path, &elements.name, IOType::None));
                        } else {
                            points.push(NetPoint::new(&path, IOType::None));
                        }
                    }
                    return Ok(points);
                }

                // ── Iter-8 ───────────────────────────────────────────────
                // Port N×1 bus expansion (mirror get_left_points same-name changes).
                // See Iter-8 comment above get_left_points.
                if !elements.name.is_empty() {
                    if let Some(lanes) = self.expand_port_lanes(&elements.name) {
                        return Ok(lanes);
                    }
                }

                // handle McBus possibly with member
                let mut points = Vec::new();
                let node_elements: Vec<McBus> = Vec::from(elements.clone());
                for elem in node_elements {
                    if elem.member.is_empty() {
                        // no sub-members, direct conversion
                        points.push(self.node_to_netpoint(&elem));
                    } else {
                        // has sub-members, register as bus and expand (flattened: member is string list)
                        // ── Iter-1.1 ─────────────────────────────────────
                        let expanded_members: Vec<String> = elem
                            .member
                            .iter()
                            .flat_map(|m| expand_member_ida(m))
                            .collect();

                        // ── Bug ② fix (mirror get_left_points) ───────────
                        let elem_owned = !elem.name.is_empty()
                            && (self.find_submodule(&elem.name).is_some()
                                || self.find_component(&elem.name).is_some());
                        if !elem.name.is_empty() && !elem_owned {
                            self.ensure_bus(&elem.name, &expanded_members)?;
                        }

                        // expand to each member's NetPoint
                        for m in &expanded_members {
                            let path = if elem.name.is_empty() {
                                m.clone()
                            } else {
                                format!("{}.{}", elem.name, m)
                            };
                            // ── P2: component pin alias → pid (normalize at construction so union sees pid) ──
                            let path = self.normalize_one_inst_pin_path(&path).unwrap_or(path);
                            if elem_owned {
                                points.push(NetPoint::with_owner(&path, &elem.name, IOType::None));
                            } else {
                                points.push(NetPoint::new(&path, IOType::None));
                            }
                        }
                    }
                }
                Ok(points)
            }

            McPhrase::Endpoint(McEndpoint::Node {
                ref input,
                ref output,
                ..
            }) => {
                let left_elems: Vec<McBus> = input.iter().flat_map(|e| e.get_left()).collect();
                let right_elems: Vec<McBus> = output.iter().flat_map(|e| e.get_right()).collect();
                self.resolve_curly_mn_points(&left_elems, &right_elems, false)
            }

            McPhrase::Parallel(phrases) => {
                // ── Iter-7.1 ──────────────────────────────────────────────
                // See get_left_points corresponding comment. Rule §10.1: `A + B + C`
                // right endpoints only expose opds[0].right. Internal pin2 pairing connection
                // generated by line.rs::process_member_internal::Parallel.
                if let Some(first) = phrases.first() {
                    match first {
                        McPhrase::FuncCall(_) => self.get_right_points(first),
                        _ => {
                            // ── BUG4 fix (mirror get_left_points Parallel branch) ──
                            let pts = self.get_right_points(first)?;
                            if pts.is_empty() {
                                self.get_right_points_from_phrase(first)
                            } else {
                                Ok(pts)
                            }
                        }
                    }
                } else {
                    Ok(Vec::new())
                }
            }

            McPhrase::Series(phrases) => {
                // P1-E2 companion: Series in get_right_points returns **last**
                // sub phrase's right points (chain endpoint).
                if let Some(last) = phrases.last() {
                    self.get_right_points(last)
                } else {
                    Ok(Vec::new())
                }
            }

            McPhrase::Transposed(inner_line) => {
                // ── P1 fix: mirror get_left_points Transposed(FuncCall) handling
                if let McPhrase::FuncCall(ref f) = **inner_line {
                    let mut left_pts = self.resolve_funccall_left_points(inner_line, &f.left)?;
                    let right_pts = self.resolve_funccall_right_points(inner_line, &f.right)?;
                    left_pts.extend(right_pts);
                    return Ok(left_pts);
                }

                let original_left = inner_line.get_left();
                let original_right = inner_line.get_right();
                let mut all_points = Vec::new();
                for elem in &original_left {
                    all_points.extend(self.expand_node_element_to_points(elem)?);
                }
                for elem in &original_right {
                    all_points.extend(self.expand_node_element_to_points(elem)?);
                }
                Ok(all_points)
            }

            McPhrase::FuncCall(ref f) => self.resolve_funccall_right_points(member, &f.right),

            McPhrase::Closure(ref c) => {
                Ok(c.right.iter().map(|e| self.node_to_netpoint(e)).collect())
            }

            McPhrase::Group(ref g) => {
                // Group right endpoint handling
                // 1. recursively collect all branch right endpoints
                // 2. if right_match=true, all branches same shape, safe to broadcast
                // 3. if right_match=false, shapes differ, need warning

                if !g.right_match && g.opds.len() > 1 {
                    eprintln!(
                        "Warning: Group has inconsistent right shapes across branches, \
                        connection may not work as expected"
                    );
                }

                let mut points = Vec::new();
                for phrase in &g.opds {
                    // ── BUG4 fix (mirror get_left_points Group branch) ───────
                    // prefer get_right_points (FuncCall goes through resolve_funccall_right_points
                    // querying auto_inst_map, paired with line.rs pointer-keeping fix hits real pins),
                    // if empty, fallback to _from_phrase (Endpoint(Component) form).
                    let pts = self.get_right_points(phrase)?;
                    if pts.is_empty() {
                        points.extend(self.get_right_points_from_phrase(phrase)?);
                    } else {
                        points.extend(pts);
                    }
                }
                Ok(points)
            }

            // Def needs to be instantiated first, handled in process_member_internal
            //
            // ── Iter-10.D (bucket D, mirror get_left_points same-name fix) ───────────
            McPhrase::Endpoint(McEndpoint::Single(
                iref @ McInstanceRef {
                    base: McInstance::Component(_),
                    ..
                },
            ))
            | McPhrase::Endpoint(McEndpoint::Single(
                iref @ McInstanceRef {
                    base: McInstance::Module(_),
                    ..
                },
            )) => {
                let bus = iref.to_bus();
                if !bus.name.is_empty() {
                    if let Some(lanes) = self.expand_port_lanes(&bus.name) {
                        return Ok(lanes);
                    }
                    if let Some(comp) = self.find_component(&bus.name) {
                        if let Some(pin) = comp.get_right_pin() {
                            return Ok(vec![pin]);
                        }
                    }
                }
                Ok(vec![])
            }
            McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Label(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::List(_),
                ..
            }))
            | McPhrase::Endpoint(McEndpoint::Single(McInstanceRef {
                base: McInstance::Interface(_),
                ..
            })) => Ok(vec![]),
            McPhrase::Endpoint(ref ep) => {
                let right = ep.get_right();
                let mut points = Vec::new();
                for bus in right {
                    points.push(self.node_to_netpoint(&bus));
                }
                Ok(points)
            }
            // ── Iter-12.1 (D-class fix): Member variant interface parsing ──────────
            // Symmetric to get_left_points Member branch logic.
            // ── Iter-12.1b: add cross-module component lookup ──
            McPhrase::Member(phrase, member_ep) => {
                let member_name = match member_ep {
                    McEndpoint::Single(ir) => match &ir.base {
                        McInstance::Label(s) => Some(s.clone()),
                        McInstance::Bus(b) if b.member.is_empty() => Some(b.name.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(ref mname) = member_name {
                    if let Some(caller) = Self::extract_caller_inst_name(phrase) {
                        // (A) direct component lookup
                        if let Some(comp) = self.find_component(&caller) {
                            if let Some(pids) = comp.find_bus_port_pin_ids(mname) {
                                return Ok(pids
                                    .iter()
                                    .map(|pid| {
                                        NetPoint::with_owner(
                                            &format!("{caller}.{pid}"),
                                            &caller,
                                            IOType::None,
                                        )
                                    })
                                    .collect());
                            }
                        }
                        // (B) cross-module component lookup
                        if let Some((sub_name, comp_name)) = caller.split_once('.') {
                            if let Some(sub) = self.find_submodule(sub_name) {
                                if let Some(comp) =
                                    sub.components.iter().find(|c| c.name == comp_name)
                                {
                                    if let Some(pids) = comp.find_bus_port_pin_ids(mname) {
                                        return Ok(pids
                                            .iter()
                                            .map(|pid| {
                                                NetPoint::with_owner(
                                                    &format!("{caller}.{pid}"),
                                                    sub_name,
                                                    IOType::None,
                                                )
                                            })
                                            .collect());
                                    }
                                }
                            }
                        }
                        // (C) expand_port_lanes fallback
                        let qualified = format!("{caller}.{mname}");
                        if let Some(lanes) = self.expand_port_lanes(&qualified) {
                            return Ok(lanes);
                        }
                    }
                }
                self.get_right_points(phrase)
            }
            McPhrase::Multiple(inner) => {
                // ── P1 fix: mirror get_left_points Multiple handling ────────
                // previously returned vec![], causing [RES, _] right endpoints (RES.out + passthrough)
                // all lost, breaking `[RES, _] + CAP'` adjacent connections.
                let mut points = Vec::new();
                for p in inner {
                    points.extend(self.get_right_points(p)?);
                }
                Ok(points)
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn get_left_points_from_line(
        &mut self,
        phrase: &McPhrase,
    ) -> Result<Vec<NetPoint>, InstError> {
        let members = self.phrase_to_members(phrase);
        if let Some(first_member) = members.first() {
            self.get_left_points(first_member)
        } else {
            Ok(vec![])
        }
    }

    #[allow(dead_code)]
    pub(super) fn get_right_points_from_line(
        &mut self,
        phrase: &McPhrase,
    ) -> Result<Vec<NetPoint>, InstError> {
        let members = self.phrase_to_members(phrase);
        if let Some(last_member) = members.last() {
            self.get_right_points(last_member)
        } else {
            Ok(vec![])
        }
    }

    pub(super) fn get_left_points_from_phrase(
        &mut self,
        phrase: &McPhrase,
    ) -> Result<Vec<NetPoint>, InstError> {
        match phrase {
            McPhrase::Series(phrases) => {
                // For a sequence, get left points from the first phrase
                if let Some(first) = phrases.first() {
                    self.get_left_points_from_phrase(first)
                } else {
                    Ok(Vec::new())
                }
            }
            // ── Iter-5.C (refined) ────────────────────────────────────────
            // Parallel each branch should expose its own left pin to external adjacency.
            // phrase layer get_left for Parallel only returns opds[0].get_left()
            // (mc_phrase.rs:934-940), non-first branch pin 1 never connects to upstream,
            // causing `(CAP(1nF) + RES(10kΩ)) -> GND` @RES6 all isolated.
            //
            // Key: use RECURSIVE `_from_phrase` here, not `get_left_points` —
            //   - get_left_points for Endpoint(Component) returns Ok(vec![])
            //     (see points.rs:268-272 _ fallback), will return empty for parse-time created
            //     @CAP5_ep / @RES6_ep, actually losing @CAP5.1 connection
            //     (previous Iter-5.C attempt regressed like this).
            //   - _from_phrase for Endpoint goes through `_` branch below, uses
            //     phrase.get_left() → Component returns [McBus("@CAP_N.1")]
            //     real pin path, expand_node_element_to_points correctly generates
            //     NetPoint with owner="@CAP_N"。
            McPhrase::Parallel(opds) => {
                let mut points = Vec::new();
                for opd in opds {
                    points.extend(self.get_left_points_from_phrase(opd)?);
                }
                Ok(points)
            }
            _ => {
                let left_elems = phrase.get_left();
                let mut points = Vec::new();
                for elem in left_elems {
                    points.extend(self.expand_node_element_to_points(&elem)?);
                }
                Ok(points)
            }
        }
    }

    pub(super) fn get_right_points_from_phrase(
        &mut self,
        phrase: &McPhrase,
    ) -> Result<Vec<NetPoint>, InstError> {
        match phrase {
            McPhrase::Series(phrases) => {
                // For a sequence, get right points from the last phrase
                if let Some(last) = phrases.last() {
                    self.get_right_points_from_phrase(last)
                } else {
                    Ok(Vec::new())
                }
            }
            // ── Iter-5.C (refined) ────────────────────────────────────────
            // Mirror handling: Parallel each branch should expose its own right pin.
            // Also use RECURSIVE `_from_phrase` not `get_right_points`,
            // for inner Endpoint(Component) correctly gets [McBus("@CAP_N.2")] real
            // pin path via phrase.get_right(). See get_left_points_from_phrase comment above.
            McPhrase::Parallel(opds) => {
                let mut points = Vec::new();
                for opd in opds {
                    points.extend(self.get_right_points_from_phrase(opd)?);
                }
                Ok(points)
            }
            _ => {
                let right_elems = phrase.get_right();
                let mut points = Vec::new();
                for elem in right_elems {
                    points.extend(self.expand_node_element_to_points(&elem)?);
                }
                Ok(points)
            }
        }
    }

    /// Single McBus → NetPoint
    ///
    /// ── Iter-12.3: all generated paths processed through canonicalize_path ──
    /// Eliminate duplicate suffixes (e.g., `VCC_1V2.VCC_1V2` → `VCC_1V2`)
    pub(super) fn node_to_netpoint(&mut self, element: &McBus) -> NetPoint {
        use crate::instant::mc_net::canonicalize_path;

        // ── [DIAG-io] P4 phantom tracing ───────────────────────────────────────
        // Record whether any endpoint with last segment in/out passes through this function, and first segment
        // find_component/find_submodule hit status (used to determine why [FIX-C] doesn't isolate `<host>.in`).
        if let Some((_fp, rest)) = element.name.split_once('.') {
            if rest == "in" || rest == "out" || rest.ends_with(".in") || rest.ends_with(".out") {}
        }

        // 1. check if it's a port
        if self.is_port(&element.name) {
            let path = canonicalize_path(&element.name);
            return NetPoint::new(&path, IOType::None);
        }

        // 2. check if it's a path access (e.g., R1.1, sub1.clk, power.VCC)
        if let Some((first_part, rest)) = element.name.split_once('.') {
            // 2.1 component pin access
            //
            // ── ★ FIX-C: real component + phantom pin suffix merging ────────────────────────
            //
            // old P2-filter already handled `<CLASS>.in/out` form (CLASS not any known
            // instance → phantom placeholder). But **one case slipped through**: `<inst>.in/out` where
            // `inst` is real component (e.g., mcu513.uC instance's uC component),
            // **but** the component itself **doesn't have** a pin named `in`/`out`. In this case, `.in`/`.out`
            // is still mc_fcall.rs placeholder injected during outer chain continuation
            // (funccall_inst.rs Iter-3.E3 comment "phantom placeholder" concept), but because
            // first_part hit find_component, old P2-filter's "secondary confirm real instance" let it pass,
            // then here `<inst>.in` directly registered as real pin access NetPoint — subsequent union-find
            // merges all same inst `.in` together, cross-chain into strange "uC.in ~ CAP_1.1" and
            // "CAP_1.2 ~ uC.out" non-physical connections (actually seen in hbl mcu513 dump).
            //
            // Fix strategy: after determining first_part is component, do another verification "is suffix really
            // declared pin". Verification relies on `c.def.pins.names_to_id` — authoritative pin name set
            // at component type level (includes dotted sub-ports, e.g., SPI.SCLK).
            //
            // Safety:
            //   - only intervene when suffix ∈ {"in", "out"}. These two literals are mc_fcall.rs
            //     fixed suffixes for placeholder generation (`<type_name>.in`/`.out`), real
            //     components almost never use "in"/"out" as pin names (standard is "1"/"2"/or physical net
            //     names). If some day a component really defines in/out pin, `contains_key`
            //     will hit, this fix won't misidentify.
            //   - if pin doesn't exist, isolate into `@_phantom_<inst>_<n>` unique name, same isolation
            //     mechanism as old P2-filter, prevent downstream union merge.
            if let Some(comp) = self.find_component(first_part) {
                if (rest == "in" || rest == "out") && !comp.def.pins.names_to_id.contains_key(rest)
                {
                    let isolated = self.auto_name(&format!("@_phantom_{first_part}"));
                    let pin = if rest == "in" { "1" } else { "2" };
                    let path = format!("{isolated}.{pin}");
                    return NetPoint::with_owner(&path, &isolated, IOType::None);
                }
                // ── P7 + P2: inst.IFACE.member / bare alias → physical pid, unified notation ──
                // First try the original Single direct lookup; if that fails, use
                // resolve_bare_member_pid to reverse-look up the dotted alias, so that
                // bare spellings like `ldo.Vout` / `ldo.GND` also resolve to `ldo.5` /
                // `ldo.2` and can union with the numbered spellings like `@CAP2.2 ~ ldo.5`.
                let last = rest.rsplit('.').next().unwrap_or(rest);
                let single_hit: Option<String> = comp
                    .def
                    .pins
                    .names_to_id
                    .get(rest)
                    .or_else(|| comp.def.pins.names_to_id.get(last))
                    .and_then(|port| match port {
                        crate::semantic::component::mc_pins::McPinPort::Single(id) => {
                            Some(id.clone())
                        }
                        _ => None,
                    });
                let resolved_pid: Option<String> = single_hit.or_else(|| {
                    let r = resolve_bare_member_pid(&comp.def.pins, rest, last);
                    // ── [P2-RECOVER-PROBE] delete after verification: before the fix these rest
                    //    values would fall back to the string "first_part.rest" without union;
                    //    now they recover to pid. Run once and watch the printed aliases to
                    //    confirm P2 was indeed truly not-unioning before.
                    if let Some(ref pid) = r {
                        eprintln!("[P2-RECOVER] {first_part}.{rest} -> {first_part}.{pid} (unresolved before fix)");
                    }
                    r
                });
                let resolved = resolved_pid
                    .map(|id| format!("{first_part}.{id}"))
                    .unwrap_or_else(|| canonicalize_path(&element.name));
                return NetPoint::with_owner(&resolved, first_part, IOType::None);
            }
            // 2.2 submodule port access
            if self.find_submodule(first_part).is_some() {
                let path = canonicalize_path(&element.name);
                return NetPoint::with_owner(&path, first_part, IOType::None);
            }
            // 2.3 bus member access (e.g., power.VCC)
            if self.is_bus(first_part) {
                // verify member exists
                if let Some(bus) = self.find_bus(first_part) {
                    // extract first-level member name
                    let member_name = rest.split('.').next().unwrap_or(rest);
                    if !bus.has_member(member_name) {
                        eprintln!(
                            "Warning: Bus '{}' has no member '{}', available: {:?}",
                            first_part, member_name, bus.members
                        );
                    }
                }
                let path = canonicalize_path(&element.name);
                return NetPoint::new(&path, IOType::None);
            }
        }

        // 3. check if it's known bus (whole reference)
        if self.is_bus(&element.name) {
            let path = canonicalize_path(&element.name);
            return NetPoint::new(&path, IOType::None);
        }

        // ── P2: filter CLASS.in / CLASS.out phantom placeholders ────────────────
        // mc_fcall.rs generates `{CLASS}.in`/`{CLASS}.out` placeholders when caller=None.
        // if reaching here means CLASS is not existing instance/port/bus/component — it's class name leak.
        // all same-type anonymous components share same `CAP.in` label → union-find short.
        // solution: generate unique isolated endpoint (not registered as label), prevent cross-call merging.
        if let Some((class_part, suffix)) = element.name.split_once('.') {
            if (suffix == "in" || suffix == "out")
                && !class_part.is_empty()
                && class_part
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase())
            {
                // secondary confirmation: definitely not existing instance
                if self.find_component(class_part).is_none()
                    && self.find_submodule(class_part).is_none()
                    && !self.is_port(class_part)
                    && !self.is_bus(class_part)
                {
                    let isolated = self.auto_name(&format!("@_phantom_{class_part}"));
                    let pin = if suffix == "in" { "1" } else { "2" };
                    let path = format!("{isolated}.{pin}");
                    // ── [P4-PHANTOM] temp probe: who's leaking CLASS.in/out ──
                    eprintln!(
                        "[P4-PHANTOM] leaking element.name={:?} -> {}",
                        element.name, path
                    );
                    return NetPoint::with_owner(&path, &isolated, IOType::None);
                }
            }
        }

        // 4. default as label handling
        // ── P5: not port/path-with-dots/bus — distinguish: single-pin component / curly member selection / net label
        // core: never set owner == entire path (that's the source of X.X, violates NetPoint doc invariant).
        // 4a. single-pin component (e.g., TEST_POINT) → inst.<unique pin>, owner=inst
        if let Some(comp) = self.find_component(&element.name) {
            if comp.def.pins.names_to_id.len() == 1 {
                // get unique pin name
                // SAFETY: guarded by `names_to_id.len() == 1` check above
                let pin = comp
                    .def
                    .pins
                    .names_to_id
                    .keys()
                    .next()
                    .cloned()
                    .expect("single-pin component has no pins");
                let path = format!("{}.{}", element.name, pin);
                return NetPoint::with_owner(&path, &element.name, IOType::None);
            }
            // multi-pin component bare reference (rare, usually notation ambiguity): owner=None, render as single token,
            // no longer owner==path duplication (avoid X6.X6).
            let path = canonicalize_path(&element.name);
            return NetPoint::new(&path, IOType::None);
        }

        // 4b. curly member selection wm7121{VCC} → inst.VCC, owner=inst
        if let Some((base, member)) = parse_curly_select(&element.name) {
            if self.find_component(&base).is_some() || self.find_submodule(&base).is_some() {
                let path = format!("{base}.{member}");
                return NetPoint::with_owner(&path, &base, IOType::None);
            }
            // base not known instance: degrade to label (member selection meaningless), still owner=None
            let path = canonicalize_path(&element.name);
            self.ensure_label(&path);
            return NetPoint::new(&path, IOType::None);
        }

        // 4c. net label (GND / USB_VBUS / AVDD09_CAP ...) → owner = None
        // conforms to NetPoint doc invariant (mc_net.rs:21 "label/port is None"),
        // render layer no longer gets owner → no more `GND.GND`.
        let path = canonicalize_path(&element.name);
        self.ensure_label(&path);
        NetPoint::new(&path, IOType::None)
    }

    // ========================================================================
    // Iter-8: N×1 bus port endpoint expansion
    // ========================================================================

    /// Check if given name (bare port name / `<sub>.<port>` path / `<comp>.<port>` path)
    /// corresponds to a declared N×1 bus port with ≥2 members; if so, expand to N independent NetPoints by declared members.
    ///
    /// This is the core fix for bugfix_report errors 2/3/4/8 (common root cause 1: bus dimension auto-expansion failure).
    /// Rule doc §10.4: `[N×1] vs [N×1]` must correspond element by element;
    /// but to achieve element-wise correspondence, the endpoint resolution layer must first expand `mic.MIC` (declared as
    /// `out MIC{P,N}::ADC.DIFF()`) into `[mic.MIC.P, mic.MIC.N]`, not
    /// a single label `[mic.MIC]`.
    ///
    /// # Three hit forms
    ///
    /// 1. **current module's own port** (bare name): `UART0` → hits self.ports
    /// 2. **submodule port** (`<sub>.<port>`): `mic.MIC` → hits sub_modules[mic].ports
    /// 3. **component bus port** (`<comp>.<port>`): `uC.XTAL` → hits
    ///    components[uC]'s bus port (looked up from component type pins/bus declaration)
    ///
    /// Current implementation covers 1, 2 (mainstream scenarios). 3rd left for future iterations — currently
    /// `uC.UART0`, `uC.XTAL` component bus port expansion depends on parser/elaborator
    /// splitting `uC{UART0|...}` curly-mn notation at an earlier stage.
    ///
    /// # Direction restriction (key safety constraint for this iteration)
    ///
    /// Only expand N×1 ports in **`Out` direction**. Rationale:
    ///
    /// - `out MIC{P,N}::ADC.DIFF()` **output** bus, parent module writes
    ///   `mic.MIC -> ...` meaning P/N each connected to receiver (§10.4 N×1 vs N×1).
    ///   Expansion is strictly required by rules.
    /// - `in vin{POWER_SYS, GND}::DC(5V)` **input** bus, parent module often writes
    ///   `usbsocket.vin -> V5V::DC(5V)` — intent is "vin overall connected to V5V
    ///   label net" (relies on submodule internal inject_port_member_labels injecting
    ///   vin.POWER_SYS/.GND labels naturally merge), not broadcasting V5V simultaneously to
    ///   POWER_SYS and GND (that would short power directly to ground). Under §10.4
    ///   `[1×1] vs [2×1]` strictly shouldn't be written, but engineering convention is this.
    /// - `io` (InOut) port semantics unclear, conservative strategy: don't expand.
    ///
    /// Future iterations when fully fixing §10.4 should decide expansion at
    /// `process_member_internal` upper layer based on "whether peer can match N×1 dimension",
    /// not blanket at endpoint resolution layer. This iteration starts with Out ports,
    /// landing error 2 fix with minimal changes.
    ///
    /// # Boundaries
    ///
    /// - Scalar port (bus_members.len() < 2): return `None`, let caller go original path,
    ///   keep single-point contract.
    /// - Name not a port (bare label / component pin / bus member): return `None`.
    /// - Already manually expanded (`element.member` non-empty): caller should **check member first**
    ///   then come here, avoid double expansion.
    pub(super) fn expand_port_lanes(&self, name: &str) -> Option<Vec<NetPoint>> {
        // ── P2 helper: extract members from "name{A, B}" / "[A, B]" / "name[A, B]" ──
        // tolerate space after comma. find first `{`/`[` and last `}`/`]`, split middle by comma.
        fn parse_brace_members(s: &str) -> Vec<String> {
            for (open, close) in [('{', '}'), ('[', ']')] {
                if let (Some(o), Some(c)) = (s.find(open), s.rfind(close)) {
                    if c > o + 1 {
                        return s[o + 1..c]
                            .split(',')
                            .map(|x| x.trim().to_string())
                            .filter(|x| !x.is_empty())
                            .collect();
                    }
                }
            }
            Vec::new()
        }
        // ── P2 helper: strip `{...}`/`[...]` suffix from name, get base name ──
        fn strip_brace_suffix(s: &str) -> &str {
            let cut = match (s.find('{'), s.find('[')) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            match cut {
                Some(i) => &s[..i],
                None => s,
            }
        }

        // ── [P2-DIAG] entry ───────────────────────────────────────────────

        // ── P2: direction-agnostic expansion ──────────────────────────────────────────
        // historically banned In to prevent `in vin{POWER_SYS,GND}` broadcasting to power+gnd short.
        // but the real cause of short is "one side expands to N, other side still scalar → hits broadcast arm",
        // **not** expansion itself. Part 1's create_connection DC anti-short alignment
        // (C3) already blocked broadcast arm's short path, so here open all directions, let
        // `dc{VDD_3V3,GND}::DC()` (in) bus ports also expand, zip equal-width with peer.
        fn iotype_allowed(_io: &IOType) -> bool {
            true
        }

        // ── Case 1: `<sub>.<port>` form ──────────────────────────────
        if let Some((owner, port_name_raw)) = name.split_once('.') {
            // prevent false hit: port containing another '.' means already specific to lane (mic.MIC.P), don't expand
            if !port_name_raw.contains('.') {
                if let Some(sub) = self.find_submodule(owner) {
                    // (a) anonymous bracket/curly port directly in reference:
                    //     `moddcdc.[VDD_3V3, GND]` → members are in brackets,
                    //     lane path is `owner.member` (no port name hierarchy,
                    //     consistent with submodule internal expansion treating members as top-level labels).
                    if port_name_raw.starts_with('[') || port_name_raw.starts_with('{') {
                        let members = parse_brace_members(port_name_raw);
                        if members.len() >= 2 {
                            return Some(
                                members
                                    .iter()
                                    .map(|m| {
                                        let path = format!("{owner}.{m}");
                                        NetPoint::with_owner(&path, owner, IOType::None)
                                    })
                                    .collect(),
                            );
                        }
                    }
                    // (b) named port: tolerate duplicate ports (vin / vin{POWER_SYS,GND}) and
                    //     bus_members unfilled — match by base name, take one with most "effective members".
                    //     effective members = bus_members non-empty use them, otherwise parse from port name.
                    let port_base = strip_brace_suffix(port_name_raw);
                    let best = sub
                        .ports
                        .iter()
                        .filter(|p| p.name == port_base || strip_brace_suffix(&p.name) == port_base)
                        .map(|p| {
                            let m = if !p.bus_members.is_empty() {
                                p.bus_members.clone()
                            } else {
                                parse_brace_members(&p.name)
                            };
                            (p, m)
                        })
                        .filter(|(p, m)| m.len() >= 2 && iotype_allowed(&p.iotype))
                        .max_by_key(|(_, m)| m.len());
                    if let Some((port, members)) = best {
                        return Some(
                            members
                                .iter()
                                .map(|m| {
                                    let path = format!("{owner}.{port_base}.{m}");
                                    NetPoint::with_owner(&path, owner, port.iotype.clone())
                                })
                                .collect(),
                        );
                    }
                }
            }
        }

        // ── Case 2: bare port name → current module's own port ────────────────────
        // Same as Case 1 (b): match by base name, take most effective members, tolerate
        // duplicate ports (dc / dc{VDD_3V3,GND}) and bus_members unfilled.
        //
        // ── S1 Bug A companion (2026-06) ─────────────────────────────────
        // after strict match fails, add eq_ignore_ascii_case fallback (e.g., body
        // boundary formal `spi` vs declared port `SPI`). Symmetric with Bug D Part 1 fix.
        {
            let port_base = strip_brace_suffix(name);
            let best = self
                .ports
                .iter()
                .filter(|p| {
                    p.name == port_base
                        || strip_brace_suffix(&p.name) == port_base
                        || p.name.eq_ignore_ascii_case(port_base)
                        || strip_brace_suffix(&p.name).eq_ignore_ascii_case(port_base)
                })
                .map(|p| {
                    let m = if !p.bus_members.is_empty() {
                        p.bus_members.clone()
                    } else {
                        parse_brace_members(&p.name)
                    };
                    (p, m)
                })
                .filter(|(p, m)| m.len() >= 2 && iotype_allowed(&p.iotype))
                .max_by_key(|(_, m)| m.len());
            if let Some((port, members)) = best {
                return Some(
                    members
                        .iter()
                        .map(|m| {
                            let path = format!("{port_base}.{m}");
                            // current module's own port has no owner (it's net top-level label).
                            NetPoint::new(&path, port.iotype.clone())
                        })
                        .collect(),
                );
            }
        }

        // ── Case 3: `<comp>.<port>` form → component bus port expansion ─────────
        //
        // (Iter-10 bucket D, bugfix_report errors 1 / 3 / 4 / 8 main path)
        //
        // Trigger scenario: hbl project mcu513 module body:
        //   - `uC.UART0 -> res[1:2]::RES(100kΩ) -> ...`     (error 3)
        //   - `uC.21 -> ...` (actually should be `uC.I2C0`)    (error 1)
        //   - `X6.XTAL -> uC.XTAL`                            (error 8)
        //   - `[uC.UART0, uC.VDD] -> ...`                     (error 4 extension)
        //
        // Expansion rule: <comp>.<port> hits component bus port → use def.pins.names_to_id
        // reverse lookup associated physical pin id, expand to [<comp>.<pid>, ...]. pid form
        // consistent with inst_table registered component pin path (init_pins uses pid as key,
        // path = `<comp>.<pid>`), ensures downstream flatten / inst_table hit.
        //
        // Unlike Case 1/2, Case 3 **doesn't restrict direction** — component physical pin is independent
        // physical pin, expanding to lanes won't cause the "parent module InOut port broadcast to
        // wrong electrical nodes" regression that Case 1/2 restrictions prevent. Component bus ports
        // are mostly io (InOut) direction, must be allowed.
        if let Some((owner, port_name)) = name.split_once('.') {
            if !port_name.contains('.') {
                if let Some(comp) = self.find_component(owner) {
                    if let Some(pids) = comp.find_bus_port_pin_ids(port_name) {
                        // pid is component physical pin number, owner is component instance name,
                        // path form `<comp>.<pid>` consistent with init_pins registration.
                        // iotype takes first pid's direction (UART0/SPI bus ports
                        // typically all members same direction; even if different, taking first as default
                        // doesn't affect create_connection N×N alignment behavior).
                        let iotype = pids
                            .first()
                            .and_then(|pid| comp.def.pins.get_pin_io(pid))
                            .unwrap_or(IOType::None);

                        return Some(
                            pids.iter()
                                .map(|pid| {
                                    let path = format!("{owner}.{pid}");
                                    NetPoint::with_owner(&path, owner, iotype.clone())
                                })
                                .collect(),
                        );
                    }
                }
            }
        }

        // ── [P2-DIAG] no hit: dump all context needed for diagnosis ──────────────
        // used to determine why bus collapsed: parsed as non-Bus base didn't reach here (that path
        // won't print, reflected by [TCA-diag]), or reached but owner's port bus_members
        // empty / port name mismatch (anonymous [A,GND] port).
        {
            if let Some((owner, _port_raw)) = name.split_once('.') {
                let _sub_ports: Option<Vec<(String, Vec<String>)>> =
                    self.find_submodule(owner).map(|s| {
                        s.ports
                            .iter()
                            .map(|p| (p.name.clone(), p.bus_members.clone()))
                            .collect()
                    });
                let _comp_has = self.find_component(owner).map(|c| c.name.clone());
            } else {
                let _self_ports: Vec<(String, Vec<String>)> = self
                    .ports
                    .iter()
                    .map(|p| (p.name.clone(), p.bus_members.clone()))
                    .collect();
            }
        }

        None
    }

    /// ── P2: component pin alias path → pid path ─────────────────────────────
    /// "ldo.VOUT.Vout" / "ldo.VIN.Vin" / "ldo.GND" → "ldo.5" / "ldo.1" / "ldo.2".
    /// Returns Some only when the first segment is a component instance of this module
    /// and the alias resolves to a unique pid; otherwise None (submodule ports / bus
    /// ports / labels / already-pid all return None, untouched).
    pub(super) fn normalize_one_inst_pin_path(&self, path: &str) -> Option<String> {
        use crate::semantic::component::mc_pins::McPinPort;

        let (inst, member) = path.split_once('.')?;
        let comp = self.find_component(inst)?;
        // Already a pure pid (key in pins table) → leave alone
        if comp.def.pins.pins.contains_key(member) {
            return None;
        }
        let last = member.rsplit('.').next().unwrap_or(member);

        // 1. names_to_id direct Single lookup (dotted full name "VOUT.Vout"/"IN.P" or bare name "VDD"/"FB")
        let direct = comp
            .def
            .pins
            .names_to_id
            .get(member)
            .or_else(|| comp.def.pins.names_to_id.get(last))
            .and_then(|p| match p {
                McPinPort::Single(id) => Some(id.clone()),
                _ => None,
            });

        let pid = if let Some(id) = direct {
            id
        } else {
            // 2. bare-alias fallback: curly-bus members (VOUT{Vout,GND}) only register
            //    the dotted form "VOUT.Vout"; bare "Vout"/"GND" is not in names_to_id
            //    → reverse-look up the last segment in pin_id_to_names.
            let mut hits: Vec<String> = Vec::new();
            for (pid, names) in comp.def.pins.pin_id_to_names.iter() {
                let matched = names.iter().any(|n| {
                    let seg = n.rsplit('.').next().unwrap_or(n);
                    n == member || n == last || seg == member || seg == last
                });
                if matched && !hits.contains(pid) {
                    hits.push(pid.clone());
                }
            }
            match hits.len() {
                0 => return None,
                1 => hits.remove(0),
                _ => {
                    // ── [P2-AMBIG-PROBE] delete after verification: same-named member lands on multiple different pids ──
                    eprintln!(
                        "[P2-AMBIG] {inst}: member={member:?} -> pids {hits:?} (take smallest)"
                    );
                    hits.sort_by(|a, b| {
                        a.parse::<i64>()
                            .unwrap_or(0)
                            .cmp(&b.parse::<i64>().unwrap_or(0))
                    });
                    hits.remove(0)
                }
            }
        };

        let new = format!("{inst}.{pid}");
        if new == path {
            None
        } else {
            Some(new)
        }
    }

    // ========================================================================
    // lookup helpers
    // ========================================================================

    pub(super) fn is_port(&self, name: &str) -> bool {
        self.ports.iter().any(|p| p.name == name)
    }

    pub(super) fn find_component(&self, name: &str) -> Option<&McComponentInst> {
        self.components.iter().find(|c| c.name == name)
    }

    pub(super) fn find_submodule(&self, name: &str) -> Option<&McModuleInst> {
        self.sub_modules.iter().find(|m| m.name == name)
    }

    pub(super) fn ensure_label(&mut self, name: &str) {
        if !self.labels.contains_key(name) {
            self.labels
                .insert(name.to_string(), NetPoint::new(name, IOType::None));
        }
    }
}
