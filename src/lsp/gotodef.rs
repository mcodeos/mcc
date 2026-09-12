// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Go-to-definition — resolve a symbol name to its definition location.
//!
//! Extracted from `rpc/handlers/defs.rs` (handle_def).

use crate::db::infra::mc_code::McCode;
use crate::query::iterators::{
    mcb_iter_components, mcb_iter_enums, mcb_iter_interfaces, mcb_iter_modules,
};
use crate::{McCMIE, McIds, McSpaceName, McURI};
use serde_json::{json, Value};
use std::path::Path;

/// Fast path: search RefDefMap name_index across all loaded files (§7.4).
/// Returns (def_uri_str, def_kind_name) if found, None otherwise.
/// T11 (N3): also returns the def's real declared name — a `use X as A`
/// alias exposes the def under a bucket key different from its registry name,
/// and the `get_def` call below must use the registry name, not the alias.
fn find_def_in_refdefmap(name: &str) -> Option<(String, String, String)> {
    for mcfile in crate::definition_space().source_files() {
        if let Ok(sym) = mcfile.symbols.lock() {
            if let Some(ref map) = sym.ref_def_map {
                if let Some(def_entry) = map.get_by_name(&mcfile.uri, name) {
                    let def_uri =
                        crate::semantic::common::uri_of_file_id(def_entry.def_loc.file_id)
                            .to_string();
                    let def_kind = def_entry.def_kind.kind_name().to_string();
                    let def_name = if def_entry.def_name.is_empty() {
                        name.to_string()
                    } else {
                        def_entry.def_name.clone()
                    };
                    return Some((def_uri, def_kind, def_name));
                }
            }
        }
    }
    None
}

/// Low-level: find a definition by name across components/modules/interfaces/enums.
/// Returns the CMIE and its URI string. Used by both `resolve` (JSON) and
/// `find_def_by_name` (RPC handlers).
///
/// Tries RefDefMap fast path first (§7.4), falls back to O(n) project table scan.
pub fn find_def_by_name_raw(name: &str) -> Option<(McCMIE, String)> {
    // ★ Fast path: RefDefMap lookup (§7.4)
    if let Some((def_uri, _def_kind, def_name)) = find_def_in_refdefmap(name) {
        let ident = McIds::from(def_name.as_str());
        let uri_obj = McURI::from(def_uri.as_str());
        if let Some(cmie) = crate::get_def(&ident, &uri_obj) {
            return Some((cmie, def_uri));
        }
    }

    // Fallback: O(n) scan across all project tables
    let iterators: [Vec<(String, String)>; 4] = [
        mcb_iter_components(),
        mcb_iter_modules(),
        mcb_iter_interfaces(),
        mcb_iter_enums(),
    ];
    for items in &iterators {
        if let Some((matched, uri)) = items.iter().find(|(n, _)| n == name) {
            let ident = McIds::from(matched.as_str());
            let uri_obj = McURI::from(uri.as_str());
            if let Some(cmie) = crate::get_def(&ident, &uri_obj) {
                return Some((cmie, uri.clone()));
            }
        }
    }
    None
}

/// Find a definition by name, restricted to the visibility set V(F) of the
/// cursor file `from_uri` (§5.4): P3 (own file) + P4 (use chain) + P5 (mcode).
/// Never returns a definition from a file that F has not `use`d.
pub fn find_def_by_name_in_file(name: &str, from_uri: &str) -> Option<(McCMIE, String)> {
    let from_uri_obj = McURI::from(from_uri);

    // ① P3 + P4: the file's own symbols / use chain (RefDefMap name_index).
    // The file's symbols lock is released before `get_def`: class resolution
    // (Resolver) re-locks the same file's symbols, and std Mutex is not
    // reentrant — holding it across the call would self-deadlock.
    let (def_uri, def_name) = {
        let ds = crate::definition_space();
        let mcfile = ds.source_file(&from_uri_obj);
        let mut def_uri = String::new();
        let mut def_name = String::new();
        if let Some(mcfile) = mcfile {
            if let Ok(sym) = mcfile.symbols.lock() {
                if let Some(ref map) = sym.ref_def_map {
                    if let Some(entry) = map.get_by_name(&from_uri_obj, name) {
                        def_uri = crate::semantic::common::uri_of_file_id(entry.def_loc.file_id)
                            .to_string();
                        // T11 (N3): an alias bucket key differs from the def's
                        // registry name — resolve the def by its real name.
                        def_name = if entry.def_name.is_empty() {
                            name.to_string()
                        } else {
                            entry.def_name.clone()
                        };
                    }
                }
            }
        }
        (def_uri, def_name)
    };
    if !def_uri.is_empty() {
        let ident = McIds::from(def_name.as_str());
        if let Some(cmie) = crate::get_def(&ident, &McURI::from(def_uri.as_str())) {
            return Some((cmie, def_uri));
        }
    }

    // ② P5: mcode system library.
    let ident = McIds::from(name);
    let cmie = crate::db::resolve::Resolver::resolve_system(&ident)?;
    let uri = crate::db::resolve::cmie_uri(&cmie)?;
    Some((cmie, uri))
}

/// Resolve a symbol name to its definition, returning structured JSON.
/// Looks across components, modules, interfaces, and enums.
pub fn resolve(name: &str) -> Option<Value> {
    let (cmie, uri) = find_def_by_name_raw(name)?;
    cmie_to_value(name, cmie, uri)
}

/// Resolve a symbol name to its definition within the cursor file's visibility
/// set V(F) (§5.4). A miss returns `None` — never a cross-file guess.
pub fn resolve_in_file(name: &str, from_uri: &str) -> Option<Value> {
    let (cmie, uri) = find_def_by_name_in_file(name, from_uri)?;
    // Final guard: the resolved definition must lie in V(F) (§5.4). Both the
    // name_index hit (P3/P4) and the mcode lookup (P5) satisfy this by
    // construction; the check guards against any future name-based fallback.
    let def = McSpaceName::new(&McIds::from(name), McURI::from(uri.as_str()));
    if !crate::db::resolve::is_visible(&McURI::from(from_uri), &def) {
        return None;
    }
    cmie_to_value(name, cmie, uri)
}

fn cmie_to_value(name: &str, cmie: McCMIE, uri: String) -> Option<Value> {
    match cmie {
        McCMIE::Component(c) => Some(json!({
            "kind": "component", "name": name, "uri": uri,
            "pin_count": c.pins.pins.len(),
        })),
        McCMIE::Module(m) => Some(json!({
            "kind": "module", "name": name, "uri": uri,
            "instance_count": m.insts.iter().count(),
        })),
        McCMIE::Interface(i) => Some(json!({
            "kind": "interface", "name": name, "uri": uri,
            "pin_count": i.pins.pins.len(),
        })),
        McCMIE::Enum(e) => Some(json!({
            "kind": "enum", "name": name, "uri": uri,
            "value_count": e.values.len(),
        })),
    }
}

/// Strict position-aware goto-def: lapper interval at `offset` + RefDefMap
/// exact resolution (shared with hover). Returns the def location and its real
/// kind (`ClassDef` / `EnumDef` / ...) — never a name-based guess, which would
/// misattribute same-name defs such as `enum CAP` vs `component CAP`.
/// Returns `None` when the position has no registered interval or no map entry.
pub fn resolve_at_pos(uri: &str, offset: usize) -> Option<Value> {
    use crate::refdef::query::resolve_at;

    let mc_uri = McURI::from(uri);
    let ds = crate::definition_space();
    let mcfile = ds.source_file_tolerant(&mc_uri)?;

    // ★ Use jump: the cursor sits on a `use`/`pub use` directive — jump to the
    // target file the directive loads (mcext gotodef parity) instead of
    // symbol resolution. Target is opened at (0,0), like the plugin.
    if let Some(target) = resolve_use_jump(&mcfile, offset) {
        return Some(json!({
            "kind": "UseJump",
            "uri": target,
            "byte_start": 0,
            "byte_end": 0,
        }));
    }

    let sym = mcfile.symbols.lock().ok()?;
    let map = sym.ref_def_map.as_ref()?;
    let hit = resolve_at(map, &sym.symbol_lapper, offset)?;

    Some(json!({
        "kind": hit.def_kind.kind_name(),
        "uri": hit.file_uri,
        "byte_start": hit.byte_start,
        "byte_end": hit.byte_end,
    }))
}

/// Resolve the `use` directive covering `offset` to its on-disk target file.
///
/// The compiler's own [`McUse`] (resolved by `update_abs_path` at load time)
/// is the source of truth — prefix rules (`./`, `../`, `/`, `$`), `@version`
/// suffixes and module auto-completion (`conn` → `conn/conn.mc`) are all
/// handled there. `uri` is rewritten to a canonical absolute path only when
/// the target exists on disk (canonicalize succeeded); an unresolved `uri`
/// (module path or relative path) is never absolute, so a missing target
/// simply yields no jump. Returns the target's absolute path.
fn resolve_use_jump(mcfile: &McCode, offset: usize) -> Option<String> {
    let target = mcfile.uselist.iter().find(|u| {
        let start = u.pos as usize;
        let end = start + u.len as usize;
        if offset >= start && offset < end {
            return true;
        }
        // The AST node's span starts just after the `use` keyword (`use
        // ./x` → pos 3), so the keyword itself is outside `[start, end)`.
        // mcext jumps when the cursor is anywhere on a use-directive line;
        // widen to the line start when the line is a `use`/`pub use`
        // directive and the cursor precedes the statement.
        offset < start && {
            let line_start = mcfile
                .content
                .get(..start)
                .and_then(|before| before.rfind('\n'))
                .map(|nl| nl + 1)
                .unwrap_or(0);
            offset >= line_start && is_use_directive_line(&mcfile.content, line_start, start)
        }
    })?;
    let path = Path::new(target.uri.as_str());
    if path.is_absolute() && path.is_file() {
        Some(target.uri.to_string())
    } else {
        None
    }
}

/// Is the text between `line_start` and the use-statement start a bare
/// `use` / `pub use` directive head (nothing else on the line before it)?
fn is_use_directive_line(content: &str, line_start: usize, pos: usize) -> bool {
    match content.get(line_start..pos) {
        Some(head) => {
            let head = head.trim_start();
            head == "use" || head == "pub use"
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::infra::init::MCC_TEST_PARSE_LOCK;
    use std::fs;
    use std::path::PathBuf;

    /// Use-jump: cursor on a `use` directive resolves to the on-disk target
    /// file (mcext gotodef parity). Mirrors the production LSP shape — the
    /// server proxy normalizes a workspace-relative path to `file://<abs>`
    /// and pushes content through `mcb_add_from_string`, keying the workspace
    /// by that URI; the compiler resolves the relative target against the
    /// file's real directory.
    #[test]
    fn def_mccode__use_jump_resolves_relative_target() {
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let dir = std::env::temp_dir().join(format!("mcc-usetest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let helper_path = dir.join("helper.mc");
        fs::write(&helper_path, "module helper {}\n").unwrap();
        let main_path = dir.join("main.mc");
        let main_src = "use ./helper\n\nmodule main {}\n";
        fs::write(&main_path, main_src).unwrap();

        // The proxy sends `file://<canonical abs path>`; the server loads the
        // file from that URI (sem string load). Canonicalize so the temp dir
        // survives macOS `/var` → `/private/var` symlink normalization.
        let f_uri: crate::McURI = format!("file://{}", main_path.canonicalize().unwrap().display());
        crate::mcc_load_from_string(&f_uri, main_src);

        // Cursor on the use path (`./helper`) → UseJump to helper.mc.
        let use_off = main_src.find("./helper").unwrap();
        let d = crate::lsp::gotodef::resolve_at_pos(&f_uri, use_off).expect("use jump");
        assert_eq!(d["kind"], "UseJump");
        let want = helper_path.canonicalize().unwrap();
        assert_eq!(
            d["uri"]
                .as_str()
                .map(|s| PathBuf::from(s).canonicalize().unwrap()),
            Some(want),
            "use-jump target must be the canonical helper.mc: {d}"
        );
        assert_eq!(d["byte_start"].as_u64(), Some(0));

        // Cursor on the `use` keyword → same jump.
        let kw_off = main_src.find("use ").unwrap();
        let d2 = crate::lsp::gotodef::resolve_at_pos(&f_uri, kw_off).expect("use keyword jump");
        assert_eq!(d2["kind"], "UseJump");

        // Cleanup.
        let _ = fs::remove_dir_all(&dir);
    }

    /// Use-jump must not shadow normal symbol goto-def: a position outside any
    /// `use` statement still resolves through the RefDefMap (class ref → head).
    #[test]
    fn def_mccode__use_jump_does_not_shadow_symbol_gotodef() {
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let source = r#"
enum CAP { X7R, MLCC, C0G }

component CAP (diel = CAP.X7R)
{
    pins = [
        1 = 1
        2 = 2
    ]
}

module main
{
    CAP C1
    C1.1 -> V5V
}
"#;
        let uri: crate::McURI = "/mcc/gotodef-cap.mc".to_string();
        crate::mcc_load_from_string(&uri, source);
        crate::mcc_build(&McIds::from("main"), &uri).expect("build failed");

        // `CAP` in `CAP C1` is a class reference → the component head.
        let comp_ref = source.find("CAP C1").unwrap();
        let d =
            crate::lsp::gotodef::resolve_at_pos(&uri, comp_ref).expect("goto-def at component ref");
        assert_eq!(
            d["kind"], "ClassDef",
            "use-jump must not shadow symbol resolution: {d}"
        );
    }

    /// Cross-file goto-def: a class reference in one file (`US513` in
    /// hbl.mc) resolves to its definition in a sibling file (us513.mc) that
    /// the `use` directive pulls into the definition space. Mirrors the app
    /// shape — the server string-loads the opened file from a `file://` URI
    /// and recursively loads its on-disk deps (hbl ↔ us513).
    #[test]
    fn def_mccode__cross_file_class_gotodef() {
        let _guard = MCC_TEST_PARSE_LOCK.lock().expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let dir = std::env::temp_dir().join(format!("mcc-xgotodef-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let def_path = dir.join("us513.mc");
        let def_src = r#"
component MCU.US513_20_F
{
    pins = [ 1 = VDD ]
}

module US513([VDD_3V3, GND]::DC(3.3V), [VCC_1V2, GND]::DC(1.2V))
{
    out DAC_OUT
}
"#;
        fs::write(&def_path, def_src).unwrap();
        let main_path = dir.join("hbl.mc");
        let main_src = "use ./us513.mc\n\nmodule main\n{\n    US513 mcu513(V3V3, V1V2)\n}\n";
        fs::write(&main_path, main_src).unwrap();

        // Production shape: proxy sends `file://<abs>`; the string load
        // recursively pulls us513.mc from disk and mcc_load_from_string
        // derives modules for both files, building the shared ref-def map.
        let f_uri: crate::McURI = format!("file://{}", main_path.canonicalize().unwrap().display());
        crate::mcc_load_from_string(&f_uri, main_src);

        // `US513` in the instance declaration is a class reference → the
        // module head in us513.mc. Component and module class heads both
        // register as `ClassDef` in the ref-def map.
        let off = main_src.find("US513 mcu513").unwrap();
        let d =
            crate::lsp::gotodef::resolve_at_pos(&f_uri, off).expect("cross-file class goto-def");
        assert_eq!(
            d["kind"], "ClassDef",
            "class ref must resolve to the class def: {d}"
        );
        let want = def_path.canonicalize().unwrap();
        assert_eq!(
            d["uri"]
                .as_str()
                .map(|s| PathBuf::from(s).canonicalize().unwrap()),
            Some(want),
            "US513 def must live in us513.mc: {d}"
        );
        assert!(
            d["byte_start"].as_u64().is_some(),
            "def must carry a source position: {d}"
        );

        // Cleanup.
        let _ = fs::remove_dir_all(&dir);
    }
}
