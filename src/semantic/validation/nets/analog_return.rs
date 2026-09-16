// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! SN-1 analog signal crossing a split ground (power-quality-design.md §3.1).
//! A port row that claims the quiet/sensitive face (§1.4 — `@class(analog)`,
//! `@noise(quiet)`, `@noise(sensitive)`) and names its reference with
//! `@return(C)` states which plane the scope's analog signals are measured
//! against. The parts that face protects are the ones **supplied from** that
//! face, so each of their declared DC pairs must return over that reference: a
//! part drawing from the analog face but returning elsewhere is an analog
//! signal whose reference is not the declared one. Error, as design §3.1 lists
//! it — the same level as PWR-5/6, a declaration the topology contradicts.
//!
//! §3.1 drafts this as "the sink-side part", and reading the part by what
//! supplies it is what makes that reachable: the flat table carries no
//! source→sink chain for a signal net (model A: a signal net has no return of
//! its own), so the part is found by its own supply pair, not by tracing the
//! signal. The subject is then fixed by **two agreeing declarations**: the
//! face's own rail (`rail [hot, R]::DC(…)`, read as written in the scope that
//! declares the face) and the port row's `@return(C)`, which must name `R`. That
//! guard is what keeps the verdict per-face and hole-free: a scope declaring two
//! quiet faces with different references has one declaration per face, so a port
//! naming the first face's reference never judges the second face's parts. What
//! the rule measures is the *topology* against that agreed declaration — the
//! **effective class** of the net the part's return member lands on
//! ([`super::eff_class`], the axis's one read), the same class-vs-class
//! comparison PI-3 makes, so a part whose return the parent layer owns still
//! counts.
//!
//! The declaring scope is the scope that owns the part's supply-side net
//! ([`NetIslandIndex`]'s `module`) and the face is pinned to it
//! ([`DomainFaces::declares`], no chain walk): the declaration, the face and the
//! part are read at one level, so an ancestor's analog face never answers for a
//! descendant's port row. A part whose supply pair resolves to another scope's
//! face than the scope owning its net is the same missing link the deferred half
//! of §3.1 waits on (§6 R3), and is not judged.
//!
//! The other half of §3.1 — a port with **no** `@return` whose two sides return
//! on different conduits — is deferred by the design itself (§6 R3: the
//! source→sink chain has no carrier), so silence here is not a green. A port
//! whose `@return` names no reference its scope's quiet faces declare is the
//! same kind of undeclared subject: the declaration contradicts no wiring this
//! rule can read, and judging it would mean guessing which face it meant.
//!
//! Not judged, never guessed (§1.3): a port row that declares no reference or
//! claims no §1.4 face, a scope whose ports declare no analog reference at all,
//! a part with no declared DC pair, a pair whose members cannot both be located
//! on the flat table, a hot member whose net anchors no world this scope
//! declares quiet, a quiet face carrying no `::DC` rail or a rail whose return
//! member is not the reference the port declares, a part whose supply net
//! belongs to another scope than the one declaring the face, a return leg whose
//! effective class does not resolve, and a return that lands on the declared
//! reference (the honoured case).

use super::faces::{face_of_words, DomainFaces, Face as DomainFace};
use super::NetCheckResult;
use crate::instant::insttab::InstTable;
use crate::instant::island::NetIslandIndex;
use crate::semantic::component::McComponent;
// `Face` in this family's leaf modules is [`pwrid::Face`] — which side of a
// declared pair a member is; §1.4's quiet/noisy face is a different question,
// asked of a world (imported above as `DomainFace`).
use crate::semantic::pwrid::Face;
use std::collections::{HashMap, HashSet};

/// SN-1: the returns of the parts a scope's analog face protects must close over
/// the reference that face and the scope's analog port rows agree on.
pub(crate) fn check_analog_return_reference(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let idx = NetIslandIndex::build(table);
    let faces = DomainFaces::read(table);
    if faces.is_empty() {
        return;
    }
    // Per declaring scope: the references its analog port rows name, and the
    // return member each of its face worlds declares. A row writing
    // `io MIC{P, N}` projects one port per member and states one reference, so
    // the references are a set — the object is the declaration, not the member.
    let mut declared: HashMap<u32, HashSet<String>> = HashMap::new();
    let mut face_ret: HashMap<(u32, String), String> = HashMap::new();
    for (id, pi) in table.power_decls() {
        let mut refs: HashSet<String> = HashSet::new();
        for p in pi.l1_ports() {
            let Some(c) = p.ret else {
                continue;
            };
            if face_of_words(p.class.as_deref(), p.noise.as_deref()) != Some(DomainFace::Quiet) {
                continue;
            }
            refs.insert(c);
        }
        if refs.is_empty() {
            continue;
        }
        for rail in pi.l1_rails() {
            if rail.ret.is_empty() {
                continue; // no return member — the face declares no pair
            }
            face_ret.insert((*id, rail.domain.clone()), rail.ret.clone());
        }
        declared.insert(*id, refs);
    }
    if declared.is_empty() {
        return;
    }
    let workspace = crate::definition_space().workspace_components();
    let defs: HashMap<String, &McComponent> = workspace
        .iter()
        .map(|(sn, c)| (sn.ident.to_string(), c.as_ref()))
        .collect();
    if defs.is_empty() {
        return;
    }
    // One report per part *and* reference: a part whose definition writes the
    // same pair twice (two pin groups of one rail) has one defect, not two, and
    // the message would otherwise name it twice.
    let mut seen: HashSet<(String, String, String, String)> = HashSet::new();
    for comp in table.get_components() {
        if comp.synthetic || comp.unselected || comp.not_fitted {
            continue;
        }
        let Some(def) = defs.get(&comp.class_name) else {
            continue;
        };
        for row in &def.pins.pwr {
            let Some(ret_member) = &row.ret else {
                continue;
            };
            // Both legs of *this* row, located on the instance by the declared
            // member carry the flatten pass wrote from the same row.
            let (Some(hot_net), Some(ret_net)) = (
                member_net(table, comp.id, Face::Hot, &row.hot),
                member_net(table, comp.id, Face::Ret, ret_member),
            ) else {
                continue;
            };
            let (Some(hot_attr), Some(ret_attr)) = (idx.get(hot_net), idx.get(ret_net)) else {
                continue;
            };
            // The supply half anchors the domain the part belongs to; the return
            // half is the landing this rule measures. Both are read at class
            // level, so the verdict crosses a module boundary.
            let (Some(hot_cls), Some(ret_cls)) = (
                super::eff_class(table, &idx, hot_attr, &mut Vec::new()),
                super::eff_class(table, &idx, ret_attr, &mut Vec::new()),
            ) else {
                continue;
            };
            // The scope that owns the part's supply-side net is the scope whose
            // declaration binds it — one level, the same one the face is read at.
            let Some(scope) = hot_attr.module else {
                continue;
            };
            let Some(refs) = declared.get(&scope) else {
                continue;
            };
            for world in &hot_cls.worlds {
                if !faces.declares(scope, DomainFace::Quiet, world) {
                    continue;
                }
                // The two declarations must agree before the wiring is judged:
                // the face's rail states the reference as `ret`, the port row as
                // `@return`.
                let Some(want) = face_ret.get(&(scope, world.clone())) else {
                    continue;
                };
                if !refs.contains(want) || &ret_cls.id == want {
                    continue;
                }
                let key = (
                    comp.path.clone(),
                    row.hot.clone(),
                    ret_member.clone(),
                    ret_cls.id.clone(),
                );
                if !seen.insert(key) {
                    continue;
                }
                let scope_path = table
                    .get_entry(scope)
                    .map(|e| e.path.clone())
                    .unwrap_or_default();
                let (pos, uri) = super::entry_pos(comp);
                results.push(NetCheckResult {
                    check: "analog-return-reference",
                    severity: "error",
                    message: crate::errcodes::format_msg(
                        crate::errcodes::ANALOG_RETURN_MISMATCH,
                        &[&comp.path, world, &ret_cls.id, &scope_path, want],
                    ),
                    net_name: ret_cls.id.clone(),
                    code: crate::errcodes::ANALOG_RETURN_MISMATCH,
                    pos,
                    uri,
                });
            }
        }
    }
}

/// The net the instance pin carrying the declared member `(face, member)` lands
/// on. The pairing is the flatten pass's own ([`InstEntry::pwr_member`], written
/// from the row that owns the pin), so two pins of one instance answer the same
/// member only when one declaration named them both.
fn member_net(table: &InstTable, comp: u32, face: Face, member: &str) -> Option<u32> {
    for pin in table.get_pins_of(comp) {
        let Some(m) = &pin.pwr_member else {
            continue;
        };
        if m.face == face && m.member == member {
            return table.get_net_of(pin.id).map(|n| n.id);
        }
    }
    None
}
