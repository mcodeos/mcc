// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Centralized matching algorithms and rules.
//!
//! Single home for every argument<->formal / left<->right / chain-zip pairing
//! decision in Pass2 (design: `mcd/doc/matching-rules-design.md`):
//!
//! - vector-width checking (`check_vector_width`) — B3/B5 (P5/P6),
//! - member<->lane positional pairing (`pair_members_to_lanes`) — B2 (P3),
//! - equal-width checked zip (`zip_checked`) — Z1/Z2 (P2),
//! - ground / voltage / bracket-member name helpers.
//!
//! Rules enforced here: no implicit shape inference (P1); count mismatches are
//! hard errors, never silently dropped (P2); scalar<->vector is an error (P5);
//! an undecided passthrough variable upgrades to the formal's vector shape (P6).

use crate::instant::mc_net::NetPoint;
use crate::semantic::common::IOType;

// ============================================================================
// Vector-width check (§3.2)
// ============================================================================

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

// ============================================================================
// Member <-> lane pairing (§3.1 B2 / §11.3)
// ============================================================================

/// Pair a formal's member names (declaration order) with actual argument
/// lanes — by name first, positional fallback for the rest. Returns, in member
/// order, the index into `arg_lanes` paired with each member (`usize::MAX`
/// when a member has no partner lane).
pub fn pair_members_to_lanes(members: &[String], arg_lanes: &[NetPoint]) -> Vec<usize> {
    let mut result: Vec<usize> = Vec::with_capacity(members.len());
    let mut used = vec![false; arg_lanes.len()];
    // Pass 1: pair by name.
    for m in members {
        let hit = arg_lanes.iter().position(|a| {
            let n = a
                .member_name
                .as_deref()
                .unwrap_or_else(|| a.path.rsplit('.').next().unwrap_or(&a.path));
            n == m.as_str()
        });
        match hit {
            Some(j) if !used[j] => {
                result.push(j);
                used[j] = true;
            }
            _ => result.push(usize::MAX),
        }
    }
    // Pass 2: positional fallback for names with no partner.
    let mut rj = 0;
    for r in result.iter_mut() {
        if *r != usize::MAX {
            continue;
        }
        while rj < arg_lanes.len() && used[rj] {
            rj += 1;
        }
        if rj < arg_lanes.len() {
            *r = rj;
            used[rj] = true;
            rj += 1;
        }
    }
    result
}

// ============================================================================
// Checked zip (§4 Z1/Z2)
// ============================================================================

/// Pair two sequences positionally; equal width only (Z1). An unequal width is
/// a hard error (Z2) — never flatten and never drop excess members. Returns
/// `Err((left_len, right_len))` on mismatch.
pub fn zip_checked<A, B>(left: &[A], right: &[B]) -> Result<Vec<(A, B)>, (usize, usize)>
where
    A: Clone,
    B: Clone,
{
    if left.len() != right.len() {
        return Err((left.len(), right.len()));
    }
    Ok(left
        .iter()
        .zip(right.iter())
        .map(|(a, b)| (a.clone(), b.clone()))
        .collect())
}

// ============================================================================
// Name helpers
// ============================================================================

/// Exact ground-name matcher on the path leaf: `GND` / `AGND` / `DGND` /
/// `PGND` / `VSS` / `GROUND` / `EARTH`. Not a `starts_with` test — `GND_OUT`
/// or `VIN` must not be treated as ground (authoritative netcheck variant).
///
/// Not a matching rule: matching decisions are purely structural (lane count,
/// member pairing, scope). This helper serves the netcheck / DC-rail-identity
/// concerns only — port-ground-member detection (`check_unbound_param_ports`)
/// and `rail_ground_point`.
pub fn is_ground_name(s: &str) -> bool {
    let leaf = s.rsplit('.').next().unwrap_or(s);
    matches!(
        leaf.to_uppercase().as_str(),
        "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "GROUND" | "EARTH"
    )
}

/// Extract voltage token from a name (uppercase normalize):
///   "V3V3"->"3V3", "VDD_3V3"->"3V3", "VCC_1V2"->"1V2", "V5V"->"5V", "VDD_CORE"->None
/// Rule: match digit+ 'V' (+digit)? fragment.
pub fn voltage_token(name: &str) -> Option<String> {
    let b = name.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'V' || b[i] == b'v') {
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                return Some(name[start..i].to_uppercase());
            }
        } else {
            i += 1;
        }
    }
    None
}

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

// ============================================================================
// Ground identity (§ dc-rail-identity-design.md)
// ============================================================================

/// Strict DC rail identity: the ground member point owned by a rail scalar.
///
/// A rail scalar (`V5V`, `usbsocket.vin`) owns its ground member
/// `{rail}.GND`. Binding a DC port's ground member to this point keeps every
/// rail's ground distinct until real wiring ties them together (shared
/// component ground pins, explicit `X.GND -> GND` connections). A scalar that
/// is itself a ground reference (bare `GND` or `s.GND`) is returned unchanged —
/// the author is explicitly naming that net.
pub fn rail_ground_point(rail: &NetPoint, gnd_member: &str) -> NetPoint {
    let leaf = rail
        .member_name
        .as_deref()
        .unwrap_or_else(|| rail.path.rsplit('.').next().unwrap_or(&rail.path));
    if is_ground_name(leaf) {
        return rail.clone();
    }
    NetPoint::new(&format!("{}.{}", rail.path, gnd_member), IOType::None)
        .with_member_name(gnd_member)
}
