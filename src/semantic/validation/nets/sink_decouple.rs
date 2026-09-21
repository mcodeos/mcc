// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PI-1 sink-pin decoupling completeness (power-quality-design.md §2.1, ruled
//! 2026-09-16 §5 ruling 2 — Warning, N = 0). A pin that **draws** from a declared
//! DC pair is the load half of that pair, and the pair is what says the load is
//! fed across two nets; a load drawing across a pair nothing decouples is the
//! completeness gap this rule exists for. The subject is the load terminal (a
//! `psnk` component pin row, and a module supply port — §6 R2, the first landing
//! includes ports), never the filter: PI-2 judges the ferrite leg's load side,
//! PI-3 judges a capacitor's return placement, and this judges the sink.
//!
//! The pair is read from the flat carries, never from a name. A component pin's
//! pair is the `::DC` row that owns it ([`super::sink_contract_for`], the row
//! whose `hot` is one of the pin's declared names) — its `ret` is the return
//! member the pin closes over; a module supply port's pair is the `pwr_ports` row
//! of the module it belongs to, matched by the same spelling. **A row with no
//! `ret` is not judged** (§2.1: a single-phase AC shape declares no pair at all,
//! and the return-missing shape is 6030's object), and neither is a site whose
//! contract row cannot be reached at all — the family's standing silence (§1.3),
//! never a guess. Nor is a pin of a part marked `nc` at its instance site (U163,
//! hbl `wm7121(NC)`): an unmounted alternative draws nothing, so the same
//! not-fitted read the family applies to element candidates applies to the sink
//! subject too.
//!
//! The candidate is the flat element class ([`InstEntry::element_class`]
//! `Capacitive`, two terminals on two distinct nets — a terminal unwired or both
//! on one net is not a decoupling placement), never a name or a pin shape (§1.2's
//! silence law: a class with no spec table is not a decoupling capacitor here).
//!
//! **Existence only, by ruling 11's partition.** The verdict is whether a
//! capacitor sits on the sink's hot net; where that capacitor's *return* leg
//! lands belongs to PI-3 ([`super::decouple`], `6038`) and to nothing else. The
//! cut is the same one ruling 11 drew for PI-2 and ruling 8 for 6022 — one cause,
//! one code — and it is load-bearing here too: without it a mis-landed return
//! would be reported twice, once as "no decoupling" (which is false — there is a
//! capacitor, misplaced) and once as `6038`'s mis-placement.
//!
//! Both sides are compared through the net's **effective class** where one
//! resolves ([`super::eff_class`], the axis's one read), so a sink inside a
//! sub-module and a capacitor on the parent's copper are one fact; an unresolved
//! net on either side falls back to the junction test (two segments of one node),
//! which is what makes the golden board's `VMAIN_5V` — a pair written on a
//! connection line, owned by no domain's rail — judgeable at all.

use super::NetCheckResult;
use crate::instant::insttab::{InstKind, InstTable};
use crate::instant::island::NetIslandIndex;
use crate::semantic::basic::attr_keys::ElementClass;
use crate::semantic::common::IOType;
use crate::semantic::component::mc_pins::PwrDir;
use crate::semantic::component::McComponent;
use std::collections::HashMap;

/// PI-1: a sink pin's declared DC pair must carry a decoupling capacitor.
pub(crate) fn check_sink_pin_decoupling(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // The load terminals first — the cheap scan that decides whether this board
    // asks the question at all. No early return on an empty capacitor list below:
    // a board with sinks and no capacitor anywhere is this rule's firing case,
    // not a reason to stay silent (the lesson PI-2 measured on its own candidate
    // list). A board with no sink site, or none that declares a pair, is simply
    // not this rule's object.
    let sites = sink_sites(table);
    if sites.is_empty() {
        return;
    }
    let workspace = crate::definition_space().workspace_components();
    let defs: HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    let judged: Vec<(u32, u32, String, String)> = sites
        .into_iter()
        .filter_map(|(site, net)| {
            declared_pair(table, &defs, site).map(|(hot, ret)| (site, net, hot, ret))
        })
        .collect();
    if judged.is_empty() {
        return;
    }

    // The declared capacitors to look for, with the nets their legs land on.
    let mut caps: Vec<Vec<u32>> = Vec::new();
    for comp in table.get_components() {
        if comp.synthetic
            || comp.unselected
            || comp.not_fitted
            || comp.element_class != Some(ElementClass::Capacitive)
            || comp.pin_count != 2
        {
            continue;
        }
        let mut nets: Vec<u32> = Vec::new();
        for pin in table.get_pins_of(comp.id) {
            for &leg in table.nets_of(pin.id) {
                if !nets.contains(&leg) {
                    nets.push(leg);
                }
            }
        }
        if nets.len() == 2 {
            caps.push(nets);
        }
    }

    let idx = NetIslandIndex::build(table);
    for (site, sink_net, hot, ret) in judged {
        let covered = caps
            .iter()
            .flatten()
            .any(|&leg| same_node(table, &idx, sink_net, leg));
        if covered {
            continue;
        }
        let Some(entry) = table.get_entry(site) else {
            continue;
        };
        let (pos, uri) = super::entry_pos(entry);
        let name = super::net_name(table, sink_net);
        // The instance-side terminal for the message (`main.buck12.VIN.Vin`, not
        // the positional pin id `main.buck12.4`) — the same spelling convention
        // 6024's sink-nominal-mismatch uses.
        let base = entry
            .path
            .rsplit_once('.')
            .map(|(p, _)| p)
            .unwrap_or(&entry.path);
        let sink_path = format!("{base}.{hot}");
        results.push(NetCheckResult {
            check: "sink-pin-decoupling",
            severity: "warning",
            message: crate::errcodes::format_msg(
                crate::errcodes::SINK_PIN_NO_DECOUPLING,
                &[&sink_path, &name, &ret],
            ),
            net_name: name,
            code: crate::errcodes::SINK_PIN_NO_DECOUPLING,
            pos,
            uri,
        });
    }
}

/// Every load terminal the rule judges, as `(entry id, the net it draws from)`.
/// Two shapes, one law (§2.1: "component pin rows and module supply ports are
/// judged alike"):
///
/// * a `Pin` whose flat `pwr_dir` says `Snk` — the typed carry of the row that
///   owns it, so the return-side pin of that same row (which carries no
///   direction) never enters;
/// * a `Port` of an instantiated module whose `pwr_ports` row says `Snk` —
///   matched by the member spelling the port's own path carries.
fn sink_sites(table: &InstTable) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (id, entry) in table.iter() {
        if entry.io_type != IOType::Power {
            continue;
        }
        let member = entry.path.rsplit('.').next().unwrap_or("");
        let is_sink = match entry.kind {
            InstKind::Pin => {
                // A part marked `nc` at its instance site is not on the board
                // (U163, hbl `wm7121(NC)`): its pins draw nothing, so the pair
                // they declare feeds no load here. The family already filters
                // not-fitted parts on the element side — the capacitor
                // candidate below, thermal, PI-3, the window rules — and this
                // is the same read applied to the subject: an unmounted
                // alternative must not report a decoupling gap for a load that
                // does not exist. An abstract instance no variant materialized
                // is off the board the same way.
                let mounted = entry
                    .parent_id
                    .and_then(|pid| table.get_entry(pid))
                    .is_some_and(|c| !c.not_fitted && !c.unselected);
                mounted && entry.pwr_dir == Some(PwrDir::Snk)
            }
            InstKind::Port => entry.parent_id.is_some_and(|module| {
                table.power_decls().get(&module).is_some_and(|pi| {
                    pi.pwr_ports
                        .iter()
                        .any(|p| p.dir == PwrDir::Snk && p.hot == member)
                })
            }),
            _ => false,
        };
        if !is_sink {
            continue;
        }
        // A terminal off the board (an unwired pad) draws from nothing — that is
        // a floating-input matter, not a decoupling gap.
        let Some(net) = table.get_net_of(*id) else {
            continue;
        };
        out.push((*id, net.id));
    }
    out
}

/// The `(hot, ret)` members the pair of `site` is written with, or `None` when
/// the declaration names no pair at all (§2.1's unjudged shape) or the site's
/// declaration cannot be reached. The definition space is where a component's pin
/// contracts live — the same door 6027 uses.
fn declared_pair(
    table: &InstTable,
    defs: &HashMap<String, &McComponent>,
    site: u32,
) -> Option<(String, String)> {
    let entry = table.get_entry(site)?;
    let (hot, ret) = match entry.kind {
        InstKind::Pin => {
            let comp = table.get_entry(entry.parent_id?)?;
            let row = super::sink_contract_for(defs.get(&comp.class_name)?, entry)?;
            (row.hot.clone(), row.ret.clone()?)
        }
        InstKind::Port => {
            let member = entry.path.rsplit('.').next().unwrap_or("");
            let row = table
                .power_decls()
                .get(&entry.parent_id?)?
                .pwr_ports
                .iter()
                .find(|p| p.dir == PwrDir::Snk && p.hot == member)?;
            (row.hot.clone(), row.ret.clone()?)
        }
        _ => return None,
    };
    (!hot.is_empty() && !ret.is_empty()).then_some((hot, ret))
}

/// Whether two flat nets are one potential. The axis's read first — equal
/// effective class, so a sub-module leg and the parent's copper it lands on are
/// one fact — and when either side resolves no class, the junction test: two
/// segments sharing a point are one node (that is how the golden board's
/// `VMAIN_5V` pair, owned by no domain's rail, stays judgeable).
fn same_node(table: &InstTable, idx: &NetIslandIndex, a: u32, b: u32) -> bool {
    if a == b {
        return true;
    }
    let class = |n: u32| {
        let attr = idx.get(n)?;
        super::eff_class(table, idx, attr, &mut Vec::new())
    };
    if let (Some(x), Some(y)) = (class(a), class(b)) {
        return x.id == y.id;
    }
    let (Some(na), Some(nb)) = (table.get_net(a), table.get_net(b)) else {
        return false;
    };
    na.points.iter().any(|p| nb.points.contains(p))
}
