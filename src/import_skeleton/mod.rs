// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `import --skeleton`: generate a compilable mcode project skeleton from an
//! `mct.netlist/1` JSON file (the mct corpus-tool contract).
//!
//! The engine is pure-in-memory [`plan`] shared verbatim by the CLI face and
//! the `import_skeleton` RPC method. Contract isolation: the structs here are
//! a *mirror* of the mct `nl` IR — no path dependency on the mct `nl` crate,
//! only the `schema/mct-netlist.schema.json` document and the `schema_version`
//! gate. An unrecognized schema version bails rather than guessing.
//!
//! Conservative stubbing is the ground rule: component values, pin directions
//! and power domains are emitted as `TODO(import)` stubs unless they parse
//! with high confidence — a wrong guess poisons every downstream check, a
//! TODO poisons nothing. The generated project must compile (`selfcheck`,
//! 0 errors) before it is handed out.

mod name;
mod render;
mod selfcheck;
mod value;

use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Mirror of the mct contract root. Only the fields the skeleton needs are
/// read; everything else defaults so older writers stay consumable.
#[derive(Deserialize, Debug)]
pub struct MctDesign {
    pub meta: MctMeta,
    #[serde(default)]
    pub components: Vec<MctComponent>,
    #[serde(default)]
    pub nets: Vec<MctNet>,
    #[serde(default)]
    pub dangle: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct MctMeta {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub attach_rate: Option<f64>,
}

#[derive(Deserialize, Debug)]
pub struct MctComponent {
    #[serde(default)]
    pub designator: String,
    #[serde(default)]
    pub libref: String,
    #[serde(default)]
    pub comment: String,
}

#[derive(Deserialize, Debug)]
pub struct MctNet {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub pins: Vec<MctPinRef>,
    #[serde(default)]
    pub power: Vec<String>,
    #[serde(default)]
    pub labels: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct MctPinRef {
    #[serde(default)]
    pub designator: String,
    #[serde(default)]
    pub pin: String,
    #[serde(default)]
    pub gate: Option<String>,
}

/// In-memory result of [`plan`]: the files to write plus a machine-readable
/// summary. The CLI writes `files` under `--out-dir`; the RPC face returns
/// them verbatim (no disk writes on the RPC side).
#[derive(Debug)]
pub struct SkeletonPlan {
    pub files: BTreeMap<String, String>,
    pub report: Value,
}

pub struct Opts<'a> {
    /// project name (falls back to the JSON title stem, then `skeleton`)
    pub name: Option<&'a str>,
}

impl Default for Opts<'_> {
    fn default() -> Self {
        Opts { name: None }
    }
}

/// One component's classification, decided once up front.
struct CompPlan {
    designator: String,
    libref: String,
    comment: String,
    /// inline builtin (class + args) — rendered as `inst::CAP(...)` chain node
    inline: Option<value::Inline>,
    /// chain line already emitted (an inline renders exactly once)
    placed: bool,
    /// instance ident after legalization
    ident: String,
    /// class ident after legalization (`{libref}.{designator}` shape)
    class: String,
    /// pins that appear in at least one net, as (raw pin name, placeholder)
    net_pins: Vec<(String, String)>,
    /// value-parsing TODO text for the block/header (None when inlined)
    value_todo: Option<String>,
    /// set when an eagle designator spans several gates — the skeleton's
    /// pin-number-only names would merge different gates' pins
    gate_todo: bool,
}

/// Read + validate + classify + render. No filesystem access.
pub fn plan(text: &str, opts: &Opts) -> Result<SkeletonPlan, String> {
    let d: MctDesign = serde_json::from_str(text)
        .map_err(|e| format!("import: failed to parse the mct.netlist/1 JSON: {e}"))?;
    const SUPPORTED: &str = "mct.netlist/1";
    if d.meta.schema_version != SUPPORTED {
        return Err(format!(
            "import: unrecognized schema_version '{}' (this mcc only speaks \
             '{SUPPORTED}'); re-export with the matching mct version",
            d.meta.schema_version
        ));
    }

    // -- Pin ownership: designator -> the pin set that appears in a net
    //    (the mct IR carries pins only through nets) -----------------------
    let mut comp_pins: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in &d.nets {
        for p in &n.pins {
            let pins = comp_pins.entry(p.designator.clone()).or_default();
            if !pins.contains(&p.pin) {
                pins.push(p.pin.clone());
            }
        }
    }

    // -- Instance idents legalize first; net idents avoid them ------------
    let mut comps: Vec<CompPlan> = Vec::new();
    let mut inst_leg = name::Legalizer::default();
    for c in &d.components {
        let pins = comp_pins.get(&c.designator).cloned().unwrap_or_default();
        let names = name::PinNames::build(&pins);
        let net_pins: Vec<(String, String)> = pins
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), names.get(i).to_string()))
            .collect();
        // Inline iff exactly two pins are netted + a whitelisted libref + a
        // high-confidence value parse. A two-pin part with a missing
        // (dangling) pin degrades to a block + TODO instead of half a chain.
        let inline = (net_pins.len() == 2).then(|| value::parse(&c.libref, &c.comment)).flatten();
        let value_todo = (inline.is_none() && !c.comment.trim().is_empty()).then(|| {
            format!(
                "value '{}' did not parse under the high-confidence rules -> \
                 partno stub",
                c.comment.trim()
            )
        });
        let (ident, _) = inst_leg.instance(&c.designator);
        let class = format!(
            "{}.{}",
            name::class_segment(c.libref.split('.').next().unwrap_or(&c.libref).trim()),
            name::class_segment(&c.designator)
        );
        // eagle: one designator across several gates (multi-gate parts) —
        // the pin-number-only skeleton names would merge different gates' pins
        let mut gates: Vec<&str> = Vec::new();
        for n in &d.nets {
            for p in &n.pins {
                if p.designator == c.designator {
                    if let Some(g) = &p.gate {
                        if !gates.contains(&g.as_str()) {
                            gates.push(g);
                        }
                    }
                }
            }
        }
        let gate_todo = gates.len() > 1;
        comps.push(CompPlan {
            designator: c.designator.clone(),
            libref: c.libref.clone(),
            comment: c.comment.clone(),
            inline,
            placed: false,
            ident,
            class,
            net_pins,
            value_todo,
            gate_todo,
        });
    }
    let inst_idents: Vec<String> = comps.iter().map(|c| c.ident.clone()).collect();

    // -- Net anchor names = the mct display name
    //    (power > label > explicit name > N$k+1) ---------------------------
    let mut net_leg = name::Legalizer::default();
    net_leg.reserve_block(inst_idents);
    let display: Vec<String> = d
        .nets
        .iter()
        .enumerate()
        .map(|(k, n)| {
            n.power
                .first()
                .or_else(|| n.labels.first())
                .cloned()
                .or_else(|| n.name.clone())
                .unwrap_or_else(|| format!("N${}", k + 1))
        })
        .collect();
    let mut net_idents = Vec::with_capacity(d.nets.len());
    let mut net_renamed = Vec::with_capacity(d.nets.len());
    for raw in &display {
        let (id, renamed) = net_leg.net(raw);
        net_idents.push(id);
        net_renamed.push(renamed);
    }

    // -- Per-net connection lines: inline parts become chain links (bridging
    //    to the opposite net's anchor), everyone else joins per participant
    //    (rendering consumes the inline decision via Option::take — exactly
    //    once; the count snapshots first) ----------------------------------
    let inline_count = comps.iter().filter(|c| c.inline.is_some()).count();
    let pin_nets = pin_nets_of(&d);
    let mut covered: BTreeSet<(String, String)> = BTreeSet::new();
    let mut net_lines: Vec<Vec<String>> = vec![Vec::new(); d.nets.len()];
    // A chain link's opposite net is referenced by ident even with no lines
    // of its own — it needs an anchor declared.
    let mut linked: BTreeSet<usize> = BTreeSet::new();
    for (k, n) in d.nets.iter().enumerate() {
        for p in &n.pins {
            let key = (p.designator.clone(), p.pin.clone());
            if covered.contains(&key) {
                continue;
            }
            let Some(cp) = comps.iter_mut().find(|c| c.designator == p.designator) else {
                continue; // designator outside the component table: out-of-contract input, skip defensively
            };
            match cp.inline.as_ref() {
                Some(inline) if !cp.placed => {
                    let other = other_net_of(&pin_nets, k, &p.designator, &p.pin);
                    let other_ident = other
                        .map(|m| net_idents[m].clone())
                        .unwrap_or_else(|| net_idents[k].clone());
                    net_lines[k].push(format!(
                        "{} - {}::{}({}) - {}",
                        net_idents[k], cp.ident, inline.class, inline.args, other_ident
                    ));
                    if let Some(m) = other {
                        linked.insert(m);
                    }
                    covered.insert(key);
                    cp.placed = true;
                    covered.insert((
                        p.designator.clone(),
                        other_pin_of(&d, &p.designator, &p.pin),
                    ));
                }
                _ => {
                    let placeholder = cp
                        .net_pins
                        .iter()
                        .find(|(raw, _)| raw == &p.pin)
                        .map(|(_, ph)| ph.clone())
                        .unwrap_or_else(|| format!("p{}", p.pin));
                    net_lines[k]
                        .push(format!("{} -> {}.{}", net_idents[k], cp.ident, placeholder));
                    covered.insert(key);
                }
            }
        }
    }
    let mut used_nets: BTreeSet<usize> = net_lines
        .iter()
        .enumerate()
        .filter(|(_, lines)| !lines.is_empty())
        .map(|(k, _)| k)
        .collect();
    used_nets.extend(linked);

    let name = opts.name.map(str::to_string).unwrap_or_else(|| {
        let t = d.meta.title.trim();
        if t.is_empty() { "skeleton".to_string() } else { t.to_string() }
    });

    let rendered = render::render_main(
        &d, &comps, &net_lines, &net_idents, &net_renamed, &display, &used_nets, &name,
    );
    let mut files = BTreeMap::new();
    files.insert("project.toml".to_string(), render::render_project(&name));
    files.insert("main.mc".to_string(), rendered.main);
    files.insert("bom.mc".to_string(), render::render_bom());

    // -- Compile self-check: deliver only at 0 errors (any generator
    //    regression explodes here) ----------------------------------------
    let sc = selfcheck::check(&files);
    if sc.errors > 0 {
        return Err(format!(
            "import: skeleton self-check failed ({} errors) - generator \
             regression, refusing delivery\n{}",
            sc.errors, sc.detail
        ));
    }

    let report = json!({
        "schema_version": "import_skeleton.1.0",
        "tool": d.meta.tool,
        "source": d.meta.source,
        "name": name,
        "components": comps.len(),
        "inline": inline_count,
        "blocks": comps.len() - inline_count,
        "nets": d.nets.len(),
        "anchors": used_nets.len(),
        "dangle": d.dangle.len(),
        "todos": rendered.todo_count,
        "selfcheck_warnings": sc.warnings,
        "files": files.keys().cloned().collect::<Vec<_>>(),
    });
    Ok(SkeletonPlan { files, report })
}

// ---------------------------------------------------------------- helpers

/// (designator, pin) -> the nets it appears in (a chain link finds its
/// opposite net here)
fn pin_nets_of(d: &MctDesign) -> BTreeMap<(String, String), Vec<usize>> {
    let mut m: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (k, n) in d.nets.iter().enumerate() {
        for p in &n.pins {
            m.entry((p.designator.clone(), p.pin.clone())).or_default().push(k);
        }
    }
    m
}

/// A two-pin inline component's other pin (same designator, different pin).
fn other_pin_of(d: &MctDesign, designator: &str, pin: &str) -> String {
    for n in &d.nets {
        for p in &n.pins {
            if p.designator == designator && p.pin != pin {
                return p.pin.clone();
            }
        }
    }
    String::new()
}

/// The first net ≠ `k` that the component's OTHER pin sits in.
fn other_net_of(
    pin_nets: &BTreeMap<(String, String), Vec<usize>>,
    k: usize,
    designator: &str,
    pin: &str,
) -> Option<usize> {
    for ((dg, pn), nets) in pin_nets {
        if dg == designator && pn != pin {
            for &m in nets {
                if m != k {
                    return Some(m);
                }
            }
        }
    }
    None
}
