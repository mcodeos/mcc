// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PI-2 filter-leg load-side decoupling (power-quality-design.md §2.2, ruling 11
//! decided 2026-09-17). A `@bridge` whose two endpoints are both **hot faces** of
//! declared rails is a supply filter leg: the ferrite is the series half of a
//! filter, not the filter, and the LC only exists once the load side it protects
//! carries a decoupling element. A declared filter leg whose load side carries
//! none is the declaration contradicting the topology — the same shape PWR-5 /
//! PWR-6 judge — so it is an Error.
//!
//! The **load side** is read from the declaration, never from the arrow order
//! (ruling 3): `@bridge(A, B)` carries only names, and the corpus holds both
//! spellings of one flow, so the side that is being protected is the endpoint
//! whose domain world reads as a quiet/sensitive face (§1.4 — `@class(analog)`,
//! `@noise(quiet)`, `@noise(sensitive)`). No quiet side means no load side to
//! speak of, and the rule is silent rather than guessing which end is down.
//!
//! Ruling 11 (2026-09-17) cuts this verdict to the **existence** face: the
//! load-side net is a declared rail's hot member and *a* capacitor sits on it, or
//! none does. Where that capacitor's return leg lands is PI-3's object
//! ([`super::decouple`], `6038`) — the partition ruling 8 gave the 6022 seam, so
//! a mis-landed return reports one code instead of the same defect twice.
//!
//! Both endpoints are read through the net's **effective class** ([`super::eff_class`])
//! and the capacitor candidate from the flat carries
//! ([`InstEntry::element_class`], never a name), so a bridge declared in a parent
//! scope whose load-side net belongs to that scope's own rails is judged the same
//! way at any depth.
//!
//! Not judged, never guessed (§1.3): a bridge that is not a supply leg (its
//! endpoints are not both hot faces — a ground-side bridge is one), a bridge with
//! no quiet side, a bridge whose two sides *both* read quiet (the load side would
//! be a guess), an endpoint whose class does not resolve, and a `@couple` edge (a
//! DC-blocking coupling element is not a filter leg).

use super::railface::{declared_rails, rails_of_leg, scope_classes};
use super::NetCheckResult;
use crate::instant::insttab::{InstEntry, InstTable};
use crate::instant::island::NetIslandIndex;
use crate::semantic::basic::attr_keys::ElementClass;
use crate::semantic::module::pi::L1EdgeKind;
use std::collections::{HashMap, HashSet};

/// PI-2: a declared filter bridge's load side must carry a decoupling capacitor.
pub(crate) fn check_bridge_load_decoupling(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Candidate filter first: neither the island index nor the scope tables are
    // built when the board declares no filter bridge at all.
    let edges = super::declared_dc_edges(table);
    let bridges: Vec<(u32, &super::DeclEdge)> = edges
        .iter()
        .flat_map(|(m, list)| {
            list.iter()
                .filter(|e| e.kind == L1EdgeKind::Bridge)
                .map(move |e| (*m, e))
        })
        .collect();
    if bridges.is_empty() {
        return;
    }
    // The declared capacitors to look for. No early return on an empty list: a
    // board with a filter leg and no capacitor at all is this rule's firing
    // case, not a reason to stay silent.
    let caps: Vec<&InstEntry> = table
        .get_components()
        .into_iter()
        .filter(|e| {
            !e.synthetic
                && !e.unselected
                && !e.not_fitted
                && e.element_class == Some(ElementClass::Capacitive)
                && e.pin_count == 2
        })
        .collect();

    let idx = NetIslandIndex::build(table);
    let classes = scope_classes(table, &idx);
    let rails = declared_rails(table, &classes);
    if rails.is_empty() {
        return;
    }

    // Net names are resolved **in the scope that wrote them** (a `@bridge` clause
    // names the nets of its own module), like the rail side of PI-3.
    let mut scope_nets: HashMap<u32, HashMap<String, Vec<u32>>> = HashMap::new();
    for net in table.get_nets() {
        let Some(module) = net.module else {
            continue;
        };
        scope_nets
            .entry(module)
            .or_default()
            .entry(net.name.clone())
            .or_default()
            .push(net.id);
    }

    // §1.4's quiet/sensitive face, per declaring scope: a domain that says
    // `@class(analog)` or `@noise(quiet|sensitive)`. Which words make a face
    // quiet is this rule's step, not the projection's (pi.rs carries them
    // verbatim).
    let mut quiet: HashMap<u32, HashSet<String>> = HashMap::new();
    for (id, pi) in table.power_decls() {
        let set: HashSet<String> = pi
            .l1_domain_faces()
            .into_iter()
            .filter(|f| {
                f.class.as_deref() == Some("analog")
                    || matches!(f.noise.as_deref(), Some("quiet") | Some("sensitive"))
            })
            .map(|f| f.name)
            .collect();
        if !set.is_empty() {
            quiet.insert(*id, set);
        }
    }

    for (scope, edge) in bridges {
        // Both endpoints as the declaring scope reads them; a name that reaches
        // no class is not judged.
        let (Some(load_a), Some(load_b)) = (
            endpoint(table, &idx, &scope_nets, scope, &edge.a),
            endpoint(table, &idx, &scope_nets, scope, &edge.b),
        ) else {
            continue;
        };
        // §2.2's subject: both ends on hot faces = a supply filter leg. A
        // ground-side bridge (`FB_agnd` shape) is not one.
        let hot = |e: &(u32, super::EffClass, u32)| {
            !rails_of_leg(table, &rails, e.2, &e.1.id, &e.1.worlds).is_empty()
        };
        if !hot(&load_a) || !hot(&load_b) {
            continue;
        }
        // The load side is the quiet one. Both quiet is a coin flip, neither is
        // no declared load side — silence is the family's standing rule.
        let (load, world) = match (
            quiet_world(table, &quiet, &load_a),
            quiet_world(table, &quiet, &load_b),
        ) {
            (Some(w), None) => (&load_a, w),
            (None, Some(w)) => (&load_b, w),
            _ => continue,
        };
        // Ruling 11: existence only. Any declared capacitor with a leg on this
        // net's class answers; its return placement is 6038's object.
        let covered = caps.iter().any(|c| {
            leg_classes(table, &idx, c)
                .iter()
                .any(|cls| cls.id == load.1.id)
        });
        if covered {
            continue;
        }
        let load_net = table
            .get_net(load.0)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| load.1.id.clone());
        let pos = edge.lo as u32;
        let uri = super::comp_def_uri(table, scope).unwrap_or_default();
        let written = format!("{} <-> {}", edge.a, edge.b);
        results.push(NetCheckResult {
            check: "bridge-load-decoupling",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING,
                &[&written, &load.1.id, &world],
            ),
            net_name: load_net,
            code: crate::errcodes::BRIDGE_LOAD_DECOUPLING_MISSING,
            pos,
            uri,
        });
    }
}

/// One `@bridge` endpoint as the declaring scope reads it: the net the name was
/// written for, its effective class, and its owning scope. `None` when the name
/// matches no net there, or the net reaches no class — an unresolvable endpoint
/// takes no verdict (its identity would come from an ancestor world).
type Endpoint = (u32, super::EffClass, u32);

fn endpoint(
    table: &InstTable,
    idx: &NetIslandIndex,
    scope_nets: &HashMap<u32, HashMap<String, Vec<u32>>>,
    scope: u32,
    name: &str,
) -> Option<Endpoint> {
    for &net in scope_nets.get(&scope)?.get(name)? {
        let Some(attr) = idx.get(net) else {
            continue;
        };
        let Some(layer) = attr.module else {
            continue;
        };
        let Some(cls) = super::eff_class(table, idx, attr, &mut Vec::new()) else {
            continue;
        };
        return Some((net, cls, layer));
    }
    None
}

/// The quiet/sensitive world an endpoint anchors, read on its own owning-scope
/// chain (a sub-module net is anchored by an ancestor's domain declaration).
fn quiet_world(
    table: &InstTable,
    quiet: &HashMap<u32, HashSet<String>>,
    endpoint: &Endpoint,
) -> Option<String> {
    let mut cur = Some(endpoint.2);
    while let Some(id) = cur {
        if let Some(set) = quiet.get(&id) {
            if let Some(world) = endpoint.1.worlds.iter().find(|w| set.contains(*w)) {
                return Some(world.clone());
            }
        }
        cur = table.get_entry(id).and_then(|e| e.parent_id);
    }
    None
}

/// The effective classes a two-terminal part's legs land on, one per distinct
/// net. Fewer than two leg nets means a pad is unwired or shorted onto the other
/// — neither is a decoupling placement.
fn leg_classes(table: &InstTable, idx: &NetIslandIndex, comp: &InstEntry) -> Vec<super::EffClass> {
    let mut nets: Vec<u32> = Vec::new();
    for pin in table.get_pins_of(comp.id) {
        for &leg in table.nets_of(pin.id) {
            if !nets.contains(&leg) {
                nets.push(leg);
            }
        }
    }
    if nets.len() != 2 {
        return Vec::new();
    }
    nets.iter()
        .filter_map(|&n| {
            let attr = idx.get(n)?;
            super::eff_class(table, idx, attr, &mut Vec::new())
        })
        .collect()
}
