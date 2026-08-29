// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Database-backed implementations of the semantic context traits.
//!
//! Provides concrete implementations of [`crate::semantic::context`] traits
//! backed by the global workspace / system tables in `db/`.

use crate::semantic::context::NameResolver;
use crate::{McCMIE, McIds, McURI};

// ============================================================================
// DbContext — NameResolver over the global workspace / system tables
// ============================================================================

pub struct DbContext;

impl NameResolver for DbContext {
    fn resolve(&self, class_name: &McIds, from_uri: &McURI) -> Option<(McCMIE, McURI)> {
        crate::db::cmie::cmie::mcb_get_cmie_with_uri(class_name, from_uri)
    }

    fn resolve_system(&self, class_name: &McIds) -> Option<McCMIE> {
        crate::db::resolve::Resolver::resolve_system(class_name)
    }
}

// ============================================================================
// Singleton
// ============================================================================

/// The global database context — used when no trait injection is needed.
pub static DB: DbContext = DbContext;
