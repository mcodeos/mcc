// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! M4.1: generic column-rank solver, factored out of `ladder_model.rs`.
//!
//! Many layout falls boil down to: assign each node a scalar `rank` (column)
//! subject to three kinds of pairwise constraints:
//!
//! * `same`     — two nodes MUST share a rank (union-find merges them).
//! * `step`     — `rank(b) == rank(a) + 1` exactly one later (e.g. a series
//!   component spans one column).
//! * `before`   — `rank(a) < rank(b)` (a strict ordering, at least one apart).
//!
//! Solving is: union-find the `same` pairs into equivalence classes, then
//! longest-path rank the resulting class DAG (Kahn), which is exactly what
//! ladder_model's `Dsu` + Kahn-rank does — this module is that algorithm made
//! reusable so M4's column model, `ladder_place` and `sp_place` do not grow a
//! third implementation with its own bugs.

use std::collections::VecDeque;

/// Why the rank problem could not be solved. Must be handled specially: a
/// silent bad column is far worse than an ugly layout, so callers should
/// log the error and fall back to their previous milestone behaviour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RankError {
    /// The same/before/step constraints form a cycle (a feedback /
    /// backward pair). No consistent ranking exists.
    Cycle,
    /// A pair refers to an out-of-range node (`a` or `b` >= `n`).
    BadIndex { node: usize },
}

/// A column-rank problem over `n` nodes.
#[derive(Debug, Clone, Default)]
pub struct RankProblem {
    pub n: usize,
    /// Force-equal rank pairs `(a, b)`.
    pub same: Vec<(usize, usize)>,
    /// Strict `a < b` (a rank strictly before b's).
    pub before: Vec<(usize, usize)>,
    /// `rank(b) == rank(a) + 1`.
    pub step: Vec<(usize, usize)>,
}

/// Solve a rank problem, returning each node's rank (column). Ranks are
/// dense per connected class forest; a class with no predecessors starts at 0.
pub fn solve(p: &RankProblem) -> Result<Vec<i32>, RankError> {
    if p.before
        .iter()
        .chain(&p.step)
        .any(|&(a, b)| a >= p.n || b >= p.n)
    {
        let idx = p
            .before
            .iter()
            .chain(&p.step)
            .find(|&&(a, b)| a >= p.n || b >= p.n)
            .copied()
            .map(|(a, b)| if a >= p.n { a } else { b })
            .unwrap_or(0);
        return Err(RankError::BadIndex { node: idx });
    }

    // ── 1. Union-find the `same` pairs into classes ────────────────────────
    let mut dsu = Dsu::new(p.n);
    for &(a, b) in &p.same {
        dsu.union(a, b);
    }
    let class_of: Vec<usize> = (0..p.n).map(|i| dsu.find(i)).collect();
    // Deterministic canonical label per class: the smallest member index.
    let mut k = 0usize;
    let mut class_to_id: std::collections::HashMap<usize, usize> = Default::default();
    let mut id_to_node: Vec<usize> = Vec::new();
    {
        let mut canon: Vec<usize> = Vec::new();
        for i in 0..p.n {
            if !canon.contains(&class_of[i]) {
                canon.push(class_of[i]);
            }
        }
        canon.sort_unstable();
        for &c in &canon {
            class_to_id.insert(c, k);
            id_to_node.push(c);
            k += 1;
        }
    }

    // ── 2. Build the class DAG -------------------------------------------------
    //
    // Every edge is "strictly after by at least one column". Under longest-path
    // ranking, `rank[v] = max(rank[u] + 1)`, which satisfies both `step`
    // (exactly +1 when u is the only tight predecessor) and `before` (> 0).
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); k];
    let mut indeg: Vec<usize> = vec![0; k];
    let mut add_edge = |u: usize, v: usize| {
        if u != v && !adj[u].contains(&v) {
            adj[u].push(v);
            indeg[v] += 1;
        }
    };
    for &(a, b) in &p.before {
        let u = class_to_id[&class_of[a]];
        let v = class_to_id[&class_of[b]];
        if u == v && a != b {
            // `same` + `before` on one pair: rank(x) < rank(y) == rank(x) — a
            // contradiction.
            return Err(RankError::Cycle);
        }
        add_edge(u, v);
    }
    for &(a, b) in &p.step {
        let u = class_to_id[&class_of[a]];
        let v = class_to_id[&class_of[b]];
        if u == v {
            return Err(RankError::Cycle);
        }
        add_edge(u, v);
    }

    // ── 3. Longest-path rank (Kahn); a node left with indeg>0 is a cycle. ──
    let mut rank: Vec<i32> = vec![0; k];
    let mut q: VecDeque<usize> = (0..k).filter(|&i| indeg[i] == 0).collect();
    let mut visited = 0usize;
    while let Some(u) = q.pop_front() {
        visited += 1;
        for &v in &adj[u] {
            rank[v] = rank[v].max(rank[u] + 1);
            indeg[v] -= 1;
            if indeg[v] == 0 {
                q.push_back(v);
            }
        }
    }
    if visited != k {
        return Err(RankError::Cycle);
    }

    // ── 4. Dereference classes back to nodes ───────────────────────────────
    let mut out = vec![0i32; p.n];
    for i in 0..p.n {
        out[i] = rank[class_to_id[&class_of[i]]];
    }
    Ok(out)
}

/// Minimal union-find over `n` elements (path halving, lower-root).
struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Dsu {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (hi, lo) = if ra > rb { (ra, rb) } else { (rb, ra) };
            self.parent[hi] = lo;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_pairs_share_rank() {
        // 4 nodes; 0~1 and 2~3 are same; 1 before 3.
        let p = RankProblem {
            n: 4,
            same: vec![(0, 1), (2, 3)],
            before: vec![(1, 3)],
            step: vec![],
        };
        let r = solve(&p).unwrap();
        assert_eq!(r[0], r[1]);
        assert_eq!(r[2], r[3]);
        // Dense ranks: two classes → 0 and 1.
        assert!(r[3] > r[0]);
        for v in r {
            assert!(v >= 0 && v <= 1, "dense: got {v}");
        }
    }

    #[test]
    fn step_is_one_more() {
        let p = RankProblem {
            n: 3,
            same: vec![],
            before: vec![],
            step: vec![(0, 1), (1, 2)],
        };
        let r = solve(&p).unwrap();
        assert_eq!(r[0], 0);
        assert_eq!(r[1], 1);
        assert_eq!(r[2], 2);
    }

    #[test]
    fn cycle_is_reported() {
        let p = RankProblem {
            n: 2,
            same: vec![],
            before: vec![(0, 1), (1, 0)],
            step: vec![],
        };
        assert_eq!(solve(&p), Err(RankError::Cycle));
    }

    #[test]
    fn bad_index_is_reported() {
        let p = RankProblem {
            n: 2,
            same: vec![],
            before: vec![(0, 5)],
            step: vec![],
        };
        assert_eq!(solve(&p), Err(RankError::BadIndex { node: 5 }));
    }

    #[test]
    fn ladder_shape() {
        // A 3-net ladder: net0 before net1 before net2, with a bridge net0~net2.
        let p = RankProblem {
            n: 3,
            same: vec![(0, 2)],
            before: vec![(0, 1), (1, 2)],
            step: vec![],
        };
        match solve(&p) {
            // 0 and 2 are one class; 1 must sit strictly between is impossible,
            // so this is a Cycle — a genuine feedback bridge.
            Err(RankError::Cycle) => {}
            other => panic!("expected cycle, got {other:?}"),
        }
    }
}
