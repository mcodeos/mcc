// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Compatibility shim (U383 module restructure): the computation core moved to
//! the top-level [`crate::quantity`] module. Every existing `crate::eval::`
//! path keeps resolving through these re-exports; consumer migration lands
//! with each consuming batch, and the shim retires when the last one moves.

pub use crate::quantity::*;
