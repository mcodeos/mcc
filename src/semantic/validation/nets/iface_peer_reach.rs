// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U391 — the peer-undershoot gate (reset-intent-design.md §2, both
//! structural candidates: chain reachability and POR-supervisor existence).
//!
//! The two established peer gates each own one direction of a declared
//! bound: E6054 judges the **overshoot** (more peer instances than the
//! declaration allows), E6060 the dangling feed of a *sink-shaped* lane —
//! a role whose pins all declare `in`. The `io`-member quadrant is the
//! gap this gate closes: a role whose member pin is `io` (the RECEIVER
//! face of `RST`, reset-intent's first domino) reads no direction shape
//! (`LaneDir::None`), so 6060's trigger never fires, and its exact-one
//! peer declaration (`peer = SOURCE(1)`) has no undershoot reader — until
//! here. The trigger is the declaration, never a family or role name:
//! `peer_span == Some((1, 1))` (the bound 6054 reads for overshoot) or
//! the retired `exclusive = true`, on a `None`-shaped lane. The
//! `Source`/`Sink` quadrants stay 6060's object — a conductor-local
//! structure witness would misfire on a sink legally fed through a
//! distributor on another net, so the two gates keep disjoint trigger
//! quadrants and never double-report.
//!
//! The failure fact is deliberately **not** "zero reachable sources":
//! an RC-only reset network is a legal reset source (the b4511 probe
//! ruling — resistor to a rail, capacitor to ground, no supervisor
//! anywhere). The falsifiable structural fact is conductor exhaustion:
//! on the whole merged conductor (the same union-find 6054/6061 read —
//! port passthrough arm (b) crosses the hierarchy, the relay/slash arm
//! (c) carries a future cable face) the lane meets no endpoint of its
//! declared peer role **and no terminal outside its family either**. Any
//! resistor pin, button pin, debugger pin, or supervisor output is a
//! non-family endpoint and keeps the lane silent; two bare receivers
//! tied together exhaust the conductor and both fire. Not-fitted and
//! NC-marked terminals count on neither side (U305's not_fitted law); a
//! dangling module port counts as structure — conservative silence, the
//! dangling-port shape is R12/C4's object.
//!
//! POR-supervisor existence (the §2 twin candidate) lands inside this
//! same code: at conductor grain a domain *is* a conductor, and a body
//! with zero peer and zero structure is exactly the absent-reset-source
//! fact. A per-domain census waits for domain objects to exist; until
//! then one code owns one conductor, one anchor, one remedy.

use std::collections::{HashMap, HashSet};

use super::{entry_pos, is_nc_entry, NetCheckResult};
use crate::instant::insttab::{IfaceLane, InstEntry, InstTable};

/// One triggering endpoint awaiting its conductor's verdict.
struct Cand<'a> {
    entry: &'a InstEntry,
    carry: &'a IfaceLane,
    /// The owner instance the peer must belong to (a peer on the candidate's
    /// own instance is a wiring self-loop, not a peer body — the same
    /// owner-exclusion the overshoot gate reads).
    owner_id: u32,
}

/// The owner-instance path of an endpoint entry (`main.uc` from
/// `main.uc.3`); a single-segment path owns itself.
fn owner_path_of(entry: &InstEntry) -> String {
    entry
        .path
        .rsplit_once('.')
        .map(|(p, _)| p.to_string())
        .unwrap_or_else(|| entry.path.clone())
}

pub(crate) fn check_iface_peer_reach(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let nets = table.get_nets();
    let mut parent = super::iface_role_peers::merged_conductor_parent(table, &nets);
    // Every terminal of the conductor, family-bearing or plain — the plain
    // terminal IS the structure witness, so the grouping cannot pre-filter
    // to lane carries the way the overshoot gate does.
    let mut groups: HashMap<usize, Vec<&InstEntry>> = HashMap::new();
    for (ni, net) in nets.iter().enumerate() {
        let root = super::iface_role_peers::find(&mut parent, ni);
        let group = groups.entry(root).or_default();
        for &pid in &net.points {
            let Some(e) = table.get_entry(pid) else {
                continue;
            };
            if group.iter().any(|seen| seen.id == e.id) {
                continue;
            }
            group.push(e);
        }
    }

    struct Fired {
        anchor: (u32, String),
        family: String,
        lane: String,
        owner_path: String,
        role: String,
        card_text: String,
    }
    let mut fired: Vec<Fired> = Vec::new();
    // One fire per lane even when the lane's terminals are torn across
    // several conductors: the verdict is the lane's body, deduped globally.
    let mut seen_lanes: HashSet<(u32, String)> = HashSet::new();

    let mut group_list: Vec<(usize, Vec<&InstEntry>)> = groups.into_iter().collect();
    group_list.sort_by_key(|(root, _)| *root);
    for (_, endpoints) in group_list {
        // The conductor's candidates: exact-one declared, no direction
        // shape, a peer role named, an owner to anchor the fire at.
        let cands: Vec<Cand<'_>> = endpoints
            .iter()
            .filter_map(|e| {
                let carry = e.iface_lane.as_ref()?;
                let exact_one = carry.peer_span == Some((1, 1)) || carry.exclusive;
                if !exact_one || carry.direction.is_some() {
                    return None;
                }
                let owner_id = e.parent_id?;
                Some(Cand {
                    entry: e,
                    carry,
                    owner_id,
                })
            })
            .collect();
        if cands.is_empty() {
            continue;
        }
        for c in cands {
            let peer_role = match &c.carry.peer_role {
                Some(r) => r,
                None => continue,
            };
            let mut peer_reached = false;
            let mut structured = false;
            for other in endpoints {
                if other.id == c.entry.id {
                    continue;
                }
                // A declared-role endpoint of the candidate's family is the
                // only thing that can satisfy the peer arm — and only a
                // *different* body counts (a same-instance meeting is a
                // wiring self-loop, the overshoot gate's own exclusion).
                // Everything else with a mounted terminal is the structure
                // witness: a plain pin, a RELAY face (a conductor, not a
                // peer body — U352), the family-typed but role-less face of
                // a module port (the conservative silence of the dangling-
                // port boundary — R12/C4 owns the dangling shape), a
                // role-less family adoption. Same family with a *different*
                // declared role is the one neither-arm shape: a second bare
                // receiver is no peer and no drive structure either.
                let peer_arm = matches!(
                    other.iface_lane.as_ref(),
                    Some(lane) if lane.family == c.carry.family && lane.role.is_some()
                );
                if peer_arm {
                    let lane = other.iface_lane.as_ref().unwrap();
                    if lane.role.as_deref() == Some(peer_role.as_str())
                        && other.parent_id.is_some_and(|oid| oid != c.owner_id)
                    {
                        peer_reached = true;
                        break;
                    }
                } else if !is_nc_entry(other) && !other.not_fitted {
                    structured = true;
                    break;
                }
            }
            if peer_reached || structured {
                continue;
            }
            let key = (c.owner_id, c.carry.lane.clone());
            if seen_lanes.insert(key) {
                fired.push(Fired {
                    anchor: entry_pos(c.entry),
                    family: c.carry.family.clone(),
                    lane: c.carry.lane.clone(),
                    owner_path: owner_path_of(c.entry),
                    role: c.carry.role.clone().unwrap_or_default(),
                    card_text: c.carry.peer_card_text.clone().unwrap_or_default(),
                });
            }
        }
    }

    fired.sort_by(|a, b| a.anchor.cmp(&b.anchor));
    for f in fired {
        results.push(NetCheckResult {
            check: "iface-peer-unreached",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::IFACE_PEER_UNREACHED,
                &[
                    &f.family as &dyn std::fmt::Display,
                    &f.lane,
                    &f.owner_path,
                    &f.role,
                    &f.card_text,
                ],
            ),
            net_name: String::new(),
            code: crate::errcodes::IFACE_PEER_UNREACHED,
            pos: f.anchor.0,
            uri: f.anchor.1,
        });
    }
}
