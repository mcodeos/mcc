// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::fmt;

use crate::core::basic::mc_bus::McBus;
use crate::core::mc_inst::McInstance;

// ============================================================================
// McMember - member item
// ============================================================================

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

// ============================================================================
// McMemberList - member list
// ============================================================================

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

// ============================================================================
// McInstanceRef - instance reference
// ============================================================================

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
                let members = bus.member.to_vec().join(",");
                return format!("{}{{{}}}", bus.name, members);
            }
        }

        if self.members.is_empty() {
            if let McInstance::Bus(bus) = &self.base {
                return bus.name.clone();
            }
            self.base.get_name()
        } else {
            let all_members: Vec<String> = self.expand_members();
            if let McInstance::Bus(bus) = &self.base {
                format!("{}{{{}}}", bus.name, all_members.join(","))
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

    pub fn from_string(s: &str) -> Self {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 1 {
            McInstanceRef::from_label(s)
        } else {
            let base_name = parts[0];
            let members: Vec<McMember> = parts[1..]
                .iter()
                .map(|m| McMember::Single(m.to_string()))
                .collect();
            McInstanceRef {
                base: McInstance::Label(base_name.to_string()),
                members: vec![McMemberList { items: members }],
            }
        }
    }

    pub fn to_bus(&self) -> crate::core::basic::mc_bus::McBus {
        use crate::core::basic::mc_bus::McBus;
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

// ============================================================================
// McEndpoint - connection endpoint
// ============================================================================

#[derive(Debug, Clone)]
pub enum McEndpoint {
    Single(McInstanceRef),
    List(Vec<McEndpoint>),
    Node {
        input: Vec<McEndpoint>,
        output: Vec<McEndpoint>,
    },
}

impl McEndpoint {
    pub fn single(ref_: McInstanceRef) -> Self {
        McEndpoint::Single(ref_)
    }

    pub fn list(nodes: Vec<McEndpoint>) -> Self {
        McEndpoint::List(nodes)
    }

    pub fn node(input: Vec<McEndpoint>, output: Vec<McEndpoint>) -> Self {
        McEndpoint::Node { input, output }
    }

    pub fn flatten(&self) -> Vec<McEndpoint> {
        match self {
            McEndpoint::Single(node) => vec![McEndpoint::Single(node.clone())],
            McEndpoint::List(nodes) => nodes.iter().flat_map(|n| n.flatten()).collect(),
            McEndpoint::Node { input, output } => {
                let mut result = Vec::new();
                for n in input.iter().chain(output.iter()) {
                    result.extend(n.flatten());
                }
                result
            }
        }
    }

    pub fn count(&self) -> usize {
        match self {
            McEndpoint::Single(_) => 1,
            McEndpoint::List(nodes) => nodes.iter().map(|n| n.count()).sum(),
            McEndpoint::Node { input, output } => {
                input.iter().map(|n| n.count()).sum::<usize>()
                    + output.iter().map(|n| n.count()).sum::<usize>()
            }
        }
    }

    pub fn from_label(name: &str) -> Self {
        McEndpoint::Single(McInstanceRef::from_label(name))
    }

    pub fn from_labels(names: Vec<&str>) -> Self {
        if names.len() == 1 {
            McEndpoint::from_label(names[0])
        } else {
            let endpoints: Vec<McEndpoint> =
                names.into_iter().map(McEndpoint::from_label).collect();
            McEndpoint::list(endpoints)
        }
    }

    pub fn series(&self, other: &McEndpoint) -> McEndpoint {
        match (self, other) {
            (McEndpoint::List(a), McEndpoint::List(b)) => {
                let mut combined = a.clone();
                combined.extend(b.clone());
                McEndpoint::list(combined)
            }
            (McEndpoint::List(a), other) => {
                let mut combined = a.clone();
                combined.push(other.clone());
                McEndpoint::list(combined)
            }
            (self_, McEndpoint::List(b)) => {
                let mut combined = vec![self_.clone()];
                combined.extend(b.clone());
                McEndpoint::list(combined)
            }
            _ => McEndpoint::list(vec![self.clone(), other.clone()]),
        }
    }

    pub fn parallel(&self, other: &McEndpoint) -> McEndpoint {
        self.series(other)
    }

    pub fn get_left(&self) -> Vec<crate::core::basic::mc_bus::McBus> {
        use crate::core::basic::mc_bus::McBus;
        match self {
            McEndpoint::Single(ref_) => vec![ref_.to_bus()],
            McEndpoint::List(nodes) => {
                if nodes.is_empty() {
                    vec![McBus::new("<error:empty_list>")]
                } else {
                    nodes[0].get_left()
                }
            }
            McEndpoint::Node { input, .. } => {
                if input.is_empty() {
                    vec![McBus::new("<error:empty_input>")]
                } else {
                    input.iter().flat_map(|n| n.get_left()).collect()
                }
            }
        }
    }

    pub fn get_right(&self) -> Vec<crate::core::basic::mc_bus::McBus> {
        use crate::core::basic::mc_bus::McBus;
        match self {
            McEndpoint::Single(ref_) => vec![ref_.to_bus()],
            McEndpoint::List(nodes) => {
                if nodes.is_empty() {
                    vec![McBus::new("<error:empty_list>")]
                } else {
                    nodes.last().unwrap().get_right()
                }
            }
            McEndpoint::Node { output, .. } => {
                if output.is_empty() {
                    vec![McBus::new("<error:empty_output>")]
                } else {
                    output.iter().flat_map(|n| n.get_right()).collect()
                }
            }
        }
    }
}

impl fmt::Display for McEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McEndpoint::Single(ref_) => write!(f, "{ref_}"),
            McEndpoint::List(nodes) => {
                let items: Vec<String> = nodes.iter().map(|n| n.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            McEndpoint::Node { input, output } => {
                let input_str: Vec<String> = input.iter().map(|n| n.to_string()).collect();
                let output_str: Vec<String> = output.iter().map(|n| n.to_string()).collect();
                write!(f, "{{{}|{}}}", input_str.join(", "), output_str.join(", "))
            }
        }
    }
}

// ============================================================================
// Macros
// ============================================================================

#[macro_export]
macro_rules! ep {
    ($name:expr) => {
        $crate::core::basic::mc_endpoint::McEndpoint::from_label($name)
    };
    ($($name:expr),+ $(,)?) => {
        $crate::core::basic::mc_endpoint::McEndpoint::from_labels(vec![$($name),+])
    };
}

#[macro_export]
macro_rules! ep_node {
    ($input:expr => $output:expr) => {
        $crate::core::basic::mc_endpoint::McEndpoint::node(vec![$input], vec![$output])
    };
}

impl From<McInstanceRef> for McEndpoint {
    fn from(ref_: McInstanceRef) -> Self {
        McEndpoint::Single(ref_)
    }
}

impl From<McInstance> for McInstanceRef {
    fn from(inst: McInstance) -> Self {
        McInstanceRef::new(inst)
    }
}

impl From<McInstance> for McEndpoint {
    fn from(inst: McInstance) -> Self {
        McEndpoint::Single(McInstanceRef::new(inst))
    }
}

impl From<McBus> for McEndpoint {
    fn from(bus: McBus) -> Self {
        let members = if bus.member.is_empty() {
            vec![]
        } else {
            vec![McMemberList {
                items: bus.member.into_iter().map(McMember::Single).collect(),
            }]
        };
        McEndpoint::Single(McInstanceRef {
            base: McInstance::Label(bus.name),
            members,
        })
    }
}

impl From<crate::core::basic::mc_bus::McNode> for McEndpoint {
    fn from(node: crate::core::basic::mc_bus::McNode) -> Self {
        let left_ep = McEndpoint::from(node.0);
        let right_ep = McEndpoint::from(node.1);
        McEndpoint::node(vec![left_ep], vec![right_ep])
    }
}
