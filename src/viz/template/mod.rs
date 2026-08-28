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

/// Stack several rendered SVGs vertically into one SVG. Used by the multi-module
/// viz (peer modules) and by the component/interface virtual-instantiation view
/// (mcd docs-mc 16-export-viz §6), where several targets from one file share a
/// single HTML document.
///
/// Each entry is `(label, svg)`: `Some(name)` renders a bold heading above the
/// target (real user modules), `None` omits it (component/interface targets
/// wrapped in a synthetic module — their heading would duplicate the IC's own
/// class label and inflate the vertical gap). Every part is re-anchored at its
/// viewBox origin so the per-part leading blank is dropped and targets stack
/// with a uniform gap.
pub fn combine_svgs(svgs: &[(Option<String>, String)]) -> String {
    let gap = 40.0; // vertical gap between targets
    let label_height = 20.0;
    let margin = 20.0;

    struct Item {
        label: Option<String>,
        ox: f64,
        oy: f64,
        w: f64,
        h: f64,
        inner: String,
    }
    let mut items: Vec<Item> = Vec::new();
    let mut max_w: f64 = 0.0;

    for (label, svg) in svgs {
        let (ox, oy, w, h) = extract_viewbox(svg);
        max_w = max_w.max(w);
        let inner = extract_svg_inner(svg);
        items.push(Item {
            label: label.clone(),
            ox,
            oy,
            w,
            h,
            inner,
        });
    }

    let total_w = max_w + margin * 2.0;
    let mut total_h = margin;
    for it in &items {
        total_h += if it.label.is_some() {
            label_height + it.h + gap
        } else {
            it.h + gap
        };
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
    for it in &items {
        if let Some(label) = &it.label {
            out.push_str(&format!(
                r##"  <text x="{:.1}" y="{:.1}" font-size="16" font-weight="700" fill="#333">{}</text>
"##,
                margin,
                y + 16.0,
                escape_xml(label)
            ));
            y += label_height;
        }

        let x_offset = (max_w - it.w) / 2.0 + margin;
        // Outer translate positions the part frame; inner translate re-anchors
        // the content at its viewBox origin so stacked parts share one baseline.
        out.push_str(&format!(
            r##"  <g transform="translate({:.1},{:.1})">
  <g transform="translate({:.1},{:.1})">
{}
  </g>
  </g>
"##,
            x_offset, y, -it.ox, -it.oy, it.inner
        ));
        y += it.h + gap;
    }

    out.push_str("</svg>\n");
    out
}

/// Extract (origin-x, origin-y, width, height) from an SVG viewBox attribute.
fn extract_viewbox(svg: &str) -> (f64, f64, f64, f64) {
    if let Some(start) = svg.find("viewBox=\"") {
        let rest = &svg[start + 9..];
        if let Some(end) = rest.find('"') {
            let vb = &rest[..end];
            let parts: Vec<&str> = vb.split_whitespace().collect();
            if parts.len() >= 4 {
                let ox = parts[0].parse::<f64>().unwrap_or(0.0);
                let oy = parts[1].parse::<f64>().unwrap_or(0.0);
                let w = parts[2].parse::<f64>().unwrap_or(200.0);
                let h = parts[3].parse::<f64>().unwrap_or(100.0);
                return (ox, oy, w, h);
            }
        }
    }
    (0.0, 0.0, 200.0, 100.0)
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
