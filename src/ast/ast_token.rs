// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// FFI binding for C struct mc_sem_token
#[repr(C)]
pub struct McSemTokenFFI {
    type_: i16,
    pos: i32,
    len: i32,
    next: *mut McSemTokenFFI,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McSemToken {
    pub type_: i16,
    pub position: i32,
    pub length: i32,
}

impl McSemToken {
    unsafe fn new(ffi: &McSemTokenFFI) -> Self {
        Self {
            type_: ffi.type_,
            position: ffi.pos,
            length: ffi.len,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct McSemTokens {
    pub tokens: Vec<McSemToken>,
}

impl McSemTokens {
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    pub unsafe fn parse(&mut self, head: *mut McSemTokenFFI) {
        self.tokens.clear();

        let mut current = head;
        while !current.is_null() {
            // Defensive: C library may return misaligned pointers in some error paths.
            // Validate alignment before dereferencing (McSemTokenFFI is 8-byte aligned on 64-bit).
            if (current as usize) % std::mem::align_of::<McSemTokenFFI>() != 0 {
                tracing::error!(
                    target: "mcc::code",
                    ptr = ?current,
                    "misaligned McSemTokenFFI pointer, aborting parse"
                );
                break;
            }
            let ffi_node = &*current;
            self.tokens.push(McSemToken::new(ffi_node));
            current = ffi_node.next;
        }
    }

    pub fn clear(&mut self) {
        self.tokens.clear();
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &McSemToken> {
        self.tokens.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut McSemToken> {
        self.tokens.iter_mut()
    }

    pub fn into_vec(self) -> Vec<McSemToken> {
        self.tokens
    }
}

// Implement IntoIterator to support direct iteration via for loops
impl<'a> IntoIterator for &'a McSemTokens {
    type Item = &'a McSemToken;
    type IntoIter = std::slice::Iter<'a, McSemToken>;

    fn into_iter(self) -> Self::IntoIter {
        self.tokens.iter()
    }
}

impl IntoIterator for McSemTokens {
    type Item = McSemToken;
    type IntoIter = std::vec::IntoIter<McSemToken>;

    fn into_iter(self) -> Self::IntoIter {
        self.tokens.into_iter()
    }
}
