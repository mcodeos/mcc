// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shim module — forwards to new `db/`, `build/`, and `semantic/` locations.
//! Will be removed when `builder/` is fully deleted.

// Re-export from new locations
pub(crate) use crate::db::cmie::tables as workspace;
pub(crate) use crate::db::diagnostic::diagnostic;
pub(crate) use crate::db::infra::context as current_uri;
pub(crate) use crate::db::infra::global;
pub(crate) use crate::db::infra::lib_mgr;
pub(crate) use crate::db::infra::mc_code;
pub(crate) use crate::db::infra::mc_use;
pub(crate) use crate::db::infra::util;
pub(crate) use crate::semantic::inst_ref as inst_ref_validator;

// Re-export main functions from their new home in build/
pub use crate::build::main as main;
pub use crate::build::main::*;
pub use crate::db::infra::lib_mgr::*;
