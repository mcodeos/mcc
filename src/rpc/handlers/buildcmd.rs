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
    // don't leak into this build's definitions or phase snapshots. The per-world
    // registry and workspace reset together (`mcc_clear_workspace`), matching
    // how libs are re-loaded below.
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
            crate::semantic::validation::ledger::LedgerMode::from_flag(p.ledger.as_deref()),
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
        crate::semantic::validation::ledger::LedgerMode::from_flag(p.ledger.as_deref()),
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

    // Batch-install the synthetic wrapper modules ONCE so the per-target loop
    // below does not re-read + re-parse the whole file for each virtual target
    // (a component library like mclibs/digital/74ahc.mc would otherwise pay
    // N × full-file re-parse for N parts).
    crate::mcc_virtual_install_synthetic_views(&targets, &mc_uri)
        .map_err(|e| JsonRpcError::custom(32107, &e.to_string()))?;

    let mut svgs: Vec<(Option<String>, String)> = Vec::new();
    let mut single_doc: Option<crate::viz::doc::VizDocument> = None;
    let mut top_name = String::new();
    for target in &targets {
        // Flatten (Pass2 + InstTable) with panic guard, mirroring execute_pass2.
        let t2 = std::time::Instant::now();
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::mcc_virtual_build_flat(target, &mc_uri, 1000)
        }));
        let (_inst, table, arena, store) = match built {
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
        let (vec_block, _report) = crate::build_mc_vec_with_report(&_inst, &table, &arena, &store);
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
            // Virtual (component/interface) targets get no heading in the
            // combined view — the wrapper module's name is fabrication and its
            // IC box already shows the class label.
            let label = if is_virtual_target {
                None
            } else {
                Some(target.clone())
            };
            svgs.push((label, root_layer.svg.clone()));
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
        let root_name = crate::viz::template::combined_view_name(&mc_uri);
        let mut doc = crate::viz::doc::VizDocument::new(1000, root_name.clone());
        let mut layer = crate::viz::layer::VizLayer::new(1000, root_name, None);
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
    fn cli_buildcmd__build_viz_component_only_file() {
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

    /// Regression: a two-pin passive (resistor) in a virtual component view
    /// must not leak its fabricated instance name (`u_1`) as the designator
    /// label — the resistor symbol renders with the class name instead.
    #[test]
    fn cli_buildcmd__build_viz_two_pin_passive_hides_u1() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let path = tmp_file(
            "twopin",
            r#"
component RES
{
    pins = [
        1 = 1, "Term 1"
        2 = 2, "Term 2"
    ]
}
"#,
        );
        let entry = path.to_string_lossy().into_owned();

        let resp = handle_build_viz(Some(json!({ "entry": entry }))).expect("build.viz ok");
        let html = resp["html"].as_str().expect("html field");
        assert!(
            !html.contains("u_1"),
            "fabricated instance name must be hidden"
        );
        // The resistor symbol's zigzag path (JSON-escaped in the embedded SVG,
        // hence the escaping-agnostic `miter` probe).
        assert!(html.contains("miter"), "the resistor zigzag must render");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Multiple peer modules in one file are all rendered and combined.
    #[test]
    fn cli_buildcmd__build_viz_multi_module_file() {
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

    /// Directory batch mode (unified principle, use-design §19.5 rule 3): a
    /// no-toml folder passed to `build.full` recurses over every `.mc` file
    /// (incl. subfolders), Pass1 covers all files, Pass2 builds each file's
    /// default top, and the envelope carries the first successful tree.
    #[test]
    fn cli_buildcmd__build_full_directory_batch() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let root = std::env::temp_dir().join(format!("mcc-dirbatch-{}", std::process::id()));
        let sub = root.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("a.mc"), "module main\n{\n}\n").unwrap();
        std::fs::write(
            sub.join("b.mc"),
            "component res\n{\n    Pin A, B;\n}\nmodule top2\n{\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("c.mc"), "component cap\n{\n    Pin A, B;\n}\n").unwrap();

        let resp = run_full_build_envelope(
            &root,
            None,
            "mcc build",
            "file",
            "test",
            true,
            crate::semantic::validation::ledger::LedgerMode::Summary,
        )
        .expect("build.full dir ok");

        // All three files (incl. the subfolder one) loaded in pass1.
        let loaded = resp["pass1"]["loaded_files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["is_system"] == false)
            .map(|f| f["uri"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            loaded.len(),
            3,
            "every .mc file under the folder must load: {loaded:?}"
        );
        assert!(
            loaded.iter().any(|u| u.ends_with("sub/b.mc")),
            "subfolder file must be included: {loaded:?}"
        );
        // a.mc `main` + sub/b.mc `top2` (c.mc defines only a component).
        assert_eq!(resp["summary"]["module_count"], 2);

        // Pass2: first file's default top; the component-only file is
        // virtualized per-file, not an error that aborts the report.
        assert_eq!(resp["pass2"]["top"], "main");
        assert!(resp["pass2"]["instances"]["name"] == "main");

        std::fs::remove_dir_all(&root).ok();
    }

    /// Regression (world-core Stage D): the directory-batch envelope is an
    /// OWNING surface too — building a toml-less folder must aggregate the real
    /// flat net-check ERC of every built file, not just pass-1 diagnostics. The
    /// old dir path built each file tree-only (`mcc_virtual_build_with_nets`, no
    /// flatten, no ERC), so Build Project on a folder under-reported exactly
    /// like the old single-file build.full did.
    #[test]
    fn cli_buildcmd__build_full_directory_batch_reports_net_erc_truth() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let root = std::env::temp_dir().join(format!("mcc-direrc-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        // First walked file carries a shorted net → E4101.
        std::fs::write(
            root.join("a.mc"),
            "component BUF {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n\
             module main {\n    BUF b1\n    BUF b2\n    b1.Y -> b2.Y\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("c.mc"), "module other\n{\n}\n").unwrap();

        let resp = run_full_build_envelope(
            &root,
            None,
            "mcc build",
            "file",
            "test",
            true,
            crate::semantic::validation::ledger::LedgerMode::Summary,
        )
        .expect("build.full dir ok");

        let codes = resp["pass2"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|d| d["code"].as_u64().map(|c| c as u32))
            .collect::<Vec<_>>();
        assert!(
            codes.contains(&4101),
            "dir build.full must carry the flat E4101 ...: {codes:?}"
        );
        let erc4101 = resp["pass2"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == 4101)
            .unwrap();
        assert!(
            erc4101["location"]["file"]
                .as_str()
                .unwrap()
                .ends_with("a.mc"),
            "ERC must be located at the source file that owns the shorted net"
        );
        assert!(
            resp["summary"]["warnings"].as_u64().unwrap_or(0)
                + resp["summary"]["errors"].as_u64().unwrap_or(0)
                >= 1,
            "summary must weight the aggregated ERC"
        );

        std::fs::remove_dir_all(&root).ok();
    }

    /// Regression: `build.full` (the "mcc build" envelope) on a component-only
    /// file must succeed with the component as top instead of E32107.
    #[test]
    fn cli_buildcmd__build_full_component_only_file() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        let path = tmp_file(
            "full-comp",
            "component HUM011D_5_S\n{\n    pins = [\n        1 = VBUS\n    ]\n}\n",
        );
        let resp = run_full_build_envelope(
            &path,
            None,
            "mcc build",
            "file",
            "test",
            true,
            crate::semantic::validation::ledger::LedgerMode::Summary,
        )
        .expect("build.full ok");
        let pass2 = &resp["pass2"];
        assert_eq!(pass2["top"], "HUM011D_5_S");
        assert_eq!(
            resp["summary"]["errors"], 0,
            "component-only build must not error"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Regression (world-core Stage C): `build.full` is an OWNING surface — the
    /// envelope must report the real flat electrical net checks, not a pass-1-only
    /// warning count. The old tree-only build (`mcc_virtual_build_with_nets`)
    /// never flattened, so `mcc build`/buildProject showed 3 warnings while the
    /// open-file viz flood showed dozens. The envelope now instantiates the top
    /// once into a CircuitWorld, flattens it once, logs the returned net
    /// diagnostics into the pass2 bucket, and summarizes them.
    #[test]
    fn cli_buildcmd__build_full_reports_net_erc_truth() {
        let _guard = parse_lock();
        crate::mcc_init_no_lib();
        crate::mcc_set_system_root(std::path::Path::new(""));
        crate::mcc_clear_workspace();

        // Two buffer outputs merged on one net → E4101 driver conflict (plus
        // companion warnings). The 4101 proves flatten's net checks ran and the
        // ERC reached the envelope's pass2 diagnostics + summary.
        let path = tmp_file(
            "full-erc",
            "component BUF {\n    pins = [\n        in 1 = A\n        out 2 = Y\n    ]\n}\n\
             module main {\n    BUF b1\n    BUF b2\n    b1.Y -> b2.Y\n}\n",
        );
        let resp = run_full_build_envelope(
            &path,
            None,
            "mcc build",
            "file",
            "test",
            true,
            crate::semantic::validation::ledger::LedgerMode::Summary,
        )
        .expect("build.full ok");
        let p2 = &resp["pass2"];
        let diags = p2["diagnostics"].as_array().unwrap();
        let codes: Vec<u32> = diags
            .iter()
            .filter_map(|d| d["code"].as_u64().map(|c| c as u32))
            .collect();
        assert!(
            codes.contains(&4101),
            "build.full must carry the flat E4101 driver-conflict ERC; got codes {codes:?}"
        );
        assert!(
            resp["summary"]["warnings"].as_u64().unwrap_or(0)
                + resp["summary"]["errors"].as_u64().unwrap_or(0)
                >= codes.len() as u64,
            "summary must weight the pass2 net diagnostics"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Regression: a file with both a module and a component resolves to the
    /// module (mcd docs-mc 16-export-viz §6). The module's component box in
    /// the block diagram must show its pin ids and names (not bare stubs).
    #[test]
    fn cli_buildcmd__build_viz_module_contains_component_shows_pin_labels() {
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
    fn cli_buildcmd__build_viz_tc275_style_constructs() {
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
