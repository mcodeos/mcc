// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::{convert::From, iter::Iterator};

/// Which mouth of a call a synthetic funcall sentinel stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum IoSide {
    In,
    Out,
}

impl IoSide {
    /// The word the sentinel's own spelling carries (`<base>.in` / `<base>.out`).
    pub(crate) fn word(self) -> &'static str {
        match self {
            IoSide::In => "in",
            IoSide::Out => "out",
        }
    }

    /// The default face pin this side stands for on a two-pin part.
    pub(crate) fn pin(self) -> &'static str {
        match self {
            IoSide::In => "1",
            IoSide::Out => "2",
        }
    }
}

/// A bus or parameterised identifier with optional member access.
///
/// # `.` (dot) and `{}` (curly braces) equivalence
///
/// In MCode, member/sub access via dot (`.`) and curly braces (`{}`) is semantically
/// equivalent:
///
/// ```text
/// DC2.VDD    ≡  DC2{VDD}      // single member access
/// res5.1     ≡  res5{1}       // single member (index)
/// rs485.A    ≡  rs485{A}      // bus member access
/// ```
///
/// Both forms resolve to the same internal representation (`McBus`). Which form is
/// used in source code is a stylistic choice; the parser normalises both to the same
/// AST and the Display/Debug output uses `{}` notation.
#[derive(Clone)]
pub struct McBus {
    pub(crate) name: String,
    pub(crate) member: Vec<String>,
    pub(crate) full_members: Vec<String>,
    /// The side a synthetic funcall sentinel stands for; `None` on every real
    /// bus. The producer declares it ([`McBus::synthetic_io`]) and consumers
    /// read it — nobody decodes the trailing segment (world-axioms §1 A1).
    /// Excluded from equality: it records where the bus came from, not what the
    /// bus is.
    pub(crate) synthetic: Option<IoSide>,
}

impl PartialEq for McBus {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.member == other.member
            && self.full_members == other.full_members
    }
}

impl Eq for McBus {}

impl std::fmt::Debug for McBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.full_members.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}{{{}}}", self.name, self.full_members.join(", "))
        }
    }
}

impl McBus {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            member: Vec::new(),
            full_members: Vec::new(),
            synthetic: None,
        }
    }

    /// The `<base>.{in,out}` endpoint a funcall synthesizes when it has no
    /// caller: the call's own interface is unknown, so the bus is a sentinel
    /// with no identity, carrying only the side it stands for. Producers are the
    /// funcall parse sites; every consumer must drop it.
    pub(crate) fn synthetic_io(base: &str, side: IoSide) -> Self {
        Self {
            name: format!("{base}.{}", side.word()),
            member: Vec::new(),
            full_members: Vec::new(),
            synthetic: Some(side),
        }
    }

    pub(crate) fn is_synthetic(&self) -> bool {
        self.synthetic.is_some()
    }

    pub(crate) fn synthetic_side(&self) -> Option<IoSide> {
        self.synthetic
    }

    pub(crate) fn new_with_members(name: &str, members: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            member: members.clone(),
            full_members: members,
            synthetic: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sentinel prefix used for error buses that should not appear in
    /// netlist or LSP output (Defect 89).
    pub(crate) const ERROR_PREFIX: &'static str = "<error:";

    pub(crate) fn add_member(&mut self, name: &str) {
        if !self.full_members.contains(&name.to_string()) {
            self.full_members.push(name.to_string());
        }
    }

    pub(crate) fn get_full_members(&self) -> &Vec<String> {
        &self.full_members
    }

    pub(crate) fn member_ref(base: &str, member: String) -> Self {
        Self {
            name: base.to_string(),
            member: vec![member.clone()],
            full_members: vec![member],
            synthetic: None,
        }
    }

    /// Calculate node size (number of leaf nodes)
    pub(crate) fn size(&self) -> usize {
        if self.member.is_empty() {
            1
        } else {
            self.member.len()
        }
    }
}

impl From<&McBus> for Vec<McBus> {
    fn from(bus: &McBus) -> Self {
        vec![McBus {
            name: bus.name.clone(),
            member: bus.member.clone(),
            full_members: bus.full_members.clone(),
            synthetic: bus.synthetic,
        }]
    }
}

impl From<McBus> for Vec<McBus> {
    fn from(bus: McBus) -> Self {
        vec![McBus {
            name: bus.name,
            member: bus.member,
            full_members: bus.full_members,
            synthetic: bus.synthetic,
        }]
    }
}

// McList

#[derive(Clone)]
/// A list of identifiers, e.g. `[VDD1, GND1]`.
///
/// # `.` (dot) and `{}` (curly braces) equivalence
///
/// Same equivalence as [`McBus`]: square-bracket grouped identifiers are internally
/// represented as a list, and member access via `.` or `{}` resolves to the same
/// underlying element.
pub struct McList {
    pub(crate) name: String,
    pub(crate) member: Vec<String>,
}

impl std::fmt::Debug for McList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.member.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}[{}]", self.name, self.member.join(", "))
        }
    }
}

impl McList {
    pub(crate) fn new_with_members(name: &str, members: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            member: members,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn add_member(&mut self, name: &str) {
        self.member.push(name.to_string());
    }
}

// McNode - represents left and right ends of a connection

#[derive(Clone)]
pub struct McNode(pub McBus, pub McBus);

impl std::fmt::Debug for McNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} ~ {:?}", self.0, self.1)
    }
}

impl McNode {
    pub fn new(left: McBus, right: McBus) -> Self {
        McNode(left, right)
    }

    pub fn left(&self) -> &McBus {
        &self.0
    }

    pub fn right(&self) -> &McBus {
        &self.1
    }

    /// Convert from (Vec<McBus>, Vec<McBus>) to McNode
    pub fn from_left_right(left_elems: &[McBus], right_elems: &[McBus]) -> Self {
        let left_bus = Self::elements_to_bus(left_elems);
        let right_bus = Self::elements_to_bus(right_elems);
        McNode(left_bus, right_bus)
    }

    /// Convert from Vec<McBus> to McNode
    /// Split elements in half, the first half as left, the second half as right
    pub fn from_node_elements(elements: &[McBus]) -> Self {
        if elements.is_empty() {
            return McNode(McBus::new("<empty>"), McBus::new("<empty>"));
        }

        // If there is only one element, treat as pass-through
        if elements.len() == 1 {
            let elem = &elements[0];
            let bus = McBus::new_with_members(&elem.name, elem.member.clone());
            return McNode(bus.clone(), bus);
        }

        // Multiple elements: first half as left, second half as right
        let mid = elements.len() / 2;
        let left_elems = &elements[..mid];
        let right_elems = &elements[mid..];

        let left_bus = Self::elements_to_bus(left_elems);
        let right_bus = Self::elements_to_bus(right_elems);

        McNode(left_bus, right_bus)
    }

    fn elements_to_bus(elems: &[McBus]) -> McBus {
        if elems.is_empty() {
            return McBus::new("<empty>");
        }
        if elems.len() == 1 {
            return elems[0].clone();
        }
        // Multiple elements: merge names, collect all members
        let name = &elems[0].name;
        let members: Vec<String> = elems.iter().flat_map(|e| e.member.clone()).collect();
        McBus::new_with_members(name, members)
    }
}

impl From<McNode> for Vec<McBus> {
    fn from(node: McNode) -> Self {
        let mut result = Vec::from(&node.0);
        result.extend(Vec::from(&node.1));
        result
    }
}

impl From<&McNode> for Vec<McBus> {
    fn from(node: &McNode) -> Self {
        let mut result = Vec::from(&node.0);
        result.extend(Vec::from(&node.1));
        result
    }
}

impl std::fmt::Display for McBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.member.is_empty() {
            write!(f, "{}", self.name)
        } else {
            let members = self.member.to_vec().join(",");
            write!(f, "{}{{{}}}", self.name, members)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sentinel records which mouth of the call it stands for, and the base
    /// it was named after. Consumers read both off the bus; the spelling is the
    /// producer's, never decoded back (world-axioms §1 A1).
    #[test]
    fn synthetic_io_declares_its_side() {
        let left = McBus::synthetic_io("uC", IoSide::In);
        assert!(left.is_synthetic());
        assert_eq!(left.synthetic_side(), Some(IoSide::In));
        assert_eq!(left.name(), "uC.in");

        let right = McBus::synthetic_io("uC", IoSide::Out);
        assert_eq!(right.synthetic_side(), Some(IoSide::Out));
        assert_eq!(right.name(), "uC.out");
    }

    /// Provenance records where a bus came from, not which net it is: a sentinel
    /// and a plain bus of the same shape are the same node. Deriving `PartialEq`
    /// over the field instead would split them.
    #[test]
    fn provenance_is_not_identity() {
        let sentinel = McBus::synthetic_io("uC", IoSide::In);
        let plain = McBus::new("uC.in");
        assert_eq!(sentinel, plain);
        assert!(!plain.is_synthetic());
    }

    /// Every conversion the funccall dispatch uses to hand left/right on must
    /// carry the provenance with it, otherwise downstream consumers see an
    /// ordinary bus.
    #[test]
    fn provenance_survives_the_hand_offs() {
        let sentinel = McBus::synthetic_io("uC", IoSide::Out);
        for carrier in [
            Vec::from(&sentinel),
            Vec::from(sentinel.clone()),
            vec![sentinel.clone()],
        ] {
            assert!(
                carrier
                    .iter()
                    .all(|b| b.synthetic_side() == Some(IoSide::Out)),
                "provenance lost in hand-off: {carrier:?}"
            );
        }
    }
}
