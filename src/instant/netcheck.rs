// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # 网表体检（Tier 0 · NETLIST CORRECTNESS）
//!
//! **这个模块只读，不修改任何数据。** 它在 pass2 结束、进入 viz 之前跑一遍，
//! 回答一个问题：*现在这份网表在电气上是对的吗？*
//!
//! 现有的 Tier 1 CORRECTNESS 检查的是「渲染完整性」（无 NaN / 不出画布 /
//! 每条 net 都画出来了）——它对一份**电气错误**的网表是全绿的。
//! 所以短路能长期存活。本模块补上这一层。
//!
//! ## 用法
//!
//! ```ignore
//! let report = netcheck::run(&inst_table);
//! report.print();                  // 打印表格
//! if !report.is_clean() {
//!     // CI 里可以在这里 fail
//! }
//! ```
//!
//! 想接 Pass1 的符号数做守恒检查（R10），传一张
//! `module_path -> pass1 component count` 的表：
//!
//! ```ignore
//! let report = netcheck::run_with_expectation(&inst_table, &expect);
//! ```
//!
//! ## 规则一览
//!
//! | 规则 | 等级 | 含义 |
//! |---|---|---|
//! | R01 LITERAL_POINT      | ERROR | 端点 path 里有 `{` `[` `,` —— 向量引用没展开 |
//! | R02 SHORT_PASSIVE      | ERROR | 二端器件两个脚落在同一张网 |
//! | R03 SHORT_RAIL         | ERROR | 一张网里有两个不同的电源域名（含 VDD 与 GND 同网） |
//! | R04 SHORT_LANE         | ERROR | 同一个总线的两个不同成员落在同一张网 |
//! | R05 UNRESOLVED_UNIT    | ERROR | 单位类型实参无法认领任何形参槽位 |
//! | R06 MEGANET            | WARN  | 非电源网点数过多且跨越器件过多 |
//! | R07 GHOST_INSTANCE     | ERROR | 网里引用的器件，实例表里没有 |
//! | R09 FLOATING_POWER_PIN | WARN  | 器件的电源 / 地管脚没有连接 |
//! | R10 SYMBOL_CONSERVATION| ERROR | Pass2 器件数 < Pass1 符号表里的器件数（需外部传入期望值） |
//! | R11 SPLIT_RAIL         | ERROR | 同一模块内同名电源网被拆成多张互不相连的网 |
//! | R12 DANGLING_PORT      | INFO  | 端口网只有它自己一个点 |
//! | R14 ORPHAN_INSTANCE     | WARN  | 注册了但不在任何网里的实例 |
//! | R15 SYNTHETIC_PIN       | WARN  | 合成端子（pin_id 不属于任何真实管脚，来自端口标量/成员处理） |

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::insttab::{InstKind, InstTable};

// ============================================================================
// 配置常量
// ============================================================================

/// R06：非电源网超过这么多点就可疑
const MEGANET_POINTS: usize = 8;
/// R06：且跨越这么多个不同器件才算可疑（纯扇出的信号网不算）
const MEGANET_OWNERS: usize = 3;

// ============================================================================
// 结果类型
// ============================================================================

/// 规则等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// 只报告，不影响 gate
    Info,
    /// 可疑，不影响 gate（但趋势要向下）
    Warn,
    /// 网表是错的，gate 必须红
    Error,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Info => "INFO ",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
        }
    }
}

/// 一条违规记录
#[derive(Debug, Clone)]
pub struct Finding {
    /// 规则号，如 "R01"
    pub rule: &'static str,
    pub level: Level,
    /// 所属模块路径（尽力而为，取不到时为空）
    pub module: String,
    /// 人类可读的一行描述
    pub detail: String,
}

/// 体检报告
#[derive(Debug, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// 每条规则的命中数（含 0 的规则，便于出稳定表格）
    pub counts: BTreeMap<&'static str, usize>,
    /// 每条规则本轮扫描的对象数（0 表示规则未实际运行）
    pub scanned: BTreeMap<&'static str, usize>,
    /// 统计信息
    pub total_nets: usize,
    pub total_components: usize,
    pub total_modules: usize,
}

impl Report {
    /// 没有 ERROR 级别的违规
    pub fn is_clean(&self) -> bool {
        !self.findings.iter().any(|f| f.level == Level::Error)
    }

    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.level == Level::Error)
            .count()
    }

    /// 渲染成表格字符串
    pub fn render(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "┌ netcheck ─────────────────────────────────────────────────────────"
        );
        let _ = writeln!(
            s,
            "│ {} modules / {} components / {} nets",
            self.total_modules, self.total_components, self.total_nets
        );
        let _ = writeln!(
            s,
            "├───────────────────────────────────────────────────────────────────"
        );
        let _ = writeln!(s, "│ 规则                         命中数（唯一值/命中数）");

        for (rule, n) in &self.counts {
            let lvl = rule_level(rule);
            let scanned = self.scanned.get(rule).copied().unwrap_or(0);
            let mark = if scanned == 0 {
                "·"
            } else if *n == 0 {
                "✓"
            } else if lvl == Level::Error {
                "✗"
            } else {
                "·"
            };
            let status = if scanned == 0 { "SKIP" } else { "" };
            let _ = writeln!(
                s,
                "│ {} {} {:<22} {:>4}  {}",
                mark,
                lvl.tag(),
                rule_name(rule),
                n,
                status
            );
        }

        if !self.findings.is_empty() {
            let _ = writeln!(
                s,
                "├─ 明细 ────────────────────────────────────────────────────────────"
            );
            // 按 (module, rule) 排序，输出稳定
            let mut sorted: Vec<&Finding> = self.findings.iter().collect();
            sorted.sort_by(|a, b| {
                (a.module.as_str(), a.rule, a.detail.as_str()).cmp(&(
                    b.module.as_str(),
                    b.rule,
                    b.detail.as_str(),
                ))
            });
            let mut cur_mod = String::from("\u{0}");
            for f in sorted {
                if f.module != cur_mod {
                    cur_mod = f.module.clone();
                    let name = if cur_mod.is_empty() {
                        "<顶层/未归属>"
                    } else {
                        cur_mod.as_str()
                    };
                    let _ = writeln!(s, "│ ── {name}");
                }
                let _ = writeln!(s, "│   [{}] {}", f.rule, f.detail);
            }
        }

        let total_errors: usize = self
            .counts
            .iter()
            .filter(|(rule, _)| rule_level(rule) == Level::Error)
            .map(|(_, &n)| n)
            .sum();
        let total_warns: usize = self
            .counts
            .iter()
            .filter(|(rule, _)| rule_level(rule) == Level::Warn)
            .map(|(_, &n)| n)
            .sum();
        let _ = writeln!(
            s,
            "└─ {} error(s)（命中总数）, {} warn(s)（命中总数） ─────────────────",
            total_errors, total_warns
        );
        s
    }

    pub fn print(&self) {
        // 用 eprintln 而不是 velog，保证在任何日志配置下都能看到
        mcc_dbg!("inst::mod", "{}", self.render());
    }
}

fn rule_level(rule: &str) -> Level {
    match rule {
        "R03a" | "R12" => Level::Info,
        "R06" | "R09" | "R14" | "R15" => Level::Warn,
        _ => Level::Error,
    }
}

fn rule_name(rule: &str) -> &'static str {
    match rule {
        "R01" => "R01 LITERAL_POINT",
        "R02" => "R02 SHORT_PASSIVE",
        "R03" => "R03 SHORT_RAIL",
        "R03a" => "R03a RAIL_ALIAS",
        "R04" => "R04 SHORT_LANE",
        "R05" => "R05 UNRESOLVED_UNIT",
        "R06" => "R06 MEGANET",
        "R07" => "R07 GHOST_INSTANCE",
        "R08" => "R08 PHANTOM_PATH",
        "R09" => "R09 FLOATING_PWR_PIN",
        "R10" => "R10 SYMBOL_CONSERV",
        "R11" => "R11 SPLIT_RAIL",
        "R12" => "R12 DANGLING_PORT",
        "R14" => "R14 ORPHAN_INSTANCE",
        "R15" => "R15 SYNTHETIC_PIN",
        _ => "?",
    }
}

// ============================================================================
// 入口
// ============================================================================

/// 跑全部规则（不含 R10，因为它需要 Pass1 的期望值）
pub fn run(table: &InstTable) -> Report {
    run_with_expectation(table, &BTreeMap::new())
}

/// 跑全部规则。
///
/// `pass1_expect`：`module 完整路径 -> Pass1 符号表里该模块的 Component 条目数`。
/// 传空表则跳过 R10。
pub fn run_with_expectation(table: &InstTable, pass1_expect: &BTreeMap<String, usize>) -> Report {
    let mut rep = Report::default();

    // 所有规则都登记一次，保证 0 命中的规则也出现在表里
    for r in [
        "R01", "R02", "R03", "R03a", "R04", "R05", "R06", "R07", "R08", "R09", "R10", "R11", "R12",
        "R14", "R15",
    ] {
        rep.counts.insert(r, 0);
    }

    let idx = Index::build(table);

    rep.total_nets = table.net_count();
    rep.total_components = table.get_components().len();
    rep.total_modules = table.get_modules().len();

    check_r01_literal_point(table, &idx, &mut rep);
    check_r02_short_passive(table, &idx, &mut rep);
    check_r03_r04_r06(table, &idx, &mut rep);
    check_r05_unresolved_unit(&mut rep);
    check_r07_ghost(table, &idx, &mut rep);
    check_r08_phantom_path(table, &idx, &mut rep);
    check_r09_floating_power(table, &idx, &mut rep);
    check_r10_conservation(table, &idx, pass1_expect, &mut rep);
    check_r11_split_rail(table, &idx, &mut rep);
    check_r12_dangling_port(table, &idx, &mut rep);
    check_r14_orphan_instance(table, &idx, &mut rep);
    check_r15_synthetic_pin(&mut rep);

    rep
}

// ============================================================================
// 索引：把「点 -> 所属模块」等反复要用的映射预先算好
// ============================================================================

struct Index {
    /// entry id -> 最近的 Module 祖先 id
    nearest_module: BTreeMap<u32, u32>,
    /// module id -> 路径
    module_path: BTreeMap<u32, String>,
    /// net id -> 归属模块路径（尽力而为）
    net_module: BTreeMap<u32, String>,
    /// entry id -> 拥有它的 Component id（自己是 Component 时就是自己）
    owner_comp: BTreeMap<u32, u32>,
}

impl Index {
    fn build(table: &InstTable) -> Self {
        let mut nearest_module = BTreeMap::new();
        let mut module_path = BTreeMap::new();
        let mut owner_comp = BTreeMap::new();

        for (id, e) in table.iter() {
            if e.kind == InstKind::Module {
                module_path.insert(*id, e.path.clone());
            }
        }

        for (id, _) in table.iter() {
            // 向上走找最近的 Module
            let mut cur = table.get_entry(*id).and_then(|e| e.parent_id);
            let mut guard = 0usize;
            while let Some(p) = cur {
                guard += 1;
                if guard > 256 {
                    break; // 防御环
                }
                match table.get_entry(p) {
                    Some(pe) => {
                        if pe.kind == InstKind::Module {
                            nearest_module.insert(*id, p);
                            break;
                        }
                        cur = pe.parent_id;
                    }
                    None => break,
                }
            }

            // 向上走找最近的 Component
            let mut cur = Some(*id);
            let mut guard = 0usize;
            while let Some(c) = cur {
                guard += 1;
                if guard > 256 {
                    break;
                }
                match table.get_entry(c) {
                    Some(ce) => {
                        if ce.kind == InstKind::Component {
                            owner_comp.insert(*id, c);
                            break;
                        }
                        cur = ce.parent_id;
                    }
                    None => break,
                }
            }
        }

        // net 归属模块 = 所有点的最近模块里，路径最长的那个公共祖先
        let mut net_module = BTreeMap::new();
        for net in table.get_nets() {
            let mut cands: Vec<&str> = Vec::new();
            for p in &net.points {
                if let Some(m) = nearest_module.get(p) {
                    if let Some(path) = module_path.get(m) {
                        cands.push(path.as_str());
                    }
                }
            }
            let m = common_module_prefix(&cands);
            net_module.insert(net.id, m);
        }

        Index {
            nearest_module,
            module_path,
            net_module,
            owner_comp,
        }
    }

    fn module_of_net(&self, net_id: u32) -> &str {
        self.net_module
            .get(&net_id)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    fn module_of_entry(&self, id: u32) -> &str {
        self.nearest_module
            .get(&id)
            .and_then(|m| self.module_path.get(m))
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}

/// 取一组模块路径的最长公共前缀（按 `.` 分段）
fn common_module_prefix(paths: &[&str]) -> String {
    if paths.is_empty() {
        return String::new();
    }
    let first: Vec<&str> = paths[0].split('.').collect();
    let mut n = first.len();
    for p in &paths[1..] {
        let segs: Vec<&str> = p.split('.').collect();
        let mut k = 0;
        while k < n && k < segs.len() && first[k] == segs[k] {
            k += 1;
        }
        n = k;
    }
    first[..n].join(".")
}

// ============================================================================
// 字符串工具（自带，不依赖 viz 层，避免跨层耦合）
// ============================================================================

/// 取路径最后一段：`"main.mic.MIC/P"` -> `"P"`
fn leaf(path: &str) -> &str {
    let a = path.rsplit('.').next().unwrap_or(path);
    a.rsplit('/').next().unwrap_or(a)
}

/// 去掉最后一段：`"main.modldo.ldo.1"` -> `Some("main.modldo.ldo")`
fn owner_path(path: &str) -> Option<&str> {
    // 先按 '/' 再按 '.'，取更靠后的那个分隔符
    let dot = path.rfind('.');
    let slash = path.rfind('/');
    let cut = match (dot, slash) {
        (Some(d), Some(s)) => Some(d.max(s)),
        (Some(d), None) => Some(d),
        (None, Some(s)) => Some(s),
        (None, None) => None,
    }?;
    if cut == 0 {
        None
    } else {
        Some(&path[..cut])
    }
}

/// 名字看起来像地
fn is_ground_name(s: &str) -> bool {
    let u = leaf(s).to_uppercase();
    matches!(
        u.as_str(),
        "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GROUND" | "EARTH"
    )
}

/// 名字看起来像电源（不含地）
fn is_supply_name(s: &str) -> bool {
    let u = leaf(s).to_uppercase();
    if is_ground_name(&u) {
        return false;
    }
    const EXACT: &[&str] = &[
        "VCC",
        "VDD",
        "VBUS",
        "VPP",
        "AVDD",
        "DVDD",
        "POWER_SYS",
        "VBAT",
        "VIN",
        "VOUT",
    ];
    if EXACT.contains(&u.as_str()) {
        return true;
    }
    if ["VCC", "VDD", "AVDD", "DVDD", "VBUS", "VBAT"]
        .iter()
        .any(|p| u.starts_with(p))
    {
        return true;
    }
    // 3V3 / 5V0 / 1V2 / V3V3 / V5V 这类
    let bytes = u.as_bytes();
    let digits = bytes.iter().filter(|b| b.is_ascii_digit()).count();
    if u.contains('V') && digits >= 1 && u.len() <= 8 {
        // 排除纯管脚名（VO1 / VO2 这类放大器输出）
        if !u.starts_with("VO") {
            return true;
        }
    }
    false
}

/// 电源网的归一化身份，用于 R11（同名电源不该有两张网）
fn rail_identity(s: &str) -> Option<String> {
    let l = leaf(s);
    if is_ground_name(l) {
        return Some("GND".to_string());
    }
    if is_supply_name(l) {
        return Some(l.to_uppercase());
    }
    None
}

// ============================================================================
// R01 · 未展开的向量引用
// ============================================================================

fn check_r01_literal_point(table: &InstTable, idx: &Index, rep: &mut Report) {
    // ★ 补丁 2-1：隔离后的字面量点已不在 InstTable 中，
    // 直接从 LITERAL_POINT_DETAILS 读取完整清单。
    let details = crate::instant::mc_net::LITERAL_POINT_DETAILS
        .lock()
        .unwrap();
    if !details.is_empty() {
        // ★ 去重：按 path 分桶，保留出现次数
        let mut buckets: BTreeMap<&str, usize> = BTreeMap::new();
        for (path, _) in details.iter() {
            *buckets.entry(path.as_str()).or_insert(0) += 1;
        }
        let unique = buckets.len();
        let total: usize = buckets.values().sum();
        set_scanned(rep, "R01", total);

        // 按出现次数降序排列
        let mut sorted: Vec<(&str, usize)> = buckets.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

        let items: Vec<String> = sorted
            .iter()
            .map(|(path, count)| {
                if *count > 1 {
                    format!("`{path}` ×{count}")
                } else {
                    format!("`{path}`")
                }
            })
            .collect();
        *rep.counts.entry("R01").or_insert(0) = unique;
        rep.findings.push(Finding {
            rule: "R01",
            level: rule_level("R01"),
            module: String::new(),
            detail: format!(
                "{} 个未展开的向量引用（{} 个唯一，{} 次出现）: {}",
                total,
                unique,
                total,
                items.join("  ")
            ),
        });
        return; // 隔离后不需要再扫 InstTable
    }

    // 兜底：如果隔离没生效（比如 release 优化掉了），仍走旧路径
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    for net in table.get_nets() {
        for p in &net.points {
            seen.insert(*p);
        }
    }

    let mut scanned = 0usize;
    for id in seen {
        let Some(e) = table.get_entry(id) else {
            continue;
        };
        scanned += 1;
        if e.path.contains('{') || e.path.contains('[') || e.path.contains(',') {
            push(
                rep,
                "R01",
                idx.module_of_entry(id).to_string(),
                format!(
                    "未展开的向量引用进入网表: `{}` (id={}, kind={})",
                    e.path, e.id, e.kind
                ),
            );
        }
    }

    // 网名里也不该有括号
    for net in table.get_nets() {
        if net.name.contains('{') || net.name.contains('[') || net.name.contains(',') {
            push(
                rep,
                "R01",
                idx.module_of_net(net.id).to_string(),
                format!("网名含字面量括号: `{}` (net#{})", net.name, net.id),
            );
        }
    }
    set_scanned(rep, "R01", scanned);
}

// ============================================================================
// R02 · 二端器件两脚同网
// ============================================================================

fn check_r02_short_passive(table: &InstTable, idx: &Index, rep: &mut Report) {
    let mut scanned = 0usize;
    for comp in table.get_components() {
        let pins = table.get_pins_of(comp.id);
        if pins.len() != 2 {
            continue;
        }
        scanned += 1;
        let n0 = table.get_net_of(pins[0].id).map(|n| n.id);
        let n1 = table.get_net_of(pins[1].id).map(|n| n.id);
        if let (Some(a), Some(b)) = (n0, n1) {
            if a == b {
                let net_name = table.get_net(a).map(|n| n.name.clone()).unwrap_or_default();
                push(
                    rep,
                    "R02",
                    idx.module_of_entry(comp.id).to_string(),
                    format!(
                        "二端器件 `{}` ({}) 两脚都在网 `{}` (net#{}) —— 短路",
                        comp.path, comp.class_name, net_name, a
                    ),
                );
            }
        }
    }
    set_scanned(rep, "R02", scanned);
}

// ============================================================================
// R03 / R04 / R06 · 网内部的语义冲突
// ============================================================================

fn check_r03_r04_r06(table: &InstTable, idx: &Index, rep: &mut Report) {
    set_scanned(rep, "R03", table.net_count());
    set_scanned(rep, "R03a", table.net_count());
    set_scanned(rep, "R04", table.net_count());
    set_scanned(rep, "R06", table.net_count());

    for net in table.get_nets() {
        let module = idx.module_of_net(net.id).to_string();

        // ── 收集这张网里的信息 ──
        let mut supplies: BTreeSet<String> = BTreeSet::new();
        let mut grounds: BTreeSet<String> = BTreeSet::new();
        // bus 前缀 -> 出现过的成员名集合
        let mut bus_members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut owners: BTreeSet<u32> = BTreeSet::new();
        let mut has_rail = false;

        for p in &net.points {
            let Some(e) = table.get_entry(*p) else {
                continue;
            };
            let l = leaf(&e.path);

            if is_ground_name(l) {
                grounds.insert(l.to_uppercase());
                has_rail = true;
            } else if is_supply_name(l) {
                supplies.insert(l.to_uppercase());
                has_rail = true;
            }

            // bus 成员：`X.MIC.P` 的前缀是 `X.MIC`，成员是 `P`
            if let Some(op) = owner_path(&e.path) {
                // 只有当 owner 本身是 Bus / Port / Interface 性质时才算 lane
                // （Component 的管脚不算 —— R1.1 和 R1.2 同网由 R02 管）
                let owner_is_bus = table
                    .get_id_by_path(op)
                    .and_then(|oid| table.get_entry(oid))
                    .map(|oe| matches!(oe.kind, InstKind::Bus | InstKind::Port))
                    .unwrap_or(false);
                if owner_is_bus {
                    bus_members
                        .entry(op.to_string())
                        .or_default()
                        .insert(l.to_string());
                }
            }

            if let Some(c) = idx.owner_comp.get(p) {
                owners.insert(*c);
            }
        }

        // ── R03：电源-地短路（ERROR） ──
        if !supplies.is_empty() && !grounds.is_empty() {
            push(
                rep,
                "R03",
                module.clone(),
                format!(
                    "网 `{}` (net#{}) 电源与地同网: {:?} + {:?} —— 短路",
                    net.name, net.id, supplies, grounds
                ),
            );
        }

        // ── R03a：电源域别名共存（INFO） ──
        let distinct_supplies = supplies.len();
        if distinct_supplies >= 2 {
            push(
                rep,
                "R03a",
                module.clone(),
                format!(
                    "网 `{}` (net#{}) 同时含多个电源域: {:?} —— 若这些名字代表不同电压则为短路",
                    net.name, net.id, supplies
                ),
            );
        }

        // ── R04：同一总线的多个成员同网 ──
        for (bus, members) in &bus_members {
            if members.len() >= 2 {
                push(
                    rep,
                    "R04",
                    module.clone(),
                    format!(
                        "总线 `{}` 的 {} 个成员落在同一张网 `{}` (net#{}): {:?}",
                        bus,
                        members.len(),
                        net.name,
                        net.id,
                        members
                    ),
                );
            }
        }

        // ── R06：巨网 ──
        if !has_rail && net.points.len() > MEGANET_POINTS && owners.len() > MEGANET_OWNERS {
            push(
                rep,
                "R06",
                module.clone(),
                format!(
                    "网 `{}` (net#{}) 有 {} 个点、跨 {} 个器件，非电源网不应这么大",
                    net.name,
                    net.id,
                    net.points.len(),
                    owners.len()
                ),
            );
        }
    }
}

// ============================================================================
// R07 · 幽灵实例 —— 端点 owner 必须解析到 InstTable 中已注册的合法条目
// ============================================================================
//
// 白名单：owner ∈ {Component, Module, Bus, Port} 合法
// 解析不出任何 entry、或 entry 是裸类名残片 → 报
//
// 单测标本：speaker 的 `DIO`（类名残片，在 InstTable 里查不到）

fn check_r07_ghost(table: &InstTable, idx: &Index, rep: &mut Report) {
    // ★ P0.5-3: 预计算每个模块的直属 Component 子节点路径集合。
    // 之前的判据"owner 在 entries 里就放行"是自证式 —— 幽灵的出生地就是 entries。
    // 新判据：对于 owner 是 Component 的端点，owner 必须出现在该模块的
    // children（kind==Component）中，而不只是"entries 里有这个字符串"。
    // 非 Component 的 owner（Module/Bus/Port/Label）跳过，由 R08 等规则处理。
    let mut module_components: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for m in table.get_modules() {
        let comps: BTreeSet<String> = table
            .children_of(m.id)
            .iter()
            .filter(|e| e.kind == InstKind::Component)
            .map(|e| e.path.clone())
            .collect();
        module_components.insert(m.id, comps);
    }

    // (module_id, owner_path) → 该 owner 引用的组件名集合
    let mut ghosts: BTreeMap<(u32, String), BTreeSet<String>> = BTreeMap::new();
    let mut scanned = 0usize;

    for net in table.get_nets() {
        for p in &net.points {
            let Some(e) = table.get_entry(*p) else {
                continue;
            };

            // 第 1 步 · 确定 owner：路径中最后一个点之前的部分，无点则为路径本身
            let owner = owner_path(&e.path)
                .map(|op| op.to_string())
                .unwrap_or_else(|| leaf(&e.path).to_string());

            // 第 2 步 · 找到端点所属的最近模块
            let module_id = match idx.nearest_module.get(p) {
                Some(m) => *m,
                None => continue,
            };

            // 第 3 步 · 查 owner 的注册类型
            // 只有 Component 类型的 owner 才需要检查是否在模块的 children 中
            // Module/Bus/Port/Label 类型的 owner 是合法的非 Component 引用，跳过
            let owner_kind = table
                .get_id_by_path(&owner)
                .and_then(|oid| table.get_entry(oid))
                .map(|oe| oe.kind.clone());

            match owner_kind {
                Some(InstKind::Component) => {
                    scanned += 1;
                    // Component 类型的 owner：必须出现在模块的 children 中
                    let is_valid = module_components
                        .get(&module_id)
                        .map(|comps| comps.contains(&owner))
                        .unwrap_or(false);
                    if !is_valid {
                        let comp_name = leaf(&owner).to_string();
                        ghosts
                            .entry((module_id, owner))
                            .or_default()
                            .insert(comp_name);
                    }
                }
                Some(InstKind::Module | InstKind::Bus | InstKind::Port) => {
                    // 合法引用，不是 ghost
                }
                Some(_) | None => {
                    // Label/Pin 类型或解析不出任何 entry → 类名残片（如 DIO）
                    scanned += 1;
                    let comp_name = leaf(&owner).to_string();
                    ghosts
                        .entry((module_id, owner))
                        .or_default()
                        .insert(comp_name);
                }
            }
        }
    }

    set_scanned(rep, "R07", scanned);

    // 按模块汇总报告
    let mut module_ghosts: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for ((module_id, _owner), comps) in &ghosts {
        module_ghosts
            .entry(*module_id)
            .or_default()
            .extend(comps.iter().cloned());
    }

    for (module_id, comps) in &module_ghosts {
        let module_name = idx
            .module_path
            .get(module_id)
            .map(|s| s.as_str())
            .unwrap_or("?");
        let mut names: Vec<&str> = comps.iter().map(|s| s.as_str()).collect();
        names.sort();
        push(
            rep,
            "R07",
            module_name.to_string(),
            format!(
                "{} 引用了 {} 个未注册器件 — {}",
                leaf(module_name),
                comps.len(),
                names.join(" ")
            ),
        );
    }
}

// ============================================================================
// R08 · 幻影路径 —— 中间段必须是已注册实例，不能只是字符串
// ============================================================================

fn check_r08_phantom_path(table: &InstTable, idx: &Index, rep: &mut Report) {
    /// 叶子是否是纯数字管脚号
    fn is_numeric_pin_leaf(s: &str) -> bool {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
    }

    // ★ P0.5-3: 预计算每个模块的直属实例子节点（Component + Module）路径集合。
    // 与 R07 同理：之前的判据"中间段在 entries 里就放行"是自证式。
    // 新判据：中间段必须是该模块的 children 中 kind∈{Component,Module} 的条目。
    let mut module_children: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for m in table.get_modules() {
        let children: BTreeSet<String> = table
            .children_of(m.id)
            .iter()
            .filter(|e| matches!(e.kind, InstKind::Component | InstKind::Module))
            .map(|e| e.path.clone())
            .collect();
        module_children.insert(m.id, children);
    }

    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut scanned = 0usize;

    for net in table.get_nets() {
        for p in &net.points {
            let Some(e) = table.get_entry(*p) else {
                continue;
            };
            let leaf_name = leaf(&e.path);

            // 第 1 步 · 筛端点：只处理叶子是纯数字管脚号的端点
            if !is_numeric_pin_leaf(leaf_name) {
                continue;
            }
            scanned += 1;

            // 第 2 步 · 查中间段
            let owner = match owner_path(&e.path) {
                Some(op) => op,
                None => continue,
            };

            // 第 3 步 · 找到端点所属的最近模块
            let module_id = match idx.nearest_module.get(p) {
                Some(m) => *m,
                None => continue,
            };

            // 中间段必须是该模块的直属 Component/Module 子节点
            let owner_is_proper = module_children
                .get(&module_id)
                .map(|children| children.contains(owner))
                .unwrap_or(false);

            if owner_is_proper {
                continue;
            }

            // 中间段未注册 → 检查上层（grandparent）是否在该模块的 children 中
            if let Some(grandparent) = owner_path(owner) {
                let gp_is_proper = module_children
                    .get(&module_id)
                    .map(|children| children.contains(grandparent))
                    .unwrap_or(false);
                if gp_is_proper {
                    let key = format!("R08:{owner}");
                    if seen.insert(key) {
                        push(
                            rep,
                            "R08",
                            idx.module_of_entry(*p).to_string(),
                            format!(
                                "幻影路径: `{}` 的中间段 `{}` 未注册为实例（上层 `{}` 存在）",
                                e.path, owner, grandparent
                            ),
                        );
                    }
                }
            }
        }
    }

    set_scanned(rep, "R08", scanned);
}

// ============================================================================
// R09 · 悬空的电源 / 地管脚
// ============================================================================

fn check_r09_floating_power(table: &InstTable, idx: &Index, rep: &mut Report) {
    let mut scanned = 0usize;
    for comp in table.get_components() {
        for pin in table.get_pins_of(comp.id) {
            let name = leaf(&pin.path);
            // 管脚号形式（"1"/"2"）看不出语义，用 class_name 里的功能名兜一下
            let fname = pin.class_name.trim();
            let is_pwr = is_ground_name(name)
                || is_supply_name(name)
                || is_ground_name(fname)
                || is_supply_name(fname);
            if !is_pwr {
                continue;
            }
            scanned += 1;
            if table.get_net_of(pin.id).is_none() {
                push(
                    rep,
                    "R09",
                    idx.module_of_entry(comp.id).to_string(),
                    format!(
                        "器件 `{}` 的电源/地管脚 `{}` 未连接",
                        comp.path,
                        leaf(&pin.path)
                    ),
                );
            }
        }
    }
    set_scanned(rep, "R09", scanned);
}

// ============================================================================
// R10 · 符号守恒（Pass1 有的，Pass2 必须也有）
// ============================================================================

fn check_r10_conservation(
    table: &InstTable,
    _idx: &Index,
    expect: &BTreeMap<String, usize>,
    rep: &mut Report,
) {
    // ★ 防呆：提前 return 之前先打 SKIP
    if expect.is_empty() {
        note(
            rep,
            "R10",
            "-".to_string(),
            "R10 未接入 pass1 符号表，本轮规则无效".to_string(),
        );
        set_scanned(rep, "R10", 0);
        return;
    }

    // ★ 防呆：若某模块的 pass1 期望集合 size < 2，打 WARN
    for (path, want) in expect {
        if *want < 2 {
            push(
                rep,
                "R10",
                path.clone(),
                format!("R10 期望集合疑似塌陷: {path} 只有 {want} 个 Component，规则本轮无效",),
            );
        }
    }

    // 统计每个模块下直属的 Component 数
    let mut actual: BTreeMap<String, usize> = BTreeMap::new();
    for m in table.get_modules() {
        let n = table
            .children_of(m.id)
            .iter()
            .filter(|e| e.kind == InstKind::Component)
            .count();
        actual.insert(m.path.clone(), n);
    }

    set_scanned(rep, "R10", expect.len());

    for (path, want) in expect {
        let got = actual.get(path).copied().unwrap_or(0);
        if got < *want {
            push(
                rep,
                "R10",
                path.clone(),
                format!(
                    "Pass1 符号表有 {want} 个器件，Pass2 只注册了 {got} 个 —— 少 {}",
                    want - got
                ),
            );
        }
    }
}

// ============================================================================
// R11 · 同名电源被拆成多张网（按 rail_identity 分桶）
// ============================================================================

fn check_r11_split_rail(table: &InstTable, idx: &Index, rep: &mut Report) {
    // rail_identity → 该身份出现在哪些 net
    let mut buckets: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    let mut scanned = 0usize;

    for net in table.get_nets() {
        let mut ids: BTreeSet<String> = BTreeSet::new();
        for p in &net.points {
            if let Some(e) = table.get_entry(*p) {
                if let Some(rid) = rail_identity(&e.path) {
                    ids.insert(rid);
                }
            }
        }
        if !ids.is_empty() {
            scanned += 1;
        }
        for rid in ids {
            buckets.entry(rid).or_default().insert(net.id);
        }
    }

    // ★ P0.5-4: 跨层端口 union —— 先通过端口连接关系合并父子模块的同 rail 网
    //
    // 问题：`main.moddcdc::GND` 和 `main::GND` 通过端口连接，
    // 但 R11 按 net 分桶时看不到这层连接，会误报 SPLIT_RAIL。
    //
    // 方案：对每个模块，收集其端口导出的 rail_identity。
    // 对每个父子模块对，如果子模块的端口导出某 rail_identity，
    // 且父子模块都有该 rail_identity 的 net，则将这些 net union。
    let mut uf: BTreeMap<u32, u32> = BTreeMap::new();
    fn uf_find(uf: &mut BTreeMap<u32, u32>, x: u32) -> u32 {
        let p = *uf.entry(x).or_insert(x);
        if p == x {
            x
        } else {
            let root = uf_find(uf, p);
            uf.insert(x, root);
            root
        }
    }
    fn uf_union(uf: &mut BTreeMap<u32, u32>, a: u32, b: u32) {
        let ra = uf_find(uf, a);
        let rb = uf_find(uf, b);
        if ra != rb {
            uf.insert(ra, rb);
        }
    }

    // 收集每个模块通过端口导出的 rail_identity
    // ★ 不仅查 Port，也查 Bus 的子成员和直接 Label —— 模块的电源参数
    // 可能注册为 Label（如 main.GND）、Bus 子 Label（如 dc.GND）或 Port。
    //
    // ★ P0.5-5: 收紧 union 条件 —— 同时记录每个 rail_identity 对应的端口 entry id，
    // 以便后续验证父层是否确实通过端口绑定连通了该端口。
    // module_id → (rail_identity → Vec<port_entry_id>)
    let mut module_port_rails: BTreeMap<u32, BTreeMap<String, Vec<u32>>> = BTreeMap::new();
    for m in table.get_modules() {
        // 1) 显式端口（Port）
        for port in table.get_ports_of(m.id) {
            if let Some(rid) = rail_identity(&port.path) {
                module_port_rails
                    .entry(m.id)
                    .or_default()
                    .entry(rid)
                    .or_default()
                    .push(port.id);
            }
        }
        // 2) 模块直属的 Bus 子节点 → 其子 Label 的 rail_identity
        //    例如 speaker 的 dc{VDD_3V3, GND} → Bus "dc" → Label "GND" / "VDD_3V3"
        for child in table.children_of(m.id) {
            match child.kind {
                InstKind::Bus => {
                    for grandchild in table.children_of(child.id) {
                        if let Some(rid) = rail_identity(&grandchild.path) {
                            module_port_rails
                                .entry(m.id)
                                .or_default()
                                .entry(rid)
                                .or_default()
                                .push(grandchild.id);
                        }
                    }
                }
                InstKind::Label => {
                    // 3) 直接 Label 子节点（如 main.GND, main.mcu513.GND）
                    if let Some(rid) = rail_identity(&child.path) {
                        module_port_rails
                            .entry(m.id)
                            .or_default()
                            .entry(rid)
                            .or_default()
                            .push(child.id);
                    }
                }
                _ => {}
            }
        }
    }

    // 收集每个 net 所在的模块集合（通过 net 中点的 nearest_module）
    let mut net_modules: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for net in table.get_nets() {
        let mut mods = BTreeSet::new();
        for p in &net.points {
            if let Some(m) = idx.nearest_module.get(p) {
                mods.insert(*m);
            }
        }
        if !mods.is_empty() {
            net_modules.insert(net.id, mods);
        }
    }

    // 对每个父子模块对，union 通过端口连接的同 rail 网
    //
    // ★ P0.5-5: 收紧 union 条件 —— 只有当父层确实通过端口绑定连通了
    // 子模块端口时才 union。仅凭"父层某条连接提到了这个端口名"不构成
    // union 依据。判断方法：端口的 entry 所在的 net 里是否同时包含父模块
    // 的点（即端口被父层连接了）。
    for m in table.get_modules() {
        let parent_entry_id = match table.get_entry(m.id).and_then(|e| e.parent_id) {
            Some(pid) => pid,
            None => continue,
        };

        // ★ parent_entry_id 是父条目的 id（可能是 Component/Module/Label…），
        // 不是父 Module 的 id。如果父条目本身就是 Module，直接用它的 id；
        // 否则通过 nearest_module 向上找到最近的 Module。
        let parent_module_id = {
            let parent_entry = table.get_entry(parent_entry_id);
            match parent_entry {
                Some(pe) if pe.kind == InstKind::Module => parent_entry_id,
                Some(_other) => match idx.nearest_module.get(&parent_entry_id) {
                    Some(pm) => *pm,
                    None => continue,
                },
                None => continue,
            }
        };

        if let Some(port_rails) = module_port_rails.get(&m.id) {
            for (rid, port_eids) in port_rails {
                // ★ 检查：父层是否确实通过端口绑定连通了该端口？
                // 至少有一个端口 entry 所在的 net 包含父模块的点 → 已连通
                let port_connected = port_eids.iter().any(|&eid| {
                    if let Some(net) = table.get_net_of(eid) {
                        net.points
                            .iter()
                            .any(|p| idx.nearest_module.get(p) == Some(&parent_module_id))
                    } else {
                        false
                    }
                });
                if !port_connected {
                    // 父层未绑定此端口，不得 union
                    continue;
                }

                // 收集父模块中该 rail_identity 的 net
                let mut parent_nets: Vec<u32> = Vec::new();
                // 收集子模块中该 rail_identity 的 net
                let mut child_nets: Vec<u32> = Vec::new();

                for (nid, mods) in &net_modules {
                    if let Some(net_set) = buckets.get(rid) {
                        if !net_set.contains(nid) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                    if mods.contains(&parent_module_id) {
                        parent_nets.push(*nid);
                    }
                    if mods.contains(&m.id) {
                        child_nets.push(*nid);
                    }
                }

                // Union 父模块和子模块中同 rail 的 net
                for &pn in &parent_nets {
                    for &cn in &child_nets {
                        uf_union(&mut uf, pn, cn);
                    }
                }
            }
        }
    }

    set_scanned(rep, "R11", scanned);

    // ★ P0.5-6: 按模块 scope —— 只报告同一模块内 rail 被拆成多张网的情况。
    // 先按模块重新分桶，再在每个模块内检查 union 后的分组数。
    // 使用 idx.net_module 作为每个 net 的"主模块"（最深公共祖先），
    // 避免跨模块共享的 net 被计入多个模块。
    let mut module_buckets: BTreeMap<String, BTreeMap<String, BTreeSet<u32>>> = BTreeMap::new();
    for (rid, nets) in &buckets {
        for nid in nets {
            let primary = idx.module_of_net(*nid);
            if primary.is_empty() {
                continue;
            }
            module_buckets
                .entry(primary.to_string())
                .or_default()
                .entry(rid.clone())
                .or_default()
                .insert(*nid);
        }
    }

    for (mod_name, rid_buckets) in &module_buckets {
        for (rid, nets) in rid_buckets {
            let mut groups: BTreeSet<u32> = BTreeSet::new();
            for nid in nets {
                groups.insert(uf_find(&mut uf, *nid));
            }
            if groups.len() >= 2 {
                let mut group_repr: BTreeMap<u32, u32> = BTreeMap::new();
                for nid in nets {
                    let root = uf_find(&mut uf, *nid);
                    group_repr.entry(root).or_insert(*nid);
                }
                let names: Vec<String> = group_repr
                    .values()
                    .filter_map(|n| {
                        table
                            .get_net(*n)
                            .map(|e| format!("{}::{}#{}", mod_name, e.name, e.id))
                    })
                    .collect();
                push(
                    rep,
                    "R11",
                    mod_name.clone(),
                    format!(
                        "模块内电源 `{}` 被拆成 {} 张互不相连的网: {}",
                        rid,
                        groups.len(),
                        names.join(", ")
                    ),
                );
            }
        }
    }
}

// ============================================================================
// R12 · 只有自己一个点的端口网
// ============================================================================

fn check_r12_dangling_port(table: &InstTable, idx: &Index, rep: &mut Report) {
    set_scanned(rep, "R12", table.net_count());
    for net in table.get_nets() {
        if net.points.len() != 1 {
            continue;
        }
        let Some(e) = table.get_entry(net.points[0]) else {
            continue;
        };
        if !matches!(e.kind, InstKind::Port | InstKind::Bus) {
            continue;
        }
        push(
            rep,
            "R12",
            idx.module_of_net(net.id).to_string(),
            format!("端口 `{}` 的网只有它自己（声明了但没接）", e.path),
        );
    }
}

// ============================================================================
// R14 · 孤例 —— 注册了 Component 但不在任何网里
// ============================================================================

fn check_r14_orphan_instance(table: &InstTable, idx: &Index, rep: &mut Report) {
    // 收集所有在网里出现过的 Component owner（通过 net 中点的 owner_comp）
    let mut wired_owners: BTreeSet<u32> = BTreeSet::new();
    for net in table.get_nets() {
        for p in &net.points {
            if let Some(c) = idx.owner_comp.get(p) {
                wired_owners.insert(*c);
            }
        }
    }

    let mut scanned = 0usize;
    let mut orphans: BTreeMap<String, Vec<String>> = BTreeMap::new(); // module -> [comp_names]

    for comp in table.get_components() {
        scanned += 1;
        if wired_owners.contains(&comp.id) {
            continue;
        }
        let module = idx.module_of_entry(comp.id).to_string();
        let mod_key = if module.is_empty() {
            "<顶层>".to_string()
        } else {
            module
        };
        orphans
            .entry(mod_key)
            .or_default()
            .push(leaf(&comp.path).to_string());
    }

    set_scanned(rep, "R14", scanned);

    for (module, names) in &orphans {
        let mut sorted = names.clone();
        sorted.sort();
        push(
            rep,
            "R14",
            module.clone(),
            format!(
                "{} 个实例注册了但不在任何网里: {}",
                sorted.len(),
                sorted.join(", ")
            ),
        );
    }
}

// ============================================================================
// R15 · 合成端子 —— viz 层检测到的 pin_id 不属于任何真实管脚
// ============================================================================

fn check_r15_synthetic_pin(rep: &mut Report) {
    let count = crate::viz::SYNTHETIC_PIN_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    set_scanned(rep, "R15", 1); // R15 runs once per viz render
    if count > 0 {
        rep.counts.insert("R15", count);
        rep.findings.push(Finding {
            rule: "R15",
            level: Level::Warn,
            module: String::new(),
            detail: format!(
                "{} 个合成端子（pin_id 不属于任何真实管脚，可能来自端口标量/成员处理或未解析的端点引用）",
                count
            ),
        });
    }
}

// ============================================================================
// 内部工具
// ============================================================================

fn push(rep: &mut Report, rule: &'static str, module: String, detail: String) {
    *rep.counts.entry(rule).or_insert(0) += 1;
    rep.findings.push(Finding {
        rule,
        level: rule_level(rule),
        module,
        detail,
    });
}

/// 添加一条不增加计数的备注（用于 SKIP 等状态说明），始终为 INFO 级别
fn note(rep: &mut Report, rule: &'static str, module: String, detail: String) {
    rep.findings.push(Finding {
        rule,
        level: Level::Info,
        module,
        detail,
    });
}

fn set_scanned(rep: &mut Report, rule: &'static str, n: usize) {
    rep.scanned.entry(rule).or_insert(n);
}

// ============================================================================
// 单元测试
// ============================================================================

// R05 · UNRESOLVED_UNIT — 单位类型实参无法认领任何形参槽位
// Counter is incremented during parameter binding in mc_param::bind_with_opts.
fn check_r05_unresolved_unit(rep: &mut Report) {
    let count = crate::semantic::basic::mc_param::R05_UNRESOLVED_UNIT
        .load(std::sync::atomic::Ordering::Relaxed);
    set_scanned(rep, "R05", 1); // R05 is a global counter, always "running"
    if count > 0 {
        rep.counts.insert("R05", count);
        rep.findings.push(Finding {
            rule: "R05",
            level: Level::Error,
            module: String::new(),
            detail: format!(
                "{} unit-typed argument(s) could not claim any formal parameter slot",
                count
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_works() {
        assert_eq!(leaf("main.mic.MIC/P"), "P");
        assert_eq!(leaf("main.modldo.ldo.1"), "1");
        assert_eq!(leaf("GND"), "GND");
        assert_eq!(leaf(""), "");
    }

    #[test]
    fn owner_path_works() {
        assert_eq!(owner_path("main.modldo.ldo.1"), Some("main.modldo.ldo"));
        assert_eq!(owner_path("main.mic.MIC/P"), Some("main.mic.MIC"));
        assert_eq!(owner_path("GND"), None);
    }

    #[test]
    fn rail_names() {
        assert!(is_ground_name("GND"));
        assert!(is_ground_name("main.x.VSS"));
        assert!(!is_ground_name("VDD"));

        assert!(is_supply_name("VDD_3V3"));
        assert!(is_supply_name("V3V3"));
        assert!(is_supply_name("VCC_1V2"));
        assert!(is_supply_name("POWER_SYS"));
        // 放大器输出不算电源
        assert!(!is_supply_name("VO1"));
        assert!(!is_supply_name("VO2"));
        // 信号名不算
        assert!(!is_supply_name("DAC_OUT"));
        assert!(!is_supply_name("SCLK"));
    }

    #[test]
    fn rail_identity_merges_grounds() {
        assert_eq!(rail_identity("main.x.GND").as_deref(), Some("GND"));
        assert_eq!(rail_identity("main.x.VSS").as_deref(), Some("GND"));
        assert_eq!(rail_identity("V3V3").as_deref(), Some("V3V3"));
        assert_eq!(rail_identity("DAC_OUT"), None);
    }

    #[test]
    fn common_prefix() {
        assert_eq!(
            common_module_prefix(&["main.modldo", "main.moddcdc"]),
            "main"
        );
        assert_eq!(common_module_prefix(&["main.mic", "main.mic"]), "main.mic");
        assert_eq!(common_module_prefix(&[]), "");
        assert_eq!(common_module_prefix(&["main"]), "main");
    }
}
