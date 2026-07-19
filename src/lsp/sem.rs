// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Semantic tokens and symbols assembly for LSP.
//!
//! Extracted from `rpc/handlers/mod.rs`.

use crate::McURI;
use serde_json::{json, Value};

pub fn classify_token_by_symbol(
    lex_type: i16,
    position: usize,
    length: usize,
    lapper: &crate::ast::ast_semantic::SymbolRangeLapper,
) -> i16 {
    // Only re-classify identifiers (lexer marks them as KEYWORD=13 or NONE=255)
    if lex_type != 13 && lex_type != 255 {
        return lex_type;
    }

    let token_end = position + length;
    let token_start = position;

    // Try symbol lapper
    if lapper.len() > 0 {
        for interval in lapper.iter() {
            let sym_start = interval.start;
            let sym_stop = interval.stop;
            if token_start < sym_stop && token_end > sym_start {
                use crate::ast::ast_semantic::SymbolType;
                if matches!(&interval.val, SymbolType::ClassDefinition(_)) {
                    return 3; // CLASS
                }
                if matches!(&interval.val, SymbolType::DeclareClass(_)) {
                    return 2; // TYPE
                }
                if matches!(&interval.val, SymbolType::DeclareInstance(_)) {
                    return 4; // FUNCTION
                }
                if matches!(&interval.val, SymbolType::InstanceRef(_)) {
                    return 9; // VARIABLE
                }
            }
        }
        return lex_type;
    }

    // Fallback: language keywords stay as KEYWORD, all other identifiers become VARIABLE
    // The actual keyword check will be done in mcext with the live document content
    lex_type
}

pub fn try_lookup_sem(candidates: &[McURI]) -> Option<Value> {
    let binding = crate::db::cmie::tables::WORKSPACE.mcodes.borrow();
    for mc_uri in candidates {
        if let Some(mcfile) = binding.get(mc_uri) {
            // Get raw tokens and symbol lapper for semantic re-classification
            let raw_tokens: Vec<(i16, i32, i32)> = mcfile
                .tokens
                .lock()
                .map(|t: std::sync::MutexGuard<'_, crate::McSemTokens>| {
                    t.iter()
                        .map(|tok| (tok.type_, tok.position, tok.length))
                        .collect()
                })
                .unwrap_or_default();

            let symbols = mcfile
                .symbols
                .lock()
                .ok()
                .map(|s| s.symbol_lapper.clone())
                .unwrap_or_else(|| crate::ast::ast_semantic::SymbolRangeLapper::new(vec![]));

            // Re-classify tokens using symbol lapper
            let tokens: Vec<serde_json::Value> = raw_tokens
                .iter()
                .map(|(lex_type, position, length)| {
                    let sem_type = classify_token_by_symbol(
                        *lex_type,
                        *position as usize,
                        *length as usize,
                        &symbols,
                    );
                    json!({
                        "type": sem_type,
                        "position": position,
                        "length": length,
                    })
                })
                .collect();

            // Compute stable result_id: hash of (token_count, first_pos, last_pos)
            let result_id = if tokens.is_empty() {
                None
            } else {
                let count = tokens.len();
                let first_pos = tokens[0]
                    .get("position")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let last_pos = tokens
                    .last()
                    .and_then(|v| v.get("position").and_then(|v| v.as_i64()))
                    .unwrap_or(0);
                Some(format!("{}-{}-{}", count, first_pos, last_pos))
            };

            let symbols = mcfile
                .symbols
                .lock()
                .map(|s| crate::ast::ast_semantic::symbol_table_to_json(&s, mc_uri))
                .unwrap_or_else(|_| serde_json::json!({}));

            return Some(json!({
                "tokens": tokens,
                "symbols": symbols,
                "result_id": result_id,
            }));
        }
    }
    None
}
