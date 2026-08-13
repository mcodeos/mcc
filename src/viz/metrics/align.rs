// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! # 对齐度量（M1-4 · 自测版）
//!
//! 本模块提供**不依赖 KiCad 的自测版对齐度量**：
//!
//! 1. **Rand index**：比较两种分区（模块结构 vs 自身）的一致性。
//!    自测版用 .mc 的 module 结构做 ground truth，自洽性验证。
//! 2. **网表指标**：与 netcheck 联动，产出 Tier 0 网表正确性汇总。
//!
//! KiCad 版的对齐度量（Kendall τ、wire/label 准确率、交叉数对比等）
//! 随 M1-3 一起推迟。

use std::collections::BTreeMap;

use crate::instant::insttab::InstTable;
use crate::instant::netcheck;

// ============================================================================
// 分区结构
// ============================================================================

/// 一个分区（partition）：把一组 item_id 分到若干个 cluster_id 中。
#[derive(Debug, Clone)]
pub struct Partition {
    /// item_id → cluster_id 的映射
    pub assignment: BTreeMap<u32, u32>,
    /// 总 item 数
    pub item_count: usize,
    /// 总 cluster 数
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

    /// 从 InstTable 的 module 结构构建分区：每个 Component 属于其最近的 Module。
    pub fn from_module_structure(table: &InstTable) -> Self {
        let mut assignment = BTreeMap::new();

        for comp in table.get_components() {
            // 向上遍历找最近的 Module 祖先
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

    /// 获取 item_id 的 cluster_id，未找到时返回 0
    pub fn cluster_of(&self, item_id: u32) -> u32 {
        self.assignment.get(&item_id).copied().unwrap_or(0)
    }
}

// ============================================================================
// Rand index
// ============================================================================

/// 计算两个分区之间的 Rand index。
///
/// Rand index = (a + b) / C(n, 2)，其中：
/// - a = 两个分区中都在同一 cluster 的 item 对数
/// - b = 两个分区中都在不同 cluster 的 item 对数
/// - n = 总 item 数
///
/// 返回 0.0 ~ 1.0 之间的值，1.0 表示完全一致。
pub fn rand_index(p1: &Partition, p2: &Partition) -> f64 {
    // 取两个分区的 item 交集
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

    let mut a = 0usize; // 同簇对数
    let mut b = 0usize; // 异簇对数

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

/// 计算自洽性 Rand index：将 module 分区与自身比较，应始终为 1.0。
pub fn self_consistency_rand(table: &InstTable) -> f64 {
    let p = Partition::from_module_structure(table);
    rand_index(&p, &p)
}

// ============================================================================
// 对齐度量汇总
// ============================================================================

/// 对齐度量汇总报告
#[derive(Debug, Default)]
pub struct AlignMetricsReport {
    /// 模块数
    pub module_count: usize,
    /// 器件数
    pub component_count: usize,
    /// Rand index（自洽性，应始终为 1.0）
    pub rand_self: f64,
    /// netcheck 错误数
    pub netcheck_errors: usize,
    /// netcheck 警告数
    pub netcheck_warns: usize,
    /// netcheck 是否干净（无 ERROR）
    pub netcheck_clean: bool,
}

impl AlignMetricsReport {
    /// 从 InstTable 计算对齐度量（自测版）
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

    /// 渲染对齐度量表
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
// 单元测试
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