// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! RPC API Handlers — Iteration B
//!
//! ### Project (Project Mode)
//!   - `project.create`         Create project workspace
//!   - `project.use`            Switch active project
//!   - `project.upload`         Upload file to project src/
//!   - `project.upload_archive` Upload entire project directory
//!   - `project.parse`          Pass1
//!   - `project.build`          Pass1 + Pass2
//!   - `project.delete`         Delete project
//!
//! ### Lib (Library Management)
//!   - `lib.load`               Load library by name into memory
//!   - `lib.unload`             Unload library from memory (if loaded)
//!
//! ### Common Pass
//!   - `build.full`             Run Pass1 + Pass2 based on the active workspace    
//!
//! ## Error codes (extended JSON-RPC standard)
//!   - 32100  IO / FS error
//!   - 32101  workspace conflict / cannot create
//!   - 32102  workspace does not exist
//!   - 32103  archive / decode failed
//!   - 32104  unsupported format
//!   - 32105  entry file not found
//!   - 32106  dependency not loaded
//!   - 32107  Pass1 / Pass2 failed
//!   - 32110  Pass1 (parse) failed
//!   - 32111  Pass2 (build) failed
//!   - 32112  component / module / entity / file not found

use super::protocol::{JsonRpcError, RpcResult};
use crate::search_api::{walk_defs, SearchInputs, SearchKind};
use crate::McURI;
// The pin-id order is one rule for the whole crate; it lives next to the type
// that owns pin ids, not here. See `McComponentInst::sorted_pin_ids`.
use crate::pin_id_cmp;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

// LSP semantic token/symbol assembly (extracted to lsp/sem.rs)
pub(crate) use params::*;
pub(crate) mod params;
pub use crate::lsp::sem::{classify_token_by_symbol, try_lookup_sem};

pub(crate) fn mcc_system_root() -> PathBuf {
    // Single source of truth: delegate to datadir::data_root() (which honors
    // $MCC_SYSTEM_ROOT). The cwd/mc/ probe and the `~/.mcode` fallback live
    // there now.
    crate::cli::datadir::data_root()
}

pub(crate) fn projects_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcc-projects")
}
pub(crate) fn project_dir(id: &str) -> PathBuf {
    projects_dir().join(id)
}
pub(crate) fn project_manifest(id: &str) -> PathBuf {
    project_dir(id).join(crate::cli::datadir::PROJECT_MANIFEST_NAME)
}
pub(crate) fn mcode_dir() -> PathBuf {
    mcc_system_root().join("mcode")
}

// Existing methods (preserved, behavior unchanged)

// Lib handlers

// defs.search (M5) — text/regex/fuzzy search across loaded definitions

// defs.query (M5 PR#2) — structured DSL query

// export (M5 PR#3) — text/JSON/CSV netlist, BOM, SPICE

/// Resolve an installed library directory under the system root.
/// Flat layout: checks `<root>/<name>` (built-in) and `<root>/<name>@<version>` (3rd-party).
pub(crate) fn resolve_installed_lib_dir(name: &str) -> Option<PathBuf> {
    let root = mcc_system_root();

    // Built-in: <root>/<name> (e.g. mcode)
    let bare = root.join(name);
    if bare.exists() {
        return Some(bare);
    }

    // 3rd-party: <root>/<name>@<version>
    if let Ok(entries) = fs::read_dir(&root) {
        let prefix = format!("{name}@");
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(&prefix) && entry.path().is_dir() {
                return Some(entry.path());
            }
        }
    }
    None
}

pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

// Trace handlers

// Common build.full handlers (based on active workspace)

// Internal: Pass1 / Pass2 execution

pub(crate) fn run_pass1(
    entry: &Path,
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    include_system: bool,
) -> RpcResult {
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());

    // Output Pass 1 trace to server log
    info!(target: "crate::pass1", "----------------------------------------");
    info!(target: "crate::pass1", "[Pass 1] Loading project from: {}", uri);
    info!(target: "crate::pass1", "----------------------------------------");

    crate::mcc_load_project(&mc_uri);
    let pass1 = collect_pass1(&mc_uri, include_system);

    let module_count = crate::mcb_module_count();
    let component_count = crate::mcb_component_count();
    let interface_count = crate::mcb_interface_count();

    // Output definition statistics to server log
    info!(target: "crate::pass1", "Total definitions loaded:");
    info!(target: "crate::pass1", "  - Modules: {}", module_count);
    info!(target: "crate::pass1", "  - Components: {}", component_count);
    info!(target: "crate::pass1", "  - Interfaces: {}", interface_count);

    // Output each module details to server log
    for (name, module_uri) in crate::mcb_iter_modules() {
        let ident = crate::McIds::from(name.as_str());
        let module_mc_uri = McURI::from(module_uri.as_str());
        if let Some(cmie) = crate::get_def(&ident, &module_mc_uri) {
            if let crate::McCMIE::Module(module_def) = cmie {
                info!(target: "crate::pass1", ">> Found module definition: {}", name);
                info!(target: "crate::pass1", "------------------------------------------------------------------");
                info!(target: "crate::pass1", "| Ports ");
                info!(target: "crate::pass1", "|-----------------------------------------------------------------");
                info!(target: "crate::pass1", "|   inputs:  {:?}",
                    module_def.insts.inputs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|   outputs: {:?}",
                    module_def.insts.outputs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|   bidirs:  {:?}",
                    module_def.insts.bidirs_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|   powers:  {:?}",
                    module_def.insts.powers_with_name().iter()
                        .map(|(n, _)| *n).collect::<Vec<_>>()
                );
                info!(target: "crate::pass1", "|");
                info!(target: "crate::pass1", "| Symbols ({} entries)", module_def.insts.iter().count());
                info!(target: "crate::pass1", "|-----------------------------------------------------------------");
                for (key, ident) in module_def.insts.iter() {
                    let type_name = ident.type_name();
                    info!(target: "crate::pass1", "|  {:<15} {}", type_name, key);
                }
                info!(target: "crate::pass1", "|");
                info!(target: "crate::pass1", "| Stmts ({} connections)", module_def.stmts.len());
                info!(target: "crate::pass1", "|-----------------------------------------------------------------");
                if module_def.stmts.is_empty() {
                    info!(target: "crate::pass1", "|   (no connections)");
                } else {
                    for (i, _stmt) in module_def.stmts.iter().enumerate() {
                        info!(target: "crate::pass1", "|");
                        info!(target: "crate::pass1", "|   +--- Series[{}] ----------", i);
                    }
                    info!(target: "crate::pass1", "|   +--------------------------------------------------");
                }
                info!(target: "crate::pass1", "------------------------------------------------------------------");
            }
        }
    }

    Ok(json!({
        "command": command,
        "workspace": {"kind": ws_kind, "name": ws_name},
        "pass1": pass1,
        "summary": {
            "module_count": module_count,
            "component_count": component_count,
            "interface_count": interface_count,
        }
    }))
}

/// Execute Pass2: resolve top module, run instantiation with panic guard,
/// collect results. Returns (top_name, pass2_json).
fn execute_pass2(mc_uri: &McURI, top: Option<&str>) -> Result<(String, Value), JsonRpcError> {
    let top_name = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_module_name_by_uri(mc_uri)
            .ok_or_else(|| JsonRpcError::custom(32107, "no top module found"))?,
    };

    let ident = crate::McIds::from(top_name.as_str());
    if crate::get_def(&ident, mc_uri).is_none() {
        return Err(JsonRpcError::custom(
            32107,
            &format!("top module '{top_name}' not defined"),
        ));
    }

    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build_with_arena(&ident, mc_uri)
    }));

    match built {
        Ok(Ok((inst, arena, store, net_store))) => {
            let view = crate::TreeView::new(&arena, &store);
            info!(target: "crate::pass2", "----------------------------------------");
            info!(target: "crate::pass2", "[Pass 2] Instantiating top module: {}", top_name);
            info!(target: "crate::pass2", "----------------------------------------");
            info!(target: "crate::pass2", ">> Instance: {} (class {})",
                inst.name.to_string(), inst.def.name.to_string());
            info!(target: "crate::pass2", "|   ports:       {}", inst.ports.len());
            info!(target: "crate::pass2", "|   components:  {}", view.components(&inst).count());
            info!(target: "crate::pass2", "|   sub_modules: {}", view.sub_modules(&inst).count());
            info!(target: "crate::pass2", "|   connections: {}", inst.connections.len());
            for sub in view.sub_modules(&inst) {
                info!(target: "crate::pass2", "|     - {} (class {})",
                    sub.name.to_string(), sub.def.name.to_string());
            }
            let pass2 = collect_pass2(&top_name, &inst, &view, &net_store);
            Ok((top_name, pass2))
        }
        Ok(Err(e)) => Err(JsonRpcError::custom(
            32107,
            &format!("instantiation failed: {e}"),
        )),
        Err(_) => Err(JsonRpcError::custom(
            32108,
            "Pass2 build panicked (engine bug); request aborted, server kept alive",
        )),
    }
}

/// Execute Pass1 + Pass2 from file
pub(crate) fn run_full_build(
    entry: &Path,
    top: Option<&str>,
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    include_system: bool,
) -> RpcResult {
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());
    // Fresh failure ledger per request (the daemon may be long-lived).
    crate::semantic::validation::ledger::clear();
    crate::mcc_load_project(&mc_uri);
    let pass1 = collect_pass1(&mc_uri, include_system);

    let (top_name, pass2) = execute_pass2(&mc_uri, top)?;

    Ok(json!({
        "command": command,
        "workspace": {"kind": ws_kind, "name": ws_name},
        "pass1": pass1,
        "pass2": pass2,
        "summary": {
            "module_count": crate::mcb_module_count(),
            "component_count": crate::mcb_component_count(),
            "interface_count": crate::mcb_interface_count(),
            "top": top_name,
        },
        "ledger": crate::semantic::validation::ledger::build_report(crate::semantic::validation::ledger::LedgerMode::Summary),
    }))
}

/// Envelope-shaped Pass1+Pass2 result, matching `mcc build`'s local `CommandResult`.
///
/// The CLI's RPC mode deserializes this payload into its own (binary-crate)
/// `CommandResult` and renders it through the same `emit_envelope` funnel as
/// the local path — so the shape must carry every field the client's types
/// require, and the summary must aggregate the same counts
/// `ResultBuilder::finish()` computes locally (design contract
/// local ↔ server). The lib crate cannot reach the
/// binary's `output::*` / `cmds::parse::*` modules, so the envelope is
/// assembled here from the lib-side collectors (`collect_pass1` /
/// `collect_pass2`) plus a cursor-based diagnostic snapshot that mirrors
/// [`crate::...PhaseTracker`]'s phase batching. Kept separate from
/// [`run_full_build`] so the legacy shape used by other RPC methods (e.g.
/// aicontract `check`) is untouched.
///
/// `_include_system` is accepted for the wire contract and deliberately not
/// honored: the local path's `public_collect_pass1` has no system filter at
/// all, so filtering here would break the identical-output contract above.
pub(crate) fn run_full_build_envelope(
    entry: &Path,
    top: Option<&str>,
    libs: &[String],
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    _include_system: bool,
    ledger_mode: crate::semantic::validation::ledger::LedgerMode,
) -> RpcResult {
    // Fresh failure ledger per request (the daemon may be long-lived); the
    // directory path below is reached after this clear.
    crate::semantic::validation::ledger::clear();

    // A directory is a container of definition spaces — one per manifest, one
    // per loose `.mc` file (§19.5 rule 3, use-design.md). Entries are resolved
    // together in one place, whether or not the folder happens to hold a
    // manifest, so what a folder means never depends on which one it is.
    if entry.is_dir() {
        return run_full_build_dir_envelope(
            entry,
            top,
            libs,
            command,
            ws_kind,
            ws_name,
            ledger_mode,
        );
    }

    let t0 = std::time::Instant::now();
    let uri = entry.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());

    // ── Diagnostic cursor: phase batching mirrors the local build. `handle_build_full`
    //    has already run `load_libs_rpc` — the diagnostics present *now* are lib-load
    //    diagnostics → Pass0. Loading/parsing the project below emits the project's
    //    Pass1 diagnostics. Each snapshot advances the cursor. ──
    let mut cursor = 0usize;
    let mut take_diags = |phase: &str| -> Vec<Value> {
        let all = crate::mcc_diagnose_all();
        let slice = if cursor <= all.len() {
            &all[cursor..]
        } else {
            &[]
        };
        let out: Vec<Value> = slice.iter().map(|d| mcc_diag_to_json(d, phase)).collect();
        cursor = all.len();
        out
    };

    let pass0 = json!({ "loaded_files": [], "diagnostics": take_diags("pass0") });

    crate::mcc_load_project(&mc_uri);

    // Include system files unconditionally: the local path's `public_collect_pass1`
    // never filters them, so the RPC payload must carry the same definitions for
    // byte-identical output (design contract: output identical local ↔ server).
    let mut pass1 = collect_pass1(&uri, true);
    pass1["diagnostics"] = Value::Array(take_diags("pass1"));
    // Local's `public_collect_pass1` never populates `definitions.ports`, so
    // the payload must carry the same (empty) list for byte-identical output.
    pass1["definitions"]["ports"] = Value::Array(vec![]);

    // Top selection (mcd spec/16-export-viz §6): explicit top → all modules
    // in the file → all components → all interfaces. Components and interfaces
    // are "virtually instantiated" via a synthetic module so a component-only
    // file (e.g. a connector library part) builds instead of failing with
    // "no top module found". When several targets share the file, the envelope
    // carries the first target's Pass 2 tree (mirrors the CLI build).
    let targets = crate::mcc_virtual_resolve_targets(&mc_uri, top)
        .map_err(|e| JsonRpcError::custom(32107, &e))?;
    let top_name = targets.first().cloned().unwrap_or_else(|| "".to_string());

    // ── World-core build (design §12.2 / §13.6): the top is instantiated ONCE
    //    and held as a live circuit in a per-build CircuitWorld — the scope's
    //    core object. Every consumer below reads a projection off it; nothing
    //    is dismembered and nothing is re-derived. The single one-way flatten
    //    runs the flat electrical net checks once; the circuit keeps both the
    //    results and their diagnostic projection, and the envelope carries the
    //    results under `pass2.net_checks` — never in `pass2.diagnostics`, which
    //    is what the local face does too (U90). A component/interface top is
    //    wrapped in a synthetic module and the projection marks it synthetic,
    //    so an unwired single-part view doesn't flag E4112/E4116. ──
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (mut world, key, synthetic) = crate::mcc_virtual_build_world(&top_name, &mc_uri, 1000)?;
        // Run the flatten (it builds the table and the checks); the returned
        // diagnostics are the fragmentary projection of the same rows the
        // envelope now carries in full, so they are not read here.
        let _diags = match synthetic {
            Some(prefix) => world.flatten_with_prefix(&key, &prefix)?,
            None => world.flatten(&key)?,
        };
        Ok::<_, Box<dyn std::error::Error>>((world, key))
    }));
    let (world, key) = match built {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => {
            return Err(JsonRpcError::custom(
                32107,
                &format!("instantiation failed: {e}"),
            ))
        }
        Err(_) => {
            return Err(JsonRpcError::custom(
                32108,
                "Pass2 build panicked (engine bug); request aborted, server kept alive",
            ))
        }
    };

    let dl = world
        .circuit(&key)
        .ok_or_else(|| JsonRpcError::custom(32107, "build produced no circuit"))?;
    let view = crate::TreeView::new(dl.arena(), dl.store());
    let net_store = dl.net_store().borrow().clone();
    let inst = dl.tree();
    let mut pass2 = collect_pass2(&top_name, inst, &view, &net_store);
    pass2["diagnostics"] = Value::Array(take_diags("pass2"));
    // The flat electrical net checks of this circuit, in their own key — the
    // local face puts the same rows there, and the CLI renders the console
    // section from it on both faces. Omitted when empty, matching the
    // `skip_serializing_if` on `Pass2Report::net_checks`.
    let net_rows = crate::semantic::validation::nets::net_check_rows(dl.net_results());
    if !net_rows.is_empty() {
        pass2["net_checks"] = json!(net_rows);
    }

    // ── Summary, mirroring ResultBuilder::finish() ──
    // Phase C S3-D: the tree tally resolves children through the view (the
    // tree's Vec fields are gone).
    let summary = build_envelope_summary(&pass0, &pass1, &pass2, Some(inst), Some(&view), t0);

    Ok(json!({
        "command": command,
        "workspace": { "kind": ws_kind, "name": ws_name },
        "pass0": pass0,
        "pass1": pass1,
        "pass2": pass2,
        "summary": summary,
        "ledger": crate::semantic::validation::ledger::build_report(ledger_mode),
    }))
}

/// Directory batch build: a directory is a *container* of definition spaces
/// (use-design.md §19.5 rule 3 — `mcc build <dir>` / the extension's Build
/// Project).
///
/// Each entry — every `.mc` file the folder holds, and every subdirectory a
/// `project.toml` names — is loaded into its own world and reported in its own
/// terms, so two unrelated files in one folder cannot collide (`mcc_for_each_entry`;
/// this is what makes `<dir>` mean the same thing with and without a project
/// manifest, and with and without a daemon).
///
/// Pass 2 then builds each entry's default top (modules directly,
/// components/interfaces virtually instantiated) and aggregates the
/// diagnostics; an entry whose build fails is recorded as a Pass 2 error and
/// skipped, so one bad file never aborts the folder report. The envelope
/// carries the first successfully-built tree (mirroring the single-file
/// contract). An explicit `top` builds that target from the first entry that
/// declares it instead of per-entry defaults.
fn run_full_build_dir_envelope(
    entry: &Path,
    top: Option<&str>,
    libs: &[String],
    command: &str,
    ws_kind: &str,
    ws_name: &str,
    ledger_mode: crate::semantic::validation::ledger::LedgerMode,
) -> RpcResult {
    let t0 = std::time::Instant::now();
    // Resolved once: the driver reloads these names for every world, and a
    // world needs mcode re-established after a reset.
    let libs = resolve_libs_rpc(libs);

    let mut merged = BatchEnvelope::default();
    // Pass 0 is the libraries this request asked for, so it has to be read
    // before the batch starts: the driver's first reset clears the world, and
    // the diagnostics go with it.
    let lib_diags: Vec<Value> = crate::mcc_diagnose_all()
        .iter()
        .map(|d| mcc_diag_to_json(d, "pass0"))
        .collect();
    add_diags(&mut merged.seen, &mut merged.pass0, lib_diags);

    // ── Pass 2 (world-core; design §12.2 / §13.6): per-entry default top build,
    //    aggregated ──
    // Every built target is instantiated ONCE into a CircuitWorld and flattened
    // once, so the single one-way flatten runs that circuit's flat electrical
    // net checks once. Components/interfaces are wrapped in a synthetic module
    // and the projection marks it synthetic, so an unwired single-part view
    // doesn't flag E4112/E4116. The first successful tree rides into the
    // envelope (mirroring the single-file contract) together with its net
    // checks; the rest need only their pass-1/pass-2 diagnostics, so neither
    // their owned parts nor their net checks are carried. The checks are
    // reported under `pass2.net_checks`, not folded into `diagnostics` — the
    // local folder build reports them the same way (U90).
    let mut top_name = String::new();
    let mut nets: Vec<crate::semantic::validation::nets::NetCheckRow> = Vec::new();
    let mut first_inst: Option<(
        crate::MccProjectTree,
        crate::NodeArena,
        crate::InstanceStore,
        crate::NetTableStore,
    )> = None;
    let mut search_done = false;

    crate::mcc_for_each_entry(entry, None, &|_| libs.clone(), |e| {
        // One cursor per world. `reset_to` shrinks the diagnostic list to this
        // world's, and the `cursor <= all.len()` guard below answers a shrink by
        // returning empty *and* rewinding — so a cursor carried across worlds
        // would silently skip the next world's first diagnostics.
        let mut cursor = 0usize;
        let mut take_diags = |phase: &str| -> Vec<Value> {
            let all = crate::mcc_diagnose_all();
            let slice = if cursor <= all.len() {
                &all[cursor..]
            } else {
                &[]
            };
            let out: Vec<Value> = slice.iter().map(|d| mcc_diag_to_json(d, phase)).collect();
            cursor = all.len();
            out
        };

        // The driver has already loaded this entry, so the diagnostics now on
        // the cursor are the entry file's and its `use` closure's.
        let world_pass1 = collect_pass1(&e.entry.to_string_lossy(), true);
        let pass1_diags = take_diags("pass1");
        merged.absorb_pass1(&world_pass1);
        add_diags(&mut merged.seen, &mut merged.pass1, pass1_diags);

        // The files this world contributes: what it loaded that is not a
        // library. In a container of entries that is *this* entry's `use`
        // closure — a sibling `.mc` file belongs to its own world, and building
        // it here would both double its ERC and defeat the separation.
        let files: Vec<PathBuf> = world_pass1["loaded_files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| f["is_system"] == false)
            .filter_map(|f| f["uri"].as_str().map(PathBuf::from))
            .collect();

        let mut failures: Vec<Value> = Vec::new();
        if let Some(t) = top {
            // An explicit top can only be looked for in a live world, so the
            // search is per entry; the first entry that declares it wins, and
            // the scan stops there whether or not its build succeeded.
            let entry_uri = McURI::from(e.entry.to_string_lossy().as_ref());
            let declares = !search_done
                && crate::mcc_get_modules_in_file(&entry_uri)
                    .iter()
                    .chain(crate::mcc_get_components_in_file(&entry_uri).iter())
                    .chain(crate::mcc_get_interfaces_in_file(&entry_uri).iter())
                    .any(|name| name == t);
            if declares {
                search_done = true;
                if let Some(pair) = build_dir_target(t, &e.entry, &mut failures, true, &mut nets) {
                    top_name = t.to_string();
                    first_inst = Some(pair);
                }
            }
        } else {
            for file in &files {
                let mc_uri = McURI::from(file.to_string_lossy().as_ref());
                let targets = match crate::mcc_virtual_resolve_targets(&mc_uri, None) {
                    Ok(t) => t,
                    Err(_) => continue, // pass1 already reports the file's problems
                };
                let Some(tgt) = targets.first().cloned() else {
                    continue;
                };
                // Keep the first successful tree; the remaining files still
                // build + flatten, and their net checks are carried as well
                // (U95) — `pass2`'s tree describes the entry it holds, while
                // `pass2.net_checks` covers every entry the folder built.
                let pair =
                    build_dir_target(&tgt, file, &mut failures, first_inst.is_none(), &mut nets);
                if first_inst.is_none() {
                    if let Some(pair) = pair {
                        top_name = tgt;
                        first_inst = Some(pair);
                    }
                }
            }
        }

        add_diags(&mut merged.seen, &mut merged.pass2, take_diags("pass2"));
        add_diags(&mut merged.seen, &mut merged.pass2, failures);
    });

    let pass0 = json!({ "loaded_files": [], "diagnostics": merged.pass0 });
    let pass1 = merged.pass1_json();
    let mut pass2 = match &first_inst {
        Some((inst, arena, store, net_store)) => {
            // Phase C S3-D: the tree tally resolves children through the view
            // (the tree's Vec fields are gone).
            let view = crate::TreeView::new(arena, store);
            let mut p2 = collect_pass2(&top_name, inst, &view, net_store);
            p2["diagnostics"] = Value::Array(merged.pass2);
            p2
        }
        None => json!({
            "top": top_name,
            "instances": Value::Null,
            "connections": [],
            "nets": [],
            "diagnostics": merged.pass2,
        }),
    };
    // Every built entry's flat electrical net checks, in their own key (see the
    // single-file path above). Omitted when empty, matching the local face's
    // `skip_serializing_if`.
    if !nets.is_empty() {
        pass2["net_checks"] = json!(nets);
    }

    // Phase C S3-D: the tree tally resolves children through the view (the
    // tree's Vec fields are gone).
    let view = first_inst.as_ref().map(|(_, a, s, _)| {
        let v = crate::TreeView::new(a, s);
        v
    });
    let summary = build_envelope_summary(
        &pass0,
        &pass1,
        &pass2,
        first_inst.as_ref().map(|(i, ..)| i),
        view.as_ref(),
        t0,
    );

    Ok(json!({
        "command": command,
        "workspace": { "kind": ws_kind, "name": ws_name },
        "pass0": pass0,
        "pass1": pass1,
        "pass2": pass2,
        "summary": summary,
        "ledger": crate::semantic::validation::ledger::build_report(ledger_mode),
    }))
}

/// Build one target out of one file, flattening it so its flat net checks run.
///
/// The checks are appended to `nets` whatever `keep_tree` says: the folder's
/// report covers every entry it built, in build order, and each row carries the
/// `uri` of the file it is located in (U95 — the local folder face reports the
/// same set). Returns the tree and its companions only when `keep_tree` — the
/// envelope carries the first successful entry's tree, and the rest need no
/// more than their diagnostics and checks, so cloning their owned parts out of
/// the circuit would be waste. A failure or a panic becomes a Pass 2 diagnostic
/// keyed at `file` and is swallowed: one bad file must not abort the folder's
/// report.
fn build_dir_target(
    target: &str,
    file: &Path,
    failures: &mut Vec<Value>,
    keep_tree: bool,
    nets: &mut Vec<crate::semantic::validation::nets::NetCheckRow>,
) -> Option<(
    crate::MccProjectTree,
    crate::NodeArena,
    crate::InstanceStore,
    crate::NetTableStore,
)> {
    let uri = file.to_string_lossy().to_string();
    let mc_uri = McURI::from(uri.as_str());
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (mut world, key, synthetic) = crate::mcc_virtual_build_world(target, &mc_uri, 1000)?;
        // Run the flatten (it builds the table and the checks); the returned
        // diagnostics are the fragmentary projection of the same rows the
        // envelope carries in full for the entry it describes, so they are not
        // read here — and not logged either (U90: the checks are not pass-2
        // diagnostics on either face).
        let _diags = match synthetic {
            Some(prefix) => world.flatten_with_prefix(&key, &prefix)?,
            None => world.flatten(&key)?,
        };
        let dl = world
            .circuit(&key)
            .ok_or_else(|| format!("dir build produced no circuit for {target}"))?;
        // Every entry's own flat net checks ride in the envelope, in build
        // order (U95). `keep_tree` decides only whether this entry's owned
        // parts are cloned out as the circuit the envelope describes.
        nets.extend(crate::semantic::validation::nets::net_check_rows(
            dl.net_results(),
        ));
        if keep_tree {
            Ok::<_, Box<dyn std::error::Error>>(Some((
                dl.tree().clone(),
                dl.arena().clone(),
                dl.store().clone(),
                dl.net_store().borrow().clone(),
            )))
        } else {
            Ok::<_, Box<dyn std::error::Error>>(None)
        }
    }));
    match built {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => {
            failures.push(build_failure_diag(
                "pass2",
                &uri,
                &format!("build failed: {e}"),
            ));
            None
        }
        Err(_) => {
            failures.push(build_failure_diag(
                "pass2",
                &uri,
                "Pass2 build panicked (engine bug); skipped",
            ));
            None
        }
    }
}

/// A diagnostic's identity: same code, same place, same words.
type DiagKey = (u64, String, u64, String);

fn diag_key(d: &Value) -> DiagKey {
    (
        d["code"].as_u64().unwrap_or(0),
        d["location"]["file"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        d["location"]["pos"].as_u64().unwrap_or(0),
        d["message"].as_str().unwrap_or_default().to_string(),
    )
}

/// Append `diags`, dropping any this batch has already reported.
fn add_diags(
    seen: &mut std::collections::HashSet<DiagKey>,
    out: &mut Vec<Value>,
    diags: Vec<Value>,
) {
    for d in diags {
        if seen.insert(diag_key(&d)) {
            out.push(d);
        }
    }
}

/// One folder's report, assembled from one world per entry.
///
/// Everything here is keyed rather than appended. A file reached by two
/// entries' `use` closures is parsed once per world, so the same definition —
/// and, with mcode, the same 53 files — arrives once per world that reaches it.
/// The caller asked about a folder, and a definition or a problem in a file is
/// one definition or one problem however many entries include that file.
#[derive(Default)]
struct BatchEnvelope {
    seen: std::collections::HashSet<DiagKey>,
    pass0: Vec<Value>,
    pass1: Vec<Value>,
    pass2: Vec<Value>,
    loaded: BTreeMap<String, Value>,
    modules: BTreeMap<(String, String), Value>,
    components: BTreeMap<(String, String), Value>,
    interfaces: BTreeMap<(String, String), Value>,
    enums: BTreeMap<(String, String), Value>,
}

impl BatchEnvelope {
    /// Fold one world's `collect_pass1` output in, keyed by `(name, uri)` —
    /// so a definition is listed once even though each world was parsed on its
    /// own, and the order comes out sorted rather than in world order.
    fn absorb_pass1(&mut self, one: &Value) {
        for f in one["loaded_files"].as_array().into_iter().flatten() {
            if let Some(uri) = f["uri"].as_str() {
                self.loaded
                    .entry(uri.to_string())
                    .or_insert_with(|| f.clone());
            }
        }
        let d = &one["definitions"];
        for (list, into) in [
            ("modules", &mut self.modules),
            ("components", &mut self.components),
            ("interfaces", &mut self.interfaces),
            ("enums", &mut self.enums),
        ] {
            for item in d[list].as_array().into_iter().flatten() {
                let name = item["name"].as_str().unwrap_or_default().to_string();
                let uri = item["uri"].as_str().unwrap_or_default().to_string();
                into.entry((name, uri)).or_insert_with(|| item.clone());
            }
        }
    }

    fn pass1_json(&self) -> Value {
        json!({
            "loaded_files": self.loaded.values().cloned().collect::<Vec<_>>(),
            "definitions": {
                "modules":    self.modules.values().cloned().collect::<Vec<_>>(),
                "components": self.components.values().cloned().collect::<Vec<_>>(),
                "interfaces": self.interfaces.values().cloned().collect::<Vec<_>>(),
                "enums":      self.enums.values().cloned().collect::<Vec<_>>(),
                // Same contract as the single-file path: ports never populated
                // locally, so the payload carries the same empty list.
                "ports": Value::Array(vec![]),
            },
            "diagnostics": self.pass1,
        })
    }
}

/// An error diagnostic JSON for a Pass 2 build failure, in the same shape as
/// [`mcc_diag_to_json`] so the client's Diagnostic type deserializes it.
fn build_failure_diag(phase: &str, uri: &str, msg: &str) -> Value {
    json!({
        "phase": phase,
        "severity": "error",
        "code": 32107,
        "message": msg,
        "location": { "file": uri, "line": 0, "column": 0, "pos": 0, "len": 0 },
        "suggestions": [],
        "related": [],
    })
}

/// Assemble the envelope summary from the three phase snapshots, mirroring
/// `ResultBuilder::finish()`. `inst` is the Pass 2 tree that produced
/// `pass2`; `None` (directory mode with no buildable file) zeroes the used /
/// instance statistics while namespace class counts still come from pass1.
fn build_envelope_summary(
    pass0: &Value,
    pass1: &Value,
    pass2: &Value,
    inst: Option<&crate::MccProjectTree>,
    view: Option<&crate::TreeView>,
    t0: std::time::Instant,
) -> Value {
    let all_diags: Vec<&Value> = pass0["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(pass1["diagnostics"].as_array().into_iter().flatten())
        .chain(pass2["diagnostics"].as_array().into_iter().flatten())
        .collect();
    let errors = all_diags
        .iter()
        .filter(|d| d["severity"] == "error")
        .count();
    let warnings = all_diags
        .iter()
        .filter(|d| d["severity"] == "warning")
        .count();
    let module_count = pass1["definitions"]["modules"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    let component_count = pass1["definitions"]["components"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    let interface_count = pass1["definitions"]["interfaces"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    let instance_count = count_instance_json(pass2.get("instances"));
    let net_count = pass2["nets"].as_array().map(|v| v.len()).unwrap_or(0);

    // ── Categorized statistics, mirroring `output/mod.rs` `render_envelope_text`
    //    Summary block: namespace classes split system/project, the classes
    //    actually instantiated (used) split system/project, and the instance
    //    breakdown by kind. A class counts as *system* when it is defined only
    //    in the system space — a same-named project definition shadows it. ──
    let (ns_mod_sys, ns_mod_proj) = def_class_split(pass1["definitions"].get("modules"));
    let (ns_comp_sys, ns_comp_proj) = def_class_split(pass1["definitions"].get("components"));
    let (ns_iface_sys, ns_iface_proj) = def_class_split(pass1["definitions"].get("interfaces"));

    let mut sys_mods = std::collections::HashSet::new();
    let mut proj_mods = std::collections::HashSet::new();
    let mut sys_comps = std::collections::HashSet::new();
    let mut proj_comps = std::collections::HashSet::new();
    for d in pass1["definitions"]["modules"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = d["name"].as_str().unwrap_or_default().to_string();
        if d["uri"].as_str().map(is_system_uri).unwrap_or(false) {
            sys_mods.insert(name);
        } else {
            proj_mods.insert(name);
        }
    }
    for d in pass1["definitions"]["components"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = d["name"].as_str().unwrap_or_default().to_string();
        if d["uri"].as_str().map(is_system_uri).unwrap_or(false) {
            sys_comps.insert(name);
        } else {
            proj_comps.insert(name);
        }
    }
    let mut used_modules = std::collections::BTreeSet::new();
    let mut used_components = std::collections::BTreeSet::new();
    let mut module_insts = 0usize;
    let mut component_insts = 0usize;
    if let (Some(inst), Some(view)) = (inst, view) {
        tally_build_stats(
            inst,
            view,
            &mut used_modules,
            &mut used_components,
            &mut module_insts,
            &mut component_insts,
        );
    }
    let is_system_class = |name: &str,
                           system: &std::collections::HashSet<String>,
                           project: &std::collections::HashSet<String>| {
        system.contains(name) && !project.contains(name)
    };
    let used_modules_system = used_modules
        .iter()
        .filter(|n| is_system_class(n, &sys_mods, &proj_mods))
        .count();
    let used_components_system = used_components
        .iter()
        .filter(|n| is_system_class(n, &sys_comps, &proj_comps))
        .count();

    json!({
        "module_count": module_count,
        "component_count": component_count,
        "interface_count": interface_count,
        "instance_count": instance_count,
        "net_count": net_count,
        "errors": errors,
        "warnings": warnings,
        "elapsed_ms": t0.elapsed().as_millis(),
        "stats": {
            "ns_modules_system": ns_mod_sys,
            "ns_modules_project": ns_mod_proj,
            "ns_components_system": ns_comp_sys,
            "ns_components_project": ns_comp_proj,
            "ns_interfaces_system": ns_iface_sys,
            "ns_interfaces_project": ns_iface_proj,
            "used_modules_system": used_modules_system,
            "used_modules_project": used_modules.len() - used_modules_system,
            "used_components_system": used_components_system,
            "used_components_project": used_components.len() - used_components_system,
            "module_insts": module_insts,
            "component_insts": component_insts,
        },
    })
}

/// Envelope `Diagnostic` JSON from an `mcc::Diagnostic`, mirroring the binary
/// crate's `output::diagnostic::from_mcc` field mapping so the payload
/// deserializes into the client's `Diagnostic` byte-identically.
fn mcc_diag_to_json(d: &crate::McDiagnostic, phase: &str) -> Value {
    let related: Vec<Value> = d
        .other
        .iter()
        .map(|ri| {
            json!({
                "message": ri.get_formatted_message(),
                "location": {
                    "file": ri.location.uri.as_str(),
                    "line": ri.location.row,
                    "column": ri.location.col,
                    "pos": ri.location.pos,
                    "len": ri.location.len,
                },
            })
        })
        .collect();
    let severity = match d.level {
        crate::DiagnosticLevel::Error => "error",
        crate::DiagnosticLevel::Warning => "warning",
        crate::DiagnosticLevel::Info => "info",
        crate::DiagnosticLevel::Hint => "hint",
    };
    json!({
        "phase": phase,
        "severity": severity,
        "code": d.code,
        "message": d.msg,
        "location": {
            "file": d.loc.uri.as_str(),
            "line": d.loc.row,
            "column": d.loc.col,
            "pos": d.loc.pos,
            "len": d.loc.len,
        },
        "suggestions": [],
        "related": related,
    })
}

/// Recursively count total instances in the Pass2 instance tree (root +
/// components + submodules), mirroring `ResultBuilder`'s `count_instances`.
fn count_instance_json(node: Option<&Value>) -> usize {
    let Some(n) = node else { return 0 };
    // A synthetic VIRT_<T> wrapper (fabricated so a standalone component or
    // interface file can be built) is not a real instance — exclude its whole
    // tree (mirrors `count_instances` in output/builder.rs), so a library
    // build reports instances=0 instead of 2 (wrapper + unit).
    if n["synthetic"].as_bool().unwrap_or(false) {
        return 0;
    }
    let mut total = 1;
    if let Some(cs) = n["components"].as_array() {
        total += cs.len();
    }
    for sub in n["sub_modules"].as_array().into_iter().flatten() {
        total += count_instance_json(Some(sub));
    }
    total
}

/// Count how many of a pass1 definition array live in the system space
/// (`/mcode/`) vs the project. Returns `(system, project)`.
fn def_class_split(arr: Option<&Value>) -> (usize, usize) {
    let Some(arr) = arr.and_then(|v| v.as_array()) else {
        return (0, 0);
    };
    let sys = arr
        .iter()
        .filter(|d| d["uri"].as_str().map(is_system_uri).unwrap_or(false))
        .count();
    (sys, arr.len() - sys)
}

/// Walk the instance tree, collecting per-kind instance counts and the distinct
/// set of classes actually instantiated (mirrors `output/mod.rs` `tally_tree`).
///
/// Phase C S3-D: children resolve through the store-backed `view` (the tree's
/// Vec fields are gone).
fn tally_build_stats(
    node: &crate::MccProjectTree,
    view: &crate::TreeView,
    used_modules: &mut std::collections::BTreeSet<String>,
    used_components: &mut std::collections::BTreeSet<String>,
    module_insts: &mut usize,
    component_insts: &mut usize,
) {
    // A synthetic VIRT_<T> wrapper (fabricated so a standalone component or
    // interface file can be built) is not a real instance — exclude its whole
    // tree (mirrors `tally_tree` in output/mod.rs).
    if crate::mcc_is_synthetic_module(&node.def.name.to_string()) {
        return;
    }
    *module_insts += 1;
    *component_insts += view.components(node).count();
    used_modules.insert(node.def.name.to_string());
    for c in view.components(node) {
        used_components.insert(c.def.name.to_string());
    }
    for sub in view.sub_modules(node) {
        tally_build_stats(
            sub,
            view,
            used_modules,
            used_components,
            module_insts,
            component_insts,
        );
    }
}

pub(crate) fn collect_pass1(_uri: &str, include_system: bool) -> Value {
    let all_modules = crate::mcb_iter_modules_with_span();
    let all_components = crate::mcb_iter_components_with_span();
    let all_interfaces = crate::mcb_iter_interfaces_with_span();
    let all_enums = crate::mcb_iter_enums_with_span();
    let all_ports = crate::mcb_iter_ports();

    // Filter out system modules, components, interfaces, enums if not include_system
    let (modules, components, interfaces, enums) = if include_system {
        (all_modules, all_components, all_interfaces, all_enums)
    } else {
        let filter =
            |items: Vec<(String, String, [usize; 2])>| -> Vec<(String, String, [usize; 2])> {
                items
                    .into_iter()
                    .filter(|(_, uri, _)| !is_system_uri(uri))
                    .collect()
            };
        (
            filter(all_modules),
            filter(all_components),
            filter(all_interfaces),
            filter(all_enums),
        )
    };

    // Filter ports - only include ports from non-system modules
    let ports: Vec<_> = if include_system {
        all_ports
    } else {
        all_ports
            .into_iter()
            .filter(|(_, _, _, uri)| !is_system_uri(uri))
            .collect()
    };

    // The four definition lists are emitted flat, so their order is the
    // payload's order. `mcb_iter_*` hands them over in load order, which is an
    // artifact of how the workspace was assembled; the local collector sorts by
    // (name, uri) and the two payloads are contracted to be identical, so sort
    // by the same key here.
    let sort_refs = |items: &mut Vec<(String, String, [usize; 2])>| {
        items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    };
    let (mut modules, mut components, mut interfaces, mut enums) =
        (modules, components, interfaces, enums);
    sort_refs(&mut modules);
    sort_refs(&mut components);
    sort_refs(&mut interfaces);
    sort_refs(&mut enums);

    let mut by_uri: BTreeMap<String, FileEntry> = BTreeMap::new();
    for m in &modules {
        let uri = m.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.modules.push(m.0.clone());
    }
    for c in &components {
        let uri = c.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.components.push(c.0.clone());
    }
    for i in &interfaces {
        let uri = i.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.interfaces.push(i.0.clone());
    }
    for en in &enums {
        let uri = en.1.clone();
        let e = by_uri
            .entry(uri.clone())
            .or_insert_with(|| FileEntry::new(&uri));
        e.enums.push(en.0.clone());
    }

    let loaded_files: Vec<Value> = by_uri.into_values().map(|f| f.into_json()).collect();

    // Convert ports to PortRef format
    let ports_json: Vec<serde_json::Value> = ports
        .iter()
        .map(|(name, iotype, module, uri)| {
            serde_json::json!({
                "name": name,
                "iotype": iotype,
                "module": module,
                "uri": uri
            })
        })
        .collect();

    json!({
        "loaded_files": loaded_files,
        "definitions": {
            "modules":    refs_json(&modules),
            "components": refs_json(&components),
            "interfaces": refs_json(&interfaces),
            "enums":      refs_json(&enums),
            "ports":      ports_json,
        },
        "diagnostics": []
    })
}

pub(crate) fn collect_pass2(
    top: &str,
    inst: &crate::MccProjectTree,
    view: &crate::TreeView,
    net_store: &crate::NetTableStore,
) -> Value {
    json!({
        "top": top,
        "instances": instance_to_json(inst, view),
        "connections": extract_connections(inst, view, net_store),
        "nets":       extract_nets(inst, view, net_store),
        "diagnostics": []
    })
}

pub(crate) fn extract_connections(
    inst: &crate::MccProjectTree,
    view: &crate::TreeView,
    net_store: &crate::NetTableStore,
) -> Vec<Value> {
    let mut out = Vec::new();
    walk_connections(inst, "", view, net_store, &mut out);
    out
}

/// Flatten the instance tree's connections into envelope rows. Each row carries
/// its module scope (e.g. `main.speaker`) because connection ids and instance
/// names repeat across modules — the CLI's `ConnectionEntry` requires it. The
/// net name is resolved against this module's net table so every connection
/// gets the same surviving name as the matching nets row (mirrors
/// `cmds::parse::walk_connections`); the statement label is only a fallback.
pub(crate) fn walk_connections(
    inst: &crate::MccProjectTree,
    scope: &str,
    view: &crate::TreeView,
    net_store: &crate::NetTableStore,
    out: &mut Vec<Value>,
) {
    let my_scope = if scope.is_empty() {
        inst.name.clone()
    } else {
        format!("{}.{}", scope, inst.name)
    };
    let mut point_to_net: HashMap<&str, &str> = HashMap::new();
    // Phase D: the module's frozen union-find net table comes from the store,
    // keyed by its canonical scope path (the tree never carries `NetPoint`).
    if let Some(table) = net_store.get(&my_scope) {
        for (net_name, points) in table {
            for p in points {
                point_to_net.entry(p.path.as_str()).or_insert(net_name);
            }
        }
    }
    for conn in &inst.connections {
        let net_name = conn
            .points
            .iter()
            .find_map(|p| point_to_net.get(p.path.as_str()).copied())
            .map(str::to_string)
            .or_else(|| conn.net_name.clone());
        out.push(json!({
            "id": conn.id,
            "module": my_scope,
            "net_name": net_name,
            "points": conn.points.iter().map(|p| p.path.clone()).collect::<Vec<_>>(),
        }));
    }
    let subs: Vec<&crate::MccProjectTree> = view.sub_modules(inst).collect();
    for sub in subs {
        walk_connections(sub, &my_scope, view, net_store, out);
    }
}

pub(crate) fn instance_to_json(inst: &crate::MccProjectTree, view: &crate::TreeView) -> Value {
    use crate::IOType;
    let ports: Vec<Value> = inst
        .ports
        .iter()
        .filter(|p| !matches!(p.iotype, IOType::None | IOType::NonCon | IOType::Return))
        .map(|p| {
            json!({
                "name":   p.name.to_string(),
                "iotype": iotype_str(&p.iotype),
            })
        })
        .collect();
    let comps: Vec<_> = view.components(inst).collect();
    let components: Vec<Value> = comps
        .iter()
        .map(|c| {
            // `sorted_pin_ids`, not `pins.keys()`: the map's own order is drawn
            // per map instance, so an unsorted walk here made `build.full` — the
            // payload behind the delegated `mcc build` — read out a different
            // pin order on every request (build-design §3.7 discipline 4).
            let pins: Vec<Value> = c
                .sorted_pin_ids()
                .into_iter()
                .map(|pin_id| {
                    let pin_name = c.pin_name(pin_id).unwrap_or_else(|| pin_id.clone());
                    json!({
                        "id":   pin_id.clone(),
                        "name": pin_name,
                    })
                })
                .collect();
            json!({
                "name":       c.name.to_string(),
                "class_name": c.def.name.to_string(),
                "pins":       pins,
                "nc":         c.nc,
            })
        })
        .collect();
    let subs: Vec<&crate::MccProjectTree> = view.sub_modules(inst).collect();
    let sub_modules: Vec<Value> = subs
        .into_iter()
        .map(|s| instance_to_json(s, view))
        .collect();
    json!({
        "name":        inst.name.to_string(),
        "kind":        "module",
        "class_name":  inst.def.name.to_string(),
        "synthetic":   crate::mcc_is_synthetic_module(&inst.def.name.to_string()),
        "ports":       ports,
        "components":  components,
        "sub_modules": sub_modules,
    })
}

pub(crate) fn extract_nets(
    inst: &crate::MccProjectTree,
    view: &crate::TreeView,
    net_store: &crate::NetTableStore,
) -> Vec<Value> {
    let mut nets = Vec::new();
    walk_nets(inst, "", view, net_store, &mut nets);
    nets
}

/// Flatten the instance tree's nets into envelope rows, each tagged with its
/// module scope (`main.speaker`) as the CLI's `NetEntry` requires.
pub(crate) fn walk_nets(
    inst: &crate::MccProjectTree,
    scope: &str,
    view: &crate::TreeView,
    net_store: &crate::NetTableStore,
    out: &mut Vec<Value>,
) {
    let my_scope = if scope.is_empty() {
        inst.name.clone()
    } else {
        format!("{}.{}", scope, inst.name)
    };
    for (name, points) in net_store.sorted(&my_scope) {
        let points: Vec<String> = points.iter().map(|point| point.path.clone()).collect();
        out.push(json!({ "module": my_scope, "name": name, "points": points }));
    }
    let subs: Vec<&crate::MccProjectTree> = view.sub_modules(inst).collect();
    for sub in subs {
        walk_nets(sub, &my_scope, view, net_store, out);
    }
}

pub(crate) fn iotype_str(io: &crate::IOType) -> &'static str {
    use crate::IOType::*;
    match io {
        In => "in",
        Out => "out",
        InOut => "inout",
        Power => "power",
        Return => "return",
        NonCon => "noncon",
        None => "none",
    }
}

// File entry grouping

impl FileEntry {
    fn new(uri: &str) -> Self {
        Self {
            uri: uri.to_string(),
            is_system: is_system_uri(uri),
            modules: vec![],
            components: vec![],
            interfaces: vec![],
            enums: vec![],
        }
    }
    fn into_json(self) -> Value {
        json!({
            "uri":        self.uri,
            "is_system":  self.is_system,
            "modules":    self.modules,
            "components": self.components,
            "interfaces": self.interfaces,
            "enums":      self.enums,
        })
    }
}

/// Check if URI is a system library
pub(crate) fn is_system_uri(uri: &str) -> bool {
    uri.contains("/mcode/") || uri.contains("\\mcode\\")
}

/// True when a diagnostic's location lives in a system-library source file.
///
/// Compiler fault tolerance: a bug inside an external library's `.mc` sources
/// is pre-existing library noise, not the user's circuit. The AI `check`
/// dry-run decides pass/fail from its diagnostic summary, so library
/// diagnostics must be excluded from that summary — a broken third-party
/// library must not block the user's project when the user's own code is
/// clean.
pub(crate) fn diag_in_system_lib(d: &Value) -> bool {
    let file = d
        .get("location")
        .and_then(|l| l.get("file"))
        .and_then(|f| f.as_str())
        .unwrap_or("");
    !file.is_empty() && diag_file_in_system_lib(file)
}

fn diag_file_in_system_lib(file: &str) -> bool {
    // In-memory overlay URIs (`/mcc/check.mc`, the AI's own content) are
    // never library files. `file://` and plain paths go to the path check;
    // any other URI scheme is treated as non-library.
    if let Some(path) = file.strip_prefix("file://") {
        return path_in_system_lib(path);
    }
    if !file.contains("://") {
        return path_in_system_lib(file);
    }
    false
}

fn path_in_system_lib(path: &str) -> bool {
    // Authoritative domain from the definition-space source manifest first.
    let uri = crate::McURI::from(path);
    match crate::definition_space().source_of(&uri) {
        Some(crate::db::defspace::SourceDomain::SystemLib(_)) => return true,
        Some(crate::db::defspace::SourceDomain::Project) => return false,
        None => {}
    }
    // Fallback: canonicalized on-disk library roots, then the legacy
    // `/mcode/` path marker (covers files loaded before the manifest existed).
    crate::db::infra::libmgr::file_is_system_library(std::path::Path::new(path))
        || is_system_uri(path)
}

pub(crate) fn refs_json(items: &[(String, String, [usize; 2])]) -> Vec<Value> {
    items
        .iter()
        .map(|(n, u, s)| json!({"name": n, "uri": u, "span": s}))
        .collect()
}

/// The library list a request implies — the caller's own list, or the global
/// `mcc.yaml [libs].load` configuration when they supplied none.
///
/// Split out from [`load_libs_rpc`] because a directory batch must reload the
/// libraries once per world and therefore needs the *names* as a value, not the
/// side effect of loading them.
///
/// mcode auto-loads by default in every mode unless disabled (mirrors the CLI's
/// `collect_libs`, manifest.rs). This matters after a `build.full` reset:
/// Phase 5 makes system libs per-world, so `clear_active` tombstones the whole
/// registry — a fresh world must re-establish mcode or its classes (e.g.
/// `enum PKG` in package.mc) go unresolved. The config check uses the global
/// config only, consistent with the `get_libs_load_list(None)` fallback (the
/// process project root is not reliably synced to the active workspace in
/// non-project mode).
pub(crate) fn resolve_libs_rpc(libs: &[String]) -> Vec<String> {
    let mut out: Vec<String> = if libs.is_empty() {
        crate::cli::config::get_libs_load_list(None).to_vec()
    } else {
        libs.to_vec()
    };
    if crate::cli::config::should_load_mcode(None) && !out.iter().any(|l| l == "mcode") {
        out.push("mcode".to_string());
    }
    out
}

pub(crate) fn load_libs_rpc(libs: &[String]) {
    // The loading half is the shared D6 loader (use-design §19.10): one loop,
    // dedup, and the same skip-if-loaded name loader the CLI uses. The
    // resolution half stays `resolve_libs_rpc` for now - its "explicit request
    // list replaces the config list" policy is deliberately not the CLI's
    // union; converging the two policies is D6 phase 2.
    crate::cli::loadctx::load_all(&crate::cli::loadctx::LoadContext::from_resolved(
        resolve_libs_rpc(libs),
    ));
}

/// The one virtual URI an inline AI dry-run is loaded under.
///
/// A dry-run is loaded, diagnosed and removed inside one handler, and
/// `RPC_STATE_LOCK` (protocol.rs) serializes handlers, so a request needs no
/// URI of its own. A per-request URI would instead consume a permanent
/// `UriId` each (`URI_TABLE` is append-only) and feed the process-lived
/// `DeclareId` ledger, making both depend on how many dry-runs this process
/// has served - build-design 3.7 discipline 0. `mcb_add_from_string` handles
/// a repeat of one URI by re-parsing it.
pub(crate) fn make_overlay_uri() -> McURI {
    McURI::from("/mcc/check.mc")
}

/// Remove a previously loaded overlay from the workspace.
/// Called after the AI check completes to prevent accumulation.
pub(crate) fn remove_overlay(uri: &McURI) {
    crate::build::loader::mcb_remove(uri);
}

// Refs

// ERC — Electrical Rule Check

/// Run the flat electrical net checks for the workspace's first module.
///
/// This is the RPC face of `mcc erc`. It used to carry its own root-net engine
/// (ERC 6001-6004) that also lacked the rail-aware driver rule the local face
/// had, so the two answered the same question differently; that engine is
/// retired (`erc/rules-catalog-design.md` §3.1 E3) and both faces now render
/// through `check::nets::erc_payload` over one `run_net_checks` result set.
pub(crate) fn run_erc() -> RpcResult {
    let top = crate::mcb_get_first_module_name()
        .ok_or_else(|| JsonRpcError::custom(32112, "semantic: no modules found"))?;

    // Resolve the top module to its defining file URI; using the bare module
    // name as a URI makes the build fail with "Target module not found".
    let uri = crate::mcb_iter_modules()
        .into_iter()
        .find(|(name, _)| name == &top)
        .map(|(_, u)| u.clone())
        .unwrap_or_else(|| top.clone());

    let entry = crate::McSpaceName {
        ident: crate::McIds::from(top.as_str()),
        uri: crate::uri_intern(&uri),
    };

    // The server must survive a Pass2 panic, as the CLI's guarded build does.
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcb_pass2_flat(&entry, 1)
    }))
    .map_err(|_| JsonRpcError::custom(32111, "semantic: build panicked"))?
    .map_err(|e| JsonRpcError::custom(32111, &format!("semantic: build failed: {e}")))?;

    let results = crate::check::nets::run_net_checks(&built.1);
    Ok(crate::check::nets::erc_payload(&top, &results))
}

// Auxiliary: parameter parsing / error handling

pub(crate) fn parse_strict<T: for<'de> Deserialize<'de>>(
    params: Option<Value>,
) -> Result<T, JsonRpcError> {
    let v = params.ok_or_else(JsonRpcError::invalid_params)?;
    serde_json::from_value(v).map_err(|_| JsonRpcError::invalid_params())
}

pub(crate) fn parse_or_default<T: for<'de> Deserialize<'de> + Default>(
    params: Option<Value>,
) -> Result<T, JsonRpcError> {
    match params {
        Some(v) => serde_json::from_value(v).map_err(|_| JsonRpcError::invalid_params()),
        None => Ok(T::default()),
    }
}

pub(crate) fn parse_string_param(
    params: Option<Value>,
    keys: &[&str],
) -> Result<String, JsonRpcError> {
    match params {
        Some(Value::String(s)) => Ok(s),
        Some(Value::Object(mut m)) => {
            for k in keys {
                if let Some(Value::String(s)) = m.remove(*k) {
                    return Ok(s);
                }
            }
            Err(JsonRpcError::invalid_params())
        }
        _ => Err(JsonRpcError::invalid_params()),
    }
}

pub(crate) fn io_err(e: std::io::Error) -> JsonRpcError {
    JsonRpcError::custom(32100, &format!("io error: {e}"))
}

// Auxiliary: file / path handling

pub(crate) fn is_safe_relative(p: &str) -> bool {
    use std::path::Component;
    let path = Path::new(p);
    if path.is_absolute() {
        return false;
    }
    for c in path.components() {
        match c {
            Component::ParentDir | Component::Prefix(_) | Component::RootDir => return false,
            _ => {}
        }
    }
    true
}

pub(crate) fn resolve_project_entry(
    _name: &str,
    entry: Option<&str>,
) -> Result<PathBuf, JsonRpcError> {
    let (_, _, root_str) = crate::workspace_info();
    let root = PathBuf::from(&root_str);
    let src_root = root.join("src");

    // Prefer src/ directory
    if let Some(rel) = entry {
        // Handle absolute paths directly
        let abs_path = PathBuf::from(rel);
        if abs_path.is_absolute() {
            if abs_path.exists() {
                return Ok(abs_path);
            } else {
                return Err(JsonRpcError::custom(
                    32105,
                    &format!("entry not found: {rel}"),
                ));
            }
        }

        // Relative path: check safety
        if !is_safe_relative(rel) {
            return Err(JsonRpcError::custom(
                32105,
                &format!("unsafe entry path: {rel}"),
            ));
        }
        // Search in src/
        let p = src_root.join(rel);
        if p.exists() {
            return Ok(p);
        }
        // Then in root
        let p = root.join(rel);
        if !p.exists() {
            return Err(JsonRpcError::custom(
                -32105,
                &format!("entry not found: {rel}"),
            ));
        }
        return Ok(p);
    }

    // Read entry from project.toml
    if let Some(rel) = read_project_entry_from_workspace() {
        // Search in src/
        let p = src_root.join(&rel);
        if p.exists() {
            return Ok(p);
        }
        // Then in root
        let p = root.join(&rel);
        if p.exists() {
            return Ok(p);
        }
    }

    // fallback: scan src/ for first .mc file
    let mut found = Vec::new();
    scan_mc_files_recursive(&src_root, &src_root, &mut found);
    if let Some(rel) = found.first() {
        return Ok(src_root.join(rel));
    }
    Err(JsonRpcError::custom(32105, "no .mc entry found in src/"))
}

pub(crate) fn scan_mc_files_recursive(root: &Path, current: &Path, out: &mut Vec<String>) {
    if let Ok(entries) = fs::read_dir(current) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                scan_mc_files_recursive(root, &p, out);
            } else if p.extension().is_some_and(|ext| ext == "mc") {
                if let Ok(rel) = p.strip_prefix(root) {
                    out.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }
}

pub(crate) fn read_project_entry_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let root = PathBuf::from(&root_str);
    let manifest = crate::cli::datadir::find_manifest_in(&root)?;
    let content = fs::read_to_string(&manifest).ok()?;
    parse_manifest_field(&content, "entry")
}

pub(crate) fn read_project_top_from_workspace() -> Option<String> {
    let (_, _, root_str) = crate::workspace_info();
    let root = PathBuf::from(&root_str);
    let manifest = crate::cli::datadir::find_manifest_in(&root)?;
    let content = fs::read_to_string(&manifest).ok()?;
    parse_manifest_field(&content, "top_module")
}

pub(crate) fn parse_manifest_field(content: &str, key: &str) -> Option<String> {
    // Simple TOML parser: support [project] section
    let mut in_project_section = false;

    for line in content.lines() {
        let line = line.trim();

        // Detect section
        if line.starts_with('[') && line.ends_with(']') {
            in_project_section = line.contains("project");
            continue;
        }

        // Search in project section
        if in_project_section && line.starts_with(key) {
            if let Some(eq) = line.find('=') {
                let v = line[eq + 1..].trim().trim_matches('"').trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

pub(crate) fn resolve_lib_root(name: &str) -> Result<PathBuf, JsonRpcError> {
    // Delegate to the single library-root resolver so the RPC/IDE path and the
    // CLI path agree: system root first (always the data root: MCC_SYSTEM_ROOT
    // env, then ~/.mcode), then data_root fallback. mcode resolves under
    // each root with a sibling fallback; other libraries match versioned
    // `<name>@<version>` then bare `<name>` (use-design §19.5 rule 2).
    crate::db::infra::libmgr::resolve_lib_root(name)
        .ok_or_else(|| JsonRpcError::custom(-32102, &format!("library '{name}' not installed")))
}

// Load handlers

// Parse handlers

// Show handlers

/// Resolve a file path to an absolute URI string for filtering.
pub(crate) fn resolve_to_abs_uri(file: &str) -> String {
    let path = std::path::Path::new(file);
    if let Ok(canonical) = path.canonicalize() {
        canonical.to_string_lossy().to_string()
    } else if path.is_absolute() {
        file.to_string()
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(path).to_string_lossy().to_string()
    } else {
        file.to_string()
    }
}

/// Filter (name, uri) pairs to only those that belong to the same project as
/// `file`. An item belongs if its URI equals the resolved file path, or is
/// under the same directory as the file (transitive `$include` files).
pub(crate) fn filter_items_by_file<T: Clone>(items: &[(T, String)], file: &str) -> Vec<T> {
    let target = resolve_to_abs_uri(file);
    let parent_dir = std::path::Path::new(&target)
        .parent()
        .map(|p| p.to_string_lossy().to_string());

    items
        .iter()
        .filter(|(_, uri)| {
            if uri == &target {
                return true;
            }
            if let Some(ref dir) = parent_dir {
                if uri.starts_with(dir) {
                    return true;
                }
            }
            false
        })
        .map(|(n, _)| n.clone())
        .collect()
}

// Show helpers (shared across drill-down handlers)

/// Find a definition by name across all four kinds.
pub(crate) fn find_def_by_name(name: &str) -> Option<(crate::McCMIE, String)> {
    crate::lsp::gotodef::find_def_by_name_raw(name)
}

/// Split a dot-qualified entity path into `(owner, member)`.
///
/// Used to reference funcs, which are nested inside a module/component and
/// therefore not top-level definitions: `main.setup` → `("main", "setup")`.
///
/// The owner is everything before the **last** dot, so dotted class names work
/// too: `comp.sub.i2c` → `("comp.sub", "i2c")`.
pub fn split_owner_member(name: &str) -> Option<(&str, &str)> {
    name.rsplit_once('.')
}

/// Resolve a dot-qualified function path `OWNER.FUNC` where OWNER is a loaded
/// Module or Component. Returns a clone of the function, or `None` when the
/// name is not a (owner, func) pair, the owner is not found, or the owner has
/// no such function.
pub fn find_func_by_path(name: &str) -> Option<crate::semantic::mc_func::McFunction> {
    let (owner, member) = split_owner_member(name)?;
    let (cmie, _) = find_def_by_name(owner)?;
    match &cmie {
        // Same declared face as the member-resolution and Pass2 dispatch
        // faces: own funcs first, then `::`-adopted recipe funcs
        // (`effective_method`'s fast path is the own-func lookup itself).
        crate::McCMIE::Component(c) => crate::db::defregistry::effective_method(c, member),
        crate::McCMIE::Module(m) => m.funcs.find(member).cloned(),
        _ => None,
    }
}

/// Build a phrase-level net map from a function body (no Pass2 — funcs depend
/// on parameters and a calling context that are not available standalone).
///
/// Each connection stmt in `func.stmts` becomes one net entry named `stmt_N`
/// (1-based), whose points are the endpoint names referenced by that stmt.
pub fn func_nets_map(func: &crate::semantic::mc_func::McFunction) -> BTreeMap<String, Vec<String>> {
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (i, stmt) in func.stmts.iter().enumerate() {
        let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
        crate::semantic::validation::body::collect_referenced_names(stmt, &mut names);
        let mut points: Vec<String> = names.into_iter().collect();
        points.sort();
        nets.insert(format!("stmt_{}", i + 1), points);
    }
    nets
}

/// Build the pin JSON view (pins + interfaces + name/id mappings). Single
/// implementation shared by the RPC handlers and the local CLI `show`
/// commands so both stay in parity.
pub fn pins_json(pins: &crate::McPins) -> Value {
    // Interface/group instances registered on this component, keyed by
    // instance name (e.g. "I2C0" → I2C0::I2C(Master)) or bare group name
    // ("PBus" → Bus). Built once and shared by the per-pin `interfaces` field
    // and the top-level `interfaces` summary.
    let mut ifaces: Vec<Value> = pins
        .names_to_id
        .iter()
        .filter_map(|(name, port)| match port {
            crate::McPinPort::Interface(iface) => {
                let mut param_strs: Vec<String> =
                    iface.params.iter().map(|p| p.to_string()).collect();
                let params: Value = if param_strs.is_empty() {
                    Value::Null
                } else {
                    // Drop placeholder "_" arguments so `I2C0::I2C(Master)`
                    // renders without trailing noise.
                    if param_strs.iter().all(|s| s == "_") {
                        Value::Null
                    } else {
                        param_strs.retain(|s| s != "_");
                        json!(param_strs)
                    }
                };
                // Pin names belonging to this interface (e.g. I2C0.SCL),
                // derived from the Single entries whose pin is registered.
                // A name whose prefix is another interface instance name
                // (e.g. I2C0.SCL on a GPIO-registered pin) is excluded.
                let iface_keys: std::collections::BTreeSet<String> = pins
                    .names_to_id
                    .iter()
                    .filter_map(|(n, port)| match port {
                        crate::McPinPort::Interface(_) => Some(n.clone()),
                        _ => None,
                    })
                    .collect();
                let mut pin_names: Vec<String> = pins
                    .names_to_id
                    .iter()
                    .filter_map(|(n, port)| match port {
                        crate::McPinPort::Single(pid) => {
                            let prefix = n.split('.').next();
                            let owned_by_other = prefix
                                .map(|p| iface_keys.contains(p) && p != name.as_str())
                                .unwrap_or(false);
                            if !owned_by_other && iface.registered_pins.iter().any(|p| p == pid) {
                                Some(n.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();
                pin_names.sort();
                Some(json!({
                    "name": name,
                    "inst_name": iface.name.to_string(),
                    "base_name": iface.base_name(),
                    "params": params,
                    "pins": iface.registered_pins,
                    "pin_names": pin_names,
                    "kind": "Interface",
                }))
            }
            crate::McPinPort::Bus(bus) => {
                // `PBus{CLK, DATA}`: a bus instance. Individual pins register
                // dot-qualified names (PBus.CLK), and the bare bus name
                // (PBus) is registered as the Bus port itself.
                let members: Vec<String> = bus.full_members.clone();
                let mut pin_names: Vec<String> = pins
                    .names_to_id
                    .iter()
                    .filter_map(|(n, port)| match port {
                        crate::McPinPort::Single(_) => {
                            if n.starts_with(&format!("{name}.")) {
                                Some(n.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();
                pin_names.sort();
                Some(json!({
                    "name": name,
                    "inst_name": name,
                    "base_name": name,
                    "params": Value::Null,
                    "pins": Value::Null,
                    "members": members,
                    "pin_names": pin_names,
                    "kind": "Bus",
                }))
            }
            _ => None,
        })
        .collect();

    // List groups (e.g. `PDM[CLK, DATA]`) are NOT registered in
    // `names_to_id` (§2.1 bare-prefix rule), so they are appended from the
    // display-only table recorded during pins parsing.
    for (list_name, members, pids) in &pins.list_groups {
        let mut pin_names: Vec<String> = pins
            .names_to_id
            .iter()
            .filter_map(|(n, port)| match port {
                crate::McPinPort::Single(pid) => {
                    if pids.contains(pid) && n.starts_with(list_name) {
                        Some(n.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();
        pin_names.sort();
        ifaces.push(json!({
            "name": list_name,
            "inst_name": list_name,
            "base_name": list_name,
            "params": Value::Null,
            "pins": Value::Null,
            "members": members,
            "pin_names": pin_names,
            "kind": "List",
        }));
    }

    // For each physical pin, the interface instances that occupy it.
    // Pins are listed in natural order (see `pin_id_cmp`) so numeric pin
    // IDs appear in numeric sequence instead of lexicographic order.
    let mut pin_entries: Vec<_> = pins.pins.iter().collect();
    pin_entries.sort_by(|(a, _), (b, _)| pin_id_cmp(a, b));
    let pin_list: Vec<Value> = pin_entries
        .into_iter()
        .map(|(pin_id, pin)| {
            let mut desc = String::new();
            for val in pin.values.iter() {
                if let crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) = val {
                    if !desc.is_empty() {
                        desc.push(' ');
                    }
                    desc.push_str(&s.value);
                }
            }
            // Non-string pin values (KVS like `volt:[...]`, ranges, ...),
            // rendered in full so dumps show everything the parser produced.
            let extra_vals: Vec<String> = pin
                .values
                .iter()
                .filter(|val| {
                    !matches!(
                        val,
                        crate::McAttrVal::AttrLiteral(crate::McLiteral::String(_))
                    )
                })
                .map(|val| val.to_string())
                .collect();
            // An interface/group belongs to this pin when it registers the
            // pin (e.g. I2C0 → [1,2]) or when the pin's name matches the
            // group's naming scheme: dot-qualified for interfaces and buses
            // (`I2C1.SCL` → I2C1, `PBus.CLK` → PBus), concatenated for List
            // groups (`PDMCLK` → PDM). The prefix fallback covers repeated
            // interface instance names where `registered_pins` only keeps
            // the last occurrence.
            let mut pin_ifaces: Vec<Value> = ifaces
                .iter()
                .filter(|i| {
                    let kind = i
                        .get("kind")
                        .and_then(|k| k.as_str())
                        .unwrap_or("Interface");
                    let key = i.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    match kind {
                        "List" => pin
                            .names
                            .iter()
                            .any(|n| n.starts_with(key) && n.len() > key.len()),
                        "Bus" => pin.names.iter().any(|n| n.split('.').next() == Some(key)),
                        _ => {
                            let via_reg = i
                                .get("pins")
                                .and_then(|p| p.as_array())
                                .map(|arr| arr.iter().any(|p| p == pin_id))
                                .unwrap_or(false);
                            let via_prefix =
                                pin.names.iter().any(|n| n.split('.').next() == Some(key));
                            via_reg || via_prefix
                        }
                    }
                })
                .map(|i| {
                    json!({
                        "name": i["inst_name"],
                        "base": i["base_name"],
                        "params": i["params"],
                        "kind": i["kind"],
                        "members": i["members"],
                    })
                })
                .collect();
            // Reorder `|` alternates to match the source declaration order
            // recorded in `pin_iface_order` (instead of the alphabetical
            // `names_to_id` order); entries without a recorded position keep
            // their relative order at the end.
            if let Some(order) = pins.pin_iface_order.get(pin_id) {
                pin_ifaces.sort_by_key(|i| {
                    let name = i.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    order.iter().position(|k| k == name).unwrap_or(usize::MAX)
                });
            }
            let mut j = json!({
                "id": pin_id,
                "iotype": format!("{:?}", pin.iotype),
                "names": pin.names,
                "interfaces": pin_ifaces,
            });
            if !desc.is_empty() {
                j["description"] = json!(desc);
            }
            if !extra_vals.is_empty() {
                j["values"] = json!(extra_vals);
            }
            // The pin's identity attributes — the trailing `@attr…` words its
            // rows carried (U43), first declaration of a key winning, as
            // `attach_row_attrs` defines. Declaration to this field alone
            // deduplicates, so a key written twice in one row shows once here
            // and is reported by 5359 instead.
            if !pin.attrs.is_empty() {
                j["attrs"] = json!(pin.attrs.iter().map(|a| a.to_string()).collect::<Vec<_>>());
            }
            j
        })
        .collect();

    let mut names_to_id = serde_json::Map::new();
    for (k, v) in &pins.names_to_id {
        names_to_id.insert(k.clone(), pinport_json(v));
    }
    let mut pin_id_to_names = serde_json::Map::new();
    for (k, v) in &pins.pin_id_to_names {
        pin_id_to_names.insert(k.clone(), json!(v));
    }

    json!({
        "pin_count": pins.pins.len(),
        "pins": pin_list,
        "interfaces": ifaces,
        "names_to_id": Value::Object(names_to_id),
        "pin_id_to_names": Value::Object(pin_id_to_names),
    })
}

pub(crate) fn pinport_json(v: &crate::McPinPort) -> Value {
    match v {
        crate::McPinPort::Single(pid) => json!({ "kind": "Single", "pin": pid }),
        crate::McPinPort::Multi(pids) => json!({ "kind": "Multi", "pins": pids }),
        crate::McPinPort::MultiGroup(groups) => {
            json!({ "kind": "MultiGroup", "groups": groups })
        }
        crate::McPinPort::List(name, items) => {
            json!({ "kind": "List", "name": name, "items": items })
        }
        crate::McPinPort::Bus(bus) => json!({ "kind": "Bus", "debug": format!("{:?}", bus) }),
        crate::McPinPort::Interface(iface) => json!({
            "kind": "Interface",
            "inst_name": iface.name.to_string(),
            "base_name": iface.base_name(),
            "registered_pins": iface.registered_pins,
        }),
        crate::McPinPort::NC => json!({ "kind": "NC" }),
        crate::McPinPort::Anon => json!({ "kind": "Anon" }),
    }
}

pub(crate) fn inst_kind_class(inst: &crate::McInstance) -> (&'static str, String) {
    match inst {
        crate::McInstance::Component(c) => ("component", c.base.name.to_string()),
        crate::McInstance::Module(m) => ("module", m.base.name.to_string()),
        crate::McInstance::Label(l) => ("label", l.clone()),
        crate::McInstance::Interface(i) => ("interface", i.base_name()),
        crate::McInstance::Bus(b) => ("bus", b.to_string()),
        crate::McInstance::BusRef { component, bus } => ("busref", format!("{component}.{bus}")),
        crate::McInstance::List(l) => {
            let name = l.name().to_string();
            let class = format!("{:?}", l);
            if class != name {
                ("list", class)
            } else {
                ("list", name)
            }
        }
        crate::McInstance::Unresolved { class_name } => ("unresolved", class_name.clone()),
        crate::McInstance::Pins => ("pins", "pins".into()),
        crate::McInstance::PinId(id) => ("pinid", id.clone()),
        crate::McInstance::Attr(a) => ("attr", a.to_string()),
        crate::McInstance::Func(f) => ("func", f.name.to_string()),
        crate::McInstance::EnumVal {
            enum_name,
            value_name,
            ..
        } => ("enumval", format!("{}.{}", enum_name, value_name)),
    }
}

pub(crate) fn attrval_json(v: &crate::McAttrVal) -> Value {
    match v {
        // Keep string literals quoted so the dump shows the source form.
        crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) => {
            json!(format!("\"{}\"", s.value))
        }
        other => json!(other.to_string()),
    }
}

// Show — missing container handlers

// Show — drill-down handlers

/// Convert a McParamDeclare to a JSON object with smart parameter metadata.
pub fn param_declare_to_json(d: &crate::semantic::basic::mc_paramd::McParamDeclare) -> Value {
    let name = d.get_primary_name().unwrap_or_default();
    let is_port = d.is_port();
    let has_default = d.has_default_value();
    let default_val = d.param_type.default_value().map(|s| s.to_string());
    let class_name = d.get_class_name();
    let iface_params: Vec<String> = d.param_type.interface_params().to_vec();
    json!({
        "name": name,
        "type": d.param_type.category_name(),
        "is_port": is_port,
        "has_default": has_default,
        "default": default_val,
        "class": class_name,
        "params": iface_params,
    })
}

// JSON builders for each entity kind (used by handle_show_dump and handle_show_dump_all)
pub(crate) fn dump_component_json(name: &str, comp: &crate::McComponent, uri: &str) -> Value {
    let params: Vec<Value> = comp.params.names_full().iter().map(|n| json!(n)).collect();
    let params_with_defaults: Vec<Value> = comp
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();
    let attrs: Vec<Value> = comp
        .attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({"no": a.no, "name": a.id.to_string(), "values": values})
        })
        .collect();
    let funcs: Vec<Value> = comp
        .funcs
        .iter()
        .map(|f| {
            let body_stmts: Vec<String> = f.body_stmts_display();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full_annotated(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_stmts": body_stmts,
            })
        })
        .collect();
    let instances: Vec<Value> = instances_json(&comp.insts, None);
    let layout = json!({
        "left": comp.layout.left,
        "right": comp.layout.right,
        "top": comp.layout.top,
        "bottom": comp.layout.bottom,
    });
    let cond_pins: Vec<String> = comp
        .cond_pins
        .iter()
        .map(|cp| format!("{:?}", cp))
        .collect();
    let cond_attrs: Vec<String> = comp
        .cond_attrs
        .iter()
        .map(|ca| format!("{:?}", ca))
        .collect();

    let mut data = pins_json(&comp.pins);
    data["name"] = json!(name);
    data["kind"] = json!("component");
    data["uri"] = json!(uri);
    data["span"] = json!({"start": comp.span.start, "end": comp.span.end});
    data["params"] = json!(params);
    data["params_with_defaults"] = json!(params_with_defaults);
    data["attrs"] = json!(attrs);
    data["funcs"] = json!(funcs);
    data["instances"] = json!(instances);
    data["layout"] = layout;
    data["cond_pins_count"] = json!(comp.cond_pins.len());
    data["cond_pins"] = json!(cond_pins);
    data["cond_attrs_count"] = json!(comp.cond_attrs.len());
    data["cond_attrs"] = json!(cond_attrs);
    data
}

pub(crate) fn dump_module_json(name: &str, module: &crate::McModule, uri: &str) -> Value {
    // Params. Interface-bound params keep their binding: `[VDD,GND]::DC(3.3V)`
    // → `{"name":"[VDD, GND]","iface":"DC","iface_params":["3.3V"]}`.
    let params: Vec<Value> = module
        .params
        .iter()
        .map(|d| {
            let display = json!(d.display_name());
            match d.interface_annotation() {
                Some((class, p)) => json!({
                    "name": d.display_name(),
                    "iface": class,
                    "iface_params": p,
                }),
                None => display,
            }
        })
        .collect();
    let params_with_defaults: Vec<Value> = module
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();
    let instances: Vec<Value> = instances_json(&module.insts, None);
    let stmts: Vec<String> = module.stmts.iter().map(|l| l.to_string()).collect();
    let funcs: Vec<Value> = module
        .funcs
        .iter()
        .map(|f| {
            let body_stmts: Vec<String> = f.body_stmts_display();
            json!({
                "name": f.name.to_string(),
                "params": f.params.names_full_annotated(),
                "returns": f.returns.kind_str(),
                "called_time": f.called_time,
                "body_stmts": body_stmts,
            })
        })
        .collect();
    json!({
        "name": name,
        "kind": "module",
        "uri": uri,
        "span": {"start": module.span.start, "end": module.span.end},
        "params": params,
        "params_with_defaults": params_with_defaults,
        "instances": instances,
        "stmts_count": module.stmts.len(),
        "stmts": stmts,
        "funcs": funcs,
    })
}

pub(crate) fn dump_interface_json(name: &str, iface: &crate::McInterface, uri: &str) -> Value {
    let params: Vec<Value> = iface.params.names_full().iter().map(|n| json!(n)).collect();
    let params_with_defaults: Vec<Value> = iface
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();
    let attrs: Vec<Value> = iface
        .attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({"no": a.no, "name": a.id.to_string(), "values": values})
        })
        .collect();
    let roles: Vec<Value> = iface
        .roles
        .iter()
        .map(|r| json!({"name": r.name.to_string(), "pins": pins_json(&r.pins)}))
        .collect();

    let mut data = pins_json(&iface.pins);
    data["name"] = json!(name);
    data["kind"] = json!("interface");
    data["uri"] = json!(uri);
    data["params"] = json!(params);
    data["params_with_defaults"] = json!(params_with_defaults);
    data["attrs"] = json!(attrs);
    data["roles"] = json!(roles);
    data["span"] = json!({"start": iface.span.start, "end": iface.span.end});
    data
}

pub(crate) fn dump_enum_json(name: &str, en: &crate::McEnumDef, uri: &str) -> Value {
    let values: Vec<Value> = en
        .values
        .iter()
        .map(|v| json!({"name": v.name.to_string(), "span": [v.span[0], v.span[1]]}))
        .collect();
    json!({
        "name": name,
        "kind": "enum",
        "uri": uri,
        "span": [en.span[0], en.span[1]],
        "value_count": values.len(),
        "values": values,
    })
}

// Helper: serialize instances (mirrors instances_json in show.rs)
pub(crate) fn instances_json(insts: &crate::McInstances, type_filter: Option<&str>) -> Vec<Value> {
    let port_spans = insts.port_spans();
    insts
        .iter_in_decl_order()
        .filter_map(|(n, inst)| {
            let (kind, class) = inst_kind_class(inst);
            let kind = if kind == "label" {
                match insts.get_label_kind(n) {
                    crate::LabelKind::Inline => "ilabel",
                    crate::LabelKind::Explicit => "label",
                }
            } else {
                kind
            };
            if let Some(t) = type_filter {
                if !kind.eq_ignore_ascii_case(t) {
                    return None;
                }
            }
            let span = port_spans
                .get(n)
                .and_then(|v| v.first())
                .map(|r| json!({"start": r.start, "end": r.end}));
            // Module port direction (`io`/`out`/`in`), empty for non-port
            // instances (components, modules, inline net labels).
            let io = match insts.insts().get(n) {
                Some((crate::IOType::InOut, _)) => "io",
                Some((crate::IOType::Out, _)) => "out",
                Some((crate::IOType::In, _)) => "in",
                _ => "",
            };
            let mut entry = json!({"name": n.to_string(), "io": io, "kind": kind, "class": class});
            if let Some(s) = span {
                entry["span"] = s;
            }
            Some(entry)
        })
        .collect()
}

// Semantic data (sem tokens + symbols) for LSP

/// Make the file's own project root the active workspace before any of its
/// definitions enter the tables. `load_project` / `add_file` name a file, not
/// a project: without this gate a file from a sibling project joins the active
/// world, and its duplicate names read as cross-project E5001 shadows.
/// Returns the resolved project root.
pub(crate) fn switch_to_file_workspace(file_path: &Path) -> PathBuf {
    let project_root = find_project_root(file_path);
    info!(target: "crate::rpc", "auto_load: project_root={}", project_root.display());

    // Whether this file's root is the workspace we are already in is one
    // question, answered in one place. Asking it here as well — comparing roots
    // while the callee compared directory *names* — is how two projects sharing
    // a basename ended up in one definition table. A repeat call for the active
    // root is a no-op, so the common path still costs no snapshot.
    if crate::workspace_switch_to(Some(project_root.clone()), crate::WorkspaceKind::Project) {
        info!(target: "crate::rpc", "auto_load: switched to workspace root={}", project_root.display());
    } else {
        info!(target: "crate::rpc", "auto_load: reusing active workspace root={}", project_root.display());
    }
    project_root
}

/// The path behind a `load_project` / `add_file` parameter: accept a plain path
/// or a `file://` URI; a relative path resolves against the server's cwd.
pub(crate) fn file_path_from_uri_param(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Some(path);
    }
    std::env::current_dir().ok().map(|cwd| cwd.join(path))
}

/// Load the project for a file that is not yet in the active workspace.
///
/// Non-project mode: the workspace root is the configured project root (the
/// folder opened in the editor). Only the opened file plus its `use` closure
/// is loaded; sibling files are intentionally NOT added, so each file is
/// parsed in its own semantic scope without bare-name pollution from
/// unrelated definitions.
pub(crate) fn auto_load_from_file_path(file_path: &Path) {
    let project_root = switch_to_file_workspace(file_path);

    // Load library dependencies from project.toml before parsing
    let file_uri = McURI::from(file_path.to_string_lossy().to_string());
    ensure_library_loaded(&file_uri);

    // Force-load mcode: mcb_init_system_lib may have registered an empty placeholder.
    // Reload from the real mcode directory to ensure library components are
    // available. Respect libs.disable_mcode so the switch applies here too.
    if crate::cli::config::should_load_mcode(Some(&project_root)) {
        if let Ok(mcode_root) = resolve_lib_root("mcode") {
            crate::db::infra::libmgr::mcb_load_lib("mcode", &mcode_root);
        }
    }

    // Load only the entry file itself (plus its use closure via mcc_load_project).
    // The entry is the opened file, not the first file of a directory scan: the
    // former sibling loop is removed, so a directory scan would be wrong here.
    let uri = McURI::from(file_path.to_string_lossy().to_string());
    info!(target: "crate::rpc", "auto_load: mcc_load_project({})", uri);
    crate::mcc_load_project(&uri);
}

/// Walk up from a file path to find the project root
/// A project root is a directory containing a project manifest
/// (project.toml) or .mc files at top level
pub(crate) fn find_project_root(file_path: &Path) -> PathBuf {
    // Priority 1: the configured project root (the folder opened in the editor,
    // set via mcext set_project_root) — for the files under it. In non-project
    // mode every .mc file under the opened folder is a peer, so the workspace
    // root is the folder itself. It is the editor's working root, not a claim on
    // files elsewhere: claiming them put a sibling repository's definitions in
    // the open project's world, and two projects sharing a directory name then
    // reported E5001 against each other. A file from outside gets its own root
    // from the walk-up below. No upward search for a nested manifest:
    // sub-projects are handled as plain files (folder-parse-design.md §2.6).
    let configured = crate::db::infra::init::mcb_get_project_root();
    if configured.is_absolute()
        && !configured.as_os_str().is_empty()
        && path_is_under(file_path, &configured)
    {
        return configured;
    }

    let mut current = if file_path.is_dir() {
        file_path.to_path_buf()
    } else {
        file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    };

    // First pass: walk up looking for a project manifest (same name as CLI
    // project-root discovery).
    let mut probe = current.clone();
    let mut toml_dir: Option<PathBuf> = None;
    loop {
        if crate::cli::datadir::find_manifest_in(&probe).is_some() {
            toml_dir = Some(probe.clone());
            break;
        }
        if let Some(parent) = probe.parent() {
            probe = parent.to_path_buf();
        } else {
            break;
        }
    }
    // If a manifest is found, use that directory
    if let Some(dir) = toml_dir {
        return dir;
    }
    // Fallback: first directory with .mc files
    loop {
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "mc") {
                    return current;
                }
            }
        }
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            break;
        }
    }
    // Fallback: use the file's parent directory
    file_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Whether `path` lies inside `root`.
///
/// Compared as written first (the editor hands both the folder and the file
/// paths out of the same source, so they agree), then in canonical form, so a
/// root reached through a symlink still contains a file reached directly —
/// `/var/x` and `/private/var/x` name the same directory.
fn path_is_under(path: &Path, root: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }
    let Ok(canonical) = path.canonicalize() else {
        return false;
    };
    if canonical.starts_with(root) {
        return true;
    }
    root.canonicalize()
        .map_or(false, |root| canonical.starts_with(root))
}

/// Ensure library dependencies are loaded for a file.
/// This is called when parsing files with content from LSP to ensure
/// the library context is available for type lookups.
pub(crate) fn ensure_library_loaded(file_uri: &McURI) {
    let libs = crate::db::infra::libmgr::mcb_loaded_libs();
    tracing::debug!(
        target: "mcc::lib",
        uri = %file_uri,
        loaded = ?libs,
        "ensure_library_loaded: start"
    );

    if !libs.is_empty() {
        return;
    }

    let path = Path::new(file_uri.as_str());
    let project_root = find_project_root(path);

    // Manifest dependencies load through the shared D6 context (use-design
    // §19.10 phase 2): `Manifest::load` is the one toml reader and
    // `load_all` the one loading loop - no hand-rolled section parse here.
    if let Some(manifest_path) = crate::cli::datadir::find_manifest_in(&project_root) {
        if let Ok(manifest) = crate::cli::manifest::Manifest::load(&manifest_path) {
            let ctx = crate::cli::loadctx::LoadContext {
                deps: manifest.dependencies.keys().cloned().collect(),
                ..crate::cli::loadctx::LoadContext::default()
            };
            tracing::debug!(target: "mcc::lib", deps = ?ctx.deps, "loading dependencies");
            crate::cli::loadctx::load_all(&ctx);
        }
    }
}

/// Classify a token using the symbol table.
/// Overrides lexer type for identifiers that have semantic classification.

/// Try to find semantic data for any of the candidate URIs

// Def

/// Handle def RPC — go-to-definition for a symbol.

// Recipes

/// Handle recipes RPC — self-describing API for AI discovery.

// Unified Lookup (F12/pass1-pass2)

/// Lookup a sub-element (pin, port, param, label) within a parent container.

/// Combined lookup: find class + optionally look up sub-element.
/// Supports compound identifiers like `uC.PA1` — finds `uC` then `PA1` within it.

/// Enumerate all visible symbols at a given scope.

// Explain

/// Handle explain RPC — look up error code descriptions.

/// Handle diagnostics RPC - return parse/semantic diagnostics for a file

/// Handle project_symbols RPC - return project-wide symbols (components, interfaces, enums,
/// modules, enum_values)

/// Handle set_project_root RPC - set project root path

/// Handle set_system_root RPC - set system root path (for library resolution)

/// Handle init RPC - initialize mcc system

/// Handle load_project RPC - load entire project

/// Handle add_file RPC - add a single file to project

/// Handle remove_file RPC - remove a file from project
// ── Sub-module declarations ──
mod admin;
mod aicontract;
mod buildcmd;
mod defs;
mod exportcmd;
mod impact;
mod import;
mod libcmd;
mod lsp;
mod rulescmd;
mod show;

pub use admin::*;
pub use aicontract::*;
pub use buildcmd::*;
pub use defs::*;
pub use exportcmd::*;
pub use impact::*;
pub use import::*;
pub use libcmd::*;
pub use lsp::*;
pub use rulescmd::*;
pub use show::*;

// ── Phase 8.3: Method registry (single source of truth for caps) ──

/// Metadata for one RPC method.
pub struct MethodMeta {
    pub name: &'static str,
    pub consumer: &'static str, // "lsp" | "ai" | "cli" | "admin"
}

/// Registry of all RPC methods. Single source of truth for caps + register_all.
pub static METHODS: &[MethodMeta] = &[
    MethodMeta {
        name: "server.info",
        consumer: "admin",
    },
    MethodMeta {
        name: "server.methods",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.list",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.info",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.load",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.unload",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.install",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.uninstall",
        consumer: "admin",
    },
    MethodMeta {
        name: "lib.search",
        consumer: "admin",
    },
    MethodMeta {
        name: "trace.set",
        consumer: "admin",
    },
    MethodMeta {
        name: "trace.get",
        consumer: "admin",
    },
    MethodMeta {
        name: "build.full",
        consumer: "cli",
    },
    MethodMeta {
        name: "build.viz",
        consumer: "cli",
    },
    MethodMeta {
        name: "parse",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.component",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.module",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.interface",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.net",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.all",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.file",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.files",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.enum",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.pins",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.ports",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.labels",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.instances",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.org-units",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.diagnostics",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.netlist",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.project",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.core-erc",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.expectation",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.nets",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.attrs",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.funcs",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.params",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.roles",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.values",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.dump",
        consumer: "cli",
    },
    MethodMeta {
        name: "check",
        consumer: "ai",
    },
    MethodMeta {
        name: "defs.search",
        consumer: "cli",
    },
    MethodMeta {
        name: "defs.query",
        consumer: "cli",
    },
    MethodMeta {
        name: "defs.checkpoint",
        consumer: "admin",
    },
    MethodMeta {
        name: "defs.diff",
        consumer: "admin",
    },
    MethodMeta {
        name: "defs.dependents",
        consumer: "admin",
    },
    MethodMeta {
        name: "defs.reverse",
        consumer: "lsp",
    },
    MethodMeta {
        name: "export",
        consumer: "cli",
    },
    MethodMeta {
        name: "impact",
        consumer: "cli",
    },
    MethodMeta {
        name: "import",
        consumer: "cli",
    },
    MethodMeta {
        name: "sem",
        consumer: "lsp",
    },
    MethodMeta {
        name: "explain",
        consumer: "ai",
    },
    MethodMeta {
        name: "def",
        consumer: "cli",
    },
    MethodMeta {
        name: "erc",
        consumer: "cli",
    },
    MethodMeta {
        name: "refs",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup_sub",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup_with_sub",
        consumer: "cli",
    },
    MethodMeta {
        name: "lookup_all",
        consumer: "cli",
    },
    MethodMeta {
        name: "rules.list",
        consumer: "ai",
    },
    MethodMeta {
        name: "rule.detail",
        consumer: "ai",
    },
    MethodMeta {
        name: "severity.set",
        consumer: "ai",
    },
    MethodMeta {
        name: "allow.add",
        consumer: "ai",
    },
    MethodMeta {
        name: "accept",
        consumer: "ai",
    },
    MethodMeta {
        name: "caps",
        consumer: "ai",
    },
    MethodMeta {
        name: "diagnostics",
        consumer: "lsp",
    },
    MethodMeta {
        name: "project_symbols",
        consumer: "lsp",
    },
    MethodMeta {
        name: "set_project_root",
        consumer: "admin",
    },
    MethodMeta {
        name: "set_system_root",
        consumer: "admin",
    },
    MethodMeta {
        name: "init",
        consumer: "lsp",
    },
    MethodMeta {
        name: "load_project",
        consumer: "admin",
    },
    MethodMeta {
        name: "add_file",
        consumer: "lsp",
    },
    MethodMeta {
        name: "remove_file",
        consumer: "lsp",
    },
    MethodMeta {
        name: "show.component.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.module.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.interface.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.net.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.enum.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.ports.list",
        consumer: "cli",
    },
    MethodMeta {
        name: "show.dump.all",
        consumer: "cli",
    },
    MethodMeta {
        name: "completion",
        consumer: "lsp",
    },
    MethodMeta {
        name: "hover",
        consumer: "lsp",
    },
];

/// Generate caps JSON from the method registry.
pub fn caps_json() -> serde_json::Value {
    use serde_json::json;

    let names: Vec<&str> = METHODS.iter().map(|m| m.name).collect();
    let ai_methods: Vec<&str> = METHODS
        .iter()
        .filter(|m| m.consumer == "ai")
        .map(|m| m.name)
        .collect();

    json!({
        "server": "mcc",
        "version": crate::buildinfo::VERSION,
        "build": crate::buildinfo::number(),
        "schema_version": 1,
        "methods": names,
        "features": {
            "diagnostics": {
                "byte_range": false,
                "end_line": true,
                "end_column": true,
                "suggestions": true,
                "related": true
            },
            "explain": true,
            "search": true,
            "query": true,
            "rules": crate::override_store::rules_summary_json(),
            // Derived from the one kind table, so a new export product cannot
            // be advertised here without existing (CIMP U18 item 4).
            "export": crate::cli::ExportKind::ALL
                .iter()
                .map(|k| k.name())
                .collect::<Vec<_>>(),
            "ai": {
                "methods": ai_methods,
                "overlay_dry_run": true,
            },
            "trace": {
                "targets": crate::cli::config::get_known_debug_targets(),
                "aliases": crate::cli::config::get_debug_aliases().into_iter().map(|(name, targets)| {
                    json!({"name": name, "targets": targets})
                }).collect::<Vec<_>>()
            }
        }
    })
}

/// Register all handlers on a server builder (single source of truth).
/// Called from `cmds/server.rs`.
pub fn register_all(
    mut builder: crate::rpc::server::RpcServerBuilder,
) -> crate::rpc::server::RpcServerBuilder {
    // Admin
    builder = builder.register_method("server.info", handle_server_info);
    builder = builder.register_method("server.methods", handle_methods);
    // Lib
    builder = builder.register_method("lib.list", handle_library_list);
    builder = builder.register_method("lib.info", handle_library_show);
    builder = builder.register_method("lib.load", handle_lib_load);
    builder = builder.register_method("lib.unload", handle_lib_unload);
    builder = builder.register_method("lib.install", handle_lib_install);
    builder = builder.register_method("lib.uninstall", handle_lib_uninstall);
    builder = builder.register_method("lib.search", handle_lib_search);
    builder = builder.register_method("trace.set", handle_trace_set);
    builder = builder.register_method("trace.get", handle_trace_get);
    // Build
    builder = builder.register_method("build.full", handle_build_full);
    builder = builder.register_method("build.viz", handle_build_viz);
    builder = builder.register_method("parse", handle_parse);
    // Show — lists
    builder = builder.register_method("show.component", handle_show_component);
    builder = builder.register_method("show.component.list", handle_show_component_list);
    builder = builder.register_method("show.module", handle_show_module);
    builder = builder.register_method("show.module.list", handle_show_module_list);
    builder = builder.register_method("show.interface", handle_show_interface);
    builder = builder.register_method("show.interface.list", handle_show_interface_list);
    builder = builder.register_method("show.net", handle_show_net);
    builder = builder.register_method("show.net.list", handle_show_net_list);
    builder = builder.register_method("show.all", handle_show_all);
    builder = builder.register_method("show.org-units", handle_show_org_units);
    builder = builder.register_method("show.diagnostics", handle_show_diagnostics);
    builder = builder.register_method("show.netlist", handle_show_netlist);
    builder = builder.register_method("show.project", handle_show_project);
    builder = builder.register_method("show.core-erc", handle_show_core_erc);
    builder = builder.register_method("show.expectation", handle_show_expectation);
    builder = builder.register_method("show.file", handle_show_file);
    builder = builder.register_method("show.files", handle_show_files);
    builder = builder.register_method("show.enum", handle_show_enum);
    builder = builder.register_method("show.enum.list", handle_show_enum_list);
    // Show — drill-down
    builder = builder.register_method("show.pins", handle_show_pins);
    builder = builder.register_method("show.ports", handle_show_ports);
    builder = builder.register_method("show.ports.list", handle_show_ports_list);
    builder = builder.register_method("show.labels", handle_show_labels);
    builder = builder.register_method("show.instances", handle_show_instances);
    builder = builder.register_method("show.nets", handle_show_nets);
    builder = builder.register_method("show.attrs", handle_show_attrs);
    builder = builder.register_method("show.funcs", handle_show_funcs);
    builder = builder.register_method("show.params", handle_show_params);
    builder = builder.register_method("show.roles", handle_show_roles);
    builder = builder.register_method("show.values", handle_show_values);
    builder = builder.register_method("show.dump", handle_show_dump);
    builder = builder.register_method("show.dump.all", handle_show_dump_all);
    // AI
    builder = builder.register_method("check", handle_check);
    // Defs
    builder = builder.register_method("defs.search", handle_defs_search);
    builder = builder.register_method("defs.query", handle_defs_query);
    builder = builder.register_method("defs.checkpoint", handle_defs_checkpoint);
    builder = builder.register_method("defs.diff", handle_defs_diff);
    builder = builder.register_method("defs.dependents", handle_defs_dependents);
    builder = builder.register_method("defs.relations", handle_defs_relations);
    builder = builder.register_method("defs.reverse", handle_defs_reverse);
    builder = builder.register_method("export", handle_export);
    builder = builder.register_method("impact", handle_impact);
    builder = builder.register_method("import", handle_import);
    // LSP
    builder = builder.register_method("sem", handle_sem);
    builder = builder.register_method("explain", handle_explain);
    builder = builder.register_method("rules.list", handle_rules_list);
    builder = builder.register_method("rule.detail", handle_rule_detail);
    builder = builder.register_method("severity.set", handle_severity_set);
    builder = builder.register_method("allow.add", handle_allow_add);
    builder = builder.register_method("accept", handle_accept);
    builder = builder.register_method("def", handle_def);
    builder = builder.register_method("erc", handle_erc);
    builder = builder.register_method("refs", handle_refs);
    builder = builder.register_method("lookup", handle_lookup);
    builder = builder.register_method("lookup_sub", handle_lookup_sub);
    builder = builder.register_method("lookup_with_sub", handle_lookup_with_sub);
    builder = builder.register_method("lookup_all", handle_lookup_all);
    builder = builder.register_method("caps", handle_caps);
    builder = builder.register_method("diagnostics", handle_diagnostics);
    builder = builder.register_method("project_symbols", handle_project_symbols);
    builder = builder.register_method("set_project_root", handle_set_project_root);
    builder = builder.register_method("set_system_root", handle_set_system_root);
    builder = builder.register_method("init", handle_init);
    builder = builder.register_method("load_project", handle_load_project);
    builder = builder.register_method("add_file", handle_add_file);
    builder = builder.register_method("remove_file", handle_remove_file);
    builder = builder.register_method("completion", handle_completion);
    builder = builder.register_method("hover", handle_hover);
    builder = builder.register_method("gotodef", handle_gotodef);
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Every variant maps to its own wire string. Locks each arm individually:
    /// when the iotype `label` keyword retired (b3892) the stale `Label` arm
    /// here degraded into a catch-all binding and every iotype reported
    /// "label" on the RPC face.
    #[test]
    fn cli_rpc__iotype_str_covers_every_variant() {
        use crate::IOType;
        assert_eq!(iotype_str(&IOType::In), "in");
        assert_eq!(iotype_str(&IOType::Out), "out");
        assert_eq!(iotype_str(&IOType::InOut), "inout");
        assert_eq!(iotype_str(&IOType::Power), "power");
        assert_eq!(iotype_str(&IOType::Return), "return");
        assert_eq!(iotype_str(&IOType::NonCon), "noncon");
        assert_eq!(iotype_str(&IOType::None), "none");
    }

    #[test]
    fn cli_rpc__find_project_root_prefers_configured_root() {
        // Save and restore the global project root so this test does not leak
        // state into other tests running in the same process.
        let saved = crate::db::infra::init::mcb_get_project_root();

        let tmp = std::env::temp_dir().join(format!("mcc-root-test-{}", std::process::id()));
        let sub = tmp.join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("x.mc");
        fs::write(&file, "").unwrap();

        // For a file under it, the configured root wins wherever it sits in the
        // tree: the opened folder is the single workspace root in non-project
        // mode.
        crate::db::infra::init::mcb_set_project_root(&tmp);
        assert_eq!(find_project_root(&file), tmp);

        // Empty configured root falls back to the original walk-up logic:
        // the first directory containing .mc files (here: the file's parent).
        crate::db::infra::init::mcb_set_project_root(std::path::Path::new(""));
        assert_eq!(find_project_root(&file), sub);

        fs::remove_dir_all(&tmp).unwrap();
        crate::db::infra::init::mcb_set_project_root(&saved);
    }

    /// The configured root is the editor's working root, not a claim on every
    /// file the editor opens. A file from a *sibling* folder keeps its own
    /// root; otherwise its definitions join the open project's world, and two
    /// projects that share a directory name report E5001 against each other
    /// (a real board and a frozen copy of it under `tests/fixtures/`).
    #[test]
    fn cli_rpc__find_project_root_does_not_claim_files_outside_it() {
        let saved = crate::db::infra::init::mcb_get_project_root();

        let open = std::env::temp_dir().join(format!("mcc-root-open-{}", std::process::id()));
        let open_src = open.join("src");
        fs::create_dir_all(&open_src).unwrap();
        fs::write(open_src.join("a.mc"), "").unwrap();

        let sibling = std::env::temp_dir().join(format!("mcc-root-sibling-{}", std::process::id()));
        let sibling_src = sibling.join("src");
        fs::create_dir_all(&sibling_src).unwrap();
        let outside = sibling_src.join("b.mc");
        fs::write(&outside, "").unwrap();

        crate::db::infra::init::mcb_set_project_root(&open);
        assert_eq!(find_project_root(&open_src.join("a.mc")), open);
        assert_eq!(find_project_root(&outside), sibling_src);

        fs::remove_dir_all(&open).unwrap();
        fs::remove_dir_all(&sibling).unwrap();
        crate::db::infra::init::mcb_set_project_root(&saved);
    }

    #[test]
    fn cli_rpc__find_project_root_detects_project_manifest() {
        let saved = crate::db::infra::init::mcb_get_project_root();
        crate::db::infra::init::mcb_set_project_root(std::path::Path::new(""));

        let tmp = std::env::temp_dir().join(format!("mcc-root-mf-{}", std::process::id()));
        let sub = tmp.join("src");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("x.mc");
        fs::write(&file, "").unwrap();

        // project.toml marks the project root.
        fs::write(tmp.join("project.toml"), "[project]\nname = \"m\"\n").unwrap();
        assert_eq!(find_project_root(&file), tmp);

        fs::remove_dir_all(&tmp).unwrap();
        crate::db::infra::init::mcb_set_project_root(&saved);
    }

    /// `load_project` names a file, and the file's own manifest decides its
    /// world. Loading a sibling project's file while another project is active
    /// must switch worlds, not merge definitions: the merge surfaced as
    /// cross-project E5001 shadows for two real projects whose module names
    /// collide.
    #[test]
    fn cli_rpc__load_project_keeps_sibling_projects_in_separate_worlds() {
        let saved_root = crate::db::infra::init::mcb_get_project_root();
        let saved_ws = crate::workspace_root();

        let proj_a = std::env::temp_dir().join(format!("mcc-ws-a-{}", std::process::id()));
        let proj_b = std::env::temp_dir().join(format!("mcc-ws-b-{}", std::process::id()));
        for (proj, module) in [(&proj_a, "WORLD_A_MOD"), (&proj_b, "WORLD_B_MOD")] {
            fs::create_dir_all(proj).unwrap();
        }
        // The workspace pipeline canonicalizes roots; on macOS /var is a
        // symlink to /private/var, so compare against canonical forms.
        let proj_a = fs::canonicalize(&proj_a).unwrap();
        let proj_b = fs::canonicalize(&proj_b).unwrap();
        for (proj, module) in [(&proj_a, "WORLD_A_MOD"), (&proj_b, "WORLD_B_MOD")] {
            fs::write(
                proj.join("project.toml"),
                format!("[project]\nname = \"{}\"\n", proj.file_name().unwrap().to_str().unwrap()),
            )
            .unwrap();
            fs::write(
                proj.join(format!("{}.mc", module.to_lowercase())),
                format!("module {module}\n{{\n}}\n"),
            )
            .unwrap();
        }

        let module_names = || {
            crate::mcb_iter_modules()
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        };

        crate::db::infra::init::mcb_set_project_root(&proj_a);
        let _ = super::admin::handle_load_project(Some(
            serde_json::json!({"entry": proj_a.join("world_a_mod.mc").to_string_lossy()}),
        ));
        assert_eq!(crate::workspace_root(), Some(proj_a.clone()));
        assert!(module_names().contains(&"WORLD_A_MOD".to_string()));

        // The sibling file carries its own manifest, so loading it switches
        // worlds instead of dropping WORLD_B_MOD into project A's tables.
        let _ = super::admin::handle_load_project(Some(
            serde_json::json!({"entry": proj_b.join("world_b_mod.mc").to_string_lossy()}),
        ));
        assert_eq!(crate::workspace_root(), Some(proj_b.clone()));
        assert!(module_names().contains(&"WORLD_B_MOD".to_string()));

        // Back in project A: its own module is intact and B's never joined it.
        assert!(crate::workspace_switch_to(Some(proj_a.clone()), crate::WorkspaceKind::Project));
        assert!(module_names().contains(&"WORLD_A_MOD".to_string()));
        assert!(!module_names().contains(&"WORLD_B_MOD".to_string()));

        fs::remove_dir_all(&proj_a).unwrap();
        fs::remove_dir_all(&proj_b).unwrap();
        let _ = crate::workspace_switch_to(saved_ws, crate::WorkspaceKind::Project);
        crate::db::infra::init::mcb_set_project_root(&saved_root);
    }

    /// U234 end-to-end: `defs.dependents` answers from the live graph, so a
    /// re-parse that drops the reference must flip the answer — count 1
    /// before, `hasDependents: false` after. The purge primitive in the
    /// loader retires the stale edge at the re-add seam.
    #[test]
    fn cli_rpc__defs_dependents_reports_no_dependents_after_reparse() {
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .expect("test parse lock");
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let dir = std::env::temp_dir().join(format!("mcc-deps-rpc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("b.mc"),
            "component V6LED\n{\n    pins = [\n        1 = A\n        2 = K\n    ]\n}\n",
        )
        .unwrap();
        let b_uri = crate::build::pass1::canonicalize_project_uri(
            &dir.join("b.mc").to_string_lossy().into_owned(),
        );
        let a_uri = crate::build::pass1::canonicalize_project_uri(
            &dir.join("a.mc").to_string_lossy().into_owned(),
        );

        crate::mcc_load_from_string(
            &b_uri,
            &std::fs::read_to_string(dir.join("b.mc")).unwrap(),
        );
        crate::mcc_load_from_string(
            &a_uri,
            "use ./b.mc\n\nmodule main\n{\n    io A\n    io GND\n    V6LED led1\n}\n",
        );

        let q = |uri: &str| {
            super::handle_defs_dependents(Some(json!({ "name": "V6LED", "uri": uri })))
                .expect("defs.dependents answers")
        };
        let before = q(&b_uri);
        assert_eq!(before["count"].as_u64(), Some(1), "the live reference counts");
        assert_eq!(before["hasDependents"].as_bool(), Some(true));

        // Re-parse with the reference gone: the stale edge must not answer.
        crate::mcc_load_from_string(
            &a_uri,
            "use ./b.mc\n\nmodule main\n{\n    io A\n    io GND\n}\n",
        );
        let after = q(&b_uri);
        assert_eq!(after["count"].as_u64(), Some(0), "no stale dependents");
        assert_eq!(after["hasDependents"].as_bool(), Some(false));

        fs::remove_dir_all(&dir).unwrap();
    }

    /// The `pins` view orders pin IDs naturally (see `pin_id_cmp`): numeric
    /// IDs first in numeric order, then non-numeric IDs with embedded digit
    /// runs compared numerically.
    #[test]
    fn cli_rpc__pin_id_cmp_orders_numeric_then_natural() {
        let mut ids = vec![
            "10", "2", "1", "12", "11", "3", "4", "9", "5", "6", "7", "8",
        ];
        ids.sort_by(|a, b| pin_id_cmp(a, b));
        assert_eq!(
            ids,
            vec!["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]
        );

        // Non-numeric IDs follow: letter runs lexically, digit runs numerically.
        let mut mixed = vec!["PA10", "A2", "B1", "PA0", "A10", "AB", "A1"];
        mixed.sort_by(|a, b| pin_id_cmp(a, b));
        assert_eq!(mixed, vec!["A1", "A2", "A10", "AB", "B1", "PA0", "PA10"]);

        // Numeric pins come before letter pins.
        let mut mixed2 = vec!["B", "2", "A", "1"];
        mixed2.sort_by(|a, b| pin_id_cmp(a, b));
        assert_eq!(mixed2, vec!["1", "2", "A", "B"]);
    }

    /// `diag_in_system_lib` classifies a diagnostic by its location file:
    /// system-library files are excluded so a broken third-party library does
    /// not fail the user's `check` dry-run.
    #[test]
    fn cli_rpc__diag_in_system_lib_classifies_by_source_domain() {
        let diag =
            |file: &str| json!({ "severity": "error", "location": { "file": file, "line": 1 } });

        // In-memory overlay URIs (the AI's own content) are never library files.
        assert!(!diag_in_system_lib(&diag("/mcc/check.mc")));
        // Plain project paths are not library files.
        assert!(!diag_in_system_lib(&diag("boards/dev/main.mc")));
        // `file://` scheme is stripped before the path check.
        assert!(!diag_in_system_lib(&diag("file:///tmp/proj/main.mc")));
        // The legacy `/mcode/` path marker marks a system library.
        assert!(diag_in_system_lib(&diag(
            "/Users/<name>/.mcode/mcode/mcode.mc"
        )));
        assert!(diag_in_system_lib(&diag(
            "file:///Users/<name>/.mcode/mcode/mcode.mc"
        )));
        // No location → not a library diagnostic.
        assert!(!diag_in_system_lib(&json!({ "severity": "error" })));

        // Authoritative branch: the definition-space source manifest decides,
        // independent of the path string. Insert a unique temp URI into the
        // manifest, assert both domains, then clean up so no state leaks.
        let lib_uri =
            crate::McURI::from(format!("/tmp/mcc-syslib-check-{}.mc", std::process::id()).as_str());
        crate::db::cmie::tables::WORKSPACE.sources.insert(
            lib_uri.clone(),
            crate::db::defspace::SourceDomain::SystemLib("acme".into()),
        );
        assert!(diag_in_system_lib(&diag(lib_uri.as_str())));
        crate::db::cmie::tables::WORKSPACE
            .sources
            .insert(lib_uri.clone(), crate::db::defspace::SourceDomain::Project);
        assert!(!diag_in_system_lib(&diag(lib_uri.as_str())));
        crate::db::cmie::tables::WORKSPACE.sources.remove(&lib_uri);
    }

    /// End-to-end: `handle_check` Mode A (the AI dry-run) is scoped to the
    /// candidate overlay file only. Unrelated on-disk project / system-library
    /// diagnostics must never fail a clean candidate — during an agent edit the
    /// disk project is frequently mid-edit (broken), and that noise used to make
    /// every `check_dry_run` report errors. The candidate's own errors still
    /// count toward pass/fail.
    #[test]
    fn cli_rpc__handle_check_scopes_to_candidate_overlay() {
        use crate::db::diagnostic::diagnostic::{diagnostic_log_at, DiagnosticLevel};

        // The check dry-run drives the C parser via mcc_load_from_string,
        // which is not re-entrant across threads — serialize against every
        // other workspace-driving test in the crate.
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .expect("test parse lock");

        let proj = crate::McURI::from("boards/dev/main.mc");
        let lib = crate::McURI::from("/Users/<name>/.mcode/mcode/mcode.mc");
        // Authoritative domain: mark the lib URI as loaded from the mcode
        // system library, exactly like a real `mcb_load_lib` would.
        crate::db::cmie::tables::WORKSPACE.sources.insert(
            lib.clone(),
            crate::db::defspace::SourceDomain::SystemLib("mcode".into()),
        );

        // One pre-existing error in the project file, one in the system lib.
        // Neither belongs to the candidate, so neither may fail the dry-run.
        diagnostic_log_at(
            1,
            DiagnosticLevel::Error,
            proj.clone(),
            0,
            0,
            "user bug",
            &[],
        );
        diagnostic_log_at(2, DiagnosticLevel::Error, lib.clone(), 0, 0, "lib bug", &[]);

        let clean = super::handle_check(Some(json!({ "content": "module main {}\n" })))
            .expect("check dry-run succeeds");
        assert_eq!(
            clean["summary"]["errors"].as_u64().unwrap(),
            0,
            "unrelated project/lib diagnostics must not fail a clean candidate"
        );
        assert_eq!(
            clean["library"]["errors"].as_u64().unwrap(),
            0,
            "unrelated lib diagnostics are not counted either"
        );
        let files: Vec<&str> = clean["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| d["location"]["file"].as_str())
            .collect();
        assert!(
            !files.contains(&"boards/dev/main.mc"),
            "project diagnostics must not leak into a dry-run of candidate content"
        );

        // The candidate's own syntax error still counts toward pass/fail.
        let broken = super::handle_check(Some(json!({ "content": "module main {\n" })))
            .expect("check dry-run succeeds");
        assert_eq!(
            broken["summary"]["errors"].as_u64().unwrap() >= 1,
            true,
            "the candidate's own parse error still counts toward pass/fail"
        );

        // Cleanup: drop the injected manifest entry and diagnostics so no
        // state leaks into other tests in this process.
        crate::db::cmie::tables::WORKSPACE.sources.remove(&lib);
        crate::db::cmie::tables::WORKSPACE
            .diagnostics
            .lock()
            .unwrap()
            .clear_file(&proj);
        crate::db::cmie::tables::WORKSPACE
            .diagnostics
            .lock()
            .unwrap()
            .clear_file(&lib);
    }

    /// Every inline dry-run loads under the *same* virtual URI (U82-2), so a
    /// second dry-run interns no new URI — the process-global `URI_TABLE` is
    /// append-only, and a per-request URI would make its size depend on how
    /// many dry-runs this process has served.
    #[test]
    fn cli_rpc__handle_check_reuses_one_overlay_uri() {
        let _guard = crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .expect("test parse lock");

        let overlay = super::make_overlay_uri();
        let key = crate::build::pass1::canonicalize_project_uri(&overlay);

        // Intern the slot now, so the high-water scan below starts inside the
        // table: ids are dense and handed out in insertion order.
        let start = crate::uri_intern(&key).0;
        let high_water = || {
            let mut hi = start;
            while !crate::uri_of_file_id(hi + 1).is_empty() {
                hi += 1;
            }
            hi
        };

        let params = Some(json!({ "content": "module main {}\n" }));
        let first = super::handle_check(params.clone()).expect("first dry-run");
        assert_eq!(first["summary"]["errors"].as_u64().unwrap(), 0);
        let after_first = high_water();
        assert!(
            !crate::db::cmie::tables::WORKSPACE.mcodes.contains_key(&key)
                && !crate::db::cmie::tables::WORKSPACE
                    .sources
                    .contains_key(&key),
            "the overlay must be released once the dry-run has reported"
        );

        super::handle_check(params).expect("second dry-run");
        assert_eq!(
            high_water(),
            after_first,
            "a second dry-run must not mint a new URI"
        );
    }
}
