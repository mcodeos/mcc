// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::{convert::From, iter::Iterator};

#[derive(Clone)]
pub struct McBus {
    pub(crate) name: String,
    pub(crate) member: Vec<String>,
    pub(crate) full_members: Vec<String>,
}

impl std::fmt::Debug for McBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.full_members.is_empty() {
            write!(f, "{}", self.name)
        } else {
            write!(f, "{}[{}]", self.name, self.full_members.join(", "))
        }
    }
}

impl McBus {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            member: Vec::new(),
            full_members: Vec::new(),
        }
    }

    pub(crate) fn new_with_members(name: &str, members: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            member: members.clone(),
            full_members: members,
        }
    }

    pub(crate) fn new_with_name_and_members(
        name: String,
        members: Vec<String>,
        full_members: Vec<String>,
    ) -> Self {
        Self {
            name,
            member: members,
            full_members,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn add_member(&mut self, name: &str) {
        if !self.full_members.contains(&name.to_string()) {
            self.full_members.push(name.to_string());
        }
    }

    pub(super) fn has_member(&self) -> bool {
        !self.member.is_empty()
    }

    pub(crate) fn has_full_member(&self, member: &str) -> bool {
        self.full_members.contains(&member.to_string())
    }

    pub(crate) fn get_full_members(&self) -> &Vec<String> {
        &self.full_members
    }

    pub(crate) fn member_ref(base: &str, member: String) -> Self {
        Self {
            name: base.to_string(),
            member: vec![member.clone()],
            full_members: vec![member],
        }
    }

    /// Find member, return new McBus with full path
    pub(crate) fn find_member(&self, id: &str) -> McBus {
        if let Some(found) = self.find_member_opt(id) {
            found
        } else {
            McBus {
                name: format!("{}.<error:{}>", self.name, id),
                member: Vec::new(),
                full_members: Vec::new(),
            }
        }
    }

    /// Option version of find member
    pub(crate) fn find_member_opt(&self, id: &str) -> Option<McBus> {
        for each in self.member.iter() {
            if *each == id {
                return Some(McBus {
                    name: format!("{}.{}", self.name, each),
                    member: Vec::new(),
                    full_members: Vec::new(),
                });
            }
        }
        None
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
        }]
    }
}

impl From<McBus> for Vec<McBus> {
    fn from(bus: McBus) -> Self {
        vec![McBus {
            name: bus.name,
            member: bus.member,
            full_members: bus.full_members,
        }]
    }
}

// ============================================================================
// McList
// ============================================================================

#[derive(Clone)]
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
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            member: Vec::new(),
        }
    }

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

    pub(super) fn has_member(&self) -> bool {
        !self.member.is_empty()
    }
}

// ============================================================================
// McNode - represents left and right ends of a connection
// ============================================================================

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
            write!(f, "{}[{}]", self.name, members)
        }
    }
}
