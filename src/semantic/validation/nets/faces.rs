// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The §1.4 face read: which worlds a declaration says are **quiet/sensitive**
//! and which are **noisy** (power-quality-design.md §1.4). One read, because
//! five rules ask it and a second copy would drift: PI-2 (is this bridge
//! endpoint's world the protected side?), PI-4 (is this sink's contract the
//! declared subface's?), SN-1 (is this part's supply anchored to a declared
//! analog face? — asked of the port row's own words, then of the face they
//! claim), SN-2 (is this ground bridge's end the return of a noisy face, of a
//! quiet one, or of both?) and SN-3 (does this quiet part's return land on a
//! noisy face?).
//!
//! The words are §1.4's: a declaration reads quiet/sensitive when it writes
//! `@class(analog)`, `@noise(quiet)` or `@noise(sensitive)`, and noisy when it
//! writes `@noise(noisy)`. The projection carrying them verbatim is §1.1
//! ([`McPowerDecls::l1_domain_faces`]); *which* of those words makes a face
//! quiet is the consumer's step, and it is the same step for all five, so it
//! lives here ([`face_of_words`]) — the four that read domains come through
//! [`DomainFaces::read`], SN-1 reads a port row's own words through it and then
//! asks [`DomainFaces::declares`] for the face those words belong to.
//!
//! Faces are read **per declaring scope**, and a world is resolved on the chain
//! of scopes that owns the net ([`super::NetIslandIndex`]'s `module`): a
//! sub-module net bound to a parent layer's world is read at that parent layer,
//! the same A′-flavoured chain walk [`super::railface`] does for rails. A world
//! no scope on the chain declares is no face at all — silence, never a guess
//! (§1.3).
//!
//! The one entry this read does **not** cover is ruling 1's *part-level* noise
//! source (`component DCDC.X { noise = noisy … }`, §1.4's "a part is a noise
//! source ⇔ its definition body says so"): the flat table carries no def-marker
//! for it (the design's §1 common prerequisites cost two carriers — the §1.1
//! domain projection and §1.2's element class — and this is neither), so the
//! first landing of SN-2/SN-3 reads the **domain** face only and the part-level
//! entry stays a zero-consumer carry, exactly as §1.2's element class began.

use crate::instant::insttab::InstTable;
use crate::semantic::basic::attr_keys;
use std::collections::{HashMap, HashSet};

/// Which declared face a world is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Face {
    /// §1.4's quiet/sensitive face — `@class(analog)`, `@noise(quiet)`,
    /// `@noise(sensitive)`.
    Quiet,
    /// §1.4's noisy face — `@noise(noisy)`.
    Noisy,
}

/// Every declaring scope's quiet and noisy domains, read off the §1.1
/// projection once per rule that asks.
#[derive(Default)]
pub(super) struct DomainFaces {
    quiet: HashMap<u32, HashSet<String>>,
    noisy: HashMap<u32, HashSet<String>>,
    /// Worlds a scope declares `@class(digital)` or `@noise(noisy)` — the
    /// signal-class axis's net-side reading (pin-expectation-design.md §4,
    /// v0.3): the §1.4 face model pairs quiet/sensitive with the analog class,
    /// so its declared opposite carries the digital class. The
    /// pin-expectation gate is the only consumer; the SN/PI rules keep
    /// reading the two §1.4 faces only.
    digital: HashMap<u32, HashSet<String>>,
}

/// §1.4's face for **one declaration's words**, wherever they were written: a
/// domain's `@class`/`@noise` (the projection [`DomainFaces::read`] walks) or a
/// port row's (`L1Port::class`/`noise`, which is where SN-1 reads the analog
/// face). One mapping, because "which words make a face quiet" is the canon's
/// step and a second copy of it would let a port row and a domain disagree.
pub(super) fn face_of_words(class: Option<&str>, noise: Option<&str>) -> Option<Face> {
    // The words are the registry's (`@class`/`@noise` rows), so the mapping
    // cannot name a word the key does not admit — a word outside the set is
    // reported where it is written (5360).
    if class == Some(attr_keys::WORD_ANALOG)
        || matches!(
            noise,
            Some(attr_keys::WORD_QUIET) | Some(attr_keys::WORD_SENSITIVE)
        )
    {
        Some(Face::Quiet)
    } else if noise == Some(attr_keys::WORD_NOISY) {
        Some(Face::Noisy)
    } else {
        None
    }
}

impl DomainFaces {
    pub(super) fn read(table: &InstTable) -> Self {
        let mut out = Self::default();
        for (id, pi) in table.power_decls() {
            for f in pi.l1_domain_faces() {
                // The class read sits beside the face read, not inside it:
                // `@class(digital)` and `@noise(noisy)` name no §1.4 face
                // between them (`face_of_words` maps only the analog side),
                // but both declare the digital class.
                if f.class.as_deref() == Some(attr_keys::WORD_DIGITAL)
                    || f.noise.as_deref() == Some(attr_keys::WORD_NOISY)
                {
                    out.digital.entry(*id).or_default().insert(f.name.clone());
                }
                match face_of_words(f.class.as_deref(), f.noise.as_deref()) {
                    Some(Face::Quiet) => {
                        out.quiet.entry(*id).or_default().insert(f.name);
                    }
                    Some(Face::Noisy) => {
                        out.noisy.entry(*id).or_default().insert(f.name);
                    }
                    None => {}
                }
            }
        }
        out
    }

    /// Whether the scope `scope` **itself** declares `world` as `face` — no
    /// chain walk, because the caller already holds the scope it means. This is
    /// the read SN-1 needs: its declaration (`@return`) and the face it compares
    /// against are both pinned to one scope, and walking the chain there would
    /// let an ancestor's face answer for a descendant's declaration.
    pub(super) fn declares(&self, scope: u32, face: Face, world: &str) -> bool {
        let map = match face {
            Face::Quiet => &self.quiet,
            Face::Noisy => &self.noisy,
        };
        map.get(&scope).is_some_and(|set| set.contains(world))
    }

    /// Whether any scope on `layer`'s chain declares a face at all — the cheap
    /// guard that keeps a board with no faces at all out of the walk.
    pub(super) fn is_empty(&self) -> bool {
        self.quiet.is_empty() && self.noisy.is_empty() && self.digital.is_empty()
    }

    /// The digital-class world `worlds` anchors, if any — the signal-class
    /// axis's net-side reading (pin-expectation-design.md §4, v0.3). The same
    /// per-scope chain walk [`Self::world_of`] does for the §1.4 faces.
    pub(super) fn digital_world(
        &self,
        table: &InstTable,
        layer: u32,
        worlds: &[String],
    ) -> Option<String> {
        let mut cur = Some(layer);
        while let Some(id) = cur {
            if let Some(set) = self.digital.get(&id) {
                if let Some(w) = worlds.iter().find(|w| set.contains(*w)) {
                    return Some(w.clone());
                }
            }
            cur = table.get_entry(id).and_then(|e| e.parent_id);
        }
        None
    }

    /// The first of `worlds` that some scope on `layer`'s chain declares as
    /// `face`. The world **is** the domain name (a world list is a domain list),
    /// so the answer names the declaring domain as well.
    pub(super) fn world_of(
        &self,
        table: &InstTable,
        face: Face,
        layer: u32,
        worlds: &[String],
    ) -> Option<String> {
        let map = match face {
            Face::Quiet => &self.quiet,
            Face::Noisy => &self.noisy,
        };
        let mut cur = Some(layer);
        while let Some(id) = cur {
            if let Some(set) = map.get(&id) {
                if let Some(w) = worlds.iter().find(|w| set.contains(*w)) {
                    return Some(w.clone());
                }
            }
            cur = table.get_entry(id).and_then(|e| e.parent_id);
        }
        None
    }

    /// The quiet/sensitive world `worlds` anchors, if any — §1.4's protected
    /// side (PI-2's load side, PI-4's subface). SN-3 asks the same question of
    /// both faces through [`Self::world_of`].
    pub(super) fn quiet_world(
        &self,
        table: &InstTable,
        layer: u32,
        worlds: &[String],
    ) -> Option<String> {
        self.world_of(table, Face::Quiet, layer, worlds)
    }
}
