// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U201 ①② — the role-anchored exclusive-peer gate
//! (xtal-oscillator-design.md §2): one adoption lane of a role declaring
//! `exclusive = true` must reach **one** peer-role instance across its
//! terminals, judged over whole nets on the flat table.
//!
//! Why whole nets and not the connection-time pair judge: the defect this
//! gate exists for is *torn across nets* — a resonator body wired `X1` onto
//! one MCU and `X2` onto another puts exactly two family endpoints on each
//! net, so the per-net point-to-point count (E4122) is quiet on both, and
//! each single connection is a legal mutual-peer pairing. The defect is only
//! visible as a fact about the **body** (the adoption lane), which is why the
//! lane identity rides the flat entry (`InstEntry::iface_lane`, decoded once
//! at flatten time from the same def-side lookups the connection-time
//! resolver makes).
//!
//! The trigger is the declaration, never a family or role name: a role that
//! declares `exclusive = true` (the XTAL pair does) is one body to one body;
//! a role that declares nothing pairs unrestricted — a multi-input receiver
//! lane fed from several transmitters is a legal shape and stays silent. One
//! terminal with no peer at all is the single-side silence law (analog-peer
//! design §1: only both-sides-declared pairs are judged), not this gate's
//! defect.

use std::collections::{HashMap, HashSet};

use super::{entry_pos, NetCheckResult};
use crate::instant::insttab::InstTable;

/// Per-lane accumulator: the carry of the lane's first-seen terminal (for the
/// diagnostic anchor and the message's role spelling) plus the set of
/// distinct peer instances reached across the lane's terminals.
struct LaneAcc {
    family: String,
    lane: String,
    owner_path: String,
    role: String,
    anchor: (u32, String),
    /// Flat paths of the peer **instances** (the component entry's path, not
    /// each pin's) — the uniqueness law counts bodies, not pins.
    peers: HashSet<String>,
}

/// The owner-instance path of an endpoint entry (`main.uc` from
/// `main.uc.3`); a single-segment path owns itself.
fn owner_path_of(entry: &crate::instant::insttab::InstEntry) -> String {
    entry
        .path
        .rsplit_once('.')
        .map(|(p, _)| p.to_string())
        .unwrap_or_else(|| entry.path.clone())
}

pub(crate) fn check_iface_exclusive_peer(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let mut lanes: HashMap<(u32, String), LaneAcc> = HashMap::new();

    for net in table.get_nets() {
        // The net's interface endpoints, resolved once: (entry, carry).
        let endpoints: Vec<_> = net
            .points
            .iter()
            .filter_map(|&pid| table.get_entry(pid))
            .filter_map(|e| e.iface_lane.as_ref().map(|c| (e, c)))
            .collect();
        if endpoints.len() < 2 {
            continue;
        }

        for (entry, carry) in &endpoints {
            // The gate judges only what a declaration states: an exclusive
            // role with a named peer. Everything else pairs unrestricted.
            if !carry.exclusive {
                continue;
            }
            let Some(peer_role) = &carry.peer_role else {
                continue;
            };
            let Some(owner_id) = entry.parent_id else {
                continue;
            };
            // Peers on this net: same family, the role my `peer` names,
            // a different terminal. The peer's *instance* is the fact the
            // uniqueness law counts.
            let mut peers: HashSet<String> = HashSet::new();
            for (other, other_carry) in &endpoints {
                if other.id == entry.id {
                    continue;
                }
                if other_carry.family != carry.family
                    || other_carry.role.as_deref() != Some(peer_role.as_str())
                {
                    continue;
                }
                if let Some(oid) = other.parent_id {
                    if oid != owner_id {
                        peers.insert(owner_path_of(other));
                    }
                }
            }
            let acc = lanes
                .entry((owner_id, carry.lane.clone()))
                .or_insert_with(|| LaneAcc {
                    family: carry.family.clone(),
                    lane: carry.lane.clone(),
                    owner_path: owner_path_of(entry),
                    role: carry.role.clone().unwrap_or_default(),
                    anchor: entry_pos(entry),
                    peers: HashSet::new(),
                });
            acc.peers.extend(peers);
        }
    }

    let mut fired: Vec<LaneAcc> = lanes
        .into_iter()
        .filter(|(_, acc)| acc.peers.len() > 1)
        .map(|(_, acc)| acc)
        .collect();
    fired.sort_by(|a, b| a.anchor.cmp(&b.anchor));
    for acc in fired {
        let mut peer_list: Vec<String> = acc.peers.into_iter().collect();
        peer_list.sort();
        let count = peer_list.len();
        results.push(NetCheckResult {
            check: "iface-exclusive-peer",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT,
                &[
                    &acc.family as &dyn std::fmt::Display,
                    &acc.lane,
                    &acc.owner_path,
                    &acc.role,
                    &count,
                    &peer_list.join(", "),
                ],
            ),
            net_name: String::new(),
            code: crate::errcodes::IFACE_EXCLUSIVE_PEER_CONFLICT,
            pos: acc.anchor.0,
            uri: acc.anchor.1,
        });
    }
}
