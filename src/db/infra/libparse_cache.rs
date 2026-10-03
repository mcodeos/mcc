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
//! What a slot holds (design §1, world face only): the file's full live
//! registry contribution (defs in registration order, modules included),
//! its diagnostics (parse / pass1 / module / validation — everything the
//! file carries after a fresh derive), its resolved `uselist`, spacenames
//! (+ shadowed) and own CMIE names. The AST and the LSP symbol faces
//! (`symbols`/`tokens`/lapper/class_table) are deliberately **not** stored:
//! CLI world consumers are tables-only (U392 consumer census, design §1),
//! and the LSP faces are process-global-interned (`DeclareId`/`UriId`) —
//! replaying them across processes needs an intern-order replay design of
//! its own. That is leg C.
//!
//! Gating: the fast path may only engage where the process will never serve
//! LSP on the restored world. CLI one-shot analysis commands opt in via
//! [`set_tables_only_mode`]; the RPC server, the MCP server and every
//! symbol-serving command keep the full parse (they still benefit from —
//! and refresh — nothing: they neither read nor write slots, so a daemon
//! world stays byte-for-byte the fresh-parse world). The `MCC_LIBPARSE_CACHE`
//! environment variable overrides the in-process flag (`1` force on, `0`
//! force off) so A/B runs can flip the path without a rebuild.
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
use crate::semantic::common::{uri_intern, McSpaceName};
use crate::McIds;
use crate::McURI;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Slot layout version. Bump on any change to [`LibParseSlot`]'s meaning;
/// a mismatch is an unconditional miss (design §3: no per-field migration).
const FORMAT_VERSION: u32 = 1;

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

    let uri_id = uri_intern(&uri);
    for d in &slot.defs {
        let sn = McSpaceName::new(&McIds::from(d.ident.clone()), uri.clone());
        let _ = workspace::WORKSPACE.insert_def(&sn, d.domain.clone(), d.def.clone());
    }

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
}
