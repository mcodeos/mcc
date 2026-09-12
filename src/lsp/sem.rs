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
    lapper: &crate::ast::sem::SymbolRangeLapper,
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
                use crate::ast::sem::SymbolKind;
                if interval.val.kind == SymbolKind::ClassDef as u8 {
                    return 3; // CLASS
                }
                // Enum heads register as EnumDef (see lapper_global_classes) but
                // are still class-like declarations — highlight them as CLASS.
                if interval.val.kind == SymbolKind::EnumDef as u8 {
                    return 3; // CLASS
                }
                if interval.val.kind == SymbolKind::ClassRef as u8 {
                    return 2; // TYPE
                }
                if interval.val.kind == SymbolKind::InstDef as u8 {
                    return 4; // FUNCTION
                }
                if interval.val.kind == SymbolKind::InstRef as u8 {
                    return 9; // VARIABLE
                }
            }
        }
        // The interval was found but no kind matched (e.g. a symbol entry that
        // is neither class nor instance) — keep the lexer classification.
        return lex_type;
    }

    // Fallback: language keywords stay as KEYWORD, all other identifiers become VARIABLE
    // The actual keyword check will be done in mcext with the live document content
    lex_type
}

/// Keyword set for reclassifying lexer KEYWORD(13) tokens.
///
/// Must match the fixed list in the VSCode extension (`mcext-main`
/// `semtok.rs::is_mcode_keyword`) exactly — the server is the authority but it
/// must reproduce the plugin's observable behavior. The lexer marks many things
/// as KEYWORD (real MCK_* keywords, MCU_* units, and plain MCTP_ID/MCTP_IDA
/// identifiers); only this list keeps the KEYWORD color, everything else falls
/// back to VARIABLE(9). Note this deliberately mirrors the plugin's set: the
/// type/unit words `hex` and the uppercase physical units (VOLT/AMP/...) are
/// NOT included, while `bool`/`true`/`false` ARE (matching the plugin).
const LANGUAGE_KEYWORDS: &[&str] = &[
    "anl",
    "as",
    "bool",
    "component",
    "else",
    "enum",
    "false",
    "float",
    "func",
    "if",
    "in",
    "int",
    "interface",
    "io",
    "module",
    "nc",
    "out",
    "pins",
    "ps",
    "pub",
    "return",
    "role",
    "string",
    "this",
    "true",
    "use",
];

/// True when `text` is a lexer KEYWORD-class token that should keep the
/// KEYWORD(13) color. Only the shared keyword list (mirroring the plugin) keeps
/// it; any other identifier becomes a variable.
fn is_lexer_keyword(text: &str) -> bool {
    LANGUAGE_KEYWORDS.contains(&text)
}

/// Split a lexer comment token (type 101, `/* ... */`) into one token per line
/// so consumers that paint per-line (mcc-cli's `.tok-16`, mcext's COMMENT) see
/// the whole comment. Returns `None` when the comment spans a single line.
fn split_multiline_comment(
    text: &str,
    position: usize,
    length: usize,
) -> Option<Vec<(i16, usize, usize)>> {
    let start = position;
    let end = position + length;
    if start >= text.len() || end > text.len() || length == 0 {
        return None;
    }
    let slice = &text[start..end];
    // Count newlines; a single-line comment has none.
    let newlines = slice.bytes().filter(|&b| b == b'\n').count();
    if newlines == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(newlines + 1);
    let mut line_start = start;
    for (i, b) in slice.bytes().enumerate() {
        if b == b'\n' {
            // Emit the line up to (not including) the newline.
            if i > line_start - start {
                out.push((16, line_start, i - (line_start - start)));
            }
            line_start = start + i + 1;
        }
    }
    // Trailing content after the last newline (may be empty).
    if line_start < end {
        out.push((16, line_start, end - line_start));
    }
    Some(out)
}

/// Reclassify a raw lexer token against the document text and symbol lapper.
///
/// Returns the semantic type and, for multi-line comments, the per-line
/// replacements (positions/lengths are byte offsets into `text`).
fn classify_token(
    text: &str,
    lex_type: i16,
    position: usize,
    length: usize,
    lapper: &crate::ast::sem::SymbolRangeLapper,
) -> (i16, Option<Vec<(i16, usize, usize)>>) {
    // Multi-line comments are not single tokens for per-line consumers.
    if lex_type == 101 {
        if let Some(lines) = split_multiline_comment(text, position, length) {
            return (16, Some(lines));
        }
        return (16, None);
    }
    // Only re-classify identifiers (lexer marks them as KEYWORD=13 or NONE=255)
    if lex_type != 13 && lex_type != 255 {
        return (lex_type, None);
    }

    // Symbol lapper wins when the identifier is a known class/instance.
    let sem_type = classify_token_by_symbol(lex_type, position, length, lapper);
    if sem_type != lex_type {
        return (sem_type, None);
    }

    // Fall back to the text-based keyword/unit check. The lexer reports KEYWORD
    // for plain identifiers too (MCTP_ID/MCTP_IDA), so anything that is not a
    // real keyword or unit becomes a variable.
    let word = text.get(position..position + length);
    match word {
        Some(w) if is_lexer_keyword(w) => (13, None),
        Some(_) => (9, None),
        None => (lex_type, None),
    }
}

pub fn try_lookup_sem(candidates: &[McURI]) -> Option<Value> {
    let ds = crate::definition_space();
    for mc_uri in candidates {
        if let Some(mcfile) = ds.source_file_tolerant(mc_uri) {
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
                .unwrap_or_else(|| crate::ast::sem::SymbolRangeLapper::new(vec![]));

            // Re-classify tokens using symbol lapper + keyword/unit check.
            // Multi-line comments (lexer type 101) are split per line so
            // per-line consumers (mcc-cli `.tok-16`, mcext COMMENT) paint the
            // whole comment, not just the first line.
            let text_for_classify = mcfile.content.clone();
            let mut tokens: Vec<serde_json::Value> = Vec::new();
            for (lex_type, position, length) in &raw_tokens {
                let (sem_type, replacements) = classify_token(
                    &text_for_classify,
                    *lex_type,
                    *position as usize,
                    *length as usize,
                    &symbols,
                );
                match replacements {
                    Some(lines) => {
                        for (t, p, l) in lines {
                            tokens.push(json!({ "type": t, "position": p, "length": l }));
                        }
                    }
                    None => {
                        tokens.push(json!({
                            "type": sem_type,
                            "position": position,
                            "length": length,
                        }));
                    }
                }
            }

            // ★ §7.6: Stable result_id for mcext dedup.
            // Hash of (token_count, total_length, first_token_pos, last_token_pos)
            // so content-identical responses skip symbol rebuilding.
            let result_id = if tokens.is_empty() {
                None
            } else {
                use std::hash::{Hash, Hasher};
                let count = tokens.len();
                let first_pos = tokens[0]
                    .get("position")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let last_pos = tokens
                    .last()
                    .and_then(|v| v.get("position").and_then(|v| v.as_i64()))
                    .unwrap_or(0);
                let total_len = raw_tokens
                    .iter()
                    .map(|(_, _, len)| *len as i64)
                    .sum::<i64>();
                let mut h = std::collections::hash_map::DefaultHasher::new();
                count.hash(&mut h);
                total_len.hash(&mut h);
                first_pos.hash(&mut h);
                last_pos.hash(&mut h);
                Some(format!("{:x}", h.finish()))
            };

            let symbols = mcfile
                .symbols
                .lock()
                .map(|s| crate::ast::sem::symbol_table_to_json(&s, mc_uri))
                .unwrap_or_else(|_| serde_json::json!({}));

            // ★ §7.6: Affected files via reverse_deps — files that `use` this one
            let affected: Vec<String> = crate::definition_space()
                .reverse_deps(mc_uri)
                .unwrap_or_default();

            return Some(json!({
                "tokens": tokens,
                "symbols": symbols,
                "result_id": result_id,
                "affected_uris": affected,
            }));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_lapper() -> crate::ast::sem::SymbolRangeLapper {
        crate::ast::sem::SymbolRangeLapper::new(vec![])
    }

    #[test]
    fn keyword_kept_as_keyword() {
        let text = "module main";
        let (t, _) = classify_token(text, 13, 0, 6, &empty_lapper());
        assert_eq!(t, 13, "module is a real keyword");
    }

    #[test]
    fn unit_identifier_becomes_variable() {
        // Mirror the plugin: uppercase physical units (VOLT) are NOT keywords
        // there, so the server must also downgrade them to VARIABLE.
        let text = "component C { v: VOLT }";
        let start = text.find("VOLT").unwrap();
        let (t, _) = classify_token(text, 13, start, 4, &empty_lapper());
        assert_eq!(t, 9, "VOLT is not in the plugin keyword list -> VARIABLE");
    }

    #[test]
    fn bool_true_false_kept_as_keyword() {
        // Mirror the plugin: bool/true/false ARE in its keyword list even though
        // the lexer has no MCK_* for them, so the server keeps them keyword.
        for word in ["bool", "true", "false"] {
            let text = format!("{word} x;");
            let (t, _) = classify_token(&text, 13, 0, word.len(), &empty_lapper());
            assert_eq!(t, 13, "{word} must stay keyword-colored (matches plugin)");
        }
    }

    #[test]
    fn type_word_kept_as_keyword() {
        // int/float/string are in the plugin keyword list; uppercase INT is not.
        let text = "io int x; io INT y;";
        let start = text.find("int").unwrap();
        let (t, _) = classify_token(text, 13, start, 3, &empty_lapper());
        assert_eq!(
            t, 13,
            "lowercase `int` is in the plugin list, stays keyword-colored"
        );
        let upper = text.find("INT").unwrap();
        let (t, _) = classify_token(text, 13, upper, 3, &empty_lapper());
        assert_eq!(
            t, 9,
            "uppercase `INT` is a plain identifier, becomes a variable"
        );
    }

    #[test]
    fn plain_identifier_becomes_variable() {
        let text = "let foo = 1";
        let (t, _) = classify_token(text, 13, 4, 3, &empty_lapper());
        assert_eq!(t, 9, "foo is not a keyword/unit -> VARIABLE");
    }

    #[test]
    fn reserved_word_stays_keyword() {
        // `nc` is a reserved keyword even though it could be mistaken for an id.
        let text = "net nc";
        let (t, _) = classify_token(text, 13, 4, 2, &empty_lapper());
        assert_eq!(t, 13);
    }

    #[test]
    fn symbol_lapper_wins_over_fallback() {
        // An InstDef symbol must classify as FUNCTION(4), not a variable or
        // keyword, even though `led_on` is not a language keyword.
        use crate::ast::sem::{SymbolKind, SymbolType};
        use rust_lapper::Interval;
        let lapper = crate::ast::sem::SymbolRangeLapper::new(vec![Interval {
            start: 0,
            stop: 6,
            val: SymbolType::new(SymbolKind::InstDef, 1),
        }]);
        let text = "led_on()";
        let (t, r) = classify_token(text, 13, 0, 6, &lapper);
        assert_eq!(t, 4);
        assert!(r.is_none());
    }

    #[test]
    fn single_line_comment_no_split() {
        let text = "// hi";
        let (t, r) = classify_token(text, 101, 0, 5, &empty_lapper());
        assert_eq!(t, 16);
        assert!(r.is_none(), "single-line comment is not split");
    }

    #[test]
    fn multiline_comment_split_per_line() {
        let text = "/* line1\nline2\nline3 */";
        let (t, r) = classify_token(text, 101, 0, text.len(), &empty_lapper());
        assert_eq!(t, 16);
        let lines = r.expect("multi-line comment splits");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], (16, 0, 8)); // "/* line1"
        assert_eq!(lines[1], (16, 9, 5)); // "line2"  (byte 8 is the newline)
        assert_eq!(lines[2], (16, 15, 8)); // "line3 */" (byte 14 is the newline)
                                           // Every piece is a complete comment line in ascending,
                                           // non-overlapping
                                           // order; the newline separators are intentionally left
                                           // out so the spans
                                           // stay per-line (consumers paint per-line, gaps fall
                                           // back to plain).
        let mut prev = 0usize;
        for (_, p, l) in &lines {
            assert!(*p >= prev, "pieces must be ordered and non-overlapping");
            prev = *p + *l;
        }
        assert!(prev <= text.len());
    }

    #[test]
    fn out_of_range_comment_not_split() {
        let text = "abc";
        let (t, r) = classify_token(text, 101, 0, 10, &empty_lapper());
        assert_eq!(t, 16);
        assert!(r.is_none());
    }
}
