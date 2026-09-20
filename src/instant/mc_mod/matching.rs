// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Centralized matching algorithms and rules.
//!
//! Single home for every argument<->formal / left<->right / chain-zip pairing
//! decision in Pass2 (design: `matching-rules-design.md`):
//!
//! - vector-width checking (`check_vector_width`) — B3/B5 (P5/P6),
//! - the one positional-pairing core (`positional_pairs`: ordinal k = ordinal
//!   k) that both pairing faces cite — `pair_members_to_lanes` (B2/P3, here)
//!   and `expand_match` (§11.3, in expand.rs),
//! - ground / voltage / bracket-member name helpers.
//!
//! Rules enforced here: no implicit shape inference (P1); count mismatches are
//! hard errors, never silently dropped (P2); scalar<->vector is an error (P5);
//! an undecided passthrough variable upgrades to the formal's vector shape (P6).

use crate::instant::mc_net::NetPoint;

// Vector-width check (§3.2)

/// Outcome of checking an actual argument's width against a vector formal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WidthCheck {
    /// Equal width: `members[i]` pairs with `arg_lanes[pair[i]]` (B2).
    Pair(Vec<usize>),
    /// Actual shape is undecided (passthrough): upgrade the variable to the
    /// formal's vector shape (B5 / P6) — bind without error.
    Upgrade(Vec<String>),
    /// Scalar-to-vector or unequal-width mismatch (B3 / B4): report E4180.
    Mismatch { expected: usize, got: usize },
}

/// Check an actual argument's lane count against a multi-member vector formal.
///
/// * `members` — the formal's member names in declaration order.
/// * `arg_lanes` — the actual argument's expanded lanes.
/// * `undecided` — true when the actual is a pass-through variable whose shape
///   is not resolvable at this call site.
pub fn check_vector_width(
    members: &[String],
    arg_lanes: &[NetPoint],
    undecided: bool,
) -> WidthCheck {
    if undecided {
        return WidthCheck::Upgrade(members.to_vec());
    }
    if members.len() == arg_lanes.len() {
        return WidthCheck::Pair(pair_members_to_lanes(members, arg_lanes));
    }
    WidthCheck::Mismatch {
        expected: members.len(),
        got: arg_lanes.len(),
    }
}

// The one positional-pairing core (§3.1 B2 / §4 Z1/Z2 / §11.3)

/// Pair ordinal k on the two sides with ordinal k: the one executable form of
/// the positional law (interface-connect rule, ruling of 2026-09-19; vec-dianlu
/// §11.3). Names are each side's local view, never a matching criterion.
///
/// The core states the pairing only — `pairs[k] = (k, k)` for
/// `k < min(lhs_len, rhs_len)`. The count guard (equal width, non-empty) and
/// its error codes belong to each calling face: `pair_members_to_lanes` pads
/// the shorter side, `expand_match` (in expand.rs) refuses a count mismatch.
/// No implicit shape repair lives here (P1/P2).
pub fn positional_pairs(lhs_len: usize, rhs_len: usize) -> Vec<(usize, usize)> {
    (0..lhs_len.min(rhs_len)).map(|k| (k, k)).collect()
}

// Member <-> lane pairing (§3.1 B2 / §11.3)

/// Pair a formal's member names (declaration order) with actual argument
/// lanes **positionally**: `members[i]` binds `arg_lanes[i]` (write order).
/// Member names are each side's local view, never a matching criterion
/// (interface-connect rule, ruling of 2026-09-19; the former name-first pass
/// with positional fallback is removed). Returns, in member order, the index
/// into `arg_lanes` paired with each member (`usize::MAX` when a member has
/// no partner lane).
pub fn pair_members_to_lanes(members: &[String], arg_lanes: &[NetPoint]) -> Vec<usize> {
    let mut lanes = vec![usize::MAX; members.len()];
    for (m, l) in positional_pairs(members.len(), arg_lanes.len()) {
        lanes[m] = l;
    }
    lanes
}

// Name helpers

/// "[VDD_3V3, GND]" / "[VCC_1V2,GND]" -> ["VDD_3V3","GND"]; non-bracket -> []
pub fn parse_bracket_members(name: &str) -> Vec<String> {
    let s = name.trim();
    if !(s.starts_with('[') && s.ends_with(']')) {
        return Vec::new();
    }
    s[1..s.len() - 1]
        .split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}
