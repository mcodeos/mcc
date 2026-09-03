// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! MemberLedger — the DefMemberId account ledger (defspace D13 / invariant C).
//!
//! The two member kinds an instance `PointId` references by id — component
//! pins and module io ports — are append-only ledgers with tombstones. A
//! [`DefMemberId`] is a stable generation index that is never reused: an
//! instance-side `PointId` that references a def member stays valid across
//! def edits, because inserting a member mid-table can no longer shift later
//! members' identities.
//!
//! The declaration "reorder the pins" must therefore be expressed as
//! "tombstone + new pin" — that is the identity-safe form of a rename.
//!
//! Ownership (T4, defspace-id-core-plan M1): the ledgers live in the def
//! registry keyed by the owning def's [`DefId`](crate::db::defregistry::DefId),
//! not on the parse artifacts — a re-parse merges by name into the surviving
//! ledger (same name reuses its id, new names append, vanished names are
//! tombstoned) instead of re-deriving ids from scratch. Interface members,
//! bus members and labels are content-addressed (declaration order only) and
//! never enter a ledger — nothing references them by a stable member id.

use std::collections::BTreeMap;

/// Stable generation index of a def member (§5 D13). Append-only — a
/// tombstoned member's id is never reused. Doubles as the stable `PinOrd`
/// that instance points reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefMemberId(pub u32);

/// A ledger entry: the member's stable id plus its declared name and iotype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefMember {
    pub id: DefMemberId,
    pub name: String,
    pub iotype: String,
}

/// Append-only member account ledger (invariant C). The vector is indexed by
/// `DefMemberId.0` (with `None` slots for tombstoned members), and
/// `name_to_id` resolves a declared name to its stable id.
#[derive(Debug, Clone, Default)]
pub struct MemberLedger {
    entries: Vec<Option<DefMember>>,
    name_to_id: BTreeMap<String, DefMemberId>,
    next_id: u32,
}

impl MemberLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a member under `name`, returning its stable id. A repeated
    /// name reuses the original id (identity preserved across re-declaration),
    /// only refreshing the iotype label.
    pub fn register(&mut self, name: &str, iotype: &str) -> DefMemberId {
        if let Some(&id) = self.name_to_id.get(name) {
            if let Some(Some(member)) = self.entries.get_mut(id.0 as usize) {
                member.iotype = iotype.to_string();
            }
            return id;
        }
        let id = DefMemberId(self.next_id);
        self.next_id += 1;
        // Backfill tombstone slots so `id.0` is a stable index into `entries`.
        while self.entries.len() < id.0 as usize {
            self.entries.push(None);
        }
        self.entries.push(Some(DefMember {
            id,
            name: name.to_string(),
            iotype: iotype.to_string(),
        }));
        self.name_to_id.insert(name.to_string(), id);
        id
    }

    /// Tombstone a member: the id is retired and never reused. A later
    /// re-declaration of the same name allocates a fresh id.
    pub fn tombstone(&mut self, name: &str) {
        if let Some(id) = self.name_to_id.remove(name) {
            if let Some(slot) = self.entries.get_mut(id.0 as usize) {
                *slot = None;
            }
        }
    }

    /// Stable id of a live member, if registered.
    pub fn id_of(&self, name: &str) -> Option<DefMemberId> {
        self.name_to_id.get(name).copied()
    }

    /// The member at a stable id (tombstones read as `None`).
    pub fn member(&self, id: DefMemberId) -> Option<&DefMember> {
        self.entries.get(id.0 as usize).and_then(|m| m.as_ref())
    }

    /// Live members in append order (tombstones skipped).
    pub fn live_members(&self) -> impl Iterator<Item = &DefMember> {
        self.entries.iter().filter_map(|m| m.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_is_append_only_with_stable_ids() {
        let mut l = MemberLedger::new();
        let a = l.register("A", "In");
        let b = l.register("B", "Out");
        assert_eq!(a.0, 0);
        assert_eq!(b.0, 1);

        // Re-declaration reuses the id.
        assert_eq!(l.register("A", "InOut"), a);
        assert_eq!(l.member(a).unwrap().iotype, "InOut");

        // Tombstone retires the slot; a re-declared name gets a fresh id.
        l.tombstone("B");
        assert!(l.member(b).is_none());
        assert!(l.id_of("B").is_none());
        let b2 = l.register("B", "Out");
        assert_eq!(b2.0, 2);
        assert_ne!(b2, b);

        // Stable ordering: live members follow append order.
        let names: Vec<&str> = l.live_members().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["A", "B"]);
    }

    #[test]
    fn mid_insert_does_not_shift_existing_ids() {
        // The core D13 guarantee: a member inserted before an existing one
        // must not change the existing member's id — instance PointIds stay
        // valid across def edits.
        let mut l = MemberLedger::new();
        l.register("1", "In");
        l.register("2", "In");
        l.register("3", "Out");
        let id2 = l.id_of("2").unwrap();
        let id3 = l.id_of("3").unwrap();

        // Simulate an edit that inserts a member between "1" and "2".
        let mut m = l.clone();
        m.register("1a", "In");
        assert_eq!(m.id_of("2"), Some(id2), "existing member id must be stable");
        assert_eq!(m.id_of("3"), Some(id3), "later member id must be stable");
        assert_eq!(m.member(id2).unwrap().name, "2");
    }
}
