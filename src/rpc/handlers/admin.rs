// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_project_list (lines 87-107 in original) ===

pub fn handle_project_list(_params: Option<Value>) -> RpcResult {
    let pdir = projects_dir();
    if !pdir.exists() {
        return Ok(json!([]));
    }
    let mut projects = Vec::new();
    for entry in fs::read_dir(&pdir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                projects.push(json!({
                    "name": name,
                    "path": path.to_string_lossy(),
                    "has_manifest": project_manifest(name).exists(),
                }));
            }
        }
    }
    Ok(Value::Array(projects))
}

// === handle_project_info (lines 109-122 in original) ===

pub fn handle_project_info(params: Option<Value>) -> RpcResult {
    let name = parse_string_param(params, &["name", "project"])?;
    let pdir = project_dir(&name);
    if !pdir.exists() {
        return Err(JsonRpcError::custom(32102, "project not found"));
    }
    let (active_id, _, _) = crate::workspace_info();
    Ok(json!({
        "name": name,
        "path": pdir.to_string_lossy(),
        "has_manifest": project_manifest(&name).exists(),
        "active": name == active_id,
    }))
}

// === handle_library_list (lines 124-196 in original) ===

pub fn handle_library_list(_params: Option<Value>) -> RpcResult {
    let mut libs = Vec::new();
    // Memory-loaded libraries
    let loaded = crate::mcb_loaded_libs();
    for name in &loaded {
        let info = crate::mcb_lib_info(name);
        libs.push(json!({
            "name": name,
            "loaded": true,
            "symbols": info.as_ref().map(|i| i.total_symbols).unwrap_or(0),
            "modules": info.as_ref().map(|i| i.module_count).unwrap_or(0),
            "components": info.as_ref().map(|i| i.component_count).unwrap_or(0),
        }));
    }
    // Disk-installed libraries: prefer reading the v1 layout's index.json.
    // Falls back to filesystem scan if index is missing or stale.
    let mut installed = Vec::new();
    if let Some(index) = crate::cli::datadir::read_index_if_present() {
        // v1 index path: enumerate system + 3rdparty from JSON.
        for entry in index.system {
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name == "mcode" && !loaded.contains(&"mcode".to_string()) {
                installed.push(json!({"name":"mcode","version":entry.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0"),"loaded":false}));
            }
        }
        for entry in index.thirdparty {
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() || loaded.contains(&name) {
                continue;
            }
            installed.push(json!({
                "name": name,
                "version": entry.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0"),
                "loaded": false,
            }));
        }
    } else {
        // Fallback: scan mcc_system_root(), system/, and 3rdparty/ (legacy).
        if mcode_dir().exists() && !loaded.contains(&"mcode".to_string()) {
            installed.push(json!({"name":"mcode","version":"*","loaded":false}));
        }
        let system_dirs = ["logs", "config", "mclibs", "projects", "unitest"];
        if let Ok(entries) = fs::read_dir(mcc_system_root()) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if entry.path().is_dir() && !system_dirs.contains(&fname.as_str()) {
                    let (name, version) = match fname.find('@') {
                        Some(at) => (fname[..at].to_string(), fname[at + 1..].to_string()),
                        None => (fname.clone(), "0.0.0".to_string()),
                    };
                    if name == "mcode" || loaded.contains(&name) {
                        continue;
                    }
                    let lib_path = mcc_system_root().join(&fname);
                    let entry_file = lib_path.join(format!("{name}.mc"));
                    if !entry_file.exists() {
                        continue;
                    }
                    installed.push(json!({"name":name,"version":version,"loaded":false}));
                }
            }
        }
    }
    Ok(json!({"loaded": libs, "installed": installed}))
}

// === handle_library_show (lines 203-226 in original) ===

pub fn handle_library_show(params: Option<Value>) -> RpcResult {
    let p: LibraryShowParams = parse_strict(params)?;
    let name = p.name.as_str();

    // Get library info
    let info = crate::mcb_lib_info(name).ok_or_else(|| {
        JsonRpcError::custom(32107, format!("Library '{}' not loaded", p.name).as_str())
    })?;

    Ok(json!({
        "name": info.name,
        "root": info.root,
        "modules": info.modules,
        "module_count": info.module_count,
        "components": info.components,
        "component_count": info.component_count,
        "interfaces": info.interfaces,
        "interface_count": info.interface_count,
        "enums": info.enums,
        "enum_count": info.enum_count,
        "total_symbols": info.total_symbols,
        "loaded": true,
    }))
}

// === handle_server_info (lines 228-241 in original) ===

pub fn handle_server_info(_params: Option<Value>) -> RpcResult {
    let (active_id, kind, root) = crate::workspace_info();
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "status": "running",
        "data_dir": mcc_system_root().to_string_lossy(),
        "active_workspace": {
            "id": active_id,
            "kind": kind,
            "root": root,
        },
        "loaded_libs": crate::mcb_loaded_libs(),
    }))
}

// === handle_methods (Phase 8.3: auto-generated from METHOD registry) ===

pub fn handle_methods(_params: Option<Value>) -> RpcResult {
    let caps = super::caps_json();
    Ok(caps["methods"].clone())
}

// === handle_set_project_root (lines 4336-4345 in original) ===
pub fn handle_set_project_root(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct SetProjectRootParams {
        path: String,
    }

    let p: SetProjectRootParams = parse_strict(params)?;
    crate::mcc_set_project_root(std::path::Path::new(&p.path));
    Ok(serde_json::json!({ "ok": true }))
}

// === handle_set_system_root (lines 4348-4357 in original) ===
pub fn handle_set_system_root(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct SetSystemRootParams {
        path: String,
    }

    let p: SetSystemRootParams = parse_strict(params)?;
    crate::mcc_set_system_root(std::path::Path::new(&p.path));
    Ok(serde_json::json!({ "ok": true }))
}

// === handle_load_project (lines 4371-4381 in original) ===
pub fn handle_load_project(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct LoadProjectParams {
        entry: String,
    }

    let p: LoadProjectParams = parse_strict(params)?;
    let mc_uri = McURI::from(p.entry.as_str());
    crate::mcc_load_project(&mc_uri);
    Ok(serde_json::json!({ "ok": true }))
}
