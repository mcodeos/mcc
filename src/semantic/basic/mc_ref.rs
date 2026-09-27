// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::fmt;

use crate::semantic::basic::mc_bus::BusErrorKind;

use crate::semantic::mc_inst::McInstance;

// McInstanceRef - instance reference

#[derive(Debug, Clone)]
pub struct McInstanceRef {
    pub base: McInstance,
}

impl McInstanceRef {
    pub fn new(base: McInstance) -> Self {
        Self { base }
    }

    pub fn full_name(&self) -> String {
        if let McInstance::Bus(bus) = &self.base {
            if !bus.member.is_empty() {
                let members = bus.member.to_vec().join(", ");
                return format!("{}{{{}}}", bus.name, members);
            }
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

        self.base.get_name().to_string()
    }

    pub fn from_label(name: &str) -> Self {
        McInstanceRef::new(McInstance::Label(name.to_string()))
    }

    pub fn to_bus(&self) -> crate::semantic::basic::mc_bus::McBus {
        use crate::semantic::basic::mc_bus::McBus;
        if let McInstance::Bus(bus) = &self.base {
            return bus.clone();
        }
        McBus::new(&self.base.get_name())
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

    pub fn from_label(name: &str) -> Self {
        McRef::Name(McInstanceRef::from_label(name))
    }

    pub fn from_labels(names: Vec<&str>) -> Self {
        if names.len() == 1 {
            McRef::from_label(names[0])
        } else {
            let endpoints: Vec<McRef> = names.into_iter().map(McRef::from_label).collect();
            McRef::group(endpoints)
        }
    }

    pub fn get_left(&self) -> Vec<crate::semantic::basic::mc_bus::McBus> {
        use crate::semantic::basic::mc_bus::McBus;
        match self {
            McRef::Name(ref_) => vec![ref_.to_bus()],
            McRef::Group(nodes) => {
                if nodes.is_empty() {
                    vec![McBus::new_error(BusErrorKind::EmptyList)]
                } else {
                    nodes[0].get_left()
                }
            }
            McRef::Ports { left, .. } => {
                if left.is_empty() {
                    vec![McBus::new_error(BusErrorKind::EmptyInput)]
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
                    vec![McBus::new_error(BusErrorKind::EmptyList)]
                } else {
                    nodes.last().unwrap().get_right()
                }
            }
            McRef::Ports { right, .. } => {
                if right.is_empty() {
                    vec![McBus::new_error(BusErrorKind::EmptyOutput)]
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
