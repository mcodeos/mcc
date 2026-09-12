// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-2 · netlist → viz projection layer
//!
//! ## Background (MC_SCHEMATIC_ROADMAP_v6 §0.2)
//! The pass2 netlist is electrically equivalent to golden, but carries three kinds of
//! noise that are **harmless in pass2 yet fatal in viz**:
//!
//! * (a) Scalar stubs coexist with member nets: `mic` layer `MIC.N`(port stub) +
//!   `MIC.N~0`(member net); `main` layer `V3V3.VCC`(member net) + `VCC`/`VDD_3V3`
//!   (scalar label views) —— one electrical net is split into 2~3 nets, glued together
//!   by pseudo endpoints of the module's own ports.
//! * (b) Duplicate endpoints of the same port: the `main.VDD_3V3` net contains both
//!   `mic.VDD_3V3`(Label) and `mic.dc.VDD_3V3`(Port) —— normalized to the one on the
//!   port declaration side (ruling ⑤ "declaration wins").
//! * (c) rail label pseudo endpoints: things like `main.V3V3.VCC`, i.e. **the current
//!   layer module's own Port/Label**, treated as a net point —— it is the net's name,
//!   not an electrical connection point.
//!
//! ## Criteria come entirely from port declarations, zero name matching (anti-pattern §2.3)
//! * Pseudo endpoint: `entry.parent_id == block.bid && kind ∈ {Port, Label}`
//!   (parent is the current layer module's own Port/Label = boundary declaration of this layer).
//! * (a) union glue: **the same pseudo endpoint appearing in multiple nets** → those nets
//!   are one electrical net. There is no GROUND sentinel: under strict DC rail identity,
//!   ground-role pseudo endpoints are not globally merged (`va.GND` and `vb.GND` stay
//!   distinct until a real wiring tie shares an endpoint).
//! * (b) criterion: two endpoints in the same net share `parent_id` (same submodule),
//!   one kind=Label, one kind=Port → drop Label keep Port (declaration side).
//!
//! ## Integration point (sole entry, cannot be bypassed)
//! This module is called by `vector::graph::fromblock::build_mc_vec_graph` —— that is the
//! only mandatory path for all block→graph conversions (mcviz / cmds / tests all go through
//! it). This is the single reverse dependency of vector→viz: projection is a viz-side policy
//! that must take effect uniformly for all callers at the boundary; no caller may bypass it
//! (the negative lesson of v4 §6 "lower layer patches upper layer").
//!
//! ## Auditable (discipline 9)
//! Every merge/dedup/removal is recorded as (layer, net, endpoint, rule a|b|c), aggregated
//! into `baseline/render_projection.md`, plus one vlog summary line per layer.

use std::collections::{BTreeSet, HashMap};

use crate::instant::insttab::{InstKind, InstTable, MemberRole};
use crate::vector::graph::naming;
use crate::vector::graph::netdef::IoDirection;
use crate::vector::model::{BoundaryInfo, McVec, McVecBlock, McVecNet};

/// One projection action record (rule a=merge / b=endpoint dedup / c=pseudo endpoint removal)
#[derive(Debug, Clone)]
pub struct ProjectionRecord {
    pub layer: String,
    pub rule: &'static str,
    pub net: String,
    pub endpoint: String,
    pub note: String,
}

/// Projection log: per-layer (net count before, after) + all action records
#[derive(Debug, Default)]
pub struct ProjectionLog {
    pub records: Vec<ProjectionRecord>,
    pub per_layer: Vec<(String, usize, usize)>,
}

impl ProjectionLog {
    /// Aggregated into `baseline/render_projection.md` (overwritten each projection, deterministic
    /// content).
    pub fn write_md(&self) {
        let mut md = String::new();
        md.push_str("# Render Projection (P7-2)\n\n");
        md.push_str("Audit log of the pass2 → viz projection layer. Rules: a=scalar ∪ member-net merge, b=same-port endpoint dedup (declaration wins), c=rail label pseudo endpoint removal.\n\n");
        md.push_str("| Layer | Nets(before) | Nets(after) |\n|---|---|---|\n");
        for (layer, before, after) in &self.per_layer {
            md.push_str(&format!("| {layer} | {before} | {after} |\n"));
        }
        md.push_str("\n## Action Records\n\n| Rule | Layer | Net | Endpoint | Note |\n|---|---|---|---|---|\n");
        for r in &self.records {
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                r.rule, r.layer, r.net, r.endpoint, r.note
            ));
        }
        let path = std::path::Path::new("baseline/render_projection.md");
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, md);
    }
}

/// Project the entire block tree (recursing into every layer).
pub fn project_block_tree(block: &McVecBlock, table: &InstTable) -> (McVecBlock, ProjectionLog) {
    let mut log = ProjectionLog::default();
    let projected = project_block_inner(block, table, &mut log);
    log.write_md();
    (projected, log)
}

fn project_block_inner(
    block: &McVecBlock,
    table: &InstTable,
    log: &mut ProjectionLog,
) -> McVecBlock {
    let mut out = McVecBlock::new(block.bid, block.name.clone());
    out.insts = block.insts.clone();
    out.nets = project_nets(block, table, &block.name, log);
    out.port_trunks = block.port_trunks.clone();
    out.blocks = block
        .blocks
        .iter()
        .map(|b| project_block_inner(b, table, log))
        .collect();
    out
}

// Single-layer projection

/// Pseudo endpoint test: parent is the current layer module's own Port/Label
/// (boundary declaration of this layer, not a connection point).
/// Returns the entry for reuse (kind / member_info).
fn pseudo_entry(
    id: i64,
    bid: i64,
    table: &InstTable,
) -> Option<&crate::instant::insttab::InstEntry> {
    pseudo_entry_with_ancestor(id, bid, table).map(|(e, _)| e)
}

/// ★ P7-8: walk ancestors up to bid (or MAX_HOPS), returning the pseudo endpoint
/// and the nearest port-group ancestor (for BoundaryInfo).
/// - One hop (DAC_OUT parent == bid): returns (DAC_OUT_entry, DAC_OUT_entry)
/// - Two hops (SCL parent == I2C0 port, I2C0 parent == bid): returns (SCL_entry, I2C0_entry)
const MAX_HOPS: u32 = 8;

fn pseudo_entry_with_ancestor(
    id: i64,
    bid: i64,
    table: &InstTable,
) -> Option<(
    &crate::instant::insttab::InstEntry,
    &crate::instant::insttab::InstEntry,
)> {
    if id < 0 {
        return None;
    }
    let e = table.get_entry(id as u32)?;
    if !matches!(e.kind, InstKind::Port | InstKind::Label) {
        return None;
    }
    // Walk ancestor chain until we reach bid or exceed MAX_HOPS.
    // Only walk through Port/Label entries; stop at Module/Component/etc.
    // This prevents member ports of submodules (e.g. main.mcu513.VCC_1V2 whose
    // parent is a Module entry) from being treated as pseudo endpoints of the
    // parent layer (discipline 13: hierarchy checks must reach fixed point).
    let mut current = e;
    for _ in 0..MAX_HOPS {
        if current.parent_id == Some(bid as u32) {
            // ★ Module-port drawing: the parent chain cannot name the group.
            // The instant table is **flat** for module ports — a group
            // (`POWER_LDO.vin`) and each of its members (`POWER_LDO.vin.V5V`) are
            // siblings whose parent is the module itself, so the walk above returns
            // straight from the member. Grouping lives in the dotted path alone;
            // recover it structurally, by longest strict dotted-path prefix, the
            // same rule `fromblock::boundary_ports_of` applies to a sub-module box.
            // Without this the boundary is named after the member (`V5V`) — a net
            // — instead of the port it crosses (`vin`).
            return Some((e, port_group_of(e, bid, table).unwrap_or(e)));
        }
        match current.parent_id {
            Some(pid) => {
                if let Some(parent) = table.get_entry(pid) {
                    if !matches!(parent.kind, InstKind::Port | InstKind::Label) {
                        return None; // stop at Module / Component boundary
                    }
                    current = parent;
                } else {
                    return None;
                }
            }
            None => return None,
        }
    }
    None
}

/// The port group a module-port entry belongs to, recovered from the dotted path.
///
/// Returns the entry whose path is the **longest strict dotted prefix** of `e`'s
/// among this module's ports — `…vin` for `…vin.V5V`, so a member resolves to its
/// group and a group (or a plain declaration with no members) resolves to itself
/// via the caller's `unwrap_or(e)`. Pure structure: ids and paths only, never a
/// name lookup.
fn port_group_of<'a>(
    e: &'a crate::instant::insttab::InstEntry,
    bid: i64,
    table: &'a InstTable,
) -> Option<&'a crate::instant::insttab::InstEntry> {
    table
        .get_ports_of(bid as u32)
        .into_iter()
        .filter(|g| {
            g.id != e.id
                && e.path.len() > g.path.len() + 1
                && e.path.starts_with(&g.path)
                && e.path.as_bytes()[g.path.len()] == b'.'
        })
        .max_by_key(|g| g.path.len())
}

fn project_nets(
    block: &McVecBlock,
    table: &InstTable,
    layer: &str,
    log: &mut ProjectionLog,
) -> Vec<McVecNet> {
    let bid = block.bid;
    let nets = &block.nets;
    log.per_layer.push((layer.to_string(), nets.len(), 0)); // after value backfilled at the end

    // Rule (a): union grouping
    // key1: pseudo endpoint id —— multiple nets sharing the same pseudo endpoint ⇒ one electrical
    // net
    // NOTE: no GROUND sentinel here (strict DC rail identity). Ground-role pseudo
    // endpoints are NOT globally merged: `va.GND` and `vb.GND` stay distinct nets
    // until they share a real wiring tie. Merging happens only through key1
    // (shared pseudo endpoint) or the endpoint union in mc_net / visit.
    let mut dsu = Dsu::new(nets.len());
    let mut first_by_pseudo: HashMap<i64, usize> = HashMap::new();

    for (ni, net) in nets.iter().enumerate() {
        for pid in net.all_point_ids() {
            if let Some(_e) = pseudo_entry(pid, bid, table) {
                // key1
                match first_by_pseudo.get(&pid) {
                    Some(&other) => dsu.union(other, ni),
                    None => {
                        first_by_pseudo.insert(pid, ni);
                    }
                }
            }
        }
    }

    // Rule (a) ground-net merge extensions
    // Under strict DC rail identity, ground-role nets stay separate unless a real
    // wiring tie shares an endpoint. Three tie sources are only visible here:
    //   (1) same-name ground label: every `GND` label in a module scope belongs
    //       to one merged ground net → same bare base "GND" (ruling ⑥: same-name
    //       GND labels merge).
    //   (2) shared real endpoint: `V1V2.GND` and `V3V3.GND` both contain
    //       `mcu513.GND` / `moddcdc.GND` — a real point in two nets is a real tie.
    //   (3) sub-module internal tie: `V5V.GND` and `V3V3.GND` are tied only because
    //       `modldo.vin.GND` and `modldo.vout.GND` sit in one sub-block net.
    // Rail member grounds (`va.GND` / `vb.GND`) never merge by name (strict DC rail
    // identity); only (2) / (3) may merge them, through a real wiring tie.
    //
    // ★ declared-return ground projection (classification-retirement-design §5):
    // only nets that resolve to a DECLARED return/ref copper participate in
    // ground merging —
    // resolvable Ground-side identity, same resolver as the per-output-net
    // `detect_net_attr`, so the merge gate and the carried attr agree by
    // construction. Undeclared power-named nets (LDO `vin.GND`, legacy bare GND)
    // stay Signal and are never merged/classified as ground (ruling ①/②: no
    // declaration, no name guessing).
    let declared_ret: Vec<bool> = nets
        .iter()
        .map(|net| {
            let ids: Vec<i64> = net.all_point_ids();
            matches!(
                detect_net_attr(&ids, block, table).map(|a| a.role),
                Some(AttrRole::Ret | AttrRole::Reference)
            )
        })
        .collect();
    let is_ground_net = |ni: usize| declared_ret[ni];

    // (1) same-name union: bare ground-role nets (declared Ret/Reference)
    // with the SAME exact name (no '.') merge. The merge key is the FULL net
    // name — distinct declared copper nets (`V3V3.GND`, `V5V.GND`) share a
    // bare base only if they are genuinely one copper, and rail-member
    // grounds (`vin.GND`) are excluded below (strict DC rail identity). Only
    // truly identical bare names (e.g. duplicate `GND` nets over one declared
    // copper) union.
    {
        let mut first_by_base: HashMap<&str, usize> = HashMap::new();
        for (ni, net) in nets.iter().enumerate() {
            if !is_ground_net(ni) {
                continue;
            }
            let base = net.name.as_str();
            if base.contains('.') {
                continue; // rail member ground (vin.GND) — strict identity, no name merge
            }
            match first_by_base.get(base) {
                Some(&other) => dsu.union(other, ni),
                None => {
                    first_by_base.insert(base, ni);
                }
            }
        }
    }

    // (2) shared real endpoint union (ground-role nets only).
    {
        let mut first_by_point: HashMap<i64, usize> = HashMap::new();
        for (ni, net) in nets.iter().enumerate() {
            if !is_ground_net(ni) {
                continue;
            }
            for pid in net.all_point_ids() {
                if pid < 0 || pseudo_entry(pid, bid, table).is_some() {
                    continue;
                }
                match first_by_point.get(&pid) {
                    Some(&other) => dsu.union(other, ni),
                    None => {
                        first_by_point.insert(pid, ni);
                    }
                }
            }
        }
    }

    // (3) sub-module internal ground tie propagation: if a raw sub-block net carries
    //     >= 2 distinct ground-role Port points, those boundary ports are electrically
    //     tied inside the sub-module (e.g. modldo's `vin.GND ~ ldo.2 ~ vout.GND`).
    //     Union every parent ground-role net containing any of the tied points.
    {
        let mut parent_nets_of: HashMap<i64, Vec<usize>> = HashMap::new();
        for (ni, net) in nets.iter().enumerate() {
            if !is_ground_net(ni) {
                continue;
            }
            for pid in net.all_point_ids() {
                if pid < 0 {
                    continue;
                }
                parent_nets_of.entry(pid).or_default().push(ni);
            }
        }
        for sb in &block.blocks {
            for sn in &sb.nets {
                let mut ground_ports: Vec<i64> = Vec::new();
                for pid in sn.all_point_ids() {
                    if pid < 0 {
                        continue;
                    }
                    let Some(e) = table.get_entry(pid as u32) else {
                        continue;
                    };
                    if e.kind != InstKind::Port {
                        continue;
                    }
                    // Declared ground role only (C-captured ::DC connection-point
                    // member / declared copper); never a name guess on the path leaf
                    // (classification-retirement ruling ②).
                    let is_ground_role = e
                        .member_info
                        .as_ref()
                        .map_or(false, |m| matches!(m.role, MemberRole::Ground));
                    if is_ground_role {
                        ground_ports.push(pid);
                    }
                }
                ground_ports.sort_unstable();
                ground_ports.dedup();
                if ground_ports.len() < 2 {
                    continue;
                }
                let mut targets: Vec<usize> = Vec::new();
                for pid in &ground_ports {
                    if let Some(idxs) = parent_nets_of.get(pid) {
                        targets.extend(idxs.iter().copied());
                    }
                }
                for i in 1..targets.len() {
                    dsu.union(targets[0], targets[i]);
                }
            }
        }
    }

    // Group by root (preserving first-seen order)
    let mut order: Vec<usize> = Vec::new();
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for ni in 0..nets.len() {
        let r = dsu.find(ni);
        let slot = members.entry(r).or_default();
        if slot.is_empty() {
            order.push(r);
        }
        slot.push(ni);
    }

    let mut out: Vec<McVecNet> = Vec::with_capacity(order.len());
    for root in order {
        let idxs = &members[&root];

        // Endpoint collection: dedup in nid order
        let mut sorted: Vec<usize> = idxs.clone();
        sorted.sort_by_key(|&i| nets[i].nid);
        let mut all_ids: Vec<i64> = Vec::new();
        for &i in &sorted {
            for pid in nets[i].all_point_ids() {
                if !all_ids.contains(&pid) {
                    all_ids.push(pid);
                }
            }
        }

        // ── Naming (read from port declarations, not member net names — builder net
        //   grouping is unstable across runs) ──
        //   Single-net group without merging: keep the original name (zero risk).
        //   Merged group:
        //     · contains a Ground-role pseudo endpoint → take the leaf name of a Label
        //       pseudo endpoint in the group (GND group → "GND", ruling ⑥)
        //     · contains a Power-role pseudo endpoint → take the last two segments of
        //       its path (→ "V3V3.VCC")
        //     · otherwise → the member net name with the most real endpoints
        //       (MIC.N + MIC.N~0 → "MIC.N~0")
        let single = idxs.len() == 1;
        let name_src = if single {
            nets[sorted[0]].name.clone()
        } else {
            group_display_name(&sorted, nets, bid, table)
        };

        // Rule (a) audit: many nets into one
        if !single {
            let names: Vec<&str> = sorted.iter().map(|&i| nets[i].name.as_str()).collect();
            log.records.push(ProjectionRecord {
                layer: layer.to_string(),
                rule: "a",
                net: name_src.clone(),
                endpoint: "-".to_string(),
                note: format!("union {} nets: {}", names.len(), names.join(" + ")),
            });
        }

        // ── Rule (b): same-parent (Label, Port) pair → drop Label keep Port ──
        let mut dropped_b: Vec<i64> = Vec::new();
        for &pid in &all_ids {
            if pid < 0 || pseudo_entry(pid, bid, table).is_some() {
                continue; // pseudo endpoints handled by rule (c), not part of (b)
            }
            if let Some(e) = table.get_entry(pid as u32) {
                if e.kind != InstKind::Label {
                    continue;
                }
                if let Some(parent) = e.parent_id {
                    let has_port_sibling = all_ids.iter().any(|&other| {
                        other >= 0
                            && other != pid
                            && table.get_entry(other as u32).map_or(false, |oe| {
                                oe.parent_id == Some(parent) && oe.kind == InstKind::Port
                            })
                    });
                    if has_port_sibling {
                        dropped_b.push(pid);
                    }
                }
            }
        }
        for pid in &dropped_b {
            if let Some(e) = table.get_entry(*pid as u32) {
                log.records.push(ProjectionRecord {
                    layer: layer.to_string(),
                    rule: "b",
                    net: nets[sorted[0]].name.clone(),
                    endpoint: e.path.clone(),
                    note:
                        "Label endpoint of the same port, normalized to the Port declaration side"
                            .to_string(),
                });
            }
        }

        // ── ★ P7-8: Rule (c) split —— rail pseudo endpoints removed, Signal become Boundary ──
        // ★ §5⑥ (classification-retirement): whether a group is a rail group is the
        // net's DECLARED supply identity — `detect_net_attr` over the whole group
        // (conduit reference / rail ret / rail hot in this layer's power scope, or a
        // DC-pair member role on any endpoint). A pseudo endpoint (this layer's own
        // Port/Label boundary declaration) is the net's NAME — main.GND, main.V5V,
        // a DC-face member port — not a connection point, so it is removed from real
        // whenever the group is a rail net. Never a power-name guess (ruling ②): a
        // member_info-None scalar port whose leaf merely LOOKS like VCC/GND (LDO
        // `vin.GND`, §5⑥ `in VDD_3V3`) leaves the group Signal, so its pseudo
        // endpoints stay in real as Boundary / PortTerminal markers.
        let attr = detect_net_attr(&all_ids, block, table);
        let group_is_rail = attr.as_ref().map_or(false, |a| a.role != AttrRole::Signal);
        let mut dropped_c: Vec<&crate::instant::insttab::InstEntry> = Vec::new();
        let mut boundary: Option<BoundaryInfo> = None;
        for &pid in &all_ids {
            if let Some((e, ancestor)) = pseudo_entry_with_ancestor(pid, bid, table) {
                // §5⑥: a pseudo endpoint is a name, not a connection point, so a
                // rail group's are dropped from `real` — but the *port* it crosses
                // is still a port, and a boundary drawn around this module must
                // show it like any other. Record the identity (below) without
                // letting the endpoint back into `real`.
                if group_is_rail {
                    dropped_c.push(e);
                }
                // ★ Module-port drawing: only a **port** declaration makes this net a
                // boundary crossing. The pseudo-endpoint test above is deliberately
                // loose — it also catches the layer's own bare net names and its
                // internal rail / conduit declarations (`MIC.VMIC`, `MIC.GNDA`,
                // `POWER_USB.ESDGND`, `main.GND`), which are names, not ports, and
                // must not appear on a boundary drawn around this module. A declared
                // port (and every member of one) carries an IO direction; a bare name
                // carries none. Structural — the direction, never the spelling.
                if boundary.is_none() && e.io_type != crate::semantic::common::IOType::None {
                    let io = match e.io_type {
                        crate::semantic::common::IOType::In => IoDirection::Input,
                        crate::semantic::common::IOType::Out => IoDirection::Output,
                        crate::semantic::common::IOType::InOut => IoDirection::Bidir,
                        crate::semantic::common::IOType::Power => IoDirection::Power,
                        crate::semantic::common::IOType::Return => IoDirection::Ground,
                        _ => IoDirection::Passive,
                    };
                    let port_name = last_segment(&ancestor.path);
                    boundary = Some(BoundaryInfo {
                        port_group_id: ancestor.id as i64,
                        port_name,
                        io,
                        is_supply: group_is_rail,
                    });
                }
            }
        }

        // Real endpoints = all - (b dropped) - (c pseudo endpoints)
        // Rail groups: drop ALL pseudo endpoints (Labels and member ports alike —
        // the net's name / DC-face boundary, not a connection point).
        // Signal groups: keep pseudo endpoints (they become PortTerminal connections).
        let real: Vec<i64> = all_ids
            .iter()
            .copied()
            .filter(|&pid| {
                if dropped_b.contains(&pid) {
                    return false;
                }
                match pseudo_entry_with_ancestor(pid, bid, table) {
                    Some((_, _)) => !group_is_rail,
                    None => true,
                }
            })
            .collect();

        // Empty net: drop entirely (audited)
        if real.is_empty() {
            for e in &dropped_c {
                log.records.push(ProjectionRecord {
                    layer: layer.to_string(),
                    rule: "c",
                    net: name_src.clone(),
                    endpoint: e.path.clone(),
                    note: "net empty after pseudo endpoint removal, entire net dropped".to_string(),
                });
            }
            continue;
        }

        // ── Audit of (c) rail pseudo endpoints (removed from real) ─
        for e in &dropped_c {
            log.records.push(ProjectionRecord {
                layer: layer.to_string(),
                rule: "c",
                net: name_src.clone(),
                endpoint: e.path.clone(),
                note: "rail boundary declaration of this layer (Port/Label), not an electrical connection point".to_string(),
            });
        }
        // ── Audit of pseudo endpoints kept as a boundary (both axes) ─
        if let Some(ref bi) = boundary {
            log.records.push(ProjectionRecord {
                layer: layer.to_string(),
                rule: "c",
                net: name_src.clone(),
                endpoint: bi.port_name.clone(),
                note: format!(
                    "{} boundary port group (id={}), kept as PortTerminal marker",
                    if bi.is_supply { "supply" } else { "non-rail" },
                    bi.port_group_id
                ),
            });
        }

        // ── ★ P7-3: power net spec (class + driver), all from port declarations ──
        //   Ground-role pseudo endpoint exists → Ground (R-1, globally the same ground, no driver);
        //   Power-role pseudo endpoint exists → Power, driver resolved in two steps:
        //     (a) a real endpoint with io==Out and member==Power (ldo.VCC / dcdc.VCC_1V2)
        // (b) otherwise, for each Power member endpoint (io != In) do a sub-layer generation-side
        // check —— if the net containing that endpoint in the raw subblock only passes through
        //         two-pin passives (e.g. usbsocket's vin.POWER_SYS via R0603), it is the source
        //         (speaker feeding the 8-pin lpa directly ⇒ consumer side)
        let rail = detect_rail_spec(&all_ids, &real, block, table, layer);

        // ── Output: a single flat group (rail/signal both consumed as flat endpoint sets in Phase
        // 3) ──
        let mut net = McVecNet::new(nets[sorted[0]].nid, name_src, vec![McVec::new(real)]);
        net.rail = rail;
        net.attr = attr;
        net.boundary = boundary;
        // ★ P9-A2.5: propagate source_span and trunk from the first net in the group
        net.source_span = nets[sorted[0]].source_span.clone();
        net.trunk = nets[sorted[0]].trunk.clone();
        // ★ §8.9.4: propagate the coarse trunk back-reference (trunk ids stay
        // valid because the trunk table is carried over verbatim)
        net.trunk_ref = nets[sorted[0]].trunk_ref;
        // ★ §8.9.2: propagate the topology shape so fine-net output can keep
        // op / anchor / order semantics even after projection merging.
        net.shape = nets[sorted[0]].shape.clone();
        out.push(net);
    }

    // Backfill after
    if let Some(entry) = log.per_layer.last_mut() {
        entry.2 = out.len();
    }
    let (merges, dedups, pseudos) = (
        log.records
            .iter()
            .filter(|r| r.layer == layer && r.rule == "a")
            .count(),
        log.records
            .iter()
            .filter(|r| r.layer == layer && r.rule == "b")
            .count(),
        log.records
            .iter()
            .filter(|r| r.layer == layer && r.rule == "c")
            .count(),
    );
    crate::vlog!(
        "[project] layer '{layer}': nets {} -> {} (a: merged {merges} groups, b: deduped {dedups}, c: pseudo endpoints {pseudos})",
        nets.len(),
        out.len()
    );
    out
}

/// Display name of a merged group —— **read from port declarations**, not member net names
/// (the builder's net grouping is unstable across runs; deriving display names from member
/// net names would misfire).
///
/// 1. Group contains a Ground-role pseudo endpoint → leaf name of a Label pseudo endpoint
///    in the group (GND group → "GND", ruling ⑥ global ground)
/// 2. Group contains a Power-role pseudo endpoint → last two segments of its path (→ "V3V3.VCC")
/// 3. Otherwise (signal group merge, e.g. MIC.N + MIC.N~0) → the member net name with the
///    most real endpoints (ties broken by smallest nid)
fn group_display_name(sorted: &[usize], nets: &[McVecNet], bid: i64, table: &InstTable) -> String {
    // All pseudo endpoints in the group (iterate member nets in nid order for determinism)
    // Note: Ground/Power roles sit on Port pseudo points (V*.GND / V*.VCC member declarations),
    // while Label pseudo points (main.GND) have member_info == None —— role probing and
    // name picking are two separate steps.
    let mut has_ground = false;
    // All distinct rail ground member paths in the group (main.va.GND → "va.GND").
    // V1 (net-identity design): a set of size > 1 means several rails' grounds
    // merged into one global ground plane → named by the bare leaf, not by an
    // arbitrary first rail path.
    let mut ground_ports: BTreeSet<String> = BTreeSet::new();
    let mut label_leaf: Option<String> = None;
    let mut power_port: Option<String> = None;
    'outer: for &i in sorted {
        for pid in nets[i].all_point_ids() {
            let Some(e) = pseudo_entry(pid, bid, table) else {
                continue;
            };
            match e.member_info.as_ref().map(|m| m.role.clone()) {
                Some(MemberRole::Ground) => {
                    has_ground = true;
                    // rail member: last two path segments (main.va.GND → "va.GND")
                    ground_ports.insert(last_two_segments(&e.path));
                }
                Some(MemberRole::Power) => {
                    // rail member: last two path segments (main.V3V3.VCC → "V3V3.VCC")
                    if power_port.is_none() {
                        power_port = Some(last_two_segments(&e.path));
                    }
                }
                _ => {}
            }
            if e.kind == InstKind::Label && label_leaf.is_none() {
                label_leaf = Some(last_segment(&e.path)); // main.GND → "GND"
            }
            if has_ground && label_leaf.is_some() && power_port.is_some() {
                break 'outer;
            }
        }
    }
    if has_ground {
        // A bare ground Label (main.GND → "GND") is the global ground declaration
        // (ruling ⑥: same-name GND labels merge) and wins over any rail member path.
        // A rail-only ground group (no bare GND label) keeps its full member path.
        if let Some(n) = label_leaf {
            if naming::is_ground(&n) {
                return n;
            }
        }
        // V1 (net-identity design, rule ⑥): several distinct rail grounds merged
        // into one group is a global ground plane → bare leaf "GND". A single
        // rail ground keeps its full member path (va.GND) — strict rail identity.
        if ground_ports.len() > 1 {
            return "GND".to_string();
        }
        if let Some(n) = ground_ports.iter().next() {
            return n.clone(); // Rail ground: the full member path is the net name (va.GND)
        }
    }
    if let Some(n) = power_port {
        return n;
    }
    let real_count = |i: usize| {
        nets[i]
            .all_point_ids()
            .into_iter()
            .filter(|pid| pseudo_entry(*pid, bid, table).is_none())
            .count()
    };
    sorted
        .iter()
        .copied()
        .max_by_key(|&i| (real_count(i), std::cmp::Reverse(nets[i].nid)))
        .map(|i| nets[i].name.clone())
        .unwrap_or_default()
}

fn last_segment(path: &str) -> String {
    path.rsplit('.').next().unwrap_or(path).to_string()
}

fn last_two_segments(path: &str) -> String {
    let segs: Vec<&str> = path.split('.').collect();
    if segs.len() >= 2 {
        format!("{}.{}", segs[segs.len() - 2], segs[segs.len() - 1])
    } else {
        path.to_string()
    }
}

// ★ P7-3: power net spec resolution (criteria all from port declarations, zero name matching)

use crate::semantic::common::IOType;
use crate::semantic::component::mc_pins::PwrDir;
use crate::vector::model::{AttrRole, NetAttrMirror, RailClass, RailSpec};

/// Resolve the power net spec from a group's pseudo endpoint roles + real endpoint
/// declarations; returns `None` for ordinary signal groups.
fn detect_rail_spec(
    all_ids: &[i64],
    real: &[i64],
    block: &McVecBlock,
    table: &InstTable,
    layer: &str,
) -> Option<RailSpec> {
    // ── Pass 1 (unchanged): the layer's own declared copper — a Ground/Power role
    // on a pseudo Port/Label boundary endpoint (parent == bid). ──
    let mut has_ground = false;
    let mut has_power = false;
    let mut volt: Option<String> = None;
    for &pid in all_ids {
        if let Some(e) = pseudo_entry(pid, block.bid, table) {
            if let Some(mi) = &e.member_info {
                match mi.role {
                    MemberRole::Ground => has_ground = true,
                    MemberRole::Power => {
                        has_power = true;
                        if volt.is_none() {
                            volt = mi.voltage.as_ref().map(|v| v.to_string());
                        }
                    }
                    MemberRole::Signal => {}
                }
            }
        }
    }

    // ── Pass 2 (parent-layer supply): a *parent*-layer power net whose DC identity
    // arrives solely on a **real** endpoint — a child module's power port (member
    // role set at flatten from the `::DC` decode) — is invisible to the pseudo scan
    // above (`pseudo_entry` stops at the Module boundary). hbl `main.V5V` is exactly
    // this: USB.vin psrc / LDO.vin psnk, no main-level pseudo role.
    //
    // Classify such a net ONLY when a structural driver actually resolves, so a
    // driver-less supply keeps today's shared-net drawing instead of being deleted
    // by the top-level R-1 branch (§ R-2s keeps a driver-ful one shared; R-1 would
    // drop a driver-less one). Ground is never inferred here: return-copper identity
    // must stay on an explicit pseudo boundary/declaration, never on a stray real
    // member role. ──
    if !has_ground && !has_power {
        let real_power = all_ids.iter().any(|&pid| {
            table
                .get_entry(pid as u32)
                .filter(|e| e.alias_of.is_none())
                .and_then(|e| e.member_info.as_ref())
                .map_or(false, |mi| mi.role == MemberRole::Power)
        });
        if !real_power {
            return None; // ordinary signal net
        }
        let Some(dp) = resolve_power_driver(real, block, table) else {
            return None; // supply with no resolvable source: leave the shared net alone
        };
        let who = table
            .get_entry(dp as u32)
            .map(|e| e.path.clone())
            .unwrap_or_else(|| format!("{dp}"));
        crate::vlog!(
            "[project] layer '{layer}': parent-layer power rail driver={who} (real-endpoint arm)"
        );
        let volt = all_ids.iter().find_map(|&pid| {
            table
                .get_entry(pid as u32)
                .filter(|e| e.alias_of.is_none())
                .and_then(|e| e.member_info.as_ref())
                .filter(|mi| mi.role == MemberRole::Power)
                .and_then(|mi| mi.voltage.as_ref())
                .map(|v| v.to_string())
        });
        return Some(RailSpec {
            class: RailClass::Power,
            driver_pin: Some(dp),
            volt,
        });
    }
    let class = if has_ground {
        RailClass::Ground
    } else {
        RailClass::Power
    };
    let driver_pin = if class == RailClass::Ground {
        None // R-1: ground is the return side; an out declaration (e.g. ldo's out GND) is not a driver
    } else {
        resolve_power_driver(real, block, table)
    };
    if let Some(dp) = driver_pin {
        let who = table
            .get_entry(dp as u32)
            .map(|e| e.path.clone())
            .unwrap_or_else(|| format!("{dp}"));
        crate::vlog!("[project] layer '{layer}': rail class=Power driver={who}");
    }
    Some(RailSpec {
        class,
        driver_pin,
        volt,
    })
}

/// Generation side of a Power rail (two-step resolution, see detect_rail_spec docs).
fn resolve_power_driver(real: &[i64], block: &McVecBlock, table: &InstTable) -> Option<i64> {
    // (a) io == Out and member == Power
    let mut by_out: Vec<i64> = real
        .iter()
        .copied()
        .filter(|&pid| endpoint_is_out_power(pid, table))
        .collect();
    by_out.dedup();
    if by_out.len() == 1 {
        return Some(by_out[0]);
    }
    if by_out.len() > 1 {
        // Multiple drivers (DRC anomaly): deterministically take the smallest id and log it
        crate::vlog!(
            "[project] rail has {} Out+Power endpoints (multiple drivers), taking the smallest id",
            by_out.len()
        );
        return Some(*by_out.iter().min().unwrap());
    }

    // (a2) A child-module psrc power port — an endpoint whose owner block's declared
    // power face exports this hot member as PwrDir::Src. Deterministic and stronger
    // than (b)'s sub-layer passives heuristic: a module's exported supply face *is*
    // the generation side regardless of what the child does internally (hbl V5V's
    // driver is USB.vin because POWER_USB declares `psrc vin{V5V,…}`). Unique
    // source wins; several sources (DRC anomaly) → ambiguous, no driver.
    let mut by_modsrc: Vec<i64> = real
        .iter()
        .copied()
        .filter(|&pid| is_child_module_psrc_port(pid, table))
        .collect();
    by_modsrc.dedup();
    match by_modsrc.len() {
        1 => return Some(by_modsrc[0]),
        0 => {}
        _ => {
            crate::vlog!(
                "[project] rail has {} child-module psrc ports (ambiguous), treating as no driver",
                by_modsrc.len()
            );
            return None;
        }
    }

    // (a3) A leaf component's `psrc`/`psbi` power pin — the flat entry carries
    // the declared energy direction (`InstEntry::pwr_dir`, recorded at flatten
    // from the component def's own `pins.pwr` rows). This is the leaf
    // counterpart of (a2): on a board whose supply faces are component pins
    // (pwrint's `usb.vin`/`ldo.VIN`), dropping the io type also drops the
    // direction, so only this arm can name the generation side.
    // as a source root, exactly as it does for the canonical
    // `nets::source_contract_for` (its `::DC(v)` is the discharge guarantee).
    // Hot member only — a `ret`/ground pin never sources. Unique source wins;
    // several → ambiguous.
    let mut by_compsrc: Vec<i64> = real
        .iter()
        .copied()
        .filter(|&pid| endpoint_is_component_pwr_source(pid, table))
        .collect();
    by_compsrc.dedup();
    match by_compsrc.len() {
        1 => return Some(by_compsrc[0]),
        0 => {}
        _ => {
            crate::vlog!(
                "[project] rail has {} component power source pins (ambiguous), treating as no driver",
                by_compsrc.len()
            );
            return None;
        }
    }

    // (a4) A passive series element fed from an already-driven net — the second
    // half of the leaf-supply reading. VDD_3V3's driver (arm a3) reaches VDDA
    // only *through* the bead `FB_a`, and VCC_1V2 only through `_L1`; those
    // rails carry no source pin of their own, so without this arm their supply
    // face stays invisible and the whole branch is drawn as plain mesh. The
    // element must be a two-terminal component with no declared power face
    // (a converter is not a hop), and its *far* terminal must sit on a net that
    // already exposes a source. One hop only — never recursive, so the
    // resolution stays deterministic and terminating. The reported driver is
    // this net's own terminal of the element, keeping the drawn chain
    // source -> element -> load.
    let mut by_passive: Vec<i64> = real
        .iter()
        .copied()
        .filter(|&pid| endpoint_is_fed_passive_hop(pid, table))
        .collect();
    by_passive.dedup();
    match by_passive.len() {
        1 => return Some(by_passive[0]),
        0 => {}
        _ => {
            crate::vlog!(
                "[project] rail has {} fed passive hops (ambiguous), treating as no driver",
                by_passive.len()
            );
            return None;
        }
    }

    // (b) For each Power member endpoint (io != In) do a sub-layer generation-side check;
    //     only a unique source counts
    let mut sources: Vec<i64> = Vec::new();
    for &pid in real {
        if pid < 0 {
            continue;
        }
        let Some(e) = table.get_entry(pid as u32) else {
            continue;
        };
        let member_power = e
            .member_info
            .as_ref()
            .map_or(false, |m| m.role == MemberRole::Power);
        if !member_power || matches!(e.io_type, IOType::In) {
            continue;
        }
        if is_rail_source_in_subblock(pid, block, table) {
            sources.push(pid);
        }
    }
    sources.dedup();
    match sources.len() {
        1 => Some(sources[0]),
        0 => None,
        _ => {
            crate::vlog!(
                "[project] rail has {} candidate sources (ambiguous), treating as no driver",
                sources.len()
            );
            None
        }
    }
}

fn endpoint_is_out_power(pid: i64, table: &InstTable) -> bool {
    pid >= 0
        && table.get_entry(pid as u32).map_or(false, |e| {
            matches!(e.io_type, IOType::Out)
                && e.member_info
                    .as_ref()
                    .map_or(false, |m| m.role == MemberRole::Power)
        })
}

/// ★ resolve_power_driver arm (a3): a real endpoint is a leaf-component source
/// pin when it is a `Pin` whose owner is a `Component`, carries the hot member
/// role, and was flattened with `PwrDir::Src` or `PwrDir::Bi` (typed carry from
/// the component def's `pins.pwr` row). Pure declaration — no topology, no name
/// table. Module ports never qualify (kind is Port; they ride arm (a2)).
fn endpoint_is_component_pwr_source(pid: i64, table: &InstTable) -> bool {
    let Some(e) = (pid >= 0).then(|| table.get_entry(pid as u32)).flatten() else {
        return false;
    };
    if e.kind != InstKind::Pin || !matches!(e.pwr_dir, Some(PwrDir::Src) | Some(PwrDir::Bi)) {
        return false;
    }
    e.member_info
        .as_ref()
        .map_or(false, |m| m.role == MemberRole::Power)
        && e.parent_id
            .and_then(|p| table.get_entry(p))
            .map_or(false, |p| p.kind == InstKind::Component)
}

/// ★ resolve_power_driver arm (a4): this endpoint is a terminal of a **fed
/// passive** — a two-terminal component carrying no declared power face of its
/// own (no pin with a power io type or a member role), whose other terminal sits
/// on a net that already exposes a source. Structural end to end: two-terminal
/// count and per-pin silence come from the flat table, and the far side is
/// judged with the same three source predicates the earlier arms use. Exactly
/// one hop, never recursive.
fn endpoint_is_fed_passive_hop(pid: i64, table: &InstTable) -> bool {
    let Some(e) = (pid >= 0).then(|| table.get_entry(pid as u32)).flatten() else {
        return false;
    };
    if e.kind != InstKind::Pin {
        return false;
    }
    let Some(comp_id) = e.parent_id else {
        return false;
    };
    if table
        .get_entry(comp_id)
        .map_or(true, |c| c.kind != InstKind::Component)
    {
        return false;
    }
    let pins = table.get_pins_of(comp_id);
    // A declared power face means the element converts rather than passes
    // through — never a hop.
    if pins.iter().any(|p| {
        matches!(p.io_type, IOType::Power)
            || p.member_info
                .as_ref()
                .map_or(false, |m| !matches!(m.role, MemberRole::Signal))
    }) {
        return false;
    }
    if pins.len() != 2 {
        return false;
    }
    let Some(far) = pins.iter().find(|p| p.id != e.id) else {
        return false;
    };
    table.nets_of(far.id).iter().any(|&nid| {
        table.get_net(nid).map_or(false, |net| {
            net.points.iter().any(|&q| {
                q != far.id
                    && (endpoint_is_out_power(q as i64, table)
                        || is_child_module_psrc_port(q as i64, table)
                        || endpoint_is_component_pwr_source(q as i64, table))
            })
        })
    })
}

/// ★ resolve_power_driver arm (a2): a real endpoint is a child-module source port
/// when its owner instance is a module whose declared power face
/// (`power_decls[parent].pwr_ports`) exports this endpoint's hot-member leaf as
/// `PwrDir::Src`. Pure declaration — no topology, no name table. Component power
/// pins never qualify (kind is Pin, and `pwr_ports` is a module-header structure);
/// leaf-component sources stay with arm (a) (`io == Out && member == Power`).
fn is_child_module_psrc_port(pid: i64, table: &InstTable) -> bool {
    let Some(e) = (pid >= 0).then(|| table.get_entry(pid as u32)).flatten() else {
        return false;
    };
    if e.kind != InstKind::Port {
        return false;
    }
    let Some(parent) = e.parent_id else {
        return false;
    };
    let member_power = e
        .member_info
        .as_ref()
        .map_or(false, |m| m.role == MemberRole::Power);
    if !member_power {
        return false;
    }
    let Some(decls) = table.power_decls().get(&parent) else {
        return false;
    };
    let leaf = last_segment(&e.path);
    decls
        .pwr_ports
        .iter()
        .any(|p| p.dir == PwrDir::Src && p.hot == leaf)
}

/// ★ §4 (classification-retirement-design): resolve the declared supply
/// identity of one projected group from its endpoints' declarations — zero name
/// matching. Two declaration arms:
///
/// (i)  **the projected layer module's own declared copper** — pseudo endpoints
///      (this block's Port/Label boundary declarations) whose leaf is a
///      declared `conduit` name (`Reference`), a declared DC rail `ret` member
///      (`Ret`) or `hot` member (`Hot`) of `power_decls[block.bid]`;
/// (ii) **connection-point DC pairs carried on an endpoint's own flat entry** —
///      `member_info.role` set at flatten from the `::DC` positional decode
///      (module-header pairs) and component DC pin rows: `Ground` → `Ret`,
///      `Power` → `Hot`.
///
/// `None` (no candidate) = legacy net with no declaration → `Signal` (ruling ① —
/// undeclared: never judged, never guessed). Ground-side roles
/// (`Ret`/`Reference`) outrank `Hot` so a merged
/// return copper with a stray supply endpoint stays the return. `copper` is the
/// first candidate's leaf (nid order) — informational only.
fn detect_net_attr(
    all_ids: &[i64],
    block: &McVecBlock,
    table: &InstTable,
) -> Option<NetAttrMirror> {
    let bid = block.bid;
    let decls = table.power_decls().get(&(bid as u32));
    let mut copper: Option<String> = None;
    let mut has_ret = false;
    let mut has_ref = false;
    let mut has_hot = false;
    for &pid in all_ids {
        let Some(e) = table.get_entry(pid as u32) else {
            continue;
        };
        if e.alias_of.is_some() {
            continue; // non-physical spelling — never a declaration endpoint
        }
        let leaf = last_segment(&e.path);
        // (i) this layer module's own declared copper (Port/Label boundary).
        if pseudo_entry(pid, bid, table).is_some() {
            if let Some(d) = decls {
                if d.l1_refs().iter().any(|r| r.name == leaf) {
                    has_ref = true;
                    copper.get_or_insert_with(|| leaf.clone());
                }
                for r in &d.l1_rails() {
                    if r.ret == leaf {
                        has_ret = true;
                        copper.get_or_insert_with(|| leaf.clone());
                    } else if r.hot == leaf {
                        has_hot = true;
                        copper.get_or_insert_with(|| leaf.clone());
                    }
                }
            }
        }
        // (ii) connection-point DC pair on the endpoint's own entry.
        if let Some(mi) = &e.member_info {
            match mi.role {
                MemberRole::Ground => {
                    has_ret = true;
                    copper.get_or_insert_with(|| leaf.clone());
                }
                MemberRole::Power => {
                    has_hot = true;
                    copper.get_or_insert_with(|| leaf.clone());
                }
                MemberRole::Signal => {}
            }
        }
    }
    if !has_ret && !has_ref && !has_hot {
        return None; // no declaration — legacy signal net, never guessed
    }
    let role = if has_ret {
        AttrRole::Ret
    } else if has_ref {
        AttrRole::Reference
    } else {
        AttrRole::Hot
    };
    Some(NetAttrMirror {
        copper,
        role,
        resolvable: true,
    })
}

/// Sub-layer generation-side check: in the raw subblock, the net containing this boundary
/// endpoint touches no active device other than boundary declarations (parent == submodule)
/// and two-pin passives ⇒ this endpoint is the "generation side".
///
/// Specimens: usbsocket.vin.POWER_SYS raw net = [R0603.2, boundary] → passes only passives →
/// source;
/// speaker.USB_VBUS_1.VDD_3V raw net = [lpa.7(8-pin), C8.1, boundary] → touches an IC → consumer
/// side.
fn is_rail_source_in_subblock(pin_id: i64, block: &McVecBlock, table: &InstTable) -> bool {
    let Some(parent_mod) = table.get_entry(pin_id as u32).and_then(|e| e.parent_id) else {
        return false;
    };
    let Some(sub) = block.blocks.iter().find(|b| b.bid == parent_mod as i64) else {
        return false; // no subblock (component pins etc.) —— sub-layer internal judgment goes to level (a)
    };
    for net in &sub.nets {
        if !net.all_point_ids().contains(&pin_id) {
            continue;
        }
        for other in net.all_point_ids() {
            if other == pin_id || other < 0 {
                continue;
            }
            let Some(oe) = table.get_entry(other as u32) else {
                continue;
            };
            let Some(op) = oe.parent_id else { continue };
            if op == parent_mod {
                continue; // the subblock's own boundary declaration, transparent
            }
            let passive = table.get_entry(op).map_or(false, |pe| {
                pe.kind == InstKind::Component && table.get_pins_of(op).len() <= 2
            });
            if !passive {
                return false; // touches an active device ⇒ consumer side
            }
        }
    }
    true
}

// Union-Find (same shape as coalesce.rs, private copy to avoid cross-module coupling)

struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.parent[x] = root;
        }
        self.parent[x]
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}
