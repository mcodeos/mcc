// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Breadcrumb / navigation (server-side render version, backup)
//!
//! The main breadcrumb is generated dynamically on the client by the JS in
//! [`super::interact`], because the breadcrumb needs to change in real time with navStack.
//!
//! This file provides a "server-side pre-rendered" version for JS-less environments
//! (printing / SEO / static-site snapshots). P2's `wrap_document` does not use it;
//! kept for P3 future extension.

use super::super::doc::VizDocument;

/// Render the breadcrumb HTML for the specified layer (server-side version)
///
/// Output looks like `<a href="...">main</a> ▸ <a href="...">mcu</a> ▸ core`
pub fn breadcrumb_html(doc: &VizDocument, current_bid: i64) -> String {
    let path = doc.path_to(current_bid);
    if path.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let last = path.len() - 1;
    for (i, &bid) in path.iter().enumerate() {
        let name = doc.layers.get(&bid).map(|l| l.name.as_str()).unwrap_or("?");
        let escaped = html_escape(name);
        if i == last {
            out.push_str("<span class=\"current\">");
            out.push_str(&escaped);
            out.push_str("</span>");
        } else {
            out.push_str(&format!(
                "<a href=\"#layer{bid}\" data-bid=\"{bid}\">{escaped}</a>"
            ));
            out.push_str("<span class=\"sep\"> ▸ </span>");
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
