// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U289 ⑥ — the E4 flat-net generic peer sweep
//! (replicated-binding-design.md §4 check 4): two role-bearing endpoints of
//! one family land on one flat net and their roles are not mutual `peer`s
//! per the interface's role block.
//!
//! Why a net walk and not the connection-time pair judge alone: E4121 only
//! sees endpoint pairs meeting inside one statement (`create_connection` or a
//! `+` junction). A `-` chain is split into per-adjacency connections, so its
//! two real endpoints never meet one judge, and a module port is role-less by
//! law (E4184) — each side's statement-level judge sees a role-less far end
//! and stays silent. On the flat table both defects are visible as facts about
//! the **net**: the chain's real endpoints share one `NetEntry`, and a module
//! port folds parent-side and child-side onto the same physical entry, so
//! nets sharing an entry id are two faces of one conductor. This sweep unions
//! those nets and judges endpoint pairs the statement-level judge could never
//! see.
//!
//! Semantics are the statement-level judge's, not a second opinion: both
//! roles known (the single-side silence law iface_peer.rs already states),
//! same family, mutual `peer` per the full declared value set (the lane
//! carry's `peer_roles`, decoded by the same def-side reader). A pair the
//! statement-level judge already reported fires again here by ruling
//! (2026-09-25): both codes own their own walk, no dedup carry.

use std::collections::{HashMap, HashSet};

use super::{entry_pos, NetCheckResult};
use crate::instant::insttab::{IfaceLane, InstEntry, InstTable};

/// One role-bearing endpoint as the sweep sees it: the physical entry and
/// its decoded lane carry.
struct RoleEndpoint<'a> {
    entry: &'a InstEntry,
    lane: &'a IfaceLane,
}

/// One confirmed non-mutual pair, waiting for the deterministic sort.
struct PeerPair {
    net_name: String,
    path_a: String,
    role_a: String,
    path_b: String,
    role_b: String,
    family: String,
    anchor: (u32, String),
}

/// Union-find root over net indices (the port-passthrough merge).
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        // Attach the higher root under the lower one: the merge result is
        // independent of the discovery order.
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[hi] = lo;
    }
}

pub(crate) fn check_iface_role_peers(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let nets = table.get_nets();

    // ── Port passthrough ──
    // A module port is role-less by law (E4184), but it IS the conductor that
    // joins its parent-side net to its child-side net: both NetEntries carry
    // the port's own InstEntry. Nets sharing any entry id are unioned into
    // one judge group, so the two device sides of a port collide directly.
    let mut parent: Vec<usize> = (0..nets.len()).collect();
    {
        // (a) A point id carried by several net segments (the boundary
        // junction, A′ 3) is one conductor.
        let mut seen: HashMap<u32, usize> = HashMap::new();
        // (b) A module port's two faces: the child side and the parent side
        // register under the same spelling (the aggregate dotted form, or the
        // slash lane of a subscript member) but as distinct entries with no
        // shared id — the path is the conductor identity the boundary keeps
        // (insttab.rs §A′: one physical point, both sides of the module
        // boundary). A spelling carried by several nets unions them. A real
        // bus member (no port behind it) has a single spelling on one net and
        // stays unmerged.
        let mut seen_path: HashMap<&str, usize> = HashMap::new();
        for (ni, net) in nets.iter().enumerate() {
            for &pid in &net.points {
                match seen.get(&pid) {
                    Some(&pj) => union(&mut parent, ni, pj),
                    None => {
                        seen.insert(pid, ni);
                    }
                }
                let Some(e) = table.get_entry(pid) else {
                    continue;
                };
                match seen_path.get(e.path.as_str()) {
                    Some(&pj) => union(&mut parent, ni, pj),
                    None => {
                        seen_path.insert(e.path.as_str(), ni);
                    }
                }
                if let Some(idx) = e.path.rfind('/') {
                    let base = &e.path[..idx];
                    match seen_path.get(base) {
                        Some(&pj) => union(&mut parent, ni, pj),
                        None => {
                            seen_path.insert(base, ni);
                        }
                    }
                }
            }
        }
    }

    // ── Group the role-bearing endpoints per merged conductor ──
    // An entry shared by several nets of one group (the port itself) counts
    // once; a physical pin lands on exactly one net, so this only dedupes
    // the passthrough entries.
    let mut groups: HashMap<usize, Vec<(usize, RoleEndpoint<'_>)>> = HashMap::new();
    for (ni, net) in nets.iter().enumerate() {
        let root = find(&mut parent, ni);
        let group = groups.entry(root).or_default();
        for &pid in &net.points {
            let Some(e) = table.get_entry(pid) else {
                continue;
            };
            // Non-physical spellings (alias entries) are not endpoints.
            if e.alias_of.is_some() {
                continue;
            }
            let Some(lane) = e.iface_lane.as_ref() else {
                continue;
            };
            let Some(_) = &lane.role else {
                continue;
            };
            if group.iter().any(|(_, re)| re.entry.id == e.id) {
                continue;
            }
            group.push((ni, RoleEndpoint { entry: e, lane }));
        }
    }

    // ── Pairwise mutual-peer judge per merged conductor ──
    let mut fired: Vec<PeerPair> = Vec::new();
    let mut reported: HashSet<(u32, u32)> = HashSet::new();
    let mut group_list: Vec<(usize, Vec<(usize, RoleEndpoint<'_>)>)> = groups.into_iter().collect();
    group_list.sort_by_key(|(root, _)| *root);
    for (_, group) in group_list {
        for ai in 0..group.len() {
            let (a_net, a) = &group[ai];
            for (_, b) in group.iter().skip(ai + 1) {
                if a.entry.id == b.entry.id {
                    continue;
                }
                // Cross-family pairs stay silent here: the statement-level
                // judge's step 1 (E4120) owns the family axis at meetings,
                // and this sweep's mandate is the peer table.
                if a.lane.family != b.lane.family {
                    continue;
                }
                let (ra, rb) = (
                    a.lane.role.as_deref().unwrap_or_default(),
                    b.lane.role.as_deref().unwrap_or_default(),
                );
                // Mutual `peer`, the full declared value sets both directions.
                if a.lane.peer_roles.iter().any(|p| p == rb)
                    && b.lane.peer_roles.iter().any(|p| p == ra)
                {
                    continue;
                }
                let key = if a.entry.id < b.entry.id {
                    (a.entry.id, b.entry.id)
                } else {
                    (b.entry.id, a.entry.id)
                };
                if !reported.insert(key) {
                    continue;
                }
                // The pair is reported on the net where it physically meets;
                // when the union spans several nets, the first one carrying
                // either endpoint names the conductor.
                let net_name = nets[*a_net].name.clone();
                fired.push(PeerPair {
                    net_name,
                    path_a: a.entry.path.clone(),
                    role_a: ra.to_string(),
                    path_b: b.entry.path.clone(),
                    role_b: rb.to_string(),
                    family: a.lane.family.clone(),
                    anchor: entry_pos(a.entry),
                });
            }
        }
    }

    fired.sort_by(|x, y| x.anchor.cmp(&y.anchor));
    for pair in fired {
        results.push(NetCheckResult {
            check: "iface-role-peer-sweep",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::IFACE_ROLE_PEER_CONFLICT,
                &[
                    &pair.net_name as &dyn std::fmt::Display,
                    &pair.path_a,
                    &pair.role_a,
                    &pair.path_b,
                    &pair.role_b,
                    &pair.family,
                ],
            ),
            net_name: pair.net_name,
            code: crate::errcodes::IFACE_ROLE_PEER_CONFLICT,
            pos: pair.anchor.0,
            uri: pair.anchor.1,
        });
    }
}
