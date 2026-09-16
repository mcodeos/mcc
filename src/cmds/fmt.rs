// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc fmt` — format `.mc` sources in place.
//!
//! The rewrite itself lives in [`mcc::fmt`]; this module is the file tree
//! walk, the in-place write and the exit code. Every file is decided on its
//! own: a source the formatter refuses is reported and skipped, never written
//! half way.

use anyhow::Result;
use mcc::cli::FmtArgs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn run(args: &FmtArgs) -> Result<ExitCode> {
    let target = resolve_target(args.target.as_deref())?;
    let files = collect_targets(&target)?;
    let quiet = mcc::cli::globals().quiet;

    let mut changed = 0usize;
    let mut failed = 0usize;
    for path in files {
        match format_file(&path, args.check) {
            Ok(true) => {
                changed += 1;
                if !quiet {
                    let verb = if args.check {
                        "would format"
                    } else {
                        "formatted"
                    };
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

/// Format one file. `Ok(true)` means it was reformatted (or, under `--check`,
/// would have been).
fn format_file(path: &Path, check: bool) -> Result<bool> {
    let content = std::fs::read_to_string(path)?;
    let formatted = mcc::fmt::format_text(&content).map_err(|e| anyhow::anyhow!("{e}"))?;
    if formatted == content {
        return Ok(false);
    }
    if !check {
        std::fs::write(path, &formatted)?;
    }
    Ok(true)
}
