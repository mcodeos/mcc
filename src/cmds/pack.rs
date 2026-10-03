// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib pack` / `mcc lib inspect` — the device-pack local loop (registry-design.md §3:
//! `.mcl` = a tar stream flowing into a single zstd stream; thin = manifest + entry, full =
//! manifest + entry + all bundled attachments). The `.mcl` install face lives in the library
//! crate (`db::infra::packinst`) so the solver and RPC share the one unpacking entry; the
//! names are re-exported here for the CLI callers.
//!
//! Everything runs offline in-process, never over RPC (the parse U90 precedent: offline
//! tools have no daemon face).
//! The pack gate = no-compile-no-pack: the in-process check pipeline runs the entry; any E
//! error refuses.

use crate::cmds::{check, manifest};
use crate::output;
use anyhow::{anyhow, Context, Result};
use mcc::cli::{packfile, CheckArgs, OutputFormat};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub use mcc::{install_mcl_at, read_archive, sha256_hex_file};

// Reports

#[derive(Serialize)]
pub struct PackReport {
    pub name: String,
    pub version: String,
    pub category: String,
    pub entry: String,
    /// Full-tier file set (thin tier = manifest + entry + README; not repeated in the report).
    pub files: Vec<String>,
    pub full: ArtifactInfo,
    pub thin: ArtifactInfo,
}

#[derive(Serialize)]
pub struct ArtifactInfo {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct InspectReport {
    pub file: String,
    /// Every file name in the archive (with paths).
    pub files: Vec<String>,
    pub pack: packfile::PackToml,
}

impl std::fmt::Display for PackReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Pack: {}@{} ({})", self.name, self.version, self.category)?;
        writeln!(f, "  entry: {}", self.entry)?;
        for rel in &self.files {
            writeln!(f, "  file:  {}", rel)?;
        }
        writeln!(f, "  full: {} ({} bytes, sha256 {})", self.full.path, self.full.bytes, self.full.sha256)?;
        writeln!(f, "  thin: {} ({} bytes, sha256 {})", self.thin.path, self.thin.bytes, self.thin.sha256)?;
        Ok(())
    }
}

impl std::fmt::Display for InspectReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p = &self.pack.package;
        writeln!(f, "Archive: {}", self.file)?;
        writeln!(f, "  format: {}  name: {}  version: {}", p.format, p.name, p.version)?;
        writeln!(f, "  category: {}  entry: {}", p.category, p.entry)?;
        if let Some(v) = &p.vendor {
            writeln!(f, "  vendor: {}", v)?;
        }
        if let Some(publ) = &p.publisher {
            writeln!(f, "  publisher: {}", publ)?;
        }
        if let Some(rm) = &p.readme {
            writeln!(f, "  readme: {}", rm)?;
        }
        if !p.keywords.is_empty() {
            writeln!(f, "  keywords: {}", p.keywords.join(", "))?;
        }
        if let Some(d) = &p.description {
            writeln!(f, "  description: {}", d)?;
        }
        for (dep, req) in &self.pack.dependencies {
            writeln!(f, "  dep: {} {}", dep, req)?;
        }
        for (vname, v) in &self.pack.variants {
            writeln!(f, "  variant: {} base={} since={}", vname, v.base, v.since)?;
        }
        for att in &self.pack.attachments {
            match (&att.path, &att.url) {
                (Some(path), _) => writeln!(
                    f,
                    "  attachment: {} kind={}{}{}",
                    path,
                    att.kind,
                    att.rev.as_deref().map(|r| format!(" rev={}", r)).unwrap_or_default(),
                    att.checksum.as_deref().map(|c| format!(" checksum={}", c)).unwrap_or_default()
                )?,
                _ => writeln!(
                    f,
                    "  linked: {} kind={}{}",
                    att.url.as_deref().unwrap_or("?"),
                    att.kind,
                    att.rev.as_deref().map(|r| format!(" rev={}", r)).unwrap_or_default()
                )?,
            }
        }
        for name in &self.files {
            writeln!(f, "  file: {}", name)?;
        }
        Ok(())
    }
}

// pack

pub fn cmd_pack(dir: &str, out: Option<&str>, format: OutputFormat) -> Result<()> {
    let report = do_pack(dir, out)?;
    eprintln!(
        "✓ packed {} {} → {} (+ thin {})",
        report.name,
        report.version,
        report.full.path,
        report.thin.path
    );
    output::emit(&report, format, None)
}

/// Pure pack: validate → check gate → file sets → dual `.mcl` artifacts. Shared CLI/RPC semantic
/// face.
pub fn do_pack(dir: &str, out: Option<&str>) -> Result<PackReport> {
    let root = PathBuf::from(dir);
    if !root.is_dir() {
        anyhow::bail!("lib pack: '{}' is not a directory", dir);
    }
    let root = root.canonicalize().context("lib pack: failed to resolve the pack directory")?;
    let pack = packfile::load(&root).context("pack gate: manifest validation failed")?;
    let entry_path = root.join(&pack.package.entry);

    // No-compile-no-pack: the in-process check gate (U90: check is already an in-process pipeline,
    // no daemon branch).
    let args = CheckArgs {
        target: Some(entry_path.to_string_lossy().into_owned()),
        dlog: false,
        errors_only: true,
        nets: false,
        pins: false,
        ledger: None,
    };
    // No-compile-no-pack: the in-process check gate (U90: check is already an in-process pipeline,
    // no daemon branch). A fatal run — either an `Err` bail or an exit-2 outcome — stopped before
    // it could render rows: the common shape is a C-parser failure that yields no AST, hence no
    // top module, and a build-stage message that blames the manifest ("cannot find top-level
    // module") instead of the source. Both paths re-run the load face and surface the parse rows
    // when the entry itself does not parse; a clean parse means check's own fatal stands.
    let outcome = match check::run(&args) {
        Ok(outcome) => outcome,
        Err(e) => {
            return Err(
                parse_error_rows(&entry_path).map_or_else(
                    || e.context("pack gate: check pipeline failure"),
                    |rows| {
                        anyhow!(
                            "pack gate: entry {} does not parse: {}; no pack emitted",
                            pack.package.entry,
                            rows
                        )
                    },
                )
            );
        }
    };
    if outcome.exit_code != 0 {
        if outcome.exit_code == 2 {
            if let Some(rows) = parse_error_rows(&entry_path) {
                anyhow::bail!(
                    "pack gate: entry {} does not parse: {}; no pack emitted",
                    pack.package.entry,
                    rows
                );
            }
        }
        anyhow::bail!(
            "pack gate: entry {} does not compile (diagnostics above); no pack emitted",
            pack.package.entry
        );
    }

    // Variant base shallow check: a declared base must be present on the entry source face
    // (semantic binding in a later batch).
    let source = std::fs::read_to_string(&entry_path)
        .with_context(|| format!("failed to read entry: {}", entry_path.display()))?;
    for (vname, v) in &pack.variants {
        let pat = format!(r"\b{}\b", regex::escape(v.base.as_str()));
        let re = regex::Regex::new(&pat)?;
        if !re.is_match(&source) {
            anyhow::bail!(
                "variants `{}` base `{}` is not present on the entry source face",
                vname,
                v.base
            );
        }
    }

    // Bundled attachments: presence + sha256 audit (a manifest-declared checksum is verified at
    // pack time).
    for att in &pack.attachments {
        if let Some(p) = &att.path {
            let ap = root.join(p);
            if !ap.is_file() {
                anyhow::bail!("bundled attachment is missing: {}", p);
            }
            if let Some(sum) = &att.checksum {
                let actual = format!("sha256:{}", sha256_hex_file(&ap)?);
                if &actual != sum {
                    anyhow::bail!(
                        "attachment {} checksum mismatch: manifest {} != actual {}",
                        p,
                        sum,
                        actual
                    );
                }
            }
        }
    }

    // File sets: thin = manifest + entry (+ readme pointer / README convention); full = thin + all
    // bundled attachments.
    let mut thin_files: Vec<(PathBuf, String)> = vec![
        (root.join("pack.toml"), "pack.toml".to_string()),
        (entry_path.clone(), pack.package.entry.clone()),
    ];
    let mut push_readme = |rel: &str, thin: &mut Vec<(PathBuf, String)>| {
        let rp = root.join(rel);
        if rp.is_file() && !thin.iter().any(|(_, r)| r == rel) {
            thin.push((rp, rel.to_string()));
        }
    };
    if let Some(rm) = &pack.package.readme {
        push_readme(rm, &mut thin_files);
    }
    for readme in ["README.md", "README"] {
        push_readme(readme, &mut thin_files);
    }
    let mut full_files = thin_files.clone();
    for att in &pack.attachments {
        if let Some(p) = &att.path {
            let cand = (root.join(p), p.clone());
            if !full_files.contains(&cand) {
                full_files.push(cand);
            }
        }
    }

    // Artifacts: `<name>-<ver>.mcl` + `<name>-<ver>.thin.mcl`, defaulting into the pack directory
    // itself.
    let out_dir = out.map(PathBuf::from).unwrap_or_else(|| root.clone());
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create the output directory: {}", out_dir.display()))?;
    let base = format!("{}-{}", pack.package.name, pack.package.version);
    let full_path = out_dir.join(format!("{}.mcl", base));
    let thin_path = out_dir.join(format!("{}.thin.mcl", base));
    write_archive(&full_path, &full_files)?;
    write_archive(&thin_path, &thin_files)?;

    Ok(PackReport {
        name: pack.package.name.clone(),
        version: pack.package.version.clone(),
        category: pack.package.category.clone(),
        entry: pack.package.entry.clone(),
        files: full_files
            .iter()
            .map(|(_, rel)| rel.clone())
            .collect(),
        full: artifact_info(&full_path)?,
        thin: artifact_info(&thin_path)?,
    })
}

/// Parse-face attribution for a fatal check run: load the entry on its own and
/// return the error rows in the dlog line form (`file:row:col: error[Ecode]: msg`),
/// or `None` when the entry parses clean — the fatal then belongs to the world,
/// not the source.
fn parse_error_rows(entry: &Path) -> Option<String> {
    manifest::init_local(Some(entry.to_str()?), &mcc::cli::globals().lib);
    let uri = entry.to_string_lossy().into_owned();
    mcc::mcc_load_project(&uri);
    let rows: Vec<String> = mcc::mcc_diagnose_all()
        .iter()
        .filter(|d| matches!(d.level, mcc::DiagnosticLevel::Error))
        .map(|d| {
            format!(
                "{}:{}:{}: error[E{:04}]: {}",
                d.loc.uri.as_str(),
                d.loc.row,
                d.loc.col,
                d.code,
                d.msg
            )
        })
        .collect();
    if rows.is_empty() { None } else { Some(rows.join("; ")) }
}

/// tar stream into a single zstd stream (§3.4 container shape); paths use pack-relative names.
fn write_archive(path: &Path, files: &[(PathBuf, String)]) -> Result<()> {
    let f = std::fs::File::create(path)
        .with_context(|| format!("failed to create the archive: {}", path.display()))?;
    let enc = zstd::stream::Encoder::new(f, 3)?;
    let mut tar = tar::Builder::new(enc);
    for (abs, rel) in files {
        tar.append_path_with_name(abs, rel)
            .with_context(|| format!("failed to archive: {}", rel))?;
    }
    tar.into_inner()?.finish()?;
    Ok(())
}

fn artifact_info(path: &Path) -> Result<ArtifactInfo> {
    Ok(ArtifactInfo {
        path: path.to_string_lossy().into_owned(),
        sha256: sha256_hex_file(path)?,
        bytes: path.metadata()?.len(),
    })
}

// inspect

pub fn cmd_inspect(file: &str, format: OutputFormat) -> Result<()> {
    let report = do_inspect(file)?;
    output::emit(&report, format, None)
}

/// Pure inspection: unpack, read the manifest, emit an offline report — no disk writes, no network.
pub fn do_inspect(file: &str) -> Result<InspectReport> {
    let files = read_archive(Path::new(file))?;
    let text = files
        .get("pack.toml")
        .ok_or_else(|| anyhow!("no pack.toml in the archive"))?;
    let pack: packfile::PackToml =
        toml::from_str(std::str::from_utf8(text).context("pack.toml is not UTF-8")?)
            .context("failed to parse the archive pack.toml")?;
    // Structural validation (format gate included); entry presence holds for thin archives and is
    // required for full ones too.
    packfile::validate_structure(&pack)?;
    if !files.contains_key(pack.package.entry.as_str()) {
        anyhow::bail!("entry `{}` is not present in the archive", pack.package.entry);
    }
    let mut names: Vec<String> = files.keys().cloned().collect();
    names.sort();
    Ok(InspectReport {
        file: file.to_string(),
        files: names,
        pack,
    })
}

// install (.mcl path) — the implementation lives in `db::infra::packinst` (re-exported above)
