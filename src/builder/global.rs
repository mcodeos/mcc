// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    builder::mc_code::McCode,
    builder::util::MultiThreadRefCell,
    core::{component::McComponent, mc_enum::McEnumDef, mc_ifs::McInterface, module::McModule},
    McSpaceName,
};
use dashmap::DashMap;
use lazy_static::lazy_static;
use std::{path::PathBuf, sync::Arc};

lazy_static! {
    pub(crate) static ref mcc_system_root: MultiThreadRefCell<PathBuf> =
        MultiThreadRefCell::new(PathBuf::new());
    pub(crate) static ref mcc_project_root: MultiThreadRefCell<PathBuf> =
        MultiThreadRefCell::new(PathBuf::new());
    pub(super) static ref mcc_blibs: MultiThreadRefCell<DashMap<String, McCode>> =
        MultiThreadRefCell::new(DashMap::new());
    pub static ref mcc_components: MultiThreadRefCell<DashMap<McSpaceName, Arc<McComponent>>> =
        MultiThreadRefCell::new(DashMap::new());
    pub static ref mcc_modules: MultiThreadRefCell<DashMap<McSpaceName, Arc<McModule>>> =
        MultiThreadRefCell::new(DashMap::new());
    pub static ref mcc_interfaces: MultiThreadRefCell<DashMap<McSpaceName, Arc<McInterface>>> =
        MultiThreadRefCell::new(DashMap::new());
    pub static ref mcc_enums: MultiThreadRefCell<DashMap<McSpaceName, Arc<McEnumDef>>> =
        MultiThreadRefCell::new(DashMap::new());
    pub(crate) static ref mcc_parsing_modules: DashMap<String, ()> = DashMap::new();
}
