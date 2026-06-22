// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ NEW — Builder-phase debug output
//!
//! Analogous to `instant/mc_mod/debug_dump`: before and after each `McModuleInst → McVecBlock`
//! conversion, prints input/output comparison to help locate "why a certain net didn't appear in the diagram".
//!
//! ## Enabling
//! Set environment variable `MC_VEC_DUMP=1` (or any non-empty value other than `0`/`false`). Disabled by default, zero overhead.
//!
//! ## Three output sections (printed once per `convert_module` call)
//! - `[VEC-IN ][<name>]`  — Input: this module's connections / sub_modules / labels count
//! - `[VEC-OUT][<name>]`  — Output: this level's insts / nets count, per-ConnectionType distribution
//! - `[VEC-DIFF][<name>]` — Consistency check (has connections but produced 0 nets, etc.)
//!
//! ## Relationship with `viz/debug`
//! This file only concerns builder; `viz/debug` concerns layout/route/render. The two use independent
//! environment variables (`MC_VEC_DUMP` vs `MC_VIZ_DUMP`), can be enabled separately to debug a specific phase.

use std::sync::OnceLock;

use super::super::model::ConnectionType;
use super::super::model::McVecBlock;
use crate::instant::mc_mod::McModuleInst;

// ============================================================================
// Enable check
// ============================================================================

static DUMP_ENABLED: OnceLock<bool> = OnceLock::new();

/// Check whether `MC_VEC_DUMP` is enabled
pub fn dump_enabled() -> bool {
    *DUMP_ENABLED.get_or_init(|| match std::env::var("MC_VEC_DUMP") {
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty() || t == "0" || t == "false" || t == "False" || t == "FALSE")
        }
        Err(_) => false,
    })
}

// ============================================================================
// Output functions (called by builder/visit)
// ============================================================================

/// Print input snapshot when entering `convert_module`
///
/// Called by builder at the start of conversion (e.g.):
/// ```ignore
/// fn convert_module(&mut self, inst: &McModuleInst, ...) -> McVecBlock {
///     debug::dump_input(inst);
///     // ... actual conversion ...
///     debug::dump_output(&block);
///     debug::dump_diff(inst, &block);
///     block
/// }
/// ```
pub fn dump_input(inst: &McModuleInst) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VEC-IN ][{}]", inst.name);
    eprintln!("{p} ── BEGIN ────────────────────────────────");
    eprintln!("{} module       = {}", p, inst.def.name);
    eprintln!("{} components   = {}", p, inst.components.len());
    eprintln!("{} sub_modules  = {}", p, inst.sub_modules.len());
    eprintln!("{} ports        = {}", p, inst.ports.len());
    eprintln!("{} connections  = {}", p, inst.connections.len());
    eprintln!("{} buses (labels) = {}", p, inst.get_buses().len());
    eprintln!("{} labels       = {}", p, inst.get_labels().len());
    eprintln!("{p} ── END ──────────────────────────────────");
}

/// Print output snapshot at end of `convert_module`
pub fn dump_output(block: &McVecBlock) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VEC-OUT][{}]", block.name);
    eprintln!("{p} ── BEGIN ────────────────────────────────");
    eprintln!("{} bid          = {}", p, block.bid);
    eprintln!("{} insts        = {}", p, block.insts.len());
    eprintln!("{} nets         = {}", p, block.nets.len());
    eprintln!("{} sub_blocks   = {}", p, block.blocks.len());

    // Per-ConnectionType distribution
    let mut by_type: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    for n in &block.nets {
        let key = match n.connection_type() {
            ConnectionType::OneToOne => "1:1",
            ConnectionType::Broadcast(_) => "broadcast",
            ConnectionType::NtoN(_) => "n:n",
            ConnectionType::Chain => "chain",
            ConnectionType::Complex => "complex",
            ConnectionType::Isolated => "isolated",
        };
        *by_type.entry(key).or_insert(0) += 1;
    }
    let mut types: Vec<_> = by_type.into_iter().collect();
    types.sort_by_key(|x| x.0);
    for (t, n) in types {
        eprintln!("{p}   net[{t}] = {n}");
    }

    // List each net's endpoint count (helps see "is it just 1 endpoint = isolated")
    for n in &block.nets {
        let total = n.total_points();
        let groups = n.nets.len();
        eprintln!(
            "{}   net #{} '{}' -> {} groups, {} total points [{}]",
            p,
            n.nid,
            n.name,
            groups,
            total,
            n.connection_type()
        );
    }
    eprintln!("{p} ── END ──────────────────────────────────");
}

/// Consistency check after `convert_module`
pub fn dump_diff(inst: &McModuleInst, block: &McVecBlock) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VEC-DIFF][{}]", inst.name);

    // Check 1: has connections but no nets
    if !inst.connections.is_empty() && block.nets.is_empty() {
        eprintln!(
            "{} ⚠ {} connections in pass2 but pass2→vec produced 0 nets",
            p,
            inst.connections.len()
        );
    }

    // Check 2: components count vs insts count reconciliation
    let expect_insts = inst.components.len() + inst.sub_modules.len();
    if expect_insts != block.insts.len() {
        eprintln!(
            "{} ⚠ insts mismatch: pass2 has {} (components+submodules) but block has {}",
            p,
            expect_insts,
            block.insts.len()
        );
    }

    // Check 3: sub_modules count vs blocks count reconciliation
    if inst.sub_modules.len() != block.blocks.len() {
        eprintln!(
            "{} ⚠ blocks mismatch: pass2 has {} sub_modules but block has {} sub_blocks",
            p,
            inst.sub_modules.len(),
            block.blocks.len()
        );
    }

    // Check 4: isolated net (only 1 group/endpoint)
    let isolated_count = block
        .nets
        .iter()
        .filter(|n| matches!(n.connection_type(), ConnectionType::Isolated))
        .count();
    if isolated_count > 0 {
        eprintln!("{p} ⚠ {isolated_count} isolated net(s) (< 2 groups, drawn as nothing)");
    }
}
