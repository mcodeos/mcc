// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! §11.5.2: read-side structural query API — the world model's "query"
//! dimension.
//!
//! The design names a small query layer sitting on arena + lanes + nets so
//! consumers (LSP / drawing / ERC) read through it uniformly instead of
//! re-walking the recursive tree: `node.children()` / `node.parent()` /
//! `point.net()` / `net.points()` / `net.fanout(point)` /
//! `lane.owner_trunk()` / module subtree walk.
//!
//! `node.children()` / `node.parent()` are the arena edges (already present);
//! `point.net()` / `net.fanout(point)` are backed by the point → net reverse
//! index built in [`DianLu::assemble`](super::dianlu::DianLu) after
//! `finalize_net_ids`. Lanes have no independent id (design §12.2:
//! content-addressed, referenced by `(trunk, ordinal)`), so the lane → trunk
//! direction is exposed both as a `LaneRef` spelling ([`DianLu::trunk`] /
//! [`DianLu::lane`]) and as the structural [`DianLu::lane_owner_trunk`]
//! convenience that finds a lane's trunk by containment.

use crate::instant::dianlu::DianLu;
use crate::instant::identity::NodeId;
use crate::instant::lane::{Lane, Net, NetId, PointId, Trunk};

impl Net {
    /// The physical points of the net (the union-find equivalence class) —
    /// design §11.5.2 `net.points()`.
    pub fn points(&self) -> &[PointId] {
        &self.points
    }
}

impl DianLu {
    /// The net a point belongs to — design §11.5.2 `point.net()`. A point
    /// belongs to exactly one union-find net, so this is exact.
    pub fn point_net(&self, p: PointId) -> Option<&Net> {
        let id = self.point_net_index().get(&p)?;
        self.nets().iter().find(|n| n.id == *id)
    }

    /// The physical points of the net with the given id — design §11.5.2
    /// `net.points()` (net-id spelling of [`Net::points`]).
    pub fn net_points(&self, id: NetId) -> Option<&[PointId]> {
        self.nets()
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.points.as_slice())
    }

    /// Fanout of a point — design §11.5.2 `net.fanout(point)`: the points
    /// sharing its net. A point's net is its electrical equivalence class, so
    /// fanout is exactly the net's member set.
    pub fn point_fanout(&self, p: PointId) -> Option<&[PointId]> {
        let id = self.point_net_index().get(&p)?;
        self.net_points(*id)
    }

    /// Resolve a `LaneRef`'s trunk by id (design §12.2 content-addressing).
    pub fn trunk(&self, id: usize) -> Option<&Trunk> {
        self.lanes().iter().find(|t| t.id == id)
    }

    /// Resolve a `LaneRef` — the `(trunk, ordinal)` lane of a trunk.
    pub fn lane(&self, trunk: usize, ord: usize) -> Option<&Lane> {
        self.trunk(trunk).and_then(|t| t.lanes.get(ord))
    }

    /// The trunk owning a lane — design §11.5.2 `lane.owner_trunk()`. Lanes
    /// have no independent id (§12.2), so the owner is found by structural
    /// containment: a lane obtained from a trunk's `lanes` resolves back to
    /// that trunk.
    pub fn lane_owner_trunk(&self, lane: &Lane) -> Option<&Trunk> {
        self.lanes()
            .iter()
            .find(|t| t.lanes.iter().any(|l| l == lane))
    }

    /// Children of a node — design §11.5.2 `node.children()`, via the arena
    /// edges.
    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.arena().children(id)
    }

    /// Parent of a node — design §11.5.2 `node.parent()`, via the arena edges.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.arena().parent(id)
    }

    /// Pre-order module subtree walk from `root` (design §11.5.2 module
    /// subtree walk). Returns every arena node id under `root`, root first.
    pub fn module_subtree(&self, root: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            out.push(id);
            if let Some(children) = self.arena().children(id) {
                // Push in reverse so the first child visits first (pre-order).
                for c in children.iter().rev() {
                    stack.push(*c);
                }
            }
        }
        out
    }
}
