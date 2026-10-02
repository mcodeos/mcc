//! pack.toml —— 器件包清单 schema（registry-design.md §2/§3.1）。
//!
//! 一个 lib 包＝一目录＋一份 `pack.toml`：`[package]`（坐标＋entry）、
//! `[dependencies]`、`[variants]`（base+since 派生面）、`[[attachments]]`
//! （bundled：path/kind/rev/license/checksum；linked：url/rev）。
//! pack/inspect/install 三面共用这里的解析与校验；校验是纯结构性的，
//! 语义面（variants base 在位、checksum 对账）在 cmds/pack.rs 与安装三查里。

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// 清单格式版本闸。当前只认 `"1"`；更高的 format 在 pack 侧拒绝出包、
/// 在 install 侧拒绝安装（装时 format 闸），提示升级 mcc。
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
    /// 清单格式版本，字符串（如 `format = "1"`）。
    pub format: String,
    /// 包名（安装落位 `~/.mcode/<name>@<ver>/` 的 name 段）。
    pub name: String,
    /// 语义版本，entry 驱动（§4.1）：改接口面才动它。
    pub version: String,
    /// 类目（power/mcu/connector/…），非空即可，不做枚举封闭。
    pub category: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// 入口 .mc 文件名（相对包根）。
    pub entry: String,
}

/// 一个派生变体：`base`＝entry 面上必须存在的 component 名，
/// `since`＝引入该变体的包版本（entry 驱动 semver 的证据行）。
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct VariantEntry {
    pub base: String,
    pub since: String,
}

/// 附件：bundled（包内实文件，带 path＋checksum）或 linked（只留指针 url/rev）。
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AttachmentEntry {
    /// bundled 附件的包内相对路径；linked 附件缺省。
    #[serde(default)]
    pub path: Option<String>,
    /// 资料种类：datasheet/doc/symbol/pcb/sim/3d/test（P1 不封闭枚举，只查非空）。
    pub kind: String,
    /// 资料版本（datasheet 版次等）。
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// bundled 附件的内容校验和，`sha256:<hex>`；pack 时写入，install 三查时对账。
    #[serde(default)]
    pub checksum: Option<String>,
    /// linked 附件的文档名（mo/ds 中央手册名等）。
    #[serde(default)]
    pub name: Option<String>,
    /// linked 附件的指针 URL。
    #[serde(default)]
    pub url: Option<String>,
}

impl AttachmentEntry {
    /// bundled＝带包内路径的实体附件；linked＝纯指针。
    pub fn is_bundled(&self) -> bool {
        self.path.is_some()
    }
}

/// 读入并校验 `<dir>/pack.toml`（结构面＋entry 在盘）。
pub fn load(dir: &Path) -> Result<PackToml> {
    let path = dir.join("pack.toml");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("打开包清单失败: {}", path.display()))?;
    let pack: PackToml = toml::from_str(&text)
        .with_context(|| format!("解析 pack.toml 失败 (TOML): {}", path.display()))?;
    validate(&pack, dir)?;
    Ok(pack)
}

/// 结构校验（不需要包目录在盘：inspect/install 对解包内容先走这半）。
/// 规则：format 闸、name/version/category 非空＋version semver、
/// entry 是包内相对路径、name==entry basename（包名律）、
/// 附件表 bundled/linked 各自的必备字段、checksum 形如 `sha256:<hex64>`。
pub fn validate_structure(pack: &PackToml) -> Result<()> {
    if pack.package.format != SUPPORTED_FORMAT {
        bail!(
            "pack.toml format = {}，本 mcc 只认 {}（请升级 mcc）",
            pack.package.format,
            SUPPORTED_FORMAT
        );
    }
    let name = pack.package.name.trim();
    if name.is_empty() {
        bail!("pack.toml [package] name 为空");
    }
    if !valid_semver(&pack.package.version) {
        bail!(
            "pack.toml [package] version `{}` 不是 x.y.z 语义版本",
            pack.package.version
        );
    }
    if pack.package.category.trim().is_empty() {
        bail!("pack.toml [package] category 为空");
    }
    let entry = &pack.package.entry;
    if entry.is_empty()
        || entry.starts_with('/')
        || entry.split('/').any(|seg| seg == "..")
    {
        bail!("pack.toml [package] entry `{}` 必须是包内相对路径", entry);
    }
    let entry_path = Path::new(entry);
    // 包名律：包名 == entry 文件 basename（stem）。一包一件，名随件走。
    let stem = entry_path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("entry 文件名非 UTF-8")?;
    if name != stem {
        bail!(
            "包名律违反: [package] name `{}` != entry basename `{}`",
            name,
            stem
        );
    }

    for (dep, req) in &pack.dependencies {
        if dep.trim().is_empty() || req.trim().is_empty() {
            bail!("[dependencies] 存在空名或空版本要求");
        }
    }
    for (vname, v) in &pack.variants {
        if vname.trim().is_empty() {
            bail!("[variants] 存在空变体名");
        }
        if v.base.trim().is_empty() {
            bail!("[variants] `{}` 的 base 为空", vname);
        }
        if !valid_semver(&v.since) {
            bail!("[variants] `{}` since `{}` 不是 x.y.z 语义版本", vname, v.since);
        }
    }

    let mut seen_paths = std::collections::BTreeSet::new();
    for att in &pack.attachments {
        if att.kind.trim().is_empty() {
            bail!("存在 kind 为空的附件条目");
        }
        match (&att.path, &att.url) {
            (Some(p), _) if !p.is_empty() => {
                if p.starts_with('/') || p.split('/').any(|seg| seg == "..") {
                    bail!("bundled 附件路径 `{}` 必须是包内相对路径", p);
                }
                if !seen_paths.insert(p.clone()) {
                    bail!("附件路径 `{}` 重复列出", p);
                }
                if let Some(sum) = &att.checksum {
                    if !valid_sha256(sum) {
                        bail!("附件 `{}` 的 checksum `{}` 应为 sha256:<hex64>", p, sum);
                    }
                }
            }
            (None, Some(u)) if !u.trim().is_empty() => {
                if att.checksum.is_some() {
                    bail!("linked 附件 `{}` 不带 checksum（指针不背内容责任）", u);
                }
                if att.path.is_some() {
                    unreachable!();
                }
            }
            _ => bail!("附件条目 kind={} 既无 path 也无 url（bundled/linked 二选一）", att.kind),
        }
    }
    Ok(())
}

/// 全量校验＝结构面＋entry 文件在盘（pack 面用；entry 不在场的包处处不合法）。
pub fn validate(pack: &PackToml, dir: &Path) -> Result<()> {
    validate_structure(pack)?;
    let entry_path = dir.join(&pack.package.entry);
    if !entry_path.is_file() {
        bail!("pack.toml entry 文件不在场: {}", entry_path.display());
    }
    Ok(())
}

/// 宽松 semver：`x.y.z`，三段非空数字（预发布/元数据段不收——器件包用不到）。
fn valid_semver(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// `sha256:<64 位小写 hex>`。
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
