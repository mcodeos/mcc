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
use lazy_static::lazy_static;
use std::{path::PathBuf, sync::Arc, sync::Mutex};

lazy_static! {
    pub(crate) static ref mcc_system_root: Mutex<PathBuf> = Mutex::new(PathBuf::new());
    pub(crate) static ref mcc_project_root: Mutex<PathBuf> = Mutex::new(PathBuf::new());
    pub static ref mcc_components: DashMap<McSpaceName, Arc<McComponent>> = DashMap::new();
    pub static ref mcc_modules: DashMap<McSpaceName, Arc<McModule>> = DashMap::new();
    pub static ref mcc_interfaces: DashMap<McSpaceName, Arc<McInterface>> = DashMap::new();
    pub static ref mcc_enums: DashMap<McSpaceName, Arc<McEnumDef>> = DashMap::new();
    pub static ref mcc_defines: DashMap<McSpaceName, Arc<McDefineDef>> = DashMap::new();
    pub(crate) static ref mcc_parsing_modules: DashMap<String, ()> = DashMap::new();
}
