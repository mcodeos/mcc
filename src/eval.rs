// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Compatibility shim (U383 module consolidation): the computation core lives
//! in the top-level [`crate::meta`] module — one meta-domain module for the
//! read half and the compute half. Every existing `crate::eval::` path keeps
//! resolving through this re-export; consumer migration lands with each
//! consuming batch, and the shim retires when the last one moves.

pub use crate::meta::*;
