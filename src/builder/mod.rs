// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shim module — forwards to new `db/`, `build/`, `query/`, and `semantic/` locations.

// Re-export from new locations
pub(crate) use crate::db::cmie::tables as workspace;
pub(crate) use crate::db::diagnostic::diagnostic;
pub(crate) use crate::db::infra::context as current_uri;
pub(crate) use crate::db::infra::global;
pub(crate) use crate::db::infra::lib_mgr;
pub(crate) use crate::db::infra::mc_code;
pub(crate) use crate::db::infra::util;
pub(crate) use crate::semantic::inst_ref as inst_ref_validator;

// Re-export functions from their new split homes
pub use crate::build::loader::*;
pub use crate::build::pass1::*;
pub use crate::build::pass2::*;
pub(crate) use crate::db::cmie::cmie::*;
pub use crate::db::infra::init::*;
pub use crate::db::infra::lib_mgr::*;
pub use crate::query::debug::*;
pub use crate::query::iterators::*;
pub use crate::query::lookup::*;
pub use crate::query::refs::*;
