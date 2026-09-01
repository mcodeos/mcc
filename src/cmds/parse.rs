// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc parse` — Parse current file (PR-2 rewrite version)
//!
//! ## Changes introduced in PR-2
//!
//! 1. **JSON mode (`--json` / `--format json`) goes through envelope path**:
//!    All decorations / progress prints are muted, stdout has clean JSON-RPC envelope,
//!    `result.pass1` / `result.pass2` / `result.viz` are sibling keys.
//!
//! 2. **Text mode doesn't change behavior**: Still `[Pass 1]` / `[Pass 2]` decorations + `| Ports |`
//!    box-drawing tables go to stderr, maintaining visual compatibility. PR-3 will unify using Renderer trait.
//!
//! 3. **Diagnostics auto bucket by phase**: `PhaseTracker` collects incremental diagnostics between pass1 / pass2,
//!    each hanging under `pass1.diagnostics` / `pass2.diagnostics`.
//!
//! 4. **When top-level module not found, return [`RpcError::invalid_params`]**, no longer anyhow::bail!,
//!    ensures structured errors are available in JSON mode.

use crate::cmds::manifest;
use crate::output::{
    self,
    builder::ResultBuilder,
    diagnostic::{batch_from_mcc, PhaseTracker},
    envelope::{
        ComponentInfo, ConnectionEntry, DefinitionRef, DefinitionsIndex, Envelope, InstanceNode,
        LoadedFile, NetEntry, Pass0Report, Pass1Report, Pass2Report, Phase, PinInfo, PortInfo,
        RpcError, ViewData, VizData, WorkspaceRef,
    },
    renderer, OutputFormatExt,
};
use anyhow::{Context, Result};
use mcc::cli::rpcclient::RpcClient;
use mcc::cli::ParseArgs;
use mcc::{IOType, McCMIE, McEndpoint, McIds, McInstance, McInstanceRef, McPhrase, McURI};
use mcc::{McParamDeclare, McParamTypeKind};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

// ============================================================================
// Entry point
// ============================================================================

pub fn run(args: &ParseArgs) -> Result<()> {
    // ── 0. RPC delegation (server mode) ──
    // --local (global flag) is honored centrally by RpcClient::probe();
    // --dlog only affects output rendering below and no longer implies
    // local execution. Use `mcc parse <file> --dlog --local` when both are wanted.
    if let Some(client) = RpcClient::probe() {
        let params = json!({
            "entry": args.target.clone(),
            "top":   mcc::cli::globals().top.clone(),
            "code":  args.code.clone(),
            "libs":  mcc::cli::globals().lib.clone(),
        });
        let result = client.call("parse", params)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // ── 0.5. Local mode initialization (shared helper) ──
    // Loads libraries from all config sources: global config + project config +
    // manifest + CLI --lib, plus the mcode default (unless disabled).
    // Without this, local-mode parse can't see mcode's interfaces and emits spurious
    // E1304 / E2702 warnings for every `X::Interface(...)` reference.
    manifest::init_local(args.target.as_deref(), &mcc::cli::globals().lib);

    // ── 0.6. Pass 0 snapshot: lib load + C parser error attribution ──
    // Must snapshot after mcc_load_project and before tracker.new(),
    // otherwise pass1 will "swallow" load phase diagnostics. See public_collect_pass0.

    // ── 1. Determine uri ──
    let mut forced_top: Option<String> = None;
    let uri: McURI = if let Some(code) = &args.code {
        let vuri = McURI::from("/mcc/snippet.mc");
        mcc::mcc_load_from_string(&vuri, code);
        vuri
    } else if let Some(t) = &args.target {
        match crate::cmds::common::load_target(
            Some(t),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        ) {
            Ok((entry_uri, top)) => {
                // Project/browse mode resolves the top (manifest top_module,
                // --top / --entry override, or the browse entry's module);
                // single-file mode leaves it to resolve_top_module below.
                forced_top = top;
                McURI::from(entry_uri.as_str())
            }
            Err(e) => return emit_error(RpcError::invalid_params(format!("{:#}", e)), args.dlog),
        }
    } else {
        return emit_error(
            RpcError::invalid_params("parse: <target>/--code not specified"),
            args.dlog,
        );
    };

    // ── 2. Stage selection ──
    let stages = Stages::from_args(args);
    let renderer: Box<dyn renderer::OutputRenderer> = if args.dlog {
        Box::new(renderer::SilentRenderer)
    } else {
        renderer::for_format_with_sort(mcc::cli::globals().format, args.sort)
    };

    // ── 3. ResultBuilder initialization ──
    let ws_ref = {
        let (id, kind_str, _) = mcc::workspace_info();
        match kind_str.as_str() {
            "Project" => WorkspaceRef::project(id),
            _ => WorkspaceRef::project(id),
        }
    };
    let mut builder = ResultBuilder::start("mcc parse").workspace(ws_ref);
    let mut tracker = PhaseTracker::new();

    tracker.skip();

    // ── 3.5. Put pass0 snapshot into builder. Load phase is over, diagnostics in get_def /
    // build phase will naturally belong to pass1/pass2 via tracker.collect. ──
    builder.set_pass0(public_collect_pass0());

    // ── 4. Select top definition (module, component, interface, or enum) ──
    let top_name = match crate::cmds::common::resolve_top_module(&uri, forced_top.clone()) {
        Some(n) => n,
        None => {
            // No module found — if --tree/--ast is set, still proceed to show
            // components/interfaces/enums. Otherwise finish.
            if !stages.tree {
                if args.dlog {
                    output::diagnostic::print_dlog_lines(false);
                }
                let env = Envelope::ok(builder.finish());
                output::emit_envelope_brief(
                    &env,
                    mcc::cli::globals().format,
                    mcc::cli::globals().output.as_deref().map(Path::new),
                    false,
                )?;
                return Ok(());
            }
            // Use a dummy — tree section will collect all CMIE types directly
            String::new()
        }
    };

    // ── 5. get_def: get top-level definition ──
    let ident = McIds::from(top_name.as_str());
    let cmie = mcc::get_def(&ident, &uri);

    // Accept all CMIE types — tree/ast view works for component/interface/enum too.
    enum ParseTarget {
        Module,
        Component,
        Interface,
        Enum,
    }
    let parse_target: Option<ParseTarget> = match &cmie {
        Some(McCMIE::Module(_)) => Some(ParseTarget::Module),
        Some(McCMIE::Component(_)) => Some(ParseTarget::Component),
        Some(McCMIE::Interface(_)) => Some(ParseTarget::Interface),
        Some(McCMIE::Enum(_)) => Some(ParseTarget::Enum),
        _ => None,
    };
    let is_module = matches!(parse_target, Some(ParseTarget::Module));

    // ── 6. Pass1 assembly ──
    if stages.pass1 {
        if stages.pass1_verbose {
            renderer.pass1_header(&uri);
            renderer.pass1_definitions(
                mcc::mcb_module_count(),
                mcc::mcb_component_count(),
                mcc::mcb_interface_count(),
            );
            for (name, module_uri) in mcc::mcb_iter_modules() {
                let ident = McIds::from(name.as_str());
                let module_mc_uri = McURI::from(module_uri.as_str());
                if let Some(cmie) = mcc::get_def(&ident, &module_mc_uri) {
                    if let McCMIE::Module(def) = cmie {
                        renderer.module_ports(&def);
                        renderer.module_symbols(&def);
                        renderer.module_stmts(&def);
                    }
                }
            }
        }

        let pass1 = public_collect_pass1(&uri, &mut tracker);
        builder.set_pass1(pass1);
    }

    // ── 7. tree / ast: go through view field (replacement output) ──
    if stages.tree {
        let has_explicit_top = forced_top.is_some() || mcc::cli::globals().top.is_some();
        let mut nodes: Vec<serde_json::Value> = Vec::new();

        if has_explicit_top {
            // Show the single top definition
            if let Some(ref cmie) = cmie {
                nodes.push(match cmie {
                    McCMIE::Module(m) => {
                        let mut children = Vec::with_capacity(m.stmts.len());
                        for stmt in m.stmts.iter() {
                            children.push(phrase_to_tree_json(stmt, args.depth, 0));
                        }
                        json!({
                            "kind": "module",
                            "name": m.name.to_string(),
                            "children": children,
                        })
                    }
                    _ => cmie_to_tree_json(cmie, args.depth),
                });
            }
        } else {
            // Tree-all mode: collect all definitions from this file.
            // Each mcb_iter_* chains workspace + global tables, so dedup
            // within each category. Enum+component can share a name+URI.
            let file_uri = uri.as_str();

            // Helper: iterate with intra-category dedup, filter by file URI
            macro_rules! collect_from {
                ($iter:expr, $seen:ident, |$name:ident, $cmie_uri:ident| $body:expr) => {{
                    let mut $seen: std::collections::HashSet<(String, String)> =
                        std::collections::HashSet::new();
                    for ($name, $cmie_uri) in $iter {
                        if !$seen.insert(($name.clone(), $cmie_uri.clone())) {
                            continue;
                        }
                        if $cmie_uri == file_uri
                            || $cmie_uri.ends_with(file_uri)
                            || file_uri.ends_with($cmie_uri.as_str())
                        {
                            $body
                        }
                    }
                }};
            }

            // Modules
            collect_from!(&mcc::mcb_iter_modules(), seen_mod, |name, cmie_uri| {
                let ident = McIds::from(name.as_str());
                if let Some(McCMIE::Module(m)) =
                    mcc::get_def(&ident, &McURI::from(cmie_uri.as_str()))
                {
                    let mut children = Vec::with_capacity(m.stmts.len());
                    for stmt in m.stmts.iter() {
                        children.push(phrase_to_tree_json(stmt, args.depth, 0));
                    }
                    nodes.push(json!({
                        "kind": "module",
                        "name": m.name.to_string(),
                        "children": children,
                    }));
                }
            });

            // Components — use get_component_def to avoid RefDefMap ambiguity
            // when a component shares its name with an enum.
            collect_from!(&mcc::mcb_iter_components(), seen_comp, |name, cmie_uri| {
                let ident = McIds::from(name.as_str());
                let cmie = mcc::get_component_def(&ident, &McURI::from(cmie_uri.as_str()))
                    .or_else(|| mcc::get_def(&ident, &McURI::from(cmie_uri.as_str())));
                if let Some(cmie) = cmie {
                    nodes.push(cmie_to_tree_json(&cmie, args.depth));
                }
            });

            // Interfaces
            collect_from!(&mcc::mcb_iter_interfaces(), seen_iface, |name, cmie_uri| {
                let ident = McIds::from(name.as_str());
                if let Some(cmie) = mcc::get_def(&ident, &McURI::from(cmie_uri.as_str())) {
                    nodes.push(cmie_to_tree_json(&cmie, args.depth));
                }
            });

            // Enums
            collect_from!(&mcc::mcb_iter_enums(), seen_enum, |name, cmie_uri| {
                let ident = McIds::from(name.as_str());
                if let Some(cmie) = mcc::get_def(&ident, &McURI::from(cmie_uri.as_str())) {
                    nodes.push(cmie_to_tree_json(&cmie, args.depth));
                }
            });
        }

        let view_data = ViewData {
            target: if args.ast {
                "ast".into()
            } else {
                "tree".into()
            },
            data: serde_json::Value::Array(nodes),
        };
        builder.set_view(view_data);
    }

    // ── 8. Pass2 assembly (Module only) ──
    if stages.pass2 && is_module {
        // ─────────────────────────────────────────────────────────────────────────────
        // Top Module Selection Strategy
        //
        // Strategy 1: Module with Top (Instantiated Hierarchy)
        //   - When a file contains modules with hierarchical instantiation (one module instantiates another),
        //     use the specified --top module as the entry point.
        //
        // Priority for Top Module Selection:
        //   1. CLI --top argument (highest priority): e.g., `mcc parse --top MyModule`
        //   2. Manifest top_module field: defined in manifest.toml
        //   3. Fallback to the first module of the target file (usually "main")
        //
        // Strategy 2: No Top Module (Flat/Peer Modules)
        //   - When a file contains multiple peer modules without hierarchical instantiation,
        //     only the DEFAULT module (the file's first module, usually "main") is
        //     instantiated. Peer modules are not all rendered: the user observes one
        //     module at a time and each observation instantiates exactly one instance.
        //     Use --top to select a specific peer module.
        // ─────────────────────────────────────────────────────────────────────────────

        // Single top-module rendering: the default module of the target file
        // (main by convention, overridable via --top). Peer modules without an
        // explicit --top are no longer all rendered — only the default module
        // is instantiated, matching the "observe one module, instantiate one
        // instance" model (mcext-folder-parse-design.md §4.1 change 4).
        renderer.pass2_header(&top_name);

        match mcc::mcc_build_with_arena(&ident, &uri) {
            Ok((inst, arena, net_store)) => {
                renderer.instances(&inst, 0, Some(&arena));
                renderer.connections(&inst, 0, Some(&arena));
                renderer.nets(&inst, 0, Some(&arena), &net_store);

                let pass2 =
                    public_collect_pass2(&top_name, &inst, Some(&arena), &net_store, &mut tracker);
                builder.set_pass2(pass2);

                // Print diagnostics before Net Summary
                if matches!(mcc::cli::globals().format, mcc::cli::OutputFormat::Text) && !args.dlog
                {
                    builder.print_diagnostics_summary();
                }
                renderer.net_summary(&inst, Some(&arena), &net_store);
            }
            Err(e) => {
                renderer.pass2_failed(&format!("{}", e));
                let err = RpcError::build_error(format!("{}", e));
                emit_error(err, args.dlog)?;
            }
        }
    }

    // ── 9. Viz assembly ──
    if stages.viz_html || stages.viz_json {
        let has_explicit_top = forced_top.is_some() || mcc::cli::globals().top.is_some();
        if has_explicit_top {
            match run_viz(&ident, &uri, args, stages.viz_json, &*renderer) {
                Ok(viz) => {
                    builder.set_viz(viz);
                }
                Err(e) => {
                    return emit_error(RpcError::internal_error(format!("viz: {}", e)), args.dlog);
                }
            }
        } else {
            // No --top specified: render all modules in the file
            let all_modules: Vec<(String, String)> = mcc::mcb_iter_modules()
                .into_iter()
                .filter(|(_, module_uri)| {
                    // Only modules from the target file
                    module_uri == uri.as_str() || module_uri.ends_with(uri.as_str())
                })
                .collect();

            if all_modules.is_empty() {
                // Fallback: render the auto-selected top module
                match run_viz(&ident, &uri, args, stages.viz_json, &*renderer) {
                    Ok(viz) => {
                        builder.set_viz(viz);
                    }
                    Err(e) => {
                        return emit_error(
                            RpcError::internal_error(format!("viz: {}", e)),
                            args.dlog,
                        );
                    }
                }
            } else {
                let mut total_boxes = 0;
                let mut total_edges = 0;
                let mut svgs: Vec<(String, String)> = Vec::new(); // (module_name, svg_string)

                for (mod_name, module_uri) in &all_modules {
                    let mod_ident = McIds::from(mod_name.as_str());
                    let mod_mc_uri = McURI::from(module_uri.as_str());

                    let (inst, table, arena) =
                        match mcc::mcc_build_flat_with_arena(&mod_ident, &mod_mc_uri, 1000) {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!(
                                    "[viz] skip module '{}': mcc_build_flat failed: {}",
                                    mod_name, e
                                );
                                continue;
                            }
                        };

                    mcc::vector::builder::reset_np_warn_count();
                    let vec_block = mcc::build_mc_vec_with_arena(&inst, &table, &arena);
                    let graph = mcc::build_mc_vec_graph(&vec_block, &table);
                    let graph_box_count = graph.boxes.len();
                    let graph_edge_count = graph.edges.len();

                    let opts = mcc::viz::api::RenderOpts::default();
                    let doc = mcc::viz::api::render_with(graph, opts);

                    total_boxes += graph_box_count;
                    total_edges += graph_edge_count;

                    // Extract the SVG from the root layer
                    if let Some(root_layer) = doc.root_layer() {
                        svgs.push((mod_name.clone(), root_layer.svg.clone()));
                    }
                }

                if svgs.is_empty() {
                    return emit_error(
                        RpcError::internal_error("viz: no modules rendered"),
                        args.dlog,
                    );
                }

                // Combine all SVGs into one big SVG, stacked vertically
                let combined_svg = combine_svgs(&svgs);

                // Build a single-layer VizDocument with the combined SVG
                let mut doc = mcc::viz::doc::VizDocument::new(1000, "all_modules".into());
                let mut layer = mcc::viz::layer::VizLayer::new(1000, "all_modules".into(), None);
                layer.svg = combined_svg;
                doc.add_layer(layer);

                let output_text = if stages.viz_json {
                    doc.to_json()
                } else {
                    mcc::viz::template::wrap_document(&doc)
                };

                let out_path = if let Some(ref p) = mcc::cli::globals().output {
                    Path::new(p).to_path_buf()
                } else {
                    // `--code` mode has no target file; fall back to a stable name.
                    let p = args
                        .target
                        .as_ref()
                        .map_or_else(|| "snippet.mc".to_string(), |t| t.clone());
                    let path = Path::new(&p);
                    let stem = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "output".to_string());
                    let parent = path.parent().unwrap_or(Path::new(""));
                    parent.join(format!("{}.html", stem))
                };
                let path_str = out_path.to_string_lossy().to_string();

                std::fs::write(&out_path, &output_text)
                    .with_context(|| format!("Failed to write file: {}", path_str))?;
                eprintln!(
                    "[viz] wrote {} ({} bytes, {} modules)",
                    path_str,
                    output_text.len(),
                    svgs.len()
                );

                builder.set_viz(VizData {
                    format: if stages.viz_json {
                        "json".into()
                    } else {
                        "html".into()
                    },
                    written_to: Some(path_str),
                    bytes: output_text.len(),
                    layers: 1,
                    boxes: total_boxes,
                    edges: total_edges,
                });
            }
        }
    }

    // ── 10. Final output ──
    // --dlog: only print dlog error/warning diagnostics, skip normal output
    if args.dlog {
        output::diagnostic::print_dlog_lines(false);
        return Ok(());
    }

    let env = Envelope::ok(builder.finish());
    let target = mcc::cli::globals().output.as_deref().map(Path::new);

    let envelope_target = if (stages.viz_html || stages.viz_json) && target.is_some() {
        None
    } else {
        target
    };

    // The detailed tables are already printed live via the TextRenderer above;
    // the trailing envelope emission stays the brief text report so the detail
    // isn't rendered twice.
    output::emit_envelope_brief(&env, mcc::cli::globals().format, envelope_target, true)?;
    Ok(())
}

// ============================================================================
// Stage selection (same as original, naming maintained)
// ============================================================================

#[derive(Debug, Clone, Copy)]
struct Stages {
    pass1: bool,
    pass1_verbose: bool,
    pass2: bool,
    viz_html: bool,
    viz_json: bool,
    tree: bool,
}

impl Stages {
    fn from_args(args: &ParseArgs) -> Self {
        let has_selector = args.pass1 || args.pass2 || args.tree || args.ast || args.all;

        let pass1 = args.all || args.pass1 || !has_selector;
        let pass1_verbose = pass1;
        let pass2 = args.all || args.pass2 || !has_selector;
        let tree = args.tree || args.ast || args.all;
        let viz_html = args.all || args.viz;
        let viz_json = args.viz_json;

        Self {
            pass1,
            pass1_verbose,
            pass2,
            viz_html,
            viz_json,
            tree,
        }
    }
}

// ============================================================================
// Pass1 collector — assemble lib global table + diagnostic snapshot into Pass1Report
// ============================================================================

pub fn public_collect_pass0() -> Pass0Report {
    // Directly snapshot `mcc_diagnose_all()` full amount outside PhaseTracker:
    // This phase hasn't established pass1/pass2 cursor yet, and we want to explicitly label lib load + C parser
    // errors as Pass0. tracker.new() runs after caller, so synchronous snapshot here
    // avoids the old bug of "swallowing critical C parser errors when no module".
    let diagnostics = batch_from_mcc(&mcc::mcc_diagnose_all(), Phase::Pass0);
    Pass0Report {
        loaded_files: vec![],
        diagnostics,
    }
}

pub fn public_collect_pass1(_uri: &McURI, tracker: &mut PhaseTracker) -> Pass1Report {
    let mut definitions = DefinitionsIndex::default();

    if let Some(modules) = try_collect_modules() {
        definitions.modules = modules;
    }
    if let Some(components) = try_collect_components() {
        definitions.components = components;
    }
    if let Some(interfaces) = try_collect_interfaces() {
        definitions.interfaces = interfaces;
    }
    if let Some(enums) = try_collect_enums() {
        definitions.enums = enums;
    }

    let loaded_files = group_by_uri(&definitions);

    let diagnostics = tracker.collect(Phase::Pass1);

    Pass1Report {
        loaded_files,
        definitions,
        diagnostics,
    }
}

fn try_collect_modules() -> Option<Vec<DefinitionRef>> {
    let items = mcc::mcb_iter_modules();
    if items.is_empty() {
        return None;
    }
    let mut defs: Vec<DefinitionRef> = items
        .into_iter()
        .map(|(name, uri)| DefinitionRef { name, uri })
        .collect();
    // Global table iteration order is HashMap-derived (nondeterministic); sort
    // so the pass1 definitions listing is stable run-to-run.
    defs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uri.cmp(&b.uri)));
    Some(defs)
}

fn try_collect_components() -> Option<Vec<DefinitionRef>> {
    let items = mcc::mcb_iter_components();
    if items.is_empty() {
        return None;
    }
    let mut defs: Vec<DefinitionRef> = items
        .into_iter()
        .map(|(name, uri)| DefinitionRef { name, uri })
        .collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uri.cmp(&b.uri)));
    Some(defs)
}

fn try_collect_interfaces() -> Option<Vec<DefinitionRef>> {
    let items = mcc::mcb_iter_interfaces();
    if items.is_empty() {
        return None;
    }
    let mut defs: Vec<DefinitionRef> = items
        .into_iter()
        .map(|(name, uri)| DefinitionRef { name, uri })
        .collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uri.cmp(&b.uri)));
    Some(defs)
}

fn try_collect_enums() -> Option<Vec<DefinitionRef>> {
    let items = mcc::mcb_iter_enums();
    if items.is_empty() {
        return None;
    }
    let mut defs: Vec<DefinitionRef> = items
        .into_iter()
        .map(|(name, uri)| DefinitionRef { name, uri })
        .collect();
    defs.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.uri.cmp(&b.uri)));
    Some(defs)
}

fn group_by_uri(defs: &DefinitionsIndex) -> Vec<LoadedFile> {
    use std::collections::BTreeMap;
    let mut by_uri: BTreeMap<String, LoadedFile> = BTreeMap::new();

    for d in &defs.modules {
        let uri = d.uri.clone();
        let is_system = uri.contains("/mcode/");
        let entry = by_uri.entry(uri.clone()).or_insert_with(|| LoadedFile {
            uri,
            is_system,
            modules: vec![],
            components: vec![],
            interfaces: vec![],
            enums: vec![],
        });
        entry.modules.push(d.name.clone());
    }
    for d in &defs.components {
        let uri = d.uri.clone();
        let is_system = uri.contains("/mcode/");
        let entry = by_uri.entry(uri.clone()).or_insert_with(|| LoadedFile {
            uri,
            is_system,
            modules: vec![],
            components: vec![],
            interfaces: vec![],
            enums: vec![],
        });
        entry.components.push(d.name.clone());
    }
    for d in &defs.interfaces {
        let uri = d.uri.clone();
        let is_system = uri.contains("/mcode/");
        let entry = by_uri.entry(uri.clone()).or_insert_with(|| LoadedFile {
            uri,
            is_system,
            modules: vec![],
            components: vec![],
            interfaces: vec![],
            enums: vec![],
        });
        entry.interfaces.push(d.name.clone());
    }
    for d in &defs.enums {
        let uri = d.uri.clone();
        let is_system = uri.contains("/mcode/");
        let entry = by_uri.entry(uri.clone()).or_insert_with(|| LoadedFile {
            uri,
            is_system,
            modules: vec![],
            components: vec![],
            interfaces: vec![],
            enums: vec![],
        });
        entry.enums.push(d.name.clone());
    }

    by_uri.into_values().collect()
}

// ============================================================================
// Pass2 collector — convert MccProjectTree to InstanceNode + nets + connections
// ============================================================================

pub fn public_collect_pass2(
    top: &str,
    inst: &mcc::MccProjectTree,
    arena: Option<&mcc::NodeArena>,
    net_store: &mcc::NetTableStore,
    tracker: &mut PhaseTracker,
) -> Pass2Report {
    let instances = Some(instance_to_node(inst, arena));
    let nets = extract_nets(inst, arena, net_store);
    let connections = extract_connections(inst, arena, net_store);
    let diagnostics = tracker.collect(Phase::Pass2);

    Pass2Report {
        top: top.to_string(),
        instances,
        nets,
        connections,
        diagnostics,
    }
}

fn instance_to_node(inst: &mcc::MccProjectTree, arena: Option<&mcc::NodeArena>) -> InstanceNode {
    let mut ports = Vec::new();
    for p in inst.ports.iter() {
        if matches!(p.iotype, IOType::None | IOType::NonCon | IOType::Return) {
            continue;
        }
        ports.push(PortInfo {
            name: p.name.to_string(),
            iotype: iotype_str(&p.iotype).into(),
        });
    }

    let components = inst
        .components
        .iter()
        .map(|c| {
            let mut pins: Vec<PinInfo> = c
                .pins
                .iter()
                .map(|(pin_id, _net_point)| {
                    let pin_name = c.pin_name(pin_id).unwrap_or_else(|| pin_id.clone());
                    PinInfo {
                        id: pin_id.clone(),
                        name: pin_name,
                    }
                })
                .collect();
            // Deterministic pin order: numeric id ascending, non-numeric at the
            // end. `McComponentInst.pins` is a HashMap with a per-process
            // random seed (RandomState), so without this the pin list order
            // changes on every run. Mirrors PinSortMode::PinId (print.rs).
            pins.sort_by_key(|p| p.id.parse::<i64>().ok().unwrap_or(i64::MAX));
            ComponentInfo {
                name: c.name.to_string(),
                class_name: c.def.name.to_string(),
                pins,
                nc: c.nc,
            }
        })
        .collect();

    let subs: Vec<&mcc::MccProjectTree> = match arena {
        Some(a) => mcc::arena_sub_modules(a, inst).collect(),
        None => inst.sub_modules.iter().collect(),
    };
    let sub_modules = subs
        .into_iter()
        .map(|s| instance_to_node(s, arena))
        .collect();

    InstanceNode {
        name: inst.name.to_string(),
        kind: "module".into(),
        class_name: inst.def.name.to_string(),
        synthetic: mcc::mcc_is_synthetic_module(&inst.def.name.to_string()),
        ports,
        components,
        sub_modules,
    }
}

fn iotype_str(io: &IOType) -> &'static str {
    match io {
        IOType::In => "in",
        IOType::Out => "out",
        IOType::InOut => "inout",
        IOType::Power => "power",
        IOType::Analog => "analog",
        IOType::Return => "return",
        IOType::NonCon => "noncon",
        IOType::Label => "label",
        IOType::None => "none",
    }
}

fn extract_connections(
    inst: &mcc::MccProjectTree,
    arena: Option<&mcc::NodeArena>,
    net_store: &mcc::NetTableStore,
) -> Vec<ConnectionEntry> {
    let mut out = Vec::new();
    walk_connections(inst, "", arena, net_store, &mut out);
    out
}

fn walk_connections(
    inst: &mcc::MccProjectTree,
    scope: &str,
    arena: Option<&mcc::NodeArena>,
    net_store: &mcc::NetTableStore,
    out: &mut Vec<ConnectionEntry>,
) {
    // Full scope path (e.g. `main.speaker`): the engine's connection ids and
    // instance names repeat across modules, so each entry must carry its scope
    // to stay unambiguous.
    let my_scope = if scope.is_empty() {
        inst.name.clone()
    } else {
        format!("{}.{}", scope, inst.name)
    };
    // Every connection's points merge into exactly one net in this module's
    // net table (Phase D: read from the frozen store, keyed by the module's
    // canonical scope path), so resolve the net name from the table — not
    // from the statement label. `ConnectionInst.net_name` keeps the label
    // *as written* (bare wires have None; merged rails keep a pre-merge name
    // like `V1V2.GND` that no longer exists as a net). Resolving against the
    // table gives every connection the same surviving name as the matching
    // Nets table row — including engine-assigned anonymous `_net{N}` numbers
    // — so the two tables always agree and stay stable across runs. The
    // statement label is only a fallback for points absent from the table
    // (e.g. NC).
    let mut point_to_net: HashMap<&str, &str> = HashMap::new();
    // Phase D: the module's frozen union-find net table comes from the store,
    // keyed by its canonical scope path.
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
        out.push(ConnectionEntry {
            id: conn.id,
            module: my_scope.clone(),
            net_name,
            points: conn.points.iter().map(|p| p.path.clone()).collect(),
        });
    }
    let subs: Vec<&mcc::MccProjectTree> = match arena {
        Some(a) => mcc::arena_sub_modules(a, inst).collect(),
        None => inst.sub_modules.iter().collect(),
    };
    for sub in subs {
        walk_connections(sub, &my_scope, arena, net_store, out);
    }
}

fn extract_nets(
    inst: &mcc::MccProjectTree,
    arena: Option<&mcc::NodeArena>,
    net_store: &mcc::NetTableStore,
) -> Vec<NetEntry> {
    let mut nets = Vec::new();
    walk_nets(inst, "", arena, net_store, &mut nets);
    nets
}

fn walk_nets(
    inst: &mcc::MccProjectTree,
    scope: &str,
    arena: Option<&mcc::NodeArena>,
    net_store: &mcc::NetTableStore,
    out: &mut Vec<NetEntry>,
) {
    let my_scope = if scope.is_empty() {
        inst.name.clone()
    } else {
        format!("{}.{}", scope, inst.name)
    };
    // Phase D: the module's frozen union-find net table comes from the store
    // (pre-sorted by build_net_table), keyed by its canonical scope path.
    if let Some(table) = net_store.get(&my_scope) {
        for (name, points) in table {
            out.push(NetEntry {
                module: my_scope.clone(),
                name: name.to_string(),
                points: points.iter().map(|point| point.path.clone()).collect(),
            });
        }
    }
    let subs: Vec<&mcc::MccProjectTree> = match arena {
        Some(a) => mcc::arena_sub_modules(a, inst).collect(),
        None => inst.sub_modules.iter().collect(),
    };
    for sub in subs {
        walk_nets(sub, &my_scope, arena, net_store, out);
    }
}

// ============================================================================
// Viz pipeline (keep as-is, add quiet/json_mode guards)
// ============================================================================

/// Combine multiple SVG strings into one large SVG, stacked vertically with module labels.
///
/// Each input SVG's content is extracted from its `<svg>` tag and placed in a
/// nested `<svg>` group with a title label. The combined canvas is sized to fit all.
fn combine_svgs(svgs: &[(String, String)]) -> String {
    let gap = 40.0; // vertical gap between modules
    let label_height = 20.0;
    let margin = 20.0;

    // Parse each SVG to extract viewBox dimensions and inner content
    let mut items: Vec<(String, f64, f64, String)> = Vec::new(); // (name, w, h, inner)
    let mut max_w: f64 = 0.0;

    for (name, svg) in svgs {
        // Extract viewBox
        let vb = extract_viewbox(svg);
        let w = vb.0.max(1.0);
        let h = vb.1.max(1.0);
        max_w = max_w.max(w);

        // Extract inner content (everything between <svg ...> and </svg>)
        let inner = extract_svg_inner(svg);
        items.push((name.clone(), w, h, inner));
    }

    let total_w = max_w + margin * 2.0;
    let mut total_h = margin;
    for (_, _, h, _) in &items {
        total_h += label_height + *h + gap;
    }
    total_h += margin;

    let mut out = format!(
        r#"<svg viewBox="0 0 {:.1} {:.1}" xmlns="http://www.w3.org/2000/svg"
     font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
     style="background:transparent">
"#,
        total_w, total_h
    );

    let mut y = margin;
    for (name, w, h, inner) in &items {
        // Module label
        out.push_str(&format!(
            r##"  <text x="{:.1}" y="{:.1}" font-size="16" font-weight="700" fill="#333">{}</text>
"##,
            margin,
            y + 16.0,
            escape_xml_viz(name)
        ));
        y += label_height;

        // Nested SVG group, centered horizontally
        let x_offset = (max_w - w) / 2.0 + margin;
        out.push_str(&format!(
            r##"  <g transform="translate({:.1},{:.1})">
{}
  </g>
"##,
            x_offset, y, inner
        ));
        y += h + gap;
    }

    out.push_str("</svg>\n");
    out
}

/// Extract (width, height) from an SVG viewBox attribute.
fn extract_viewbox(svg: &str) -> (f64, f64) {
    // Find viewBox="0 0 W H"
    if let Some(start) = svg.find("viewBox=\"") {
        let rest = &svg[start + 9..];
        if let Some(end) = rest.find('"') {
            let vb = &rest[..end];
            let parts: Vec<&str> = vb.split_whitespace().collect();
            if parts.len() >= 4 {
                let w = parts[2].parse::<f64>().unwrap_or(200.0);
                let h = parts[3].parse::<f64>().unwrap_or(100.0);
                return (w, h);
            }
        }
    }
    (200.0, 100.0)
}

/// Extract the inner content of an SVG (everything between the opening <svg...> and closing </svg>).
fn extract_svg_inner(svg: &str) -> String {
    // Find the first '>' after '<svg'
    if let Some(start) = svg.find("<svg") {
        if let Some(gt) = svg[start..].find('>') {
            let inner_start = start + gt + 1;
            if let Some(end) = svg.rfind("</svg>") {
                return svg[inner_start..end].trim().to_string();
            }
        }
    }
    svg.to_string()
}

fn escape_xml_viz(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn run_viz(
    ident: &McIds,
    uri: &McURI,
    args: &ParseArgs,
    json_mode_viz: bool,
    renderer: &dyn renderer::OutputRenderer,
) -> Result<VizData> {
    use tracing::{debug, info, warn};

    info!(target: "mcc_cli::viz", "generating circuit visualization");

    let (inst, table, arena) = mcc::mcc_build_flat_with_arena(ident, uri, 1000)
        .map_err(|e| anyhow::anyhow!("mcc_build_flat failed: {}", e))?;

    debug!(
        target: "mcc_cli::viz",
        entries = table.len(),
        nets = table.net_count(),
        components = table.get_components().len(),
        modules = table.get_modules().len(),
        "InstTable"
    );

    mcc::vector::builder::reset_np_warn_count();
    let vec_block = mcc::build_mc_vec_with_arena(&inst, &table, &arena);
    debug!(
        target: "mcc_cli::viz",
        bid = vec_block.bid,
        insts = vec_block.inst_count(),
        nets = vec_block.net_count(),
        blocks = vec_block.blocks.len(),
        "McVecBlock"
    );

    // ★ netcheck: netlist health check
    let nc_report = mcc::instant::netcheck::run(&table);
    nc_report.print();

    let graph = mcc::build_mc_vec_graph(&vec_block, &table);
    // PR-3C: capture count before render_with consumes graph
    let graph_box_count = graph.boxes.len();
    let graph_edge_count = graph.edges.len();
    debug!(
        target: "mcc_cli::viz",
        boxes = graph_box_count,
        edges = graph_edge_count,
        sub_graphs = graph.sub_graphs.len(),
        "McVecGraph"
    );

    if graph_box_count == 0 {
        warn!(target: "mcc_cli::viz", "0 boxes in graph");
    }

    let opts = mcc::viz::api::RenderOpts::default();
    let doc = mcc::viz::api::render_with(graph, opts);

    let (output_text, format_name) = if json_mode_viz {
        (doc.to_json(), "json".to_string())
    } else {
        let html = mcc::viz::template::wrap_document(&doc);
        debug!(
            target: "mcc_cli::viz",
            layers = doc.layer_count(),
            svg_bytes = doc.total_svg_bytes(),
            html_bytes = html.len(),
            "VizDocument"
        );
        (html, "html".to_string())
    };

    let written_to = if let Some(p) = &mcc::cli::globals().output {
        std::fs::write(p, &output_text).with_context(|| format!("Failed to write file: {}", p))?;
        renderer.viz_written(p, output_text.len());
        Some(p.clone())
    } else if !json_mode_viz {
        // Derive output path from input file: <input_dir>/<input_name>.html
        let path = args
            .target
            .as_ref()
            .filter(|t| Path::new(t).exists() && Path::new(t).is_file())
            .map(|t| {
                let p = Path::new(t);
                let stem = p.file_stem().unwrap().to_string_lossy();
                let parent = p.parent().unwrap_or(Path::new(""));
                parent.join(format!("{}.html", stem))
            })
            .unwrap_or_else(|| Path::new("circuit.html").to_path_buf());
        let path_str = path.to_string_lossy().to_string();
        std::fs::write(&path, &output_text)
            .with_context(|| format!("Failed to write file: {}", path_str))?;
        renderer.viz_written(&path_str, output_text.len());
        Some(path_str)
    } else {
        None
    };

    Ok(VizData {
        format: format_name,
        written_to,
        bytes: output_text.len(),
        layers: doc.layer_count(),
        boxes: graph_box_count,
        edges: graph_edge_count,
    })
}

// ============================================================================
// Tree → JSON value (for view mode)
// ============================================================================

fn phrase_to_tree_json(p: &McPhrase, max_depth: usize, cur: usize) -> serde_json::Value {
    use serde_json::json;

    let truncated = max_depth > 0 && cur >= max_depth;
    let recurse = |children: &[McPhrase]| -> Vec<serde_json::Value> {
        if truncated {
            Vec::new()
        } else {
            children
                .iter()
                .map(|c| phrase_to_tree_json(c, max_depth, cur + 1))
                .collect()
        }
    };

    match p {
        McPhrase::Series(ps, _) => json!({
            "kind": "Series",
            "label": format!("{} items", ps.len()),
            "children": recurse(ps),
        }),
        McPhrase::Parallel(ps) => json!({
            "kind": "Parallel",
            "label": format!("{} items", ps.len()),
            "children": recurse(ps),
        }),
        McPhrase::Multiple(ps) => json!({
            "kind": "Multiple",
            "label": format!("{} items", ps.len()),
            "children": if truncated {
                Vec::new()
            } else {
                ps.iter()
                    .map(|c| {
                        if matches!(c, McPhrase::Lead) {
                            // §1 P5.1: within a `[...]` vector, `_` is a placeholder
                            json!({"kind": "Lead", "usage": "placeholder", "label": "", "children": []})
                        } else {
                            phrase_to_tree_json(c, max_depth, cur + 1)
                        }
                    })
                    .collect()
            },
        }),
        McPhrase::Group(g) => json!({
            "kind": "Group",
            "label": format!("{} opds", g.opds.len()),
            "children": recurse(&g.opds),
        }),
        McPhrase::Closure(c) => json!({
            "kind": "Closure",
            "label": format!("params={} body={}", c.params.len(), c.body.len()),
            "children": recurse(&c.body),
        }),
        McPhrase::FuncCall(fc) => json!({
            "kind": "FuncCall",
            "label": format!("{}({} args)", fc.func_name, fc.params.len()),
            "children": fc.caller.as_ref().map(|c| vec![phrase_to_tree_json(c, max_depth, cur + 1)]).unwrap_or_default(),
        }),
        McPhrase::Transposed(inner) => json!({
            "kind": "Transposed",
            "label": "",
            "children": [phrase_to_tree_json(inner, max_depth, cur + 1)],
        }),
        McPhrase::Member(inner, ep) => json!({
            "kind": "Member",
            "label": format!(".{}", ep),
            "children": [phrase_to_tree_json(inner, max_depth, cur + 1)],
        }),
        McPhrase::Lead => json!({
            "kind": "Lead",
            "usage": "passthrough",
            "label": "",
            "children": [],
        }),
        McPhrase::Endpoint(ep) => json!({
            "kind": "Endpoint",
            "label": endpoint_label(ep),
            "children": [],
        }),
    }
}

fn endpoint_label(ep: &McEndpoint) -> String {
    match ep {
        McEndpoint::Single(McInstanceRef {
            base: McInstance::Component(c),
            ..
        }) => format!("component:{}", c.name),
        McEndpoint::Single(McInstanceRef {
            base: McInstance::Module(m),
            ..
        }) => format!("module:{}", m.name),
        McEndpoint::Single(McInstanceRef {
            base: McInstance::Label(l),
            ..
        }) => format!("label:{}", l),
        McEndpoint::Single(McInstanceRef { base: p, .. }) => format!("port:{}", p),
        McEndpoint::Node { .. } => "node".to_string(),
        McEndpoint::List(_) => "list".to_string(),
    }
}

// ============================================================================
// Error emit helper
// ============================================================================

fn emit_error(err: RpcError, dlog: bool) -> Result<()> {
    if dlog {
        // dlog mode only suppresses the pretty envelope, not fatal errors.
        // Emit any accumulated diagnostics, then surface the error so the
        // process exits non-zero instead of silently succeeding.
        output::diagnostic::print_dlog_lines(false);
        return Err(anyhow::anyhow!(err.message));
    }
    if mcc::cli::globals().format.is_structured() {
        let env = Envelope::err(err);
        output::emit_envelope(
            &env,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
            false,
        )?;
        Ok(())
    } else {
        Err(anyhow::anyhow!(err.message))
    }
}

// ============================================================================
// cmie_to_tree_json — generic tree view for component / interface / enum
// ============================================================================

/// Extract the type annotation (class/unit) from a parameter declaration.
fn param_cls(d: &McParamDeclare) -> Option<String> {
    match &d.param_type.kind {
        McParamTypeKind::UnitValue { unit } => Some(format!("UV.{}", unit)),
        McParamTypeKind::UnitValueDefault { unit, .. } => Some(format!("UV.{}", unit)),
        McParamTypeKind::CompoundUnit { unit_type, .. } => Some(unit_type.to_string()),
        McParamTypeKind::EnumClass { class_name } => Some(class_name.clone()),
        McParamTypeKind::EnumClassDefault { class_name, .. } => Some(class_name.clone()),
        McParamTypeKind::Interface { class_name, .. } => Some(class_name.clone()),
        McParamTypeKind::InterfaceWithRole { class_name, .. } => Some(class_name.clone()),
        McParamTypeKind::ComponentInstance { class_name } => Some(class_name.clone()),
        McParamTypeKind::BasicString { .. } => Some("STRING".into()),
        McParamTypeKind::BasicInt { .. } => Some("INT".into()),
        McParamTypeKind::BasicHex { .. } => Some("HEX".into()),
        McParamTypeKind::BasicFloat { .. } => Some("FLOAT".into()),
        _ => None,
    }
}

/// Extract the default value from a parameter declaration, if any.
fn param_default(d: &McParamDeclare) -> Option<String> {
    match &d.param_type.kind {
        McParamTypeKind::UnitValueDefault { default_val, .. } => default_val.clone(),
        McParamTypeKind::EnumClassDefault { default_val, .. } => default_val.clone(),
        McParamTypeKind::CompoundUnit { default_val, .. } => default_val.clone(),
        _ => None,
    }
}

/// Build a JSON tree representation for a non-Module CMIE definition.
fn cmie_to_tree_json(cmie: &McCMIE, _max_depth: usize) -> serde_json::Value {
    match cmie {
        McCMIE::Component(c) => {
            // ── params: name, cls (type annotation), default ──
            let params: Vec<_> = c
                .params
                .iter()
                .map(|d| {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "name".into(),
                        json!(d.get_primary_name().unwrap_or_default()),
                    );
                    if let Some(cls) = param_cls(d) {
                        obj.insert("cls".into(), json!(cls));
                    }
                    if let Some(def) = param_default(d) {
                        obj.insert("default".into(), json!(def));
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();

            // ── attrs: key = value ──
            let attrs: Vec<_> = c
                .attrs
                .iter()
                .map(|a| {
                    let vals: Vec<String> = a.values.iter().map(|v| v.to_string()).collect();
                    json!({
                        "key": a.id.to_string(),
                        "value": if vals.len() == 1 { vals[0].clone() } else { vals.join(", ") },
                    })
                })
                .collect();

            // ── pins: id, names, iotype ──
            let pins: Vec<_> = c
                .pins
                .pins
                .iter()
                .map(|(id, pin)| {
                    json!({
                        "id": id,
                        "names": pin.names,
                        "io": iotype_str(&pin.iotype),
                    })
                })
                .collect();

            // ── funcs: name, param count ──
            let funcs: Vec<_> = c
                .funcs
                .iter()
                .map(|f| {
                    json!({
                        "name": f.name.to_string(),
                        "params": f.params.len(),
                    })
                })
                .collect();

            let mut obj = serde_json::Map::new();
            obj.insert("kind".into(), json!("component"));
            obj.insert("name".into(), json!(c.name.to_string()));
            obj.insert("uri".into(), json!(c.uri.to_string()));
            obj.insert("params".into(), json!(params));
            if !attrs.is_empty() {
                obj.insert("attrs".into(), json!(attrs));
            }
            if !pins.is_empty() {
                obj.insert("pins".into(), json!(pins));
            }
            if !funcs.is_empty() {
                obj.insert("funcs".into(), json!(funcs));
            }
            serde_json::Value::Object(obj)
        }
        McCMIE::Interface(i) => {
            let params: Vec<_> = i
                .params
                .iter()
                .map(|d| {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "name".into(),
                        json!(d.get_primary_name().unwrap_or_default()),
                    );
                    if let Some(cls) = param_cls(d) {
                        obj.insert("cls".into(), json!(cls));
                    }
                    if let Some(def) = param_default(d) {
                        obj.insert("default".into(), json!(def));
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();
            json!({
                "kind": "interface",
                "name": i.name.to_string(),
                "params": params,
            })
        }
        McCMIE::Enum(e) => {
            let values: Vec<_> = e.values.iter().map(|v| v.name.to_string()).collect();
            json!({
                "kind": "enum",
                "name": e.name.to_string(),
                "uri": e.uri.to_string(),
                "values": values,
            })
        }
        McCMIE::Module(_) => {
            json!({"kind": "module", "note": "use phrase tree for modules"})
        }
    }
}
