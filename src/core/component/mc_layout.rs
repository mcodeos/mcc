// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    ast::{ast_node::AstNode, c_macros::*, error::message::*},
    builder::diagnostic::dlog_warning,
    McIds,
};

#[derive(Debug, Clone)]
pub struct McLayout {
    pub left: Vec<u32>,
    pub right: Vec<u32>,
    pub top: Vec<u32>,
    pub bottom: Vec<u32>,
}

impl McLayout {
    pub(super) fn new(node: &AstNode) -> Option<Self> {
        if !node.is_type(MCAST_ATTRIBUTE) {
            return None;
        }

        let sub_node1 = match node.get_sub_node() {
            Some(n) => n,
            None => {
                dlog_warning(2001, node, MISSING_SUBNODE);
                return None;
            }
        };
        if !sub_node1.is_type(MCAST_ATT_ID) {
            dlog_warning(2002, node, TYPE_MISMATCH);
            return None;
        }
        let sub_node2 = match sub_node1.get_next() {
            Some(n) => n,
            None => {
                dlog_warning(2003, node, MISSING_SUBNODE);
                return None;
            }
        };
        if !sub_node2.is_type(MCAST_SET_ATTRIBUTES) {
            if sub_node2.is_type(MCAST_ATT_VALUES) {
                return None;
            } else {
                dlog_warning(2004, node, TYPE_MISMATCH);
                return None;
            }
        }
        let sub_node1_ids_node = match sub_node1.get_sub_node() {
            Some(n) => n,
            None => {
                dlog_warning(2005, node, MISSING_SUBNODE);
                return None;
            }
        };

        let id = McIds::new(&sub_node1_ids_node)?;

        if id.to_string() == "layout" {
            let first_edge_node = match sub_node2.get_sub_node() {
                Some(n) => n,
                None => {
                    dlog_warning(2006, node, "While building layout: Missing subnode for edge");
                    return None;
                }
            };

            let mut ret = Self {
                left: Vec::new(),
                right: Vec::new(),
                top: Vec::new(),
                bottom: Vec::new(),
            };

            for each_edge in first_edge_node.iter() {
                if !each_edge.is_type(MCAST_ATTRIBUTE) {
                    dlog_warning(2007, node, "Type mismatch in layout edge");
                    return None;
                }

                let name_node = match each_edge.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(2008, node, "Missing subnode for layout edge name");
                        return None;
                    }
                };
                let value_node = match name_node.get_next() {
                    Some(n) => n,
                    None => {
                        dlog_warning(2009, node, "Missing subnode for layout value");
                        return None;
                    }
                };

                if !value_node.is_type(MCAST_ATT_VALUES) {
                    dlog_warning(2010, node, "Type mismatch in layout value");
                    return None;
                }

                let set_node = match value_node.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(2011, node, "Missing subnode for layout set");
                        return None;
                    }
                };
                if set_node.get_next().is_some() {
                    dlog_warning(2012, node, "Malformed layout: unexpected extra nodes");
                    return None;
                }

                let first_value = match set_node.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(2013, node, "Missing subnode for layout values");
                        return None;
                    }
                };

                let mut all_values: Vec<u32> = Vec::new();
                for x in first_value.iter() {
                    let int_node = match x.get_sub_node() {
                        Some(n) => n,
                        None => {
                            dlog_warning(2014, node, "CONST node missing subnode INT");
                            return None;
                        }
                    };
                    let val = match int_node.to_u32() {
                        Some(v) => v,
                        None => {
                            dlog_warning(2015, node, "Parse error in layout pin number");
                            return None;
                        }
                    };
                    all_values.push(val);
                }

                let name_id_node = match name_node.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(2016, node, "Missing subnode for layout edge name id");
                        return None;
                    }
                };

                if name_id_node.is_type(MCAST_ID) {
                    let name = name_id_node.to_id_or_ida().remove(0);
                    if name == "left" {
                        ret.left = all_values;
                    } else if name == "right" {
                        ret.right = all_values;
                    } else if name == "top" {
                        ret.top = all_values;
                    } else if name == "bottom" {
                        ret.bottom = all_values;
                    } else {
                        dlog_warning(2017, node, "Invalid edge. Edges should be one of: \"left\", \"right\", \"top\", \"bottom\"");
                        return None;
                    }
                } else {
                    dlog_warning(2018, node, "Malformed layout: edge name not an ID");
                    return None;
                }
            }

            Some(ret)
        } else {
            None
        }
    }

    pub(super) fn empty() -> Self {
        Self {
            left: Vec::new(),
            right: Vec::new(),
            top: Vec::new(),
            bottom: Vec::new(),
        }
    }
}
