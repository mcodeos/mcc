// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! BOM overlay: engineering-level part selection over abstract slots.
//!
//! `bom.overlay.mc` sits at a project root as a sidecar (not in the `use`
//! topology; discovery walks up from the entry file per build). The carrier is
//! mc grammar — one `overlay <top> { path = Class }` block whose rows reuse
//! the `define`-table row shape verbatim, so the mc parser reads it with real
//! lex spans and normal parse diagnostics. Keys are instance paths relative
//! to the top module named in the header (`"LDO.ldo"`); values are component
//! class names resolved against the live defs at bind time. A binding
//! replaces the instance's class identity only — never its shape. See
//! bom-overlay-design.md (U267①); the reading face is the sole producer of
//! [`BindingRow`], and the two consumers (bind seam, E5067/E5068 checks) judge
//! rows, never the carrier.

use crate::ast::node::AstNode;
use crate::ast::{bindings::Frontend, macros::{
    MCAST_ATT_ID, MCAST_ATT_VALUES, MCAST_ATTRIBUTE, MCAST_BODY, MCAST_NAME, MCAST_OVERLAY,
}};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

pub(crate) const OVERLAY_FILE: &str = "bom.overlay.mc";

/// One overlay row as read: the U245 §4 seam contract quadruple. The value is
/// a name, not a resolution — identity is minted by the DefRegistry alone.
#[derive(Debug, Clone)]
pub(crate) struct BindingRow {
    /// Instance path relative to the block-header top.
    pub path: String,
    /// The class name the row names.
    pub value_class: String,
    /// Real lex span of the row (byte offset, length) in the overlay file.
    pub span: (u32, u32),
    /// The overlay file's own uri.
    pub uri: String,
}

/// Outcome of one bind attempt, recorded per overlay key for the ERC checks.
#[derive(Debug, Clone)]
pub(crate) enum BindOutcome {
    /// Slot bound to the variant def.
    Bound,
    /// Value name resolved to no live component def.
    ValueUnresolved,
    /// Value def exists but is not a `:` descendant of the declared slot class.
    ValueNotDescendant,
    /// The key's instance declares a concrete class that differs from the
    /// overlay value — the overlay would silently swallow the board's real
    /// part (b2, E5068 Error).
    KeyNotSlot,
    /// The key's instance declares a concrete class and the overlay row
    /// repeats exactly that class — the duplicate form (b1, E5068 Warning,
    /// the migration trailblazer that retires with U267③).
    KeyRedundant,
}

/// A duplicate key inside one overlay block (visible now the carrier is mc
/// grammar; the TOML carrier's parser rejected duplicates outright). Judged
/// at read time by the same b1/b2 shape: same value is a W (b1), conflicting
/// values are an E (b2) — the second row can never win, so a conflict means
/// the board's selection is ambiguous.
#[derive(Debug, Clone)]
pub(crate) struct DuplicateRow {
    pub key: String,
    pub first_value: String,
    pub dup_value: String,
    pub span: (u32, u32),
    pub uri: String,
}

#[derive(Debug, Default)]
struct BomOverlayState {
    root: Option<PathBuf>,
    /// Canonical entry uri of the build in flight (set by [`begin_build`]).
    entry: Option<String>,
    /// The top module the block header names.
    header_top: Option<String>,
    /// Rows in file order; the read face's single product.
    rows: Vec<BindingRow>,
    /// overlay key (top-relative instance path) -> variant class name;
    /// first row wins on duplicates.
    entries: BTreeMap<String, String>,
    duplicates: Vec<DuplicateRow>,
    /// overlay key -> (canonical instance path, bind outcome); per build run
    binds: BTreeMap<String, (String, BindOutcome)>,
}

static BOM_OVERLAY: LazyLock<RwLock<BomOverlayState>> =
    LazyLock::new(|| RwLock::new(BomOverlayState::default()));

/// Load the overlay that owns `dir` (the nearest ancestor carrying an
/// `OVERLAY_FILE`), replacing whatever the registry held. No ancestor carries
/// one, or the file does not parse, clears the registry — a build whose board
/// has no overlay has no slots to bind. Parse failures report through the
/// normal parse diagnostic domain (E1000 + parser dlog under the overlay
/// file's own uri) before the registry clears.
fn load_for_dir(dir: &Path) {
    let mut hit: Option<(PathBuf, String)> = None;
    let mut cur = Some(dir.to_path_buf());
    while let Some(d) = cur {
        let candidate = d.join(OVERLAY_FILE);
        if let Ok(t) = fs::read_to_string(&candidate) {
            hit = Some((d, t));
            break;
        }
        cur = d.parent().map(|p| p.to_path_buf());
    }
    let Some((root, text)) = hit else {
        let mut state = BOM_OVERLAY.write().expect("bom overlay lock");
        state.root = None;
        state.rows.clear();
        state.header_top = None;
        state.entries.clear();
        state.duplicates.clear();
        return;
    };
    let uri = root.join(OVERLAY_FILE).to_string_lossy().into_owned();
    let (header_top, rows, duplicates, parse_ok) = parse_overlay(&text, &uri);
    let mut entries = BTreeMap::new();
    if parse_ok {
        // First row wins: a duplicate never replaces the earlier selection
        // (its own E5068 W/E fires from `duplicates` instead).
        for row in &rows {
            entries
                .entry(row.path.clone())
                .or_insert_with(|| row.value_class.clone());
        }
    }
    let mut state = BOM_OVERLAY.write().expect("bom overlay lock");
    state.root = Some(root);
    state.header_top = header_top;
    state.rows = rows;
    state.entries = entries;
    state.duplicates = duplicates;
}

/// Parse one `bom.overlay.mc` through the standard C parser. Returns the
/// header top, the rows in file order, the duplicate-key list, and whether
/// the parse came back clean (an error token clears the registry at the
/// caller). Diagnostics drain into the workspace under the overlay file's
/// own uri, exactly as a circuit file's parse errors do.
fn parse_overlay(
    text: &str,
    uri: &str,
) -> (
    Option<String>,
    Vec<BindingRow>,
    Vec<DuplicateRow>,
    bool,
) {
    let mut header_top = None;
    let mut rows: Vec<BindingRow> = Vec::new();
    let mut duplicates: Vec<DuplicateRow> = Vec::new();
    // reset + load + lex + parse must run under the one frontend lock; the
    // Frontend value itself is that lock, and the diagnostics drain below
    // reads the same process-global buffers before it drops.
    let fe = Frontend::acquire();
    fe.reset(0);
    // The overlay file names itself for parse diagnostics: anchor the current
    // uri at it so dlog entries land on the overlay, and give Location::new
    // a line index to resolve byte offsets with.
    let _uri_guard = crate::db::infra::context::UriGuard::new(&crate::McURI::from(uri));
    crate::db::diagnostic::diagnostic::dlog_clear_file(&crate::McURI::from(uri));
    let line_index = line_index::LineIndex::new(text);
    crate::db::infra::context::push_line_index(crate::McURI::from(uri), line_index);

    let c_content = CString::new(text).expect("overlay content CString");
    let fcontent_ptr = unsafe {
        crate::ast::bindings::mcc_load_from_string(c_content.as_ptr() as *const i8, text.len())
    };
    let mut parse_ok = !fcontent_ptr.is_null();
    if !fcontent_ptr.is_null() {
        let fname_cstr = CString::new(uri).expect("overlay uri CString");
        unsafe {
            fe.set_lex_file(fname_cstr.as_ptr());
            fe.lex(fcontent_ptr);
            let ast = AstNode::new(fe.parse());
            if !ast.is_null() {
                // mc_value_link chains the top-level statements off the root's
                // NEXT pointer (the root's own sub stays empty).
                let mut cur = ast.get_next();
                while let Some(stmt) = cur {
                    cur = stmt.get_next();
                    if !stmt.is_type(MCAST_OVERLAY) {
                        continue;
                    }
                    // overlay <top> { rows }: sub = [MCAST_NAME, MCAST_BODY].
                    // Single block per file (v1); a second block is ignored.
                    if header_top.is_none() {
                        header_top = stmt
                            .get_sub_node()
                            .filter(|n| n.is_type(MCAST_NAME))
                            .and_then(|n| node_name(&n));
                    }
                    let Some(body) = stmt
                        .get_sub_node()
                        .and_then(|name| name.get_next())
                        .filter(|n| n.is_type(MCAST_BODY))
                    else {
                        continue;
                    };
                    for clause in body.clause_list() {
                        if !clause.is_type(MCAST_ATTRIBUTE) {
                            continue;
                        }
                        let Some(row) = binding_row(&clause, uri) else {
                            continue;
                        };
                        if let Some(first) = rows.iter().find(|r| r.path == row.path) {
                            duplicates.push(DuplicateRow {
                                key: row.path.clone(),
                                first_value: first.value_class.clone(),
                                dup_value: row.value_class.clone(),
                                span: row.span,
                                uri: row.uri.clone(),
                            });
                        } else {
                            rows.push(row);
                        }
                    }
                }
            }
        }
    }

    // Error tokens (E1000) — a bad parse clears the registry at the caller.
    {
        let mut err_ptr = fe.get_error_tokens();
        while !err_ptr.is_null() {
            let (pos, len, next) = unsafe {
                let err = &*err_ptr;
                (err.pos as u32, err.len as u32, err.next)
            };
            let location = crate::db::diagnostic::diagnostic::Location::new(
                crate::McURI::from(uri),
                pos,
                len,
            );
            let diagnostic = crate::db::diagnostic::diagnostic::Diagnostic::new(
                1000,
                crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                location,
                "syntax error".to_string(),
            );
            crate::db::cmie::tables::WORKSPACE
                .diagnostics
                .lock()
                .unwrap()
                .add_diagnostic(diagnostic);
            parse_ok = false;
            err_ptr = next;
        }
    }
    // Structured parser dlog entries, same drain as a circuit file.
    {
        let mut raw: Vec<(u32, i32, u32, u32, String)> = Vec::new();
        let mut dlog_ptr = fe.get_dlog_entries();
        while !dlog_ptr.is_null() {
            let tuple = unsafe {
                let entry = &*dlog_ptr;
                let msg = if entry.msg.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(entry.msg).to_string_lossy().to_string()
                };
                (entry.code, entry.level, entry.pos, entry.len, msg, entry.next)
            };
            raw.push((tuple.0, tuple.1, tuple.2, tuple.3, tuple.4));
            dlog_ptr = tuple.5;
        }
        for (code, level, pos, len, msg) in raw {
            if msg.is_empty() {
                continue;
            }
            match level {
                2 => crate::db::diagnostic::diagnostic::dlog_warning_at(code, pos, len, &msg),
                _ => crate::db::diagnostic::diagnostic::dlog_error_at(code, pos, len, &msg),
            }
        }
    }

    crate::db::infra::context::pop_line_index();
    if !fcontent_ptr.is_null() {
        unsafe { libc::free(fcontent_ptr as *mut libc::c_void) };
    }
    (header_top, rows, duplicates, parse_ok)
}

/// The dotted name an id-chain node spells (`main`, `LDO.ldo`): token nodes
/// carry it in `data`, id chains in their sub chain.
fn node_name(n: &AstNode) -> Option<String> {
    let dotted = n.to_id_or_ida().join(".");
    if dotted.is_empty() {
        n.to_string().or_else(|| n.get_sub_node().and_then(|s| s.to_string()))
    } else {
        Some(dotted)
    }
}

/// One attribute row -> [`BindingRow`]: key = the dotted id chain, value =
/// the first attr value's name, span = the row's real lex span.
fn binding_row(attr: &AstNode, uri: &str) -> Option<BindingRow> {
    let id_node = attr.get_sub_node().filter(|n| n.is_type(MCAST_ATT_ID))?;
    let ids_node = id_node.get_sub_node()?;
    let path = ids_node
        .to_id_or_ida()
        .join(".");
    let values_node = id_node
        .get_next()
        .filter(|n| n.is_type(MCAST_ATT_VALUES))?;
    let value_class = values_node
        .get_sub_node()
        .and_then(|v| node_name(&v))?;
    let span = attr.get_rlen();
    let (pos, len) = if span > 0 {
        (attr.get_rpos(), attr.get_rlen())
    } else {
        (attr.get_pos(), attr.get_len())
    };
    Some(BindingRow {
        path,
        value_class,
        span: (pos, len),
        uri: uri.to_string(),
    })
}

/// Open a build run: clear the bind ledger and record the build's canonical
/// entry uri (call from the instantiation root once the entry module is
/// resolved). The overlay is rediscovered per build from the entry file's
/// nearest ancestor carrying one, so the overlay binds exactly the boards in
/// the directory tree that owns it — never a registry left over from another
/// project or a synthetic build over virtual sources.
pub(crate) fn begin_build(entry_uri: &str) {
    let entry = crate::build::pass1::canonicalize_project_uri(&crate::McURI::from(entry_uri));
    let parent = Path::new(&entry).parent().map(|p| p.to_path_buf());
    if let Some(dir) = parent {
        load_for_dir(&dir);
    }
    let mut state = BOM_OVERLAY.write().expect("bom overlay lock");
    state.binds.clear();
    state.entry = Some(entry);
}

/// The consumer gate: an overlay was discovered for the build in flight, and
/// that build enters through a file under the discovered root (true by
/// construction for a discovered overlay, false once discovery cleared the
/// registry).
fn overlay_active(state: &BomOverlayState) -> bool {
    let Some(root) = &state.root else {
        return false;
    };
    state
        .entry
        .as_deref()
        .is_some_and(|e| Path::new(e).starts_with(root))
}

/// The overlay key for a component instance declared inside the module being
/// built at `current_path` (`"main.modldo"` + `ldo` -> `"modldo.ldo"`): the
/// canonical path minus the leading top-module segment.
pub(crate) fn overlay_key(current_path: &str, inst: &str) -> String {
    match current_path.split_once('.') {
        Some((_top, rest)) => format!("{rest}.{inst}"),
        None => inst.to_string(),
    }
}

/// Attempt to bind the declared class of one component instance. Called from
/// the Pass2 declaration expansion for every declared component; records the
/// outcome and returns the replacement def when the bind succeeds.
///
/// Resolution is anchored at `uri` (the instantiating module's file), so the
/// value name reads the same visibility world as the module face author.
pub(crate) fn apply_binding(
    current_path: &str,
    inst: &str,
    declared: &std::sync::Arc<crate::semantic::component::McComponent>,
    uri: &crate::McURI,
) -> Option<std::sync::Arc<crate::semantic::component::McComponent>> {
    let key = overlay_key(current_path, inst);
    let value = {
        let state = BOM_OVERLAY.read().expect("bom overlay lock");
        if !overlay_active(&state) {
            None
        } else {
            // The header names the top it binds; a build whose top differs
            // consumes nothing, so every row dangles into the b3 domain.
            let top = current_path.split('.').next();
            let top_ok = state.header_top.as_deref().is_some_and(|t| Some(t) == top);
            if top_ok {
                state.entries.get(&key).cloned()
            } else {
                None
            }
        }
    }?;
    let inst_path = format!("{current_path}.{inst}");
    if !declared.is_abstract {
        // Concrete instance under the key: same class is the duplicate form
        // (b1, Warning); a different class would silently swallow the board's
        // real part (b2, Error).
        let outcome = if declared.name.to_string() == value {
            BindOutcome::KeyRedundant
        } else {
            BindOutcome::KeyNotSlot
        };
        record(key, inst_path, outcome);
        return None;
    }
    let Some((value_def, value_id)) = resolve_value(&value, uri) else {
        record(key, inst_path, BindOutcome::ValueUnresolved);
        return None;
    };
    let Some(declared_id) = def_id_of(declared) else {
        record(key, inst_path, BindOutcome::ValueNotDescendant);
        return None;
    };
    let descendant = value_id
        .and_then(|id| crate::db::defregistry::variant_base_of(id))
        .is_some_and(|base| base == declared_id);
    if !descendant {
        record(key, inst_path, BindOutcome::ValueNotDescendant);
        return None;
    }
    record(key, inst_path, BindOutcome::Bound);
    Some(value_def)
}

fn record(key: String, path: String, outcome: BindOutcome) {
    BOM_OVERLAY
        .write()
        .expect("bom overlay lock")
        .binds
        .insert(key, (path, outcome));
}

/// Resolve a value name against the live component defs (any domain — a
/// value may name a library part), returning (def, def_id). The design sends
/// the overlay through the DefRegistry, not the visibility tables, so this is
/// an exact ident-string match; two live rows with one name resolve to
/// nothing (the exact-name law leaves no second guess).
fn resolve_value(
    name: &str,
    _uri: &crate::McURI,
) -> Option<(
    std::sync::Arc<crate::semantic::component::McComponent>,
    Option<crate::DefId>,
)> {
    let ds = crate::definition_space();
    let hits: Vec<_> = ds
        .all_components()
        .into_iter()
        .filter(|(sn, _)| sn.ident.to_string() == name)
        .collect();
    if hits.len() != 1 {
        return None;
    }
    let (sn, comp) = hits.into_iter().next()?;
    let id = crate::db::defregistry::def_id(&sn, crate::DefKind::Component);
    Some((comp, id))
}

fn def_id_of(comp: &crate::semantic::component::McComponent) -> Option<crate::DefId> {
    let sn = crate::McSpaceName::new(&comp.name, comp.uri.clone());
    crate::db::defregistry::def_id(&sn, crate::DefKind::Component)
}

/// The bind ledger read face for the ERC checks: (key, canonical instance
/// path, outcome) triples, in key order.
pub(crate) fn bind_outcomes() -> Vec<(String, String, BindOutcome)> {
    let state = BOM_OVERLAY.read().expect("bom overlay lock");
    if !overlay_active(&state) {
        return Vec::new();
    }
    state
        .binds
        .iter()
        .map(|(k, (p, o))| (k.clone(), p.clone(), o.clone()))
        .collect()
}

/// Overlay keys that no bind attempt ever consumed (dangling paths).
pub(crate) fn dangling_keys() -> Vec<String> {
    let state = BOM_OVERLAY.read().expect("bom overlay lock");
    if !overlay_active(&state) {
        return Vec::new();
    }
    state
        .entries
        .keys()
        .filter(|k| !state.binds.contains_key(k.as_str()))
        .cloned()
        .collect()
}

/// The class name a key maps to (for check messages).
pub(crate) fn overlay_value(key: &str) -> Option<String> {
    BOM_OVERLAY
        .read()
        .expect("bom overlay lock")
        .entries
        .get(key)
        .cloned()
}

/// The read face for check anchoring: a key's row span (real lex span) and
/// the overlay file uri, so E5067/E5068 point at the overlay row itself.
pub(crate) fn row_anchor(key: &str) -> Option<((u32, u32), String)> {
    let state = BOM_OVERLAY.read().expect("bom overlay lock");
    if !overlay_active(&state) {
        return None;
    }
    state
        .rows
        .iter()
        .find(|r| r.path == key)
        .map(|r| (r.span, r.uri.clone()))
}

/// Duplicate keys inside one overlay block, in file order.
pub(crate) fn duplicate_rows() -> Vec<DuplicateRow> {
    let state = BOM_OVERLAY.read().expect("bom overlay lock");
    if !overlay_active(&state) {
        return Vec::new();
    }
    state.duplicates.clone()
}
