// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc import <FILE> [TARGET]` -- read an EDA artifact back and report how it
//! differs from the current world (world-repartition-design.md §6.2).
//!
//! The engine is `import_report` in the library crate, shared verbatim with the
//! `import` RPC method, so the CLI and MCP faces cannot drift.
//!
//! The default half only: a read-back is a convergence *suggestion*, so nothing
//! is written. `--skeleton` (an `mct.netlist/1` JSON in, compilable project
//! out) is the other half: it writes the generated project under `--out-dir`
//! and skips the read-back comparison entirely.

use crate::cmds::{common, manifest};
use crate::output::die;
use anyhow::Result;
use mcc::cli::{globals, ImportArgs, OutputFormat};
use mcc::rpc::handlers::{import_report, ImportError};
use serde_json::Value;
use std::process::ExitCode;

/// Exits `2` when the artifact or the world cannot be constructed, `1` when the
/// artifact differs from the world or the world build is not diagnostic-clean,
/// `0` when the two agree.
pub fn run(args: &ImportArgs) -> Result<ExitCode> {
    if args.skeleton {
        return run_skeleton(args);
    }
    if matches!(args.from, mcc::cli::ImportFormat::MctJson) {
        anyhow::bail!(
            "import: --from mctjson requires --skeleton (read-back comparison has no mct-side export reference)",
        );
    }
    let Some(target) = manifest::effective_target(args.target.as_deref()) else {
        anyhow::bail!("import: <target> not specified and no project manifest here");
    };
    // The standard components are defs like any other, so the world needs the
    // libraries loaded -- `export` and `impact` initialize the same way.
    manifest::init_local(Some(&target), &globals().lib);
    let (entry_uri, _) = common::load_target(
        Some(&target),
        globals().top.as_deref(),
        globals().entry.as_deref(),
    )?;

    let report = match import_report(
        &args.file,
        args.from,
        &entry_uri,
        globals().top.as_deref(),
        &globals().lib,
    ) {
        Ok(report) => report,
        Err(ImportError::Input(m)) => die!("mcc::import", 2, "{}", m),
        Err(ImportError::Build(m)) => die!("mcc::import", 2, "{}", m),
    };

    let format = if args.json {
        OutputFormat::Json
    } else {
        globals().format
    };
    if matches!(format, OutputFormat::Text | OutputFormat::Csv) {
        write_report(&render_text(&report))?;
    } else {
        // The report's shape is its own, not a projection (design §6.2), so it
        // goes out verbatim rather than through the command envelope.
        // `schema_version` is what makes that shape versionable.
        let buf = match format {
            OutputFormat::JsonPretty => serde_json::to_string_pretty(&report)?,
            OutputFormat::Yaml => serde_yaml::to_string(&report)?,
            _ => serde_json::to_string(&report)?,
        };
        write_report(&buf)?;
    }

    let count = report.get("count").and_then(Value::as_u64).unwrap_or(0);
    let diags = report
        .get("diagnostics")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    Ok(if count > 0 || diags > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

/// Write the report to `--output` or stdout.
fn write_report(buf: &str) -> Result<()> {
    match &globals().output {
        Some(path) => std::fs::write(path, format!("{buf}\n"))?,
        None => println!("{buf}"),
    }
    Ok(())
}

// === --skeleton: mct.netlist/1 -> project files ===

/// Generate a compilable project skeleton from an `mct.netlist/1` JSON file.
/// Exits `2` on unreadable/invalid input or a failed self-check, `0` when the
/// skeleton compiled clean. The summary line mirrors `import`'s report shape.
fn run_skeleton(args: &ImportArgs) -> Result<ExitCode> {
    if !matches!(args.from, mcc::cli::ImportFormat::MctJson) {
        anyhow::bail!(
            "import: --skeleton requires --from mctjson (the skeleton consumes only the mct corpus-tool contract JSON)",
        );
    }
    if args.target.is_some() {
        anyhow::bail!("import: --skeleton generates a new project and does not combine with the <target> read-back comparison");
    }
    let text = std::fs::read_to_string(&args.file)
        .map_err(|e| anyhow::anyhow!("import: cannot read '{}': {e}", args.file))?;
    let plan = mcc::import_skeleton::plan(&text, &mcc::import_skeleton::Opts { name: args.name.as_deref() })
        .map_err(|m| anyhow::anyhow!("{m}"))?;

    let name = plan
        .report
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("skeleton")
        .to_string();
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| format!("./{name}"));
    std::fs::create_dir_all(&out_dir)?;
    let mut written = Vec::new();
    for (path, content) in &plan.files {
        let full = std::path::Path::new(&out_dir).join(path);
        std::fs::write(&full, content)?;
        written.push(full.display().to_string());
    }

    // Text face: one summary block (same fields the report carries).
    println!(
        "import --skeleton {} -> {}",
        args.file,
        out_dir
    );
    let cell = |k: &str| {
        plan.report
            .get(k)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| "-".into())
    };
    println!(
        "  components {} (inline {} / block {}) | nets {} | anchors {} | dangle {} | todos {}",
        cell("components"),
        cell("inline"),
        cell("blocks"),
        cell("nets"),
        cell("anchors"),
        cell("dangle"),
        cell("todos")
    );
    println!("  selfcheck: 0 errors ({} warnings)", cell("selfcheck_warnings"));
    for w in &written {
        println!("  written: {w}");
    }
    Ok(ExitCode::SUCCESS)
}

/// A value as it reads in a column: strings bare, everything else as JSON, and
/// a missing field as `-` (schema §5.3: absence is not zero).
fn cell(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => "-".to_string(),
    }
}

fn render_text(r: &Value) -> String {
    let mut out = format!("import {} {}\n", cell(r.get("format")), cell(r.get("uri")));
    out.push_str(&format!("  world_ver    {}\n", cell(r.get("world_ver"))));
    out.push_str(&format!("  top          {}\n", cell(r.get("top"))));
    out.push_str(&format!("  changes      {}\n", cell(r.get("count"))));
    if let Some(changes) = r.get("changes").and_then(Value::as_array) {
        for c in changes {
            out.push_str(&format!(
                "    {} {} {} {}\n",
                cell(c.get("type")),
                cell(c.get("kind")),
                cell(c.get("id")),
                cell(c.get("delta"))
            ));
        }
    }
    out.push_str(&format!("  diagnostics  {}\n", cell(r.get("diagnostics"))));
    out.trim_end().to_string()
}
