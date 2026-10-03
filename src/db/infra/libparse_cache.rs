// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Library parse result persistent cache (U392 leg B,
//! lib-parse-cache-design §0–§5).
//!
//! One cold CLI build re-parses every library file twice (load-time pass1 +
//! the module pass) and throws the derivation away — ~80% of the load tax
//! (U392 probe, log/10.3.u392-load-probe.md §5). This module persists the
//! **world face** of a library file's derivation to a content-addressed slot
//! under `<data_root>/cache/libparse/<lib>/<src-hash>/` and restores it on
//! the next load, so both parse passes are skipped for an unchanged file.
//!
//! What a slot holds (design §1): the file's full live registry
//! contribution (defs in registration order, modules included), its
//! diagnostics (parse / pass1 / module / validation — everything the file
//! carries after a fresh derive), its resolved `uselist`, spacenames
//! (+ shadowed) and own CMIE names — plus, since leg C, the **LSP symbol
//! faces**: `symbols` (local/global tables, lapper, def faces,
//! `RefDefMap`), `tokens`, `cross_file_targets` and the file's
//! `lsp.class_table` rows. The AST is still not stored: its consumers
//! (`completion`, `join`) re-parse the single file on demand (design §4).
//!
//! The LSP faces are process-global-interned (`DeclareId`/`UriId`), so a
//! slot never stores a bare interned id: every id in the interned domain is
//! symbolized to its canonical key (uri path, kind, scope, name) at capture
//! and re-interned at replay — **symbolic replay** (design §5.1 leg C).
//! There is no intern-order factor: replay is order-independent, so a slot
//! captured by one project replays under another. File-scoped id domains
//! (enum-class counter, instance counter, class `ReferenceId`s) carry no
//! cross-process identity at all and replay verbatim.
//!
//! Gating: the fast path engages for the CLI one-shot commands (both the
//! tables-only world faces and — since leg C — the symbol-serving query
//! commands `show`/`query`/`def`/`refs`, whose consumers read exactly the
//! restored faces). The RPC server and the MCP server keep the full parse
//! until the daemon side of leg C lands (an RPC world must also serve
//! `completion`, whose AST face the slot does not carry). The
//! `MCC_LIBPARSE_CACHE` environment variable overrides the in-process flag
//! (`1` force on, `0` force off) so A/B runs can flip the path without a
//! rebuild.
//!
//! Key (design §2): source-text hash ‖ dependency-set content hashes ‖ the
//! registration-order position (`DefId` counter) the file's defs started
//! at. All three are verified on every hit; any mismatch is a miss that
//! falls back to the fresh parse. Slots are (format version, build number)
//! stamped; a mismatch is an unconditional miss, no field migration.
//!
//! Invariant (design §0 ④): the cache only ever writes under
//! `<data_root>/cache/` and never touches library sources or the project.

use crate::db::cmie::tables as workspace;
use crate::db::defregistry::{DefValue, LoadDomain};
use crate::db::infra::mc_code::McCode;
use crate::refdef::{NameLayer, RefDefEntry, RefDefMap, SourceLocation, SymbolKind};
use crate::semantic::common::{uri_intern, uri_of_file_id, ContainerKind, McSpaceName};
use crate::McIds;
use crate::McURI;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Slot layout version. Bump on any change to [`LibParseSlot`]'s meaning;
/// a mismatch is an unconditional miss (design §3: no per-field migration).
/// 2 = leg C added the LSP face ([`LspSlot`]).
const FORMAT_VERSION: u32 = 2;

/// Gate: whether this process may serve cache hits (and record slots). The
/// RPC/MCP servers never set it; the CLI's tables-only commands do. The env
/// override (`MCC_LIBPARSE_CACHE=1` / `=0`) wins over the flag so an A/B run
/// can flip the path without a rebuild; the third state means "no override".
static TABLES_ONLY: AtomicBool = AtomicBool::new(false);
static ENV_OVERRIDE: AtomicU8 = AtomicU8::new(0); // 0 none, 1 force-on, 2 force-off

/// Opt this process into the cache fast path. Only for callers whose whole
/// lifetime is one tables-only world build (CLI build / check / export
/// faces); a process that will serve LSP must never call this.
pub fn set_tables_only_mode(enabled: bool) {
    TABLES_ONLY.store(enabled, Ordering::Relaxed);
}

/// Read the env override once per query — loads are rare (once per file),
/// and this keeps a test or A/B run able to flip the variable mid-process.
fn env_allows() -> Option<bool> {
    match std::env::var("MCC_LIBPARSE_CACHE").as_deref() {
        Ok("1") => Some(true),
        Ok("0") => Some(false),
        _ => None,
    }
}

/// Whether the fast path may engage in this process right now.
pub fn tables_only() -> bool {
    if let Some(v) = env_allows() {
        return v;
    }
    TABLES_ONLY.load(Ordering::Relaxed)
}

/// One def of the captured file, reduced to what the replay needs: the
/// definition name (the space name's uri is always the owning file), the
/// load domain it registered under, and the value itself. Order in the slot
/// = original `DefId` order, so a replay re-allocates the same ids.
#[derive(Serialize, Deserialize)]
struct CachedDef {
    ident: String,
    domain: LoadDomain,
    def: DefValue,
}

// U392 leg C: the LSP face
/// One persisted symbol id. [`SymId::Key`] ids live in the process-global
/// interned `DeclareId` domain — the slot stores the canonical key and
/// replay re-interns it (the raw id itself is never stored).
/// [`SymId::Raw`] ids are file-scoped (the enum-class counter, the instance
/// counter, the class `ReferenceId`s) and replay verbatim. The decision is
/// made at capture time by intern-ledger membership, so it reflects the
/// true id domain of the captured value instead of a per-`SymbolKind`
/// domain table that could drift as kinds are added.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SymId {
    Key {
        uri: String,
        kind: u8,
        scope: String,
        name: String,
    },
    Raw(u32),
}

/// A `SourceLocation` in portable form: the owning file's canonical path
/// (re-interned on replay) plus the per-file container/func table indices,
/// which replay verbatim.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct SymLoc {
    uri: String,
    container_id: u32,
    func_id: u32,
    start: u32,
    end: u32,
}

/// The LSP symbol faces of one file (design §1): `McSemTokens`, the
/// per-file `McSemSymbols` (local/global tables, lapper, def faces,
/// `RefDefMap`), `cross_file_targets` and the file's own
/// `lsp.class_table` rows. Every interned id appears as a [`SymId`]; map
/// entries are stored as vecs because replay rebuilds the maps and their
/// iteration order carries no meaning.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct LspSlot {
    /// `McSemToken { type_, position, length }` — plain data, no ids.
    tokens: Vec<(i16, i32, i32)>,
    // ── LocalSymbolTable
    inst_id_counter: u32,
    name_to_declare_id: Vec<((String, u8, String, String), SymId, SymLoc)>,
    name_to_declare_ids: Vec<(String, Vec<(String, u8, u8, String)>)>,
    scope_index: Vec<(String, String)>,
    inst_id_to_span: Vec<(u32, u32, u32)>,
    inst_id_to_declare_inst: Vec<(u32, SymId)>,
    // ── GlobalSymbolTable
    class_id_counter: u32,
    declare_class_id_counter: u32,
    class_name_to_id: Vec<(String, String, SymId)>,
    class_id_to_span: Vec<(SymId, String, u32, u32)>,
    declare_class_id_to_span: Vec<(u32, String, u32, u32)>,
    span_to_declare_class_id: Vec<(String, u32, u32, u32)>,
    declare_id_to_class_id: Vec<(u32, SymId)>,
    enum_class_name_to_id: Vec<(String, String, u32)>,
    enum_class_id_to_span: Vec<(u32, String, u32, u32)>,
    enum_value_id_to_span: Vec<(u32, String, u32, u32)>,
    // ── lapper: (start, stop, kind, id) in lapper (sorted) order — replay
    // inserts in the same order, reproducing the internal layout exactly.
    lapper: Vec<(u32, u32, u8, SymId)>,
    // ── def faces
    def_map: Vec<(u8, SymId, SymLoc)>,
    ref_entries: Vec<(u8, SymId, u32, u32)>,
    def_names: Vec<(u8, SymId, String)>,
    container_table: Vec<String>,
    func_table: Vec<String>,
    // ── RefDefMap
    ref_def_map: Option<RefDefSlot>,
    // ── McCode-level
    cross_file_targets: Vec<(SymId, String, u32, u32, u8)>,
    /// `lsp.class_table` rows owned by this file: (uri, ContainerKind,
    /// name, id, span).
    class_table: Vec<(String, u8, String, SymId, u32, u32)>,
}

/// `RefDefMap` in slot form. `def_to_refs` is not stored: replay rebuilds
/// it through `RefDefMap::insert`, which also re-records the U234 refgraph
/// edges the who-uses prefilter reads.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct RefDefSlot {
    entries: Vec<(u8, SymId, SymLoc, u8, u8, String)>,
    containers: Vec<String>,
    /// (lookup-file uri, name, layer, entry…)
    name_index: Vec<(String, String, u8, u8, SymId, SymLoc, u8, u8, String)>,
    owner_uri: String,
}

/// The per-file slot (design §1 world face + §2 key factors). bincode on
/// disk; `(format_version, build_nr)` in the header gates the whole slot.
#[derive(Serialize, Deserialize)]
struct LibParseSlot {
    format_version: u32,
    build_nr: u32,
    /// sha256 of the source text at capture time.
    src_hash: [u8; 32],
    /// `DefId` counter value when this file's defs started allocating. A
    /// hit requires the live counter to equal it (design §2, third factor).
    seq_start: u32,
    /// Canonical dependency URIs, parallel to `dep_hashes`.
    deps: Vec<String>,
    /// sha256 of each dependency's current text at capture time.
    dep_hashes: Vec<[u8; 32]>,
    /// The file's live defs in `DefId` order.
    defs: Vec<CachedDef>,
    /// Everything the file carries in the diagnostic manager after a fresh
    /// derive: parse (E1000), pass1 duplicates, module diagnostics and the
    /// validation findings attributed to this file. Replayed verbatim on a
    /// hit (design §4: the hit path skips the passes that would emit them).
    diagnostics: Vec<crate::db::diagnostic::diagnostic::Diagnostic>,
    /// Resolved use table — module-pass topo order and the world_ver
    /// envelope both read it.
    uselist: Vec<crate::db::infra::mc_use::McUse>,
    /// Spacenames as (ident, uri) pairs; re-interned on replay (the raw
    /// `UriId` is process-local, the path is the portable form).
    spacenames: Vec<(String, String)>,
    /// The displaced-candidates ledger (T11), same re-interning rule.
    spacenames_shadowed: Vec<(String, Vec<String>)>,
    own_cmie_names: Vec<String>,
    /// The LSP symbol faces (leg C). Captured whenever a slot is written;
    /// consumers decide whether they need it.
    lsp: LspSlot,
}

fn build_nr() -> u32 {
    env!("MCC_BUILD_NR").parse().unwrap_or(0)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Library names are manifest identifiers; keep only the characters that are
/// safe as one path segment, so a stray name cannot escape the cache root.
fn sanitize_segment(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

fn slot_dir(lib_name: &str, src_hash_hex: &str) -> PathBuf {
    crate::cli::datadir::data_root()
        .join("cache")
        .join("libparse")
        .join(sanitize_segment(lib_name))
        .join(src_hash_hex)
}

/// Try to restore `canonical_uri` from its slot. `content` is the file's
/// current text (already read by the caller — it is needed for the source
/// hash either way). Every key factor is verified here; any mismatch (or
/// any I/O / decode failure) is a plain miss and the caller falls back to
/// the fresh parse. On a hit the world is mutated: defs are re-registered
/// (reproducing their `DefId`s — the seq factor guards that), diagnostics
/// are replayed, and the returned `McCode` is complete for every
/// tables-only consumer with `modules_parsed` already set so the module
/// pass clean-skips the file.
pub(crate) fn try_replay(
    canonical_uri: &str,
    content: &str,
    is_system_lib: bool,
    lib_name: &str,
) -> Option<McCode> {
    if !tables_only() || !is_system_lib || content.is_empty() {
        return None;
    }
    let src_hash = sha256(content.as_bytes());
    let bytes = std::fs::read(slot_dir(lib_name, &hex(&src_hash)).join("slot.bin")).ok()?;
    let slot: LibParseSlot = bincode::deserialize(&bytes).ok()?;
    if slot.format_version != FORMAT_VERSION || slot.build_nr != build_nr() {
        return None;
    }
    if slot.src_hash != src_hash {
        return None;
    }
    // Registration-order factor: replay only reproduces the fresh `DefId`s
    // while the live counter sits exactly where this file's parse started.
    if workspace::WORKSPACE.registry().next_def_id_snapshot() != slot.seq_start {
        return None;
    }
    // Dependency factor: every dep must still read and hash equal. This is
    // what lets a changed dependency invalidate its dependents' slots
    // without any watcher (design §2: key invalidation only).
    if slot.deps.len() != slot.dep_hashes.len() {
        return None;
    }
    for (dep, want) in slot.deps.iter().zip(slot.dep_hashes.iter()) {
        let got = std::fs::read(dep).ok()?;
        if sha256(&got) != *want {
            return None;
        }
    }

    let uri: McURI = canonical_uri.to_string();
    crate::current_uri::set(&uri);
    // Mirror the fresh path's step 9 pre-clean (no-op in a fresh world,
    // correct on an in-process re-load).
    workspace::WORKSPACE.remove_project_defs_by_uri(&uri);

    let mut mc = McCode::new_from_string(&uri, content)?;
    mc.mcbase = is_system_lib;
    mc.uselist = slot.uselist.clone();
    mc.spacenames = slot
        .spacenames
        .iter()
        .map(|(ident, uri)| {
            (
                McIds::from(ident.clone()),
                McSpaceName::new(&McIds::from(ident.clone()), uri.clone()),
            )
        })
        .collect();
    mc.spacenames_shadowed = slot
        .spacenames_shadowed
        .iter()
        .map(|(ident, uris)| {
            (
                McIds::from(ident.clone()),
                uris
                    .iter()
                    .map(|u| McSpaceName::new(&McIds::from(ident.clone()), u.clone()))
                    .collect(),
            )
        })
        .collect();
    mc.own_cmie_names = slot.own_cmie_names.iter().map(|n| McIds::from(n.clone())).collect();
    // Stamp the current disk mtime so a later in-process reload takes the
    // existing mtime fast path exactly as a freshly parsed file would.
    mc.disk_mtime = std::fs::metadata(canonical_uri)
        .and_then(|m| m.modified())
        .ok();
    mc.pass1_complete = true;
    mc.modules_parsed = true;
    mc.use_table_dirty = false;

    // LSP face first (leg C): it can still reject the slot (a corrupted
    // conversion), and it must fail before any world mutation so the caller's
    // fresh-parse fallback starts from a clean state.
    replay_lsp(&mut mc, &slot.lsp)?;

    for d in &slot.defs {
        let sn = McSpaceName::new(&McIds::from(d.ident.clone()), uri.clone());
        let _ = workspace::WORKSPACE.insert_def(&sn, d.domain.clone(), d.def.clone());
    }

    {
        let mut dm = workspace::WORKSPACE.diagnostics.lock().unwrap();
        for d in &slot.diagnostics {
            dm.add_diagnostic(d.clone());
        }
    }
    Some(mc)
}

/// Capture and store the slot for one library file, after its derive round
/// completed (module pass + validation included — the slot must hold every
/// diagnostic the file carries, design §4). Called from
/// `mcb_parse_all_modules` for each freshly re-derived library file; every
/// guard here is a plain "nothing worth caching" bail-out.
pub(crate) fn store_slot(canonical_uri: &str, lib_name: &str) {
    if !tables_only() || lib_name.is_empty() {
        return;
    }
    let Some(entry) = workspace::WORKSPACE.mcodes.get(canonical_uri) else {
        return;
    };
    let mc = entry.value();
    // Only files whose authority is the disk are cacheable — in-memory
    // entries (mcb_add_from_string / synthetic VIRT_* modules) have no
    // stable source hash.
    if mc.disk_mtime.is_none() || mc.content.is_empty() {
        return;
    }
    let rows = workspace::WORKSPACE.registry().capture_defs_for_uri(canonical_uri);
    if rows.is_empty() {
        return;
    }
    let seq_start = rows[0].0;
    let mut deps = Vec::new();
    let mut dep_hashes = Vec::new();
    for u in &mc.uselist {
        let dep = crate::build::pass1::canonicalize_project_uri(&u.uri);
        let bytes = match std::fs::read(&dep) {
            Ok(b) => b,
            Err(_) => return, // a dep that cannot be read can never be verified — no slot
        };
        dep_hashes.push(sha256(&bytes));
        deps.push(dep);
    }
    let diagnostics = workspace::WORKSPACE
        .diagnostics
        .lock()
        .unwrap()
        .get_diagnostics_for_file(&crate::McURI::from(canonical_uri))
        .into_iter()
        .cloned()
        .collect();

    // The LSP face capture failing (poisoned lock) means no slot at all —
    // a slot whose halves disagree would serve one consumer a world the
    // other half never saw.
    let Some(lsp) = capture_lsp(mc, canonical_uri) else {
        return;
    };
    let slot = LibParseSlot {
        format_version: FORMAT_VERSION,
        build_nr: build_nr(),
        src_hash: sha256(mc.content.as_bytes()),
        seq_start,
        deps,
        dep_hashes,
        defs: rows
            .into_iter()
            .map(|(_, sn, domain, def)| CachedDef {
                ident: sn.ident.to_string(),
                domain,
                def,
            })
            .collect(),
        diagnostics,
        uselist: mc.uselist.clone(),
        spacenames: mc
            .spacenames
            .iter()
            .map(|(ident, sn)| (ident.to_string(), sn.uri.as_uri().to_string()))
            .collect(),
        spacenames_shadowed: mc
            .spacenames_shadowed
            .iter()
            .map(|(ident, sns)| {
                (
                    ident.to_string(),
                    sns.iter().map(|sn| sn.uri.as_uri().to_string()).collect(),
                )
            })
            .collect(),
        own_cmie_names: mc.own_cmie_names.iter().map(|n| n.to_string()).collect(),
        lsp,
    };

    let Ok(bytes) = bincode::serialize(&slot) else {
        return;
    };
    let dir = slot_dir(lib_name, &hex(&slot.src_hash));
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    // Atomic replace (the `LockFile::store` precedent): a concurrent writer
    // of the same key produces the same bytes, so last-rename-wins is safe.
    let tmp = dir.join(format!("slot.bin.tmp-{}", std::process::id()));
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join("slot.bin"));
    }
}

// U392 leg C: capture / replay of the LSP face
/// `SymbolKind` ↔ slot ordinal. The enum is `#[repr(u8)]` with explicit
/// discriminants; the match keeps the two directions total and visible
/// (no transmute — a stale arm is a compile error on the capture side via
/// `as u8` and a `None` miss on the replay side).
fn symbol_kind_from_u8(k: u8) -> Option<SymbolKind> {
    Some(match k {
        0 => SymbolKind::ClassDef,
        1 => SymbolKind::ClassRef,
        2 => SymbolKind::InstDef,
        3 => SymbolKind::InstRef,
        4 => SymbolKind::PortDef,
        5 => SymbolKind::PortRef,
        6 => SymbolKind::LabelDef,
        7 => SymbolKind::LabelRef,
        8 => SymbolKind::FuncDef,
        9 => SymbolKind::FuncRef,
        10 => SymbolKind::PinIdDef,
        11 => SymbolKind::PinIdRef,
        12 => SymbolKind::PinNameDef,
        13 => SymbolKind::PinNameRef,
        14 => SymbolKind::PinIfaceDef,
        15 => SymbolKind::PinIfaceRef,
        16 => SymbolKind::EnumDef,
        17 => SymbolKind::EnumRef,
        18 => SymbolKind::EnumValDef,
        19 => SymbolKind::EnumValRef,
        20 => SymbolKind::RoleDef,
        21 => SymbolKind::ParamDef,
        22 => SymbolKind::DefineDef,
        23 => SymbolKind::AttrDef,
        24 => SymbolKind::FuncParamRef,
        25 => SymbolKind::BusDef,
        26 => SymbolKind::BusRef,
        27 => SymbolKind::UnknownDef,
        28 => SymbolKind::BusMemberDef,
        29 => SymbolKind::BusMemberRef,
        30 => SymbolKind::NetDef,
        31 => SymbolKind::NetRef,
        _ => return None,
    })
}

/// `ContainerKind` ↔ slot ordinal (declaration order of the enum).
fn container_kind_from_u8(k: u8) -> Option<ContainerKind> {
    Some(match k {
        0 => ContainerKind::Function,
        1 => ContainerKind::Component,
        2 => ContainerKind::Module,
        3 => ContainerKind::Interface,
        4 => ContainerKind::Enum,
        5 => ContainerKind::File,
        _ => return None,
    })
}

fn container_kind_to_u8(k: &ContainerKind) -> u8 {
    match k {
        ContainerKind::Function => 0,
        ContainerKind::Component => 1,
        ContainerKind::Module => 2,
        ContainerKind::Interface => 3,
        ContainerKind::Enum => 4,
        ContainerKind::File => 5,
    }
}

/// `NameLayer` ↔ slot ordinal (`NameLayer::rank` order: Own < Use < System).
fn name_layer_from_u8(k: u8) -> Option<NameLayer> {
    Some(match k {
        0 => NameLayer::Own,
        1 => NameLayer::Use,
        2 => NameLayer::System,
        _ => return None,
    })
}

fn name_layer_to_u8(l: &NameLayer) -> u8 {
    match l {
        NameLayer::Own => 0,
        NameLayer::Use => 1,
        NameLayer::System => 2,
    }
}

/// Re-intern one symbolized id (design §5 leg C: symbolic replay). The
/// intern table itself is the raw→fresh map — `intern_declare_id` is keyed,
/// so every slot site carrying one captured id re-interns to the same fresh
/// id without any explicit remap table. `Raw` ids are file-scoped and pass
/// through untouched.
fn replay_id(sid: &SymId) -> Option<u32> {
    match sid {
        SymId::Raw(r) => Some(*r),
        SymId::Key { uri, kind, scope, name } => Some(
            crate::ast::sem::intern_declare_id(
                uri_intern(uri).0,
                scope,
                name,
                symbol_kind_from_u8(*kind)?,
            )
            .raw(),
        ),
    }
}

fn replay_loc(l: &SymLoc) -> SourceLocation {
    SourceLocation {
        file_id: uri_intern(&l.uri).0,
        container_id: l.container_id,
        func_id: l.func_id,
        byte_start: l.start,
        byte_end: l.end,
    }
}

/// Capture the LSP face of a freshly derived library file (called from
/// [`store_slot`], i.e. after the module pass built lappers and consolidated
/// ref-def maps). Returns `None` only on a poisoned lock — the caller then
/// writes no slot at all, keeping the slot schema uniform.
fn capture_lsp(mc: &McCode, canonical_uri: &str) -> Option<LspSlot> {
    let sem = mc.symbols.lock().ok()?;
    let gt = sem.global_table.lock().ok()?;
    // Intern ledger snapshot: membership decides Key vs Raw per id.
    let keys = crate::ast::sem::declare_id_key_snapshot();
    let symid = |raw: u32| -> SymId {
        match keys.get(&raw) {
            Some((fid, kind, scope, name)) => SymId::Key {
                uri: uri_of_file_id(*fid).to_string(),
                kind: *kind,
                scope: scope.clone(),
                name: name.clone(),
            },
            None => SymId::Raw(raw),
        }
    };
    let symloc = |l: &SourceLocation| SymLoc {
        uri: uri_of_file_id(l.file_id).to_string(),
        container_id: l.container_id,
        func_id: l.func_id,
        start: l.byte_start,
        end: l.byte_end,
    };
    let lt = &sem.local_table;
    let tokens = mc.tokens.lock().ok()?;

    let class_table = workspace::WORKSPACE
        .lsp
        .class_table
        .lock()
        .unwrap()
        .iter()
        .filter(|((uri, _, _), _)| uri == canonical_uri)
        .map(|((uri, k, name), (id, span))| {
            (
                uri.clone(),
                container_kind_to_u8(k),
                name.clone(),
                symid(id.raw()),
                span.start as u32,
                span.end as u32,
            )
        })
        .collect();

    let mut slot = LspSlot {
        tokens: tokens
            .tokens
            .iter()
            .map(|t| (t.type_, t.position, t.length))
            .collect(),
        inst_id_counter: lt.inst_id_counter_raw(),
        name_to_declare_id: lt
            .name_to_declare_id
            .iter()
            .map(|((fid, kind, scope, name), (decl, loc))| {
                (
                    (
                        uri_of_file_id(*fid).to_string(),
                        *kind,
                        scope.clone(),
                        name.clone(),
                    ),
                    symid(decl.raw()),
                    symloc(loc),
                )
            })
            .collect(),
        name_to_declare_ids: lt
            .name_to_declare_ids
            .iter()
            .map(|(name, scopes)| {
                (
                    name.clone(),
                    scopes
                        .iter()
                        .map(|(fid, k, p, s)| (uri_of_file_id(*fid).to_string(), *k, *p, s.clone()))
                        .collect(),
                )
            })
            .collect(),
        scope_index: lt
            .scope_index
            .iter()
            .map(|(s, fid)| (s.clone(), uri_of_file_id(*fid).to_string()))
            .collect(),
        inst_id_to_span: lt
            .inst_id_to_span
            .iter()
            .map(|(id, span)| (id.raw(), span.start as u32, span.end as u32))
            .collect(),
        inst_id_to_declare_inst: lt
            .inst_id_to_declare_inst
            .iter()
            .map(|(id, decl)| (id.raw(), symid(decl.raw())))
            .collect(),
        class_id_counter: gt.counters_raw().0,
        declare_class_id_counter: gt.counters_raw().1,
        class_name_to_id: gt
            .class_name_to_id
            .iter()
            .map(|((uri, name), id)| (uri.clone(), name.to_string(), symid(id.raw())))
            .collect(),
        class_id_to_span: gt
            .class_id_to_span
            .iter()
            .map(|(id, (uri, span))| (symid(id.raw()), uri.clone(), span.start as u32, span.end as u32))
            .collect(),
        declare_class_id_to_span: gt
            .declare_class_id_to_span
            .iter()
            .map(|(r, (uri, span))| (r.raw(), uri.clone(), span.start as u32, span.end as u32))
            .collect(),
        span_to_declare_class_id: gt
            .span_to_declare_class_id
            .iter()
            .map(|((uri, span), r)| (uri.clone(), span.start as u32, span.end as u32, r.raw()))
            .collect(),
        declare_id_to_class_id: gt
            .declare_id_to_class_id
            .iter()
            .map(|(r, id)| (r.raw(), symid(id.raw())))
            .collect(),
        enum_class_name_to_id: gt
            .enum_class_name_to_id
            .iter()
            .map(|((uri, name), id)| (uri.clone(), name.to_string(), id.raw()))
            .collect(),
        enum_class_id_to_span: gt
            .enum_class_id_to_span
            .iter()
            .map(|(id, (uri, span))| (id.raw(), uri.clone(), span.start as u32, span.end as u32))
            .collect(),
        enum_value_id_to_span: gt
            .enum_value_id_to_span
            .iter()
            .map(|(id, (uri, span))| (id.raw(), uri.clone(), span.start as u32, span.end as u32))
            .collect(),
        lapper: sem
            .symbol_lapper
            .intervals
            .iter()
            .map(|iv| (iv.start as u32, iv.stop as u32, iv.val.kind, symid(iv.val.id)))
            .collect(),
        def_map: sem
            .def_map
            .iter()
            .map(|((k, id), loc)| (*k as u8, symid(*id), symloc(loc)))
            .collect(),
        ref_entries: sem
            .ref_entries
            .iter()
            .map(|(k, id, s, e)| (*k as u8, symid(*id), *s as u32, *e as u32))
            .collect(),
        def_names: sem
            .def_names
            .iter()
            .map(|((k, id), name)| (*k as u8, symid(*id), name.clone()))
            .collect(),
        container_table: sem.container_table.clone(),
        func_table: sem.func_table.clone(),
        ref_def_map: sem.ref_def_map.as_ref().map(|m| RefDefSlot {
            entries: m
                .entries
                .iter()
                .map(|((k, id), e)| {
                    (
                        *k as u8,
                        symid(*id),
                        symloc(&e.def_loc),
                        e.def_kind as u8,
                        e.cmie_kind,
                        e.def_name.clone(),
                    )
                })
                .collect(),
            containers: m.containers.clone(),
            name_index: m
                .name_index
                .iter()
                .flat_map(|((file_uri, name), cands)| {
                    cands.iter().map(move |c| {
                        (
                            file_uri.clone(),
                            name.clone(),
                            name_layer_to_u8(&c.layer),
                            c.entry.ref_kind as u8,
                            symid(c.entry.ref_id),
                            symloc(&c.entry.def_loc),
                            c.entry.def_kind as u8,
                            c.entry.cmie_kind,
                            c.entry.def_name.clone(),
                        )
                    })
                })
                .collect(),
            owner_uri: m.owner_uri.clone(),
        }),
        cross_file_targets: mc
            .cross_file_targets_snapshot()
            .iter()
            .map(|(id, uri, span, kind)| {
                (
                    symid(id.raw()),
                    uri.clone(),
                    span.start as u32,
                    span.end as u32,
                    *kind,
                )
            })
            .collect(),
        class_table,
    };
    // Canonical order for every map-derived vec: capture iterates live
    // HashMaps whose order is process-nondeterministic, and a slot's bytes
    // must be a function of the world, not of the iteration (the symbolic
    // fixed point and the same-key concurrent-write story both rely on it).
    slot.name_to_declare_id.sort();
    slot.name_to_declare_ids.sort();
    slot.scope_index.sort();
    slot.inst_id_to_span.sort();
    slot.inst_id_to_declare_inst.sort();
    slot.class_name_to_id.sort();
    slot.class_id_to_span.sort();
    slot.declare_class_id_to_span.sort();
    slot.span_to_declare_class_id.sort();
    slot.declare_id_to_class_id.sort();
    slot.enum_class_name_to_id.sort();
    slot.enum_class_id_to_span.sort();
    slot.enum_value_id_to_span.sort();
    slot.def_map.sort();
    slot.def_names.sort();
    if let Some(r) = slot.ref_def_map.as_mut() {
        r.entries.sort();
        r.name_index.sort();
    }
    slot.class_table.sort();
    Some(slot)
}

/// Rebuild the LSP faces of a cache-hit file from the slot. All fallible
/// decoding happens before any commit, so a `None` here leaves the world
/// untouched and the caller falls back to the fresh parse.
fn replay_lsp(mc: &mut McCode, lsp: &LspSlot) -> Option<()> {
    let kind = |k: u8| -> Option<SymbolKind> { symbol_kind_from_u8(k) };
    let rid = |sid: &SymId| -> Option<u32> { replay_id(sid) };
    let span = |s: u32, e: u32| -> std::ops::Range<usize> { s as usize..e as usize };

    // ── decode pass ──
    let mut lt = crate::ast::sem::LocalSymbolTable::new();
    lt.set_inst_id_counter_raw(lsp.inst_id_counter);
    for ((uri, k, scope, name), sid, loc) in &lsp.name_to_declare_id {
        lt.name_to_declare_id.insert(
            (uri_intern(uri).0, *k, scope.clone(), name.clone()),
            (
                crate::ast::sem::DeclareId::from(rid(sid)?),
                replay_loc(loc),
            ),
        );
    }
    for (name, scopes) in &lsp.name_to_declare_ids {
        lt.name_to_declare_ids.insert(
            name.clone(),
            scopes
                .iter()
                .map(|(u, k, p, s)| (uri_intern(u).0, *k, *p, s.clone()))
                .collect(),
        );
    }
    for (s, u) in &lsp.scope_index {
        lt.scope_index.insert(s.clone(), uri_intern(u).0);
    }
    for (id, s, e) in &lsp.inst_id_to_span {
        lt.inst_id_to_span
            .insert(crate::ast::sem::ReferenceId::from(*id), span(*s, *e));
    }
    for (id, sid) in &lsp.inst_id_to_declare_inst {
        lt.inst_id_to_declare_inst.insert(
            crate::ast::sem::ReferenceId::from(*id),
            crate::ast::sem::DeclareId::from(rid(sid)?),
        );
    }

    let mut gt = crate::ast::sem::GlobalSymbolTable::new();
    gt.set_counters_raw(lsp.class_id_counter, lsp.declare_class_id_counter);
    for (u, n, sid) in &lsp.class_name_to_id {
        gt.class_name_to_id.insert(
            (u.clone(), McIds::from(n.clone())),
            crate::ast::sem::DeclareId::from(rid(sid)?),
        );
    }
    for (sid, u, s, e) in &lsp.class_id_to_span {
        gt.class_id_to_span.insert(
            crate::ast::sem::DeclareId::from(rid(sid)?),
            (u.clone(), span(*s, *e)),
        );
    }
    for (r, u, s, e) in &lsp.declare_class_id_to_span {
        gt.declare_class_id_to_span
            .insert(crate::ast::sem::ReferenceId::from(*r), (u.clone(), span(*s, *e)));
    }
    for (u, s, e, r) in &lsp.span_to_declare_class_id {
        gt.span_to_declare_class_id.insert(
            (u.clone(), span(*s, *e)),
            crate::ast::sem::ReferenceId::from(*r),
        );
    }
    for (r, sid) in &lsp.declare_id_to_class_id {
        gt.declare_id_to_class_id.insert(
            crate::ast::sem::ReferenceId::from(*r),
            crate::ast::sem::DeclareId::from(rid(sid)?),
        );
    }
    for (u, n, raw) in &lsp.enum_class_name_to_id {
        gt.enum_class_name_to_id.insert(
            (u.clone(), McIds::from(n.clone())),
            crate::ast::sem::DeclareId::from(*raw),
        );
    }
    for (raw, u, s, e) in &lsp.enum_class_id_to_span {
        gt.enum_class_id_to_span.insert(
            crate::ast::sem::DeclareId::from(*raw),
            (u.clone(), span(*s, *e)),
        );
    }
    for (raw, u, s, e) in &lsp.enum_value_id_to_span {
        gt.enum_value_id_to_span.insert(
            crate::ast::sem::DeclareId::from(*raw),
            (u.clone(), span(*s, *e)),
        );
    }

    let mut lapper = crate::ast::sem::SymbolRangeLapper::new(vec![]);
    for (s, e, k, sid) in &lsp.lapper {
        lapper.insert(rust_lapper::Interval {
            start: *s as usize,
            stop: *e as usize,
            val: crate::refdef::SymbolType { kind: *k, id: rid(sid)? },
        });
    }

    let mut def_map = HashMap::new();
    for (k, sid, loc) in &lsp.def_map {
        def_map.insert((kind(*k)?, rid(sid)?), replay_loc(loc));
    }
    let mut ref_entries = Vec::new();
    for (k, sid, s, e) in &lsp.ref_entries {
        ref_entries.push((kind(*k)?, rid(sid)?, *s as usize, *e as usize));
    }
    let mut def_names = HashMap::new();
    for (k, sid, name) in &lsp.def_names {
        def_names.insert((kind(*k)?, rid(sid)?), name.clone());
    }
    let ref_def_map = match &lsp.ref_def_map {
        Some(r) => {
            let mut m = RefDefMap::default();
            m.containers = r.containers.clone();
            m.owner_uri = r.owner_uri.clone();
            // Rebuild through the public mutators, not by direct field fill:
            // `insert` also maintains `def_to_refs` and records the U234
            // def-resolution edge into the who-uses refgraph (the edge
            // endpoints are name+file pairs — portable, no raw ids), and
            // `add_name_candidate` re-applies the same-def dedup. Skipping
            // these would silently degrade the who-uses prefilter on a
            // cache-hit world.
            for (k, sid, loc, dk, ck, name) in &r.entries {
                m.insert(
                    kind(*k)?,
                    rid(sid)?,
                    RefDefEntry {
                        ref_kind: kind(*k)?,
                        ref_id: rid(sid)?,
                        def_loc: replay_loc(loc),
                        def_kind: symbol_kind_from_u8(*dk)?,
                        cmie_kind: *ck,
                        def_name: name.clone(),
                    },
                );
            }
            for (file_uri, name, layer, rk, sid, loc, dk, ck, dn) in &r.name_index {
                m.add_name_candidate(
                    &McURI::from(file_uri.clone()),
                    name,
                    name_layer_from_u8(*layer)?,
                    RefDefEntry {
                        ref_kind: symbol_kind_from_u8(*rk)?,
                        ref_id: rid(sid)?,
                        def_loc: replay_loc(loc),
                        def_kind: symbol_kind_from_u8(*dk)?,
                        cmie_kind: *ck,
                        def_name: dn.clone(),
                    },
                );
            }
            Some(m)
        }
        None => None,
    };

    let cross_file_targets = lsp
        .cross_file_targets
        .iter()
        .map(|(sid, uri, s, e, k)| {
            Some((
                crate::ast::sem::DeclareId::from(rid(sid)?),
                McURI::from(uri.clone()),
                span(*s, *e),
                *k,
            ))
        })
        .collect::<Option<Vec<_>>>()?;

    // ── commit pass ──
    {
        let mut sem = mc.symbols.lock().ok()?;
        *sem.global_table.lock().ok()? = gt;
        sem.local_table = lt;
        sem.symbol_lapper = lapper;
        sem.def_map = def_map;
        sem.ref_entries = ref_entries;
        sem.def_names = def_names;
        sem.container_table = lsp.container_table.clone();
        sem.func_table = lsp.func_table.clone();
        sem.ref_def_map = ref_def_map;
    }
    {
        let mut tokens = mc.tokens.lock().ok()?;
        tokens.tokens = lsp
            .tokens
            .iter()
            .map(|(t, p, l)| crate::ast::token::McSemToken {
                type_: *t,
                position: *p,
                length: *l,
            })
            .collect();
    }
    mc.set_cross_file_targets(cross_file_targets);

    for (uri, ck, name, sid, s, e) in &lsp.class_table {
        workspace::WORKSPACE.lsp.class_table.lock().unwrap().insert(
            (uri.clone(), container_kind_from_u8(*ck)?, name.clone()),
            (crate::ast::sem::DeclareId::from(rid(sid)?), span(*s, *e)),
        );
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slot must refuse a foreign format version / build number outright
    /// — the version header is the whole migration story (design §3).
    #[test]
    fn libparse_cache__version_header_gates_the_slot() {
        assert_ne!(FORMAT_VERSION, 0);
        // The gate default must stay off: a process that never opted in
        // (the servers) must never take the fast path.
        assert!(!TABLES_ONLY.load(Ordering::Relaxed));
    }

    #[test]
    fn libparse_cache__sanitize_segment_drops_path_separators() {
        assert_eq!(sanitize_segment("mcode"), "mcode");
        assert_eq!(sanitize_segment("../../etc"), "etc");
        assert_eq!(sanitize_segment(""), "");
    }

    #[test]
    fn libparse_cache__hex_and_sha_shapes() {
        assert_eq!(hex(&[0xde, 0xad]), "dead");
        assert_eq!(sha256(b"").len(), 32);
        assert_ne!(sha256(b"a"), sha256(b"b"));
    }

    /// Parse + derive one corpus file so every LSP face is populated.
    fn derive(corpus: &str, label: &str) -> McCode {
        let path = std::path::PathBuf::from("/tmp").join(format!(
            "mcc-libparse-c-{}-{label}.mc",
            std::process::id()
        ));
        std::fs::write(&path, corpus).expect("write corpus");
        let uri = path.to_string_lossy().to_string();
        let mut code = McCode::new_from_string(&uri, corpus).expect("mc code from string");
        code.parse_ast();
        code.parse_pass1_types();
        let _ = code.parse_pass1_modules();
        code.create_lapper();
        code
    }

    /// Symbolic fixed point (design §5 leg C): the slot form carries no
    /// interned id, so after wiping the intern ledger and replaying, a fresh
    /// capture of the restored world must equal the slot byte-for-byte —
    /// the symbolic form is id-free, hence replay-order- and
    /// intern-position-independent. The corpus has no enum (the one
    /// file-scoped counter domain a re-capture could not re-symbolize);
    /// the raw pass-through law is covered separately below.
    #[test]
    fn libparse_cache__lsp_capture_replay_is_a_symbolic_fixed_point() {
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        const CORPUS: &str = r#"
component RES.SMD0603
{
    name = "resistor"
    pins = [
        a 1 = PASS
        b 2 = PASS
    ]
}

interface Isolation
{
    pins = [
        1 = IN
        2 = OUT
    ]
}
"#;
        let code = derive(CORPUS, "fixed");
        let uri = code.uri.to_string();
        let s1 = capture_lsp(&code, &uri).expect("capture");
        assert!(!s1.lapper.is_empty(), "corpus must populate the lapper");
        assert!(!s1.name_to_declare_id.is_empty());
        // The on-disk encoding must survive a bincode round-trip.
        let bytes = bincode::serialize(&s1).expect("bincode encode");
        let s1d: LspSlot = bincode::deserialize(&bytes).expect("bincode decode");
        assert_eq!(s1d, s1, "bincode round-trip drift");

        // Wipe the intern ledger: replay allocates fresh raw ids, but the
        // symbolic form must not move.
        crate::ast::sem::reset_declare_id_space();
        let shell = McCode::new_from_string(&uri, CORPUS).expect("shell mc");
        let mut shell = shell;
        replay_lsp(&mut shell, &s1d).expect("replay");
        let s2 = capture_lsp(&shell, &uri).expect("recapture");
        assert_eq!(s1d, s2, "symbolic replay drift");
    }

    /// Raw pass-through law: file-scoped id domains (the enum-class counter
    /// domain and the class ReferenceId counters) replay verbatim — the
    /// slot stores their raw values and replay must not re-intern them.
    #[test]
    fn libparse_cache__lsp_raw_domains_replay_verbatim() {
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        const CORPUS: &str = r#"
component RES.SMD0603
{
    name = "resistor"
    pins = [
        a 1 = PASS
        b 2 = PASS
    ]
}

enum GRADE { IND, AUTO }
"#;
        let code = derive(CORPUS, "raw");
        let uri = code.uri.to_string();
        let s1 = capture_lsp(&code, &uri).expect("capture");
        assert!(
            !s1.enum_class_name_to_id.is_empty(),
            "corpus must register an enum class"
        );
        crate::ast::sem::reset_declare_id_space();
        let mut shell = McCode::new_from_string(&uri, CORPUS).expect("shell mc");
        replay_lsp(&mut shell, &s1).expect("replay");
        {
            let sem = shell.symbols.lock().unwrap();
            let gt = sem.global_table.lock().unwrap();
            assert_eq!(
                gt.counters_raw(),
                (s1.class_id_counter, s1.declare_class_id_counter),
                "counters must restore verbatim"
            );
            for (raw, uri_s, start, end) in &s1.enum_class_id_to_span {
                let got = gt.enum_class_id_to_span.get(&crate::ast::sem::DeclareId::from(*raw));
                assert!(
                    got.is_some(),
                    "enum class id {raw} must survive replay verbatim"
                );
                let (gu, gs) = got.unwrap();
                assert_eq!(gu.as_str(), uri_s.as_str());
                assert_eq!(gs.start as u32, *start);
                assert_eq!(gs.end as u32, *end);
            }
        }
    }
}
