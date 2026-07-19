// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::McIds;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum McOpd {
    Id(McIds),
    This(McIds),
    Pins(McIds),
    Uscore,
}

impl McOpd {
    pub fn new(node: &AstNode) -> Option<Self> {
        // Check is MCAST_OPD type
        let node_type = node.get_type();
        if node_type != MCAST_OPD {
            // For MCAST_ID/MCAST_IDA/MCAST_IDS, still try to handle (may have DOT sibling nodes)
            if node_type == MCAST_ID || node_type == MCAST_IDA || node_type == MCAST_IDS {
                return Self::new_from_ids_node(node);
            } else if node_type == MCAST_INSTANCE {
                // When MCAST_INSTANCE appears in operand context, extract instance name as McOpd::Id
                if let Some(sub) = node.get_sub_node() {
                    if let Some(ids) = McIds::new(&sub) {
                        return Some(McOpd::Id(ids));
                    }
                    // Child node may be MCAST_OPD-wrapped identifier
                    if sub.get_type() == MCAST_OPD {
                        if let Some(inner) = sub.get_sub_node() {
                            if let Some(ids) = McIds::new(&inner) {
                                return Some(McOpd::Id(ids));
                            }
                        }
                    }
                }
                return None;
            } else {
                return None;
            }
        }
        let Some(snode) = node.get_sub_node() else {
            dlog_error(1101, node, "Missing subnode");
            return None;
        };

        match snode.get_type() {
            // | mc_underscore
            MCAST_OPD_USCORE => Some(McOpd::Uscore),

            //mc_opd: mc_ids
            //      | mc_ids MCPT_DOT mc_int
            MCAST_ID | MCAST_IDA | MCAST_IDS => {
                if let Some(mut ids) = crate::McIds::new(&snode) {
                    let next_node = snode.get_next();
                    if let Some(dot) = next_node {
                        ids.append(&dot);
                        return Some(McOpd::Id(ids));
                    }
                    Some(McOpd::Id(ids))
                } else {
                    None
                }
            }
            // | MCK_THIS
            // | MCK_THIS mc_idm
            // | MCK_THIS MCPT_DOT mc_int
            // | MCK_THIS mc_idm MCPT_DOT mc_int
            MCAST_OPD_THIS => {
                let mut thisid = McIds::from("this");
                if let Some(nextnode) = snode.get_next() {
                    thisid.append(&nextnode);
                    return Some(McOpd::This(thisid));
                }
                Some(McOpd::This(thisid))
            }
            // | MCK_PINS mc_idm
            // | MCK_PINS MCPT_DOT mc_int
            MCAST_OPD_PINS => {
                let mut pinsid = McIds::from("pins");
                if let Some(nextnode) = snode.get_next() {
                    pinsid.append(&nextnode);
                    return Some(McOpd::Pins(pinsid));
                }
                Some(McOpd::Pins(pinsid))
            }
            // When MCAST_INSTANCE appears as MCAST_OPD child node,
            // extract instance name as McOpd::Id
            MCAST_INSTANCE => {
                if let Some(sub) = snode.get_sub_node() {
                    if let Some(ids) = McIds::new(&sub) {
                        return Some(McOpd::Id(ids));
                    }
                    // Child node may be MCAST_OPD-wrapped
                    if sub.get_type() == MCAST_OPD {
                        if let Some(inner) = sub.get_sub_node() {
                            if let Some(ids) = McIds::new(&inner) {
                                return Some(McOpd::Id(ids));
                            }
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Helper function to process MCAST_ID/MCAST_IDA/MCAST_IDS nodes
    /// This is needed because sometimes the parser passes these types directly
    /// instead of wrapped in MCAST_OPD, especially when there's a DOT sibling
    fn new_from_ids_node(node: &AstNode) -> Option<Self> {
        if let Some(mut ids) = McIds::new(node) {
            if let Some(dot) = node.get_next() {
                ids.append(&dot);
                return Some(McOpd::Id(ids));
            }
            Some(McOpd::Id(ids))
        } else {
            None
        }
    }

    pub fn expand(&self) -> Vec<String> {
        match self {
            McOpd::Id(id) => id.expand(),
            McOpd::This(this) => this.expand(),
            McOpd::Pins(pins) => pins.expand(),
            McOpd::Uscore => vec![],
        }
    }

    /// Get main name (if any)
    /*pub fn get_primary_name(&self) -> Option<String> {
        match self {
            McOpd::Id(name) => Some(name.clone()),
            McOpd::This(name) => Some(name.clone()),
            McOpd::Pins(name) => Some(name.clone()),
            McOpd::Uscore => None,
        }
    }*/

    /// Get member list (if any)
    pub fn get_members(&self) -> Vec<&McOpd> {
        match self {
            McOpd::Id(_) => vec![],
            McOpd::This(_) => vec![],
            McOpd::Pins(_) => vec![],
            McOpd::Uscore => vec![],
        }
    }

    /// Try to convert to simple string list (for anonymous params)
    pub fn to_string_list(&self) -> Option<Vec<String>> {
        match self {
            McOpd::Id(name) => Some(vec![name.to_string()]),
            McOpd::This(name) => Some(vec![name.to_string()]),
            McOpd::Pins(name) => Some(vec![name.to_string()]),
            McOpd::Uscore => Some(vec!["_".to_string()]),
        }
    }

    /// Check if operand matches target name
    pub fn match_name(&self, target: &str) -> bool {
        match self {
            McOpd::Id(name) => name.match_name(target),
            McOpd::This(name) => name.match_name(target),
            McOpd::Pins(name) => name.match_name(target),
            McOpd::Uscore => target == "_",
        }
    }
}

impl McOpd {
    pub fn to_string(&self) -> String {
        match self {
            McOpd::Id(s) => s.to_string(),
            McOpd::This(s) => s.to_string(),
            McOpd::Pins(s) => s.to_string(),
            McOpd::Uscore => "_".to_string(),
        }
    }

    /*pub fn is_empty(&self) -> bool {
        match self {
            McOpd::Id(s) => s.is_empty(),
            McOpd::This(s) => s.is_empty(),
            McOpd::Pins(s) => s.is_empty(),
            McOpd::Uscore => false,
        }
    }*/

    pub fn len(&self) -> usize {
        match self {
            McOpd::Id(s) => s.len(),
            McOpd::This(s) => s.len(),
            McOpd::Pins(s) => s.len(),
            McOpd::Uscore => 0,
        }
    }
}

impl From<&str> for McOpd {
    fn from(s: &str) -> Self {
        McOpd::Id(McIds::from(s))
    }
}

impl std::fmt::Display for McOpd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McOpd::Id(name) => write!(f, "{name}"),
            McOpd::This(name) => write!(f, "{name}"),
            McOpd::Pins(name) => write!(f, "{name}"),
            McOpd::Uscore => write!(f, "_"),
        }
    }
}
