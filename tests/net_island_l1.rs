// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Net-island attribution index — L1 (island-attribution-design.md §5/§7).
//!
//! The flat index resolves each net against *its owning module's own*
//! declaration set (conduits + domain rails), keyed by the new
//! `NetEntry.module` field. This replaces the historical global hot-name
//! reverse-lookup (last-segment guess + cross-scope `remove` on collision)
//! with per-scope resolution: parent and child may carry a net of the same
//! name and each resolves against its own scope's declaration set.
//!
//! L1 scope is deliberately narrow (design §7 L1 bullet): identity is anchored
//! only on owning-scope declarations — a net whose name equals an owning-scope
//! `conduit` (copper) or a declared rail `hot`/`ret` member (supply face).
//! Everything else (derived supply faces, dotted pass-through members, bare
//! undeclared grounds, signal/anonymous wires) stays `resolvable = false`;
//! the index never guesses past the owning scope, and no rule consumes it yet
//! (golden residuals stay verbatim).

mod common;

use mcc::{McIds, McURI};

/// A two-pin passive, declared in-file so the tests don't depend on the
/// installed mcode library.
const FB: &str = "component FB {\n    pins = [\n        io [1,2] = [X, Y]\n    ]\n}\n";

/// Build a two-module board (top `main` + a child `p` of def `power`) and
/// return the flat table + island index.
fn build_islands(src: &str) -> (mcc::InstTable, mcc::NetIslandIndex) {
    let _lock = common::lock();
    common::reset();
    let uri: McURI = "/mcc/net-island-l1.mc".to_string();
    mcc::mcc_load_from_string(&uri, src);
    let (_, table) = mcc::mcc_build_flat(&McIds::from("main"), &uri, 1000).expect("flat build");
    let idx = mcc::NetIslandIndex::build(&table);
    (table, idx)
}

/// Attribution of the single net whose owning scope path is `scope` and whose
/// name is `name` (panics loudly when zero or several match).
fn attr_of<'a>(
    table: &mcc::InstTable,
    idx: &'a mcc::NetIslandIndex,
    scope: &str,
    name: &str,
) -> &'a mcc::NetAttribution {
    let mut hits: Vec<&mcc::NetAttribution> = idx
        .nets()
        .filter(|a| {
            a.name == name
                && a.module
                    .is_some_and(|m| table.get_entry(m).map(|e| e.path == scope).unwrap_or(false))
        })
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one net {name:?} in scope {scope:?}; got {hits:?}"
    );
    hits.remove(0)
}

/// Owning scope of a module entry id, as a path.
fn module_path(table: &mcc::InstTable, id: u32) -> String {
    table
        .get_entry(id)
        .map(|e| e.path.clone())
        .unwrap_or_else(|| format!("#{id}"))
}

#[test]
fn rail_hot_and_return_nets_anchor_to_declared_supply_faces() {
    // Mirrors the golden main shape (main.mc ①/②) on a small scale: a main
    // root GND, a quiet return GNDA (AVDD rail) and a protective ESDGND, each
    // DC-bridged so the role-coherency ERC stays quiet. Attribution must come
    // from the owning module's own decl set — hot faces get their declaring
    // domain, rail returns get their return copper + domain, and a named
    // copper no rail returns to is a worldless Reference.
    //
    // (The flat layer no longer re-partitions bare ground labels into
    // per-line `@N` fragments, so bare and rail-member grounds alike reach
    // the index as intact nets; the
    // quiet/protective names below are the real-board hand-check in the
    // the batch log.)
    let src = format!(
        "{FB}\n\
         module main {{\n\
         \x20   conduit GND    @role(main) @star\n\
         \x20   conduit GNDA   @role(quiet)\n\
         \x20   conduit ESDGND @role(protective)\n\
         \x20   domain DVDD {{ rail [VDD_3V3, GND]::DC(3.3V) }}\n\
         \x20   domain AVDD {{ rail [VDDA, GNDA]::DC(3.3V) }}\n\
         \x20   VDD_3V3 - r1::FB() - VDDA @bridge(VDD_3V3, VDDA)\n\
         \x20   GNDA    - r2::FB() - GND  @bridge(GNDA, GND)\n\
         \x20   ESDGND  - r4::FB() - GNDA @bridge(ESDGND, GNDA)\n\
         }}\n"
    );
    let (table, idx) = build_islands(&src);

    // Hot rail faces: role Hot, world = declaring domain, no declared conduit.
    let vdd = attr_of(&table, &idx, "main", "VDD_3V3");
    assert_eq!(vdd.role, mcc::NetRole::Hot);
    assert_eq!(vdd.worlds, vec!["DVDD".to_string()]);
    assert_eq!(vdd.copper, None);
    assert!(vdd.resolvable);
    assert_eq!(module_path(&table, vdd.module.unwrap()), "main");

    let vdda = attr_of(&table, &idx, "main", "VDDA");
    assert_eq!(vdda.role, mcc::NetRole::Hot);
    assert_eq!(vdda.worlds, vec!["AVDD".to_string()]);

    // Return copper: role Ret, copper = the owning-scope conduit the rail
    // returns to (the quiet subface return of the AVDD rail), world = domain.
    let gnda = attr_of(&table, &idx, "main", "GNDA");
    assert_eq!(gnda.role, mcc::NetRole::Ret);
    assert_eq!(gnda.copper.as_deref(), Some("GNDA"));
    assert_eq!(gnda.worlds, vec!["AVDD".to_string()]);
    assert!(gnda.resolvable);

    // A named copper no rail returns to (the protective ESDGND) is a
    // Reference — worldless, still resolvable.
    let esd = attr_of(&table, &idx, "main", "ESDGND");
    assert_eq!(esd.role, mcc::NetRole::Reference);
    assert_eq!(esd.copper.as_deref(), Some("ESDGND"));
    assert!(esd.worlds.is_empty());
    assert!(esd.resolvable);
}

#[test]
fn same_net_name_in_parent_and_child_resolves_per_owning_scope() {
    // Parent and child each carry a net named V5V (a rail hot). The index must
    // key each by its own `NetEntry.module` and resolve it against that
    // module's *own* decl set — no global-name collision, no `remove` on
    // ambiguity.
    let src = format!(
        "{FB}\n\
         module power {{\n\
         \x20   conduit GND @role(main)\n\
         \x20   domain ISO {{ rail [V5V, GND]::DC(5V) }}\n\
         \x20   V5V - q::FB() - GND\n\
         }}\n\
         module main {{\n\
         \x20   conduit GND @role(main)\n\
         \x20   domain DVDD {{ rail [V5V, GND]::DC(5V) }}\n\
         \x20   power p\n\
         \x20   V5V - r::FB() - GND\n\
         }}\n"
    );
    let (table, idx) = build_islands(&src);

    // Child's V5V resolves to the power def's rail (ISO world); the parent's
    // V5V to the main def's rail (DVDD world). Two distinct owning modules.
    let child = attr_of(&table, &idx, "main.p", "V5V");
    let parent = attr_of(&table, &idx, "main", "V5V");
    assert_ne!(child.module, parent.module);
    assert_eq!(child.role, mcc::NetRole::Hot);
    assert_eq!(child.worlds, vec!["ISO".to_string()]);
    assert_eq!(parent.role, mcc::NetRole::Hot);
    assert_eq!(parent.worlds, vec!["DVDD".to_string()]);
}

#[test]
fn unanchored_nets_are_signal_and_unresolvable() {
    // Signal wires and derived/unknown faces carry no owning-scope declaration
    // anchor → role Signal, resolvable false. L1 never guesses.
    let src = format!(
        "{FB}\n\
         module main {{\n\
         \x20   conduit GND @role(main)\n\
         \x20   domain DVDD {{ rail [VDD_3V3, GND]::DC(3.3V) }}\n\
         \x20   VDD_3V3 - r::FB() - GND\n\
         \x20   MID  - s::FB() - GND      // a mid-rail face, no declared identity\n\
         \x20   LEFT - t::FB() - RIGHT    // pure signal path\n\
         }}\n"
    );
    let (table, idx) = build_islands(&src);

    let mid = attr_of(&table, &idx, "main", "MID");
    assert_eq!(mid.role, mcc::NetRole::Signal);
    assert_eq!(mid.copper, None);
    assert!(mid.worlds.is_empty());
    assert!(!mid.resolvable);

    let left = attr_of(&table, &idx, "main", "LEFT");
    assert_eq!(left.role, mcc::NetRole::Signal);
    assert!(!left.resolvable);

    let right = attr_of(&table, &idx, "main", "RIGHT");
    assert_eq!(right.role, mcc::NetRole::Signal);
    assert!(!right.resolvable);

    // The anchored supply nets still resolve alongside the unresolvable ones.
    let vdd = attr_of(&table, &idx, "main", "VDD_3V3");
    assert_eq!(vdd.role, mcc::NetRole::Hot);
    assert!(vdd.resolvable);
}

#[test]
fn every_flat_net_records_an_owning_module() {
    // `NetEntry.module` is the L1 structural change: every flat net comes from
    // exactly one module's frozen table, so no net may carry `module: None`.
    let src = format!(
        "{FB}\n\
         module power {{\n\
         \x20   conduit GND @role(main)\n\
         \x20   ESDGND - q::FB() - GND\n\
         }}\n\
         module main {{\n\
         \x20   power p\n\
         \x20   TOP - r::FB() - BOT\n\
         }}\n"
    );
    let (table, _idx) = build_islands(&src);
    assert!(!table.get_nets().is_empty(), "board must produce nets");
    for net in table.get_nets() {
        assert!(
            net.module.is_some(),
            "net {:?} ({} points) must record an owning module",
            net.name,
            net.points.len()
        );
    }
}
