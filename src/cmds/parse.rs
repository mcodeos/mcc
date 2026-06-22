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

use crate::cli::rpc_client::RpcClient;
use crate::cli::ParseArgs;
use crate::cmds::manifest;
use crate::cmds::print::{
    print_connections, print_module_inst, print_net_summary, print_nets, print_phrase_members,
};
use crate::output::{
    self,
    builder::ResultBuilder,
    diagnostic::{batch_from_mcc, PhaseTracker},
    envelope::{
        ComponentInfo, ConnectionEntry, DefinitionRef, DefinitionsIndex, Envelope, ExtractData,
        InstanceNode, LoadedFile, NetEntry, Pass0Report, Pass1Report, Pass2Report, Phase, PinInfo,
        PortInfo, RpcError, ViewData, VizData, WorkspaceRef,
    },
    renderer, OutputFormatExt,
};
use anyhow::{Context, Result};
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
        renderer.pass2_header(&top_name);

        match mcc::mcc_build(&ident, &uri) {
            Ok(inst) => {
                renderer.instances(&inst, 0);
                renderer.connections(&inst, 0);
                renderer.nets(&inst, 0);

                let pass2 = public_collect_pass2(&top_name, &inst, &mut tracker);
                builder.set_pass2(pass2);

                // Print diagnostics before Net Summary
                builder.print_diagnostics_summary();
                renderer.net_summary(&inst);
            }
            Err(e) => {
                renderer.pass2_failed(&format!("{}", e));
                return emit_error(
                    args,
                    RpcError::build_error(format!("instantiation failed: {}", e)),
                );
            }
        }
    }

    // ── 9. Viz assembly ──
    if stages.viz_html || stages.viz_json {
        match run_viz(&ident, &uri, args, stages.viz_json, &*renderer) {
            Ok(viz) => {
                builder.set_viz(viz);
            }
            Err(e) => {
                return emit_error(args, RpcError::internal_error(format!("viz: {}", e)));
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
                    let pin_name = c
                        .def
                        .pins
                        .pins
                        .get(pin_id)
                        .and_then(|p| p.names.first())
                        .cloned()
                        .unwrap_or_else(|| pin_id.clone());
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
        let path = "circuit.html".to_string();
        std::fs::write(&path, &output_text)
            .with_context(|| format!("Failed to write file: {}", path))?;
        renderer.viz_written(&path, output_text.len());
        Some(path)
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
