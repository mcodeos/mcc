// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! McVec visualization CLI entry
//!
//! ## Usage
//! ```bash
//! cargo run --bin mcviz <project_root> <module_name>           # -> circuit.html (new pipeline, real expand)
//! cargo run --bin mcviz <project_root> <module_name> -o out.html
//! cargo run --bin mcviz <project_root> <module_name> --json    # -> stdout JSON
//! cargo run --bin mcviz <project_root> <module_name> --legacy  # legacy pipeline (fake expand, for compare test)
//! ```
//!
//! ## P2 changes
//! - Added multi-layer pre-rendered `VizDocument`; submodule expand can actually swap SVG (no more alert)
//! - Added `--legacy` flag to preserve old path, for easier compare verification
//! - `--json` changed to output `VizDocument` JSON (including all layers)

use std::env;
use std::path::Path;
use std::process;

use mcc::vector::builder::{build_mc_vec, np_warn_count, reset_np_warn_count};
use mcc::vector::graph::build_mc_vec_graph;
use mcc::{
    mcc_build_flat, mcc_init, mcc_load_project, mcc_set_project_root, mcc_set_system_root, McIds,
};

// ─── New P2 pipeline ─────────────────────────────────────────────
use mcc::viz::api::{render_with, RenderOpts};
use mcc::viz::template::wrap_document;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        print_usage();
        process::exit(1);
    }

    let project_root = &args[1];
    let module_name = &args[2];

    // ── Parse optional arguments ──
    let mut output_file: Option<String> = None;
    let mut json_mode = false;
    let mut legacy_mode = false;
    let mut no_promote = false;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                if i + 1 < args.len() {
                    output_file = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: -o requires a file path argument");
                    process::exit(1);
                }
            }
            "--json" => {
                json_mode = true;
                i += 1;
            }
            "--legacy" => {
                legacy_mode = true;
                i += 1;
            }
            "--no-promote" => {
                no_promote = true;
                i += 1;
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            other => {
                eprintln!("Error: unknown option '{}'", other);
                process::exit(1);
            }
        }
    }

    // ── Project load + flatten ──
    let project_path = Path::new(project_root);
    if !project_path.exists() {
        eprintln!("Error: project root '{}' does not exist", project_root);
        process::exit(1);
    }

    // Standalone tool: need to set paths by itself
    // - System path: set if not set (only set once)
    // - Project path: must be set (visualization tool needs to know project location)
    mcc_set_system_root(project_path);
    mcc_set_project_root(project_path);
    mcc_init();

    let entry_uri = match find_entry_uri(project_path, module_name) {
        Some(uri) => uri,
        None => {
            eprintln!(
                "Error: could not find entry .mc file for module '{}' in '{}'",
                module_name, project_root
            );
            process::exit(1);
        }
    };

    mcc_load_project(&entry_uri);

    let ident = McIds::from(module_name.as_str());
    let (inst, table) = match mcc_build_flat(&ident, &entry_uri, 1000) {
        Ok((inst, table)) => (inst, table),
        Err(e) => {
            eprintln!("Error: mcc_build_flat failed: {}", e);
            process::exit(1);
        }
    };

    // ── McVecBlock + McVecGraph ──
    reset_np_warn_count();
    let vec_block = build_mc_vec(&inst, &table);
    let graph = build_mc_vec_graph(&vec_block, &table);

    // ── Output: three modes ──
    let output = if legacy_mode {
        eprintln!("[mcviz] using LEGACY pipeline (fake expand)");
        run_legacy(graph, json_mode)
    } else if json_mode {
        eprintln!("[mcviz] using NEW P2 pipeline -> VizDocument JSON");
        let opts = RenderOpts {
            apply_promote: !no_promote,
            ..Default::default()
        };
        let doc = render_with(graph, opts);
        doc.to_json()
    } else {
        eprintln!("[mcviz] using NEW P2 pipeline -> HTML (real expand)");
        let opts = RenderOpts {
            apply_promote: !no_promote,
            ..Default::default()
        };
        let doc = render_with(graph, opts);
        let layer_count = doc.layer_count();
        let svg_bytes = doc.total_svg_bytes();
        let html = wrap_document(&doc);
        eprintln!(
            "[mcviz] VizDocument: {} layers, {} bytes total SVG, HTML {} bytes",
            layer_count,
            svg_bytes,
            html.len()
        );
        html
    };

    // ── Write file / stdout ──
    match output_file.as_deref() {
        Some(path) => match std::fs::write(path, &output) {
            Ok(_) => eprintln!("[mcviz] wrote {} ({} bytes)", path, output.len()),
            Err(e) => {
                eprintln!("Error writing to {}: {}", path, e);
                process::exit(1);
            }
        },
        None => {
            println!("{}", output);
        }
    }

    // ── Summary ──
    let bus_warns = inst
        .all_diagnostics()
        .iter()
        .filter(|d| d.code == 921)
        .count();
    eprintln!(
        "[Viz Metrics] np_warns={} bus_warns={}",
        np_warn_count(),
        bus_warns,
    );
}

/// Legacy pipeline (preserved for compare verification)
fn run_legacy(graph: mcc::vector::graph::McVecGraph, json_mode: bool) -> String {
    if json_mode {
        graph.to_json_pretty()
    } else {
        mcc::viz::api::render_to_html(graph)
    }
}

fn print_usage() {
    eprintln!("Usage: mcviz <project_root> <module_name> [options]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  -o <file>      Output to file (default: stdout)");
    eprintln!("  --json         Output JSON instead of HTML");
    eprintln!("  --legacy       Use old pipeline (no real expand, for compare)");
    eprintln!("  --no-promote   Disable top-layer simplification (show all nets)");
    eprintln!("  -h, --help     Show this help");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  mcviz ./my_project Main -o circuit.html");
    eprintln!("  mcviz ./my_project Main --json > graph.json");
    eprintln!("  mcviz ./my_project Main --legacy -o circuit_old.html  # for comparison");
}

fn find_entry_uri(project_root: &Path, module_name: &str) -> Option<String> {
    let target_name = format!("{}.mc", module_name);
    if let Ok(entries) = std::fs::read_dir(project_root) {
        let mut first_mc: Option<String> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("mc") {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                if file_name.to_lowercase() == target_name.to_lowercase() {
                    return Some(path.to_string_lossy().to_string());
                }
                if first_mc.is_none() {
                    first_mc = Some(path.to_string_lossy().to_string());
                }
            }
        }
        return first_mc;
    }
    None
}
