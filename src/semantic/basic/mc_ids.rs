// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::mc_ida::{IdaSegment, McIda, SquareItem};
use super::mc_literal::McInt;
use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::db::diagnostic::diagnostic::dlog_error;

/// Expand a numeric slice in declaration order (eval.md §11.1: the declared
/// direction is authoritative). An ascending declaration `1:4` yields
/// `[1, 2, 3, 4]`; a descending declaration `4:1` yields `[4, 3, 2, 1]` —
/// descending members must not be silently dropped.
pub(crate) fn expand_numeric_slice(from: i64, to: i64) -> Vec<i64> {
    if from <= to {
        (from..=to).collect()
    } else {
        (to..=from).rev().collect()
    }
}

/// Expand a letter slice in declaration order (`a:e` ascending, `e:a`
/// descending), mirroring `expand_numeric_slice`.
pub(crate) fn expand_char_slice(from: char, to: char) -> Vec<char> {
    if from <= to {
        (from..=to).collect()
    } else {
        (to..=from).rev().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IdsSegment {
    Int(Box<McInt>),
    Slice {
        from: Box<McInt>,
        to: Box<McInt>,
    },
    Ida(Box<McIda>),
    DotInt(Box<McInt>),
    DotIda(Box<McIda>),
    Curly(Vec<IdsSegment>),
    /// Square bracket segment, contains multiple members, e.g., [VDD, GND]
    Square(Vec<IdsSegment>),
}

impl IdsSegment {}

#[derive(Clone, Debug)]
pub struct McIds {
    pub segments: Vec<IdsSegment>,
}

impl McIds {
    /// Normalize segments for Eq/Hash: convert `DotIda` / `DotInt` to
    /// `Curly`, so that `DC2.VDD` and `DC2{VDD}` are treated as the same
    /// key (Defect 88).  This matches the semantic equivalence documented
    /// in `McBus`.
    fn normalized_eq_hash(&self) -> Vec<IdsSegment> {
        self.segments
            .iter()
            .map(|seg| match seg {
                IdsSegment::DotIda(ida) => IdsSegment::Curly(vec![IdsSegment::Ida(ida.clone())]),
                IdsSegment::DotInt(n) => IdsSegment::Curly(vec![IdsSegment::Int(n.clone())]),
                other => other.clone(),
            })
            .collect()
    }
}

impl PartialEq for McIds {
    fn eq(&self, other: &Self) -> bool {
        self.normalized_eq_hash() == other.normalized_eq_hash()
    }
}

impl Eq for McIds {}

impl std::hash::Hash for McIds {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.normalized_eq_hash().hash(state);
    }
}

impl From<&str> for McIds {
    fn from(s: &str) -> Self {
        Self {
            segments: vec![IdsSegment::Ida(Box::new(McIda::from(s)))],
        }
    }
}

impl From<String> for McIds {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl From<&String> for McIds {
    fn from(s: &String) -> Self {
        Self::from(s.as_str())
    }
}

/// Wrap an AST-parsed `McIda` as a single `Ida` segment.
///
/// Definition-side class names (component / module / interface / enum) are
/// `McIda` parsed from the AST; wrapping them preserves the internal segment
/// structure without a `to_string()` display round-trip. Together with
/// `From<&str>` this gives one canonical single-`Ida` form for definition
/// names, so `class_name_to_id` lookups stay consistent regardless of which
/// construction path produced the key.
impl From<McIda> for McIds {
    fn from(ida: McIda) -> Self {
        Self {
            segments: vec![IdsSegment::Ida(Box::new(ida))],
        }
    }
}

/// Parse a display string into the same segment tree the AST front end
/// produces, so the pure-string ports (P3) have one shared text entry and no
/// caller needs its own text re-parse.
///
/// The string is first split into curly-free text runs at `{...}` group
/// boundaries (escape- and bracket-aware). Each run keeps the exact `McIda`
/// text grammar — dots stay inline, squares and escapes parse exactly as
/// `McIda::from` — and each curly group becomes a `Curly` segment whose
/// members follow the AST `MCAST_OPD_CURLY` encoding: `,` / `|` separators,
/// numeric slices (`1:3`, both sides i64) become `Slice`, bare numbers `Int`,
/// names `Ida`. `member_set` (equivalent.rs) then expands the tree to the
/// same ordered member list the old text post-pass (`split_curly_groups`)
/// produced — the separation no longer has to be re-derived from text.
///
/// An empty curly body yields an empty `Curly` segment, whose zero-width
/// expansion removes the whole member (matching the old text pass). Braces
/// nested inside square-bracket content or a second top-level curly group are
/// kept literal (non-corpus shapes; the AST never produces them either).
pub(crate) fn parse_display(display: &str) -> McIds {
    // Locate top-level curly groups first. Escapes hide the next byte, and
    // square content is skipped whole so `{` inside brackets stays literal.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let bytes = display.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'[' => {
                let mut depth = 1usize;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'\\' => j += 2,
                        b'[' => {
                            depth += 1;
                            j += 1;
                        }
                        b']' => {
                            depth -= 1;
                            j += 1;
                        }
                        _ => j += 1,
                    }
                }
                i = j;
            }
            b'{' => {
                let mut depth = 1usize;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'\\' => j += 2,
                        b'{' => {
                            depth += 1;
                            j += 1;
                        }
                        b'}' => {
                            depth -= 1;
                            j += 1;
                        }
                        _ => j += 1,
                    }
                }
                if depth == 0 {
                    groups.push((i, j - 1));
                    i = j;
                } else {
                    // Unbalanced brace — keep scanning; it stays literal text.
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }

    let mut segments: Vec<IdsSegment> = Vec::new();
    let mut prev_end = 0usize;
    for (open, close) in groups {
        if open > prev_end {
            if let Some(ida) = text_run_segment(&display[prev_end..open]) {
                segments.push(ida);
            }
        }
        segments.push(IdsSegment::Curly(curly_body_segments(
            &display[open + 1..close],
        )));
        prev_end = close + 1;
    }
    if prev_end < display.len() {
        if let Some(ida) = text_run_segment(&display[prev_end..]) {
            segments.push(ida);
        }
    }
    McIds { segments }
}

/// Parse a curly-free text run with the exact `McIda` text grammar (squares,
/// escapes, dots inline) and wrap it as a single `Ida` segment.
fn text_run_segment(run: &str) -> Option<IdsSegment> {
    if run.is_empty() {
        return None;
    }
    let ida = McIda::from(run);
    if ida.segments.is_empty() {
        None
    } else {
        Some(IdsSegment::Ida(Box::new(ida)))
    }
}

/// Split a curly body (`...` between `{` and `}`) into ordered member
/// segments. `,` / `|` split at top level only — an escaped separator
/// (`\|`) stays inside its member, mirroring `McIda` escape handling.
fn curly_body_segments(body: &str) -> Vec<IdsSegment> {
    let mut segments: Vec<IdsSegment> = Vec::new();
    let mut token = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    token.push(next);
                }
            }
            ',' | '|' => {
                push_curly_token(&mut segments, &token);
                token.clear();
            }
            _ => token.push(c),
        }
    }
    push_curly_token(&mut segments, &token);
    segments
}

/// Push one trimmed curly member token with the AST `MCAST_OPD_CURLY`
/// encoding: numeric slice → `Slice`, bare number → `Int`, name → `Ida`.
fn push_curly_token(segments: &mut Vec<IdsSegment>, token: &str) {
    let tok = token.trim();
    if tok.is_empty() {
        return;
    }
    // R12: a numeric slice expands to its interval at expand time; a
    // non-numeric colon token stays a literal member.
    if let Some((from, to)) = tok.split_once(':') {
        if let (Ok(f), Ok(t)) = (from.trim().parse::<i64>(), to.trim().parse::<i64>()) {
            segments.push(IdsSegment::Slice {
                from: Box::new(McInt::from(f.to_string().as_str())),
                to: Box::new(McInt::from(t.to_string().as_str())),
            });
            return;
        }
    }
    if let Ok(n) = tok.parse::<i64>() {
        segments.push(IdsSegment::Int(Box::new(McInt::from(
            n.to_string().as_str(),
        ))));
    } else {
        segments.push(IdsSegment::Ida(Box::new(McIda::from(tok))));
    }
}

/// Split `display` at its single trailing curly member group into the base
/// text and the ordered raw member names.
///
/// Structural counterpart of the old `find('{')` + prefix-strip ports
/// (`param_name_to_inst`, `this_ref_to_bus`, `group_members`): the base is
/// the segment text before the group and the members come straight out of the
/// trailing `Curly` segment (R12 slices expanded by their segment, no
/// `base.member` path to strip). Returns `None` unless the display is a
/// non-empty curly group with non-empty base text before it.
pub(crate) fn curly_base_members(display: &str) -> Option<(String, Vec<String>)> {
    let ids = parse_display(display);
    let n = ids.segments.len();
    match ids.segments.last() {
        Some(IdsSegment::Curly(inner)) if !inner.is_empty() && n >= 2 => {
            let base = ids.segments[..n - 1]
                .iter()
                .map(ToString::to_string)
                .collect::<String>();
            if base.is_empty() {
                return None;
            }
            let members = curly_member_names(inner);
            if members.is_empty() {
                None
            } else {
                Some((base, members))
            }
        }
        _ => None,
    }
}

/// Raw ordered member names of a curly group: expand the group alone and drop
/// the `.` join prefix the base would normally carry.
fn curly_member_names(inner: &[IdsSegment]) -> Vec<String> {
    let only = McIds {
        segments: vec![IdsSegment::Curly(inner.to_vec())],
    };
    only.expand()
        .into_iter()
        .map(|m| m.strip_prefix('.').unwrap_or(&m).to_string())
        .filter(|m| !m.is_empty())
        .collect()
}

/// Split a pure-string port key / member reference (module port side) into
/// its base name and raw declared members through the shared `parse_display`
/// segment tree (P3) — the caller never re-splits the display text and there
/// is no parallel text grammar.
///
/// Recognized canonical shapes (module port declaration keys and the member
/// references that match them):
///
/// - scalar `name`           -> ("name", [])
/// - curly `name{A, B}`      -> ("name", ["A", "B"])  (group with base text)
/// - bare curly `{A, B}`     -> ("", ["A", "B"])
/// - square `[A, B]`         -> ("", ["A", "B"])
/// - named square `p[A, B]`  -> ("p", ["A", "B"])     (square inside one Ida)
///
/// Members are raw text: an R12 `1:2` slice token stays a single `"1:2"`
/// member. The Pass1/Pass2 module-port width readers (`module_port_elems`,
/// `eval_port_elems`, points.rs `expand_port_lanes`) all treat a literal
/// slice token as one lane — `member_set` on the segment tree is the only
/// place slices expand. This is the key/ref counterpart of
/// [`curly_base_members`], which expands slices (member-set semantics) and
/// requires a non-empty base.
pub(crate) fn display_base_members(display: &str) -> (String, Vec<String>) {
    let ids = parse_display(display);
    // 1. Trailing curly group: the base is the segment text before it. An
    //    empty group (`name{}`) keeps the base and contributes no members.
    if let Some(IdsSegment::Curly(inner)) = ids.segments.last() {
        let base = ids.segments[..ids.segments.len() - 1]
            .iter()
            .map(ToString::to_string)
            .collect::<String>();
        if inner.is_empty() {
            return (base, Vec::new());
        }
        let members: Vec<String> = inner.iter().map(ToString::to_string).collect();
        return (base, members);
    }
    // 2. Square content inside a single text run (parse_display keeps a
    //    square whole inside the `Ida`, mirroring `McIda`): the base is the
    //    run's prefix and the members are the first square group's raw items.
    if ids.segments.len() == 1 {
        match &ids.segments[0] {
            IdsSegment::Square(inner) => {
                let members: Vec<String> = inner.iter().map(ToString::to_string).collect();
                if !members.is_empty() {
                    return (String::new(), members);
                }
            }
            IdsSegment::Ida(ida) => {
                let mut members: Vec<String> = Vec::new();
                for seg in &ida.segments {
                    if let IdaSegment::Square(items) = seg {
                        members.extend(items.iter().map(ToString::to_string));
                        break;
                    }
                }
                if !members.is_empty() {
                    return (ida.prefix().to_string(), members);
                }
            }
            _ => {}
        }
    }
    // 3. Scalar (or non-canonical) display: no members, whole text as base.
    (display.to_string(), Vec::new())
}

impl McIds {
    pub fn new(node: &AstNode) -> Option<Self> {
        // 1. MCAST_IDS
        //    |- MCAST_ID/MCAST_IDA  - (MCAST_ID/MCAST_IDA/MCAST_OPD_DOT/MCAST_OPD_CURLY)*  - MCAST_INT+
        // where:
        // |- MCAST_OPD_DOT
        //     |- MCAST_ID/MCAST_IDA
        // |- MCAST_OPD_CURLY
        //     |- (MCAST_ID / MCAST_IDA / MCAST_INT / MCAST_OPD_COLON)*
        // 2. MCK_THIS / MCK_PINS
        //    |- MCK_THIS
        //    |- MCK_THIS mc_idm
        //    |- MCK_THIS MCPT_DOT mc_int
        //    |- MCK_THIS mc_idm MCPT_DOT mc_int
        //    |- MCK_PINS mc_idm
        //    |- MCK_PINS MCPT_DOT mc_int

        let mut segments = Vec::new();

        // Handle MCAST_OPD_THIS and MCAST_OPD_PINS cases
        match node.get_type() {
            // Use McIda to handle ID and IDA processing
            // Treat the entire IDA string as one IdsSegment::Ida to maintain consistency with McIds::from
            MCAST_ID | MCAST_IDA => {
                if let Some(ida) = McIda::new(node) {
                    segments.push(IdsSegment::Ida(Box::new(ida)));
                }
            }

            MCAST_PARAM => {
                // MCAST_PARAM is a wrapper, get its sub-node and recurse
                if let Some(sub) = node.get_sub_node() {
                    return McIds::new(&sub);
                }
                return None;
            }

            MCAST_OPD_THIS | MCAST_OPD_PINS => {
                // Add "this" or "pins" as an Ida segment
                let keyword = if node.get_type() == MCAST_OPD_THIS {
                    "this"
                } else {
                    "pins"
                };
                let ida = McIda::from(keyword);
                segments.push(IdsSegment::Ida(Box::new(ida)));

                // Handle subsequent child nodes
                let Some(mut current) = node.get_next() else {
                    // Only "this" or "pins" case
                    return Some(McIds { segments });
                };

                // Handle mc_idm (if exists)
                if current.get_type() != MCAST_OPD_DOT {
                    // Try to parse as McIda
                    if let Some(ida) = McIda::new(&current) {
                        segments.push(IdsSegment::DotIda(Box::new(ida)));

                        // Check if there's more .mc_int
                        if let Some(next) = current.get_next() {
                            current = next;
                        } else {
                            return Some(McIds { segments });
                        }
                    }
                }

                // Handle .mc_int
                if current.get_type() == MCAST_OPD_DOT {
                    if let Some(subnode) = current.get_sub_node() {
                        if subnode.get_type() == MCAST_INT {
                            if let Some(int) = McInt::new(&subnode) {
                                segments.push(IdsSegment::DotInt(Box::new(int)));
                            }
                        }
                    }
                }

                return Some(McIds { segments });
            }
            // Lemon automatically creates MCAST_* nodes for non-terminals
            // mc_opd returns wrapped MCAST_OPD, need to extract sub-node
            MCAST_OPD => {
                if let Some(sub) = node.get_sub_node() {
                    return McIds::new(&sub);
                }
                return None;
            }
            // Handle cases where square bracket vectors appear directly as nodes (not inside MCAST_IDS)
            // Example: [VDD2, GND2] in mc_phrase
            MCAST_OPD_SQUARE_VEC => {
                if let Some(square_seg) = Self::parse_square(node) {
                    segments.push(square_seg);
                }
            }
            // Bare (non-OPD-wrapped) square vector, `[VDD, GND]` as a direct
            // child of a parameter/value node (grammar's non-`&` variant).
            // Same member shape as MCAST_OPD_SQUARE_VEC, so parse_square
            // applies unchanged; without this arm the node was dropped by
            // the `_ => None` fallthrough.
            MCAST_SQUARE_VEC => {
                if let Some(square_seg) = Self::parse_square(node) {
                    segments.push(square_seg);
                }
            }
            MCAST_IDS => {
                // Original logic: handle MCAST_IDS case
                let Some(ids_subnodes) = node.get_sub_node() else {
                    dlog_error(
                        crate::errcodes::NAME_IDS_NO_NODES,
                        node,
                        &crate::errcodes::format_msg(crate::errcodes::NAME_IDS_NO_NODES, &[]),
                    );
                    return None;
                };

                let mut new_segments = Vec::new();
                for each in ids_subnodes.iter() {
                    match each.get_type() {
                        // Use McInt to handle integer processing
                        MCAST_INT => {
                            if let Some(int_value) = McInt::new(&each) {
                                new_segments.push(IdsSegment::Int(Box::new(int_value)));
                            }
                        }
                        // Use McIda to handle ID and IDA processing
                        // Treat the entire IDA string as one IdsSegment::Ida to maintain consistency with MCAST_ID/MCAST_IDA branch
                        MCAST_ID | MCAST_IDA => {
                            if let Some(ida) = McIda::new(&each) {
                                new_segments.push(IdsSegment::Ida(Box::new(ida)));
                            }
                        }

                        MCAST_OPD_DOT => {
                            let Some(subnode) = each.get_sub_node() else {
                                dlog_error(
                                    crate::errcodes::NAME_MISSING_SUBNODE,
                                    &each,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::NAME_MISSING_SUBNODE,
                                        &[],
                                    ),
                                );
                                continue;
                            };
                            match subnode.get_type() {
                                MCAST_INT => {
                                    if let Some(int) = McInt::new(&subnode) {
                                        new_segments.push(IdsSegment::DotInt(Box::new(int)));
                                    }
                                }
                                MCAST_ID | MCAST_IDA => {
                                    if let Some(ida) = McIda::new(&subnode) {
                                        new_segments.push(IdsSegment::DotIda(Box::new(ida)));
                                    }
                                }
                                _ => {}
                            }
                        }

                        MCAST_OPD_CURLY => {
                            if let Some(curly_seg) = Self::parse_curly(&each) {
                                new_segments.push(curly_seg);
                            }
                        }

                        _ => {}
                    }
                }
                segments = new_segments;
            }
            _ => return None,
        };

        Some(McIds { segments })
    }

    /// Build `McIds` from a CMIE declaration's `MCAST_IDS` node, including a
    /// following dotted suffix. The grammar emits `mc_ids MCPT_DOT mc_int`
    /// (`TTL.7400`) with the `.int` as a SIBLING of the ids node; [`McIds::new`]
    /// alone returns only the prefix (`TTL`), collapsing all dotted-numeric
    /// definitions to one name. [`McOpd::new`](crate::semantic::basic::mc_opd)
    /// already appends the sibling — CMIE name extraction must do the same.
    pub fn new_with_dot(node: &AstNode) -> Option<Self> {
        let mut ids = Self::new(node)?;
        if let Some(next) = node.get_next() {
            ids.append(&next);
        }
        Some(ids)
    }

    pub fn append(&mut self, node: &AstNode) {
        node.iter().for_each(|each| match each.get_type() {
            MCAST_OPD_DOT => {
                if let Some(subnode) = each.get_sub_node() {
                    match subnode.get_type() {
                        MCAST_ID | MCAST_IDA => {
                            if let Some(ida) = McIda::new(&subnode) {
                                self.segments.push(IdsSegment::DotIda(Box::new(ida)));
                            }
                        }
                        MCAST_INT => {
                            if let Some(int) = McInt::new(&subnode) {
                                self.segments.push(IdsSegment::DotInt(Box::new(int)));
                            }
                        }
                        _ => {}
                    }
                }
            }
            MCAST_OPD_CURLY => {
                if let Some(curly_seg) = Self::parse_curly(&each) {
                    self.segments.push(curly_seg);
                }
            }
            MCAST_OPD_SQUARE_VEC => {
                if let Some(square_seg) = Self::parse_square(&each) {
                    self.segments.push(square_seg);
                }
            }
            _ => {}
        });
    }

    fn parse_curly(node: &AstNode) -> Option<IdsSegment> {
        let Some(curly_subnodes) = node.get_sub_node() else {
            dlog_error(
                crate::errcodes::NAME_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(crate::errcodes::NAME_MISSING_SUBNODE, &[]),
            );
            return None;
        };

        let curly_segs = curly_subnodes
            .iter()
            .filter_map(|each| {
                match each.get_type() {
                    MCAST_INT => McInt::new(&each).map(|int| IdsSegment::Int(Box::new(int))),
                    MCAST_ID | MCAST_IDA => {
                        McIda::new(&each).map(|ida| IdsSegment::Ida(Box::new(ida)))
                    }
                    // Handle MCAST_OPD_COLON (e.g. 1:10)
                    MCAST_OPD_COLON => (|| -> Option<IdsSegment> {
                        let left = each.get_sub_node()?;
                        let right = left.get_next()?;

                        let left_int = McInt::new(&left)
                            .ok_or_else(|| {
                                dlog_error(
                                    crate::errcodes::NAME_RANGE_SIDE_FAILED,
                                    &left,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::NAME_RANGE_SIDE_FAILED,
                                        &[&"left"],
                                    ),
                                );
                            })
                            .ok()?;

                        let right_int = McInt::new(&right)
                            .ok_or_else(|| {
                                dlog_error(
                                    crate::errcodes::NAME_RANGE_SIDE_FAILED,
                                    &right,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::NAME_RANGE_SIDE_FAILED,
                                        &[&"right"],
                                    ),
                                );
                            })
                            .ok()?;

                        Some(IdsSegment::Slice {
                            from: Box::new(left_int),
                            to: Box::new(right_int),
                        })
                    })(),
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        Some(IdsSegment::Curly(curly_segs))
    }

    /// Parse square bracket vector, e.g. [VDD, GND]
    fn parse_square(node: &AstNode) -> Option<IdsSegment> {
        let Some(square_subnodes) = node.get_sub_node() else {
            dlog_error(
                crate::errcodes::NAME_SQUARE_VECTOR_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(
                    crate::errcodes::NAME_SQUARE_VECTOR_MISSING_SUBNODE,
                    &[],
                ),
            );
            return None;
        };

        let square_segs = square_subnodes
            .iter()
            .filter_map(|each| {
                // Each element of mc_phrases may be:
                // 1. mc_opd (MCAST_OPD) - subnode is mc_ids
                // 2. mc_literal (MCAST_LITERAL)
                // 3. Other direct nodes
                // If MCAST_OPD, need to get its mc_ids child node
                let ids_node = if each.get_type() == MCAST_OPD {
                    each.get_sub_node().unwrap_or(each.clone())
                } else {
                    each.clone()
                };

                // Try parsing with McIds::new
                if let Some(ids) = McIds::new(&ids_node) {
                    // McIds may have only one segment, take first
                    if let Some(seg) = ids.segments.into_iter().next() {
                        return Some(seg);
                    }
                }

                // Fallback: parse directly
                match ids_node.get_type() {
                    MCAST_INT => McInt::new(&ids_node).map(|int| IdsSegment::Int(Box::new(int))),
                    MCAST_ID | MCAST_IDA => {
                        McIda::new(&ids_node).map(|ida| IdsSegment::Ida(Box::new(ida)))
                    }
                    MCAST_IDS => {
                        // MCAST_IDS like [24,25] should be recursively parsed as nested square
                        Self::parse_square(&ids_node)
                    }
                    MCAST_EXPRESSION => {
                        // Handle expressions like 1:2 inside square brackets [1:2]
                        if let Some(exp_sub) = ids_node.get_sub_node() {
                            if exp_sub.get_type() == MCAST_OPD_COLON {
                                // Extract from and to for Slice
                                let from = exp_sub.get_sub_node().and_then(|n| McInt::new(&n));
                                let to = exp_sub
                                    .get_sub_node()
                                    .and_then(|n| n.get_next())
                                    .and_then(|n| McInt::new(&n));
                                if let (Some(f), Some(t)) = (from, to) {
                                    return Some(IdsSegment::Slice {
                                        from: Box::new(f),
                                        to: Box::new(t),
                                    });
                                }
                            }
                        }
                        None
                    }
                    MCAST_OPD_COLON => {
                        // Handle colon range like 1:2 directly
                        let left = ids_node.get_sub_node()?;
                        let right = left.get_next()?;
                        if let (Some(from), Some(to)) = (McInt::new(&left), McInt::new(&right)) {
                            return Some(IdsSegment::Slice {
                                from: Box::new(from),
                                to: Box::new(to),
                            });
                        }
                        None
                    }
                    _ => None,
                }
            })
            .collect::<Vec<_>>();

        Some(IdsSegment::Square(square_segs))
    }

    pub fn len(&self) -> usize {
        self.segments
            .iter()
            .map(|seg| match seg {
                IdsSegment::Int(int) => int.to_string().len(),
                IdsSegment::Ida(ida) => ida.len(),
                IdsSegment::DotInt(int) => int.to_string().len() + 1,
                IdsSegment::DotIda(ida) => ida.to_string().len() + 1,
                IdsSegment::Curly(curly_segs) => {
                    curly_segs
                        .iter()
                        .map(|ids| {
                            // Calculate length for each segment inside curly braces
                            match ids {
                                IdsSegment::Int(int) => int.to_string().len(),
                                IdsSegment::Ida(ida) => ida.len(),
                                IdsSegment::Slice { from, to } => {
                                    // Slice format like "1:10", calculate its string length
                                    format!("{}:{}", from.value, to.value).len()
                                }
                                _ => ids.to_string().len(),
                            }
                        })
                        .sum::<usize>()
                        + 1
                }
                IdsSegment::Square(square_segs) => {
                    square_segs
                        .iter()
                        .map(|ids| ids.to_string().len())
                        .sum::<usize>()
                        + 2
                }
                IdsSegment::Slice { from, to } => format!("{}:{}", from.value, to.value).len(),
            })
            .sum::<usize>()
    }

    /// Get base name (without curly brace part)
    /// Example DC4{VDD, GND} returns "DC4"
    pub fn base_name(&self) -> String {
        let mut result = String::new();
        for seg in &self.segments {
            match seg {
                IdsSegment::Curly(_) | IdsSegment::Square(_) => break,
                IdsSegment::Int(int) => result.push_str(&int.to_string()),
                IdsSegment::Ida(ida) => {
                    // For Ida, only take the original prefix before square brackets, e.g., PWR_[VDD2, GND2] -> PWR_
                    result.push_str(ida.prefix());
                }
                IdsSegment::DotInt(num) => {
                    result.push('.');
                    result.push_str(&num.value.to_string());
                }
                IdsSegment::DotIda(ida) => {
                    result.push('.');
                    result.push_str(&ida.to_string());
                }
                IdsSegment::Slice { from, to } => {
                    result.push_str(&format!("{}:{}", from.value, to.value));
                }
            }
        }
        result
    }

    /// Check if it contains square bracket segment (Square)
    /// DC4{VDD, GND} returns false (only Curly)
    /// PWR_[VDD2, GND2] returns true (contains Square)
    pub fn has_square(&self) -> bool {
        self.segments.iter().any(|seg| match seg {
            IdsSegment::Square(_) => true,
            IdsSegment::Ida(ida) => ida.has_square(),
            IdsSegment::DotIda(ida) => ida.has_square(),
            _ => false,
        })
    }

    /// Only get prefix (don't expand, just take original string before square brackets)
    /// Example DC4{VDD, GND} returns "DC4", PWR_[VDD2, GND2] returns "PWR_"
    pub fn prefix_only(&self) -> String {
        let mut result = String::new();
        for seg in &self.segments {
            match seg {
                IdsSegment::Curly(_) | IdsSegment::Square(_) => break,
                IdsSegment::Int(int) => result.push_str(&int.to_string()),
                IdsSegment::Ida(ida) => {
                    // For Ida, only take the part before square brackets in the original string
                    result.push_str(ida.prefix());
                }
                IdsSegment::DotInt(num) => {
                    result.push('.');
                    result.push_str(&num.value.to_string());
                }
                IdsSegment::DotIda(ida) => {
                    result.push('.');
                    result.push_str(&ida.to_string());
                }
                IdsSegment::Slice { from, to } => {
                    result.push_str(&format!("{}:{}", from.value, to.value));
                }
            }
        }
        result
    }

    /// Check if only has square bracket segment (Square), no other prefix
    /// [VDD1, GND1] returns true
    /// PWR_[VDD2, GND2] returns false (because has prefix PWR_)
    pub fn is_square_only(&self) -> bool {
        self.segments.len() == 1 && matches!(&self.segments[0], IdsSegment::Square(_))
    }

    /// Get the last segment
    pub fn last_segment(&self) -> Option<&IdsSegment> {
        self.segments.last()
    }

    /// Check if the last segment is a curly bracket
    pub fn is_curly_bracket(&self) -> bool {
        matches!(self.last_segment(), Some(IdsSegment::Curly(_)))
    }

    /// Check if the last segment is a square bracket
    pub fn is_square_bracket(&self) -> bool {
        matches!(self.last_segment(), Some(IdsSegment::Square(_)))
    }

    /// Any outer Curly group (`DC2{VDD,GND}`), anywhere in the segment list —
    /// distinct from `is_curly_bracket()` (last segment only).
    pub fn has_curly(&self) -> bool {
        self.segments
            .iter()
            .any(|seg| matches!(seg, IdsSegment::Curly(_)))
    }

    /// Any dot access (`A.B` → DotIda/DotInt), anywhere in the segment list.
    /// Exactly equivalent to `to_string().contains('.')` for outer segments,
    /// without re-parsing display output (AST-driven guideline).
    pub fn has_dot(&self) -> bool {
        self.segments
            .iter()
            .any(|seg| matches!(seg, IdsSegment::DotIda(_) | IdsSegment::DotInt(_)))
    }

    /// Count of square-bearing segments: an outer `Square` counts one, and an
    /// `Ida`/`DotIda` with an embedded square counts one. Used to detect
    /// matrix forms (`A[1:2][3:4]`, `R[1:2]C[1:3]`) where more than one
    /// segment carries a square.
    pub fn square_segment_count(&self) -> usize {
        self.segments
            .iter()
            .map(|seg| match seg {
                IdsSegment::Square(_) => 1,
                IdsSegment::Ida(ida) => usize::from(ida.has_square()),
                IdsSegment::DotIda(ida) => usize::from(ida.has_square()),
                _ => 0,
            })
            .sum()
    }

    /// Build a two-segment dot-chain `McIds` from already-split parts (for
    /// tokenizer paths that yield one dotted token, e.g. a single MCAST_IDA
    /// whose text contains a `.`). AST-faithful: base as `Ida`, member as
    /// `DotIda` — `from(&str)` on a dotted token keeps the dot inside one Id,
    /// which would misclassify. `to_string()` reproduces `base.member`.
    pub(crate) fn from_dot_pair(base: &str, member: &str) -> Self {
        Self {
            segments: vec![
                IdsSegment::Ida(Box::new(McIda::from(base))),
                IdsSegment::DotIda(Box::new(McIda::from(member))),
            ],
        }
    }

    pub fn expand(&self) -> Vec<String> {
        // First expand each segment to get possible string lists for each segment
        let expanded_segments: Vec<Vec<String>> = self
            .segments
            .iter()
            .map(|seg| {
                // Define expansion logic for each segment
                match seg {
                    IdsSegment::Int(int) => vec![int.to_string()],
                    IdsSegment::Ida(ida) => ida.expand(),
                    IdsSegment::DotIda(ida) => {
                        ida.expand().into_iter().map(|s| format!(".{s}")).collect()
                    }
                    IdsSegment::DotInt(num) => {
                        vec![format!(".{}", num.value)]
                    }
                    IdsSegment::Curly(curly_segs) => {
                        // For multiple segments inside curly braces, first expand each segment
                        // Example DC4{VDD, GND} -> DC4.VDD, DC4.GND
                        let mut curly_results: Vec<String> = Vec::new();
                        for curly_seg in curly_segs {
                            // Expand single segment
                            let expanded: Vec<String> = match curly_seg {
                                IdsSegment::Int(int) => vec![int.to_string()],
                                IdsSegment::Ida(ida) => ida.expand(),
                                IdsSegment::Slice { from, to } => {
                                    expand_numeric_slice(from.value, to.value)
                                        .into_iter()
                                        .map(|i| i.to_string())
                                        .collect()
                                }
                                // Other types shouldn't appear in curly braces, or need special handling
                                _ => vec![curly_seg.to_string()],
                            };
                            // Add "." before each expanded item and add to result
                            for item in expanded {
                                curly_results.push(format!(".{item}"));
                            }
                        }

                        curly_results
                    }
                    IdsSegment::Square(square_segs) => {
                        // For Square, recursively expand nested Squares to preserve grouping
                        // while flattening scalar elements
                        let mut all_groups: Vec<Vec<String>> = Vec::new();
                        let mut current_group: Vec<String> = Vec::new();

                        for inner_seg in square_segs {
                            match inner_seg {
                                IdsSegment::Square(inner_square_segs) => {
                                    // Nested Square - recursively expand to get groups
                                    // First save current group if non-empty
                                    if !current_group.is_empty() {
                                        all_groups.push(current_group);
                                        current_group = Vec::new();
                                    }
                                    // Recursively expand nested Square
                                    // We need to handle this specially since we're inside a map
                                    // The nested Square should be treated as a group
                                    let nested: Vec<String> = inner_square_segs
                                        .iter()
                                        .filter_map(|s| match s {
                                            IdsSegment::Ida(ida) => ida.expand().into_iter().next(),
                                            IdsSegment::Int(int) => Some(int.to_string()),
                                            _ => None,
                                        })
                                        .collect();
                                    all_groups.push(nested);
                                }
                                _ => {
                                    // Scalar - expand normally and add to current group
                                    let expanded: Vec<String> = match inner_seg {
                                        IdsSegment::Ida(ida) => ida.expand(),
                                        IdsSegment::Int(int) => vec![int.to_string()],
                                        IdsSegment::Slice { from, to } => {
                                            expand_numeric_slice(from.value, to.value)
                                                .into_iter()
                                                .map(|i| i.to_string())
                                                .collect()
                                        }
                                        _ => vec![inner_seg.to_string()],
                                    };
                                    current_group.extend(expanded);
                                }
                            }
                        }

                        // Handle remaining scalars in current group
                        if !current_group.is_empty() {
                            all_groups.push(current_group);
                        }

                        // If only one group, return it directly (flattened)
                        // Otherwise return all groups preserved
                        if all_groups.len() == 1 {
                            all_groups.into_iter().next().unwrap()
                        } else {
                            all_groups.into_iter().flatten().collect()
                        }
                    }
                    IdsSegment::Slice { from, to } => {
                        // Handle slice, e.g., 1:10 (declaration order; a
                        // descending 10:1 stays descending, §11.1).
                        expand_numeric_slice(from.value, to.value)
                            .into_iter()
                            .map(|i| i.to_string())
                            .collect()
                    }
                }
            })
            .collect();

        // Cartesian product of all expanded segments
        let mut results = vec![String::new()];
        for options in expanded_segments {
            let mut new_results = Vec::new();
            for base in results {
                for option in options.iter() {
                    new_results.push(format!("{base}{option}"));
                }
            }
            results = new_results;
        }

        results
    }

    /// Expand with parameter bindings (e.g., R[1:rows]C[1:cols] with rows=2, cols=10 -> R1C1, R1C2, ..., R2C10)
    pub fn expand_with_bindings(&self, bindings: &[(String, i64)]) -> Vec<String> {
        // First substitute parameters for each segment
        let substituted_segments: Vec<IdsSegment> = self
            .segments
            .iter()
            .map(|seg| self.substitute_segment(seg, bindings))
            .collect();

        // Expand using substituted segments
        let expanded_segments: Vec<Vec<String>> = substituted_segments
            .iter()
            .map(|seg| self.expand_single_segment(seg))
            .collect();

        // Cartesian product
        let mut results = vec![String::new()];
        for options in expanded_segments {
            let mut new_results = Vec::new();
            for base in results {
                for option in options.iter() {
                    new_results.push(format!("{base}{option}"));
                }
            }
            results = new_results;
        }

        results
    }

    /// Substitute parameters for a single segment
    fn substitute_segment(&self, seg: &IdsSegment, bindings: &[(String, i64)]) -> IdsSegment {
        match seg {
            IdsSegment::Ida(ida) => {
                if ida.has_param_ref() {
                    IdsSegment::Ida(Box::new(ida.substitute_bindings(bindings)))
                } else {
                    seg.clone()
                }
            }
            _ => seg.clone(),
        }
    }

    /// Expand a single segment
    fn expand_single_segment(&self, seg: &IdsSegment) -> Vec<String> {
        match seg {
            IdsSegment::Int(int) => vec![int.to_string()],
            IdsSegment::Ida(ida) => ida.expand(),
            IdsSegment::DotIda(ida) => ida.expand().into_iter().map(|s| format!(".{s}")).collect(),
            IdsSegment::DotInt(num) => vec![format!(".{}", num.value)],
            IdsSegment::Curly(curly_segs) => {
                let mut curly_results: Vec<String> = Vec::new();
                for curly_seg in curly_segs {
                    let expanded = self.expand_single_segment(curly_seg);
                    for item in expanded {
                        curly_results.push(format!(".{item}"));
                    }
                }
                curly_results
            }
            IdsSegment::Square(square_segs) => {
                let mut all_groups: Vec<Vec<String>> = Vec::new();
                let mut current_group: Vec<String> = Vec::new();

                for inner_seg in square_segs {
                    match inner_seg {
                        IdsSegment::Square(inner_square) => {
                            if !current_group.is_empty() {
                                all_groups.push(current_group.clone());
                                current_group.clear();
                            }
                            let inner_expanded = self
                                .expand_single_segment(&IdsSegment::Square(inner_square.clone()));
                            all_groups.push(inner_expanded);
                        }
                        _ => {
                            let expanded = self.expand_single_segment(inner_seg);
                            current_group.extend(expanded);
                        }
                    }
                }
                if !current_group.is_empty() {
                    all_groups.push(current_group);
                }

                if all_groups.len() == 1 {
                    all_groups.into_iter().next().unwrap()
                } else {
                    all_groups.into_iter().flatten().collect()
                }
            }
            IdsSegment::Slice { from, to } => expand_numeric_slice(from.value, to.value)
                .into_iter()
                .map(|i| i.to_string())
                .collect(),
        }
    }

    /// Number of elements after expansion
    pub fn count(&self) -> usize {
        self.expand().len()
    }

    /// Check if contains parameter references
    pub fn has_param_ref(&self) -> bool {
        for seg in &self.segments {
            if let IdsSegment::Ida(ida) = seg {
                if ida.has_param_ref() {
                    return true;
                }
            }
        }
        false
    }

    /// Determine if it's Bus type (IDA{CurlyMembers} form)
    /// Example DC1{VDD, GND} returns true
    /// Note: uC.ADC{P,N} is not Bus, this is component member interface access
    /// Note: Square form (e.g., GPIO[1:2]) is not Bus, it's Multi/List
    pub fn is_bus(&self) -> bool {
        if self.segments.len() >= 2 {
            let last = &self.segments[self.segments.len() - 1];
            // Only Curly {} form counts as Bus
            if let IdsSegment::Curly(_) = last {
                let second_last = &self.segments[self.segments.len() - 2];
                return matches!(second_last, IdsSegment::Ida(_));
            }
            // Square form (e.g., GPIO[1:2] or PDM[CLK, DATA]) is not Bus
        }
        false
    }

    /// Determine if it's Multi/List type (IDA[SquareMembers] form)
    /// Example GPIO[1:2] or PDM[CLK, DATA] returns true
    /// Also supports pure Square form, e.g., [LX, GND]
    pub fn is_list(&self) -> bool {
        if self.segments.len() >= 2 {
            let last = &self.segments[self.segments.len() - 1];
            if let IdsSegment::Square(_) = last {
                let second_last = &self.segments[self.segments.len() - 2];
                return matches!(second_last, IdsSegment::Ida(_));
            }
        }
        // Support pure Square form, e.g. [LX, GND]
        if self.segments.len() == 1 {
            if let IdsSegment::Square(_) = &self.segments[0] {
                return true;
            }
        }
        false
    }

    /// Square members embedded inside a single IDA segment (e.g.
    /// `PDM[CLK, DATA]` tokenized as one IDA by the C parser). §2.1: such
    /// names are List form — pins register as PDMCLK/PDMDATA and the bare
    /// prefix `PDM` does not exist. Returns None for other shapes so callers
    /// that only recognize outer-level Square segments are unaffected.
    pub fn embedded_square_members(&self) -> Option<Vec<String>> {
        if self.segments.len() != 1 {
            return None;
        }
        let IdsSegment::Ida(ida) = &self.segments[0] else {
            return None;
        };
        // §2.20.5: an Ida carrying more than one square segment (e.g.
        // `R[1:2]C[1:3]`) is a matrix definition, not a List with a single
        // embedded square. Return None so callers treat it as an expandable
        // name (multi-pin Cartesian product) instead of a prefixed list.
        let square_count = ida
            .segments
            .iter()
            .filter(|s| matches!(s, IdaSegment::Square(_)))
            .count();
        if square_count > 1 {
            return None;
        }
        let mut members = Vec::new();
        for seg in &ida.segments {
            if let IdaSegment::Square(items) = seg {
                for item in items {
                    match item {
                        SquareItem::Id(id) => members.push(id.clone()),
                        SquareItem::Range(start, end) => {
                            if let (Ok(from), Ok(to)) = (start.parse::<i64>(), end.parse::<i64>()) {
                                for i in expand_numeric_slice(from, to) {
                                    members.push(i.to_string());
                                }
                            } else {
                                members.push(format!("{start}:{end}"));
                            }
                        }
                    }
                }
            }
        }
        if members.is_empty() {
            None
        } else {
            Some(members)
        }
    }

    /// Get Square portion members (only valid when is_list() returns true)
    /// e.g. PDM[CLK, DATA] returns ["CLK", "DATA"]
    /// e.g. GPIO[1:4] returns ["1", "2", "3", "4"]
    pub fn list_members(&self) -> Option<Vec<String>> {
        if !self.is_list() {
            return None;
        }

        // Get Square segment (might be first or last)
        let square_segs = if self.segments.len() == 1 {
            // Pure Square form, e.g. [LX, GND]
            if let IdsSegment::Square(segs) = &self.segments[0] {
                segs
            } else {
                return None;
            }
        } else {
            // IDA[SquareMembers] form, e.g. GPIO[1:2]
            let last = &self.segments[self.segments.len() - 1];
            if let IdsSegment::Square(segs) = last {
                segs
            } else {
                return None;
            }
        };

        let mut result = Vec::new();
        for seg in square_segs {
            match seg {
                IdsSegment::Ida(ida) => result.extend(ida.expand()),
                IdsSegment::Int(int_val) => result.push(int_val.to_string()),
                IdsSegment::Slice { from, to } => {
                    for i in expand_numeric_slice(from.value, to.value) {
                        result.push(i.to_string());
                    }
                }
                _ => {}
            }
        }
        Some(result)
    }

    /// Get the Bus name and members (only valid when is_bus() returns true)
    pub fn as_bus(&self) -> Option<(String, Vec<String>)> {
        if !self.is_bus() || self.segments.len() < 2 {
            return None;
        }
        let second_last = &self.segments[self.segments.len() - 2];
        let last = &self.segments[self.segments.len() - 1];

        let name = match second_last {
            IdsSegment::Ida(ida) => {
                let expanded = ida.expand();
                if expanded.is_empty() {
                    return None;
                }
                expanded[0].clone()
            }
            _ => return None,
        };

        let members = match last {
            IdsSegment::Curly(curly_segs) => {
                let mut result = Vec::new();
                for seg in curly_segs {
                    match seg {
                        IdsSegment::Ida(ida) => result.extend(ida.expand()),
                        IdsSegment::Int(int_val) => result.push(int_val.to_string()),
                        IdsSegment::Slice { from, to } => {
                            // Expand a numeric range inside curly braces
                            // (`IO0{0:7}` → IO0_0..IO0_7), mirroring the square
                            // bracket form. Declaration order follows the slice
                            // direction (§11.1).
                            for i in expand_numeric_slice(from.value, to.value) {
                                result.push(i.to_string());
                            }
                        }
                        _ => {}
                    }
                }
                result
            }
            IdsSegment::Square(square_segs) => {
                let mut result = Vec::new();
                for seg in square_segs {
                    match seg {
                        IdsSegment::Ida(ida) => result.extend(ida.expand()),
                        IdsSegment::Int(int_val) => result.push(int_val.to_string()),
                        IdsSegment::Slice { from, to } => {
                            // Expand range to individual values in declaration
                            // order (descending stays descending, §11.1)
                            for i in expand_numeric_slice(from.value, to.value) {
                                result.push(i.to_string());
                            }
                        }
                        _ => {}
                    }
                }
                result
            }
            _ => return None,
        };

        Some((name, members))
    }

    /// Detect component member access pattern (COMPONENT.MEMBER{CurlyMembers} form)
    /// e.g. uC.ADC{P,N} returns Some(("uC", "ADC", ["P", "N"]))
    /// This pattern should not create a new instance, but should be treated as a member reference of the component
    pub fn as_component_member(&self) -> Option<(String, String, Vec<String>)> {
        if self.segments.len() >= 3 {
            let last = &self.segments[self.segments.len() - 1];
            let second_last = &self.segments[self.segments.len() - 2];
            let third_last = &self.segments[self.segments.len() - 3];

            if let (
                IdsSegment::Curly(curly_segs),
                IdsSegment::DotIda(dot_ida),
                IdsSegment::Ida(base_ida),
            ) = (last, second_last, third_last)
            {
                let component = base_ida.expand().first()?.clone();
                let member = dot_ida.expand().join(".");

                let members: Vec<String> = curly_segs
                    .iter()
                    .filter_map(|seg| match seg {
                        IdsSegment::Ida(ida) => Some(ida.expand().join(".")),
                        IdsSegment::Int(int_val) => Some(int_val.to_string()),
                        _ => None,
                    })
                    .collect();

                if !members.is_empty() {
                    return Some((component, member, members));
                }
            }
        }
        None
    }

    /// Check if operand matches target name
    pub fn match_name(&self, target: &str) -> bool {
        self.expand().iter().any(|expanded| expanded == target)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get the primary name
    pub fn get_primary_name(&self) -> Option<String> {
        if self.segments.is_empty() {
            None
        } else {
            Some(self.to_string())
        }
    }

    /// Return the root identifier of a possibly nested member path.
    ///
    /// `U_MCU.UART0.TX` resolves in module scope through `U_MCU`; the remaining
    /// segments are members of that instance and are validated separately.
    pub fn root_name(&self) -> Option<String> {
        self.segments.first().map(ToString::to_string)
    }

    /// Return all possible name forms that could appear at a usage site.
    /// For unused-parameter detection, check if ANY of these appear in the body.
    ///
    /// Forms included:
    /// 1. Canonical form: `to_string()` → "GPIO[1:2]", "rs485{A, B}"
    /// 2. Expanded individual names: `expand()` → ["GPIO1", "GPIO2"], ["rs485.A", "rs485.B"]
    /// 3. Base name: `base_name()` → "GPIO", "rs485"
    /// 4. Dot-member forms for curly bus: "rs485.A", "rs485.B"
    /// 5. DOT access base: "DC2" from "DC2.VDD"
    pub fn all_name_forms(&self) -> Vec<String> {
        let mut forms = Vec::new();

        // 1. Canonical
        forms.push(self.to_string());

        // 2. Expanded
        forms.extend(self.expand());

        // 3. Base name
        let base = self.base_name();
        if !base.is_empty() {
            forms.push(base.clone());
        }

        // 4. Curly bus → dot-member forms
        if let Some((bus_name, members)) = self.as_bus() {
            for m in &members {
                forms.push(format!("{}.{}", bus_name, m));
            }
        }

        // 5. DOT access → base name
        if let Some((d_base, _member)) = self.as_dot_access() {
            if d_base != base {
                forms.push(d_base);
            }
        }

        forms
    }

    /// Get the member list
    pub fn get_members(&self) -> Vec<&McIds> {
        // McIds does not have the concept of members, return empty list
        vec![]
    }

    /// Get the base name (without the square bracket part)
    /// e.g. GPIO[1:2] returns Some("GPIO")
    /// e.g. DC2.VDD returns None (because of .)
    pub fn get_base_name(&self) -> Option<String> {
        // Only consider single-segment IDA
        if self.segments.len() == 1 {
            match &self.segments[0] {
                IdsSegment::Ida(ida) => {
                    // Check if there is a square bracket segment
                    for seg in &ida.segments {
                        if let IdaSegment::Square(_) = seg {
                            // Has square brackets, find the preceding Id segment
                            for id_seg in &ida.segments {
                                if let IdaSegment::Id(name) = id_seg {
                                    return Some(name.clone());
                                }
                            }
                        }
                    }
                    // No square brackets
                    None
                }
                _ => None,
            }
        } else {
            // Multi-segment may be like DC2.VDD, not handled
            None
        }
    }

    /// Split a plain dot chain into its segment names — `uC.ADC.P` →
    /// `["uC", "ADC", "P"]`. Returns `None` when a curly/square group is
    /// present (the chain is not a plain dot chain) or a segment expands to
    /// multiple names. Consumers use this instead of `to_string()` +
    /// `split_once('.')` text re-parsing.
    pub fn dot_chain_parts(&self) -> Option<Vec<String>> {
        let mut parts = Vec::with_capacity(self.segments.len());
        for seg in &self.segments {
            match seg {
                IdsSegment::Ida(ida) | IdsSegment::DotIda(ida) => {
                    let expanded = ida.expand();
                    if expanded.len() != 1 {
                        return None;
                    }
                    parts.push(expanded[0].clone());
                }
                IdsSegment::Int(int) | IdsSegment::DotInt(int) => {
                    parts.push(int.value.to_string());
                }
                // Curly / Square groups are not a plain dot chain.
                _ => return None,
            }
        }
        Some(parts)
    }

    /// Detect if it is a DOT access pattern (e.g. DC2.VDD)
    /// Returns (base_name, member_name) if it is a DOT pattern, otherwise returns None
    pub fn as_dot_access(&self) -> Option<(String, String)> {
        if self.segments.len() == 2 {
            match (&self.segments[0], &self.segments[1]) {
                (IdsSegment::Ida(base), IdsSegment::DotIda(member)) => {
                    let base_name = base.expand().first()?.clone();
                    let member_name = member.expand().first()?.clone();
                    Some((base_name, member_name))
                }
                (IdsSegment::Ida(base), IdsSegment::DotInt(member)) => {
                    let base_name = base.expand().first()?.clone();
                    Some((base_name, member.value.to_string()))
                }
                _ => None,
            }
        } else {
            None
        }
    }
}

impl std::fmt::Display for McIds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let segments_str = self
            .segments
            .iter()
            .map(|seg| seg.to_string())
            .collect::<Vec<_>>()
            .join("");
        write!(f, "{segments_str}")
    }
}

impl Ord for McIds {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.to_string().cmp(&other.to_string())
    }
}

impl PartialOrd for McIds {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for IdsSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdsSegment::Int(int) => write!(f, "{}", int.value),
            IdsSegment::Ida(ida) => {
                write!(f, "{ida}")
            }
            IdsSegment::DotIda(ida) => {
                write!(f, ".{ida}")
            }
            IdsSegment::DotInt(num) => {
                write!(f, ".{}", num.value)
            }
            IdsSegment::Curly(curly_segs) => {
                write!(f, "{{")?;
                for (i, opdc) in curly_segs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{opdc}")?;
                }
                write!(f, "}}")
            }
            IdsSegment::Slice { from, to } => write!(f, "{}:{}", from.value, to.value),
            IdsSegment::Square(square_segs) => {
                write!(f, "[")?;
                for (i, seg) in square_segs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{seg}")?;
                }
                write!(f, "]")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sem_mcids__display_base_members_scalar() {
        assert_eq!(
            display_base_members("vin"),
            ("vin".to_string(), Vec::<String>::new())
        );
        assert_eq!(
            display_base_members("@0"),
            ("@0".to_string(), Vec::<String>::new())
        );
    }

    #[test]
    fn sem_mcids__display_base_members_curly_group() {
        // Canonical module-port key forms: named curly bus / interface ports.
        assert_eq!(
            display_base_members("vin{POWER_SYS, GND}"),
            (
                "vin".to_string(),
                vec!["POWER_SYS".to_string(), "GND".to_string()]
            )
        );
        assert_eq!(
            display_base_members("dc{VDD_3V3, GND}"),
            (
                "dc".to_string(),
                vec!["VDD_3V3".to_string(), "GND".to_string()]
            )
        );
    }

    #[test]
    fn sem_mcids__display_base_members_square_group() {
        // Square-only module-port key / reference (`dcdc.[VDD_3V3, GND]`
        // dotted member arrives here with an empty base).
        assert_eq!(
            display_base_members("[VDD_3V3, GND]"),
            (
                String::new(),
                vec!["VDD_3V3".to_string(), "GND".to_string()]
            )
        );
        // A named square keeps the base text and its members.
        assert_eq!(
            display_base_members("PWR_[VDD2, GND2]"),
            (
                "PWR_".to_string(),
                vec!["VDD2".to_string(), "GND2".to_string()]
            )
        );
    }

    #[test]
    fn sem_mcids__display_base_members_slice_token_is_single_member() {
        // Raw member text: a `1:2` slice stays one lane (member_set is the
        // only place slices expand).
        assert_eq!(
            display_base_members("[1:2]"),
            (String::new(), vec!["1:2".to_string()])
        );
    }

    #[test]
    fn sem_mcids__display_base_members_empty_group_keeps_base() {
        assert_eq!(
            display_base_members("vin{}"),
            ("vin".to_string(), Vec::<String>::new())
        );
    }

    #[test]
    fn sem_mcids__display_base_members_bare_curly() {
        assert_eq!(
            display_base_members("{A, B}"),
            (String::new(), vec!["A".to_string(), "B".to_string()])
        );
    }
}
