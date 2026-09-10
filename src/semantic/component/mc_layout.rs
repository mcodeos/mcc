// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_warning;
use crate::{
    ast::{macros::*, node::AstNode},
    McIds,
};

/// Semantic form of a `layout = [ ... ]` attribute.
///
/// Each edge lists the pins (by number) or function names (by description) that
/// must be placed on that side of the box, in **counterclockwise package
/// order**: `left` top→bottom, `bottom` left→right, `right` bottom→top (so the
/// list reads up from the bottom), `top` right→left (so the list reads leftward
/// from the right). Members are kept as source strings — numbers stay numeric
/// text, names keep their exact spelling — because both drawing paths match
/// against `pin_id` *or* the pin description with plain string equality.
///
/// A `bottom = [6:9]` range expands to its individual members here (ascending
/// or descending, preserving the author's direction) so the drawer never has to
/// interpret a colon itself.
#[derive(Debug, Clone, Default)]
pub struct McLayout {
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub top: Vec<String>,
    pub bottom: Vec<String>,
}

impl McLayout {
    pub(crate) fn new(node: &AstNode) -> Option<Self> {
        if !node.is_type(MCAST_ATTRIBUTE) {
            return None;
        }
        // node: MCAST_ATTRIBUTE( MCAST_ATT_ID( ids ), MCAST_ATT_VALUES( ... ) )
        let att_id = match node.get_sub_node() {
            Some(n) if n.is_type(MCAST_ATT_ID) => n,
            Some(_) | None => {
                warn(node, crate::errcodes::LAYOUT_TYPE_MISMATCH);
                return None;
            }
        };
        let id = match attr_id(&att_id) {
            Some(id) => id,
            None => {
                warn(node, crate::errcodes::LAYOUT_NAME_MISSING_SUBNODE);
                return None;
            }
        };
        if id != "layout" {
            return None;
        }

        let values_node = match att_id.get_next() {
            Some(n) if n.is_type(MCAST_ATT_VALUES) => n,
            Some(_) | None => {
                warn(node, crate::errcodes::LAYOUT_SET_MISSING_SUBNODE);
                return None;
            }
        };

        let mut ret = Self::empty();
        // Each mc_attr_value under ATT_VALUES; the one that matters is the
        // bracketed set `[ edge = ... ... ]` (MCAST_SET_ATTRIBUTES).
        let Some(first_value) = values_node.get_sub_node() else {
            warn(node, crate::errcodes::LAYOUT_VALUE_MISSING_SUBNODE);
            return Some(ret);
        };
        for value in first_value.iter() {
            if value.is_type(MCAST_SET_ATTRIBUTES) {
                // SET children (linked) are the per-edge attributes.
                let Some(edges) = value.get_sub_node() else {
                    warn(node, crate::errcodes::LAYOUT_EDGE_MISSING_SUBNODE);
                    continue;
                };
                for edge in edges.iter() {
                    parse_edge(&edge, &mut ret);
                }
            } else {
                warn(node, crate::errcodes::LAYOUT_VALUE_TYPE_MISMATCH);
            }
        }
        Some(ret)
    }

    pub(super) fn empty() -> Self {
        Self {
            left: Vec::new(),
            right: Vec::new(),
            top: Vec::new(),
            bottom: Vec::new(),
        }
    }

    /// True when no edge carries any member (drawers treat this as "no hint").
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
            && self.right.is_empty()
            && self.top.is_empty()
            && self.bottom.is_empty()
    }
}

/// Read the attribute's name (`layout`, or an edge name like `left`) from its
/// `MCAST_ATT_ID` child.
fn attr_id(att_id: &AstNode) -> Option<String> {
    let ids = att_id.get_sub_node()?;
    McIds::new(&ids).map(|m| m.to_string())
}

/// Edge alias map: `up`/`down` are synonyms for `top`/`bottom` (legacy mcpub
/// content, e.g. tc275 uses them).
fn canonical_edge(name: &str) -> Option<&'static str> {
    match name {
        "left" => Some("left"),
        "right" => Some("right"),
        "top" | "up" => Some("top"),
        "bottom" | "down" => Some("bottom"),
        _ => None,
    }
}

fn edge_slot<'a>(layout: &'a mut McLayout, name: &str) -> Option<&'a mut Vec<String>> {
    match name {
        "left" => Some(&mut layout.left),
        "right" => Some(&mut layout.right),
        "top" => Some(&mut layout.top),
        "bottom" => Some(&mut layout.bottom),
        _ => None,
    }
}

/// Parse one edge attribute (`left = [1:3, 4]`) into the matching edge list.
fn parse_edge(edge: &AstNode, ret: &mut McLayout) {
    if !edge.is_type(MCAST_ATTRIBUTE) {
        warn(edge, crate::errcodes::LAYOUT_EDGE_TYPE_MISMATCH);
        return;
    }
    let att_id = match edge.get_sub_node() {
        Some(n) if n.is_type(MCAST_ATT_ID) => n,
        _ => {
            warn(edge, crate::errcodes::LAYOUT_EDGE_NAME_MISSING_SUBNODE);
            return;
        }
    };
    let name = match attr_id(&att_id) {
        Some(n) => n,
        None => {
            warn(edge, crate::errcodes::LAYOUT_EDGE_NAME_ID_MISSING_SUBNODE);
            return;
        }
    };
    let canonical = match canonical_edge(&name) {
        Some(c) => c,
        None => {
            warn(edge, crate::errcodes::LAYOUT_EDGE_INVALID);
            return;
        }
    };

    // edge value: MCAST_ATT_VALUES wrapping the bracketed member list.
    let values_node = match att_id.get_next() {
        Some(n) if n.is_type(MCAST_ATT_VALUES) => n,
        _ => {
            warn(edge, crate::errcodes::LAYOUT_VALUE_MISSING_SUBNODE);
            return;
        }
    };
    let Some(first_value) = values_node.get_sub_node() else {
        warn(edge, crate::errcodes::LAYOUT_VALUES_MISSING_SUBNODE);
        return;
    };
    let mut members: Vec<String> = Vec::new();
    for value in first_value.iter() {
        expand_members(&value, edge, &mut members);
    }
    if let Some(slot) = edge_slot(ret, canonical) {
        slot.extend(members);
    }
}

/// Flatten one `mc_attr_value` node into member strings.
///
/// Accepts the shapes the grammar actually produces for a layout member list:
///
/// - `[ 4, 3, 2 ]`  → `MCAST_EXPRESSION(MCAST_OPD_SQUARE_VEC(int…) )`
/// - `[ 6:9 ]`      → square vec holding `MCAST_OPD_COLON(int 6, int 9)`
/// - `[ DP, DN ]`   → square vec holding `MCAST_OPD(ids)` per name
/// - bare values    → an `MCAST_INT` / `MCAST_OPD` directly
///
/// Anything else is reported and skipped; a malformed edge never aborts the
/// rest of the layout.
fn expand_members(node: &AstNode, edge: &AstNode, out: &mut Vec<String>) {
    if node.is_null() {
        return;
    }
    match node.get_type() {
        MCAST_EXPRESSION | MCAST_OPD_SQUARE_VEC | MCAST_PARAMS | MCAST_OPDS => {
            if let Some(sub) = node.get_sub_node() {
                for child in sub.iter() {
                    expand_members(&child, edge, out);
                }
            }
        }
        MCAST_OPD_COLON => {
            // `a:b` / `b:a` — expand inclusively, preserving the author's
            // direction (ascending or descending).
            let (Some(left), Some(right)) = (
                node.get_sub_node(),
                node.get_sub_node().and_then(|l| l.get_next()),
            ) else {
                warn(edge, crate::errcodes::LAYOUT_CONST_MISSING_INT);
                return;
            };
            match expand_numeric_range(&left, &right) {
                Some(range) => out.extend(range),
                None => {
                    warn(edge, crate::errcodes::LAYOUT_PIN_NUMBER_PARSE);
                }
            }
        }
        MCAST_INT | MCAST_STRING | MCAST_FLOAT | MCAST_HEX | MCAST_CONST | MCAST_UVALUE => {
            if let Some(s) = node.to_string() {
                out.push(s);
            }
        }
        MCAST_OPD | MCAST_ID | MCAST_IDA | MCAST_IDS => {
            for name in node.to_id_or_ida_or_num() {
                out.push(name);
            }
        }
        _ => {
            // Tolerate unknown member forms (nested expressions etc.) by
            // descending; only if nothing textual comes back do we warn.
            if let Some(sub) = node.get_sub_node() {
                for child in sub.iter() {
                    expand_members(&child, edge, out);
                }
            }
            warn(edge, crate::errcodes::LAYOUT_CONST_MISSING_INT);
        }
    }
}

/// Both colon operands numeric → inclusive ordered list (respects direction);
/// otherwise `None` (caller warns).
fn expand_numeric_range(left: &AstNode, right: &AstNode) -> Option<Vec<String>> {
    let lo = parse_i64(left)?;
    let hi = parse_i64(right)?;
    let mut v: Vec<String> = Vec::new();
    if lo <= hi {
        for x in lo..=hi {
            v.push(x.to_string());
        }
    } else {
        for x in (hi..=lo).rev() {
            v.push(x.to_string());
        }
    }
    Some(v)
}

fn parse_i64(node: &AstNode) -> Option<i64> {
    node.to_u32()
        .map(|v| v as i64)
        .or_else(|| node.to_string()?.trim().parse::<i64>().ok())
}

fn warn(node: &AstNode, code: u32) {
    dlog_warning(code, node, &crate::errcodes::format_msg(code, &[]));
}
