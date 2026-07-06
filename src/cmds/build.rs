// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc build` — one-shot build (manifest-driven)
//!
//! Read manifest → auto-load dependencies → Pass1 + Pass2 → envelope.
//!
//! ## Output funnel
//!
//! Iteration B: hand off the direct `eprintln!("[Pass 1] ...")` /
//! `eprintln!("[Pass 2] ...")` calls entirely to the `Renderer` trait. So:
//!   - JSON mode → `SilentRenderer`, stdout is clean and only emits the envelope
//!   - Text mode → `TextRenderer`, visually consistent with `mcc parse`
//!
//! ## Exit code
//!
//! `run` returns `(Result<()>, usize)`: success/failure + error count.
//! `dispatch` sets `exit_code` based on this (aligned with `check`).

use crate::cli::{rpc_client::RpcClient, BuildArgs, OutputFormat};
use crate::cmds::manifest;
use crate::cmds::proj::resolve_workspace_ref;
use crate::output::{
    self, builder::ResultBuilder, diagnostic::PhaseTracker, envelope::*, renderer, OutputFormatExt,
};
use anyhow::{Context, Result};
use mcc::viz::{
    layout::{
        FlowLayouter, HierarchicalLayouter, LayeredLayouter, RadialLayouter,
        SchematicRadialLayouter, SchematicSubLayouter,
    },
    traits::Layouter,
};
use mcc::McIds;
use serde_json::json;
use std::path::{Path, PathBuf};

/// Build command return result (includes exit_code; aligned with `check`)
pub struct BuildOutcome {
    pub exit_code: i32,
}

pub fn run(args: &BuildArgs) -> Result<BuildOutcome> {
    match RpcClient::probe() {
        Some(c) => run_rpc(&c, args),
        None => run_local(args),
    }
}

fn resolve_project_root(args: &BuildArgs) -> PathBuf {
    if let Some(ref entry) = args.entry {
        let entry_path = Path::new(entry);
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut search_dir = cwd.join(entry_path.parent().unwrap_or(entry_path));
        loop {
            if manifest::Manifest::find_in(&search_dir).is_some() {
                return search_dir;
            }
            match search_dir.parent() {
                Some(parent) => {
                    if parent == search_dir {
                        return cwd;
                    }
                    search_dir = parent.to_path_buf();
                }
                None => return cwd,
            }
        }
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn run_rpc(c: &RpcClient, args: &BuildArgs) -> Result<BuildOutcome> {
    let project_root = resolve_project_root(args);
    let manifest =
        manifest::Manifest::find_in(&project_root).and_then(|p| manifest::Manifest::load(&p).ok());
    let entry_abs = if let Some(ref e) = args.entry {
        project_root.join(e)
    } else if let Some(ref m) = manifest {
        m.entry_path(&project_root)
    } else {
        project_root.clone()
    };
    let libs: Vec<String> = manifest
        .as_ref()
        .map(|m| m.dependencies.keys().cloned().collect())
        .unwrap_or_default();

    let result = c.call(
        "build.full",
        json!({
            "entry": entry_abs.to_string_lossy(),
            "top": args.top,
            "libs": libs,
            "include_system": args.include_system,
        }),
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    // RPC mode conservatively returns 0 (the server side has its own logs)
    Ok(BuildOutcome { exit_code: 0 })
}

fn run_local(args: &BuildArgs) -> Result<BuildOutcome> {
    let renderer = renderer::for_format(args.format);
    let mut builder = ResultBuilder::start("mcc build").workspace(resolve_workspace_ref());
    let mut tracker = PhaseTracker::new();
    tracker.skip();

    // ── 0. Initialize system root (same as parse command) ──
    mcc::mcc_set_system_root(std::path::Path::new(""));
    manifest::load_libs(&args.lib);

    // ── 0.5. Pass 0 snapshot: lib load + manifest + project load phase diagnostics ──
    builder.set_pass0(crate::cmds::parse::public_collect_pass0());

    // ── 1. manifest parsing ──
    let project_root: PathBuf = resolve_project_root(args);
    tracing::debug!(target: "mcc::build", project_root = ?project_root, "resolved project root");

    let (entry_uri, top_name) = match manifest::build_from_manifest(
        &project_root,
        args.top.as_deref(),
        args.entry.as_deref(),
    ) {
        Ok(r) => r,
        Err(e) => {
            let err = RpcError::invalid_params(format!("{:#}", e));
            emit_err(&args.format, err)?;
            return Ok(BuildOutcome { exit_code: 1 });
        }
    };

    // ── 2. Pass1 ──
    renderer.pass1_header(&entry_uri);
    renderer.pass1_definitions(
        mcc::mcb_module_count(),
        mcc::mcb_component_count(),
        mcc::mcb_interface_count(),
    );
    builder.set_pass1(crate::cmds::parse::public_collect_pass1(
        &entry_uri,
        &mut tracker,
    ));

    // ── 3. Pass2 ──
    let ident = McIds::from(top_name.as_str());
    if mcc::get_def(&ident, &entry_uri).is_none() {
        let err = RpcError::invalid_params(format!("'{}' not found", top_name));
        emit_err(&args.format, err)?;
        return Ok(BuildOutcome { exit_code: 1 });
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Top Module Selection Strategy
    //
    // Strategy 1: Module with Top (Instantiated Hierarchy)
    //   - When a file contains modules with hierarchical instantiation (one module instantiates another),
    //     use the specified --top module as the entry point.
    //
    // Priority for Top Module Selection:
    //   1. CLI --top argument (highest priority): e.g., `mcc build --top MyModule`
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
    let modules_in_file = mcc::mcc_get_modules_in_file(&entry_uri);
    let should_render_all = modules_in_file.len() > 1 && args.top.is_none();

    let inst = if should_render_all {
        // For multi-module rendering, build each module separately for Pass 2 output
        // Process ALL modules in the loop, keeping the first one for further processing
        let first_mod_name = modules_in_file
            .first()
            .cloned()
            .unwrap_or_else(|| top_name.clone());
        let first_mod_ident = McIds::from(first_mod_name.as_str());

        match mcc::mcc_build(&first_mod_ident, &entry_uri) {
            Ok(first_inst) => {
                // Process ALL modules: first one already built, others in loop
                for (idx, mod_name) in modules_in_file.iter().enumerate() {
                    if idx == 0 {
                        // First module already built, output its details
                        renderer.pass2_header(mod_name);
                        renderer.instances(&first_inst, 0);
                        renderer.nets(&first_inst, 0);
                        renderer.net_summary(&first_inst);
                    } else {
                        // Build and output subsequent modules
                        let mod_ident = McIds::from(mod_name.as_str());
                        if let Ok(mod_inst) = mcc::mcc_build(&mod_ident, &entry_uri) {
                            renderer.pass2_header(mod_name);
                            renderer.instances(&mod_inst, 0);
                            renderer.nets(&mod_inst, 0);
                            renderer.net_summary(&mod_inst);
                        }
                    }
                }
                builder.set_pass2(crate::cmds::parse::public_collect_pass2(
                    &top_name,
                    &first_inst,
                    &mut tracker,
                ));
                first_inst
            }
            Err(e) => {
                renderer.pass2_failed(&format!("{}", e));
                let err = RpcError::build_error(format!("{}", e));
                emit_err(&args.format, err)?;
                return Ok(BuildOutcome { exit_code: 1 });
            }
        }
    } else {
        // Single module rendering: call pass2_header only once
        renderer.pass2_header(&top_name);
        // Single module rendering (original logic)
        match mcc::mcc_build(&ident, &entry_uri) {
            Ok(i) => {
                renderer.instances(&i, 0);
                renderer.nets(&i, 0);
                renderer.net_summary(&i);
                builder.set_pass2(crate::cmds::parse::public_collect_pass2(
                    &top_name,
                    &i,
                    &mut tracker,
                ));
                i
            }
            Err(e) => {
                renderer.pass2_failed(&format!("{}", e));
                let err = RpcError::build_error(format!("{}", e));
                emit_err(&args.format, err)?;
                return Ok(BuildOutcome { exit_code: 1 });
            }
        }
    };

    // ── 4. Viz generation ──
    if args.viz {
        // Reuse should_render_all and modules_in_file computed earlier

        if should_render_all {
            // Render all modules: build viz for each module and combine them
            let mut svgs: Vec<(String, String)> = Vec::new();
            let mut total_boxes = 0;
            let mut total_edges = 0;

            for mod_name in &modules_in_file {
                let mod_ident: McIds = McIds::from(mod_name.as_str());
                let mod_uri = entry_uri.clone();

                match mcc::mcc_build_flat(&mod_ident, &mod_uri, 1000) {
                    Ok((mod_inst, mod_table)) => {
                        mcc::vector::builder::reset_np_warn_count();
                        let (vec_block, _report) =
                            mcc::build_mc_vec_with_report(&mod_inst, &mod_table);
                        let graph = mcc::build_mc_vec_graph(&vec_block, &mod_table);

                        total_boxes += graph.boxes.len();
                        total_edges += graph.edges.len();

                        let opts = build_viz_opts(args.layouter.as_deref());
                        let doc = mcc::viz::api::render_with(graph, opts);

                        if let Some(root_layer) = doc.root_layer() {
                            svgs.push((mod_name.clone(), root_layer.svg.clone()));
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[viz] skip module '{}': mcc_build_flat failed: {}",
                            mod_name, e
                        );
                    }
                }
            }

            if svgs.is_empty() {
                return Err(anyhow::anyhow!("viz: no modules rendered"));
            }

            // Combine all SVGs into one big SVG, stacked vertically
            let combined_svg = combine_svgs(&svgs);

            // Build a single-layer VizDocument with the combined SVG
            let mut doc = mcc::viz::doc::VizDocument::new(1000, "all_modules".into());
            let mut layer = mcc::viz::layer::VizLayer::new(1000, "all_modules".into(), None);
            layer.svg = combined_svg;
            doc.add_layer(layer);

            let html = mcc::viz::template::wrap_document(&doc);

            let output_path = args.output.as_deref().unwrap_or("circuit.html");
            std::fs::write(output_path, &html)
                .with_context(|| format!("failed to write file: {}", output_path))?;
            renderer.viz_written(output_path, html.len());

            eprintln!(
                "[viz] rendered {} modules: {} boxes, {} edges",
                svgs.len(),
                total_boxes,
                total_edges
            );
        } else {
            // Single module render (explicit --top or only one module)
            let table = mcc::mcc_build_flat(&ident, &entry_uri, 1000)
                .map_err(|e| anyhow::anyhow!("mcc_build_flat failed: {}", e))?;

            // Pipeline diagnostics gated behind MC_VIZ_DUMP (silent by default).
            if mcc::viz::log::enabled() {
                let sub = inst.sub_modules.iter().find(|s| s.name == "mcu513");
                eprintln!(
                    "[CHK] inst-side mcu513.components = {}",
                    sub.map(|s| s.components.len()).unwrap_or(0)
                );
                eprintln!("[DUMP] ====== InstTable contents ======");
                table.1.dump();
                eprintln!("[DUMP] ==============================");
            }

            let (vec_block, build_report) = mcc::build_mc_vec_with_report(&inst, &table.1);
            let graph = mcc::build_mc_vec_graph(&vec_block, &table.1);

            let opts = build_viz_opts(args.layouter.as_deref());
            let (doc, metrics) = mcc::viz::api::render_with_metrics(graph, opts);
            let quality = metrics.finish_quality(Some(&build_report));
            // Metrics summary: always shown (this is the acceptance yardstick).
            for line in quality.report_lines() {
                eprintln!("{line}");
            }

            // [P0-DET] CLI golden guard: compare against baseline when MCC_GOLDEN_CHECK is set
            if std::env::var("MCC_GOLDEN_CHECK").is_ok() {
                let sig = doc.to_json();
                let gp = std::path::PathBuf::from("tests/golden/hbl.golden.json");
                if std::env::var("UPDATE_GOLDEN").is_ok() || !gp.exists() {
                    std::fs::create_dir_all(gp.parent().unwrap()).ok();
                    std::fs::write(&gp, &sig).ok();
                    eprintln!("[golden] wrote {}", gp.display());
                } else if sig != std::fs::read_to_string(&gp).unwrap_or_default() {
                    eprintln!(
                        "[golden] MISMATCH vs {} (UPDATE_GOLDEN=1 to refresh)",
                        gp.display()
                    );
                    return Ok(BuildOutcome { exit_code: 1 });
                }
            }

            let html = mcc::viz::template::wrap_document(&doc);

            let output_path = args.output.as_deref().unwrap_or("circuit.html");
            std::fs::write(output_path, &html)
                .with_context(|| format!("failed to write file: {}", output_path))?;
            renderer.viz_written(output_path, html.len());

            // [P0/A2] Electrical-fidelity hard gate: a non-perfect fidelity report means
            // the drawing is electrically wrong (dropped/partial nets, unrendered pins,
            // box/wire collisions). Fail the build so it can't pass silently.
            if !quality.is_perfect() {
                eprintln!(
                    "[gate] FIDELITY not perfect -> build failed. See report above. \
                     (set MCC_FIDELITY_GATE=0 to downgrade to warning)"
                );
                let gate_on = std::env::var("MCC_FIDELITY_GATE")
                    .map(|v| v.trim() != "0" && !v.eq_ignore_ascii_case("false"))
                    .unwrap_or(true);
                if gate_on {
                    return Ok(BuildOutcome { exit_code: 1 });
                }
            }
        }
    }

    // ── 5. Exit code: based on error count ──
    let errors = builder.error_count();

    // ── 6. Emit envelope ──
    let env = Envelope::ok(builder.finish());
    let envelope_target = if args.viz && args.output.is_some() {
        None
    } else {
        args.output.as_deref().map(Path::new)
    };
    output::emit_envelope(&env, args.format, envelope_target, false)?;
    Ok(BuildOutcome {
        exit_code: if errors > 0 { 1 } else { 0 },
    })
}

fn emit_err(fmt: &OutputFormat, err: RpcError) -> Result<()> {
    if fmt.is_structured() {
        output::emit_envelope(&Envelope::err(err), *fmt, None, false)?;
        Ok(())
    } else {
        Err(anyhow::anyhow!(err.message))
    }
}

fn build_viz_opts(layouter_name: Option<&str>) -> mcc::viz::api::RenderOpts {
    let mut opts = mcc::viz::api::RenderOpts::default();
    if let Some(name) = layouter_name {
        let (top, sub, top_cands, sub_cands): (
            Box<dyn Layouter>,
            Box<dyn Layouter>,
            Vec<Box<dyn Layouter>>,
            Vec<Box<dyn Layouter>>,
        ) = match name {
            "flow" => (
                Box::new(FlowLayouter::default()),
                Box::new(FlowLayouter::sub()),
                vec![Box::new(FlowLayouter::default())],
                vec![Box::new(FlowLayouter::sub())],
            ),
            "schematic_radial" => (
                Box::new(SchematicRadialLayouter::default()),
                Box::new(FlowLayouter::sub()),
                vec![Box::new(SchematicRadialLayouter::default())],
                vec![Box::new(FlowLayouter::sub())],
            ),
            "schematic_sub" => (
                Box::new(FlowLayouter::default()),
                Box::new(SchematicSubLayouter::default()),
                vec![Box::new(FlowLayouter::default())],
                vec![Box::new(SchematicSubLayouter::default())],
            ),
            "hierarchical" => (
                Box::new(HierarchicalLayouter::default()),
                Box::new(FlowLayouter::sub()),
                vec![Box::new(HierarchicalLayouter::default())],
                vec![Box::new(FlowLayouter::sub())],
            ),
            "radial" => (
                Box::new(RadialLayouter),
                Box::new(RadialLayouter),
                vec![Box::new(RadialLayouter)],
                vec![Box::new(RadialLayouter)],
            ),
            "layered" => (
                Box::new(LayeredLayouter::default()),
                Box::new(LayeredLayouter::sub()),
                vec![Box::new(LayeredLayouter::default())],
                vec![Box::new(LayeredLayouter::sub())],
            ),
            other => {
                eprintln!(
                    "[viz] unknown layouter '{}', using default. Choices: flow|schematic_radial|schematic_sub|hierarchical|radial|layered",
                    other
                );
                return opts;
            }
        };
        opts.top_layouter = top;
        opts.sub_layouter = sub;
        opts.top_candidates = top_cands;
        opts.sub_candidates = sub_cands;
        eprintln!("[viz] locked layouter: top={} sub={}", name, name);
    }
    opts
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper functions for multi-module SVG rendering (copied from parse.rs)
// ─────────────────────────────────────────────────────────────────────────────

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
        let vb = extract_viewbox_build(svg);
        let w = vb.0.max(1.0);
        let h = vb.1.max(1.0);
        max_w = max_w.max(w);

        // Extract inner content (everything between <svg ...> and </svg>)
        let inner = extract_svg_inner_build(svg);
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
            escape_xml_viz_build(name)
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
fn extract_viewbox_build(svg: &str) -> (f64, f64) {
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
fn extract_svg_inner_build(svg: &str) -> String {
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

fn escape_xml_viz_build(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod phase0_golden {
    use super::build_viz_opts;
    use crate::cmds::manifest;
    use mcc::McIds;
    use std::path::{Path, PathBuf};

    /// hbl fixture driven by env vars; skipped if unset (CI without fixture passes).
    ///   MCC_GOLDEN_PROJECT=<hbl project root>  [MCC_GOLDEN_ENTRY=<entry>] [MCC_GOLDEN_TOP=<top name>]
    fn hbl_project() -> Option<(PathBuf, Option<String>, Option<String>)> {
        let root = std::env::var("MCC_GOLDEN_PROJECT").ok()?;
        Some((
            PathBuf::from(root),
            std::env::var("MCC_GOLDEN_ENTRY").ok(),
            std::env::var("MCC_GOLDEN_TOP").ok(),
        ))
    }

    /// Replicate the real `--viz` sequence from build.rs, stopping before render.
    fn build_graph(
        root: &Path,
        entry: Option<&str>,
        top: Option<&str>,
    ) -> mcc::vector::graph::McVecGraph {
        mcc::mcc_init_no_lib();
        let (entry_uri, top_name) =
            manifest::build_from_manifest(root, top, entry).expect("build_from_manifest");
        let ident = McIds::from(top_name.as_str());
        let inst = mcc::mcc_build(&ident, &entry_uri).expect("mcc_build");
        let table = mcc::mcc_build_flat(&ident, &entry_uri, 1000).expect("mcc_build_flat");
        let vec_block = mcc::build_mc_vec(&inst, &table.1);
        mcc::build_mc_vec_graph(&vec_block, &table.1)
    }

    /// Fingerprint = VizDocument::to_json() (structure + per-layer SVG).
    fn render_signature(graph: mcc::vector::graph::McVecGraph) -> String {
        let opts = build_viz_opts(None); // default = FlowLayouter
        mcc::viz::api::render_with(graph, opts).to_json()
    }

    /// Core guard: same input rendered twice must produce byte-identical fingerprints.
    /// Isolates layout+route determinism; any HashMap-order leak (including in flow.rs)
    /// will surface here.
    #[test]
    fn determinism_render_twice() {
        let Some((root, entry, top)) = hbl_project() else {
            eprintln!("[phase0] set MCC_GOLDEN_PROJECT to enable; skipping");
            return;
        };
        let graph = build_graph(&root, entry.as_deref(), top.as_deref());
        let a = render_signature(graph.clone());
        let b = render_signature(graph);
        assert_eq!(
            a, b,
            "render_with nondeterministic on identical input graph"
        );
    }

    /// Secondary guard: two independent build+render cycles should also match
    /// (covers build-phase determinism).
    #[test]
    fn determinism_two_builds() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let a = render_signature(build_graph(&root, entry.as_deref(), top.as_deref()));
        let b = render_signature(build_graph(&root, entry.as_deref(), top.as_deref()));
        assert_eq!(
            a, b,
            "two independent builds differ (global-state or HashMap leak)"
        );
    }

    /// Golden regression: first run (or UPDATE_GOLDEN=1) writes baseline;
    /// subsequent runs compare byte-for-byte.
    #[test]
    fn golden_roundtrip_hbl() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let sig = render_signature(build_graph(&root, entry.as_deref(), top.as_deref()));
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hbl.golden.json");
        if std::env::var("UPDATE_GOLDEN").is_ok() || !path.exists() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &sig).unwrap();
            eprintln!("[golden] wrote baseline -> {}", path.display());
            return;
        }
        let golden = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            sig, golden,
            "hbl render changed vs golden. If intended: UPDATE_GOLDEN=1 cargo test golden_roundtrip_hbl"
        );
    }

    /// Smoke test: metrics accumulation on hbl produces sensible counts.
    #[test]
    fn metrics_hbl_smoke() {
        let Some((root, entry, top)) = hbl_project() else {
            return;
        };
        let graph = build_graph(&root, entry.as_deref(), top.as_deref());
        let (_, metrics) =
            mcc::viz::api::render_with_metrics(graph, mcc::viz::api::RenderOpts::default());
        let (fid, read) = metrics.finish(None); // self-consistent even without build report
        eprintln!("{}", fid.report_line());
        eprintln!("{}", read.report_line());
        assert!(fid.pins_rendered <= fid.pins_total);
        assert!(fid.nets_rendered <= fid.nets_total);
        assert!(read.total_wirelength >= 0.0 && read.weighted() >= 0.0);
    }

    /// Build hbl1 and produce a metrics snapshot for regression comparison.
    fn build_hbl1_metrics_snapshot() -> mcc::viz::metrics::SchematicMetricsSnapshot {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        manifest::load_libs(&vec!["mcode".into()]);

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let entry_rel = "projects/hbl1/hbl.mc";
        let (entry_uri, top_name) = manifest::build_from_manifest(&root, None, Some(entry_rel))
            .expect("build_from_manifest");

        let ident = McIds::from(top_name.as_str());
        let inst = mcc::mcc_build(&ident, &entry_uri).expect("mcc_build");
        let table = mcc::mcc_build_flat(&ident, &entry_uri, 1000).expect("mcc_build_flat");
        let (vec_block, build_report) = mcc::build_mc_vec_with_report(&inst, &table.1);
        let graph = mcc::build_mc_vec_graph(&vec_block, &table.1);

        let opts = build_viz_opts(None);
        let (_, metrics) = mcc::viz::api::render_with_metrics(graph, opts);
        let quality = metrics.finish_quality(Some(&build_report));

        let semantic = quality
            .semantic
            .as_ref()
            .map(|s| mcc::viz::metrics::SemanticSnapshot::from_summary(s))
            .unwrap_or_default();

        mcc::viz::metrics::SchematicMetricsSnapshot::from_quality(
            &quality,
            semantic,
            "hbl1",
            "projects/hbl1/hbl.mc",
            "cargo run -- build projects/hbl1/hbl.mc --lib mcode --viz",
        )
    }

    /// Golden regression: first run (or UPDATE_GOLDEN=1) writes baseline;
    /// subsequent runs compare using metrics snapshot rules.
    #[test]
    fn hbl1_metrics_golden_roundtrip() {
        if std::env::var("MCC_HBL1_METRICS").is_err() {
            eprintln!("skip hbl1 metrics golden; set MCC_HBL1_METRICS=1");
            return;
        }

        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/hbl1.metrics.json");

        std::thread::Builder::new()
            .name("hbl1-metrics".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let snapshot = build_hbl1_metrics_snapshot();

                if std::env::var("UPDATE_GOLDEN").is_ok() || !path.exists() {
                    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                    std::fs::write(&path, snapshot.to_json()).unwrap();
                    eprintln!("[metrics-golden] wrote baseline -> {}", path.display());
                    return;
                }

                let baseline = mcc::viz::metrics::SchematicMetricsSnapshot::from_json(
                    &std::fs::read_to_string(&path).unwrap(),
                )
                .expect("parse baseline");

                let report = mcc::viz::metrics::compare_metrics_snapshot(&snapshot, &baseline);
                assert!(report.passed, "{}", report.report_text());
            })
            .unwrap()
            .join()
            .unwrap();
    }

    /// Semantic smoke test: ensure hbl1 semantic analysis doesn't panic
    /// and produces non-zero counts.
    #[test]
    fn hbl1_semantic_smoke() {
        if std::env::var("MCC_HBL1_METRICS").is_err() {
            eprintln!("skip hbl1 semantic smoke; set MCC_HBL1_METRICS=1");
            return;
        }

        std::thread::Builder::new()
            .name("hbl1-semantic".into())
            .stack_size(32 * 1024 * 1024)
            .spawn(move || {
                let snapshot = build_hbl1_metrics_snapshot();
                let s = &snapshot.semantic;
                assert!(s.boxes_total > 0, "semantic boxes_total should be > 0");
                assert!(s.nets_total > 0, "semantic nets_total should be > 0");
                assert!(s.pins_total > 0, "semantic pins_total should be > 0");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

// ============================================================================
// D1–D8 detector tests
// ============================================================================
// Each test creates a small .mc fixture that triggers a specific detector,
// builds it, and asserts that the expected diagnostic code was emitted.
// Tests use a global lock because mcc global state (workspace) is not thread-safe.
#[cfg(test)]
mod d_detectors {
    use mcc::McDiagnostic;
    use mcc::McIds;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Helper: build a fixture string and return diagnostics produced.
    /// Returns (diagnostics, build_error) — build_error is Some(msg) if mcc_build failed.
    fn build_fixture(content: &str) -> (Vec<McDiagnostic>, Option<String>) {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        let uri = "/mcc/snippet.mc".to_string();
        mcc::mcc_clear_workspace();
        mcc::vector::builder::resolve::reset_np_warn_count();
        mcc::mcc_load_from_string(&uri, content);
        let ident = McIds::from("top");
        let build_result = mcc::mcc_build(&ident, &uri);
        let build_err = build_result.as_ref().err().map(|e| e.to_string());
        if build_result.is_ok() {
            let _ = mcc::mcc_build_flat(&ident, &uri, 1000);
        }
        let diags = mcc::mcc_diagnose_all();
        (diags, build_err)
    }

    /// Like build_fixture but panics on build failure.
    fn build_fixture_or_panic(content: &str) -> Vec<McDiagnostic> {
        let (diags, err) = build_fixture(content);
        if let Some(e) = err {
            panic!(
                "mcc_build failed: {e}. Diags: {:?}",
                diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
            );
        }
        diags
    }
    /// Helper: build fixture + vector graph, return diagnostics.
    fn build_fixture_with_graph(content: &str) -> Vec<McDiagnostic> {
        mcc::mcc_init_no_lib();
        mcc::mcc_set_system_root(std::path::Path::new(""));
        let uri = "/mcc/snippet.mc".to_string();
        mcc::mcc_clear_workspace();
        mcc::vector::builder::resolve::reset_np_warn_count();
        mcc::mcc_load_from_string(&uri, content);
        let ident = McIds::from("top");
        let inst = mcc::mcc_build(&ident, &uri).expect("mcc_build");
        let table = mcc::mcc_build_flat(&ident, &uri, 1000).expect("mcc_build_flat");
        let vec_block = mcc::build_mc_vec(&inst, &table.1);
        let _graph = mcc::build_mc_vec_graph(&vec_block, &table.1);
        mcc::mcc_diagnose_all()
    }

    fn has_code(diags: &[McDiagnostic], code: u32) -> bool {
        diags.iter().any(|d| d.code == code)
    }

    // ── D1 SORT_HAZARD ─────────────────────────────────────────────────

    #[test]
    fn d1_sort_hazard_non_monotonic_pins() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // D1 fires when bus pin numbers are non-monotonic.
        // [5,2] = BUS{CLK, DATA} → pin order differs from member order.
        let fixture = r#"
component MyChip {
    pins = [
        io [5,2] = BUS{CLK, DATA}
    ]
}
module top {
    io CLK, DATA
    MyChip chip
    chip{CLK, DATA} -> (CLK, DATA)
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            build_err.is_none(),
            "D1 build should succeed. Build err: {:?}",
            build_err
        );
        assert!(
            has_code(&diags, 2001),
            "D1 SORT_HAZARD should fire for non-monotonic pins [5,2]. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D2 FLOATING_PLACEHOLDER ─────────────────────────────────────────

    #[test]
    fn d2_floating_placeholder_unbound_lead() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
module top {
    _ -> _
}
"#;
        let diags = build_fixture_with_graph(fixture);
        assert!(
            has_code(&diags, 2002),
            "D2 FLOATING_PLACEHOLDER should fire for unbound '_'. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D3 MERGED_SHORT ─────────────────────────────────────────────────

    #[test]
    fn d3_merged_short_same_physical_pin() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // D3 fires when two points in the same net resolve to the same id.
        // The bracket expansion [A, A] creates two points both resolving to
        // the same port A, which is a merged short.
        let fixture = r#"
module top {
    io A
    [A, A] -> GND
}
"#;
        let diags = build_fixture_with_graph(fixture);
        assert!(
            has_code(&diags, 2003),
            "D3 MERGED_SHORT should fire for duplicate bracket entries. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D4 GHOST_PORT ───────────────────────────────────────────────────

    #[test]
    fn d4_ghost_port_placeholder_pin() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // D4 fires when a component has only an estimated pin count (pins = N)
        // without actual pin definitions. The builder synthesizes placeholder
        // pins (id ≥ 8e9) which are then detected as ghost ports.
        // Use "TEST_POINT" class name to trigger guess_chip_pin_count → 2,
        // so placeholder pins are created.
        let fixture = r#"
component TEST_POINT {
    pins = 4
}
module top {
    io A
    TEST_POINT chip
    chip.1 -> A
}
"#;
        let diags = build_fixture_with_graph(fixture);
        assert!(
            has_code(&diags, 2004),
            "D4 GHOST_PORT should fire for placeholder pins. Diags: {:?}",
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D5 BUS_ORDER_MISMATCH ───────────────────────────────────────────

    #[test]
    fn d5_bus_order_mismatch_all_pairs() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
component MyChip {
    pins = [
        io [1,2] = PORT_A{A, B}
        io [1,2] = PORT_B{X, Y}
    ]
}
module top {
    MyChip chip
    chip{PORT_A} -> chip{PORT_B}
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        let mismatched = mcc::mcc_bus_bits_mismatched();
        assert!(
            build_err.is_none(),
            "D5 build should succeed. Build err: {:?}",
            build_err
        );
        assert!(
            has_code(&diags, 2005) || mismatched > 0,
            "D5 BUS_ORDER_MISMATCH should fire for A↔X, B↔Y. mismatched={} diags: {:?}",
            mismatched,
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D6 DROPPED_STATEMENT ────────────────────────────────────────────

    #[test]
    fn d6_dropped_statement_indexed_alias() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // D6 fires when a single-element square bracket expands to an
        // unknown name that is not a known instance.
        let fixture = r#"
module top {
    io A
    [Unknown] -> A
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            has_code(&diags, 2006),
            "D6 DROPPED_STATEMENT should fire for indexed alias. Build err: {:?}. Diags: {:?}",
            build_err,
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D7 PULLUP_DEGENERATE ────────────────────────────────────────────

    #[test]
    fn d7_pullup_degenerate_signal_bridge() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
module top {
    io SCL, SDA
    RES(10k).Pullup(SCL, SDA)
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            has_code(&diags, 2007),
            "D7 PULLUP_DEGENERATE should fire for signal-signal bridge. Build err: {:?}. Diags: {:?}",
            build_err,
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }

    // ── D8 AMBIGUOUS_PRECEDENCE ─────────────────────────────────────────

    #[test]
    fn d8_ambiguous_precedence_mixed_ops() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fixture = r#"
module top {
    io A, B, C, D
    A -> B + C -> D
}
"#;
        let (diags, build_err) = build_fixture(fixture);
        assert!(
            has_code(&diags, 2008),
            "D8 AMBIGUOUS_PRECEDENCE should fire for mixed operators. Build err: {:?}. Diags: {:?}",
            build_err,
            diags.iter().map(|d| (d.code, &d.msg)).collect::<Vec<_>>()
        );
    }
}
