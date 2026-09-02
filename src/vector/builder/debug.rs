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

use super::super::model::McVecBlock;
use crate::instant::inststore::TreeView;
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

/// `eprintln!`-compatible macro that only prints when `MC_VEC_DUMP` is enabled.
/// Used to gate the vector builder / graph pipeline's per-net / per-box progress.
#[macro_export]
macro_rules! velog {
    ($($arg:tt)*) => {
        if $crate::vector::builder::debug::dump_enabled() {
            eprintln!($($arg)*);
        }
    };
}

// ============================================================================
// Output functions (called by builder/visit)
// ============================================================================

/// Print input snapshot when entering `convert_module`
///
/// `view` supplies the component/sub-module counts from the Phase C store
/// (design §4) — the tree's own children Vecs are gone. Called by builder at
/// the start of conversion (e.g.):
/// ```ignore
/// fn convert_module(&mut self, inst: &McModuleInst, view: &TreeView, ...) -> McVecBlock {
///     debug::dump_input(inst, view);
///     // ... actual conversion ...
///     debug::dump_output(&block);
///     debug::dump_diff(inst, &block, view);
///     block
/// }
/// ```
pub fn dump_input(inst: &McModuleInst, view: &TreeView) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VEC-IN ][{}]", inst.name);
    mcc_dbg!("vec", "{p} ── BEGIN ────────────────────────────────");
    mcc_dbg!("vec", "{} module       = {}", p, inst.def.name);
    let components = view.components(inst).count();
    let sub_modules = view.sub_modules(inst).count();
    mcc_dbg!("vec", "{} components   = {}", p, components);
    mcc_dbg!("vec", "{} sub_modules  = {}", p, sub_modules);
    mcc_dbg!("vec", "{} ports        = {}", p, inst.ports.len());
    mcc_dbg!("vec", "{} connections  = {}", p, inst.connections.len());
    // Phase E: labels/buses no longer live on the tree — they are read from
    // the frozen store fragments by the projection consumers, so the input
    // snapshot stops printing their counts here.
    mcc_dbg!("vec", "{p} ── END ──────────────────────────────────");
}

/// Print output snapshot at end of `convert_module`
pub fn dump_output(block: &McVecBlock) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VEC-OUT][{}]", block.name);
    mcc_dbg!("vec", "{p} ── BEGIN ────────────────────────────────");
    mcc_dbg!("vec", "{} bid          = {}", p, block.bid);
    mcc_dbg!("vec", "{} insts        = {}", p, block.insts.len());
    mcc_dbg!("vec", "{} nets         = {}", p, block.nets.len());
    mcc_dbg!("vec", "{} sub_blocks   = {}", p, block.blocks.len());

    // Per-ConnectionType distribution (NetShape-based)
    let mut by_type: std::collections::HashMap<&'static str, usize> =
        std::collections::HashMap::new();
    for n in &block.nets {
        let key = n.shape_type_key();
        *by_type.entry(key).or_insert(0) += 1;
    }
    let mut types: Vec<_> = by_type.into_iter().collect();
    types.sort_by_key(|x| x.0);
    for (t, n) in types {
        mcc_dbg!("vec", "{p}   net[{t}] = {n}");
    }

    // List each net's endpoint count (helps see "is it just 1 endpoint = isolated")
    for n in &block.nets {
        let total = n.total_points();
        let groups = n.nets.len();
        mcc_dbg!(
            "vec",
            "{}   net #{} '{}' -> {} groups, {} total points [{}]",
            p,
            n.nid,
            n.name,
            groups,
            total,
            n.shape_type_name()
        );
    }
    mcc_dbg!("vec", "{p} ── END ──────────────────────────────────");
}

/// Consistency check after `convert_module`
pub fn dump_diff(inst: &McModuleInst, block: &McVecBlock, view: &TreeView) {
    if !dump_enabled() {
        return;
    }
    let p = format!("[VEC-DIFF][{}]", inst.name);

    let components = view.components(inst).count();
    let sub_modules = view.sub_modules(inst).count();

    // Check 1: has connections but no nets
    if !inst.connections.is_empty() && block.nets.is_empty() {
        mcc_dbg!(
            "vec",
            "{} ⚠ {} connections in pass2 but pass2→vec produced 0 nets",
            p,
            inst.connections.len()
        );
    }

    // Check 2: components count vs insts count reconciliation
    let expect_insts = components + sub_modules;
    if expect_insts != block.insts.len() {
        mcc_dbg!(
            "vec",
            "{} ⚠ insts mismatch: pass2 has {} (components+submodules) but block has {}",
            p,
            expect_insts,
            block.insts.len()
        );
    }

    // Check 3: sub_modules count vs blocks count reconciliation
    if sub_modules != block.blocks.len() {
        mcc_dbg!(
            "vec",
            "{} ⚠ blocks mismatch: pass2 has {} sub_modules but block has {} sub_blocks",
            p,
            sub_modules,
            block.blocks.len()
        );
    }

    // Check 4: isolated net (only 1 group/endpoint)
    let isolated_count = block
        .nets
        .iter()
        .filter(|n| n.shape_type_key() == "isolated")
        .count();
    if isolated_count > 0 {
        mcc_dbg!(
            "vec",
            "{p} ⚠ {isolated_count} isolated net(s) (< 2 groups, drawn as nothing)"
        );
    }
}
