// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared helpers for local CLI commands: unified target loading (file or
//! directory project mode), top-module resolution, and guarded Pass2 builds.

use crate::cmds::manifest;
use mcc::vector::model::trunk::{TrunkCtx, TrunkKind};
use std::path::{Path, PathBuf};

/// Load the CLI target into the engine and return the entry URI plus the
/// resolved top module when the target kind supplies one.
///
/// - File target: loaded directly; no top is implied.
/// - Directory target: project mode — `project.toml` drives the entry,
///   dependency libraries and top module ([`manifest::build_from_manifest`]);
///   without a usable manifest, browse mode selects the unique `module main`
///   entry ([`manifest::select_browse_entry`]). The returned top honors
///   `--top` / `--entry` and falls back to the browse entry's module.
pub fn load_target(
    target: Option<&str>,
    cli_top: Option<&str>,
    cli_entry: Option<&str>,
) -> anyhow::Result<(String, Option<String>)> {
    let Some(t) = target else {
        return Ok((String::new(), None));
    };
    let p = Path::new(t);
    if p.is_dir() {
        // Absolutize the root like `build` does: a relative root would make
        // the manifest entry URI relative too, and the module registry keys
        // on the canonical URI, so the top-module lookup would come up empty.
        let root = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(p)
        };
        match manifest::build_from_manifest(&root, cli_top, cli_entry) {
            Ok((entry_uri, top)) => Ok((entry_uri, Some(top))),
            Err(manifest_err) => {
                let entry_path =
                    manifest::select_browse_entry(&root, cli_entry).map_err(|browse_err| {
                        anyhow::anyhow!("{} (manifest: {:#})", browse_err, manifest_err)
                    })?;
                mcc::mcc_set_project_root(&root);
                let entry_uri = entry_path.to_string_lossy().to_string();
                mcc::mcc_load_project(&entry_uri);
                let top = cli_top
                    .map(str::to_string)
                    .or_else(|| mcc::mcb_get_module_name_by_uri(&entry_uri));
                Ok((entry_uri, top))
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
        let entry_uri = path.to_string_lossy().to_string();
        mcc::mcc_load_project(&entry_uri);
        Ok((entry_uri, None))
    }
}

/// Resolve the top module: an explicit top (manifest top_module, or `--top` /
/// `--entry` override) first, else the module declared in the entry file,
/// else the first loaded module.
pub fn resolve_top_module(entry_uri: &str, explicit_top: Option<String>) -> Option<String> {
    explicit_top
        .or_else(|| mcc::cli::globals().top.clone())
        .or_else(|| mcc::mcb_get_module_name_by_uri(&entry_uri.to_string()))
        .or_else(mcc::mcb_get_first_module_name)
}

/// Run Pass2 for `top` in `uri`, converting an engine panic into an error so
/// a Pass2 bug cannot abort the CLI process.
pub fn build_pass2(top: &str, uri: &str) -> Result<mcc::McModuleInst, String> {
    let ident = mcc::McIds::from(top);
    let mcc_uri = mcc::McURI::from(uri);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build(&ident, &mcc_uri)
    })) {
        Ok(Ok(inst)) => Ok(inst),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".to_string()),
    }
}

/// Like [`build_pass2`], but also returns the Phase D frozen string net-table
/// store so the caller can read the tree-level string nets (`McModuleInst`
/// never carries `NetPoint`).
pub fn build_pass2_with_nets(
    top: &str,
    uri: &str,
) -> Result<(mcc::McModuleInst, mcc::NetTableStore), String> {
    let ident = mcc::McIds::from(top);
    let mcc_uri = mcc::McURI::from(uri);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build_with_nets(&ident, &mcc_uri)
    })) {
        Ok(Ok(pair)) => Ok(pair),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".to_string()),
    }
}

/// Like [`build_pass2`], but also returns the Phase C companion arena so the
/// tree-rendering consumer can walk the arena `children` edges instead of the
/// tree's `sub_modules` Vec (design §4, plan §9 C item 3), plus the Phase D
/// frozen string net-table store for the tree-level net consumers.
pub fn build_pass2_with_arena(
    top: &str,
    uri: &str,
) -> Result<(mcc::McModuleInst, mcc::NodeArena, mcc::NetTableStore), String> {
    let ident = mcc::McIds::from(top);
    let mcc_uri = mcc::McURI::from(uri);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build_with_arena(&ident, &mcc_uri)
    })) {
        Ok(Ok(triple)) => Ok(triple),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".to_string()),
    }
}

// ============================================================================
// §8.9.5 layered connection rendering (shared by `show dianlu` and `verify`)
// ============================================================================

/// One connection row for layered rendering. `dir` is the source direction
/// tag (`"LtoR"` / `"RtoL"` / anything else = undirected, the `{:?}` form of
/// `ConnDir`); `trunk` is the structured group context (§8.9.6) decided
/// at the AST layer — `None` (or `kind == Plain`) for plain connections,
/// `Some` with a member name for bus/interface member lanes.
pub struct ConnView {
    pub net: String,
    pub points: Vec<String>,
    pub dir: String,
    pub trunk: Option<TrunkCtx>,
}

/// Join connection endpoints with the separator that reflects the source
/// connector direction: `->` (LtoR), `<-` (RtoL), `-` (undirected).
fn join_conn_points(points: &[String], dir: &str) -> String {
    let sep = match dir {
        "LtoR" => " -> ",
        "RtoL" => " <- ",
        _ => " - ",
    };
    points.join(sep)
}

/// §8.9.5 layered connection rendering.
///
/// Bus/interface member lanes carry a structured group context with
/// `kind != Plain` and a resolved `member` (§8.9.6) and are grouped into
/// coarse trunks by the group `name`: each trunk renders one header line
/// (`[bus] SPI0`) followed by one indented member line per member lane.
/// Connections without a group context (scalar labels / plain pins like
/// `V3V3`, `GND`, `1`) render as single lines, preserving the flat view.
/// `indent` prefixes every trunk / plain line (member lines get one level
/// more).
pub fn render_layered_conns(conns: &[ConnView], indent: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    // First-seen ordered trunks: (name, kind, [(member, points, dir)])
    let mut trunks: Vec<(String, TrunkKind, Vec<(String, Vec<String>, String)>)> = Vec::new();
    for c in conns {
        match &c.trunk {
            Some(pg) if pg.kind != TrunkKind::Plain && pg.member.is_some() => {
                let base = pg.name.clone().unwrap_or_else(|| c.net.clone());
                let member = pg.member.clone().unwrap_or_else(|| c.net.clone());
                match trunks.iter_mut().find(|(b, _, _)| *b == base) {
                    Some((_, _, members)) => {
                        members.push((member, c.points.clone(), c.dir.clone()));
                    }
                    None => trunks.push((
                        base,
                        pg.kind,
                        vec![(member, c.points.clone(), c.dir.clone())],
                    )),
                }
            }
            _ => lines.push(format!(
                "{indent}{} : {}",
                c.net,
                join_conn_points(&c.points, &c.dir)
            )),
        }
    }
    for (base, kind, members) in trunks {
        lines.push(format!("{indent}[{}] {base}", kind.label()));
        for (member, points, dir) in members {
            lines.push(format!(
                "{indent}  {member:<8} : {}",
                join_conn_points(&points, &dir)
            ));
        }
    }
    lines
}
