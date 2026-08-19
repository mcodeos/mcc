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
use serde_json::json;

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

    // Unified target loading: directory → project mode (manifest-driven,
    // browse fallback), file → loaded directly.
    if let Some(t) = &args.target {
        crate::cmds::common::load_target(
            Some(t),
            mcc::cli::globals().top.as_deref(),
            mcc::cli::globals().entry.as_deref(),
        )?;
    }

    let top = mcc::cli::globals()
        .top
        .clone()
        .or_else(mcc::mcb_get_first_module_name)
        .ok_or_else(|| anyhow::anyhow!("erc: no modules found — specify --top"))?;

    // Resolve the module's real URI (modules may live in a different file
    // than the entry), matching `show` / `nets_map`.
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top)
        .map(|(_, u)| u.clone())
        .unwrap_or_else(|| top.clone());

    let inst = crate::cmds::common::build_pass2(top.as_str(), &uri)
        .map_err(|e| anyhow::anyhow!("erc: {e}"))?;

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
        .iter()
        .flat_map(|(_, pts)| pts.iter())
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
