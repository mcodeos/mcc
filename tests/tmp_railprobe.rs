// Copyright (c) 2026 MCode
//
// TEMP probe for P7-3 design — delete after.

use mcc::McIds;
use std::path::PathBuf;

#[test]
fn tmp_probe_rails() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/hbl");
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

    println!("== graph '{}' boxes ==", graph.name);
    for b in &graph.boxes {
        println!(
            "  box {} '{}' kind={:?} prov={:?}",
            b.id, b.name, b.kind, b.provenance
        );
    }
    println!("== graph '{}' nets ==", graph.name);
    for n in &graph.nets {
        let rail = match &n.rail {
            Some(r) => format!(
                " RAIL(class={:?}, driver={:?}, volt={:?})",
                r.class,
                r.driver_pin
                    .and_then(|p| table.get_entry(p as u32).map(|e| e.path.clone())),
                r.volt
            ),
            None => String::new(),
        };
        let eps: Vec<String> = n
            .endpoints
            .iter()
            .map(|e| {
                let extra = table
                    .get_entry(e.pin_id as u32)
                    .map(|en| format!("[io={:?}]", en.io_type))
                    .unwrap_or_default();
                format!("{}#{}{}", e.box_id, e.pin_name, extra)
            })
            .collect();
        println!("  net {} '{}' kind={:?}{} eps={:?}", n.nid, n.name, n.kind, rail, eps);
    }

    // Run the full render pipeline, to see final edges / decorations
    let (_doc, metrics) = mcc::viz::api::render_with_metrics(graph, mcc::viz::api::RenderOpts::default());
    for r in &metrics.renderdiff_layers {
        println!(
            "== FINAL layer '{}': boxes={} gnd_edges={} power_edges={}",
            r.layer, r.total_boxes, r.gnd_edges, r.power_edges
        );
        for (f, t, l) in &r.edges {
            println!("   edge {f} ~ {t} : {l}");
        }
    }
}
