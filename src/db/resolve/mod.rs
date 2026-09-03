// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Unified class-name resolution policy — the single source of truth for the
//! P3→P4→P5 visibility rules.
//!
//! Design: `mcd/docs-new/features/resolve-unification.md`; rules:
//! `mcd/docs-new/features/name-space-global.md` §5.4 (visibility enforcement).
//!
//! All class-name consumers (pass1 semantic parse, pass2 instantiation, LSP
//! goto-def / hover / find-references) resolve through [`Resolver`] so the
//! policy is enforced identically everywhere:
//!
//!   ① `RefDefMap` name_index bucket under `(F, name)` — P3 (own file) + P4
//!      (use chain); the deterministic winner (`get_by_name`) resolves, while
//!      the whole bucket (every same-name candidate with its layer) stays
//!      queryable for visibility checks
//!   ② per-world system-lib registry lookup — P5 (mcode system library,
//!      visible inside the world that loaded it)
//!
//! A workspace-wide name-only scan is forbidden (§5.4.3): loading a file into
//! the workspace makes its symbols importable by other files via `use`, but
//! does NOT make them visible to every file by name.

pub(crate) mod member;
pub(crate) mod policy;
pub(crate) mod visibility;

pub(crate) use policy::cmie_uri;
pub use policy::Resolver;
pub use visibility::is_visible;
pub(crate) use visibility::use_chain_reaches;
