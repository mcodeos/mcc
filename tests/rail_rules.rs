// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-3 · Rail 三分法验收测试（MC_SCHEMATIC_ROADMAP_v6 P7-3 验收清单）
//!
//! - main 层：GND 边 = 0，rail flag 盒子 = 0，driver 段边 = 4，
//!   与 §1.2 七行核对表逐条一致（边表断言在 tests/renderdiff.rs）。
//! - main 层 `compute_isolated_ids` 返回空集（usbsocket/modldo/moddcdc 不再是孤岛）。
//! - 子层：每个 GND 端点恰好 1 个接地符号（S1），
//!   每个非 GND rail 端点恰好 1 个 rail 圆点（S2）。
//!
//! 判据按**盒子名/网名**断言（id 跨进程不稳定）。

use std::collections::HashSet;
use std::path::PathBuf;

use mcc::viz::api::{render_with_metrics, RenderOpts};
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

/// mcc_* workspace 是全局状态，测试必须串行（与 tests/renderdiff.rs 同款）
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_graph() -> mcc::vector::graph::McVecGraph {
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
    mcc::vector::graph::fromblock::build_mc_vec_graph(&vec_block, &table)
}

#[test]
fn main_layer_isolated_set_is_empty() {
    // 验收：driver 段边把 usbsocket/modldo/moddcdc 接进主流向，
    // compute_isolated_ids(main, hub) 必须返回空集。
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut graph = build_graph();
    mcc::vector::graph::apply_promote_recursive(&mut graph);
    // 镜像流水线：classify_rails 先于孤岛计算（flow.rs phase_prepare → phase_placement）
    mcc::viz::layout::rails::classify_rails(&mut graph, /*is_top=*/ true);
    // hub = 信号度最高的盒子（main 层 = mcu513）
    let mut degree: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for n in &graph.nets {
        let mut boxes: Vec<i64> = Vec::new();
        for e in &n.endpoints {
            if !boxes.contains(&e.box_id) {
                boxes.push(e.box_id);
            }
        }
        if boxes.len() >= 2 {
            for b in boxes {
                *degree.entry(b).or_insert(0) += 1;
            }
        }
    }
    let hub = *degree
        .iter()
        .max_by_key(|(id, d)| (**d, std::cmp::Reverse(**id)))
        .map(|(id, _)| id)
        .expect("hub");
    let hub_name = graph
        .boxes
        .iter()
        .find(|b| b.id == hub)
        .map(|b| b.name.clone())
        .unwrap_or_default();
    assert_eq!(hub_name, "mcu513", "main 层 hub 应为 mcu513（信号度最高）");

    let isolated: HashSet<i64> = mcc::viz::layout::flow::compute_isolated_ids(&graph, hub);
    let detail: Vec<String> = isolated
        .iter()
        .filter_map(|id| {
            graph.boxes.iter().find(|b| b.id == *id).map(|b| {
                let nets: Vec<String> = graph
                    .nets
                    .iter()
                    .filter(|n| n.box_ids().contains(&b.id))
                    .map(|n| format!("{}({:?})", n.name, n.kind))
                    .collect();
                format!("id={} name='{}' kind={:?} nets={:?}", b.id, b.name, b.kind, nets)
            })
        })
        .collect();
    assert!(
        isolated.is_empty(),
        "main 层孤岛集应为空（电源模块已被 driver 段接入主流向），实际 {} 条：{:?}",
        isolated.len(),
        detail
    );
}

#[test]
fn sub_layers_s1_s2_decoration_counts() {
    // 验收：S1 —— 每个 GND 端点恰好 1 个接地符号（装饰数 = 子层 Ground rail 端点数）
    //       S2 —— 每个 非 GND rail 端点恰好 1 个 rail 圆点
    // 期望值来自 golden 网表（PASS2 §1.8）各模块 GND / 电源网端点数。
    let _guard = RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let graph = build_graph();
    let (_doc, metrics) = render_with_metrics(graph, RenderOpts::default());

    let get = |layer: &str| {
        metrics
            .renderdiff_layers
            .iter()
            .find(|r| r.layer == layer)
            .unwrap_or_else(|| panic!("layer {layer} 不在报表里"))
    };

    // main：R-1/R-3 顶层不落符号
    let main = get("main");
    assert_eq!(main.decorations_ground, 0, "顶层 R-1 不落 GND 符号");
    assert_eq!(main.decorations_power, 0, "顶层 R-3 不落 rail 圆点");

    // 子层期望 = golden 网表 rail 网端点数 − P7-2 规则(c)删掉的伪端点
    // （每条 rail 网的 port.X / member.X 边界声明点各 1 个；推导见下）：
    //   mcu513   GND 9−1=8          power = VDD_3V3 7−1 + VCC_1V2 3−1 = 8
    //   mic      dc.GND 7−1=6       power = dc.VDD_3V3 3−1 = 2
    //   modldo   GND 4−1=3          power = POWER_SYS 4−1 + VCC 3−1 = 5
    //   moddcdc  GND 8−1=7          power = VDD_3V3 4−1 + VCC_1V2 5−1 = 7
    //   speaker  USB_VBUS_1.GND 8−1=7  power = VDD_3V 3−1 = 2
    //            （VDD_3V3 网只剩 R7.1 单端点，P7-2 audit 已裁决为 stub 删除）
    //   usbsocket vin.GND 7−1=6     power = vin.POWER_SYS 2−1 = 1
    let expect: &[(&str, usize, usize)] = &[
        ("mcu513", 8, 8),
        ("mic", 6, 2),
        ("modldo", 3, 5),
        ("moddcdc", 7, 7),
        ("speaker", 7, 2),
        ("usbsocket", 6, 1),
    ];
    for (layer, gnd, pwr) in expect {
        let r = get(layer);
        assert_eq!(
            r.decorations_ground, *gnd,
            "层 {layer} S1 接地符号数（golden GND 端点数）"
        );
        assert_eq!(
            r.decorations_power, *pwr,
            "层 {layer} S2 rail 圆点数（golden 非 GND rail 端点数）"
        );
        // S1/S2 的语义：这些端点不再有跨盒边
        assert_eq!(r.gnd_edges, 0, "层 {layer} 不应有 GND 跨盒边");
        assert_eq!(r.power_edges, 0, "层 {layer} 不应有 power 跨盒边");
    }
}
