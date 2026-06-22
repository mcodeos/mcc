// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Grid A* maze routing (M2)
//!
//! Rasterize the canvas into a uniform grid (cell=8px), each cell has three states:
//! Blocked (covered by inflated box) / Reserved (occupied by routed wires, stage 2) / Free.
//! A* finds an orthogonal path on the grid that avoids obstacles, avoids wires, and minimizes turns.
//!
//! Design highlights (see ROADMAP section 3):
//! - Cost = step + turn penalty + cross penalty (very high) + hug penalty; heuristic = Manhattan (admissible).
//! - Pin entry/exit uses "escape point" (pin moves ESCAPE>inflate outward along exit direction, lands on free cell).
//! - Stage 2: after routing each net, `reserve_segments` occupies the cells, later A* avoids them → no wire-on-wire.
//!
//! This module is independent and unit-testable; called by scheduler to plug into the main flow.

use std::collections::{BinaryHeap, HashMap};

use crate::vector::graph::{McVecGraph, Point, Route, Segment, VizNet};

use super::side::{compute_exit_for_pin, ExitSide};

// ── Tuning constants ───────────────────────────────────────────────────────────────
pub const GRID_CELL: f64 = 8.0; // Cell edge length (smaller = can fit narrower gaps, slower)
pub const GRID_INFLATE: f64 = 8.0; // Box inflation (how far wires stay from components)
pub const GRID_MARGIN: f64 = 96.0; // Canvas margin (room for detours)
pub const GRID_GAP: i64 = 1; // How many cells to leave on each side of a wire when reserving (parallel wire spacing)
const ESCAPE: f64 = 20.0; // Pin escape distance (> inflate + cell, ensure landing on free cell outside obstacle)

// Direction encoding (u8, convenient for heap ordering)
const DIR_NONE: u8 = 0;
const DIR_E: u8 = 1;
const DIR_W: u8 = 2;
const DIR_S: u8 = 3;
const DIR_N: u8 = 4;

/// A* cost weights (integers, to avoid float ordering issues; step fixed at 10)
#[derive(Debug, Clone)]
pub struct AStarCfg {
    pub step: i64,  // Per cell (fixed 10)
    pub turn: i64,  // Turn penalty (~30 = 3 cells equivalent, makes wires go straight)
    pub cross: i64, // Penalty for crossing other wires (very high, only as last resort)
    pub hug: i64, // Penalty for hugging obstacles/boxes (small, wires avoid grazing component shells)
    pub clear: i64, // Penalty for grazing other nets' wires (default 0: enabling it made flash worse; tune later if needed)
}

impl Default for AStarCfg {
    fn default() -> Self {
        Self {
            step: 10,
            turn: 30,
            cross: 10_000,
            hug: 4,
            clear: 0,
        }
    }
}

// ── Grid ─────────────────────────────────────────────────────────────────────────
pub struct Grid {
    ox: f64, // World x of column 0
    oy: f64, // World y of row 0
    cols: usize,
    rows: usize,
    cell: f64,
    blocked: Vec<bool>,
    reserved: Vec<i64>, // net_id; -1 = empty
    history: Vec<i64>, // ★ Negotiated congestion: each cell's historical cost (busy intersections get more expensive)
}

impl Grid {
    /// Build grid from graph: bbox covering all boxes + margin, inflate each box and mark as Blocked.
    /// Only takes current-layer boxes (sub-graphs are built by scheduler recursively).
    pub fn from_graph(graph: &McVecGraph, cell: f64, inflate: f64) -> Self {
        let (mut minx, mut miny, mut maxx, mut maxy) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for b in &graph.boxes {
            minx = minx.min(b.x);
            miny = miny.min(b.y);
            maxx = maxx.max(b.x + b.w);
            maxy = maxy.max(b.y + b.h);
        }
        if !minx.is_finite() {
            // No boxes: fall back to 1×1 empty grid
            return Grid {
                ox: 0.0,
                oy: 0.0,
                cols: 1,
                rows: 1,
                cell,
                blocked: vec![false],
                reserved: vec![-1],
                history: vec![0],
            };
        }
        minx -= GRID_MARGIN;
        miny -= GRID_MARGIN;
        maxx += GRID_MARGIN;
        maxy += GRID_MARGIN;
        let cols = (((maxx - minx) / cell).ceil() as usize).max(1) + 1;
        let rows = (((maxy - miny) / cell).ceil() as usize).max(1) + 1;
        let mut g = Grid {
            ox: minx,
            oy: miny,
            cols,
            rows,
            cell,
            blocked: vec![false; cols * rows],
            reserved: vec![-1; cols * rows],
            history: vec![0; cols * rows],
        };
        for b in &graph.boxes {
            g.block_rect(b.x, b.y, b.w, b.h, inflate);
        }
        g
    }

    #[inline]
    fn idx(&self, c: usize, r: usize) -> usize {
        r * self.cols + c
    }
    #[inline]
    fn cr(&self, idx: usize) -> (usize, usize) {
        (idx % self.cols, idx / self.cols)
    }
    /// World coordinates → cell (col,row), clamped to bounds
    pub fn world_to_cell(&self, x: f64, y: f64) -> (usize, usize) {
        let c = (((x - self.ox) / self.cell).round() as i64).clamp(0, self.cols as i64 - 1);
        let r = (((y - self.oy) / self.cell).round() as i64).clamp(0, self.rows as i64 - 1);
        (c as usize, r as usize)
    }
    pub fn cell_center(&self, c: usize, r: usize) -> (f64, f64) {
        (
            self.ox + c as f64 * self.cell,
            self.oy + r as f64 * self.cell,
        )
    }
    fn cell_center_idx(&self, idx: usize) -> (f64, f64) {
        let (c, r) = self.cr(idx);
        self.cell_center(c, r)
    }
    #[inline]
    fn is_blocked_idx(&self, idx: usize) -> bool {
        self.blocked[idx]
    }
    #[inline]
    fn reserved_idx(&self, idx: usize) -> i64 {
        self.reserved[idx]
    }

    /// Mark rectangle (after inflation) as Blocked
    pub fn block_rect(&mut self, x: f64, y: f64, w: f64, h: f64, inflate: f64) {
        let (c0, r0) = self.world_to_cell(x - inflate, y - inflate);
        let (c1, r1) = self.world_to_cell(x + w + inflate, y + h + inflate);
        for r in r0..=r1 {
            for c in c0..=c1 {
                let i = self.idx(c, r);
                self.blocked[i] = true;
            }
        }
    }

    fn adjacent_to_blocked(&self, c: usize, r: usize) -> bool {
        (c + 1 < self.cols && self.blocked[self.idx(c + 1, r)])
            || (c > 0 && self.blocked[self.idx(c - 1, r)])
            || (r + 1 < self.rows && self.blocked[self.idx(c, r + 1)])
            || (r > 0 && self.blocked[self.idx(c, r - 1)])
    }

    #[inline]
    fn history_idx(&self, idx: usize) -> i64 {
        self.history[idx]
    }

    /// Whether a cell is adjacent to cells occupied by **other nets**
    /// (openness: grazing other nets' wires adds cost → bias toward open areas)
    fn adjacent_to_other_reserved(&self, c: usize, r: usize, net_id: i64) -> bool {
        let nb = |cc: usize, rr: usize| -> bool {
            let v = self.reserved[self.idx(cc, rr)];
            v != -1 && v != net_id
        };
        (c + 1 < self.cols && nb(c + 1, r))
            || (c > 0 && nb(c - 1, r))
            || (r + 1 < self.rows && nb(c, r + 1))
            || (r > 0 && nb(c, r - 1))
    }

    /// Accumulate historical cost at world point (x,y)'s cell + 4 neighbors
    /// (negotiated congestion: cells that fight repeatedly become more expensive)
    pub fn bump_history_at(&mut self, x: f64, y: f64, amount: i64) {
        let (c, r) = self.world_to_cell(x, y);
        let mut cells = vec![(c, r)];
        if c + 1 < self.cols {
            cells.push((c + 1, r));
        }
        if c > 0 {
            cells.push((c - 1, r));
        }
        if r + 1 < self.rows {
            cells.push((c, r + 1));
        }
        if r > 0 {
            cells.push((c, r - 1));
        }
        for (cc, rr) in cells {
            let i = self.idx(cc, rr);
            self.history[i] += amount;
        }
    }

    fn neighbors(&self, c: usize, r: usize) -> [(u8, usize, usize); 4] {
        // Out-of-bounds: use self-occupied (caller filters in-bounds; here just return safe value)
        [
            if c + 1 < self.cols {
                (DIR_E, c + 1, r)
            } else {
                (DIR_NONE, c, r)
            },
            if c > 0 {
                (DIR_W, c - 1, r)
            } else {
                (DIR_NONE, c, r)
            },
            if r + 1 < self.rows {
                (DIR_S, c, r + 1)
            } else {
                (DIR_NONE, c, r)
            },
            if r > 0 {
                (DIR_N, c, r - 1)
            } else {
                (DIR_NONE, c, r)
            },
        ]
    }

    /// Rip-up: clear all cells occupied by a net (for rip-up use). Blocked is untouched.
    pub fn unreserve_net(&mut self, net_id: i64) {
        for v in self.reserved.iter_mut() {
            if *v == net_id {
                *v = -1;
            }
        }
    }

    /// Reserve a net's wires (+ gap cells on each side) → later A* treats them as "occupied by others" and avoids them.
    /// Only occupies currently empty (-1) cells, doesn't overwrite occupied ones; doesn't touch Blocked.
    pub fn reserve_segments(&mut self, segs: &[Segment], net_id: i64, gap: i64) {
        let g = gap.max(0) as usize;
        for s in segs {
            let (c0, r0) = self.world_to_cell(s.from.x.min(s.to.x), s.from.y.min(s.to.y));
            let (c1, r1) = self.world_to_cell(s.from.x.max(s.to.x), s.from.y.max(s.to.y));
            let cc0 = c0.saturating_sub(g);
            let rr0 = r0.saturating_sub(g);
            let cc1 = (c1 + g).min(self.cols - 1);
            let rr1 = (r1 + g).min(self.rows - 1);
            for r in rr0..=rr1 {
                for c in cc0..=cc1 {
                    let i = self.idx(c, r);
                    if self.reserved[i] == -1 {
                        self.reserved[i] = net_id;
                    }
                }
            }
        }
    }

    /// Whether an already-routed wire collides (used to trigger A* reroute):
    /// - Overlaps cells reserved by **other nets** → yes (wire on wire)
    /// - Overlaps Blocked and is **not inside this net's endpoint box** → yes
    ///   (piercing component; edge-grazing at endpoints is normal, excluded)
    pub fn route_collides(
        &self,
        segs: &[Segment],
        net_id: i64,
        endpoint_rects: &[(f64, f64, f64, f64)],
    ) -> bool {
        const PAD: f64 = GRID_INFLATE + 4.0;
        for s in segs {
            for (c, r) in self.cells_on_segment(s) {
                let i = self.idx(c, r);
                let res = self.reserved[i];
                if res != -1 && res != net_id {
                    return true;
                }
                if self.blocked[i] {
                    let (wx, wy) = self.cell_center_idx(i);
                    let inside_ep = endpoint_rects.iter().any(|&(x, y, w, h)| {
                        wx >= x - PAD && wx <= x + w + PAD && wy >= y - PAD && wy <= y + h + PAD
                    });
                    if !inside_ep {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// List the cells an orthogonal segment passes through (horizontal/vertical;
    /// diagonals degenerate to bbox, fault-tolerant)
    fn cells_on_segment(&self, s: &Segment) -> Vec<(usize, usize)> {
        let (c0, r0) = self.world_to_cell(s.from.x, s.from.y);
        let (c1, r1) = self.world_to_cell(s.to.x, s.to.y);
        let mut out = Vec::new();
        let (cl, ch) = (c0.min(c1), c0.max(c1));
        let (rl, rh) = (r0.min(r1), r0.max(r1));
        for r in rl..=rh {
            for c in cl..=ch {
                out.push((c, r));
            }
        }
        out
    }

    /// Walk outward from the pin along the exit direction cell by cell, find the first free cell as A* start/goal (avoiding endpoint box inflation).
    /// All blocked (surroundings full) returns None → caller keeps original routing.
    fn escape_cell(&self, pin: (f64, f64), side: ExitSide, base: f64) -> Option<usize> {
        for k in 0..10 {
            let d = base + k as f64 * self.cell;
            let p = escape_point(pin, side, d);
            let (c, r) = self.world_to_cell(p.0, p.1);
            let idx = self.idx(c, r);
            if !self.is_blocked_idx(idx) {
                return Some(idx);
            }
        }
        None
    }
}

// ── A\* ─────────────────────────────────────────────────────────────────────

/// Walk from start cell to goal cell on the grid, return cell sequence (including start/goal); None if no solution.
/// start/goal must be free cells (guaranteed by using escape points).
pub fn astar(
    grid: &Grid,
    start: usize,
    goal: usize,
    net_id: i64,
    cfg: &AStarCfg,
) -> Option<Vec<usize>> {
    if grid.is_blocked_idx(start) || grid.is_blocked_idx(goal) {
        return None;
    }
    let (gc, gr) = grid.cr(goal);
    let heur = |idx: usize| -> i64 {
        let (c, r) = grid.cr(idx);
        ((c as i64 - gc as i64).abs() + (r as i64 - gr as i64).abs()) * cfg.step
    };

    let mut gscore: HashMap<(usize, u8), i64> = HashMap::new();
    let mut came: HashMap<(usize, u8), (usize, u8)> = HashMap::new();
    let mut heap: BinaryHeap<std::cmp::Reverse<(i64, i64, usize, u8)>> = BinaryHeap::new();

    gscore.insert((start, DIR_NONE), 0);
    heap.push(std::cmp::Reverse((heur(start), 0, start, DIR_NONE)));

    while let Some(std::cmp::Reverse((_f, g, cell, dir))) = heap.pop() {
        if cell == goal {
            // Backtrack
            let mut path = vec![cell];
            let mut state = (cell, dir);
            while let Some(&prev) = came.get(&state) {
                path.push(prev.0);
                state = prev;
            }
            path.reverse();
            return Some(path);
        }
        if let Some(&best) = gscore.get(&(cell, dir)) {
            if g > best {
                continue; // expired entry
            }
        }
        let (c, r) = grid.cr(cell);
        for (ndir, nc, nr) in grid.neighbors(c, r) {
            if ndir == DIR_NONE {
                continue; // out-of-bounds placeholder
            }
            let nidx = grid.idx(nc, nr);
            if grid.is_blocked_idx(nidx) {
                continue;
            }
            let mut cost = cfg.step;
            if dir != DIR_NONE && ndir != dir {
                cost += cfg.turn;
            }
            let res = grid.reserved_idx(nidx);
            if res != -1 && res != net_id {
                cost += cfg.cross;
            }
            if cfg.hug > 0 && grid.adjacent_to_blocked(nc, nr) {
                cost += cfg.hug;
            }
            if cfg.clear > 0 && grid.adjacent_to_other_reserved(nc, nr, net_id) {
                cost += cfg.clear; // openness: grazing other wires adds cost
            }
            cost += grid.history_idx(nidx); // negotiated congestion: busy cells get more expensive
            let ng = g + cost;
            let nstate = (nidx, ndir);
            if ng < *gscore.get(&nstate).unwrap_or(&i64::MAX) {
                gscore.insert(nstate, ng);
                came.insert(nstate, (cell, dir));
                heap.push(std::cmp::Reverse((ng + heur(nidx), ng, nidx, ndir)));
            }
        }
    }
    None
}

/// Cell path → world orthogonal segments (merge consecutive same-direction cells, keep only corners)
pub fn cells_to_segments(grid: &Grid, path: &[usize]) -> Vec<Segment> {
    if path.len() < 2 {
        return Vec::new();
    }
    let pts: Vec<(f64, f64)> = path.iter().map(|&i| grid.cell_center_idx(i)).collect();
    let mut corners = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let a = pts[i - 1];
        let b = pts[i];
        let c = pts[i + 1];
        let d1 = (b.0 - a.0, b.1 - a.1);
        let d2 = (c.0 - b.0, c.1 - b.1);
        // Same direction (cross product = 0 and same sign) → b is not a corner, skip
        let collinear = (d1.0 * d2.1 - d1.1 * d2.0).abs() < 1e-6;
        if !collinear {
            corners.push(b);
        }
    }
    corners.push(*pts.last().unwrap());
    corners
        .windows(2)
        .map(|w| Segment {
            from: Point::new(w[0].0, w[0].1),
            to: Point::new(w[1].0, w[1].1),
        })
        .collect()
}

// ── High-level: reroute a 2-endpoint net ───────────────────────────────────────────

/// Use A* to reroute a 2-endpoint net, avoiding Blocked + other nets' Reserved. None if no solution (caller keeps original routing).
pub fn reroute_two_point(
    grid: &Grid,
    graph: &McVecGraph,
    net: &VizNet,
    cfg: &AStarCfg,
) -> Option<Route> {
    if net.endpoints.len() != 2 {
        return None;
    }
    let a = &net.endpoints[0];
    let b = &net.endpoints[1];
    let ba = graph.boxes.iter().find(|x| x.id == a.box_id)?;
    let bb = graph.boxes.iter().find(|x| x.id == b.box_id)?;

    let (pa, sa) = compute_exit_for_pin(ba, a.pin_id, Some(bb));
    let (pb, sb) = compute_exit_for_pin(bb, b.pin_id, Some(ba));

    // Move outward cell by cell to find free cell outside obstacle as start/end (if either end not found → give up, keep original routing)
    let s = grid.escape_cell(pa, sa, ESCAPE)?;
    let g = grid.escape_cell(pb, sb, ESCAPE)?;

    let path = astar(grid, s, g, net.nid, cfg)?;

    let cs = grid.cell_center_idx(s);
    let cg = grid.cell_center_idx(g);

    let mut segs: Vec<Segment> = Vec::new();
    // Pin → start/end cell center (orthogonal L-shape connection)
    segs.extend(ortho_link(pa, cs));
    segs.extend(cells_to_segments(grid, &path));
    segs.extend(ortho_link(cg, pb));

    let mut route = Route::new();
    route.segments = segs;
    Some(route)
}

/// Use A* to route a **multi-endpoint net (≥2 endpoints)** as a **tree**: connect ep0-ep1 as trunk, remaining endpoints each
/// join the built tree via multi-target A* (built part as goal set), always avoiding other nets' Reserved. Any segment no solution → None.
/// (M5: multi-endpoint nets on grid, replacing trunk_tap's crossing/wire-on-wire cases)
pub fn reroute_multi_point(
    grid: &Grid,
    graph: &McVecGraph,
    net: &VizNet,
    cfg: &AStarCfg,
) -> Option<Route> {
    let n = net.endpoints.len();
    if n < 2 {
        return None;
    }
    // Each endpoint: exit point + escape cell
    let mut esc_cell: Vec<usize> = Vec::with_capacity(n);
    let mut pin_pt: Vec<(f64, f64)> = Vec::with_capacity(n);
    for ep in &net.endpoints {
        let b = graph.boxes.iter().find(|x| x.id == ep.box_id)?;
        // Exit direction reference: any other endpoint box
        let other = net
            .endpoints
            .iter()
            .find(|e| e.box_id != ep.box_id)
            .and_then(|e| graph.boxes.iter().find(|x| x.id == e.box_id));
        let (p, s) = compute_exit_for_pin(b, ep.pin_id, other);
        let cell = grid.escape_cell(p, s, ESCAPE)?;
        esc_cell.push(cell);
        pin_pt.push(p);
    }

    let mut tree: HashMap<usize, ()> = HashMap::new(); // Tree cell set (using HashMap as Set)
    let mut segs: Vec<Segment> = Vec::new();

    // Connect ep0 - ep1 first
    let path01 = astar(grid, esc_cell[0], esc_cell[1], net.nid, cfg)?;
    for &c in &path01 {
        tree.insert(c, ());
    }
    segs.extend(cells_to_segments(grid, &path01));
    segs.extend(ortho_link(pin_pt[0], grid.cell_center_idx(esc_cell[0])));
    segs.extend(ortho_link(pin_pt[1], grid.cell_center_idx(esc_cell[1])));

    // Connect remaining endpoints to the tree one by one
    for k in 2..n {
        let path = astar_to_tree(grid, esc_cell[k], &tree, net.nid, cfg)?;
        for &c in &path {
            tree.insert(c, ());
        }
        segs.extend(cells_to_segments(grid, &path));
        segs.extend(ortho_link(pin_pt[k], grid.cell_center_idx(esc_cell[k])));
    }

    let mut route = Route::new();
    route.segments = segs;
    Some(route)
}

/// Multi-target A* (Dijkstra, no heuristic): from start, walk to **any cell in tree set `tree`**. Used for multi-endpoint nets joining tree.
fn astar_to_tree(
    grid: &Grid,
    start: usize,
    tree: &HashMap<usize, ()>,
    net_id: i64,
    cfg: &AStarCfg,
) -> Option<Vec<usize>> {
    if grid.is_blocked_idx(start) {
        return None;
    }
    if tree.contains_key(&start) {
        return Some(vec![start]);
    }
    let mut gscore: HashMap<(usize, u8), i64> = HashMap::new();
    let mut came: HashMap<(usize, u8), (usize, u8)> = HashMap::new();
    let mut heap: BinaryHeap<std::cmp::Reverse<(i64, usize, u8)>> = BinaryHeap::new();
    gscore.insert((start, DIR_NONE), 0);
    heap.push(std::cmp::Reverse((0, start, DIR_NONE)));
    while let Some(std::cmp::Reverse((g, cell, dir))) = heap.pop() {
        if tree.contains_key(&cell) {
            let mut path = vec![cell];
            let mut state = (cell, dir);
            while let Some(&prev) = came.get(&state) {
                path.push(prev.0);
                state = prev;
            }
            path.reverse();
            return Some(path);
        }
        if let Some(&best) = gscore.get(&(cell, dir)) {
            if g > best {
                continue;
            }
        }
        let (c, r) = grid.cr(cell);
        for (ndir, nc, nr) in grid.neighbors(c, r) {
            if ndir == DIR_NONE {
                continue;
            }
            let nidx = grid.idx(nc, nr);
            if grid.is_blocked_idx(nidx) {
                continue;
            }
            let mut cost = cfg.step;
            if dir != DIR_NONE && ndir != dir {
                cost += cfg.turn;
            }
            let res = grid.reserved_idx(nidx);
            if res != -1 && res != net_id {
                cost += cfg.cross;
            }
            if cfg.hug > 0 && grid.adjacent_to_blocked(nc, nr) {
                cost += cfg.hug;
            }
            if cfg.clear > 0 && grid.adjacent_to_other_reserved(nc, nr, net_id) {
                cost += cfg.clear; // openness: grazing other wires adds cost
            }
            cost += grid.history_idx(nidx); // negotiated congestion: busy cells get more expensive
            let ng = g + cost;
            let nstate = (nidx, ndir);
            if ng < *gscore.get(&nstate).unwrap_or(&i64::MAX) {
                gscore.insert(nstate, ng);
                came.insert(nstate, (cell, dir));
                heap.push(std::cmp::Reverse((ng, nidx, ndir)));
            }
        }
    }
    None
}

fn escape_point(p: (f64, f64), side: ExitSide, d: f64) -> (f64, f64) {
    match side {
        ExitSide::Left => (p.0 - d, p.1),
        ExitSide::Right => (p.0 + d, p.1),
        ExitSide::Top => (p.0, p.1 - d),
        ExitSide::Bottom => (p.0, p.1 + d),
    }
}

/// Orthogonal connection between two points: straight line if aligned, otherwise one L-shape (horizontal first, then vertical)
fn ortho_link(from: (f64, f64), to: (f64, f64)) -> Vec<Segment> {
    if (from.0 - to.0).abs() < 0.5 || (from.1 - to.1).abs() < 0.5 {
        vec![Segment {
            from: Point::new(from.0, from.1),
            to: Point::new(to.0, to.1),
        }]
    } else {
        let corner = (to.0, from.1);
        vec![
            Segment {
                from: Point::new(from.0, from.1),
                to: Point::new(corner.0, corner.1),
            },
            Segment {
                from: Point::new(corner.0, corner.1),
                to: Point::new(to.0, to.1),
            },
        ]
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    fn empty_grid(cols: usize, rows: usize) -> Grid {
        Grid {
            ox: 0.0,
            oy: 0.0,
            cols,
            rows,
            cell: GRID_CELL,
            blocked: vec![false; cols * rows],
            reserved: vec![-1; cols * rows],
            history: vec![0; cols * rows],
        }
    }

    #[test]
    fn straight_line_no_obstacle() {
        let g = empty_grid(20, 5);
        let start = g.idx(0, 2);
        let goal = g.idx(10, 2);
        let path = astar(&g, start, goal, 1, &AStarCfg::default()).expect("path");
        // Horizontal line: 11 cells
        assert_eq!(path.len(), 11);
        // merge into 1 segment
        let segs = cells_to_segments(&g, &path);
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn detours_around_block() {
        let mut g = empty_grid(20, 9);
        // Place a vertical wall near (10, 4) blocking the horizontal line on y=4
        for r in 3..=5 {
            let i = g.idx(10, r);
            g.blocked[i] = true;
        }
        let start = g.idx(2, 4);
        let goal = g.idx(18, 4);
        let path = astar(&g, start, goal, 1, &AStarCfg::default()).expect("path");
        // Must detour around the blocked (10,4)
        assert!(!path.iter().any(|&i| i == g.idx(10, 4)));
    }

    #[test]
    fn blocked_endpoint_returns_none() {
        let mut g = empty_grid(10, 10);
        let start = g.idx(0, 0);
        let goal = g.idx(5, 5);
        let gi = g.idx(5, 5);
        g.blocked[gi] = true;
        assert!(astar(&g, start, goal, 1, &AStarCfg::default()).is_none());
    }

    #[test]
    fn avoids_reserved_other_net() {
        let mut g = empty_grid(20, 9);
        // Other net (id=2) occupies y=4 row x=8..12
        for c in 8..=12 {
            let i = g.idx(c, 4);
            g.reserved[i] = 2;
        }
        let start = g.idx(2, 4);
        let goal = g.idx(18, 4);
        // Our net id=1, cross penalty very high → should detour around y=4 occupied cells
        let path = astar(&g, start, goal, 1, &AStarCfg::default()).expect("path");
        let crosses_reserved = path.iter().any(|&i| g.reserved[i] == 2);
        assert!(!crosses_reserved, "should detour around reserved cells");
    }

    #[test]
    fn same_net_reserved_is_free() {
        let mut g = empty_grid(20, 5);
        for c in 8..=12 {
            let i = g.idx(c, 2);
            g.reserved[i] = 1; // our own net occupies
        }
        let start = g.idx(0, 2);
        let goal = g.idx(18, 2);
        // Our net id=1: self-occupied cells are not penalized → still takes straight line
        let path = astar(&g, start, goal, 1, &AStarCfg::default()).expect("path");
        assert_eq!(cells_to_segments(&g, &path).len(), 1);
    }
}
