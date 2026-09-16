// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! PWR-5 protection-device placement (exposed-protection-design.md §4). A class
//! declares itself a protection element in its own definition body —
//! `protect = shunt` / `protect = series` — and the declaration is the only
//! witness: a fuse and an ordinary copper pass are structurally identical
//! two-terminal elements with no DC row, so no name table and no pin shape can
//! tell them apart (world-axioms §1 A1; the flat carry is
//! [`InstEntry::protection`], decoded once at flatten time). Two verdicts, both
//! Error:
//!
//! * **shunt** (`PROTECT_SHUNT_NO_REFERENCE` = 6032) — the device dumps the
//!   transient it exists for onto a reference, so at least one of its legs must
//!   land on a reference its scope (or an ancestor world) declares
//!   `@role(protective)`/`@role(earth)`. The leg is read through the net's
//!   effective class exactly like PWR-6/PWR-7 do, so a leg on a `@clamp` target,
//!   on the protective copper itself, or on a rail's return member all count
//!   (design §4: a clamp edge or a return member owns the leg). Whether the
//!   reference is a *legitimate* dump target (role correct, island
//!   single-point) stays PWR-7/PWR-8's verdict and is not repeated here.
//! * **series** (`PROTECT_SERIES_NOT_IN_PATH` = 6033) — the device says it
//!   carries the supply through itself, so it must be a two-terminal element
//!   with no DC row whose ends sit on two different nets, both on a supply tree.
//!   A DC row means it is a power face rather than raw copper; two ends on one
//!   net means it bypasses itself; an end off every supply tree means it
//!   protects nothing. The declared *order* (which side of the protected device
//!   the fuse sits on) is deferred — design §6 R4 owns it.
//!
//! Not adjudicated, never guessed: a leg whose class does not resolve in its own
//! scope, and a series device with an end on a return/reference net (a
//! protective-earth-bond PTC is in series on a *reference* path, a face §4 does
//! not rule — see the design's open rows).

use super::NetCheckResult;
use crate::instant::insttab::{InstEntry, InstKind, InstTable, ProtectionKind};
use crate::instant::island::{NetIslandIndex, NetRole};

/// The references each module instance declares with an explicit `@role`, by
/// instance id → class id (conduit bare name) → role. Built from the same
/// `McPowerDecls` layer PWR-7's role lookup uses, so the two rules read one
/// declaration plane.
fn declared_ref_roles(table: &InstTable) -> std::collections::HashMap<u32, std::collections::HashMap<String, String>> {
    let mut out: std::collections::HashMap<u32, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();
    for (id, pi) in table.power_decls() {
        if !table
            .get_entry(*id)
            .is_some_and(|e| matches!(e.kind, InstKind::Module))
        {
            continue;
        }
        let roles: std::collections::HashMap<String, String> = pi
            .l1_refs()
            .into_iter()
            .filter_map(|r| r.role.map(|role| (r.name, role)))
            .collect();
        if !roles.is_empty() {
            out.insert(*id, roles);
        }
    }
    out
}

/// The role that `cls` carries in `layer` or, failing that, in the nearest
/// ancestor world that declares it. A reference's identity is the copper, and
/// the world that supplies its role may sit above the scope that wires it
/// (iron rule 1 §6 — the same reason PWR-7 leaves a non-same-scope clamp target
/// alone), so the declaration is looked up along the module chain rather than in
/// the owning scope alone. A role word other than protective/earth is *not* a
/// dump target — returning it lets the caller report the leg honestly.
fn role_in_chain(
    table: &InstTable,
    roles: &std::collections::HashMap<u32, std::collections::HashMap<String, String>>,
    layer: u32,
    cls: &str,
) -> Option<String> {
    let mut cur = Some(layer);
    while let Some(id) = cur {
        if let Some(role) = roles.get(&id).and_then(|m| m.get(cls)) {
            return Some(role.clone());
        }
        cur = table.get_entry(id).and_then(|e| e.parent_id);
    }
    None
}

/// Every flatten-visible entry of a class that declares itself the given kind of
/// protection device, deduplicated by instance path (one device is reported
/// once, however many entries the flatten carries for it).
fn marked_components(table: &InstTable, kind: ProtectionKind) -> Vec<&InstEntry> {
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    table
        .iter()
        .filter(|(_, e)| e.kind == InstKind::Component && e.protection == Some(kind))
        .map(|(_, e)| e)
        .filter(|e| seen.insert(e.path.as_str()))
        .collect()
}

/// PWR-5 shunt half (design §4): a `protect = shunt` device must have a leg on a
/// protective/earth reference.
pub(crate) fn check_protect_shunt_reference(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let marked = marked_components(table, ProtectionKind::Shunt);
    if marked.is_empty() {
        return;
    }
    let roles = declared_ref_roles(table);
    let idx = NetIslandIndex::build(table);

    for comp in marked {
        // Every leg's effective class, and whether one of them reaches a
        // protective/earth reference in its scope chain.
        let mut classes: Vec<String> = Vec::new();
        let mut nets: Vec<String> = Vec::new();
        let mut wired = 0usize;
        let mut dumped = false;
        for pin in table.get_pins_of(comp.id) {
            for &leg in table.nets_of(pin.id) {
                wired += 1;
                let Some(net) = table.get_net(leg) else {
                    continue;
                };
                let Some(layer) = net.module else {
                    continue;
                };
                let Some(attr) = idx.get(net.id) else {
                    continue;
                };
                let Some(cls) = super::eff_class(table, &idx, attr, &mut Vec::new()) else {
                    continue;
                };
                nets.push(net.name.clone());
                if !classes.contains(&cls.id) {
                    classes.push(cls.id.clone());
                }
                if role_in_chain(table, &roles, layer, &cls.id)
                    .is_some_and(|r| r == "protective" || r == "earth")
                {
                    dumped = true;
                }
            }
        }
        if dumped {
            continue;
        }
        // Two silences, both the family's standing rule rather than a pass:
        // a device with no leg at all is the floating-input family's business,
        // and a leg whose class does not resolve is never guessed (net-island
        // §8: an unresolvable net's identity comes from an ancestor world).
        if wired == 0 || nets.is_empty() {
            continue;
        }
        let legs = classes.join(", ");
        let (pos, uri) = table
            .get_entry(comp.id)
            .map(super::entry_pos)
            .unwrap_or((0, String::new()));
        results.push(NetCheckResult {
            check: "protect-shunt-reference",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::PROTECT_SHUNT_NO_REFERENCE,
                &[&comp.path, &legs],
            ),
            net_name: nets[0].clone(),
            code: crate::errcodes::PROTECT_SHUNT_NO_REFERENCE,
            pos,
            uri,
        });
    }
}

/// PWR-5 series half (design §4): a `protect = series` device must be a
/// current-transparent two-terminal element in series on a supply path.
pub(crate) fn check_protect_series_path(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let marked = marked_components(table, ProtectionKind::Series);
    if marked.is_empty() {
        return;
    }
    let idx = NetIslandIndex::build(table);
    let mut reach = super::reach::ReachScan::new(table);

    for comp in marked {
        let pins = table.get_pins_of(comp.id);
        let mut ends: Vec<u32> = Vec::new();
        let mut reason: Option<String> = None;
        if pins.len() != 2 {
            reason = Some(format!(
                "it is not a two-terminal element ({} terminals)",
                pins.len()
            ));
        } else if pins.iter().any(|p| p.pwr_dir.is_some()) {
            // The device itself declares a DC row: a power face, not raw copper,
            // so the supply does not pass through it (§8.5's transparent-copper
            // test, read on the device rather than on the net).
            reason = Some(
                "it carries a DC power row, so it is not a current-transparent pass element"
                    .to_string(),
            );
        } else {
            for pin in &pins {
                for &leg in table.nets_of(pin.id) {
                    if !ends.contains(&leg) {
                        ends.push(leg);
                    }
                }
            }
            if ends.len() < 2 {
                let names: Vec<String> = ends
                    .iter()
                    .filter_map(|n| table.get_net(*n).map(|e| e.name.clone()))
                    .collect();
                reason = Some(format!(
                    "its two terminals do not separate two nets (they sit on {})",
                    if names.is_empty() {
                        "no net at all".to_string()
                    } else {
                        names.join(", ")
                    }
                ));
            }
        }

        // A return/reference end is a face §4 does not rule: a protective
        // earth-bond element (a PTC in the PE path) is in series on a reference
        // path, not on a supply tree. Not adjudicated rather than guessed.
        if reason.is_none()
            && ends.iter().any(|n| {
                matches!(
                    idx.get(*n).map(|a| a.role),
                    Some(NetRole::Ret) | Some(NetRole::Reference)
                )
            })
        {
            continue;
        }

        if reason.is_none() {
            // "On a supply tree" is the fed face 6019/PWR-1 already owns
            // (`ReachScan::has_supply_of`), not the 6021 budget root: a
            // capacity-less source boundary is deliberately Opaque to the
            // budget walk, so the budget root would call an ordinary fuse
            // downstream of a capacity-less source "off the supply tree".
            let unfed: Vec<String> = ends
                .iter()
                .filter(|n| !reach.has_supply_of(**n))
                .filter_map(|n| table.get_net(*n).map(|e| e.name.clone()))
                .collect();
            if !unfed.is_empty() {
                reason = Some(format!(
                    "end '{}' is on no supply tree",
                    unfed.join("', '")
                ));
            }
        }

        let Some(reason) = reason else {
            continue;
        };
        let (pos, uri) = table
            .get_entry(comp.id)
            .map(super::entry_pos)
            .unwrap_or((0, String::new()));
        let net_name = ends
            .first()
            .and_then(|n| table.get_net(*n))
            .map(|e| e.name.clone())
            .unwrap_or_else(|| comp.path.clone());
        results.push(NetCheckResult {
            check: "protect-series-path",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::PROTECT_SERIES_NOT_IN_PATH,
                &[&comp.path, &reason],
            ),
            net_name,
            code: crate::errcodes::PROTECT_SERIES_NOT_IN_PATH,
            pos,
            uri,
        });
    }
}
