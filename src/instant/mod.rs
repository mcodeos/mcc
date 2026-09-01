// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

pub(crate) mod arena;
pub(crate) mod dianlu;
pub(crate) mod identity;
pub(crate) mod insttab;
pub(crate) mod lane;
pub(crate) mod mc_bus;
pub(crate) mod mc_comp;
pub(crate) mod mc_mod;
pub mod mc_net;
pub(crate) mod net_store;
pub mod netcheck;
pub(crate) mod provenance;

/// Reset the R05 UNRESOLVED_UNIT counter (call before each build run).
pub fn reset_r05_counter() {
    crate::semantic::basic::mc_param::reset_r05_counter();
}
