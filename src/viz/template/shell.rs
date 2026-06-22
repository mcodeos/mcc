// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! HTML skeleton (DOCTYPE / html / head / body container)
//!
//! ## ★ Fix: HTML-safe JSON embedding
//!
//! Embedding JSON directly as `{doc_json}` into a `<script>` is unsafe:
//! if the JSON contains `</` (e.g. SVG's `</g>` `</text>` `</svg>`),
//! the browser HTML parser does not care about quotes / escapes; it scans for `</script>`
//! and immediately closes the `<script>` tag — even if your string actually contains `"</svg>"`.
//! The result is that the `const DOC = ...` line is incomplete, DOC becomes undefined or a
//! partial object, and the frontend reports "Error: root layer #X not found".
//!
//! Fix: before embedding, replace `</` with `<\/`. In a JS string, `<\/` is equivalent to `</`
//! (the backslash before a character that doesn't need escaping is ignored), but the HTML
//! parser does not trigger tag closure.
//!
//! This is the standard way of embedding JSON in `<script>` (similar to PHP's json_encode JSON_HEX_TAG).

/// Wrapper: assemble all fragments into a complete HTML
///
/// # Parameters
/// - `title`     page title
/// - `css`       inline CSS
/// - `doc_json`  output of `VizDocument::to_json()`
/// - `js`        interactive JS
pub fn wrap(title: &str, css: &str, doc_json: &str, js: &str) -> String {
    // ★ Key fix: HTML-safe embedding
    let safe_json = make_html_safe(doc_json);

    format!(
        r##"<!DOCTYPE html>
<html lang="zh">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title} — McVec Circuit Viewer</title>
  <style>
{css}
  </style>
</head>
<body>
  <div id="breadcrumb"></div>
  <div id="main-container">
    <div id="canvas"></div>
  </div>
  <div id="stats"></div>
  <script>
    const DOC = {safe_json};
  </script>
  <script>
{js}
  </script>
</body>
</html>"##,
    )
}

/// Transform a JSON string into a form that can be safely embedded in `<script>`
///
/// Replaces three categories of dangerous sequences:
/// - `</`     → `<\/`  (avoid the HTML parser misinterpreting it as a tag close)
/// - `<!--`   → `<\!--` (avoid the start of an HTML comment)
/// - `-->`    → `--\>` (avoid the end of an HTML comment)
fn make_html_safe(json: &str) -> String {
    json.replace("</", "<\\/")
        .replace("<!--", "<\\!--")
        .replace("-->", "--\\>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_safe_replaces_close_tag() {
        let dangerous = r#"{"svg":"<g><text>R1</text></g></svg>"}"#;
        let safe = make_html_safe(dangerous);
        assert!(!safe.contains("</"));
        assert!(safe.contains("<\\/g>"));
        assert!(safe.contains("<\\/svg>"));
    }

    #[test]
    fn test_html_safe_preserves_open_tags() {
        let s = r#"{"svg":"<svg><g>"}"#;
        let safe = make_html_safe(s);
        assert!(safe.contains("<svg>"));
        assert!(safe.contains("<g>"));
    }
}
