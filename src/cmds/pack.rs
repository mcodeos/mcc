// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc lib pack` / `mcc lib inspect` ＋ `.mcl` 安装路径 —— 器件包本地闭环
//! （registry-design.md §3：`.mcl` ＝ tar 流入 zstd 单流，帧 magic `28 B5 2F FD`
//! 即格式指纹；thin＝清单＋entry，full＝清单＋entry＋全部 bundled 附件）。
//!
//! 全部离线进程内执行，不走 RPC（同 parse 的 U90 先例：离线工具没有守护面）。
//! pack 门＝「编译不过不出包」：进程内调 check 管线跑 entry，任何 E 错即拒。

use crate::cmds::check;
use crate::output;
use anyhow::{anyhow, Context, Result};
use mcc::cli::{datadir, packfile, CheckArgs, OutputFormat};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// zstd 帧 magic（RFC 8878 §3.1.1）——`.mcl` 的格式指纹，装时按字节识别，不靠扩展名。
pub const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

// Reports

#[derive(Serialize)]
pub struct PackReport {
    pub name: String,
    pub version: String,
    pub category: String,
    pub entry: String,
    /// full 档文件集（thin 档 = 清单 + entry + README，报告不重复列）。
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
    /// 档内全部文件名（含路径）。
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

/// Pure pack: validate → check 门 → file sets → 双档 `.mcl`。共享 CLI/RPC 语义面。
pub fn do_pack(dir: &str, out: Option<&str>) -> Result<PackReport> {
    let root = PathBuf::from(dir);
    if !root.is_dir() {
        anyhow::bail!("lib pack: '{}' 不是目录", dir);
    }
    let root = root.canonicalize().context("lib pack: 解析包目录失败")?;
    let pack = packfile::load(&root).context("pack 门：清单校验不过")?;
    let entry_path = root.join(&pack.package.entry);

    // 编译不过不出包：进程内 check 门（U90：check 已是进程内管线，无守护分支）。
    let args = CheckArgs {
        target: Some(entry_path.to_string_lossy().into_owned()),
        dlog: false,
        errors_only: true,
        nets: false,
        pins: false,
        ledger: None,
    };
    let outcome = check::run(&args).context("pack 门：check 管线故障")?;
    if outcome.exit_code != 0 {
        anyhow::bail!(
            "pack 门：entry {} 编译不过（诊断见上），不出包",
            pack.package.entry
        );
    }

    // variants base 名浅查：声明的 base 必须在 entry 源面在位（语义绑定后续批）。
    let source = std::fs::read_to_string(&entry_path)
        .with_context(|| format!("读取 entry 失败: {}", entry_path.display()))?;
    for (vname, v) in &pack.variants {
        let pat = format!(r"\b{}\b", regex::escape(v.base.as_str()));
        let re = regex::Regex::new(&pat)?;
        if !re.is_match(&source) {
            anyhow::bail!(
                "variants `{}` 的 base `{}` 在 entry 源面不在位",
                vname,
                v.base
            );
        }
    }

    // bundled 附件：在位＋sha256 对账（清单已声明 checksum 的，pack 时即验证）。
    for att in &pack.attachments {
        if let Some(p) = &att.path {
            let ap = root.join(p);
            if !ap.is_file() {
                anyhow::bail!("bundled 附件不在场: {}", p);
            }
            if let Some(sum) = &att.checksum {
                let actual = format!("sha256:{}", sha256_hex_file(&ap)?);
                if &actual != sum {
                    anyhow::bail!(
                        "附件 {} checksum 不符: 清单 {} ≠ 实际 {}",
                        p,
                        sum,
                        actual
                    );
                }
            }
        }
    }

    // 文件集：thin = 清单 + entry (+README)；full = thin + 全部 bundled 附件。
    let mut thin_files: Vec<(PathBuf, String)> = vec![
        (root.join("pack.toml"), "pack.toml".to_string()),
        (entry_path.clone(), pack.package.entry.clone()),
    ];
    for readme in ["README.md", "README"] {
        let rp = root.join(readme);
        if rp.is_file() {
            thin_files.push((rp, readme.to_string()));
        }
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

    // 出档：`<name>-<ver>.mcl` ＋ `<name>-<ver>.thin.mcl`，默认落在包目录自身。
    let out_dir = out.map(PathBuf::from).unwrap_or_else(|| root.clone());
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("创建输出目录失败: {}", out_dir.display()))?;
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

/// tar 流入 zstd 单流（§3.4 容器形），路径用包内相对名。
fn write_archive(path: &Path, files: &[(PathBuf, String)]) -> Result<()> {
    let f = std::fs::File::create(path)
        .with_context(|| format!("创建档失败: {}", path.display()))?;
    let enc = zstd::stream::Encoder::new(f, 3)?;
    let mut tar = tar::Builder::new(enc);
    for (abs, rel) in files {
        tar.append_path_with_name(abs, rel)
            .with_context(|| format!("入档失败: {}", rel))?;
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

/// 纯检视：解包读清单，离线吐报告，不落盘、不出网。
pub fn do_inspect(file: &str) -> Result<InspectReport> {
    let files = read_archive(Path::new(file))?;
    let text = files
        .get("pack.toml")
        .ok_or_else(|| anyhow!("档内无 pack.toml"))?;
    let pack: packfile::PackToml =
        toml::from_str(std::str::from_utf8(text).context("pack.toml 非 UTF-8")?)
            .context("档内 pack.toml 解析失败")?;
    // 结构面校验（含 format 闸）；entry 在场性对 thin 档也成立，对 full 档同样要求。
    packfile::validate_structure(&pack)?;
    if !files.contains_key(pack.package.entry.as_str()) {
        anyhow::bail!("档内 entry `{}` 不在场", pack.package.entry);
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

/// `.mcl` 安装：解包 → 装时三查＋format 闸 → staging 落位 `data_root/<name>@<ver>/`
/// → rebuild_index。三查（registry-design.md §4.2）：①清单在场 ②entry 在场且
/// basename 匹配 ③sha256 逐文件对附件表。thin 档附件天然不在场，③按在场文件核验。
pub fn install_mcl(archive: &Path, expected_name: Option<&str>) -> Result<(String, PathBuf)> {
    let files = read_archive(archive)?;

    // 三查①：清单在场＋结构校验（format 闸、包名律、semver 在此）。
    let text = files
        .get("pack.toml")
        .ok_or_else(|| anyhow!("装时三查① 失败：档内无 pack.toml"))?;
    let pack: packfile::PackToml =
        toml::from_str(std::str::from_utf8(text).context("pack.toml 非 UTF-8")?)
            .context("档内 pack.toml 解析失败")?;
    packfile::validate_structure(&pack)?;

    let entry = &pack.package.entry;
    // 三查②：entry 在场（basename 匹配已由 validate_structure 的包名律覆盖）。
    if !files.contains_key(entry.as_str()) {
        anyhow::bail!("装时三查② 失败：entry `{}` 不在档内", entry);
    }

    // 清单是权威面：未列名文件拒收（thin 档允许清单+entry+README）。
    let mut allowed: std::collections::BTreeSet<&str> =
        ["pack.toml", entry.as_str(), "README", "README.md"].into_iter().collect();
    for att in &pack.attachments {
        if let Some(p) = &att.path {
            allowed.insert(p.as_str());
        }
    }
    for name in files.keys() {
        if !allowed.contains(name.as_str()) {
            anyhow::bail!("档内文件 `{}` 未在 pack.toml 列名（清单是权威面）", name);
        }
    }

    // 三查③：sha256 逐文件对附件表（在场才核验；thin 档附件不在场属正常态）。
    for att in &pack.attachments {
        if let (Some(p), Some(sum)) = (&att.path, &att.checksum) {
            if let Some(data) = files.get(p) {
                let mut h = Sha256::new();
                h.update(data);
                let actual = format!("sha256:{}", hex(&h.finalize()));
                if &actual != sum {
                    anyhow::bail!(
                        "装时三查③ 失败：附件 `{}` sha256 不符（清单 {} ≠ 实际 {}）",
                        p,
                        sum,
                        actual
                    );
                }
            }
        }
    }

    // 调用方给的 name 若与清单相悖，以清单为准、拒绝静默错位。
    if let Some(n) = expected_name {
        if n != pack.package.name {
            anyhow::bail!(
                "--from 档的包名是 `{}`，与给定 name `{}` 不符",
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

    // staging → rename：同盘原子落位，半途失败不留半个包。
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

/// 读入 `.mcl`：按 zstd 帧指纹识别（不靠扩展名），解出「相对路径 → 字节」表。
/// zip-slip 护栏：绝对路径或 `..` 段直接拒。
pub fn read_archive(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .with_context(|| format!("打开档失败: {}", path.display()))?
        .read_to_end(&mut raw)?;
    if raw.len() < 4 || raw[0..4] != ZSTD_MAGIC {
        anyhow::bail!(
            "{} 不是 .mcl 档（zstd 帧指纹 28 B5 2F FD 不符）",
            path.display()
        );
    }
    let tarbytes = zstd::stream::decode_all(&raw[..])
        .with_context(|| format!("zstd 解码失败: {}", path.display()))?;
    let mut arch = tar::Archive::new(&tarbytes[..]);
    let mut files = BTreeMap::new();
    for e in arch.entries()? {
        let mut e = e?;
        let p = e.path()?.to_path_buf();
        if p.is_absolute()
            || p.components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!("档内出现非法路径: {}", p.display());
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
