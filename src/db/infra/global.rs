// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::{
    semantic::{
        component::McComponent, mc_define::McDefineDef, mc_enum::McEnumDef, mc_ifs::McInterface,
        module::McModule,
    },
    McSpaceName,
};
use dashmap::DashMap;
use std::sync::LazyLock;
use std::{path::PathBuf, sync::Arc, sync::Mutex};

pub(crate) static mcc_system_root: LazyLock<Mutex<PathBuf>> =
    LazyLock::new(|| Mutex::new(PathBuf::new()));
pub(crate) static mcc_project_root: LazyLock<Mutex<PathBuf>> =
    LazyLock::new(|| Mutex::new(PathBuf::new()));
pub static mcc_components: LazyLock<DashMap<McSpaceName, Arc<McComponent>>> =
    LazyLock::new(DashMap::new);
pub static mcc_modules: LazyLock<DashMap<McSpaceName, Arc<McModule>>> =
    LazyLock::new(DashMap::new);
pub static mcc_interfaces: LazyLock<DashMap<McSpaceName, Arc<McInterface>>> =
    LazyLock::new(DashMap::new);
pub static mcc_enums: LazyLock<DashMap<McSpaceName, Arc<McEnumDef>>> =
    LazyLock::new(DashMap::new);
pub static mcc_defines: LazyLock<DashMap<McSpaceName, Arc<McDefineDef>>> =
    LazyLock::new(DashMap::new);
pub(crate) static mcc_parsing_modules: LazyLock<DashMap<String, ()>> =
    LazyLock::new(DashMap::new);
