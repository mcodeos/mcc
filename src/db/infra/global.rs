// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Process-global process roots.
//!
//! Phase 5: the five system-library definition tables (`mcc_components`,
//! `mcc_modules`, `mcc_interfaces`, `mcc_enums`, `mcc_defines`) are gone —
//! system-lib definitions live per-world in the definition registry and the
//! workspace tables. Only the system/project root paths remain global.

use std::sync::LazyLock;
use std::{path::PathBuf, sync::Mutex};

#[allow(non_upper_case_globals)]
pub(crate) static mcc_system_root: LazyLock<Mutex<PathBuf>> =
    LazyLock::new(|| Mutex::new(PathBuf::new()));
#[allow(non_upper_case_globals)]
pub(crate) static mcc_project_root: LazyLock<Mutex<PathBuf>> =
    LazyLock::new(|| Mutex::new(PathBuf::new()));
