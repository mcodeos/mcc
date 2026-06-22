// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

pub(crate) mod current_uri;
pub mod diagnostic;
pub mod global;
pub(crate) mod inst_ref_validator;
pub mod lib_mgr;
pub mod main;
pub mod mc_code;
pub mod mc_use;
pub(crate) mod util;
pub(crate) mod workspace;
pub use lib_mgr::*;
pub use main::*;
