// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U217 — the AC mains face gates (ac-interface-design.md §7), the ERC slice
//! of the `::AC.1P` landing. Three judges over the flat table, each reading
//! the flatten-time [`crate::instant::insttab::AcFaceCarry`] — the face a
//! row's own `::AC.*` declaration states — never a name, a spelling, or a
//! family table:
//!
//! - **the return gate** (`check_ac_face_return`, 6057): a direction-word
//!   `psrc/psnk/psbi` row declares a two-member face, and the face is the
//!   pair — a member wired while its partner reaches no net is a single-line
//!   supply. Judged per face, over the flat table rather than the nets,
//!   because the defect *is* the absent net. Unwired *pins* stay E4119's
//!   object; this gate judges declared supply faces only.
//! - **the nominal gate** (`check_ac_nominal_conflict`, 6058): two `::AC.*`
//!   faces on one copper stating different region nominals is a contract
//!   contradiction — 230 V / 50 Hz and 120 V / 60 Hz are not one mains. The
//!   DC twin is `check_voltage_mismatch` (the 0.5 V tolerance included); the
//!   frequency axis compares exactly. The empty form states no nominal and is
//!   outside the judge — a region-neutral face conflicts with nothing.
//! - **the protective-word gate** (`check_protective_pin_copper`, 6059): a
//!   pin row's `@role(protective)`/`@role(earth)` word demands protective
//!   copper — the net the pin lands on must touch a conductor its own scope
//!   declares with that role. This gives the identity-axis words their first
//!   net-side verdict (pin-expectation v0.1 gave only `quiet` one); an
//!   unwired pin stays the unwired-pin family's object.
//!
//! All three iterate `InstTable::iter_entries` (id order) and the net list,
//! sort their fires by anchor, and stay silent on exactly the shapes the
//! design doc rules silence: the all-dangling face, the region-neutral
//! meeting, the unwired protective pin.

use std::collections::{HashMap, HashSet};

use super::{NetCheckResult, entry_pos};
use crate::instant::insttab::{AcFaceMember, InstTable};

/// The words a pin's `@role(...)` carry can claim the protective identity
/// with. The closed vocabulary the role key already enforces at the write
/// site (E5360); the copper-identity reader shares the two words.
const PROTECTIVE_WORDS: [&str; 2] = ["protective", "earth"];

/// Per-axis mismatch tolerances: volts inherit the DC twin's 0.5 V band
/// (`NET_VOLTAGE_MISMATCH`), hertz compares exactly at f64 resolution.
const VOLT_TOL: f64 = 0.5;
const HZ_TOL: f64 = 1e-6;

/// The ids living on a net with at least two points — "wired" for every
/// gate here: a face member on a single-point net (or on no net at all)
/// reaches nothing.
fn wired_ids(table: &InstTable) -> HashSet<u32> {
    let mut wired = HashSet::new();
    for net in table.get_nets() {
        if net.points.len() >= 2 {
            wired.extend(net.points.iter().copied());
        }
    }
    wired
}

/// U217 §7 ②: an energized `::AC.*` face leaves its declared return
/// unconnected (`AC_FACE_RETURN_MISSING`, 6057). A face whose variant
/// declares no Neutral slot (the delta three-wire shape, `AC.3P3W`) states
/// no return conductor — its tear form is one wired member facing dangling
/// peers, judged by the same gate (U229, ac-delta-return-design.md §4.1).
pub(crate) fn check_ac_face_return(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let wired = wired_ids(table);

    // Face groups: (owner instance, row face name) -> the positional members.
    // The flatten site maps every member by its registry position, so a group
    // holds one Ret and every Hot the variant declares; a nominal-only carry
    // (component rows, member `None`) is not a positional face and never
    // enters a group.
    let mut faces: HashMap<(u32, String), Vec<(&crate::instant::insttab::InstEntry, AcFaceMember)>> =
        HashMap::new();
    for entry in table.iter_entries() {
        let Some(carry) = entry.ac_face.as_ref() else {
            continue;
        };
        let Some(member) = carry.member else {
            continue;
        };
        let Some(owner_id) = entry.parent_id else {
            continue;
        };
        faces
            .entry((owner_id, carry.face.clone()))
            .or_default()
            .push((entry, member));
    }

    // One fire per dangling member, anchored there; sorted for determinism.
    // The law generalizes over the group size (registry-driven positions):
    // an energized face is torn when the face's return dangles while a phase
    // is wired, or a phase dangles off a wired return — and a return-less
    // (delta) face is torn when exactly one member is wired against dangling
    // peers. Both-wired (healthy) and all-dangling (an unused face — the
    // silence law, the same shape the exclusive-peer gate keeps) stay silent.
    let mut fired: Vec<(&crate::instant::insttab::InstEntry, String, String, String, String)> =
        Vec::new();
    for ((_owner_id, _face), members) in faces {
        // Exactly one return slot per group (every registered variant states
        // exactly one Neutral), or no slot at all — the delta three-wire
        // shape, which the branch below judges.
        let mut ret_e: Option<&crate::instant::insttab::InstEntry> = None;
        let mut multi_ret = false;
        let mut hots: Vec<&crate::instant::insttab::InstEntry> = Vec::new();
        for (e, m) in &members {
            match m {
                AcFaceMember::Ret => {
                    if ret_e.is_some() {
                        multi_ret = true;
                    }
                    ret_e = Some(*e);
                }
                AcFaceMember::Hot => hots.push(*e),
            }
        }
        if multi_ret {
            // No registered variant states two Neutral slots; a face mapping
            // that way is not a declared shape — nothing here can tear.
            continue;
        }
        let mut fires: Vec<(&crate::instant::insttab::InstEntry, &crate::instant::insttab::InstEntry)> =
            Vec::new(); // (wired side, dangling side)
        match ret_e {
            Some(ret_e) => {
                if hots.is_empty() {
                    continue;
                }
                let ret_wired = wired.contains(&ret_e.id);
                let any_hot_wired = hots.iter().any(|e| wired.contains(&e.id));
                if !ret_wired && any_hot_wired {
                    // The first wired phase stands for the supply side in the
                    // message — one fire per face, not one per wired phase.
                    if let Some(wired_hot) = hots.iter().copied().find(|e| wired.contains(&e.id)) {
                        fires.push((wired_hot, ret_e));
                    }
                }
                if ret_wired {
                    for hot_e in hots.iter().copied() {
                        if !wired.contains(&hot_e.id) {
                            fires.push((ret_e, hot_e));
                        }
                    }
                }
            }
            None => {
                // A group without a Neutral slot states no return conductor —
                // the delta three-wire face (AC.3P3W): the return runs
                // phase-to-phase, so the tear form is a face with exactly one
                // wired member facing dangling peers — no loop closes, one
                // fire per dangling member anchored on the wired one. Two or
                // more wired members close a line-voltage loop (quiet); the
                // all-dangling face is the unused-declaration silence law.
                let wired_hots: Vec<&crate::instant::insttab::InstEntry> = hots
                    .iter()
                    .copied()
                    .filter(|e| wired.contains(&e.id))
                    .collect();
                if let [wired_e] = wired_hots[..] {
                    for hot_e in hots.iter().copied() {
                        if !wired.contains(&hot_e.id) {
                            fires.push((wired_e, hot_e));
                        }
                    }
                }
            }
        };
        for (wired_e, dangling_e) in fires {
            let (Some(wired_c), Some(dangling_c)) =
                (wired_e.ac_face.as_ref(), dangling_e.ac_face.as_ref())
            else {
                continue;
            };
            // The face display is the member entry's path minus its member
            // suffix — for a module row (`psnk mains{L, N}::…`) that path is
            // `PSU.mains`, which already names the face; appending the row
            // name again would read `PSU.mains.mains`.
            let face_display = dangling_e
                .path
                .strip_suffix(&format!(".{}", dangling_c.label))
                .unwrap_or(&dangling_e.path)
                .to_string();
            fired.push((
                dangling_e,
                face_display,
                format!("{}, {}", wired_c.label, dangling_c.label),
                wired_c.label.clone(),
                dangling_c.label.clone(),
            ));
        }
    }

    fired.sort_by(|a, b| entry_pos(a.0).cmp(&entry_pos(b.0)));
    for (dangling, face_display, labels, wired_label, dangling_label) in fired {
        let (pos, uri) = entry_pos(dangling);
        results.push(NetCheckResult {
            check: "ac-face-return",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::AC_FACE_RETURN_MISSING,
                &[
                    &face_display as &dyn std::fmt::Display,
                    &labels,
                    &wired_label,
                    &dangling_label,
                ],
            ),
            net_name: String::new(),
            code: crate::errcodes::AC_FACE_RETURN_MISSING,
            pos,
            uri,
        });
    }
}

/// U217 §7 ④: two `::AC.*` faces state different region nominals on one
/// copper (`AC_NOMINAL_CONFLICT`, 6058). One fire per axis per face pair:
/// a two-member face wires both members across two nets, and each net
/// repeats the same contradiction — the pair is one fact, deduped by the
/// two faces' identities (owner + row face).
pub(crate) fn check_ac_nominal_conflict(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // (entry standing for the fire, net name, axis, unit, first value,
    // other entry, other value) — one row per surviving fire.
    let mut fired: Vec<(
        &crate::instant::insttab::InstEntry,
        String,
        &'static str,
        &'static str,
        f64,
        &crate::instant::insttab::InstEntry,
        f64,
    )> = Vec::new();
    let mut seen: HashSet<(u32, String, u32, String, &'static str)> = HashSet::new();

    for net in table.get_nets() {
        let stated: Vec<_> = net
            .points
            .iter()
            .filter_map(|&pid| table.get_entry(pid))
            .filter_map(|e| {
                let carry = e.ac_face.as_ref()?;
                if carry.volts.is_none() && carry.hz.is_none() {
                    return None;
                }
                // A positional member's face identity is (owner, row face);
                // a member-less carry (component rows) falls back to the
                // entry itself, whose `parent_id` may be absent — the id
                // keeps the key unique.
                let owner = e.parent_id.unwrap_or(e.id);
                Some((e, carry, owner))
            })
            .collect();
        if stated.len() < 2 {
            continue;
        }
        // Per axis: when the stated values disagree, one fire naming both
        // sides. The first two disagreeing entries stand for the net — a
        // third voice repeats the contradiction, not a new fact.
        for (axis, unit, tol) in [
            ("voltage", "V", VOLT_TOL),
            ("frequency", "Hz", HZ_TOL),
        ] {
            let stated_axis: Vec<(&crate::instant::insttab::InstEntry, f64)> = stated
                .iter()
                .filter_map(|(e, c, _owner)| {
                    let v = if unit == "V" { c.volts } else { c.hz }?;
                    Some((*e, v))
                })
                .collect();
            if stated_axis.len() < 2 {
                continue;
            }
            let (first_e, first_v) = stated_axis[0];
            let Some((other_e, other_v)) = stated_axis[1..]
                .iter()
                .copied()
                .find(|(_, v)| (v - first_v).abs() > tol)
            else {
                continue;
            };
            // The dedupe key is the two faces' identities: owner instance +
            // row face, per axis — the same two faces meeting across the
            // member nets repeat one contradiction, not a new one.
            let face_of = |e: &crate::instant::insttab::InstEntry| {
                e.ac_face.as_ref().map(|c| c.face.clone()).unwrap_or_default()
            };
            let owner_of = |e: &crate::instant::insttab::InstEntry| e.parent_id.unwrap_or(e.id);
            let key = (
                owner_of(first_e),
                face_of(first_e),
                owner_of(other_e),
                face_of(other_e),
                axis,
            );
            if !seen.insert(key) {
                continue;
            }
            fired.push((
                first_e,
                net.name.clone(),
                axis,
                unit,
                first_v,
                other_e,
                other_v,
            ));
        }
    }

    fired.sort_by(|a, b| entry_pos(a.0).cmp(&entry_pos(b.0)));
    for (first_e, net_name, axis, unit, first_v, other_e, other_v) in fired {
        let (pos, uri) = entry_pos(first_e);
        results.push(NetCheckResult {
            check: "ac-nominal-conflict",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::AC_NOMINAL_CONFLICT,
                &[
                    &net_name as &dyn std::fmt::Display,
                    &axis as &dyn std::fmt::Display,
                    &first_e.path,
                    &format!("{} {}", fmt_val(first_v), unit),
                    &other_e.path,
                    &format!("{} {}", fmt_val(other_v), unit),
                ],
            ),
            net_name,
            code: crate::errcodes::AC_NOMINAL_CONFLICT,
            pos,
            uri,
        });
    }
}

/// Render a decoded f64 the way the author wrote it: no trailing `.0` for a
/// whole number (230 → "230", 50.0 → "50").
fn fmt_val(v: f64) -> String {
    if (v - v.trunc()).abs() < 1e-9 {
        format!("{}", v.trunc() as i64)
    } else {
        format!("{v}")
    }
}

/// U217 §7 ③: a pin's `@role(protective)`/`@role(earth)` word reaches no
/// protective conductor (`PROTECTIVE_PIN_NO_COPPER`, 6059).
pub(crate) fn check_protective_pin_copper(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    // Scope-declared conduit roles: module instance id -> (name -> role). The
    // same read the protective-reference gates make (`declared_ref_roles`).
    let mut conduit_roles: HashMap<u32, HashMap<String, String>> = HashMap::new();
    for (mod_id, pi) in table.power_decls() {
        for r in pi.l1_refs() {
            if let Some(role) = r.role {
                conduit_roles
                    .entry(*mod_id)
                    .or_default()
                    .insert(r.name, role);
            }
        }
    }

    let is_protective = |role: &str| PROTECTIVE_WORDS.contains(&role);

    // A net's protective conductor test: does any *other* endpoint of the net
    // sit on a conductor its owning scope declares @role(protective)/earth?
    // (A label/port entry whose parent module declares its name with the
    // role.) The judged pin's own word is the demand, never the witness.
    let touches_protective = |entry: &crate::instant::insttab::InstEntry,
                              peers: &[&crate::instant::insttab::InstEntry],
                              conduit_roles: &HashMap<u32, HashMap<String, String>>|
     -> bool {
        peers.iter().any(|other| {
            if other.id == entry.id {
                return false;
            }
            let Some(parent) = other.parent_id else {
                return false;
            };
            let name = other.path.rsplit_once('.').map(|(_, n)| n).unwrap_or(&other.path);
            conduit_roles
                .get(&parent)
                .and_then(|roles| roles.get(name))
                .is_some_and(|role| is_protective(role))
        })
    };

    let mut fired: Vec<(&crate::instant::insttab::InstEntry, String, String, String)> = Vec::new();
    for net in table.get_nets() {
        if net.points.len() < 2 {
            // An unwired pin is the unwired-pin family's object, never ours.
            continue;
        }
        let entries: Vec<_> = net
            .points
            .iter()
            .filter_map(|&pid| table.get_entry(pid))
            .collect();
        for entry in &entries {
            let Some(role) = entry
                .exp_role
                .iter()
                .find(|w| is_protective(w))
                .cloned()
            else {
                continue;
            };
            if !touches_protective(entry, &entries, &conduit_roles) {
                fired.push((entry, net.name.clone(), role, entry.path.clone()));
            }
        }
    }

    fired.sort_by(|a, b| entry_pos(a.0).cmp(&entry_pos(b.0)));
    for (entry, net_name, role, path) in fired {
        let (pos, uri) = entry_pos(entry);
        results.push(NetCheckResult {
            check: "protective-pin-copper",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::PROTECTIVE_PIN_NO_COPPER,
                &[&path as &dyn std::fmt::Display, &role, &net_name],
            ),
            net_name,
            code: crate::errcodes::PROTECTIVE_PIN_NO_COPPER,
            pos,
            uri,
        });
    }
}
