// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Pass2 Instantiation - Module Instance
//!
//! McModuleInst is the core data structure for module instantiation, representing a complete module instance.

// ============================================================================
// McBusInst - Bus Instance
// ============================================================================

/// Bus Instance
///
/// Stores bus structures like `power{VCC, GND}`.
#[derive(Debug, Clone)]
pub struct McBusInst {
    /// Bus name
    pub name: String,
    /// List of bus members
    pub members: Vec<String>,
}

/// Create a new bus instance
impl McBusInst {
    /// Create a new bus instance
    pub fn new(name: &str, members: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            members,
        }
    }

    /// Check if a member exists
    pub fn has_member(&self, member: &str) -> bool {
        self.members.iter().any(|m| m == member)
    }

    /// Get the index of a member
    pub fn member_index(&self, member: &str) -> Option<usize> {
        self.members.iter().position(|m| m == member)
    }
    /// Get the number of members
    pub fn size(&self) -> usize {
        self.members.len()
    }

    /// ========================================================================
    /// Iteration 3: Member Incremental Merge (Deduplicate Accumulate)
    /// ========================================================================

    /// Merge members from `incoming` into `self.members` if they don't exist.
    ///
    /// ## Semantics
    /// - **Order of existing members is preserved.**
    /// - New members are appended in the order they appear in `incoming`.
    /// - Duplicate members (by string equality) are skipped.
    /// - Returns the number of members actually added (useful for detecting no-op calls).
    ///
    /// ## Design Motivation (Iter 3 Core)
    /// Before `McModuleInst::ensure_bus`, we used to compare bus names
    /// with "equality" in "bus exists" branch. But in practice, bus
    /// access is **accumulative**:
    ///
    /// ```text
    /// uC.XTAL - X6.2          # First: ensure_bus("uC", ["XTAL"])
    /// uC.pins[8:11] - SPI.SCLK # Second: ensure_bus("uC", ["pins[8:11]"])
    /// uC.UART0 - cap4.1        # Third:  ensure_bus("uC", ["UART0"])
    /// ```
    ///
    /// Equality comparison would report false positives (WARN #921) for
    /// later contributions, and **drop new members**. The drop consequence
    /// is not just noise—it affects `InstTable` path registration
    /// (`main.mcu513.uC/pins[8:11]`, etc.), where missing members break
    /// Iter 1's fallback bus member paths, leading to silent wire loss
    /// from the graph.
    ///
    /// Append members from `incoming` that do not exist in `self.members`.
    ///
    /// ## Semantics
    /// - **Order of existing members is preserved.**
    /// - New members are appended in the order they appear in `incoming`.
    /// - Duplicate members (by string equality) are skipped.
    /// - Returns the number of members actually added (useful for detecting no-op calls).
    ///
    /// ## Design Motivation (Iter 3 Core)
    /// Prior to Iter 3, `McModuleInst::ensure_bus` used **equality comparison**
    /// when a bus already existed. This led to false positives (WARN #921) and
    /// member loss when multiple contributions were made.
    ///
    /// By changing to **incremental merging**, each access accumulates its members,
    /// eliminating false positives and preserving the order of contributions.
    pub fn merge_members(&mut self, incoming: &[String]) -> usize {
        let mut added = 0;
        for m in incoming {
            if !self.has_member(m) {
                self.members.push(m.clone());
                added += 1;
            }
        }
        added
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Test `has_member` behavior
    #[test]
    fn test_has_member() {
        let bus = McBusInst::new("power", vec!["VCC".into(), "GND".into()]);
        assert!(bus.has_member("VCC"));
        assert!(bus.has_member("GND"));
        assert!(!bus.has_member("SCL"));
    }

    // ========================================================================
    // Iteration 3: merge_members semantics
    // ========================================================================

    /// Test `merge_members` behavior
    #[test]
    fn test_merge_appends_new_members_in_order() {
        let mut bus = McBusInst::new("uC", vec!["XTAL".into()]);

        let added = bus.merge_members(&["UART0".into()]);
        assert_eq!(added, 1);
        assert_eq!(bus.members, vec!["XTAL", "UART0"]);

        let added = bus.merge_members(&["6".into(), "19".into()]);
        assert_eq!(added, 2);
        assert_eq!(bus.members, vec!["XTAL", "UART0", "6", "19"]);
    }

    /// Test `merge_members` behavior
    #[test]
    fn test_merge_skips_duplicates_noop() {
        let mut bus = McBusInst::new("MIC", vec!["P".into(), "N".into()]);
        let added = bus.merge_members(&["N".into()]);
        assert_eq!(added, 0, "redundant access should add nothing");
        assert_eq!(bus.members, vec!["P", "N"]);
    }

    /// Test `merge_members` behavior
    #[test]
    fn test_merge_partial_overlap() {
        let mut bus = McBusInst::new("b", vec!["A".into(), "B".into()]);
        let added = bus.merge_members(&["B".into(), "C".into(), "A".into(), "D".into()]);
        assert_eq!(added, 2, "only C and D are new");
        assert_eq!(bus.members, vec!["A", "B", "C", "D"]);
    }

    /// Test `merge_members` behavior
    #[test]
    fn test_merge_dedupes_within_incoming() {
        let mut bus = McBusInst::new("b", vec![]);
        let added = bus.merge_members(&["X".into(), "Y".into(), "X".into()]);
        assert_eq!(added, 2);
        assert_eq!(bus.members, vec!["X", "Y"]);
    }

    /// Test `merge_members` behavior
    #[test]
    fn test_merge_empty_incoming() {
        let mut bus = McBusInst::new("b", vec!["A".into()]);
        let added = bus.merge_members(&[]);
        assert_eq!(added, 0);
        assert_eq!(bus.members, vec!["A"]);
    }

    /// Test `merge_members` behavior
    #[test]
    fn test_merge_into_empty_bus() {
        let mut bus = McBusInst::new("b", vec![]);
        let added = bus.merge_members(&["A".into(), "B".into()]);
        assert_eq!(added, 2);
        assert_eq!(bus.members, vec!["A", "B"]);
    }
}
