// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! First-class operand shape (vec-arch.md §4.1).
//!
//! `OpdShape` is the single source of truth for the shape of a connection
//! operand, covering the five shapes in vec-dianlu.md §2.1 plus `Unknown`.
//! It records **what kind** of vector an operand is (point / row vector /
//! column vector / node), not just its row count — the current `Shape { rows }`
//! only stores the latter and cannot tell a two-pin row vector from a
//! single point.
//!
//! Each variant stores `McBus` elements directly (`name` + `member` +
//! `full_members`). Reusing `McBus` keeps the Pass2 member expansion
//! (`expand_port_lanes` / `resolve_netpoint_v2`) intact — a separate `Element`
//! type holding only `name` + `members` would drop `full_members`.

use super::mc_bus::McBus;

/// Operand shape (vec-dianlu.md §2.1 / §3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpdShape {
    /// Single point `1*1`: left = right = single element.
    Point(McBus),
    /// Row vector `1*2`: left = first element, right = second element
    /// (two-pin component default).
    Row(McBus, McBus),
    /// Column vector `N*1`: left = right = all elements.
    Column(Vec<McBus>),
    /// Node `M*1, N*1`: left = left column, right = right column (may be
    /// asymmetric).
    Node(Vec<McBus>, Vec<McBus>),
    /// Unknown shape (shape-by-use / unresolved FuncCall return).
    Unknown,
}

impl OpdShape {
    /// Left port row count (vec-arch.md §4.1).
    ///
    /// `Point`/`Row` expose one element on the left; `Column` exposes every
    /// element; `Node` exposes its left column; `Unknown` is `0` (wildcard).
    ///
    /// The row count is the **leaf** count (`sum of each McBus::size`), not the
    /// `Vec` length: a single `McBus` carrying several members (e.g. a
    /// `Bus("UART0", ["TX", "RX"])` that `eval_port_elems` returns unflattened)
    /// still counts as one row per member, matching `shape_of_bus_list` and the
    /// Pass2 lane expansion.
    pub fn size_left(&self) -> usize {
        match self {
            OpdShape::Point(e) => e.size(),
            OpdShape::Row(l, _) => l.size(),
            OpdShape::Column(v) => Self::leaf_count(v),
            OpdShape::Node(l, _) => Self::leaf_count(l),
            OpdShape::Unknown => 0,
        }
    }

    /// Right port row count (vec-arch.md §4.1).
    pub fn size_right(&self) -> usize {
        match self {
            OpdShape::Point(e) => e.size(),
            OpdShape::Row(_, r) => r.size(),
            OpdShape::Column(v) => Self::leaf_count(v),
            OpdShape::Node(_, r) => Self::leaf_count(r),
            OpdShape::Unknown => 0,
        }
    }

    /// Sum of the member counts of a port element list.
    fn leaf_count(elems: &[McBus]) -> usize {
        elems.iter().map(|e| e.size()).sum()
    }

    /// Left port as a single-sided element list (vec-arch.md §5.1).
    pub fn port_left(&self) -> Vec<McBus> {
        match self {
            OpdShape::Point(e) => vec![e.clone()],
            OpdShape::Row(l, _) => vec![l.clone()],
            OpdShape::Column(v) => v.clone(),
            OpdShape::Node(l, _) => l.clone(),
            OpdShape::Unknown => Vec::new(),
        }
    }

    /// Right port as a single-sided element list (vec-arch.md §5.1).
    pub fn port_right(&self) -> Vec<McBus> {
        match self {
            OpdShape::Point(e) => vec![e.clone()],
            OpdShape::Row(_, r) => vec![r.clone()],
            OpdShape::Column(v) => v.clone(),
            OpdShape::Node(_, r) => r.clone(),
            OpdShape::Unknown => Vec::new(),
        }
    }

    /// Whether the shape is unknown (vec-arch.md §5.3 `Deferred` / `Error`
    /// both surface as `Unknown` here; the distinction is made by the caller
    /// via an explicit empty guard, not by this accessor).
    pub fn is_unknown(&self) -> bool {
        matches!(self, OpdShape::Unknown)
    }

    /// Strict math transpose (vec-arch.md §5.2 / §6.2): a column vector
    /// transposes to a row vector and vice versa, preserving member order.
    ///
    /// Returns `Err(rows)` when the transposed result has no connectable
    /// expression — a column or node side wider than 2 rows — matching the
    /// existing `check_transpose_allowed` `rows >= 3` rule (E2902). A 2-pin
    /// bridge (`Row` → `Column([L, R])`) and a 2-row column (`Column([A, B])`
    /// → `Row(A, B)`) are the two representable non-trivial cases; `Point` /
    /// `Unknown` transpose to themselves.
    pub fn transpose(&self) -> Result<OpdShape, usize> {
        match self {
            OpdShape::Point(_) | OpdShape::Unknown => Ok(self.clone()),
            // 1*2 row vector -> 2*1 column vector.
            OpdShape::Row(l, r) => Ok(OpdShape::Column(vec![l.clone(), r.clone()])),
            OpdShape::Column(v) => {
                let w = self.size_left();
                if w == 2 {
                    // 2*1 column vector -> 1*2 row vector.
                    Ok(OpdShape::Row(v[0].clone(), v[1].clone()))
                } else if w > 2 {
                    Err(w)
                } else {
                    // A degenerate single-element column behaves as a point.
                    Ok(self.clone())
                }
            }
            OpdShape::Node(_, _) => {
                // Each side column transposes to a row vector; a row vector is
                // only connectable when its width is <= 2 (vec-arch.md §5.2).
                let lw = self.size_left();
                let rw = self.size_right();
                if lw > 2 || rw > 2 {
                    Err(lw.max(rw))
                } else {
                    Ok(self.clone())
                }
            }
        }
    }

    /// Directional reverse (vec-arch.md §5.2 / §6.3): swaps the left/right
    /// ports where they are independent (row vector / node), and is an
    /// identity for point / column / unknown.
    ///
    /// Not yet wired into a production path; kept as tested semantic API
    /// (see the `reverse_*` tests below).
    #[allow(dead_code)]
    pub fn reverse(&self) -> OpdShape {
        match self {
            OpdShape::Point(_) | OpdShape::Column(_) | OpdShape::Unknown => self.clone(),
            OpdShape::Row(l, r) => OpdShape::Row(r.clone(), l.clone()),
            OpdShape::Node(l, r) => OpdShape::Node(r.clone(), l.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus(name: &str) -> McBus {
        McBus::new(name)
    }

    #[test]
    fn sem_opdshape__point_is_single_sided() {
        let p = OpdShape::Point(bus("A"));
        assert_eq!(p.size_left(), 1);
        assert_eq!(p.size_right(), 1);
        assert_eq!(p.port_left(), vec![bus("A")]);
        assert_eq!(p.port_right(), vec![bus("A")]);
        assert!(!p.is_unknown());
    }

    #[test]
    fn sem_opdshape__row_splits_left_and_right() {
        let r = OpdShape::Row(bus("R.1"), bus("R.2"));
        assert_eq!(r.size_left(), 1);
        assert_eq!(r.size_right(), 1);
        assert_eq!(r.port_left(), vec![bus("R.1")]);
        assert_eq!(r.port_right(), vec![bus("R.2")]);
        assert!(!r.is_unknown());
    }

    #[test]
    fn sem_opdshape__column_exposes_all_on_both_sides() {
        let c = OpdShape::Column(vec![bus("TX"), bus("RX"), bus("GND")]);
        assert_eq!(c.size_left(), 3);
        assert_eq!(c.size_right(), 3);
        assert_eq!(c.port_left(), vec![bus("TX"), bus("RX"), bus("GND")]);
        assert_eq!(c.port_right(), vec![bus("TX"), bus("RX"), bus("GND")]);
        assert!(!c.is_unknown());
    }

    #[test]
    fn sem_opdshape__node_splits_asymmetric_columns() {
        let n = OpdShape::Node(vec![bus("VDD"), bus("GND")], vec![bus("VCC")]);
        assert_eq!(n.size_left(), 2);
        assert_eq!(n.size_right(), 1);
        assert_eq!(n.port_left(), vec![bus("VDD"), bus("GND")]);
        assert_eq!(n.port_right(), vec![bus("VCC")]);
        assert!(!n.is_unknown());
    }

    #[test]
    fn sem_opdshape__unknown_is_empty_wildcard() {
        let u = OpdShape::Unknown;
        assert_eq!(u.size_left(), 0);
        assert_eq!(u.size_right(), 0);
        assert!(u.port_left().is_empty());
        assert!(u.port_right().is_empty());
        assert!(u.is_unknown());
    }

    #[test]
    fn sem_opdshape__from_sides_classifies_unknown() {
        let s = OpdShape::from_sides(vec![], vec![]);
        assert_eq!(s, OpdShape::Unknown);
    }

    #[test]
    fn sem_opdshape__from_sides_classifies_point() {
        let s = OpdShape::from_sides(vec![bus("A")], vec![bus("A")]);
        assert_eq!(s, OpdShape::Point(bus("A")));
    }

    #[test]
    fn sem_opdshape__from_sides_classifies_column() {
        let s = OpdShape::from_sides(vec![bus("TX"), bus("RX")], vec![bus("TX"), bus("RX")]);
        assert_eq!(s, OpdShape::Column(vec![bus("TX"), bus("RX")]));
    }

    #[test]
    fn sem_opdshape__from_sides_classifies_row() {
        // Two distinct single-element ports (a two-pin device).
        let s = OpdShape::from_sides(vec![bus("R.1")], vec![bus("R.2")]);
        assert_eq!(s, OpdShape::Row(bus("R.1"), bus("R.2")));
    }

    #[test]
    fn sem_opdshape__from_sides_classifies_asymmetric_node() {
        let s = OpdShape::from_sides(vec![bus("VDD"), bus("GND")], vec![bus("VCC")]);
        assert_eq!(
            s,
            OpdShape::Node(vec![bus("VDD"), bus("GND")], vec![bus("VCC")])
        );
    }

    #[test]
    fn sem_opdshape__from_sides_left_empty_is_node() {
        // `return <expr>` FuncCall: no left contact, right = return value.
        let s = OpdShape::from_sides(vec![], vec![bus("OUT")]);
        assert_eq!(s, OpdShape::Node(vec![], vec![bus("OUT")]));
        assert_eq!(s.size_left(), 0);
        assert_eq!(s.size_right(), 1);
    }

    // ---- transpose / reverse (vec-arch.md §5.2 / §6.2 / §6.3) ----

    #[test]
    fn sem_opdshape__transpose_point_is_identity() {
        let p = OpdShape::Point(bus("A"));
        assert_eq!(p.transpose(), Ok(p));
    }

    #[test]
    fn sem_opdshape__transpose_row_to_column() {
        // 1*2 row vector -> 2*1 column vector (the 2-pin bridge CAP').
        let r = OpdShape::Row(bus("R.1"), bus("R.2"));
        assert_eq!(
            r.transpose(),
            Ok(OpdShape::Column(vec![bus("R.1"), bus("R.2")]))
        );
    }

    #[test]
    fn sem_opdshape__transpose_column_two_to_row() {
        // 2*1 column vector -> 1*2 row vector (the newly-added math transpose).
        let c = OpdShape::Column(vec![bus("A"), bus("B")]);
        assert_eq!(c.transpose(), Ok(OpdShape::Row(bus("A"), bus("B"))));
    }

    #[test]
    fn sem_opdshape__transpose_column_wide_is_error() {
        // 3*1 column vector has no connectable transpose -> E2902.
        let c = OpdShape::Column(vec![bus("A"), bus("B"), bus("C")]);
        assert_eq!(c.transpose(), Err(3));
    }

    #[test]
    fn sem_opdshape__transpose_node_connectable() {
        // A node whose sides are each <= 2 rows transposes to connectable row
        // vectors.
        let n = OpdShape::Node(vec![bus("VDD"), bus("GND")], vec![bus("VCC")]);
        assert_eq!(n.transpose(), Ok(n.clone()));
    }

    #[test]
    fn sem_opdshape__transpose_node_wide_is_error() {
        // A node side wider than 2 rows has no connectable transpose -> E2902.
        let n = OpdShape::Node(vec![bus("A"), bus("B"), bus("C")], vec![bus("D")]);
        assert_eq!(n.transpose(), Err(3));
    }

    #[test]
    fn sem_opdshape__transpose_unknown_is_identity() {
        assert_eq!(OpdShape::Unknown.transpose(), Ok(OpdShape::Unknown));
    }

    #[test]
    fn sem_opdshape__reverse_row_swaps_pins() {
        // R101^ -> R101{2,1}: a two-pin device reverses pin 1 / pin 2.
        let r = OpdShape::Row(bus("R101.1"), bus("R101.2"));
        assert_eq!(r.reverse(), OpdShape::Row(bus("R101.2"), bus("R101.1")));
    }

    #[test]
    fn sem_opdshape__reverse_node_swaps_sides() {
        let n = OpdShape::Node(vec![bus("VDD"), bus("GND")], vec![bus("VCC")]);
        assert_eq!(
            n.reverse(),
            OpdShape::Node(vec![bus("VCC")], vec![bus("VDD"), bus("GND")])
        );
    }

    #[test]
    fn sem_opdshape__reverse_point_column_unknown_identity() {
        assert_eq!(
            OpdShape::Point(bus("A")).reverse(),
            OpdShape::Point(bus("A"))
        );
        let c = OpdShape::Column(vec![bus("A"), bus("B")]);
        assert_eq!(c.reverse(), c);
        assert_eq!(OpdShape::Unknown.reverse(), OpdShape::Unknown);
    }
}
