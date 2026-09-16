// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! SN-3 sensitive return landing on a noisy face (power-quality-design.md §3.3,
//! ruling 10 decided 2026-09-16). A part supplied from a **quiet/sensitive**
//! face (§1.4) returns through the reference of a **noisy** one: the hot member
//! of one of its declared DC pairs sits on the protected face, and that same
//! pair's return member lands on the noisy face's copper. The plane a protected
//! part returns to *is* part of its protection — that is the reference its
//! sensitive signal is measured against — so returning it onto a noise source's
//! own reference puts the signal straight back onto the copper the quiet face
//! was isolating it from. Error, as design §3.3 lists it.
//!
//! The subject is a **declared pair of a part**, never a name. The pair is the
//! part's own `pins.pwr` row, and both members are read from that one row, so
//! the return judged is the return *of the pair that was declared*; the two
//! faces are read from the net's own attribution (`NetAttribution::worlds`
//! against the words the declaring scopes wrote) through [`super::faces`] — the
//! one §1.4 read this family's four face rules share, so PI-2, PI-4, SN-2 and
//! this rule cannot drift apart on what makes a face quiet.
//!
//! A part whose definition declares no pair carries no witness here, and that
//! is exactly where the family's seams are: a two-terminal passive's return
//! placement is PI-3's object (§2.3, `6038` — a capacitor's return is the pair
//! its rail declares, not a supply row of its own), and a return that *bridges*
//! the two faces rather than landing on one is SN-2's (§3.2 — this rule judges
//! the direct landing: no bridge, hard error, which is why §7 lands it first).
//!
//! The seam with 6027 is structural, not a matter of wording (ruling 10's
//! measurement): 6027 judges the return pins of a ≥3-pin device **spanning ≥2
//! classes**, so this rule's target shape — a sensitive part's single return pin
//! landing on one wrong class — is no span at all and 6027 is silent there.
//! Where the two do coincide the witnesses differ (6027 reports the class pair
//! and the device, this reports the protected part and the noisy face it returns
//! into), which is why ruling 10 cut them as two codes.
//!
//! Not judged, never guessed (§1.3, the family's standing rule): a part with no
//! declared pair, a row whose pair names no return member (a single-phase AC
//! shape declares no pair — axis ④'s object), a pair whose two members cannot
//! both be located on the flat table, a hot member whose net anchors no
//! quiet/sensitive world, a return member whose net anchors no noisy world, and
//! a net whose owning scope declares nothing at all.

use super::faces::DomainFaces;
// `Face` in this family's leaf modules is [`pwrid::Face`] (which side of a pair a
// member is); §1.4's quiet/noisy face is a different question, asked of a world.
use super::faces::Face as DomainFace;
use super::NetCheckResult;
use crate::instant::insttab::InstTable;
use crate::instant::island::NetIslandIndex;
use crate::semantic::component::McComponent;
use crate::semantic::pwrid::Face;
use std::collections::{HashMap, HashSet};

/// SN-3: a part supplied from a quiet/sensitive face must not return into a
/// noisy one.
pub(crate) fn check_sensitive_return_on_noisy(
    table: &InstTable,
    results: &mut Vec<NetCheckResult>,
) {
    // The faces first: a board whose scopes declare neither face has nothing to
    // ask, and neither has this rule.
    let faces = DomainFaces::read(table);
    if faces.is_empty() {
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
    let idx = NetIslandIndex::build(table);
    // One report per *pair as declared*: a part whose definition writes the same
    // `[hot, ret]` pair twice (two pin groups of one rail) has one defect, not
    // two, and the message would name it twice.
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
                super::member_net_of(table, comp.id, Face::Hot, &row.hot),
                super::member_net_of(table, comp.id, Face::Ret, ret_member),
            ) else {
                continue;
            };
            // The protected side is the supply half: the part belongs to the
            // quiet face because what feeds it is declared there.
            let Some(quiet) = face_world(table, &idx, &faces, hot_net, DomainFace::Quiet) else {
                continue;
            };
            let Some(noisy) = face_world(table, &idx, &faces, ret_net, DomainFace::Noisy) else {
                continue;
            };
            let ret_net_name = super::net_name(table, ret_net);
            let key = (
                comp.path.clone(),
                row.hot.clone(),
                ret_member.clone(),
                ret_net_name.clone(),
            );
            if !seen.insert(key) {
                continue;
            }
            let (pos, uri) = super::entry_pos(comp);
            results.push(NetCheckResult {
                check: "sensitive-return-noisy",
                severity: "error",
                message: crate::errcodes::format_msg(
                    crate::errcodes::SENSITIVE_RETURN_ON_NOISY,
                    &[
                        &comp.path,
                        &quiet,
                        ret_member,
                        &ret_net_name,
                        &noisy,
                    ],
                ),
                net_name: ret_net_name,
                code: crate::errcodes::SENSITIVE_RETURN_ON_NOISY,
                pos,
                uri,
            });
        }
    }
}

/// §1.4's face for a flat net, as the scope chain that owns it resolves it: the
/// world of `face` its declared worlds anchor, or `None`. A net whose
/// attribution resolves no owning scope, and a net whose worlds no scope on the
/// chain declares, anchor no face — silence, never a guess (§1.3).
fn face_world(
    table: &InstTable,
    idx: &NetIslandIndex,
    faces: &DomainFaces,
    net: u32,
    face: DomainFace,
) -> Option<String> {
    let attr = idx.get(net)?;
    let layer = attr.module?;
    faces.world_of(table, face, layer, &attr.worlds)
}
