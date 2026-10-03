// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `.mcl` install path, in the library crate so the solver (registry-design.md §4) and the
//! RPC face share one unpacking entry — the container shape is a tar stream flowing into a
//! single zstd stream; the frame magic `28 B5 2F FD` is the format fingerprint, identified by
//! bytes at install time, never by extension. Thin = manifest + entry, full = manifest + entry
//! + all bundled attachments.

use crate::cli::{datadir, packfile};
use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The zstd frame magic (RFC 8878 §3.1.1) — the `.mcl` format fingerprint, identified by bytes at
/// install time, never by extension.
pub const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// `.mcl` install: unpack → install-time three checks + format gate → staging into
/// `data_root/<name>@<ver>/`
/// → rebuild_index. Three checks (registry-design.md §4.2): (1) manifest present, (2) entry present
/// with
/// matching basename, (3) sha256 per file against the attachment table. Thin archives legitimately
/// lack attachments; check (3) audits what is present.
pub fn install_mcl(archive: &Path, expected_name: Option<&str>) -> Result<(String, PathBuf)> {
    install_mcl_at(archive, expected_name, &datadir::data_root())
}

/// `.mcl` install into an explicit root (project `libs/` or the data root).
///
/// The staging directory lives under the *target* root so the final rename
/// stays same-volume for project installs too. The install-scope guard
/// applies after pack.toml names the pack: mcode is global-only, and a
/// project-tier target refuses it.
pub fn install_mcl_at(
    archive: &Path,
    expected_name: Option<&str>,
    target_root: &Path,
) -> Result<(String, PathBuf)> {
    install_mcl_at_scope(archive, expected_name, target_root, crate::ensure_install_scope)
}

/// [`install_mcl_at`] with a caller-chosen install-scope guard. The registry
/// solve face passes [`crate::ensure_install_scope_registry`] (law amendment 1: a
/// registry-solved pack may land in the data root); every other caller keeps
/// the `--from` guard byte for byte.
pub fn install_mcl_at_scope(
    archive: &Path,
    expected_name: Option<&str>,
    target_root: &Path,
    scope: fn(&str, &Path) -> Result<()>,
) -> Result<(String, PathBuf)> {
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

    // Install-scope guard (pack.toml is the authority face for the name).
    scope(&pack.package.name, target_root)?;

    // The landed directory normalizes to the canonical two-segment version,
    // even when a legacy x.y.z pack.toml says otherwise.
    let ver = datadir::normalize_version(&pack.package.version).to_string();
    let target = target_root.join(format!("{}@{}", pack.package.name, ver));
    if target.exists() {
        anyhow::bail!("lib install: target already exists '{}'", target.display());
    }

    // staging → rename: same-volume atomic placement; a mid-way failure leaves no half pack.
    // Staging lives under the target root so the rename stays same-volume.
    let root_dir = target_root;
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
        // Two-window race: another process may have placed the same pack
        // between the exists() check above and this rename (os error 66,
        // ENOTEMPTY). The winner verified the same sha256 pack bytes and
        // landed the same name@version, so treat the placement as already
        // done instead of surfacing a spurious error (U393 leg5 live
        // probe: 9/20 concurrent windows hit this; placement always won).
        if target.exists() {
            return Ok((format!("{}@{}", pack.package.name, ver), target));
        }
        return Err(e.into());
    }
    // The index only covers the global data root; project tiers list by scan.
    if root_dir == datadir::data_root() {
        datadir::rebuild_index()?;
    }
    Ok((
        format!("{}@{}", pack.package.name, ver),
        target,
    ))
}

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

pub fn sha256_hex_file(p: &Path) -> Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(hex(&h.finalize()))
}

pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{:02x}", b)).collect()
}
