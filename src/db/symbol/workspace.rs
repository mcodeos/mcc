// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! LSP Symbol workspace tables.
//!
//! Currently defined in `db/cmie/tables.rs` alongside the `WorkspaceManager`:
//! - `GlobalInstTable` — cross-file instance declaration table
//! - `global_class_table` — (uri, kind, class_name) → (class_id, span)
//! - `global_declare_class_refs` — declare class references
//!
//! These are accessed via `workspace::WORKSPACE.global_inst_table` etc.
//!
//! TODO: Extract these types from `db/cmie/tables.rs` into this file when
//! the WorkspaceManager is refactored to support trait-based injection (Phase 7).
