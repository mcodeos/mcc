// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-2 · projection-layer acceptance test
//!
//! ## Acceptance items (MC_SCHEMATIC_ROADMAP_v6 P7-2)
//! - Three noise classes (scalar stubs / duplicate endpoints / label pseudo-points)
//!   go to zero on the viz side; the pass2 side is untouched (guarded
//!   independently by tests/netdiff.rs, discipline 10).
//! - main layer net count 19 → **14 = golden**; GND four nets merged into one,
//!   V3V3 three nets merged into one; endpoint sets match
//!   tests/golden/hbl/main.golden.toml point by point.
//! - Projection is auditable: each record in baseline/render_projection.md
//!   has (rule, layer, net, endpoint).
//!
//! ## All criteria asserted by **path** (InstTable ids are unstable across processes; do not hard-code ids).

use std::collections::BTreeSet;
use std::path::PathBuf;

use mcc::vector::model::McVecBlock;
use mcc::{InstKind, InstTable, McIds, MemberRole};

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl")
}

/// The mcc_* workspace is global state; tests must be serialized (same as tests/renderdiff.rs)
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

/// Turn an absolute entry path into a layer-relative endpoint name ("main.mcu513.GND" → "mcu513.GND").
fn rel<'a>(path: &'a str, layer_prefix: &str) -> &'a str {
    path.strip_prefix(layer_prefix).unwrap_or(path)
}

/// All nets in a layer: name → (relative endpoint set).
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
                    table
                        .get_entry(id as u32)
                        .map(|e| rel(&e.path, prefix).to_string())
                })
                .collect();
            (n.name.clone(), eps)
        })
        .collect()
}

/// ★ P7-8: after projection, only **rail** pseudo endpoints (Ground/Power role)
/// must be absent from real. Non-rail pseudo endpoints (Signal) are intentionally
/// kept — they become PortTerminal connections in fromblock.rs.
fn assert_no_rail_pseudo(block: &McVecBlock, table: &InstTable) {
    for net in &block.nets {
        for pid in net.all_point_ids() {
            if pid < 0 {
                continue;
            }
            if let Some(e) = table.get_entry(pid as u32) {
                if e.parent_id == Some(block.bid as u32)
                    && matches!(e.kind, InstKind::Port | InstKind::Label)
                {
                    let is_rail = e.member_info.as_ref().map_or(false, |m| {
                        matches!(m.role, MemberRole::Ground | MemberRole::Power)
                    });
                    assert!(
                        !is_rail,
                        "layer '{}' net '{}' still has rail pseudo-endpoint '{}' (rule c not fully cleaned)",
                        block.name,
                        net.name,
                        e.path
                    );
                }
            }
        }
    }
    for sub in &block.blocks {
        assert_no_rail_pseudo(sub, table);
    }
}

#[test]
fn projection_main_layer_matches_pass2_golden() {
    let (raw, table) = build_hbl_block();
    let (projected, _log) = mcc::viz::project::project_block_tree(&raw, &table);

    // ── Net counts: match golden (PASS2 §1.8) layer by layer; mcu513's 25 = golden 21 + 4 spi lanes
    //   (spi.8/9/10/11 are bus lanes expanded by pass2, the subject of P7-5 S9, not projection noise) ──
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
        ("mcu513", 25), // golden 21 + 4 spi lanes (known difference, not noise)
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
        let (before, after) =
            got.unwrap_or_else(|| panic!("layer {name} is not in the projection result"));
        assert_eq!(
            after, want,
            "layer {name}: {after} nets after projection, want {want} (before projection {before})"
        );
    }

    // ── main layer GND: 4 nets merged into one (rule a), endpoints ⊆ golden main.GND's 9 points ──
    //
    // ★ Known builder non-determinism (input to P7-4/G14, not a projection defect):
    //   visit.rs's GND label net randomly adsorbs one cluster per run —— {flash.4, _C1.2} (Pin cluster)
    //   or {mic.dc.GND} (Label cluster); the other cluster is absent from that run's block.nets.
    //   The two shapes project to 8 / 7 points respectively; their union equals golden's 9 points
    //   (under the Pin-cluster shape, mic.dc.GND is re-added by promote at the graph layer).
    //   The projection layer is only responsible for: no pseudo-points, no duplicate endpoints,
    //   all six modules' GND present, no more than golden.
    let main = &projected; // the root is main
    let main_nets = nets_of(main, &table, "main.");
    let gnd = main_nets.iter().find(|(n, _)| n == "GND").expect("GND net");
    let golden_gnd: BTreeSet<String> = [
        "usbsocket.vin.GND",
        "modldo.vin.GND",
        "modldo.vout.GND",
        "moddcdc.GND",
        "mcu513.GND",
        "mic.dc.GND",
        "speaker.USB_VBUS_1.GND",
        "flash.4",
        "_C1.2",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for must in [
        "mcu513.GND",
        "modldo.vin.GND",
        "modldo.vout.GND",
        "moddcdc.GND",
        "speaker.USB_VBUS_1.GND",
        "usbsocket.vin.GND",
    ] {
        assert!(
            gnd.1.contains(must),
            "GND should contain {must}: {:?}",
            gnd.1
        );
    }
    assert!(
        gnd.1.iter().all(|e| golden_gnd.contains(e)),
        "GND endpoints must not exceed golden's 9 points: {:?}",
        gnd.1
    );
    assert!(
        (7..=9).contains(&gnd.1.len()),
        "GND endpoints should be 7~9 (builder dual shapes), got {}",
        gnd.1.len()
    );

    // ── V3V3.VCC: 3 nets merged into one (member net + VCC label view + VDD_3V3 label view) ──
    let v33 = main_nets
        .iter()
        .find(|(n, _)| n == "V3V3.VCC")
        .expect("V3V3.VCC");
    assert!(
        v33.1.contains("mic.dc.VDD_3V3"),
        "mic's VDD_3V3 should be merged into V3V3.VCC: {:?}",
        v33.1
    );
    assert!(v33.1.contains("flash.8"));
    assert!(v33.1.contains("modldo.vout.VCC"));
    assert!(v33.1.contains("moddcdc.VDD_3V3"));
    assert!(v33.1.contains("mcu513.VDD_3V3"));
    assert!(v33.1.contains("speaker.USB_VBUS_1.VDD_3V"));
    // Rule b: mic.VDD_3V3 (Label) is normalized to mic.dc.VDD_3V3 (Port declaration side)
    assert!(
        !v33.1.contains("mic.VDD_3V3"),
        "rule b should remove duplicate Label endpoint mic.VDD_3V3"
    );

    // ── The other rails match golden point by point ──
    let v12 = main_nets
        .iter()
        .find(|(n, _)| n == "V1V2.VCC")
        .expect("V1V2.VCC");
    assert_eq!(v12.1.len(), 2);
    let v5 = main_nets
        .iter()
        .find(|(n, _)| n == "V5V.VCC")
        .expect("V5V.VCC");
    assert_eq!(
        v5.1,
        ["modldo.vin.VCC", "usbsocket.vin.POWER_SYS"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>()
    );

    // ── mic layer: MIC.N stub ∪ MIC.N~0 member net (rule a specimen) ──
    let mic = main
        .blocks
        .iter()
        .find(|b| b.name == "mic")
        .expect("mic block");
    let mic_nets = nets_of(mic, &table, "main.mic.");
    assert_eq!(
        mic_nets.len(),
        4,
        "mic should have 4 nets (golden), got {:?}",
        mic_nets
    );
    let micn = mic_nets
        .iter()
        .find(|(n, _)| n == "MIC.N~0")
        .expect("MIC.N~0");
    assert!(micn.1.contains("mic.2") && micn.1.contains("C1.2") && micn.1.contains("dio2.1"));

    // ── Rule c: zero pseudo-endpoints across the whole tree ──
    assert_no_rail_pseudo(&projected, &table);
}

#[test]
fn projection_audit_md_is_written() {
    let (raw, table) = build_hbl_block();
    let _ = mcc::viz::project::project_block_tree(&raw, &table);

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline/render_projection.md");
    let md = std::fs::read_to_string(&path)
        .expect("baseline/render_projection.md should have been written");
    // Every record points to (rule, layer, net, endpoint) —— spot-check three key records
    assert!(
        md.contains("| a | main | GND"),
        "should have the GND four-nets-merged record:\n{md}"
    );
    assert!(
        md.contains("union 4 nets: GND + V1V2.GND + V3V3.GND + V5V.GND"),
        "the merge record should list all merged net names:\n{md}"
    );
    assert!(
        md.contains("| b | main | V3V3.VCC | main.mic.VDD_3V3"),
        "should have the rule b dedup record:\n{md}"
    );
    assert!(
        md.contains("| c | main | V5V.VCC | main.V5V.VCC"),
        "should have the rule c pseudo-endpoint record:\n{md}"
    );
    assert!(
        md.contains("| mic | 5 | 4 |"),
        "the layer summary table should contain mic 5→4:\n{md}"
    );
}
