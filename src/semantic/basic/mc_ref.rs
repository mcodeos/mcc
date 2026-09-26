// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::fmt;

use crate::semantic::basic::mc_bus::McBus;
use crate::semantic::mc_inst::McInstance;

// McMember - member item

#[derive(Debug, Clone)]
pub enum McMember {
    Single(String),
    Range { start: usize, end: usize },
}

impl McMember {
    pub fn expand(&self) -> Vec<String> {
        match self {
            McMember::Single(s) => vec![s.clone()],
            McMember::Range { start, end } => (*start..=*end).map(|i| i.to_string()).collect(),
        }
    }
}

impl fmt::Display for McMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McMember::Single(s) => write!(f, "{s}"),
            McMember::Range { start, end } => write!(f, "{start}:{end}"),
        }
    }
}

// McMemberList - member list

#[derive(Debug, Clone)]
pub struct McMemberList {
    pub items: Vec<McMember>,
}

impl McMemberList {
    pub fn new(items: Vec<McMember>) -> Self {
        Self { items }
    }

    pub fn expand(&self) -> Vec<String> {
        self.items.iter().flat_map(|m| m.expand()).collect()
    }

    pub fn count(&self) -> usize {
        self.items.iter().map(|m| m.expand().len()).sum()
    }
}

impl fmt::Display for McMemberList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let items_str: Vec<String> = self.items.iter().map(|m| m.to_string()).collect();
        write!(f, "{}", items_str.join(", "))
    }
}

// McInstanceRef - instance reference

#[derive(Debug, Clone)]
pub struct McInstanceRef {
    pub base: McInstance,
    pub members: Vec<McMemberList>,
}

impl McInstanceRef {
    pub fn new(base: McInstance) -> Self {
        Self {
            base,
            members: Vec::new(),
        }
    }

    pub fn with_members(mut self, members: Vec<McMemberList>) -> Self {
        self.members = members;
        self
    }

    pub fn add_member(&mut self, member: McMemberList) {
        self.members.push(member);
    }

    pub fn full_name(&self) -> String {
        if let McInstance::Bus(bus) = &self.base {
            if !bus.member.is_empty() {
                let members = bus.member.to_vec().join(", ");
                return format!("{}{{{}}}", bus.name, members);
            }
        }

        if self.members.is_empty() {
            if let McInstance::Bus(bus) = &self.base {
                return bus.name.clone();
            }
            // An interface instance created by a `name::IFACE(params)` declareb
            // (e.g. `V5V::DC(5V)`) keeps its interface annotation so connection
            // lines render `V5V::DC(5V)` instead of the bare instance name.
            if let McInstance::Interface(i) = &self.base {
                let name_str = if i.name.is_list() {
                    i.name
                        .list_members()
                        .map(|m| format!("{{{}}}", m.join(",")))
                        .unwrap_or_else(|| format!("{}", i.name))
                } else {
                    format!("{}", i.name)
                };
                let params_str = if i.params.is_empty() {
                    String::new()
                } else {
                    format!(
                        "({})",
                        i.params
                            .iter()
                            .map(|p| format!("{p}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                return format!("{name_str}::{}{params_str}", i.base.name);
            }
            self.base.get_name()
        } else {
            let all_members: Vec<String> = self.expand_members();
            if let McInstance::Bus(bus) = &self.base {
                format!("{}{{{}}}", bus.name, all_members.join(", "))
            } else {
                let members_str: Vec<String> =
                    self.members.iter().map(|ml| format!("{{{ml}}}")).collect();
                format!("{}.{}", self.base.get_name(), members_str.join(""))
            }
        }
    }

    pub fn expand_members(&self) -> Vec<String> {
        self.members.iter().flat_map(|ml| ml.expand()).collect()
    }

    pub fn from_label(name: &str) -> Self {
        McInstanceRef::new(McInstance::Label(name.to_string()))
    }

    pub fn from_bus(name: &str, members: Vec<String>) -> Self {
        let member_list = if members.is_empty() {
            vec![]
        } else {
            vec![McMemberList {
                items: members.into_iter().map(McMember::Single).collect(),
            }]
        };
        McInstanceRef {
            base: McInstance::Label(name.to_string()),
            members: member_list,
        }
    }

    pub fn to_bus(&self) -> crate::semantic::basic::mc_bus::McBus {
        use crate::semantic::basic::mc_bus::McBus;
        if let McInstance::Bus(bus) = &self.base {
            return bus.clone();
        }
        let members = self.expand_members();
        McBus::new_with_members(&self.base.get_name(), members)
    }
}

impl fmt::Display for McInstanceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.full_name())
    }
}

// McRef - connection reference (the **reference face**: the spelling the user
// wrote, closed over syntax and carrying no port semantics).

#[derive(Debug, Clone)]
pub enum McRef {
    /// A single reference: a bare name, an instance path, a bus, or a named
    /// list.
    Name(McInstanceRef),
    /// A bracketed group `[a, b, ...]` — a pure reference container. Its
    /// members are references, so the value face's node shape cannot be
    /// expressed here at all.
    Group(Vec<McRef>),
    /// An explicit two-sided port spelling `{a, b | c}`.
    Ports {
        left: Vec<McRef>,
        right: Vec<McRef>,
    },
}

impl McRef {
    pub fn name(ref_: McInstanceRef) -> Self {
        McRef::Name(ref_)
    }

    pub fn group(refs: Vec<McRef>) -> Self {
        McRef::Group(refs)
    }

    pub fn ports(left: Vec<McRef>, right: Vec<McRef>) -> Self {
        McRef::Ports { left, right }
    }

    /// The references this spelling names, in source order: `Ports` yields its
    /// `left` list then its `right` list, the same order and repetition the old
    /// `flatten` produced. Zero port semantics — it answers only "which
    /// references does this spelling point at", so it invents no law.
    pub fn leaves(&self) -> Vec<&McInstanceRef> {
        match self {
            McRef::Name(r) => vec![r],
            McRef::Group(refs) => refs.iter().flat_map(|r| r.leaves()).collect(),
            McRef::Ports { left, right } => left
                .iter()
                .chain(right.iter())
                .flat_map(|r| r.leaves())
                .collect(),
        }
    }

    pub fn flatten(&self) -> Vec<McRef> {
        match self {
            McRef::Name(node) => vec![McRef::Name(node.clone())],
            McRef::Group(nodes) => nodes.iter().flat_map(|n| n.flatten()).collect(),
            McRef::Ports { left, right } => {
                let mut result = Vec::new();
                for n in left.iter().chain(right.iter()) {
                    result.extend(n.flatten());
                }
                result
            }
        }
    }

    pub fn count(&self) -> usize {
        match self {
            McRef::Name(_) => 1,
            McRef::Group(nodes) => nodes.iter().map(|n| n.count()).sum(),
            McRef::Ports { left, right } => {
                left.iter().map(|n| n.count()).sum::<usize>()
                    + right.iter().map(|n| n.count()).sum::<usize>()
            }
        }
    }

    pub fn from_label(name: &str) -> Self {
        McRef::Name(McInstanceRef::from_label(name))
    }

    pub fn from_labels(names: Vec<&str>) -> Self {
        if names.len() == 1 {
            McRef::from_label(names[0])
        } else {
            let endpoints: Vec<McRef> =
                names.into_iter().map(McRef::from_label).collect();
            McRef::group(endpoints)
        }
    }

    pub fn series(&self, other: &McRef) -> McRef {
        match (self, other) {
            (McRef::Group(a), McRef::Group(b)) => {
                let mut combined = a.clone();
                combined.extend(b.clone());
                McRef::group(combined)
            }
            (McRef::Group(a), other) => {
                let mut combined = a.clone();
                combined.push(other.clone());
                McRef::group(combined)
            }
            (self_, McRef::Group(b)) => {
                let mut combined = vec![self_.clone()];
                combined.extend(b.clone());
                McRef::group(combined)
            }
            _ => McRef::group(vec![self.clone(), other.clone()]),
        }
    }

    pub fn get_left(&self) -> Vec<crate::semantic::basic::mc_bus::McBus> {
        use crate::semantic::basic::mc_bus::McBus;
        match self {
            McRef::Name(ref_) => vec![ref_.to_bus()],
            McRef::Group(nodes) => {
                if nodes.is_empty() {
                    vec![McBus::new("<error:empty_list>")]
                } else {
                    nodes[0].get_left()
                }
            }
            McRef::Ports { left, .. } => {
                if left.is_empty() {
                    vec![McBus::new("<error:empty_input>")]
                } else {
                    left.iter().flat_map(|n| n.get_left()).collect()
                }
            }
        }
    }

    pub fn get_right(&self) -> Vec<crate::semantic::basic::mc_bus::McBus> {
        use crate::semantic::basic::mc_bus::McBus;
        match self {
            McRef::Name(ref_) => vec![ref_.to_bus()],
            McRef::Group(nodes) => {
                if nodes.is_empty() {
                    vec![McBus::new("<error:empty_list>")]
                } else {
                    nodes.last().unwrap().get_right()
                }
            }
            McRef::Ports { right, .. } => {
                if right.is_empty() {
                    vec![McBus::new("<error:empty_output>")]
                } else {
                    right.iter().flat_map(|n| n.get_right()).collect()
                }
            }
        }
    }
}

impl fmt::Display for McRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McRef::Name(ref_) => write!(f, "{ref_}"),
            McRef::Group(nodes) => {
                let items: Vec<String> = nodes.iter().map(|n| n.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            McRef::Ports { left, right } => {
                let left_str: Vec<String> = left.iter().map(|n| n.to_string()).collect();
                let right_str: Vec<String> = right.iter().map(|n| n.to_string()).collect();
                write!(f, "{{{}|{}}}", left_str.join(", "), right_str.join(", "))
            }
        }
    }
}

// Macros

#[macro_export]
macro_rules! ep {
    ($name:expr) => {
        $crate::semantic::basic::mc_ref::McRef::from_label($name)
    };
    ($($name:expr),+ $(,)?) => {
        $crate::semantic::basic::mc_ref::McRef::from_labels(vec![$($name),+])
    };
}

#[macro_export]
macro_rules! ep_node {
    ($input:expr => $output:expr) => {
        $crate::semantic::basic::mc_ref::McRef::ports(vec![$input], vec![$output])
    };
}

impl From<McInstanceRef> for McRef {
    fn from(ref_: McInstanceRef) -> Self {
        McRef::Name(ref_)
    }
}

impl From<McInstance> for McInstanceRef {
    fn from(inst: McInstance) -> Self {
        McInstanceRef::new(inst)
    }
}

impl From<McInstance> for McRef {
    fn from(inst: McInstance) -> Self {
        McRef::Name(McInstanceRef::new(inst))
    }
}

impl From<McBus> for McRef {
    fn from(bus: McBus) -> Self {
        let members = if bus.member.is_empty() {
            vec![]
        } else {
            vec![McMemberList {
                items: bus.member.into_iter().map(McMember::Single).collect(),
            }]
        };
        McRef::Name(McInstanceRef {
            base: McInstance::Label(bus.name),
            members,
        })
    }
}

impl From<crate::semantic::basic::mc_bus::McNode> for McRef {
    fn from(node: crate::semantic::basic::mc_bus::McNode) -> Self {
        let left_ep = McRef::from(node.0);
        let right_ep = McRef::from(node.1);
        McRef::ports(vec![left_ep], vec![right_ep])
    }
}
