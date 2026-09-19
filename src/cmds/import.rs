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
//! is written. `--fragment` (an overlay-gated `.mc` fragment) is a later batch.

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
