// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-1 · renderdiff 集成测试 —— 渲染层的尺子
//!
//! ## 与 netdiff 的分工（纪律 10）
//! - `netdiff`：网表 golden（连得对不对）
//! - `renderdiff`：渲染 golden（画得对不对）—— `baseline/render_golden.toml`
//!
//! ## P7-1 阶段的断言形状（v6 §4）
//! **中途大面积红是正确形状**：
//! - main 层 extra（rail flag + 顶层无源件）必须显著 > 0（当前 ≈ 27）
//! - GND 边 / power 边不为 0（rail 三分法是 P7-3）
//! - 7 层全部有读数（尺子在量东西）
//! - 全绿反而是尺子坏了 —— 回头看纪律 9
//!
//! P7-3 之后这些断言会翻转（红→绿），届时再改断言而不是改 golden。

use std::path::PathBuf;

use mcc::viz::api::{render_with_metrics, RenderOpts};
use mcc::viz::metrics::renderdiff::{RenderGolden, Verdict};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline/render_golden.toml")
}

/// mcc_* workspace 是全局状态，渲染必须串行（并行跑会互相踩 → SIGABRT）
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// build hbl → render 全树 → 返回逐层 renderdiff 报告字符串
fn render_once(golden: &RenderGolden) -> (Vec<String>, usize, usize, usize, Vec<(String, usize, usize, usize)>) {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    let mcode_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode");
    mcc::mcc_set_system_root(mcode_dir.as_path());
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    mcc::mcb_load_lib("mcode", mcode_dir.as_path());
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");

    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    let graph = mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);

    let (_doc, metrics) = render_with_metrics(graph, RenderOpts::default());

    let mut lines = Vec::new();
    let (mut red, mut green, mut skip) = (0, 0, 0);
    let mut per_layer = Vec::new();
    for r in &metrics.renderdiff_layers {
        let d = golden.diff_layer(r);
        per_layer.push((r.layer.clone(), d.red, d.green, d.skipped));
        red += d.red;
        green += d.green;
        skip += d.skipped;
        lines.push(d.report_line());
    }
    (lines, red, green, skip, per_layer)
}

#[test]
fn renderdiff_measures_all_seven_layers() {
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, _red, _green, _skip, per_layer) = render_once(&golden);

    // 7 层全部有读数
    assert_eq!(per_layer.len(), 7, "必须量到 7 层：main + 6 子层，实际 {:?}", per_layer);
    for line in &lines {
        println!("{line}");
    }
}

#[test]
fn renderdiff_main_layer_rail_contract_is_green_after_p73() {
    // ★ P7-3 验收：rail 三分法落地后，main 层的电源契约（§1.2 七行核对表）全绿。
    // P7-1 时代此测试断言"大面积红"（m_red >= 6）——P7-3 后按 renderdiff.rs 头部
    // 预告翻转断言（改断言，不改 golden）。
    //
    // 剩余的红各有明确归属（不是 rail 契约）：
    //   G10.boxes/names —— 悬空端口端子框（契约 C4，P7-5 域）
    //   G11.edges      —— 信号网显示名还是 __net_N（网名投影，P7-5 域）
    //   G12.s6_size    —— 盒子装 pin（S6）
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, _red, _green, _skip, per_layer) = render_once(&golden);

    let main = per_layer.iter().find(|(l, ..)| l == "main").expect("main layer");
    let (_, m_red, _m_green, _m_skip) = main;
    assert!(
        *m_red >= 2 && *m_red <= 6,
        "main 层剩余红应为结构性红（boxes/names/edges/s6），实际 {m_red}"
    );

    // ── G11 rail 契约（§1.2 七行核对表逐条）──
    let main_reading = metrics_main_reading(&golden);
    assert_eq!(main_reading.gnd_edges, 0, "R-1：GND 边 = 0");
    assert_eq!(main_reading.power_edges, 4, "R-2：driver 段 = 4");
    assert_eq!(main_reading.two_pin_passives, 0, "C5：顶层不画无源件");
    assert_eq!(main_reading.rail_flag_boxes, 0, "纪律 11：端子不是盒子");
    assert_eq!(main_reading.synth_endpoint_boxes, 0, "合成盒子 = 0");

    // 4 条 driver 边的 (from, to, label) 与 golden 边表逐条一致
    let mut power_edges: Vec<(String, String, String)> = main_reading
        .edges
        .iter()
        .filter(|(_, _, l)| l.contains("V") && l.ends_with(".VCC") || *l == "V5V.VCC")
        .cloned()
        .collect();
    power_edges.sort();
    let mut want: Vec<(String, String, String)> = vec![
        ("modldo".into(), "moddcdc".into(), "V3V3.VCC".into()),
        ("modldo".into(), "mcu513".into(), "V3V3.VCC".into()),
        ("moddcdc".into(), "mcu513".into(), "V1V2.VCC".into()),
        ("usbsocket".into(), "modldo".into(), "V5V.VCC".into()),
    ];
    want.sort();
    assert_eq!(power_edges, want, "driver 段边表应逐条等于 golden");

    // ── 子层：S1/S2 语义 = 无跨盒 rail 边（全部就地符号）──
    for r in sub_readings(&golden) {
        assert_eq!(r.gnd_edges, 0, "子层 {} GND 边应为 0", r.layer);
        assert_eq!(r.power_edges, 0, "子层 {} power 边应为 0", r.layer);
    }

    // 尺子仍在量东西（纪律 9）：全树 7 层都有读数
    assert_eq!(per_layer.len(), 7);
    for line in &lines {
        println!("{line}");
    }
}

/// 拿 main 层的完整 LayerReading（比 per_layer 的计数三元组多了 gnd/power/passives 字段）。
fn metrics_main_reading(golden: &RenderGolden) -> mcc::viz::metrics::renderdiff::LayerReading {
    readings(golden).into_iter().find(|r| r.layer == "main").expect("main reading")
}

fn sub_readings(golden: &RenderGolden) -> Vec<mcc::viz::metrics::renderdiff::LayerReading> {
    readings(golden)
        .into_iter()
        .filter(|r| r.layer != "main")
        .collect()
}

fn readings(_golden: &RenderGolden) -> Vec<mcc::viz::metrics::renderdiff::LayerReading> {
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let project_root = hbl_project_dir();
    let entry_path = project_root.join("src/hbl.mc");
    let entry_uri: String = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    let mcode_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mcode");
    mcc::mcc_set_system_root(mcode_dir.as_path());
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    mcc::mcb_load_lib("mcode", mcode_dir.as_path());
    mcc::mcc_load_project(&entry_uri);

    let (tree, table) =
        mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");
    let vec_block = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    let graph = mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table);
    let (_doc, metrics) = render_with_metrics(graph, RenderOpts::default());
    metrics.renderdiff_layers
}

#[test]
// ★ P7-4 解锁（原 ignore 理由：19 个几何写者 last-writer-wins 导致布局不确定，
// main flags 23↔21 / wire_box 11↔12，mcu513 box_box 16↔11）。
// P7-4d 修复 4 处 HashMap 迭代序病灶（group.rs 配对发射序 / mc_net into_nets
// 分组序 / visit by_root 迭代 / connection.rs 链起点+丢点）后，20 次渲染
// 逐字节一致已实测通过（384s）。保留为常驻契约：布局确定性回归即红。
fn renderdiff_report_is_deterministic() {
    // ★ P7-1 验收项：连续 20 次渲染，报表逐字节一致（G14 的报表子集）
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let first: Vec<String> = render_once(&golden).0;
    assert_eq!(first.len(), 7);

    for i in 1..20 {
        let again = render_once(&golden).0;
        if again != first {
            // ★ P7-4：失败时打印逐层首差异行，定位不确定的层与判据
            for (a, b) in first.iter().zip(again.iter()) {
                if a != b {
                    panic!(
                        "第 {} 次渲染与首次不一致 —— 布局不确定\n  首次: {:?}\n  本次: {:?}",
                        i + 1,
                        a,
                        b
                    );
                }
            }
            panic!("第 {} 次渲染与首次不一致（层数或层名不同）{:?} vs {:?}", i + 1, first, again);
        }
    }
}

#[test]
fn renderdiff_skip_is_visible_not_green() {
    // 纪律 9：eval=0 的判据显示 SKIP，不许显示 ✓（单元级保证，防回归）
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, _r, _g, _s, per_layer) = render_once(&golden);

    // 子层 golden 无名单 → G10.names 必须 SKIP
    let sub = per_layer
        .iter()
        .find(|(l, ..)| l == "modldo")
        .expect("modldo layer");
    let main_report = lines
        .iter()
        .zip(per_layer.iter())
        .find(|(_, (l, ..))| l == "modldo")
        .map(|(l, _)| l.clone())
        .unwrap();
    assert!(
        main_report.contains("· G10.names"),
        "子层无名单必须显示 · SKIP，报表：\n{}",
        main_report
    );
    assert_eq!(sub.0, "modldo");
}

#[test]
fn renderdiff_verdict_types_distinguishable() {
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let (lines, ..) = render_once(&golden);
    let joined = lines.join("\n");
    // main 层有 ✗（红）与 ✓（绿）与 ·（SKIP）三种符号共存 —— 尺子在量真东西
    assert!(joined.contains("✗"), "报表里必须有红");
    assert!(joined.contains("·"), "报表里必须有可见 SKIP");
    let _ = Verdict::Ok(String::new());
}

/// ★ P7-4e 验收契约：全树几何双写 = 0。
///
/// 基线（P7-4c 细粒度尺）343 处 → 维度所有权尺（Placement / PinFinal /
/// Route 三段）42 处真违规 → 删 feedback nudge（19）+ 虚线框豁免（1）+
/// PinFinal 归段（22）后归零。回归即红：任何越段写几何的改动在此暴露。
#[test]
fn renderdiff_geom_double_writes_baseline() {
    let golden = RenderGolden::load(&golden_path()).expect("golden parse");
    let readings = readings(&golden);

    let mut total = 0usize;
    for r in &readings {
        assert_eq!(
            r.geom_double_writes,
            r.geom_double_write_list.len(),
            "{} 层计数与明细长度应一致",
            r.layer
        );
        if r.geom_double_write_list.is_empty() {
            continue;
        }
        println!("[{}] {} 处双写:", r.layer, r.geom_double_write_list.len());
        for d in &r.geom_double_write_list {
            println!("  {d}");
        }
        total += r.geom_double_write_list.len();
    }
    assert_eq!(
        total, 0,
        "几何单一写者契约破坏：{total} 处跨段越权写入（清单见上方输出）"
    );
}
