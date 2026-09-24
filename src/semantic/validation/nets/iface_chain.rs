// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U112 ② — the chain-level source-reach gate
//! (clock-intent-design.md §2.2): a sink-shaped adoption lane must reach a
//! source of its own family along the adoption chain.
//!
//! The peer gates judge pairs of terminals on one net; this gate judges the
//! chain. A unidirectional family (the CLK face's shape) splits its roles by
//! declared pin direction — every member pin `in` is a sink lane, every
//! member pin `out` a source lane — and the receiver's defect is not a torn
//! pairing but a **dangling feed**: no source reachable at all. The walk
//! starts at the sink's own net and crosses wherever the family's lanes lead:
//! a sink lane on a visited net whose owner instance also declares a source
//! pin of the same family on another net is a distributor, and its source
//! net joins the walk. Reaching a source endpoint — directly on a visited
//! net, or through any number of distributors — satisfies the lane.
//!
//! Reachability, not uniqueness: a multi-input receiver fed from several
//! transmitters is a legal shape (the same law the exclusive-peer gate's
//! module doc keeps for pairing), so several reachable sources stay quiet —
//! the defect this gate exists for is the orphan, zero sources. The trigger
//! is the role's declared pin-direction shape ([`IfaceLane::direction`]),
//! never a family or role name: a role with no direction words (XTAL's
//! passive-leaf pair) and a mixed role (a DCE with `out` TX beside `in` RX)
//! read `None` and are never judged. Source-shaped lanes stay silent
//! everywhere — an unconnected source drives nothing, the load law, not an
//! orphan.

use std::collections::{HashMap, HashSet};

use super::{entry_pos, NetCheckResult};
use crate::instant::insttab::{IfaceLane, InstEntry, InstTable, LaneDir};

/// The owner-instance path of an endpoint entry (`main.uc` from
/// `main.uc.3`); a single-segment path owns itself.
fn owner_path_of(entry: &InstEntry) -> String {
    entry
        .path
        .rsplit_once('.')
        .map(|(p, _)| p.to_string())
        .unwrap_or_else(|| entry.path.clone())
}

/// One flat scan builds the walk's two faces: every family endpoint with the
/// net it sits on, plus per owner instance the nets where its source-shaped
/// lanes of each family sit (the distributor's hand-off face).
struct ChainIndex<'a> {
    /// `(net, entry, lane carry)` per interface endpoint, in net order.
    eps: Vec<(usize, &'a InstEntry, &'a IfaceLane)>,
    /// Endpoint indices per net.
    on_net: HashMap<usize, Vec<usize>>,
    /// Nets where an owner instance's source lanes of one family sit —
    /// `(owner id, family)` → net indices.
    src_nets: HashMap<(u32, String), Vec<usize>>,
}

impl<'a> ChainIndex<'a> {
    fn build(table: &'a InstTable) -> Self {
        let mut idx = ChainIndex {
            eps: Vec::new(),
            on_net: HashMap::new(),
            src_nets: HashMap::new(),
        };
        for (ni, net) in table.get_nets().iter().enumerate() {
            for &pid in &net.points {
                let Some(e) = table.get_entry(pid) else {
                    continue;
                };
                let Some(carry) = e.iface_lane.as_ref() else {
                    continue;
                };
                let i = idx.eps.len();
                idx.eps.push((ni, e, carry));
                idx.on_net.entry(ni).or_default().push(i);
                if carry.direction == Some(LaneDir::Source) {
                    if let Some(owner) = e.parent_id {
                        idx.src_nets
                            .entry((owner, carry.family.clone()))
                            .or_default()
                            .push(ni);
                    }
                }
            }
        }
        idx
    }
}

pub(crate) fn check_iface_chain_source(table: &InstTable, results: &mut Vec<NetCheckResult>) {
    let idx = ChainIndex::build(table);
    struct Fired {
        anchor: (u32, String),
        family: String,
        lane: String,
        owner_path: String,
        role: String,
    }
    let mut fired: Vec<Fired> = Vec::new();

    for &(ni, entry, carry) in &idx.eps {
        // The gate judges only sink-shaped lanes: the source-less side of a
        // unidirectional family. Source lanes are the load law's object, and
        // `None`-shaped lanes declared no direction to be judged by.
        if carry.direction != Some(LaneDir::Sink) {
            continue;
        }
        let Some(owner) = entry.parent_id else {
            continue;
        };
        let family = carry.family.clone();
        // The adoption-chain walk: nets only, family members only.
        let mut visited: HashSet<usize> = HashSet::new();
        let mut frontier: Vec<usize> = vec![ni];
        visited.insert(ni);
        let mut reached = false;
        while let Some(cur) = frontier.pop() {
            for &j in idx.on_net.get(&cur).into_iter().flatten() {
                let (_, other, other_carry) = idx.eps[j];
                if other_carry.family != family {
                    continue;
                }
                match other_carry.direction {
                    // A source of my family on a visited net: fed.
                    Some(LaneDir::Source) => {
                        reached = true;
                        break;
                    }
                    Some(LaneDir::Sink) => {
                        // A distributor: another instance's sink lane whose
                        // owner also drives the family from somewhere else —
                        // its source nets join the walk.
                        let Some(o2) = other.parent_id else {
                            continue;
                        };
                        if o2 == owner {
                            continue;
                        }
                        if let Some(nets) = idx.src_nets.get(&(o2, family.clone())) {
                            for &n in nets {
                                if visited.insert(n) {
                                    frontier.push(n);
                                }
                            }
                        }
                    }
                    None => {}
                }
            }
            if reached {
                break;
            }
        }
        if !reached {
            fired.push(Fired {
                anchor: entry_pos(entry),
                family: family.clone(),
                lane: carry.lane.clone(),
                owner_path: owner_path_of(entry),
                role: carry.role.clone().unwrap_or_default(),
            });
        }
    }

    fired.sort_by(|a, b| a.anchor.cmp(&b.anchor));
    for f in fired {
        results.push(NetCheckResult {
            check: "iface-chain-source",
            severity: "error",
            message: crate::errcodes::format_msg(
                crate::errcodes::IFACE_CHAIN_SOURCE_UNREACHED,
                &[
                    &f.family as &dyn std::fmt::Display,
                    &f.lane,
                    &f.owner_path,
                    &f.role,
                ],
            ),
            net_name: String::new(),
            code: crate::errcodes::IFACE_CHAIN_SOURCE_UNREACHED,
            pos: f.anchor.0,
            uri: f.anchor.1,
        });
    }
}
