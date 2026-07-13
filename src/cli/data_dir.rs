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

use std::path::PathBuf;
use tracing::debug;

pub const MCC_SYSTEM_ENV: &str = "MCC_SYSTEM_ROOT";

/// MCC data root directory (single source of truth).
/// Priority: `$MCC_SYSTEM_ROOT` > `~/.mcode`.
pub fn data_root() -> PathBuf {
    if let Ok(val) = std::env::var(MCC_SYSTEM_ENV) {
        let p = PathBuf::from(val);
        if p.is_absolute() {
            return p;
        }
        if let Ok(cwd) = std::env::current_dir() {
            return cwd.join(p);
        }
        return p;
    }
    if let Some(home) = dirs::home_dir() {
        home.join(".mcode")
    } else {
        PathBuf::from(".mcode")
    }
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

/// Daemon PID file. **Always at `~/.mcode/logs/mcc.pid`** (M4 invariant),
/// decoupled from `$MCC_SYSTEM_ROOT` so `start`/`stop`/`status` and clients
/// in any shell locate the same single daemon.
pub fn pid_file() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".mcode").join("logs").join("mcc.pid"))
        .unwrap_or_else(|| PathBuf::from(".mcode/logs/mcc.pid"))
}

/// Ensure canonical directories exist + write index.json. Idempotent.
pub fn ensure_dirs() -> std::io::Result<()> {
    for d in [logs_dir(), config_dir()] {
        if !d.exists() {
            std::fs::create_dir_all(&d)?;
            debug!(target: "mcc::dirs", path = ?d, "created");
        }
    }
    if let Err(e) = rebuild_index() {
        debug!(target: "mcc::dirs", error = ?e, "rebuild_index failed (non-fatal)");
    }
    Ok(())
}

// ============================================================================
// index.json maintenance
// ============================================================================

/// Rebuild `index.json` from the current state of the data root.
/// Called on every install/uninstall and by `ensure_dirs`.
pub fn rebuild_index() -> std::io::Result<()> {
    use serde_json::{json, Value};

    let root = data_root();
    let mut system_entries: Vec<Value> = Vec::new();
    let mut tp_entries: Vec<Value> = Vec::new();
    let skip = ["logs", "config", "projects", "index.json"];

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
            if skip.contains(&name.as_str()) {
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
fn parse_name_version(s: &str) -> Option<(&str, &str)> {
    let at = s.find('@')?;
    let (name, rest) = s.split_at(at);
    let ver = &rest[1..];
    if name.is_empty() || ver.is_empty() {
        None
    } else {
        Some((name, ver))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;

    pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn env_override_absolute() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(MCC_SYSTEM_ENV).ok();
        let unique = format!("/tmp/mcc-test-env-{}-{}", std::process::id(), line!());
        std::env::set_var(MCC_SYSTEM_ENV, &unique);
        assert_eq!(data_root(), PathBuf::from(&unique));
        match prev {
            Some(v) => std::env::set_var(MCC_SYSTEM_ENV, v),
            None => std::env::remove_var(MCC_SYSTEM_ENV),
        }
    }

    #[test]
    fn pid_always_at_home_mcode() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(MCC_SYSTEM_ENV).ok();
        let unique = format!("/tmp/somewhere-else-{}-{}", std::process::id(), line!());
        std::env::set_var(MCC_SYSTEM_ENV, &unique);
        let p = pid_file();
        assert!(p.to_string_lossy().ends_with(".mcode/logs/mcc.pid"));
        assert!(!p.to_string_lossy().contains("/tmp/somewhere-else"));
        match prev {
            Some(v) => std::env::set_var(MCC_SYSTEM_ENV, v),
            None => std::env::remove_var(MCC_SYSTEM_ENV),
        }
    }

    #[test]
    fn parse_name_version_ok() {
        assert_eq!(parse_name_version("ti.mcu@1.0"), Some(("ti.mcu", "1.0")));
        assert_eq!(parse_name_version("stm32@2.0"), Some(("stm32", "2.0")));
    }

    #[test]
    fn parse_name_version_invalid() {
        assert_eq!(parse_name_version("mcode"), None);
        assert_eq!(parse_name_version("@1.0"), None);
        assert_eq!(parse_name_version("name@"), None);
    }

    #[test]
    fn sub_dirs_under_data_root() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prev = std::env::var(MCC_SYSTEM_ENV).ok();
        let unique = format!("/tmp/mcc-data_dir-test-{}-{}", std::process::id(), line!());
        std::env::set_var(MCC_SYSTEM_ENV, &unique);
        assert_eq!(mcode_dir(), PathBuf::from(format!("{unique}/mcode")));
        assert_eq!(logs_dir(), PathBuf::from(format!("{unique}/logs")));
        assert_eq!(config_dir(), PathBuf::from(format!("{unique}/config")));
        assert_eq!(index_file(), PathBuf::from(format!("{unique}/index.json")));
        match prev {
            Some(v) => std::env::set_var(MCC_SYSTEM_ENV, v),
            None => std::env::remove_var(MCC_SYSTEM_ENV),
        }
    }
}
