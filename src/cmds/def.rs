// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc def` — Go-to-definition for symbols (M6).
//!
//! ```bash
//! mcc def RES --lib mcode        # find component definition
//! mcc def main -F circuit.mc     # find module definition
//! ```

use anyhow::Result;
use mcc::cli::{rpc_client::RpcClient, DefArgs};
use mcc::{get_def, McCMIE, McIds, McURI};
use serde_json::json;

pub fn run(args: &DefArgs) -> Result<()> {
    if let Some(c) = RpcClient::probe() {
        let params = json!({ "name": args.name });
        match c.call("def", params) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            Err(e) => tracing::debug!(target: "mcc::def", "RPC failed, using local: {}", e),
        }
    }

    run_local(args)
}

fn run_local(args: &DefArgs) -> Result<()> {
    mcc::mcc_init_no_lib();
    if !args.lib.is_empty() {
        crate::cmds::manifest::load_libs(&args.lib);
    }
    if let Some(f) = &args.file {
        let uri = McURI::from(f.as_str());
        mcc::mcc_load_project(&uri);
    }

    let name = &args.name;
    let iterators: [(&str, Vec<(String, String)>); 4] = [
        ("component", mcc::mcb_iter_components()),
        ("module", mcc::mcb_iter_modules()),
        ("interface", mcc::mcb_iter_interfaces()),
        ("enum", mcc::mcb_iter_enums()),
    ];

    for (kind, items) in &iterators {
        if let Some((matched, uri)) = items.iter().find(|(n, _)| n == name) {
            let ident = McIds::from(matched.as_str());
            let uri_obj = McURI::from(uri.as_str());

            let detail = match get_def(&ident, &uri_obj) {
                Some(McCMIE::Component(c)) => json!({
                    "kind": "component",
                    "name": matched,
                    "uri": uri,
                    "pin_count": c.pins.pins.len(),
                }),
                Some(McCMIE::Module(m)) => json!({
                    "kind": "module",
                    "name": matched,
                    "uri": uri,
                    "instance_count": m.insts.iter().count(),
                }),
                Some(McCMIE::Interface(i)) => json!({
                    "kind": "interface",
                    "name": matched,
                    "uri": uri,
                    "pin_count": i.pins.pins.len(),
                }),
                Some(McCMIE::Enum(e)) => json!({
                    "kind": "enum",
                    "name": matched,
                    "uri": uri,
                    "value_count": e.values.len(),
                }),
                None => json!({
                    "kind": kind,
                    "name": matched,
                    "uri": uri,
                }),
            };

            println!("{}", serde_json::to_string_pretty(&detail)?);
            return Ok(());
        }
    }

    anyhow::bail!("definition not found: {name}");
}
