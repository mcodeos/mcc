// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::core::component::McComponent;
use crate::core::mc_enum::McEnumDef;
use crate::core::mc_ifs::McInterface;
use crate::core::module::McModule;
use crate::{
    McIds, MCAST_IOTYPE, MCAST_IOTYPE_ANL, MCAST_IOTYPE_IN, MCAST_IOTYPE_IO, MCAST_IOTYPE_NC,
    MCAST_IOTYPE_OUT, MCAST_IOTYPE_PS, MCAST_IOTYPE_RETURN,
};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum IOType {
    In,
    Out,
    InOut,
    Power,
    Analog,
    Return,
    NonCon,
    None,
}

impl IOType {
    pub(crate) fn new(node: &AstNode) -> Option<IOType> {
        if node.get_type() != MCAST_IOTYPE {
            return None;
        }
        if let Some(subnode) = node.get_sub_node() {
            match subnode.get_type() {
                MCAST_IOTYPE_IN => return Some(IOType::In),
                MCAST_IOTYPE_OUT => return Some(IOType::Out),
                MCAST_IOTYPE_IO => return Some(IOType::InOut),
                MCAST_IOTYPE_PS => return Some(IOType::Power),
                MCAST_IOTYPE_ANL => return Some(IOType::Analog),
                MCAST_IOTYPE_RETURN => return Some(IOType::Return),
                MCAST_IOTYPE_NC => return Some(IOType::NonCon),
                _ => return Some(IOType::None),
            }
        }
        None
    }
}

pub enum McCMIE {
    Component(Arc<McComponent>),
    Module(Arc<McModule>),
    Interface(Arc<McInterface>),
    Enum(Arc<McEnumDef>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct McSpaceName {
    pub ident: McIds, //comp/mod/ifs/enum
    pub uri: McURI,   //dir.file
}

impl McSpaceName {
    pub(crate) fn new(ident: &McIds, uri: McURI) -> Self {
        Self {
            ident: ident.clone(),
            uri,
        }
    }
}

impl std::fmt::Display for McSpaceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ident_str = self.ident.to_string();
        // Pad ident to min 25 chars for alignment
        let padded = format!("{ident_str:<25}");
        write!(f, "{} @{}", padded, self.uri)
    }
}

pub type McURI = String;

#[allow(dead_code)]
pub fn print_backtrace(label: &str) {
    eprintln!("\n=== BACKTRACE: {label} ===");
    let bt = std::backtrace::Backtrace::capture();
    eprintln!("{bt}");
}
