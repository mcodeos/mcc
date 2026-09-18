// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc impact <SYM> [FILE]` -- the blast radius of changing one def
//! (world-repartition-design.md §6.1).
//!
//! The engine is `impact_report` in the library crate, shared verbatim with the
//! `impact` RPC method, so the CLI and MCP faces cannot drift.

use crate::cmds::{common, manifest};
use crate::output::die;
use anyhow::Result;
use mcc::cli::{ImpactArgs, OutputFormat};
use mcc::rpc::handlers::{impact_report, ImpactError};
use serde_json::Value;

/// Exits `1` when `sym` names nothing, `2` when the world cannot be built: the
/// two are different answers and the caller acts on them differently.
pub fn run(args: &ImpactArgs) -> Result<()> {
    let Some(target) = manifest::effective_target(args.file.as_deref()) else {
        anyhow::bail!("impact: <file> not specified and no project manifest here");
    };
    // The standard components are defs like any other, so the world needs the
    // libraries loaded -- `diff` and `join` initialize the same way.
    manifest::init_local(Some(&target), &mcc::cli::globals().lib);
    let (entry_uri, _) = common::load_target(
        Some(&target),
        mcc::cli::globals().top.as_deref(),
        mcc::cli::globals().entry.as_deref(),
    )?;

    let report = match impact_report(
        &args.sym,
        &entry_uri,
        mcc::cli::globals().top.as_deref(),
        &mcc::cli::globals().lib,
    ) {
        Ok(report) => report,
        Err(ImpactError::Unresolved(m)) => die!("mcc::impact", 1, "{}", m),
        Err(ImpactError::Build(m)) => die!("mcc::impact", 2, "{}", m),
    };

    let format = if args.json {
        OutputFormat::Json
    } else {
        mcc::cli::globals().format
    };
    if matches!(format, OutputFormat::Text | OutputFormat::Csv) {
        return write_report(&render_text(&report));
    }
    // The report's shape is its own, not a projection (design §6.1), so it goes
    // out verbatim rather than through the command envelope. `schema_version`
    // is what makes that shape versionable.
    let buf = match format {
        OutputFormat::JsonPretty => serde_json::to_string_pretty(&report)?,
        OutputFormat::Yaml => serde_yaml::to_string(&report)?,
        _ => serde_json::to_string(&report)?,
    };
    write_report(&buf)
}

/// Write the report to `--output` or stdout.
fn write_report(buf: &str) -> Result<()> {
    match &mcc::cli::globals().output {
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
    let impacted = r.get("impacted");
    let circuits: Vec<(String, String)> = impacted
        .and_then(|i| i.get("circuits"))
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .map(|c| (cell(c.get("top")), cell(c.get("entry"))))
                .collect()
        })
        .unwrap_or_default();
    let nets: Vec<String> = impacted
        .and_then(|i| i.get("nets"))
        .and_then(|n| n.as_array())
        .map(|a| a.iter().map(|v| cell(Some(v))).collect())
        .unwrap_or_default();

    let mut out = format!("impact {}\n", cell(r.get("sym")));
    out.push_str(&format!("  world_ver  {}\n", cell(r.get("world_ver"))));
    out.push_str(&format!("  uri        {}\n", cell(r.get("uri"))));
    out.push_str(&format!("  defId      {}\n", cell(r.get("defId"))));
    out.push_str(&format!("  circuits   {}\n", circuits.len()));
    for (top, entry) in &circuits {
        out.push_str(&format!("    {}  {}\n", top, entry));
    }
    out.push_str(&format!("  nets       {}\n", nets.len()));
    for name in &nets {
        out.push_str(&format!("    {}\n", name));
    }
    out.push_str(&format!(
        "  consumers  {}\n",
        cell(impacted.and_then(|i| i.get("consumers")))
    ));
    out.trim_end().to_string()
}
