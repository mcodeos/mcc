// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Lexical-safe `.mc` source formatter behind `mcc fmt`.
//!
//! Token text and token order come from the compiler's own lexer, so only the
//! whitespace *between* tokens is ever rewritten, and each rewrite is verified
//! by re-lexing the result. `fmt-design.md` carries the rule table and the
//! invariants; this module implements them. The C lexer is process-global, so
//! this is a single-threaded API like every other parse path in the crate.

use crate::ast::bindings;
use crate::ast::token::McLexTokenFFI;

/// Spaces per nesting level.
const INDENT: usize = 4;

/// Marker for a piece that is a comment rather than a lexer token. The lexer's
/// own comment type (`MCC_TK_COMMENT`, `astdef.h`).
const TK_COMMENT: i16 = 16;

/// Symbols that may be a prefix operator, a sign or a unit marker. A token's
/// text cannot tell a prefix use from a binary one, so where the author glued
/// one to its operand the spacing is kept.
const GLUED_PREFIX: &[&str] = &["-", "+", "!", "~", "'"];

/// Operators written with one space on both sides.
const SPACED: &[&str] = &[
    "=", "==", "!=", "<", ">", "<=", ">=", "+=", "->", "<-", "=>", "|", "&", "^", "*",
];

/// Tokens whose spacing is the author's call: the lexer has lookahead rules, and
/// a `use` path is sliced up to its first whitespace, so `/` must not gain one.
const AUTHOR_SPACED: &[&str] = &["::", ".", ":", "@", "/"];

/// Punctuation that may directly follow a value, so a `(` after one of these is
/// separated rather than glued. A closed list of grammar terminals, not a guess
/// read off a name.
const PUNCT: &[&str] = &[
    "(", ")", "[", "]", "{", "}", ",", ";", ".", ":", "::", "@", "=", "+=", "==", "!=", "<", ">",
    "<=", ">=", "->", "<-", "=>", "+", "-", "&", "|", "*", "/", "^", "'", "~", "_",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Other,
    LineComment,
    BlockComment,
}

/// One token or comment, as the lexer's stream presents it.
#[derive(Clone)]
struct Piece {
    /// Lexer terminal id of the token this piece came from, or `TK_COMMENT`.
    tid: i16,
    text: String,
    kind: Kind,
    /// Line breaks between the previous piece and this one.
    breaks: usize,
    /// No whitespace at all between the previous piece and this one.
    glued: bool,
    /// Byte column this piece starts at, counted from the source line's first
    /// byte. Kept for comment-only lines, whose indentation is the author's.
    col: usize,
}

/// A physical line of the input, as its tokens.
#[derive(Default)]
struct Line {
    toks: Vec<Tok>,
    /// See [`Piece::col`]: the column the line's first token starts at.
    col: usize,
}

#[derive(Clone)]
struct Tok {
    text: String,
    kind: Kind,
    /// The author wrote this token with no whitespace before it.
    glued: bool,
}

/// One line of the output.
#[derive(Default)]
struct RLine {
    blank: bool,
    indent: usize,
    code: String,
    comment: Option<String>,
    /// Byte offset of the single top-level `=` in `code`, when alignable.
    eq_off: Option<usize>,
    /// Innermost bracket scope the line leaves us in; trailing comments align
    /// per scope.
    scope: u32,
}

/// Format `content`. `Err` means the file was refused and must not be written.
pub fn format_text(content: &str) -> Result<String, String> {
    let (bom, body) = match content.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", content),
    };
    if body.trim().is_empty() {
        return Ok(content.to_string());
    }
    let out = format_once(body)?;
    // I2: the token stream must survive the rewrite untouched.
    if fingerprint(body)? != fingerprint(&out)? {
        return Err("refused: the rewrite changed the token stream".to_string());
    }
    // I3: formatting is a fixed point. Only worth paying for when it moved.
    if out != body && format_once(&out)? != out {
        return Err("refused: formatting is not idempotent".to_string());
    }
    Ok(format!("{bom}{out}"))
}

fn format_once(body: &str) -> Result<String, String> {
    let mut lines = build_lines(body)?;
    // R5/R12, R6 and R11 each read a line structure another one writes -- a
    // folded group decides which line carries a declaration keyword for R12, and
    // a deleted blank decides whether R11 sees the next line at all -- so the
    // three run to a fixed point. Every pass that reports a change has merged or
    // dropped a line, so the loop terminates, and stopping when nothing changes
    // is what makes the whole rewrite its own fixed point (I3).
    loop {
        let mut changed = normalize_blank_lines(&mut lines);
        changed |= join_open_braces(&mut lines);
        changed |= join_wrapped_lines(&mut lines);
        if !changed {
            break;
        }
    }
    render(&lines)
}

/// Token starts as the lexer sees them. Its matches are contiguous and cover
/// every byte, so consecutive starts delimit each token's full span -- the token
/// text plus whatever whitespace or comment its rule swallowed on either side.
fn token_starts(content: &str) -> Result<Vec<(i16, usize)>, String> {
    let src = std::ffi::CString::new(content)
        .map_err(|_| "refused: source contains a NUL byte".to_string())?;
    let mut out: Vec<(i16, usize)> = Vec::new();
    // The C frontend is a process-wide singleton: this session owns the lexer's
    // token list until it ends, so it holds the lock that says so.
    let fe = bindings::Frontend::acquire();
    unsafe {
        fe.reset(0);
        let buf = bindings::mcc_load_from_string(src.as_ptr() as *const i8, content.len());
        if buf.is_null() {
            return Err("refused: the lexer could not load the source".to_string());
        }
        // Mirrors `McCode::parse_ast_from_string`: the load leaves parser state
        // behind, so reset once more before lexing.
        fe.reset(0);
        fe.lex(buf);
        let mut cur = fe.get_tokens();
        while !cur.is_null() {
            if (cur as usize) % std::mem::align_of::<McLexTokenFFI>() != 0 {
                libc::free(buf as *mut libc::c_void);
                return Err("refused: misaligned token pointer from the lexer".to_string());
            }
            let t = &*cur;
            let start = t.tpos as usize;
            if start >= content.len() {
                break;
            }
            if out.last().is_some_and(|(_, prev)| *prev >= start) {
                libc::free(buf as *mut libc::c_void);
                return Err("refused: the lexer's token spans are not in order".to_string());
            }
            out.push((t.tid, start));
            cur = t.next;
        }
        libc::free(buf as *mut libc::c_void);
    }
    if out.is_empty() {
        return Err("refused: the lexer produced no tokens".to_string());
    }
    Ok(out)
}

/// The source as a flat sequence of tokens and comments, each carrying its line
/// break count and whether the author glued it to the piece before it.
///
/// A token owns the bytes of its own match, and the bytes up to the next
/// token's match are whitespace or skipped comments; both halves are atomized
/// together, so nothing in the file is left unaccounted for.
fn pieces(content: &str) -> Result<Vec<Piece>, String> {
    let starts = token_starts(content)?;
    let mut out: Vec<Piece> = Vec::new();
    let mut breaks = 0usize;
    let mut saw_ws = false;
    let mut col = 0usize;
    let mut prev = 0usize;
    for (i, &(tid, start)) in starts.iter().enumerate() {
        let end = starts
            .get(i + 1)
            .map(|(_, next)| *next)
            .unwrap_or(content.len());
        if start > prev {
            push_region(
                &content[prev..start],
                None,
                &mut out,
                &mut breaks,
                &mut saw_ws,
                &mut col,
            )?;
        }
        push_region(
            &content[start..end],
            Some(tid),
            &mut out,
            &mut breaks,
            &mut saw_ws,
            &mut col,
        )?;
        prev = end;
    }
    Ok(out)
}

/// Turn one byte region into pieces. `tid` is the token whose match opens the
/// region, or `None` for a region before the first token (which can only hold
/// whitespace and comments).
fn push_region(
    region: &str,
    tid: Option<i16>,
    out: &mut Vec<Piece>,
    breaks: &mut usize,
    saw_ws: &mut bool,
    col: &mut usize,
) -> Result<(), String> {
    for atom in atomize(region)? {
        let (tid, text, kind) = match atom {
            Atom::Ws(run) => {
                *breaks += run.matches('\n').count();
                *saw_ws = true;
                advance_col(&run, col);
                continue;
            }
            Atom::Comment(text, kind) => (TK_COMMENT, text, kind),
            Atom::Code(text) => match tid {
                Some(tid) => (tid, text, Kind::Other),
                None => {
                    return Err("refused: a byte the lexer did not report as a token".to_string())
                }
            },
        };
        let starts_at = *col;
        advance_col(&text, col);
        out.push(Piece {
            tid,
            text,
            kind,
            breaks: *breaks,
            glued: *breaks == 0 && !*saw_ws,
            col: starts_at,
        });
        *breaks = 0;
        *saw_ws = false;
    }
    Ok(())
}

/// Move the running column past `text`, resetting it after the last newline.
fn advance_col(text: &str, col: &mut usize) {
    *col = match text.rfind('\n') {
        Some(k) => text.len() - k - 1,
        None => *col + text.len(),
    };
}

/// Non-whitespace `(tid, text)` sequence. Two rewrites of one source are
/// equivalent exactly when their lexer never saw a different token.
fn fingerprint(content: &str) -> Result<Vec<(i16, String)>, String> {
    Ok(pieces(content)?
        .into_iter()
        .map(|p| (p.tid, p.text))
        .collect())
}

enum Atom {
    Ws(String),
    Comment(String, Kind),
    Code(String),
}

/// Split one token region into the whitespace, comments and code it contains.
///
/// A region opens with one lexer match and runs on to the next match, so its
/// code may be several runs: a match can hold internal whitespace (`cm[4][open,
/// close]` is a single id token), and skipped bytes between two matches are
/// whitespace or comments. Runs of a single match are kept verbatim -- the
/// whitespace inside them belongs to the token and cannot be re-spaced.
fn atomize(region: &str) -> Result<Vec<Atom>, String> {
    let bytes = region.as_bytes();
    let mut out: Vec<Atom> = Vec::new();
    let mut run: Option<usize> = None;
    let mut first_code = true;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            flush_code(region, &mut run, i, &mut out, &mut first_code)?;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            out.push(Atom::Ws(region[start..i].to_string()));
        } else if b == b'"' {
            run.get_or_insert(i);
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                let closes = bytes[i] == b'"';
                i += 1;
                if closes {
                    break;
                }
            }
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            flush_code(region, &mut run, i, &mut out, &mut first_code)?;
            let mut end = i;
            while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
                end += 1;
            }
            out.push(Atom::Comment(region[i..end].to_string(), Kind::LineComment));
            i = end;
        } else if b == b'#' {
            flush_code(region, &mut run, i, &mut out, &mut first_code)?;
            let mut end = i;
            while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
                end += 1;
            }
            out.push(Atom::Comment(region[i..end].to_string(), Kind::LineComment));
            i = end;
        } else if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            flush_code(region, &mut run, i, &mut out, &mut first_code)?;
            let start = i;
            i += 2;
            while i < bytes.len() {
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    i += 2;
                    break;
                }
                i += 1;
            }
            out.push(Atom::Comment(
                region[start..i].to_string(),
                Kind::BlockComment,
            ));
        } else {
            run.get_or_insert(i);
            i += 1;
        }
    }
    flush_code(region, &mut run, bytes.len(), &mut out, &mut first_code)?;
    Ok(out)
}

fn flush_code(
    region: &str,
    run: &mut Option<usize>,
    end: usize,
    out: &mut Vec<Atom>,
    first_code: &mut bool,
) -> Result<(), String> {
    let Some(start) = run.take() else {
        return Ok(());
    };
    if *first_code {
        *first_code = false;
        out.push(Atom::Code(region[start..end].to_string()));
        return Ok(());
    }
    // A second code run on the same line belongs to the same lexer match: a
    // match can carry internal whitespace (`cm[4][open, close]` is one id
    // token, but so is `;}` after a statement). Its text is what the parser
    // sees, so the bytes between the runs are kept verbatim. A line break or a
    // comment between them means the match merely absorbed leading layout
    // (`;\n// note\n}` is one brace token), and the runs are laid out apart.
    let mut sep = String::new();
    let mut mergeable = false;
    for atom in out.iter().rev() {
        match atom {
            Atom::Ws(ws) if !ws.contains('\n') => sep.insert_str(0, ws),
            Atom::Code(_) => {
                mergeable = true;
                break;
            }
            _ => break,
        }
    }
    if !mergeable {
        out.push(Atom::Code(region[start..end].to_string()));
        return Ok(());
    }
    while matches!(out.last(), Some(Atom::Ws(_))) {
        out.pop();
    }
    let Some(Atom::Code(mut prev)) = out.pop() else {
        return Err("refused: a token's span starts mid-token".to_string());
    };
    prev.push_str(&sep);
    prev.push_str(&region[start..end]);
    out.push(Atom::Code(prev));
    Ok(())
}

/// Split the stream into physical lines. A line comment always runs to the end
/// of its line, so it can only be a line's last token.
fn build_lines(content: &str) -> Result<Vec<Line>, String> {
    let mut lines: Vec<Line> = vec![Line::default()];
    for piece in pieces(content)? {
        for _ in 0..piece.breaks.min(2) {
            lines.push(Line::default());
        }
        let line = lines.last_mut().expect("at least one line");
        if line.toks.is_empty() {
            line.col = piece.col;
        }
        line.toks.push(Tok {
            text: piece.text,
            kind: piece.kind,
            glued: piece.glued,
        });
    }
    for line in &lines {
        let n = line.toks.len();
        if line
            .toks
            .iter()
            .enumerate()
            .any(|(i, t)| t.kind == Kind::LineComment && i + 1 < n)
        {
            return Err("refused: a line comment is not the last token on its line".to_string());
        }
    }
    Ok(lines)
}

fn trailing_comment_count(toks: &[Tok]) -> usize {
    match toks.last() {
        Some(t) if t.kind == Kind::LineComment => 1,
        _ => 0,
    }
}

/// The line's tokens with a trailing line comment dropped.
fn code_toks(toks: &[Tok]) -> &[Tok] {
    &toks[..toks.len() - trailing_comment_count(toks)]
}

fn ends_with_opener(line: &Line) -> bool {
    code_toks(&line.toks)
        .last()
        .is_some_and(|t| ends_in(&t.text, "{[("))
}

/// R5: fold blank runs to one, drop the ones a block delimiter already fences
/// off, drop leading/trailing ones, and drop the ones R12 reads as a break the
/// author did not mean (design §4 R12). Returns true when it dropped a line.
fn normalize_blank_lines(lines: &mut Vec<Line>) -> bool {
    let mut src: Vec<Line> = std::mem::take(lines);
    let decl: Vec<bool> = src.iter().map(is_declaration).collect();
    let head = head_run(&src, &decl);
    let keyword: Vec<bool> = src.iter().map(starts_a_declaration).collect();

    let mut out: Vec<Line> = Vec::with_capacity(src.len());
    let mut prev_non_blank: Option<usize> = None;
    for i in 0..src.len() {
        if src[i].toks.is_empty() {
            let Some(prev) = prev_non_blank else { continue };
            let joins_the_run = (i + 1..src.len())
                .find(|&j| !src[j].toks.is_empty())
                .is_some_and(|j| {
                    decl[prev]
                        && decl[j]
                        && ((keyword[prev] && keyword[j]) || (head[prev] && head[j]))
                });
            if joins_the_run {
                continue;
            }
            let Some(prev_line) = out.last() else {
                continue;
            };
            if prev_line.toks.is_empty() || ends_with_opener(prev_line) {
                continue;
            }
            out.push(std::mem::take(&mut src[i]));
            continue;
        }
        prev_non_blank = Some(i);
        let line = std::mem::take(&mut src[i]);
        let first = code_toks(&line.toks).first().map(|t| t.text.as_str());
        let fenced = first.is_some_and(|t| starts_in(t, "}]){"));
        if fenced && out.last().is_some_and(|l| l.toks.is_empty()) {
            out.pop();
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.toks.is_empty()) {
        out.pop();
    }
    let changed = out.len() != src.len();
    *lines = out;
    changed
}

/// The grammar's declaration words: the ones that open a pin row (`in [3, 4] =
/// XTAL`), a conductor (`conduit GNDA @role(quiet)`), a rail, a domain, or the
/// pin block. A closed list of grammar terminals, not a list of user names.
const DECL_KEYWORDS: &[&str] = &[
    "anl", "conduit", "domain", "in", "io", "label", "nc", "out", "pins", "psbi", "psnk", "psrc",
    "rail", "ref",
];

/// R12: a line that declares something rather than stating a connection. A
/// top-level connection operator (`a -> b`, `x + y`) makes a line a statement;
/// a member assignment (`partno = "..."`), a pin row, a group header and an
/// instance (`comp.sub uC`) are all declarations. A comment or a lone brace is
/// neither, so the run of declarations a block opens with ends there.
fn is_declaration(line: &Line) -> bool {
    let code = code_toks(&line.toks);
    let Some(first) = code.first() else {
        return false;
    };
    if first.kind != Kind::Other {
        return false;
    }
    if code.len() == 1 && (is_opener(&first.text) || is_closer(&first.text)) {
        return false;
    }
    let mut depth = 0i32;
    for t in code {
        if depth == 0 && RIGHT_OPERAND.contains(&t.text.as_str()) {
            return false;
        }
        depth += bracket_delta(&t.text);
    }
    true
}

fn starts_a_declaration(line: &Line) -> bool {
    code_toks(&line.toks)
        .first()
        .is_some_and(|t| t.kind == Kind::Other && DECL_KEYWORDS.contains(&t.text.as_str()))
}

/// R12: which lines belong to the declaration run a block opens with -- the ones
/// between `{` and the first statement or comment. They read as one list of what
/// the block holds, so the blank lines between them are not paragraph breaks. A
/// declaration that leaves a group open (`pins = [`) ends the run: the lines
/// inside that group are its members, laid out on their own.
fn head_run(lines: &[Line], decl: &[bool]) -> Vec<bool> {
    let mut head = vec![false; lines.len()];
    let mut open = false;
    for (i, line) in lines.iter().enumerate() {
        if line.toks.is_empty() {
            continue;
        }
        if !open {
            open = code_toks(&line.toks).last().is_some_and(|t| t.text == "{");
            continue;
        }
        head[i] = decl[i];
        open = decl[i] && !ends_with_opener(line);
    }
    head
}

/// R6: an opening brace alone on its line joins the previous line (K&R).
/// Returns true when it merged one.
fn join_open_braces(lines: &mut Vec<Line>) -> bool {
    let mut i = 0;
    let mut changed = false;
    while i < lines.len() {
        let opens = code_toks(&lines[i].toks)
            .first()
            .is_some_and(|t| t.text == "{");
        if opens && i > 0 && !lines[i - 1].toks.is_empty() {
            // Joining behind a line comment would comment the brace out.
            let prev_ends_in_comment = lines[i - 1]
                .toks
                .last()
                .is_some_and(|t| t.kind == Kind::LineComment);
            if !prev_ends_in_comment {
                let brace = lines[i].toks.remove(0);
                lines[i - 1].toks.push(brace);
                changed = true;
                if lines[i].toks.is_empty() {
                    lines.remove(i);
                    continue;
                }
            }
        }
        i += 1;
    }
    changed
}

/// Operators that leave a line waiting for its right operand. [`CONTINUATION`]
/// and this list are the same question asked from either side of a break: R10
/// reads the operator that *starts* a line, R11 the one that *ends* it.
const RIGHT_OPERAND: &[&str] = &["->", "<-", "=>", "+", "-", "|", "&", "*"];

/// Longest line R11 may join. An author breaks a group that would not fit, so a
/// group that would not fit when folded keeps its lines.
const MAX_WIDTH: usize = 100;

/// Tokens that keep the expression going once the group it was in has closed:
/// `f(a) -> b`, `f(a)[i]`, `X.setup(b).Y`. Anything else closes the construct
/// with the group.
const CHAINED: &[&str] = &[".", "::", "[", "("];

/// R11: fold back a line break the author wrote inside one construct.
///
/// R10 only moves indentation; R11 removes the break, so it fires only where the
/// line itself says it is unfinished -- and never behind a line comment (code
/// placed after one is commented out) nor in front of a token the lexer's
/// lookahead reads (design §3.1.2). Each fold is retried on the line it produced,
/// so one pass reaches the fixed point I3 demands. Returns true when it merged
/// a line.
fn join_wrapped_lines(lines: &mut Vec<Line>) -> bool {
    let mut depth = 0usize;
    let mut i = 0;
    let mut changed = false;
    while i < lines.len() {
        while let Some(target) = merge_target(lines, i, INDENT * depth) {
            let tail: Vec<Line> = lines.drain(i + 1..=target).collect();
            for line in tail {
                lines[i].toks.extend(line.toks);
            }
            changed = true;
        }
        for t in code_toks(&lines[i].toks) {
            depth = (depth as i32 + bracket_delta(&t.text)).max(0) as usize;
        }
        i += 1;
    }
    changed
}

/// Index of the line `i` folds into, if any. Each limb names the shape left open
/// on the line; a trailing comma alone does not, because a list of sibling
/// entries and a continued expression look identical without the AST.
fn merge_target(lines: &[Line], i: usize, indent: usize) -> Option<usize> {
    if lines[i]
        .toks
        .last()
        .is_some_and(|t| t.kind == Kind::LineComment)
    {
        return None;
    }
    let code = code_toks(&lines[i].toks);
    let last = code.last()?;
    // R11-a: an assignment still waiting for its right hand side. Blank lines
    // the author left in between are part of the same wait.
    if last.text == "=" || last.text == "+=" {
        let mut j = i + 1;
        while j < lines.len() && lines[j].toks.is_empty() {
            j += 1;
        }
        return (j < lines.len() && joins_here(lines, j)).then_some(j);
    }
    let j = i + 1;
    if j >= lines.len() || lines[j].toks.is_empty() {
        return None;
    }
    // R11-b: a comma that left a bracket open separates items of a list the line
    // has not finished, so the list folds up to the line that closes it.
    if last.text == "," && net_delta(code) > 0 {
        return group_target(lines, i, indent);
    }
    let next = code_toks(&lines[j].toks);
    let continues = RIGHT_OPERAND.contains(&last.text.as_str())
        // R11-c: a lone word heads the declaration written below it (`in` /
        // `[3, 4] = XTAL`), which the next line's bracketed group and its
        // top-level `=` identify. A lone block comment is not a word: pulling
        // code up onto it would put that code inside the comment. And a line that
        // opens a group without declaring anything (`[a, b] + c`) is a statement
        // of its own, so folding it would merge two statements.
        || (code.len() == 1
            && last.kind == Kind::Other
            && !is_opener(&last.text)
            && !is_closer(&last.text)
            && next
                .first()
                .is_some_and(|t| t.text == "[" || t.text == "(")
            && declares(next));
    if continues && joins_here(lines, j) {
        return Some(j);
    }
    if opens_a_group(code) {
        return group_target(lines, i, indent);
    }
    None
}

/// R11-d: the line ends inside a group whose opener was glued to its head --
/// the spelling of a call or a group (`UC.power(`, `MIC{P`). A block
/// (`component A {`) and a value list (`pins = [`) are spelled with a space, and
/// an opener an operator introduces (`-> [`, `= (`) is an operand rather than a
/// head.
fn opens_a_group(code: &[Tok]) -> bool {
    let mut open: Vec<usize> = Vec::new();
    for (i, t) in code.iter().enumerate() {
        let delta = bracket_delta(&t.text);
        if delta > 0 {
            open.push(i);
        } else if delta < 0 {
            open.pop();
        }
    }
    let Some(&at) = open.last() else {
        return false;
    };
    let opener = &code[at];
    let Some(head) = at.checked_sub(1).and_then(|n| code.get(n)) else {
        return false;
    };
    opener.glued
        && head.text != "="
        && head.text != "+="
        && !RIGHT_OPERAND.contains(&head.text.as_str())
}

/// Fold the lines from `i` through the one that closes the group the line left
/// open -- R11-d's call (`UC.power(`, `MIC{P`) or R11-b's unfinished list entry
/// (`io [1,`). The fold ends where the group closes and the expression with it;
/// a close followed by an operator or a member access leaves the expression open
/// (`MIC{P, N} -> [...]`). A lone `}` is a block's own line, and a group that
/// would not fit [`MAX_WIDTH`] or that carries a comment before its close keeps
/// its lines (design §7).
fn group_target(lines: &[Line], i: usize, indent: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut width = indent;
    let mut prev: Option<&Tok> = None;
    for (j, line) in lines.iter().enumerate().skip(i) {
        if j > i && !joins_here(lines, j) {
            return None;
        }
        let inner = code_toks(&line.toks);
        if inner.is_empty() {
            return None;
        }
        for (k, t) in inner.iter().enumerate() {
            if let Some(p) = prev {
                width += gap(p, t).len();
            }
            width += t.text.chars().count();
            prev = Some(t);
            if width > MAX_WIDTH {
                return None;
            }
            let delta = bracket_delta(&t.text);
            depth += delta;
            let closed = delta < 0 && depth == 0;
            let chained = inner.get(k + 1).is_some_and(|n| {
                RIGHT_OPERAND.contains(&n.text.as_str()) || CHAINED.contains(&n.text.as_str())
            });
            if closed && !chained {
                // A lone `}` ends a block, and a block owns its lines.
                let block_end = inner.len() == 1 && is_closer(&inner[0].text);
                return (j > i && !block_end).then_some(j);
            }
        }
        // Still inside the group at the end of this line: a trailing comment
        // would swallow everything folded onto it.
        if line.toks.len() != inner.len() {
            return None;
        }
    }
    None
}

/// True when the line below opens with code R7 may re-space: a comment-only line
/// leaves nothing to join, and a token whose spacing decides a lexer match must
/// keep the whitespace the author gave it.
fn joins_here(lines: &[Line], j: usize) -> bool {
    code_toks(&lines[j].toks)
        .first()
        .is_some_and(|t| t.kind != Kind::LineComment && !AUTHOR_SPACED.contains(&t.text.as_str()))
}

fn net_delta(code: &[Tok]) -> i32 {
    code.iter().map(|t| bracket_delta(&t.text)).sum()
}

/// True when the line carries a top-level `=`: the shape of a declaration
/// (`in [3, 4] = XTAL`, `x = 1`) rather than of an expression that merely opens
/// with a group (`[a, b] + c`).
fn declares(code: &[Tok]) -> bool {
    let mut depth = 0i32;
    for t in code {
        if t.text == "=" && depth == 0 {
            return true;
        }
        depth += bracket_delta(&t.text);
    }
    false
}

fn is_opener(text: &str) -> bool {
    starts_in(text, "{[(")
}

/// True when the text's first character is one of `set`.
fn starts_in(text: &str, set: &str) -> bool {
    text.chars().next().is_some_and(|c| set.contains(c))
}

/// True when the text's last character is one of `set`.
fn ends_in(text: &str, set: &str) -> bool {
    text.chars().next_back().is_some_and(|c| set.contains(c))
}

/// Net bracket depth change of one code token, ignoring brackets inside string
/// literals (the lexer hands a whole literal back as one token).
fn bracket_delta(text: &str) -> i32 {
    let mut delta = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' | '[' | '(' if !in_string => delta += 1,
            '}' | ']' | ')' if !in_string => delta -= 1,
            _ => {}
        }
    }
    delta
}

fn is_closer(text: &str) -> bool {
    starts_in(text, "}])")
}

/// Operators no statement can open with, so a line starting with one continues
/// the expression above. A trailing comma says the same thing for a list, but a
/// list of sibling entries looks identical to a continued expression without the
/// AST, so R10 does not read it; R11 reads it only where the line left a bracket
/// open (design §7).
const CONTINUATION: &[&str] = &["|", "-", "+", "->", "<-", "=>"];

fn starts_continuation(code: &[Tok]) -> bool {
    code.first()
        .is_some_and(|t| CONTINUATION.contains(&t.text.as_str()))
}

/// Push/pop one bracket scope id per bracket in `text`, skipping string
/// literals the same way [`bracket_delta`] does. The base scope stays on the
/// stack, so the returned top is never absent.
fn scan_scopes(text: &str, stack: &mut Vec<u32>, next: &mut u32) {
    let mut in_string = false;
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' | '[' | '(' if !in_string => {
                stack.push(*next);
                *next += 1;
            }
            '}' | ']' | ')' if !in_string => {
                if stack.len() > 1 {
                    stack.pop();
                }
            }
            _ => {}
        }
    }
}

/// True for a body-introducing keyword. The lexer hands `else if` back as one
/// token (`MCK_ELSE_IF`) whose text holds whatever whitespace the author wrote
/// between the two words, so the test is on the words.
fn is_control(text: &str) -> bool {
    let mut words = text.split_ascii_whitespace();
    match words.next() {
        Some("if") => words.next().is_none(),
        Some("else") => match words.next() {
            None => true,
            Some("if") => words.next().is_none(),
            Some(_) => false,
        },
        _ => false,
    }
}

/// True when this line opens a body without braces, so the next line is one
/// level deeper.
///
/// The lexer cannot tell a condition from a body, so the shapes the grammar
/// allows are enumerated: a bare condition that is the whole line (`if x == y`,
/// with the body below) or a parenthesized one that closes at the end
/// (`if (x)`). A line carrying a second control word has both branches on it,
/// and a line assigning a value has its body on it.
fn is_braceless_control(toks: &[Tok], assigned: bool) -> bool {
    let code = code_toks(toks);
    let Some(first) = code.first() else {
        return false;
    };
    if first.kind != Kind::Other || !is_control(&first.text) {
        return false;
    }
    if code.last().is_some_and(|t| t.text == "{") {
        return false;
    }
    if code[1..]
        .iter()
        .any(|t| t.kind == Kind::Other && is_control(&t.text))
    {
        return false;
    }
    if assigned {
        return false;
    }
    if code.get(1).is_some_and(|t| t.text == "(") {
        return group_closes_at_end(code, 1);
    }
    true
}

/// True when the group opened at `open` closes on the line's last token.
fn group_closes_at_end(code: &[Tok], open: usize) -> bool {
    let mut depth = 0i32;
    for (i, t) in code.iter().enumerate().skip(open) {
        depth += bracket_delta(&t.text);
        if depth == 0 {
            return i + 1 == code.len();
        }
    }
    true
}

fn render(lines: &[Line]) -> Result<String, String> {
    let mut out: Vec<RLine> = Vec::with_capacity(lines.len());
    let mut depth = 0usize;
    let mut prev_control = false;
    let mut scopes: Vec<u32> = vec![0];
    let mut next_scope = 1u32;
    // RHS column and scope of the last assignment written at its own indent:
    // R10 hangs that expression's continuation lines there.
    let mut anchor: Option<(usize, u32)> = None;
    for line in lines {
        if line.toks.is_empty() {
            anchor = None;
            out.push(RLine {
                blank: true,
                ..Default::default()
            });
            continue;
        }
        let code = code_toks(&line.toks);
        for t in code {
            scan_scopes(&t.text, &mut scopes, &mut next_scope);
        }
        // The scope the line *leaves* us in: a line opening a block is that
        // block's header, one closing a block belongs to the scope it closes
        // back into.
        let scope = *scopes.last().expect("the base scope is never popped");
        let leading = code.iter().take_while(|t| is_closer(&t.text)).count();
        let extra = usize::from(prev_control && leading == 0);
        let hangs = anchor.is_some_and(|(_, a)| a == scope) && starts_continuation(code);
        // R4 excepts a comment-only line: its indentation is the author's
        // grouping -- a hand-drawn table, a commented-out block -- so it is kept
        // as written instead of snapped to the bracket depth.
        let comment_only = line.toks.iter().all(|t| t.kind != Kind::Other);
        let indent = if comment_only {
            line.col
        } else if hangs {
            anchor.expect("hanging implies an anchor").0
        } else {
            INDENT * (depth.saturating_sub(leading) + extra)
        };
        let mut r = RLine {
            indent,
            ..Default::default()
        };
        r.eq_off = emit_code(code, &mut r.code);
        if code.is_empty() || trailing_comment_count(&line.toks) == 1 {
            r.comment = Some(line.toks.last().expect("non-empty").text.clone());
        }
        for t in code {
            depth = (depth as i32 + bracket_delta(&t.text)).max(0) as usize;
        }
        r.scope = scope;
        let assigned = r.eq_off.is_some();
        // A comment-only line is layout, not a statement: it neither is nor ends
        // a braceless body, and it neither sets nor ends a continuation run.
        if !code.is_empty() {
            if !hangs {
                anchor = r
                    .eq_off
                    .filter(|off| r.code.len() > off + 2)
                    .map(|off| (indent + off + 2, scope));
            }
            prev_control = is_braceless_control(&line.toks, assigned);
        }
        out.push(r);
    }
    align_assignments(&mut out);
    align_comments(&mut out);
    let mut s = String::new();
    for r in &out {
        if r.blank {
            s.push('\n');
            continue;
        }
        s.push_str(&" ".repeat(r.indent));
        s.push_str(&r.code);
        if let Some(c) = &r.comment {
            if !r.code.is_empty() {
                s.push_str("  ");
            }
            s.push_str(c);
        }
        s.push('\n');
    }
    Ok(s)
}

/// Emit one line's code with canonical inter-token spacing; returns the byte
/// offset of the single top-level `=` when the line is alignment-eligible.
fn emit_code(toks: &[Tok], out: &mut String) -> Option<usize> {
    let mut eq_off = None;
    let mut eq_count = 0usize;
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate() {
        if i > 0 {
            out.push_str(gap(&toks[i - 1], t));
            depth += bracket_delta(&toks[i - 1].text);
        }
        // Only a top-level `=` is an assignment; a nested one (inside a list or
        // a call) belongs to the construct, not to the line.
        if t.text == "=" && depth == 0 {
            eq_count += 1;
            eq_off = Some(out.len());
        }
        out.push_str(&t.text);
    }
    if eq_count == 1 {
        eq_off
    } else {
        None
    }
}

/// R7: the whitespace between one token and the next.
fn gap(prev: &Tok, next: &Tok) -> &'static str {
    let (p, n) = (prev.text.as_str(), next.text.as_str());
    if n == ")" || n == "]" || n == "," || n == ";" {
        return "";
    }
    if p == "(" || p == "[" {
        return "";
    }
    if p == "," || p == ";" {
        return " ";
    }
    // `.`, `:`, `@` and `/` bind to what they qualify, but whether a space sits
    // between them and a neighbour is the author's call: the lexer has lookahead
    // rules here (`debug.mc` is one token only while the extension ends the
    // name, so pulling `: print_log` up against it splits it), and whitespace is
    // only ever removed where the removal cannot join two tokens.
    if AUTHOR_SPACED.contains(&p) || AUTHOR_SPACED.contains(&n) {
        return if next.glued { "" } else { " " };
    }
    if SPACED.contains(&n) || SPACED.contains(&p) {
        return " ";
    }
    if n == "(" {
        return if is_control(&p) || PUNCT.contains(&p) {
            " "
        } else {
            ""
        };
    }
    // Group and block braces carry meaning in their spacing, and so does the
    // spacing before `[`: `GPIO[2]` (group suffix), `io [1,2]` (pin line) and
    // `= [VCC, GND]` (value list) are told apart by it. Keep what the author
    // wrote; only the AST can separate the three (design §7).
    if n == "[" || n == "{" || n == "}" || p == "{" || p == "}" {
        return if next.glued { "" } else { " " };
    }
    if GLUED_PREFIX.contains(&p) {
        return if next.glued { "" } else { " " };
    }
    // A whitespace run collapses to one space, but no space is invented: some
    // lexer rules need two tokens adjacent (`mc` in a `.mc` suffix), and
    // inserting one would split the token. The I2 re-lex check backstops every
    // other rule here.
    if next.glued {
        ""
    } else {
        " "
    }
}

/// R9: align the `=` of a run of assignment lines at one indent.
fn align_assignments(lines: &mut [RLine]) {
    let mut i = 0;
    while i < lines.len() {
        if !alignable(&lines[i]) {
            i += 1;
            continue;
        }
        let indent = lines[i].indent;
        let mut j = i + 1;
        while j < lines.len() && alignable(&lines[j]) && lines[j].indent == indent {
            j += 1;
        }
        if j - i >= 2 {
            let widest = (i..j).filter_map(|k| lines[k].eq_off).max().unwrap_or(0);
            for k in i..j {
                if let Some(off) = lines[k].eq_off {
                    lines[k].code.insert_str(off, &" ".repeat(widest - off));
                }
            }
        }
        i = j;
    }
}

fn alignable(r: &RLine) -> bool {
    !r.blank && r.comment.is_none() && r.eq_off.is_some() && !r.code.is_empty()
}

/// R8: align the trailing comments of every commented line in one bracket
/// scope. Blank lines and uncommented lines inside the scope do not break the
/// run -- only leaving the scope does -- so an entry list reads as one column
/// however its author spaced it.
fn align_comments(lines: &mut [RLine]) {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..lines.len() {
        if !commented(&lines[i]) {
            continue;
        }
        match groups
            .iter_mut()
            .find(|g| lines[g[0]].scope == lines[i].scope)
        {
            Some(g) => g.push(i),
            None => groups.push(vec![i]),
        }
    }
    for group in groups {
        let target = group.iter().map(|&k| width(&lines[k])).max().unwrap_or(0);
        for &k in &group {
            let pad = target.saturating_sub(width(&lines[k]));
            lines[k].code.push_str(&" ".repeat(pad));
        }
    }
}

fn commented(r: &RLine) -> bool {
    r.comment.is_some() && !r.code.is_empty()
}

/// Rendered width of a line, indentation included.
fn width(r: &RLine) -> usize {
    r.indent + r.code.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::infra::init::MCC_TEST_PARSE_LOCK;

    /// The C lexer is process-global, so every test that lexes holds the shared
    /// lock (same convention as the other parse-driving tests).
    fn fmt(src: &str) -> String {
        let _guard = parse_lock();
        format_text(src).expect("source should be formattable")
    }

    fn parse_lock() -> std::sync::MutexGuard<'static, ()> {
        MCC_TEST_PARSE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn fmt__empty_and_blank_sources_are_untouched() {
        let _guard = parse_lock();
        assert_eq!(format_text("").unwrap(), "");
        assert_eq!(format_text("  \n\n").unwrap(), "  \n\n");
    }

    #[test]
    fn fmt__indent_follows_bracket_depth() {
        let src = "module main {\n  component A(rs::UV.OHM) {\n        pins = [1, 2]\n }\n}\n";
        let want =
            "module main {\n    component A(rs::UV.OHM) {\n        pins = [1, 2]\n    }\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn fmt__joins_leading_brace_and_folds_blank_lines() {
        let src = "module main\n{\n\n\n    component A\n    {\n    }\n}\n\n";
        let want = "module main {\n    component A {\n    }\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn fmt__normalizes_inner_spacing_and_trailing_space() {
        let src = "module main {\n    pins = [TX ,RX]   \n    x =a\n}\n";
        let want = "module main {\n    pins = [TX, RX]\n    x    = a\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// A line comment immediately before a closing brace: the lexer hands the
    /// brace back inside the comment's own token span, so the brace is only
    /// recoverable from that span.
    #[test]
    fn fmt__keeps_the_brace_after_a_line_comment() {
        let src = "component A {\n    a = 1 // c\n}\n";
        let want = "component A {\n    a = 1  // c\n}\n";
        assert_eq!(fmt(src), want);
    }

    #[test]
    fn fmt__control_body_is_indented_one_level() {
        let src = "func f() {\n    if (a)\n        do_a\n    else\n        do_b\n}\n";
        let want = "func f() {\n    if (a)\n        do_a\n    else\n        do_b\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// A comment-only line between a braceless header and its body is layout:
    /// the body keeps the level the header opened.
    #[test]
    fn fmt__comment_line_keeps_the_braceless_body_level() {
        let src = "module t() {\n    if (a)\n        // note\n        do_a\n}\n";
        let want = "module t() {\n    if (a)\n        // note\n        do_a\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R4 excepts a comment-only line: its indentation is the author's layout
    /// (a hand-drawn table, a commented-out block), so it is not snapped to the
    /// bracket depth even when that leaves it off the block's column.
    #[test]
    fn fmt__a_comment_keeps_the_indent_the_author_gave_it() {
        let src = "module m {\n    a -> b\n          // hand-drawn table\n    b -> c\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn fmt__aligns_assignment_runs_and_trailing_comments() {
        let src = "component A {\n    a = 1\n    bbb = 2\n    x = 3 // one\n    yy = 4 // two\n}\n";
        let want =
            "component A {\n    a   = 1\n    bbb = 2\n    x = 3   // one\n    yy = 4  // two\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R8 keys on the bracket scope, so a blank line or an uncommented line
    /// inside one scope does not end the run -- and the line that opens the
    /// scope is that scope's header.
    #[test]
    fn fmt__aligns_comments_across_blank_lines_within_one_scope() {
        let src = "component A {\n    p = [ // one\n        aaa = 1 // two\n\n        b = 2 // three\n    ]\n}\n";
        let want =
            "component A {\n    p = [        // one\n        aaa = 1  // two\n\n        b = 2    // three\n    ]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// A nested scope is a group of its own: its lines do not join the outer
    /// column, and the outer lines still align across it.
    #[test]
    fn fmt__comment_alignment_is_per_scope() {
        let src = "component A {\n    a = 1 // one\n    sub = [\n        bbbbb = 2 // two\n    ]\n    ccc = 333 // three\n}\n";
        let want = "component A {\n    a = 1      // one\n    sub = [\n        bbbbb = 2  // two\n    ]\n    ccc = 333  // three\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R10: a line the expression above continues hangs at that expression's
    /// first column, so a wrapped definition reads as one right hand side.
    #[test]
    fn fmt__continuation_lines_hang_at_the_rhs_column() {
        let src = "component A {\n    pins = [\n        io [1, 2] = I2C0::I2C(Master) // one\n        | GPIO[3, 4]::GPIO(2, Controller) // two\n    ]\n}\n";
        let want = "component A {\n    pins = [\n        io [1, 2] = I2C0::I2C(Master)                  // one\n                    | GPIO[3, 4]::GPIO(2, Controller)  // two\n    ]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R10 reads the leading operator only: a sibling entry after a comma is a
    /// statement of its own, and an AST-free pass cannot tell it from a
    /// continued list, so it keeps the block indent. Folded it would exceed
    /// [`MAX_WIDTH`], so R11 leaves it wrapped.
    #[test]
    fn fmt__a_sibling_entry_after_a_comma_does_not_hang() {
        let src = "component p12(\n    temperature::UV.TEMP = 25C,\n    frequency::UV.HZ     = 100kHz,\n    tolerance::UV.PPM    = 20ppm,\n    drift::UV.PPM        = 5ppm\n) {\n    pins = [1 = A]\n}\n";
        assert_eq!(fmt(src), src);
    }

    /// R11-a: an assignment waits for its right hand side, so the break after
    /// the `=` -- blank lines included -- folds back into one line.
    #[test]
    fn fmt__folds_a_break_after_an_assignment() {
        let src = "component A {\n    pins =\n\n    [\n        io [1, 2] = X\n    ]\n}\n";
        let want = "component A {\n    pins = [\n        io [1, 2] = X\n    ]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R11-b: a comma whose line left a bracket open separates items of a list
    /// the line has not finished, so the entry folds back.
    #[test]
    fn fmt__folds_a_break_inside_an_open_bracket() {
        let src = "component A {\n    pins = [\n        io [1,\n           2] = I2C0::I2C(Master)\n    ]\n}\n";
        let want = "component A {\n    pins = [\n        io [1, 2] = I2C0::I2C(Master)\n    ]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R11-c: a lone word is not the statement the next line's bracketed operand
    /// belongs to, so `in` takes its pin list back onto its own line.
    #[test]
    fn fmt__folds_a_lone_word_into_its_bracketed_operand() {
        let src = "component A {\n    pins = [\n        in\n        [3, 4] = XTAL::XTAL(32kHz)\n    ]\n}\n";
        let want = "component A {\n    pins = [\n        in [3, 4] = XTAL::XTAL(32kHz)\n    ]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// The mirror of R11-c: a line that opens a group without declaring anything
    /// is a statement of its own, so the lone word above keeps its line and the
    /// two statements are not merged.
    #[test]
    fn fmt__a_lone_word_does_not_swallow_the_next_statement() {
        let src = "module m {\n    VEXT\n    [a, b] + c\n}\n";
        assert_eq!(fmt(src), src);
    }

    /// The mirror of R10's continuation: a trailing operator leaves its right
    /// operand open just as a leading one continues the line above.
    #[test]
    fn fmt__folds_a_break_after_a_trailing_operator() {
        let src = "component A {\n    pins = [\n        io [1, 2] = I2C0::I2C(Master) |\n            GPIO[3, 4]::GPIO(2, Controller)\n    ]\n}\n";
        let want = "component A {\n    pins = [\n        io [1, 2] = I2C0::I2C(Master) | GPIO[3, 4]::GPIO(2, Controller)\n    ]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// A comma whose brackets balance is a sibling entry, not a continuation:
    /// nothing on the line is left open, so the list keeps one entry per line.
    #[test]
    fn fmt__a_balanced_comma_does_not_fold() {
        let src = "component A {\n    pins = [\n        io [1, 2] = A,\n        io [3, 4] = B\n    ]\n}\n";
        assert_eq!(fmt(src), src);
    }

    /// Folding behind a line comment would comment out the folded operand.
    #[test]
    fn fmt__a_trailing_line_comment_blocks_the_fold() {
        let src = "component A {\n    pins = // why\n        [1]\n}\n";
        let want = "component A {\n    pins =  // why\n    [1]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// A token whose spacing decides a lexer match keeps the author's break
    /// (design §3.1.2), so `+` does not pull `/2` up onto its line.
    #[test]
    fn fmt__a_lexer_sensitive_token_blocks_the_fold() {
        let src = "component A {\n    x = 1 +\n        /2\n}\n";
        let want = "component A {\n    x = 1 +\n    /2\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R11-d: a line ending in an opener glued to its head is a call still
    /// waiting for its arguments, so the group folds onto one line.
    #[test]
    fn fmt__folds_a_call_whose_arguments_are_wrapped() {
        let src = "module m {\n    UC.power(\n        [a.VDD_3V3, a.GND], [b.VCC_1V2, b.GND])\n    UC.GND -> a.GND\n}\n";
        let want = "module m {\n    UC.power([a.VDD_3V3, a.GND], [b.VCC_1V2, b.GND])\n    UC.GND -> a.GND\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// The same for a group whose close is followed by an operator: the
    /// expression is longer than the group, so the fold follows it to the end.
    #[test]
    fn fmt__folds_a_group_the_expression_continues_past() {
        let src = "module m {\n    MIC{P\n        , N} -> [\n        C4::CAP(),\n        C5::CAP()] -> UC.ADC{P, N}\n}\n";
        let want = "module m {\n    MIC{P, N} -> [C4::CAP(), C5::CAP()] -> UC.ADC{P, N}\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// A trailing comment may ride on the folded line, but a comment before the
    /// close would swallow the lines folded onto it.
    #[test]
    fn fmt__a_comment_on_the_last_group_line_rides_along() {
        let src = "module m {\n    Crystal2.DST310S(\n        NC) X6  // no fit\n    X6.setup(a.GND).XTAL -> UC.XTAL\n}\n";
        let want = "module m {\n    Crystal2.DST310S(NC) X6  // no fit\n    X6.setup(a.GND).XTAL -> UC.XTAL\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// A block's brace is spelled with a space and a block owns its lines, so
    /// neither the header nor the body folds.
    #[test]
    fn fmt__a_block_does_not_fold_into_one_line() {
        let src = "define CAP.X5R {\n    manufacture = \"TDK\"\n    spec        = [v = 10V]\n}\n";
        assert_eq!(fmt(src), src);
    }

    /// A group wider than [`MAX_WIDTH`] when folded is a group the author had to
    /// break, so it keeps its lines.
    #[test]
    fn fmt__a_group_wider_than_the_limit_does_not_fold() {
        let src = "module m {\n    CAP(1uF, 10V, CAP.X5R, 10V).Cap(\n        [AVDD09_CAP_A, AVDD09_CAP_B, AVDD09_CAP_C, AVDD09_CAP_D, AVDD09_CAP_E])\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn fmt__second_pass_is_a_fixed_point() {
        let src = "component A(rs::UV.OHM) {\n  pins = [1, 2]\n  spec = [\n    v = 5V\n  ]\n}\n";
        let once = fmt(src);
        assert_eq!(fmt(&once), once);
    }

    /// I2: the lexer must see the same tokens, in the same order, with the same
    /// text, after the rewrite.
    #[test]
    fn fmt__token_stream_survives_the_rewrite() {
        let _guard = parse_lock();
        for src in [
            "component A {\n    a = 1 // c\n}\n",
            "module main\n{\n    pins = [TX ,RX]\n}\n",
            "component A {\n    x = 1 # one\n    yy = 2 /* two */\n}\n",
            "interface I {\n    conduit [1:2] = [TX, RX]::UART.TTL()\n}\n",
        ] {
            let want = format_text(src).expect("source should be formattable");
            assert_eq!(fingerprint(src).unwrap(), fingerprint(&want).unwrap());
        }
    }

    /// R12: the declarations a block opens with are one run, so the blank lines
    /// between them -- a keyword, a one-line group header and a plain instance
    /// line alike -- are dropped. The blank before the first statement stays.
    #[test]
    fn fmt__folds_the_blank_lines_between_leading_declarations() {
        let src = "module m {\n    conduit GNDA @role(quiet)\n\n    domain AVAUD @class(analog) { rail [VMIC, GNDA]::DC(3.3V) }\n\n    out MIC{P, N}::ADC.DIFF(Transmitter)\n\n    MICROPHONE.SIP2 mic\n\n    mic{1, 2} -> C1::CAP(470pF)' -> MIC{P, N}\n}\n";
        let want = "module m {\n    conduit GNDA @role(quiet)\n    domain AVAUD @class(analog) { rail [VMIC, GNDA]::DC(3.3V) }\n    out MIC{P, N}::ADC.DIFF(Transmitter)\n    MICROPHONE.SIP2 mic\n\n    mic{1, 2} -> C1::CAP(470pF)' -> MIC{P, N}\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// R12's keyword test is positional, not header-only: a blank between two
    /// declaration keywords folds wherever it sits, a pin list included.
    #[test]
    fn fmt__folds_the_blank_between_keyword_lines_anywhere() {
        let src = "component A {\n    pins = [\n        in [3, 4] = XTAL::XTAL(32kHz)\n\n        psnk [5, 21] = [VDD, GND]::DC(3.3V)\n    ]\n}\n";
        let want = "component A {\n    pins = [\n        in [3, 4]    = XTAL::XTAL(32kHz)\n        psnk [5, 21] = [VDD, GND]::DC(3.3V)\n    ]\n}\n";
        assert_eq!(fmt(src), want);
    }

    /// The blank R12 keeps: nothing above declares, so a blank between two
    /// statements is the author's paragraph break.
    #[test]
    fn fmt__keeps_a_blank_between_statements() {
        let src = "module m {\n    a.x -> b.x\n\n    b.y -> c.y\n}\n";
        assert_eq!(fmt(src), src);
    }

    /// A `use` URI is one path, not operands: it is sliced up to the first
    /// whitespace, so `/` must not gain a space and the author's spacing stands.
    #[test]
    fn fmt__keeps_a_use_uri_whole() {
        let _guard = parse_lock();
        for src in [
            "use ./utils/mcu/debug.mc\n",
            "use ./utils.mcu@1.0\n",
            "use ../lib/mosfet@1.0\n",
            "use /lib.man.custom@1.0\n",
            "use ./utils/mcu/debug.mc : print_log\n",
        ] {
            assert_eq!(format_text(src).unwrap(), src);
        }
    }
}
