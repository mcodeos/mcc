// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Meta declaration face (meta grammar batch 1a, arc step 2).
//!
//! A `meta` block is the library-side schema of one judged quantity — the
//! chart-of-accounts row every `Meta(...)` instantiation is written
//! against (meta-system-design §3; rulings 13-28). Batch 1a stores the
//! declaration verbatim: the column rows (`judge = leq`, `role = demand`,
//! `params = mode`, `binds = …`) land as raw word lists and nothing reads
//! them yet — the judging passes that consume them are G-series work
//! (param-accounting-design §4, R10's declared consumer).
//!
//! Deliberately NOT a `DefKind`: a meta is a schema row, not a class — it
//! never instantiates, never enters the name-resolution priority order, and
//! has no enumeration consumer today, so promoting it into the definition
//! space would trip the §2.5 member-boundary audit for zero readers. It
//! lives in the workspace `metas` table ([`crate::db::cmie::tables`]) keyed
//! by [`McSpaceName`], beside the bom sidecar precedent.

use crate::ast::{macros::*, node::AstNode};
use crate::{McIds, McURI};

/// One declaration row: the column name and the words written on it
/// (`params = mode` → column `params`, values `[mode]`).
#[derive(Debug)]
pub struct MetaRow {
    pub column: McIds,
    pub values: Vec<McIds>,
    /// Byte span [start, end) of the whole row within the source file.
    pub span: [u32; 2],
}

/// One `meta <name> { rows }` declaration.
#[derive(Debug)]
pub struct McMetaDef {
    pub name: McIds,
    /// Byte span of the declaration name (the gotodef jump target, the
    /// enum/recipe precedent).
    pub span: [u32; 2],
    pub uri: McURI,
    pub rows: Vec<MetaRow>,
}

impl McMetaDef {
    /// Read the word out of an ids wrapper node (`att_id`, an operand) —
    /// both wrap the real `MCAST_IDS` one level down, so drill once.
    fn ids_under(node: &AstNode) -> Option<McIds> {
        if node.is_type(MCAST_IDS) {
            return McIds::new(node);
        }
        let sub = node.get_sub_node()?;
        let ids = sub.iter().find(|x| x.is_type(MCAST_IDS))?;
        McIds::new(&ids)
    }

    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCAST_META
        // |- MCAST_NAME - MCAST_BODY
        //     |- MCAST_ATTRIBUTE (att_id = att_values) ...
        let subnodes = node.get_sub_node()?;

        //1. name — same read as component / recipe (mc_class_name = ids
        //   [+ .int dot])
        let name_node = subnodes.iter().find(|x| x.is_type(MCAST_NAME))?;
        let ids_node = name_node.get_sub_node()?;
        let name = McIds::new_with_dot(&ids_node)?;

        let start = ids_node.get_pos() as u32;
        let end = start.saturating_add(ids_node.get_len() as u32);

        //2. body rows — every clause that reads as `column = word...` lands
        //   verbatim; a clause of another shape has no business in a meta
        //   body and is skipped here (the grammar admits it only through the
        //   generic attribute row, so reaching this arm with one is a
        //   caller bug, not an authoring error to diagnose).
        let mut rows = Vec::new();
        if let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) {
            for clause in body.clause_list() {
                if !clause.is_type(MCAST_ATTRIBUTE) {
                    continue;
                }
                let att = match clause.get_sub_node() {
                    Some(s) => s,
                    None => continue,
                };
                let column = match Self::ids_under(&att) {
                    Some(c) => c,
                    None => continue,
                };
                let values_node = match att.get_next() {
                    Some(v) if v.is_type(MCAST_ATT_VALUES) => v,
                    _ => continue,
                };
                let values_sub = match values_node.get_sub_node() {
                    Some(s) => s,
                    None => continue,
                };
                let values: Vec<McIds> =
                    values_sub.iter().filter_map(|opd| Self::ids_under(&opd)).collect();
                let row_start = clause.get_pos() as u32;
                let row_end = row_start.saturating_add(clause.get_len() as u32);
                rows.push(MetaRow {
                    column,
                    values,
                    span: [row_start, row_end],
                });
            }
        }

        Some(Self {
            name,
            span: [start, end],
            uri: uri.clone(),
            rows,
        })
    }
}
