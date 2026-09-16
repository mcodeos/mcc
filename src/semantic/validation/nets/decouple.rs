// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PI-3 decoupling-return face (power-quality-design.md §2.3, ruled 2026-09-16).
//! A decoupling capacitor's two legs are **one declared DC pair**: the rail the
//! part sits across states that pair (`rail [hot, ret]::DC(…)`), and the
//! capacitor is the element whose whole job is to close the loop the rail
//! declares. A return leg that lands anywhere else is a structural position
//! error — the DC pair the part bridges is not the pair the board declares — so
//! it is an Error, and deliberately **not** filtered by `@class(analog)`: §9.1's
//! analog face is only where this bites most often.
//!
//! The candidate is read from the flat carries, never from a name
//! ([`InstEntry::element_class`] `Capacitive`, decoded once at flatten time from
//! the definition's spec table — the class, not the spelling). Both legs are read
//! through the net's **effective class** ([`super::eff_class`]), the axis's one
//! read: that is what makes the verdict hold across a module boundary (a part
//! instantiated in a sub-module, whose leg the parent layer owns, resolves to the
//! parent's class — the raw island attribution would call every sub-module net
//! unjudged and go silently green; that is R4's measured hole). The comparison is
//! class-to-class (`EffClass::id`), not name-to-name: the sub-module net `vin.GND`
//! and the parent's `GND` are one fact.
//!
//! The rail side is resolved in the scope that **declares** it — its two members
//! are looked up by the name written there — and only rails on the part's own
//! owning-scope chain are candidates, so a sibling scope's rail never supplies
//! the pair (the declaring layer is the part's layer or an ancestor, iron rule 1).
//!
//! Not judged, never guessed: a part that is not a declared capacitor, a part not
//! sitting across a declared rail at all (no rail on the chain whose hot member
//! resolves to the hot leg's class and whose domain the leg anchors — a capacitor
//! between two hot faces is not a decoupling placement either), a leg whose class
//! does not resolve, and a rail whose return member reads no class in its own
//! scope. Silence there is the family's standing rule (design §1.3: no declared
//! face, no verdict), not a pass.

use super::railface::{declared_rails, rails_of_leg, scope_classes, RailFace};
use super::NetCheckResult;
use crate::instant::insttab::{InstEntry, InstTable};
use crate::instant::island::NetIslandIndex;
use crate::semantic::basic::attr_keys::ElementClass;

/// PI-3: a decoupling capacitor's return leg must land on the return member of the
/// rail it sits across.
pub(crate) fn check_decoupling_return_face(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Candidate filter first: neither the island index nor the scope table is
    // built when no part asks the question.
    let candidates: Vec<&InstEntry> = table
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
    if candidates.is_empty() {
        return;
    }

    let idx = NetIslandIndex::build(table);
    let classes = scope_classes(table, &idx);
    let rails = declared_rails(table, &classes);
    if rails.is_empty() {
        return;
    }

    for comp in candidates {
        // The nets this capacitor's two terminals land on. One net only means a
        // terminal is unwired or the part bypasses itself — neither is a
        // decoupling placement.
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
        // Each leg's effective class and owning scope. A leg whose class does not
        // resolve is not judged (an unresolvable net's identity would come from an
        // ancestor world — never guessed past a declaration anchor).
        let mut legs: Vec<(u32, super::EffClass, u32)> = Vec::new();
        for &n in &nets {
            let Some(attr) = idx.get(n) else {
                continue;
            };
            let Some(layer) = attr.module else {
                continue;
            };
            let Some(cls) = super::eff_class(table, &idx, attr, &mut Vec::new()) else {
                continue;
            };
            legs.push((n, cls, layer));
        }
        if legs.len() != 2 || legs[0].1.id == legs[1].1.id {
            continue; // one class on both legs bridges nothing
        }
        // §2.3 judges the shape "one leg on the rail's hot, the other on r".
        // Exactly one hot leg, or there is no rail pair to compare against (a
        // capacitor across two hot faces is not a decoupling placement).
        let hot_legs: Vec<usize> = (0..2)
            .filter(|&i| {
                let (_, cls, layer) = &legs[i];
                !rails_of_leg(table, &rails, *layer, &cls.id, &cls.worlds).is_empty()
            })
            .collect();
        if hot_legs.len() != 1 {
            continue;
        }
        let h = hot_legs[0];
        let r = 1 - h;
        let (h_net, h_cls, h_layer) = &legs[h];
        let r_cls = &legs[r].1;
        // The declared pair(s) this hot leg sits across. A rail whose return member
        // reads no class in its own scope is not adjudicable: it still claims the
        // leg, but it states no pair to compare against.
        let pair: Vec<&RailFace> = rails_of_leg(table, &rails, *h_layer, &h_cls.id, &h_cls.worlds)
            .into_iter()
            .filter(|f| !f.ret.is_empty())
            .collect();
        if pair.is_empty() {
            continue;
        }
        if pair.iter().any(|f| f.ret.contains(&r_cls.id)) {
            continue; // the return leg is the declared return member
        }
        let (pos, uri) = table
            .get_entry(comp.id)
            .map(super::entry_pos)
            .unwrap_or((0, String::new()));
        let net_name = table
            .get_net(*h_net)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| comp.path.clone());
        results.push(NetCheckResult {
            check: "decoupling-return-face",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::DECOUPLING_RETURN_MISMATCH,
                &[&comp.path, &h_cls.id, &pair[0].ret_name, &r_cls.id],
            ),
            net_name,
            code: crate::errcodes::DECOUPLING_RETURN_MISMATCH,
            pos,
            uri,
        });
    }
}
