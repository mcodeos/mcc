//! pack.toml — the device-pack manifest schema (registry-design.md §2/§3.1).
//!
//! One lib pack = one directory + one `pack.toml`: `[package]` (coordinates + entry),
//! `[dependencies]`, `[variants]` (base+since derived faces), `[[attachments]]`
//! (bundled: path/kind/rev/license/checksum; linked: url/rev).
//! pack/inspect/install share this parsing and validation; validation is purely structural —
// ! the semantic face (variant base presence, checksum audit) lives in cmds/pack.rs and the
   install three checks.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Manifest format gate. Only `"1"` is accepted; a higher format refuses to pack and
/// refuses to install (the install-time format gate), telling the user to upgrade mcc.
pub const SUPPORTED_FORMAT: &str = "1";

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PackToml {
    pub package: PackageSection,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub variants: BTreeMap<String, VariantEntry>,
    #[serde(default)]
    pub attachments: Vec<AttachmentEntry>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PackageSection {
    /// Manifest format version, as a string (e.g. `format = "1"`).
    pub format: String,
    /// Pack name (the name segment of the install location `~/.mcode/<name>@<ver>/`).
    pub name: String,
    /// Semver, entry-driven (§4.1): bump it only when the interface face changes.
    pub version: String,
    /// Category (power/mcu/connector/…); must be non-empty, no closed enumeration.
    pub category: String,
    /// Chip vendor (TI/ST/…); connectors/electromechanical parts without documented
    #[serde(default)]
    pub vendor: Option<String>,
    /// provenance carry the honest `unknown`.
    /// Pack publisher (`mcode` for the mcpub transition; the verified/community slot in the
        cloud-libs era).
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// Entry .mc file name (relative to the pack root).
    pub entry: String,
    /// Pack description document (Markdown, path relative to the pack root). Long text lives in
    /// the file, the manifest keeps the pointer; the thin tier must carry it (README.md/README
    #[serde(default)]
    pub readme: Option<String>,
    /// are auto-included in thin by default; this field makes it explicit).
    /// Search tokens (lowercase, short: device family / package / function). Raw material for
    /// search ranking; no closed enumeration. The registry side (P3) weighs them later.
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// One derived variant: `base` = a component name that must exist on the entry face,
/// `since` = the pack version that introduced it (the entry-driven semver evidence row).
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct VariantEntry {
    pub base: String,
    pub since: String,
}

/// Attachment: bundled (a real in-pack file with path + checksum) or linked (a url/rev pointer
    only).
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AttachmentEntry {
    /// Bundled attachment path relative to the pack root; absent for linked attachments.
    #[serde(default)]
    pub path: Option<String>,
    /// Material kind: datasheet/doc/symbol/pcb/sim/3d/test (P1 keeps it open; only non-empty is
        checked).
    pub kind: String,
    /// Material revision (datasheet rev and the like).
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// Bundled attachment content checksum, `sha256:<hex>`; written at pack time, audited by
        install check ③.
    #[serde(default)]
    pub checksum: Option<String>,
    /// Linked attachment document name (the mo/ds central manual name, etc.).
    #[serde(default)]
    pub name: Option<String>,
    /// Linked attachment pointer URL.
    #[serde(default)]
    pub url: Option<String>,
}

impl AttachmentEntry {
    /// bundled = a physical attachment with an in-pack path; linked = a pure pointer.
    pub fn is_bundled(&self) -> bool {
        self.path.is_some()
    }
}

/// Read and validate `<dir>/pack.toml` (structure + entry on disk).
pub fn load(dir: &Path) -> Result<PackToml> {
    let path = dir.join("pack.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to open the pack manifest: {}", path.display()))?;
    let pack: PackToml = toml::from_str(&text)
        .with_context(|| format!("failed to parse pack.toml (TOML): {}", path.display()))?;
    validate(&pack, dir)?;
    Ok(pack)
}

/// Structural validation (no pack directory needed: inspect/install run this half on unpacked
    content first).
/// Rules: format gate, name/version/category non-empty + semver version,
/// entry a relative in-pack path, name == entry basename (the pack-name law),
/// each attachment row's required fields per bundled/linked, checksum shaped `sha256:<hex64>`.
pub fn validate_structure(pack: &PackToml) -> Result<()> {
    if pack.package.format != SUPPORTED_FORMAT {
        bail!(
            "pack.toml format = {} but this mcc only supports {} (please upgrade mcc)",
            pack.package.format,
            SUPPORTED_FORMAT
        );
    }
    let name = pack.package.name.trim();
    if name.is_empty() {
        bail!("pack.toml [package] name is empty");
    }
    if !valid_semver(&pack.package.version) {
        bail!(
            "pack.toml [package] version `{}` is not an x.y.z semver",
            pack.package.version
        );
    }
    if pack.package.category.trim().is_empty() {
        bail!("pack.toml [package] category is empty");
    }
    for (field, val) in [
        ("vendor", &pack.package.vendor),
        ("publisher", &pack.package.publisher),
    ] {
        if val.as_ref().map(|s| s.trim().is_empty()).unwrap_or(false) {
            bail!("pack.toml [package] {} is an empty string (omit it or make it non-empty)", field);
        }
    }
    if let Some(rm) = &pack.package.readme {
        if rm.is_empty() || !rm.to_ascii_lowercase().ends_with(".md") {
            bail!("pack.toml [package] readme `{}` must be an in-pack .md file", rm);
        }
    }
    // Keywords: non-empty short tokens; lowercase normalization is the packer's duty (only
       emptiness and duplicates are rejected here).
    let mut seen_kw = std::collections::BTreeSet::new();
    for kw in &pack.package.keywords {
        if kw.trim().is_empty() {
            bail!("pack.toml [package] keywords has an empty token");
        }
        if !seen_kw.insert(kw.to_ascii_lowercase()) {
            bail!("pack.toml [package] keywords duplicate token: `{}`", kw);
        }
    }
    let entry = &pack.package.entry;
    if entry.is_empty()
        || entry.starts_with('/')
        || entry.split('/').any(|seg| seg == "..")
    {
        bail!("pack.toml [package] entry `{}` must be a relative in-pack path", entry);
    }
    let entry_path = Path::new(entry);
    // Pack-name law: pack name == the entry file basename (stem). One pack per part; the name
       follows the part.
    let stem = entry_path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("entry file name is not UTF-8")?;
    if name != stem {
        bail!(
            "pack-name law violation: [package] name `{}` != entry basename `{}`",
            name,
            stem
        );
    }

    for (dep, req) in &pack.dependencies {
        if dep.trim().is_empty() || req.trim().is_empty() {
            bail!("[dependencies] has an empty name or an empty version requirement");
        }
    }
    for (vname, v) in &pack.variants {
        if vname.trim().is_empty() {
            bail!("[variants] has an empty variant name");
        }
        if v.base.trim().is_empty() {
            bail!("[variants] `{}` has an empty base", vname);
        }
        if !valid_semver(&v.since) {
            bail!("[variants] `{}` since `{}` is not an x.y.z semver", vname, v.since);
        }
    }

    let mut seen_paths = std::collections::BTreeSet::new();
    for att in &pack.attachments {
        if att.kind.trim().is_empty() {
            bail!("an attachment entry has an empty kind");
        }
        match (&att.path, &att.url) {
            (Some(p), _) if !p.is_empty() => {
                if p.starts_with('/') || p.split('/').any(|seg| seg == "..") {
                    bail!("bundled attachment path `{}` must be a relative in-pack path", p);
                }
                if !seen_paths.insert(p.clone()) {
                    bail!("attachment path `{}` is listed twice", p);
                }
                if let Some(sum) = &att.checksum {
                    if !valid_sha256(sum) {
                        bail!("attachment `{}` checksum `{}` must be sha256:<hex64>", p, sum);
                    }
                }
            }
            (None, Some(u)) if !u.trim().is_empty() => {
                if att.checksum.is_some() {
                    bail!("linked attachment `{}` must not carry a checksum (pointers bear no content responsibility)", u);
                }
                if att.path.is_some() {
                    unreachable!();
                }
            }
            _ => bail!("attachment entry kind={} has neither path nor url (bundled/linked, pick one)", att.kind),
        }
    }
    Ok(())
}

/// Full validation = structure + entry file on disk (pack face; a pack whose entry is absent is
    invalid everywhere).
pub fn validate(pack: &PackToml, dir: &Path) -> Result<()> {
    validate_structure(pack)?;
    let entry_path = dir.join(&pack.package.entry);
    if !entry_path.is_file() {
        bail!("pack.toml entry file is missing: {}", entry_path.display());
    }
    Ok(())
}

/// Loose semver: `x.y.z`, three non-empty numeric segments (no prerelease/metadata — device packs
    never need them).
fn valid_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// `sha256:<64 lowercase hex chars>`.
fn valid_sha256(s: &str) -> bool {
    match s.strip_prefix("sha256:") {
        Some(hex) => hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_shape() {
        assert!(valid_semver("0.1.0"));
        assert!(valid_semver("1.20.3"));
        assert!(!valid_semver("0.1"));
        assert!(!valid_semver("0.1.0-beta"));
        assert!(!valid_semver("a.b.c"));
    }

    #[test]
    fn sha256_shape() {
        let ok = "sha256:";
        let ok = format!("{}{}", ok, "a".repeat(64));
        assert!(valid_sha256(&ok));
        assert!(!valid_sha256(&format!("sha256:{}", "A".repeat(64))));
        assert!(!valid_sha256(&format!("sha256:{}", "a".repeat(63))));
        assert!(!valid_sha256("md5:deadbeef"));
    }
}
