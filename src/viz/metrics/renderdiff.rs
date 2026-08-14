// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! ★ P7-1 · renderdiff —— the ruler of the render layer
//!
//! ## Positioning
//! Separate from netdiff (netlist criteria) and equally strict (discipline 10):
//! - netdiff governs "is it connected right" (pass2 golden)
//! - renderdiff governs "is it drawn right" (render golden: `baseline/render_golden.toml`)
//!
//! ## Criterion groups
//! - **G10 structure conservation**: box count vs golden; synth box count == 0
//!   (provenance marker); all endpoints of every net in the same route connected
//!   component (reuses `RenderedConnectivityReport`)
//! - **G11 power contract**: GND edge count == 0; rail power edge count == R-2 driver
//!   segment expectation; top-level passives == 0 (contract C5)
//! - **G12 geometric legality**: box_box / wire_box == 0; box w/h ≥ minimum size for
//!   pin distribution (S6); no negative coordinates / off-canvas
//!
//! ## Principles
//! - Criteria are **structural similarity**, not pixel similarity; every explainable
//!   difference vs the reference figure is recorded in the diff table of
//!   `MC_SCHEMATIC_ROADMAP_v6.md` §1.1
//! - **Never edit golden to turn criteria green**. Large-scale red mid-way is the
//!   correct shape (v6 §4)
//! - Discipline 9: every criterion prints its evaluated object count; 0 shows `· SKIP`,
//!   never `✓`

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::vector::graph::kinds::BoxKind;
use crate::vector::graph::{McVecGraph, NetKind};

// ============================================================================
// Golden (TOML schema)
// ============================================================================

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RenderGolden {
    pub layer: BTreeMap<String, LayerGolden>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LayerGolden {
    /// Module name matching pass2 golden (renderdiff's key is graph.name = instance name)
    #[serde(default)]
    pub module: String,
    pub boxes: usize,
    /// Golden-expected box roster (matched by instance name, for match/missing/extra)
    #[serde(default)]
    pub box_names: Vec<String>,
    /// Target count of Phase 1.5/1.6 synth boxes (always 0)
    #[serde(default)]
    pub synth_boxes: usize,
    /// Target count of rail flag boxes (always 0, discipline 11)
    #[serde(default)]
    pub rail_flags: usize,
    /// Target count of GND edges (cross-box ground nets)
    #[serde(default)]
    pub gnd_edges: usize,
    /// Target count of rail power edges (cross-box power nets, i.e. R-2 driver segments)
    #[serde(default)]
    pub power_edges: usize,
    /// Target count of top-level passives (contract C5: block diagram draws no R/C; always 0, only meaningful at top level)
    #[serde(default)]
    pub top_passives: usize,
    /// Expected edge list (from/to by **box name**, label = net name or bus entry name)
    #[serde(default)]
    pub edge: Vec<GEdge>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GEdge {
    pub from: String,
    pub to: String,
    pub label: String,
}

// ============================================================================
// Reading (measured from the final graph)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct LayerReading {
    pub layer: String,
    pub bid: i64,
    /// boxes total
    pub total_boxes: usize,
    pub declared_boxes: usize,
    pub synth_endpoint_boxes: usize,
    pub rail_flag_boxes: usize,
    /// Box roster (declared boxes + synth boxes, all listed for diff)
    pub box_names: Vec<String>,
    /// Cross-box ground net count (= drawn GND edges)
    pub gnd_edges: usize,
    /// Cross-box power net count
    pub power_edges: usize,
    /// TwoPin passive box count
    pub two_pin_passives: usize,
    /// ★ P7-3 S1: ground symbol decoration count (sub-layer = GND endpoint count; always 0 at top)
    #[serde(default)]
    pub decorations_ground: usize,
    /// ★ P7-3 S2: rail dot decoration count (sub-layer = non-GND rail endpoint count; always 0 at top)
    #[serde(default)]
    pub decorations_power: usize,
    /// ★ P7-4: geometric double-write count of this layer (collected via stage-boundary snapshot diff; target 0)
    #[serde(default)]
    pub geom_double_writes: usize,
    /// ★ P7-4c: full double-write detail (box / earlier writer → later writer), for baseline diagnosis;
    /// should converge to empty after P7-4e merges writers by stage.
    #[serde(default)]
    pub geom_double_write_list: Vec<String>,
    /// (from,to,label) list of cross-box nets (unordered pairs)
    pub edges: Vec<(String, String, String)>,
    // G12
    pub box_box: usize,
    pub wire_box: usize,
    pub s6_violations: usize,
    pub offcanvas_boxes: usize,
    // G10 connectivity (injected by the caller from RenderedConnectivityReport)
    pub pins_total: usize,
    pub pins_unreachable: usize,
    /// Self-check: how many objects each criterion evaluated (discipline 9)
    pub evaluated: EvalCounts,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EvalCounts {
    pub boxes: usize,
    pub nets: usize,
    pub sizes: usize,
}

impl LayerReading {
    /// Measure all readings of one layer from the final graph, after route and before render.
    ///
    /// * `col` —— collision report from `audit_all` (G12)
    /// * `conn` —— optional connectivity report (pins_total, pins_unreachable) (G10 item 3).
    ///   When `None` that criterion shows `· SKIP`.
    pub fn measure(
        graph: &McVecGraph,
        col: &crate::viz::route::audit::CollisionReport,
        conn: Option<(usize, usize)>,
    ) -> Self {
        use crate::vector::graph::boxdef::BoxProvenance as P;

        let mut declared = 0usize;
        let mut synth = 0usize;
        let mut flags = 0usize;
        let mut passives = 0usize;
        let mut s6 = 0usize;
        let mut offcanvas = 0usize;
        let mut names = Vec::new();

        for b in &graph.boxes {
            match b.provenance {
                P::Declared => declared += 1,
                P::SynthesizedFromEndpoint => synth += 1,
                P::SynthesizedRailFlag => flags += 1,
            }
            if matches!(b.kind, BoxKind::TwoPin) {
                passives += 1;
            }
            names.push(if b.name.is_empty() {
                format!("{}#", b.id)
            } else {
                b.name.clone()
            });

            // S6: does the box fit its own pin distribution (reuses size::box_size's minimum size formula)
            let (mw, mh) = crate::viz::layout::size::box_size(b);
            if b.w + 1.0 < mw || b.h + 1.0 < mh {
                s6 += 1;
            }
            if b.x < -0.5 || b.y < -0.5 {
                offcanvas += 1;
            }
        }

        // G11: cross-box net classification (endpoints of the same net in ≥2 distinct boxes = an edge was drawn)
        let name_of = |id: i64| -> String {
            graph
                .boxes
                .iter()
                .find(|b| b.id == id)
                .map(|b| {
                    if b.name.is_empty() {
                        format!("{}#", b.id)
                    } else {
                        b.name.clone()
                    }
                })
                .unwrap_or_else(|| format!("{}#", id))
        };
        let mut gnd = 0usize;
        let mut pwr = 0usize;
        let mut edges = Vec::new();
        for net in &graph.nets {
            let mut distinct: Vec<i64> = Vec::new();
            for ep in &net.endpoints {
                if !distinct.contains(&ep.box_id) {
                    distinct.push(ep.box_id);
                }
            }
            if distinct.len() < 2 {
                continue;
            }
            match net.kind {
                NetKind::Ground => gnd += 1,
                NetKind::Power => pwr += 1,
                _ => {}
            }
            edges.push((
                name_of(distinct[0]),
                name_of(distinct[1]),
                net.name.clone(),
            ));
        }

        let (pins_total, pins_unreachable) = conn.unwrap_or((0, 0));

        // ★ P7-3: S1/S2 readings —— pin decoration counts (ground symbols / rail dots).
        // Always 0 for top-level R-1/R-3 (block diagram places no symbols); sub-layer =
        // endpoint count judged "in-place symbol" by R-1/R-3.
        let (dec_ground, dec_power) = {
            let mut g = 0usize;
            let mut p = 0usize;
            for d in &graph.rail_decorations {
                if d.is_ground {
                    g += 1;
                } else {
                    p += 1;
                }
            }
            (g, p)
        };

        LayerReading {
            layer: graph.name.clone(),
            bid: graph.bid,
            total_boxes: graph.boxes.len(),
            declared_boxes: declared,
            synth_endpoint_boxes: synth,
            rail_flag_boxes: flags,
            box_names: names,
            gnd_edges: gnd,
            power_edges: pwr,
            two_pin_passives: passives,
            edges,
            decorations_ground: dec_ground,
            decorations_power: dec_power,
            geom_double_writes: graph.geom_double_writes.len(),
            geom_double_write_list: graph
                .geom_double_writes
                .iter()
                .map(|d| {
                    format!(
                        "{}#{}: {} -> {} [{}]",
                        d.box_name,
                        d.box_id,
                        d.prev_writer,
                        d.cur_writer,
                        d.dims.join("+")
                    )
                })
                .collect(),
            box_box: col.box_box,
            wire_box: col.wire_box,
            s6_violations: s6,
            offcanvas_boxes: offcanvas,
            pins_total,
            pins_unreachable,
            evaluated: EvalCounts {
                boxes: graph.boxes.len(),
                nets: graph.nets.len(),
                sizes: graph.boxes.len(),
            },
        }
    }
}

// ============================================================================
// Diff (reading vs golden)
// ============================================================================

/// Conclusion of one criterion. `Skip` = zero evaluated objects (discipline 9), never counts as green.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Ok(String),
    Fail(String),
    Skip(String),
}

#[derive(Debug, Clone)]
pub struct LayerDiff {
    pub layer: String,
    /// Per-criterion conclusions of G10/G11/G12
    pub findings: Vec<(String, Verdict)>,
    /// Red count (Fail)
    pub red: usize,
    /// Green count (Ok)
    pub green: usize,
    /// Skip count
    pub skipped: usize,
}

impl LayerDiff {
    pub fn report_line(&self) -> String {
        let head = format!(
            "[renderdiff] layer '{}': {} red / {} green / {} skip",
            self.layer, self.red, self.green, self.skipped
        );
        let body: Vec<String> = self
            .findings
            .iter()
            .map(|(id, v)| {
                let mark = match v {
                    Verdict::Ok(_) => "✓",
                    Verdict::Fail(_) => "✗",
                    Verdict::Skip(_) => "·",
                };
                let msg = match v {
                    Verdict::Ok(m) | Verdict::Fail(m) | Verdict::Skip(m) => m,
                };
                format!("{mark} {id}: {msg}")
            })
            .collect();
        if body.is_empty() {
            head
        } else {
            format!("{head}\n  {}", body.join("\n  "))
        }
    }
}

fn sorted_lower(names: &[String]) -> Vec<String> {
    let mut v: Vec<String> = names.iter().map(|s| s.to_lowercase()).collect();
    v.sort();
    v
}

/// Multiset diff: returns (missing, extra)
fn multiset_diff(expected: &[String], actual: &[String]) -> (Vec<String>, Vec<String>) {
    use std::collections::BTreeMap;
    let mut exp: BTreeMap<&str, i32> = BTreeMap::new();
    let mut act: BTreeMap<&str, i32> = BTreeMap::new();
    for s in expected {
        *exp.entry(s.as_str()).or_insert(0) += 1;
    }
    for s in actual {
        *act.entry(s.as_str()).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, e) in &exp {
        let a = act.get(k).copied().unwrap_or(0);
        if a < *e {
            for _ in 0..(e - a) {
                missing.push(k.to_string());
            }
        }
    }
    for (k, a) in &act {
        let e = exp.get(k).copied().unwrap_or(0);
        if a > &e {
            for _ in 0..(a - e) {
                extra.push(k.to_string());
            }
        }
    }
    (missing, extra)
}

/// Edge matching key: (unordered endpoint name pair, label).
fn edge_key(e: &(String, String, String)) -> (String, String, String) {
    let (a, b) = if e.0 <= e.1 {
        (&e.0, &e.1)
    } else {
        (&e.1, &e.0)
    };
    (a.clone(), b.clone(), e.2.clone())
}

impl RenderGolden {
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("render_golden.toml read failed: {e}"))?;
        toml::from_str(&text).map_err(|e| format!("render_golden.toml parse failed: {e}"))
    }

    /// One layer reading vs one layer golden, outputs per-criterion conclusions.
    pub fn diff_layer(&self, r: &LayerReading) -> LayerDiff {
        let g = match self.layer.get(&r.layer) {
            Some(g) => g,
            None => {
                let mut findings = vec![(
                    "G10".to_string(),
                    Verdict::Fail(format!(
                        "layer '{}' not in golden (boxes={}) — golden needs a new entry or the layer name is wrong",
                        r.layer, r.total_boxes
                    )),
                )];
                findings.push((
                    "G12".to_string(),
                    Verdict::Skip(format!("no golden for layer, collisions box_box={} wire_box={} unjudged", r.box_box, r.wire_box)),
                ));
                return LayerDiff {
                    layer: r.layer.clone(),
                    findings,
                    red: 1,
                    green: 0,
                    skipped: 1,
                };
            }
        };

        let mut findings: Vec<(String, Verdict)> = Vec::new();

        // ── G10 structure conservation ─────────────────────────────────
        // (1) box count
        findings.push(num_check(
            "G10.boxes",
            g.boxes,
            r.total_boxes,
            r.evaluated.boxes,
            format!(
                "declared={} synth={} flags={}",
                r.declared_boxes, r.synth_endpoint_boxes, r.rail_flag_boxes
            ),
        ));

        // (2) box roster (match/missing/extra)
        if g.box_names.is_empty() {
            findings.push((
                "G10.names".into(),
                Verdict::Skip(format!("golden has no roster, eval={}", r.box_names.len())),
            ));
        } else {
            let (missing, extra) = multiset_diff(
                &sorted_lower(&g.box_names),
                &sorted_lower(&r.box_names),
            );
            if missing.is_empty() && extra.is_empty() {
                findings.push((
                    "G10.names".into(),
                    Verdict::Ok(format!("{} boxes all match", r.box_names.len())),
                ));
            } else {
                findings.push((
                    "G10.names".into(),
                    Verdict::Fail(format!(
                        "missing={} extra={}",
                        fmt_list(&missing),
                        fmt_list(&extra)
                    )),
                ));
            }
        }

        // (3) synth boxes == 0
        findings.push(num_check(
            "G10.synth",
            g.synth_boxes,
            r.synth_endpoint_boxes,
            r.evaluated.boxes,
            "Phase1.5/1.6 synth".into(),
        ));

        // (4) rail flag boxes == 0
        findings.push(num_check(
            "G10.flags",
            g.rail_flags,
            r.rail_flag_boxes,
            r.evaluated.boxes,
            "Discipline 11 terminals are not boxes".into(),
        ));

        // (5) connectivity: all endpoints of every net in one route connected component
        if r.pins_total == 0 {
            findings.push((
                "G10.conn".into(),
                Verdict::Skip("conn report not injected (eval=0)".into()),
            ));
        } else if r.pins_unreachable == 0 {
            findings.push((
                "G10.conn".into(),
                Verdict::Ok(format!("{}/{} pins reachable", r.pins_total - r.pins_unreachable, r.pins_total)),
            ));
        } else {
            findings.push((
                "G10.conn".into(),
                Verdict::Fail(format!(
                    "{}/{} pins unreachable",
                    r.pins_unreachable, r.pins_total
                )),
            ));
        }

        // ── G11 power contract ────────────────────────────────────────
        findings.push(num_check(
            "G11.gnd_edges",
            g.gnd_edges,
            r.gnd_edges,
            r.evaluated.nets,
            "GND edges (R-1: no driver, not drawn)".into(),
        ));
        findings.push(num_check(
            "G11.power_edges",
            g.power_edges,
            r.power_edges,
            r.evaluated.nets,
            "rail power edges (R-2: driver segments)".into(),
        ));
        if g.top_passives > 0 || r.layer == "main" {
            // C5 judged at top level only (golden gives top_passives semantics to the main layer only)
            findings.push(num_check(
                "G11.top_passives",
                g.top_passives,
                r.two_pin_passives,
                r.evaluated.boxes,
                "Contract C5 block diagram draws no passives".into(),
            ));
        }

        // Edge list (structural comparison: is each expected edge covered by an actual net; which extras exist)
        if g.edge.is_empty() {
            findings.push((
                "G11.edges".into(),
                Verdict::Skip(format!("golden has no edge list, actual edges={}", r.edges.len())),
            ));
        } else {
            let exp: Vec<(String, String, String)> = g
                .edge
                .iter()
                .map(|e| (e.from.clone(), e.to.clone(), e.label.clone()))
                .collect();
            let act: Vec<(String, String, String)> =
                r.edges.iter().map(|e| edge_key(e)).collect();
            let (missing, extra) = multiset_diff_str3(&exp, &act);
            if missing.is_empty() && extra.is_empty() {
                findings.push((
                    "G11.edges".into(),
                    Verdict::Ok(format!("{} edges all match", r.edges.len())),
                ));
            } else {
                findings.push((
                    "G11.edges".into(),
                    Verdict::Fail(format!(
                        "missing=[{}] extra=[{}]",
                        missing
                            .iter()
                            .map(|t| format!("{}~{}:{}", t.0, t.1, t.2))
                            .collect::<Vec<_>>()
                            .join(", "),
                        extra
                            .iter()
                            .map(|t| format!("{}~{}:{}", t.0, t.1, t.2))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                ));
            }
        }

        // ── G12 geometric legality ───────────────────────────────────────
        findings.push(num_check(
            "G12.box_box",
            0,
            r.box_box,
            r.evaluated.boxes,
            "box collisions".into(),
        ));
        findings.push(num_check(
            "G12.wire_box",
            0,
            r.wire_box,
            r.evaluated.nets,
            "wire through box".into(),
        ));
        findings.push(num_check(
            "G12.s6_size",
            0,
            r.s6_violations,
            r.evaluated.sizes,
            "box cannot fit pins (S6)".into(),
        ));
        findings.push(num_check(
            "G12.offcanvas",
            0,
            r.offcanvas_boxes,
            r.evaluated.boxes,
            "negative-coordinate boxes".into(),
        ));

        let red = findings.iter().filter(|f| matches!(f.1, Verdict::Fail(_))).count();
        let green = findings.iter().filter(|f| matches!(f.1, Verdict::Ok(_))).count();
        let skipped = findings.iter().filter(|f| matches!(f.1, Verdict::Skip(_))).count();
        LayerDiff {
            layer: r.layer.clone(),
            findings,
            red,
            green,
            skipped,
        }
    }
}

fn num_check(id: &str, expect: usize, actual: usize, evaluated: usize, note: String) -> (String, Verdict) {
    if evaluated == 0 {
        return (
            id.to_string(),
            Verdict::Skip(format!("eval=0 ({note})")),
        );
    }
    if expect == actual {
        (
            id.to_string(),
            Verdict::Ok(format!("{actual} == golden {expect}（{note}）")),
        )
    } else {
        (
            id.to_string(),
            Verdict::Fail(format!("{actual} != golden {expect}（{note}）")),
        )
    }
}

fn fmt_list(v: &[String]) -> String {
    if v.is_empty() {
        "[]".to_string()
    } else if v.len() <= 8 {
        format!("[{}]", v.join(","))
    } else {
        format!("[{} …+{}]", v[..8].join(","), v.len() - 8)
    }
}

fn multiset_diff_str3(
    exp: &[(String, String, String)],
    act: &[(String, String, String)],
) -> (Vec<(String, String, String)>, Vec<(String, String, String)>) {
    use std::collections::BTreeMap;
    let key = |t: &(String, String, String)| format!("{}\u{1}{}\u{1}{}", t.0, t.1, t.2);
    let mut e: BTreeMap<String, i32> = BTreeMap::new();
    let mut a: BTreeMap<String, i32> = BTreeMap::new();
    for t in exp {
        *e.entry(key(t)).or_insert(0) += 1;
    }
    for t in act {
        *a.entry(key(t)).or_insert(0) += 1;
    }
    let mut missing = Vec::new();
    let mut extra = Vec::new();
    for (k, n) in &e {
        let m = a.get(k).copied().unwrap_or(0);
        if m < *n {
            let parts: Vec<&str> = k.split('\u{1}').collect();
            for _ in 0..(n - m) {
                missing.push((parts[0].into(), parts[1].into(), parts[2].into()));
            }
        }
    }
    for (k, m) in &a {
        let n = e.get(k).copied().unwrap_or(0);
        if m > &n {
            let parts: Vec<&str> = k.split('\u{1}').collect();
            for _ in 0..(m - n) {
                extra.push((parts[0].into(), parts[1].into(), parts[2].into()));
            }
        }
    }
    (missing, extra)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiset_diff_counts_multiplicity() {
        let (m, e) = multiset_diff(
            &["a".to_string(), "b".to_string()],
            &["a".to_string(), "a".to_string(), "c".to_string()],
        );
        assert_eq!(m, vec!["b".to_string()]);
        assert_eq!(e, vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn skip_when_eval_zero() {
        let (_, v) = num_check("G12.box_box", 0, 0, 0, "t".into());
        assert!(matches!(v, Verdict::Skip(_)), "eval=0 must SKIP, not green");
    }

    #[test]
    fn golden_toml_parses() {
        // Minimal sample isomorphic to baseline/render_golden.toml
        let text = r#"
[layer.main]
module = "main"
boxes = 10
synth_boxes = 0
rail_flags = 0
gnd_edges = 0
power_edges = 4
top_passives = 0
box_names = ["a", "b"]

[[layer.main.edge]]
from = "a"
to = "b"
label = "USB_5V"
"#;
        let g: RenderGolden = toml::from_str(text).unwrap();
        assert_eq!(g.layer["main"].boxes, 10);
        assert_eq!(g.layer["main"].edge.len(), 1);
    }
}
