// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `vscode://` source links for the **standalone** artifact (design §3.4 / D3)
//!
//! The webview path needs none of this. A page inside the editor has a host to
//! post a byte offset to, and the host resolves it against the file it already
//! has open. A `circuit.html` opened in a browser has no host — its fallback was
//! to show and copy `uri:offset`, which no editor accepts as input, so a click
//! there led nowhere.
//!
//! The complete form of that fallback is VS Code's own URL handler,
//! `vscode://file/<abs path>:<line>:<col>`, which the browser hands to the OS.
//! Line/column are the one thing the page cannot work out for itself — deriving
//! them needs the file's *content*, and the renderer deliberately stamps byte
//! offsets only (design §3.2: one source of truth, no second line table). They
//! are therefore resolved here, at the **write site**, where the sources are
//! ordinary files on disk: this pass reads each referenced file once, converts
//! its offsets, and adds a `data-src-vscode` attribute beside the existing
//! `data-src-uri` / `data-src-offset` pair.
//!
//! Two properties this pass deliberately keeps:
//!
//! - **It does not touch the renderer.** It runs on the finished
//!   [`VizDocument`], in the standalone writer only. The render golden is
//!   recorded by an independent render (`build.rs`'s `render_signature` →
//!   `VizDocument::to_json()`) that never passes through here, and the webview
//!   path wraps its own document without stamping, so *those* two carry no
//!   link. The standalone artifact's own embedded `DOC` does — that is the
//!   whole point — while the golden stays byte-identical.
//! - **It fails closed.** A URI that cannot be read, or an offset past the end
//!   of its file, simply gets no attribute; the page then keeps its copy-the-
//!   coordinate fallback rather than linking somewhere wrong.
//!
//! The link is machine-local by construction — an absolute path on this
//! machine, pointing at a file that a recipient of the HTML may not have. That
//! is inherent to the `vscode://file` form, not a property of this pass: there
//! is no relative variant of the handler. It is also why the link is only ever
//! followed on an explicit modifier-click (see the page's source navigation),
//! never as a side effect of opening the file.

use std::collections::HashMap;
use std::path::Path;

use super::doc::VizDocument;

/// One read file per distinct URI, so a schematic with hundreds of stamped
/// elements still reads each source once. `None` caches "unreadable", which is
/// the common case for a stale or relative URI.
type FileCache = HashMap<String, Option<Vec<u8>>>;

/// Stamp the links and wrap the page — the standalone artifact's single entry
/// point.
///
/// Every writer of a `circuit.html` goes through this rather than calling
/// [`super::template::wrap_document`] itself, so the links cannot be left out of
/// one write site by accident. The webview path is the opposite case and keeps
/// calling `wrap_document` directly: it has a host to post a byte offset to and
/// no use for a URI the browser would have to hand to the OS.
pub fn wrap_standalone(doc: &mut VizDocument, project_root: &Path) -> String {
    stamp_vscode_links(doc, project_root);
    super::template::wrap_document(doc)
}

/// Stamp the source links onto an already-wrapped HTML document.
///
/// The delegated build face writes the server's `build.viz` output to disk,
/// and that output comes out through [`super::template::wrap_document`] (the
/// webview wrapper) because the document itself lives on the server. This is
/// the same contract [`wrap_standalone`] enforces for locally rendered
/// documents — a `circuit.html` on disk must carry the `data-src-vscode`
/// links — applied to the wrapped string instead of a held document.
pub fn stamp_wrapped(html: &str, project_root: &Path) -> String {
    linkify(html, project_root, &mut FileCache::new())
}

/// Add a `data-src-vscode` link beside every `data-src-uri`/`data-src-offset`
/// pair in the document's layers.
///
/// `project_root` resolves URIs that arrive relative (a `use`d file recorded
/// before canonicalization); absolute URIs are used as they are.
pub fn stamp_vscode_links(doc: &mut VizDocument, project_root: &Path) {
    let mut cache = FileCache::new();
    // Layer order has to be stable or the output HTML differs run to run, which
    // the build's determinism guard would (correctly) flag.
    let mut bids: Vec<i64> = doc.layers.keys().copied().collect();
    bids.sort();
    for bid in bids {
        if let Some(layer) = doc.layers.get_mut(&bid) {
            layer.svg = linkify(&layer.svg, project_root, &mut cache);
        }
    }
}

/// The two attributes the renderer writes adjacent, and the shape this pass
/// relies on: `data-src-uri="…" data-src-offset="…"`.
const URI_ATTR: &str = "data-src-uri=\"";
const OFF_ATTR: &str = "\" data-src-offset=\"";

fn linkify(svg: &str, root: &Path, cache: &mut FileCache) -> String {
    let mut out = String::with_capacity(svg.len() + svg.len() / 64);
    let mut rest = svg;
    while let Some(i) = rest.find(URI_ATTR) {
        let val = i + URI_ATTR.len();
        out.push_str(&rest[..val]);
        rest = &rest[val..];

        // The renderer escapes the URI, so the value holds no literal quote.
        let Some(close) = rest.find('"') else {
            out.push_str(rest);
            return out;
        };
        let uri = &rest[..close];
        out.push_str(uri);
        rest = &rest[close..];

        // Anything but the expected adjacent offset attribute is left exactly as
        // it was — a shape this pass does not understand is not a shape it edits.
        if !rest.starts_with(OFF_ATTR) {
            out.push('"');
            rest = &rest[1..];
            continue;
        }
        rest = &rest[OFF_ATTR.len()..];
        let Some(close_off) = rest.find('"') else {
            out.push_str(rest);
            return out;
        };
        let off_text = &rest[..close_off];
        out.push_str(&format!("\" data-src-offset=\"{off_text}\""));
        rest = &rest[close_off + 1..];

        if let Ok(offset) = off_text.parse::<usize>() {
            if let Some(link) = vscode_link(uri, offset, root, cache) {
                out.push_str(&format!(" data-src-vscode=\"{}\"", escape_attr(&link)));
            }
        }
    }
    out.push_str(rest);
    out
}

/// `vscode://file/<abs path>:<line>:<col>`, or `None` when the coordinate cannot
/// be resolved on disk.
fn vscode_link(uri: &str, offset: usize, root: &Path, cache: &mut FileCache) -> Option<String> {
    let path = unescape_attr(uri);
    let bytes = cache.entry(path.clone()).or_insert_with(|| {
        let p = Path::new(&path);
        let p = if p.is_absolute() {
            p.to_path_buf()
        } else {
            root.join(p)
        };
        std::fs::read(p).ok()
    });
    let bytes = bytes.as_deref()?;
    let content = std::str::from_utf8(bytes).ok()?;
    // A URI that resolved relative still has to name a real file for the link to
    // mean anything; `is_absolute` after resolution is the cheapest such check.
    let resolved = Path::new(&path);
    let absolute = if resolved.is_absolute() {
        resolved.to_path_buf()
    } else {
        root.join(resolved)
    };
    if !absolute.is_absolute() {
        return None;
    }
    let abs = absolute.to_string_lossy();

    let at = offset.min(content.len());
    let line = crate::hierarchy::line_of_byte(content, at);
    // VS Code counts columns like its own documents: 1-based, in UTF-16 code
    // units. A byte count would land short on any line holding a non-ASCII
    // comment before the token — the same trap the editor-side bridge documents
    // (design §3.2), reached here from the other end.
    let line_start = content[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = content[line_start..at].encode_utf16().count() + 1;

    Some(format!(
        "vscode://file{}:{}:{}",
        percent_encode_path(&abs),
        line,
        col
    ))
}

/// Percent-encode everything outside the path-safe set. `:` is encoded too —
/// VS Code reads the trailing `:<line>:<col>` literally, so a colon inside a
/// file name would otherwise be taken as the coordinate separator.
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Inverse of the renderer's attribute escaping (which escapes `&`, `<`, `>`
/// and `"`). Without this a path like `/p/R&D/x.mc` would be looked up with a
/// literal `&amp;` in it.
fn unescape_attr(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> std::path::PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "mcc-sourcelink-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The conversion this pass exists for: a byte offset becomes a 1-based
    /// line and a UTF-16 column, so the link lands where the editor's own
    /// Go-to-line would. The non-ASCII line is the point — `RX` sits at UTF-16
    /// column 11 there but at byte column 19, so a byte count would drop the
    /// cursor eight characters past the token.
    ///
    /// Those four glyphs are spelled as escapes rather than written out: this
    /// repo's source is English-only, and the fixture has to be non-ASCII for
    /// the test to mean anything.
    #[test]
    fn link_carries_line_and_utf16_column() {
        let dir = tmpdir();
        let path = dir.join("m.mc");
        std::fs::write(
            &path,
            "module m()\n{\n  // \u{4e2d}\u{6587}\u{6ce8}\u{91ca} RX\n}\n",
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let offset = content.find("RX").unwrap();

        let mut cache = FileCache::new();
        let link = vscode_link(&path.to_string_lossy(), offset, &dir, &mut cache).unwrap();
        assert!(link.ends_with(":3:11"), "wrong line/column: {link}");
        assert!(link.starts_with("vscode://file/"), "{link}");
    }

    /// The link is stamped beside the coordinate the webview path still uses,
    /// and only there — the host path must keep reading a bare byte offset.
    #[test]
    fn stamping_adds_the_link_and_leaves_the_offset_alone() {
        let dir = tmpdir();
        let path = dir.join("m.mc");
        std::fs::write(&path, "a\nb\n").unwrap();
        let svg = format!(
            r##"  <g data-src-uri="{}" data-src-offset="2"><rect/></g>"##,
            path.to_string_lossy()
        );
        let out = linkify(&svg, &dir, &mut FileCache::new());
        assert!(out.contains(&format!(r##"data-src-offset="2""##)), "{out}");
        assert!(
            out.contains(r##"data-src-vscode="vscode://file/"##),
            "{out}"
        );
        assert!(out.contains(":2:1"), "{out}");
    }

    /// An unreadable URI gets no link at all: the page then keeps its
    /// copy-the-coordinate fallback instead of offering a jump that cannot land.
    #[test]
    fn unreadable_uri_gets_no_link() {
        let dir = tmpdir();
        let svg = r##"  <g data-src-uri="/no/such/file.mc" data-src-offset="1"><rect/></g>"##;
        let out = linkify(svg, &dir, &mut FileCache::new());
        assert!(!out.contains("data-src-vscode"), "{out}");
        assert!(
            out.contains(r##"data-src-uri="/no/such/file.mc""##),
            "{out}"
        );
        assert!(out.contains(r##"data-src-offset="1""##), "{out}");
    }

    /// An offset past the end of its file clamps rather than panicking — a
    /// source edited since the schematic was rendered still yields a usable
    /// jump, landing at end of file — and a relative URI resolves against the
    /// project root. That URI is the `use`d-file case: recorded before
    /// canonicalization, so it names its file only relative to the project.
    #[test]
    fn offset_past_eof_clamps_and_relative_uris_resolve() {
        let dir = tmpdir();
        std::fs::write(dir.join("rel.mc"), "one\ntwo\n").unwrap();
        let svg = r##"  <g data-src-uri="rel.mc" data-src-offset="9999"><rect/></g>"##;
        let out = linkify(svg, &dir, &mut FileCache::new());
        assert!(
            out.contains("data-src-vscode"),
            "relative uri should resolve: {out}"
        );
        assert!(
            out.contains("/rel.mc:3:1\""),
            "should clamp to end of file: {out}"
        );
    }

    /// Text without the attribute pair is passed through byte for byte.
    #[test]
    fn unrelated_svg_is_untouched() {
        let svg = r##"<rect x="1" data-pin-id="7"/><text>VCC</text>"##;
        assert_eq!(linkify(svg, Path::new("/"), &mut FileCache::new()), svg);
    }

    /// The delegated-build face receives a fully wrapped HTML document from
    /// the server and must still leave the vscode links on disk — the same
    /// contract `wrap_standalone` enforces, just over a string the CLI does
    /// not hold as a document. A pair outside any layer must stamp too, and
    /// template text without the pair must come through untouched.
    #[test]
    fn stamp_wrapped_stamps_a_wrapped_document() {
        let dir = tmpdir();
        let path = dir.join("m.mc");
        std::fs::write(&path, "a\nb\n").unwrap();
        let html = format!(
            concat!(
                r#"<html><body><script>const DOC = 1;</script>"#,
                r##"<g data-src-uri="{}" data-src-offset="2"><rect/></g>"##,
                r#"<footer>circuit.html</footer></body></html>"#
            ),
            path.to_string_lossy()
        );
        let out = stamp_wrapped(&html, &dir);
        assert!(
            out.contains(r##"data-src-vscode="vscode://file/"##),
            "{out}"
        );
        assert!(out.contains(":2:1"), "{out}");
        assert!(out.contains("<footer>circuit.html</footer>"), "{out}");
    }

    /// A wrapped document whose layers carry no coordinate pair (no render
    /// attributes at all) passes through unchanged — the same pass-through
    /// rule `linkify` applies, so an attribute-free template is not edited.
    #[test]
    fn stamp_wrapped_without_pairs_is_untouched() {
        let html = r#"<html><body><span class="hint">open dev tools</span></body></html>"#;
        assert_eq!(stamp_wrapped(html, Path::new("/")), html);
    }
}
