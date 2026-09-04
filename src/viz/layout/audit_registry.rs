// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! VizLayout rule registry — self-describing static table, owned by viz.
//!
//! Stage-4 architecture (check-rule-registry-design v0.7, stage 4): the viz
//! layout checks are registered **inside viz**, not copied into the central
//! numeric catalog in `crate::rules`. Reasons:
//!
//!   * The A-series layout invariants carry string ids ("A1".."A34") and
//!     milestone gates, not errcode numbers; the central catalog keys on
//!     numeric codes.
//!   * viz does not read `crate::rules` today; copying rows into it would
//!     fabricate a coupling that does not exist.
//!   * Execution stays in the viz pipeline (`audit_equi_tree` /
//!     `select::fidelity_gate`); this table is registration-only metadata that
//!     a read-only top-level aggregation can query without owning the rows.
//!
//! The table order is authoritative: the A rows reproduce the
//! [`super::equi_audit::audit_equi_tree`] collection order byte-for-byte, and
//! the F rows follow for the fidelity gate tiers. Every A-row `id`/`name`/
//! `since` mirrors the corresponding `Check::new(..)` / `Check::skipped(..)`
//! constructor in `equi_audit.rs`; keeping the two in sync is enforced by the
//! order/uniqueness locks in `crate::rules::tests` and by this module's tests.

use super::equi_audit::Milestone;

/// One registered VizLayout rule row.
///
/// Registration-only metadata: nothing in this crate executes from the table.
/// `severity` is the governance default the rule would carry if it emitted a
/// finding; the fidelity tiers map directly onto the gate semantics
/// (blocking / ratchet / informational). A-series rows are all `error`
/// invariants. `computable == false` marks rules declared but not yet
/// computable (A5/A6 wait on the column model) — they keep their row so the
/// catalog answers "declared" queries while the run-time report stays
/// `Skipped`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VizAuditRule {
    /// String id, e.g. "A1", "A2b", "F1". Distinct from errcode codes by design.
    pub id: &'static str,
    /// One-line invariant / gate text, identical to the run-time check name.
    pub name: &'static str,
    /// Milestone the invariant must hold from (A rows) or the gate baseline
    /// (F rows: the fidelity gate has applied since the first layout iteration).
    pub since: Milestone,
    /// Governance default severity: "error", "warning" or "info".
    pub severity: &'static str,
    /// Whether the rule is computable today. `false` = declared, not yet
    /// computable (run-time status stays `Skipped`).
    pub computable: bool,
    /// Owner inside the viz pipeline, e.g. "equi_audit::check_a1_rows".
    pub host: &'static str,
}

macro_rules! a_rule {
    ($id:literal, $name:literal, $since:ident, $host:literal) => {
        VizAuditRule {
            id: $id,
            name: $name,
            since: Milestone::$since,
            severity: "error",
            computable: true,
            host: $host,
        }
    };
}

/// A-series rows in `audit_equi_tree` collection order (§5-5: the runner
/// order is the table order), followed by the F-series fidelity tiers.
pub static VIZ_AUDIT_RULES: &[VizAuditRule] = &[
    // ── A series: layout invariants (equi_audit), collection order ──────────
    a_rule!("A1", "rows_fallback == 0", M2, "equi_audit::check_a1_rows"),
    a_rule!(
        "A2",
        "lane(layout) == lane(render replay)",
        M2,
        "equi_audit::check_a2_lane_replay"
    ),
    a_rule!(
        "A2b",
        "anchor(layout) == anchor(render replay)",
        M2,
        "equi_audit::check_a2b_anchor_replay"
    ),
    a_rule!(
        "A3",
        "no dangling segment endpoints",
        M0,
        "equi_audit::check_a3_dangling"
    ),
    a_rule!(
        "A4",
        "two-pin passive orientation matches pins",
        M3,
        "equi_audit::check_a4_passive_orientation"
    ),
    VizAuditRule {
        id: "A5",
        name: "cols unique within a row",
        since: Milestone::M4,
        severity: "error",
        computable: false,
        host: "equi_audit::check_a5_col_unique",
    },
    VizAuditRule {
        id: "A6",
        name: "bridge endpoints share a column",
        since: Milestone::M4,
        severity: "error",
        computable: false,
        host: "equi_audit::check_a6_bridge_same_col",
    },
    a_rule!(
        "A7",
        "no wire passes through a foreign box",
        M3,
        "equi_audit::check_a7_wire_through_box"
    ),
    a_rule!(
        "A8",
        "multi-tap nets carry a junction dot",
        M5,
        "equi_audit::check_a8_junction_present"
    ),
    a_rule!(
        "A9",
        "one ground glyph per ground net",
        M0,
        "equi_audit::check_a9_ground_glyphs"
    ),
    a_rule!(
        "A10",
        "same-side rows exclusive, no foreign member",
        M2,
        "equi_audit::check_a10_same_side_rows"
    ),
    a_rule!(
        "A11",
        "same row implies W/E opposite sides",
        M3,
        "equi_audit::check_a11_same_row_opposite"
    ),
    a_rule!(
        "A12",
        "row bands do not overlap",
        M3,
        "equi_audit::check_a12_row_band_overlap"
    ),
    a_rule!(
        "A13",
        "no overlapping pin slots",
        M2,
        "equi_audit::check_a13_pin_overlap"
    ),
    a_rule!(
        "A14",
        "pin labels fit inside the box",
        M2,
        "equi_audit::check_a14_label_fit"
    ),
    a_rule!(
        "A15",
        "ground stub is short",
        M2,
        "equi_audit::check_a15_ground_band"
    ),
    a_rule!(
        "A16",
        "ground net count conserved",
        M6,
        "equi_audit::check_a16_ground_count_conservation"
    ),
    a_rule!(
        "A17",
        "symbol text overlaps no box / foreign wire",
        M3_5,
        "equi_audit::check_a17_text_overlap"
    ),
    a_rule!(
        "A18",
        "no wire runs collinear with a box edge",
        M3_5,
        "equi_audit::check_a18_wire_collinear_edge"
    ),
    a_rule!(
        "A21",
        "same-row members do not collide",
        M4,
        "equi_audit::check_a21_members_do_not_overlap"
    ),
    a_rule!(
        "A22",
        "cross-row member sits in both trunk spans",
        M4,
        "equi_audit::check_a22_spanning_member_in_span"
    ),
    a_rule!(
        "A23",
        "shunt sits next to its decoupled pin",
        M4,
        "equi_audit::check_a23_shunt_near_anchor_pin"
    ),
    a_rule!(
        "A24",
        "same-side members do not cross",
        M5,
        "equi_audit::check_a24_no_wire_crossings"
    ),
    a_rule!(
        "A25",
        "label text clears foreign member boxes",
        M5,
        "equi_audit::check_a25_label_clear_of_members"
    ),
    a_rule!(
        "A26",
        "shunt up/down balance on a row",
        M5,
        "equi_audit::check_a26_shunt_balance"
    ),
    a_rule!(
        "A27",
        "IC side pin sits on its net's row",
        M6,
        "equi_audit::check_a27_pin_on_its_row"
    ),
    a_rule!(
        "A28",
        "Along part's two nets share a row",
        M6,
        "equi_audit::check_a28_along_is_collinear"
    ),
    a_rule!(
        "A29",
        "run trunks do not overlap",
        M6,
        "equi_audit::check_a29_run_spans_disjoint"
    ),
    a_rule!(
        "A30",
        "satellite facing pin sits on its row",
        M6,
        "equi_audit::check_a30_satellite_pins_on_rows"
    ),
    a_rule!(
        "A34",
        "every pin lies on its net's row",
        M7,
        "equi_audit::check_a34_every_pin_on_its_row"
    ),
    a_rule!(
        "A31",
        "a row has at most two horizontal ends",
        M7,
        "equi_audit::check_a31_row_end_budget"
    ),
    a_rule!(
        "A32",
        "a label is pulled off its wire",
        M7,
        "equi_audit::check_a32_label_has_a_stub"
    ),
    // F series: fidelity gate tiers (layout::select::fidelity_gate)
    VizAuditRule {
        id: "F1",
        name: "Tier 1 CORRECTNESS: no dropped/partial nets, every pin rendered, bus bits paired",
        since: Milestone::M0,
        severity: "error",
        computable: true,
        host: "layout::select::fidelity_gate[Tier 1]",
    },
    VizAuditRule {
        id: "F2",
        name: "Tier 2 QUALITY: zero box/wire collisions, full layout-model coverage",
        since: Milestone::M0,
        severity: "warning",
        computable: true,
        host: "layout::select::fidelity_gate[Tier 2]",
    },
    VizAuditRule {
        id: "F3",
        name: "Tier 3 INFO: authored pin sides honored; readability report",
        since: Milestone::M0,
        severity: "info",
        computable: true,
        host: "layout::select::fidelity_gate[Tier 3]",
    },
];

/// Read-only access to every registered VizLayout rule, in table order.
pub fn viz_audit_rules() -> &'static [VizAuditRule] {
    VIZ_AUDIT_RULES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_set_size_is_32_plus_3() {
        // "~34" declared rules: the 32 A-series rows (A1..A34 with A2b and the
        // A19/A20/A33 gaps) plus the three fidelity gate tiers.
        assert_eq!(VIZ_AUDIT_RULES.len(), 35);
        let a_rows = VIZ_AUDIT_RULES
            .iter()
            .filter(|r| r.id.starts_with('A'))
            .count();
        let f_rows = VIZ_AUDIT_RULES
            .iter()
            .filter(|r| r.id.starts_with('F'))
            .count();
        assert_eq!(a_rows, 32);
        assert_eq!(f_rows, 3);
    }

    #[test]
    fn ids_are_unique_and_names_are_unique() {
        let mut ids: Vec<&str> = VIZ_AUDIT_RULES.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        assert!(ids.windows(2).all(|w| w[0] != w[1]));
        let mut names: Vec<&str> = VIZ_AUDIT_RULES.iter().map(|r| r.name).collect();
        names.sort_unstable();
        assert!(names.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn a5_a6_are_declared_but_not_computable() {
        for id in ["A5", "A6"] {
            let row = VIZ_AUDIT_RULES.iter().find(|r| r.id == id).unwrap();
            assert!(!row.computable, "{id} waits on the column model");
            assert_eq!(row.severity, "error");
            assert_eq!(row.since, Milestone::M4);
        }
    }

    #[test]
    fn fidelity_tiers_carry_the_gate_levels() {
        let f1 = VIZ_AUDIT_RULES.iter().find(|r| r.id == "F1").unwrap();
        assert_eq!(f1.severity, "error");
        let f2 = VIZ_AUDIT_RULES.iter().find(|r| r.id == "F2").unwrap();
        assert_eq!(f2.severity, "warning");
        let f3 = VIZ_AUDIT_RULES.iter().find(|r| r.id == "F3").unwrap();
        assert_eq!(f3.severity, "info");
        for f in VIZ_AUDIT_RULES.iter().filter(|r| r.id.starts_with('F')) {
            assert_eq!(f.since, Milestone::M0);
            assert!(f.computable);
        }
    }
}
