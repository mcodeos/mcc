// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Reference/declaration form classification (resolve-gate-design.md §1.2①).
//!
//! `classify(&McIds) -> Form` derives the syntactic form class straight from
//! the McIds AST segment tree — never from `to_string()` / display output
//! (AST-driven guideline). It distinguishes the two square-bracket families
//! (`List` = pure/comma-list `[A,B]` vs `Array` = prefix+range `res[1:2]` vs
//! `Indexed` = prefix+single `res[4]`), the dot chain, and curly groups,
//! including square brackets embedded inside a single IDA token
//! (`PDM[CLK,DATA]`, `res[1:2]` — the C parser tokenizes these as one Ida).
//!
//! `RefVerdict` (§1.2②) is the outcome of a single reference-resolution entry
//! (`HasFindInst::resolve_reference`), which converges the Phase 1 gate's
//! four duplicated miss-decision trees.

use super::mc_ida::{IdaSegment, McIda, SquareItem};
use super::mc_ids::{IdsSegment, McIds};

/// Reference/declaration form class, derived from the McIds AST (§1.2①).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Form {
    /// Single-segment bare identifier (`GPIO`, `VCC`).
    Bare,
    /// Plain dot chain, 2+ segments, no curly/square (`MIC.P`, `uC.ADC.P`).
    Dotted,
    /// Pure comma-list `[A,B]` (legitimate net list, §1.3 ③) or a single
    /// embedded comma-list `PDM[CLK,DATA]`.
    List,
    /// Prefix + square range (`res[1:2]`).
    Array,
    /// Prefix + square single index (`res[4]`).
    Indexed,
    /// Curly group (`DC2{VDD,GND}`, `uC.ADC{P,N}`).
    Curly,
    /// Anything else: matrix `R[1:2]C[1:3]`, square+member `res[4].B`,
    /// square+curly combos, lone slice/dot segments.
    Mixed,
}

impl Form {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bare => "bare",
            Self::Dotted => "dotted",
            Self::List => "list",
            Self::Array => "array",
            Self::Indexed => "indexed",
            Self::Curly => "curly",
            Self::Mixed => "mixed",
        }
    }
}

/// The §1.2② reference outcome, produced by `HasFindInst::resolve_reference`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefVerdict {
    /// Base resolves to an instance / port / bus / param member.
    Resolved,
    /// Array/member multi-expansion (§3 array iteration).
    #[allow(dead_code)] // Phase 3 (§3): array/member multi-expansion.
    ResolvedMany(Vec<String>),
    /// Bare single-segment scope miss → legitimate new net (§1.3 ②).
    Wire,
    /// Structured miss with an undeclared base (§1.3 ②): the caller must
    /// drop the statement; the gate candidate + UnresolvedRef(Error) ledger
    /// row are already recorded.
    UnresolvedRef {
        base: String,
        member: Option<String>,
    },
    /// Base IS a declared instance name in scope (B-family, §1.3 ①): keep
    /// the ghost-bus, defer to §3 materialization. Observable behavior of the
    /// gate sites is unchanged; only the internal dispatch moves here.
    Deferred,
}

/// Square-bracket content kind — the `List` vs `Array` vs `Indexed` split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SquareKind {
    CommaList,
    Range,
    SingleIndex,
    None,
}

/// Classify the syntactic form of `ids` from its AST segments.
pub fn classify(ids: &McIds) -> Form {
    // ── single-segment ──────────────────────────────────────────────────
    if ids.segments.len() == 1 {
        return match &ids.segments[0] {
            // Pure outer square `[A,B]` — no prefix (is_square_only).
            IdsSegment::Square(inner) => match outer_square_kind(inner) {
                SquareKind::CommaList => Form::List,
                SquareKind::Range => Form::Array,
                SquareKind::SingleIndex => Form::Indexed,
                SquareKind::None => Form::Mixed,
            },
            IdsSegment::Ida(ida) => classify_embedded_square(ida),
            IdsSegment::Int(_) => Form::Bare,
            // Lone Dot*/Slice segments — no sane reference form.
            _ => Form::Mixed,
        };
    }

    // ── multi-segment ───────────────────────────────────────────────────
    let has_square = ids.has_square();
    let has_curly = ids.has_curly();
    let is_plain_dot = ids.dot_chain_parts().is_some();
    match (has_square, has_curly, is_plain_dot) {
        (false, false, true) => Form::Dotted,
        (false, true, _) => Form::Curly,
        (true, true, _) => Form::Mixed,     // A[B,C]{D,E}
        (true, false, true) => Form::Mixed, // res[4].B
        (true, false, false) => {
            // Square(s) present, no curly, not a plain dot chain.
            if ids.square_segment_count() > 1 {
                Form::Mixed // A[1:2][3:4] matrix
            } else {
                match ids.segments.last() {
                    Some(IdsSegment::Square(inner)) => match outer_square_kind(inner) {
                        SquareKind::CommaList => Form::List,
                        SquareKind::Range => Form::Array,
                        SquareKind::SingleIndex => Form::Indexed,
                        SquareKind::None => Form::Mixed,
                    },
                    _ => Form::Mixed,
                }
            }
        }
        (false, false, false) => Form::Mixed,
    }
}

/// Classify a single `Ida` by its embedded square segments (`res[4]`,
/// `PDM[CLK,DATA]`, `R[1:2]C[1:3]`).
fn classify_embedded_square(ida: &McIda) -> Form {
    let square_count = ida
        .segments
        .iter()
        .filter(|s| matches!(s, IdaSegment::Square(_)))
        .count();
    match square_count {
        0 => Form::Bare,
        1 => {
            let items = ida
                .segments
                .iter()
                .find_map(|s| match s {
                    IdaSegment::Square(items) => Some(items),
                    _ => None,
                })
                .unwrap();
            match embedded_square_kind(items) {
                SquareKind::CommaList => Form::List,
                SquareKind::Range => Form::Array,
                SquareKind::SingleIndex => Form::Indexed,
                SquareKind::None => Form::Mixed,
            }
        }
        _ => Form::Mixed, // R[1:2]C[1:3] matrix
    }
}

/// Outer `IdsSegment::Square` content kind.
fn outer_square_kind(inner: &[IdsSegment]) -> SquareKind {
    match inner.len() {
        0 => SquareKind::None,
        1 => match &inner[0] {
            IdsSegment::Slice { .. } => SquareKind::Range,
            IdsSegment::Int(_) => SquareKind::SingleIndex,
            IdsSegment::Ida(ida) => {
                if ida.has_square() {
                    SquareKind::None // nested square — odd, treat as Mixed
                } else {
                    SquareKind::SingleIndex
                }
            }
            _ => SquareKind::None,
        },
        _ => SquareKind::CommaList,
    }
}

/// Embedded `IdaSegment::Square` content kind.
fn embedded_square_kind(items: &[SquareItem]) -> SquareKind {
    match items.len() {
        0 => SquareKind::None,
        1 => match &items[0] {
            SquareItem::Range(_, _) => SquareKind::Range,
            SquareItem::Id(_) => SquareKind::SingleIndex,
        },
        _ => SquareKind::CommaList,
    }
}

/// Extract the reference base (root) and optional member for a form, reusing
/// the exact AST helpers the gate sites already use (`dot_chain_parts`,
/// `as_bus`), so the base derived here can never diverge from the caller's.
pub(crate) fn reference_parts(ids: &McIds, form: Form) -> (String, Option<String>) {
    match form {
        Form::Dotted => match ids.dot_chain_parts() {
            Some(parts) if !parts.is_empty() => {
                let member = if parts.len() >= 2 {
                    Some(parts[1..].join("."))
                } else {
                    None
                };
                (parts[0].clone(), member)
            }
            _ => (ids.to_string(), None),
        },
        Form::Curly => match ids.as_bus() {
            Some((name, _members)) => (name, None),
            None => (ids.base_name(), None),
        },
        Form::Array | Form::Indexed => {
            let base = ids.base_name();
            if base.is_empty() {
                (ids.to_string(), None)
            } else {
                (base, None)
            }
        }
        Form::Bare | Form::List | Form::Mixed => (ids.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::basic::mc_literal::McInt;

    fn ids(segments: Vec<IdsSegment>) -> McIds {
        McIds { segments }
    }
    fn ida(s: &str) -> Box<McIda> {
        Box::new(McIda::from(s))
    }
    fn int(v: i64) -> Box<McInt> {
        Box::new(McInt { value: v })
    }
    fn square(inner: Vec<IdsSegment>) -> IdsSegment {
        IdsSegment::Square(inner)
    }
    fn curly(inner: Vec<IdsSegment>) -> IdsSegment {
        IdsSegment::Curly(inner)
    }
    fn slice(from: i64, to: i64) -> IdsSegment {
        IdsSegment::Slice {
            from: int(from),
            to: int(to),
        }
    }

    #[test]
    fn sem_form__bare_single_identifiers() {
        assert_eq!(classify(&McIds::from("GPIO")), Form::Bare);
        assert_eq!(classify(&McIds::from("VCC")), Form::Bare);
    }

    #[test]
    fn sem_form__dotted_dot_chains() {
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("A")),
                IdsSegment::DotIda(ida("B")),
            ])),
            Form::Dotted
        );
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("A")),
                IdsSegment::DotIda(ida("B")),
                IdsSegment::DotIda(ida("C")),
            ])),
            Form::Dotted
        );
        // K.45 — dotted numeric member.
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("K")),
                IdsSegment::DotInt(int(45))
            ])),
            Form::Dotted
        );
    }

    #[test]
    fn sem_form__list_pure_and_embedded_square() {
        // Pure outer square [A, B] — no prefix.
        assert_eq!(
            classify(&ids(vec![square(vec![
                IdsSegment::Ida(ida("A")),
                IdsSegment::Ida(ida("B")),
            ])])),
            Form::List
        );
        // Embedded comma-list in a single Ida token.
        assert_eq!(classify(&McIds::from("PDM[CLK,DATA]")), Form::List);
    }

    #[test]
    fn sem_form__array_prefix_range() {
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("GPIO")),
                square(vec![slice(1, 2)])
            ])),
            Form::Array
        );
        assert_eq!(classify(&McIds::from("res[1:2]")), Form::Array);
    }

    #[test]
    fn sem_form__indexed_prefix_single() {
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("res")),
                square(vec![IdsSegment::Int(int(4))])
            ])),
            Form::Indexed
        );
        assert_eq!(classify(&McIds::from("res[4]")), Form::Indexed);
    }

    #[test]
    fn sem_form__curly_bus_and_component_member() {
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("DC2")),
                curly(vec![
                    IdsSegment::Ida(ida("VDD")),
                    IdsSegment::Ida(ida("GND"))
                ]),
            ])),
            Form::Curly
        );
        // uC.ADC{P,N} — component-member curly.
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("uC")),
                IdsSegment::DotIda(ida("ADC")),
                curly(vec![IdsSegment::Ida(ida("P")), IdsSegment::Ida(ida("N"))]),
            ])),
            Form::Curly
        );
    }

    #[test]
    fn sem_form__mixed_matrices_and_combos() {
        // Matrix declaration — two embedded squares.
        assert_eq!(classify(&McIds::from("R[1:2]C[1:3]")), Form::Mixed);
        // Outer matrix — two square-bearing segments.
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("A")),
                square(vec![slice(1, 2)]),
                square(vec![slice(3, 4)]),
            ])),
            Form::Mixed
        );
        // Square + curly combo.
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("A")),
                square(vec![IdsSegment::Ida(ida("B")), IdsSegment::Ida(ida("C"))]),
                curly(vec![IdsSegment::Ida(ida("D"))]),
            ])),
            Form::Mixed
        );
        // Indexed element + member (res[4].B).
        assert_eq!(
            classify(&ids(vec![
                IdsSegment::Ida(ida("res")),
                square(vec![IdsSegment::Int(int(4))]),
                IdsSegment::DotIda(ida("B")),
            ])),
            Form::Mixed
        );
    }

    #[test]
    fn sem_form__reference_parts_base_and_member() {
        let dotted = ids(vec![
            IdsSegment::Ida(ida("uC")),
            IdsSegment::DotIda(ida("ADC")),
            IdsSegment::DotIda(ida("P")),
        ]);
        assert_eq!(
            reference_parts(&dotted, Form::Dotted),
            ("uC".to_string(), Some("ADC.P".to_string()))
        );
        let curly = ids(vec![
            IdsSegment::Ida(ida("DC2")),
            curly(vec![
                IdsSegment::Ida(ida("VDD")),
                IdsSegment::Ida(ida("GND")),
            ]),
        ]);
        assert_eq!(
            reference_parts(&curly, Form::Curly),
            ("DC2".to_string(), None)
        );
        let arr = ids(vec![IdsSegment::Ida(ida("res")), square(vec![slice(1, 2)])]);
        assert_eq!(
            reference_parts(&arr, Form::Array),
            ("res".to_string(), None)
        );
    }

    #[test]
    fn sem_form__from_dot_pair_roundtrips_text() {
        let pair = McIds::from_dot_pair("MISSING", "PIN");
        assert_eq!(classify(&pair), Form::Dotted);
        assert_eq!(pair.to_string(), "MISSING.PIN");
        assert_eq!(
            reference_parts(&pair, Form::Dotted),
            ("MISSING".to_string(), Some("PIN".to_string()))
        );
    }
}
