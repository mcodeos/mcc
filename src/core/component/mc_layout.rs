// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

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
        assert!(node.is_type(MCAST_ATTRIBUTE));

        let sub_node1 = node.get_sub_node().expect(MISSING_SUBNODE);
        if !sub_node1.is_type(MCAST_ATT_ID) {
            panic!("{}", TYPE_MISMATCH)
        }
        let sub_node2 = sub_node1.get_next().expect(MISSING_SUBNODE);
        if !sub_node2.is_type(MCAST_SET_ATTRIBUTES) {
            if sub_node2.is_type(MCAST_ATT_VALUES) {
                return None;
            } else {
                panic!("{}", TYPE_MISMATCH)
            }
        }
        let sub_node1_ids_node = sub_node1.get_sub_node().expect(MISSING_SUBNODE);

        let id = McIds::new(&sub_node1_ids_node)?;

        if id.to_string() == "layout" {
            let first_edge_node = sub_node2
                .get_sub_node()
                .expect("While building layout: Missing subnode for edge");

            let mut ret = Self {
                left: Vec::new(),
                right: Vec::new(),
                top: Vec::new(),
                bottom: Vec::new(),
            };

            for each_edge in first_edge_node.iter() {
                if !each_edge.is_type(MCAST_ATTRIBUTE) {
                    panic!("Type mismatch")
                }

                let name_node = each_edge.get_sub_node().expect("Missing subnode");
                let value_node = name_node
                    .get_next()
                    .expect("Missing subnode for layout value");

                if !value_node.is_type(MCAST_ATT_VALUES) {
                    panic!("Type mismatch")
                }

                let set_node = value_node.get_sub_node().expect("Missing subnode");
                if set_node.get_next().is_some() {
                    panic!("Malformed layout")
                }

                let first_value = set_node.get_sub_node().expect("Missing subnode");

                // first_value is CONST, its subnode is INT
                let all_values: Vec<u32> = first_value
                    .iter()
                    .map(|x: AstNode| {
                        x.get_sub_node()
                            .expect("CONST node missing subnode INT")
                            .to_u32()
                            .expect("Parse Error")
                    })
                    .collect();

                let name_id_node = name_node.get_sub_node().expect("Missing subnode");

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
                        panic!("Invalid edge. Edges should be one of: \"left\", \"right\", \"top\", \"bottom\"")
                    }
                } else {
                    panic!("Malformed layout")
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
