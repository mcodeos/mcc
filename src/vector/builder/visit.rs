// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Main conversion driver: recursive `McModuleInst` tree → `McVecBlock` tree
//!
//! [`McVecBuilder`] holds `&InstTable` reference, responsible for scheduling:
//! - [`super::resolve`]    Resolve NetPoint
//! - [`super::connection`] Analyze connection pair topology
//! - [`super::debug`]      MC_VEC_DUMP=1 debug logging
//!
//! ## ★ P02 (S1) changes
//! - `McVecBuilder` holds [`BuilderReport`], accumulates all NetPoint resolve results
//! - Added [`McVecBuilder::with_mode`] to switch [`BuildMode`] (Tolerant / Strict / NoDataLoss)
//! - Added [`McVecBuilder::try_build`] returning `Result<McVecBlock, BuilderError>`
//! - Old [`McVecBuilder::build`] behavior unchanged (Tolerant + always Ok)
//!
//! ## Flow
//! ```text
//! For each McModuleInst (recursive from top):
//!   1. Look up this module's bid via InstTable (failure → report.unresolved_modules)
//!   2. Iterate components → collect insts IDs
//!   3. Iterate connections → group by net_name → topology analysis → generate McVecNet
//!      ↑ Use resolve_netpoint_v2, all records go into report
//!      ↑ After resolve, remaining ids < 2 → report.dropped_nets
//!      ↑ Has failed points but ≥ 2 resolved successfully → report.partial_nets
//!   4. Iterate sub_modules → recursively generate child McVecBlock
//! ```

use std::collections::HashMap;

use crate::instant::inst_table::{InstKind, InstTable};
use crate::instant::mc_mod::McModuleInst;

use super::super::model::McVecBlock;
use super::builder_report::{
    BuildMode, BuilderError, BuilderReport, DroppedNet, PartialNet, ResolutionOutcome,
};
use crate::builder::diagnostic::{diagnotic_log, DiagnosticLevel};

use super::connection::{merge_pairs_to_vecnet, ConnPair, NetGroupMap};
use super::debug;
use super::resolve::resolve_netpoint_v2;

// ============================================================================
// McVecBuilder
// ============================================================================

/// Pass2 → McVecBlock converter
///
/// Holds `InstTable` reference to resolve global IDs, recursively traverses `McModuleInst` tree.
///
/// **P02**: Also holds `BuilderReport` to accumulate diagnostics, and `BuildMode` to decide error tolerance strategy.
pub struct McVecBuilder<'a> {
    /// Flattened instance table (provides globally unique ID + path lookup)
    inst_table: &'a InstTable,
    /// Net ID counter (unique across all levels)
    net_id_counter: i64,
    /// ★ NEW P02: Accumulated diagnostics
    report: BuilderReport,
    /// ★ NEW P02: Error tolerance strategy
    mode: BuildMode,
}

impl<'a> McVecBuilder<'a> {
    /// Create converter (default Tolerant mode)
    pub fn new(inst_table: &'a InstTable) -> Self {
        Self {
            inst_table,
            net_id_counter: 0,
            report: BuilderReport::new(),
            mode: BuildMode::Tolerant,
        }
    }

    /// ★ NEW P02: Switch error tolerance mode
    pub fn with_mode(mut self, mode: BuildMode) -> Self {
        self.mode = mode;
        self
    }

    /// Current mode
    pub fn mode(&self) -> BuildMode {
        self.mode
    }

    /// Get current accumulated report (does not consume builder)
    pub fn report(&self) -> &BuilderReport {
        &self.report
    }

    /// Entry (compat): Build complete `McVecBlock` tree from top-level `McModuleInst`
    ///
    /// **Behavior**:
    /// - Tolerant: Always returns a block with data, even with dropped/partial nets
    /// - Strict/NoDataLoss: **Still returns block, does not error**; check via `report()`,
    ///   or use new [`McVecBuilder::try_build`] for `Result`
    pub fn build(&mut self, root: &McModuleInst) -> McVecBlock {
        let block = self.convert_module(root, "");
        // Print summary once at end of build by default
        self.report.print_summary();
        block
    }

    /// ★ NEW P02: Entry (new), returns Result based on mode
    ///
    /// - `Tolerant` → always `Ok`
    /// - `NoDataLoss` → `Err(PartialNets)` when partial_nets exist
    /// - `Strict` → `Err(DataLoss)` on any drop/partial/unresolved
    pub fn try_build(&mut self, root: &McModuleInst) -> Result<McVecBlock, BuilderError> {
        let block = self.convert_module(root, "");
        self.report.print_summary();

        match self.mode {
            BuildMode::Tolerant => Ok(block),
            BuildMode::NoDataLoss => {
                if !self.report.partial_nets.is_empty() {
                    Err(BuilderError::PartialNets(self.report.clone()))
                } else {
                    Ok(block)
                }
            }
            BuildMode::Strict => {
                if self.report.is_clean() {
                    Ok(block)
                } else {
                    Err(BuilderError::DataLoss(self.report.clone()))
                }
            }
        }
    }

    // ========================================================================
    // Phase 1: Recursive traversal — each McModuleInst → McVecBlock
    // ========================================================================

    fn convert_module(&mut self, inst: &McModuleInst, prefix: &str) -> McVecBlock {
        // ── Debug: input snapshot (printed when MC_VEC_DUMP=1) ──
        debug::dump_input(inst);

        // 1. Determine this module's path and ID
        let my_path = if prefix.is_empty() {
            inst.name.clone()
        } else {
            format!("{}.{}", prefix, inst.name)
        };

        // ★ P02: Use get_id_by_path directly, record failure in unresolved_modules
        let bid = match self.inst_table.get_id_by_path(&my_path) {
            Some(id) => id as i64,
            None => {
                self.report.unresolved_modules.push(my_path.clone());
                crate::velog!("[mc_vec_builder] Warning: module path '{my_path}' not found");
                -1
            }
        };

        let mut block = McVecBlock::new(bid, inst.name.clone());

        // 2. Collect this level's component instance IDs
        //
        // ★ S3.5 change: Detailed diagnostics — print to stderr when each component path resolve fails
        // If upstream visit phase drops a component, downstream from_block.rs can only see empty insts
        // and will use Phase 1.5 to substitute endpoints as PowerLabel, causing ICs/resistors/capacitors to not render at all.
        if !inst.components.is_empty() {
            crate::velog!(
                "[visit] '{}': {} component(s) declared in pass2",
                my_path,
                inst.components.len()
            );
        }
        for comp in &inst.components {
            let comp_path = format!("{}.{}", my_path, comp.name);
            match self.inst_table.get_id_by_path(&comp_path) {
                Some(comp_id) => {
                    crate::velog!("[visit]   ✓ '{comp_path}' → id={comp_id}");
                    block.insts.push(comp_id as i64);
                }
                None => {
                    crate::velog!(
                        "[visit]   ✗ MISSING '{comp_path}' (component declared in pass2 but \
                         InstTable.get_id_by_path returned None — pass1/pass2 \
                         registration bug?)"
                    );
                    self.report
                        .unresolved_modules
                        .push(format!("{comp_path} (component)"));
                }
            }
        }

        // 3. Build McVecNet from connections
        block.nets = self.build_nets_from_connections(inst, &my_path);

        // 4. Recursively process sub-modules
        if !inst.sub_modules.is_empty() {
            crate::velog!(
                "[visit] '{}': {} sub_module(s) declared in pass2",
                my_path,
                inst.sub_modules.len()
            );
        }
        for sub in &inst.sub_modules {
            let sub_path = format!("{}.{}", my_path, sub.name);
            match self.inst_table.get_id_by_path(&sub_path) {
                Some(sub_id) => {
                    crate::velog!("[visit]   ✓ sub '{sub_path}' → id={sub_id}");
                    block.insts.push(sub_id as i64);
                }
                None => {
                    crate::velog!("[visit]   ✗ MISSING sub '{sub_path}'");
                }
            }
            let sub_block = self.convert_module(sub, &my_path);
            block.blocks.push(sub_block);
        }

        // ── FIX: Use InstTable as authority, backfill this level's structural child instances by parent_id ──
        // The instantiated sub-module body may be empty in this McModuleInst,
        // but InstTable has already registered all Components/Modules during flatten phase.
        if bid >= 0 {
            let existing: std::collections::HashSet<i64> = block.insts.iter().copied().collect();
            for child in self.inst_table.children_of(bid as u32) {
                if matches!(child.kind, InstKind::Component | InstKind::Module)
                    && !existing.contains(&(child.id as i64))
                {
                    crate::velog!(
                        "[visit]   + backfilled '{}' (id={}, kind={}) from InstTable",
                        child.path,
                        child.id,
                        child.kind
                    );
                    block.insts.push(child.id as i64);
                }
            }
        }

        // ── Debug: output snapshot + consistency check (printed when MC_VEC_DUMP=1) ──
        debug::dump_output(&block);
        debug::dump_diff(inst, &block);

        block
    }

    // ========================================================================
    // Phase 2: Build McVecNet from connections
    //
    // ★ P02 change: Use resolve_netpoint_v2, accumulate each outcome into report
    // ========================================================================

    fn build_nets_from_connections(
        &mut self,
        inst: &McModuleInst,
        module_path: &str,
    ) -> Vec<super::super::model::McVecNet> {
        // Step 1: Group by net_name, collect all connection pairs
        let mut net_groups: NetGroupMap = NetGroupMap::new();

        for conn in &inst.connections {
            let net_name = conn.effective_net_name();

            // ── ★ P0-1: Bracket-structure-aware resolve ─────────────────────────────────
            //
            // Old implementation flattened all conn.points resolve results into all_ids, then
            // used `windows(2)` sliding-window pairing. This is correct for ordinary chain
            // connections (`A - B - C`), but catastrophically wrong for bracket-expanded connections:
            //
            //   `moddcdc.[VDD_3V3, GND] -> [V3V3, GND]`
            //     resolve → [vdd3v3_id, gnd_id, v3v3_id, gnd2_id]
            //     windows(2) → (vdd3v3, gnd), (gnd, v3v3), (v3v3, gnd2)
            //     ↑↑↑ The middle pair (gnd, v3v3) shorts "ground" and "3V3" ↑↑↑
            //
            // And these ConnPairs all fall under the same effective_net_name (e.g. "V3V3"), so
            // the V3V3 net ends up with 4 endpoints including moddcdc.GND → whole-schematic electrical error.
            //
            // New implementation:
            //   - Each NetPoint individually runs resolve_netpoint_v2, gets (ids, members):
            //     non-bracket point ids.len() == 1, members[0] == None
            //     bracket point    ids.len() == N, members[k] == Some("VDD_3V3"/"GND"/...)
            //   - Determine "symmetric bracket connection": all points have width either max_w
            //     or 1 (the latter as broadcast). In this case **split by position into max_w
            //     independent sub-nets**, each using bracket member name as sub-net name
            //     (e.g. split into separate "V3V3" net and "GND" net, mutually uncontaminated).
            //   - Other cases (pure scalar chain / heterogeneous mix) still use the original
            //     windows(2) behavior, preserving chain semantics that hbl already handles.
            //
            // Note: ResolutionOutcome::BracketExpanded { member } already carries the member name,
            // no need for additional path re-resolution.

            struct PointResult {
                ids: Vec<i64>,
                /// Aligned with ids: Some means bracket expanded member; None means scalar / fallback
                members: Vec<Option<String>>,
                /// Whether the entire NetPoint is a bracket expansion (at least one Some in members)
                is_bracket: bool,
            }

            let mut per_point: Vec<PointResult> = Vec::new();
            let mut failed_points: Vec<String> = Vec::new();
            let original_point_count = conn.points.len();

            for p in &conn.points {
                let outcome = resolve_netpoint_v2(self.inst_table, p, module_path, &net_name);

                if outcome.ids.is_empty() {
                    failed_points.push(p.path.clone());
                }

                // Extract BracketExpanded { member } from records, aligned with ids.
                // resolve_netpoint_v2 in the bracket branch pushes one record **per member**
                // (successful BracketExpanded / failed Failed), but ids are only pushed on success.
                // So we scan records, only append member name for successful positions, advancing in sync with ids.
                let mut members: Vec<Option<String>> = Vec::with_capacity(outcome.ids.len());
                let mut is_bracket = false;
                for r in &outcome.records {
                    match &r.outcome {
                        ResolutionOutcome::BracketExpanded { member } => {
                            members.push(Some(member.clone()));
                            is_bracket = true;
                        }
                        ResolutionOutcome::Direct
                        | ResolutionOutcome::OwnerFallback
                        | ResolutionOutcome::BareLabelFallback
                        | ResolutionOutcome::BracketPortMember { .. } => {
                            // Phase D: BracketPortMember is like Direct — "single id hit",
                            // not positional expansion (input is bare name, not `prefix.[A,B]` list), so
                            // goes to None branch — does not participate in downstream symmetric_bracket positional pairing.
                            members.push(None);
                        }
                        ResolutionOutcome::Failed => { /* does not correspond to any id, don't push */
                        }
                    }
                }
                // Defensive: in case records/ids lengths mismatch (theoretically impossible), pad/truncate
                if members.len() != outcome.ids.len() {
                    crate::velog!(
                        "[NET] WARN: outcome.records/ids length mismatch (records={}, ids={}) \
                         for point '{}' net '{}'; falling back to all-None member annotation",
                        members.len(),
                        outcome.ids.len(),
                        p.path,
                        net_name
                    );
                    members = vec![None; outcome.ids.len()];
                    is_bracket = false;
                }

                self.report.resolutions.extend(outcome.records);

                per_point.push(PointResult {
                    ids: outcome.ids,
                    members,
                    is_bracket,
                });
            }

            // ── D2: FLOATING_PLACEHOLDER detection ──────────────────────────
            // Check if any `_` placeholder (path starts with "(lead)_") failed to resolve.
            for (i, p) in conn.points.iter().enumerate() {
                if p.path.starts_with("(lead)_") {
                    if let Some(pr) = per_point.get(i) {
                        if pr.ids.is_empty() {
                            let pos = p.src_pos.unwrap_or(0) as u32;
                            diagnotic_log(
                                2002,
                                DiagnosticLevel::Error,
                                pos,
                                1,
                                &format!(
                                    "FLOATING_PLACEHOLDER: '_' placeholder in net '{}' (module '{}') \
                                     could not be bound to any existing pin. The placeholder is floating.",
                                    net_name, module_path
                                ),
                                &[],
                            );
                        }
                    }
                }
            }

            // ── D3: MERGED_SHORT detection ──────────────────────────────────
            // Check if multiple point paths (same or different) resolve to the
            // same id, indicating a bracket expansion duplicate or a port without
            // bit width causing signal merging.
            {
                let mut id_to_paths: HashMap<i64, Vec<&String>> = HashMap::new();
                for (i, pr) in per_point.iter().enumerate() {
                    for &id in &pr.ids {
                        id_to_paths
                            .entry(id)
                            .or_default()
                            .push(&conn.points[i].path);
                    }
                }
                for (id, paths) in &id_to_paths {
                    // Fire when ≥2 points (even same-named) resolve to the same id.
                    if paths.len() >= 2 {
                        let all_power = paths.iter().all(|p| {
                            let upper = p.to_uppercase();
                            crate::vector::graph::naming::is_power_rail(&upper)
                        });
                        if !all_power {
                            let mut unique_paths: Vec<&String> = Vec::new();
                            {
                                let mut seen: std::collections::HashSet<&String> =
                                    std::collections::HashSet::new();
                                for p in paths.iter() {
                                    if seen.insert(p) {
                                        unique_paths.push(p);
                                    }
                                }
                            }
                            let pos = conn
                                .points
                                .iter()
                                .find(|p| unique_paths.iter().any(|up| **up == p.path))
                                .and_then(|p| p.src_pos)
                                .unwrap_or(0) as u32;
                            diagnotic_log(
                                2003,
                                DiagnosticLevel::Error,
                                pos,
                                1,
                                &format!(
                                    "MERGED_SHORT: net '{}' (module '{}') has {} point(s) \
                                     resolving to the same node (id={}). Paths: {:?}. \
                                     This may indicate a bracket expansion duplicate or a port \
                                     declared without bit width causing signal merging.",
                                    net_name,
                                    module_path,
                                    paths.len(),
                                    id,
                                    unique_paths
                                ),
                                &[],
                            );
                        }
                    }
                }
            }

            let resolved_point_count: usize = per_point.iter().map(|pr| pr.ids.len()).sum();

            // ★ DEBUG: Print failed points detail for diagnosing point loss
            if !failed_points.is_empty() {
                crate::velog!(
                    "[NET] {}: lost points! net='{}' (module='{}'), failed: {:?}",
                    if resolved_point_count < 2 {
                        "ALL"
                    } else {
                        "PARTIAL"
                    },
                    net_name,
                    module_path,
                    failed_points
                );
            }

            // ★ P02: entire net dropped
            if resolved_point_count < 2 {
                self.report.dropped_nets.push(DroppedNet {
                    module_path: module_path.into(),
                    net_name: net_name.clone(),
                    original_point_count,
                    resolved_point_count: resolved_point_count,
                });
                continue;
            }

            // ★ P02: partial point loss (some points failed but the whole net is still usable)
            if !failed_points.is_empty() {
                self.report.partial_nets.push(PartialNet {
                    module_path: module_path.into(),
                    net_name: net_name.clone(),
                    failed_points,
                    resolved_point_count,
                });
            }

            // ── Pairing strategy selection ─────────────────────────────────────────────────
            // Bracket mode: at least one point is bracket-expanded, and **all non-empty points**
            // have width either equal to max_w (= that bracket's bit width), or 1 (scalar broadcast).
            let widths: Vec<usize> = per_point.iter().map(|pr| pr.ids.len()).collect();
            let max_w = widths.iter().copied().max().unwrap_or(0);
            let any_bracket = per_point.iter().any(|pr| pr.is_bracket);
            let symmetric_bracket = any_bracket
                && max_w >= 2
                && per_point
                    .iter()
                    .all(|pr| pr.ids.is_empty() || pr.ids.len() == max_w || pr.ids.len() == 1);

            if symmetric_bracket {
                // Split into max_w sub-nets by position, sub-net name taken from that position's bracket member name
                // (if that position has no bracket annotation / multiple bracket member names conflict, fall back to
                // effective_net_name + "[k]" suffix, guaranteeing uniqueness).
                crate::velog!(
                    "[NET] bracket-mode net='{}' (module='{}'), width={}, points={} → split into {} sub-nets",
                    net_name,
                    module_path,
                    max_w,
                    per_point.len(),
                    max_w
                );

                for k in 0..max_w {
                    // Get member name at this position (from any bracket point; use the
                    // **first** bracket point's member as authority to avoid alias conflicts)
                    let member_name_opt: Option<String> = per_point
                        .iter()
                        .filter(|pr| pr.is_bracket && pr.ids.len() == max_w)
                        .find_map(|pr| pr.members.get(k).cloned().flatten());

                    let sub_net_name =
                        member_name_opt.unwrap_or_else(|| format!("{net_name}[{k}]"));

                    // Collect id chain at this position:
                    //   - width == max_w point: take ids[k]
                    //   - width == 1 scalar point: broadcast to every sub-net
                    //   - width == 0: skip
                    let mut chain_ids: Vec<i64> = Vec::with_capacity(per_point.len());
                    for pr in &per_point {
                        match pr.ids.len() {
                            0 => continue,
                            1 => chain_ids.push(pr.ids[0]),
                            w if w == max_w => chain_ids.push(pr.ids[k]),
                            _ => {} // impossible (guaranteed by symmetric_bracket)
                        }
                    }

                    if chain_ids.len() < 2 {
                        continue;
                    }

                    let group = net_groups.entry(sub_net_name).or_default();
                    for pair in chain_ids.windows(2) {
                        group.push(ConnPair {
                            left: pair[0],
                            right: pair[1],
                        });
                    }
                }
            } else {
                // Old behavior: pure scalar chain / heterogeneous mix → flatten + windows(2)
                let all_ids: Vec<i64> = per_point
                    .iter()
                    .flat_map(|pr| pr.ids.iter().copied())
                    .collect();
                let group = net_groups.entry(net_name).or_default();
                for pair in all_ids.windows(2) {
                    group.push(ConnPair {
                        left: pair[0],
                        right: pair[1],
                    });
                }
            }
        }

        // ── ★ FIX-B: Cross-net merge of groups sharing endpoints ─────────────────────────────────
        //
        // Symptom: In hbl mcu513 module, the same physical pin (e.g. `cap5.1`, `CAP_1.1`) appears
        // in pairs of multiple nets simultaneously, rendering as:
        //   GND       (7 pts) : ... cap5.1 ...
        //   VCC_1V2   (2 pts) : VCC_1V2 ~ cap5.1
        //   VDD_3V3   (2 pts) : VDD_3V3 ~ cap5.1
        // One physical node spanning 3 nets = electrically 3 main rails shorted, no router can produce
        // a reasonable result. This is not a bug in the mc_net.rs::NetTable path — that path
        // uses union-find, same id appearing multiple times inevitably merges to one root; but visit.rs's
        // path **groups by net_name itself**, different names won't merge even if sharing endpoints.
        //
        // Fix strategy (lightweight union-find on net_groups):
        //   1. Assign each group a group_id (= ordinal position in net_groups)
        //   2. Scan all (group_id, endpoint_id) pairs, build endpoint_id → belonging
        //      group_id list
        //   3. For any endpoint "referenced by ≥2 groups", union those groups together
        //   4. Merge same-root groups (concat pairs), choose name using "most informative" heuristic
        //      (power-rail first > non-anonymous first > lexicographic)
        //
        // Side effect: If the user **really** wrote a short (e.g. `cap5 -> [VCC_1V2, VDD_3V3, GND]`),
        // this pass will merge 3 nets into one, making the short appear as just "3 rails
        // side by side in one net" output — more obviously wrong than 3 split nets, aiding diagnosis.
        // Also logs `[FIX-B] cross-net merge` warning so the author knows this is a merge artifact.
        //
        // Compatibility: Does not affect any scenario where groups are endpoint-disjoint (most normal netlists),
        // since no shared endpoint is found, union-find won't merge, behavior is fully equivalent.
        if net_groups.len() >= 2 {
            // Materialize net_groups into Vec to ensure stable group_id
            // BTreeMap has no drain; the into_iter() from `take` is already ordered by name → group_id is also determined
            let mut groups_vec: Vec<(String, Vec<ConnPair>)> =
                std::mem::take(&mut net_groups).into_iter().collect();
            let n = groups_vec.len();

            // union-find on group indices (0..n)
            let mut parent: Vec<usize> = (0..n).collect();
            fn uf_find(parent: &mut [usize], mut x: usize) -> usize {
                while parent[x] != x {
                    parent[x] = parent[parent[x]];
                    x = parent[x];
                }
                x
            }
            fn uf_union(parent: &mut [usize], a: usize, b: usize) {
                let ra = uf_find(parent, a);
                let rb = uf_find(parent, b);
                if ra != rb {
                    parent[ra] = rb;
                }
            }

            // endpoint_id → first group seen
            //
            // ── ★ FIX-B correction (after first run feedback): only merge Pin-type endpoints ──────
            //
            // First version indiscriminately treated any id as "shared endpoint → merge": causing
            // the `lpa` component (id=1197, InstKind::Component) on the main.speaker path to trigger
            // a catastrophic 18-net merge into 'DIO' because multiple nets all reference its different pins
            // (US_SPEAKER_MUTE / dc / dc.GND / DIO_ESD all mixed together).
            //
            // Root cause: multi-pin component (lpa) only registered component id 1197 in InstTable, its
            // individual pins (VDD/GND/VO1/...) have no independent id — resolve_netpoint_v2 on the
            // OwnerFallback path backfills with the component id; 18 different signal nets each reference
            // component 1197's different pins, but ConnPair endpoints are all 1197, my previous Fix B
            // treated them as "shared id" and merged everything into one super-net.
            //
            // Fix: only trigger merge for InstKind::Pin (Pin is a physically unique node, appearing in
            // multiple nets = real short or multiple connections to same node, should merge). Component/Module/
            // Port/Bus/Label skip — they may appear in multiple nets due to "Owner backfill", not
            // representing a physical short.
            //
            // Side effect: top-level `V3V3` `V1V2` Port ids (1001/1002) are InstKind::Port,
            // now also don't trigger merge. This avoids the first version's problem of incorrectly merging
            // bracket-broadcast shorts (created upstream by bracket-mode) into GND — those shorts are
            // upstream bugs, we no longer "help wrongly" here, letting symptoms surface in the net list
            // for the author to fix source or for later smarter bracket-broadcast detection to handle.
            let mut ep_to_first_group: HashMap<i64, usize> = HashMap::new();
            for (gid, (_name, pairs)) in groups_vec.iter().enumerate() {
                let mut ep_in_this_group: std::collections::HashSet<i64> =
                    std::collections::HashSet::new();
                for pair in pairs {
                    ep_in_this_group.insert(pair.left);
                    ep_in_this_group.insert(pair.right);
                }
                for ep in ep_in_this_group {
                    if ep < 0 {
                        continue; // -1 and other synthetic endpoints don't participate
                    }
                    // ★ Key fix: only consider Pin type, skip Component/Module/Port/Bus/Label
                    let is_pin = self
                        .inst_table
                        .get_entry(ep as u32)
                        .map(|e| matches!(e.kind, InstKind::Pin))
                        .unwrap_or(false);
                    if !is_pin {
                        continue;
                    }
                    match ep_to_first_group.get(&ep) {
                        Some(&prior_gid) => {
                            uf_union(&mut parent, prior_gid, gid);
                            crate::velog!(
                                "[FIX-B] cross-net merge: Pin id={} appears in groups '{}' \
                                 and '{}', unioning",
                                ep,
                                groups_vec[prior_gid].0,
                                groups_vec[gid].0
                            );
                        }
                        None => {
                            ep_to_first_group.insert(ep, gid);
                        }
                    }
                }
            }

            // Aggregate by root
            let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
            for i in 0..n {
                let r = uf_find(&mut parent, i);
                by_root.entry(r).or_default().push(i);
            }

            // Name selection heuristic: first check for power-rail name (using an approximation
            // of the same-rule lightweight helper from ITER-5 above — here at the inst layer we can't
            // cross to vector layer import naming, use local small judgment). Priority:
            //   1) power-rail name (contains power/ground)
            //   2) non-`__net_N` anonymous name
            //   3) lexicographically smallest
            fn looks_like_rail(name: &str) -> bool {
                let u = name.to_uppercase();
                let exact = [
                    "VCC", "VDD", "VBUS", "GND", "VSS", "AGND", "DGND", "PGND", "AVDD", "VPP",
                ];
                if exact.contains(&u.as_str()) {
                    return true;
                }
                for pfx in &["VCC", "VDD", "V3V", "V5V", "V1V", "GND", "VSS"] {
                    if u.starts_with(pfx) {
                        return true;
                    }
                }
                false
            }
            fn name_priority(name: &str) -> (u8, bool, String) {
                // Smaller is higher priority (sort key)
                // tier 0: rail name
                // tier 1: non-`__net_N`
                // tier 2: other
                let is_anon = name.starts_with("__net_");
                let tier: u8 = if looks_like_rail(name) {
                    0
                } else if !is_anon {
                    1
                } else {
                    2
                };
                (tier, is_anon, name.to_string())
            }

            // Rebuild net_groups: one merged group per root
            let mut merged: NetGroupMap = NetGroupMap::new();
            for (_root, members) in by_root {
                if members.len() == 1 {
                    // Single-member group — replay as-is
                    let (name, pairs) = std::mem::take(&mut groups_vec[members[0]]);
                    merged.insert(name, pairs);
                    continue;
                }
                // Multi-member group — concat pairs, choose best name
                let mut all_pairs: Vec<ConnPair> = Vec::new();
                let mut names: Vec<String> = Vec::new();
                for idx in &members {
                    let (name, pairs) = std::mem::take(&mut groups_vec[*idx]);
                    names.push(name);
                    all_pairs.extend(pairs);
                }
                names.sort_by_key(|n| name_priority(n));
                let chosen = names[0].clone();
                crate::velog!(
                    "[FIX-B] merged {} groups into '{}': dropped names = {:?}",
                    members.len(),
                    chosen,
                    &names[1..]
                );
                // If name conflicts (chosen already in merged), append pairs rather than overwrite
                merged.entry(chosen).or_default().extend(all_pairs);
            }

            net_groups = merged;
        }

        // Step 2: Generate McVecNet for each group
        let mut result = Vec::with_capacity(net_groups.len());
        for (net_name, pairs) in net_groups {
            let nid = self.next_net_id();
            let mcvec_net = merge_pairs_to_vecnet(nid, net_name, &pairs);
            result.push(mcvec_net);
        }

        result
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Allocate next net ID
    fn next_net_id(&mut self) -> i64 {
        let id = self.net_id_counter;
        self.net_id_counter += 1;
        id
    }
}

// ============================================================================
// Public API
// ============================================================================

/// One-step build `McVecBlock` tree from pass2 result (Tolerant mode)
///
/// **Behavior-compatible**: all errors swallowed, check total warnings via `np_warn_count()`.
/// For structured diagnostics, use [`build_mc_vec_with_report`] or hold `McVecBuilder` directly.
pub fn build_mc_vec(root: &McModuleInst, inst_table: &InstTable) -> McVecBlock {
    let mut builder = McVecBuilder::new(inst_table);
    builder.build(root)
}

/// ★ NEW P02: Return both block and report
///
/// After getting report, caller can:
/// - Run CI assert `report.is_clean()`
/// - Print `report.summary_string()`
/// - Serialize to JSON for frontend "health" display
pub fn build_mc_vec_with_report(
    root: &McModuleInst,
    inst_table: &InstTable,
) -> (McVecBlock, BuilderReport) {
    let mut builder = McVecBuilder::new(inst_table);
    let block = builder.build(root);
    let report = builder.report().clone();
    (block, report)
}

/// ★ NEW P02: Strict mode entry, any data loss immediately Err
pub fn build_mc_vec_strict(
    root: &McModuleInst,
    inst_table: &InstTable,
) -> Result<McVecBlock, BuilderError> {
    let mut builder = McVecBuilder::new(inst_table).with_mode(BuildMode::Strict);
    builder.try_build(root)
}
