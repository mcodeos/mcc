// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::AstNode;
use crate::core::component::McComponent;
use crate::core::mc_enum::McEnumDef;
use crate::core::mc_ifs::McInterface;
use crate::core::module::McModule;
use crate::{
    McIds, MCAST_IOTYPE, MCAST_IOTYPE_ANL, MCAST_IOTYPE_IN, MCAST_IOTYPE_IO, MCAST_IOTYPE_LABEL,
    MCAST_IOTYPE_NC, MCAST_IOTYPE_OUT, MCAST_IOTYPE_PS, MCAST_IOTYPE_RETURN,
};
use std::ops::Range;
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
    Label,
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
                MCAST_IOTYPE_LABEL => return Some(IOType::Label),
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

// ============================================================================
// ScopePath: hierarchical container chain for def/ref positioning
// ============================================================================

/// Kind of a container in the scope hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerKind {
    /// Inside a function body
    Function,
    /// Inside a component definition
    Component,
    /// Inside a module definition
    Module,
    /// Inside an interface definition
    Interface,
    /// Inside an enum definition
    Enum,
    /// At file level (no parent container)
    File,
}

impl ContainerKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "func",
            Self::Component => "component",
            Self::Module => "module",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::File => "file",
        }
    }
}

/// A single container in the scope chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerInfo {
    pub kind: ContainerKind,
    pub name: String,
}

impl ContainerInfo {
    pub fn new(kind: ContainerKind, name: &str) -> Self {
        Self {
            kind,
            name: name.to_string(),
        }
    }
}

/// Full hierarchical position of a def or ref.
///
/// Encodes the chain from inner to outer:
///   func → container (component/module/interface/enum) → file → project → libs
///
/// Used for priority-based lookup: when resolving a reference, search from
/// the innermost scope outward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopePath {
    /// Source file URI
    pub uri: McURI,
    /// Innermost function name (if inside a function body)
    pub func: Option<String>,
    /// Direct parent container
    pub container: ContainerInfo,
    /// Container chain from inner to outer (includes file-level)
    pub container_chain: Vec<ContainerInfo>,
}

impl ScopePath {
    /// Create a file-level ScopePath (no container).
    pub fn file_level(uri: &McURI) -> Self {
        Self {
            uri: uri.clone(),
            func: None,
            container: ContainerInfo::new(ContainerKind::File, ""),
            container_chain: vec![],
        }
    }

    /// Create a module-level ScopePath.
    pub fn module(uri: &McURI, mod_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: None,
            container: ContainerInfo::new(ContainerKind::Module, mod_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Create a component-level ScopePath.
    pub fn component(uri: &McURI, comp_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: None,
            container: ContainerInfo::new(ContainerKind::Component, comp_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Create a function-level ScopePath inside a module.
    pub fn func_in_module(uri: &McURI, mod_name: &str, func_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: Some(func_name.to_string()),
            container: ContainerInfo::new(ContainerKind::Module, mod_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Create a function-level ScopePath inside a component.
    pub fn func_in_component(uri: &McURI, comp_name: &str, func_name: &str) -> Self {
        Self {
            uri: uri.clone(),
            func: Some(func_name.to_string()),
            container: ContainerInfo::new(ContainerKind::Component, comp_name),
            container_chain: vec![ContainerInfo::new(ContainerKind::File, "")],
        }
    }

    /// Build the scope string for name_to_declare_id key.
    /// Format: `"Container.func"` or `"Container"` if no func.
    pub fn scope_key(&self) -> String {
        match &self.func {
            Some(f) => format!("{}.{}", self.container.name, f),
            None => self.container.name.clone(),
        }
    }

    /// Priority level for lookup (higher = more inner = checked first).
    ///   func=5  component/module=4  file=3  project=2  libs=1
    pub fn priority(&self) -> u8 {
        if self.func.is_some() {
            5
        } else {
            match self.container.kind {
                ContainerKind::Component
                | ContainerKind::Module
                | ContainerKind::Interface
                | ContainerKind::Enum => 4,
                ContainerKind::File => 3,
                ContainerKind::Function => 5,
            }
        }
    }
}

impl Default for ScopePath {
    fn default() -> Self {
        Self {
            uri: McURI::new(),
            func: None,
            container: ContainerInfo::new(ContainerKind::File, ""),
            container_chain: vec![],
        }
    }
}

// ============================================================================
// Unified Lookup types (shared by pass1/pass2, F12, Hover, Completion)
// ============================================================================

/// Result of a single symbol lookup.
#[derive(Debug, Clone)]
pub struct LookupResult {
    /// URI of the file where the definition was found.
    pub uri: McURI,
    /// Byte range of the definition in the source file.
    pub span: Range<usize>,
    /// Symbol kind for IDE features.
    pub kind: LookupSymbolKind,
    /// The container that owns this definition.
    pub container: Option<ContainerInfo>,
    /// Scope path string (e.g. "US513.i2c").
    pub scope: String,
    /// The definition name.
    pub name: String,
}

/// Symbol kind for unified lookup results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupSymbolKind {
    Component,
    Module,
    Interface,
    Enum,
    EnumValue,
    Function,
    Port,
    Label,
    Param,
    Pin,
    Instance,
    Define,
    Role,
    Unknown,
}

impl LookupSymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Component => "component",
            Self::Module => "module",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::EnumValue => "enum_value",
            Self::Function => "function",
            Self::Port => "port",
            Self::Label => "label",
            Self::Param => "param",
            Self::Pin => "pin",
            Self::Instance => "instance",
            Self::Define => "define",
            Self::Role => "role",
            Self::Unknown => "unknown",
        }
    }
}

/// Filter for `unified_lookup_all()`.
#[derive(Debug, Clone, Default)]
pub struct ScopeFilter {
    /// Only include results from this kind.
    pub kind: Option<ContainerKind>,
    /// Only include results whose name starts with this prefix.
    pub prefix: Option<String>,
    /// Max results to return.
    pub limit: Option<usize>,
}

impl ScopeFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_kind(mut self, kind: ContainerKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

#[allow(dead_code)]
pub fn print_backtrace(label: &str) {
    eprintln!("\n=== BACKTRACE: {label} ===");
    let bt = std::backtrace::Backtrace::capture();
    eprintln!("{bt}");
}
