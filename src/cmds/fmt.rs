// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc fmt` — format `.mc` sources in place.
//!
//! The rewrite itself lives in [`mcc::fmt`]; this module is the file tree
//! walk, the in-place write and the exit code. Every file is decided on its
//! own: a source the formatter refuses is reported and skipped, never written
//! half way.
//!
//! `--rename` adds a style-gate rename pass ahead of the whitespace pass
//! (default off: a rename rewrites the token sequence, which the plain fmt
//! canon forbids). The edit set comes from the same derivation the QuickFix
//! channel serves ([`mcc::lsp::quickfix`]), so it only touches names whose
//! consumer set is provably complete; a file the engine refuses simply keeps
//! its names.

use anyhow::Result;
use mcc::cli::FmtArgs;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// One rename edit: byte span `[start, end)` in the file's original content,
/// the flagged spelling (for the report line) and the corrected one.
type RenameEdit = (usize, usize, String, String);

pub fn run(args: &FmtArgs) -> Result<ExitCode> {
    let target = resolve_target(args.target.as_deref())?;
    let files = collect_targets(&target)?;
    let quiet = mcc::cli::globals().quiet;

    // The rename pass needs the whole tree loaded before the first file is
    // decided (the workspace is process-global), so the edit sets are derived
    // up front and looked up per file below.
    let renames = if args.rename {
        Some(rename_edits(&files)?)
    } else {
        None
    };

    let mut changed = 0usize;
    let mut failed = 0usize;
    for path in files {
        let edits: Option<&Vec<RenameEdit>> =
            renames.as_ref().and_then(|m| m.get(&path));
        match format_file(&path, args.check, edits) {
            Ok(true) => {
                changed += 1;
                if !quiet {
                    let verb = if args.check {
                        "would format"
                    } else {
                        "formatted"
                    };
                    if let Some(edits) = edits {
                        for (pos, _, from, to) in edits {
                            let at = if args.check { "would rename" } else { "renamed" };
                            println!("{at} '{from}' to '{to}' at byte {pos} in {}", path.display());
                        }
                    }
                    println!("{} {}", verb, path.display());
                }
            }
            Ok(false) => {}
            Err(e) => {
                failed += 1;
                eprintln!("error: {}: {}", path.display(), e);
            }
        }
    }

    if failed > 0 {
        return Ok(ExitCode::from(2));
    }
    if args.check && changed > 0 {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// An omitted target is the current directory. Unlike the definition-space
/// commands, no `project.toml` is required: the target is a file tree, and
/// `cd src && mcc fmt` is the common case (fmt-design.md §6).
fn resolve_target(target: Option<&str>) -> Result<PathBuf> {
    let path = match target {
        Some(t) => PathBuf::from(t),
        None => std::env::current_dir()?,
    };
    if !path.exists() {
        anyhow::bail!("fmt: target not found: {}", path.display());
    }
    Ok(path)
}

fn collect_targets(target: &Path) -> Result<Vec<PathBuf>> {
    if target.is_file() {
        if target.extension().is_some_and(|e| e == "mc") {
            return Ok(vec![target.to_path_buf()]);
        }
        anyhow::bail!("fmt: not a `.mc` file: {}", target.display());
    }
    Ok(mcc::mcc_collect_mc_files(target))
}

/// Derive the rename edit set for every target file: load the whole tree into
/// the workspace, build each file's module, then run the QuickFix derivation
/// per diagnostic. A file that fails to load or build contributes no edits —
/// its style diagnostics still fire, only the auto-rename is skipped, and a
/// name with unindexable consumers is left as-is by the derivation itself.
fn rename_edits(files: &[PathBuf]) -> Result<HashMap<PathBuf, Vec<RenameEdit>>> {
    mcc::mcc_clear_workspace();
    let mut uris: HashMap<PathBuf, String> = HashMap::new();
    for path in files {
        let content = std::fs::read_to_string(path)?;
        // The workspace addresses sources by absolute URI: a relative CLI
        // target loads fine but `mcc_diagnose` finds nothing under it.
        let uri = path
            .canonicalize()
            .unwrap_or_else(|_| path.clone())
            .to_string_lossy()
            .to_string();
        mcc::mcc_load_from_string(&mcc::McURI::from(uri.as_str()), &content);
        uris.insert(path.clone(), uri);
    }
    // Build each file's module so pass1 fills the def/ref tables the edit
    // derivation joins on; build failures are not fmt's business to report.
    for uri in uris.values() {
        if let Some(top) = mcc::mcb_get_module_name_by_uri(&mcc::McURI::from(uri.as_str())) {
            let ident = mcc::McIds::from(top.as_str());
            let _ = mcc::mcc_build(&ident, &mcc::McURI::from(uri.as_str()));
        }
    }
    mcc::mcb_parse_all_modules();

    let mut out: HashMap<PathBuf, Vec<RenameEdit>> = HashMap::new();
    for (path, uri) in &uris {
        for d in mcc::mcc_diagnose(&mcc::McURI::from(uri.as_str())) {
            let Some(fix) = mcc::lsp::quickfix::fix_payload(&d) else {
                continue;
            };
            let Some(edits) = fix["edits"].as_array() else {
                continue;
            };
            for e in edits {
                // Net labels are module-local today, so every edit names the
                // flagged file; a foreign file would mean the derivation
                // outgrew this face — skip rather than apply blindly. Compare
                // against the diagnostic's own URI spelling: the engine may
                // canonicalize the path we loaded (`/tmp` → `/private/tmp`).
                if e["file"].as_str() != Some(d.loc.uri.as_str()) {
                    continue;
                }
                let (Some(pos), Some(len), Some(from), Some(to)) = (
                    e["pos"].as_u64(),
                    e["len"].as_u64(),
                    d.msg.split('\'').nth(1).map(str::to_string),
                    e["replacement"].as_str().map(str::to_string),
                ) else {
                    continue;
                };
                out.entry(path.clone())
                    .or_default()
                    .push((pos as usize, len as usize, from, to));
            }
        }
    }
    Ok(out)
}

/// Format one file. `Ok(true)` means it was reformatted (or, under `--check`,
/// would have been). `edits` (from `--rename`) are applied to the original
/// content first — their byte spans are only valid there; the whitespace pass
/// runs on the renamed text.
fn format_file(path: &Path, check: bool, edits: Option<&Vec<RenameEdit>>) -> Result<bool> {
    let content = std::fs::read_to_string(path)?;
    let renamed = match edits {
        Some(edits) if !edits.is_empty() => Some(apply_edits(&content, edits)),
        _ => None,
    };
    let base = renamed.as_deref().unwrap_or(&content);
    let formatted = mcc::fmt::format_text(base).map_err(|e| anyhow::anyhow!("{e}"))?;
    let result = if formatted == base { renamed } else { Some(formatted) };
    match result {
        None => Ok(false),
        Some(text) => {
            if !check {
                std::fs::write(path, &text)?;
            }
            Ok(true)
        }
    }
}

/// Apply rename edits to their exact byte spans, descending so earlier
/// offsets survive the rewrites.
fn apply_edits(content: &str, edits: &[RenameEdit]) -> String {
    let mut sorted: Vec<&RenameEdit> = edits.iter().collect();
    sorted.sort_by_key(|(start, _, _, _)| std::cmp::Reverse(*start));
    let mut text = content.to_string();
    for &(start, len, _, ref replacement) in sorted {
        let end = (start + len).min(text.len());
        if start > end || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        text.replace_range(start..end, &replacement);
    }
    text
}
