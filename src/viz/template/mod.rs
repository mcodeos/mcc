// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! HTML wrapper
//!
//! ## Architecture after P2 completes
//!
//! ### New pipeline (replacing legacy::HtmlTemplate::wrap)
//! ```text
//!   VizDocument
//!       │
//!       ▼
//!  wrap_document(doc) ──→ HTML
//!         │
//!         ├── shell::wrap()          skeleton
//!         ├── theme::css()           styles
//!         ├── interact::js()         ★ real, working expand JS
//!         └── doc.to_json()          ★ all layers' SVG stuffed into JSON at once
//! ```
//!
//! ### Sub-modules
//! - [`shell`]    —— HTML skeleton
//! - [`theme`]    —— CSS (light / dark auto-adapt)
//! - [`nav`]      —— server-side breadcrumb (backup, mainly relies on JS)
//! - [`interact`] —— ★ client-side JS (real expand/collapse/navigation)
//!
//! ### Compatibility
//! `legacy.rs::HtmlTemplate::wrap` is preserved; old callers using
//! `crate::viz::template::HtmlTemplate` continue to work (old path, fake expand).
//! New code should use [`wrap_document`].

pub mod interact;
pub mod nav;
pub mod shell;
pub mod theme;

use super::doc::VizDocument;

// ============================================================================
// New top-level API: wrap_document
// ============================================================================

/// Wrap [`VizDocument`] into a complete HTML
///
/// This is the core P2 entry point, replacing the old `HtmlTemplate::wrap`.
///
/// # Example
/// ```ignore
/// let doc = viz::api::render(graph);
/// let html = viz::template::wrap_document(&doc);
/// std::fs::write("circuit.html", &html)?;
/// ```
pub fn wrap_document(doc: &VizDocument) -> String {
    let css = theme::css();
    let js = interact::js();
    let doc_json = doc.to_json();
    shell::wrap(&doc.root_name, css, &doc_json, js)
}
