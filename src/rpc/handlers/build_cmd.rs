// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;


// === handle_build_full (lines 767-807 in original) ===

pub fn handle_build_full(params: Option<Value>) -> RpcResult {
    let p: BuildFullParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    let (id, kind, root_str) = crate::workspace_info();
    if kind == "Anonymous" {
        let entry = p.entry.as_deref().ok_or_else(|| {
            JsonRpcError::custom(-32602, "build.full: need <entry> or active workspace")
        })?;
        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path
        } else {
            cwd.join(&entry_path)
        };
        return run_full_build(
            &abs_entry,
            p.top.as_deref(),
            "build.full",
            "file",
            &id,
            p.include_system,
        );
    }

    let _root = PathBuf::from(&root_str);
    let entry_path = match kind.as_str() {
        "Project" => resolve_project_entry(&id, p.entry.as_deref())?,
        _ => return Err(JsonRpcError::custom(32102, "unknown workspace kind")),
    };
    let top = p.top.or_else(read_project_top_from_workspace);
    run_full_build(
        &entry_path,
        top.as_deref(),
        "build.full",
        "project",
        &id,
        p.include_system,
    )
}

// === handle_extract (lines 1700-1733 in original) ===

pub fn handle_extract(params: Option<Value>) -> RpcResult {
    let p: ExtractRpcParams = parse_or_default(params)?;
    load_libs_rpc(&p.libs);

    let (id, kind, root_str) = crate::workspace_info();
    if kind == "Anonymous" {
        let entry = p
            .entry
            .as_deref()
            .ok_or_else(|| JsonRpcError::custom(-32602, "extract: need to specify <entry>"))?;
        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path.clone()
        } else {
            cwd.join(&entry_path)
        };
        let uri = McURI::from(abs_entry.to_string_lossy().as_ref() as &str);
        crate::mcc_load_project(&uri);
        return extract_from_uri(&abs_entry, p.top.as_deref(), &p.target);
    }

    let _root = PathBuf::from(&root_str);
    let entry_path = match kind.as_str() {
        "Project" => resolve_project_entry(&id, p.entry.as_deref())?,
        _ => {
            return Err(JsonRpcError::custom(
                -32102,
                "extract: only project workspace is supported",
            ))
        }
    };
    extract_from_uri(&entry_path, p.top.as_deref(), &p.target)
}

// === handle_convert (lines 3960-3971 in original) ===

pub fn handle_convert(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct ConvertParams {
        entry: String,
        #[serde(default)]
        format: Option<String>,
    }
    let p: ConvertParams = parse_strict(params)?;
    // Delegate to parse — convert is a thin wrapper
    let bp = json!({ "entry": p.entry, "format": p.format.unwrap_or_else(|| "json".into()), "include_system": false });
    handle_parse(Some(bp))
}
