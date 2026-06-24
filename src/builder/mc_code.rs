// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::ast_node::McValueFFI;
use crate::ast::ast_semantic::{DeclareId, McSemSymbols, Span, SymbolRangeLapper, SymbolType};
use crate::ast::ast_token::McSemTokens;
use crate::ast::error::message::MISSING_SUBNODE;
use crate::builder::diagnostic::dlog_error;
use crate::builder::global;
use crate::builder::mc_use::McUse;
use crate::builder::workspace;
use crate::core::mc_enum::McEnumDef;
use crate::core::mc_ifs::McInterface;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global deduplication flag: each parse cycle outputs AST visit only once
/// Reset at the mcc_load_project entry point (mcb_reset_ast_visit_flag)
pub static AST_VISIT_DONE: AtomicBool = AtomicBool::new(false);

pub fn mcb_reset_ast_visit_flag() {
    AST_VISIT_DONE.store(false, Ordering::SeqCst);
}
use crate::{ast::ast_node::AstNode, ast::c_macros::*, core::common::McCMIE};
use crate::{current_uri, McComponent, McIds, McModule, McSpaceName, McURI};
use core::panic;
use line_index::LineIndex;
use rust_lapper::Interval;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct McCode {
    pub(crate) mcbase: bool,                             //is mcode base lib
    pub(crate) uri: McURI,                               //absolute path+filename of this .mc file
    pub(crate) ast: AstNode,                             //head pointer to the ast
    pub(crate) tokens: Arc<Mutex<McSemTokens>>,          //tokens parsed during ast parse
    pub(crate) symbols: Arc<Mutex<McSemSymbols>>,        //semantic symbols defined in this file
    pub(crate) uselist: Vec<McUse>,                      //
    pub(crate) spacenames: BTreeMap<McIds, McSpaceName>, //
    pub(crate) line_index: Option<LineIndex>, //line index for position to line/column conversion
    pub(crate) pass1_complete: bool,          // tracks whether parse_pass1_types() has been called
}

////////////////////////////////
impl McCode {
    fn collect_direct_uses(&self, current_path: &Path) -> Vec<McUse> {
        let mut uses = Vec::new();
        self.ast
            .iter()
            .filter(|x| x.is_type(MCAST_USE) || x.is_type(MCAST_USE_PUB))
            .for_each(|node| {
                if let Some(mc_use) = McUse::new(&node, current_path) {
                    uses.push(mc_use);
                }
            });
        uses
    }

    /// Convert character position to line number and column number
    /// Returns (line, column) where both are 1-based
    pub fn pos_to_line_col(&self, pos: u32) -> (u32, u32) {
        if let Some(line_index) = &self.line_index {
            let max_pos: u32 = line_index.len().into();
            if pos > max_pos {
                return (1, 1);
            }
            let line_col = line_index.line_col(line_index::TextSize::new(pos));
            // Convert from zero-based to one-based
            (line_col.line + 1, line_col.col + 1)
        } else {
            // If we don't have line index, return (1, 1) as fallback
            (1, 1)
        }
    }

    pub fn new(uri: &McURI, base: bool) -> Option<Self> {
        //case1: use (abs / relative) + current path
        //case2: mcode abs
        //case3: mcb_add  abs
        //case4: cmie (name -> abs path)
        if fs::metadata(Path::new(&uri)).is_err() {
            tracing::debug!(target: "mcc::code", uri = %uri, "file not found");
            return None;
        }

        Some(McCode {
            mcbase: base,
            uri: uri.clone(),
            ast: AstNode::new(null_mut()),
            tokens: Arc::new(Mutex::new(McSemTokens::new())),
            symbols: Arc::new(Mutex::new(McSemSymbols::new())),
            spacenames: BTreeMap::new(),
            uselist: Vec::new(),
            line_index: None,
            pass1_complete: false,
        })
    }

    pub fn new_empty() -> Self {
        Self {
            mcbase: false,
            uri: String::new(),
            ast: AstNode::new(null_mut()),
            tokens: Arc::new(Mutex::new(McSemTokens::new())),
            symbols: Arc::new(Mutex::new(McSemSymbols::new())),
            spacenames: BTreeMap::new(),
            uselist: Vec::new(),
            line_index: None,
            pass1_complete: false,
        }
    }

    /// Create McCode from an in-memory string (no disk file dependency)
    pub fn new_from_string(uri: &McURI, content: &str) -> Option<Self> {
        Some(McCode {
            mcbase: false,
            uri: uri.clone(),
            ast: AstNode::new(null_mut()),
            tokens: Arc::new(Mutex::new(McSemTokens::new())),
            symbols: Arc::new(Mutex::new(McSemSymbols::new())),
            spacenames: BTreeMap::new(),
            uselist: Vec::new(),
            line_index: Some(LineIndex::new(content)),
            pass1_complete: false,
        })
    }
    pub fn free(&mut self) {
        if !self.ast.is_null() {
            unsafe {
                crate::ast::c_bindings::mcc_free(self.ast.get_ptr());
            }
        }
        self.ast.set_ptr(null_mut() as *mut McValueFFI);
    }

    pub fn parse_ast(&mut self) {
        current_uri::set(&self.uri);
        crate::builder::diagnostic::dlog_clear_file(&self.uri);

        // eprintln!("parse: {:#?}", self.uri);
        let binding = self.uri.clone();
        let fname = Path::new(&binding);

        // First reset, enable trace based on config (must be done before mcc_load)
        let project_root = {
            let meta = workspace::WORKSPACE.active_meta();
            if !meta.id.is_empty() {
                Some(meta.root.clone())
            } else {
                None
            }
        };
        let trace_flag = crate::cli::config::get_trace_flag(project_root.as_deref());
        // Exclude visit bit (0x08) to avoid mcc_parse() internally re-outputting the AST tree
        // visit output is controlled uniformly by Rust side explicitly calling mcc_visit_tree_color()
        let parse_flag = trace_flag & !0x08u8;
        unsafe {
            crate::ast::c_bindings::mcc_reset(parse_flag);
        }

        // Use C mcc_load instead of Rust read_to_string
        // Must use CString to ensure null-terminated string for C
        let c_path = std::ffi::CString::new(binding.clone()).expect("Failed to create CString");
        let fcontent_ptr = unsafe { crate::ast::c_bindings::mcc_load(c_path.as_ptr() as *mut i8) };
        if fcontent_ptr.is_null() {
            tracing::warn!(target: "mcc::code", file = ?fname, "mcc_load failed");
            return;
        }

        // Create line index from the loaded content
        unsafe {
            let fcontent_cstr = std::ffi::CStr::from_ptr(fcontent_ptr as *mut i8);
            if let Ok(fcontent) = fcontent_cstr.to_str() {
                self.line_index = Some(LineIndex::new(fcontent));
            }
        }

        self.free();

        unsafe {
            // Call mcc_reset to ensure complete state cleanup (exclude visit bit, avoid duplicate output)
            crate::ast::c_bindings::mcc_reset(parse_flag);

            // Clear tokens and symbols, ensure no residual data
            if let Ok(mut t) = self.tokens.lock() {
                *t = McSemTokens::new();
            }
            if let Ok(mut s) = self.symbols.lock() {
                *s = McSemSymbols::new();
            }

            crate::ast::c_bindings::mcc_lex(fcontent_ptr);

            let ast = AstNode::new(crate::ast::c_bindings::mcc_parse());
            if ast.is_null() {
                tracing::warn!(target: "mcc::code", file = ?fname, "AST parse returned null");
            } else {
                // Output AST visit (if trace.visit is enabled), once per cycle
                // Skip during system library loading, to prevent mcode loading from preempting user file visit quota
                if crate::cli::config::get_trace_visit() == Some(true)
                    && !crate::cli::config::is_system_lib_loading()
                    && !AST_VISIT_DONE.swap(true, Ordering::SeqCst)
                {
                    crate::ast::c_bindings::mcc_visit_tree_color(
                        ast.get_ptr() as *mut McValueFFI
                    );
                }
                self.ast = ast;
            }

            // Free the loaded content
            libc::free(fcontent_ptr as *mut libc::c_void);

            match self.tokens.lock() {
                Ok(mut t) => {
                    // Clear tokens first, then parse new tokens
                    *t = McSemTokens::new();
                    t.parse(crate::ast::c_bindings::mcc_get_sem_tokens())
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "tokens mutex poisoned");
                }
            }
        }
    }

    pub fn parse_ast_quiet(&mut self) {
        current_uri::set(&self.uri);
        crate::builder::diagnostic::dlog_clear_file(&self.uri);

        let binding = self.uri.clone();
        let fname = Path::new(&binding);

        unsafe {
            crate::ast::c_bindings::mcc_reset(0);
        }

        let c_path = std::ffi::CString::new(binding.clone()).expect("Failed to create CString");
        let fcontent_ptr = unsafe { crate::ast::c_bindings::mcc_load(c_path.as_ptr() as *mut i8) };
        if fcontent_ptr.is_null() {
            tracing::warn!(target: "mcc::code", file = ?fname, "mcc_load failed");
            return;
        }

        unsafe {
            crate::ast::c_bindings::mcc_reset(0);

            if let Ok(mut t) = self.tokens.lock() {
                *t = McSemTokens::new();
            }
            if let Ok(mut s) = self.symbols.lock() {
                *s = McSemSymbols::new();
            }

            crate::ast::c_bindings::mcc_lex(fcontent_ptr);
            let ast = AstNode::new(crate::ast::c_bindings::mcc_parse());
            if !ast.is_null() {
                self.ast = ast;
            }
            libc::free(fcontent_ptr as *mut libc::c_void);
        }
    }

    /// Parse AST from an in-memory string (no disk file dependency)
    /// Note: the caller must set log flags via `mcc_reset()` before calling
    pub fn parse_ast_from_string(&mut self, content: &str) {
        current_uri::set(&self.uri);
        crate::builder::diagnostic::dlog_clear_file(&self.uri);

        let c_content = std::ffi::CString::new(content).expect("Failed to create CString");
        let fcontent_ptr = unsafe {
            crate::ast::c_bindings::mcc_load_from_string(
                c_content.as_ptr() as *const i8,
                content.len(),
            )
        };
        if fcontent_ptr.is_null() {
            tracing::warn!(target: "mcc::code", uri = %self.uri, "mcc_load_from_string failed");
            return;
        }

        self.free();

        unsafe {
            if let Ok(mut t) = self.tokens.lock() {
                *t = McSemTokens::new();
            }
            if let Ok(mut s) = self.symbols.lock() {
                *s = McSemSymbols::new();
            }

            crate::ast::c_bindings::mcc_lex(fcontent_ptr);

            let ast = AstNode::new(crate::ast::c_bindings::mcc_parse());
            if ast.is_null() {
                tracing::warn!(target: "mcc::code", uri = %self.uri, "AST parse returned null");
            } else {
                // Output AST visit (if trace.visit is enabled), once per cycle
                // Skip during system library loading, to prevent mcode loading from preempting user file visit quota
                if crate::cli::config::get_trace_visit() == Some(true)
                    && !crate::cli::config::is_system_lib_loading()
                    && !AST_VISIT_DONE.swap(true, Ordering::SeqCst)
                {
                    crate::ast::c_bindings::mcc_visit_tree_color(
                        ast.get_ptr() as *mut McValueFFI
                    );
                }
                self.ast = ast;
            }

            libc::free(fcontent_ptr as *mut libc::c_void);

            match self.tokens.lock() {
                Ok(mut t) => {
                    *t = McSemTokens::new();
                    t.parse(crate::ast::c_bindings::mcc_get_sem_tokens())
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "tokens mutex poisoned");
                }
            }
        }
    }

    pub fn parse_nsp(&mut self) {
        // Check whether prj_mcodes already has the file's built spacenames
        // If yes, reuse existing spacenames and uselist to avoid rebuilding
        let canonical_uri = {
            let path_buf = PathBuf::from(self.uri.clone());
            path_buf
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| self.uri.clone())
        };

        // Check whether prj_mcodes already has the file and spacenames are built
        if let Some(existing) = workspace::WORKSPACE.mcodes.borrow().get(&canonical_uri) {
            if !existing.spacenames.is_empty() {
                // Reuse existing spacenames and uselist
                self.spacenames.clone_from(&existing.spacenames);
                self.uselist.clone_from(&existing.uselist);
                // Mark pass1 as complete (dependency file has been processed)
                self.pass1_complete = true;
                return;
            }
        }

        self.uselist.clear();
        self.spacenames.clear();

        let path_buf = PathBuf::from(self.uri.clone());
        let Some(current_path) = path_buf.parent() else {
            tracing::warn!(target: "mcc::code", uri = %self.uri, "cannot get parent path");
            return;
        };

        //1. uses to use list
        self.uselist = self.collect_direct_uses(current_path);

        //2. load spacenames from use targets
        let mut uses_stack = Vec::<McUse>::new();
        let mut visited_uses = HashSet::<String>::new();
        self.uselist
            .iter()
            .for_each(|mu| uses_stack.push(mu.clone()));

        while let Some(mcuse) = uses_stack.pop() {
            // ★ Fix: use the same path normalization logic as mcb_add_recursive
            // Relative paths should be resolved relative to the current file's directory, not CWD
            let use_path = current_path.join(&mcuse.uri);
            let canonical_use_uri = use_path
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| mcuse.uri.clone());
            if !visited_uses.insert(canonical_use_uri.clone()) {
                continue;
            }

            // (1). load ast
            // ★ Fix: always parse AST (because AstNode pointer cannot be reused across contexts)
            // but can reuse existing spacenames and uselist
            let existing_spacenames: BTreeMap<McIds, McSpaceName>;
            let existing_uselist: Vec<McUse>;
            {
                if let Some(existing) = workspace::WORKSPACE.mcodes.borrow().get(&canonical_use_uri)
                {
                    existing_spacenames = existing.spacenames.clone();
                    existing_uselist = existing.uselist.clone();
                } else {
                    existing_spacenames = BTreeMap::new();
                    existing_uselist = Vec::new();
                }
            }
            let mut mcfile = match McCode::new(&mcuse.uri, self.mcbase) {
                Some(mcfile) => mcfile,
                None => {
                    tracing::debug!(target: "mcc::code", uri = %mcuse.uri, "use file not found");
                    continue;
                }
            };
            if self.mcbase {
                mcfile.parse_ast_quiet();
            } else {
                mcfile.parse_ast();
            }

            // (2). load idx from current file
            let cmie_list = mcfile.parse_cmie_names();

            // (2.5) ★ Step 3: ensure CMIE definitions are registered to the global table
            // If spacenames and uselist already exist, reuse them directly
            if !existing_spacenames.is_empty() && !existing_uselist.is_empty() {
                // Reuse existing spacenames
                for (key, value) in &existing_spacenames {
                    if !self.spacenames.contains_key(key) {
                        self.spacenames.insert(key.clone(), value.clone());
                    }
                }
            } else {
                // Need to build spacenames and uselist
                mcfile.uri = canonical_use_uri.clone();
                for (_, space_name) in mcfile.spacenames.iter_mut() {
                    space_name.uri = canonical_use_uri.clone();
                }
                let saved_uri = crate::current_uri::try_get();
                crate::current_uri::set(&canonical_use_uri);
                mcfile.parse_pass1_types();
                if let Some(ref uri) = saved_uri {
                    crate::current_uri::set(uri);
                }
                mcfile.parse_nsp();
                workspace::WORKSPACE
                    .mcodes
                    .borrow()
                    .insert(canonical_use_uri.clone(), mcfile.clone());
                for (key, value) in &mcfile.spacenames {
                    if !self.spacenames.contains_key(key) {
                        self.spacenames.insert(key.clone(), value.clone());
                    }
                }
                // Mark the current file's pass1 as complete (dependent components are already registered in parse_pass1_types above)
                self.pass1_complete = true;
            }

            match mcuse.impt_ids {
                None => {
                    for cmie in cmie_list {
                        self.spacenames
                            .insert(cmie.clone(), McSpaceName::new(&cmie, mcuse.uri.clone()));
                    }
                }
                Some(classes) => {
                    for class in classes {
                        if cmie_list.contains(&class) {
                            self.spacenames
                                .insert(class.clone(), McSpaceName::new(&class, mcuse.uri.clone()));
                        } else {
                            tracing::warn!(
                                target: "mcc::code",
                                definition = %class,
                                uri = %mcuse.uri,
                                "use'd definition does not exist in target file"
                            );
                        }
                    }
                }
            }

            for (key, value) in &mcfile.spacenames {
                if !self.spacenames.contains_key(key) {
                    self.spacenames.insert(key.clone(), value.clone());
                }
            }

            let dep_path_buf = PathBuf::from(mcfile.uri.clone());
            let dep_current_path = dep_path_buf.parent().unwrap_or(current_path);
            for mc_use in mcfile.collect_direct_uses(dep_current_path) {
                if mc_use.public {
                    uses_stack.push(mc_use);
                }
            }
        }

        //3. self file cmie definitions
        self.parse_cmie_names();
    }

    /// List of class names defined in this file
    pub fn parse_cmie_names(&mut self) -> Vec<McIds> {
        let mut cmies: Vec<McIds> = Vec::<McIds>::new();
        for node in self.ast.iter() {
            if node.is_type(MCAST_INTERFACE)
                || node.is_type(MCAST_COMPONENT)
                || node.is_type(MCAST_MODULE)
            {
                let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);
                if let Some(class_name) = McIds::new(
                    &subnodes
                        .iter()
                        .find(|x| x.is_type(MCAST_NAME))
                        .expect(MISSING_SUBNODE)
                        .get_sub_node() // ids
                        .expect(MISSING_SUBNODE),
                ) {
                    if cmies.contains(&class_name) {
                        dlog_error(501, &node, "Definition already exists");
                    } else {
                        self.spacenames.insert(
                            class_name.clone(),
                            McSpaceName::new(&class_name, self.uri.clone()),
                        );
                        cmies.push(class_name);
                    }
                }
            } //TODO enum
        }
        cmies
    }

    /// Load a single CMIE from mcode base lib and add to global tables
    pub fn parse_cmie_single(&mut self, ident: &McIds) -> Option<McCMIE> {
        for node in self.ast.iter() {
            if node.is_type(MCAST_INTERFACE)
                || node.is_type(MCAST_COMPONENT)
                || node.is_type(MCAST_MODULE)
                || node.is_type(MCAST_ENUM)
            {
                let subnodes = node.get_sub_node().expect(MISSING_SUBNODE);
                if let Some(name) = McIds::new(
                    &subnodes
                        .iter()
                        .find(|x| x.is_type(MCAST_NAME))
                        .expect(MISSING_SUBNODE)
                        .get_sub_node() // ids
                        .expect(MISSING_SUBNODE),
                ) {
                    if ident == &name {
                        match node.get_type() {
                            MCAST_COMPONENT => {
                                if let Some(comp) = McComponent::new(&node, &self.uri) {
                                    let components_guard = global::mcc_components.borrow_mut();
                                    let result = components_guard
                                        .entry(McSpaceName {
                                            ident: comp.name.clone(),
                                            uri: self.uri.clone(),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(1002, &node, "Duplicate component");
                                        })
                                        .or_insert(Arc::new(comp));
                                    return Some(McCMIE::Component(result.value().clone()));
                                };
                            }

                            MCAST_MODULE => {
                                // Phase 3: pre-parse function bodies before Arc wrapping
                                if let Some(mdl) = McModule::new(&node, &self.uri) {
                                    let modules_guard = global::mcc_modules.borrow();
                                    let result = modules_guard
                                        .entry(McSpaceName {
                                            ident: mdl.name.clone(),
                                            uri: self.uri.clone(),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(1004, &node, "Duplicate module");
                                        })
                                        .or_insert(Arc::new(mdl));
                                    return Some(McCMIE::Module(result.value().clone()));
                                }
                            }
                            MCAST_INTERFACE => {
                                if let Some(ifs) = McInterface::new(&node, &self.uri) {
                                    let ifs_guard = global::mcc_interfaces.borrow_mut();
                                    let result = ifs_guard
                                        .entry(McSpaceName {
                                            ident: ifs.name.clone(),
                                            uri: self.uri.clone(),
                                        })
                                        .and_modify(|_| {
                                            dlog_error(1501, &node, "Duplicate interface");
                                        })
                                        .or_insert(Arc::new(ifs));
                                    return Some(McCMIE::Interface(result.value().clone()));
                                }
                            }
                            MCAST_ENUM => {
                                if let Some(enum_def) = McEnumDef::new(&node, &self.uri) {
                                    let space_name = McSpaceName {
                                        ident: enum_def.name.clone(),
                                        uri: self.uri.clone(),
                                    };
                                    let arc_enum = Arc::new(enum_def);
                                    if self.mcbase {
                                        let enums_guard = global::mcc_enums.borrow_mut();
                                        enums_guard
                                            .entry(space_name.clone())
                                            .and_modify(|_| {
                                                dlog_error(1504, &node, "Duplicate enum");
                                            })
                                            .or_insert(arc_enum.clone());
                                    } else {
                                        let enums_guard = workspace::WORKSPACE.enums.borrow_mut();
                                        enums_guard
                                            .entry(space_name.clone())
                                            .and_modify(|_| {
                                                dlog_error(1004, &node, "Duplicate enum");
                                            })
                                            .or_insert(arc_enum.clone());
                                    }
                                    return Some(McCMIE::Enum(arc_enum));
                                }
                            }
                            _ => panic!(),
                        }
                    }
                }
            }
        }

        None
    }

    /// Parse current file, add all definitions to project tables (parse_cmie_to_project)
    /// Phase 1a: only register component/interface/enum definitions to the global table
    /// This step does not parse module body, ensuring cross-file type definitions are ready first
    pub fn parse_pass1_types(&mut self) {
        for node in self.ast.iter() {
            match node.get_type() {
                MCAST_INTERFACE => {
                    if let Some(ifs) = McInterface::new(&node, &self.uri) {
                        let space_name = McSpaceName {
                            ident: ifs.name.clone(),
                            uri: self.uri.clone(),
                        };
                        if self.mcbase {
                            global::mcc_interfaces
                                .borrow_mut()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1001, &node, "Duplicate interface");
                                })
                                .or_insert(Arc::new(ifs));
                        } else {
                            workspace::WORKSPACE
                                .interfaces
                                .borrow_mut()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1001, &node, "Duplicate interface");
                                })
                                .or_insert(Arc::new(ifs));
                        }
                    }
                }
                MCAST_COMPONENT => {
                    if let Some(comp) = McComponent::new(&node, &self.uri) {
                        // ★ First clone the needed data (name + uri) for global_table,
                        // then move comp into the Arc table
                        let comp_name_str = comp.name.to_string();
                        let comp_span: Span =
                            (node.get_pos() as usize)..((node.get_pos() + node.get_len()) as usize);
                        let self_uri = self.uri.clone();

                        let space_name = McSpaceName {
                            ident: comp.name.clone(),
                            uri: self.uri.clone(),
                        };
                        {
                            if self.mcbase {
                                global::mcc_components
                                    .borrow_mut()
                                    .entry(space_name)
                                    .and_modify(|_| {
                                        dlog_error(1002, &node, "Duplicate component");
                                    })
                                    .or_insert(Arc::new(comp));
                            } else {
                                workspace::WORKSPACE
                                    .components
                                    .borrow()
                                    .entry(space_name)
                                    .and_modify(|_| {
                                        dlog_error(1002, &node, "Duplicate component");
                                    })
                                    .or_insert(Arc::new(comp));
                            }
                        } // borrow is dropped at end of block

                        // ★ Fix: also register to global_table.class_id_to_span,
                        // letting create_lapper() find the component's span.
                        // Previously only inserted into workspace.components without filling class_id_to_span,
                        // causing LSP goto_definition's symbol_lapper to always be empty.
                        self.add_global_class(&self_uri, &comp_name_str, comp_span);
                    }
                }
                MCAST_ENUM => {
                    if let Some(enum_def) = McEnumDef::new(&node, &self.uri) {
                        let space_name = McSpaceName {
                            ident: enum_def.name.clone(),
                            uri: self.uri.clone(),
                        };
                        if self.mcbase {
                            global::mcc_enums
                                .borrow()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1004, &node, "Duplicate enum");
                                })
                                .or_insert(Arc::new(enum_def));
                        } else {
                            workspace::WORKSPACE
                                .enums
                                .borrow()
                                .entry(space_name)
                                .and_modify(|_| {
                                    dlog_error(1004, &node, "Duplicate enum");
                                })
                                .or_insert(Arc::new(enum_def));
                        }
                    }
                }
                _ => {} // MCAST_MODULE handled in the second phase
            }
        }
        // Mark Pass1 parse as complete
        self.pass1_complete = true;

        // ★ Fix: parse_pass1_types filled global_table, but did not call create_lapper.
        // LSP goto_definition needs lapper to map (offset -> SymbolType).
        self.create_lapper();
    }

    /// Phase 1b: parse all module definitions (at this point all component/interface/enum are already registered)
    pub fn parse_pass1_modules(&mut self) {
        // eprintln!("[DEBUG parse_pass1_modules] uri={}", self.uri);
        for (_i, node) in self.ast.iter().enumerate() {
            let node_type = node.get_type();
            if node_type == MCAST_MODULE {
                if let Some(module) = McModule::new(&node, &self.uri) {
                    let module_name = module.name.clone();
                    workspace::WORKSPACE
                        .modules
                        .borrow()
                        .entry(McSpaceName {
                            ident: module_name.clone(),
                            uri: self.uri.clone(),
                        })
                        .and_modify(|_| {
                            dlog_error(1503, &node, "Duplicate module");
                        })
                        .or_insert(Arc::new(module));
                }
            }
        }
    }

    /// Backward-compatible interface: parse all definitions sequentially (single-file scenario or system library)
    pub fn parse_pass1(&mut self) {
        self.parse_pass1_types();
        self.parse_pass1_modules();
    }

    // ========================================================================
    // Phase 3: Pre-parse function bodies
    // ========================================================================

    /// Pre-parse function bodies for all functions in the module.
    pub fn add_global_class(
        &mut self,
        uri: &McURI,
        class_name: &String,
        span: Span,
    ) -> Option<DeclareId> {
        match self.symbols.lock() {
            Ok(sem) => match sem.global_table.lock() {
                Ok(mut gt) => {
                    let gt: &mut crate::ast::ast_semantic::GlobalSymbolTable = &mut gt;
                    Some(gt.add_class(uri, class_name, span))
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (add_global_class)");
                    None
                }
            },
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_global_class)");
                None
            }
        }
    }
    pub fn add_declare_class(&mut self, uri: &McURI, span: Span, class_id: DeclareId) {
        match self.symbols.lock() {
            Ok(sem) => match sem.global_table.lock() {
                Ok(mut gt) => {
                    let gt: &mut crate::ast::ast_semantic::GlobalSymbolTable = &mut gt;
                    gt.add_declare_class(uri, span, class_id)
                }
                Err(e) => {
                    tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (add_declare_class)")
                }
            },
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_declare_class)")
            }
        }
    }
    pub fn add_declare_inst(&mut self, span: Span) -> Option<DeclareId> {
        match self.symbols.lock() {
            Ok(mut symbols) => Some(symbols.local_table.add_declare(span)),
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_declare_inst)");
                None
            }
        }
    }
    pub fn add_inst_reference(&mut self, span: Span, declr_id: DeclareId) {
        match self.symbols.lock() {
            Ok(mut symbols) => {
                symbols.local_table.add_inst(span, declr_id);
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (add_inst_reference)")
            }
        }
    }

    pub fn create_lapper(&mut self) {
        match self.symbols.lock() {
            Ok(mut sem) => {
                let mut symbol_lapper = SymbolRangeLapper::new(vec![]);

                match sem.global_table.lock() {
                    Ok(gt) => {
                        let clsids: Vec<_> = gt
                            .class_name_to_id
                            .iter()
                            .filter(|((uri, _clsname), _clsid)| uri == &self.uri)
                            .map(|(_key, clsid)| *clsid)
                            .collect();

                        let _ = clsids.iter().map(|clsid| {
                            if let Some((_uri, span)) = gt.class_id_to_span.get(clsid) {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::ClassDefinition(*clsid),
                                });
                            }
                        });

                        let _ = gt
                            .span_to_declare_class_id
                            .iter()
                            .filter(|((uri, _span), _refid)| uri == &self.uri)
                            .map(|((_uri, span), refid)| {
                                symbol_lapper.insert(Interval {
                                    start: span.start,
                                    stop: span.end,
                                    val: SymbolType::DeclareClass(*refid),
                                });
                            });
                    }
                    Err(e) => {
                        tracing::error!(target: "mcc::code", error = %e, "global_table mutex poisoned (create_lapper)")
                    }
                }

                //local
                let _ = sem
                    .local_table
                    .declare_inst_to_span
                    .iter()
                    .map(|(dcl_id, span)| {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::DeclareInstance(*dcl_id),
                        });
                    });
                let _ = sem
                    .local_table
                    .inst_id_to_span
                    .iter()
                    .map(|(inst_id, span)| {
                        symbol_lapper.insert(Interval {
                            start: span.start,
                            stop: span.end,
                            val: SymbolType::InstanceReference(*inst_id),
                        });
                    });

                sem.symbol_lapper = symbol_lapper;
            }
            Err(e) => {
                tracing::error!(target: "mcc::code", error = %e, "symbols mutex poisoned (create_lapper)")
            }
        }
    }

    pub fn pass2(&mut self) {}
}
