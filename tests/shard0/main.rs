// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shard binary: many former `tests/*.rs` test targets merged into one, so a
//! change in `src/` relinks a handful of binaries instead of 138.

#[path = "../common/mod.rs"]
pub mod common;

mod rule_audit_a;
mod u151_label_boundary;
mod u153_anon_port_func_formal;
mod u152_body_face_decl_order;
mod u152c6_pin_canonical_order;
