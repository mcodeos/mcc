// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Cross-platform data directory management
//!
//! ## Data directory
//!
//! Priority: `$MCC_SYSTEM_ROOT` > local mc/ directory > `~/.mcode`
//!
//! ## Directory structure
//!
//! ```
//! <MCC_SYSTEM_ROOT>/
//! ├── mcode/           # mcode official base library
//! ├── ti.mcu/         # TI library
//! ├── stm32/          # STM32 library
//! ├── infineon/       # Infineon library
//! ├── logs/           # Log directory
//! └── config/         # Config files
//! ```

use std::path::PathBuf;
use tracing::debug;

pub const MCC_SYSTEM_ENV: &str = "MCC_SYSTEM_ROOT";

/// MCC data root directory.
/// Priority: `$MCC_SYSTEM_ROOT` > local mc/ directory > `~/.mcode`
pub fn data_root() -> PathBuf {
    if let Ok(val) = std::env::var(MCC_SYSTEM_ENV) {
        return PathBuf::from(val);
    }
    if let Some(home) = dirs::home_dir() {
        home.join(".mcode")
    } else {
        PathBuf::from(".mcode")
    }
}

pub fn mcode_dir() -> PathBuf {
    data_root().join("mcode")
}
pub fn logs_dir() -> PathBuf {
    data_root().join("logs")
}
pub fn config_dir() -> PathBuf {
    data_root().join("config")
}

pub fn log_file() -> PathBuf {
    logs_dir().join("mcc.log")
}
pub fn pid_file() -> PathBuf {
    logs_dir().join("mcc.pid")
}

/// Ensure all necessary directories exist. Called once at startup.
pub fn ensure_dirs() -> std::io::Result<()> {
    for d in [mcode_dir(), logs_dir(), config_dir()] {
        if !d.exists() {
            std::fs::create_dir_all(&d)?;
            debug!(target: "mcc::dirs", path = ?d, "created");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_override() {
        std::env::set_var(MCC_SYSTEM_ENV, "/tmp/mcc-test");
        assert_eq!(data_root(), PathBuf::from("/tmp/mcc-test"));
        std::env::remove_var(MCC_SYSTEM_ENV);
    }
}
