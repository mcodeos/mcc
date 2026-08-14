// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-1 · renderdiff —— 渲染层的尺子
//!
//! ## 定位
//! 与 netdiff（网表判据）分离且同等严格（纪律 10）：
//! - netdiff 管"连得对不对"（pass2 golden）
//! - renderdiff 管"画得对不对"（渲染 golden：`baseline/render_golden.toml`）
//!
//! ## 判据组
//! - **G10 结构守恒**：盒子数 vs golden；合成盒子数 == 0（provenance 标记）；
//!   每条网所有端点落在同一 route 连通分量（复用 `RenderedConnectivityReport`）
//! - **G11 电源契约**：GND 边数 == 0；rail power 边数 == R-2 driver 段期望；
//!   顶层无源件 == 0（契约 C5）
//! - **G12 几何合法性**：box_box / wire_box == 0；盒子 w/h ≥ pin 分布最小尺寸（S6）；
//!   无负坐标 / 出画布
//!
//! ## 原则
//! - 判据是**结构相似**不是像素相似；与参考图的每一处可解释差异都记录在
//!   `MC_SCHEMATIC_ROADMAP_v6.md` §1.1 的差异表里
//! - **不许为了判据变绿改 golden**。中途大面积红是正确形状（v6 §4）
//! - 纪律 9：每条判据打印求值对象数，为 0 显示 `· SKIP`，不许显示 `✓`

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::vector::graph::kinds::BoxKind;
use crate::vector::graph::{McVecGraph, NetKind};

// ============================================================================
// Golden (TOML schema)
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderGolden {
    pub layer: BTreeMap<String, LayerGolden>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LayerGolden {
    /// 对应 pass2 golden 的模块名（renderdiff 的 key 是 graph.name = 实例名）
    #[serde(default)]
    pub module: String,
    pub boxes: usize,
    /// golden 期望的盒子名单（按实例名匹配，做 match/missing/extra）
    #[serde(default)]
    pub box_names: Vec<String>,
    /// Phase 1.5/1.6 合成盒子目标数（恒 0）
    #[serde(default)]
    pub synth_boxes: usize,
    /// rail flag 盒子目标数（恒 0，纪律 11）
    #[serde(default)]
    pub rail_flags: usize,
    /// GND 边（跨盒子 ground net）目标数
    #[serde(default)]
    pub gnd_edges: usize,
    /// rail power 边（跨盒子 power net，即 R-2 driver 段）目标数
    #[serde(default)]
    pub power_edges: usize,
    /// 顶层无源件目标数（契约 C5：框图不画 R/C；恒 0，仅顶层有意义）
    #[serde(default)]
    pub top_passives: usize,
    /// 期望的边表（from/to 按**盒子名**，label = 网名或总线条目名）
    #[serde(default)]
    pub edge: Vec<GEdge>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GEdge {
    pub from: String,
    pub to: String,
    pub label: String,
}

// ============================================================================
// Reading (measured from the final graph)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct LayerReading {
    pub layer: String,
    pub bid: i64,
    /// boxes total
    pub total_boxes: usize,
    pub declared_boxes: usize,
    pub synth_endpoint_boxes: usize,
    pub rail_flag_boxes: usize,
    /// 盒子名单（declared 盒子 + 合成盒子，全部列出供 diff）
    pub box_names: Vec<String>,
    /// 跨盒子 ground net 数（= 画出来的 GND 边）
    pub gnd_edges: usize,
    /// 跨盒子 power net 数
    pub power_edges: usize,
    /// TwoPin 无源件盒子数
    pub two_pin_passives: usize,
    /// ★ P7-3 S1：接地符号装饰数（子层 = GND 端点数；顶层恒 0）
    #[serde(default)]
    pub decorations_ground: usize,
    /// ★ P7-3 S2：rail 圆点装饰数（子层 = 非 GND rail 端点数；顶层恒 0）
    #[serde(default)]
    pub decorations_power: usize,
    /// ★ P7-4：本层几何双写数（段边界快照对比采集；目标 0）
    #[serde(default)]
    pub geom_double_writes: usize,
    /// ★ P7-4c：全量双写明细（盒子 / 前写者 → 后写者），基线诊断用；
    /// P7-4e 写者归段合并后应收敛为空。
    #[serde(default)]
    pub geom_double_write_list: Vec<String>,
    /// 跨盒子 net 的 (from,to,label) 列表（无序对）
    pub edges: Vec<(String, String, String)>,
    // G12
    pub box_box: usize,
    pub wire_box: usize,
    pub s6_violations: usize,
    pub offcanvas_boxes: usize,
    // G10 连通性（由调用方从 RenderedConnectivityReport 注入）
    pub pins_total: usize,
    pub pins_unreachable: usize,
    /// 自检：每条判据求值了多少个对象（纪律 9）
    pub evaluated: EvalCounts,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EvalCounts {
    pub boxes: usize,
    pub nets: usize,
    pub sizes: usize,
}

impl LayerReading {
    /// 从 route 之后、render 之前的最终 graph 量测一层的全部读数。
    ///
    /// * `col` —— `audit_all` 的碰撞报告（G12）
    /// * `conn` —— 可选的连通性报告 (pins_total, pins_unreachable)（G10 第三项）。
    ///   `None` 时该判据显示 `· SKIP`。
    pub fn measure(
        graph: &McVecGraph,
        col: &crate::viz::route::audit::CollisionReport,
        conn: Option<(usize, usize)>,
    ) -> Self {
        use crate::vector::graph::boxdef::BoxProvenance as P;

        let mut declared = 0usize;
        let mut synth = 0usize;
        let mut flags = 0usize;
        let mut passives = 0usize;
        let mut s6 = 0usize;
        let mut offcanvas = 0usize;
        let mut names = Vec::new();

        for b in &graph.boxes {
            match b.provenance {
                P::Declared => declared += 1,
                P::SynthesizedFromEndpoint => synth += 1,
                P::SynthesizedRailFlag => flags += 1,
            }
            if matches!(b.kind, BoxKind::TwoPin) {
                passives += 1;
            }
            names.push(if b.name.is_empty() {
                format!("{}#", b.id)
            } else {
                b.name.clone()
            });

            // S6: 盒子装不装得下自己的 pin 分布（复用 size::box_size 的最小尺寸公式）
            let (mw, mh) = crate::viz::layout::size::box_size(b);
            if b.w + 1.0 < mw || b.h + 1.0 < mh {
                s6 += 1;
            }
            if b.x < -0.5 || b.y < -0.5 {
                offcanvas += 1;
            }
        }

        // G11：跨盒子 net 分类（同一 net 端点落在 ≥2 个不同盒子 = 画了一条边）
        let name_of = |id: i64| -> String {
            graph
                .boxes
                .iter()
                .find(|b| b.id == id)
                .map(|b| {
                    if b.name.is_empty() {
                        format!("{}#", b.id)
                    } else {
                        b.name.clone()
                    }
                })
                .unwrap_or_else(|| format!("{}#", id))
        };
        let mut gnd = 0usize;
        let mut pwr = 0usize;
        let mut edges = Vec::new();
        for net in &graph.nets {
            let mut distinct: Vec<i64> = Vec::new();
            for ep in &net.endpoints {
                if !distinct.contains(&ep.box_id) {
                    distinct.push(ep.box_id);
                }
            }
            if distinct.len() < 2 {
                continue;
            }
            match net.kind {
                NetKind::Ground => gnd += 1,
                NetKind::Power => pwr += 1,
                _ => {}
            }
            edges.push((
                name_of(distinct[0]),
                name_of(distinct[1]),
                net.name.clone(),
            ));
        }

        let (pins_total, pins_unreachable) = conn.unwrap_or((0, 0));

        // ★ P7-3: S1/S2 读数 —— pin 装饰数（接地符号 / rail 圆点）。
        // 顶层 R-1/R-3 恒 0（框图不落符号），子层 = 被 R-1/R-3 判"就地落符号"的端点数。
        let (dec_ground, dec_power) = {
            let mut g = 0usize;
            let mut p = 0usize;
            for d in &graph.rail_decorations {
                if d.is_ground {
                    g += 1;
                } else {
                    p += 1;
                }
            }
            (g, p)
        };

        LayerReading {
            layer: graph.name.clone(),
            bid: graph.bid,
            total_boxes: graph.boxes.len(),
            declared_boxes: declared,
            synth_endpoint_boxes: synth,
            rail_flag_boxes: flags,
            box_names: names,
            gnd_edges: gnd,
            power_edges: pwr,
            two_pin_passives: passives,
            edges,
            decorations_ground: dec_ground,
            decorations_power: dec_power,
            geom_double_writes: graph.geom_double_writes.len(),
            geom_double_write_list: graph
                .geom_double_writes
                .iter()
                .map(|d| {
                    format!(
                        "{}#{}: {} -> {} [{}]",
                        d.box_name,
                        d.box_id,
                        d.prev_writer,
                        d.cur_writer,
                        d.dims.join("+")
                    )
                })
                .collect(),
            box_box: col.box_box,
            wire_box: col.wire_box,
            s6_violations: s6,
            offcanvas_boxes: offcanvas,
            pins_total,
            pins_unreachable,
            evaluated: EvalCounts {
                boxes: graph.boxes.len(),
                nets: graph.nets.len(),
                sizes: graph.boxes.len(),
            },
        }
    }
}

// ============================================================================
// Diff (reading vs golden)
// ============================================================================

/// 一条判据的结论。`Skip` = 求值对象数为 0（纪律 9），不许当绿。
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ok(String),
    Fail(String),
    Skip(String),
}

#[derive(Debug, Clone)]
pub struct LayerDiff {
    pub layer: String,
    /// G10/G11/G12 逐条结论
    pub findings: Vec<(String, Verdict)>,
    /// 红的数量（Fail）
    pub red: usize,
    /// 绿的数量（Ok）
    pub green: usize,
    /// 跳过数
    pub skipped: usize,
}

impl LayerDiff {
    pub fn report_line(&self) -> String {
        let head = format!(
            "[renderdiff] layer '{}': {} red / {} green / {} skip",
            self.layer, self.red, self.green, self.skipped
        );
        let body: Vec<String> = self
            .findings
            .iter()
            .map(|(id, v)| {
                let mark = match v {
                    Verdict::Ok(_) => "✓",
                    Verdict::Fail(_) => "✗",
                    Verdict::Skip(_) => "·",
                };
                let msg = match v {
                    Verdict::Ok(m) | Verdict::Fail(m) | Verdict::Skip(m) => m,
                };
                format!("{mark} {id}: {msg}")
            })
            .collect();
        if body.is_empty() {
            head
        } else {
            format!("{head}\n  {}", body.join("\n  "))
        }
    }
}

fn sorted_lower(names: &[String]) -> Vec<String> {
    let mut v: Vec<String> = names.iter().map(|s| s.to_lowercase()).collect();
    v.sort();
    v
}

/// 多重集合 diff：返回 (missing, extra)
fn multiset_diff(expected: &[String], actual: &[String]) -> (Vec<String>, Vec<String>) {
    use std::collections::BTreeMap;
    let mut exp: BTreeMap<&str, i32> = BTreeMap::new();
    let mut act: BTreeMap<&str, i32> = BTreeMap::new();
    for s in expected {
        *exp.entry(s.as_str()).or_insert(0) += 1;
    }
    for s in actual {
        *act.entry(s.as_str()).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, e) in &exp {
        let a = act.get(k).copied().unwrap_or(0);
        if a < *e {
            for _ in 0..(e - a) {
                missing.push(k.to_string());
            }
        }
    }
    for (k, a) in &act {
        let e = exp.get(k).copied().unwrap_or(0);
        if a > &e {
            for _ in 0..(a - e) {
                extra.push(k.to_string());
            }
        }
    }
    (missing, extra)
}

/// 边匹配 key：(无序端点名对, label)。
fn edge_key(e: &(String, String, String)) -> (String, String, String) {
    let (a, b) = if e.0 <= e.1 {
        (&e.0, &e.1)
    } else {
        (&e.1, &e.0)
    };
    (a.clone(), b.clone(), e.2.clone())
}

impl RenderGolden {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("render_golden.toml read failed: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("render_golden.toml parse failed: {e}"))
    }

    /// 一层 reading vs 一层 golden，输出逐条判据结论。
    pub fn diff_layer(&self, r: &LayerReading) -> LayerDiff {
        let g = match self.layer.get(&r.layer) {
            Some(g) => g,
            None => {
                let mut findings = vec![(
                    "G10".to_string(),
                    Verdict::Fail(format!(
                        "layer '{}' not in golden (boxes={}) — golden 需要补录或层名错了",
                        r.layer, r.total_boxes
                    )),
                )];
                findings.push((
                    "G12".to_string(),
                    Verdict::Skip(format!("no golden for layer, collisions box_box={} wire_box={} unjudged", r.box_box, r.wire_box)),
                ));
                return LayerDiff {
                    layer: r.layer.clone(),
                    findings,
                    red: 1,
                    green: 0,
                    skipped: 1,
                };
            }
        };

        let mut findings: Vec<(String, Verdict)> = Vec::new();

        // ── G10 结构守恒 ─────────────────────────────────────────────
        // (1) 盒子数
        findings.push(num_check(
            "G10.boxes",
            g.boxes,
            r.total_boxes,
            r.evaluated.boxes,
            format!(
                "declared={} synth={} flags={}",
                r.declared_boxes, r.synth_endpoint_boxes, r.rail_flag_boxes
            ),
        ));

        // (2) 盒子名单（match/missing/extra）
        if g.box_names.is_empty() {
            findings.push((
                "G10.names".into(),
                Verdict::Skip(format!("golden 无名单，eval={}", r.box_names.len())),
            ));
        } else {
            let (missing, extra) = multiset_diff(
                &sorted_lower(&g.box_names),
                &sorted_lower(&r.box_names),
            );
            if missing.is_empty() && extra.is_empty() {
                findings.push((
                    "G10.names".into(),
                    Verdict::Ok(format!("{} 盒子全部 match", r.box_names.len())),
                ));
            } else {
                findings.push((
                    "G10.names".into(),
                    Verdict::Fail(format!(
                        "missing={} extra={}",
                        fmt_list(&missing),
                        fmt_list(&extra)
                    )),
                ));
            }
        }

        // (3) 合成盒子 == 0
        findings.push(num_check(
            "G10.synth",
            g.synth_boxes,
            r.synth_endpoint_boxes,
            r.evaluated.boxes,
            "Phase1.5/1.6 合成".into(),
        ));

        // (4) rail flag 盒子 == 0
        findings.push(num_check(
            "G10.flags",
            g.rail_flags,
            r.rail_flag_boxes,
            r.evaluated.boxes,
            "纪律11 端子不是盒子".into(),
        ));

        // (5) 连通性：每条网所有端点同一 route 连通分量
        if r.pins_total == 0 {
            findings.push((
                "G10.conn".into(),
                Verdict::Skip("conn report 未注入（eval=0）".into()),
            ));
        } else if r.pins_unreachable == 0 {
            findings.push((
                "G10.conn".into(),
                Verdict::Ok(format!("{}/{} pins reachable", r.pins_total - r.pins_unreachable, r.pins_total)),
            ));
        } else {
            findings.push((
                "G10.conn".into(),
                Verdict::Fail(format!(
                    "{}/{} pins unreachable",
                    r.pins_unreachable, r.pins_total
                )),
            ));
        }

        // ── G11 电源契约 ─────────────────────────────────────────────
        findings.push(num_check(
            "G11.gnd_edges",
            g.gnd_edges,
            r.gnd_edges,
            r.evaluated.nets,
            "GND 边（R-1：无 driver 不画）".into(),
        ));
        findings.push(num_check(
            "G11.power_edges",
            g.power_edges,
            r.power_edges,
            r.evaluated.nets,
            "rail power 边（R-2：driver 段）".into(),
        ));
        if g.top_passives > 0 || r.layer == "main" {
            // 仅顶层判 C5（golden 里只有 main 层给了 top_passives 语义）
            findings.push(num_check(
                "G11.top_passives",
                g.top_passives,
                r.two_pin_passives,
                r.evaluated.boxes,
                "契约C5 框图不画无源件".into(),
            ));
        }

        // 边表（结构比对：每条期望边是否有一条实际网覆盖；实际多了哪些）
        if g.edge.is_empty() {
            findings.push((
                "G11.edges".into(),
                Verdict::Skip(format!("golden 无边表，实际边={}", r.edges.len())),
            ));
        } else {
            let exp: Vec<(String, String, String)> = g
                .edge
                .iter()
                .map(|e| (e.from.clone(), e.to.clone(), e.label.clone()))
                .collect();
            let act: Vec<(String, String, String)> =
                r.edges.iter().map(|e| edge_key(e)).collect();
            let (missing, extra) = multiset_diff_str3(&exp, &act);
            if missing.is_empty() && extra.is_empty() {
                findings.push((
                    "G11.edges".into(),
                    Verdict::Ok(format!("{} 条边全部 match", r.edges.len())),
                ));
            } else {
                findings.push((
                    "G11.edges".into(),
                    Verdict::Fail(format!(
                        "missing=[{}] extra=[{}]",
                        missing
                            .iter()
                            .map(|t| format!("{}~{}:{}", t.0, t.1, t.2))
                            .collect::<Vec<_>>()
                            .join(", "),
                        extra
                            .iter()
                            .map(|t| format!("{}~{}:{}", t.0, t.1, t.2))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                ));
            }
        }

        // ── G12 几何合法性 ───────────────────────────────────────────
        findings.push(num_check(
            "G12.box_box",
            0,
            r.box_box,
            r.evaluated.boxes,
            "盒子碰撞".into(),
        ));
        findings.push(num_check(
            "G12.wire_box",
            0,
            r.wire_box,
            r.evaluated.nets,
            "线穿盒".into(),
        ));
        findings.push(num_check(
            "G12.s6_size",
            0,
            r.s6_violations,
            r.evaluated.sizes,
            "盒子装不下 pin（S6）".into(),
        ));
        findings.push(num_check(
            "G12.offcanvas",
            0,
            r.offcanvas_boxes,
            r.evaluated.boxes,
            "负坐标盒子".into(),
        ));

        let red = findings.iter().filter(|f| matches!(f.1, Verdict::Fail(_))).count();
        let green = findings.iter().filter(|f| matches!(f.1, Verdict::Ok(_))).count();
        let skipped = findings.iter().filter(|f| matches!(f.1, Verdict::Skip(_))).count();
        LayerDiff {
            layer: r.layer.clone(),
            findings,
            red,
            green,
            skipped,
        }
    }
}

fn num_check(id: &str, expect: usize, actual: usize, evaluated: usize, note: String) -> (String, Verdict) {
    if evaluated == 0 {
        return (
            id.to_string(),
            Verdict::Skip(format!("eval=0（{note}）")),
        );
    }
    if expect == actual {
        (
            id.to_string(),
            Verdict::Ok(format!("{actual} == golden {expect}（{note}）")),
        )
    } else {
        (
            id.to_string(),
            Verdict::Fail(format!("{actual} != golden {expect}（{note}）")),
        )
    }
}

fn fmt_list(v: &[String]) -> String {
    if v.is_empty() {
        "[]".to_string()
    } else if v.len() <= 8 {
        format!("[{}]", v.join(","))
    } else {
        format!("[{} …+{}]", v[..8].join(","), v.len() - 8)
    }
}

fn multiset_diff_str3(
    exp: &[(String, String, String)],
    act: &[(String, String, String)],
) -> (Vec<(String, String, String)>, Vec<(String, String, String)>) {
    use std::collections::BTreeMap;
    let key = |t: &(String, String, String)| format!("{}\u{1}{}\u{1}{}", t.0, t.1, t.2);
    let mut e: BTreeMap<String, i32> = BTreeMap::new();
    let mut a: BTreeMap<String, i32> = BTreeMap::new();
    for t in exp {
        *e.entry(key(t)).or_insert(0) += 1;
    }
    for t in act {
        *a.entry(key(t)).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, n) in &e {
        let m = a.get(k).copied().unwrap_or(0);
        if m < *n {
            let parts: Vec<&str> = k.split('\u{1}').collect();
            for _ in 0..(n - m) {
                missing.push((parts[0].into(), parts[1].into(), parts[2].into()));
            }
        }
    }
    for (k, m) in &a {
        let n = e.get(k).copied().unwrap_or(0);
        if m > &n {
            let parts: Vec<&str> = k.split('\u{1}').collect();
            for _ in 0..(m - n) {
                extra.push((parts[0].into(), parts[1].into(), parts[2].into()));
            }
        }
    }
    (missing, extra)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiset_diff_counts_multiplicity() {
        let (m, e) = multiset_diff(
            &["a".to_string(), "b".to_string()],
            &["a".to_string(), "a".to_string(), "c".to_string()],
        );
        assert_eq!(m, vec!["b".to_string()]);
        assert_eq!(e, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn skip_when_eval_zero() {
        let (_, v) = num_check("G12.box_box", 0, 0, 0, "t".into());
        assert!(matches!(v, Verdict::Skip(_)), "eval=0 必须 SKIP 不能绿");
    }

    #[test]
    fn golden_toml_parses() {
        // 与 baseline/render_golden.toml 同构的最小样例
        let text = r#"
[layer.main]
module = "main"
boxes = 10
synth_boxes = 0
rail_flags = 0
gnd_edges = 0
power_edges = 4
top_passives = 0
box_names = ["a", "b"]

[[layer.main.edge]]
from = "a"
to = "b"
label = "USB_5V"
"#;
        let g: RenderGolden = toml::from_str(text).unwrap();
        assert_eq!(g.layer["main"].boxes, 10);
        assert_eq!(g.layer["main"].edge.len(), 1);
    }
}
