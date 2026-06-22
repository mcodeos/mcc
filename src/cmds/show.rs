// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc show` — Show detailed information for specified component/module/interface/net

use crate::cli::{rpc_client::RpcClient, OutputFormat, ShowArgs, ShowTarget};
use anyhow::Result;
use serde_json::json;
use tracing::error;

pub fn run(args: &ShowArgs) -> Result<()> {
    let client = RpcClient::probe();

    if let Some(c) = &client {
        let (method, params) = if args.name.is_none() {
            // List all
            let method = match args.target {
                ShowTarget::Component => "show.component.list",
                ShowTarget::Module => "show.module.list",
                ShowTarget::Interface => "show.interface.list",
                ShowTarget::Net => "show.net.list",
                ShowTarget::File => "show.file",
            };
            (method, json!({ "file": args.file }))
        } else {
            // Show single
            let method = match args.target {
                ShowTarget::Component => "show.component",
                ShowTarget::Module => "show.module",
                ShowTarget::Interface => "show.interface",
                ShowTarget::Net => "show.net",
                ShowTarget::File => "show.file",
            };
            (method, json!({ "name": args.name, "file": args.file }))
        };

        match c.call(method, params) {
            Ok(result) => {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }
            Err(e) => {
                // RPC call failed (e.g., method doesn't exist), fallback to local mode
                tracing::debug!(target: "mcc::show", "RPC failed, using local mode: {}", e);
            }
        }
    }

    run_local(args)
}

fn run_local(args: &ShowArgs) -> Result<()> {
    // First initialize (don't auto-load system library)
    mcc::mcc_init_no_lib();

    // If file is specified, load it (all targets need it)
    if let Some(file) = &args.file {
        let uri = mcc::McURI::from(file.as_str());
        mcc::mcc_load_project(&uri);
    }

    // List all when name is None
    if args.name.is_none() {
        return list_all(args);
    }

    match args.target {
        ShowTarget::Component => show_component(args.name.as_ref().unwrap(), args),
        ShowTarget::Module => show_module(args.name.as_ref().unwrap(), args),
        ShowTarget::Interface => show_interface(args.name.as_ref().unwrap(), args),
        ShowTarget::Net => show_net(args.name.as_deref().unwrap_or(""), args),
        ShowTarget::File => show_file(args),
    }
}

/// Search for files with the same name, recursively in common directories
fn find_files_with_name(name: &str) -> Vec<String> {
    use std::fs;

    // Extract file name (remove directory part)
    let file_name = std::path::Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(name);

    let mut matches = Vec::new();

    // Recursively search directory
    fn search_dir(dir: &std::path::Path, file_name: &str, matches: &mut Vec<String>, depth: usize) {
        if depth > 5 {
            return; // Limit recursion depth
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip hidden directories and common build directories
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        if !fname.starts_with('.') && fname != "target" && fname != "node_modules" {
                            search_dir(&path, file_name, matches, depth + 1);
                        }
                    }
                } else if path.is_file() {
                    if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                        if fname == file_name && fname.ends_with(".mc") {
                            if let Ok(canonical) = path.canonicalize() {
                                if let Some(p) = canonical.to_str() {
                                    matches.push(p.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Start search from current directory
    search_dir(std::path::Path::new("."), file_name, &mut matches, 0);

    matches
}

fn show_file(args: &ShowArgs) -> Result<()> {
    // For File target, name is the file path
    let file = args.name.as_ref().expect("show file requires file name");

    // Check if file exists, if not try to search
    let actual_path = if std::path::Path::new(file).exists() {
        file.to_string()
    } else {
        // Try to search for files with same name
        let matches = find_files_with_name(file);
        if matches.is_empty() {
            error!(target: "mcc::show", "file not found: {}", file);
            std::process::exit(1);
        } else if matches.len() == 1 {
            // Only one match, use directly
            matches[0].clone()
        } else {
            // Multiple matches, list for user to choose
            let list: Vec<String> = matches
                .iter()
                .enumerate()
                .map(|(i, p)| format!("  {}: {}", i + 1, p))
                .collect();
            error!(target: "mcc::show", "multiple files named '{}':\n{}", file, list.join("\n"));
            std::process::exit(1);
        }
    };

    // Initialize (don't auto-load system library)
    mcc::mcc_init_no_lib();

    // Load file
    let uri = mcc::McURI::from(actual_path.as_str());
    mcc::mcc_load_project(&uri);

    // Get all definitions
    let components: Vec<(String, String)> = mcc::mcb_iter_components();
    let modules: Vec<(String, String)> = mcc::mcb_iter_modules();
    let interfaces: Vec<(String, String)> = mcc::mcb_iter_interfaces();

    // Human-readable format: flat structure
    let data = json!({
        "type": "file",
        "file": actual_path,
        "module_count": modules.len(),
        "module_list": modules.iter().map(|(n, _)| n).cloned().collect::<Vec<_>>(),
        "component_count": components.len(),
        "component_list": components.iter().map(|(n, _)| n).cloned().collect::<Vec<_>>(),
        "interface_count": interfaces.len(),
        "interface_list": interfaces.iter().map(|(n, _)| n).cloned().collect::<Vec<_>>(),
    });
    output_json(&data, args.format)?;
    Ok(())
}

fn list_all(args: &ShowArgs) -> Result<()> {
    // Initialize (don't auto-load system library)
    mcc::mcc_init_no_lib();

    // If file is specified, first find the file then load
    if let Some(file) = &args.file {
        let actual_path = if std::path::Path::new(file).exists() {
            file.clone()
        } else {
            let matches = find_files_with_name(file);
            if matches.is_empty() {
                error!(target: "mcc::show", "file not found: {}", file);
                return Ok(());
            } else if matches.len() == 1 {
                matches[0].clone()
            } else {
                let list: Vec<String> = matches
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("  {}: {}", i + 1, p))
                    .collect();
                error!(target: "mcc::show", "multiple files named '{}':\n{}", file, list.join("\n"));
                return Ok(());
            }
        };
        let uri = mcc::McURI::from(actual_path.as_str());
        mcc::mcc_load_project(&uri);
    }

    match args.target {
        ShowTarget::Component => {
            let comps: Vec<(String, String)> = mcc::mcb_iter_components();
            let names: Vec<String> = comps.iter().map(|(n, _)| n.clone()).collect();
            let data = json!({
                "type": "component",
                "count": names.len(),
                "list": names,
            });
            output_json(&data, args.format);
        }
        ShowTarget::Module => {
            let modules: Vec<(String, String)> = mcc::mcb_iter_modules();
            let names: Vec<String> = modules.iter().map(|(n, _)| n.clone()).collect();
            let data = json!({
                "type": "module",
                "count": names.len(),
                "list": names,
            });
            output_json(&data, args.format);
        }
        ShowTarget::Interface => {
            let ifaces: Vec<(String, String)> = mcc::mcb_iter_interfaces();
            let names: Vec<String> = ifaces.iter().map(|(n, _)| n.clone()).collect();
            let data = json!({
                "type": "interface",
                "count": names.len(),
                "list": names,
            });
            output_json(&data, args.format);
        }
        ShowTarget::Net => {
            // File already loaded in run_local; build netlist: prefer --top, otherwise get first module
            let top_name = args
                .top
                .as_ref()
                .cloned()
                .or_else(|| mcc::mcb_get_first_module_name());

            // Find URI corresponding to top_name from loaded modules (mcc_build needs correct URI)
            let uri: Option<mcc::McURI> = top_name.as_ref().and_then(|name: &String| {
                mcc::mcb_iter_modules()
                    .iter()
                    .find(|(n, _)| *n == *name)
                    .map(|(_, u)| mcc::McURI::from(u.as_str()))
            });

            let built = top_name.as_ref().and_then(|name: &String| {
                let ident = mcc::McIds::from(name.as_str());
                let uri = uri.clone()?;
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    mcc::mcc_build(&ident, &uri)
                }))
                .ok() // panic → None
                .and_then(|r| r.ok()) // Err → None
            });

            let items: Vec<serde_json::Value> = if let Some(inst) = built {
                use std::collections::BTreeMap;
                let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for conn in &inst.connections {
                    let net = conn
                        .net_name
                        .clone()
                        .unwrap_or_else(|| format!("__net_{}", conn.id));
                    if net == "NC" {
                        continue;
                    }
                    let bucket = nets.entry(net).or_default();
                    for p in &conn.points {
                        if p.path == "NC" {
                            continue;
                        }
                        let label = if let Some(ref o) = p.owner {
                            format!("{}.{}", o, p.path.split('.').last().unwrap_or(&p.path))
                        } else {
                            p.path.clone()
                        };
                        if !bucket.contains(&label) {
                            bucket.push(label);
                        }
                    }
                }
                nets.into_iter()
                    .map(|(n, pts)| json!({ "name": n, "points": pts }))
                    .collect()
            } else {
                vec![]
            };
            let data = json!({
                "type": "net",
                "count": items.len(),
                "nets": items,
            });
            output_json(&data, args.format);
        }
        ShowTarget::File => {
            // File type is used via show file <path>
            let data = json!({
                "type": "file",
                "note": "Use 'mcc show file <path>' to view all elements in file",
            });
            output_json(&data, args.format);
        }
    }
    Ok(())
}

fn show_component(name: &str, args: &ShowArgs) -> Result<()> {
    // Initialize (don't auto-load system library)
    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));

    // Load libraries specified by --lib, so interface bindings can be resolved
    if !args.lib.is_empty() {
        crate::cmds::manifest::load_libs(&args.lib);
    }

    // If file is specified, load it
    if let Some(file) = &args.file {
        let uri = mcc::McURI::from(file.as_str());
        mcc::mcc_load_project(&uri);
    }

    // Only search from loaded components
    let comps: Vec<(String, String)> = mcc::mcb_iter_components();

    // Exact match
    let found = comps.iter().find(|(n, _)| n == name);

    let (matched_name, uri) = match found {
        Some((n, u)) => (n.clone(), u.clone()),
        None => {
            if args.file.is_some() {
                error!(target: "mcc::show", "component not found: {} in file '{}'", name, args.file.as_ref().unwrap());
            } else {
                error!(target: "mcc::show", "component not found: {}\nhint: load library first (mcc lib load <name>) or use --file", name);
            }
            std::process::exit(1);
        }
    };

    let cmie = match mcc::get_def(
        &mcc::McIds::from(matched_name.as_str()),
        &mcc::McURI::from(uri.as_str()),
    ) {
        Some(c) => c,
        None => {
            error!(target: "mcc::show", "component not found: {}", name);
            std::process::exit(1);
        }
    };

    let mcc::McCMIE::Component(comp) = cmie else {
        error!(target: "mcc::show", "'{}' is not a Component", name);
        std::process::exit(1);
    };

    // Build detailed pin info: iterate all defined pins
    let pins: Vec<serde_json::Value> = comp
        .pins
        .pins
        .iter()
        .map(|(pin_id, pin)| {
            // Try to extract description from values
            let mut desc = String::new();
            for val in pin.values.iter() {
                if let mcc::McAttrVal::AttrLiteral(mcc::McLiteral::String(s)) = val {
                    if !desc.is_empty() {
                        desc.push(' ');
                    }
                    desc.push_str(&s.value);
                }
            }

            let mut pin_json = json!({
                "id": pin_id,
                "iotype": format!("{:?}", pin.iotype),
                "names": pin.names,
            });
            if !desc.is_empty() {
                pin_json["description"] = json!(desc);
            }
            pin_json
        })
        .collect();

    // names_to_id: structurize output of each McPinPort variant, let user directly see pin↔interface binding
    let mut names_to_id = serde_json::Map::new();
    for (k, v) in &comp.pins.names_to_id {
        let entry = match v {
            mcc::McPinPort::Single(pid) => json!({"kind": "Single", "pin": pid}),
            mcc::McPinPort::Multi(pids) => json!({"kind": "Multi", "pins": pids}),
            mcc::McPinPort::MultiGroup(groups) => json!({"kind": "MultiGroup", "groups": groups}),
            mcc::McPinPort::List(name, items) => {
                json!({"kind": "List", "name": name, "items": items})
            }
            mcc::McPinPort::Bus(bus) => json!({"kind": "Bus", "debug": format!("{:?}", bus)}),
            mcc::McPinPort::Interface(iface) => json!({
                "kind": "Interface",
                "inst_name": iface.name.to_string(),
                "base_name": iface.base_name(),
                "registered_pins": iface.registered_pins,
            }),
            mcc::McPinPort::NC => json!({"kind": "NC"}),
        };
        names_to_id.insert(k.clone(), entry);
    }

    let mut pin_id_to_names = serde_json::Map::new();
    for (k, v) in &comp.pins.pin_id_to_names {
        pin_id_to_names.insert(k.clone(), json!(v));
    }

    let data = json!({
        "name": matched_name,
        "uri": uri,
        "pins": pins,
        "pin_count": comp.pins.pins.len(),
        "names_to_id": names_to_id,
        "pin_id_to_names": pin_id_to_names,
    });

    output_json(&data, args.format)
}

fn show_module(name: &str, args: &ShowArgs) -> Result<()> {
    // Initialize (don't auto-load system library)
    mcc::mcc_init_no_lib();

    // If file is specified, load it
    if let Some(file) = &args.file {
        let uri = mcc::McURI::from(file.as_str());
        mcc::mcc_load_project(&uri);
    }

    // Exact match from loaded modules (no longer misuse first_module_name as URI)
    let modules: Vec<(String, String)> = mcc::mcb_iter_modules();
    let (matched_name, uri) = match modules.iter().find(|(n, _)| n == name) {
        Some((n, u)) => (n.clone(), u.clone()),
        None => {
            error!(target: "mcc::show", "module not found: {}", name);
            std::process::exit(1);
        }
    };

    let cmie = match mcc::get_def(
        &mcc::McIds::from(matched_name.as_str()),
        &mcc::McURI::from(uri.as_str()),
    ) {
        Some(c) => c,
        None => {
            error!(target: "mcc::show", "module not found: {}", name);
            std::process::exit(1);
        }
    };

    let mcc::McCMIE::Module(module) = cmie else {
        error!(target: "mcc::show", "'{}' is not a Module", name);
        std::process::exit(1);
    };

    let insts: Vec<serde_json::Value> = module
        .insts
        .iter()
        .map(|(n, inst)| {
            let (kind, class) = match inst {
                mcc::McInstance::Component(c) => ("component", c.name.to_string()),
                mcc::McInstance::Module(m) => ("module", m.name.to_string()),
                mcc::McInstance::Label(l) => ("label", l.clone()),
                mcc::McInstance::Interface(i) => ("interface", i.name.to_string()),
                mcc::McInstance::Bus(b) => ("bus", b.name().to_string()),
                mcc::McInstance::BusRef { component, bus } => {
                    ("busref", format!("{}.{}", component, bus))
                }
                mcc::McInstance::List(l) => ("list", l.name().to_string()),
            };
            json!({ "name": n.to_string(), "kind": kind, "class": class })
        })
        .collect();

    let data = json!({
        "name": matched_name,
        "uri": uri,
        "instances": insts,
    });

    output_json(&data, args.format)
}

fn show_interface(name: &str, args: &ShowArgs) -> Result<()> {
    // Initialize (don't auto-load system library)
    mcc::mcc_init_no_lib();

    // If file is specified, load it
    if let Some(file) = &args.file {
        let uri = mcc::McURI::from(file.as_str());
        mcc::mcc_load_project(&uri);
    }

    let ifaces: Vec<(String, String)> = mcc::mcb_iter_interfaces();

    let (matched_name, uri) = match ifaces.iter().find(|(n, _)| n == name) {
        Some((n, u)) => (n.clone(), u.clone()),
        None => {
            error!(target: "mcc::show", "interface not found: {}", name);
            std::process::exit(1);
        }
    };

    let ident = mcc::McIds::from(matched_name.as_str());
    let uri_obj = mcc::McURI::from(uri.as_str());

    let cmie = match mcc::get_def(&ident, &uri_obj) {
        Some(c) => c,
        None => {
            error!(target: "mcc::show", "interface not found: {}", name);
            std::process::exit(1);
        }
    };

    let mcc::McCMIE::Interface(_) = cmie else {
        error!(target: "mcc::show", "'{}' is not an Interface", name);
        std::process::exit(1);
    };

    let data = json!({
        "name": matched_name,
        "uri": uri,
    });

    output_json(&data, args.format)
}

fn show_net(name: &str, args: &ShowArgs) -> Result<()> {
    // Initialize (do not auto-load system libraries)
    mcc::mcc_init_no_lib();

    // If file is specified, load it
    if let Some(file) = &args.file {
        let uri = mcc::McURI::from(file.as_str());
        mcc::mcc_load_project(&uri);
    }

    // Top level: prefer --top, otherwise first module
    let top_name = args
        .top
        .as_ref()
        .cloned()
        .or_else(|| mcc::mcb_get_first_module_name())
        .unwrap_or_else(|| {
            error!(target: "mcc::show", "no modules found\nhint: load file first or use --file");
            std::process::exit(1);
        });

    // Find URI corresponding to top_name from loaded modules
    let uri = mcc::mcb_iter_modules()
        .iter()
        .find(|(n, _)| *n == top_name)
        .map(|(_, u)| mcc::McURI::from(u.as_str()))
        .unwrap_or_else(|| mcc::McURI::from(top_name.as_str()));

    let ident = mcc::McIds::from(top_name.as_str());

    // Guardrail: Pass2 panic doesn't exit directly
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build(&ident, &uri)
    }));
    let inst = match built {
        Ok(Ok(i)) => i,
        Ok(Err(e)) => {
            error!(target: "mcc::show", "build failed: {}", e);
            std::process::exit(1);
        }
        Err(_) => {
            error!(target: "mcc::show", "build panicked (engine Pass2 bug)");
            std::process::exit(1);
        }
    };

    use std::collections::BTreeMap;
    let mut nets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for conn in &inst.connections {
        let net = conn
            .net_name
            .clone()
            .unwrap_or_else(|| format!("__net_{}", conn.id));
        if net == "NC" {
            continue;
        }
        let bucket = nets.entry(net).or_default();
        for p in &conn.points {
            if p.path == "NC" {
                continue;
            }
            let label = if let Some(ref o) = p.owner {
                format!("{}.{}", o, p.path.split('.').last().unwrap_or(&p.path))
            } else {
                p.path.clone()
            };
            if !bucket.contains(&label) {
                bucket.push(label);
            }
        }
    }

    // Find matching net
    let data = if name.is_empty() {
        let items: Vec<serde_json::Value> = nets
            .iter()
            .map(|(n, points)| json!({ "name": n, "points": points }))
            .collect();
        json!({ "nets": items })
    } else {
        match nets.get(name) {
            Some(points) => json!({ "name": name, "points": points }),
            None => {
                json!({ "name": name, "points": Vec::<String>::new(), "error": "net not found" })
            }
        }
    };

    output_json(&data, args.format)
}

fn output_json(data: &serde_json::Value, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", data),
        OutputFormat::JsonPretty => println!("{}", serde_json::to_string_pretty(data).unwrap()),
        OutputFormat::Yaml => println!("{}", serde_yaml::to_string(data).unwrap_or_default()),
        OutputFormat::Text => {
            // Text format: human-readable format
            if let Some(obj) = data.as_object() {
                for (k, v) in obj {
                    println!("{}: {}", k, v);
                }
            } else {
                println!("{}", data);
            }
        }
    }
    Ok(())
}
