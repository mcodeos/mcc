// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::ast::c_macros::*;
use crate::semantic::basic::mc_uval::McUnitValue;
use std::fmt;

/// Basic data types
#[derive(Debug, Clone)]
pub enum McType {
    Int,
    Hex,
    Float,
    String,
}

impl std::fmt::Display for McType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McType::Int => write!(f, "INT"),
            McType::Hex => write!(f, "HEX"),
            McType::Float => write!(f, "FLOAT"),
            McType::String => write!(f, "STRING"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct McInt {
    pub value: i64,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct McHex {
    pub value: i64,
    pub hex_str: String,
}

#[derive(Clone, PartialEq)]
pub struct McFloat {
    pub value: f64,
}

impl McInt {
    pub fn new(node: &AstNode) -> Option<Self> {
        match node.get_type() {
            MCAST_INT => {
                let str_value = unsafe {
                    std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char)
                        .to_str()
                        .expect("Bad encoding")
                };

                match str_value.parse::<i64>() {
                    Ok(value) => Some(Self { value }),
                    Err(_) => None,
                }
            }
            MCAST_UNIT_INT => {
                let str_value = unsafe {
                    std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char)
                        .to_str()
                        .expect("Bad encoding")
                };

                // Remove unit part
                let num_str = str_value.split_whitespace().next().unwrap_or(str_value);
                match num_str.parse::<i64>() {
                    Ok(value) => Some(Self { value }),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    }
}

impl std::fmt::Display for McLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McLiteral::Int(i) => write!(f, "{i}"),
            McLiteral::Float(fl) => write!(f, "{fl}"),
            McLiteral::String(s) => write!(f, "{s}"),
            McLiteral::Const(c) => write!(f, "{c}"),
            McLiteral::Uval(u) => write!(f, "{u}"),
        }
    }
}

impl McHex {
    pub fn new(node: &AstNode) -> Option<Self> {
        if node.get_type() != MCAST_HEX {
            return None;
        }
        let str_value = unsafe {
            std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char)
                .to_str()
                .expect("Bad encoding")
        };

        let hex_str = str_value.trim_start_matches("0x").trim_start_matches("0X");
        match i64::from_str_radix(hex_str, 16) {
            Ok(value) => Some(Self {
                value,
                hex_str: str_value.to_string(),
            }),
            Err(_) => None,
        }
    }
}

impl fmt::Display for McHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.hex_str)
    }
}

impl fmt::Debug for McHex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.hex_str)
    }
}

impl fmt::Display for McInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl fmt::Debug for McInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<&str> for McInt {
    fn from(value: &str) -> Self {
        let value = value.parse::<i64>().expect("Bad encoding");
        Self { value }
    }
}

impl McInt {
    /// Convert to owned string form.
    /// Note: `McInt` already implements `Display`, so `mc_int.to_string()` also works
    /// via the standard `ToString` trait. This inherent method just makes it explicit.
    pub fn to_string(&self) -> String {
        self.value.to_string()
    }
}

impl McFloat {
    pub fn new(node: &AstNode) -> Option<Self> {
        match node.get_type() {
            MCAST_FLOAT => {
                let str_value = unsafe {
                    std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char)
                        .to_str()
                        .expect("Bad encoding")
                };

                match str_value.parse::<f64>() {
                    Ok(value) => Some(Self { value }),
                    Err(_) => None,
                }
            }
            _ => None,
        }
    }
}

impl fmt::Display for McFloat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl fmt::Debug for McFloat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<&str> for McFloat {
    fn from(value: &str) -> Self {
        let value = value.parse::<f64>().expect("Bad encoding");
        Self { value }
    }
}

#[derive(Debug, Clone)]
pub struct McString {
    pub value: String,
}

impl McString {
    pub fn new(node: &AstNode) -> Option<Self> {
        match node.get_type() {
            MCAST_STRING => unsafe {
                let c_str = std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char);
                if let Ok(str_value) = c_str.to_str() {
                    // Strip surrounding quotes if present
                    let value = if str_value.starts_with('"')
                        && str_value.ends_with('"')
                        && str_value.len() >= 2
                    {
                        str_value[1..str_value.len() - 1].to_string()
                    } else {
                        str_value.to_string()
                    };
                    Some(Self { value })
                } else {
                    None
                }
            },
            _ => None,
        }
    }
}

impl std::fmt::Display for McString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self.value)
    }
}

impl From<&str> for McString {
    fn from(value: &str) -> Self {
        Self {
            value: value.to_string(),
        }
    }
}

#[derive(Clone)]
pub enum McConst {
    // Only override MCK_CONST keyword constant
    Keyword(String),
}
impl McConst {
    pub fn new(node: &AstNode) -> Option<Self> {
        // Only handle MCK_CONST keyword constant
        if node.is_type(MCAST_CONST) {
            unsafe {
                let c_str = std::ffi::CStr::from_ptr(node.get_data() as *const std::ffi::c_char);
                if let Ok(str_value) = c_str.to_str() {
                    Some(McConst::Keyword(str_value.to_string()))
                } else {
                    None
                }
            }
        } else {
            None
        }
    }
}

impl std::fmt::Display for McConst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McConst::Keyword(keyword) => write!(f, "{keyword}"),
        }
    }
}

impl std::fmt::Debug for McConst {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McConst::Keyword(keyword) => write!(f, "McConst::Keyword({keyword:?})"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum McLiteral {
    Int(McInt),
    Float(McFloat),
    String(McString),
    Const(McConst),
    Uval(McUnitValue),
}

impl McLiteral {
    pub fn new(node: &AstNode) -> Option<Self> {
        match node.get_type() {
            MCAST_INT => McInt::new(node).map(McLiteral::Int),
            MCAST_FLOAT => McFloat::new(node).map(McLiteral::Float),
            MCAST_HEX => McInt::new(node).map(McLiteral::Int),
            MCAST_STRING => McString::new(node).map(McLiteral::String),
            MCAST_CONST => McConst::new(node).map(McLiteral::Const),
            MCAST_UVALUE => McUnitValue::new(node).map(McLiteral::Uval),
            _ => None,
        }
    }
}
