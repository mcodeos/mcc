// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Cross-platform data directory management (single source of truth).
//!
//! ## Data directory
//!
//! Priority: `$MCC_SYSTEM_ROOT` > `~/.mcode`
//!
//! ## Directory structure (v1)
//!
//! ```text
//! <MCC_SYSTEM_ROOT>/
//! ├── mcode/                     # Official mcode library (built-in)
//! │   ├── mcode.mc               # Entry file (basename = lib name)
//! │   └── ...
//! ├── <name>@<version>/          # User-installed 3rd-party libraries
//! ├── logs/                      # mcc.log, mcc.pid
//! ├── config/                    # User config
//! └── index.json                 # Top-level index
//! ```
//!
//! Built-in and 3rd-party libs share the same flat namespace — `@version`
//! suffix naturally separates the two (mcode has no version suffix;
//! 3rd-party libs always carry one).
//!
//! This root holds the **installation's** files. Files a command *generates
//! for a project* (viz html, products, audits) belong to the project's
//! `build/` instead — see [`crate::cli::outlet`] for that law.

use std::path::{Path, PathBuf};
use tracing::debug;

pub const MCC_SYSTEM_ENV: &str = "MCC_SYSTEM_ROOT";

/// Project manifest file name. CLI, RPC and MCP all use this same name, so a
/// project root is detected uniformly regardless of entry point (use-design §19.5).
pub const PROJECT_MANIFEST_NAME: &str = "project.toml";

/// Return the project manifest in `root`, or `None` when it does not exist.
pub fn find_manifest_in(root: &Path) -> Option<PathBuf> {
    let p = root.join(PROJECT_MANIFEST_NAME);
    p.exists().then_some(p)
}

/// Default data root — `~/.mcode` (used whenever `$MCC_SYSTEM_ROOT` is unset).
fn default_data_root() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".mcode"))
        .unwrap_or_else(|| PathBuf::from(".mcode"))
}

/// Effective `$MCC_SYSTEM_ROOT` override, if any. A relative value is resolved
/// against the current directory (matching the historical `data_root`
/// behaviour); `None` when the variable is unset or empty — an empty value
/// used to resolve to the current directory, silently relocating the whole
/// data root (config/, index.json, logs/) into whatever directory the command
/// ran in.
fn env_data_root_override() -> Option<PathBuf> {
    let val = std::env::var(MCC_SYSTEM_ENV).ok()?;
    if val.trim().is_empty() {
        return None;
    }
    let p = PathBuf::from(val);
    if p.is_absolute() {
        Some(p)
    } else {
        Some(std::env::current_dir().unwrap_or_default().join(p))
    }
}

/// MCC data root directory (single source of truth).
/// Priority: `$MCC_SYSTEM_ROOT` > `~/.mcode`.
pub fn data_root() -> PathBuf {
    env_data_root_override().unwrap_or_else(default_data_root)
}

/// Official mcode library root — `<root>/mcode`.
pub fn mcode_dir() -> PathBuf {
    data_root().join("mcode")
}

pub fn logs_dir() -> PathBuf {
    data_root().join("logs")
}

pub fn config_dir() -> PathBuf {
    data_root().join("config")
}

pub fn index_file() -> PathBuf {
    data_root().join("index.json")
}

pub fn log_file() -> PathBuf {
    logs_dir().join("mcc.log")
}

/// Daemon PID file.
///
/// The **default-root** daemon keeps the historical global location
/// `~/.mcode/logs/mcc.pid` (M4 invariant), so `start`/`stop`/`status` and
/// clients in any shell reach the single default daemon without the env var.
/// An **explicitly isolated** `$MCC_SYSTEM_ROOT` (any non-default value)
/// instead owns its PID file at `<root>/logs/mcc.pid` — each data root gets
/// its own daemon slot, so a custom-root server can run side by side with the
/// default one on a different port. Reach a custom-root daemon by running
/// `start`/`stop`/`status`/clients under the same `MCC_SYSTEM_ROOT`.
pub fn pid_file() -> PathBuf {
    match env_data_root_override() {
        Some(root) if root != default_data_root() => root.join("logs").join("mcc.pid"),
        _ => default_data_root().join("logs").join("mcc.pid"),
    }
}

/// Well-commented default `mcc.yaml` written on first run.
const DEFAULT_MCC_YAML: &str = include_str!("../../mcc.yaml");

/// Ensure canonical directories exist + write index.json + seed default config.
/// Idempotent.
pub fn ensure_dirs() -> std::io::Result<()> {
    for d in [logs_dir(), config_dir()] {
        if !d.exists() {
            std::fs::create_dir_all(&d)?;
            debug!(target: "mcc::dirs", path = ?d, "created");
        }
    }

    // Seed default config file if absent
    let cfg = config_dir().join("mcc.yaml");
    if !cfg.exists() {
        if let Err(e) = std::fs::write(&cfg, DEFAULT_MCC_YAML) {
            debug!(target: "mcc::dirs", path = ?cfg, error = ?e, "failed to write default config (non-fatal)");
        } else {
            debug!(target: "mcc::dirs", path = ?cfg, "default mcc.yaml written");
        }
    }

    if let Err(e) = rebuild_index() {
        debug!(target: "mcc::dirs", error = ?e, "rebuild_index failed (non-fatal)");
    }
    Ok(())
}

// index.json maintenance

/// Rebuild `index.json` from the current state of the data root.
/// Called on every install/uninstall and by `ensure_dirs`.
pub fn rebuild_index() -> std::io::Result<()> {
    use serde_json::{json, Value};

    let root = data_root();
    let mut system_entries: Vec<Value> = Vec::new();
    let mut tp_entries: Vec<Value> = Vec::new();

    if let Ok(read) = std::fs::read_dir(&root) {
        for entry in read.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if LIB_DIR_SKIP.contains(&name.as_str()) {
                continue;
            }
            // Hidden directories are never libraries — pointing the data root
            // at a working copy (the README's local-hacking recipe) must not
            // index `.git` as a system lib.
            if name.starts_with('.') {
                continue;
            }
            if let Some((lib_name, ver)) = parse_name_version(&name) {
                tp_entries.push(json!({
                    "name": lib_name,
                    "version": ver,
                    "path": name,
                }));
            } else {
                system_entries.push(json!({
                    "name": name,
                    "version": "0.0.0",
                    "path": name,
                }));
            }
        }
    }

    let index = json!({
        "version": 1,
        "system": system_entries,
        "3rdparty": tp_entries,
    });
    std::fs::write(
        index_file(),
        serde_json::to_string_pretty(&index)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
    )?;
    debug!(
        target: "mcc::dirs",
        system = system_entries.len(),
        tp = tp_entries.len(),
        "index.json written"
    );
    Ok(())
}

/// Parsed `index.json` content.
#[derive(Debug, Default, Clone)]
pub struct IndexFile {
    pub version: u32,
    pub system: Vec<serde_json::Value>,
    pub thirdparty: Vec<serde_json::Value>,
}

/// Read `index.json` if it exists. Returns None if missing or malformed.
pub fn read_index_if_present() -> Option<IndexFile> {
    let path = index_file();
    let text = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let version = v.get("version").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let system = v
        .get("system")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let thirdparty = v
        .get("3rdparty")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    Some(IndexFile {
        version,
        system,
        thirdparty,
    })
}

/// Read and parse `index.json`. Returns an error if missing or malformed.
pub fn read_index() -> std::io::Result<IndexFile> {
    let path = index_file();
    let text = std::fs::read_to_string(&path).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("read {}: {}", path.display(), e),
        )
    })?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("parse {}: {}", path.display(), e),
        )
    })?;
    let version = v.get("version").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
    let system = v
        .get("system")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let thirdparty = v
        .get("3rdparty")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(IndexFile {
        version,
        system,
        thirdparty,
    })
}

/// Parse `<name>@<version>` into a (name, version) tuple.
pub fn parse_name_version(s: &str) -> Option<(&str, &str)> {
    let at = s.find('@')?;
    let (name, rest) = s.split_at(at);
    let ver = &rest[1..];
    if name.is_empty() || ver.is_empty() {
        None
    } else {
        Some((name, ver))
    }
}

/// Directory names under a library root that are never libraries.
///
/// The union of the skip lists behind the CLI scanner (`cmds/lib.rs`), the
/// RPC search (`rpc/handlers/libcmd.rs`) and `rebuild_index` — one const so
/// they cannot drift. Dot-dirs are skipped by rule, not by list.
pub const LIB_DIR_SKIP: &[&str] = &["logs", "config", "projects", "mclibs", "unitest", "index.json"];

/// The project-local library directory: `<project_root>/libs`.
///
/// Third-party libraries install here by default (cargo-style, vendored and
/// git-committable); the global data root only ever holds the mcode official
/// library.
pub fn project_libs_dir(root: &Path) -> PathBuf {
    root.join("libs")
}

/// Normalize a version string to the canonical two-segment `MAJOR.MINOR`.
///
/// Versions are two numbers by convention (`0.1`); a trailing `.0` on a
/// legacy three-segment value is dropped (`0.1.0` → `0.1`). A version that
/// does not end in `.0` (or has more segments) is returned unchanged —
/// reading is tolerant, writing is canonical.
pub fn normalize_version(ver: &str) -> &str {
    let nums: Vec<&str> = ver.split('.').collect();
    if nums.len() == 3 && nums[2] == "0" {
        // Re-borrow the first two segments joined — safe: same input slice.
        return &ver[..nums[0].len() + 1 + nums[1].len()];
    }
    ver
}

/// One library directory found by [`scan_lib_dir`].
#[derive(Debug, Clone)]
pub struct ScannedLib {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
}

/// Scan a flat library root (`data_root` or a project's `libs/`) for
/// installed libraries: every directory named `<name>@<version>`, plus the
/// legacy bare `mcode` directory (reported as `version = "*"`, matching
/// `mcode_dir()`'s unversioned layout). Versioned `mcode@0.5` copies are
/// listed normally. Skips [`LIB_DIR_SKIP`] and dot-dirs.
pub fn scan_lib_dir(root: &Path) -> Vec<ScannedLib> {
    let mut result = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return result;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.starts_with('.') || LIB_DIR_SKIP.contains(&fname.as_str()) {
            continue;
        }
        if let Some((name, ver)) = parse_name_version(&fname) {
            result.push(ScannedLib {
                name: name.to_string(),
                version: ver.to_string(),
                path: p,
            });
        } else if fname == "mcode" {
            result.push(ScannedLib {
                name: "mcode".into(),
                version: "*".into(),
                path: p,
            });
        }
    }
    result
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Unique scratch directory for one env-override test, rooted under the OS
    /// temp dir (`TMPDIR`/`TEMP`, via [`std::env::temp_dir`]) — never assume a
    /// fixed `/tmp`, which only exists on Unix. Each test passes a distinct
    /// `tag` so parallel runs do not collide.
    fn scratch_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mcc-{tag}-{}", std::process::id()))
    }

    #[test]
    fn cli_datadir__env_override_absolute() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(MCC_SYSTEM_ENV).ok();
        let unique = scratch_dir("test-env");
        std::env::set_var(MCC_SYSTEM_ENV, &unique);
        assert_eq!(data_root(), unique);
        match prev {
            Some(v) => std::env::set_var(MCC_SYSTEM_ENV, v),
            None => std::env::remove_var(MCC_SYSTEM_ENV),
        }
    }

    #[test]
    fn cli_datadir__pid_default_root_stays_global() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(MCC_SYSTEM_ENV).ok();
        let default = default_data_root();

        // Unset env → historical global location.
        std::env::remove_var(MCC_SYSTEM_ENV);
        assert_eq!(pid_file(), default.join("logs").join("mcc.pid"));

        // Override that equals the default root keeps the same location.
        std::env::set_var(MCC_SYSTEM_ENV, &default);
        assert_eq!(pid_file(), default.join("logs").join("mcc.pid"));

        match prev {
            Some(v) => std::env::set_var(MCC_SYSTEM_ENV, v),
            None => std::env::remove_var(MCC_SYSTEM_ENV),
        }
    }

    #[test]
    fn cli_datadir__pid_follows_isolated_override_root() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(MCC_SYSTEM_ENV).ok();
        let unique = scratch_dir("pid-root");
        std::env::set_var(MCC_SYSTEM_ENV, &unique);
        let p = pid_file();
        // An isolated MCC_SYSTEM_ROOT owns its own daemon slot under the root.
        assert_eq!(p, unique.join("logs").join("mcc.pid"));
        assert!(
            p.starts_with(&unique),
            "pid file {:?} must live under the override root {:?}",
            p,
            unique
        );
        match prev {
            Some(v) => std::env::set_var(MCC_SYSTEM_ENV, v),
            None => std::env::remove_var(MCC_SYSTEM_ENV),
        }
    }

    #[test]
    fn cli_datadir__parse_name_version_ok() {
        assert_eq!(parse_name_version("ti.mcu@1.0"), Some(("ti.mcu", "1.0")));
        assert_eq!(parse_name_version("stm32@2.0"), Some(("stm32", "2.0")));
    }

    #[test]
    fn cli_datadir__parse_name_version_invalid() {
        assert_eq!(parse_name_version("mcode"), None);
        assert_eq!(parse_name_version("@1.0"), None);
        assert_eq!(parse_name_version("name@"), None);
    }

    #[test]
    fn cli_datadir__normalize_version_two_segment_canonical() {
        // Canonical form passes through; legacy trailing .0 is dropped.
        assert_eq!(normalize_version("0.1"), "0.1");
        assert_eq!(normalize_version("1.5"), "1.5");
        assert_eq!(normalize_version("0.1.0"), "0.1");
        assert_eq!(normalize_version("10.20.0"), "10.20");
        // A non-zero third segment is not ours to reinterpret — leave it.
        assert_eq!(normalize_version("0.1.2"), "0.1.2");
    }

    #[test]
    fn cli_datadir__scan_lib_dir_versions_and_skips() {
        let unique = scratch_dir("scan-lib-dir");
        let _ = std::fs::remove_dir_all(&unique);
        std::fs::create_dir_all(unique.join("hc32l110@0.1")).unwrap();
        std::fs::create_dir_all(unique.join("mcpub@1.0.0")).unwrap();
        std::fs::create_dir_all(unique.join("mcode")).unwrap();
        std::fs::create_dir_all(unique.join("mcode@0.5")).unwrap();
        std::fs::create_dir_all(unique.join("logs")).unwrap();
        std::fs::create_dir_all(unique.join(".git")).unwrap();
        std::fs::write(unique.join("index.json"), "{}").unwrap();

        let mut libs = scan_lib_dir(&unique);
        libs.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
        let summary: Vec<(String, String)> = libs
            .iter()
            .map(|l| (l.name.clone(), l.version.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("hc32l110".into(), "0.1".into()),
                ("mcode".into(), "*".into()),
                ("mcode".into(), "0.5".into()),
                ("mcpub".into(), "1.0.0".into()),
            ]
        );
        let _ = std::fs::remove_dir_all(&unique);
    }

    #[test]
    fn cli_datadir__sub_dirs_under_data_root() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(MCC_SYSTEM_ENV).ok();
        let unique = scratch_dir("data-dir");
        std::env::set_var(MCC_SYSTEM_ENV, &unique);
        assert_eq!(mcode_dir(), unique.join("mcode"));
        assert_eq!(logs_dir(), unique.join("logs"));
        assert_eq!(config_dir(), unique.join("config"));
        assert_eq!(index_file(), unique.join("index.json"));
        match prev {
            Some(v) => std::env::set_var(MCC_SYSTEM_ENV, v),
            None => std::env::remove_var(MCC_SYSTEM_ENV),
        }
    }
}
