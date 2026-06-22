// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::ast::c_bindings;
use crate::ast::c_macros::*;
use crate::builder::diagnostic::{dlog_error, Position};
use crate::message::*;
use crate::McIds;
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

    /// Safely read .data field as CStr.
    ///
    /// C parser occasionally emits corrupted AST (e.g. writing string path to
    /// next/sub fields), which would cause .data to be a small integer (e.g. 0x24 / 0x1),
    /// which would pass to `CStr::from_ptr` and cause SIGSEGV.
    /// Here we validate .data is a valid heap pointer before dereferencing.
    fn data_as_cstr(&self) -> Option<&std::ffi::CStr> {
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
            dlog_error(205, self, AST_EMPTY);
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
                    dlog_error(206, self, "Invalid UTF-8 string");
                    return None;
                };
                u32::from_str(rust_str).map_err(|_| "Parse failed").ok()
            }
            _ => {
                dlog_error(207, self, TYPE_MISMATCH);
                None
            }
        }
    }

    /// node of certain types to String
    pub fn to_string(&self) -> Option<String> {
        if self.is_null() {
            dlog_error(201, self, AST_EMPTY);
            return None;
        }
        let mc_value = unsafe { &*self.get_ptr() };

        match mc_value.type_ {
            // === Data nodes (use mc_value.data) ===
            MCAST_STRING | MCAST_INT | MCAST_FLOAT | MCAST_URI_PREFIX | MCAST_URI_VERSION
            | MCAST_ID => match self.data_as_cstr()?.to_str() {
                Ok(s) => Some(s.to_string()),
                Err(e) => {
                    dlog_error(203, self, &format!("UTF-8 encoding error: {e}"));
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
            // In AST, expression `+` (rule 294) creates MCAST_OPD_PLUS (181), NOT MCAST_OPD_PLUS (71).
            // This is used in attribute string concatenation like: description = "text" + param + "text"
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
            MCAST_IOTYPE_IN..MCAST_IOTYPE_NC => {
                self.data_as_cstr()?.to_str().ok().map(|s| s.to_string())
            }

            // Unit value types — e.g. 5V, 100mA, 2.2uH (data nodes)
            t if (MCAST_UVAL_VOLT..=MCAST_UVAL_NOISE).contains(&t) => {
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
                eprintln!(
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
                extract_ida(rust_str)
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
                c_bindings::mcc_free(self.ptr);
            }
        }
    }
}

unsafe impl Send for AstNode {}
unsafe impl Sync for AstNode {}

fn extract_ida(ida: &str) -> Vec<String> {
    #[derive(Debug)]
    enum Segment {
        Single(String),
        Multiple(Vec<String>),
        PriorMult(Vec<String>),
    }

    impl Segment {
        fn size(&self) -> usize {
            match self {
                Segment::Single(_) => 1,
                Segment::Multiple(vec) | Segment::PriorMult(vec) => vec.len(),
            }
        }
    }

    let mut segments: Vec<Segment> = Vec::<Segment>::new();
    let mut priority: Vec<usize> = Vec::new();
    let mut indices_without_priority: Vec<usize> = Vec::new();

    let mut start: usize = 0; // Current segment start (id / slice)
    let mut tmp_segment = Vec::<String>::new(); // [] internal parsed part
    let mut parsing_separator = false; // Whether current character is a separator or space
    let mut in_slice = false; // Whether current character is a slice colon
    let slice_left: usize = 0; // Left value of slice when parsing slice colon part
    let mut end: usize = 0; // Current character end position + 1
    let mut in_prior_mult = false;
    let mut waiting_double_lsquare = false;
    let mut waiting_double_rsquare = false;

    let mut in_bracket = false; // Whether current character is in a bracket
    let mut bracket_start = 0; // Bracket start position

    for (index, ch) in ida.char_indices() {
        match ch {
            '[' => {
                assert!(!waiting_double_rsquare);
                if waiting_double_lsquare {
                    waiting_double_lsquare = false;
                    in_prior_mult = true;
                } else {
                    if start < index {
                        indices_without_priority.push(segments.len());
                        segments.push(Segment::Single(ida[start..index].to_string()));
                    }
                    // Record bracket start position
                    bracket_start = index;
                    in_bracket = true;
                    // start;
                    assert!(tmp_segment.is_empty());
                    parsing_separator = true;
                    assert!(!in_slice);
                    // slice_left;
                    // end;
                    waiting_double_lsquare = true;
                }
            }
            ']' => {
                if waiting_double_rsquare {
                    waiting_double_rsquare = false;
                    in_prior_mult = false;
                    start = index + 1;
                    in_bracket = false;
                } else {
                    if !parsing_separator {
                        parsing_separator = true;
                        end = index;
                    }
                    if in_slice {
                        // Range expression: save the entire expression (including brackets) as a single segment
                        // Example: [1:rows] and [1:cols]
                        let full_range = &ida[bracket_start..index + 1];
                        segments.push(Segment::Single(full_range.to_string()));
                        in_slice = false;
                    } else {
                        tmp_segment.push(ida[start..end].to_string());
                        // For non-range expressions, still use Multiple type
                        if in_prior_mult {
                            priority.push(segments.len());
                            segments.push(Segment::PriorMult(tmp_segment));
                        } else {
                            indices_without_priority.push(segments.len());
                            segments.push(Segment::Multiple(tmp_segment));
                        }
                    }

                    start = index + 1;
                    tmp_segment = Vec::new();
                    waiting_double_rsquare = false;
                    in_bracket = false;
                }
            }
            ',' => {
                assert!(!waiting_double_rsquare);
                if !parsing_separator {
                    parsing_separator = true;
                    end = index;
                }
                if in_slice {
                    // Add right boundary to range expression
                    tmp_segment.push(ida[start..end].to_string());
                    in_slice = false;
                } else {
                    tmp_segment.push(ida[start..end].to_string());
                }
            }
            ':' => {
                assert!(!waiting_double_rsquare);
                assert!(!in_slice);
                if !parsing_separator {
                    parsing_separator = true;
                    end = index;
                }
                // Range expression
                // Example: [1:rows] and [1:cols]
                // Save left boundary as string
                tmp_segment.push(ida[start..end].to_string());
                tmp_segment.push(":".to_string());
                in_slice = true;
            }
            ' ' | '\t' => {
                assert!(!waiting_double_rsquare);
                if waiting_double_lsquare {
                    waiting_double_lsquare = false;
                }
                if !parsing_separator {
                    parsing_separator = true;
                    end = index;
                }
            }
            _ => {
                assert!(!waiting_double_rsquare);
                if waiting_double_lsquare {
                    waiting_double_lsquare = false;
                }
                if parsing_separator {
                    parsing_separator = false;
                    start = index;
                } else {
                    // do nothing
                }
            }
        }
    }

    // Process remaining part of string
    if start < ida.len() {
        indices_without_priority.push(segments.len());
        segments.push(Segment::Single(ida[start..].to_string()));
    }

    priority.extend(indices_without_priority);

    // eprintln!("IDA segments = {:?}", segments);

    let mut result = Vec::new();
    let mut seg_indices = vec![0; segments.len()];

    loop {
        let mut current = String::new();

        for (i, seg) in segments.iter().enumerate() {
            match seg {
                Segment::Single(id) => current.push_str(id),
                Segment::Multiple(vec) | Segment::PriorMult(vec) => {
                    current.push_str(&vec[seg_indices[i]])
                }
            }
        }
        result.push(current);

        // Increment indices
        let mut carry = true;
        for i in priority.iter().rev() {
            if carry {
                seg_indices[*i] += 1;
                if seg_indices[*i] >= segments[*i].size() {
                    seg_indices[*i] = 0;
                    carry = true;
                } else {
                    carry = false;
                    break;
                }
            }
        }

        // If all indices roll over to 0, we have exhausted all combinations of segments
        if carry {
            break;
        }
    }

    // eprintln!("IDA extracted. Result:");
    //for each in result.iter() {
    //    eprintln!("{}", each);
    //}

    result
}

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
                    // Corrupted memory - C parser sometimes writes strings/other fields to .next pointer
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
