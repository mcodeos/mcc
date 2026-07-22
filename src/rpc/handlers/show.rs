// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_parse (lines 2215-2275 in original) ===

pub fn handle_parse(params: Option<Value>) -> RpcResult {
    let p: ParseParams = parse_or_default(params)?;
    let (id, kind, root) = crate::workspace_info();

    // S3 fix: load the libs passed via CLI --lib into the mcode global table, otherwise mcb_get_cmie
    // cannot find interfaces like SPI/I2C/DC, and the component pin's 'X::Interface(...)' syntax
    // will fall back to a bare alias (e.g. pin registered as Single("VIN{Vin, GND}"))
    load_libs_rpc(&p.libs);

    // Without workspace, parse the file directly
    if kind == "Anonymous" {
        let entry = p
            .entry
            .as_deref()
            .ok_or_else(|| JsonRpcError::custom(-32602, "parse: need to specify <target> file"))?;

        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };

        let _uri = McURI::from(abs_entry.to_string_lossy().as_ref() as &str);

        return run_pass1(&abs_entry, "parse", "file", &id, p.include_system);
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let root_path = cwd.join(&root);

    let entry_str = p.entry.as_deref().map(|s| {
        let entry_path = PathBuf::from(s);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };
        if abs_entry.starts_with(&root_path) {
            abs_entry
                .strip_prefix(&root_path)
                .unwrap_or(&abs_entry)
                .to_string_lossy()
                .to_string()
        } else {
            s.to_string()
        }
    });

    let entry_path = match kind.as_str() {
        "Project" => resolve_project_entry(&id, entry_str.as_deref())?,
        _ => {
            return Err(JsonRpcError::custom(
                -32102,
                "parse: only project workspace is supported",
            ))
        }
    };
    run_pass1(&entry_path, "parse", "project", &id, p.include_system)
}

// === handle_show_component_list (lines 2330-2351 in original) ===

pub fn handle_show_component_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let comps: Vec<(String, String)> = crate::mcb_iter_components();
    let names: Vec<String> = if let Some(ref file) = p.file {
        filter_items_by_file(&comps, file)
    } else {
        comps.iter().map(|(n, _)| n.clone()).collect()
    };

    Ok(json!({
        "type": "component",
        "count": names.len(),
        "list": names,
    }))
}

// === handle_show_module_list (lines 2353-2374 in original) ===

pub fn handle_show_module_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let modules: Vec<(String, String)> = crate::mcb_iter_modules();
    let names: Vec<String> = if let Some(ref file) = p.file {
        filter_items_by_file(&modules, file)
    } else {
        modules.iter().map(|(n, _)| n.clone()).collect()
    };

    Ok(json!({
        "type": "module",
        "count": names.len(),
        "list": names,
    }))
}

// === handle_show_interface_list (lines 2376-2397 in original) ===

pub fn handle_show_interface_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let ifaces: Vec<(String, String)> = crate::mcb_iter_interfaces();
    let names: Vec<String> = if let Some(ref file) = p.file {
        filter_items_by_file(&ifaces, file)
    } else {
        ifaces.iter().map(|(n, _)| n.clone()).collect()
    };

    Ok(json!({
        "type": "interface",
        "count": names.len(),
        "list": names,
    }))
}

// === handle_show_net_list (lines 2399-2406 in original) ===

pub fn handle_show_net_list(_params: Option<Value>) -> RpcResult {
    Ok(json!({
        "type": "net",
        "count": 0,
        "list": [],
        "note": "Nets need to be retrieved when viewing modules via show.module",
    }))
}

// === handle_show_component (lines 2408-2479 in original) ===

pub fn handle_show_component(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    // If a file is specified, load it
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.component: need to specify name"))?;
    let comps = crate::mcb_iter_components();
    let name_str = name.as_str();

    let (matched_name, uri) = comps
        .iter()
        .find(|(n, _)| n == name_str)
        .map(|(n, u)| (n.clone(), u.clone()))
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("component not found: {name}")))?;

    let ident = crate::McIds::from(matched_name.as_str());
    let uri_obj = crate::McURI::from(uri.as_str());

    let cmie = crate::get_def(&ident, &uri_obj)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("component not found: {name}")))?;

    match cmie {
        crate::McCMIE::Component(comp) => {
            // Build detailed pin information
            let pins: Vec<serde_json::Value> = comp
                .pins
                .pins
                .iter()
                .map(|(pin_id, pin)| {
                    // Try to extract description from values
                    let mut desc = String::new();
                    for val in pin.values.iter() {
                        if let crate::McAttrVal::AttrLiteral(crate::McLiteral::String(s)) = val {
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

            Ok(json!({
                "name": matched_name,
                "uri": uri,
                "pins": pins,
                "pin_count": comp.pins.pins.len(),
            }))
        }
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not a Component"),
        )),
    }
}

// === handle_show_module (lines 2481-2541 in original) ===

pub fn handle_show_module(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.module: need to specify name"))?;

    // Find the module's actual URI by iterating all modules (not just the first one)
    let modules = crate::mcb_iter_modules();
    let (_, module_uri) = modules
        .iter()
        .find(|(n, _)| n == name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("module not found: {name}")))?;

    let uri = crate::McURI::from(module_uri.as_str());
    let ident = crate::McIds::from(name.as_str());

    let cmie = crate::get_def(&ident, &uri)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("module not found: {name}")))?;

    match cmie {
        crate::McCMIE::Module(module) => {
            let insts: Vec<serde_json::Value> = module
                .insts
                .iter()
                .map(|(n, inst)| {
                    let (kind, class) = inst_kind_class(inst);
                    json!({ "name": n.to_string(), "kind": kind, "class": class })
                })
                .collect();

            Ok(json!({
                "name": name,
                "uri": uri,
                "instances": insts,
            }))
        }
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not a Module"),
        )),
    }
}

// === handle_show_interface (lines 2543-2576 in original) ===

pub fn handle_show_interface(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.interface: need to specify name"))?;

    let ifaces = crate::mcb_iter_interfaces();
    let name_str = name.as_str();

    let (matched_name, uri) = ifaces
        .iter()
        .find(|(n, _)| n == name_str)
        .map(|(n, u)| (n.clone(), u.clone()))
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("interface not found: {name}")))?;

    let ident = crate::McIds::from(matched_name.as_str());
    let uri_obj = crate::McURI::from(uri.as_str());

    let cmie = crate::get_def(&ident, &uri_obj)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("interface not found: {name}")))?;

    match cmie {
        crate::McCMIE::Interface(_) => Ok(json!({
            "name": matched_name,
            "uri": uri,
        })),
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not an Interface"),
        )),
    }
}

// === handle_show_net (lines 2578-2636 in original) ===

pub fn handle_show_net(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;

    let top_name = crate::mcb_get_first_module_name()
        .ok_or_else(|| JsonRpcError::custom(-32003, "no module found"))?;

    let uri = crate::McURI::from(top_name.as_str());
    let ident = crate::McIds::from(top_name.as_str());

    let inst = crate::mcc_build(&ident, &uri)
        .map_err(|e| JsonRpcError::custom(-32002, &format!("build failed: {e}")))?;

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
        for pt in &conn.points {
            if pt.path == "NC" {
                continue;
            }
            let label = if let Some(ref o) = pt.owner {
                format!(
                    "{}.{}",
                    o,
                    pt.path.split('.').next_back().unwrap_or(&pt.path)
                )
            } else {
                pt.path.clone()
            };
            if !bucket.contains(&label) {
                bucket.push(label);
            }
        }
    }

    if p.name.is_none() {
        let items: Vec<serde_json::Value> = nets
            .iter()
            .map(|(n, points)| json!({ "name": n, "points": points }))
            .collect();
        Ok(json!({ "nets": items }))
    } else {
        let name = p.name.unwrap();
        match nets.get(&name) {
            Some(points) => Ok(json!({ "name": name, "points": points })),
            None => {
                let msg = format!("net not found: {name}");
                Err(JsonRpcError::custom(32003, &msg))
            }
        }
    }
}

// === handle_show_all (lines 2759-2776 in original) ===

pub fn handle_show_all(_params: Option<Value>) -> RpcResult {
    let comps = crate::mcb_iter_components();
    let mods = crate::mcb_iter_modules();
    let ifaces = crate::mcb_iter_interfaces();
    let enums = crate::mcb_iter_enums();

    Ok(json!({
        format!("component_list({})", comps.len()): comps.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        format!("module_list({})", mods.len()): mods.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        format!("interface_list({})", ifaces.len()): ifaces.iter().map(|(n,_)| n).collect::<Vec<_>>(),
        format!("enum_list({})", enums.len()): enums.iter().map(|(n,_)| n).collect::<Vec<_>>(),
    }))
}

// === handle_show_file (lines 2778-2822 in original) ===

pub fn handle_show_file(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;
    let file = p.file.unwrap_or_default();

    // Load the file if provided
    if !file.is_empty() {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let comps: Vec<(String, String)> = crate::mcb_iter_components();
    let mods: Vec<(String, String)> = crate::mcb_iter_modules();
    let ifaces: Vec<(String, String)> = crate::mcb_iter_interfaces();
    let enums: Vec<(String, String)> = crate::mcb_iter_enums();

    // Filter by file when a file path is specified
    let (component_list, module_list, interface_list, enum_list) = if file.is_empty() {
        (
            comps.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
            mods.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
            ifaces.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
            enums.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(),
        )
    } else {
        (
            filter_items_by_file(&comps, &file),
            filter_items_by_file(&mods, &file),
            filter_items_by_file(&ifaces, &file),
            filter_items_by_file(&enums, &file),
        )
    };

    Ok(json!({
        "file": file,
        format!("component_list({})", component_list.len()): component_list,
        format!("module_list({})", module_list.len()): module_list,
        format!("interface_list({})", interface_list.len()): interface_list,
        format!("enum_list({})", enum_list.len()): enum_list,
    }))
}

// === handle_show_files (lines 2824-2863 in original) ===

pub fn handle_show_files(_params: Option<Value>) -> RpcResult {
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct FileInfo {
        component_count: usize,
        module_count: usize,
        interface_count: usize,
        enum_count: usize,
    }

    let mut files: BTreeMap<String, FileInfo> = BTreeMap::new();
    for (_, uri) in crate::mcb_iter_components() {
        files.entry(uri).or_default().component_count += 1;
    }
    for (_, uri) in crate::mcb_iter_modules() {
        files.entry(uri).or_default().module_count += 1;
    }
    for (_, uri) in crate::mcb_iter_interfaces() {
        files.entry(uri).or_default().interface_count += 1;
    }
    for (_, uri) in crate::mcb_iter_enums() {
        files.entry(uri).or_default().enum_count += 1;
    }

    let items: Vec<Value> = files
        .into_iter()
        .map(|(uri, info)| {
            json!({
                "uri": uri,
                "component_count": info.component_count,
                "module_count": info.module_count,
                "interface_count": info.interface_count,
                "enum_count": info.enum_count,
            })
        })
        .collect();

    Ok(json!({ "type": "files", "count": items.len(), "files": items }))
}

// === handle_show_enum_list (lines 2865-2878 in original) ===

pub fn handle_show_enum_list(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }
    let enums = crate::mcb_iter_enums();
    let names: Vec<String> = if let Some(ref file) = p.file {
        filter_items_by_file(&enums, file)
    } else {
        enums.iter().map(|(n, _)| n.clone()).collect()
    };
    Ok(json!({ "type": "enum", "count": names.len(), "list": names }))
}

// === handle_show_enum (lines 2880-2905 in original) ===

pub fn handle_show_enum(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.enum: need to specify name"))?;

    let (cmie, uri) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("enum not found: {name}")))?;

    match cmie {
        crate::McCMIE::Enum(en) => {
            let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
            Ok(json!({
                "name": name,
                "uri": uri,
                "value_count": values.len(),
                "values": values,
            }))
        }
        _ => Err(JsonRpcError::custom(
            -32002,
            &format!("'{name}' is not an Enum"),
        )),
    }
}

// === handle_show_pins (lines 2911-2934 in original) ===

pub fn handle_show_pins(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.pins: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let pins = match &cmie {
        crate::McCMIE::Component(c) => &c.pins,
        crate::McCMIE::Interface(i) => &i.pins,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have pins (only components and interfaces do)"),
            ))
        }
    };
    let mut data = pins_json(pins);
    data["name"] = json!(name);
    Ok(data)
}

// === handle_show_ports (lines 2936-2961 in original) ===

pub fn handle_show_ports(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.ports: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let module = match &cmie {
        crate::McCMIE::Module(m) => m,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not a Module"),
            ))
        }
    };
    let ports: Vec<Value> = module
        .insts
        .iter_ports()
        .map(|(pname, io)| json!({ "name": pname, "iotype": format!("{:?}", io) }))
        .collect();
    Ok(json!({ "name": name, "port_count": ports.len(), "ports": ports }))
}

// === handle_show_ports_list (lines 2963-2971 in original) ===

pub fn handle_show_ports_list(_params: Option<Value>) -> RpcResult {
    let ports: Vec<Value> = crate::mcb_iter_ports()
        .into_iter()
        .map(|(name, iotype, module, uri)| {
            json!({ "name": name, "iotype": iotype, "module": module, "uri": uri })
        })
        .collect();
    Ok(json!({ "type": "port", "count": ports.len(), "ports": ports }))
}

// === handle_show_labels (lines 2973-2999 in original) ===

pub fn handle_show_labels(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.labels: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let module = match &cmie {
        crate::McCMIE::Module(m) => m,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not a Module"),
            ))
        }
    };
    let labels: Vec<String> = module
        .insts
        .iter()
        .filter(|(_, inst)| matches!(inst, crate::McInstance::Label(_)))
        .map(|(n, _)| n.to_string())
        .collect();
    Ok(json!({ "name": name, "label_count": labels.len(), "labels": labels }))
}

// === handle_show_instances (lines 3001-3034 in original) ===

pub fn handle_show_instances(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.instances: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let insts = match &cmie {
        crate::McCMIE::Component(c) => &c.insts,
        crate::McCMIE::Module(m) => &m.insts,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have instances (only components and modules do)"),
            ))
        }
    };
    let items: Vec<Value> = insts
        .iter()
        .filter_map(|(n, inst)| {
            let (kind, class) = inst_kind_class(inst);
            if let Some(ref t) = p.type_filter {
                if !kind.eq_ignore_ascii_case(t) {
                    return None;
                }
            }
            Some(json!({ "name": n.to_string(), "kind": kind, "class": class }))
        })
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "instances": items }))
}

// === handle_show_nets (lines 3036-3091 in original) ===

pub fn handle_show_nets(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.nets: need to specify name"))?;

    let top = p.top.as_ref().unwrap_or(name);
    let top_uri = crate::mcb_iter_modules()
        .iter()
        .find(|(n, _)| n == top)
        .map(|(_, u)| crate::McURI::from(u.as_str()))
        .unwrap_or_else(|| crate::McURI::from(top));
    let ident = crate::McIds::from(top.as_str());

    let inst = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build(&ident, &top_uri)
    }))
    .map_err(|_| JsonRpcError::custom(-32002, "build panicked (engine Pass2 bug)"))?
    .map_err(|e| JsonRpcError::custom(-32002, &format!("build failed: {e}")))?;

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
        for pt in &conn.points {
            if pt.path == "NC" {
                continue;
            }
            let label = if let Some(ref o) = pt.owner {
                format!(
                    "{}.{}",
                    o,
                    pt.path.split('.').next_back().unwrap_or(&pt.path)
                )
            } else {
                pt.path.clone()
            };
            if !bucket.contains(&label) {
                bucket.push(label);
            }
        }
    }

    let items: Vec<Value> = nets
        .iter()
        .map(|(n, points)| json!({ "name": n, "points": points }))
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "nets": items }))
}

// === handle_show_attrs (lines 3093-3121 in original) ===

pub fn handle_show_attrs(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.attrs: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let attrs = match &cmie {
        crate::McCMIE::Component(c) => &c.attrs,
        crate::McCMIE::Interface(i) => &i.attrs,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have attributes (only components and interfaces do)"),
            ))
        }
    };
    let items: Vec<Value> = attrs
        .iter()
        .map(|a| {
            let values: Vec<Value> = a.values.iter().map(attrval_json).collect();
            json!({ "no": a.no, "name": a.id.to_string(), "values": values })
        })
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "attrs": items }))
}

// === handle_show_funcs (lines 3123-3148 in original) ===

pub fn handle_show_funcs(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.funcs: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let funcs = match &cmie {
        crate::McCMIE::Component(c) => &c.funcs,
        crate::McCMIE::Module(m) => &m.funcs,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have functions (only components and modules do)"),
            ))
        }
    };
    let items: Vec<Value> = funcs
        .iter()
        .map(|f| json!({ "name": f.name.to_string(), "params": f.params.names() }))
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "funcs": items }))
}

// === handle_show_params (lines 3150-3187 in original) ===

pub fn handle_show_params(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.params: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let (param_list, arity) = match &cmie {
        crate::McCMIE::Component(c) => {
            let list: Vec<Value> = c.params.iter().map(|d| param_declare_to_json(d)).collect();
            (list, c.params.arity())
        }
        crate::McCMIE::Module(m) => {
            let list: Vec<Value> = m.params.iter().map(|d| param_declare_to_json(d)).collect();
            (list, m.params.arity())
        }
        crate::McCMIE::Interface(i) => {
            let list: Vec<Value> = i.params.iter().map(|d| param_declare_to_json(d)).collect();
            (list, i.params.arity())
        }
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' does not have params"),
            ))
        }
    };
    Ok(json!({
        "name": name,
        "count": param_list.len(),
        "required": arity.required,
        "optional": arity.optional,
        "params": param_list
    }))
}

// === handle_show_roles (lines 3206-3236 in original) ===

pub fn handle_show_roles(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.roles: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let iface = match &cmie {
        crate::McCMIE::Interface(i) => i,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not an Interface"),
            ))
        }
    };
    let items: Vec<Value> = iface
        .roles
        .iter()
        .map(|r| {
            json!({
                "name": r.name.to_string(),
                "pins": pins_json(&r.pins),
            })
        })
        .collect();
    Ok(json!({ "name": name, "count": items.len(), "roles": items }))
}

// === handle_show_values (lines 3238-3259 in original) ===

pub fn handle_show_values(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.values: need to specify name"))?;

    let (cmie, _) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let en = match &cmie {
        crate::McCMIE::Enum(e) => e,
        _ => {
            return Err(JsonRpcError::custom(
                -32002,
                &format!("'{name}' is not an Enum"),
            ))
        }
    };
    let values: Vec<String> = en.values.iter().map(|v| v.name.to_string()).collect();
    Ok(json!({ "name": name, "count": values.len(), "values": values }))
}

// === handle_show_dump (lines 3261-3284 in original) ===

pub fn handle_show_dump(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_strict(params)?;
    let name = p
        .name
        .as_ref()
        .ok_or_else(|| JsonRpcError::custom(-32602, "show.dump: need to specify name"))?;

    // If a file is specified, load it first
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    let (cmie, uri) = find_def_by_name(name)
        .ok_or_else(|| JsonRpcError::custom(-32003, &format!("entity not found: {name}")))?;

    let data = match &cmie {
        crate::McCMIE::Component(comp) => dump_component_json(name, comp, &uri),
        crate::McCMIE::Module(module) => dump_module_json(name, module, &uri),
        crate::McCMIE::Interface(iface) => dump_interface_json(name, iface, &uri),
        crate::McCMIE::Enum(en) => dump_enum_json(name, en, &uri),
    };
    Ok(data)
}

// === handle_show_dump_all (lines 3286-3349 in original) ===

pub fn handle_show_dump_all(params: Option<Value>) -> RpcResult {
    let p: ShowParams = parse_or_default(params)?;

    // Load file if specified
    if let Some(file) = &p.file {
        let uri = McURI::from(file.as_str());
        crate::mcc_load_project(&uri);
    }

    macro_rules! collect_defs {
        ($out:ident, $iter:expr, $variant:ident, $dump_fn:ident) => {
            for (name, _) in $iter {
                match find_def_by_name(&name) {
                    Some((crate::McCMIE::$variant(def), uri)) => {
                        $out.push($dump_fn(&name, def.as_ref(), &uri));
                    }
                    Some(_) => {} // wrong variant, skip
                    None => {
                        tracing::debug!(target: "mcc::rpc", name = %name, "dump_all: def not found (stale iterator?)");
                    }
                }
            }
        };
    }
    let mut all = Vec::new();
    collect_defs!(
        all,
        crate::mcb_iter_components(),
        Component,
        dump_component_json
    );
    collect_defs!(all, crate::mcb_iter_modules(), Module, dump_module_json);
    collect_defs!(
        all,
        crate::mcb_iter_interfaces(),
        Interface,
        dump_interface_json
    );
    collect_defs!(all, crate::mcb_iter_enums(), Enum, dump_enum_json);

    // Apply file filter for consistency
    let mut all = if let Some(ref file) = p.file {
        let target = resolve_to_abs_uri(file);
        let parent_dir = std::path::Path::new(&target)
            .parent()
            .map(|p| p.to_string_lossy().to_string());
        all.into_iter()
            .filter(|e| {
                let uri = e["uri"].as_str().unwrap_or("");
                uri == target || parent_dir.as_ref().map_or(false, |d| uri.starts_with(d))
            })
            .collect::<Vec<_>>()
    } else {
        all
    };

    // Sort by source position
    all.sort_by_key(|e| e["span"]["start"].as_u64().unwrap_or(u64::MAX));

    Ok(json!({
        "type": "dump_all",
        "total": all.len(),
        "entities": all,
    }))
}

// === handle_report (lines 3924-3954 in original) ===

pub fn handle_report(_params: Option<Value>) -> RpcResult {
    let comps = crate::mcb_iter_components();
    let mods = crate::mcb_iter_modules();
    let ifaces = crate::mcb_iter_interfaces();
    let enums = crate::mcb_iter_enums();

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

    Ok(json!({
        "summary": {
            "component_count": comps.len(),
            "module_count": mods.len(),
            "interface_count": ifaces.len(),
            "enum_count": enums.len(),
        },
        "components_by_prefix": by_prefix,
        "components": comps.iter().take(20).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "modules": mods.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "interfaces": ifaces.iter().take(10).map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
        "enums": enums.iter().map(|(n, u)| json!({"name": n, "uri": u})).collect::<Vec<_>>(),
    }))
}
