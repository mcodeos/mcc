// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc report` — Structured design summary (M5b).

use crate::cmds::manifest;
use crate::output::{emit_projection, OutputFormatExt, ProjectionKey};
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ReportArgs};
use serde_json::{json, Value};
use std::path::Path;

pub fn run(args: &ReportArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        match c.call("report", json!({ "entry": args.target })) {
            Ok(result) => return emit_report(result),
            Err(e) => tracing::debug!(target: "mcc::report", "RPC failed: {}", e),
        }
    }
    run_local(args)
}

/// Structured face → A-tier envelope (U86 item 7, first slice); text / csv are
/// untouched.
///
/// `report` was **format-blind** before this slice: it printed pretty JSON in
/// *every* format, text mode included (mcd/log/9.18.cli-output-face-inventory.md
/// §2). That oddity is left as it is — this slice changes the machine face only.
fn emit_report(data: Value) -> Result<()> {
    if mcc::cli::globals().format.is_structured() {
        return emit_projection(
            ProjectionKey::Report,
            data,
            mcc::cli::globals().format,
            mcc::cli::globals().output.as_deref().map(Path::new),
        );
    }
    println!("{}", serde_json::to_string_pretty(&data)?);
    Ok(())
}

fn run_local(args: &ReportArgs) -> Result<()> {
    // An omitted target defaults to the current directory when it holds a
    // project manifest.
    let target = manifest::effective_target(args.target.as_deref());
    manifest::init_local(target.as_deref(), &mcc::cli::globals().lib);

    if let Some(t) = target.as_deref() {
        crate::cmds::common::load_target(
            Some(t),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        )?;
    }

    let comps = mcc::mcb_iter_components();
    let mods = mcc::mcb_iter_modules();
    let ifaces = mcc::mcb_iter_interfaces();
    let enums = mcc::mcb_iter_enums();

    // Aggregate components by prefix (R, C, L, D, X, etc.)
    let mut by_prefix: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for (name, _) in &comps {
        let prefix = name
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "?".into());
        *by_prefix.entry(prefix).or_default() += 1;
    }

    let result = json!({
        "command": "report",
        "summary": {
            "component_count": comps.len(),
            "module_count": mods.len(),
            "interface_count": ifaces.len(),
            "enum_count": enums.len(),
            "total_definitions": comps.len() + mods.len() + ifaces.len() + enums.len(),
        },
        "components_by_prefix": by_prefix,
        "components": comps.iter().take(20).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "modules": mods.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "interfaces": ifaces.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "enums": enums.iter().map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
    });

    emit_report(result)
}
