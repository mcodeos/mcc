// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#[derive(Debug)]
pub enum ParseError {
    MissingSubNode,
    InvalidSyntax(String),
    TypeMismatch,
    ParseIntError,
    ParseFloatError,
    BadEncoding,
    InvalidASTStructure,
    InvalidUnit,
    InvalidLogicLevel,
    // Other parse-related errors...
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSubNode => write!(f, "Missing required sub-node in AST"),
            Self::InvalidSyntax(msg) => write!(f, "Invalid syntax: {msg}"),
            Self::TypeMismatch => write!(f, "Type mismatch in AST node"),
            Self::ParseIntError => write!(f, "Failed to parse integer value"),
            Self::ParseFloatError => write!(f, "Failed to parse floating-point value"),
            Self::BadEncoding => write!(f, "Bad string encoding in AST data"),
            Self::InvalidASTStructure => write!(f, "Invalid AST structure"),
            Self::InvalidUnit => write!(f, "Invalid unit specification"),
            Self::InvalidLogicLevel => write!(f, "Invalid logic level specification"),
        }
    }
}

pub(crate) mod message {
    pub const MISSING_SUBNODE: &str = "AST: Missing subnode";
}
