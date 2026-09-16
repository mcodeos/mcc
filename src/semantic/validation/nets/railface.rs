// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The declared-rail read shared by the two rules that judge an element **by the
//! rail pair it sits across** — PI-3 (`decouple.rs`, a capacitor's return leg) and
//! PWR-4b (`thermal.rs`, a shunt's dissipation at the rail's window).
//!
//! Both need the same three steps: resolve each net's **effective class**
//! ([`super::eff_class`], the axis's one read), project every declared DC rail onto
//! the classes its two members resolve to **in the scope that declares it**, and
//! find the rails on the part's own owning-scope chain whose hot member is the
//! class a leg reads. Reading it here once is what keeps the two verdicts on one
//! law: a name is only ever used to find the net a declaration wrote it for, never
//! to decide.
//!
//! The step that matters for both is the boundary walk: a part instantiated inside
//! a sub-module has two nets of its own, and the raw island attribution calls both
//! of them `Signal` (R4's measured hole) — the identical part in `main` is judged
//! and one module down goes silently green. Resolving each leg to a class and
//! comparing class ids (`vin.GND` ≡ the parent's `GND`) is what carries either
//! verdict across the boundary, and `EffClass` is the only read that does it. A
//! leg that reaches no class is never guessed past a declaration anchor (§1.3).

use super::window::PwrWindow;
use crate::instant::insttab::{InstKind, InstTable};
use crate::instant::island::{NetIslandIndex, NetRole};
use std::collections::HashMap;

/// One declared DC rail, projected onto the potential classes its two members
/// resolve to **in the scope that declares it** — the "rail of the domain D the
/// hot leg declares, and its explicit return member R" of the power-quality
/// design §2.3, read at class level.
pub(super) struct RailFace {
    /// The domain the rail row sits in: the world a hot leg must anchor to.
    pub(super) domain: String,
    /// Class ids the hot member resolves to in the declaring scope.
    pub(super) hot: Vec<String>,
    /// Class ids the return member resolves to. Empty = the member reads no class
    /// here, so this rail cannot adjudicate anything.
    pub(super) ret: Vec<String>,
    /// The return member as written, for messages.
    pub(super) ret_name: String,
    /// The rail's own declared promise (`v ± tol`), absent when the nominal does
    /// not decode — the same value `WindowDeriv`'s rail face carries, reached
    /// without a net name to match on.
    pub(super) win: Option<PwrWindow>,
}

/// Per owning module scope: net name → the class ids the nets of that name
/// resolve to. Both sides of either verdict (the rails' members and the parts'
/// legs) are name-free once this is built.
pub(super) fn scope_classes(
    table: &InstTable,
    idx: &NetIslandIndex,
) -> HashMap<u32, HashMap<String, Vec<String>>> {
    let mut out: HashMap<u32, HashMap<String, Vec<String>>> = HashMap::new();
    for net in table.get_nets() {
        let Some(attr) = idx.get(net.id) else {
            continue;
        };
        let Some(layer) = attr.module else {
            continue;
        };
        let Some(cls) = super::eff_class(table, idx, attr, &mut Vec::new()) else {
            continue;
        };
        let classes = out
            .entry(layer)
            .or_default()
            .entry(attr.name.clone())
            .or_default();
        if !classes.contains(&cls.id) {
            classes.push(cls.id.clone());
        }
    }
    out
}

/// Every declared DC rail, keyed by the module that declares it. Only `::DC`
/// rails list (`l1_rails`); an AC rail states no DC pair.
pub(super) fn declared_rails(
    table: &InstTable,
    classes: &HashMap<u32, HashMap<String, Vec<String>>>,
) -> HashMap<u32, Vec<RailFace>> {
    let mut out: HashMap<u32, Vec<RailFace>> = HashMap::new();
    for (id, pi) in table.power_decls() {
        if !table
            .get_entry(*id)
            .is_some_and(|e| matches!(e.kind, InstKind::Module))
        {
            continue;
        }
        let Some(scope) = classes.get(id) else {
            continue;
        };
        let mut list: Vec<RailFace> = Vec::new();
        for rail in pi.l1_rails() {
            // A rail whose hot member reads no class in its own scope can never be
            // matched, so it is not a candidate at all; one whose return member
            // reads none is kept (it still claims the hot leg) and adjudicates
            // nothing.
            let hot = scope.get(&rail.hot).cloned().unwrap_or_default();
            if hot.is_empty() {
                continue;
            }
            list.push(RailFace {
                domain: rail.domain.clone(),
                hot,
                ret: scope.get(&rail.ret).cloned().unwrap_or_default(),
                ret_name: rail.ret.clone(),
                win: rail.v.map(|v| PwrWindow::around(v, rail.tol)),
            });
        }
        if !list.is_empty() {
            out.insert(*id, list);
        }
    }
    out
}

/// The rails on `layer`'s chain (the part's own owning scope, then its ancestors)
/// whose hot member resolves to `hot_class` and whose domain is one of the worlds
/// that leg anchors — i.e. the declared pair(s) this leg sits across.
pub(super) fn rails_of_leg<'a>(
    table: &InstTable,
    rails: &'a HashMap<u32, Vec<RailFace>>,
    layer: u32,
    hot_class: &str,
    worlds: &[String],
) -> Vec<&'a RailFace> {
    let mut out = Vec::new();
    let mut cur = Some(layer);
    while let Some(id) = cur {
        if let Some(list) = rails.get(&id) {
            for face in list {
                if face.hot.iter().any(|c| c == hot_class)
                    && worlds.iter().any(|w| w == &face.domain)
                {
                    out.push(face);
                }
            }
        }
        cur = table.get_entry(id).and_then(|e| e.parent_id);
    }
    out
}

/// The declared rail a two-terminal element sits across, read at class level:
/// `Some((hot leg net, the rail's declared window))` when one leg's effective
/// class is a declared rail's hot member — on the part's chain, anchored to that
/// rail's domain — and the other leg lands on that rail's return member (by class
/// id, so a sub-module leg bound to the parent's return counts) or on a named
/// reference. `None` when the part sits across no declared rail: the same "no
/// declared face, no verdict" silence the family keeps everywhere else.
pub(super) fn across_a_declared_rail(
    table: &InstTable,
    idx: &NetIslandIndex,
    rails: &HashMap<u32, Vec<RailFace>>,
    nets: &[u32],
) -> Option<(u32, PwrWindow)> {
    for (i, &leg) in nets.iter().enumerate() {
        let other = nets.get(1 - i)?;
        let attr = idx.get(leg)?;
        let Some(layer) = attr.module else {
            continue;
        };
        let Some(cls) = super::eff_class(table, idx, attr, &mut Vec::new()) else {
            continue;
        };
        for face in rails_of_leg(table, rails, layer, &cls.id, &cls.worlds) {
            let Some(win) = face.win.clone() else {
                continue;
            };
            let ret_class = idx
                .get(*other)
                .and_then(|a| super::eff_class(table, idx, a, &mut Vec::new()));
            let on_ret = ret_class.is_some_and(|c| face.ret.contains(&c.id))
                || matches!(
                    idx.get(*other).map(|a| a.role),
                    Some(NetRole::Ret | NetRole::Reference)
                );
            if on_ret {
                return Some((leg, win));
            }
        }
    }
    None
}
