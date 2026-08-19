// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared helpers for local CLI commands: unified target loading (file or
//! directory project mode), top-module resolution, and guarded Pass2 builds.

use crate::cmds::manifest;
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
        match manifest::build_from_manifest(p, cli_top, cli_entry) {
            Ok((entry_uri, top)) => Ok((entry_uri, Some(top))),
            Err(manifest_err) => {
                let entry_path =
                    manifest::select_browse_entry(p, cli_entry).map_err(|browse_err| {
                        anyhow::anyhow!("{} (manifest: {:#})", browse_err, manifest_err)
                    })?;
                mcc::mcc_set_project_root(p);
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
