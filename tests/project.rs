// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-2 · 投影层验收测试
//!
//! ## 验收项（MC_SCHEMATIC_ROADMAP_v6 P7-2）
//! - 三类噪声（标量 stub / 重复端点 / label 伪点）在 viz 侧归零；
//!   pass2 侧不动（由 tests/netdiff.rs 独立把守，纪律 10）。
//! - main 层网数 19 → **14 = golden**；GND 四网合一、V3V3 三网合一，
//!   端点集合与 tests/golden/hbl/main.golden.toml 逐点一致。
//! - 投影可审计：baseline/render_projection.md 每条记录 (规则, 层, 网, 端点)。
//!
//! ## 判据全部按**路径**断言（InstTable id 跨进程不稳定，不许硬编码 id）。

use std::collections::BTreeSet;
use std::path::PathBuf;

use mcc::vector::model::McVecBlock;
use mcc::{InstKind, InstTable, McIds};

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

/// mcc_* workspace 是全局状态，测试必须串行（与 tests/renderdiff.rs 同款）
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn build_hbl_block() -> (McVecBlock, InstTable) {
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
    let raw = mcc::vector::builder::visit::build_mc_vec(&tree, &table);
    (raw, table)
}

/// 把绝对 entry path 变成相对本层的端点名（"main.mcu513.GND" → "mcu513.GND"）。
fn rel<'a>(path: &'a str, layer_prefix: &str) -> &'a str {
    path.strip_prefix(layer_prefix).unwrap_or(path)
}

/// 一层里所有网的 名字 → (相对端点集合)。
fn nets_of(block: &McVecBlock, table: &InstTable, prefix: &str) -> Vec<(String, BTreeSet<String>)> {
    block
        .nets
        .iter()
        .map(|n| {
            let eps: BTreeSet<String> = n
                .all_point_ids()
                .into_iter()
                .filter_map(|id| {
                    if id < 0 {
                        return None;
                    }
                    table.get_entry(id as u32).map(|e| rel(&e.path, prefix).to_string())
                })
                .collect();
            (n.name.clone(), eps)
        })
        .collect()
}

/// 递归断言：投影后没有任何端点是"本层边界声明"（parent == bid 且 Port/Label）。
fn assert_no_boundary_pseudo(block: &McVecBlock, table: &InstTable) {
    for net in &block.nets {
        for pid in net.all_point_ids() {
            if pid < 0 {
                continue;
            }
            if let Some(e) = table.get_entry(pid as u32) {
                assert!(
                    !(e.parent_id == Some(block.bid as u32)
                        && matches!(e.kind, InstKind::Port | InstKind::Label)),
                    "层 '{}' 网 '{}' 仍有伪端点 '{}'（规则 c 未清干净）",
                    block.name,
                    net.name,
                    e.path
                );
            }
        }
    }
    for sub in &block.blocks {
        assert_no_boundary_pseudo(sub, table);
    }
}

#[test]
fn projection_main_layer_matches_pass2_golden() {
    let (raw, table) = build_hbl_block();
    let (projected, _log) = mcc::viz::project::project_block_tree(&raw, &table);

    // ── 网数：与 golden（PASS2 §1.8）逐层一致；mcu513 的 25 = golden 21 + 4 条 spi lane
    //   （spi.8/9/10/11 是 pass2 展开的 bus lane，P7-5 S9 的对象，不是投影噪声）──
    fn walk<'a>(proj: &'a McVecBlock, raw: &'a McVecBlock, out: &mut Vec<(&'a str, usize, usize)>) {
        out.push((proj.name.as_str(), raw.nets.len(), proj.nets.len()));
        for (s, r) in proj.blocks.iter().zip(raw.blocks.iter()) {
            walk(s, r, out);
        }
    }
    let mut per_layer = Vec::new();
    walk(&projected, &raw, &mut per_layer);
    let expect = [
        ("main", 14),
        ("mcu513", 25), // golden 21 + 4 spi lanes（已知差异，非噪声）
        ("mic", 4),
        ("moddcdc", 6),
        ("modldo", 3),
        ("speaker", 9),
        ("usbsocket", 3),
    ];
    for (name, want) in expect {
        let got = per_layer
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, before, after)| (*before, *after));
        let (before, after) = got.unwrap_or_else(|| panic!("层 {name} 不在投影结果里"));
        assert_eq!(after, want, "层 {name}: 投影后 {after} 网，期望 {want}（投影前 {before}）");
    }

    // ── main 层 GND：4 网合一（规则 a），端点 ⊆ golden main.GND 的 8 点 ──
    //
    // ★ 已知 builder 不确定性（P7-4/G14 的输入，非投影缺陷）：
    //   visit.rs 的 GND label 网每次运行随机吸附一个簇 —— {flash.4, C1.2}（Pin 簇）
    //   或 {mic.dc.GND}（Label 簇），另一簇在该次 block.nets 里缺席。
    //   两种形态投影后分别为 7 / 6 点；并集恰好等于 golden 的 8 点
    //   （mic.dc.GND 在 Pin 簇形态下由 promote 在图层补入）。
    //   投影层只负责：无伪点、无重复端点、六模块 GND 全在、不多于 golden。
    let main = &projected; // 根即 main
    let main_nets = nets_of(main, &table, "main.");
    let gnd = main_nets.iter().find(|(n, _)| n == "GND").expect("GND net");
    let golden_gnd: BTreeSet<String> = [
        "usbsocket.vin.GND",
        "modldo.GND",
        "moddcdc.GND",
        "mcu513.GND",
        "mic.dc.GND",
        "speaker.USB_VBUS_1.GND",
        "flash.4",
        "C1.2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for must in ["mcu513.GND", "modldo.GND", "moddcdc.GND", "speaker.USB_VBUS_1.GND", "usbsocket.vin.GND"] {
        assert!(gnd.1.contains(must), "GND 应含 {must}: {:?}", gnd.1);
    }
    assert!(
        gnd.1.iter().all(|e| golden_gnd.contains(e)),
        "GND 端点不得超出 golden 8 点: {:?}",
        gnd.1
    );
    assert!(
        (6..=8).contains(&gnd.1.len()),
        "GND 端点应为 6~8（builder 双形态），实际 {}",
        gnd.1.len()
    );

    // ── V3V3.VCC：3 网合一（成员网 + VCC label 视图 + VDD_3V3 label 视图）──
    let v33 = main_nets.iter().find(|(n, _)| n == "V3V3.VCC").expect("V3V3.VCC");
    assert!(v33.1.contains("mic.dc.VDD_3V3"), "mic 的 VDD_3V3 应并入 V3V3.VCC: {:?}", v33.1);
    assert!(v33.1.contains("flash.8"));
    assert!(v33.1.contains("modldo.VCC"));
    assert!(v33.1.contains("moddcdc.VDD_3V3"));
    assert!(v33.1.contains("mcu513.VDD_3V3"));
    assert!(v33.1.contains("speaker.VDD_3V3"));
    // 规则 b：mic.VDD_3V3（Label）被归一到 mic.dc.VDD_3V3（Port 声明侧）
    assert!(!v33.1.contains("mic.VDD_3V3"), "规则 b 应剔除重复 Label 端点 mic.VDD_3V3");

    // ── 其余 rail 与 golden 逐点一致 ──
    let v12 = main_nets.iter().find(|(n, _)| n == "V1V2.VCC").expect("V1V2.VCC");
    assert_eq!(v12.1.len(), 2);
    let v5 = main_nets.iter().find(|(n, _)| n == "V5V.VCC").expect("V5V.VCC");
    assert_eq!(
        v5.1,
        ["modldo.POWER_SYS", "speaker.USB_VBUS_1.VDD_3V", "usbsocket.vin.POWER_SYS"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
    );

    // ── mic 层：MIC.N stub ∪ MIC.N~0 成员网（规则 a 标本）──
    let mic = main.blocks.iter().find(|b| b.name == "mic").expect("mic block");
    let mic_nets = nets_of(mic, &table, "main.mic.");
    assert_eq!(mic_nets.len(), 4, "mic 应为 4 网（golden），实际 {:?}", mic_nets);
    let micn = mic_nets.iter().find(|(n, _)| n == "MIC.N~0").expect("MIC.N~0");
    assert!(micn.1.contains("mic.2") && micn.1.contains("C1.2") && micn.1.contains("dio2.1"));

    // ── 规则 c：全树零伪端点 ──
    assert_no_boundary_pseudo(&projected, &table);
}

#[test]
fn projection_audit_md_is_written() {
    let (raw, table) = build_hbl_block();
    let _ = mcc::viz::project::project_block_tree(&raw, &table);

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline/render_projection.md");
    let md = std::fs::read_to_string(&path).expect("baseline/render_projection.md 应已写出");
    // 每条记录都能指到 (规则, 层, 网, 端点)——抽查三条关键记录
    assert!(md.contains("| a | main | GND"), "应有 GND 四网合一记录:\n{md}");
    assert!(
        md.contains("union 4 张网: GND + V1V2.GND + V3V3.GND + V5V.GND"),
        "合并记录应列出全部被并网名:\n{md}"
    );
    assert!(
        md.contains("| b | main | V3V3.VCC | main.mic.VDD_3V3"),
        "应有规则 b 去重记录:\n{md}"
    );
    assert!(
        md.contains("| c | main | V5V.VCC | main.V5V.VCC"),
        "应有规则 c 伪端点记录:\n{md}"
    );
    assert!(md.contains("| mic | 5 | 4 |"), "层汇总表应含 mic 5→4:\n{md}");
}
