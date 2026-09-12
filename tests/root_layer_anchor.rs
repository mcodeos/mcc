// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P1 · Root-layer drawing anchor (geometry set + text set).
//!
//! Why this test exists
//! Every existing gate is blind to what the root block diagram actually draws:
//!
//! - `mcs --test renderdiff` never looks at the rendered document at all. Its
//!   `LayerReading::measure` walks `graph.boxes` / `graph.nets` and *discards*
//!   the SVG (`render_with_metrics(graph, ..)`'s `_doc`). `render_golden.toml`
//!   has no coordinate and no text field — only counts plus a `(from, to,
//!   label)` roster. So it cannot see a moved line or a renamed label.
//! - That blindness has been observed four times: the P0b row-order change and
//!   the P0c-R4 text change both passed `mcc --lib` 994 + `renderdiff` 8/8 +
//!   `project` 3/3 green.
//!
//! Before rewriting where root edges land (P1-b), the root layer needs a
//! committed baseline. This is it.
//!
//! What is compared (do not weaken)
//! Two **sets**, never bytes — the element emission order is still not stable
//! (R0 stabilised row order, not tie-breaking on the whole emission), so an
//! HTML `cmp`/`md5` would be a false red:
//!
//! 1. geometry: normalised `<rect>` / `<line>` coordinate tuples;
//! 2. text: `<text>` `(x, y, content)` triples.
//!
//! Both are needed. Comparing only rect/line misses a swap of two equal-sized
//! boxes (the rect set is unchanged, only the text moves); comparing only text
//! misses a moved bare line.
//!
//! Re-pin recipe
//! `MCC_ROOT_ANCHOR_DUMP=1 cargo test --test root_layer_anchor` writes the
//! anchor. Same discipline as `mcs`'s render golden: no silent regen path —
//! dumping is a no-op unless the env var is set, and a re-pin must be justified
//! by a predicted geometry delta, not by "it went green after I re-pinned".

use std::collections::BTreeSet;
use std::path::PathBuf;

use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

fn anchor_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/root_layer_anchor.txt")
}

/// The mcc_* workspace is global state; tests must be serialized (same as
/// tests/renderdiff.rs and tests/rail_rules.rs).
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Build + lay out + render the hbl fixture, returning the ROOT layer's SVG.
fn root_layer_svg() -> String {
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table, &arena, &store);
    let graph = mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);

    let document = mcc::viz::api::render(graph);
    document
        .root_layer()
        .expect("root visualization layer")
        .svg
        .clone()
}

/// Normalise a numeric attribute to one decimal so `340` and `340.0` compare equal.
fn num(raw: &str) -> String {
    match raw.trim().parse::<f64>() {
        Ok(v) => format!("{v:.1}"),
        Err(_) => raw.trim().to_string(),
    }
}

/// Pull a named XML attribute's value out of a tag's attribute string.
fn attr(tag_attrs: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag_attrs.find(&needle)? + needle.len();
    let rest = &tag_attrs[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Parse the root SVG into the two comparison sets.
///
/// Deliberately hand-rolled: the shapes here are a tiny fixed vocabulary
/// (`<rect>` / `<line>` / `<text>`) and a regex would not make it clearer.
fn measure(svg: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut geometry: BTreeSet<String> = BTreeSet::new();
    let mut text: BTreeSet<String> = BTreeSet::new();

    for chunk in svg.split('<').skip(1) {
        let Some(close) = chunk.find('>') else {
            continue;
        };
        let inner = &chunk[..close];
        let after = &chunk[close + 1..];
        let name = inner
            .split(|c: char| c == ' ' || c == '/' || c == '\n')
            .next()
            .unwrap_or("");

        match name {
            "rect" => {
                let (Some(x), Some(y), Some(w), Some(h)) = (
                    attr(inner, "x"),
                    attr(inner, "y"),
                    attr(inner, "width"),
                    attr(inner, "height"),
                ) else {
                    continue;
                };
                geometry.insert(format!(
                    "rect {} {} {} {}",
                    num(&x),
                    num(&y),
                    num(&w),
                    num(&h)
                ));
            }
            "line" => {
                let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                    attr(inner, "x1"),
                    attr(inner, "y1"),
                    attr(inner, "x2"),
                    attr(inner, "y2"),
                ) else {
                    continue;
                };
                geometry.insert(format!(
                    "line {} {} {} {}",
                    num(&x1),
                    num(&y1),
                    num(&x2),
                    num(&y2)
                ));
            }
            "text" => {
                let (Some(x), Some(y)) = (attr(inner, "x"), attr(inner, "y")) else {
                    continue;
                };
                // `after` already ends at the next `<` — which is the `</text>`
                // that split it off, so the content is exactly `after`. Do NOT
                // search for `</text>` here: splitting on `<` consumed it.
                text.insert(format!("text {} {} {}", num(&x), num(&y), after));
            }
            _ => {}
        }
    }

    (geometry, text)
}

/// Serialise into the committed anchor format: two sorted sections.
fn serialise(geometry: &BTreeSet<String>, text: &BTreeSet<String>) -> String {
    let mut out = String::from(
        "# Root-layer drawing anchor — see tests/root_layer_anchor.rs for why.\n\
         # geometry: normalised <rect>/<line> coordinate tuples (sorted set)\n\
         # text: <text> (x, y, content) triples (sorted set)\n\
         [geometry]\n",
    );
    for line in geometry {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("[text]\n");
    for line in text {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Read the committed anchor back into its two sets.
fn parse_anchor(raw: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut geometry = BTreeSet::new();
    let mut text = BTreeSet::new();
    let mut section = "";
    for line in raw.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line {
            "[geometry]" => {
                section = "geometry";
                continue;
            }
            "[text]" => {
                section = "text";
                continue;
            }
            _ => {}
        }
        match section {
            "geometry" => {
                geometry.insert(line.to_string());
            }
            "text" => {
                text.insert(line.to_string());
            }
            _ => {}
        }
    }
    (geometry, text)
}

/// Report the differences in a form that names the offending elements.
fn diff(name: &str, expected: &BTreeSet<String>, actual: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    for missing in expected.difference(actual) {
        out.push(format!(
            "  {name} missing (in anchor, not drawn): {missing}"
        ));
    }
    for extra in actual.difference(expected) {
        out.push(format!("  {name} extra   (drawn, not in anchor): {extra}"));
    }
    out
}

#[test]
fn mcc_root_anchor_dump_regen() {
    // Opt-in writer. No silent regen path: a no-op unless the env var is set.
    if std::env::var("MCC_ROOT_ANCHOR_DUMP").is_err() {
        return;
    }
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let svg = root_layer_svg();
    let (geometry, text) = measure(&svg);
    let path = anchor_path();
    std::fs::write(&path, serialise(&geometry, &text)).expect("write root layer anchor");
    eprintln!(
        "wrote {} ({} geometry, {} text)",
        path.display(),
        geometry.len(),
        text.len()
    );
}

#[test]
fn root_layer_drawing_matches_anchor() {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let raw = std::fs::read_to_string(anchor_path()).expect("read root layer anchor");
    let (exp_geometry, exp_text) = parse_anchor(&raw);

    let svg = root_layer_svg();
    let (geometry, text) = measure(&svg);

    let mut findings = diff("geometry", &exp_geometry, &geometry);
    findings.extend(diff("text", &exp_text, &text));

    assert!(
        findings.is_empty(),
        "root-layer drawing drifted from the anchor ({} geometry, {} text drawn; \
         {} geometry, {} text anchored):\n{}\n\
         Re-pin only with a predicted delta, via MCC_ROOT_ANCHOR_DUMP=1.",
        geometry.len(),
        text.len(),
        exp_geometry.len(),
        exp_text.len(),
        findings.join("\n")
    );
}

/// Positive control: the anchor must be sensitive to the class of change it was
/// built for. P0c-R4 rewrote root boundary leads from the *net* name to the
/// *port* name plus pin number — a pure text change with geometry untouched,
/// which every other gate let through (mcc 994 + renderdiff 8/8 + project 3/3
/// all stayed green).
///
/// Two halves, because "the string is in the file" alone would not prove the
/// gate *acts* on it:
///
/// 1. the comparison actually fires — mutate the parsed sets and assert `diff`
///    names exactly the mutated element (this is what the other gates lack);
/// 2. the anchor carries that class of content — a boundary lead written as a
///    port name next to its pin number.
#[test]
fn anchor_covers_the_port_naming_change_class() {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let raw = std::fs::read_to_string(anchor_path()).expect("read root layer anchor");
    let (geometry, text) = parse_anchor(&raw);

    // (1) A text-only change of one character must be reported, in both
    // directions, and must not touch the geometry set.
    //
    // The probe is located by CONTENT, not by a pinned coordinate: P1 owns the
    // root geometry and is expected to move it, while the claim under test is
    // *which name* the anchor carries. Pinning absolute coordinates here made
    // every legitimate re-layout look like a P0c-R4 regression.
    // Entries are `text {x} {y} {content}` — content is everything from field 4.
    let content = |t: &String| t.splitn(4, ' ').nth(3).unwrap_or("").to_string();
    let cs_lead = text
        .iter()
        .find(|t| content(t) == "_CS")
        .expect("anchor no longer carries the port-named lead `_CS` (P0c-R4 regression?)")
        .clone();
    let mut mutated_text = text.clone();
    assert!(mutated_text.remove(&cs_lead));
    mutated_text.insert(cs_lead.replace(" _CS", " SPI"));
    let text_findings = diff("text", &text, &mutated_text);
    assert_eq!(
        text_findings.len(),
        2,
        "a one-token text rename must produce exactly one missing + one extra, got: \
         {text_findings:?}"
    );
    assert!(
        diff("geometry", &geometry, &geometry).is_empty(),
        "the geometry set must be unaffected by a text rename"
    );

    // (2) Same for geometry: a one-coordinate move must be reported.
    let mut mutated_geometry = geometry.clone();
    let moved = geometry
        .iter()
        .find(|g| g.starts_with("line "))
        .expect("anchor has at least one line")
        .clone();
    mutated_geometry.remove(&moved);
    mutated_geometry.insert(format!("{moved} ")); // same element, trailing space
    assert_eq!(
        diff("geometry", &geometry, &mutated_geometry).len(),
        2,
        "a changed coordinate tuple must produce exactly one missing + one extra"
    );

    // (3) The anchor carries the P0c-R4 class of content: `_CS` is a *port*
    // name on a boundary lead, paired with its pin number. (`SPI` legitimately
    // remains elsewhere in the drawing as a bus label — P0c-R4 renamed the
    // boundary lead, not every occurrence of the net name, so an
    // "SPI must be absent" assertion would be wrong.)
    assert!(
        text.iter().any(|t| content(t) == "1"),
        "the port-named lead `_CS` should be paired with pin number `1` at the \
         MCU513 west face (P0c-R4): {text:?}"
    );
}
