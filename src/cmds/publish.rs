// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib publish` (registry-p3-protocol.md §4.2): pack → sign → stage the
//! tree delta → transport. The P3 face is the maintainer-script form the
//! design pinned (PUT /api/v1/publish is phase two): the command computes the
//! tree delta locally — merged `/lib/<name>.json`, regenerated partno alias
//! rows, the dual artifacts under `/dl/`, a regenerated `/search.json` when
//! the registry is a local tree — and either applies it in place (`file://`
//! registry, `transport = none`), shells out to rsync/scp, or just lists what
//! would move.
//!
//! Laws this module holds: **immutable publishing** (a version already in
//! the tree refuses — no force; to withdraw is yank + a new version), the
//! partno strings come off the entry source (never guessed from variant
//! keys), and a signature is only present when `[registry.publish] key` is.

use anyhow::{bail, Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};

use mcc::cli::packfile;
use mcc::{LibMeta, RegistrySource, SearchEntry, Tier, VersionMeta};

/// What publish staged (the report the CLI prints).
struct Delta {
    files: Vec<PathBuf>,
    root: PathBuf,
}

pub fn cmd_publish(source: &str, go: bool) -> Result<()> {
    let project_root = crate::cmds::lib::client_project_root();
    let url = mcc::cli::config::get_registry_url(project_root.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(
            "lib publish: no registry configured — set [registry] url in mcc.yaml or \
             [config.registry] url in project.toml"
        )
    })?;
    let src = RegistrySource::from_url(&url)
        .map_err(|e| anyhow::anyhow!("lib publish: {e}"))?;

    // ── 1. pack + coordinates ──
    let path = PathBuf::from(source);
    let (pack, report, full, thin) = if path.join("pack.toml").is_file() {
        let out = std::env::temp_dir().join(format!(
            "mcc-publish-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).context("cannot create publish scratch dir")?;
        let report = crate::cmds::pack::do_pack(source, Some(out.to_string_lossy().as_ref()))
            .map_err(|e| anyhow::anyhow!("lib publish: pack gate failed: {e}"))?;
        let manifest = packfile::load(&path).context("lib publish: manifest reload failed")?;
        let full = PathBuf::from(&report.full.path);
        let thin = PathBuf::from(&report.thin.path);
        (manifest, report, full, thin)
    } else {
        anyhow::bail!(
            "lib publish: `{}` carries no pack.toml — publish takes the pack directory \
             (the thin tier is the download face; a bare .mcl has no thin anchor)",
            source
        );
    };

    let version = canon_version(&pack.package.version);
    let name = pack.package.name.clone();

    // ── 2. the version row: checksums off the artifacts, partnos off the
    //      entry source (mkregistry.sh parity — never from variant keys) ──
    let entry_text = std::fs::read_to_string(path.join(&pack.package.entry))
        .with_context(|| format!("cannot read entry {}", pack.package.entry))?;
    let partnos = scan_partnos(&entry_text);
    let vmeta = VersionMeta {
        yanked: false,
        // PackReport carries the bare hex digest; the metadata form wants the
        // `sha256:`-prefixed face the installer verifies against.
        checksum: Some(format!("sha256:{}", report.full.sha256)),
        thin_checksum: Some(format!("sha256:{}", report.thin.sha256)),
        size: Some(std::fs::metadata(&thin)?.len()),
        deps: pack.dependencies.clone(),
        variants: partnos.clone(),
        sig: None,
        keyid: None,
    };
    let description = pack.package.description.clone();

    // ── 3. sign (when the key is configured) ──
    let key_path = global_publish("lib publish")?.and_then(|p| p.key).map(PathBuf::from);
    let mut vmeta = vmeta;
    let mut signed = false;
    if let Some(key) = &key_path {
        let mut entry = serde_json::to_value(&vmeta).context("version entry does not serialize")?;
        if let serde_json::Value::Object(map) = &mut entry {
            map.remove("sig");
            map.remove("keyid");
        }
        let (sig, keyid) = mcc::sign_version(&entry, key)
            .map_err(|e| anyhow::anyhow!("lib publish: signing failed: {e}"))?;
        vmeta.sig = Some(sig);
        vmeta.keyid = Some(keyid);
        signed = true;
    }

    // ── 4. merge against the live tree; immutable publishing law ──
    let existing = src.meta_json(&name)?;
    if existing.as_ref().is_some_and(|m| m.versions.contains_key(&version)) {
        bail!(
            "lib publish: {name}@{version} is already in the registry — versions are \
             immutable; to withdraw is yank + a new version"
        );
    }
    let mut meta = existing.unwrap_or(LibMeta {
        name: name.clone(),
        category: pack.package.category.clone(),
        package: None,
        description: None,
        versions: Default::default(),
    });
    meta.category = pack.package.category.clone();
    meta.description = description.clone();
    meta.versions.insert(version.clone(), vmeta.clone());

    // partno alias rows: one file per partno, versions restricted to where
    // the partno exists (this batch = this version).
    let mut aliases: Vec<LibMeta> = Vec::new();
    for p in &partnos {
        let alias = src.meta_json(p)?;
        if alias.as_ref().is_some_and(|m| m.versions.contains_key(&version)) {
            bail!(
                "lib publish: partno alias {p}@{version} is already in the registry — \
                 versions are immutable"
            );
        }
        let mut m = alias.unwrap_or(LibMeta {
            name: p.clone(),
            category: pack.package.category.clone(),
            package: Some(name.clone()),
            description: None,
            versions: Default::default(),
        });
        m.package = Some(name.clone());
        m.versions.insert(version.clone(), vmeta.clone());
        aliases.push(m);
    }

    // ── 5. stage the delta ──
    let delta_root = std::env::current_dir()?.join("publish-delta");
    if delta_root.exists() {
        std::fs::remove_dir_all(&delta_root)
            .with_context(|| format!("cannot reset {}", delta_root.display()))?;
    }
    let mut files: Vec<PathBuf> = Vec::new();
    files.push(stage_meta(&delta_root, &meta)?);
    for a in &aliases {
        files.push(stage_meta(&delta_root, a)?);
    }
    let dl = delta_root
        .join("dl")
        .join(&pack.package.category)
        .join(&name)
        .join(&version);
    std::fs::create_dir_all(&dl)?;
    for (from, tier) in [(&full, Tier::Full), (&thin, Tier::Thin)] {
        let to = dl.join(format!("{name}-{version}{}", tier.suffix()));
        std::fs::copy(from, &to)
            .with_context(|| format!("cannot stage artifact {}", to.display()))?;
        files.push(to);
    }

    // search.json: regenerable only when the whole tree is readable — the
    // file face. An HTTP registry gets the note (the generator runs at the
    // target after transport, mcpub/mksearch.sh).
    if let RegistrySource::File(root) = &src {
        let idx = regenerate_search(root, Some(&delta_root))?;
        files.push(idx);
    }

    let delta = Delta { files, root: delta_root.clone() };

    // ── 6. list, then transport on --go ──
    eprintln!("✓ staged {name}@{version}{} ({} file(s) under {})", if signed { ", signed" } else { ", community (no key configured)" }, delta.files.len(), delta_root.display());
    for f in &delta.files {
        eprintln!("  {}", f.display());
    }
    if !go {
        eprintln!("= dry run: nothing moved. Re-run with --go to apply ([registry.publish] transport = {}).", global_publish("lib publish")?.and_then(|p| p.transport).unwrap_or_else(|| "none".into()));
        return Ok(());
    }
    transport(&src, &delta)
}

// ── faces ──

/// The publisher config (global mcc.yaml only — publishing is a maintainer
/// act, never a per-project setting). A config that fails to parse is an
/// error, never a silent "no key configured" — an unsigned publish must be
/// a choice, not a config typo. `lib yank` shares the section (the same key
/// re-signs the yanked row), hence the verb parameter for the error prefix.
pub(crate) fn global_publish(verb: &str) -> Result<Option<mcc::cli::config::PublishConfig>> {
    match mcc::cli::config::load_global_config() {
        Ok(c) => Ok(c.registry.publish),
        Err(e) => Err(anyhow::anyhow!("{verb}: {e:#}")),
    }
}

/// x.y.z → x.y (the repo's canonical two-segment law; shorter passes).
pub(crate) fn canon_version(v: &str) -> String {
    let segs: Vec<&str> = v.split('.').collect();
    if segs.len() > 2 {
        segs[..2].join(".")
    } else {
        v.to_string()
    }
}

/// The partno strings off the entry source — the same shallow line scan
/// mkregistry.sh performs (`partno = "…"`, word-bounded, never the variant
/// identifier keys).
fn scan_partnos(entry_text: &str) -> Vec<String> {
    let re = Regex::new(r#"^\s*partno\s*=\s*"([^"]+)""#).expect("static regex");
    let mut seen = std::collections::BTreeSet::new();
    for line in entry_text.lines() {
        if let Some(caps) = re.captures(line) {
            seen.insert(caps[1].to_string());
        }
    }
    seen.into_iter().collect()
}

/// Write one merged metadata file into the delta (compact single line — the
/// mkregistry.sh form).
fn stage_meta(delta_root: &Path, meta: &LibMeta) -> Result<PathBuf> {
    let dir = delta_root.join("lib");
    std::fs::create_dir_all(&dir)?;
    let to = dir.join(format!("{}.json", meta.name));
    std::fs::write(&to, serde_json::to_string(meta)?)
        .with_context(|| format!("cannot stage {}", to.display()))?;
    Ok(to)
}

/// Regenerate `<tree>/search.json` from the live tree plus the delta rows
/// (the file-face advantage: the whole catalog is readable). With a delta
/// root the index is staged there (publish's tree delta); with `None` it is
/// written straight into the tree (yank mutates in place). Returns the path
/// written.
pub(crate) fn regenerate_search(tree: &Path, delta_root: Option<&Path>) -> Result<PathBuf> {
    let lib_dir = tree.join("lib");
    let mut rows: Vec<SearchEntry> = Vec::new();
    let mut collect = |dir: &Path| -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            if p.to_string_lossy().ends_with(".etag") {
                continue;
            }
            let text = std::fs::read_to_string(&p)?;
            let Ok(meta) = serde_json::from_str::<LibMeta>(&text) else { continue };
            if meta.package.is_some() {
                continue; // partno aliases are not catalog rows
            }
            let latest = meta
                .versions
                .iter()
                .filter(|(_, v)| !v.yanked)
                .map(|(k, _)| k.clone())
                .max_by_key(|v| mcc::version_key(v));
            rows.push(SearchEntry {
                name: meta.name,
                category: meta.category,
                description: meta.description,
                latest,
            });
        }
        Ok(())
    };
    // Delta rows first: dedup keeps the first of each run, and the staged
    // row (this publish's versions included) must win over the tree's stale
    // pre-publish row.
    if let Some(delta) = delta_root {
        collect(&delta.join("lib"))?;
    }
    collect(&lib_dir)?;
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows.dedup_by(|a, b| a.name == b.name);
    let to = delta_root
        .map(|d| d.join("search.json"))
        .unwrap_or_else(|| tree.join("search.json"));
    let body = serde_json::to_string(&serde_json::json!({ "packages": rows }))?;
    std::fs::write(&to, body).with_context(|| format!("cannot write {}", to.display()))?;
    Ok(to)
}

/// Apply/ship the staged delta (§4.2 step 4): in place for a `file://`
/// registry with `transport = none`; rsync/scp when configured; `none` on an
/// HTTP registry stops at the staged listing (the generator runs at the
/// target).
fn transport(src: &RegistrySource, delta: &Delta) -> Result<()> {
    let cfg = global_publish("lib publish")?.unwrap_or_default();
    let mode = cfg.transport.as_deref().unwrap_or("none");
    match (src, mode) {
        (RegistrySource::File(root), "none") => {
            let target = root
                .canonicalize()
                .with_context(|| format!("cannot resolve tree {}", root.display()))?;
            copy_dir_recursive(&delta.root, &target)?;
            eprintln!("✓ applied in place → {}", target.display());
        }
        (_, "rsync") => {
            let target = cfg.target.as_deref().ok_or_else(|| {
                anyhow::anyhow!("lib publish: transport rsync needs [registry.publish] target")
            })?;
            run_cmd("rsync", ["-a", &format!("{}/", delta.root.display()), target])?;
            eprintln!("✓ transported → {target}");
        }
        (_, "scp") => {
            let target = cfg.target.as_deref().ok_or_else(|| {
                anyhow::anyhow!("lib publish: transport scp needs [registry.publish] target")
            })?;
            run_cmd("scp", ["-r", &delta.root.display().to_string(), target])?;
            eprintln!("✓ transported → {target}");
        }
        (RegistrySource::Http { .. }, "none") => {
            eprintln!(
                "= delta staged at {} — configure [registry.publish] transport/target, or upload it to the tree manually (then regenerate search.json with mcpub/mksearch.sh)",
                delta.root.display()
            );
        }
        (_, other) => {
            bail!("lib publish: unknown [registry.publish] transport `{other}` (rsync | scp | none)");
        }
    }
    Ok(())
}

fn run_cmd<const N: usize>(prog: &str, args: [&str; N]) -> Result<()> {
    let status = std::process::Command::new(prog)
        .args(args)
        .status()
        .with_context(|| format!("cannot run {prog}"))?;
    if !status.success() {
        bail!("{prog} failed ({status})");
    }
    Ok(())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir_recursive(&p, &t)?;
        } else {
            std::fs::copy(&p, &t).with_context(|| format!("cannot copy {}", p.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canon_version_follows_the_two_segment_law() {
        assert_eq!(canon_version("1.2.0"), "1.2");
        assert_eq!(canon_version("0.2"), "0.2");
        assert_eq!(canon_version("3"), "3");
    }

    #[test]
    fn partnos_come_off_the_entry_source_lines() {
        let text = "component X {\n  partno = \"X-3.3\"\n  partno=\"X-5.0\"\n  // partno = \"commented\"\n  variant = \"not-a-partno\"\n}\n";
        assert_eq!(scan_partnos(text), vec!["X-3.3", "X-5.0"]);
    }
}
