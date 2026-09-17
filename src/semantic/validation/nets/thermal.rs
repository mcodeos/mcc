// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PWR-4b package dissipation (package-thermal-design.md §3 and §7). PWR-4's
//! first half (6021) budgets a whole net's declared sink demand against a
//! declared source capacity; this half asks the same kind of question of one
//! element — the power it dissipates in place against the ceiling its own
//! package declares (`spec.power_rated`). Both sides are declared values in both
//! faces, so nothing is solved.
//!
//! **Two faces, one code.** The candidate is read from the flat carries, never
//! from a name: the class declares itself dissipating ([`InstEntry::element_class`]
//! `Resistive`, the ledger's certificate for `spec.resistance`), it is a
//! two-terminal element, and both its resistance and its rating decode as
//! quantities.
//!
//! * **Shunt** (design §3.1) — one leg on a declared rail hot face, the other on
//!   that rail's return or a named reference. That placement is what makes the
//!   rail's window the voltage *across the element*: `P = V²/R` with `V` the far
//!   corner `max(|lo|, |hi|)`. Read locally off the island roles first, then at
//!   class level across a module boundary (`railface`, R4's measured hole).
//! * **Series** (design §7) — the removal method: cut the element out and ask
//!   each leg whether it still carries a feed ([`ReachScan::fed_without`]).
//!   Exactly one leg losing its feed means the element really is the cut between
//!   a source and that side, and the current through it is the whole demand of
//!   the region that went dark: `P = I²R`, with the sum asked of the budget
//!   engine under the same removal so 6035 and 6021 read one copper the same
//!   way. No direction word is needed — removal gives the order, exactly as it
//!   does for PWR-5's series half (exposed-protection-design.md §8.3).
//!
//! Comparing against the declared rating as it stands is the design's ruling
//! (derating factor 1.0 — a multiplier needs temperature/package context this
//! layer has none of, and rail-contract-design.md §8.6 keeps one out of the
//! budget axis for the same reason). Exceeding it is an advisory Warning.
//!
//! Not judged, never guessed: a device with no declared rating (a class writing
//! `_`); a leg whose class does not resolve in its own scope, and a shunt whose
//! legs are both supply faces (a divider's middle leg is `Signal` — no identity
//! anchor, so its potential is not a declared fact either); a series element
//! that is **bypassed** — the supply reaches both ends without it, so it is not
//! a part in line and no current through it is a fact here; and a series element
//! whose downstream region declares no demand at all (`amp` is opt-in, so that
//! current is unknown rather than zero).

use super::budget_derive::BudgetLoadScan;
use super::railface;
use super::reach::ReachScan;
use super::window::{WindowDeriv, WindowState};
use super::NetCheckResult;
use crate::instant::insttab::{InstEntry, InstTable};
use crate::instant::island::{NetIslandIndex, NetRole};
use crate::semantic::basic::attr_keys::ElementClass;

/// PWR-4b: an element's dissipation in place must not exceed its own declared
/// package rating — the shunt face (`V²/R`) and the series face (`I²R`).
pub(crate) fn check_element_dissipation(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Candidate filter first: neither face builds an index or a walk when no
    // element actually asks a question.
    let candidates = dissipation_candidates(table);
    if candidates.is_empty() {
        return;
    }
    check_shunt_face(table, &candidates, results);
    check_series_face(table, &candidates, results);
}

/// The elements this rule judges at all — all four clauses are declared reads:
/// the class carries the dissipating certificate, it is a two-terminal element,
/// its resistance is a positive quantity, and it declares a rating to compare
/// against (an undeclared rating is never guessed at).
fn dissipation_candidates(table: &InstTable) -> Vec<&InstEntry> {
    table
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
        .collect()
}

/// The two nets this element's terminals land on, deduplicated. One net only
/// means the device bypasses itself (R02's face) or a terminal is unwired —
/// neither is an element in place.
fn two_legs(table: &InstTable, comp: &InstEntry) -> Option<(u32, u32)> {
    let mut nets: Vec<u32> = Vec::new();
    for pin in table.get_pins_of(comp.id) {
        for &leg in table.nets_of(pin.id) {
            if !nets.contains(&leg) {
                nets.push(leg);
            }
        }
    }
    match nets.len() {
        2 => Some((nets[0], nets[1])),
        _ => None,
    }
}

/// The shunt face: the rail this element sits across, as the window across the
/// element.
fn check_shunt_face(
    table: &InstTable,
    candidates: &[&InstEntry],
    results: &mut Vec<NetCheckResult>,
) {
    let idx = NetIslandIndex::build(table);
    let mut deriv = WindowDeriv::new(table);
    let classes = railface::scope_classes(table, &idx);
    let rails = railface::declared_rails(table, &classes);

    for comp in candidates {
        let (Some(r), Some(rated)) = (comp.resistance_ohm, comp.power_rated_w) else {
            continue;
        };
        let Some((net_a, net_b)) = two_legs(table, comp) else {
            continue;
        };
        // Read the pair locally off the island roles first — one leg `Hot`, the
        // other that rail's `Ret`/`Reference` — which is what makes `V` a
        // declared value rather than a derivation: a Resolved window is the
        // rail's own promise, and anything else (NoSupply/Unresolved) is not
        // judged.
        let role_of = |n: u32| idx.get(n).map(|a| a.role);
        let (hot, win) = match (role_of(net_a), role_of(net_b)) {
            (Some(NetRole::Hot), Some(NetRole::Ret | NetRole::Reference)) => {
                let WindowState::Resolved(w) = deriv.window_of_net(net_a) else {
                    continue;
                };
                (net_a, w)
            }
            (Some(NetRole::Ret | NetRole::Reference), Some(NetRole::Hot)) => {
                let WindowState::Resolved(w) = deriv.window_of_net(net_b) else {
                    continue;
                };
                (net_b, w)
            }
            // Neither leg carries a role of its own — which is exactly what every
            // part inside a sub-module looks like in its own scope. Read the pair
            // at class level instead: one leg on a declared rail's hot member and
            // the other on that rail's return, reached across the module boundary
            // by the effective-class walk, which also supplies the rail's own
            // declared window (R4's measured hole — this same shunt is judged in
            // `main` and was silently green one module down, so §3.2's shunt face
            // only ever held at the top layer).
            _ => match railface::across_a_declared_rail(table, &idx, &rails, &[net_a, net_b]) {
                Some((hot, w)) => (hot, w),
                None => continue, // not a shunt across one declared rail
            },
        };
        let v = win.lo.abs().max(win.hi.abs());
        let p = v * v / r;
        if p <= rated + 1e-9 {
            continue;
        }
        let cause = format!(
            "the rail window's far corner puts {} V across the {} Ω part",
            super::fmt_round(v),
            super::fmt_round(r)
        );
        report(table, comp, p, rated, cause, hot, results);
    }
}

/// The series face: the removal method. Cut this element out of the copper and
/// ask each end whether it still carries a feed; the end that **lost** its feed
/// is the load side, and by construction every bit of that side's current runs
/// through the element that fed it.
fn check_series_face(
    table: &InstTable,
    candidates: &[&InstEntry],
    results: &mut Vec<NetCheckResult>,
) {
    let reach = ReachScan::new(table);
    let mut load = BudgetLoadScan::new(table);

    for comp in candidates {
        let (Some(r), Some(rated)) = (comp.resistance_ohm, comp.power_rated_w) else {
            continue;
        };
        let Some((net_a, net_b)) = two_legs(table, comp) else {
            continue;
        };
        // "Lost its feed" is one predicate compared with itself: fed before the
        // cut, unfed after it. Both ends keeping their feed means the supply
        // reaches them without this element (it is bypassed — no current through
        // it is a fact here, and PWR-5's 6033 owns that shape); both ends losing
        // it means the sides cannot be told apart, so the attribution is refused
        // rather than guessed.
        let lost_a = reach.fed_intact(net_a) && !reach.fed_without(net_a, comp.id);
        let lost_b = reach.fed_intact(net_b) && !reach.fed_without(net_b, comp.id);
        let loaded = if lost_a && reach.fed_without(net_b, comp.id) {
            net_a
        } else if lost_b && reach.fed_without(net_a, comp.id) {
            net_b
        } else {
            continue;
        };
        // The demand of the region that went dark, asked of the budget engine
        // with this element cut out of the flood: 6021's own reading of the same
        // copper, so the two codes can never disagree about one net's demand.
        let i = load.region_demand_without(loaded, comp.id);
        if i <= 0.0 {
            continue; // no declared demand — an unknown current, not a zero one
        }
        let p = i * i * r;
        if p <= rated + 1e-9 {
            continue;
        }
        let cause = format!(
            "the {} A its downstream region draws passes through the {} Ω part",
            super::fmt_round(i),
            super::fmt_round(r)
        );
        report(table, comp, p, rated, cause, loaded, results);
    }
}

/// One 6035 row, anchored at the element. `cause` fills the message's reading
/// slot, so the one code reports both faces without either borrowing the
/// other's words.
fn report(
    table: &InstTable,
    comp: &InstEntry,
    p: f64,
    rated: f64,
    cause: String,
    net: u32,
    results: &mut Vec<NetCheckResult>,
) {
    let (pos, uri) = table
        .get_entry(comp.id)
        .map(super::entry_pos)
        .unwrap_or((0, String::new()));
    let net_name = table
        .get_net(net)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| comp.path.clone());
    results.push(NetCheckResult {
        check: "element-dissipation-over-rating",
        severity: "warning",
        message: crate::errcodes::format_msg(
            crate::errcodes::SHUNT_DISSIPATION_OVER_RATING,
            &[
                &comp.path,
                &super::fmt_round(p),
                &super::fmt_round(rated),
                &cause,
            ],
        ),
        net_name,
        code: crate::errcodes::SHUNT_DISSIPATION_OVER_RATING,
        pos,
        uri,
    });
}
