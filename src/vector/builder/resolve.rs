// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! NetPoint resolution: string path → InstTable global ID
//!
//! ## Two API sets maintained here
//! - **Old API** (`resolve_netpoint` / `np_warn_count` / `reset_np_warn_count`):
//!   Silently swallows errors, only writes warnings to a process-level atomic counter. **deprecated**, kept for compatibility
//! - **★ NEW P02 API** (`resolve_netpoint_v2`): Returns [`ResolveOutcome`],
//!   each resolution's status (direct / owner-fallback / bare-label / failed) as structured records,
//!   for `McVecBuilder` to accumulate into [`super::report::BuilderReport`]
//!
//! ## Resolution levels (in attempt order)
//! 1. **Bracket list**: `sub.[A, B, C]` → split into independent paths
//! 2. **Direct path**: silently try `module_path.path` / `path` / trailing `.` → `/`
//! 3. **Owner fallback** (Iter 7): NetPoint has owner but path can't be resolved, fall back to owner sub-module
//! 4. **Bare-label fallback** (Iter 7): top-level rail using `.N` index, strip last segment and retry

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::instant::insttab::InstTable;
use crate::instant::mc_net::NetPoint;

use super::report::{ResolutionOutcome, ResolutionRecord};

// ============================================================================
// NP_WARN_COUNT: process-level atomic counter (compat for old callers)
// ============================================================================

static NP_WARN_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Returns the current process-wide cumulative "NetPoint not found" warning count
///
/// **deprecated** — new code should use `McVecBuilder::report()`.
/// This API is kept for `mcviz.rs` compatibility.
pub fn np_warn_count() -> usize {
    NP_WARN_COUNT.load(Ordering::Relaxed)
}

/// Reset "NetPoint not found" warning count to zero
pub fn reset_np_warn_count() {
    NP_WARN_COUNT.store(0, Ordering::Relaxed);
}

// ============================================================================
// ★ NEW P02 API: ResolveOutcome
// ============================================================================

/// Complete result of a single NetPoint resolution
///
/// Compared to old `Vec<i64>`, adds "process metadata" — which fallback level was used.
/// For accumulation into `McVecBuilder.report.resolutions`.
#[derive(Debug, Clone, Default)]
pub struct ResolveOutcome {
    /// Resolved InstTable ID list (bracket may produce multiple)
    pub ids: Vec<i64>,
    /// Records for each attempted path
    pub records: Vec<ResolutionRecord>,
}

impl ResolveOutcome {
    fn empty() -> Self {
        Self::default()
    }
}

// ============================================================================
// ★ NEW P02 API: resolve_netpoint_v2
// ============================================================================

/// Resolve `NetPoint` to zero or more InstTable global IDs + full diagnostics
///
/// Differs from old `resolve_netpoint`:
/// - Does not touch `NP_WARN_COUNT` (unless called from old entry point)
/// - Returns `ResolveOutcome` containing per-step `ResolutionRecord`
/// - Caller puts records into `BuilderReport.resolutions`
pub fn resolve_netpoint_v2(
    table: &InstTable,
    point: &NetPoint,
    module_path: &str,
    net_name: &str,
) -> ResolveOutcome {
    let mut out = ResolveOutcome::empty();

    // Iter 2: bracket list expansion
    if let Some(expanded_paths) = expand_bracket_list(&point.path) {
        for p in &expanded_paths {
            let id_opt = try_resolve_path(table, p, module_path);
            let member = p
                .rsplit_once('.')
                .map(|(_, t)| t.to_string())
                .unwrap_or_else(|| p.clone());
            match id_opt {
                Some(id) => {
                    out.ids.push(id);
                    out.records.push(ResolutionRecord {
                        module_path: module_path.into(),
                        net_name: net_name.into(),
                        point_path: p.clone(),
                        outcome: ResolutionOutcome::BracketExpanded { member },
                    });
                }
                None => {
                    out.records.push(ResolutionRecord {
                        module_path: module_path.into(),
                        net_name: net_name.into(),
                        point_path: p.clone(),
                        outcome: ResolutionOutcome::Failed,
                    });
                }
            }
        }
        return out;
    }

    // Iter 7: single path — try silently first
    if let Some(id) = try_resolve_path(table, &point.path, module_path) {
        out.ids.push(id);
        out.records.push(ResolutionRecord {
            module_path: module_path.into(),
            net_name: net_name.into(),
            point_path: point.path.clone(),
            outcome: ResolutionOutcome::Direct,
        });
        return out;
    }

    // Iter 7: owner fallback
    if let Some(owner) = &point.owner {
        let candidates = [format!("{module_path}.{owner}"), owner.clone()];
        for cand in &candidates {
            if let Some(id) = table.get_id_by_path(cand) {
                out.ids.push(id as i64);
                out.records.push(ResolutionRecord {
                    module_path: module_path.into(),
                    net_name: net_name.into(),
                    point_path: point.path.clone(),
                    outcome: ResolutionOutcome::OwnerFallback,
                });
                return out;
            }
        }
    }

    // Iter 7: bare-label fallback (only when owner=None)
    if point.owner.is_none() {
        if let Some((stem, _tail)) = point.path.rsplit_once('.') {
            if !stem.is_empty() {
                let candidates = [format!("{module_path}.{stem}"), stem.to_string()];
                for cand in &candidates {
                    if let Some(id) = table.get_id_by_path(cand) {
                        out.ids.push(id as i64);
                        out.records.push(ResolutionRecord {
                            module_path: module_path.into(),
                            net_name: net_name.into(),
                            point_path: point.path.clone(),
                            outcome: ResolutionOutcome::BareLabelFallback,
                        });
                        return out;
                    }
                }
            }
        }
    }

    // ── ★ Phase D: bracket-port-member fallback ─────────────────────────────────
    //
    // Symptom: hbl mcu513 module declares ports as `[VDD_3V3, GND]` and `[VCC_1V2, GND]`
    // (registered in InstTable as `main.mcu513.[VDD_3V3, GND]` — bracket-form Port path),
    // mcu513's body references bare names `VDD_3V3` or `VCC_1V2` — all three fallbacks miss,
    // entire net "all dropped", cap5.1 etc. decoupling capacitor connections to power rails silently deleted.
    //
    // Add a new fallback level here: scan all Port entries under this module; if a Port path matches
    // `<module_path>.[<m1>, <m2>, ...]` and the bare name we're looking for == some `<mi>`, return
    // that Port's id as a hit. Semantically **equivalent to** "bare name X refers to member X of
    // mcu513's bracket port [X, ...]", attaching the corresponding connection to that Port in topology —
    // consistent behavior with explicitly writing `mcu513.[VDD_3V3, GND]` port reference.
    //
    // Trigger conditions:
    //   - point.path is a **bare name** (no `.`), point.owner is None
    //   - direct / owner / bare-label three-level fallback all miss
    //
    // Side effects:
    //   - If user source has ambiguity (e.g. cap5.1 simultaneously connects to VDD_3V3 / VCC_1V2 / GND,
    //     across 3 rails), all three connections now resolve successfully, naturally split into 3
    //     separate nets by visit.rs's net_groups (because net_name differs) — visually cap5.1 appears
    //     across 3 nets, hinting that the source has issues, no longer silently swallowed.
    //   - Does not affect the normal path where "a truly independent Port exists" (that already hits
    //     at the Direct step, never reaches here).
    if point.owner.is_none() && !point.path.contains('.') && !point.path.is_empty() {
        let bare_name = &point.path;
        if let Some(mod_id) = table.get_id_by_path(module_path) {
            for port in table.get_ports_of(mod_id) {
                if let Some(members) = parse_bracket_port_members(&port.path, module_path) {
                    if members.iter().any(|m| m == bare_name) {
                        crate::velog!(
                            "[Phase-D] bracket-port-member hit: bare name '{}' in '{}' \
                             → port '{}' (id={})",
                            bare_name,
                            module_path,
                            port.path,
                            port.id
                        );
                        out.ids.push(port.id as i64);
                        out.records.push(ResolutionRecord {
                            module_path: module_path.into(),
                            net_name: net_name.into(),
                            point_path: point.path.clone(),
                            outcome: ResolutionOutcome::BracketPortMember {
                                member: bare_name.clone(),
                                port_path: port.path.clone(),
                            },
                        });
                        return out;
                    }
                }
            }
        }
    }

    // All fallbacks failed
    out.records.push(ResolutionRecord {
        module_path: module_path.into(),
        net_name: net_name.into(),
        point_path: point.path.clone(),
        outcome: ResolutionOutcome::Failed,
    });
    out
}

/// ── ★ Phase D helper ────────────────────────────────────────────────────────
///
/// Checks if port_path matches `<module_path>.[<m1>, <m2>, ...]` and returns member list.
///
/// Example:
///   - `parse_bracket_port_members("main.mcu513.[VDD_3V3 "main.mcu "main.mcu513")`
///     → `Some(["VDD_3V3", "GND"])`
///   - `parse_bracket_port_members("main.mcu513.SPI", "main.mcu513")` → `None`
///
/// Rules follow `insttab::expand_bracket_list`:
///   - Must start with `<module_path>.[` prefix
///   - Must end with `]
///   - body split by `,` + trim, filters empty members
///   - Does not support nested / extra content between members
fn parse_bracket_port_members(port_path: &str, module_path: &str) -> Option<Vec<String>> {
    let prefix = format!("{module_path}.[");
    if !port_path.starts_with(&prefix) {
        return None;
    }
    if !port_path.ends_with(']') {
        return None;
    }
    let body = &port_path[prefix.len()..port_path.len() - 1];
    if body.is_empty() {
        return None;
    }
    let members: Vec<String> = body
        .split(',')
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

// ============================================================================
// Old main entry: resolve_netpoint (deprecated, delegates to v2)
// ============================================================================

/// Parses `NetPoint` to zero or more InstTable global(s).
///
/// **deprecated** —— Use [`resolve_netpoint_v2`] for structured diagnostics.
///
/// This function is kept for compatibility with existing callers, internally delegates to v2
/// and flattens result to `Vec<i64>`, while counting `Failed` into `NP_WARN_COUNT` and printing to stderr (preserving old behavior).
pub fn resolve_netpoint(table: &InstTable, point: &NetPoint, module_path: &str) -> Vec<i64> {
    // v2 doesn't need net_name (it's just metadata), use placeholder
    let outcome = resolve_netpoint_v2(table, point, module_path, "");

    // Count Failed + print, consistent with old API behavior
    for rec in &outcome.records {
        match &rec.outcome {
            ResolutionOutcome::Failed => {
                NP_WARN_COUNT.fetch_add(1, Ordering::Relaxed);
                crate::velog!(
                    "[mc_vec_builder] Warning: NetPoint '{}' not found (module: {})",
                    rec.point_path,
                    module_path
                );
            }
            ResolutionOutcome::OwnerFallback => {
                crate::velog!(
                    "[mc_vec_builder] Iter7 owner-fallback: '{}' (module: {})",
                    rec.point_path,
                    module_path
                );
            }
            ResolutionOutcome::BareLabelFallback => {
                crate::velog!(
                    "[mc_vec_builder] Iter7 bare-label-fallback: '{}' (module: {})",
                    rec.point_path,
                    module_path
                );
            }
            ResolutionOutcome::BracketPortMember { member, port_path } => {
                crate::velog!(
                    "[mc_vec_builder] Phase-D bracket-port-member: '{}' → member '{}' of '{}' (module: {})",
                    rec.point_path, member, port_path, module_path
                );
            }
            _ => {}
        }
    }

    outcome.ids
}

// ============================================================================
// Single path resolution (silent version)
// ============================================================================

/// Silent version: no warning, no counter increment
///
/// Three-level fallback:
/// 1. `module_path.path`
/// 2. `path` directly
/// 3. Last `.` → `/` (bus member)
pub fn try_resolve_path(table: &InstTable, path: &str, module_path: &str) -> Option<i64> {
    let full_path = format!("{module_path}.{path}");

    if let Some(id) = table.get_id_by_path(&full_path) {
        return Some(id as i64);
    }
    if let Some(id) = table.get_id_by_path(path) {
        return Some(id as i64);
    }
    for candidate in [full_path.as_str(), path] {
        if let Some(pos) = candidate.rfind('.') {
            let bus_style = format!("{}/{}", &candidate[..pos], &candidate[pos + 1..]);
            if let Some(id) = table.get_id_by_path(&bus_style) {
                return Some(id as i64);
            }
        }
    }
    None
}

/// With warning + counter increment
///
/// For diagnostic use of individual bracket members.
///
/// **deprecated**: Use `resolve_netpoint_v2`.
pub fn resolve_path(table: &InstTable, path: &str, module_path: &str) -> Option<i64> {
    if let Some(id) = try_resolve_path(table, path, module_path) {
        return Some(id);
    }
    NP_WARN_COUNT.fetch_add(1, Ordering::Relaxed);
    crate::velog!("[mc_vec_builder] Warning: NetPoint '{path}' not found (module: {module_path})");
    None
}

// ============================================================================
// Bracket list expansion
// ============================================================================

/// Expands `<prefix>.[<m1>, <m2>, ...]` into individual path lists
///
/// Mismatch / Invalid syntax / Empty member → Returns `None`, caller treats as single path.
pub fn expand_bracket_list(path: &str) -> Option<Vec<String>> {
    let open = path.find(".[")?;
    if !path.ends_with(']') {
        return None;
    }
    let close = path.len() - 1;
    if close <= open + 2 {
        return None;
    }
    let prefix = &path[..open];
    if prefix.is_empty() {
        return None;
    }
    let body = &path[open + 2..close];
    let members: Vec<String> = body
        .split(',')
        .map(|m| m.trim())
        .filter(|m| !m.is_empty())
        .map(|m| format!("{prefix}.{m}"))
        .collect();
    if members.is_empty() {
        None
    } else {
        Some(members)
    }
}

// ============================================================================
// resolve_id: Simple path-based ID lookup
// ============================================================================

/// Looks up InstTable ID by path, returns -1 (with warning)
pub fn resolve_id(table: &InstTable, path: &str) -> i64 {
    table
        .get_id_by_path(path)
        .map(|id| id as i64)
        .unwrap_or_else(|| {
            crate::velog!("[mc_vec_builder] Warning: path '{path}' not found");
            -1
        })
}
