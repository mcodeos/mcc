// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # Alignment metrics (M1-4 · self-test edition)
//!
//! This module provides **KiCad-independent self-test alignment metrics**:
//!
//! 1. **Rand index**: consistency of two partitions (module structure vs itself).
//!    The self-test edition uses the .mc module structure as ground truth for
//!    self-consistency validation.
//! 2. **Netlist metrics**: works with netcheck, producing a Tier 0 netlist
//!    correctness summary.
//!
//! KiCad-based alignment metrics (Kendall τ, wire/label accuracy, crossing-count
//! comparison, etc.) are deferred along with M1-3.

use std::collections::BTreeMap;

use crate::instant::insttab::InstTable;
use crate::instant::netcheck;

// ============================================================================
// Partition structures
// ============================================================================

/// A partition: assigns a set of item_ids to several cluster_ids.
#[derive(Debug, Clone)]
pub struct Partition {
    /// item_id → cluster_id mapping
    pub assignment: BTreeMap<u32, u32>,
    /// Total item count
    pub item_count: usize,
    /// Total cluster count
    pub cluster_count: usize,
}

impl Partition {
    pub fn from_assignment(assignment: BTreeMap<u32, u32>) -> Self {
        let item_count = assignment.len();
        let cluster_count = assignment.values().collect::<std::collections::BTreeSet<_>>().len();
        Self {
            assignment,
            item_count,
            cluster_count,
        }
    }

    /// Build a partition from InstTable's module structure: each Component belongs
    /// to its nearest Module.
    pub fn from_module_structure(table: &InstTable) -> Self {
        let mut assignment = BTreeMap::new();

        for comp in table.get_components() {
            // Walk upward to find the nearest Module ancestor
            let mut cur = comp.parent_id;
            let mut module_id = 0u32;
            let mut guard = 0usize;
            while let Some(p) = cur {
                guard += 1;
                if guard > 256 {
                    break;
                }
                match table.get_entry(p) {
                    Some(e) => {
                        if e.kind == crate::instant::insttab::InstKind::Module {
                            module_id = p;
                            break;
                        }
                        cur = e.parent_id;
                    }
                    None => break,
                }
            }
            assignment.insert(comp.id, module_id);
        }

        Self::from_assignment(assignment)
    }

    /// Get the cluster_id of an item_id; returns 0 when not found
    pub fn cluster_of(&self, item_id: u32) -> u32 {
        self.assignment.get(&item_id).copied().unwrap_or(0)
    }
}

// ============================================================================
// Rand index
// ============================================================================

/// Compute the Rand index between two partitions.
///
/// Rand index = (a + b) / C(n, 2), where:
/// - a = item pairs in the same cluster in both partitions
/// - b = item pairs in different clusters in both partitions
/// - n = total item count
///
/// Returns a value in 0.0 ~ 1.0; 1.0 means fully consistent.
pub fn rand_index(p1: &Partition, p2: &Partition) -> f64 {
    // Take the item intersection of the two partitions
    let items: Vec<u32> = p1
        .assignment
        .keys()
        .filter(|k| p2.assignment.contains_key(k))
        .copied()
        .collect();

    let n = items.len();
    if n < 2 {
        return 1.0;
    }

    let mut a = 0usize; // same-cluster pairs
    let mut b = 0usize; // different-cluster pairs

    for i in 0..n {
        for j in (i + 1)..n {
            let same_p1 = p1.cluster_of(items[i]) == p1.cluster_of(items[j]);
            let same_p2 = p2.cluster_of(items[i]) == p2.cluster_of(items[j]);
            if same_p1 && same_p2 {
                a += 1;
            } else if !same_p1 && !same_p2 {
                b += 1;
            }
        }
    }

    let total_pairs = n * (n - 1) / 2;
    (a + b) as f64 / total_pairs as f64
}

/// Compute the self-consistency Rand index: compares the module partition with
/// itself; should always be 1.0.
pub fn self_consistency_rand(table: &InstTable) -> f64 {
    let p = Partition::from_module_structure(table);
    rand_index(&p, &p)
}

// ============================================================================
// Alignment metrics summary
// ============================================================================

/// Alignment metrics summary report
#[derive(Debug, Default)]
pub struct AlignMetricsReport {
    /// Module count
    pub module_count: usize,
    /// Component count
    pub component_count: usize,
    /// Rand index (self-consistency, should always be 1.0)
    pub rand_self: f64,
    /// netcheck error count
    pub netcheck_errors: usize,
    /// netcheck warning count
    pub netcheck_warns: usize,
    /// Whether netcheck is clean (no ERROR)
    pub netcheck_clean: bool,
}

impl AlignMetricsReport {
    /// Compute alignment metrics from InstTable (self-test edition)
    pub fn compute(table: &InstTable) -> Self {
        let nc_report = netcheck::run(table);

        let rep = Self {
            module_count: table.get_modules().len(),
            component_count: table.get_components().len(),
            rand_self: self_consistency_rand(table),
            netcheck_errors: nc_report.error_count(),
            netcheck_warns: nc_report
                .findings
                .iter()
                .filter(|f| f.level == netcheck::Level::Warn)
                .count(),
            netcheck_clean: nc_report.is_clean(),
        };
        rep
    }

    /// Render the alignment metrics table
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(s, "┌ align_metrics ────────────────────────────────────────────────────");
        let _ = writeln!(
            s,
            "│ {} modules / {} components",
            self.module_count, self.component_count
        );
        let _ = writeln!(s, "├───────────────────────────────────────────────────────────────────");
        let _ = writeln!(
            s,
            "│ Rand index (self-consistency):  {:.4}  {}",
            self.rand_self,
            if (self.rand_self - 1.0).abs() < 1e-9 {
                "✓"
            } else {
                "✗"
            }
        );
        let _ = writeln!(s, "│");
        let _ = writeln!(s, "│ Tier 0 NETLIST CORRECTNESS:");
        let _ = writeln!(
            s,
            "│   errors: {}  warns: {}  clean: {}",
            self.netcheck_errors, self.netcheck_warns, self.netcheck_clean
        );
        let _ = writeln!(
            s,
            "└─ {} ────────────────────────────────────────────────",
            if self.netcheck_clean && (self.rand_self - 1.0).abs() < 1e-9 {
                "ALL PASS"
            } else {
                "FAIL"
            }
        );
        s
    }

    pub fn print(&self) {
        mcc_dbg!("inst::mod", "{}", self.render());
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rand_index_identical() {
        let mut a = BTreeMap::new();
        a.insert(1, 10);
        a.insert(2, 10);
        a.insert(3, 20);
        a.insert(4, 20);
        let p1 = Partition::from_assignment(a.clone());
        let p2 = Partition::from_assignment(a);
        assert!((rand_index(&p1, &p2) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_rand_index_disjoint() {
        let mut a = BTreeMap::new();
        a.insert(1, 10);
        a.insert(2, 10);
        a.insert(3, 20);
        a.insert(4, 20);
        let mut b = BTreeMap::new();
        b.insert(1, 10);
        b.insert(2, 20); // swapped
        b.insert(3, 10); // swapped
        b.insert(4, 20);
        let p1 = Partition::from_assignment(a);
        let p2 = Partition::from_assignment(b);
        let ri = rand_index(&p1, &p2);
        // a: {1,2} in 10, {3,4} in 20  vs  b: {1,3} in 10, {2,4} in 20
        // (1,4) diff-diff, (2,3) diff-diff → b=2, a=0 → 2/6 = 1/3
        assert!((ri - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_rand_index_single_item() {
        let mut a = BTreeMap::new();
        a.insert(1, 10);
        let p = Partition::from_assignment(a);
        assert!((rand_index(&p, &p) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_rand_index_empty() {
        let p = Partition::from_assignment(BTreeMap::new());
        assert!((rand_index(&p, &p) - 1.0).abs() < 1e-9);
    }
}