// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

#![allow(dead_code)]

// Allow `mcc::*` references inside lib-root modules to resolve to self
// (the crate is itself named `mcc`; without this, `mcc::foo` would look for
// an external crate called `mcc`).
extern crate self as mcc;

//1. lib internal
use crate::builder::diagnostic::Diagnostic;
use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::debug;

//2. crate wise
pub(crate) mod ast;
pub(crate) mod builder;
pub(crate) mod cli;
pub(crate) mod core;
pub(crate) mod instant;
pub(crate) use builder::current_uri;
pub mod rpc;
pub mod vector;
pub mod viz;
//3. mcc re-exports
pub use crate::core::basic::mc_bus::McBus;
pub use crate::core::basic::mc_opd::McOpd;
pub use crate::core::common::{IOType, McCMIE, McSpaceName, McURI};
pub use crate::core::{
    basic::{
        mc_endpoint::{McEndpoint, McInstanceRef},
        mc_phrase::McPhrase,
    },
    component::{
        mc_pins::{McPinPort, McPins},
        Mc2Component, McComponent,
    },
    mc_define::McDefineDef,
    mc_enum::McEnumDef,
    mc_inst::{McInstance, McInstances},
    module::{Mc2Module, McModule},
};
pub mod error_codes;
pub mod export_api;
pub mod query_api;
pub mod search_api;
pub use ast::ast_semantic::{McSemSymbols, Span, SymbolType};
pub use ast::ast_token::{McSemToken, McSemTokens};
pub use ast::c_macros::*;
pub use ast::error::*;
pub use builder::{mcb_print, MccProjectTree};
pub use core::basic::mc_param::{McParamBindings, McParamDeclare, McParamDeclares, McParamValue};
pub use core::basic::mc_param_type::{McIoTy, McParamArity, McParamType, McParamTypeKind};
pub use core::basic::mc_paramd::{ParamDiagKind, ParamDiagnostic};
pub use instant::inst_table::InstKind;
pub use instant::inst_table::InstTable;
pub use instant::mc_comp::McComponentInst;
pub use instant::mc_mod::McModuleInst;
pub use instant::mc_net::NetPoint;

pub use builder::{
    mcb_get_first_module_name, mcb_get_module_name_by_uri, mcb_module_count, mcb_print_lines,
};

pub use builder::lib_mgr::LibInfo;
pub use builder::{mcb_lib_info, mcb_load_lib, mcb_loaded_libs, mcb_unload_lib};
pub use cli::config::{get_libs_load_list, should_load_mcode};

pub use builder::{
    mcb_add_recursive, mcb_get_refs, mcb_get_system_root, mcb_loaded_file_count,
    mcb_parse_all_modules, mcb_print_loaded_files,
};

pub use builder::diagnostic::{
    Diagnostic as McDiagnostic, DiagnosticLevel, Location as McLocation,
};
pub use builder::{mcb_component_count, mcb_interface_count};
pub use builder::{
    mcb_debug_get_cmie, mcb_get_module_def_by_name, mcb_get_module_with_diagnostics,
};
pub use builder::{
    mcb_iter_components, mcb_iter_enums, mcb_iter_interfaces, mcb_iter_modules, mcb_iter_ports,
};

// 🆕 New exports
pub use core::basic::mc_ida::McIda;
pub use core::basic::mc_ids::McIds;
pub use core::basic::mc_literal::{McConst, McFloat, McInt, McLiteral, McString};
pub use core::component::mc_attr::{McAttrVal, McAttribute};
pub use core::mc_func::{McFunction, McFunctions};

// 🆕 Step 8: McVec rendering side data structure exports
pub use vector::builder::build_mc_vec;
pub use vector::builder::build_mc_vec_with_report;
pub use vector::model::{ConnectionType, McVec, McVecBlock, McVecNet};

pub use vector::graph::{
    build_mc_vec_graph, BoxKind, EdgeType, IoSummary, McVecBox, McVecEdge, McVecGraph, Wire,
};
/// mcc struct ParserResult
#[derive(Debug)]
pub struct ParserResult {
    pub sem_tokens: Arc<Mutex<McSemTokens>>,
    pub sem_symbols: Arc<Mutex<McSemSymbols>>,
}
pub type McSemTokensArcCell = Arc<Mutex<McSemTokens>>;
pub type McSemSymbolsArcCell = Arc<Mutex<McSemSymbols>>;

// ============================================================================
// Smart Parameter Diagnostics (M6)
// ============================================================================

use std::sync::LazyLock;

/// Global store for smart parameter diagnostics produced during compilation.
static PARAM_DIAGNOSTICS: LazyLock<Mutex<Vec<ParamDiagnostic>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Record a parameter diagnostic (called by finalize() in mc_paramd).
pub fn mcc_record_param_diag(diag: &ParamDiagnostic) {
    if let Ok(mut diags) = PARAM_DIAGNOSTICS.lock() {
        diags.push(diag.clone());
    }
}

/// Retrieve and clear all parameter diagnostics.
pub fn mcc_flush_param_diags() -> Vec<ParamDiagnostic> {
    let mut diags = PARAM_DIAGNOSTICS.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut *diags)
}

/// Retrieve parameter diagnostics without clearing.
pub fn mcc_get_param_diags() -> Vec<ParamDiagnostic> {
    PARAM_DIAGNOSTICS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// Set system root path (parent directory of mcode library)
///
/// When to call:
/// - Call once at server startup
/// - Subsequent calls automatically skip if already set
///
/// Path resolution priority:
/// 1. Environment variable `MCC_SYSTEM_ROOT`
/// 2. `{path}/mc/` directory
/// 3. `~/.mcode/` directory
pub fn mcc_set_system_root(path: &Path) {
    use crate::cli::data_dir;

    let current = builder::mcb_get_system_root();
    if !current.as_os_str().is_empty() {
        debug!(target: "mcc::sysinit", system_root = ?current, "already set, skip");
        return;
    }

    // S3 fix: empty path means base uses cwd (absolute path), avoid relative path 'mc' being misused
    let base = if path.as_os_str().is_empty() {
        env::current_dir().unwrap_or_default()
    } else {
        path.to_path_buf()
    };
    let candidate_mc = base.join("mc");
    let candidate_mcode = base.join("mcode");

    let system_root = if let Ok(val) = env::var(data_dir::MCC_SYSTEM_ENV) {
        let p = PathBuf::from(&val);
        debug!(target: "mcc::sysinit", path = %val, "using MCC_SYSTEM_ROOT");
        if p.is_absolute() {
            p
        } else {
            env::current_dir().unwrap_or_default().join(p)
        }
    } else if candidate_mc.exists() || candidate_mcode.exists() {
        // ── S3 fix ──
        // Detection: if mc/ or mcode/ subdirectory exists under the project root,
        // use project root as system_root. lib_root = system_root.join(lib_name).
        debug!(target: "mcc::sysinit", path = ?base, "using project root (mc/ or mcode/ subdir found)");
        base
    } else {
        let default_path = data_dir::data_root();
        debug!(target: "mcc::sysinit", path = ?default_path, "using default");
        default_path
    };

    debug!(target: "mcc::sysinit", system_root = ?system_root);
    builder::mcb_set_system_root(&system_root);
}

/// Set project root path
///
/// When to call:
/// - Called when `mcc proj create`
/// - Automatically called when `mcc proj use` (workspace.switch_to)
/// - Standalone tools (like mcviz) need to call explicitly
pub fn mcc_set_project_root(path: &Path) {
    debug!(target: "mcc::sysinit", project_root = ?path);
    builder::mcb_set_project_root(path);
}

/// mcc interface
pub fn mcc_init() {
    builder::mcb_init();
    builder::mcb_init_system_lib();
}

/// mcc interface (don't load system library, optional at server startup)
pub fn mcc_init_no_lib() {
    builder::mcb_init();
}

/// mcc interface mcc_add
pub fn mcc_add(uri: &McURI) {
    builder::mcb_add(uri);
}

pub fn mcc_load_project(entry_uri: &McURI) {
    use std::collections::HashSet;
    builder::mc_code::mcb_reset_ast_visit_flag();
    let mut loaded = HashSet::new();
    builder::mcb_add_recursive(entry_uri, &mut loaded, false);
    builder::mcb_parse_all_modules();
}

/// Load .mc file from memory string (no disk file dependency)
/// uri is virtual path, content is .mc file content
pub fn mcc_load_from_string(uri: &McURI, content: &str) {
    builder::mc_code::mcb_reset_ast_visit_flag();
    builder::mcb_add_from_string(uri, content);
    builder::mcb_parse_all_modules();
}

/// mcc interface mcc_remove
pub fn mcc_remove(uri: &McURI) {
    builder::mcb_remove(uri);
}

/// mcc interface mcc_query
pub fn mcc_query(uri: &McURI) -> Option<ParserResult> {
    builder::mcb_query(uri)
}

/// mcc interface mcc_build
pub fn mcc_build(ident: &McIds, uri: &McURI) -> Result<MccProjectTree, Box<dyn Error>> {
    let canonical_uri = builder::mcb_canonicalize_uri(uri);
    builder::mcb_pass2(&McSpaceName::new(ident, canonical_uri))
}

/// mcc interface: build + flatten (Step 7)
///
/// Execute Pass2 instantiation and generate flattened instance table.
/// `start_id` specifies the starting ID value (typically 1000).
pub fn mcc_build_flat(
    ident: &McIds,
    uri: &McURI,
    start_id: u32,
) -> Result<(MccProjectTree, InstTable), Box<dyn Error>> {
    let canonical_uri = builder::mcb_canonicalize_uri(uri);
    let inst = builder::mcb_pass2(&McSpaceName::new(ident, canonical_uri))?;
    let table = InstTable::from_module_inst(&inst, start_id);
    Ok((inst, table))
}

pub fn mcc_diagnose(uri: &McURI) -> Vec<Diagnostic> {
    crate::builder::workspace::WORKSPACE
        .diagnostics
        .borrow()
        .get_diagnostics_for_file(uri)
        .into_iter()
        .cloned()
        .collect()
}

pub fn mcc_diagnose_all() -> Vec<Diagnostic> {
    crate::builder::workspace::WORKSPACE
        .diagnostics
        .borrow()
        .get_diagnostics()
        .to_vec()
}

/// Clear workspace state (for test isolation).
pub fn mcc_clear_workspace() {
    crate::builder::workspace::WORKSPACE.clear_active();
}

/// Read D5 BUS_BITS_MISMATCHED counter (for test assertions).
pub fn mcc_bus_bits_mismatched() -> usize {
    crate::instant::mc_mod::group::BUS_BITS_MISMATCHED.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn get_def(class_name: &McIds, uri: &McURI) -> Option<McCMIE> {
    builder::mcb_get_cmie(class_name, uri)
}

pub fn get_module_def(class_name: &McIds) -> Option<Arc<McModule>> {
    builder::mcb_get_module_def_by_name(class_name)
}

/// Get all module names in a specific file
pub fn mcc_get_modules_in_file(uri: &McURI) -> Vec<String> {
    builder::mcb_get_modules_in_file(uri)
}

pub fn debug_get_def(class_name: &McIds, uri: &McURI) {
    builder::mcb_debug_get_cmie(class_name, uri);
}

pub fn get_module_with_diagnostics(
    class_name: &McIds,
    uri: &McURI,
) -> (Option<Arc<McModule>>, Vec<String>) {
    builder::mcb_get_module_with_diagnostics(class_name, uri)
}

// ============================================================================
// 🆕 LineMemberInfo complete info extraction API (rendering side friendly format)
// ============================================================================

/// Extract structured info for all McPhrases in McModule
pub fn get_lines_info(module: &McModule) -> Vec<LineInfo> {
    module.lines.iter().map(phrase_to_info).collect()
}

/// Structured info for single McPhrase
#[derive(Debug, Clone)]
pub struct LineInfo {
    pub members: Vec<LineMemberInfo>,
}

/// Complete structured info for LineMemberInfo (corresponding to all variants)
#[derive(Debug, Clone)]
pub enum LineMemberInfo {
    Lead,
    Bus {
        elements: Vec<NodeElementInfo>,
    },
    Node {
        left: Vec<NodeElementInfo>,
        right: Vec<NodeElementInfo>,
    },
    Vector {
        lines: Vec<LineInfo>,
    },
    Parallel {
        lines: Vec<LineInfo>,
    },
    Transposed {
        inner: Box<LineInfo>,
    },
    FuncCall {
        caller: Option<Box<LineInfo>>,
        func_name: String,
        params: Vec<String>,
        left: Vec<NodeElementInfo>,
        right: Vec<NodeElementInfo>,
    },
    Closure {
        params: Vec<String>,
        right: Vec<NodeElementInfo>,
        body: Vec<LineInfo>,
    },
    Group {
        lines: Vec<LineInfo>,
        left_match: bool,
        right_match: bool,
    },
    Endpoint {
        info: String,
    },
}

/// McBus rendering side friendly info
#[derive(Debug, Clone)]
pub struct NodeElementInfo {
    pub name: String,
    pub iotype: String,
    pub members: Vec<String>, // flattened: member name string list
}

/// Convert McPhrase to LineInfo
fn phrase_to_info(phrase: &McPhrase) -> LineInfo {
    match phrase {
        McPhrase::Series(phrases) => {
            // Combine all phrases' members into one LineInfo
            let mut all_members = Vec::new();
            for p in phrases {
                let info = phrase_to_info(p);
                all_members.extend(info.members);
            }
            LineInfo {
                members: all_members,
            }
        }
        McPhrase::Parallel(phrases) => LineInfo {
            members: vec![LineMemberInfo::Parallel {
                lines: phrases.iter().map(phrase_to_info).collect(),
            }],
        },
        McPhrase::Closure(c) => LineInfo {
            members: vec![LineMemberInfo::Closure {
                params: c
                    .params
                    .iter()
                    .map(|d: &McParamDeclare| d.to_string())
                    .collect(),
                right: c.right.iter().map(node_element_to_info).collect(),
                body: c.body.iter().map(phrase_to_info).collect(),
            }],
        },
        McPhrase::Group(g) => LineInfo {
            members: vec![LineMemberInfo::Group {
                lines: g.opds.iter().map(phrase_to_info).collect(),
                left_match: g.left_match,
                right_match: g.right_match,
            }],
        },
        McPhrase::FuncCall(f) => LineInfo {
            members: vec![LineMemberInfo::FuncCall {
                caller: f.caller.as_ref().map(|c| Box::new(phrase_to_info(c))),
                func_name: f.func_name.to_string(),
                params: f
                    .params
                    .iter()
                    .map(|v: &McParamValue| v.to_string())
                    .collect(),
                left: f.left.iter().map(node_element_to_info).collect(),
                right: f.right.iter().map(node_element_to_info).collect(),
            }],
        },
        McPhrase::Transposed(line) => LineInfo {
            members: vec![LineMemberInfo::Transposed {
                inner: Box::new(phrase_to_info(line)),
            }],
        },
        McPhrase::Lead => LineInfo { members: vec![] },
        McPhrase::Multiple(phrases) => LineInfo {
            members: phrases
                .iter()
                .flat_map(|p| phrase_to_info(p).members)
                .collect(),
        },
        McPhrase::Endpoint(McEndpoint::Node {
            ref input,
            ref output,
            ..
        }) => LineInfo {
            members: vec![LineMemberInfo::Node {
                left: input
                    .iter()
                    .flat_map(|e| e.get_left())
                    .map(|b| node_element_to_info(&b))
                    .collect::<Vec<_>>(),
                right: output
                    .iter()
                    .flat_map(|e| e.get_right())
                    .map(|b| node_element_to_info(&b))
                    .collect::<Vec<_>>(),
            }],
        },
        McPhrase::Endpoint(ep) => LineInfo {
            members: vec![LineMemberInfo::Endpoint {
                info: ep.to_string(),
            }],
        },
        McPhrase::Member(phrase, ep) => {
            let mut members = phrase_to_info(phrase).members;
            members.push(LineMemberInfo::Endpoint {
                info: ep.to_string(),
            });
            LineInfo { members }
        }
    }
}

fn node_element_to_info(elem: &McBus) -> NodeElementInfo {
    NodeElementInfo {
        name: elem.name.clone(),
        iotype: "none".to_string(),
        // flattened: members is string list, convert to simple name list
        members: elem.member.to_vec(),
    }
}

// ============================================================================
// 🆕 Debug prints
// ============================================================================

/// Print all connection line info for module (for debugging)
pub fn print_module_lines(module: &McModule) {
    eprintln!(
        "=== Module '{}' Lines ({}) ===",
        module.name,
        module.lines.len()
    );
    for (i, line) in module.lines.iter().enumerate() {
        let info = phrase_to_info(line);
        eprintln!("  Line[{}]: {} members", i, info.members.len());
        for (j, member) in info.members.iter().enumerate() {
            print_member_info_indent(member, 4, j);
        }
    }
}

fn print_member_info_indent(member: &LineMemberInfo, indent: usize, idx: usize) {
    let pad = " ".repeat(indent);
    match member {
        LineMemberInfo::Lead => {
            eprintln!("{pad}[{idx}] Lead");
        }
        LineMemberInfo::Bus { elements } => {
            let names: Vec<&str> = elements.iter().map(|e| e.name.as_str()).collect();
            eprintln!("{pad}[{idx}] Bus({names:?})");
        }
        LineMemberInfo::Node { left, right } => {
            let l: Vec<&str> = left.iter().map(|e| e.name.as_str()).collect();
            let r: Vec<&str> = right.iter().map(|e| e.name.as_str()).collect();
            eprintln!("{pad}[{idx}] Node {{ left: {l:?}, right: {r:?} }}");
        }
        LineMemberInfo::Parallel { lines } => {
            eprintln!("{}[{}] Parallel({} phrases)", pad, idx, lines.len());
        }
        LineMemberInfo::Transposed { inner: _ } => {
            eprintln!("{pad}[{idx}] Transposed:");
        }
        LineMemberInfo::FuncCall {
            func_name, params, ..
        } => {
            eprintln!(
                "{}[{}] FuncCall {{ func: {}, params: {} }}",
                pad,
                idx,
                func_name,
                params.len()
            );
        }
        LineMemberInfo::Closure { params, body, .. } => {
            eprintln!(
                "{}[{}] Closure {{ params: {}, body: {} lines }}",
                pad,
                idx,
                params.len(),
                body.len()
            );
        }
        LineMemberInfo::Group { lines, .. } => {
            eprintln!("{}[{}] Group {{ {} lines }}", pad, idx, lines.len());
        }
        LineMemberInfo::Vector { lines } => {
            eprintln!("{}[{}] Vector({} lines)", pad, idx, lines.len());
        }
        LineMemberInfo::Endpoint { info } => {
            eprintln!("{pad}[{idx}] Endpoint({info})");
        }
    }
}

/// Print members of an McPhrase
fn print_phrase_members(phrase: &McPhrase, indent: usize) {
    match phrase {
        McPhrase::Series(phrases) => {
            for (j, p) in phrases.iter().enumerate() {
                print_phrase_members(p, indent);
                if j < phrases.len() - 1 {
                    eprintln!("{}    ---", " ".repeat(indent));
                }
            }
        }
        McPhrase::Parallel(phrases) => {
            for (k, p) in phrases.iter().enumerate() {
                print_phrase_members(p, indent);
                if k < phrases.len() - 1 {
                    eprintln!("{}    ---", " ".repeat(indent));
                }
            }
        }
        McPhrase::Closure(c) => {
            for (k, p) in c.body.iter().enumerate() {
                eprintln!("{}    body[{}]:", " ".repeat(indent), k);
                print_phrase_members(p, indent + 4);
            }
        }
        McPhrase::Group(g) => {
            for (k, p) in g.opds.iter().enumerate() {
                print_phrase_members(p, indent);
                if k < g.opds.len() - 1 {
                    eprintln!("{}    ---", " ".repeat(indent));
                }
            }
        }
        McPhrase::FuncCall(f) => {
            if let Some(c) = &f.caller {
                eprintln!("{}    caller:", " ".repeat(indent));
                print_phrase_members(c, indent + 4);
            }
        }
        McPhrase::Transposed(line) => {
            print_phrase_members(line, indent);
        }
        McPhrase::Member(phrase, ep) => {
            print_phrase_members(phrase, indent);
            eprintln!("{}    .{}", " ".repeat(indent), ep);
        }
        _ => {
            // For other phrase types, just show the phrase type
            eprintln!("{}    {:?}", " ".repeat(indent), phrase);
        }
    }
}

pub use builder::workspace::WorkspaceKind;

pub fn workspace_info() -> (String, String, String) {
    let meta = builder::workspace::WORKSPACE.active_meta();
    (
        meta.id,
        format!("{:?}", meta.kind),
        meta.root.to_string_lossy().to_string(),
    )
}

pub fn workspace_create(id: &str, kind: WorkspaceKind, root: &std::path::Path) -> bool {
    builder::workspace::WORKSPACE.create_and_switch(id.to_string(), kind, root.to_path_buf())
}

pub fn workspace_switch(id: &str) -> bool {
    builder::workspace::WORKSPACE.switch_to(id)
}

pub fn workspace_remove(id: &str) -> bool {
    builder::workspace::WORKSPACE.remove(id)
}

pub fn workspace_list() -> Vec<(String, String)> {
    builder::workspace::WORKSPACE
        .list()
        .into_iter()
        .map(|(id, kind)| (id, format!("{kind:?}")))
        .collect()
}

// Fix E: make pub wrapper for pub(crate) C log initialization (path consistent with mc_code.rs)
pub fn mcc_log_init(log_file: &str) {
    if let Ok(c) = std::ffi::CString::new(log_file) {
        unsafe {
            crate::ast::c_bindings::mc_log_init(c.as_ptr());
        }
    }
}
pub fn mcc_log_close() {
    unsafe {
        crate::ast::c_bindings::mc_log_close();
    }
}

// Allow binary to register filter override callback
pub use crate::cli::config::set_log_stream_applier;

// Allow binary to suppress engine-level stdout traces (e.g. AST visit tree) when a
// command emits a structured JSON result on stdout, protecting the JSON contract.
pub use crate::cli::config::set_trace_stdout_suppressed;

/// Load trace config from global config file
/// Called by binary at startup
pub fn init_trace_config() {
    use crate::cli::config::{get_runtime_trace, load_global_config};
    if let Ok(config) = load_global_config() {
        if let Ok(mut trace) = get_runtime_trace().write() {
            *trace = config.trace;
        }
    }
}
