// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc erc` — Electrical Rule Check (M6).
//!
//! Checks: single-point nets, unconnected ports, multi-drive nets.
//! Requires Pass2 (instantiation) to build the netlist.

use crate::cmds::manifest;
use anyhow::Result;
use mcc::cli::{rpcclient::RpcClient, ErcArgs};
use mcc::McURI;
use serde_json::json;
use std::path::{Path, PathBuf};

pub fn run(args: &ErcArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = json!({ "top": mcc::cli::globals().top });
        match c.call("erc", params) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            Err(e) => tracing::debug!(target: "mcc::erc", "RPC failed, using local: {}", e),
        }
    }

    run_local(args)
}

fn run_local(args: &ErcArgs) -> Result<()> {
    manifest::init_local(args.target.as_deref(), &mcc::cli::globals().lib);

    if let Some(t) = &args.target {
        let p = Path::new(t);
        if p.is_dir() {
            // Project mode: manifest-driven; browse fallback (§19.5 rule 3 of
            // use-design.md) when the directory has no manifest.
            match manifest::build_from_manifest(p, None, mcc::cli::globals().entry.as_deref()) {
                Ok(_) => {}
                Err(manifest_err) => {
                    let entry =
                        manifest::select_browse_entry(p, mcc::cli::globals().entry.as_deref())
                            .map_err(|browse_err| {
                                anyhow::anyhow!("{} (manifest: {:#})", browse_err, manifest_err)
                            })?;
                    let entry_uri = entry.to_string_lossy().to_string();
                    mcc::mcc_load_project(&entry_uri);
                }
            }
        } else {
            let path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .join(p)
            };
            let uri = McURI::from(path.to_string_lossy().as_ref());
            mcc::mcc_load_project(&uri);
        }
    }

    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .ok_or_else(|| anyhow::anyhow!("erc: no modules found — specify --top"))?;

    let uri = mcc::McURI::from(top.as_str());
    let ident = mcc::McIds::from(top.as_str());

    let inst = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build(&ident, &uri)
    }))
    .map_err(|_| anyhow::anyhow!("erc: build panicked"))?
    .map_err(|e| anyhow::anyhow!("erc: build failed: {e}"))?;

    let mut diags: Vec<serde_json::Value> = Vec::new();

    // ── Single-point nets ──
    for (name, points) in &inst.nets {
        if name.starts_with("__net_") || name == "NC" {
            continue;
        }
        if points.len() <= 1 {
            let code = mcc::errcodes::ERC_SINGLE_POINT_NET;
            let msg = mcc::errcodes::format_msg(code, &[&name]);
            diags.push(json!({
                "code": code,
                "severity": "warning",
                "check": "single_point_net",
                "message": msg,
            }));
        }
    }

    // ── Unconnected ports ──
    let all_paths: std::collections::HashSet<&str> = inst
        .nets
        .values()
        .flat_map(|pts| pts.iter())
        .map(|p| p.path.as_str())
        .collect();

    for port in &inst.ports {
        if !all_paths.contains(port.name.as_str()) {
            let code = mcc::errcodes::ERC_UNCONNECTED_PORT;
            let msg = mcc::errcodes::format_msg(code, &[&port.name]);
            diags.push(json!({
                "code": code,
                "severity": "warning",
                "check": "unconnected_port",
                "message": msg,
            }));
        }
    }

    // ── Multi-drive / floating net detection ──
    let mut multi_drive = 0u32;
    let mut floating = 0u32;

    for (name, points) in &inst.nets {
        if name.starts_with("__net_") || name.as_str() == "NC" {
            continue;
        }
        let drivers: Vec<_> = points
            .iter()
            .filter(|p| {
                matches!(
                    p.iotype,
                    mcc::IOType::Out
                        | mcc::IOType::InOut
                        | mcc::IOType::Power
                        | mcc::IOType::Analog
                )
            })
            .collect();

        if drivers.len() > 1 {
            multi_drive += 1;
            let names: Vec<_> = drivers.iter().map(|d| d.path.as_str()).collect();
            let code = mcc::errcodes::ERC_MULTI_DRIVE_NET;
            let args: &[&dyn std::fmt::Display] = &[&name, &drivers.len(), &names.join(", ")];
            let msg = mcc::errcodes::format_msg(code, args);
            diags.push(json!({
                "code": code,
                "severity": "error",
                "check": "multi_drive",
                "message": msg,
            }));
        } else if drivers.is_empty() && points.len() > 1 {
            floating += 1;
            let code = mcc::errcodes::ERC_FLOATING_NET;
            let msg = mcc::errcodes::format_msg(code, &[&name]);
            diags.push(json!({
                "code": code,
                "severity": "warning",
                "check": "floating_net",
                "message": msg,
            }));
        }
    }

    let result = json!({
        "command": "erc",
        "top": top,
        "summary": {
            "net_count": inst.nets.len(),
            "connection_count": inst.connections.len(),
            "component_count": inst.components.len(),
            "port_count": inst.ports.len(),
            "violations": diags.len(),
            "single_point_nets": diags.iter().filter(|d| d["check"] == "single_point_net").count(),
            "unconnected_ports": diags.iter().filter(|d| d["check"] == "unconnected_port").count(),
            "multi_drive_nets": multi_drive,
            "floating_nets": floating,
        },
        "violations": diags,
    });

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
