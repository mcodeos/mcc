// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U245 pilot on the hbl corpus: `bom.overlay.toml` at the project root binds
//! the abstract `LDO.SOT23_5` slot to the `LDO.SGM2019_33YN5G_TR` variant.
//! The flat row rides the variant's identity (class name, partno) while the
//! pins stay the base's, both overlay checks stay silent, and the row is
//! selected (no W6005).

use std::path::PathBuf;

use crate::common;
use mcc::McIds;

fn hbl_project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hbl")
}

#[test]
fn bomovl__hbl_fixture_overlay_binds_the_ldo_slot() {
    let _guard = common::lock();
    let project_root = hbl_project_dir();
    let entry_uri = project_root
        .join("src/hbl.mc")
        .to_string_lossy()
        .into_owned();

    mcc::mcc_init();
    mcc::mcc_set_project_root(&project_root);
    mcc::mcc_load_project(&entry_uri);

    let (_, table, _arena, _store) =
        mcc::mcc_build_flat_with_arena(&McIds::from("main"), &entry_uri, 1000).expect("build hbl");

    let id = table
        .get_id_by_path("main.LDO.ldo")
        .expect("the LDO slot row exists");
    let entry = table.get_entry(id).unwrap();
    assert_eq!(
        entry.class_name, "LDO.SGM2019_33YN5G_TR",
        "the bound row rides the variant's identity"
    );
    assert!(!entry.unselected, "a bound slot is selected");

    for d in mcc::mcc_diagnose_all() {
        assert_ne!(
            d.code,
            mcc::errcodes::BOM_OVERLAY_VALUE_NOT_DESCENDANT,
            "the pilot bind must be legal: {}",
            d.msg
        );
        assert_ne!(
            d.code,
            mcc::errcodes::BOM_OVERLAY_KEY_NOT_SLOT,
            "the pilot key must hit a real slot: {}",
            d.msg
        );
    }

    // Hand the workspace back with no project root, so no later test in this
    // binary inherits the fixture's overlay.
    mcc::mcc_set_project_root(std::path::Path::new(""));
}
