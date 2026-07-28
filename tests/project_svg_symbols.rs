use std::path::PathBuf;

use mcc::{McIds, McURI};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/svg_symbol_project")
}

#[test]
fn project_manifest_symbol_reaches_rendered_svg() {
    let project_root = fixture_root();
    let entry_path = project_root.join("src/main.mc");
    let entry_uri: McURI = entry_path.to_string_lossy().into_owned();

    mcc::mcc_init_no_lib();
    mcc::mcc_set_system_root(std::path::Path::new(""));
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_clear_workspace();
    mcc::mcc_load_project(&entry_uri);

    let (instance, table) = mcc::mcc_build_flat(&McIds::from("main"), &entry_uri, 1000)
        .expect("build SVG symbol fixture");
    let block = mcc::build_mc_vec(&instance, &table);
    let graph = mcc::build_mc_vec_graph(&block, &table);
    let document = mcc::viz::api::render(graph);

    assert!(
        document.validate().is_empty(),
        "invalid visualization document"
    );
    let svg = &document.root_layer().expect("root visualization layer").svg;
    assert_eq!(svg.matches("class=\"comp custom\"").count(), 2);
    assert!(svg.contains("data-symbol-source=\"symbols/usb-mini-b.svg\""));
    assert!(svg.contains("viewBox=\"0.000 0.000 120.000 90.000\""));
    assert!(svg.contains("preserveAspectRatio=\"xMidYMid meet\""));
    assert!(svg.contains("overflow=\"hidden\""));
    assert!(!svg.contains("<script"));
    assert!(!svg.contains("href="));
}
