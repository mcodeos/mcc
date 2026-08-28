// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_build_full (lines 767-807 in original) ===

pub fn handle_build_full(params: Option<Value>) -> RpcResult {
    let p: BuildFullParams = parse_or_default(params)?;
    // `build.full` is a fresh pipeline (local `mcc build` runs in a fresh
    // process): reset the active workspace so the previous request's project
    // tables and diagnostics (both append-only across the server's lifetime)
    // don't leak into this build's definitions or phase snapshots. System-lib
    // tables (`global::mcc_*`) survive, matching how libs are re-loaded below.
    crate::mcc_clear_workspace();
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
        return run_full_build_envelope(
            &abs_entry,
            p.top.as_deref(),
            "mcc build",
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
    run_full_build_envelope(
        &entry_path,
        top.as_deref(),
        "mcc build",
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

    // Target selection (mcd docs-mc 16-export-viz §6): explicit top → all
    // modules in the file → all components → all interfaces. Components and
    // interfaces are "virtually instantiated" via a synthetic module.
    let targets = crate::mcc_virtual_resolve_targets(&mc_uri, top.as_deref())
        .map_err(|e| JsonRpcError::custom(32107, &e))?;

    let mut svgs: Vec<(String, String)> = Vec::new();
    let mut single_doc: Option<crate::viz::doc::VizDocument> = None;
    let mut top_name = String::new();
    for target in &targets {
        // Flatten (Pass2 + InstTable) with panic guard, mirroring execute_pass2.
        let t2 = std::time::Instant::now();
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::mcc_virtual_build_flat(target, &mc_uri, 1000)
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
        if top_name.is_empty() {
            top_name = target.clone();
        }
        tracing::info!(target: "mcc::perf", step = "build_flat", ms = t2.elapsed().as_millis() as u64, "build.viz step");

        crate::vector::builder::reset_np_warn_count();
        let t3 = std::time::Instant::now();
        let (vec_block, _report) = crate::build_mc_vec_with_report(&_inst, &table);
        let is_virtual_target = !crate::mcc_get_modules_in_file(&mc_uri)
            .iter()
            .any(|m| m == target);
        let graph = crate::build_mc_vec_graph(&vec_block, &table);
        // Virtual (component/interface) targets render in the device pipeline
        // with the fabricated instance name hidden so the physical pins show.
        let graph = if is_virtual_target {
            crate::mcc_virtual_prepare_graph(graph, target)
        } else {
            graph
        };
        tracing::info!(target: "mcc::perf", step = "vec_graph", ms = t3.elapsed().as_millis() as u64, "build.viz step");

        let opts = build_viz_render_opts(p.layouter.as_deref());
        let t4 = std::time::Instant::now();
        let doc = crate::viz::api::render_with(graph, opts);
        tracing::info!(target: "mcc::perf", step = "render", ms = t4.elapsed().as_millis() as u64, "build.viz step");
        if let Some(root_layer) = doc.root_layer() {
            svgs.push((target.clone(), root_layer.svg.clone()));
            if targets.len() == 1 {
                single_doc = Some(doc);
            }
        }
    }

    if svgs.is_empty() {
        return Err(JsonRpcError::custom(
            32107,
            "viz: no module rendered for the requested targets",
        ));
    }

    // Single target: keep the full render document. Multiple targets (peer
    // modules, or several components/interfaces in one file): stack the SVGs
    // vertically into one self-contained HTML.
    let doc = if let Some(doc) = single_doc {
        doc
    } else {
        let combined_svg = crate::viz::template::combine_svgs(&svgs);
        let mut doc = crate::viz::doc::VizDocument::new(1000, "all_targets".into());
        let mut layer = crate::viz::layer::VizLayer::new(1000, "all_targets".into(), None);
        layer.svg = combined_svg;
        doc.add_layer(layer);
        doc
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize against every other workspace-driving test in the crate
    /// (the C parser is not re-entrant across threads).
    fn parse_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::db::infra::init::MCC_TEST_PARSE_LOCK
            .lock()
            .expect("test parse lock")
    }

    fn tmp_file(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mcc-buildviz-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("part.mc");
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Regression: `build.viz` on a component-only file (no project.toml, no
    /// module) must not fail with "no top module found". The component is
    /// "virtually instantiated" (mcd docs-mc 16-export-viz §6) and rendered as
    /// an IC with its physical pins; the fabricated instance name is hidden.
    #[test]
    fn build_viz_component_only_file() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let path = tmp_file(
            "comp",
            r#"
component HUM011D_5_S
{
    pins = [
        [1, [5,6,7]] = [VBUS, GND]::DC(5V)
    ]
}
"#,
        );
        let entry = path.to_string_lossy().into_owned();

        let resp = handle_build_viz(Some(json!({ "entry": entry }))).expect("build.viz ok");
        let html = resp["html"].as_str().expect("html field");
        assert!(
            html.contains("HUM011D_5_S"),
            "component class name must render, got {} bytes",
            html.len()
        );
        assert!(
            !html.contains("u_1"),
            "the fabricated instance name must be hidden"
        );
        assert_eq!(resp["top"], "HUM011D_5_S");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Multiple peer modules in one file are all rendered and combined.
    #[test]
    fn build_viz_multi_module_file() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let path = tmp_file("multi", "module BLINKER { }\nmodule BUZZER { }\n");
        let entry = path.to_string_lossy().into_owned();

        let resp = handle_build_viz(Some(json!({ "entry": entry }))).expect("build.viz ok");
        let html = resp["html"].as_str().expect("html field");
        assert!(html.contains("BLINKER"), "module label BLINKER must render");
        assert!(html.contains("BUZZER"), "module label BUZZER must render");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Regression: `build.full` (the "mcc build" envelope) on a component-only
    /// file must succeed with the component as top instead of E32107.
    #[test]
    fn build_full_component_only_file() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let path = tmp_file(
            "full-comp",
            "component HUM011D_5_S\n{\n    pins = [\n        1 = VBUS\n    ]\n}\n",
        );
        let entry = path.to_string_lossy().into_owned();

        let resp = run_full_build_envelope(&path, None, "mcc build", "file", "test", true)
            .expect("build.full ok");
        let pass2 = &resp["pass2"];
        assert_eq!(pass2["top"], "HUM011D_5_S");
        assert_eq!(
            resp["summary"]["errors"], 0,
            "component-only build must not error"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Regression: a file with both a module and a component resolves to the
    /// module (mcd docs-mc 16-export-viz §6). The module's component box in
    /// the block diagram must show its pin ids and names (not bare stubs).
    #[test]
    fn build_viz_module_contains_component_shows_pin_labels() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let path = tmp_file(
            "modcomp",
            r#"
component TLE7368(partno)
{
    pins = [
        in [1,18,19,36] = GNDA
        in 9 = EN_UC
        in 10 = EN_IGN
    ]
}

module TLE7368E(pwr, EN_IGN, EN_UC)
{
    TLE7368("TLE7368E") tle
}
"#,
        );
        let entry = path.to_string_lossy().into_owned();

        let resp = handle_build_viz(Some(json!({ "entry": entry }))).expect("build.viz ok");
        assert_eq!(resp["top"], "TLE7368E", "module target must win");
        let html = resp["html"].as_str().expect("html field");
        let svg = html.replace("\\\"", "\"");
        assert!(
            svg.contains("EN_UC"),
            "component pin name must render in the module block diagram"
        );
        assert!(
            svg.contains("GNDA"),
            "component pin name must render in the module block diagram"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
    /// Regression: tc275-style pin/func constructs used to SIGSEGV in the
    /// semantic layer because the C parser can emit AST nodes whose `.data`
    /// pointer is NULL (the guarded `data_as_cstr` now returns None instead
    /// of dereferencing NULL inside strlen).
    /// Regression: tc275-style pin/func constructs used to SIGSEGV in the
    /// semantic layer because the C parser can emit AST nodes whose `.data`
    /// pointer is NULL (the guarded `data_as_cstr` now returns None instead
    /// of dereferencing NULL inside strlen).
    #[test]
    fn build_viz_tc275_style_constructs() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let path = tmp_file(
            "tc275",
            r#"
component TC275
{
    pins = [
        101 = VSS
        [1:9] = P02[0:8]
        [102:103] = XTAL{X1,X2}
        [111:115] = JTAG{TDI,TMS,TDO,_TRST,TCK}
        [160:163] = P11[2:3,6,9]
        [84,86:88] = P32[0,2:4]
    ]
    func CapDigital(gnd)
    {
        CAP(100nF).Cap([this{10,24,68,100,123}, gnd])
        CAP(100nF).Cap([[pins{104,154}, pins.155, pins.164], gnd])
    }
    func HwReset()
    {
        PORST_OUT <- (R105 - _PORST) + q.d + (R106 - VEXT)
    }
}
"#,
        );
        let entry = path.to_string_lossy().into_owned();

        let resp = handle_build_viz(Some(json!({ "entry": entry }))).expect("build.viz ok");
        let html = resp["html"].as_str().expect("html field");
        assert_eq!(resp["top"], "TC275");
        assert!(!html.is_empty());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
