// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::db::diagnostic::diagnostic::dlog_warning;
use crate::{
    ast::{ast_node::AstNode, c_macros::*, error::message::*},
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
                dlog_warning(
                    crate::errcodes::LAYOUT_MISSING_SUBNODE,
                    node,
                    MISSING_SUBNODE,
                );
                return None;
            }
        };
        if !sub_node1.is_type(MCAST_ATT_ID) {
            dlog_warning(crate::errcodes::LAYOUT_TYPE_MISMATCH, node, TYPE_MISMATCH);
            return None;
        }
        let sub_node2 = match sub_node1.get_next() {
            Some(n) => n,
            None => {
                dlog_warning(
                    crate::errcodes::LAYOUT_SET_MISSING_SUBNODE,
                    node,
                    MISSING_SUBNODE,
                );
                return None;
            }
        };
        if !sub_node2.is_type(MCAST_SET_ATTRIBUTES) {
            if sub_node2.is_type(MCAST_ATT_VALUES) {
                return None;
            } else {
                dlog_warning(
                    crate::errcodes::LAYOUT_VALUES_TYPE_MISMATCH,
                    node,
                    TYPE_MISMATCH,
                );
                return None;
            }
        }
        let sub_node1_ids_node = match sub_node1.get_sub_node() {
            Some(n) => n,
            None => {
                dlog_warning(
                    crate::errcodes::LAYOUT_NAME_MISSING_SUBNODE,
                    node,
                    MISSING_SUBNODE,
                );
                return None;
            }
        };

        let id = McIds::new(&sub_node1_ids_node)?;

        if id.to_string() == "layout" {
            let first_edge_node = match sub_node2.get_sub_node() {
                Some(n) => n,
                None => {
                    dlog_warning(
                        crate::errcodes::LAYOUT_EDGE_MISSING_SUBNODE,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::LAYOUT_EDGE_MISSING_SUBNODE,
                            &[],
                        ),
                    );
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
                    dlog_warning(
                        crate::errcodes::LAYOUT_EDGE_TYPE_MISMATCH,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::LAYOUT_EDGE_TYPE_MISMATCH,
                            &[],
                        ),
                    );
                    return None;
                }

                let name_node = match each_edge.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(
                            crate::errcodes::LAYOUT_EDGE_NAME_MISSING_SUBNODE,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::LAYOUT_EDGE_NAME_MISSING_SUBNODE,
                                &[],
                            ),
                        );
                        return None;
                    }
                };
                let value_node = match name_node.get_next() {
                    Some(n) => n,
                    None => {
                        dlog_warning(
                            crate::errcodes::LAYOUT_VALUE_MISSING_SUBNODE,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::LAYOUT_VALUE_MISSING_SUBNODE,
                                &[],
                            ),
                        );
                        return None;
                    }
                };

                if !value_node.is_type(MCAST_ATT_VALUES) {
                    dlog_warning(
                        crate::errcodes::LAYOUT_VALUE_TYPE_MISMATCH,
                        node,
                        &crate::errcodes::format_msg(
                            crate::errcodes::LAYOUT_VALUE_TYPE_MISMATCH,
                            &[],
                        ),
                    );
                    return None;
                }

                let set_node = match value_node.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(
                            crate::errcodes::LAYOUT_SET_SUBNODE_MISSING,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::LAYOUT_SET_SUBNODE_MISSING,
                                &[],
                            ),
                        );
                        return None;
                    }
                };
                if set_node.get_next().is_some() {
                    dlog_warning(
                        crate::errcodes::LAYOUT_EXTRA_NODES,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::LAYOUT_EXTRA_NODES, &[]),
                    );
                    return None;
                }

                let first_value = match set_node.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(
                            crate::errcodes::LAYOUT_VALUES_MISSING_SUBNODE,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::LAYOUT_VALUES_MISSING_SUBNODE,
                                &[],
                            ),
                        );
                        return None;
                    }
                };

                let mut all_values: Vec<u32> = Vec::new();
                for x in first_value.iter() {
                    let int_node = match x.get_sub_node() {
                        Some(n) => n,
                        None => {
                            dlog_warning(
                                crate::errcodes::LAYOUT_CONST_MISSING_INT,
                                node,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::LAYOUT_CONST_MISSING_INT,
                                    &[],
                                ),
                            );
                            return None;
                        }
                    };
                    let val = match int_node.to_u32() {
                        Some(v) => v,
                        None => {
                            dlog_warning(
                                crate::errcodes::LAYOUT_PIN_NUMBER_PARSE,
                                node,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::LAYOUT_PIN_NUMBER_PARSE,
                                    &[],
                                ),
                            );
                            return None;
                        }
                    };
                    all_values.push(val);
                }

                let name_id_node = match name_node.get_sub_node() {
                    Some(n) => n,
                    None => {
                        dlog_warning(
                            crate::errcodes::LAYOUT_EDGE_NAME_ID_MISSING_SUBNODE,
                            node,
                            &crate::errcodes::format_msg(
                                crate::errcodes::LAYOUT_EDGE_NAME_ID_MISSING_SUBNODE,
                                &[],
                            ),
                        );
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
                        dlog_warning(
                            crate::errcodes::LAYOUT_EDGE_INVALID,
                            node,
                            &crate::errcodes::format_msg(crate::errcodes::LAYOUT_EDGE_INVALID, &[]),
                        );
                        return None;
                    }
                } else {
                    dlog_warning(
                        crate::errcodes::LAYOUT_EDGE_NAME_NOT_ID,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::LAYOUT_EDGE_NAME_NOT_ID, &[]),
                    );
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
