// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib pack` / `mcc lib inspect` + the `.mcl` install path — the device-pack local loop
//! (registry-design.md §3: `.mcl` = a tar stream flowing into a single zstd stream; the
//! frame magic `28 B5 2F FD` is the format fingerprint; thin = manifest + entry, full =
//! manifest + entry + all bundled attachments).
//!
//! Everything runs offline in-process, never over RPC (the parse U90 precedent: offline
//! tools have no daemon face).
//! The pack gate = no-compile-no-pack: the in-process check pipeline runs the entry; any E
//! error refuses.

use crate::cmds::check;
use crate::output;
use anyhow::{anyhow, Context, Result};
use mcc::cli::{datadir, packfile, CheckArgs, OutputFormat};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The zstd frame magic (RFC 8878 §3.1.1) — the `.mcl` format fingerprint, identified by bytes at
/// install time, never by extension.
pub const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

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
    let outcome = check::run(&args).context("pack gate: check pipeline failure")?;
    if outcome.exit_code != 0 {
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

// install (.mcl path)

/// `.mcl` install: unpack → install-time three checks + format gate → staging into
/// `data_root/<name>@<ver>/`
/// → rebuild_index. Three checks (registry-design.md §4.2): (1) manifest present, (2) entry present
/// with
/// matching basename, (3) sha256 per file against the attachment table. Thin archives legitimately
/// lack attachments; check (3) audits what is present.
pub fn install_mcl(archive: &Path, expected_name: Option<&str>) -> Result<(String, PathBuf)> {
    let files = read_archive(archive)?;

    // Check (1): manifest present + structural validation (format gate, pack-name law, semver live
    // here).
    let text = files
        .get("pack.toml")
        .ok_or_else(|| anyhow!("install check (1) failed: no pack.toml in the archive"))?;
    let pack: packfile::PackToml =
        toml::from_str(std::str::from_utf8(text).context("pack.toml is not UTF-8")?)
            .context("failed to parse the archive pack.toml")?;
    packfile::validate_structure(&pack)?;

    let entry = &pack.package.entry;
    // Check (2): entry present (basename matching is already covered by validate_structure's
    // pack-name law).
    if !files.contains_key(entry.as_str()) {
        anyhow::bail!("install check (2) failed: entry `{}` is not in the archive", entry);
    }

    // The manifest is the authority face: unlisted files are refused (thin allows
    // manifest+entry+README).
    let mut allowed: std::collections::BTreeSet<&str> =
        ["pack.toml", entry.as_str(), "README", "README.md"].into_iter().collect();
    if let Some(rm) = &pack.package.readme {
        allowed.insert(rm.as_str());
    }
    for att in &pack.attachments {
        if let Some(p) = &att.path {
            allowed.insert(p.as_str());
        }
    }
    for name in files.keys() {
        if !allowed.contains(name.as_str()) {
            anyhow::bail!("archive file `{}` is not listed in pack.toml (the manifest is the authority face)", name);
        }
    }

    // Check (3): sha256 per file against the attachment table (only what is present; thin archives
    // legitimately lack attachments).
    for att in &pack.attachments {
        if let (Some(p), Some(sum)) = (&att.path, &att.checksum) {
            if let Some(data) = files.get(p) {
                let mut h = Sha256::new();
                h.update(data);
                let actual = format!("sha256:{}", hex(&h.finalize()));
                if &actual != sum {
                    anyhow::bail!(
                        "install check (3) failed: attachment `{}` sha256 mismatch (manifest {} != actual {})",
                        p,
                        sum,
                        actual
                    );
                }
            }
        }
    }

    // A caller-supplied name contradicting the manifest is refused — the manifest wins, no silent
    // misplacement.
    if let Some(n) = expected_name {
        if n != pack.package.name {
            anyhow::bail!(
                "the --from archive pack name is `{}`, contradicting the given name `{}`",
                pack.package.name,
                n
            );
        }
    }

    let target = datadir::data_root().join(format!(
        "{}@{}",
        pack.package.name, pack.package.version
    ));
    if target.exists() {
        anyhow::bail!("lib install: target already exists '{}'", target.display());
    }

    // staging → rename: same-volume atomic placement; a mid-way failure leaves no half pack.
    let root_dir = datadir::data_root();
    std::fs::create_dir_all(&root_dir)?;
    let staging = root_dir.join(format!(".staging-{}-{}", pack.package.name, std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;
    let extract = (|| -> Result<()> {
        for (rel, data) in &files {
            let dst = staging.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(dst, data)?;
        }
        Ok(())
    })();
    if let Err(e) = extract {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e.into());
    }
    datadir::rebuild_index()?;
    Ok((
        format!("{}@{}", pack.package.name, pack.package.version),
        target,
    ))
}

// archive reading

/// Read a `.mcl`: identified by the zstd frame fingerprint (never the extension), decoded into a
/// relative-path → bytes table.
/// Zip-slip guard: absolute paths or `..` segments are refused outright.
pub fn read_archive(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .with_context(|| format!("failed to open the archive: {}", path.display()))?
        .read_to_end(&mut raw)?;
    if raw.len() < 4 || raw[0..4] != ZSTD_MAGIC {
        anyhow::bail!(
            "{} is not an .mcl archive (zstd frame fingerprint 28 B5 2F FD mismatch)",
            path.display()
        );
    }
    let tarbytes = zstd::stream::decode_all(&raw[..])
        .with_context(|| format!("zstd decode failed: {}", path.display()))?;
    let mut arch = tar::Archive::new(&tarbytes[..]);
    let mut files = BTreeMap::new();
    for e in arch.entries()? {
        let mut e = e?;
        let p = e.path()?.to_path_buf();
        if p.is_absolute()
            || p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!("illegal path in the archive: {}", p.display());
        }
        let mut buf = Vec::new();
        e.read_to_end(&mut buf)?;
        files.insert(p.to_string_lossy().into_owned(), buf);
    }
    Ok(files)
}

// checksums

fn sha256_hex_file(p: &Path) -> Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(hex(&h.finalize()))
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{:02x}", b)).collect()
}
