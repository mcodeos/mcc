// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! SN-2 a noisy face and a quiet/sensitive face sharing one DC ground bridge
//! (power-quality-design.md §3.2, ruling 9's reading of "no filtering intent").
//! A declared `@bridge` whose two ends are the **returns** of a noisy face
//! (§1.4 `@noise(noisy)`) and of a quiet/sensitive one (`@class(analog)`,
//! `@noise(quiet)`, `@noise(sensitive)`), carried by anything other than a
//! magnetic element, is the two faces' references meeting through plain copper:
//! a filter is what lets a quiet face keep its own reference while the two
//! coppers meet, so without one the plane the protected parts are measured
//! against sits straight on the noise source's return. Error — P7 lists it as
//! reportable, the same level as the axis's other declaration-vs-topology cuts.
//!
//! The two ends are read as **returns** by negating PI-2's supply-leg test: a
//! bridge whose end is a declared rail's hot member is a supply filter leg
//! ([`super::bridge`], `6037`), so a leg where *neither* end is a hot member is
//! the ground-side one this rule judges. Reading it as one predicate's two
//! outcomes is what keeps the pair from both claiming a leg, and it is what
//! makes the sharing real: a face's return member is the copper its parts close
//! over.
//!
//! Both ends are read at the **name** level — the clause names nets of the scope
//! that wrote it, and that scope's own DC rails say what each name is: a rail's
//! `hot` member, or the return of a domain whose §1.4 face the scope declares.
//! Reading the written name rather than the net's effective class is what lets
//! the rule judge §3.2's plainest form, a **direct copper tie** (ruling 9's
//! own words): when the tie merges the two coppers the class read collapses to
//! one class answering both faces, while the declaration still names two
//! returns. A name no rail of its scope writes, one that is some rail's hot
//! member, and one that is the return of two faces at once (a quiet face whose
//! own rail returns on the noisy copper) all leave the pair unjudged — silence,
//! never a guess (§1.3).
//!
//! **No filtering intent** (ruling 9, reading (a) negated, 2026-09-16) is read
//! from the leg's carrier — the two-terminal part whose legs land on exactly
//! these two classes, the design's bridge-carrier element: a magnetic element there is a
//! declared filter, so the bridge is not judged here (whether that filter is
//! complete is PI-2's verdict, ruling 11's partition — this rule reports the
//! tie, not the filter's adequacy). Reading (b) (the bridge has no load-side
//! decoupling) would report §3.2 and §2.2 for one cause and was rejected;
//! reading (c) (an explicit filtering declaration on the quiet side) has no
//! carrier at all. A leg no single element carries — bare copper, or a chain of
//! elements none of which spans it alone — is read the way the design reads a
//! bridge with no magnetic element: reported, and the message says no single
//! two-terminal element carries the leg.
//!
//! The carrier is the **leg's own** element: the clause's byte span in the
//! declaring scope's def file picks out which of the parts spanning the two
//! classes this leg carries (6022's per-leg match). Parallel legs each have
//! their own clause and their own carrier, so a magnetic element on the
//! neighbour leg is not this leg's filter — without that match one filtered
//! leg answers for its unfiltered twin, and the message names an arbitrary
//! element.
//!
//! Not judged, never guessed (§1.3): a `@couple` edge (a DC-blocking coupling
//! element is not a ground tie), a bridge that is a supply leg (either end a
//! rail's hot member — PI-2's object), one whose ends are both noisy or both
//! quiet, a name no DC rail of the declaring scope writes or one reaching two
//! faces, an end whose class does not resolve, and a leg carried by a magnetic
//! element.

use super::faces::{DomainFaces, Face as DomainFace};
use super::railface::scope_classes;
use super::NetCheckResult;
use crate::instant::insttab::{InstEntry, InstTable};
use crate::instant::island::NetIslandIndex;
use crate::semantic::basic::attr_keys::ElementClass;
use crate::semantic::module::pi::L1EdgeKind;
use std::collections::HashMap;

/// SN-2: a DC ground bridge between a noisy face's return and a quiet one's must
/// be carried by a filtering element.
pub(crate) fn check_shared_return_bridge(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // The declared ground bridges first: with none of them there is no leg to
    // judge, and neither the island index nor the scope tables are built.
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
    // §1.4's two faces — the read PI-2/PI-4/SN-1/SN-3 share.
    let faces = DomainFaces::read(table);
    if faces.is_empty() {
        return;
    }
    // What each declaring scope's own DC rails say about the names it writes: a
    // rail's hot member, or the return of a domain the scope declares a face on.
    let mut roles: HashMap<u32, HashMap<String, EndRole>> = HashMap::new();
    for (id, pi) in table.power_decls() {
        let mut map: HashMap<String, EndRole> = HashMap::new();
        for rail in pi.l1_rails() {
            map.entry(rail.hot.clone()).or_default().hot = true;
            let face = if faces.declares(*id, DomainFace::Noisy, &rail.domain) {
                Some(DomainFace::Noisy)
            } else if faces.declares(*id, DomainFace::Quiet, &rail.domain) {
                Some(DomainFace::Quiet)
            } else {
                None
            };
            // A rail whose domain claims no face names no side: it is not a
            // return this rule can weigh (the same silence PI-2 keeps).
            if let Some(f) = face {
                map.entry(rail.ret.clone())
                    .or_default()
                    .returns
                    .push((f, rail.domain.clone()));
            }
        }
        for role in map.values_mut() {
            // Sorted by the domain name so a name returning on two faces of the
            // same kind still picks one deterministically for the message.
            role.returns.sort_by(|a, b| a.1.cmp(&b.1));
            role.returns.dedup();
        }
        roles.insert(*id, map);
    }
    if roles.is_empty() {
        return;
    }
    let idx = NetIslandIndex::build(table);
    let classes = scope_classes(table, &idx);
    // Every two-terminal candidate: the leg's carrier, and with it the filtering
    // intent (an element class, never a name).
    let parts: Vec<&InstEntry> = table
        .get_components()
        .into_iter()
        .filter(|e| !e.synthetic && !e.unselected && !e.not_fitted && e.pin_count == 2)
        .collect();

    for (scope, edge) in bridges {
        // Both ends as the declaring scope's own rails read them; a name no rail
        // of that scope writes is no declared return, so there is nothing to
        // judge.
        let (Some(ra), Some(rb)) = (
            roles.get(&scope).and_then(|m| m.get(&edge.a)),
            roles.get(&scope).and_then(|m| m.get(&edge.b)),
        ) else {
            continue;
        };
        // PI-2's supply-leg test negated: a rail's hot member at either end makes
        // this the supply filter leg 6037 judges, not a ground bridge.
        if ra.hot || rb.hot {
            continue;
        }
        // One noisy side and one quiet/sensitive side. A name that is the return
        // of both faces at once (the quiet face whose own rail returns on the
        // noisy copper) names no single side, so the pair is not judged.
        let (Some((fa, da)), Some((fb, db))) = (sole_face(ra), sole_face(rb)) else {
            continue;
        };
        let (noisy, quiet) = match (fa, fb) {
            (DomainFace::Noisy, DomainFace::Quiet) => (da, db),
            (DomainFace::Quiet, DomainFace::Noisy) => (db, da),
            _ => continue,
        };
        // The leg's two classes: the carrier is the element spanning exactly
        // them. An end whose class does not resolve leaves the leg unlocatable —
        // an unresolvable end takes no verdict. Two ends resolving to *one*
        // class are the merged copper of a direct tie, which no single element
        // spans and the message names as such.
        let (Some(ca), Some(cb)) = (
            class_of(&classes, scope, &edge.a),
            class_of(&classes, scope, &edge.b),
        ) else {
            continue;
        };
        // Filtering intent: a magnetic element on this leg is a declared filter,
        // and its adequacy is PI-2's verdict.
        let candidates: Vec<&InstEntry> = if ca == cb {
            Vec::new()
        } else {
            parts
                .iter()
                .copied()
                .filter(|p| carries(table, &idx, p, &ca, &cb))
                .collect()
        };
        // …and it must be *this* leg's element. A parallel leg has its own
        // clause and its own carrier, so a magnetic part standing on the
        // neighbour is not this leg's filter — the pair match alone would let
        // one filtered leg answer for an unfiltered twin, and would name an
        // arbitrary element in the message. The clause's byte span in the
        // declaring scope's def file tells the legs apart (the same per-leg
        // match 6022 makes); a carrier whose sites are unreachable there
        // (library/func body) falls back to the pair match alone.
        let mdef = super::comp_def_uri(table, scope);
        let on_leg: Vec<&InstEntry> = candidates
            .iter()
            .copied()
            .filter(|p| on_clause_span(table, p, edge, mdef.as_deref()))
            .collect();
        let leg = if on_leg.is_empty() {
            candidates
        } else {
            on_leg
        };
        if leg
            .iter()
            .any(|p| p.element_class == Some(ElementClass::Magnetic))
        {
            continue;
        }
        let carrier = leg.first().copied();
        let written = format!("{} <-> {}", edge.a, edge.b);
        let via = match carrier {
            Some(p) => format!(
                "carried by the two-terminal element '{}' rather than by a magnetic element",
                p.path
            ),
            None => "carried by no single two-terminal element".to_string(),
        };
        let pos = edge.lo as u32;
        let uri = super::comp_def_uri(table, scope).unwrap_or_default();
        results.push(NetCheckResult {
            check: "shared-return-bridge",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::SHARED_RETURN_BRIDGE,
                &[&written, &noisy, &quiet, &via],
            ),
            net_name: edge.a.clone(),
            code: crate::errcodes::SHARED_RETURN_BRIDGE,
            pos,
            uri,
        });
    }
}

/// What one scope's declared DC rails say about a net name: whether it is a
/// rail's supply-side member, and which §1.4 faces it is the return of (with one
/// domain naming that face, for the message).
#[derive(Default)]
struct EndRole {
    hot: bool,
    returns: Vec<(DomainFace, String)>,
}

/// The one face this name is a return of, with the domain that names it. `None`
/// when the name is no return at all, or the return of two faces at once — the
/// pair would be a guess either way.
fn sole_face(role: &EndRole) -> Option<(DomainFace, String)> {
    let first = role.returns.first()?;
    if role.returns.iter().any(|(f, _)| *f != first.0) {
        return None;
    }
    Some(first.clone())
}

/// The class a name resolves to in one scope — the identity the leg's carrier is
/// matched against. `None` when the name reaches no class there.
fn class_of(
    classes: &HashMap<u32, HashMap<String, Vec<String>>>,
    scope: u32,
    name: &str,
) -> Option<String> {
    classes.get(&scope)?.get(name)?.first().cloned()
}

/// Whether any of this part's wiring sites lies inside the clause's byte span in
/// the declaring scope's def file — this part is the one *this* clause's leg
/// carries. The sites are the same ones 6022 reads ([`super::leg_sites`]).
fn on_clause_span(
    table: &InstTable,
    part: &InstEntry,
    edge: &super::DeclEdge,
    mdef: Option<&str>,
) -> bool {
    let Some(mdef) = mdef else {
        return false;
    };
    let pins = table.get_pins_of(part.id);
    super::leg_sites(part, &pins)
        .iter()
        .any(|(off, uri)| uri == mdef && edge.lo <= (*off as usize) && (*off as usize) < edge.hi)
}

/// Whether this two-terminal part's legs land on exactly the two classes the
/// bridge joins — the leg's carrier. A pad unwired or shorted onto the other
/// leg lands on fewer than two distinct classes and carries nothing.
fn carries(table: &InstTable, idx: &NetIslandIndex, part: &InstEntry, a: &str, b: &str) -> bool {
    let mut ids: Vec<String> = Vec::new();
    for pin in table.get_pins_of(part.id) {
        for &leg in table.nets_of(pin.id) {
            let Some(attr) = idx.get(leg) else {
                continue;
            };
            let Some(cls) = super::eff_class(table, idx, attr, &mut Vec::new()) else {
                continue;
            };
            if !ids.contains(&cls.id) {
                ids.push(cls.id);
            }
        }
    }
    ids.len() == 2 && ids.iter().any(|c| c == a) && ids.iter().any(|c| c == b)
}
