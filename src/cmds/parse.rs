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
use serde_json::json;
use std::path::Path;

// ============================================================================
// Entry point
// ============================================================================

pub fn run(args: &ParseArgs) -> Result<()> {
    // ── 0. RPC takes priority (server mode) ──
    if let Some(client) = RpcClient::probe() {
        let params = json!({
            "entry": args.target.clone(),
            "top":   args.top.clone(),
            "code":  args.code.clone(),
            "libs":  args.lib.clone(),
        });
        let result = client.call("parse", params)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // ── 0.5. Local mode initialization (don't load library by default), then explicitly load via --lib ──
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    manifest::load_libs(&args.lib);

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
        let p = Path::new(t);
        if p.is_dir() {
            match manifest::build_from_manifest(p, args.top.as_deref(), None) {
                Ok((entry_uri, top)) => {
                    forced_top = Some(top);
                    McURI::from(entry_uri.as_str())
                }
                Err(e) => return emit_error(args, RpcError::invalid_params(format!("{:#}", e))),
            }
        } else {
            let uri = McURI::from(t.as_str());
            mcc::mcc_load_project(&uri);
            uri
        }
    } else {
        return emit_error(
            args,
            RpcError::invalid_params("parse: <target>/--code not specified"),
        );
    };

    // ── 2. Stage selection ──
    let stages = Stages::from_args(args);
    let renderer = renderer::for_format_with_sort(args.format, args.sort);

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

    // ── 4. Select top module ──
    let top_name = match forced_top
        .clone()
        .or_else(|| args.top.clone())
        .or_else(|| mcc::mcb_get_module_name_by_uri(&uri))
        .or_else(|| mcc::mcb_get_first_module_name())
    {
        Some(n) => n,
        None => {
            // Even if top module not found, pass0 is already snapshotted in step 3.5, directly finish.
            // Text mode will render pass0 section (including C parser errors) + summary.
            let env = Envelope::ok(builder.finish());
            output::emit_envelope(
                &env,
                args.format,
                args.output.as_deref().map(Path::new),
                false,
            )?;
            return Ok(());
        }
    };

    // ── 5. get_def: get top-level module definition ──
    let ident = McIds::from(top_name.as_str());
    let cmie = match mcc::get_def(&ident, &uri) {
        Some(c) => c,
        None => {
            return emit_error(
                args,
                RpcError::invalid_params(format!("parse: definition '{}' not found", top_name)),
            );
        }
    };

    let module_def = match cmie {
        McCMIE::Module(m) => m,
        McCMIE::Component(c) => {
            return emit_error(
                args,
                RpcError::invalid_params(format!("'{}' is Component, not Module", c.name)),
            );
        }
        McCMIE::Interface(i) => {
            return emit_error(
                args,
                RpcError::invalid_params(format!("'{}' is Interface, not Module", i.name)),
            );
        }
        McCMIE::Enum(e) => {
            return emit_error(
                args,
                RpcError::invalid_params(format!("'{}' is Enum, not Module", e.name)),
            );
        }
    };

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
                        renderer.module_lines(&def);
                    }
                }
            }
        }

        let pass1 = public_collect_pass1(&uri, &mut tracker);
        builder.set_pass1(pass1);
    }

    // ── 7. tree / ast: go through view field (replacement output) ──
    if stages.tree {
        let mut nodes = Vec::with_capacity(module_def.lines.len());
        for line in module_def.lines.iter() {
            nodes.push(phrase_to_tree_json(line, args.depth, 0));
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

    // ── 8. Pass2 assembly ──
    if stages.pass2 {
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
        //   3. Fallback to "main" if no top is specified
        //
        // Strategy 2: No Top Module (Flat/Peer Modules)
        //   - When a file contains multiple peer modules without hierarchical instantiation,
        //     render ALL modules as if each were a top-level module.
        //   - This is called "virtual instantiation" - each module is instantiated once
        //     just to visualize/analyze the module definitions without hierarchical context.
        //
        // Detection: If no --top is specified and multiple modules exist in the file,
        //            we assume virtual instantiation mode.
        // ─────────────────────────────────────────────────────────────────────────────

        // Check if we should render all modules (no explicit --top specified and multiple modules exist)
        let has_explicit_top = forced_top.is_some() || args.top.is_some();
        let all_modules: Vec<(String, String)> = mcc::mcb_iter_modules()
            .into_iter()
            .filter(|(_, module_uri)| {
                // Only modules from the target file
                module_uri == uri.as_str() || module_uri.ends_with(uri.as_str())
            })
            .collect();
        let should_render_all = !has_explicit_top && all_modules.len() > 1;

        if should_render_all {
            // Multi-module rendering: build each module separately
            let first_mod_name = all_modules
                .first()
                .map(|(n, _)| n.clone())
                .unwrap_or_else(|| top_name.clone());
            let first_mod_ident = McIds::from(first_mod_name.as_str());

            match mcc::mcc_build(&first_mod_ident, &uri) {
                Ok(first_inst) => {
                    // Process ALL modules: first one already built, others in loop
                    for (idx, (mod_name, _)) in all_modules.iter().enumerate() {
                        if idx == 0 {
                            // First module already built, output its details
                            renderer.pass2_header(mod_name);
                            renderer.instances(&first_inst, 0);
                            renderer.connections(&first_inst, 0);
                            renderer.nets(&first_inst, 0);
                            if args.format == mcc::cli::OutputFormat::Text {
                                builder.print_diagnostics_summary();
                            }
                            renderer.net_summary(&first_inst);
                        } else {
                            // Build and output subsequent modules
                            let mod_ident = McIds::from(mod_name.as_str());
                            if let Ok(mod_inst) = mcc::mcc_build(&mod_ident, &uri) {
                                renderer.pass2_header(mod_name);
                                renderer.instances(&mod_inst, 0);
                                renderer.connections(&mod_inst, 0);
                                renderer.nets(&mod_inst, 0);
                                renderer.net_summary(&mod_inst);
                            }
                        }
                    }
                    // Use first module for pass2 collection
                    let pass2 = public_collect_pass2(&top_name, &first_inst, &mut tracker);
                    builder.set_pass2(pass2);
                }
                Err(e) => {
                    renderer.pass2_failed(&format!("{}", e));
                    let err = RpcError::build_error(format!("{}", e));
                    emit_error(args, err)?;
                }
            }
        } else {
            // Single module rendering: call pass2_header only once
            renderer.pass2_header(&top_name);

            match mcc::mcc_build(&ident, &uri) {
                Ok(inst) => {
                    renderer.instances(&inst, 0);
                    renderer.connections(&inst, 0);
                    renderer.nets(&inst, 0);

                    let pass2 = public_collect_pass2(&top_name, &inst, &mut tracker);
                    builder.set_pass2(pass2);

                    // Print diagnostics before Net Summary
                    if args.format == mcc::cli::OutputFormat::Text {
                        builder.print_diagnostics_summary();
                    }
                    renderer.net_summary(&inst);
                }
                Err(e) => {
                    renderer.pass2_failed(&format!("{}", e));
                    let err = RpcError::build_error(format!("{}", e));
                    emit_error(args, err)?;
                }
            }
        }
    }

    // ── 9. Viz assembly ──
    if stages.viz_html || stages.viz_json {
        let has_explicit_top = forced_top.is_some() || args.top.is_some();
        if has_explicit_top {
            match run_viz(&ident, &uri, args, stages.viz_json, &*renderer) {
                Ok(viz) => {
                    builder.set_viz(viz);
                }
                Err(e) => {
                    return emit_error(args, RpcError::internal_error(format!("viz: {}", e)));
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
                        return emit_error(args, RpcError::internal_error(format!("viz: {}", e)));
                    }
                }
            } else {
                let mut total_boxes = 0;
                let mut total_edges = 0;
                let mut svgs: Vec<(String, String)> = Vec::new(); // (module_name, svg_string)

                for (mod_name, module_uri) in &all_modules {
                    let mod_ident = McIds::from(mod_name.as_str());
                    let mod_mc_uri = McURI::from(module_uri.as_str());

                    let (inst, table) = match mcc::mcc_build_flat(&mod_ident, &mod_mc_uri, 1000) {
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
                    let vec_block = mcc::build_mc_vec(&inst, &table);
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
                    return emit_error(args, RpcError::internal_error("viz: no modules rendered"));
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

                let out_path = if let Some(ref p) = args.output {
                    Path::new(p).to_path_buf()
                } else {
                    let p = Path::new(args.target.as_ref().unwrap());
                    let stem = p.file_stem().unwrap().to_string_lossy();
                    let parent = p.parent().unwrap_or(Path::new(""));
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
    let env = Envelope::ok(builder.finish());
    let target = args.output.as_deref().map(Path::new);

    let envelope_target = if (stages.viz_html || stages.viz_json) && target.is_some() {
        None
    } else {
        target
    };

    output::emit_envelope(&env, args.format, envelope_target, true)?;
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
    Some(
        items
            .into_iter()
            .map(|(name, uri)| DefinitionRef { name, uri })
            .collect(),
    )
}

fn try_collect_components() -> Option<Vec<DefinitionRef>> {
    let items = mcc::mcb_iter_components();
    if items.is_empty() {
        return None;
    }
    Some(
        items
            .into_iter()
            .map(|(name, uri)| DefinitionRef { name, uri })
            .collect(),
    )
}

fn try_collect_interfaces() -> Option<Vec<DefinitionRef>> {
    let items = mcc::mcb_iter_interfaces();
    if items.is_empty() {
        return None;
    }
    Some(
        items
            .into_iter()
            .map(|(name, uri)| DefinitionRef { name, uri })
            .collect(),
    )
}

fn try_collect_enums() -> Option<Vec<DefinitionRef>> {
    let items = mcc::mcb_iter_enums();
    if items.is_empty() {
        return None;
    }
    Some(
        items
            .into_iter()
            .map(|(name, uri)| DefinitionRef { name, uri })
            .collect(),
    )
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
    tracker: &mut PhaseTracker,
) -> Pass2Report {
    let instances = Some(instance_to_node(inst));
    let nets = extract_nets(inst);
    let connections = extract_connections(inst);
    let diagnostics = tracker.collect(Phase::Pass2);

    Pass2Report {
        top: top.to_string(),
        instances,
        nets,
        connections,
        diagnostics,
    }
}

fn instance_to_node(inst: &mcc::MccProjectTree) -> InstanceNode {
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
        .map(|c| ComponentInfo {
            name: c.name.to_string(),
            class_name: c.def.name.to_string(),
            pins: c
                .pins
                .iter()
                .map(|(pin_id, _net_point)| {
                    let pin_name = c.pin_name(pin_id).unwrap_or_else(|| pin_id.clone());
                    PinInfo {
                        id: pin_id.clone(),
                        name: pin_name,
                    }
                })
                .collect(),
            nc: c.nc,
        })
        .collect();

    let sub_modules = inst.sub_modules.iter().map(instance_to_node).collect();

    InstanceNode {
        name: inst.name.to_string(),
        kind: "module".into(),
        class_name: inst.def.name.to_string(),
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

fn extract_connections(inst: &mcc::MccProjectTree) -> Vec<ConnectionEntry> {
    let mut out = Vec::new();
    walk_connections(inst, &mut out);
    out
}

fn walk_connections(inst: &mcc::MccProjectTree, out: &mut Vec<ConnectionEntry>) {
    for conn in &inst.connections {
        out.push(ConnectionEntry {
            id: conn.id as u64,
            net_name: conn.net_name.clone(),
            points: conn.points.iter().map(|p| p.path.clone()).collect(),
        });
    }
    for sub in &inst.sub_modules {
        walk_connections(sub, out);
    }
}

fn extract_nets(inst: &mcc::MccProjectTree) -> Vec<NetEntry> {
    use std::collections::BTreeMap;
    let mut by_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    walk_nets(inst, &mut by_name);
    by_name
        .into_iter()
        .map(|(name, points)| NetEntry { name, points })
        .collect()
}

fn walk_nets(
    inst: &mcc::MccProjectTree,
    out: &mut std::collections::BTreeMap<String, Vec<String>>,
) {
    for conn in &inst.connections {
        let net_name = conn
            .net_name
            .clone()
            .unwrap_or_else(|| format!("__net_{}", conn.id));
        let entry = out.entry(net_name).or_default();
        for p in &conn.points {
            if !entry.contains(&p.path) {
                entry.push(p.path.clone());
            }
        }
    }
    for sub in &inst.sub_modules {
        walk_nets(sub, out);
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

    let (inst, table) = mcc::mcc_build_flat(ident, uri, 1000)
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
    let vec_block = mcc::build_mc_vec(&inst, &table);
    debug!(
        target: "mcc_cli::viz",
        bid = vec_block.bid,
        insts = vec_block.inst_count(),
        nets = vec_block.net_count(),
        blocks = vec_block.blocks.len(),
        "McVecBlock"
    );

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

    let written_to = if let Some(p) = &args.output {
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
            vec![]
        } else {
            children
                .iter()
                .map(|c| phrase_to_tree_json(c, max_depth, cur + 1))
                .collect()
        }
    };

    match p {
        McPhrase::Series(ps) => json!({
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
            "children": recurse(ps),
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
        McPhrase::Lead => json!({"kind": "Lead", "label": "", "children": []}),
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

fn emit_error(args: &ParseArgs, err: RpcError) -> Result<()> {
    if args.format.is_structured() {
        let env = Envelope::err(err);
        output::emit_envelope(
            &env,
            args.format,
            args.output.as_deref().map(Path::new),
            false,
        )?;
        Ok(())
    } else {
        Err(anyhow::anyhow!(err.message))
    }
}
