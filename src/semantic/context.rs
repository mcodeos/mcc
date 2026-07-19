// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Semantic analysis context traits — Phase 7.
//!
//! These traits decouple `semantic/` from the global state in `db/`,
//! enabling unit testing without workspace singletons.
//!
//! ## Traits
//!
//! - [`NameResolver`] — resolve class names to CMIE definitions (replaces `mcb_get_cmie`)
//! - [`SymbolRegistry`] — register/lookup LSP symbols (replaces `mcb_register_*`)
//! - [`DiagnosticSink`] — report diagnostics (replaces direct `diagnostic_log` calls)
//!
//! ## Usage
//!
//! ```ignore
//! fn parse_component(ctx: &impl NameResolver, name: &McIds, uri: &McURI) -> Option<McComponent> {
//!     ctx.resolve(name, uri).and_then(|cmie| cmie.into_component())
//! }
//! ```

use crate::{McCMIE, McIds, McURI};

// ============================================================================
// NameResolver — resolve class names to definitions
// ============================================================================

/// Resolves class names (components, modules, interfaces, enums) to their
/// CMIE definitions. Abstracts over the double-layer (workspace → global)
/// lookup in `db/cmie/cmie.rs`.
pub trait NameResolver {
    /// Resolve a class name in the context of a source URI.
    /// Returns the CMIE and the URI where it was defined.
    fn resolve(&self, class_name: &McIds, from_uri: &McURI) -> Option<(McCMIE, McURI)>;

    /// Look up a class name in system tables only (no workspace layer).
    fn resolve_system(&self, class_name: &McIds) -> Option<McCMIE>;
}

// ============================================================================
// SymbolRegistry — LSP symbol registration and lookup
// ============================================================================

/// Registers and looks up LSP symbols (instance declarations, class references).
/// Abstracts over the global instance/class tables in `db/cmie/tables.rs`.
pub trait SymbolRegistry {
    /// Register an instance declaration and return its ID.
    fn register_instance_decl(
        &self,
        uri: &str,
        scope: Option<&str>,
        name: &str,
        pos: u32,
        len: u32,
    ) -> u32;

    /// Register a reference to a previously declared instance.
    fn register_instance_ref(
        &self,
        uri: &str,
        decl_id: u32,
        scope: Option<&str>,
        pos: u32,
        len: u32,
    );

    /// Look up an instance declaration by URI, scope, and name.
    fn lookup_instance_decl(&self, uri: &str, name: &str, scope: Option<&str>) -> Option<u32>;

    /// Register a class definition at a span.
    fn register_declare_class(&self, uri: &str, class_name: &str, pos: u32, len: u32);

    /// Find all references to a named symbol across all files.
    fn find_refs(&self, name: &str) -> Vec<(String, String, (u32, u32))>;
}

// ============================================================================
// DiagnosticSink — diagnostic reporting
// ============================================================================

/// Sink for diagnostic messages (errors, warnings, hints).
/// Abstracts over `db/diagnostic/diagnostic.rs`.
pub trait DiagnosticSink {
    /// Report a diagnostic at a source location.
    fn report(
        &self,
        code: u32,
        severity: DiagnosticSeverity,
        uri: &str,
        pos: u32,
        len: u32,
        message: &str,
        suggestions: &[String],
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Hint,
    Info,
    Warning,
    Error,
}

// ============================================================================
// Composite context — bundles all three traits
// ============================================================================

/// Convenience trait bundling all semantic context traits.
/// Most `semantic/` functions should accept `&impl SemanticContext` or
/// `&dyn SemanticContext`.
pub trait SemanticContext: NameResolver + SymbolRegistry + DiagnosticSink {}

// Blanket impl: any type implementing all three gets SemanticContext for free.
impl<T: NameResolver + SymbolRegistry + DiagnosticSink> SemanticContext for T {}
