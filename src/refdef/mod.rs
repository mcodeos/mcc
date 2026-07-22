// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Unified Ref/Def resolution module (see design doc §16).
//!
//! Centralizes symbol type definitions, registration, ref collection,
//! ref→def matching, and query APIs previously scattered across
//! `ast/ast_semantic.rs` and `db/infra/mc_code.rs`.

pub mod types;
pub mod register;
pub mod matching;
pub mod collect;
pub mod query;

// Re-export all type definitions for convenience
pub use types::*;
