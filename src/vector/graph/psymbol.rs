// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Project-local SVG symbol loading.
//!
//! Symbols are declared in `<project>/symbols/manifest.toml`. Files are resolved below that
//! directory, parsed as a deliberately small SVG subset, and stored as validated fragments.
//! Rendering never reads from disk and never receives active content such as scripts, styles,
//! external references, event handlers, or XML entities.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use serde::Deserialize;

use super::boxdef::{CustomSymbol, SvgViewBox};

const SYMBOL_MANIFEST: &str = "symbols/manifest.toml";
const MAX_MANIFEST_BYTES: u64 = 128 * 1024;
const MAX_SVG_BYTES: u64 = 256 * 1024;
const MAX_SVG_ELEMENTS: usize = 256;

#[derive(Debug, Default)]
struct ProjectSymbols {
    root: Option<PathBuf>,
    symbols: BTreeMap<String, CustomSymbol>,
}

static PROJECT_SYMBOLS: LazyLock<RwLock<ProjectSymbols>> =
    LazyLock::new(|| RwLock::new(ProjectSymbols::default()));

#[derive(Debug, Default)]
pub(crate) struct SymbolLoadReport {
    pub manifest_found: bool,
    pub loaded: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolManifest {
    schema_version: u32,
    #[serde(default)]
    symbols: Vec<SymbolManifestEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolManifestEntry {
    class: String,
    file: String,
}

#[derive(Debug)]
struct ParsedTag {
    name: String,
    attrs: BTreeMap<String, String>,
    closing: bool,
    self_closing: bool,
}

/// Load and replace the active project's symbol registry.
pub(crate) fn load_project_symbols(project_root: &Path) -> SymbolLoadReport {
    if project_root.as_os_str().is_empty() {
        clear_project_symbols();
        return SymbolLoadReport::default();
    }

    let canonical_root = match fs::canonicalize(project_root) {
        Ok(root) => root,
        Err(error) => {
            let warning = format!(
                "cannot resolve project root '{}': {error}",
                project_root.display()
            );
            replace_registry(None, BTreeMap::new());
            return SymbolLoadReport {
                warnings: vec![warning],
                ..SymbolLoadReport::default()
            };
        }
    };

    let (symbols, report) = read_project_symbols(&canonical_root);
    replace_registry(Some(canonical_root), symbols);
    report
}

pub(crate) fn clear_project_symbols() {
    replace_registry(None, BTreeMap::new());
}

/// Resolve a symbol for the active project. A lazy reload covers callers that set the legacy
/// project-root global directly instead of going through `mcc_set_project_root`.
pub(crate) fn resolve_project_symbol(class_name: &str) -> Option<CustomSymbol> {
    let project_root = crate::db::infra::init::mcb_get_project_root();
    if project_root.as_os_str().is_empty() {
        return None;
    }
    let canonical_root = fs::canonicalize(&project_root).ok()?;
    let needs_reload = PROJECT_SYMBOLS
        .read()
        .map(|state| state.root.as_ref() != Some(&canonical_root))
        .unwrap_or(true);
    if needs_reload {
        let report = load_project_symbols(&canonical_root);
        log_report(&canonical_root, &report);
    }
    PROJECT_SYMBOLS
        .read()
        .ok()?
        .symbols
        .get(class_name)
        .cloned()
}

pub(crate) fn log_report(project_root: &Path, report: &SymbolLoadReport) {
    if report.manifest_found {
        tracing::info!(
            target: "mcc::viz::symbols",
            project_root = %project_root.display(),
            loaded = report.loaded,
            "loaded project SVG symbols"
        );
    }
    for warning in &report.warnings {
        tracing::warn!(
            target: "mcc::viz::symbols",
            project_root = %project_root.display(),
            warning = %warning,
            "project SVG symbol rejected"
        );
    }
}

fn replace_registry(root: Option<PathBuf>, symbols: BTreeMap<String, CustomSymbol>) {
    if let Ok(mut state) = PROJECT_SYMBOLS.write() {
        state.root = root;
        state.symbols = symbols;
    }
}

fn read_project_symbols(
    canonical_root: &Path,
) -> (BTreeMap<String, CustomSymbol>, SymbolLoadReport) {
    let manifest_path = canonical_root.join(SYMBOL_MANIFEST);
    if !manifest_path.is_file() {
        return (BTreeMap::new(), SymbolLoadReport::default());
    }

    let mut report = SymbolLoadReport {
        manifest_found: true,
        ..SymbolLoadReport::default()
    };
    let symbol_dir = manifest_path.parent().unwrap_or(canonical_root);
    let canonical_symbol_dir = match fs::canonicalize(symbol_dir) {
        Ok(path) if path.starts_with(canonical_root) => path,
        Ok(_) => {
            report
                .warnings
                .push("symbol directory resolves outside the project root".into());
            return (BTreeMap::new(), report);
        }
        Err(error) => {
            report.warnings.push(format!(
                "cannot resolve symbol directory '{}': {error}",
                symbol_dir.display()
            ));
            return (BTreeMap::new(), report);
        }
    };
    let canonical_manifest = match fs::canonicalize(&manifest_path) {
        Ok(path) if path.parent() == Some(canonical_symbol_dir.as_path()) => path,
        Ok(_) => {
            report
                .warnings
                .push("symbol manifest must be a file directly below symbols/".into());
            return (BTreeMap::new(), report);
        }
        Err(error) => {
            report.warnings.push(format!(
                "cannot resolve symbol manifest '{}': {error}",
                manifest_path.display()
            ));
            return (BTreeMap::new(), report);
        }
    };
    let content = match read_bounded_utf8(&canonical_manifest, MAX_MANIFEST_BYTES) {
        Ok(content) => content,
        Err(error) => {
            report.warnings.push(error);
            return (BTreeMap::new(), report);
        }
    };
    let manifest: SymbolManifest = match toml::from_str(&content) {
        Ok(manifest) => manifest,
        Err(error) => {
            report.warnings.push(format!(
                "cannot parse '{}': {error}",
                canonical_manifest.display()
            ));
            return (BTreeMap::new(), report);
        }
    };
    if manifest.schema_version != 1 {
        report.warnings.push(format!(
            "unsupported symbol manifest schema_version {}; expected 1",
            manifest.schema_version
        ));
        return (BTreeMap::new(), report);
    }

    let mut symbols = BTreeMap::new();
    let mut classes = BTreeSet::new();

    for entry in manifest.symbols {
        let class_name = entry.class.trim();
        if class_name.is_empty() {
            report
                .warnings
                .push("symbol entry has an empty class name".into());
            continue;
        }
        if !classes.insert(class_name.to_string()) {
            report
                .warnings
                .push(format!("duplicate symbol class '{class_name}'"));
            continue;
        }

        match load_symbol_entry(&canonical_symbol_dir, &entry.file) {
            Ok(symbol) => {
                symbols.insert(class_name.to_string(), symbol);
            }
            Err(error) => report
                .warnings
                .push(format!("class '{class_name}': {error}")),
        }
    }

    report.loaded = symbols.len();
    (symbols, report)
}

fn load_symbol_entry(canonical_symbol_dir: &Path, file: &str) -> Result<CustomSymbol, String> {
    let relative = Path::new(file);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("file path '{file}' must stay below symbols/"));
    }
    if relative.extension().and_then(|ext| ext.to_str()) != Some("svg") {
        return Err(format!("file '{file}' must use the .svg extension"));
    }

    let requested = canonical_symbol_dir.join(relative);
    let canonical_file = fs::canonicalize(&requested)
        .map_err(|error| format!("cannot resolve '{}': {error}", requested.display()))?;
    if !canonical_file.starts_with(canonical_symbol_dir) {
        return Err(format!("file '{file}' resolves outside symbols/"));
    }

    let input = read_bounded_utf8(&canonical_file, MAX_SVG_BYTES)?;
    let (svg_body, view_box) = sanitize_svg(&input)
        .map_err(|error| format!("unsafe or invalid SVG '{}': {error}", file))?;
    Ok(CustomSymbol {
        source: format!("symbols/{file}"),
        svg_body,
        view_box,
    })
}

fn read_bounded_utf8(path: &Path, max_bytes: u64) -> Result<String, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect '{}': {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("'{}' is not a regular file", path.display()));
    }
    if metadata.len() > max_bytes {
        return Err(format!(
            "'{}' is {} bytes; limit is {} bytes",
            path.display(),
            metadata.len(),
            max_bytes
        ));
    }
    fs::read_to_string(path)
        .map_err(|error| format!("cannot read UTF-8 file '{}': {error}", path.display()))
}

/// Validate a small, presentation-only SVG subset and return the outer SVG's body + viewBox.
fn sanitize_svg(input: &str) -> Result<(String, SvgViewBox), String> {
    if input.as_bytes().contains(&0) {
        return Err("NUL bytes are not allowed".into());
    }

    let mut cursor = 0usize;
    let mut stack: Vec<String> = Vec::new();
    let mut body_start = None;
    let mut body_end = None;
    let mut view_box = None;
    let mut element_count = 0usize;
    let bytes = input.as_bytes();

    while cursor < bytes.len() {
        let relative_start = input[cursor..]
            .find('<')
            .ok_or_else(|| "content exists outside the root <svg> element".to_string())?;
        let tag_start = cursor + relative_start;
        if !input[cursor..tag_start].trim().is_empty() {
            return Err("text nodes are not allowed".into());
        }
        let tag_end = find_tag_end(input, tag_start + 1)?;
        let parsed = parse_tag(&input[tag_start + 1..tag_end])?;

        if parsed.closing {
            let expected = stack
                .pop()
                .ok_or_else(|| format!("unexpected closing tag </{}>", parsed.name))?;
            if parsed.name != expected {
                return Err(format!(
                    "mismatched closing tag </{}>; expected </{}>",
                    parsed.name, expected
                ));
            }
            if parsed.name == "svg" {
                body_end = Some(tag_start);
                if !input[tag_end + 1..].trim().is_empty() {
                    return Err("content after </svg> is not allowed".into());
                }
            }
        } else {
            element_count += 1;
            if element_count > MAX_SVG_ELEMENTS {
                return Err(format!("SVG has more than {MAX_SVG_ELEMENTS} elements"));
            }
            if body_start.is_none() {
                if parsed.name != "svg" {
                    return Err("root element must be <svg>".into());
                }
                if parsed.self_closing {
                    return Err("root <svg> cannot be empty".into());
                }
                view_box = Some(validate_root_attrs(&parsed.attrs)?);
                body_start = Some(tag_end + 1);
            } else {
                validate_graphic_tag(&parsed)?;
            }
            if !parsed.self_closing {
                stack.push(parsed.name);
            }
        }

        cursor = tag_end + 1;
        if body_end.is_some() {
            break;
        }
    }

    if !stack.is_empty() {
        return Err(format!("unclosed tag <{}>", stack.last().unwrap()));
    }
    let start = body_start.ok_or_else(|| "missing root <svg>".to_string())?;
    let end = body_end.ok_or_else(|| "missing closing </svg>".to_string())?;
    let body = input[start..end].trim();
    if body.is_empty() {
        return Err("SVG body is empty".into());
    }
    Ok((body.to_string(), view_box.unwrap()))
}

fn find_tag_end(input: &str, start: usize) -> Result<usize, String> {
    let mut quote = None;
    for (offset, byte) in input.as_bytes()[start..].iter().copied().enumerate() {
        match (quote, byte) {
            (None, b'\'' | b'"') => quote = Some(byte),
            (Some(active), current) if active == current => quote = None,
            (None, b'>') => return Ok(start + offset),
            _ => {}
        }
    }
    Err("unterminated SVG tag".into())
}

fn parse_tag(raw: &str) -> Result<ParsedTag, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('?') {
        return Err("comments, declarations, DOCTYPE, and entities are not allowed".into());
    }
    if let Some(rest) = trimmed.strip_prefix('/') {
        let name = rest.trim();
        if !is_xml_name(name) || name.contains(char::is_whitespace) {
            return Err(format!("invalid closing tag '</{name}>'"));
        }
        return Ok(ParsedTag {
            name: name.to_string(),
            attrs: BTreeMap::new(),
            closing: true,
            self_closing: false,
        });
    }

    let (content, self_closing) = match trimmed.strip_suffix('/') {
        Some(content) => (content.trim_end(), true),
        None => (trimmed, false),
    };
    let name_end = content.find(char::is_whitespace).unwrap_or(content.len());
    let name = &content[..name_end];
    if !is_xml_name(name) {
        return Err(format!("invalid tag name '<{name}>'"));
    }
    let attrs = parse_attrs(&content[name_end..])?;
    Ok(ParsedTag {
        name: name.to_string(),
        attrs,
        closing: false,
        self_closing,
    })
}

fn parse_attrs(mut input: &str) -> Result<BTreeMap<String, String>, String> {
    let mut attrs = BTreeMap::new();
    loop {
        input = input.trim_start();
        if input.is_empty() {
            return Ok(attrs);
        }
        let name_end = input
            .find(|c: char| c.is_whitespace() || c == '=')
            .unwrap_or(input.len());
        let name = &input[..name_end];
        if !is_xml_name(name) {
            return Err(format!("invalid attribute name '{name}'"));
        }
        input = input[name_end..].trim_start();
        input = input
            .strip_prefix('=')
            .ok_or_else(|| format!("attribute '{name}' must have a quoted value"))?
            .trim_start();
        let quote = input
            .chars()
            .next()
            .filter(|c| *c == '\'' || *c == '"')
            .ok_or_else(|| format!("attribute '{name}' must use quotes"))?;
        input = &input[quote.len_utf8()..];
        let value_end = input
            .find(quote)
            .ok_or_else(|| format!("unterminated value for attribute '{name}'"))?;
        let value = &input[..value_end];
        if value.contains(['&', '<', '>']) {
            return Err(format!("attribute '{name}' contains forbidden markup"));
        }
        if attrs.insert(name.to_string(), value.to_string()).is_some() {
            return Err(format!("duplicate attribute '{name}'"));
        }
        input = &input[value_end + quote.len_utf8()..];
    }
}

fn is_xml_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':'))
}

fn validate_root_attrs(attrs: &BTreeMap<String, String>) -> Result<SvgViewBox, String> {
    for (name, value) in attrs {
        match name.as_str() {
            "viewBox" => {}
            "xmlns" if value == "http://www.w3.org/2000/svg" => {}
            "xmlns" => return Err("root xmlns must be the SVG namespace".into()),
            _ => return Err(format!("root SVG attribute '{name}' is not allowed")),
        }
    }
    let raw = attrs
        .get("viewBox")
        .ok_or_else(|| "root SVG requires viewBox".to_string())?;
    let values = parse_number_list(raw)?;
    if values.len() != 4 || values.iter().any(|value| !value.is_finite()) {
        return Err("viewBox must contain four finite numbers".into());
    }
    if values[2] <= 0.0 || values[3] <= 0.0 {
        return Err("viewBox width and height must be positive".into());
    }
    Ok(SvgViewBox {
        min_x: values[0],
        min_y: values[1],
        width: values[2],
        height: values[3],
    })
}

fn validate_graphic_tag(tag: &ParsedTag) -> Result<(), String> {
    if !matches!(
        tag.name.as_str(),
        "g" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon"
    ) {
        return Err(format!("element <{}> is not allowed", tag.name));
    }
    for (name, value) in &tag.attrs {
        if name.starts_with("on") || name.contains(':') {
            return Err(format!("attribute '{name}' is not allowed"));
        }
        match name.as_str() {
            "d" => validate_path_data(value)?,
            "points" => {
                parse_number_list(value)?;
            }
            "x" | "y" | "x1" | "y1" | "x2" | "y2" | "cx" | "cy" | "r" | "rx" | "ry" | "width"
            | "height" | "stroke-width" | "stroke-miterlimit" | "opacity" | "fill-opacity"
            | "stroke-opacity" | "stroke-dashoffset" => {
                parse_number(value)?;
            }
            "fill" | "stroke" => validate_color(value)?,
            "stroke-linecap" if matches!(value.as_str(), "butt" | "round" | "square") => {}
            "stroke-linejoin" if matches!(value.as_str(), "miter" | "round" | "bevel") => {}
            "fill-rule" | "clip-rule" if matches!(value.as_str(), "nonzero" | "evenodd") => {}
            "stroke-dasharray" if value == "none" => {}
            "stroke-dasharray" => {
                parse_number_list(value)?;
            }
            "vector-effect" if value == "non-scaling-stroke" => {}
            _ => return Err(format!("attribute '{name}' is not allowed")),
        }
    }
    Ok(())
}

fn parse_number(value: &str) -> Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("'{value}' is not a number"))?;
    if !parsed.is_finite() {
        return Err(format!("'{value}' is not finite"));
    }
    Ok(parsed)
}

fn parse_number_list(value: &str) -> Result<Vec<f64>, String> {
    let normalized = value.replace(',', " ");
    let values: Result<Vec<_>, _> = normalized.split_whitespace().map(parse_number).collect();
    let values = values?;
    if values.is_empty() {
        return Err("numeric list is empty".into());
    }
    Ok(values)
}

fn validate_color(value: &str) -> Result<(), String> {
    if matches!(value, "none" | "currentColor") {
        return Ok(());
    }
    let Some(hex) = value.strip_prefix('#') else {
        return Err(format!(
            "color '{value}' must be none, currentColor, or hex"
        ));
    };
    if matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("color '{value}' is not a supported hex color"))
    }
}

fn validate_path_data(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("path data is empty".into());
    }
    let allowed_commands = "MmZzLlHhVvCcSsQqTtAaEe";
    if value.chars().all(|c| {
        c.is_ascii_digit()
            || c.is_ascii_whitespace()
            || matches!(c, '.' | ',' | '+' | '-')
            || allowed_commands.contains(c)
    }) {
        Ok(())
    } else {
        Err("path data contains unsupported characters".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    struct TempProject {
        root: PathBuf,
    }

    impl TempProject {
        fn new() -> Self {
            let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "mcc-svg-symbol-test-{}-{serial}",
                std::process::id()
            ));
            fs::create_dir_all(root.join("symbols")).unwrap();
            Self { root }
        }

        fn write(&self, relative: &str, content: &str) {
            fs::write(self.root.join(relative), content).unwrap();
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn valid_project_symbol_loads_as_fragment() {
        let project = TempProject::new();
        project.write(
            "symbols/manifest.toml",
            r#"schema_version = 1

[[symbols]]
class = "USB.MINI_B"
file = "usb-mini-b.svg"
"#,
        );
        project.write(
            "symbols/usb-mini-b.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 80">
  <rect x="5" y="5" width="90" height="70" rx="6" fill="none" stroke="#222222" stroke-width="2"/>
</svg>"##,
        );

        let canonical = fs::canonicalize(&project.root).unwrap();
        let (symbols, report) = read_project_symbols(&canonical);
        assert_eq!(report.loaded, 1, "{:?}", report.warnings);
        assert!(report.warnings.is_empty());
        let symbol = symbols.get("USB.MINI_B").unwrap();
        assert_eq!(symbol.source, "symbols/usb-mini-b.svg");
        assert!(!symbol.svg_body.contains("<svg"));
        assert_eq!(symbol.view_box.width, 100.0);
        assert_eq!(symbol.view_box.height, 80.0);
    }

    #[test]
    fn active_content_and_external_references_are_rejected() {
        for (name, svg) in [
            (
                "script",
                r#"<svg viewBox="0 0 10 10"><script>alert(1)</script></svg>"#,
            ),
            (
                "event",
                r#"<svg viewBox="0 0 10 10"><rect x="0" y="0" width="10" height="10" onload="alert(1)"/></svg>"#,
            ),
            (
                "external",
                r#"<svg viewBox="0 0 10 10"><use href="https://example.com/a.svg#x"/></svg>"#,
            ),
        ] {
            let error = sanitize_svg(svg).unwrap_err();
            assert!(!error.is_empty(), "{name} SVG should be rejected");
        }
    }

    #[test]
    fn symbol_path_cannot_escape_project_directory() {
        let project = TempProject::new();
        project.write(
            "symbols/manifest.toml",
            r#"schema_version = 1

[[symbols]]
class = "USB.MINI_B"
file = "../outside.svg"
"#,
        );
        project.write(
            "outside.svg",
            r#"<svg viewBox="0 0 10 10"><path d="M0 0 L10 10"/></svg>"#,
        );

        let canonical = fs::canonicalize(&project.root).unwrap();
        let (symbols, report) = read_project_symbols(&canonical);
        assert!(symbols.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("must stay below symbols/"));
    }

    #[cfg(unix)]
    #[test]
    fn symbol_symlink_cannot_escape_symbol_directory() {
        use std::os::unix::fs::symlink;

        let project = TempProject::new();
        project.write(
            "symbols/manifest.toml",
            r#"schema_version = 1

[[symbols]]
class = "USB.MINI_B"
file = "linked.svg"
"#,
        );
        project.write(
            "outside.svg",
            r#"<svg viewBox="0 0 10 10"><path d="M0 0 L10 10"/></svg>"#,
        );
        symlink(
            project.root.join("outside.svg"),
            project.root.join("symbols/linked.svg"),
        )
        .unwrap();

        let canonical = fs::canonicalize(&project.root).unwrap();
        let (symbols, report) = read_project_symbols(&canonical);
        assert!(symbols.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("resolves outside symbols/"));
    }

    #[cfg(unix)]
    #[test]
    fn manifest_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let project = TempProject::new();
        project.write("outside-manifest.toml", "schema_version = 1\n");
        symlink(
            project.root.join("outside-manifest.toml"),
            project.root.join("symbols/manifest.toml"),
        )
        .unwrap();

        let canonical = fs::canonicalize(&project.root).unwrap();
        let (symbols, report) = read_project_symbols(&canonical);
        assert!(symbols.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("directly below symbols/"));
    }

    #[test]
    fn missing_manifest_is_a_clean_fallback() {
        let project = TempProject::new();
        let canonical = fs::canonicalize(&project.root).unwrap();
        let (symbols, report) = read_project_symbols(&canonical);
        assert!(symbols.is_empty());
        assert!(!report.manifest_found);
        assert!(report.warnings.is_empty());
    }
}
