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
//! - [`interact`] —— ★ client-side JS (real expand/collapse/navigation)
//!
//! ### Compatibility
//! `legacy.rs::HtmlTemplate::wrap` is preserved; old callers using
//! `crate::viz::template::HtmlTemplate` continue to work (old path, fake expand).
//! New code should use [`wrap_document`].

pub mod interact;
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

// ============================================================================
// Multi-target combination
// ============================================================================

/// Stack several rendered SVGs vertically into one SVG, each labelled with its
/// target name. Used by the multi-module viz (peer modules) and by the
/// component/interface virtual-instantiation view (mcd docs-mc 16-export-viz
/// §6), where several targets from one file share a single HTML document.
pub fn combine_svgs(svgs: &[(String, String)]) -> String {
    let gap = 40.0; // vertical gap between targets
    let label_height = 20.0;
    let margin = 20.0;

    let mut items: Vec<(String, f64, f64, String)> = Vec::new(); // (name, w, h, inner)
    let mut max_w: f64 = 0.0;

    for (name, svg) in svgs {
        let vb = extract_viewbox(svg);
        let w = vb.0.max(1.0);
        let h = vb.1.max(1.0);
        max_w = max_w.max(w);
        let inner = extract_svg_inner(svg);
        items.push((name.clone(), w, h, inner));
    }

    let total_w = max_w + margin * 2.0;
    let mut total_h = margin;
    for (_, _, h, _) in &items {
        total_h += label_height + *h + gap;
    }
    total_h += margin;

    let mut out = format!(
        r#"<svg viewBox="0 0 {:.1} {:.1}" xmlns="http://www.w3.org/2000/svg"
     font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
     style="background:transparent">
"#,
        total_w, total_h
    );

    let mut y = margin;
    for (name, w, h, inner) in &items {
        out.push_str(&format!(
            r##"  <text x="{:.1}" y="{:.1}" font-size="16" font-weight="700" fill="#333">{}</text>
"##,
            margin,
            y + 16.0,
            escape_xml(name)
        ));
        y += label_height;

        let x_offset = (max_w - w) / 2.0 + margin;
        out.push_str(&format!(
            r##"  <g transform="translate({:.1},{:.1})">
{}
  </g>
"##,
            x_offset, y, inner
        ));
        y += h + gap;
    }

    out.push_str("</svg>\n");
    out
}

/// Extract (width, height) from an SVG viewBox attribute.
fn extract_viewbox(svg: &str) -> (f64, f64) {
    if let Some(start) = svg.find("viewBox=\"") {
        let rest = &svg[start + 9..];
        if let Some(end) = rest.find('"') {
            let vb = &rest[..end];
            let parts: Vec<&str> = vb.split_whitespace().collect();
            if parts.len() >= 4 {
                let w = parts[2].parse::<f64>().unwrap_or(200.0);
                let h = parts[3].parse::<f64>().unwrap_or(100.0);
                return (w, h);
            }
        }
    }
    (200.0, 100.0)
}

/// Extract the inner content of an SVG (everything between the opening
/// `<svg...>` and closing `</svg>`).
fn extract_svg_inner(svg: &str) -> String {
    if let Some(start) = svg.find("<svg") {
        if let Some(gt) = svg[start..].find('>') {
            let inner_start = start + gt + 1;
            if let Some(end) = svg.rfind("</svg>") {
                return svg[inner_start..end].trim().to_string();
            }
        }
    }
    svg.to_string()
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
