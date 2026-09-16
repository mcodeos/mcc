// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PI-4 filter-subface overreach (power-quality-design.md §2.4, ruling 4 —
//! independent code `6042`, measured 2026-09-16 on the R5 board set).
//!
//! §2.2's supply leg protects a **load side**, and that side is a *subface*: the
//! quiet/sensitive domain's own declared pair, its hot member's copper plus its
//! return member's copper. A sink drawing across that pair is inside the domain
//! the filter was declared for. A sink that touches the subface with only one
//! member of its own pair — its supply from another domain's hot copper, or its
//! return off the subface — is the filter subface feeding a part that never
//! declared to belong to it: the protection the leg was declared for and the
//! supply contract of the part behind it disagree.
//!
//! The witness is the sink's **own declared pair**, resolved on the instance and
//! compared **member by member**: the hot terminal's copper must be the rail's
//! hot member and the return member's copper must be the rail's return. Which
//! declaration owns the terminal is the family's existing read — a component's
//! `psnk` row ([`super::sink_contract_for`]'s shape, [read here the same
//! way][`sink_pairs`]) or an instantiated module's `psnk` port row — and where
//! each member lands is the flatten pass's own carry
//! ([`super::member_net_of`]), so the pair is read *where it was bound*: two
//! instances of one class with different call sites are judged apart, which a
//! definition-space read could never do. Both sides of the comparison are
//! **class ids** ([`super::eff_class`], the axis's one read), never spellings, so
//! a sub-module leg bound to the parent's copper counts and a name decides
//! nothing.
//!
//! The prerequisite is §2.2's leg, and it is load-bearing (§2.4's measured
//! table): 6022 judges the *leg's declaration* (an undeclared crossing) and PI-4
//! judges the *sink's contract* (who may draw from the subface). Declared ⇒ 6022
//! is silenced by its own per-leg span match; undeclared ⇒ no supply leg exists
//! and PI-4 does not apply at all. No configuration makes both rules fire on one
//! witness, which is why ruling 4 gave them two codes rather than one
//! contextualised wording.
//!
//! Error (§2.4): the declaration and the topology contradict each other, the
//! same level as this axis's other cuts. Not judged, never guessed (§1.3): a
//! bridge that is not a supply leg, a leg with no quiet side or two quiet sides
//! (PI-2's own silence, kept here so the load side is never a coin flip), a rail
//! whose return member resolves no class (then no subface holds the two coppers
//! together), a sink row naming no return member, a pair whose members this
//! instance does not carry, a terminal landing on no class, and a pair touching
//! no member of the subface at all. A part drawing from the quiet face's *analog
//! input* rather than from a supply terminal has no supply pair — the board's own
//! bias line is that legal shape and stays silent.

use super::faces::DomainFaces;
use super::railface::{declared_rails, rails_of_leg, scope_classes};
use super::NetCheckResult;
use crate::instant::insttab::{InstKind, InstTable};
use crate::instant::island::NetIslandIndex;
use crate::semantic::component::mc_pins::PwrDir;
use crate::semantic::component::McComponent;
use crate::semantic::module::pi::L1EdgeKind;
use crate::semantic::pwrid::Face;
use std::collections::{HashMap, HashSet};

/// PI-4: a sink drawing from a declared filter leg's load-side subface must
/// declare the subface's own supply pair.
pub(crate) fn check_filter_subface_overreach(
    table: &InstTable,
    results: &mut Vec<NetCheckResult>,
) {
    // The declared filter legs first: a subface exists only where a leg declares
    // one, so a board with no bridge is not this rule's object at all.
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
    // The sinks the verdict is about: every declared supply pair on the board,
    // both members located on the instance that carries them.
    let sinks = sink_pairs(table);
    if sinks.is_empty() {
        return;
    }
    let idx = NetIslandIndex::build(table);
    let classes = scope_classes(table, &idx);
    let rails = declared_rails(table, &classes);
    if rails.is_empty() {
        return;
    }
    let faces = DomainFaces::read(table);
    let scope_nets = super::scope_nets(table);
    // One verdict per sink: two legs protecting one domain declare one subface,
    // and the second must not repeat the first's report.
    let mut reported: HashSet<u32> = HashSet::new();

    for (scope, edge) in bridges {
        // Both endpoints as the writing scope reads them (§2.2's own read).
        let (Some(load_a), Some(load_b)) = (
            super::edge_endpoint(table, &idx, &scope_nets, scope, &edge.a),
            super::edge_endpoint(table, &idx, &scope_nets, scope, &edge.b),
        ) else {
            continue;
        };
        // A supply leg: both ends are hot faces of declared rails.
        let hot = |e: &(u32, super::EffClass, u32)| {
            !rails_of_leg(table, &rails, e.2, &e.1.id, &e.1.worlds).is_empty()
        };
        if !hot(&load_a) || !hot(&load_b) {
            continue;
        }
        // The load side is the quiet one; both quiet is a coin flip and neither
        // is no declared load side (§2.2's own silence, kept for the subface).
        let (load, world) = match (
            faces.quiet_world(table, load_a.2, &load_a.1.worlds),
            faces.quiet_world(table, load_b.2, &load_b.1.worlds),
        ) {
            (Some(w), None) => (&load_a, w),
            (None, Some(w)) => (&load_b, w),
            _ => continue,
        };
        // The subface: the quiet domain's own declared pair, on the chain of the
        // scope the load-side net belongs to.
        let Some(subface) = rails_of_leg(table, &rails, load.2, &load.1.id, &load.1.worlds)
            .into_iter()
            .find(|f| f.domain == world)
        else {
            continue;
        };
        // A rail whose return member reads no class declares no subface: there
        // is no second copper for a pair to be beyond.
        if subface.ret.is_empty() {
            continue;
        }
        let on_subface =
            |c: &str| subface.hot.iter().any(|x| x == c) || subface.ret.iter().any(|x| x == c);

        for sink in &sinks {
            if reported.contains(&sink.subject) {
                continue;
            }
            // Member by member, in classes: the hot terminal on the subface's
            // hot copper, the return member on its return copper.
            let (Some(hot_cls), Some(ret_cls)) = (
                class_id(table, &idx, sink.hot_net),
                class_id(table, &idx, sink.ret_net),
            ) else {
                continue;
            };
            if !on_subface(&hot_cls) && !on_subface(&ret_cls) {
                continue; // the pair draws elsewhere — not this subface's business
            }
            if subface.hot.iter().any(|x| *x == hot_cls) && subface.ret.iter().any(|x| *x == ret_cls) {
                continue; // the honoured shape: the domain's own pair
            }
            reported.insert(sink.subject);
            let Some(entry) = table.get_entry(sink.subject) else {
                continue;
            };
            let (pos, uri) = super::entry_pos(entry);
            let drawn = format!("[{}, {}]", sink.hot_name, sink.ret_name);
            let declared = format!(
                "[{}, {}]",
                subface.hot.first().cloned().unwrap_or_default(),
                subface.ret_name
            );
            let written = format!("{} <-> {}", edge.a, edge.b);
            results.push(NetCheckResult {
                check: "filter-subface-overreach",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::FILTER_SUBFACE_OVERREACH,
                    &[&sink.terminal, &drawn, &world, &written, &declared],
                ),
                net_name: sink.hot_name.clone(),
                code: crate::errcodes::FILTER_SUBFACE_OVERREACH,
                pos,
                uri,
            });
        }
    }
}

/// One declared supply pair as its own instance wires it — the subject of the
/// verdict, and (by its instance) the key that keeps two legs from reporting it
/// twice.
struct SinkPair {
    /// The instance the verdict anchors to: the part, or the sub-module.
    subject: u32,
    /// The drawing terminal as the message names it (`main.uc.AVDD`).
    terminal: String,
    hot_net: u32,
    ret_net: u32,
    hot_name: String,
    ret_name: String,
}

/// Every declared supply pair on the board, both members located on the instance
/// they were bound on. Two shapes, one law: a component's own `psnk` row
/// (`def.pins.pwr`, the row 6036 reads too) and an instantiated module's `psnk`
/// port row (`pwr_ports`, its module-level counterpart). A `psbi` is not read
/// here: this rule judges what *draws* from the subface, and the axis's sink
/// sites are `Snk` throughout (the same cut 6036 makes).
fn sink_pairs(table: &InstTable) -> Vec<SinkPair> {
    let workspace = crate::definition_space().workspace_components();
    let defs: HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    let mut out: Vec<SinkPair> = Vec::new();
    for comp in table.get_components() {
        if comp.synthetic || comp.unselected || comp.not_fitted {
            continue;
        }
        let Some(def) = defs.get(&comp.class_name) else {
            continue;
        };
        for row in &def.pins.pwr {
            if row.dir != PwrDir::Snk {
                continue;
            }
            let Some(ret) = &row.ret else {
                continue;
            };
            let Some(pair) = pair_of(table, comp.id, &comp.path, &row.hot, ret) else {
                continue;
            };
            out.push(pair);
        }
    }
    // The module shape: an instantiated module's own port rows declare its pairs
    // the way a component's pin rows do, and the instance carries them the same
    // way (the member carry is written for `Port` entries too).
    for (id, pi) in table.power_decls() {
        let Some(entry) = table.get_entry(*id) else {
            continue;
        };
        if !matches!(entry.kind, InstKind::Module) {
            continue;
        }
        for row in &pi.pwr_ports {
            if row.dir != PwrDir::Snk {
                continue;
            }
            let Some(ret) = &row.ret else {
                continue;
            };
            let Some(pair) = pair_of(table, *id, &entry.path, &row.hot, ret) else {
                continue;
            };
            out.push(pair);
        }
    }
    out
}

/// One instance's declared pair, both members located. `None` when either member
/// is not carried by this instance (a row the wiring never bound, an unwired
/// terminal) — a pair that cannot be located takes no verdict (§1.3).
fn pair_of(
    table: &InstTable,
    subject: u32,
    subject_path: &str,
    hot: &str,
    ret: &str,
) -> Option<SinkPair> {
    let hot_net = super::member_net_of(table, subject, Face::Hot, hot)?;
    let ret_net = super::member_net_of(table, subject, Face::Ret, ret)?;
    Some(SinkPair {
        subject,
        terminal: format!("{subject_path}.{hot}"),
        hot_net,
        ret_net,
        hot_name: super::net_name(table, hot_net),
        ret_name: super::net_name(table, ret_net),
    })
}

/// The effective class id `net` resolves to — the axis's one read
/// ([`super::eff_class`]), and the only identity either side of the subface
/// comparison uses. `None` when the net reaches no declared class.
fn class_id(table: &InstTable, idx: &NetIslandIndex, net: u32) -> Option<String> {
    let attr = idx.get(net)?;
    super::eff_class(table, idx, attr, &mut Vec::new()).map(|c| c.id)
}
