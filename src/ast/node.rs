// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::bindings;
use crate::ast::macros::*;
use crate::db::diagnostic::diagnostic::{dlog_error, Position};
use crate::{McIda, McIds};
use std::ffi::{c_char, c_void, CStr};
use std::ptr::NonNull;
use std::str::FromStr;

// C struct mc_value for FFI binding
#[repr(C)]
pub struct McValueFFI {
    pub type_: u16,
    pub data: *mut c_void,
    pub pos: i32,
    pub len: i32,
    pub rpos: i32,
    pub rlen: i32,
    pub next: *mut McValueFFI,
    pub sub: *mut McValueFFI,
}

/// AST node wrapper for safe access
#[derive(Debug)]
pub struct AstNode {
    ptr: *mut McValueFFI,
    owned: bool,
}

pub struct AstNodeIter {
    current: *mut McValueFFI,
}

impl Clone for AstNode {
    fn clone(&self) -> AstNode {
        AstNode {
            ptr: self.ptr,
            owned: false,
        }
    }
}

impl AstNode {
    pub fn new(ptr: *mut McValueFFI) -> Self {
        Self { ptr, owned: true }
    }

    pub fn iter(&self) -> AstNodeIter {
        AstNodeIter {
            current: self.get_ptr(),
        }
    }

    pub fn from_ptr(ptr: *mut McValueFFI) -> Option<Self> {
        // Validate pointer: reject null / low address (< 0x1000, user-space never reachable)
        // / misaligned. C parser occasionally emits corrupted AST (e.g. writing string path to
        // next/sub fields), without validation consumer would dereference → SIGSEGV.
        let addr = ptr as usize;
        if addr == 0 || addr < 0x1000 || addr % std::mem::align_of::<McValueFFI>() != 0 {
            return None;
        }
        Some(AstNode { ptr, owned: false })
    }

    pub fn set_ptr(&mut self, ptr: *mut McValueFFI) {
        self.ptr = ptr;
    }

    pub fn get_ptr(&self) -> *mut McValueFFI {
        self.ptr
    }

    pub fn get_pos(&self) -> Position {
        if self.is_null() {
            return 0;
        }
        unsafe { (*self.ptr).pos.try_into().unwrap_or(0) }
    }

    pub fn get_len(&self) -> u32 {
        if self.is_null() {
            return 0;
        }
        unsafe { (*self.ptr).len.try_into().unwrap_or(0) }
    }

    /// The rule's coverage span start (U159 `rpos/rlen` face): the whole text
    /// the rule reduced from, not just its name anchor. 0 when the parser
    /// recorded no rule span (`rlen == 0` means "no rule span recorded").
    pub fn get_rpos(&self) -> u32 {
        if self.is_null() {
            return 0;
        }
        unsafe { (*self.ptr).rpos.try_into().unwrap_or(0) }
    }

    pub fn get_rlen(&self) -> u32 {
        if self.is_null() {
            return 0;
        }
        unsafe { (*self.ptr).rlen.try_into().unwrap_or(0) }
    }

    pub fn is_null(&self) -> bool {
        // Validate pointer: reject null OR < 0x1000 (user-space never reachable)
        // Make is_null() actually a safe check: all methods that walk
        // `if self.is_null() { return } unsafe { &*self.ptr }` pattern
        // (to_string / to_i32 / get_type / is_type / etc.) once benefit.
        let addr = self.get_ptr() as usize;
        addr == 0 || addr < 0x1000
    }

    pub fn is_type(&self, type_: u16) -> bool {
        if self.is_null() {
            return false;
        }
        self.get_type() == type_
    }

    pub fn get_type(&self) -> u16 {
        if self.is_null() {
            return 0;
        }
        unsafe { (*self.ptr).type_ }
    }

    pub fn get_data(&self) -> *mut std::ffi::c_void {
        if self.is_null() {
            return std::ptr::null_mut();
        }
        // Validate .data pointer: reject null OR < 0x1000 (user-space never reachable)
        // C parser occasionally emits corrupted AST (e.g. writing string path to
        // next/sub fields), without validation consumer would dereference → SIGSEGV.
        let data_addr = unsafe { (*self.ptr).data } as usize;
        if data_addr < 0x1000 {
            return std::ptr::null_mut();
        }
        unsafe { (*self.ptr).data }
    }

    pub fn get_sub_node(&self) -> Option<AstNode> {
        if self.is_null() {
            return None;
        }
        unsafe { AstNode::from_ptr((*self.ptr).sub as *mut McValueFFI) }
    }

    pub fn get_next(&self) -> Option<AstNode> {
        if self.is_null() {
            return None;
        }
        unsafe { AstNode::from_ptr((*self.ptr).next as *mut McValueFFI) }
    }

    /// The clauses of a body, with an in-body partition made transparent.
    ///
    /// `block <name> { ... }` (AST kind `MCAST_PARTITION`) groups the clauses
    /// already written in a body: it opens no scope and issues no id, so a
    /// clause written inside a partition is read exactly as if it had been
    /// written in the enclosing body. Every walk over a body's clauses reads
    /// them from here, so no face can disagree about which clauses a body
    /// holds — a partition cannot be read by one walk and rejected by the
    /// next, which is what each body's own catch-all did before this existed
    /// (measured: E3081 in a module body, E5058 in a recipe, E5253/E5255/
    /// E5261 in a component, an interface and a define).
    ///
    /// The partition's own name and level word are not returned: they are the
    /// grouping's label, not clauses of the body.
    pub fn clause_list(&self) -> Vec<AstNode> {
        let mut out: Vec<AstNode> = Vec::new();
        self.collect_clauses(&mut out);
        out
    }

    fn collect_clauses(&self, out: &mut Vec<AstNode>) {
        let Some(first) = self.get_sub_node() else {
            return;
        };
        for child in first.iter() {
            if child.is_type(MCAST_PARTITION) {
                // [ level, name, body ] — the body is the last child; an empty
                // partition has a body node with no clauses at all.
                if let Some(body) = child
                    .get_sub_node()
                    .and_then(|sub| sub.iter().find(|n| n.is_type(MCAST_BODY)))
                {
                    body.collect_clauses(out);
                }
            } else {
                out.push(child);
            }
        }
    }

    /// Safely read .data field as CStr.
    ///
    /// C parser occasionally emits corrupted AST (e.g. writing string path to
    /// next/sub fields), which would cause .data to be a small integer (e.g. 0x24 / 0x1),
    /// which would pass to `CStr::from_ptr` and cause SIGSEGV.
    /// Here we validate .data is a valid heap pointer before dereferencing.
    ///
    /// Every consumer must go through this accessor (never
    /// `CStr::from_ptr(node.get_data())` directly): a NULL / small-integer
    /// `.data` deref segfaults inside `strlen`.
    pub fn data_as_cstr(&self) -> Option<&std::ffi::CStr> {
        if self.is_null() {
            return None;
        }
        let data_addr = unsafe { (*self.ptr).data } as usize;
        if data_addr < 0x1000 {
            return None;
        }
        let p = NonNull::new(unsafe { (*self.ptr).data })?;
        Some(unsafe { CStr::from_ptr(p.as_ptr() as *const c_char) })
    }

    pub fn to_float(&self) -> Option<f64> {
        if self.is_null() {
            return None;
        }
        let mc_value = unsafe { &*self.get_ptr() };
        match mc_value.type_ {
            // data node
            MCAST_FLOAT => self
                .data_as_cstr()?
                .to_str()
                .ok()
                .and_then(|s| s.parse::<f64>().ok()),
            _ => {
                // Silent for non-float types — caller handles None
                None
            }
        }
    }

    pub fn to_i32(&self) -> Option<i32> {
        if self.is_null() {
            return None;
        }
        let mc_value = unsafe { &*self.get_ptr() };
        match mc_value.type_ {
            // data nodes
            MCAST_INT => self
                .data_as_cstr()?
                .to_str()
                .ok()
                .and_then(|s| i32::from_str(s).ok()),
            MCAST_HEX => {
                if let Ok(s) = self.data_as_cstr()?.to_str() {
                    let s = s.trim_start_matches("0x").trim_start_matches("0X");
                    i32::from_str_radix(s, 16).ok()
                } else {
                    None
                }
            }
            MCAST_FLOAT => {
                // Delegate to to_float method for consistency
                self.to_float().map(|f| f as i32)
            }
            // CONST is a data node — data contains the constant string
            MCAST_CONST => self
                .data_as_cstr()?
                .to_str()
                .ok()
                .and_then(|s| i32::from_str(s).ok()),
            _ => {
                // Silent for non-integer types — caller handles None
                None
            }
        }
    }

    pub fn to_u32(&self) -> Option<u32> {
        if self.is_null() {
            dlog_error(
                crate::errcodes::AST_NODE_EMPTY,
                self,
                &crate::errcodes::format_msg(crate::errcodes::AST_NODE_EMPTY, &[]),
            );
            return None;
        }
        let mc_value = unsafe { &*self.get_ptr() };
        match mc_value.type_ {
            // data node
            MCAST_INT => {
                let Some(rust_str) = self
                    .data_as_cstr()?
                    .to_str()
                    .map_err(|_| "Invalid UTF-8 string")
                    .ok()
                else {
                    dlog_error(
                        crate::errcodes::AST_UTF8_ERROR,
                        self,
                        &crate::errcodes::format_msg(crate::errcodes::AST_UTF8_ERROR, &[]),
                    );
                    return None;
                };
                u32::from_str(rust_str).map_err(|_| "Parse failed").ok()
            }
            _ => {
                dlog_error(
                    crate::errcodes::AST_TYPE_MISMATCH,
                    self,
                    &crate::errcodes::format_msg(crate::errcodes::AST_TYPE_MISMATCH, &[]),
                );
                None
            }
        }
    }

    /// Split a named argument into its `(key, value)` node pair.
    ///
    /// The argument grammar spells a named key two ways and both mean the same
    /// thing, so both must read back the same way:
    ///   `k: v`  -> `MCAST_OPD_COLON` children `(key, value)`
    ///   `k = v` -> `MCAST_ATTRIBUTE` children `(MCAST_ATT_ID, MCAST_ATT_VALUES)`
    /// The `MCAST_ATT_*` wrappers are unwrapped so the value is the value node
    /// itself (an `MCAST_RANGE_PLUSMINUS` and the like keep their type, which is
    /// where a structural `±` comes from). `None` when `node` is not a named
    /// argument — an argument reader must not turn a positional one into a
    /// keyless pair.
    pub fn named_arg_parts(&self) -> Option<(AstNode, AstNode)> {
        match self.get_type() {
            MCAST_OPD_COLON => {
                let key = self.get_sub_node()?;
                let value = key.get_next()?;
                Some((key, value))
            }
            MCAST_ATTRIBUTE => {
                let id = self.get_sub_node().filter(|c| c.is_type(MCAST_ATT_ID))?;
                let key = match id.get_sub_node() {
                    Some(key) => key,
                    None => id.clone(),
                };
                let values = id.get_next().filter(|c| c.is_type(MCAST_ATT_VALUES))?;
                let value = values.get_sub_node()?;
                Some((key, value))
            }
            _ => None,
        }
    }

    /// node of certain types to String
    pub fn to_string(&self) -> Option<String> {
        if self.is_null() {
            dlog_error(
                crate::errcodes::AST_NODE_EMPTY,
                self,
                &crate::errcodes::format_msg(crate::errcodes::AST_NODE_EMPTY, &[]),
            );
            return None;
        }
        let mc_value = unsafe { &*self.get_ptr() };

        match mc_value.type_ {
            // === Data nodes (use mc_value.data) ===
            MCAST_STRING | MCAST_INT | MCAST_FLOAT | MCAST_URI_PREFIX | MCAST_URI_VERSION
            | MCAST_ID => match self.data_as_cstr()?.to_str() {
                Ok(s) => Some(s.to_string()),
                Err(e) => {
                    dlog_error(
                        crate::errcodes::AST_UTF8_ERROR,
                        self,
                        &crate::errcodes::format_msg(crate::errcodes::AST_UTF8_ERROR, &[&e]),
                    );
                    None
                }
            },

            MCAST_IDA => {
                // IDA is an identifier with a dot, directly read the raw string
                self.data_as_cstr()?.to_str().ok().map(|s| s.to_string())
            }

            // === Sub nodes (use mc_value.sub) ===
            MCAST_OPD_DOT => {
                // DOT node: left.right
                if let Some(left) = self.get_sub_node() {
                    if let Some(right) = left.get_next() {
                        let l = left.to_string().unwrap_or_default();
                        let r = right.to_string().unwrap_or_default();
                        Some(format!("{l}.{r}"))
                    } else {
                        left.to_string()
                    }
                } else {
                    None
                }
            }

            MCAST_OPD_SQUARE_VEC => {
                // [A, B, C] → "[A,B,C]"
                if let Some(sub_nodes) = self.get_sub_node() {
                    let parts: Vec<String> =
                        sub_nodes.iter().filter_map(|n| n.to_string()).collect();
                    Some(format!("[{}]", parts.join(",")))
                } else {
                    None
                }
            }

            MCAST_HEX => self.data_as_cstr()?.to_str().ok().map(|s| s.to_string()),

            MCAST_UNIT_INT | MCAST_UNIT_FLOAT | MCAST_UNIT_STRING => {
                self.data_as_cstr()?.to_str().ok().map(|s| s.to_string())
            }

            // ★ Fix 2: OPD_PLUS / OPD_MINUS — binary ops / string concatenation
            MCAST_OPD_PLUS | MCAST_OPD_MINUS => {
                if let Some(left) = self.get_sub_node() {
                    let l = left.to_string().unwrap_or_default();
                    if let Some(right) = left.get_next() {
                        let r = right.to_string().unwrap_or_default();
                        let op = if mc_value.type_ == MCAST_OPD_PLUS {
                            "+"
                        } else {
                            "-"
                        };
                        Some(format!("{l} {op} {r}"))
                    } else {
                        Some(l)
                    }
                } else {
                    None
                }
            }

            // ★ Fix 3: OPR_PLUS / OPR_MINUS / OPR_MULTI / OPR_DIVID — expression-level operators
            // In AST, expression `+` (rule 294) creates MCAST_OPD_PLUS (181), NOT MCAST_OPD_PLUS
            // (71).
            // This is used in attribute string concatenation like: description = "text" + param +
            // "text"
            MCAST_OPD_MULTI | MCAST_OPD_DIVID => {
                if let Some(left) = self.get_sub_node() {
                    let l = left.to_string().unwrap_or_default();
                    if let Some(right) = left.get_next() {
                        let r = right.to_string().unwrap_or_default();
                        let op = match mc_value.type_ {
                            MCAST_OPD_PLUS => "+",
                            MCAST_OPD_MINUS => "-",
                            MCAST_OPD_MULTI => "*",
                            MCAST_OPD_DIVID => "/",
                            _ => "?",
                        };
                        Some(format!("{l} {op} {r}"))
                    } else {
                        Some(l)
                    }
                } else {
                    None
                }
            }

            // ★ Fix 3: MCAST_PARAMS — parameter wrapper node
            MCAST_PARAMS => {
                if let Some(sub) = self.get_sub_node() {
                    let parts: Vec<String> = sub.iter().filter_map(|n| n.to_string()).collect();
                    if parts.is_empty() {
                        None
                    } else {
                        Some(parts.join(", "))
                    }
                } else {
                    None
                }
            }

            // ★ Fix 2: OPD_CURLY — name{members}
            MCAST_OPD_CURLY => {
                if let Some(left) = self.get_sub_node() {
                    let name = left.to_string().unwrap_or_default();
                    if let Some(right) = left.get_next() {
                        let members = right.to_string().unwrap_or_default();
                        Some(format!("{name}{{{members}}}"))
                    } else {
                        Some(name)
                    }
                } else {
                    None
                }
            }

            // ★ Fix 2: OPD_CURLY_MN — name{left|right}
            MCAST_OPD_CURLY_MN => {
                if let Some(sub) = self.get_sub_node() {
                    sub.to_string()
                } else {
                    None
                }
            }

            // ★ Fix 2: OPD_IDAN — indexed identifier array
            MCAST_OPD_IDAN => {
                if let Some(sub) = self.get_sub_node() {
                    let parts: Vec<String> = sub.iter().filter_map(|n| n.to_string()).collect();
                    Some(parts.join(","))
                } else {
                    None
                }
            }

            // OPD_OPDS — comma-separated list of opds; the CURLY_MN grammar
            // wraps each side of `comp{left|right}` in this node so the `|`
            // boundary survives mc_value_link3's next-chain flattening.
            MCAST_OPDS => {
                if let Some(sub) = self.get_sub_node() {
                    let parts: Vec<String> = sub.iter().filter_map(|n| n.to_string()).collect();
                    Some(parts.join(","))
                } else {
                    None
                }
            }

            // ★ Fix 2: RANGE — range expression like 1:5
            MCAST_OPD_COLON => {
                if let Some(left) = self.get_sub_node() {
                    let l = left.to_string().unwrap_or_default();
                    if let Some(right) = left.get_next() {
                        let r = right.to_string().unwrap_or_default();
                        Some(format!("{l}:{r}"))
                    } else {
                        Some(l)
                    }
                } else {
                    None
                }
            }

            // ★ Fix 2: RANGE_PLUSMINUS
            MCAST_RANGE_PLUSMINUS => {
                if let Some(left) = self.get_sub_node() {
                    let l = left.to_string().unwrap_or_default();
                    if let Some(right) = left.get_next() {
                        let r = right.to_string().unwrap_or_default();
                        Some(format!("{l}±{r}"))
                    } else {
                        Some(l)
                    }
                } else {
                    None
                }
            }

            // CONST — data node with constant string
            MCAST_CONST => self.data_as_cstr()?.to_str().ok().map(|s| s.to_string()),

            // ★ Fix 2: NAME — container node
            MCAST_NAME | MCAST_IOTYPE => {
                if let Some(sub) = self.get_sub_node() {
                    sub.to_string()
                } else {
                    None
                }
            }
            MCAST_IOTYPE_IN..=MCAST_IOTYPE_NC => {
                self.data_as_cstr()?.to_str().ok().map(|s| s.to_string())
            }

            // Unit value types — e.g. 5V, 100mA, 2.2uH (data nodes)
            t if (MCAST_UVAL_VOLT..=MCAST_UVAL_CHARGE).contains(&t) => {
                self.data_as_cstr()?.to_str().ok().map(|s| s.to_string())
            }

            // Handle MCAST_UVALUE nodes
            MCAST_UVALUE => {
                if let Some(s) = self.data_as_cstr().and_then(|c| c.to_str().ok()) {
                    Some(s.to_string())
                } else if let Some(sub_node) = self.get_sub_node() {
                    // Try to get string from subnode
                    sub_node.to_string()
                } else {
                    None
                }
            }

            // ★ Fix 2: OPD_FCALL — extract name from function call
            MCAST_OPD_FCALL => {
                if let Some(sub) = self.get_sub_node() {
                    for child in sub.iter() {
                        match child.get_type() {
                            MCAST_NAME => {
                                if let Some(name_inner) = child.get_sub_node() {
                                    return name_inner.to_string();
                                }
                            }
                            MCAST_ID | MCAST_IDA => {
                                return child.to_string();
                            }
                            _ => {}
                        }
                    }
                }
                None
            }

            // ★ Fix 2: OPD_LEAD — placeholder
            MCAST_OPD_USCORE => Some("_".to_string()),

            // For any other node type, try to handle it gracefully
            // instead of logging an error immediately
            _ => {
                // Try to get string representation from subnodes
                if let Some(sub) = self.get_sub_node() {
                    if let Some(s) = sub.to_string() {
                        return Some(s);
                    }
                }

                // Try to get data if available
                if let Some(s) = self.data_as_cstr().and_then(|c| c.to_str().ok()) {
                    return Some(s.to_string());
                }

                // Final fallback: return a string representation of the node type
                mcc_dbg!(
                    "parse::ast",
                    "DEBUG: Unknown node type {} at pos {}",
                    unsafe { (*self.ptr).type_ },
                    self.get_pos()
                );
                Some(format!("<node_type_{}>", unsafe { (*self.ptr).type_ }))
            }
        }
    }

    /// sub MCAST_ID / MCAST_IDS node list to String Vec
    pub fn subs_to_string_vec(&self) -> Option<Vec<String>> {
        if let Some(sub_nodes) = self.get_sub_node() {
            let nodes = sub_nodes
                .iter()
                .map(|each_opdc| McIds::new(&each_opdc))
                .collect::<Option<Vec<_>>>()?;
            Some(nodes.iter().map(|id: &McIds| id.to_string()).collect())
        } else {
            None
        }
    }

    /// sub MCAST_ID / MCAST_IDS node list to McIds Vec
    pub fn subs_to_mcids_vec(&self) -> Option<Vec<McIds>> {
        if let Some(sub_nodes) = self.get_sub_node() {
            Some(
                sub_nodes
                    .iter()
                    .map(|each_opdc| McIds::new(&each_opdc))
                    .collect::<Option<Vec<_>>>()?,
            )
        } else {
            None
        }
    }

    pub fn to_id_or_ida(&self) -> Vec<String> {
        if self.is_null() {
            return Vec::new();
        }
        let mc_value = unsafe { &*self.get_ptr() };

        match mc_value.type_ {
            MCAST_ID => {
                let Some(cstr) = self.data_as_cstr() else {
                    return Vec::<String>::new();
                };
                let Ok(rust_str) = cstr.to_str() else {
                    return Vec::<String>::new();
                };
                vec![rust_str.to_string()]
            }
            MCAST_IDA => {
                let Some(cstr) = self.data_as_cstr() else {
                    return Vec::<String>::new();
                };
                let Ok(rust_str) = cstr.to_str() else {
                    return Vec::<String>::new();
                };
                // U292 step-1: one IDA bracket parser. This used to run
                // `extract_ida`, a parallel parser whose whole-literal range
                // segments (`[1:4]` kept as one string), PriorMult double
                // brackets and `[a:b:c]` debug assert are all either
                // grammar-illegal or asserted-never-to-leak. The canonical
                // semantics is McIda's (§11.1 declaration-order range
                // expansion, §2.12 escapes, param refs carried literally),
                // the same parser the pins/net/instantiation layers are
                // built on.
                McIda::from(rust_str).expand()
            }
            MCAST_IDS => {
                let mut result = Vec::new();
                let mut current = self.get_sub_node();
                while let Some(node) = current {
                    result.extend(node.to_id_or_ida());
                    current = node.get_next();
                }
                result
            }
            _ => {
                if let Some(sub_node) = self.get_sub_node() {
                    return sub_node.to_id_or_ida();
                }
                Vec::<String>::new()
            }
        }
    }

    pub fn to_id_or_ida_or_num(&self) -> Vec<String> {
        if self.is_null() {
            return Vec::new();
        }

        let mc_value = unsafe { &*self.get_ptr() };

        if mc_value.type_ == MCAST_INT {
            let Some(cstr) = self.data_as_cstr() else {
                return Vec::new();
            };
            let Ok(rust_str) = cstr.to_str() else {
                return Vec::new();
            };
            return vec![rust_str.to_string()];
        }

        self.to_id_or_ida()
    }
}

impl Drop for AstNode {
    fn drop(&mut self) {
        if self.owned {
            unsafe {
                bindings::mcc_free(self.ptr);
            }
        }
    }
}

unsafe impl Send for AstNode {}
unsafe impl Sync for AstNode {}


impl Iterator for AstNodeIter {
    type Item = AstNode;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }

        // Safety check: verify pointer is properly aligned before dereferencing
        // Corrupted pointers from the C parser are gracefully handled here
        // instead of killing the process (which would kill the LSP server).
        let addr = self.current as usize;
        if addr % std::mem::align_of::<McValueFFI>() != 0 {
            tracing::warn!(target: "mcc::ast", addr = %format!("{:#x}", addr), "misaligned pointer, stopping iteration");
            self.current = std::ptr::null_mut();
            return None;
        }

        let current_ptr = self.current;

        // Read the next pointer
        self.current = unsafe {
            let next_ptr = (*current_ptr).next;
            if !next_ptr.is_null() {
                let next_addr = next_ptr as usize;
                let misaligned = next_addr % std::mem::align_of::<McValueFFI>() != 0;
                if next_addr < 0x1000 || misaligned {
                    // Corrupted memory - C parser sometimes writes strings/other fields to .next
                    // pointer
                    if next_addr < 0x1000 {
                        tracing::warn!(target: "mcc::ast", addr = %format!("{:#x}", next_addr), "invalid next pointer, stopping iteration");
                    } else {
                        tracing::warn!(target: "mcc::ast", addr = %format!("{:#x}", next_addr), "misaligned next pointer, stopping iteration");
                    }
                    let node = AstNode::from_ptr(current_ptr);
                    self.current = std::ptr::null_mut();
                    return node;
                }
            }
            next_ptr
        };

        AstNode::from_ptr(current_ptr)
    }
}

/// One IDA bracket parser (U292 step-1): `to_id_or_ida`'s MCAST_IDA arm and
/// the `McIda` text parser must read the same string the same way. These
/// locks parse real source and assert the reads at the AST face, so a
/// re-introduced parallel parser cannot silently fork the semantics again.
#[cfg(test)]
mod ida_unify_tests {
    use super::*;
    use crate::ast::bindings::{self, Frontend};
    use crate::ast::macros::*;
    use crate::db::infra::init::MCC_TEST_PARSE_LOCK;

    /// Parse `src`, walk the whole tree, and return `(text, to_id_or_ida)`
    /// for every MCAST_IDA node, in walk order.
    fn ida_reads(src: &str) -> Vec<(String, Vec<String>)> {
        let csrc = std::ffi::CString::new(src).expect("source has no NUL byte");
        let _guard = MCC_TEST_PARSE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let fe = Frontend::acquire();
        let mut out = Vec::new();
        unsafe {
            // Same reset-load-reset line as `McCode::parse_ast_from_string`:
            // the load leaves parser state behind, so reset once more.
            fe.reset(0);
            let buf = bindings::mcc_load_from_string(csrc.as_ptr(), src.len());
            assert!(!buf.is_null(), "the frontend could not load the source");
            fe.reset(0);
            fe.lex(buf);
            let root = AstNode::new(fe.parse());
            collect_ida(&root, &mut out);
            // The load buffer is a plain malloc'd C string (mirrors fmt.rs),
            // not an AST node: freeing it through `mcc_free` walks it as a
            // mc_value and corrupts the heap.
            libc::free(buf.cast::<libc::c_void>());
        }
        out
    }

    /// Plain recursion over the tree: every child is visited once, through
    /// exactly one of its parent links (sub = first child, next = sibling).
    unsafe fn collect_ida(node: &AstNode, out: &mut Vec<(String, Vec<String>)>) {
        if node.is_null() {
            return;
        }
        if node.is_type(MCAST_IDA) {
            let text = node
                .data_as_cstr()
                .and_then(|c| c.to_str().ok())
                .unwrap_or_default()
                .to_string();
            out.push((text, node.to_id_or_ida()));
        }
        if let Some(sub) = node.get_sub_node() {
            collect_ida(&sub, out);
        }
        if let Some(next) = node.get_next() {
            collect_ida(&next, out);
        }
    }

    fn read_of(src: &str, text: &str) -> Vec<String> {
        ida_reads(src)
            .into_iter()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("no MCAST_IDA node with text `{text}` in {src:?}"))
            .1
    }

    #[test]
    fn ida_unify__numeric_ranges_expand_declaration_order() {
        // §11.1: the glued fat token expands at the AST face the same way it
        // expands everywhere else (R1C1..R2C3). The retired parallel parser
        // kept the whole bracket text and returned one name.
        let src = "component CU() {\n    pins = [ 1:6 = R[1:2]C[1:3] ]\n}\nmodule main {\n}\n";
        assert_eq!(
            read_of(src, "R[1:2]C[1:3]"),
            vec!["R1C1", "R1C2", "R1C3", "R2C1", "R2C2", "R2C3"]
        );
    }

    #[test]
    fn ida_unify__param_ref_stays_one_literal_name() {
        // An unresolvable range stays one name (bracket-free, the McIda
        // spelling): no fabricated members and no bracket text leak.
        let src =
            "component CU(rows::INT) {\n    pins = [ 1 : rows = R[1:rows] ]\n}\nmodule main {\n}\n";
        assert_eq!(read_of(src, "R[1:rows]"), vec!["R1:rows"]);
    }

    #[test]
    fn ida_unify__letter_range_expands_declaration_order() {
        // §11.1 letter ranges (declared order: a:c -> a, b, c).
        let src = "component CU() {\n    pins = [ 1:3 = P[a:c] ]\n}\nmodule main {\n}\n";
        assert_eq!(read_of(src, "P[a:c]"), vec!["Pa", "Pb", "Pc"]);
    }
}
