// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-2 · 网表 → viz 的投影层
//!
//! ## 背景（MC_SCHEMATIC_ROADMAP_v6 §0.2）
//! pass2 的网表电气上等价 golden，但带三类**在 pass2 无害、在 viz 致命**的噪声：
//!
//! * (a) 标量 stub 与成员网并存：`mic` 层 `MIC.N`(port stub) + `MIC.N~0`(成员网)；
//!   `main` 层 `V3V3.VCC`(成员网) + `VCC`/`VDD_3V3`(标量 label 视图) —— 同一条电气网
//!   被拆成 2~3 张网，靠模块自身端口的伪端点互相粘连。
//! * (b) 同一端口的重复端点：`main.VDD_3V3` 网里同时有 `mic.VDD_3V3`(Label) 与
//!   `mic.dc.VDD_3V3`(Port) —— 归一到端口声明侧那一个（裁决⑤"声明为准"）。
//! * (c) rail label 伪端点：`main.V3V3.VCC` 这类**本层模块自己的 Port/Label**
//!   被当作一个 net point —— 它是网的名字，不是一个电气连接点。
//!
//! ## 判据全部来自端口声明，零名字匹配（反模式 §2.3）
//! * 伪端点：`entry.parent_id == block.bid && kind ∈ {Port, Label}`
//!   （父节点是本层模块自身的 Port/Label = 本层的边界声明）。
//! * (a) 的 union 胶水：**同一伪端点出现在多张网里** → 那些网是同一条电气网；
//!   以及 **`member_info.role == Ground` 的伪端点全局同一地**（golden 裁决⑥）。
//! * (b) 的判据：同一 net 内两个端点的 `parent_id` 相同（同一个子模块），
//!   一个 kind=Label、一个 kind=Port → 丢 Label 保 Port（声明侧）。
//!
//! ## 落点（唯一入口，不可绕过）
//! 本模块由 `vector::graph::fromblock::build_mc_vec_graph` 调用 —— 那是所有
//! block→graph 转换的唯一必经点（mcviz / cmds / 测试全部走它）。这是 vector→viz
//! 的唯一一处反向依赖：投影是 viz 侧策略，必须在边界上对所有调用方统一生效，
//! 不允许任何调用方绕过（v4 §6"下层补上层"的反面教训）。
//!
//! ## 可审计（纪律 9）
//! 每次合并/去重/剔除都记录 (层, 网, 端点, 规则 a|b|c)，汇总写入
//! `baseline/render_projection.md`，并在 vlog 打一行每层摘要。

use std::collections::HashMap;

use crate::instant::insttab::{InstKind, InstTable, MemberRole};
use crate::vector::model::{McVec, McVecBlock, McVecNet};

/// 一条投影动作记录（规则 a=合并 / b=端点去重 / c=伪端点剔除）
#[derive(Debug, Clone)]
pub struct ProjectionRecord {
    pub layer: String,
    pub rule: &'static str,
    pub net: String,
    pub endpoint: String,
    pub note: String,
}

/// 投影日志：逐层 (网数前, 网数后) + 全部动作记录
#[derive(Debug, Default)]
pub struct ProjectionLog {
    pub records: Vec<ProjectionRecord>,
    pub per_layer: Vec<(String, usize, usize)>,
}

impl ProjectionLog {
    /// 汇总写入 `baseline/render_projection.md`（每次投影覆盖写，内容确定）。
    pub fn write_md(&self) {
        let mut md = String::new();
        md.push_str("# Render Projection (P7-2)\n\n");
        md.push_str("pass2 → viz 投影层的审计日志。规则：a=标量∪成员网合并，b=同端口端点去重（声明为准），c=rail label 伪端点剔除。\n\n");
        md.push_str("| 层 | 网数(前) | 网数(后) |\n|---|---|---|\n");
        for (layer, before, after) in &self.per_layer {
            md.push_str(&format!("| {layer} | {before} | {after} |\n"));
        }
        md.push_str("\n## 动作记录\n\n| 规则 | 层 | 网 | 端点 | 说明 |\n|---|---|---|---|---|\n");
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

/// 对整棵 block 树做投影（递归每一层）。
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
    out.blocks = block
        .blocks
        .iter()
        .map(|b| project_block_inner(b, table, log))
        .collect();
    out
}

// ============================================================================
// 单层投影
// ============================================================================

/// 伪端点判定：父节点是本层模块自身的 Port/Label（本层的边界声明，不是连接点）。
/// 返回 entry 便于复用（kind / member_info）。
fn pseudo_entry(id: i64, bid: i64, table: &InstTable) -> Option<&crate::instant::insttab::InstEntry> {
    if id < 0 {
        return None;
    }
    let e = table.get_entry(id as u32)?;
    let is_boundary = e.parent_id == Some(bid as u32)
        && matches!(e.kind, InstKind::Port | InstKind::Label);
    if is_boundary {
        Some(e)
    } else {
        None
    }
}

fn project_nets(
    block: &McVecBlock,
    table: &InstTable,
    layer: &str,
    log: &mut ProjectionLog,
) -> Vec<McVecNet> {
    let bid = block.bid;
    let nets = &block.nets;
    log.per_layer.push((layer.to_string(), nets.len(), 0)); // after 值最后回填

    // ── 规则 (a)：union 分组 ─────────────────────────────────────────────
    // key1: 伪端点 id —— 多张网共享同一个伪端点 ⇒ 同一条电气网
    // key2: GROUND 哨兵 —— member_info.role == Ground 的伪端点全局同一地（裁决⑥）
    let mut dsu = Dsu::new(nets.len());
    let mut first_by_pseudo: HashMap<i64, usize> = HashMap::new();
    const GROUND: i64 = -1; // 哨兵 key（真实 id >= 0，不冲突）
    let mut first_ground: Option<usize> = None;

    for (ni, net) in nets.iter().enumerate() {
        for pid in net.all_point_ids() {
            if let Some(e) = pseudo_entry(pid, bid, table) {
                // key1
                match first_by_pseudo.get(&pid) {
                    Some(&other) => dsu.union(other, ni),
                    None => {
                        first_by_pseudo.insert(pid, ni);
                    }
                }
                // key2
                if e.member_info.as_ref().map_or(false, |m| m.role == MemberRole::Ground) {
                    match first_ground {
                        Some(other) => dsu.union(other, ni),
                        None => first_ground = Some(ni),
                    }
                }
            }
        }
    }

    // ── 按 root 分组（保持首次出现顺序）──────────────────────────────────
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

        // ── 端点收集：按 nid 顺序去重 ────────────────────────────────────
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

        // ── 命名（从端口声明读，不依赖成员网名——builder 网分组跨运行不稳定）──
        //   未发生合并的单网组：保留原名（零风险）。
        //   合并组：
        //     · 组内含 Ground 角色伪端点 → 取组内 Label 伪端点的叶名（GND 组 → "GND"，裁决⑥）
        //     · 组内含 Power 角色伪端点 → 取该伪端点路径末两段（→ "V3V3.VCC"）
        //     · 其余 → 真实端点最多的成员网名（MIC.N + MIC.N~0 → "MIC.N~0"）
        let single = idxs.len() == 1;
        let name_src = if single {
            nets[sorted[0]].name.clone()
        } else {
            group_display_name(&sorted, nets, bid, table)
        };

        // ── 规则 (a) 审计：多网合一 ──────────────────────────────────────
        if !single {
            let names: Vec<&str> = sorted.iter().map(|&i| nets[i].name.as_str()).collect();
            log.records.push(ProjectionRecord {
                layer: layer.to_string(),
                rule: "a",
                net: name_src.clone(),
                endpoint: "-".to_string(),
                note: format!("union {} 张网: {}", names.len(), names.join(" + ")),
            });
        }

        // ── 规则 (b)：同父 (Label, Port) 对 → 丢 Label 保 Port ────────────
        let mut dropped_b: Vec<i64> = Vec::new();
        for &pid in &all_ids {
            if pid < 0 || pseudo_entry(pid, bid, table).is_some() {
                continue; // 伪端点由规则 (c) 处理，不参与 (b)
            }
            if let Some(e) = table.get_entry(pid as u32) {
                if e.kind != InstKind::Label {
                    continue;
                }
                if let Some(parent) = e.parent_id {
                    let has_port_sibling = all_ids.iter().any(|&other| {
                        other >= 0
                            && other != pid
                            && table
                                .get_entry(other as u32)
                                .map_or(false, |oe| {
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
                    note: "同一端口的 Label 端点，归一到 Port 声明侧".to_string(),
                });
            }
        }

        // ── 规则 (c)：伪端点剔除（先审计再丢）────────────────────────────
        let mut dropped_c: Vec<&crate::instant::insttab::InstEntry> = Vec::new();
        for &pid in &all_ids {
            if let Some(e) = pseudo_entry(pid, bid, table) {
                dropped_c.push(e);
            }
        }

        // ── 真实端点 = 全部 - (b 丢弃) - (c 伪端点) ───────────────────────
        let real: Vec<i64> = all_ids
            .iter()
            .copied()
            .filter(|&pid| {
                !dropped_b.contains(&pid) && pseudo_entry(pid, bid, table).is_none()
            })
            .collect();

        // 空网：整张丢（审计）
        if real.is_empty() {
            for e in &dropped_c {
                log.records.push(ProjectionRecord {
                    layer: layer.to_string(),
                    rule: "c",
                    net: name_src.clone(),
                    endpoint: e.path.clone(),
                    note: "伪端点剔除后网为空，整网丢弃".to_string(),
                });
            }
            continue;
        }

        // ── (c) 的常规审计（非空网逐端点记录）────────────────────────────
        for e in &dropped_c {
            log.records.push(ProjectionRecord {
                layer: layer.to_string(),
                rule: "c",
                net: name_src.clone(),
                endpoint: e.path.clone(),
                note: "本层边界声明（Port/Label），不是电气连接点".to_string(),
            });
        }

        // ── ★ P7-3: 电源网规格（class + driver），全部来自端口声明 ────────
        //   Ground 角色伪端点存在 → Ground（R-1，全局同一地，无 driver）；
        //   Power 角色伪端点存在 → Power，driver 两级解析：
        //     (a) 真实端点里 io==Out 且 member==Power（modldo.VCC / moddcdc.VCC_1V2）
        //     (b) 否则对每个 Power 成员端点（io != In）做子层产生侧检查——
        //         原始子块里该端点所在网若只穿过两脚无源件（如 usbsocket 的
        //         vin.POWER_SYS 经 R0603），它就是源（speaker 直喂 8 脚 lpa ⇒ 消费侧）
        let rail = detect_rail_spec(&all_ids, &real, block, table, layer);

        // ── 产出：单一扁平组（rail/信号在 Phase 3 均按扁平端点集消费）────
        let mut net = McVecNet::new(nets[sorted[0]].nid, name_src, vec![McVec::new(real)]);
        net.rail = rail;
        out.push(net);
    }

    // 回填 after
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
        "[project] layer '{layer}': nets {} -> {} (a: 合并 {merges} 组, b: 去重 {dedups}, c: 伪端点 {pseudos})",
        nets.len(),
        out.len()
    );
    out
}

/// 合并组的显示名 —— **从端口声明读**，不依赖成员网名
/// （builder 的网分组跨运行不稳定，从成员网名推显示名会翻车）。
///
/// 1. 组内含 Ground 角色伪端点 → 组内 Label 伪端点的叶名（GND 组 → "GND"，裁决⑥全局地）
/// 2. 组内含 Power 角色伪端点 → 该伪端点路径的末两段（→ "V3V3.VCC"）
/// 3. 其余（信号组合并，如 MIC.N + MIC.N~0）→ 真实端点最多的成员网名（并列取 nid 最小）
fn group_display_name(sorted: &[usize], nets: &[McVecNet], bid: i64, table: &InstTable) -> String {
    // 组内全部伪端点（按 nid 顺序遍历成员网，保证确定序）
    // 注意：Ground/Power 角色挂在 Port 伪点上（V*.GND / V*.VCC 成员声明），
    // 而 Label 伪点（main.GND）的 member_info 是 None —— 角色探测与取名单独两步。
    let mut has_ground = false;
    let mut label_leaf: Option<String> = None;
    let mut power_port: Option<String> = None;
    'outer: for &i in sorted {
        for pid in nets[i].all_point_ids() {
            let Some(e) = pseudo_entry(pid, bid, table) else { continue };
            match e.member_info.as_ref().map(|m| m.role.clone()) {
                Some(MemberRole::Ground) => has_ground = true,
                Some(MemberRole::Power) => {
                    // rail 成员：路径末两段（main.V3V3.VCC → "V3V3.VCC"）
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
        if let Some(n) = label_leaf {
            return n; // 全局地：Label 叶名即网名（GND 组 → "GND"，裁决⑥）
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

// ============================================================================
// ★ P7-3: 电源网规格解析（判据全部来自端口声明，零名字匹配）
// ============================================================================

use crate::semantic::common::IOType;
use crate::vector::model::{RailClass, RailSpec};

/// 从一组的伪端点角色 + 真实端点声明解析电源网规格；普通信号组返回 `None`。
fn detect_rail_spec(
    all_ids: &[i64],
    real: &[i64],
    block: &McVecBlock,
    table: &InstTable,
    layer: &str,
) -> Option<RailSpec> {
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
    if !has_ground && !has_power {
        return None; // 普通信号网
    }
    let class = if has_ground { RailClass::Ground } else { RailClass::Power };
    let driver_pin = if class == RailClass::Ground {
        None // R-1：地是回流端，out 声明（如 modldo 的 out GND）不是 driver
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

/// Power rail 的产生侧（两级解析，见 detect_rail_spec 文档）。
fn resolve_power_driver(real: &[i64], block: &McVecBlock, table: &InstTable) -> Option<i64> {
    // (a) io == Out 且 member == Power
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
        // 多 driver（DRC 异常态）：确定性取 id 最小者并记录
        crate::vlog!(
            "[project] rail 有 {} 个 Out+Power 端点（多 driver），取 id 最小者",
            by_out.len()
        );
        return Some(*by_out.iter().min().unwrap());
    }

    // (b) Power 成员端点（io != In）逐个做子层产生侧检查，唯一源才成立
    let mut sources: Vec<i64> = Vec::new();
    for &pid in real {
        if pid < 0 {
            continue;
        }
        let Some(e) = table.get_entry(pid as u32) else { continue };
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
            crate::vlog!("[project] rail 有 {} 个候选源（歧义），按无 driver 处理", sources.len());
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

/// 子层产生侧检查：原始子块里该边界端点所在网，除边界声明（parent == 子模块）
/// 与两脚无源件外不触任何有源器件 ⇒ 该端点是"产生侧"。
///
/// 标本：usbsocket.vin.POWER_SYS 原始网 = [R0603.2, 边界] → 只穿无源件 → 源；
///       speaker.USB_VBUS_1.VDD_3V 原始网 = [lpa.7(8脚), C8.1, 边界] → 触 IC → 消费侧。
fn is_rail_source_in_subblock(pin_id: i64, block: &McVecBlock, table: &InstTable) -> bool {
    let Some(parent_mod) = table
        .get_entry(pin_id as u32)
        .and_then(|e| e.parent_id)
    else {
        return false;
    };
    let Some(sub) = block.blocks.iter().find(|b| b.bid == parent_mod as i64) else {
        return false; // 无子块（组件引脚等）——子层内部判定交给 (a) 级
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
                continue; // 子块自己的边界声明，透明
            }
            let passive = table
                .get_entry(op)
                .map_or(false, |pe| {
                    pe.kind == InstKind::Component && table.get_pins_of(op).len() <= 2
                });
            if !passive {
                return false; // 触到有源器件 ⇒ 消费侧
            }
        }
    }
    true
}

// ============================================================================
// Union-Find（与 coalesce.rs 同型，私有实现避免跨模块耦合）
// ============================================================================

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
