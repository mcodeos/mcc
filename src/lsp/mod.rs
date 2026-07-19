// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Language Server Protocol — protocol-independent LSP business logic.
//!
//! Extracted from `db/infra/mc_code.rs` (semantic token/symbol assembly) and
//! `rpc/handlers.rs` (goto-def, references, hover, completion, diagnostics).

pub mod completion;
pub mod diagnostics;
pub mod gotodef;
pub mod hover;
pub mod references;
pub mod sem;
pub mod symbols;
