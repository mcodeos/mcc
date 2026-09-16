// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Declared power-face identity — "is this a supply / is this a ground?".
//!
//! Every answer in this module comes from a **declaration the author wrote**: a
//! `::DC(hot, ret)` pair on a `psrc/psnk/psbi` row, a rail declaration inside a
//! domain, a `conduit` (reference / return) declaration, or the member list of a
//! declared interface pair. No answer is ever derived from the **shape** of a
//! name.
//!
//! That is the rule, not a preference. [world-axioms §1 A1] states it: a name is
//! a label the author gave an entity, so the spelling (`GND`, `GND_OUT`, `V3V3`,
//! `gnd`) carries no promise about what the entity is, and any criterion resting
//! on a guess about it is a defect whatever answer it happens to give today. The
//! exemption is that an **already-declared or already-registered** name *is* an
//! identity, compared by exact string equality. `is_ground_name`-style word
//! tables, `starts_with("GND")` prefixes, digit-and-`V` patterns and
//! `to_uppercase` normalisation are therefore inadmissible —
//! [identity-design §3.1] already listed the first two as forbidden merge
//! criteria, and [§2 b3372] (U49) fixed name equality as exact string equality
//! with no folding.
//!
//! [world-axioms §1 A1]: ../doc/arch/space/world-axioms.md
//! [identity-design §3.1]: ../doc/net/identity-design.md
//! [§2 b3372]: ../doc/net/identity-design.md

use std::collections::BTreeSet;

use super::component::mc_pins::McPins;
use super::module::pi::McPowerDecls;

/// What a declaration says a piece of copper **is**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Face {
    /// The supply side: the first `::` member, or a rail's `hot`.
    Hot,
    /// The return side: the second `::` member, or a rail's `ret`. This is the
    /// ground-side of the contract.
    Ret,
    /// Declared copper whose declaration names no side of a contract — a
    /// `conduit` / `ref`. It is *declared*, so it is an identity; it is not a
    /// supply and not a return, so no rule may treat it as either.
    Copper,
}

/// The contract name a `::DC` pair declares itself under when the declaration
/// does not carry one of its own (module power-port rows validate `::DC` and do
/// not keep the spelling). Path-independent on purpose: two components whose
/// `pins.pwr` rows both write a `GND` return declare the *same* return
/// identity, which is what R11 compares.
pub const DC_CONTRACT: &str = "DC";

/// A `conduit` / `ref` declaration — a declared reference copper with no
/// `[hot, ret]` slot of its own.
pub const CONDUIT_CONTRACT: &str = "conduit";

/// One member exactly as its declaration spelled it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredMember {
    pub face: Face,
    /// The member spelling verbatim from the declaration (`GND`, `VDD_3V3`).
    pub member: String,
    /// The path-independent name of the declaration that owns the member — the
    /// `::DC` interface name for a pin / power-port row, the domain name for a
    /// rail declaration, [`CONDUIT_CONTRACT`] for a reference declaration.
    pub contract: String,
}

impl DeclaredMember {
    fn new(face: Face, member: &str, contract: &str) -> Self {
        Self {
            face,
            member: member.to_string(),
            contract: contract.to_string(),
        }
    }

    /// The identity of the declared conductor this member is: the declaration
    /// that owns it plus its spelling, verbatim — case preserved, no leaf
    /// extraction. Two members share this identity when the *same declaration*
    /// names them the same way, which is the only thing that may make two
    /// conductors one rail.
    pub fn identity(&self) -> String {
        format!("{}:{}", self.contract, self.member)
    }
}

/// The power-face member names **one scope** declares, each spelled as written.
///
/// This is the "identity table on the declaration side" a name may legally be
/// looked up in: a bare reference is legitimate exactly when the scope declares
/// that spelling on the relevant face, matched by exact string equality.
#[derive(Debug, Default, Clone)]
pub struct DeclaredFaces {
    hot: BTreeSet<String>,
    ret: BTreeSet<String>,
    /// Every name the scope declares, faces included. Wider than the two face
    /// sets: a `conduit` / `ref` declaration names a piece of copper without
    /// saying which side of a contract it is on, so it declares a **name** but
    /// no **face**. "Is this name declared here?" and "is this a declared
    /// return?" are different questions (§8.4 of the identity design).
    declared: BTreeSet<String>,
}

impl DeclaredFaces {
    /// The faces a component's own `pins.pwr` rows declare
    /// (`psrc/psnk/psbi … ::DC(…)`).
    pub fn of_pins(pins: &McPins) -> Self {
        let mut faces = Self::default();
        for row in &pins.pwr {
            faces.hot.insert(row.hot.clone());
            faces.declared.insert(row.hot.clone());
            if let Some(ret) = &row.ret {
                faces.ret.insert(ret.clone());
                faces.declared.insert(ret.clone());
            }
        }
        faces
    }

    /// The names a module's own power-intent declarations give it: every
    /// declared DC rail's `hot` / `ret` (the rail's domain is not part of the
    /// *spelling*), every `psrc/psnk/psbi` power-port row's members, and every
    /// declared reference copper (`conduit` / `ref`).
    pub fn of_module(pi: &McPowerDecls) -> Self {
        let mut faces = Self::default();
        for rail in pi.l1_rails() {
            faces.hot.insert(rail.hot.clone());
            faces.ret.insert(rail.ret.clone());
            faces.declared.insert(rail.hot);
            faces.declared.insert(rail.ret);
        }
        // Both the budget-face capture and the wider identity-face one: the
        // first member of a written pair is the supply face, the second the
        // return, whenever the row states a `::DC` contract.
        let pairs = pi
            .pwr_ports
            .iter()
            .map(|row| (row.hot.clone(), row.ret.clone()))
            .chain(pi.dc_port_pairs.iter().cloned());
        for (hot, ret) in pairs {
            faces.hot.insert(hot.clone());
            faces.declared.insert(hot);
            if let Some(ret) = ret {
                faces.ret.insert(ret.clone());
                faces.declared.insert(ret);
            }
        }
        for r in pi.l1_refs() {
            faces.declared.insert(r.name);
        }
        faces
    }

    /// Does this scope declare `name` on the return (ground) side?
    pub fn declares_ret(&self, name: &str) -> bool {
        self.ret.contains(name)
    }

    /// Does this scope declare `name` at all? The exemption A1 grants: a name
    /// the owner itself declared is an identity, however it is spelled.
    pub fn declares(&self, name: &str) -> bool {
        self.declared.contains(name)
    }
}

/// The power-face member a component's `pins.pwr` rows declare for a pin,
/// matched against the pin's own declared **names** (the spelling the
/// declaration wrote — never split, never re-spelled).
pub fn member_of_names(pins: &McPins, names: &[&str]) -> Option<DeclaredMember> {
    for row in &pins.pwr {
        if names.iter().any(|n| *n == row.hot) {
            return Some(DeclaredMember::new(Face::Hot, &row.hot, &row.iface));
        }
        if let Some(ret) = &row.ret {
            if names.iter().any(|n| *n == ret) {
                return Some(DeclaredMember::new(Face::Ret, ret, &row.iface));
            }
        }
    }
    None
}

/// The declared member of an endpoint whose face comes from a **direction word
/// alone**: a `psrc/psnk/psbi` row with no `::` tail still declares which side
/// of the contract the endpoint is, but names no pair, so the endpoint's own
/// spelling is the member under the default `::DC` contract.
pub fn member_from_face(spelling: &str, face: Face) -> DeclaredMember {
    DeclaredMember::new(face, spelling, DC_CONTRACT)
}

/// The power-face member a **module's own** declarations give the net-visible
/// name `name`. A rail declaration answers under its domain name, so two
/// domains' same-spelled returns stay two identities (`va:GND` ≠ `vb:GND`),
/// while a power-port row answers under its `::DC` contract.
pub fn member_of_module(pi: &McPowerDecls, name: &str) -> Option<DeclaredMember> {
    for rail in pi.l1_rails() {
        if rail.hot == name {
            return Some(DeclaredMember::new(Face::Hot, &rail.hot, &rail.domain));
        }
        if rail.ret == name {
            return Some(DeclaredMember::new(Face::Ret, &rail.ret, &rail.domain));
        }
    }
    let pairs = pi
        .pwr_ports
        .iter()
        .map(|row| (row.hot.clone(), row.ret.clone()))
        .chain(pi.dc_port_pairs.iter().cloned());
    for (hot, ret) in pairs {
        if hot == name {
            return Some(DeclaredMember::new(Face::Hot, &hot, DC_CONTRACT));
        }
        if ret.as_deref() == Some(name) {
            return Some(DeclaredMember::new(
                Face::Ret,
                ret.as_deref().unwrap_or_default(),
                DC_CONTRACT,
            ));
        }
    }
    for r in pi.l1_refs() {
        if r.name == name {
            return Some(DeclaredMember::new(Face::Copper, &r.name, CONDUIT_CONTRACT));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faces(names: &[&str]) -> DeclaredFaces {
        let mut f = DeclaredFaces::default();
        for n in names {
            f.declared.insert(n.to_string());
        }
        f
    }

    #[test]
    fn declared_faces_match_by_exact_string_only() {
        let mut f = faces(&["GND", "VDD_3V3"]);
        f.ret.insert("GND".to_string());
        f.hot.insert("VDD_3V3".to_string());
        assert!(f.declares_ret("GND"));
        assert!(f.declares("GND") && f.declares("VDD_3V3"));
        // Case is part of the name (U49): a different spelling is a different
        // name, not the same one folded.
        assert!(!f.declares_ret("gnd"));
        assert!(!f.declares_ret("Gnd"));
        assert!(!f.declares("gnd"));
        // A name the scope never declared is not a face, however supply-like.
        assert!(!f.declares_ret("VSS"));
        assert!(!f.declares("V3V3"));
    }

    #[test]
    fn a_declared_name_need_not_be_a_face() {
        // A `conduit` names copper without picking a side: declared, but no face.
        let f = faces(&["VBUS_RAW"]);
        assert!(f.declares("VBUS_RAW"));
        assert!(!f.declares_ret("VBUS_RAW"));
    }

    #[test]
    fn identity_is_the_contract_plus_the_written_spelling() {
        let m = DeclaredMember::new(Face::Ret, "GND", DC_CONTRACT);
        assert_eq!(m.identity(), "DC:GND");
        let a = DeclaredMember::new(Face::Ret, "GND", "va");
        let b = DeclaredMember::new(Face::Ret, "GND", "vb");
        assert_ne!(a.identity(), b.identity());
        // The face is not part of the identity: a member spelled `GND` is one
        // conductor identity whether a row happens to write it hot or ret.
        let h = DeclaredMember::new(Face::Hot, "GND", DC_CONTRACT);
        assert_eq!(h.identity(), m.identity());
    }
}
