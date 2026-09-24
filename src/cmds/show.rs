// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc show` — Show detailed content of parsed definitions.
//!
//! Targets:
//!   * overview : `all` (layered by origin, `--scope`; -F anchors the file layer)
//!              : `defs` (whole current definition space: `DefId` per live def,
//!                layered by origin with `--scope`; the def-space twin of
//!                `dianlu` for cross-checking `show dianlu --ids` `D<id>` tags)
//!   * entity   : `component` / `module` / `interface` / `enum` (<name> required)
//!   * drill    : `pins` / `ports` / `labels` / `instances` / `nets` / `attrs`
//!                / `funcs` / `params` / `roles` / `values` / `net` (<name> = owning
//!                entity; funcs are referenced dot-qualified as `OWNER.FUNC` for
//!                `params` and `nets`)
//!   * debug    : `dump` / `lapper` / `ast`
//!   * power    : `pwr` (recursive power-intent tree: planes / DC faces /
//!                rail-member nets + component power contracts)
//!              : `pwrflow` (top-level power-flow single view derived from one
//!                flat build: world crowns / rail contract table / supply tree;
//!                `--full` widens the rail contract, `--decaps` unfolds decaps)
//!
//! Top-level name lists live in `mcc list` (see cmds/list.rs).

use crate::output::{compact, die, emit_projection_sub, OutputFormatExt, ProjectionKey};
use anyhow::{Context, Result};
use mcc::cli::{rpcclient::RpcClient, OutputFormat, ShowArgs, ShowScope, ShowTarget};
use mcc::{host_func_names, org_unit_counts, org_unit_items};
use mcc::{InstEntry, InstKind, InstTable, McIds, McURI, MemberRole, TreeView};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// ⚠ The CLI no longer delegates to a running server for the entity / drill-down
/// targets (CIMP §1 U90, ruling (b)). Those requests carried `name` plus the raw
/// `file` argument, which the server resolved against *its own* `current_dir()`,
/// so with a daemon running `mcc show component X -F circuit.mc` read the wrong
/// world. The RPC methods stay for direct callers.
///
/// One server call in this file survives, and deliberately: `show lapper`'s
/// fallback posts `{"uri", "content"}` to `sem` — it ships the file's **bytes**,
/// so the answer is about the client's source. The rule is "no delegation for a
/// context that cannot be delivered", not "no delegation".
pub fn run(args: &ShowArgs) -> Result<()> {
    run_local(args)
}

fn run_local(args: &ShowArgs) -> Result<()> {
    let loaded = prepare(args);

    let name = args.name.as_deref();
    match args.target {
        // overview / debug
        ShowTarget::All => show_all(args, loaded.as_deref()),
        ShowTarget::Defs => show_defs(args, loaded.as_deref()),
        ShowTarget::Lapper => show_lapper(args, loaded.as_deref()),
        ShowTarget::Ast => show_ast(args, loaded.as_deref()),

        // entity detail (name required; lists moved to `mcc list`)
        ShowTarget::Component => match name {
            None => need_list_hint(args, "component"),
            Some(n) => show_component(n, args),
        },
        ShowTarget::Module => match name {
            None => need_list_hint(args, "module"),
            Some(n) => show_module(n, args),
        },
        ShowTarget::Interface => match name {
            None => need_list_hint(args, "interface"),
            Some(n) => show_interface(n, args),
        },
        ShowTarget::Enum => match name {
            None => need_list_hint(args, "enum"),
            Some(n) => show_enum(n, args),
        },
        ShowTarget::Net => match name {
            None => need_list_hint(args, "nets"),
            Some(n) => show_net(n, args),
        },
        ShowTarget::Dianlu => show_dianlu(args),
        ShowTarget::Pwr => show_pwr(args),
        ShowTarget::Pwrflow => show_pwrflow(args),
        ShowTarget::Sim => show_sim(args),
        ShowTarget::Stage => show_stage(args),
        ShowTarget::OrgUnits => show_org_units(args),
        ShowTarget::Diagnostics => show_diagnostics(loaded.as_deref()),
        ShowTarget::Netlist => show_netlist(loaded.as_deref()),
        ShowTarget::Project => show_project(loaded.as_deref()),

        // drill-down
        ShowTarget::Pins => drill_pins(require_name(args), args),
        ShowTarget::Ports => match name {
            None => need_list_hint(args, "ports"),
            Some(n) => drill_ports(n, args),
        },
        ShowTarget::Labels => drill_labels(require_name(args), args),
        ShowTarget::Instances => drill_instances(require_name(args), args),
        ShowTarget::Nets => drill_nets(require_name(args), args),
        ShowTarget::Attrs => drill_attrs(require_name(args), args),
        ShowTarget::Funcs => drill_funcs(require_name(args), args),
        ShowTarget::Params => drill_params(require_name(args), args),
        ShowTarget::Roles => drill_roles(require_name(args), args),
        ShowTarget::Values => drill_values(require_name(args), args),
    }
}

// Setup

/// One-shot environment setup: init engine, load `--lib` libraries, load the
/// target file. All handlers assume this ran, so none of them re-init.
///
/// A directory target is treated as project mode (mirrors `parse <dir>`):
/// `project.toml` provides the entry file and dependency libraries, or browse
/// mode selects the unique `.mc` file declaring `module main` when no
/// manifest exists.
///
/// The target path comes from `-F`, or from the positional argument for
/// targets that take no entity name ([`target_path`]). When neither is given,
/// the current directory is the target if it holds a project manifest — for
/// entity queries (`show component MCU`) as much as for name-less ones.
///
/// Returns the URI the target **resolved to** — for a directory target that is
/// the entry file inside it, not the directory. A face that needs the target
/// path must read this instead of deriving one from the raw argument: a
/// directory re-derived from the raw argument is a URI nothing was parsed
/// under, so the face prints an empty reading, exits 0 and says nothing
/// (CIMP §1 U93).
fn prepare(args: &ShowArgs) -> Option<String> {
    let file_opt = crate::cmds::manifest::effective_target(target_path(args));
    let file_opt = file_opt.as_deref();
    crate::cmds::manifest::init_local(file_opt, &mcc::cli::globals().lib);

    let f = file_opt?;
    if Path::new(f).is_dir() {
        // Directory target: unified project/browse-mode loading.
        match crate::cmds::common::load_target(
            Some(f),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        ) {
            Ok((entry_uri, _)) => Some(normalize_uri_path(&entry_uri)),
            Err(e) => die!("mcc::show", 1, "directory target: {:#}", e),
        }
    } else {
        let actual = resolve_file(f);
        // Absolutize so the engine does not join a relative path onto the
        // project root (which would double the directory components).
        let path = if Path::new(&actual).is_absolute() {
            actual
        } else {
            std::env::current_dir()
                .map(|c| c.join(&actual).to_string_lossy().to_string())
                .unwrap_or(actual)
        };
        let path = normalize_uri_path(&path);
        let uri = mcc::McURI::from(path.as_str());
        mcc::mcc_load_project(&uri);
        Some(path)
    }
}

/// Lexically normalize a path used as a URI: drop `.` components, leaving `..`
/// and every real component alone. Not `Path::canonicalize`, which resolves
/// symlinks too and would return a URI the engine never registered.
///
/// The engine keys a loaded file on the normalized spelling, so a target
/// written `./x.mc` does not match its own defs: the faces that anchor a layer
/// or a dump on that URI print an empty reading, exit 0 and say nothing.
fn normalize_uri_path(path: &str) -> String {
    let mut out = PathBuf::new();
    for c in Path::new(path).components() {
        if c != std::path::Component::CurDir {
            out.push(c.as_os_str());
        }
    }
    if out.as_os_str().is_empty() {
        ".".to_string()
    } else {
        out.to_string_lossy().to_string()
    }
}

/// Effective target path for file-based targets: `-F` wins; otherwise the
/// positional argument is the target for targets that take no entity name
/// (`show all` / `show dianlu` / `show ast` / `show lapper`).
///
/// This is the list of targets whose **positional** is a path, so it is also
/// the list of targets `prepare` resolves a directory for. `ast` and `lapper`
/// take a path exactly like `dianlu` does; leaving them out made the same
/// input shape (a directory) behave one way on one face and another way on the
/// next (CIMP §1 U93).
fn target_path(args: &ShowArgs) -> Option<&str> {
    if args.file.is_some() {
        return args.file.as_deref();
    }
    match args.target {
        ShowTarget::All
        | ShowTarget::Defs
        | ShowTarget::Dianlu
        | ShowTarget::Pwr
        | ShowTarget::Pwrflow
        | ShowTarget::Ast
        | ShowTarget::Lapper => args.name.as_deref(),
        _ => None,
    }
}

fn require_name<'a>(args: &'a ShowArgs) -> &'a str {
    match args.name.as_deref() {
        Some(n) => n,
        None => {
            die!(
                "mcc::show",
                2,
                "'show {:?}' requires an entity name",
                args.target
            );
        }
    }
}

/// Name lists moved to `mcc list`; a bare `show <target>` without a name
/// prints a hint instead of silently listing.
fn need_list_hint(args: &ShowArgs, list_kind: &str) -> Result<()> {
    die!(
        "mcc::show",
        2,
        "'show {:?}' requires an entity name\nto list {} names, use `mcc list {}`",
        args.target,
        list_kind,
        list_kind
    );
}

/// Resolve a file path; if it doesn't exist, search by base name in the tree.
pub(crate) fn resolve_file(file: &str) -> String {
    if Path::new(file).exists() {
        return file.to_string();
    }
    let matches = find_files_with_name(file);
    match matches.len() {
        0 => {
            die!("mcc::show", 1, "file not found: {}", file);
        }
        1 => matches[0].clone(),
        _ => {
            let list: Vec<String> = matches
                .iter()
                .enumerate()
                .map(|(i, p)| format!("  {}: {}", i + 1, p))
                .collect();
            die!(
                "mcc::show",
                1,
                "multiple files named '{}':\n{}",
                file,
                list.join("\n")
            );
        }
    }
}

/// Search for files with the same name, recursively in common directories.
fn find_files_with_name(name: &str) -> Vec<String> {
    use std::fs;

    let file_name = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name);

    let mut matches = Vec::new();

    fn search_dir(dir: &Path, file_name: &str, matches: &mut Vec<String>, depth: usize) {
        if depth > 5 {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        if !fname.starts_with('.') && fname != "target" && fname != "node_modules" {
                            search_dir(&path, file_name, matches, depth + 1);
                        }
                    }
                } else if path.is_file() {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        if fname == file_name && fname.ends_with(".mc") {
                            if let Ok(canonical) = path.canonicalize() {
                                if let Some(p) = canonical.to_str() {
                                    matches.push(p.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    search_dir(Path::new("."), file_name, &mut matches, 0);
    matches
}

// Definition lookup

/// Find a definition by name across all kinds; returns its CMIE.
fn find_def(name: &str) -> Option<mcc::McCMIE> {
    let lists = [
        mcc::mcb_iter_components(),
        mcc::mcb_iter_modules(),
        mcc::mcb_iter_interfaces(),
        mcc::mcb_iter_enums(),
    ];
    for list in &lists {
        if let Some((n, u)) = list.iter().find(|(n, _)| n == name) {
            if let Some(cmie) =
                mcc::get_def(&mcc::McIds::from(n.as_str()), &mcc::McURI::from(u.as_str()))
            {
                return Some(cmie);
            }
        }
    }
    None
}

/// Find a component definition by name, bypassing the RefDefMap ambiguity
/// that arises when a component and an enum share the same name+URI
/// (e.g. `enum CAP` + `component CAP` in mcode/cap.mc, P0-3).
fn find_component_def(name: &str) -> Option<mcc::McCMIE> {
    for (n, u) in mcc::mcb_iter_components() {
        if n == name {
            if let Some(c) =
                mcc::get_component_def(&mcc::McIds::from(n.as_str()), &mcc::McURI::from(u.as_str()))
            {
                return Some(c);
            }
        }
    }
    None
}

/// Find a definition restricted to one kind (`cmie_kind`: 0 component,
/// 1 module, 2 interface, 3 enum). Kind-blind lookups return the first kind
/// hit when a name collides across kinds (e.g. `component USB.MINIB` +
/// `interface USB.MINIB` in the mcode library); a kind-directed `show`
/// must prefer the requested kind instead.
fn find_kind_def(name: &str, cmie_kind: u8) -> Option<mcc::McCMIE> {
    let iter: Vec<(String, String)> = match cmie_kind {
        0 => mcc::mcb_iter_components(),
        1 => mcc::mcb_iter_modules(),
        2 => mcc::mcb_iter_interfaces(),
        3 => mcc::mcb_iter_enums(),
        _ => return None,
    };
    for (n, u) in iter {
        if n == name {
            if let Some(c) = mcc::get_kind_def(
                cmie_kind,
                &mcc::McIds::from(n.as_str()),
                &mcc::McURI::from(u.as_str()),
            ) {
                return Some(c);
            }
        }
    }
    None
}

fn def_or_exit(name: &str) -> mcc::McCMIE {
    match find_def(name) {
        Some(c) => c,
        None => {
            die!("mcc::show", 1, "definition not found: {}\nhint: load a file with -F, a library with --lib, or start a server", name);
        }
    }
}

fn component_def_or_exit(name: &str) -> mcc::McCMIE {
    match find_component_def(name) {
        Some(c) => c,
        None => {
            die!("mcc::show", 1, "definition not found: {}\nhint: load a file with -F, a library with --lib, or start a server", name);
        }
    }
}

/// Report that `<what>` is not applicable to the kind of `<name>`, then exit.
fn not_applicable(what: &str, name: &str) -> ! {
    die!("mcc::show", 1, "'{}' is not available for '{}'", what, name);
}

// Containers: overview / list / detail

fn show_all(args: &ShowArgs, loaded: Option<&str>) -> Result<()> {
    // The file layer is anchored on the target the command **loaded**. With no
    // target named on the command line, the cwd project `prepare` may have
    // picked up is not this face's file layer, so `target_path` gates it.
    let target = target_path(args).and(loaded).map(str::to_string);
    let scopes = resolve_scopes(args.scope, target.is_some());

    let mut data = serde_json::Map::new();
    for s in &scopes {
        data.insert(
            scope_name(*s).to_string(),
            collect_scope(*s, target.as_deref()),
        );
    }
    // The target file path is kept under its own key so it does not collide
    // with the "file" layer collection above.
    if let Some(t) = &target {
        data.insert("target_file".to_string(), json!(t));
    }
    data.insert("type".to_string(), json!("layered_all"));
    emit_show(args.target, &json!(data), args.span)
}

/// Resolve the `--scope` default policy shared by `show all` and `list all`:
///   * default is the `file` layer when a target file is present
///   * without a target file the `file` layer has nothing to match against,
///     so fall back to every loaded layer (keeps the overview role)
pub(crate) fn resolve_scopes(scope: Option<ShowScope>, has_target: bool) -> Vec<ShowScope> {
    let scope = scope.unwrap_or(ShowScope::File);
    match (scope, has_target) {
        (ShowScope::All, _) | (ShowScope::File, false) => {
            vec![ShowScope::File, ShowScope::Use, ShowScope::System]
        }
        (s, _) => vec![s],
    }
}

/// JSON tag used by [`render_layered_text`] to detect `show all` output.
const LAYERED_ALL_TYPE: &str = "layered_all";

fn scope_name(scope: ShowScope) -> &'static str {
    match scope {
        ShowScope::File => "file",
        ShowScope::Use => "use",
        ShowScope::System => "system",
        ShowScope::All => "all",
    }
}

/// Collect the definitions of one layer (file / use / system) from the loaded
/// tables, grouped into the same module/component/interface/enum lists that
/// the flat `show all` used to print.
fn collect_scope(scope: ShowScope, target: Option<&str>) -> Value {
    let mut components = Vec::new();
    let mut modules = Vec::new();
    let mut interfaces = Vec::new();
    let mut enums = Vec::new();
    for (n, u) in mcc::mcb_iter_components() {
        if classify_def_scope(&u, target) == scope {
            components.push(n);
        }
    }
    for (n, u) in mcc::mcb_iter_modules() {
        if classify_def_scope(&u, target) == scope {
            modules.push(n);
        }
    }
    for (n, u) in mcc::mcb_iter_interfaces() {
        if classify_def_scope(&u, target) == scope {
            interfaces.push(n);
        }
    }
    for (n, u) in mcc::mcb_iter_enums() {
        if classify_def_scope(&u, target) == scope {
            enums.push(n);
        }
    }
    json!({
        format!("module_list({})", modules.len()): modules,
        format!("component_list({})", components.len()): components,
        format!("interface_list({})", interfaces.len()): interfaces,
        format!("enum_list({})", enums.len()): enums,
    })
}

/// Classify a definition URI into a layer:
///   * `File`   — the definition lives in the -F target file
///   * `System` — the definition lives inside a loaded system library
///   * `Use`    — everything else (use-imported / project libraries)
pub(crate) fn classify_def_scope(uri: &str, target: Option<&str>) -> ShowScope {
    if let Some(t) = target {
        if uri_matches(uri, t) {
            return ShowScope::File;
        }
    }
    if is_system_uri(uri) {
        return ShowScope::System;
    }
    ShowScope::Use
}

/// True when `uri` belongs to a loaded system library (mcode or an installed
/// library resolved under the data root).
fn is_system_uri(uri: &str) -> bool {
    let path = std::path::Path::new(uri);
    mcc::mcb_loaded_libs()
        .iter()
        .any(|name| mcc::resolve_lib_root(name).is_some_and(|root| path.starts_with(&root)))
}

// defs: the current definition space, in registry form

/// One live def of the current definition space as a registry display row.
///
/// Gathered from the unified definition view ([`mcc::definition_space`]) —
/// one row per live def, workspace-first under layered coexist — with the
/// def's registry `DefId` resolved by its `(name, uri)` key, exactly the key
/// `show dianlu --ids` annotates instances with. Func members are host
/// members (design §12.1) and list under their module / component host.
struct DefsRow {
    id: Option<u32>,
    kind: mcc::DefKind,
    name: String,
    uri: String,
    funcs: Vec<String>,
}

/// Def kinds in display order: the two hosts that carry func members first,
/// then the smaller definition kinds.
///
/// ⚠ This is a **display order**, not a count — it holds six of the registry's
/// seven `DefKind` variants (no `Func`), and the seventh is not missing here by
/// accident: a func is a host member and prints under its host (design §12.1).
/// The list itself now lives in the registry
/// ([`mcc::DEF_KIND_ORDER`]), which is what the one read walks, so this alias
/// exists to keep the group order spelled once.
const DEF_KINDS: [mcc::DefKind; 5] = mcc::DEF_KIND_ORDER;

/// Every live def of the current definition space, one [`DefsRow`] per def.
///
/// ★ CIMP §1 U120 (2026-09-19): this is a **consumer** of the registry's one
/// read ([`mcc::DefinitionSpace::all_defs`]), not a union of six per-kind
/// calls. Adding a kind to the space now changes `DEF_KIND_ORDER` in the
/// registry and nothing here.
///
/// The funcs column is read off each host's own `funcs` table rather than
/// assembled from the func rows: a host prints its members in the order the
/// author wrote them, which is the host table's order. The read is still one
/// read — the rows, their keys and their order all come from `all_defs`.
fn defs_rows() -> Vec<DefsRow> {
    mcc::definition_space()
        .all_defs()
        .into_iter()
        .map(|(kind, sn, data)| DefsRow {
            id: mcc::def_id(&sn, kind),
            kind,
            name: sn.ident.to_string(),
            uri: mcc::uri_resolve(sn.uri).to_string(),
            funcs: host_func_names(&data),
        })
        .collect()
}

/// Plural group label of one def kind (registry table grouping).
fn def_kind_group(kind: mcc::DefKind) -> &'static str {
    // The table lives beside the variants ([`mcc::DefKind::group`]).
    kind.group()
}

/// Resolved layers for `show defs`. Unlike `show all` there is no implicit
/// file-layer default: the whole current definition space is shown unless
/// `--scope` narrows it, so any `D<id>` printed by `show dianlu --ids` —
/// including system-lib defs — resolves here.
fn defs_scopes(scope: Option<ShowScope>) -> Vec<ShowScope> {
    match scope {
        None | Some(ShowScope::All) => vec![ShowScope::File, ShowScope::Use, ShowScope::System],
        Some(s) => vec![s],
    }
}

/// The rows of one `(scope, kind)` group, ordered by `DefId` then name.
fn gather_rows<'a>(
    rows: &'a [DefsRow],
    layer_of: &[ShowScope],
    scope: ShowScope,
    kind: mcc::DefKind,
) -> Vec<&'a DefsRow> {
    let mut out: Vec<&DefsRow> = rows
        .iter()
        .zip(layer_of)
        .filter(|(r, l)| r.kind == kind && **l == scope)
        .map(|(r, _)| r)
        .collect();
    out.sort_by(|a, b| {
        a.id.unwrap_or(u32::MAX)
            .cmp(&b.id.unwrap_or(u32::MAX))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// `show defs [target]`: the current definition space in registry form —
/// every live def with its `DefId`, kind, name and declaring file, grouped by
/// origin layer (file / use / system) and ordered by `DefId` within each
/// kind. Text prints aligned `D<id>` rows; structured output nests layers
/// then kinds. The def-space twin of `show dianlu` — instance `D<id>` tags
/// resolve to their rows here.
///
/// The file layer is anchored on the target the command **loaded**, exactly as
/// in [`show_all`]: a directory names no origin of its own, so a path derived
/// from the raw argument puts every def of the entry file in the `use` layer
/// and the layer never appears (CIMP §1 U99).
fn show_defs(args: &ShowArgs, loaded: Option<&str>) -> Result<()> {
    let target = target_path(args).and(loaded).map(str::to_string);
    let scopes = defs_scopes(args.scope);
    let rows = defs_rows();
    if rows.is_empty() {
        let data = json!({ "type": "defs" });
        if matches!(mcc::cli::globals().format, OutputFormat::Text) {
            println!("===== defs =====\n(no definitions loaded)");
            return Ok(());
        }
        return emit_show(args.target, &data, args.span);
    }

    let layer_of: Vec<ShowScope> = rows
        .iter()
        .map(|r| classify_def_scope(&r.uri, target.as_deref()))
        .collect();
    // Non-empty kind groups per scope. Empty layers are skipped by default;
    // an explicitly requested --scope that matches nothing still reports.
    let groups_of = |scope: ShowScope| -> Vec<(mcc::DefKind, Vec<&DefsRow>)> {
        DEF_KINDS
            .iter()
            .map(|kind| (*kind, gather_rows(&rows, &layer_of, scope, *kind)))
            .filter(|(_, g)| !g.is_empty())
            .collect()
    };

    if matches!(mcc::cli::globals().format, OutputFormat::Text) {
        let mut lines = vec!["===== defs =====".to_string()];
        for scope in &scopes {
            let groups = groups_of(*scope);
            if groups.is_empty() {
                if args.scope.is_some() {
                    lines.push(format!("-- {} -- (no definitions)", scope_name(*scope)));
                }
                continue;
            }
            lines.push(format!("-- {} --", scope_name(*scope)));
            for (kind, rows_of) in groups {
                lines.push(format!("  {} ({}):", def_kind_group(kind), rows_of.len()));
                for r in rows_of {
                    let id = match r.id {
                        Some(v) => format!("D{v}"),
                        None => "D-".to_string(),
                    };
                    lines.push(format!("    {id:<6} {:<26} {}", r.name, r.uri));
                    if !r.funcs.is_empty() {
                        lines.push(format!("      funcs: {}", r.funcs.join(", ")));
                    }
                }
            }
        }
        let rendered = lines.join("\n");
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, rendered)?;
        } else {
            println!("{rendered}");
        }
        return Ok(());
    }

    // Structured output: layers → kinds → def rows.
    let mut data = serde_json::Map::new();
    data.insert("type".to_string(), json!("defs"));
    for scope in &scopes {
        let groups = groups_of(*scope);
        if groups.is_empty() && args.scope.is_none() {
            continue;
        }
        let mut kinds = serde_json::Map::new();
        for (kind, rows_of) in groups {
            kinds.insert(
                def_kind_group(kind).to_string(),
                json!(rows_of.into_iter().map(def_row_json).collect::<Vec<_>>()),
            );
        }
        data.insert(scope_name(*scope).to_string(), json!(kinds));
    }
    if let Some(t) = &target {
        data.insert("target_file".to_string(), json!(t));
    }
    emit_show(args.target, &json!(data), args.span)
}

/// One registry row as JSON: `id` (null when the def carries no registry id),
/// the definition name and its declaring file; func members, when present,
/// list by name under their host row.
fn def_row_json(r: &DefsRow) -> Value {
    let mut v = json!({
        "id": r.id,
        "name": r.name,
        "uri": r.uri,
    });
    if !r.funcs.is_empty() {
        v["funcs"] = json!(r.funcs);
    }
    v
}

fn show_ast(args: &ShowArgs, loaded: Option<&str>) -> Result<()> {
    // Same rule as `show lapper`: a name is required here even though
    // `prepare` may have resolved one, and the URI is the one the command
    // **loaded** rather than one derived again from the raw argument (U93).
    let named = require_name(args);
    let uri_str = loaded.unwrap_or(named).to_string();
    let mc_uri = McURI::from(uri_str.as_str());
    // Enable AST tree output (MCC_LOG_VISIT) from C engine
    if let Ok(mut trace) = mcc::get_runtime_trace().write() {
        trace.visit = Some(true);
    }
    let structured = !matches!(mcc::cli::globals().format, OutputFormat::Text);
    if structured {
        // JSON face: the re-parse below captures the tree into the engine
        // buffer instead of printing it, and the face emits it as data.
        mcc::set_ast_visit_json(true);
        mcc::set_trace_stdout_suppressed(true);
    } else {
        mcc::set_trace_stdout_suppressed(false);
    }
    // The tree is printed *by the parse*, so this face has to own one. The
    // flag alone is not enough: `prepare` already parsed the target, and a
    // second `mcc_load_project` on a parsed file parses nothing and prints
    // nothing. Dropping the entry and loading it again re-parses it here.
    mcc::mcc_remove(&mc_uri);
    mcc::mcb_reset_ast_visit_flag();
    mcc::clear_ast_visit_json();
    mcc::mcc_load_project(&mc_uri);
    if structured {
        // Capture is keyed per URI: read the entry file's own tree, not
        // whichever file in the load chain happened to parse first.
        let tree = mcc::take_ast_visit_json_for(&uri_str)
            .unwrap_or_else(|| json!({"error": "no AST visit captured", "uri": uri_str}));
        return emit_show_owned(ShowTarget::Ast, json!({"uri": uri_str, "ast": tree}));
    }
    Ok(())
}

fn show_lapper(args: &ShowArgs, loaded: Option<&str>) -> Result<()> {
    // A name is required even though `prepare` may have resolved one: with no
    // positional it can fall back to the cwd project, and this face dumps the
    // symbols of one file, not of a project.
    let named = require_name(args);
    // The URI the command actually loaded, taken verbatim rather than derived
    // again from the raw argument: the symbol dump is keyed on the loaded URI,
    // so a second derivation that differs by as little as a symlink or a
    // directory component finds nothing and dumps an empty table (U93).
    let uri_str = loaded.unwrap_or(named).to_string();
    let mc_uri = McURI::from(uri_str.as_str());

    // Suppress AST tree printing during parsing
    mcc::set_trace_stdout_suppressed(true);

    // prepare() already called mcc_load_project. If the file is already loaded,
    // dump symbols directly. Otherwise, load and parse first.
    let is_text = matches!(mcc::cli::globals().format, OutputFormat::Text);
    if is_text {
        if let Some(text) = mcc::dump_symbols_f12_text(&mc_uri) {
            return write_show_text(&text);
        }
    } else {
        if let Some(json_val) = mcc::dump_symbols_json(&mc_uri) {
            return emit_show_owned(ShowTarget::Lapper, json_val);
        }
    }

    // Not loaded yet — load project and try again. `prepare` loads the target
    // for this face too, so this is a safety net rather than the usual route;
    // it re-runs the same init (nearest `project.toml` drives the dependency
    // set, project root set so project-relative `use` paths resolve).
    let path = Path::new(&uri_str);
    let project_root = super::manifest::find_project_root(Some(&uri_str))
        .unwrap_or_else(|| path.parent().map(|p| p.to_path_buf()).unwrap_or_default());
    if !project_root.as_os_str().is_empty() {
        mcc::mcc_set_project_root(&project_root);
    }
    let libs = super::manifest::collect_libs(Some(&project_root), &[]);
    super::manifest::load_libs(&libs);
    mcc::mcc_load_project(&mc_uri);
    if is_text {
        if let Some(text) = mcc::dump_symbols_f12_text(&mc_uri) {
            return write_show_text(&text);
        }
    } else {
        if let Some(json_val) = mcc::dump_symbols_json(&mc_uri) {
            return emit_show_owned(ShowTarget::Lapper, json_val);
        }
    }

    // Fallback: send to RPC server
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", uri_str))?;
    let c = RpcClient::probe().context("no mcc server running and file not in local workspace")?;
    let result = c.call("sem", json!({"uri": uri_str, "content": content}))?;
    let symbols = &result["symbols"];

    emit_show_owned(
        ShowTarget::Lapper,
        json!({
            "file": uri_str,
            "lapper": symbols["lapper"],
            "local": symbols["local"],
            "ref_def_map": symbols["ref_def_map"],
            "cross_file_targets": symbols["global"]["cross_file_targets"],
        }),
    )
}

fn show_component(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = component_def_or_exit(name);
    let mcc::McCMIE::Component(comp) = cmie else {
        die!("mcc::show", 1, "'{}' is not a Component", name);
    };
    let mut data = pins_json(&comp.pins);
    data["name"] = json!(name);
    data["uri"] = json!(comp.uri.to_string());
    emit_show(args.target, &data, args.span)
}

fn show_module(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = find_kind_def(name, 1).unwrap_or_else(|| def_or_exit(name));
    let mcc::McCMIE::Module(module) = cmie else {
        die!("mcc::show", 1, "'{}' is not a Module", name);
    };
    let data = json!({
        "name": name,
        "uri": module.uri.to_string(),
        "instances": instances_json(&module.insts, None),
    });
    emit_show(args.target, &data, args.span)
}

fn show_interface(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = find_kind_def(name, 2).unwrap_or_else(|| def_or_exit(name));
    let mcc::McCMIE::Interface(iface) = cmie else {
        die!("mcc::show", 1, "'{}' is not an Interface", name);
    };
    let roles: Vec<String> = iface.roles.iter().map(|r| r.name.to_string()).collect();
    let data = json!({
        "name": name,
        "uri": iface.uri.to_string(),
        "pin_count": iface.pins.pins.len(),
        "role_count": roles.len(),
        "roles": roles,
        "params": iface.params.names_full(),
    });
    emit_show(args.target, &data, args.span)
}

fn show_enum(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = find_kind_def(name, 3).unwrap_or_else(|| def_or_exit(name));
    let mcc::McCMIE::Enum(en) = cmie else {
        die!("mcc::show", 1, "'{}' is not an Enum", name);
    };
    let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
    let data = json!({
        "name": name,
        "uri": en.uri.to_string(),
        "value_count": values.len(),
        "values": values,
    });
    emit_show(args.target, &data, args.span)
}

/// Points of one Pass2 net (net list moved to `mcc list nets`).
fn show_net(name: &str, args: &ShowArgs) -> Result<()> {
    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| {
            die!(
                "mcc::show",
                1,
                "no modules found\nhint: load a file with -F or use --top"
            );
        });
    let nets = nets_map(&top);

    let data = match nets.get(name) {
        Some(points) => json!({ "name": name, "points": points }),
        None => json!({ "name": name, "points": Vec::<String>::new(), "error": "net not found" }),
    };
    emit_show(args.target, &data, args.span)
}

// show dianlu — whole circuit tree after instantiation (Pass2)

/// `show dianlu`: instantiate the top module (--top or first module) and walk
/// the resulting `McModuleInst` tree. Output is organized as one section per
/// module in source order: same-level instances (components, sub-modules,
/// labels, buses) and connections first, then each sub-module in its own
/// nested section. Interface-typed buses are annotated with their interface
/// class (e.g. `uC.UART0{TX, RX} :: UART.TTL(DCE)`).
fn show_dianlu(args: &ShowArgs) -> Result<()> {
    // Top module resolution mirrors `parse`: a directory target (project
    // mode) resolves the top through the manifest / browse entry; a
    // single-file target uses --top, else the module defined in that file,
    // else the first loaded module.
    let (entry_uri, top) = if let Some(f) = target_path(args) {
        let p = Path::new(f);
        if p.is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )
            .unwrap_or_else(|e| {
                die!("mcc::show", 1, "directory target: {:#}", e);
            })
        } else {
            let path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(p)
            };
            (path.to_string_lossy().to_string(), None)
        }
    } else {
        (String::new(), None)
    };
    let top = crate::cmds::common::resolve_top_module(&entry_uri, top).unwrap_or_else(|| {
        die!(
            "mcc::show",
            1,
            "no modules found\nhint: load a file with -F or use --top"
        );
    });
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top.clone()));

    // Guardrail: a Pass2 panic must not abort the process.
    let (inst, arena, store, net_store) = crate::cmds::common::build_pass2_with_arena(&top, &uri)
        .unwrap_or_else(|e| {
            die!("mcc::show", 1, "{e}");
        });

    // Global module-nesting overview first: every
    // module in source order with its declared / declareb / funcall-generated
    // instances, so the whole instance structure is visible before the
    // per-module sections.
    let view = mcc::TreeView::new(&arena, &store);
    let hierarchy = mcc::hierarchy::build_hierarchy(&mcc::hierarchy::collect_module_nodes(
        &inst, &top, &view, &net_store,
    ));

    // Text mode: hand-rendered sections (aligned with the user-facing
    // circuit view; the generic key: value fallback would bury the tree).
    if matches!(mcc::cli::globals().format, OutputFormat::Text) {
        let mut lines = Vec::new();
        lines.push(format!("===== Hierarchy: {top} ====="));
        let mut htext = String::new();
        mcc::hierarchy::render_hierarchy_text(&mut htext, &hierarchy);
        lines.push(htext.trim_end().to_string());
        lines.push(String::new());
        // `--ids` also annotates each pin/port with its lane-layer physical
        // point `N<n>:<m>` (the instance node/def-id tags come along for
        // free, since the point carries its owning node id).
        render_dianlu_section(&inst, &top, &view, &net_store, &mut lines, args.ids);
        let rendered = lines.join("\n");
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, rendered)?;
        } else {
            println!("{rendered}");
        }
        return Ok(());
    }

    let data = json!({
        "type": "dianlu",
        "top": top,
        "hierarchy": hierarchy,
        "sections": dianlu_sections(&inst, &top, &view, &net_store, args.ids),
    });
    emit_show(args.target, &data, args.span)
}

// show pwr — recursive power-intent tree (Pass2 + flat InstTable)

/// `show pwr`: build the top module (`--top` or first loaded module) with the
/// **flat** InstTable (Pass2 + flatten) so each module node can report the
/// merged net its DC-face members / rail labels actually land on, alongside
/// the source-level declarations (`McModule.pi`: conduit/@role, domain rails,
/// body edges, identity port rows).
///
/// The dump is one node per module instance, nested: `decl` (declared planes /
/// faces), `rails` (this module's own Ground/Power member endpoints grouped by
/// the merged net they hang on, with the full net point set — the cross-module
/// union when the project top is used), `components` (the power contracts of
/// the leaves it declares), and `children`. See `mcc show erc` for the rule
/// findings; this command reports the *facts* the rules evaluate.
fn show_pwr(args: &ShowArgs) -> Result<()> {
    // Top-module resolution mirrors `show dianlu` (file/dir positional with
    // `-F` override; `--top` selects the module within the loaded set).
    let (entry_uri, top) = if let Some(f) = target_path(args) {
        let p = Path::new(f);
        if p.is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )
            .unwrap_or_else(|e| {
                die!("mcc::show", 1, "directory target: {:#}", e);
            })
        } else {
            let path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(p)
            };
            (path.to_string_lossy().to_string(), None)
        }
    } else {
        (String::new(), None)
    };
    let top = crate::cmds::common::resolve_top_module(&entry_uri, top).unwrap_or_else(|| {
        die!(
            "mcc::show",
            1,
            "no modules found\nhint: load a file with -F or use --top"
        );
    });
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top.clone()));

    // One flat build gives both the modeling tree (decls) and the InstTable
    // (merged nets). `mcc_build_flat_with_arena` does not log the net-check
    // diagnostics it runs — the caller decides — so a power dump stays quiet.
    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&mcc::McIds::from(top.clone()), &uri, 1000).unwrap_or_else(
            |e| {
                die!("mcc::show", 1, "{e}");
            },
        );
    let view = mcc::TreeView::new(&arena, &store);

    if matches!(mcc::cli::globals().format, OutputFormat::Text) {
        let mut lines = Vec::new();
        lines.push(format!("===== Power Intent: {top} ====="));
        lines.push(String::new());
        render_pwr_section(&tree, &top, &view, &table, &mut lines, args.ids);
        let rendered = lines.join("\n");
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, rendered)?;
        } else {
            println!("{rendered}");
        }
        return Ok(());
    }

    let data = json!({
        "type": "pwr",
        "format": "power-intent/v1",
        "top": top,
        "tree": pwr_node_json(&tree, &top, &view, &table, args.ids),
    });
    emit_show(args.target, &data, args.span)
}

// `show sim` — model profile registry joined to the built world

/// Render `mcc show sim`: the library `sim/` model-profile cards
/// (worldmodel-design §7 W3) plus their per-instance resolution. Text mode
/// emits two sections — the card table and one row per adoption lane of the
/// flat world; JSON mode emits the typed view under
/// `{"type":"sim","format":"model-profile/v1",…}`. Data layer only: a lane
/// without a card reads `not-curated` explicitly, never silently.
fn show_sim(args: &ShowArgs) -> Result<()> {
    use mcc::model_profile::{LoadedProfiles, ProfileTier};

    // §1 cards: every loaded library's `sim/` sidecar, read through the
    // definition space so the view follows the library load lifecycle.
    let libs: Vec<(String, LoadedProfiles)> = mcc::definition_space()
        .libs()
        .map(|(name, b)| (name, b.profiles))
        .filter(|(_, p)| !p.cards.is_empty() || !p.invalid.is_empty())
        .collect();

    // §2 world join (top resolution mirrors `show pwr`).
    let (entry_uri, top) = if let Some(f) = target_path(args) {
        let p = Path::new(f);
        if p.is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )
            .unwrap_or_else(|e| {
                die!("mcc::show", 1, "directory target: {:#}", e);
            })
        } else {
            let path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(p)
            };
            (path.to_string_lossy().to_string(), None)
        }
    } else {
        (String::new(), None)
    };
    let top = crate::cmds::common::resolve_top_module(&entry_uri, top).unwrap_or_else(|| {
        die!(
            "mcc::show",
            1,
            "no modules found\nhint: load a file with -F or use --top"
        );
    });
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top.clone()));

    let (_tree, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&mcc::McIds::from(top.clone()), &uri, 1000).unwrap_or_else(
            |e| {
                die!("mcc::show", 1, "{e}");
            },
        );

    // One row per (owner instance, adoption lane): the card join key is the
    // lane's (family, role) face; a role-face no card covers stays visible
    // as not-curated (worldmodel §7 `none` doctrine: say what is missing).
    let mut rows: Vec<(
        String,
        String,
        String,
        Option<&mcc::model_profile::ModelProfileCard>,
        String,
    )> = Vec::new();
    for (_, entry) in table.iter() {
        let Some(lane) = entry.iface_lane.as_ref() else {
            continue;
        };
        let (owner_path, owner_class) = match entry.parent_id.and_then(|id| table.get_entry(id)) {
            Some(owner) => (owner.path.clone(), owner.class_name.clone()),
            None => (entry.path.clone(), entry.class_name.clone()),
        };
        let role = lane.role.clone().unwrap_or_default();
        let card = libs
            .iter()
            .filter_map(|(_, p)| p.card_for(&lane.family, &role))
            .next();
        rows.push((
            owner_path,
            owner_class,
            format!("{}::{}", lane.family, lane.role.clone().unwrap_or_default()),
            card,
            lane.lane.clone(),
        ));
    }
    rows.sort_by(|a, b| (&a.0, &a.1, &a.2, &a.4).cmp(&(&b.0, &b.1, &b.2, &b.4)));
    rows.dedup_by(|a, b| a.0 == b.0 && a.4 == b.4 && a.2 == b.2);

    if matches!(mcc::cli::globals().format, OutputFormat::Text) {
        let mut lines = Vec::new();
        lines.push(format!("===== Sim Model Profiles: {top} ====="));
        lines.push(String::new());
        lines.push(format!("-- cards ({})", libs.len()));
        for (lib, p) in &libs {
            for c in &p.cards {
                let model = c.model.as_deref().unwrap_or("-");
                lines.push(format!(
                    "  {lib}: {:<22} {:<9} {:<22} missing: {}",
                    c.key,
                    tier_word(c.tier),
                    model,
                    if c.missing.is_empty() {
                        "-".to_string()
                    } else {
                        c.missing.join(", ")
                    },
                ));
            }
            for bad in &p.invalid {
                lines.push(format!("  {lib}: INVALID {} : {}", bad.file, bad.error));
            }
        }
        lines.push(String::new());
        lines.push(format!("-- instances ({})", rows.len()));
        for (owner, class, face, card, _) in &rows {
            let resolution = match card {
                Some(c) => format!(
                    "{} {}{}",
                    tier_word(c.tier),
                    c.model.as_deref().unwrap_or(""),
                    if c.missing.is_empty() {
                        String::new()
                    } else {
                        format!("  missing: {}", c.missing.join(", "))
                    }
                ),
                None => "not-curated (no card for this role face)".to_string(),
            };
            lines.push(format!("  {owner:<28} {class:<22} {face:<24} {resolution}"));
        }
        let rendered = lines.join("\n");
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, rendered)?;
        } else {
            println!("{rendered}");
        }
        return Ok(());
    }

    let tier_json = |t: ProfileTier| tier_word(t).to_string();
    let data = json!({
        "type": "sim",
        "format": "model-profile/v1",
        "top": top,
        "cards": libs.iter().map(|(lib, p)| json!({
            "lib": lib,
            "cards": p.cards.iter().map(|c| json!({
                "key": c.key,
                "tier": tier_json(c.tier),
                "model": c.model,
                "pins": c.pins.iter().map(|pin| json!({
                    "pin": pin.pin,
                    "kind": pin.kind,
                    "source": pin.source,
                })).collect::<Vec<_>>(),
                "boundary_on": c.boundary_on,
                "consumes": c.consumes,
                "assumptions": c.assumptions,
                "missing": c.missing,
                "external": c.external,
                "notes": c.notes,
            })).collect::<Vec<_>>(),
            "invalid": p.invalid.iter().map(|b| json!({
                "file": b.file,
                "error": b.error,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "instances": rows.iter().map(|(owner, class, face, card, lane)| json!({
            "owner": owner,
            "class": class,
            "face": face,
            "lane": lane,
            "card": card.map(|c| json!({
                "key": c.key,
                "tier": tier_json(c.tier),
                "model": c.model,
                "missing": c.missing,
            })),
            "curated": card.is_some(),
        })).collect::<Vec<_>>(),
    });
    emit_show(args.target, &data, args.span)
}

/// The canonical tier word, as the card file spells it.
fn tier_word(t: mcc::model_profile::ProfileTier) -> &'static str {
    match t {
        mcc::model_profile::ProfileTier::Descend => "descend",
        mcc::model_profile::ProfileTier::Derive => "derive",
        mcc::model_profile::ProfileTier::Boundary => "boundary",
        mcc::model_profile::ProfileTier::Host => "host",
        mcc::model_profile::ProfileTier::None => "none",
    }
}

// `show pwrflow` — derived power-flow single view

/// Render `mcc show pwrflow`: the compiler-generated top-level power-flow
/// single view (`semantic/validation/pwrflow.rs`; design
/// `flow-single-view-design.md`). Text mode emits
/// the three projected sections — §1 world crowns, §2 rail contract table,
/// §3 supply tree — with `--full` widening rail contract columns and `--decaps`
/// unfolding folded decoupler annotations; JSON mode emits the whole typed view
/// under `{"type":"pwrflow","format":"power-flow/v1",…}` for tooling.
fn show_pwrflow(args: &ShowArgs) -> Result<()> {
    // Top-module resolution mirrors `show_pwr` (file/dir positional with `-F`
    // override; `--top` selects the module within the loaded set).
    let (entry_uri, top) = if let Some(f) = target_path(args) {
        let p = Path::new(f);
        if p.is_dir() {
            crate::cmds::common::load_target(
                Some(f),
                mcc::cli::globals().top.as_deref(),
                mcc::cli::globals().entry.as_deref(),
            )
            .unwrap_or_else(|e| {
                die!("mcc::show", 1, "directory target: {:#}", e);
            })
        } else {
            let path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(p)
            };
            (path.to_string_lossy().to_string(), None)
        }
    } else {
        (String::new(), None)
    };
    let top = crate::cmds::common::resolve_top_module(&entry_uri, top).unwrap_or_else(|| {
        die!(
            "mcc::show",
            1,
            "no modules found\nhint: load a file with -F or use --top"
        );
    });
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top.clone()));

    let (tree, table, arena, store) =
        mcc::mcc_build_flat_with_arena(&mcc::McIds::from(top.clone()), &uri, 1000).unwrap_or_else(
            |e| {
                die!("mcc::show", 1, "{e}");
            },
        );
    // Only the flat table feeds the derived view; the modeling tree/arena are
    // kept alive for the build's lifetime but not rendered here.
    let _ = (&tree, &arena, &store);

    let flow = mcc::build_pwrflow(&table, &top).unwrap_or_else(|e| {
        die!("mcc::show", 1, "pwrflow: {e}");
    });

    if matches!(mcc::cli::globals().format, OutputFormat::Text) {
        let mut lines = Vec::new();
        render_pwrflow_sections(&flow, args, &mut lines);
        let rendered = lines.join("\n");
        if let Some(path) = &mcc::cli::globals().output {
            std::fs::write(path, rendered)?;
        } else {
            println!("{rendered}");
        }
        return Ok(());
    }

    let data = pwrflow_json(&flow);
    emit_show(args.target, &data, args.span)
}

// `show stage` — one pipeline segment as data (stage-readout-design §5.3 ①)

/// Load `target` and build one segment of its pipeline as a [`StageView`].
///
/// This is the whole body of `show stage <seg>` minus the printing, and it is
/// shared rather than duplicated because `mcc diff <A> <B> --view <seg>` has to
/// produce **the same reading** of each operand that `show stage` would produce
/// of it: a difference taken over worlds built one way against a `show` that
/// builds them another is not a difference between two states of one thing. The
/// load order, the top resolution and the per-segment construction therefore
/// live here once (design §5.3 ruling ③, the same reason `join` builds its views
/// out of the same blocks `show stage` does).
///
/// `target` is the operand itself — the caller decides how it was spelled
/// (`-F` / the cwd manifest for `show`, the positional for `diff`).
///
/// Failures come back as an `Err` instead of dying here: `die!`'s tracing
/// target must be a literal, and only the caller knows which command ran. The
/// message texts are the ones each caller prints verbatim.
///
/// Calling this **more than once in one process** is the point: `mcc::mcc_init_no_lib`
/// through `init_local` resets the engine, so each call reads a world of its own
/// rather than accumulating onto the previous one.
pub(crate) fn build_stage_view(
    seg: mcc::stages::StageSeg,
    target: Option<&str>,
) -> Result<mcc::stages::StageView> {
    let (mut entry_uri, resolved_top) = crate::cmds::common::load_target(
        target,
        mcc::cli::globals().top.as_deref(),
        mcc::cli::globals().entry.as_deref(),
    )?;
    if entry_uri.is_empty() {
        // Nothing loaded from a path: use the URI of the module that is loaded.
        entry_uri = mcc::mcb_iter_modules()
            .iter()
            .find(|(n, _)| Some(n.clone()) == resolved_top.clone())
            .map(|(_, u)| u.to_string())
            .or_else(|| mcc::mcb_iter_modules().first().map(|(_, u)| u.to_string()))
            .unwrap_or_default();
    }
    let top = resolved_top
        .or_else(|| {
            crate::cmds::common::resolve_top_module(&entry_uri, mcc::cli::globals().top.clone())
        })
        .ok_or_else(|| {
            // A `Result` rather than a `die!` here: the two callers are
            // different commands, and `die!`'s tracing target has to be a
            // literal, so the dying stays at the call sites that know which
            // command this is.
            anyhow::anyhow!("no modules found\nhint: load a file with -F or use --top")
        })?;

    // `build_tree_diags` rather than `build_tree`: its diagnostics feed the
    // text face's second line as a *count*. Walking in through the shared
    // export entry gets the panic guard and the top-module resolution for free.
    let (tree, table, arena, store, diags) =
        mcc::export::build_tree_diags(&entry_uri, Some(top.as_str()), &mcc::cli::globals().lib)
            .map_err(|e| anyhow::anyhow!("stage: {e}"))?;

    let loaded = mcc::stages::read::Loaded::new(tree, table, arena, store, &top, diags.len());
    Ok(mcc::stages::read::build_segment(seg, &loaded))
}

/// Render `mcc show stage <p1|p2|vec|viz>`: one segment of the compile
/// pipeline, read out as a projection envelope.
///
/// The `<name>` positional is the **segment**, not an entity — which is why
/// [`target_path`] deliberately does not list `Stage`: the file comes from
/// `-F`, or from the current directory's project manifest, the same way it does
/// for `show all` / `show dianlu`.
///
/// Two faces from one `items` (design §5.3 ruling ③): `-f text` prints the
/// fixed-width table, and every machine format prints the envelope. The text
/// face obeys §5.3's four prohibitions — no ANSI, no box drawing, no
/// tab-delimited columns, and a missing value printed as `-`; and it is a
/// **readout**: the diagnostics count is a number in the header, never a gate,
/// so the exit code stays 0 (law C).
fn show_stage(args: &ShowArgs) -> Result<()> {
    let seg_name = args.name.as_deref().unwrap_or("p2");
    let Some(seg) = mcc::stages::StageSeg::parse(seg_name) else {
        // A bad *argument* is not a judged readout: this one may fail loudly.
        die!(
            "mcc::show",
            2,
            "unknown stage segment '{seg_name}'\nexpected one of: p1 | p2 | vec | viz"
        );
    };

    // The slice face is a `stage.viz` reading today: the selector vocabulary
    // (net name / intent family) is that view's. Another segment given a
    // selector is a caller error, not a silently ignored flag.
    if (!args.select.is_empty() || !args.exclude.is_empty()) && seg != mcc::stages::StageSeg::Viz {
        die!(
            "mcc::show",
            2,
            "--select / --exclude slice the drawn subgraph; only 'viz' accepts them"
        );
    }

    // The file: `-F` wins, else the cwd manifest that `prepare` already loaded
    // through. A directory resolves to its manifest entry file.
    let target = crate::cmds::manifest::effective_target(args.file.as_deref());
    let mut view = match build_stage_view(seg, target.as_deref()) {
        Ok(v) => v,
        Err(e) => die!("mcc::show", 1, "{e}"),
    };
    if seg == mcc::stages::StageSeg::Viz {
        let selection = match mcc::stages::slice::Selection::compile(&args.select, &args.exclude) {
            Ok(s) => s,
            Err(e) => die!("mcc::show", 2, "{e}"),
        };
        view = match mcc::stages::slice::apply_viz(view, &selection) {
            Ok(v) => v,
            Err(e) => die!("mcc::show", 2, "{e}"),
        };
    }

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face on purpose: `emit_envelope` has no
        // CSV arm either, and a fixed-width readout is not CSV-safe (a path may
        // contain a comma), so a real CSV face would be a separate decision
        // rather than something to fake here.
        let rendered = match seg {
            mcc::stages::StageSeg::P2 => mcc::stages::p2::render_p2_text(&view),
            mcc::stages::StageSeg::Vec => mcc::stages::vec::render_vec_text(&view),
            mcc::stages::StageSeg::Viz => mcc::stages::viz::render_viz_text(&view),
            _ => format!("{}\n{}", view.header_line(), view.counts_line(seg)),
        };
        return write_stage_text(&rendered);
    }
    emit_stage_envelope(&view, "mcc show stage")
}

/// `show org-units`: the **organization directory** of the current definition
/// space (organization-units-design.md §8; CIMP §1 U120).
///
/// One `items` array, every unit under its own key: the definition kinds by
/// `(uri, ident)` (a func as its host's member, design §12.1), a bus by its
/// name in its host (T12: no `DefId`), a clause by its position (§1: no
/// declaration object). The rows themselves — and the rule that each unit
/// keeps its own key — are built by [`mcc::query::units`], which `list
/// func|bus|clause`, `query --kind func|bus|clause` and the `show.org-units`
/// RPC method read too, so no two faces can describe one unit two ways.
///
/// The view is **derived, read-only and issues no id** (§8.1): it enumerates
/// the definition space through the registry's one read and follows the
/// carriers already there. It holds no cross-space correspondence, which is
/// what keeps it a directory rather than the table §0.1 forbids.
///
/// Items are ordered by `(kind, canonical key)`: the kinds in the registry's
/// display order, each kind by its canonical key, so inserting one def cannot
/// reshuffle the artifact. `counts` names the nine kind words — there is no
/// `diagnostics` word here, because this view reads the definition space and a
/// definition space carries no diagnostics: a `diagnostics 0` line would read
/// as a measurement rather than as "not applicable".
fn show_org_units(_args: &ShowArgs) -> Result<()> {
    let items = org_unit_items();
    let counts = org_unit_counts(&items);

    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_default();
    let view = mcc::stages::StageView::with_view(mcc::stages::ORG_UNITS_VIEW, &top, items, counts);

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face for the same reason `show stage`
        // does: a fixed-width readout is not CSV-safe (a path may contain a
        // comma), so a real CSV face would be a decision of its own.
        return write_stage_text(&render_org_units_text(&view));
    }
    emit_stage_envelope(&view, "mcc show org-units")
}

/// The `org-units` text face, rendered from the **same** items the envelope
/// carries: header, the count words, then one line per row — `class`, `key`,
/// `loc`. §5.3's four prohibitions hold: no ANSI, no box drawing, no
/// tab-delimited columns, and a missing value printed as `-`.
fn render_org_units_text(view: &mcc::stages::StageView) -> String {
    let mut lines = vec![view.header_line()];
    let words: Vec<String> = view
        .counts
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k} {}", v.as_u64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    lines.push(format!("# {}", words.join("  ")));
    let widest = |f: fn(&Value) -> String| -> usize {
        view.items.iter().map(|i| f(i).len()).max().unwrap_or(0)
    };
    let class_col = widest(|i| i["class"].as_str().unwrap_or("-").to_string()).max(5);
    let key_col = widest(|i| i["key"].as_str().unwrap_or("-").to_string()).max(3);
    lines.push(format!(
        "{:<class_col$}  {:<key_col$}  {}",
        "class", "key", "loc"
    ));
    for it in &view.items {
        lines.push(format!(
            "{:<class_col$}  {:<key_col$}  {}",
            it["class"].as_str().unwrap_or("-"),
            it["key"].as_str().unwrap_or("-"),
            mcc::stages::loc_cell(&it["loc"]),
        ));
    }
    lines.join("\n")
}

/// The `diagnostics` read face (projection-schema-design.md §2.4; CIMP §1
/// U280): the collected diagnostics of the loaded world as one projection
/// envelope.
///
/// The read mirrors `mcc check`'s per-world collection: the entry resolved
/// the way `check_one_world` resolves it, one tolerated flat pass2 run (its
/// net/ERC findings land in the workspace store), then the store as a whole.
/// The view builder dedups and sorts, so the envelope is a function of the
/// world rather than of the store's append order.
///
/// law C holds here in both directions: the flat run's `Err` is tolerated so
/// a world that cannot flatten still reports what pass1 collected, and the
/// face's exit code stays 0 however many errors the items carry — this is a
/// readout, not a gate.
fn show_diagnostics(loaded: Option<&str>) -> Result<()> {
    let mod_name = loaded
        .and_then(|uri| mcc::mcb_get_module_name_by_uri(&mcc::McURI::from(uri)))
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| "main".to_string());
    if let Some(uri) = loaded {
        let entry = mcc::McSpaceName {
            ident: mcc::McIds::from(mod_name.as_str()),
            uri: mcc::uri_intern(uri),
        };
        let _ = mcc::mcb_pass2_flat(&entry, 1);
    }
    let diags = mcc::mcc_diagnose_all();

    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_default();
    let view = mcc::stages::diagview::diagnostics_view(&top, &diags);

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face for the same reason `show stage`
        // does: a fixed-width readout is not CSV-safe (a path may contain a
        // comma), so a real CSV face would be a decision of its own.
        return write_stage_text(&mcc::stages::diagview::render_diag_text(&view));
    }
    emit_stage_envelope(&view, "mcc show diagnostics")
}

/// The `netlist` read face (projection-schema-design.md §2.2; CIMP §1 U280):
/// the flattening's connectivity as one projection envelope.
///
/// The read mirrors the export face's own: one flat pass2 run, then the
/// copper islands off the flat table — the same builder
/// ([`mcc::stages::netlistview::net_items`]) the JSON export consumes, so the
/// two faces cannot spell the connectivity two ways. law C holds as on the
/// other views: the exit code stays 0 whatever the items hold.
fn show_netlist(loaded: Option<&str>) -> Result<()> {
    let mod_name = loaded
        .and_then(|uri| mcc::mcb_get_module_name_by_uri(&mcc::McURI::from(uri)))
        .or_else(mcc::mcb_get_first_module_name)
        .unwrap_or_else(|| "main".to_string());
    let top = mcc::cli::globals()
        .top
        .clone()
        .unwrap_or_else(|| mod_name.clone());
    let Some(uri) = loaded else {
        return Ok(());
    };
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(mod_name.as_str()),
        uri: mcc::uri_intern(uri),
    };
    // A flattening that did not happen leaves nothing to read — fail loudly
    // rather than print an empty reading and say nothing (the U93 split).
    let (_tree, table) = match mcc::mcb_pass2_flat(&entry, 1) {
        Ok(pair) => pair,
        Err(e) => die!("mcc::show", 1, "netlist: flat pass2 failed: {e}"),
    };
    let view = mcc::stages::netlistview::netlist_view(&top, &table);

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face for the same reason `show stage`
        // does: a fixed-width readout is not CSV-safe (a path may contain a
        // comma), so a real CSV face would be a decision of its own.
        return write_stage_text(&mcc::stages::netlistview::render_netlist_text(&view));
    }
    emit_stage_envelope(&view, "mcc show netlist")
}

/// The `project-model` read face (projection-schema-design.md §2.1; CIMP §1
/// U280): the hierarchical module/instance tree as one projection envelope.
///
/// The read is the full-fidelity flat build ([`mcc::mcb_pass2_flat_with`]) —
/// the same workspace-grounded one-read the `netlist` face uses, kept whole
/// instead of flattened away, because this view *is* the hierarchy. One
/// builder ([`mcc::stages::projmodel::project_model_view`]) feeds both faces;
/// law C holds as on the other views: the exit code stays 0 whatever the
/// items hold.
fn show_project(loaded: Option<&str>) -> Result<()> {
    let Some(uri) = loaded else {
        return Ok(());
    };
    // The top resolves the way every read face resolves it (`read::load`): an
    // explicit `--top`, else the workspace's first module — a per-file module
    // lookup here would pick by intern order and split the faces.
    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .or_else(|| {
            loaded.and_then(|uri| mcc::mcb_get_module_name_by_uri(&mcc::McURI::from(uri)))
        })
        .unwrap_or_else(|| "main".to_string());
    let entry = mcc::McSpaceName {
        ident: mcc::McIds::from(top.as_str()),
        uri: mcc::uri_intern(uri),
    };
    // A flattening that did not happen leaves nothing to read — fail loudly
    // rather than print an empty reading and say nothing (the U93 split).
    let (tree, table, arena, store, diags) = match mcc::mcb_pass2_flat_with(&entry, 1, None) {
        Ok(parts) => parts,
        Err(e) => die!("mcc::show", 1, "project: flat pass2 failed: {e}"),
    };
    let loaded = mcc::stages::read::Loaded::new(tree, table, arena, store, &top, diags.len());
    let view = mcc::stages::projmodel::project_model_view(&loaded);

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face for the same reason `show stage`
        // does: a fixed-width readout is not CSV-safe (a path may contain a
        // comma), so a real CSV face would be a decision of its own.
        return write_stage_text(&mcc::stages::projmodel::render_project_model_text(
            &view,
        ));
    }
    emit_stage_envelope(&view, "mcc show project")
}

/// Write the stage text face to `--output` or stdout, the same way
/// [`show_pwrflow`] does: this face is ours, so it does not go through
/// [`crate::output::emit_envelope`]'s prose renderer.
fn write_stage_text(rendered: &str) -> Result<()> {
    if let Some(path) = &mcc::cli::globals().output {
        std::fs::write(path, format!("{rendered}\n"))?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

/// Emit the stage view through the standard envelope channel.
///
/// A new `view` value on the existing envelope, not a second envelope format
/// (design §3 law B / §5.3 ruling 2).
///
/// law C holds mechanically here: [`crate::output::emit_envelope`] is a pure
/// serializer and never touches the exit code, so a view's diagnostic count
/// cannot veto the readout.
fn emit_stage_envelope(view: &mcc::stages::StageView, command: &'static str) -> Result<()> {
    let mut builder = crate::output::builder::ResultBuilder::start(command);
    builder.set_stage(crate::output::envelope::StageViewData::from(view));
    let env = crate::output::envelope::Envelope::ok(builder.finish());
    // `-o` belongs to the command, not to one of its faces: the text face of
    // `show stage` already writes the file, so the structured faces must too.
    // Hardcoding `None` here made one command behave two ways depending on `-f`.
    crate::output::emit_envelope(
        &env,
        mcc::cli::globals().format,
        mcc::cli::globals()
            .output
            .as_deref()
            .map(std::path::Path::new),
        true,
    )
}

/// §5 three-section text projection. Section order and column meaning follow
/// the design doc's target output; every line is derived, none hand-written.
fn render_pwrflow_sections(flow: &mcc::PwrFlow, args: &ShowArgs, lines: &mut Vec<String>) {
    lines.push(format!("===== Power Flow: {} =====", flow.top));
    lines.push(String::new());

    // [1] world crowns
    lines.push("── [1] World crowns (role → return copper; EARTH carries no DC) ──".to_string());
    for c in &flow.crown {
        let star = if c.star { "*" } else { "" };
        lines.push(format!(
            "  {:<10} {}{:<14} {}",
            c.world,
            c.copper,
            star,
            crown_hint(c)
        ));
    }
    lines.push(String::new());

    // [2] rail contract table
    lines.push("── [2] Rail contracts ──────────────────────────────────".to_string());
    if args.full {
        lines.push(format!(
            "  {:<8} {:<20} {:<20} {:<18} {:<10} loads",
            "rail", "hot/ret", "gen", "contract", "world"
        ));
        for r in &flow.rails {
            lines.push(format!(
                "  {:<8} {:<20} {:<20} {:<18} {:<10} {}",
                r.domain,
                format!("{} / {}", r.hot, r.ret),
                r.gen,
                rail_contract_text(r, args.full),
                r.world,
                rail_suffix(r, args)
            ));
        }
    } else {
        lines.push(format!(
            "  {:<8} {:<20} {:<20} {:<12} {:<10} {}",
            "rail", "hot/ret", "gen", "contract", "world", "note"
        ));
        for r in &flow.rails {
            lines.push(format!(
                "  {:<8} {:<20} {:<20} {:<12} {:<10} {}",
                r.domain,
                format!("{} / {}", r.hot, r.ret),
                r.gen,
                rail_contract_text(r, false),
                r.world,
                rail_suffix(r, args)
            ));
        }
    }
    lines.push(String::new());

    // [3] supply tree
    lines.push("── [3] Supply tree (fan-out indent; cross-world = return change) ──".to_string());
    if flow.roots.is_empty() {
        lines.push("  (no supply roots derived)".to_string());
    }
    for (i, root) in flow.roots.iter().enumerate() {
        if i > 0 {
            lines.push(String::new());
        }
        render_flow_node(root, "", true, lines);
    }
    lines.push(String::new());
}

/// Rail `contract` column: `DC {v_text}` plus `±tol` / capacity / eff when
/// `--full` (design §4 trim ruling: default trimmed, detail back to `show pwr`).
fn rail_contract_text(r: &mcc::RailRow, full: bool) -> String {
    let mut s = format!("DC {}", r.v_text);
    if full {
        if let Some(t) = r.tol {
            s.push_str(&format!(" ±{t}%"));
        }
        if let Some(c) = r.capacity_amps {
            s.push_str(&format!(" {c}A"));
        }
        if let Some(e) = r.eff {
            s.push_str(&format!(" eff {e}"));
        }
    }
    s
}

/// Trailing annotation of a rail row: world cross + loads + decaps.
fn rail_suffix(r: &mcc::RailRow, args: &ShowArgs) -> String {
    let mut parts: Vec<String> = Vec::new();
    if r.cross_world {
        parts.push("cross-world".to_string());
    }
    if args.decaps && !r.decaps.is_empty() {
        parts.push(format!("(decaps: {})", r.decaps.join(", ")));
    } else if !r.decaps.is_empty() {
        parts.push(format!("(×{} decaps)", r.decaps.len()));
    }
    if args.full && !r.loads.is_empty() {
        parts.push(format!("loads: {}", r.loads.join(", ")));
    }
    parts.join(" ")
}

/// One-line hint for a crown row (what this return copper carries).
fn crown_hint(c: &mcc::CrownRow) -> &'static str {
    match c.world.as_str() {
        "quiet" => "analog sub-plane",
        "isolated" => "isolated secondary",
        "earth" => "chassis",
        "protective" => "protective",
        _ => {
            if c.rail_return {
                "digital main reference"
            } else {
                ""
            }
        }
    }
}

/// Recursive fan-out text for one tree node (`│  ` / `├─ ` / `└─ ` scaffold).
fn render_flow_node(node: &mcc::FlowNode, prefix: &str, last: bool, lines: &mut Vec<String>) {
    let arm = if last { "└─ " } else { "├─ " };
    let mut label = node.label.clone();
    let mut tags: Vec<String> = Vec::new();
    if let Some(v) = &node.via {
        label = format!("{v} ─→ {label}");
    }
    if let Some(w) = &node.world {
        tags.push(format!("world {w}"));
    }
    if node.cross_world {
        tags.push("cross-world".to_string());
    }
    if !node.note.is_empty() {
        tags.push(node.note.clone());
    }
    if !tags.is_empty() {
        label = format!("{label}  ({})", tags.join(", "));
    }
    lines.push(format!("{prefix}{arm}{label}"));

    let child_prefix = format!("{prefix}{}", if last { "   " } else { "│  " });
    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_flow_node(child, &child_prefix, i + 1 == n, lines);
    }
}

/// §5 JSON projection: the same typed view under `power-flow/v1`, with the
/// §3 forest emitted as nested nodes (each with class/return-world/via + the
/// rail/bus/load kind tags the tooling needs).
fn pwrflow_json(flow: &mcc::PwrFlow) -> Value {
    json!({
        "type": "pwrflow",
        "format": "power-flow/v1",
        "top": flow.top,
        "crown": flow
            .crown
            .iter()
            .map(|c| json!({
                "copper": c.copper,
                "world": c.world,
                "star": c.star,
                "rail_return": c.rail_return,
            }))
            .collect::<Vec<_>>(),
        "rails": flow
            .rails
            .iter()
            .map(|r| json!({
                "domain": r.domain,
                "hot": r.hot,
                "ret": r.ret,
                "world": r.world,
                "v_text": r.v_text,
                "v": r.v,
                "tol": r.tol,
                "capacity_amps": r.capacity_amps,
                "eff": r.eff,
                "gen": r.gen,
                "cross_world": r.cross_world,
                "loads": r.loads,
                "decaps": r.decaps,
            }))
            .collect::<Vec<_>>(),
        // §5: the tree serializes flat — `nodes` carry class/identity/world,
        // `edges` (parent→child) carry the transition `via` and `cross_world`.
        "tree": flow_tree_json(&flow.roots, &flow.rails),
    })
}

/// §5 flat projection of the §3 forest: DFS-indexed `nodes` + `edges`. A rail
/// node additionally carries its declared contract (`domain`, nominal, budget)
/// joined from the rail rows by hot-net name.
fn flow_tree_json(roots: &[mcc::FlowNode], rails: &[mcc::RailRow]) -> Value {
    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut index = 0usize;

    fn walk(
        node: &mcc::FlowNode,
        parent: Option<usize>,
        rails: &[mcc::RailRow],
        nodes: &mut Vec<Value>,
        edges: &mut Vec<Value>,
        index: &mut usize,
    ) {
        let my = *index;
        *index += 1;
        let mut v = json!({
            "index": my,
            "class": node.class,
            "id": node.id,
            "label": node.label,
            "world": node.world,
            "note": node.note,
        });
        if node.class == "rail" {
            if let Some(r) = rails.iter().find(|r| r.hot == node.id) {
                v["contract"] = json!({
                    "domain": r.domain,
                    "v_text": r.v_text,
                    "v": r.v,
                    "tol": r.tol,
                    "capacity_amps": r.capacity_amps,
                    "eff": r.eff,
                });
            }
        }
        nodes.push(v);
        if let Some(p) = parent {
            edges.push(json!({
                "from": p,
                "to": my,
                "via": node.via,
                "cross_world": node.cross_world,
            }));
        }
        for child in &node.children {
            walk(child, Some(my), rails, nodes, edges, index);
        }
    }

    for root in roots {
        walk(root, None, rails, &mut nodes, &mut edges, &mut index);
    }
    json!({ "nodes": nodes, "edges": edges })
}

struct PwrNetAgg {
    name: String,
    members: Vec<Value>, // {member: rel path, role}
    points: Vec<String>,
}

/// The role of a direct Port/Label child of a module, when it is a power
/// endpoint (DC-face member or rail label): the flatten-inferred
/// Ground/Power `MemberInfo` role, else (bare rail labels carry no role) the
/// model-A ground convention — a `GND*` leaf. Returns `None` for signal rows
/// and `alias_of` collapses (non-physical spellings never reach the nets).
fn rail_member_role(child: &InstEntry) -> Option<&'static str> {
    if child.alias_of.is_some() {
        return None;
    }
    match &child.member_info {
        Some(mi) => match mi.role {
            MemberRole::Ground => Some("Ground"),
            MemberRole::Power => Some("Power"),
            MemberRole::Signal => None,
        },
        None => {
            let leaf = child.path.rsplit('.').next().unwrap_or("");
            if leaf.starts_with("GND") {
                Some("Ground")
            } else {
                None
            }
        }
    }
}

/// Group a module instance's own rail endpoints (the Ground/Power Port members
/// and rail labels registered directly under it) by the merged net each hangs
/// on, attaching the full net point set. Order is by first-registered net.
fn module_rail_nets(module_path: &str, table: &InstTable) -> Vec<PwrNetAgg> {
    let Some(module_id) = table.get_id_by_path(module_path) else {
        return Vec::new();
    };
    let mut order: Vec<u32> = Vec::new();
    let mut aggs: BTreeMap<u32, PwrNetAgg> = BTreeMap::new();
    for child in table.children_of(module_id) {
        if !matches!(child.kind, InstKind::Port | InstKind::Label) {
            continue;
        }
        let Some(role) = rail_member_role(child) else {
            continue;
        };
        let Some(net) = table.get_net_of(child.id) else {
            continue;
        };
        let rel = child
            .path
            .strip_prefix(module_path)
            .and_then(|s| s.strip_prefix('.'))
            .unwrap_or(&child.path)
            .to_string();
        if !aggs.contains_key(&net.id) {
            order.push(net.id);
            aggs.insert(
                net.id,
                PwrNetAgg {
                    name: if net.name.is_empty() {
                        format!("_net#{}", net.id)
                    } else {
                        net.name.clone()
                    },
                    members: Vec::new(),
                    points: Vec::new(),
                },
            );
        }
        aggs.get_mut(&net.id)
            .expect("just inserted")
            .members
            .push(json!({ "member": rel, "role": role }));
    }
    for (net_id, agg) in aggs.iter_mut() {
        if let Some(net) = table.get_net(*net_id) {
            let mut pts: Vec<String> = net
                .points
                .iter()
                .filter_map(|pid| table.get_entry(*pid).map(|e| e.path.clone()))
                .collect();
            pts.sort();
            pts.dedup();
            agg.points = pts;
        }
    }
    order
        .into_iter()
        .filter_map(|id| aggs.remove(&id))
        .collect()
}

/// Power contracts of a component def (`psrc/psnk/psbi ...::DC(...)` rows).
fn pwr_contracts(comp: &mcc::McComponentInst) -> Vec<Value> {
    comp.def
        .pins
        .pwr
        .iter()
        .map(|p| {
            let dir = format!("{:?}", p.dir).to_lowercase();
            let dc = p
                .params
                .iter()
                .map(|q| match &q.key {
                    Some(k) => format!("{k}:{}", q.text),
                    None => q.text.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            json!({ "dir": dir, "iface": p.iface, "hot": p.hot, "ret": p.ret, "dc": dc })
        })
        .collect()
}

/// Recursive power node: one module instance + its nested sub-modules.
fn pwr_node_json(
    inst: &mcc::McModuleInst,
    path: &str,
    view: &TreeView,
    table: &InstTable,
    ids: bool,
) -> Value {
    let subs: Vec<&mcc::McModuleInst> = view.sub_modules(inst).collect();
    let components: Vec<Value> = view
        .components(inst)
        .map(|c| {
            let contracts = pwr_contracts(c);
            let mut v = json!({
                "name": c.name,
                "class": c.def.name.to_string(),
            });
            if !contracts.is_empty() {
                v["power"] = json!(contracts);
            }
            if ids {
                v["node_id"] = json!(c.node_id.map(|n| n.0));
                v["def_id"] = json!(registry_def_id(
                    &c.def.name,
                    &c.def.uri,
                    mcc::DefKind::Component,
                ));
            }
            v
        })
        .collect();
    let rail_nets = module_rail_nets(path, table);
    let mut node = json!({
        "path": path,
        "kind": "module",
        "class": inst.def.name.to_string(),
        "decl": mcc::mcc_module_power_json(&inst.def),
        "rails": rail_nets
            .iter()
            .map(|a| json!({"net": a.name, "members": a.members, "points": a.points}))
            .collect::<Vec<_>>(),
        "components": components,
        "children": subs
            .iter()
            .map(|s| pwr_node_json(s, &format!("{path}.{}", s.name), view, table, ids))
            .collect::<Vec<_>>(),
    });
    if ids {
        node["node_id"] = json!(inst.node_id.map(|n| n.0));
        node["def_id"] = json!(registry_def_id(
            &inst.def.name,
            &inst.def_uri,
            mcc::DefKind::Module,
        ));
    }
    node
}

/// Text rendering of one module section; sub-modules follow as their own
/// sections below (mirrors `show dianlu`'s section layout).
fn render_pwr_section(
    inst: &mcc::McModuleInst,
    path: &str,
    view: &TreeView,
    table: &InstTable,
    lines: &mut Vec<String>,
    ids: bool,
) {
    let mut header = format!(
        "===== Pwr: {path} (module {}) =====",
        inst.def.name.to_string()
    );
    if ids {
        header.push(' ');
        header.push_str(&dianlu_id_tag(
            inst.node_id.map(|n| n.0),
            registry_def_id(&inst.def.name, &inst.def_uri, mcc::DefKind::Module),
        ));
    }
    lines.push(header);

    // Declared planes / rails / edges / identity rows (lib projection).
    let decl = mcc::mcc_module_power_json(&inst.def);
    let n_conduits = decl["conduits"].as_array().map_or(0, |a| a.len());
    let n_rails = decl["rails"].as_array().map_or(0, |a| a.len());
    let n_edges = decl["edges"].as_array().map_or(0, |a| a.len());
    let n_ident = decl["port_identities"].as_array().map_or(0, |a| a.len());
    if n_conduits + n_rails + n_edges + n_ident == 0 {
        lines.push("  (no power-intent declarations)".to_string());
    } else {
        if let Some(conduits) = decl["conduits"].as_array() {
            for r in conduits {
                let mut s = format!("  conduit {}", r["name"].as_str().unwrap_or(""));
                if let Some(role) = r["role"].as_str() {
                    s.push_str(&format!(" @role({role})"));
                }
                if r.get("star").and_then(|x| x.as_bool()).unwrap_or(false) {
                    s.push_str(" @star");
                }
                lines.push(s);
            }
        }
        if let Some(rails) = decl["rails"].as_array() {
            for r in rails {
                let mut s = format!(
                    "  rail [{hot} / {ret}] {domain}",
                    hot = r["hot"].as_str().unwrap_or(""),
                    ret = r["ret"].as_str().unwrap_or(""),
                    domain = r["domain"].as_str().unwrap_or("")
                );
                let mut params: Vec<String> = Vec::new();
                let v_text = r["v_text"].as_str().unwrap_or("");
                if !v_text.is_empty() {
                    params.push(v_text.to_string());
                }
                if let Some(t) = r["tol"].as_f64() {
                    params.push(format!("tol:±{}%", t * 100.0));
                }
                if let Some(a) = r["capacity_amps"].as_f64() {
                    params.push(format!("capacity:{}A", a));
                }
                if let Some(e) = r["eff"].as_f64() {
                    params.push(format!("eff:{}", e));
                }
                if !params.is_empty() {
                    s.push_str(&format!(" ::DC({})", params.join(", ")));
                }
                if let Some(bad) = r["bad"].as_str() {
                    s.push_str(&format!("  // bad: {bad}"));
                }
                lines.push(s);
            }
        }
        if let Some(edges) = decl["edges"].as_array() {
            for e in edges {
                let endpoints = e["endpoints"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                lines.push(format!(
                    "  edge @{} {}",
                    e["kind"].as_str().unwrap_or(""),
                    endpoints
                ));
            }
        }
        if let Some(identities) = decl["port_identities"].as_array() {
            for p in identities {
                let mut s = format!(
                    "  io {} {}",
                    p["kind"].as_str().unwrap_or(""),
                    p["name"].as_str().unwrap_or("")
                );
                if let Some(c) = p["class"].as_str() {
                    s.push_str(&format!(" @class({c})"));
                }
                if let Some(r) = p["return"].as_str() {
                    s.push_str(&format!(" @return({r})"));
                }
                if let Some(n) = p["noise"].as_str() {
                    s.push_str(&format!(" @noise({n})"));
                }
                if let Some(n) = p["nature"].as_str() {
                    s.push_str(&format!(" @nature({n})"));
                }
                if let Some(b) = p["bind_role"].as_str() {
                    s.push_str(&format!(" @bind_role({b})"));
                }
                if let Some(x) = p["exposed"].as_array() {
                    let vals: Vec<&str> = x.iter().filter_map(|v| v.as_str()).collect();
                    if !vals.is_empty() {
                        s.push_str(&format!(" @exposed({})", vals.join(",")));
                    }
                }
                lines.push(s);
            }
        }
    }

    // Rail members → merged nets (the core debug signal).
    let net_aggs = module_rail_nets(path, table);
    if net_aggs.is_empty() {
        lines.push("  (no rail members → nets)".to_string());
    } else {
        lines.push("  rail members -> nets:".to_string());
        for agg in &net_aggs {
            let members: Vec<String> = agg
                .members
                .iter()
                .map(|m| {
                    format!(
                        "{}[{}]",
                        m["member"].as_str().unwrap_or(""),
                        m["role"].as_str().unwrap_or("")
                    )
                })
                .collect();
            lines.push(format!(
                "    {}  ->  net \"{}\"  ({} pts)",
                members.join(", "),
                agg.name,
                agg.points.len()
            ));
            lines.push(format!("      [{}]", agg.points.join(", ")));
        }
    }

    // Component power contracts at this level.
    let comps: Vec<Value> = view
        .components(inst)
        .filter(|c| !c.def.pins.pwr.is_empty())
        .map(|c| {
            let contracts = pwr_contracts(c);
            let mut v = json!({
                "name": c.name,
                "class": c.def.name.to_string(),
                "power": contracts,
            });
            if ids {
                v["def_id"] = json!(registry_def_id(
                    &c.def.name,
                    &c.def.uri,
                    mcc::DefKind::Component,
                ));
            }
            v
        })
        .collect();
    if !comps.is_empty() {
        lines.push("  component power contracts:".to_string());
        for c in &comps {
            for p in c["power"].as_array().unwrap_or(&vec![]) {
                let dir = p["dir"].as_str().unwrap_or("");
                let hot = p["hot"].as_str().unwrap_or("");
                let ret = p["ret"].as_str().unwrap_or("");
                let dc = p["dc"].as_str().unwrap_or("");
                lines.push(format!(
                    "    {} {} {} [{hot} / {ret}]: {}",
                    c["name"].as_str().unwrap_or(""),
                    c["class"].as_str().unwrap_or(""),
                    dir,
                    dc
                ));
            }
        }
    }

    lines.push(String::new());
    for sub in view.sub_modules(inst) {
        render_pwr_section(
            sub,
            &format!("{path}.{}", sub.name),
            view,
            table,
            lines,
            ids,
        );
    }
}

/// The registry `DefId` of the def that `(name, uri)` names — the def-space
/// half of an instance identity, keyed exactly the way `instant/lane.rs`
/// builds its lookups. `None` when the def is not registered in the active
/// workspace (stub / degraded instance).
fn registry_def_id(name: &mcc::McIds, uri: &mcc::McURI, kind: mcc::DefKind) -> Option<u32> {
    mcc::def_id(&mcc::McSpaceName::new(name, uri.clone()), kind)
}

/// `--ids` tag for one instance: its node `NodeId` and the `DefId` of the def
/// it instantiates; `-` marks an id that is absent (node not added to the
/// tree / def not registered).
fn dianlu_id_tag(node: Option<u32>, def: Option<u32>) -> String {
    let n = node.map_or_else(|| "N-".to_string(), |v| format!("N{v}"));
    let d = def.map_or_else(|| "D-".to_string(), |v| format!("D{v}"));
    format!("[{n} {d}]")
}

/// The stable `DefMemberId` (raw value) of one live def member under the def
/// `(name, uri)` of `kind`, from the def registry's append-only member
/// ledger (invariant C / D13) — the def-space half of a lane physical point,
/// keyed the way `instant/lane.rs` builds its lookups. `None` when the def is
/// not registered or the member is not a live ledger entry (dynamic /
/// interface-derived pins stay off-ledger).
fn registry_member_id(
    name: &mcc::McIds,
    uri: &mcc::McURI,
    kind: mcc::DefKind,
    member: &str,
) -> Option<u32> {
    mcc::def_member_id_of(&mcc::McSpaceName::new(name, uri.clone()), kind, member).map(|m| m.0)
}

/// Lane-layer physical-point string `N<n>:<m>` of one pin/port (shown under
/// `--ids`): the owning instance node `NodeId` plus the pin's stable
/// def-member id (`PointId`, design §4 D1). `-` marks a part that is absent
/// (node missing / member off-ledger).
fn point_tag(node: Option<u32>, member: Option<u32>) -> String {
    let n = node.map_or_else(|| "N-".to_string(), |v| format!("N{v}"));
    let m = member.map_or_else(|| "-".to_string(), |v| v.to_string());
    format!("{n}:{m}")
}

/// Render one module section (text): instances then connections, recursing
/// into sub-modules as their own sections below. With `ids`, the section
/// header and every instance line carry a `[N<n> D<d>]` identity tag: the
/// node `NodeId` in the dianlu space plus the `DefId` of the def that the
/// instance instantiates in the definition space (`-` when either is absent).
/// `ids` also annotates each instance pin/port with its lane-layer physical
/// point `N<n>:<m>` (node + def-member id).
fn render_dianlu_section(
    inst: &mcc::McModuleInst,
    path: &str,
    view: &TreeView,
    store: &mcc::NetTableStore,
    lines: &mut Vec<String>,
    ids: bool,
) {
    let mut header = format!("===== Section: {path} (module) =====");
    if ids {
        header.push(' ');
        header.push_str(&dianlu_id_tag(
            inst.node_id.map(|n| n.0),
            registry_def_id(&inst.def.name, &inst.def_uri, mcc::DefKind::Module),
        ));
    }
    lines.push(header);
    lines.push("Instances:".to_string());

    for comp in view.components(inst) {
        let tag = if ids {
            format!(
                " {}",
                dianlu_id_tag(
                    comp.node_id.map(|n| n.0),
                    registry_def_id(&comp.def.name, &comp.def.uri, mcc::DefKind::Component,),
                )
            )
        } else {
            String::new()
        };
        let pins = if ids {
            comp_pin_points(comp)
                .into_iter()
                .map(|(_, label, member)| {
                    format!("{label}@{}", point_tag(comp.node_id.map(|n| n.0), member))
                })
                .collect::<Vec<_>>()
        } else {
            comp_pin_labels(comp)
        };
        lines.push(format!(
            "  [C]{} {}: {} [pins: {}]",
            tag,
            comp.name,
            comp.def.name,
            pins.join(", ")
        ));
    }
    let subs: Vec<&mcc::McModuleInst> = view.sub_modules(inst).collect();
    for sub in &subs {
        let tag = if ids {
            format!(
                " {}",
                dianlu_id_tag(
                    sub.node_id.map(|n| n.0),
                    registry_def_id(&sub.def.name, &sub.def_uri, mcc::DefKind::Module),
                )
            )
        } else {
            String::new()
        };
        let mut line = format!("  [M]{} {}: {}", tag, sub.name, sub.def.name);
        if ids {
            let ports = module_port_points(sub)
                .into_iter()
                .map(|(label, member)| {
                    format!("{label}@{}", point_tag(sub.node_id.map(|n| n.0), member))
                })
                .collect::<Vec<_>>();
            if !ports.is_empty() {
                line.push_str(&format!(" [ports: {}]", ports.join(", ")));
            }
        }
        lines.push(line);
    }

    let mut labels: Vec<&String> = store.labels_of(path).keys().collect();
    labels.sort();
    for label in labels {
        lines.push(format!("  [L] {label}"));
    }

    let mut buses: Vec<&mcc::McBusInst> = store.buses_of(path).values().collect();
    buses.sort_by(|a, b| a.name.cmp(&b.name));
    for bus in buses {
        let mut line = format!("  [B] {}{{{}}}", bus.name, bus.members.join(", "));
        if let Some(ty) = bus_interface_type(inst, bus, view) {
            line.push_str(&format!(" :: {ty}"));
        }
        lines.push(line);
    }
    // Interface buses projected by component instances (Pass2 keeps their
    // members as physical pins, so they are surfaced here synthetically).
    for (name, members, ty) in comp_interface_buses(inst, store, path, view) {
        lines.push(format!("  [B] {name}{{{}}} :: {ty}", members.join(", ")));
    }

    lines.push("Connections:".to_string());
    // §8.9.5 layered display (vocabulary trunk / lane / wire, rendered by
    // `cmds::common::render_layered_conns`): bus/interface member lanes that mate the same two ends
    // render as `[trunk] left <-> right` headers with numbered lane lines
    // underneath; everything else (independent connections) renders as
    // single `[wire]` lines.
    let views: Vec<crate::cmds::common::ConnView> = inst
        .connections
        .iter()
        .filter_map(|conn| {
            let net = conn.effective_net_name();
            if net == "NC" {
                return None;
            }
            let points: Vec<String> = conn
                .points
                .iter()
                .filter(|p| p.path != "NC")
                .map(|p| p.path.clone())
                .collect();
            if points.is_empty() {
                return None;
            }
            Some(crate::cmds::common::ConnView {
                net,
                points,
                dir: format!("{:?}", conn.dir),
                // §8.9.6: structured group context (name/member/kind); lanes
                // without a group context render as wire lines.
                trunk: conn.trunk.clone(),
            })
        })
        .collect();
    lines.extend(crate::cmds::common::render_layered_conns(&views, "  "));

    for sub in subs {
        lines.push(String::new());
        render_dianlu_section(
            sub,
            &format!("{path}.{}", sub.name),
            view,
            store,
            lines,
            ids,
        );
    }
}

/// Build the structured (JSON/YAML) representation: one section object per
/// module, in the same order as the text renderer. With `ids`, the section
/// and its instance entries carry `node_id` (dianlu space) / `def_id`
/// (definition space) identity keys, mirroring the text `[N… D…]` tags;
/// component entries also gain a `points` map and sub-module entries a
/// `ports` map: pin/port name → lane-layer physical point `N<n>:<m>`.
fn dianlu_sections(
    inst: &mcc::McModuleInst,
    path: &str,
    view: &TreeView,
    store: &mcc::NetTableStore,
    ids: bool,
) -> Vec<Value> {
    let subs: Vec<&mcc::McModuleInst> = view.sub_modules(inst).collect();
    let mut section = json!({
        "module": path,
        "uri": inst.def_uri.to_string(),
        "components": view.components(inst).map(|c| comp_json(c, ids)).collect::<Vec<_>>(),
        "connections": Vec::<Value>::new(),
    });
    section["sub_modules"] = json!(subs
        .iter()
        .map(|s| {
            let mut v = json!({
                "name": s.name,
                "class": s.def.name.to_string(),
            });
            if ids {
                v["node_id"] = json!(s.node_id.map(|n| n.0));
                v["def_id"] = json!(registry_def_id(
                    &s.def.name,
                    &s.def_uri,
                    mcc::DefKind::Module,
                ));
                let ports: std::collections::BTreeMap<String, String> = module_port_points(s)
                    .into_iter()
                    .map(|(label, member)| (label, point_tag(s.node_id.map(|n| n.0), member)))
                    .collect();
                if !ports.is_empty() {
                    v["ports"] = json!(ports);
                }
            }
            v
        })
        .collect::<Vec<_>>());
    if ids {
        section["node_id"] = json!(inst.node_id.map(|n| n.0));
        section["def_id"] = json!(registry_def_id(
            &inst.def.name,
            &inst.def_uri,
            mcc::DefKind::Module,
        ));
    }

    let mut labels: Vec<&String> = store.labels_of(path).keys().collect();
    labels.sort();
    section["labels"] = json!(labels);

    let mut buses: Vec<&mcc::McBusInst> = store.buses_of(path).values().collect();
    buses.sort_by(|a, b| a.name.cmp(&b.name));
    section["buses"] = json!(buses
        .iter()
        .map(|bus| {
            json!({
                "name": bus.name,
                "members": bus.members,
                "interface": bus_interface_type(inst, bus, view),
            })
        })
        .chain(
            comp_interface_buses(inst, store, path, view)
                .into_iter()
                .map(|(name, members, ty)| {
                    json!({
                        "name": name,
                        "members": members,
                        "interface": Some(ty),
                    })
                })
        )
        .collect::<Vec<_>>());

    let conns: Vec<Value> = inst
        .connections
        .iter()
        .filter_map(|conn| {
            let net = conn.effective_net_name();
            if net == "NC" {
                return None;
            }
            let points: Vec<&str> = conn
                .points
                .iter()
                .filter(|p| p.path != "NC")
                .map(|p| p.path.as_str())
                .collect();
            if points.is_empty() {
                return None;
            }
            Some(json!({
                "net": net,
                "points": points,
                "dir": format!("{:?}", conn.dir),
                // §8.9.6: structured group context (name/member/kind),
                // present only for bus/interface member connections so
                // downstream can layer.
                "trunk": conn.trunk.as_ref().map(|pg| pg.to_json_value()),
            }))
        })
        .collect();
    section["connections"] = json!(conns);

    let mut sections = vec![section];
    for sub in subs {
        sections.extend(dianlu_sections(
            sub,
            &format!("{path}.{}", sub.name),
            view,
            store,
            ids,
        ));
    }
    sections
}

/// Component instance as JSON: name, class, and sorted pin labels. With
/// `ids`, the entry also carries its node `NodeId` (`node_id`), the `DefId`
/// of the class it instantiates (`def_id`), and a `points` map adding each
/// pin's lane-layer physical point `N<n>:<m>`.
fn comp_json(comp: &mcc::McComponentInst, ids: bool) -> Value {
    let mut v = json!({
        "name": comp.name,
        "class": comp.def.name.to_string(),
        "pins": comp_pin_labels(comp),
    });
    if ids {
        v["node_id"] = json!(comp.node_id.map(|n| n.0));
        v["def_id"] = json!(registry_def_id(
            &comp.def.name,
            &comp.def.uri,
            mcc::DefKind::Component,
        ));
        let pts: std::collections::BTreeMap<String, String> = comp_pin_points(comp)
            .into_iter()
            .map(|(raw, _, member)| (raw, point_tag(comp.node_id.map(|n| n.0), member)))
            .collect();
        v["points"] = json!(pts);
    }
    v
}

/// The `(raw pin key, display label)` rows of one component instance's pins,
/// in display order: numeric pin ids first, then named pins (deterministic).
/// The label shows the physical id, with the longest readable alias in
/// parentheses when one exists.
fn comp_pin_rows(comp: &mcc::McComponentInst) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = comp
        .sorted_pin_ids()
        .into_iter()
        .map(|pid| {
            let alias = comp
                .cond_pin_names
                .get(pid)
                .and_then(|names| names.iter().max_by_key(|n| n.len()).cloned())
                .or_else(|| {
                    comp.def
                        .pins
                        .pin_id_to_names
                        .get(pid)
                        .and_then(|names| names.iter().max_by_key(|n| n.len()).cloned())
                });
            match alias {
                Some(n) if n.as_str() != pid.as_str() => (pid.clone(), format!("{pid}({n})")),
                _ => (pid.clone(), pid.clone()),
            }
        })
        .collect();
    rows.sort_by(|a, b| mcc::pin_id_cmp(&a.0, &b.0));
    rows
}

/// Preferred user-facing label per connected pin, sorted by pin id.
fn comp_pin_labels(comp: &mcc::McComponentInst) -> Vec<String> {
    comp_pin_rows(comp)
        .into_iter()
        .map(|(_, label)| label)
        .collect()
}

/// Rows of one component instance's pins under `--ids`: raw pin key, display
/// label, and the pin's stable def-member id (lane-layer physical point
/// `N<n>:<m>`), when the pin is a live ledger member of the instantiated def.
fn comp_pin_points(comp: &mcc::McComponentInst) -> Vec<(String, String, Option<u32>)> {
    comp_pin_rows(comp)
        .into_iter()
        .map(|(raw, label)| {
            let member =
                registry_member_id(&comp.def.name, &comp.def.uri, mcc::DefKind::Component, &raw);
            (raw, label, member)
        })
        .collect()
}

/// Rows of one sub-module instance's io ports under `--ids`: port name plus
/// its stable def-member id (lane-layer physical point on the module node).
fn module_port_points(sub: &mcc::McModuleInst) -> Vec<(String, Option<u32>)> {
    sub.ports
        .iter()
        .map(|p| {
            let member =
                registry_member_id(&sub.def.name, &sub.def_uri, mcc::DefKind::Module, &p.name);
            (p.name.clone(), member)
        })
        .collect()
}

/// Resolve an interface class annotation for a bus that projects a component
/// interface (e.g. bus `uC.UART0` → `UART.TTL(DCE)`). Plain buses return None.
fn bus_interface_type(
    inst: &mcc::McModuleInst,
    bus: &mcc::McBusInst,
    view: &TreeView,
) -> Option<String> {
    let (comp_name, member) = bus.name.split_once('.')?;
    let comp = view.components(inst).find(|c| c.name == comp_name)?;
    let mcc::McPinPort::Interface(iface) = comp.def.pins.names_to_id.get(member)? else {
        return None;
    };
    Some(iface_type_string(iface))
}

/// Synthetic interface buses projected by component instances: Pass2 keeps
/// interface members as physical pins, so a component binding such as
/// `io [1:2] = UART0::UART.TTL(DCE)` never registers a bus. Surface it as
/// `inst.IFACE{members}` with its interface class annotation. A bus that
/// already exists in the module's overlay fragment (prefixed form registered
/// by a function body) is skipped.
fn comp_interface_buses(
    inst: &mcc::McModuleInst,
    store: &mcc::NetTableStore,
    path: &str,
    view: &TreeView,
) -> Vec<(String, Vec<String>, String)> {
    let mut out = Vec::new();
    for comp in view.components(inst) {
        for (iface_name, port) in &comp.def.pins.names_to_id {
            let mcc::McPinPort::Interface(iface) = port else {
                continue;
            };
            // Anonymous interfaces (`[VCC, GND]::DC(...)`) carry their member
            // names inside the bracket key; surface the bus under the
            // component name alone (e.g. `PWR{VCC, GND}`).
            let name = if iface_name.starts_with('[') {
                comp.name.clone()
            } else {
                format!("{}.{}", comp.name, iface_name)
            };
            if store.buses_of(path).contains_key(&name) {
                continue;
            }
            out.push((name, iface_member_names(iface), iface_type_string(iface)));
        }
    }
    out.sort();
    out
}

/// Interface member names in declaration order: bracket-keyed anonymous
/// interfaces (`[VCC, GND]::DC(...)`) carry their member names inside the
/// interface name; otherwise use the declared pin name mapping, then the
/// interface instance members, then the interface definition's pins, then
/// the registered chip pin IDs.
fn iface_member_names(iface: &mcc::Mc2Interface) -> Vec<String> {
    let name = iface.name.to_string();
    if let Some(inner) = name.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let members: Vec<String> = inner.split(',').map(|m| m.trim().to_string()).collect();
        if !members.is_empty() {
            return members;
        }
    }
    if !iface.pin_name_mapping.is_empty() {
        return iface.pin_name_mapping.clone();
    }
    let insts: Vec<String> = iface.insts.iter().map(|m| m.id.to_string()).collect();
    if !insts.is_empty() {
        return insts;
    }
    let names: Vec<String> = iface.base.pins.names_to_id.keys().cloned().collect();
    if !names.is_empty() {
        return names;
    }
    iface.registered_pins.clone()
}

/// Interface class string with params, e.g. `UART.TTL(DCE)` / `I2C(Master)`.
fn iface_type_string(iface: &mcc::Mc2Interface) -> String {
    let base = iface.base_name();
    let params: Vec<String> = iface.params.iter().map(|p| p.to_string()).collect();
    if params.is_empty() {
        base
    } else {
        format!("{base}({})", params.join(", "))
    }
}

// Drill-down handlers

fn drill_pins(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let pins = match &cmie {
        mcc::McCMIE::Component(c) => &c.pins,
        mcc::McCMIE::Interface(i) => &i.pins,
        _ => not_applicable("pins", name),
    };
    let mut data = pins_json(pins);
    data["name"] = json!(name);
    emit_show(args.target, &data, args.span)
}

fn drill_ports(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Module(module) = &cmie else {
        not_applicable("ports", name);
    };
    let ports: Vec<Value> = module
        .insts
        .insts()
        .iter()
        .filter(|(_, (io_type, _))| {
            !matches!(
                io_type,
                mcc::IOType::None | mcc::IOType::Return | mcc::IOType::NonCon
            )
        })
        .map(|(pname, (io_type, inst))| {
            let (ptype, members) = port_type_members(inst);
            json!({
                "name": pname,
                "iotype": format!("{:?}", io_type),
                "type": ptype,
                "members": members,
            })
        })
        .collect();
    let data = json!({ "name": name, "port_count": ports.len(), "ports": ports });
    emit_show(args.target, &data, args.span)
}

/// Extract a port's type and sub-members from its instance:
/// - Interface ports: type = interface class name with params (e.g. `I2C(Master)`),
///   members = registered chip pin IDs when available.
/// - List/Bus ports: type = `list` / `bus`, members = declared member names
///   (e.g. `MIC{P,N}` → `P, N`).
/// - Component/Module ports: type = the class name.
/// - Bare ports: type = `pin`.
fn port_type_members(inst: &mcc::McInstance) -> (String, Vec<String>) {
    match inst {
        mcc::McInstance::Interface(i) => {
            let base = i.base_name();
            let params: Vec<String> = i.params.iter().map(|p| p.to_string()).collect();
            let ty = if params.is_empty() {
                base
            } else {
                format!("{base}({})", params.join(", "))
            };
            let members = if i.registered_pins.is_empty() {
                i.insts.iter().map(|m| m.id.to_string()).collect()
            } else {
                i.registered_pins.clone()
            };
            (ty, members)
        }
        mcc::McInstance::List(_) => ("list".to_string(), inst.members()),
        mcc::McInstance::Bus(_) => ("bus".to_string(), inst.members()),
        mcc::McInstance::Component(c) => (c.name.to_string(), Vec::new()),
        mcc::McInstance::Module(m) => (m.name.to_string(), Vec::new()),
        _ => ("pin".to_string(), Vec::new()),
    }
}

fn drill_labels(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Module(module) = &cmie else {
        not_applicable("labels", name);
    };
    let labels: Vec<String> = module
        .insts
        .iter()
        .filter(|(_, inst)| matches!(inst, mcc::McInstance::Label(_)))
        .map(|(n, _)| n.to_string())
        .collect();
    let data = json!({ "name": name, "label_count": labels.len(), "labels": labels });
    emit_show(args.target, &data, args.span)
}

fn drill_instances(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    match &cmie {
        mcc::McCMIE::Component(c) => {
            let items = instances_json(&c.insts, args.r#type.as_deref());
            let data = json!({ "name": name, "count": items.len(), "instances": items });
            emit_show(args.target, &data, args.span)
        }
        mcc::McCMIE::Module(_) => {
            // Source annotations (stage 5, design §4.5): build the module so
            // every instance carries its origin (src / decl / gen), the
            // declaration / call-site / func-body line, and the caller chain.
            let top = mcc::cli::globals()
                .top
                .clone()
                .unwrap_or_else(|| name.to_string());
            let uri = mcc::mcb_iter_modules()
                .iter()
                .find(|(n, _)| n == &top)
                .map(|(_, u)| mcc::McURI::from(u.as_str()))
                .unwrap_or_else(|| mcc::McURI::from(top.as_str()));
            let (inst, arena, store, net_store) =
                crate::cmds::common::build_pass2_with_arena(&top, &uri)
                    .map_err(anyhow::Error::msg)?;
            let content = std::fs::read_to_string(&inst.def_uri.to_string()).ok();
            let view = mcc::TreeView::new(&arena, &store);
            let fam =
                mcc::hierarchy::extract_instance_families(&inst, &top, &net_store, &content, &view);
            let mut items: Vec<Value> = Vec::new();
            for (n, k, l, cl, o) in fam.source {
                if args
                    .r#type
                    .as_deref()
                    .is_none_or(|t| k.eq_ignore_ascii_case(t))
                {
                    items.push(json!({
                        "name": n, "kind": k, "class": cl,
                        "origin": o, "line": l,
                    }));
                }
            }
            for (n, l, cl) in fam.declareb {
                items.push(json!({
                    "name": n, "kind": "declareb", "class": cl,
                    "origin": "decl", "line": l,
                }));
            }
            for (n, l, cl, caller) in fam.generated {
                items.push(json!({
                    "name": n, "kind": "component", "class": cl,
                    "origin": "gen", "line": l, "caller": caller,
                }));
            }
            items.sort_by_key(|e| e["line"].as_u64().unwrap_or(u64::MAX));
            let data = json!({ "name": name, "count": items.len(), "instances": items });
            emit_show(args.target, &data, args.span)
        }
        _ => not_applicable("instances", name),
    }
}

fn drill_nets(name: &str, args: &ShowArgs) -> Result<()> {
    // Func body nets: `OWNER.FUNC` — connection-line-level nets (no Pass2,
    // funcs depend on parameters and a calling context).
    if let Some(func) = mcc::rpc::handlers::find_func_by_path(name) {
        let nets = mcc::rpc::handlers::func_nets_map(&func);
        let items: Vec<Value> = nets
            .iter()
            .map(|(n, points)| json!({ "name": n, "points": points }))
            .collect();
        let data = json!({ "name": name, "kind": "func", "count": items.len(), "nets": items });
        return emit_show(args.target, &data, args.span);
    }

    // `nets <module>` uses the entity as the top module.
    let top = mcc::cli::globals()
        .top
        .clone()
        .unwrap_or_else(|| name.to_string());
    let nets = nets_map(&top);
    let items: Vec<Value> = nets
        .iter()
        .map(|(n, points)| json!({ "name": n, "points": points }))
        .collect();
    let data = json!({ "name": name, "count": items.len(), "nets": items });
    emit_show(args.target, &data, args.span)
}

fn drill_attrs(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let attrs = match &cmie {
        mcc::McCMIE::Component(c) => &c.attrs,
        mcc::McCMIE::Interface(i) => &i.attrs,
        _ => not_applicable("attrs", name),
    };
    let items: Vec<Value> = attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({ "no": a.no, "name": a.id.to_string(), "values": values })
        })
        .collect();
    let data = json!({ "name": name, "count": items.len(), "attrs": items });
    emit_show(args.target, &data, args.span)
}

fn drill_funcs(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let funcs = match &cmie {
        mcc::McCMIE::Component(c) => &c.funcs,
        mcc::McCMIE::Module(m) => &m.funcs,
        _ => not_applicable("funcs", name),
    };
    let items: Vec<Value> = funcs
        .iter()
        .map(|f| json!({ "name": f.name.to_string(), "params": f.params.names_full_annotated() }))
        .collect();
    let data = json!({ "name": name, "count": items.len(), "funcs": items });
    emit_show(args.target, &data, args.span)
}

fn drill_params(name: &str, args: &ShowArgs) -> Result<()> {
    // Func params: `OWNER.FUNC` (dot-qualified; funcs are not top-level defs).
    if let Some(func) = mcc::rpc::handlers::find_func_by_path(name) {
        let items: Vec<Value> = func.params.iter().map(|d| param_json(d)).collect();
        let data = json!({ "name": name, "kind": "func", "count": items.len(), "params": items });
        return emit_show(args.target, &data, args.span);
    }
    let cmie = def_or_exit(name);
    let params = match &cmie {
        mcc::McCMIE::Component(c) => &c.params,
        mcc::McCMIE::Module(m) => &m.params,
        mcc::McCMIE::Interface(i) => &i.params,
        _ => not_applicable("params", name),
    };
    let items: Vec<Value> = params.iter().map(param_json).collect();
    let arity = params.arity();
    let data = json!({
        "name": name,
        "count": items.len(),
        "required": arity.required,
        "optional": arity.optional,
        "params": items
    });
    emit_show(args.target, &data, args.span)
}

/// One parameter declaration as JSON, mirroring the RPC `show.params` shape.
/// `name` uses the display form so compound params render as `[VDD_3V3, GND]`.
fn param_json(d: &mcc::McParamDeclare) -> Value {
    let mut j = mcc::rpc::handlers::param_declare_to_json(d);
    j["name"] = json!(d.display_name());
    j
}

fn drill_roles(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Interface(iface) = &cmie else {
        not_applicable("roles", name);
    };
    let items: Vec<Value> = iface
        .roles
        .iter()
        .map(|r| {
            json!({
                "name": r.name.to_string(),
                "pins": pins_json(&r.pins),
            })
        })
        .collect();
    let data = json!({ "name": name, "count": items.len(), "roles": items });
    emit_show(args.target, &data, args.span)
}

fn drill_values(name: &str, args: &ShowArgs) -> Result<()> {
    let cmie = def_or_exit(name);
    let mcc::McCMIE::Enum(en) = &cmie else {
        not_applicable("values", name);
    };
    let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
    let data = json!({ "name": name, "count": values.len(), "values": values });
    emit_show(args.target, &data, args.span)
}

// Entity detail collection (used by `show all` file-layer text details)

/// Collect every entity defined in a single `.mc` file as full-field detail
/// values, sorted by source position so the output follows the file layout.
fn collect_dump_file(file: &str) -> Vec<Value> {
    let resolved = resolve_file(file);
    let file_uri = resolved.as_str();
    let mut all: Vec<Value> = Vec::new();

    // mcb_iter_* chains workspace + global tables, so dedup within each
    // category (a component and an enum can share a name+URI, e.g. mcode's
    // `component CAP` + `enum CAP` in cap.mc, so the set must not be shared).
    let mut seen_comp: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut seen_mod: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut seen_iface: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut seen_enum: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    for (name, cmie_uri) in mcc::mcb_iter_components() {
        if !seen_comp.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri)
        {
            continue;
        }
        let ident = McIds::from(name.as_str());
        let cmie = mcc::get_component_def(&ident, &McURI::from(cmie_uri.as_str()))
            .or_else(|| mcc::get_def(&ident, &McURI::from(cmie_uri.as_str())));
        if let Some(mcc::McCMIE::Component(comp)) = cmie {
            all.push(dump_component(&name, &comp));
        }
    }
    for (name, cmie_uri) in mcc::mcb_iter_modules() {
        if !seen_mod.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri) {
            continue;
        }
        let ident = McIds::from(name.as_str());
        if let Some(mcc::McCMIE::Module(module)) =
            mcc::get_def(&ident, &McURI::from(cmie_uri.as_str()))
        {
            all.push(dump_module(&name, &module));
        }
    }
    for (name, cmie_uri) in mcc::mcb_iter_interfaces() {
        if !seen_iface.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri)
        {
            continue;
        }
        let ident = McIds::from(name.as_str());
        if let Some(mcc::McCMIE::Interface(iface)) =
            mcc::get_def(&ident, &McURI::from(cmie_uri.as_str()))
        {
            all.push(dump_interface(&name, &iface));
        }
    }
    for (name, cmie_uri) in mcc::mcb_iter_enums() {
        if !seen_enum.insert((name.clone(), cmie_uri.clone())) || !uri_matches(&cmie_uri, file_uri)
        {
            continue;
        }
        let ident = McIds::from(name.as_str());
        if let Some(mcc::McCMIE::Enum(en)) = mcc::get_def(&ident, &McURI::from(cmie_uri.as_str())) {
            all.push(dump_enum(&name, &en));
        }
    }

    all.sort_by_key(|e| e["span"]["start"].as_u64().unwrap_or(u64::MAX));
    all
}

/// True when `cmie_uri` and `file_uri` refer to the same file. The workspace
/// may register a URI in a canonical form different from the caller-provided
/// path, so either string may be a prefix/suffix of the other.
fn uri_matches(cmie_uri: &str, file_uri: &str) -> bool {
    cmie_uri == file_uri || cmie_uri.ends_with(file_uri) || file_uri.ends_with(cmie_uri)
}

fn dump_component(name: &str, comp: &mcc::McComponent) -> Value {
    // Params
    let params: Vec<Value> = comp.params.names_full().iter().map(|n| json!(n)).collect();
    let params_with_defaults: Vec<Value> = comp
        .params
        .get_params_with_defaults()
        .iter()
        .map(|(id, default)| json!({"name": id.to_string(), "default": default}))
        .collect();

    // Attrs
    let attrs: Vec<Value> = comp
        .attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({"no": a.no, "name": a.id.to_string(), "values": values})
        })
        .collect();

    // Funcs (with body stmts)
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

    // Insts (sub-instances)
    let instances = instances_json(&comp.insts, None);

    // Layout
    let layout = json!({
        "left": comp.layout.left,
        "right": comp.layout.right,
        "top": comp.layout.top,
        "bottom": comp.layout.bottom,
    });

    // CondPins / CondAttrs (debug representation)
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
    data["uri"] = json!(comp.uri.to_string());
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

fn dump_module(name: &str, module: &mcc::McModule) -> Value {
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

    // Insts (ports + sub-instances)
    let instances = instances_json(&module.insts, None);

    // Stmts (connection phrases)
    let stmts: Vec<String> = module.stmts.iter().map(|l| l.to_string()).collect();

    // Funcs
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

    // LSP goto-def data: param/port definition positions
    let defs: Vec<Value> = module
        .params
        .iter_defs_with_span()
        .map(|(name, span)| json!({"name": name, "span": {"start": span.start, "end": span.end}}))
        .collect();
    // LSP goto-def data: port reference positions in net stmts
    let refs: Vec<Value> = module
        .params
        .iter_net_refs()
        .map(|(span, name, scope)| json!({"name": name, "scope": scope, "span": {"start": span.start, "end": span.end}}))
        .collect();

    json!({
        "name": name,
        "kind": "module",
        "uri": module.uri.to_string(),
        "span": {"start": module.span.start, "end": module.span.end},
        "params": params,
        "params_with_defaults": params_with_defaults,
        "instances": instances,
        "stmts_count": module.stmts.len(),
        "stmts": stmts,
        "funcs": funcs,
        "defs": defs,
        "refs": refs,
    })
}

fn dump_interface(name: &str, iface: &mcc::McInterface) -> Value {
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
        .map(|r| {
            json!({
                "name": r.name.to_string(),
                "pins": pins_json(&r.pins),
            })
        })
        .collect();

    let mut data = pins_json(&iface.pins);
    data["name"] = json!(name);
    data["kind"] = json!("interface");
    data["uri"] = json!(iface.uri.to_string());
    data["params"] = json!(params);
    data["params_with_defaults"] = json!(params_with_defaults);
    data["attrs"] = json!(attrs);
    data["roles"] = json!(roles);
    data["span"] = json!({"start": iface.span.start, "end": iface.span.end});
    data
}

fn dump_enum(name: &str, en: &mcc::McEnumDef) -> Value {
    let values: Vec<Value> = en
        .values
        .iter()
        .map(|v| {
            json!({
                "name": v.name.to_string(),
                "span": [v.span[0], v.span[1]],
            })
        })
        .collect();

    json!({
        "name": name,
        "kind": "enum",
        "uri": en.uri.to_string(),
        "span": [en.span[0], en.span[1]],
        "value_count": values.len(),
        "values": values,
    })
}

// Rendering helpers

/// Build the JSON view of a `McPins` (pins + interfaces + name/id mappings).
/// Single implementation lives in `rpc::handlers` so the CLI and the server
/// stay in parity.
fn pins_json(pins: &mcc::McPins) -> Value {
    mcc::rpc::handlers::pins_json(pins)
}

fn inst_kind_class(inst: &mcc::McInstance) -> (&'static str, String) {
    match inst {
        mcc::McInstance::Component(c) => ("component", c.base.name.to_string()),
        mcc::McInstance::Module(m) => ("module", m.base.name.to_string()),
        mcc::McInstance::Label(l) => ("label", l.clone()),
        mcc::McInstance::Interface(i) => ("interface", i.base_name()),
        mcc::McInstance::Bus(b) => ("bus", b.to_string()),
        mcc::McInstance::BusRef { component, bus } => ("busref", format!("{}.{}", component, bus)),
        mcc::McInstance::List(l) => {
            let name = l.name().to_string();
            // Show debug form (includes members) for lists with members
            let class = format!("{:?}", l);
            if class != name {
                ("list", class)
            } else {
                ("list", name)
            }
        }
        mcc::McInstance::Unresolved { class_name } => ("unresolved", class_name.clone()),
        mcc::McInstance::Pins => ("pins", "pins".into()),
        mcc::McInstance::PinId(id) => ("pinid", id.clone()),
        mcc::McInstance::Attr(a) => ("attr", a.to_string()),
        mcc::McInstance::Func(f) => ("func", f.name.to_string()),
        mcc::McInstance::EnumVal {
            enum_name,
            value_name,
            ..
        } => ("enumval", format!("{}.{}", enum_name, value_name)),
    }
}

fn instances_json(insts: &mcc::McInstances, type_filter: Option<&str>) -> Vec<Value> {
    let port_spans = insts.port_spans();
    insts
        .iter_in_decl_order()
        .filter_map(|(n, inst)| {
            let (kind, class) = inst_kind_class(inst);
            let kind = if kind == "label" {
                match insts.get_label_kind(n) {
                    mcc::LabelKind::Inline => "ilabel",
                    mcc::LabelKind::Explicit => "label",
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
                Some((mcc::IOType::InOut, _)) => "io",
                Some((mcc::IOType::Out, _)) => "out",
                Some((mcc::IOType::In, _)) => "in",
                _ => "",
            };
            let mut entry = json!({
                "name": n.to_string(),
                "io": io,
                "kind": kind,
                "class": class,
                "params": inst_params(inst),
            });
            if let Some(s) = span {
                entry["span"] = s;
            }
            Some(entry)
        })
        .collect()
}

/// Render the construction parameters of an instance as strings
/// (component `params`, module `args`, interface `params`).
fn inst_params(inst: &mcc::McInstance) -> Vec<String> {
    match inst {
        mcc::McInstance::Component(c) => c.params.iter().map(|p| p.to_string()).collect(),
        mcc::McInstance::Module(m) => m.args.iter().map(|p| p.to_string()).collect(),
        mcc::McInstance::Interface(i) => i.params.iter().map(|p| p.to_string()).collect(),
        _ => Vec::new(),
    }
}

fn attrval_json(v: &mcc::McAttrVal) -> Value {
    match v {
        // Keep string literals quoted so the dump shows the source form.
        mcc::McAttrVal::AttrLiteral(mcc::McLiteral::String(s)) => {
            json!(format!("\"{}\"", s.value))
        }
        other => json!(other.to_string()),
    }
}

/// Build the Pass2 netlist for a module: net name -> ordered point labels.
///
/// Thin wrapper over the shared net-table projection (`cmds/nets.rs`) so the
/// fold lives in exactly one place; the `error!` + `exit(1)` guardrail is
/// preserved for callers that cannot propagate an error.
pub(crate) fn nets_map(top: &str) -> BTreeMap<String, Vec<String>> {
    crate::cmds::nets::top_nets(top, None).unwrap_or_else(|e| {
        die!("mcc::show", 1, "{e}");
    })
}

// Output

/// Render `show all` layered output (tagged `type: "layered_all"`) in text
/// mode: one section per layer, separated by `------`. Sections follow a fixed
/// order (system -> use -> file) instead of the JSON map's alphabetical order.
/// The `file` layer renders per-entity details (full-field compact text);
/// the `use`/`system` layers keep the name-list overview. Returns `None` for
/// every other data shape.
fn render_layered_text(data: &Value, span: bool) -> Option<String> {
    if data.get("type")?.as_str()? != LAYERED_ALL_TYPE {
        return None;
    }
    let obj = data.as_object()?;
    let file = obj.get("target_file").and_then(|v| v.as_str());
    let mut lines = Vec::new();
    for layer in ["system", "use", "file"] {
        let Some(section) = obj.get(layer) else {
            continue;
        };
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("------ {layer} ------"));
        if layer == "file" && file.is_some() {
            for entity in collect_dump_file(file.unwrap()) {
                lines.push(compact::render_entity(&entity, span));
            }
        } else if let Some(sec) = section.as_object() {
            for (k, v) in sec {
                lines.push(format!("{k}: {v}"));
            }
        }
    }
    Some(lines.join("\n"))
}

/// `list all` text renderer: `{type:"all", count, list:[{name, kind, uri}]}`
/// → a `count:` header followed by one `kind: name` line per definition.
fn render_all_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("all") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let items = data.get("list")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for item in items {
        let name = item.get("name")?.as_str()?;
        let kind = item.get("kind")?.as_str()?;
        out.push_str(&format!("{kind}: {name}\n"));
    }
    Some(out.trim_end().to_string())
}

/// `list <kind>` text renderer: `{type, count, list:[names]}` → a `count:`
/// header followed by one name per line.
fn render_kind_list_text(data: &Value) -> Option<String> {
    let t = data.get("type")?.as_str()?;
    if !matches!(t, "component" | "module" | "interface" | "enum") {
        return None;
    }
    let names: Vec<&str> = data
        .get("list")?
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    let mut out = format!("count: {}\n", names.len());
    for n in names {
        out.push_str(n);
        out.push('\n');
    }
    Some(out.trim_end().to_string())
}

/// `list nets` text renderer: `{type:"net", count, nets:[{name, points}]}`
/// → a `count:` header followed by one `name: point, point` line per net.
fn render_nets_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("net") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let nets = data.get("nets")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for net in nets {
        let name = net.get("name")?.as_str()?;
        let points: Vec<&str> = net
            .get("points")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        out.push_str(&format!("{name}: {}\n", points.join(", ")));
    }
    Some(out.trim_end().to_string())
}

/// `list ports` text renderer: `{type:"port", count, ports:[{name, iotype, module}]}`
/// → a `count:` header followed by one `name: iotype (module)` line per port.
fn render_ports_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("port") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let ports = data.get("ports")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for port in ports {
        let name = port.get("name")?.as_str()?;
        let iotype = port.get("iotype")?.as_str()?;
        let module = port.get("module")?.as_str()?;
        out.push_str(&format!("{name}: {iotype} ({module})\n"));
    }
    Some(out.trim_end().to_string())
}

/// `list files` text renderer: `{type:"files", count, files:[{uri, *_count}]}`
/// → a `count:` header followed by one `uri: comp=N mod=N iface=N enum=N` line.
fn render_files_list_text(data: &Value) -> Option<String> {
    if data.get("type").and_then(|v| v.as_str()) != Some("files") {
        return None;
    }
    let count = data.get("count")?.as_u64()?;
    let files = data.get("files")?.as_array()?;
    let mut out = format!("count: {count}\n");
    for f in files {
        let uri = f.get("uri")?.as_str()?;
        let comp = f
            .get("component_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mod_ = f.get("module_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let iface = f
            .get("interface_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let en = f.get("enum_count").and_then(|v| v.as_u64()).unwrap_or(0);
        out.push_str(&format!(
            "{uri}: comp={comp} mod={mod_} iface={iface} enum={en}\n"
        ));
    }
    Some(out.trim_end().to_string())
}

/// Render a component/pins data object (`name`, `uri`, `pin_count`, `pins`)
/// as an aligned text table. Returns `None` when the data has no `pins` array.
fn render_pins_text(data: &Value) -> Option<String> {
    let pins = data.get("pins")?;
    if pins.as_array().is_none() {
        return None;
    }

    let mut out = String::new();
    if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
        out.push_str(&format!("component: {name}\n"));
    }
    if let Some(uri) = data.get("uri").and_then(|v| v.as_str()) {
        out.push_str(&format!("uri: {uri}\n"));
    }
    if let Some(n) = data.get("pin_count").and_then(|v| v.as_u64()) {
        out.push_str(&format!("pin_count: {n}\n"));
    }
    out.push('\n');
    out.push_str(&render_pins_table(pins));
    Some(out)
}

/// Render an attrs drill-down as an aligned table (text format only).
fn render_attrs_text(data: &Value) -> Option<String> {
    let attrs = data.get("attrs")?.as_array()?;
    let mut rows: Vec<(String, String)> = Vec::new();
    for a in attrs {
        let name = a.get("name")?.as_str()?.to_string();
        let values: Vec<String> = a
            .get("values")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| match v.as_str() {
                        Some(s) => s.to_string(),
                        None => v.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        rows.push((name, values.join(", ")));
    }

    let name_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(10).max(10);
    let val_w = rows.iter().map(|r| r.1.len()).max().unwrap_or(30).max(30);

    let mut out = String::new();
    if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
        out.push_str(&format!("{name}\n"));
    }
    if let Some(n) = data.get("count").and_then(|v| v.as_u64()) {
        out.push_str(&format!("attr_count: {n}\n"));
    }
    out.push('\n');
    out.push_str(&format!("  {:<name_w$}  {}\n", "name", "values"));
    out.push_str(&format!(
        "  {}  {}\n",
        "-".repeat(name_w),
        "-".repeat(val_w.min(80)),
    ));
    for (name, values) in &rows {
        out.push_str(&format!("  {:<name_w$}  {}\n", name, values));
    }
    Some(out)
}

/// Render a pin list (as produced by `pins_json`) as an aligned table.
/// Header/divider/rows only — callers add the entity title lines.
fn render_pins_table(pins: &Value) -> String {
    let Some(pins) = pins.as_array() else {
        return String::new();
    };
    let mut rows: Vec<(String, String, String, String)> = Vec::new();
    for p in pins {
        let id = p
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let io = p
            .get("iotype")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let names: Vec<String> = p
            .get("names")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let ifaces: Vec<String> = p
            .get("interfaces")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(iface_display).collect())
            .unwrap_or_default();
        rows.push((id, io, names.join(", "), ifaces.join(" | ")));
    }

    let id_w = rows.iter().map(|r| r.0.len()).max().unwrap_or(4).max(4);
    let io_w = rows.iter().map(|r| r.1.len()).max().unwrap_or(8).max(8);
    let names_w = rows.iter().map(|r| r.2.len()).max().unwrap_or(40).max(40);
    let name_w = rows.iter().map(|r| r.3.len()).max().unwrap_or(12).max(12);

    let mut out = String::new();
    out.push_str(&format!(
        "  {:<id_w$}  {:<io_w$}  {:<names_w$}  {}\n",
        "id", "io", "names", "interfaces"
    ));
    out.push_str(&format!(
        "  {}  {}  {}  {}\n",
        "-".repeat(id_w),
        "-".repeat(io_w),
        "-".repeat(names_w),
        "-".repeat(name_w.min(60)),
    ));
    for (id, io, names, ifaces) in &rows {
        out.push_str(&format!(
            "  {:<id_w$}  {:<io_w$}  {:<names_w$}  {}\n",
            id, io, names, ifaces
        ));
    }
    out
}

/// Render an aligned text table from string rows. Column widths adapt to the
/// widest header/cell. Used by the `show` drill-downs in text format.
fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let cols = headers.len();
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            let h = headers[c].len();
            rows.iter().map(|r| r[c].len()).max().unwrap_or(h).max(h)
        })
        .collect();
    let mut out = String::new();
    let hdr: Vec<String> = (0..cols)
        .map(|c| format!("{:<w$}", headers[c], w = widths[c]))
        .collect();
    out.push_str(&format!("  {}\n", hdr.join("  ")));
    out.push_str(&format!(
        "  {}\n",
        widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("  ")
    ));
    for r in rows {
        let cells: Vec<String> = (0..cols)
            .map(|c| format!("{:<w$}", r[c], w = widths[c]))
            .collect();
        out.push_str(&format!("  {}\n", cells.join("  ")));
    }
    out
}

/// Render any `show` drill-down as readable text: an aligned table (object
/// arrays) or a per-line list (string arrays), headed by the entity name and a
/// count line. JSON output is unaffected — this fires only for text format.
fn render_drill_text(data: &Value) -> Option<String> {
    let name = data.get("name")?.as_str()?;

    // Count line: drill-downs use port_count / label_count / count.
    let count_key = ["port_count", "label_count", "count"]
        .iter()
        .find(|k| data.get(**k).is_some())?;
    let count = data.get(count_key)?.as_u64()?;

    let body: String = if let Some(arr) = data.get("ports").and_then(|v| v.as_array()) {
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|p| {
                let members = p
                    .get("members")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|m| m.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                vec![
                    p.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    p.get("iotype")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    p.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    members,
                ]
            })
            .collect();
        render_table(&["name", "iotype", "type", "members"], &rows)
    } else if let Some(arr) = data.get("instances").and_then(|v| v.as_array()) {
        let has_origin = arr.iter().any(|i| i.get("origin").is_some());
        let has_caller = arr.iter().any(|i| {
            i.get("caller")
                .and_then(|v| v.as_str())
                .is_some_and(|c| !c.is_empty())
        });
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|i| {
                let params = i
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let mut row = vec![
                    i.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    i.get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    i.get("class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    params,
                ];
                if has_origin {
                    let origin = i
                        .get("origin")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let line = i
                        .get("line")
                        .and_then(|v| v.as_u64())
                        .map(|l| format!("L{l}"))
                        .unwrap_or_default();
                    row.push(if line.is_empty() {
                        origin
                    } else {
                        format!("{origin}@{line}")
                    });
                }
                if has_caller {
                    row.push(
                        i.get("caller")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    );
                }
                row
            })
            .collect();
        let mut headers = vec!["name", "kind", "class", "params"];
        if has_origin {
            headers.push("origin");
        }
        if has_caller {
            headers.push("caller");
        }
        render_table(&headers, &rows)
    } else if let Some(arr) = data.get("funcs").and_then(|v| v.as_array()) {
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|f| {
                let params = f
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                vec![
                    f.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    params,
                ]
            })
            .collect();
        render_table(&["name", "params"], &rows)
    } else if let Some(arr) = data.get("nets").and_then(|v| v.as_array()) {
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|n| {
                let points = n
                    .get("points")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                vec![
                    n.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    points,
                ]
            })
            .collect();
        render_table(&["name", "points"], &rows)
    } else if let Some(arr) = data.get("roles").and_then(|v| v.as_array()) {
        // Each role: heading followed by its pins table.
        let mut body = String::new();
        for r in arr {
            let rname = r
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            body.push_str(&format!("role: {rname}\n"));
            if let Some(pins) = r.get("pins").and_then(|p| p.get("pins")) {
                body.push_str(&render_pins_table(pins));
            }
            body.push('\n');
        }
        body
    } else if let Some(arr) = data.get("params").and_then(|v| v.as_array()) {
        // Parameter declarations: name / type / default table. The type column
        // shows the concrete interface class with its constructor params
        // (e.g. `DC(3.3V)`) and falls back to the semantic category
        // (e.g. `A1-Label`) when no class is bound.
        let rows: Vec<Vec<String>> = arr
            .iter()
            .map(|p| {
                let class = p.get("class").and_then(|v| v.as_str()).unwrap_or("");
                let params = p
                    .get("params")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let ty = if class.is_empty() {
                    p.get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                } else if params.is_empty() {
                    class.to_string()
                } else {
                    format!("{class}({params})")
                };
                vec![
                    p.get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    ty,
                    p.get("default")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                ]
            })
            .collect();
        render_table(&["name", "type", "default"], &rows)
    } else if let Some(arr) = data
        .get("labels")
        .or_else(|| data.get("values"))
        .and_then(|v| v.as_array())
    {
        // Simple string lists: one entry per line.
        arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| format!("  {s}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    } else {
        return None;
    };

    let mut out = format!("{name}\n{count_key}: {count}\n\n");
    out.push_str(&body);
    Some(out)
}

/// Format one interface entry of a pin. Interfaces render as
/// `Name::Base(param1, param2)`, buses as `Name{CLK, DATA}`, and List groups
/// as `Name[CLK, DATA]` to mirror the `.mc` source notation.
fn iface_display(v: &Value) -> Option<String> {
    let kind = v
        .get("kind")
        .and_then(|x| x.as_str())
        .unwrap_or("Interface");
    let inst = v.get("name").and_then(|x| x.as_str())?;
    let members: Vec<String> = v
        .get("members")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(|x| x.to_string()))
                .collect()
        })
        .unwrap_or_default();
    match kind {
        "Bus" => Some(format!("{}{{{}}}", inst, members.join(", "))),
        "List" => Some(format!("{}[{}]", inst, members.join(", "))),
        _ => {
            let base = v.get("base").and_then(|x| x.as_str()).unwrap_or(inst);
            let params: Vec<String> = v
                .get("params")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(|x| x.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            if params.is_empty() {
                Some(format!("{inst}::{base}"))
            } else {
                Some(format!("{inst}::{base}({})", params.join(", ")))
            }
        }
    }
}

/// One `show` sub-face's payload → the A-tier command envelope (U86 item 7,
/// second slice), sharing the single [`ProjectionKey::Show`] key with the other
/// twenty. The sub-face rides in the envelope's `command` (`mcc show pins`);
/// [`ShowTarget::name`] records why that and not a payload field.
///
/// Text / csv keep [`output`]'s renderers byte for byte — csv is deliberately
/// not [`OutputFormatExt::is_structured`], so every sub-face's `-f csv` face is
/// unchanged by this slice.
fn emit_show(target: ShowTarget, data: &Value, span: bool) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection_sub(
            ProjectionKey::Show,
            target.name(),
            data.clone(),
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    output(data, span)
}

/// [`emit_show`] for the sub-faces whose payload is built inline and therefore
/// handed over by value: the three `lapper` sites and `run`'s RPC branch.
///
/// The non-structured fallback is the **pretty-JSON print** those four sites had
/// before the envelope, not [`output`]'s csv arm: `-f csv` / `-f yaml` printed
/// pretty JSON there, and a wrapping change does not get to fix that. (The
/// `-f yaml` oddity is shared with the other sub-faces, which [`output`] renders
/// the same way — recorded, not fixed.)
fn emit_show_owned(target: ShowTarget, data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection_sub(
            ProjectionKey::Show,
            target.name(),
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    write_show_text(&serde_json::to_string_pretty(&data)?)
}

/// Write a rendered text face to `--output` or stdout.
///
/// `-o` is the command's, not one face's: the text faces that go through
/// [`output`] already honour it, so the ones that print directly must too.
/// `show lapper` (both branches below) and [`emit_show_owned`]'s non-structured
/// fallback printed straight to stdout and silently ignored the flag.
fn write_show_text(rendered: &str) -> Result<()> {
    match &mcc::cli::globals().output {
        Some(path) => {
            std::fs::write(path, format!("{rendered}\n"))?;
            Ok(())
        }
        None => {
            println!("{rendered}");
            Ok(())
        }
    }
}

pub(crate) fn output(data: &Value, span: bool) -> Result<()> {
    let rendered = match mcc::cli::globals().format {
        OutputFormat::Json => data.to_string(),
        OutputFormat::JsonPretty => serde_json::to_string_pretty(data)?,
        OutputFormat::Yaml => serde_yaml::to_string(data).unwrap_or_default(),
        OutputFormat::Csv => data.to_string(),
        OutputFormat::Text => {
            // Entity dump values (kind == "func") render like the other
            // drill-downs below (list / table); everything else falls through
            // the layered / list renderers.
            if let Some(t) = render_layered_text(data, span) {
                // show all: per-layer sections; the file layer renders details
                t
            } else if let Some(t) = render_all_list_text(data) {
                // list all: `kind: name` per line, count header
                t
            } else if let Some(t) = render_kind_list_text(data) {
                // list component/module/interface/enum: one name per line
                t
            } else if let Some(t) = render_nets_list_text(data) {
                // list nets: `name: point, point` per line
                t
            } else if let Some(t) = render_ports_list_text(data) {
                // list ports: `name: iotype (module)` per line
                t
            } else if let Some(t) = render_files_list_text(data) {
                // list files: `uri: comp=N mod=N iface=N enum=N` per line
                t
            } else if let Some(t) = render_pins_text(data) {
                // component / pins drill-down: aligned pin table
                t
            } else if let Some(t) = render_attrs_text(data) {
                // attrs drill-down: aligned name/values table
                t
            } else if let Some(t) = render_drill_text(data) {
                // other drill-downs (ports/labels/instances/nets/funcs/params/
                // roles/values): aligned tables or per-line lists
                t
            } else if let Some(obj) = data.as_object() {
                obj.iter()
                    .map(|(k, v)| format!("{}: {}", k, v))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                data.to_string()
            }
        }
    };

    if let Some(path) = &mcc::cli::globals().output {
        std::fs::write(path, rendered)?;
    } else {
        println!("{}", rendered);
    }
    Ok(())
}
