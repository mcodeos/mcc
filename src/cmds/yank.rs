// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib yank` (registry-design.md §7④/§5): mark a version yanked in the
//! configured `file://` registry tree. Metadata-only, by the mark-not-delete
//! law: the `dl/` artifacts stay (a lock holding this version keeps
//! installing it — reproducibility outranks withdrawal), fresh solves skip
//! the row, and the search index's `latest` drops it. The correction path
//! stays yank + a new version: immutable publishing is not relaxed here.
//!
//! The yanked row is judged on the live tree, never the offline cache. A
//! signed row is re-signed through `[registry.publish] key` — and a signed
//! tree without a usable key refuses **before anything is written**, because
//! a stale signature would turn the row into tamper for every consumer.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

use mcc::{LibMeta, RegistrySource};

pub fn cmd_yank(name: &str, version: &str) -> Result<()> {
    let project_root = crate::cmds::lib::client_project_root();
    let url = mcc::cli::config::get_registry_url(project_root.as_deref()).ok_or_else(|| {
        anyhow::anyhow!(
            "lib yank: no registry configured — set [registry] url in mcc.yaml or \
             [config.registry] url in project.toml"
        )
    })?;
    let src = RegistrySource::from_url(&url).map_err(|e| anyhow::anyhow!("lib yank: {e}"))?;
    let root = match &src {
        RegistrySource::File(root) => root.clone(),
        RegistrySource::Http { .. } => anyhow::bail!(
            "lib yank: an HTTP registry has no write face — PUT /api/v1/publish is phase two \
             (registry-p3-protocol.md §1); yank the file tree at the host, then transport"
        ),
    };
    let version = crate::cmds::publish::canon_version(version);

    // The live tree (meta_json verifies signatures; the cache fallback is
    // deliberately not in this path — yank judges the truth it edits).
    let Some(package) = src.meta_json(name)? else {
        bail!("lib yank: `{name}` is not in the registry tree {}", root.display());
    };
    if package.versions.get(&version).is_some_and(|v| v.yanked) {
        eprintln!("= {name}@{version} is already yanked — nothing to do");
        return Ok(());
    }
    if !package.versions.contains_key(&version) {
        let known: Vec<&String> = package.versions.keys().collect();
        bail!("lib yank: {name}@{version} is not in the tree (known: {known:?})");
    }

    // The touched rows: the package row plus every partno alias row carrying
    // this version (the alias entries duplicate the version metadata).
    let mut touched: Vec<LibMeta> = vec![package];
    if let Ok(entries) = std::fs::read_dir(root.join("lib")) {
        for e in entries {
            let e = e?;
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json")
                || p.to_string_lossy().ends_with(".etag")
            {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let Ok(alias) = serde_json::from_str::<LibMeta>(&text) else { continue };
            if alias.package.as_deref() == Some(name)
                && alias.versions.contains_key(&version)
                && !touched.iter().any(|m| m.name == alias.name)
            {
                touched.push(alias);
            }
        }
    }
    for m in touched.iter_mut() {
        if let Some(v) = m.versions.get_mut(&version) {
            v.yanked = true;
        }
    }

    // The pre-write refusal: any touched row already signed needs the
    // publisher key back; a config that fails to parse is an error, never a
    // silent unsigned write. Unsigned rows stay unsigned (yank invents no
    // signatures).
    let signed = touched
        .iter()
        .any(|m| m.versions.get(&version).is_some_and(|v| v.sig.is_some()));
    let key: Option<PathBuf> = if signed {
        match crate::cmds::publish::global_publish("lib yank") {
            Ok(cfg) => cfg.and_then(|p| p.key).map(PathBuf::from),
            Err(e) => return Err(e),
        }
    } else {
        None
    };
    if signed && key.is_none() {
        bail!(
            "lib yank: {name}@{version} carries a signature but [registry.publish] key is not \
             usable — yanking would leave a stale signature (tamper for every consumer); \
             configure the key, then re-run"
        );
    }
    if let Some(key) = &key {
        for m in touched.iter_mut() {
            let Some(v) = m.versions.get_mut(&version) else { continue };
            if v.sig.is_none() {
                continue;
            }
            let mut entry = serde_json::to_value(&*v).context("version entry does not serialize")?;
            if let serde_json::Value::Object(map) = &mut entry {
                map.remove("sig");
                map.remove("keyid");
            }
            let (sig, keyid) = mcc::sign_version(&entry, key)
                .map_err(|e| anyhow::anyhow!("lib yank: re-signing {name}@{version} failed: {e}"))?;
            v.sig = Some(sig);
            v.keyid = Some(keyid);
        }
    }
    // Self-check before anything lands: a rewritten row must pass the same
    // read-face verification every consumer will run.
    for m in &touched {
        mcc::verify_meta(m)
            .map_err(|e| anyhow::anyhow!("lib yank: rewritten `{}` failed verification: {e}", m.name))?;
    }

    for m in &touched {
        let path = root.join("lib").join(format!("{}.json", m.name));
        std::fs::write(&path, serde_json::to_string(m)?)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    crate::cmds::publish::regenerate_search(&root, None)?;
    mcc::refresh_cached_meta(&src.cache_shard(), &touched);

    eprintln!(
        "✓ yanked {name}@{version} ({} metadata row(s){})",
        touched.len(),
        if signed { ", re-signed" } else { "" }
    );
    eprintln!(
        "  search.json regenerated; the offline cache row refreshed; dl/ untouched — \
         a lock holding this version keeps installing it, fresh solves skip it"
    );
    Ok(())
}
