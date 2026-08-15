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

// === handle_build_viz — build + render to a self-contained HTML string ===
//
// Consumption path A (mcext webview) and B (md preview) both call this: it
// resolves entry/top exactly like build.full, runs Pass2, flattens to
// McVecGraph, renders, and returns the wrapped HTML (inline CSS + interact.js).

pub fn handle_build_viz(params: Option<Value>) -> RpcResult {
    let t_all = std::time::Instant::now();
    let p: BuildVizParams = parse_or_default(params)?;
    let t0 = std::time::Instant::now();
    load_libs_rpc(&p.libs);
    tracing::info!(target: "mcc::perf", step = "load_libs", ms = t0.elapsed().as_millis() as u64, "build.viz step");

    let (id, kind, root_str) = crate::workspace_info();
    let (entry_path, top) = if kind == "Anonymous" {
        let entry = p.entry.as_deref().ok_or_else(|| {
            JsonRpcError::custom(-32602, "build.viz: need <entry> or active workspace")
        })?;
        let cwd = std::env::current_dir().unwrap_or_default();
        let entry_path = PathBuf::from(entry);
        let abs_entry = if entry_path.is_absolute() {
            entry_path
        } else {
            cwd.join(&entry_path)
        };
        (abs_entry, p.top.clone())
    } else {
        let _root = PathBuf::from(&root_str);
        let entry_path = match kind.as_str() {
            "Project" => resolve_project_entry(&id, p.entry.as_deref())?,
            _ => return Err(JsonRpcError::custom(32102, "unknown workspace kind")),
        };
        let top = p.top.clone().or_else(read_project_top_from_workspace);
        (entry_path, top)
    };

    let mc_uri = McURI::from(entry_path.to_string_lossy().as_ref());
    let t1 = std::time::Instant::now();
    crate::mcc_load_project(&mc_uri);
    tracing::info!(target: "mcc::perf", step = "load_project", ms = t1.elapsed().as_millis() as u64, "build.viz step");

    let top_name = match top.as_deref() {
        Some(t) => t.to_string(),
        None => crate::mcb_get_module_name_by_uri(&mc_uri)
            .ok_or_else(|| JsonRpcError::custom(32107, "no top module found"))?,
    };

    let ident = crate::McIds::from(top_name.as_str());
    if crate::get_def(&ident, &mc_uri).is_none() {
        return Err(JsonRpcError::custom(
            32107,
            &format!("top module '{top_name}' not defined"),
        ));
    }

    // Flatten (Pass2 + InstTable) with panic guard, mirroring execute_pass2.
    let t2 = std::time::Instant::now();
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::mcc_build_flat(&ident, &mc_uri, 1000)
    }));
    let (_inst, table) = match built {
        Ok(Ok(x)) => x,
        Ok(Err(e)) => return Err(JsonRpcError::custom(32107, &format!("build failed: {e}"))),
        Err(_) => {
            return Err(JsonRpcError::custom(
                32108,
                "Pass2 build panicked (engine bug); request aborted, server kept alive",
            ))
        }
    };
    tracing::info!(target: "mcc::perf", step = "build_flat", ms = t2.elapsed().as_millis() as u64, "build.viz step");

    crate::vector::builder::reset_np_warn_count();
    let t3 = std::time::Instant::now();
    let (vec_block, _report) = crate::build_mc_vec_with_report(&_inst, &table);
    let graph = crate::build_mc_vec_graph(&vec_block, &table);
    tracing::info!(target: "mcc::perf", step = "vec_graph", ms = t3.elapsed().as_millis() as u64, "build.viz step");

    let opts = build_viz_render_opts(p.layouter.as_deref());
    let t4 = std::time::Instant::now();
    let doc = crate::viz::api::render_with(graph, opts);
    tracing::info!(target: "mcc::perf", step = "render", ms = t4.elapsed().as_millis() as u64, "build.viz step");
    let html = crate::viz::template::wrap_document(&doc);
    tracing::info!(target: "mcc::perf", step = "total", ms = t_all.elapsed().as_millis() as u64, "build.viz step");

    Ok(json!({
        "command": "build.viz",
        "top": top_name,
        "html": html,
        "svg_bytes": doc.total_svg_bytes(),
        "layers": doc.layer_count(),
    }))
}

/// Map a layouter name to [`crate::viz::api::RenderOpts`]; unknown names fall
/// back to the default (flow). Mirrors `build_viz_opts` in the CLI build cmd.
/// ★ P7-era cleanup: the experimental layouters (hierarchical / layered /
/// radial / schematic_radial / schematic_sub) were removed as dead code;
/// only `flow` remains selectable.
fn build_viz_render_opts(layouter_name: Option<&str>) -> crate::viz::api::RenderOpts {
    use crate::viz::layout::FlowLayouter;
    use crate::viz::traits::Layouter;

    let mut opts = crate::viz::api::RenderOpts::default();
    let name = match layouter_name {
        Some(n) => n,
        None => return opts,
    };
    let (top, sub, top_cands, sub_cands): (
        Box<dyn Layouter>,
        Box<dyn Layouter>,
        Vec<Box<dyn Layouter>>,
        Vec<Box<dyn Layouter>>,
    ) = match name {
        "flow" => (
            Box::new(FlowLayouter::default()) as Box<dyn Layouter>,
            Box::new(FlowLayouter::sub()),
            vec![Box::new(FlowLayouter::default()) as Box<dyn Layouter>],
            vec![Box::new(FlowLayouter::sub())],
        ),
        _ => return opts,
    };
    opts.top_layouter = top;
    opts.sub_layouter = sub;
    opts.top_candidates = top_cands;
    opts.sub_candidates = sub_cands;
    opts
}
