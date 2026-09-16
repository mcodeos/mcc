// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PWR-4b package dissipation (package-thermal-design.md §3). PWR-4's first half
//! (6021) budgets a whole net's declared sink demand against a declared source
//! capacity; this half asks the same kind of question of one element — the power
//! it dissipates in place against the ceiling its own package declares
//! (`spec.power_rated`). Both sides are declared values, so nothing is solved.
//!
//! Only the **shunt** placement is judged. The candidate is read from the flat
//! carries, never from a name: the class declares itself dissipating
//! ([`InstEntry::element_class`] `Resistive`, the ledger's certificate for
//! `spec.resistance`), it is a two-terminal element, and both its resistance and
//! its rating decode as quantities. The placement is then read off the net
//! island index — one leg on a declared rail hot face, the other on that rail's
//! return or a named reference — which is what makes the rail's window the
//! voltage *across the element* rather than a number the engine had to derive:
//! `P = V²/R` with `V` the far corner `max(|lo|, |hi|)` of that window.
//! Comparing against the declared rating as it stands is the design's ruling
//! (derating factor 1.0 — a multiplier needs temperature/package context this
//! layer has none of, and rail-contract-design.md §8.6 keeps one out of the
//! budget axis for the same reason). Exceeding it is an advisory Warning.
//!
//! Not judged, never guessed: a **series** pass element (the engine reads a
//! two-terminal device with no DC row as current-transparent copper, so both
//! its legs carry one window and neither the volts across it nor a per-element
//! current exists — design §3.2 R2), a device with no declared rating (a class
//! that writes `_`), a leg whose class does not resolve in its own scope, and a
//! shunt whose legs are both supply faces (a divider's middle leg is `Signal` —
//! no identity anchor, so its potential is not a declared fact either).

use super::window::{WindowDeriv, WindowState};
use super::NetCheckResult;
use crate::instant::insttab::{InstEntry, InstTable};
use crate::instant::island::{NetIslandIndex, NetRole};
use crate::semantic::basic::attr_keys::ElementClass;

/// PWR-4b: a shunt element's dissipation at the rail window's worst corner must
/// not exceed its own declared package rating.
pub(crate) fn check_shunt_dissipation(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Candidate filter first: the window engine is only built when a device
    // actually asks a question.
    let candidates: Vec<&InstEntry> = table
        .get_components()
        .into_iter()
        .filter(|e| {
            !e.synthetic
                && !e.unselected
                && !e.not_fitted
                && e.element_class == Some(ElementClass::Resistive)
                && e.pin_count == 2
                && e.resistance_ohm.is_some_and(|r| r > 0.0)
                && e.power_rated_w.is_some()
        })
        .collect();
    if candidates.is_empty() {
        return;
    }

    let idx = NetIslandIndex::build(table);
    let mut deriv = WindowDeriv::new(table);

    for comp in candidates {
        let (Some(r), Some(rated)) = (comp.resistance_ohm, comp.power_rated_w) else {
            continue;
        };
        // The nets this element's two terminals land on. One net only means the
        // device bypasses itself (R02's face) or a terminal is unwired — neither
        // is a shunt in place.
        let mut nets: Vec<u32> = Vec::new();
        for pin in table.get_pins_of(comp.id) {
            for &leg in table.nets_of(pin.id) {
                if !nets.contains(&leg) {
                    nets.push(leg);
                }
            }
        }
        if nets.len() != 2 {
            continue;
        }
        let role_of = |n: u32| idx.get(n).map(|a| a.role);
        let hot = match (role_of(nets[0]), role_of(nets[1])) {
            (Some(NetRole::Hot), Some(NetRole::Ret | NetRole::Reference)) => nets[0],
            (Some(NetRole::Ret | NetRole::Reference), Some(NetRole::Hot)) => nets[1],
            _ => continue, // not a shunt across one declared rail
        };
        // The window of the hot leg is the rail's own declared promise: a
        // Resolved state is what makes `V` a declared value rather than a
        // derivation. Anything else (NoSupply/Unresolved) is not judged.
        let WindowState::Resolved(win) = deriv.window_of_net(hot) else {
            continue;
        };
        let v = win.lo.abs().max(win.hi.abs());
        let p = v * v / r;
        if p <= rated + 1e-9 {
            continue;
        }
        let (pos, uri) = table
            .get_entry(comp.id)
            .map(super::entry_pos)
            .unwrap_or((0, String::new()));
        let net_name = table
            .get_net(hot)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| comp.path.clone());
        results.push(NetCheckResult {
            check: "shunt-dissipation-over-rating",
            severity: "warning",
            message: crate::errcodes::format_msg(
                crate::errcodes::SHUNT_DISSIPATION_OVER_RATING,
                &[
                    &comp.path,
                    &super::fmt_round(p),
                    &super::fmt_round(v),
                    &super::fmt_round(r),
                    &super::fmt_round(rated),
                ],
            ),
            net_name,
            code: crate::errcodes::SHUNT_DISSIPATION_OVER_RATING,
            pos,
            uri,
        });
    }
}
