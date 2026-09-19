// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

// Lock for the offset -> (line, column) conversion the diagnostic layer runs
// on every anchor (CIMP §1 U124).
//
// `Location::new` feeds two offsets into that conversion: the span start, and
// `pos + len`, its end. Neither is guaranteed to be a position the text has.
// There are two ways to miss, and they are not the same miss:
//
//   * past the end of the text -- guarded at both call sites, which fall back
//     to `(1, 1)`;
//   * inside a multi-byte character -- unguarded, because every such offset is
//     `<= len` and the guard only tests `pos > len`. It reaches
//     `line_index::LineIndex::line_col`, whose own documentation says
//     `try_line_col` returns `None` for that case, and `line_col` is written
//     `try_line_col(..).expect("invalid offset")`.
//
// So an anchor whose span starts, or ends, inside a multi-byte character takes
// the process down instead of producing a diagnostic. The offsets come from
// the parser and the evaluator; the consuming side cannot assume they are
// aligned.
//
// Both entries are pinned here, not just the start: `pos + len` is computed by
// this very function from a length the caller chose, so the end offset is the
// likelier of the two to miss.
//
// Every branch that must keep working has a case, because the cheapest way to
// satisfy the diseased ones -- answer `(1, 1)` for everything -- satisfies
// them and breaks the rest. The boundary in the last case is the crate's own:
// an offset equal to the text length is a valid position (the crate tests
// `offset > len`, not `>=`), so a guard tightened to `>=` would regress there.

// Family naming `{family}__{essence}` deliberately doubles the underscore to
// keep the grep-able family token separate (matrix §1 taxonomy).
#![allow(non_snake_case)]

use crate::common;

use mcc::McLocation;

/// A parseable board whose first line carries a two-byte character inside a
/// comment, so nothing but the converter depends on where it sits.
const SOURCE: &str = "// amp\u{b5}F reference\nmodule main\n{\n    io P\n}\n";

const URI: &str = "/mcc/u124-offset-alignment.mc";

/// Byte offset of the two-byte character.
fn char_start() -> u32 {
    SOURCE
        .find('\u{b5}')
        .expect("the fixture holds a two-byte character") as u32
}

/// The text length, which the crate treats as a valid offset.
fn len() -> u32 {
    SOURCE.len() as u32
}

/// Load the fixture, then build an anchor through the layer's own constructor.
/// Going through `Location::new` rather than the converter directly keeps the
/// lock on the path a diagnostic actually takes, including the `pos + len`
/// the constructor computes for the end position.
fn anchor(pos: u32, len: u32) -> McLocation {
    let _lock = common::lock();
    common::reset();

    let uri = URI.to_string();
    mcc::mcc_load_from_string(&uri, SOURCE);

    McLocation::new(uri, pos, len)
}

// -- 1. the diseased branch at the start: an offset inside a multi-byte character --

/// The offset sits on the second byte of a two-byte character, so nothing in
/// the text answers "which line and column is this". The conversion falls back
/// to `(1, 1)` -- the same fallback, at the same level, as an offset past the
/// end.
///
/// Before the fix this call panics: `line_col` unwraps the `None` that
/// `try_line_col` returns for exactly this input.
#[test]
fn sem_offalign__span_start_inside_a_multibyte_char_falls_back() {
    let loc = anchor(char_start() + 1, 0);
    assert_eq!(
        (loc.row, loc.col),
        (1, 1),
        "an unaligned offset is not a position in the text"
    );
}

// -- 2. the diseased branch at the end: the offset this layer computes itself --

/// The start is aligned and the length is not: a one-byte span beginning on
/// the character's first byte ends on its second. The start still resolves to
/// the character's own position -- only the end, which `Location::new`
/// computes as `pos + len`, has no answer.
///
/// This is the likelier of the two entries in practice, since the length is
/// chosen by whoever produced the span rather than derived from the text.
#[test]
fn sem_offalign__span_end_inside_a_multibyte_char_falls_back() {
    let loc = anchor(char_start(), 1);
    assert_eq!(
        (loc.row, loc.col),
        (1, char_start() + 1),
        "the aligned start must still resolve"
    );
    assert_eq!(
        (loc.end_row, loc.end_col),
        (1, 1),
        "an unaligned end is not a position in the text"
    );
}

// -- 3. the twin that must not regress: past the end of the text --

/// The offset is past the last byte. Guarded before the fix and after it, so
/// the fallback here is behaviour this lock preserves rather than one it
/// establishes.
#[test]
fn sem_offalign__offset_past_the_end_keeps_the_same_fallback() {
    let loc = anchor(len() + 1, 0);
    assert_eq!((loc.row, loc.col), (1, 1));
}

// -- 4. the boundary in the crate's own contract --

/// An offset equal to the text length is a position past the final newline,
/// not an error. A rewrite that tightened the guard to `>=` would answer
/// `(1, 1)` here while passing cases 1 and 2.
#[test]
fn sem_offalign__offset_equal_to_the_length_is_a_position() {
    let loc = anchor(len(), 0);
    assert_ne!(
        (loc.row, loc.col),
        (1, 1),
        "the text length is a valid offset, not a fallback"
    );
}

// -- 5. the legal twin: real positions, including right after the character --

/// Both edges of the two-byte character resolve to real positions: its first
/// byte, and the byte just after it. This is what the fallback must not
/// swallow, and it is also what proves the fixture's line index was found at
/// all -- an engine that answered `(1, 1)` everywhere would pass cases 1-3 and
/// fail this one.
///
/// Columns are 1-based and counted in bytes from the start of the line, so the
/// character's first byte is one past its offset and the byte after it is
/// three past.
#[test]
fn sem_offalign__positions_around_a_multibyte_char_resolve() {
    let start = char_start();
    let first = anchor(start, 0);
    assert_eq!((first.row, first.col), (1, start + 1));

    let after = anchor(start + 2, 0);
    assert_eq!((after.row, after.col), (1, start + 3));
}
