// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::db::diagnostic::diagnostic::dlog_error;
use crate::semantic::basic::mc_ids::IdsSegment;
use crate::semantic::basic::mc_literal::McInt;
use crate::McIds;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
                // When MCAST_INSTANCE appears in operand context, extract instance name as
                // McOpd::Id
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
            } else if matches!(
                node_type,
                MCAST_OPD_DOT | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN
            ) {
                // P1-2: top-level DOT/CURLY operands
                // McExpression::new routes OPD_DOT (`a.b`) / OPD_CURLY
                // (`a{B,C}`) / OPD_CURLY_MN directly here; letting them fall
                // through to `_ => None` drops the expression silently. Rebuild
                // the id chain from the sub-nodes, structurally (U343 B1 arm 1):
                // every element must carry a real operand reading — the old
                // Display-text fallback pasted non-McIds subtrees (CURLY_MN's
                // OPDS sides, unknown nodes, even `<node_type_N>` placeholders)
                // into the chain as flat name text.
                //
                // Face-aware routing: a dot chain has no face grouping; a
                // curly keeps its base ahead of one member group holding the
                // body; a curly-MN's OPDS side wrappers are the pipe boundary
                // and both sides fold into that one group (the value-face
                // reading — the same ',' / '|' equivalence the ids-level
                // reader applies).
                let mut segments = Vec::new();
                let mut group: Vec<IdsSegment> = Vec::new();
                let mut first = true;
                let mut cur = node.get_sub_node();
                while let Some(n) = cur {
                    // A dot chain's int tail keeps its dot (`TTL.7400`,
                    // McIds::new_with_dot): a non-first `mc_int` element is
                    // the `.int`, not a bare name.
                    if node_type == MCAST_OPD_DOT && !first && n.get_type() == MCAST_INT {
                        let Some(int) = crate::semantic::basic::mc_literal::McInt::new(&n) else {
                            return None;
                        };
                        segments.push(IdsSegment::DotInt(Box::new(int)));
                        first = false;
                        cur = n.get_next();
                        continue;
                    }
                    let to_group = if node_type == MCAST_OPD_DOT {
                        false
                    } else if node_type == MCAST_OPD_CURLY {
                        !first
                    } else {
                        n.get_type() == MCAST_OPDS
                    };
                    let pushed = if to_group {
                        Self::push_group_member(&mut group, &n)
                    } else {
                        Self::push_chain_segments(&mut segments, &n)
                    };
                    first = false;
                    if !pushed {
                        // An element with no operand reading: reject the whole
                        // operand instead of inventing segments from its text.
                        return None;
                    }
                    cur = n.get_next();
                }
                if !group.is_empty() {
                    segments.push(IdsSegment::Curly(group));
                }
                if segments.is_empty() {
                    return None;
                }
                return Some(McOpd::Id(McIds { segments }));
            } else {
                return None;
            }
        }
        let Some(snode) = node.get_sub_node() else {
            dlog_error(
                crate::errcodes::NAME_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(crate::errcodes::NAME_MISSING_SUBNODE, &[]),
            );
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
            // | MCK_THIS | MCK_PINS
            // | (MCK_THIS|MCK_PINS) mc_idm
            // | (MCK_THIS|MCK_PINS) MCPT_DOT mc_int
            // | (MCK_THIS|MCK_PINS) mc_idm MCPT_DOT mc_int
            MCAST_OPD_THIS | MCAST_OPD_PINS => {
                // The payload holds the spelling the source used, and that text
                // picks the self face to build.
                let keyword = snode
                    .data_as_cstr()
                    .and_then(|c| c.to_str().ok())
                    .unwrap_or("this");
                let mut selfid = McIds::from(keyword);
                if let Some(nextnode) = snode.get_next() {
                    // `append` walks the node and every following sibling, so
                    // one call carries the whole tail (`this{a,b}.3` links
                    // [this, idm, .int]).
                    selfid.append(&nextnode);
                }
                if keyword == "pins" {
                    Some(McOpd::Pins(selfid))
                } else {
                    Some(McOpd::This(selfid))
                }
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

    /// Read one member of a curly group (`x{a, b}` body / curly-MN sides).
    /// Members follow the ids-level curly reader's member law (U249): an ids
    /// chain (`ADC.P`) stays one `Ids` member so the member count and the
    /// expansion are single-member — a plain `McIds::new` read would spread
    /// it into two sibling segments. An OPDS side wrapper unfolds into its
    /// members (the keyword arms double-wrap the side list).
    fn push_group_member(group: &mut Vec<IdsSegment>, n: &AstNode) -> bool {
        if n.get_type() == MCAST_OPDS {
            let mut inner = n.get_sub_node();
            let mut any = false;
            while let Some(m) = inner {
                if !Self::push_group_member(group, &m) {
                    return false;
                }
                any = true;
                inner = m.get_next();
            }
            return any;
        }
        let ids_node = if n.get_type() == MCAST_OPD {
            match n.get_sub_node() {
                Some(sub) if sub.get_type() == MCAST_IDS => sub,
                _ => n.clone(),
            }
        } else {
            n.clone()
        };
        if ids_node.get_type() == MCAST_IDS {
            return match McIds::new(&ids_node) {
                Some(ids) => {
                    group.push(IdsSegment::Ids(Box::new(ids)));
                    true
                }
                None => false,
            };
        }
        Self::push_chain_segments(group, n)
    }

    /// Read one element of a DOT/CURLY/CURLY_MN operand chain into id
    /// segments, structurally (U343 B1 arm 1). Returns false when the node
    /// has no operand reading, so the caller can reject the whole operand
    /// rather than paste Display text into the name chain.
    fn push_chain_segments(segments: &mut Vec<IdsSegment>, n: &AstNode) -> bool {
        if let Some(mut ids) = McIds::new(n) {
            segments.append(&mut ids.segments);
            return true;
        }
        match n.get_type() {
            // A bare integer element (`mc_phrase MCPT_DOT mc_int` keeps the
            // int outside the ids node).
            MCAST_INT => match McInt::new(n) {
                Some(int) => {
                    segments.push(IdsSegment::Int(Box::new(int)));
                    true
                }
                None => false,
            },
            // `x.y` on a non-ids base: the dot's payload is an mc_ids or
            // mc_int node. An int tail keeps its DotInt shape so the render
            // keeps the dot (`TTL.7400`), same as the ids-internal reader.
            MCAST_OPD_DOT => {
                let Some(sub) = n.get_sub_node() else {
                    return false;
                };
                if sub.get_type() == MCAST_INT {
                    match McInt::new(&sub) {
                        Some(int) => {
                            segments.push(IdsSegment::DotInt(Box::new(int)));
                            true
                        }
                        None => false,
                    }
                } else if let Some(mut ids) = McIds::new(&sub) {
                    segments.append(&mut ids.segments);
                    true
                } else {
                    false
                }
            }
            // `x{a, b}` on a non-ids base: the same curly group the
            // ids-internal reader (McIds::parse_curly) would have built.
            MCAST_OPD_CURLY => match McIds::parse_curly(n) {
                Some(seg) => {
                    segments.push(seg);
                    true
                }
                None => false,
            },
            // An OPDS wrapper (the curly-MN side list, `mca.y` wraps each
            // `mc_opds` in one so the `|` boundary survives link3): unfold it
            // into the caller's target vec — the CURLY_MN arm passes the
            // member-group vec here, a plain chain element passes the top
            // segment list. Nested wrappers (the keyword arm's
            // `this{n|m}` double-wraps its side list) recurse the same way.
            MCAST_OPDS => {
                let mut inner = n.get_sub_node();
                let mut any = false;
                while let Some(m) = inner {
                    if !Self::push_chain_segments(segments, &m) {
                        return false;
                    }
                    any = true;
                    inner = m.get_next();
                }
                any
            }
            // `x{a|b}` in value face (U343 B1 arm 1). The MCAST_OPDS side
            // wrappers are the pipe boundary; a value operand reads both
            // sides as one member group — the same ',' / '|' equivalence
            // the ids-level reader applies. The base (any non-OPDS chain
            // element) keeps its place ahead of the group, matching the
            // source order the chain walk visits.
            MCAST_OPD_CURLY_MN => {
                let Some(sub) = n.get_sub_node() else {
                    return false;
                };
                let mut group: Vec<IdsSegment> = Vec::new();
                let mut cur = Some(sub);
                while let Some(el) = cur {
                    if el.get_type() == MCAST_OPDS {
                        if !Self::push_group_member(&mut group, &el) {
                            return false;
                        }
                    } else if !Self::push_chain_segments(segments, &el) {
                        return false;
                    }
                    cur = el.get_next();
                }
                if !group.is_empty() {
                    segments.push(IdsSegment::Curly(group));
                }
                true
            }
            _ => false,
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
