// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power-intent declarations — typed capture (power-intent-design.md §5).
//!
//! Grammar (mcast/src/mca.y, clause 3.7/3.8/3.9; keyword `conduit`, legacy
//! alias `ref` accepted since 2026-09-07):
//! ```text
//!   conduit = `conduit <mc_ids> <@attrs...>`                      → MCAST_REF(107)
//!   domain  = `domain  <mc_ids> <@attrs...> { rail ... }`         → MCAST_DOMAIN(108)
//!   rail    = `rail [<hot>, <ret>]::<iface>(<params>)`            → MCAST_RAIL(109)
//! ```
//! AST shapes (confirmed on `mca -t v` dumps; node names kept internal):
//! ```text
//!   CONDUIT(107).sub = [ IDS(name), (MCAST_ATTRIBUTE)* ]
//!   DOMAIN(108).sub  = [ IDS(name), (MCAST_ATTRIBUTE)*, MCAST_BODY ]
//!                       BODY.sub = (MCAST_RAIL)*
//!   RAIL(109).sub    = DECLARE(26)
//!                       DECLARE.sub = [ CLASS(28), INSTANCE(29) ]
//!                         CLASS.sub      = [ IDS(iface), (PARAMS(23))* ]
//!                         INSTANCE.sub   = OPD_SQUARE_VEC(62)
//!                                           62.sub = [ OPD(hot), OPD(ret) ]
//! ```
//! Trailing `@key(value)` / `@key` attributes reuse the ordinary 3.1
//! MCAST_ATTRIBUTE(MCAST_ATT_ID, MCAST_ATT_VALUES?) node, so the existing
//! [`McAttributes`] reader consumes them unchanged (bare flags like `@star`
//! have no value sibling).
//!
//! Relation edges (`@bridge(a,b)` / `@couple(a,b)` / `@clamp(net)` —
//! power-intent-design.md §3/§6 iron rule 2) trail the **connection** net
//! statement: `MCAST_NET(33).sub = [ <phrase>, (MCAST_ATTRIBUTE)* ]`. The Rust
//! net reader consumes only the phrase head and drops those trailing nodes, so
//! [`McPowerDecls`] re-walks the net clause and captures them as [`McNetEdge`]s
//! (they never trigger L0 merge; they only build L1 potential-class edges).
//!
//! This increment **captures structure only** (identity + attributes + rail
//! hot/ret/iface + net relation-edge attributes). Numeric decode of rail ctor
//! params (volt / tol / capacity / eff) is deferred to the rail-contract step
//! (design §13 landing 2): params are kept as `key` + token text for now, with
//! spans retained for re-anchoring. Consuming the captured edges in the
//! flatten/ERC layer is design §13 landing 1.
#![allow(dead_code)] // store-only layer: captured fields are read by later phases

use crate::ast::macros::*;
use crate::ast::node::AstNode;
use crate::ast::sem::Span;
use crate::semantic::component::mc_attr::{McAttrVal, McAttribute, McAttributes};
use crate::semantic::component::mc_pins::{McPwrPin, PwrDir};
use std::ops::Range;

/// All power-intent declarations collected from one module body.
#[derive(Debug, Clone, Default)]
pub struct McPowerDecls {
    pub refs: Vec<McRefDecl>,
    pub domains: Vec<McDomainDecl>,
    /// Relation edges hung on connection-net statements (`@bridge`/`@couple`/
    /// `@clamp`/`@star`), in source order. A net with no trailing attributes
    /// contributes no edge.
    pub net_edges: Vec<McNetEdge>,
    /// Module-interface port rows that carry identity-axis trailing attributes
    /// (`io … @class/@noise/@nature/@return/@exposed`, `out … @bind_role`) —
    /// design §5.1 unified slot. The generic net reader registers the port
    /// operands but drops the trailing words, so they are re-captured here.
    pub ports: Vec<McPortDecl>,
}

impl McPowerDecls {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the module carries no power-intent declaration. Used by the
    /// empty-module (E5459) predicate so a module that only *declares* refs /
    /// domains is not mistaken for a stub.
    pub fn is_empty(&self) -> bool {
        self.refs.is_empty() && self.domains.is_empty()
    }

    pub fn parse_ref(&mut self, node: &AstNode) {
        if let Some(r) = McRefDecl::from_node(node) {
            self.refs.push(r);
        }
    }

    pub fn parse_domain(&mut self, node: &AstNode) {
        if let Some(d) = McDomainDecl::from_node(node) {
            self.domains.push(d);
        }
    }

    pub fn parse_net(&mut self, node: &AstNode) {
        if let Some(e) = McNetEdge::from_node(node) {
            self.net_edges.push(e);
        }
    }

    pub fn parse_port(&mut self, node: &AstNode) {
        if let Some(p) = McPortDecl::from_node(node) {
            self.ports.push(p);
        }
    }

    /// Decode the module's `conduit` declarations into the L1 identity view
    /// the FlatErc checks consume (design §3.2): name + `@role` value + `@star`
    /// flag. A conduit with no `@role` defaults to `main` (default per §5.2).
    pub fn l1_refs(&self) -> Vec<L1Ref> {
        self.refs
            .iter()
            .map(|r| L1Ref {
                name: r.name.clone(),
                role: attr_texts(&r.attrs, "role").into_iter().next(),
                star: has_attr(&r.attrs, "star"),
                span: r.span.clone(),
            })
            .collect()
    }

    /// Decode the module's connection-net relation edges into typed L1 edges
    /// (design §3.3): `@bridge`/`@couple` carry two endpoint net names,
    /// `@clamp` carries one. A trailing attribute whose key is not one of the
    /// three relation words is not an edge (it is some other net policy).
    pub fn l1_edges(&self) -> Vec<L1Edge> {
        let mut out = Vec::new();
        for e in &self.net_edges {
            for a in e.attrs.iter() {
                let kind = match a.id.to_string().as_str() {
                    "bridge" => L1EdgeKind::Bridge,
                    "couple" => L1EdgeKind::Couple,
                    "clamp" => L1EdgeKind::Clamp,
                    _ => continue,
                };
                out.push(L1Edge {
                    kind,
                    endpoints: value_texts(a),
                    span: e.span.clone(),
                });
            }
        }
        out
    }

    /// Decode every DC `rail [hot, ret]::DC(v, tol, capacity, eff)` guarantee
    /// into a typed L1 contract (§4.1). Only `::DC` rails are DC pairs here —
    /// a non-`DC` iface (an AC/nature rail) belongs to the later AC-axis step
    /// and is skipped. A rail whose ctor args fail to decode still lists with
    /// `bad: Some(..)` so the two-root check stays independent of value decode.
    pub fn l1_rails(&self) -> Vec<L1Rail> {
        let mut out = Vec::new();
        for d in &self.domains {
            for r in &d.rails {
                if r.iface != "DC" {
                    continue;
                }
                out.push(decode_rail(&d.name, r));
            }
        }
        out
    }

    /// Decode identity-bearing module port rows into typed L1 identity reads
    /// (design §5.2/§8/§9): one entry per declared net-visible member of each
    /// row, carrying the row's identity-axis words (`@class`/`@nature`/`@noise`/
    /// `@return`/`@exposed`/`@bind_role`). No rule semantics here — the ERC
    /// axes (SN-1 return coupling, PWR-6 exposed→clamp, port role contract)
    /// consume these later.
    pub fn l1_ports(&self) -> Vec<L1Port> {
        let mut out = Vec::new();
        for p in &self.ports {
            for name in &p.names {
                out.push(L1Port {
                    kind: p.kind.clone(),
                    name: name.clone(),
                    class: first_text(&p.attrs, "class"),
                    nature: first_text(&p.attrs, "nature"),
                    noise: first_text(&p.attrs, "noise"),
                    ret: first_text(&p.attrs, "return"),
                    exposed: attr_texts(&p.attrs, "exposed"),
                    bind_role: first_text(&p.attrs, "bind_role"),
                    span: p.span.clone(),
                });
            }
        }
        out
    }
}

/// One decoded DC rail guarantee — the §4.1 window shape (`v±tol` →
/// `[vmin, vmax]` is derived by the ERC consumer), signed nominal, and the
/// source-exclusive budget params. `v` is `None` only when the rail's nominal
/// text does not decode to a DC volts value; that failure is kept as `bad`
/// (reported by the ERC rule) rather than silently dropped.
#[derive(Debug, Clone)]
pub struct L1Rail {
    pub domain: String,
    pub hot: String,
    pub ret: String,
    /// Verbatim nominal text as written (`3.3V`, `5A`, …) — for messages.
    pub v_text: String,
    /// Signed nominal volts; `None` when `v_text` is not a DC volts value.
    pub v: Option<f64>,
    /// ±x% tolerance as a fraction of `|v|` (`0.05` = ±5%).
    pub tol: Option<f64>,
    pub capacity_amps: Option<f64>,
    pub eff: Option<f64>,
    /// First decode problem, if any (bad nominal unit / unknown param key).
    pub bad: Option<String>,
    pub span: Span,
}

/// Decode one `McRailDecl` into its typed L1 contract.
fn decode_rail(domain: &str, r: &McRailDecl) -> L1Rail {
    let mut out = L1Rail {
        domain: domain.to_string(),
        hot: r.hot.clone(),
        ret: r.ret.clone(),
        v_text: String::new(),
        v: None,
        tol: None,
        capacity_amps: None,
        eff: None,
        bad: None,
        span: r.span.clone(),
    };
    for p in &r.params {
        match p.key.as_deref() {
            None => {
                out.v_text = p.text.clone();
                match parse_volts(&p.text) {
                    Some(x) => out.v = Some(x),
                    None => flag_bad(
                        &mut out.bad,
                        format!("nominal '{}' is not a DC volts value", p.text),
                    ),
                }
            }
            Some("tol") => match parse_tol(&p.text) {
                Some(x) => out.tol = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("tol '{}' is not a ±percent window", p.text),
                ),
            },
            Some("capacity") => match parse_amps(&p.text) {
                Some(x) => out.capacity_amps = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("capacity '{}' is not a DC current", p.text),
                ),
            },
            Some("eff") => match parse_frac(&p.text) {
                Some(x) => out.eff = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("eff '{}' is not an efficiency factor", p.text),
                ),
            },
            Some(k) => flag_bad(
                &mut out.bad,
                format!("unknown DC rail contract parameter '{k}'",),
            ),
        }
    }
    if r.params.is_empty() {
        flag_bad(&mut out.bad, "missing nominal 'v'".to_string());
    }
    out
}

// ============================================================================
// §5.2 direction-word family — component `psrc/psnk/psbi` pin `::DC(...)` decode
// ============================================================================

/// One decoded component `psrc/psnk/psbi ...::DC(...)` pin contract
/// (design §4.1/§4.4, rail-contract-design.md §8). The **guarantee** words decode
/// exactly like a domain rail (`psrc` / `psbi` — positional nominal +
/// `tol`/`capacity`/`eff` source budget, source-exclusive §5.2). The
/// **requirement** word (`psnk`) writes its nominal plus an optional `amp`
/// demand key (sink-exclusive, rail-contract §8.1); a source-exclusive budget
/// key on a sink, an `amp` on a source row, or a window key (`req`/`abs` — which
/// belong in the component `spec` block, §4.4 write-site rule, never
/// per-schematic) is flagged. `v` is `None` when the nominal text does not
/// decode to a DC volts value; the failure is kept as `bad` (reported by the
/// E-PWR-001 ERC rule) rather than silently dropped.
#[derive(Debug, Clone)]
pub struct L1PwrPin {
    pub dir: PwrDir,
    /// First `::` member — the power/hot terminal at the def (e.g. `IN`).
    pub hot: String,
    pub ret: Option<String>,
    /// Verbatim nominal text as written (`3.3V`, `5A`, …) — for messages.
    pub v_text: String,
    /// Signed nominal volts; `None` when `v_text` is not a DC volts value.
    pub v: Option<f64>,
    /// ±x% tolerance fraction (source words only; a sink never carries one).
    pub tol: Option<f64>,
    pub capacity_amps: Option<f64>,
    pub eff: Option<f64>,
    /// The `psnk` instance's declared DC current demand (`amp:`, in amps) —
    /// sink-exclusive (rail-contract-design.md §8.1); the PWR-4 budget kernel
    /// sums it per net against the supply root's `capacity`.
    pub amp: Option<f64>,
    /// First decode problem, if any (missing/non-DC nominal, a
    /// source-exclusive key on a sink, an `amp` on a source row, or a
    /// `spec`-belonging window key).
    pub bad: Option<String>,
    /// Trailing `@attr…` run of the source pin row (design §5.1), carried
    /// through decode so identity words (`@class(digital|analog)`, …) reach the
    /// rule layer instead of being dropped between capture and the typed read.
    pub attrs: McAttributes,
    pub span: Range<usize>,
}

/// Decode one captured [`McPwrPin`] into its typed L1 contract. Direction
/// drives which *side* of the contract the `::DC` writes: `Src`/`Bi` →
/// guarantee (mirror [`decode_rail`]), `Snk` → requirement (§4.1 table).
pub(crate) fn decode_pwr_pin(pin: &McPwrPin) -> L1PwrPin {
    let is_sink = pin.dir == PwrDir::Snk;
    let mut out = L1PwrPin {
        dir: pin.dir,
        hot: pin.hot.clone(),
        ret: pin.ret.clone(),
        v_text: String::new(),
        v: None,
        tol: None,
        capacity_amps: None,
        eff: None,
        amp: None,
        bad: None,
        attrs: pin.attrs.clone(),
        span: pin.span.clone(),
    };
    for p in &pin.params {
        match p.key.as_deref() {
            None => {
                out.v_text = p.text.clone();
                match parse_volts(&p.text) {
                    Some(x) => out.v = Some(x),
                    None => flag_bad(
                        &mut out.bad,
                        format!("nominal '{}' is not a DC volts value", p.text),
                    ),
                }
            }
            Some("tol") | Some("capacity") | Some("eff") if is_sink => flag_bad(
                &mut out.bad,
                format!(
                    "'{}' is a source-exclusive budget key (§5.2); a psnk declares only its nominal (and optional amp demand, §8.1)",
                    p.key.as_deref().unwrap_or("")
                ),
            ),
            // rail-contract-design.md §8.1: `amp` is the sink-exclusive demand
            // key. On a source/psbi row it is off-register — a source declares
            // what it can supply (`capacity`), never a net load.
            Some("amp") if !is_sink => flag_bad(
                &mut out.bad,
                format!(
                    "'amp' is a sink-exclusive demand key (§8.1); a source declares capacity, its input draw is derived, not a net load",
                ),
            ),
            Some("amp") => match parse_amps(&p.text) {
                Some(x) => out.amp = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("amp '{}' is not a DC current", p.text),
                ),
            },
            Some("tol") => match parse_tol(&p.text) {
                Some(x) => out.tol = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("tol '{}' is not a ±percent window", p.text),
                ),
            },
            Some("capacity") => match parse_amps(&p.text) {
                Some(x) => out.capacity_amps = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("capacity '{}' is not a DC current", p.text),
                ),
            },
            Some("eff") => match parse_frac(&p.text) {
                Some(x) => out.eff = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("eff '{}' is not an efficiency factor", p.text),
                ),
            },
            Some("req") | Some("abs") => flag_bad(
                &mut out.bad,
                format!(
                    "window key '{}' belongs in the component spec (input_req, §4.4), not on the pin ::DC",
                    p.key.as_deref().unwrap_or("")
                ),
            ),
            Some(k) => flag_bad(
                &mut out.bad,
                format!("unknown DC pin contract parameter '{k}'",),
            ),
        }
    }
    if out.v_text.is_empty() {
        if is_sink {
            flag_bad(&mut out.bad, "sink nominal is mandatory (§4.4)".to_string());
        } else {
            flag_bad(&mut out.bad, "missing nominal 'v'".to_string());
        }
    }
    out
}

pub(crate) fn flag_bad(slot: &mut Option<String>, msg: String) {
    if slot.is_none() {
        *slot = Some(msg);
    }
}

/// Parse a signed DC volts value from rail nominal text: `3.3V`, `-15V`,
/// `5` (bare number = volts on a DC rail). A foreign unit (`5A`, `5Hz`) and
/// any window/structural form (`~`, `±`, `*`) do not decode.
pub(crate) fn parse_volts(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() || t.contains(['~', '±', '*']) {
        return None;
    }
    let (sign, rest) = match t.strip_prefix('-') {
        Some(r) => (-1.0, r),
        None => (1.0, t.strip_prefix('+').unwrap_or(t)),
    };
    let num = rest
        .strip_suffix('V')
        .or_else(|| rest.strip_suffix('v'))
        .unwrap_or(rest);
    if num.trim().is_empty() || num != num.trim() {
        return None;
    }
    num.trim().parse::<f64>().ok().map(|x| sign * x)
}

/// Parse a ±percent tolerance window from `±5%` / `5%` text → fraction (0.05).
pub(crate) fn parse_tol(text: &str) -> Option<f64> {
    let t = text.trim();
    let t = t.strip_prefix('±').unwrap_or(t);
    let t = t.strip_suffix('%').unwrap_or(t);
    t.trim().parse::<f64>().ok().map(|x| x.abs() / 100.0)
}

/// Parse a DC current from capacity text (`500mA`, `1.5A`, `20mA`, `300mA`).
pub(crate) fn parse_amps(text: &str) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase();
    let (mult, body) = if let Some(r) = t.strip_suffix("ma") {
        (0.001, r)
    } else if let Some(r) = t.strip_suffix("µa") {
        (1e-6, r)
    } else if let Some(r) = t.strip_suffix("ua") {
        (1e-6, r)
    } else if let Some(r) = t.strip_suffix('a') {
        (1.0, r)
    } else {
        (1.0, t.as_str())
    };
    body.trim().parse::<f64>().ok().map(|x| x * mult)
}

/// Parse a plain factor (`0.95`) or percentage (`95%`) → fraction.
pub(crate) fn parse_frac(text: &str) -> Option<f64> {
    let t = text.trim();
    if let Some(r) = t.strip_suffix('%') {
        r.trim().parse::<f64>().ok().map(|x| x / 100.0)
    } else {
        t.parse::<f64>().ok()
    }
}

/// L1 conductor identity — one `conduit` projected for the ERC checks. `role`
/// is `None` only when the conduit carries no `@role` at all (defaults to `main`).
#[derive(Debug, Clone)]
pub struct L1Ref {
    pub name: String,
    pub role: Option<String>,
    pub star: bool,
    pub span: Span,
}

/// Which (DC, AC) relation a declared edge carries (design §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L1EdgeKind {
    /// (1,0) — DC stays on one plane; noise separated (ferrite / single-point resistor).
    Bridge,
    /// (0,1) — no DC path; AC/transient coupling only (Y capacitor).
    Couple,
    /// net→ref transient dump (TVS/MOV/GDT).
    Clamp,
}

/// One declared relation edge on a connection net, with its endpoint net names
/// (the `@bridge(a, b)` / `@couple(a, b)` argument list / `@clamp(ref)` arg).
#[derive(Debug, Clone)]
pub struct L1Edge {
    pub kind: L1EdgeKind,
    pub endpoints: Vec<String>,
    pub span: Span,
}

/// Text values of every attribute whose key is `key` (`role`, …).
fn attr_texts(attrs: &McAttributes, key: &str) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.id.to_string() == key)
        .flat_map(|a| value_texts(a))
        .collect()
}

/// First value of the attribute `key`, if present (`@class(analog)` → `analog`).
fn first_text(attrs: &McAttributes, key: &str) -> Option<String> {
    attr_texts(attrs, key).into_iter().next()
}

/// Whether an attribute with key `key` is present (bare flags like `@star`).
fn has_attr(attrs: &McAttributes, key: &str) -> bool {
    attrs.iter().any(|a| a.id.to_string() == key)
}

/// Endpoint/value strings of one attribute: identifier operands (net / ref
/// names, e.g. `@bridge(GND, GNDA)` → `["GND", "GNDA"]`) and literals; KVS /
/// expression / nested-attribute values are not endpoints.
fn value_texts(attr: &McAttribute) -> Vec<String> {
    attr.values
        .iter()
        .filter_map(|v| match v {
            McAttrVal::AttrVariable(opd, _) => Some(opd.to_string()),
            McAttrVal::AttrLiteral(lit) => Some(lit.to_string()),
            _ => None,
        })
        .collect()
}

/// A relation edge declared on one connection-net statement — the trailing
/// `@bridge(a,b)` / `@couple(a,b)` / `@clamp(net)` / `@star` of a `MCAST_NET`
/// clause (design §3/§6 iron rule 2: an edge never merges L0 conductors; it
/// only relates two L1 potential classes). Flatten/ERC consumption (§13
/// landing 1) reads the endpoint names out of `attrs`.
#[derive(Debug, Clone)]
pub struct McNetEdge {
    pub attrs: McAttributes,
    pub span: Span,
}

impl McNetEdge {
    fn from_node(node: &AstNode) -> Option<Self> {
        // Trailing attributes sit *after the first child* of the MCAST_NET
        // clause (phrase head, then @attrs as its next-siblings). Start the
        // walk at the first child — `iter()` then covers the phrase + the
        // trailing @attrs via their `next` pointers. A net with no trailing
        // @attrs is an ordinary connection and carries no edge.
        let head = node.get_sub_node()?;
        let attrs = collect_attrs(&head);
        if attrs.is_empty() {
            return None;
        }
        Some(Self {
            attrs,
            span: clause_span(node),
        })
    }
}

/// One module-interface port row that carries identity-axis trailing
/// attributes — `io MIC{P, N} @class(analog) @return(GNDA)`,
/// `out shield_to_earth @bind_role(earth)`, `io DP_OUT, DM_OUT @exposed(…)`.
/// Only rows with ≥1 trailing attribute are captured (a plain `io` row is an
/// ordinary port, not a power-intent declaration). `kind` is the iotype word
/// (`io`/`in`/`out`/`psrc`/…); `names` are the row's declared net-visible
/// members, bus-curly expanded (`MIC{P, N}` → `["MIC.P", "MIC.N"]`).
#[derive(Debug, Clone)]
pub struct McPortDecl {
    pub kind: String,
    pub names: Vec<String>,
    pub attrs: McAttributes,
    pub span: Span,
}

impl McPortDecl {
    fn from_node(node: &AstNode) -> Option<Self> {
        // MCAST_NET_PORTS.sub = [ IOTYPE keyword carrier,
        //                         (port operand)*,
        //                         (MCAST_ATTRIBUTE)* ]
        let head = node.get_sub_node()?;
        let mut children = head.iter();
        let iotype_node = children.next()?;
        let kind = leaf_text(&iotype_node)?;

        let mut names = Vec::new();
        let mut attrs = McAttributes::new();
        for c in children {
            match c.get_type() {
                MCAST_OPD => names.extend(opd_member_names(&c)),
                MCAST_ATTRIBUTE => {
                    attrs.parse(&c);
                }
                // DECLARE operands (a `psrc vin{…}::DC(…)` row) and other
                // non-identifier children contribute no net-visible member.
                _ => {}
            }
        }
        if attrs.is_empty() {
            // No identity words → an ordinary port row, not this layer's concern.
            return None;
        }
        Some(Self {
            kind,
            names,
            attrs,
            span: clause_span(node),
        })
    }
}

/// One decoded identity-bearing port member — the typed projection of
/// [`McPortDecl`] that the §9/§8 identity axes read. Field-per-key extraction
/// only; defaults are `None`/empty when the word is absent.
#[derive(Debug, Clone)]
pub struct L1Port {
    pub kind: String,
    pub name: String,
    pub class: Option<String>,
    pub nature: Option<String>,
    pub noise: Option<String>,
    /// `@return(<conduit>)` — the declared return-reference anchor (SN-1).
    pub ret: Option<String>,
    /// `@exposed(<level>)` levels — one per occurrence (PWR-5/6 threat entry).
    pub exposed: Vec<String>,
    /// `@bind_role(<role>)` — the port-role contract the parent binding must
    /// satisfy (composition-terminal-design §4 reference-binding).
    pub bind_role: Option<String>,
    pub span: Span,
}

/// `conduit GND @role(main) @star` — a conductor-identity declaration
/// (design §5.1/§6: the owning module's role on a bare net name).
#[derive(Debug, Clone)]
pub struct McRefDecl {
    pub name: String,
    pub attrs: McAttributes,
    pub span: Span,
}

impl McRefDecl {
    fn from_node(node: &AstNode) -> Option<Self> {
        let head = node.get_sub_node()?;
        let name_node = head.iter().next()?;
        if name_node.get_type() != MCAST_IDS {
            return None;
        }
        let name = id_text(&name_node)?;
        Some(Self {
            name,
            attrs: collect_attrs(&head),
            span: clause_span(node),
        })
    }
}

/// `domain DVDD @class(digital) { rail ... }` — a domain/rail source block.
/// The rail list is the semantic payload (each rail = one DC power pair).
#[derive(Debug, Clone)]
pub struct McDomainDecl {
    pub name: String,
    pub attrs: McAttributes,
    pub rails: Vec<McRailDecl>,
    pub span: Span,
}

impl McDomainDecl {
    fn from_node(node: &AstNode) -> Option<Self> {
        let head = node.get_sub_node()?;
        let name_node = head.iter().next()?;
        if name_node.get_type() != MCAST_IDS {
            return None;
        }
        let name = id_text(&name_node)?;

        let attrs = collect_attrs(&head);

        // Rails live under the domain's MCAST_BODY child.
        let rails = head
            .iter()
            .find(|c| c.is_type(MCAST_BODY))
            .and_then(|body| body.get_sub_node())
            .map(|first| {
                first
                    .iter()
                    .filter(|c| c.is_type(MCAST_RAIL))
                    .filter_map(|rail| McRailDecl::from_node(&rail))
                    .collect()
            })
            .unwrap_or_default();

        Some(Self {
            name,
            attrs,
            rails,
            span: clause_span(node),
        })
    }
}

/// One `rail [hot, ret]::iface(params)` line (the domain's DC power pair).
/// `hot`/`ret` are the member net names; `iface` e.g. `DC`. Params are the
/// iface constructor arguments (3.3V, tol:±5%, …) held as text for now.
#[derive(Debug, Clone)]
pub struct McRailDecl {
    pub hot: String,
    pub ret: String,
    pub iface: String,
    pub params: Vec<McRailParam>,
    pub span: Span,
}

impl McRailDecl {
    fn from_node(node: &AstNode) -> Option<Self> {
        // rail.sub = the DC-decl phrase. When a phrase wrapper is present,
        // drill to the MCAST_DECLARE below it.
        let content = node.get_sub_node()?;
        let declare = find_declare(&content)?;
        let span = clause_span(node);

        // Class side: iface name + optional constructor params.
        let mut iface = String::new();
        let mut params = Vec::new();
        if let Some(class) = child_of_type(&declare, MCAST_CLASS) {
            if let Some(ch) = class.get_sub_node() {
                if let Some(ids) = ch.iter().find(|c| c.is_type(MCAST_IDS)) {
                    iface = id_text(&ids).unwrap_or_default();
                }
                for c in ch.iter() {
                    if c.is_type(MCAST_PARAMS) {
                        params = read_params(&c);
                    }
                }
            }
        }

        // Operand side: the [hot, ret] square vector.
        let (mut hot, mut ret) = (String::new(), String::new());
        if let Some(inst) = child_of_type(&declare, MCAST_INSTANCE) {
            if let Some(sq) = find_square_vec(&inst) {
                if let Some(first) = sq.get_sub_node() {
                    let mut parts = first.iter().filter_map(|opd| net_name(&opd));
                    if let Some(h) = parts.next() {
                        hot = h;
                    }
                    if let Some(r) = parts.next() {
                        ret = r;
                    }
                }
            }
        }

        Some(Self {
            hot,
            ret,
            iface,
            params,
            span,
        })
    }
}

/// One rail constructor argument. Positional (e.g. `3.3V`) has `key: None`;
/// named (e.g. `tol:±5%`) has `key: Some("tol")`. `text` is the token-level
/// rendering (typed decode deferred to the rail-contract step).
#[derive(Debug, Clone)]
pub struct McRailParam {
    pub key: Option<String>,
    pub text: String,
    pub span: Range<usize>,
}

impl McRailParam {
    fn from_node(node: &AstNode) -> Option<Self> {
        let span = node_span(node);
        let value = node.get_sub_node()?;
        if value.get_type() == MCAST_OPD_COLON {
            // named param `key : value`
            if let Some(left) = value.get_sub_node() {
                // leaf_text already returns None for empty identifiers.
                let key = leaf_text(&left);
                let val = left.get_next().map(|r| value_text(&r)).unwrap_or_default();
                return Some(Self {
                    key,
                    text: val,
                    span,
                });
            }
        }
        Some(Self {
            key: None,
            text: value_text(&value),
            span,
        })
    }
}

// ============================================================================
// helpers
// ============================================================================

fn collect_attrs(head: &AstNode) -> McAttributes {
    let mut attrs = McAttributes::new();
    for c in head.iter() {
        if c.is_type(MCAST_ATTRIBUTE) {
            attrs.parse(&c);
        }
    }
    attrs
}

/// Locate a MCAST_DECLARE under `node` (skipping optional phrase wrappers).
fn find_declare(node: &AstNode) -> Option<AstNode> {
    if node.is_type(MCAST_DECLARE) {
        return Some(node.clone());
    }
    node.get_sub_node()
        .and_then(|head| head.iter().find(|c| c.is_type(MCAST_DECLARE)))
}

fn child_of_type(node: &AstNode, ty: u16) -> Option<AstNode> {
    node.get_sub_node()?.iter().find(|c| c.get_type() == ty)
}

fn find_square_vec(inst: &AstNode) -> Option<AstNode> {
    if inst.is_type(MCAST_OPD_SQUARE_VEC) {
        return Some(inst.clone());
    }
    // instance.sub is normally the vector directly
    let head = inst.get_sub_node()?;
    if head.is_type(MCAST_OPD_SQUARE_VEC) {
        return Some(head);
    }
    head.iter().find(|c| c.is_type(MCAST_OPD_SQUARE_VEC))
}

fn read_params(params_node: &AstNode) -> Vec<McRailParam> {
    let mut out = Vec::new();
    if let Some(head) = params_node.get_sub_node() {
        for p in head.iter() {
            if p.is_type(MCAST_PARAM) {
                if let Some(param) = McRailParam::from_node(&p) {
                    out.push(param);
                }
            }
        }
    }
    out
}

/// Net/name of an operand: `opd.sub = IDS` → identifier text.
fn net_name(opd: &AstNode) -> Option<String> {
    if let Some(sub) = opd.get_sub_node() {
        if sub.is_type(MCAST_IDS) {
            return id_text(&sub);
        }
    }
    crate::McOpd::new(opd).map(|o| o.to_string())
}

/// Identifier text from an MCAST_IDS chain (`ids.sub = id leaf`).
fn id_text(ids: &AstNode) -> Option<String> {
    leaf_text(ids).filter(|s| !s.is_empty())
}

/// Net-visible member names one port operand declares. A plain operand yields
/// its identifier (`DP_OUT` → `["DP_OUT"]`); a bus-curly operand expands to its
/// members (`MIC{P, N}` → `["MIC.P", "MIC.N"]`) because the net model names
/// the members, not the bus. Operands that are not identifier chains (a
/// `::DC(…)` declare instance, …) yield nothing.
fn opd_member_names(opd: &AstNode) -> Vec<String> {
    let mut out = Vec::new();
    let Some(ids) = opd.get_sub_node() else {
        return out;
    };
    let Some(base) = id_text(&ids) else {
        return out;
    };
    // The bus-curly member group rides the IDS child chain: base id leaf, then
    // MCAST_OPD_CURLY (`{P, N}`) as its next sibling.
    let mut members: Vec<String> = Vec::new();
    if let Some(first) = ids.get_sub_node() {
        for c in first.iter() {
            if c.is_type(MCAST_OPD_CURLY) {
                if let Some(m0) = c.get_sub_node() {
                    for m in m0.iter() {
                        if let Some(t) = leaf_text(&m) {
                            members.push(t);
                        }
                    }
                }
            }
        }
    }
    if members.is_empty() {
        out.push(base);
    } else {
        for m in members {
            out.push(format!("{base}.{m}"));
        }
    }
    out
}

/// First non-empty token text reachable from `node` (self or descendants).
/// Drills through IDS → IDA/ID leaves that carry the lexed text.
fn leaf_text(node: &AstNode) -> Option<String> {
    if let Some(c) = node.data_as_cstr() {
        if let Ok(s) = c.to_str() {
            let s = s.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    node.get_sub_node().and_then(|sub| leaf_text(&sub))
}

/// Text of a value node (rail param / kvs value). Structural signs (±) come
/// from the node kind, not the text.
fn value_text(node: &AstNode) -> String {
    match node.get_type() {
        MCAST_RANGE_PLUSMINUS => {
            let inner = leaf_text(node).unwrap_or_default();
            if inner.is_empty() {
                "±".to_string()
            } else if inner.starts_with('±') {
                inner
            } else {
                format!("±{inner}")
            }
        }
        MCAST_OPD_COLON => {
            if let Some(left) = node.get_sub_node() {
                let key = leaf_text(&left).unwrap_or_default();
                let right = left.get_next().map(|r| value_text(&r)).unwrap_or_default();
                if right.is_empty() {
                    if key.is_empty() {
                        String::new()
                    } else {
                        format!("{key}:")
                    }
                } else if key.is_empty() {
                    right
                } else {
                    format!("{key}: {right}")
                }
            } else {
                String::new()
            }
        }
        _ => leaf_text(node).unwrap_or_default(),
    }
}

fn clause_span(node: &AstNode) -> Span {
    let start = node.get_pos() as usize;
    Span {
        start,
        end: start + node.get_len() as usize,
    }
}

fn node_span(node: &AstNode) -> Range<usize> {
    let start = node.get_pos() as usize;
    start..(start + node.get_len() as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::infra::init::MCC_TEST_PARSE_LOCK;

    const SRC: &str = r#"module main {
    ref GND     @role(main) @star
    ref GNDA    @role(quiet)
    domain DVDD @class(digital) { rail [VDD_3V3, GND]::DC(3.3V, tol:±5%, capacity:500mA, eff:0.95) }
    domain AVDD @class(analog)  { rail [VDDA, GNDA]::DC(3.3V, tol:±2%, capacity:20mA) }
    domain ISO  { rail [V5V_ISO, GND_ISO]::DC(5V) }
}
"#;

    /// Parse `src` into a real McModule (pass1) and return its PI decls.
    fn parse_pi(src: &str) -> McPowerDecls {
        // The C parser / workspace tables are process-global and not
        // re-entrant across threads — hold the suite-wide parse lock
        // (init.rs), not a private one, or this races other tests (SIGSEGV
        // under `cargo test --lib` parallel).
        let _guard = MCC_TEST_PARSE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let root = crate::cli::datadir::data_root();
        crate::mcc_set_system_root(&root);
        crate::mcc_init();
        let uri: crate::McURI = "/mcc/pi-capture-test.mc".to_string();
        crate::mcc_load_from_string(&uri, src);
        let module = crate::definition_space()
            .workspace_modules()
            .into_iter()
            .find(|(sn, _)| sn.ident.to_string() == "main")
            .expect("module 'main' not parsed")
            .1;
        module.pi.clone()
    }

    #[test]
    fn captures_ref_identities_with_role_and_star() {
        let pi = parse_pi(SRC);
        assert_eq!(pi.refs.len(), 2, "refs: {}", pi.refs.len());
        assert_eq!(pi.refs[0].name, "GND");
        assert_eq!(pi.refs[1].name, "GNDA");

        let gnd_attr_texts: Vec<String> = pi.refs[0].attrs.iter().map(|a| a.to_string()).collect();
        assert!(
            gnd_attr_texts.iter().any(|t| t == "role = main"),
            "GND attrs: {gnd_attr_texts:?}"
        );
        // bare flag `@star` — parsed without panic, empty values, printed as key only
        assert!(
            gnd_attr_texts.iter().any(|t| t == "star"),
            "GND attrs: {gnd_attr_texts:?}"
        );
        assert!(
            pi.refs[0]
                .attrs
                .iter()
                .find(|a| a.values.is_empty())
                .is_some(),
            "star flag has no values"
        );
    }

    const SRC_EDGES: &str = r#"module main {
    ref GND   @role(main) @star
    ref GNDA  @role(quiet)
    domain DVDD @class(digital) { rail [VDD_3V3, GND]::DC(3.3V) }
    domain AVDD @class(analog)  { rail [VDDA, GNDA]::DC(3.3V, tol:±2%) }
    VDD_3V3 -> FB_a::IND.FB(600Ω@100MHz) -> VDDA @bridge(VDD_3V3, VDDA)
    VDD_3V3 -> FB_b::IND.FB(600Ω@100MHz) -> GND
}
"#;

    #[test]
    fn captures_net_relation_edges() {
        let pi = parse_pi(SRC_EDGES);
        // Only the net carrying trailing @bridge is an edge; the plain net
        // contributes none.
        assert_eq!(pi.net_edges.len(), 1, "edges: {:?}", pi.net_edges);
        let texts: Vec<String> = pi.net_edges[0]
            .attrs
            .iter()
            .map(|a| a.to_string())
            .collect();
        assert_eq!(texts, vec!["bridge = VDD_3V3, VDDA"], "edge attrs");
    }

    #[test]
    fn l1_decode_projects_refs_and_edges() {
        // refs → (name, role, @star); net @bridge attrs → typed (kind, endpoints).
        let pi = parse_pi(SRC);
        let refs = pi.l1_refs();
        let gnd = refs.iter().find(|r| r.name == "GND").expect("GND");
        assert_eq!(gnd.role.as_deref(), Some("main"));
        assert!(gnd.star, "GND carries @star");
        let gnda = refs.iter().find(|r| r.name == "GNDA").expect("GNDA");
        assert_eq!(gnda.role.as_deref(), Some("quiet"));
        assert!(!gnda.star);
        // refs without @role default to role None (→ main at the check layer)
        assert!(refs.iter().all(|r| r.role.is_some()));

        let pi2 = parse_pi(SRC_EDGES);
        let edges = pi2.l1_edges();
        assert_eq!(
            edges.len(),
            1,
            "only the net with trailing @bridge is an edge"
        );
        assert_eq!(edges[0].kind, L1EdgeKind::Bridge);
        assert_eq!(
            edges[0].endpoints,
            vec!["VDD_3V3".to_string(), "VDDA".to_string()]
        );
    }

    #[test]
    fn l1_rails_decode_dc_contracts() {
        let pi = parse_pi(SRC);
        let rails = pi.l1_rails();
        assert_eq!(rails.len(), 3, "rails: {rails:?}");
        let dvdd = rails
            .iter()
            .find(|r| r.hot == "VDD_3V3")
            .expect("VDD_3V3 rail");
        assert_eq!(dvdd.domain, "DVDD");
        assert_eq!(dvdd.v, Some(3.3));
        assert_eq!(dvdd.tol, Some(0.05), "±5% → 0.05");
        assert_eq!(dvdd.capacity_amps, Some(0.5), "500mA → 0.5A");
        assert_eq!(dvdd.eff, Some(0.95));
        assert!(dvdd.bad.is_none(), "DVDD: {:?}", dvdd.bad);

        let avdd = rails.iter().find(|r| r.hot == "VDDA").expect("VDDA rail");
        assert_eq!(avdd.tol, Some(0.02), "±2% → 0.02");
        assert_eq!(avdd.capacity_amps, Some(0.02), "20mA → 0.02A");
        assert_eq!(avdd.eff, None, "no eff declared");

        let iso = rails
            .iter()
            .find(|r| r.hot == "V5V_ISO")
            .expect("V5V_ISO rail");
        assert_eq!(iso.v, Some(5.0));
        assert_eq!(iso.tol, None, "bare ::DC(5V) has no window");
        assert!(iso.bad.is_none());
    }

    #[test]
    fn l1_rails_flag_bad_nominal_and_unknown_key() {
        // `5A` is a current, not a DC volts value → bad nominal. `req:` is a
        // sink-side window key that has no place on a source rail (register
        // discipline §5.2) → flagged.
        const BAD: &str = r#"module main {
    ref GND @role(main)
    domain DVDD @class(digital) { rail [VDD_3V3, GND]::DC(5A, req:±3%) }
}
"#;
        let pi = parse_pi(BAD);
        let rails = pi.l1_rails();

        assert_eq!(rails.len(), 1);
        let r = &rails[0];
        assert_eq!(r.v, None, "5A must not decode as volts");
        assert!(r.bad.is_some(), "expected a decode problem, got {rails:?}");
    }

    #[test]
    fn captures_domains_and_rail_pairs() {
        let pi = parse_pi(SRC);
        assert_eq!(pi.domains.len(), 3);
        let dvdd = pi
            .domains
            .iter()
            .find(|d| d.name == "DVDD")
            .expect("DVDD domain");
        assert!(
            dvdd.attrs
                .iter()
                .any(|a| a.to_string() == "class = digital"),
            "DVDD attrs"
        );
        assert_eq!(dvdd.rails.len(), 1);
        let rail = &dvdd.rails[0];
        assert_eq!(rail.hot, "VDD_3V3");
        assert_eq!(rail.ret, "GND");
        assert_eq!(rail.iface, "DC");
        assert_eq!(rail.params.len(), 4, "params: {:?}", rail.params);
        // first positional param = the volt
        assert!(rail.params[0].key.is_none());
        assert_eq!(rail.params[0].text, "3.3V");
        assert_eq!(rail.params[1].key.as_deref(), Some("tol"));
        assert_eq!(rail.params[1].text, "±5%");
        assert_eq!(rail.params[2].key.as_deref(), Some("capacity"));
        assert_eq!(rail.params[2].text, "500mA");
        assert_eq!(rail.params[3].key.as_deref(), Some("eff"));
        assert_eq!(rail.params[3].text, "0.95");

        // analog domain with fewer params + ISO domain with no attrs/lean rail
        let iso = pi.domains.iter().find(|d| d.name == "ISO").expect("ISO");
        assert!(iso.attrs.is_empty());
        assert_eq!(iso.rails[0].ret, "GND_ISO");
        assert_eq!(iso.rails[0].params.len(), 1);
    }

    // ── §5.2 direction-word family pin-DC decode (design §4.1 / §4.4) ───────

    /// Load `src` and return the captured pwr pin contracts (`McPins.pwr`) of
    /// the component whose name equals `want` — same parse harness as
    /// [`parse_pi`], over `workspace_components()` instead of modules.
    fn parse_pwr_pins(src: &str, want: &str) -> Vec<McPwrPin> {
        let _guard = MCC_TEST_PARSE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let root = crate::cli::datadir::data_root();
        crate::mcc_set_system_root(&root);
        crate::mcc_init();
        let uri: crate::McURI = "/mcc/pi-pwr-decode-test.mc".to_string();
        crate::mcc_load_from_string(&uri, src);
        crate::definition_space()
            .workspace_components()
            .into_iter()
            .find(|(_, c)| c.name.to_string() == want)
            .map(|(_, c)| c.pins.pwr.clone())
            .unwrap_or_else(|| panic!("component '{want}' not parsed"))
    }

    const PWR_SRC: &str = r#"component LDO.X {
    pins = [
        psnk [1,2]=[IN, GND]::DC(5V)
        psrc [3,4]=[OUT, GND]::DC(3.3V, tol:±5%, capacity:300mA, eff:0.95)
    ]
}
module main {
}
"#;

    /// A sink (`psnk`) requirement decodes its nominal — and only its nominal.
    /// The `::DC` of a requirement side never carries a source budget (§5.2).
    #[test]
    fn decode_psnk_sink_nominal() {
        let pins = parse_pwr_pins(PWR_SRC, "LDO.X");
        let sink = decode_pwr_pin(pins.iter().find(|p| p.dir == PwrDir::Snk).expect("psnk"));
        assert_eq!(sink.hot, "IN");
        assert_eq!(sink.v_text, "5V");
        assert_eq!(sink.v, Some(5.0));
        assert_eq!(sink.tol, None);
        assert_eq!(sink.capacity_amps, None);
        assert!(sink.bad.is_none(), "sink decode: {:?}", sink.bad);
    }

    /// §5.1 identity-axis carry: a trailing `@class(analog)` on a psnk pin row
    /// (component leaf — the exact hs analog-supply row shape
    /// `psnk [13,12]=[VDDA,VSSA]::DC(5V) @class(analog), "Analog power/ground"`,
    /// attributes plus a `, label` value tail) is captured into the structural
    /// [`McPwrPin`] and carried through [`decode_pwr_pin`] so the rule layer
    /// reads it — no silent drop.
    #[test]
    fn decode_psnk_class_attr_carries_identity() {
        const SRC: &str = r#"component MCU.M {
    pins = [
        psnk [13, 12] = [VDDA, VSSA]::DC(5V) @class(analog), "Analog power/ground"
    ]
}
module main {
}
"#;
        let pins = parse_pwr_pins(SRC, "MCU.M");
        assert_eq!(pins.len(), 1, "the @class row must capture one pwr pin");
        let cap = &pins[0];
        assert_eq!(cap.hot, "VDDA");
        assert_eq!(cap.ret.as_deref(), Some("VSSA"));
        assert_eq!(
            attr_texts(&cap.attrs, "class"),
            vec!["analog".to_string()],
            "structural capture must keep the @class identity word"
        );
        let out = decode_pwr_pin(cap);
        assert_eq!(out.v, Some(5.0), "nominal still decodes beside the attrs");
        assert_eq!(
            attr_texts(&out.attrs, "class"),
            vec!["analog".to_string()],
            "typed decode must carry the identity axis, got {:?}",
            out.attrs.iter().map(|a| a.to_string()).collect::<Vec<_>>()
        );
        assert!(out.bad.is_none(), "decode: {:?}", out.bad);
    }

    const SRC_PORTS: &str = r#"module main {
    out shield_to_earth @bind_role(earth)
    io  DP_OUT, DM_OUT  @exposed(esd_contact)
    io  MIC{P, N}       @class(analog) @return(GNDA)
    io  SEL                    // plain row, no identity word → not a capture
    conduit GND @role(main)
}
"#;

    #[test]
    fn captures_module_port_identity_rows() {
        // §5.1 identity rows on module-interface ports: only rows carrying a
        // trailing attribute are McPortDecls; the plain `io SEL` is not.
        let pi = parse_pi(SRC_PORTS);
        assert_eq!(pi.ports.len(), 3, "ports: {:#?}", pi.ports);

        let out = pi
            .ports
            .iter()
            .find(|p| p.kind == "out")
            .expect("the out row");
        assert_eq!(out.names, vec!["shield_to_earth".to_string()]);
        assert_eq!(
            attr_texts(&out.attrs, "bind_role"),
            vec!["earth".to_string()],
            "row attrs: {:?}",
            out.attrs
        );

        let io = pi.ports.iter().find(|p| p.kind == "io").expect("io rows");
        // DP_OUT, DM_OUT are separate operands → both carry the row's @exposed.
        assert_eq!(io.names, vec!["DP_OUT".to_string(), "DM_OUT".to_string()]);
        assert_eq!(
            attr_texts(&io.attrs, "exposed"),
            vec!["esd_contact".to_string()]
        );

        let mic = pi
            .ports
            .iter()
            .find(|p| p.names.iter().any(|n| n == "MIC.P"))
            .expect("the MIC bus row");
        assert_eq!(
            mic.names,
            vec!["MIC.P".to_string(), "MIC.N".to_string()],
            "bus-curly operand expands to its net-visible members"
        );
        assert_eq!(attr_texts(&mic.attrs, "class"), vec!["analog".to_string()]);
        assert_eq!(attr_texts(&mic.attrs, "return"), vec!["GNDA".to_string()]);
    }

    #[test]
    fn l1_ports_project_identity_axis_fields() {
        let pi = parse_pi(SRC_PORTS);
        let ports = pi.l1_ports();
        assert_eq!(ports.len(), 5, "per-member decode: {:#?}", ports);

        let earth = ports
            .iter()
            .find(|p| p.name == "shield_to_earth")
            .expect("out member");
        assert_eq!(earth.kind, "out");
        assert_eq!(earth.bind_role.as_deref(), Some("earth"));
        assert!(earth.exposed.is_empty());

        let mic_p = ports.iter().find(|p| p.name == "MIC.P").expect("MIC.P");
        assert_eq!(mic_p.class.as_deref(), Some("analog"));
        assert_eq!(
            mic_p.ret.as_deref(),
            Some("GNDA"),
            "@return → conduit anchor"
        );
        assert_eq!(mic_p.bind_role, None);

        let dp = ports.iter().find(|p| p.name == "DP_OUT").expect("DP_OUT");
        assert_eq!(dp.exposed, vec!["esd_contact".to_string()]);
    }

    /// A source (`psrc`) pin mirrors the domain-rail decode: nominal + the
    /// source-exclusive budget keys decode to typed values.
    #[test]
    fn decode_psrc_source_budget() {
        let pins = parse_pwr_pins(PWR_SRC, "LDO.X");
        let src = decode_pwr_pin(pins.iter().find(|p| p.dir == PwrDir::Src).expect("psrc"));
        assert_eq!(src.hot, "OUT");
        assert_eq!(src.v, Some(3.3));
        assert_eq!(src.tol, Some(0.05), "±5% → 0.05");
        assert_eq!(src.capacity_amps, Some(0.3), "300mA → 0.3A");
        assert_eq!(src.eff, Some(0.95));
        assert!(src.bad.is_none(), "source decode: {:?}", src.bad);
    }

    /// A sink that tries to hang source-exclusive / spec-belonging keys on its
    /// `::DC` is flagged at decode time — tol/capacity/eff are source-exclusive (PWR-4),
    /// req/abs belong in the component `spec` (§4.4 write-site rule).
    #[test]
    fn decode_psnk_with_source_key_flags() {
        const BAD: &str = r#"component WRONG {
    pins = [
        psnk [1,2]=[IN, GND]::DC(3.3V, tol:±5%, req:±3%)
    ]
}
module main {
}
"#;
        let pins = parse_pwr_pins(BAD, "WRONG");
        let sink = decode_pwr_pin(&pins[0]);
        assert_eq!(sink.v, Some(3.3), "the nominal still decodes");
        let bad = sink.bad.expect("tol on a sink must be flagged");
        assert!(
            bad.contains("source-exclusive"),
            "expected a source-exclusive-key flag, got: {bad}"
        );
    }

    /// rail-contract-design.md §8.1: `amp` is the sink-exclusive demand key —
    /// a `psnk` may carry its instance's DC current draw after its nominal.
    #[test]
    fn decode_psnk_amp_demand() {
        const PWR_SINK_AMP: &str = r#"component SINK.A {
    pins = [
        psnk [1,2]=[VDD, GND]::DC(3.3V, amp:40mA)
    ]
}
module main {
}
"#;
        let pins = parse_pwr_pins(PWR_SINK_AMP, "SINK.A");
        let sink = decode_pwr_pin(&pins[0]);
        assert_eq!(sink.hot, "VDD");
        assert_eq!(sink.v, Some(3.3));
        assert_eq!(sink.amp, Some(0.04), "40mA → 0.04A");
        assert!(sink.bad.is_none(), "sink amp decode: {:?}", sink.bad);
    }

    /// `amp` on a source row is off-register: a source declares what it can
    /// supply (`capacity`); its own input draw is derived (§7.2), not a net load.
    #[test]
    fn decode_amp_on_source_flags() {
        const SRC_AMP: &str = r#"component SRC.A {
    pins = [
        psrc [1,2]=[OUT, GND]::DC(5V, amp:100mA)
    ]
}
module main {
}
"#;
        let pins = parse_pwr_pins(SRC_AMP, "SRC.A");
        let src = decode_pwr_pin(&pins[0]);
        assert!(
            src.capacity_amps.is_none(),
            "amp must not decode as capacity"
        );
        let bad = src.bad.expect("amp on a source must be flagged");
        assert!(
            bad.contains("sink-exclusive"),
            "expected a sink-exclusive demand-key flag, got: {bad}"
        );
    }

    /// A non-current `amp` value on a sink is a decode failure (6012 reports it).
    #[test]
    fn decode_bad_amp_on_sink_flags() {
        const BAD_AMP: &str = r#"component SINK.B {
    pins = [
        psnk [1,2]=[VDD, GND]::DC(3.3V, amp:5V)
    ]
}
module main {
}
"#;
        let pins = parse_pwr_pins(BAD_AMP, "SINK.B");
        let sink = decode_pwr_pin(&pins[0]);
        let bad = sink
            .bad
            .expect("a non-current amp on a sink must be flagged");
        assert!(
            bad.contains("not a DC current"),
            "expected a bad-amp flag, got: {bad}"
        );
    }

    /// psbi = conditional source: while discharging it is a source, so the
    /// source budget keys decode on it (battery capacity feeds PWR-4).
    #[test]
    fn decode_psbi_conditional_source_budget() {
        const BAT: &str = r#"component PWR.BAT {
    pins = [
        psbi [1,2]=[BAT, GND]::DC(3.7V, capacity:1A)
    ]
}
module main {
}
"#;
        let pins = parse_pwr_pins(BAT, "PWR.BAT");
        let bi = decode_pwr_pin(&pins[0]);
        assert_eq!(bi.dir, PwrDir::Bi);
        assert_eq!(bi.v, Some(3.7));
        assert_eq!(bi.capacity_amps, Some(1.0));
        assert!(bi.bad.is_none(), "psbi decode: {:?}", bi.bad);
    }

    /// A missing nominal on a sink is the one hard violation the decode owns:
    /// §4.4: a sink's nominal is mandatory. (Pure structural construction — no grammar dependence.)
    #[test]
    fn decode_sink_without_nominal_is_mandatory_violation() {
        let pin = McPwrPin {
            dir: PwrDir::Snk,
            iface: "DC".to_string(),
            hot: "IN".to_string(),
            ret: Some("GND".to_string()),
            params: Vec::new(),
            span: 0..1,
            attrs: McAttributes::new(),
        };
        let out = decode_pwr_pin(&pin);
        assert_eq!(out.v, None);
        let bad = out.bad.expect("a sink must carry a nominal");
        assert!(
            bad.contains("mandatory"),
            "expected the mandatory-nominal message, got: {bad}"
        );
    }

    /// The member-bus spelling `VIN{Vin, GND}` (golden LDO/DCDC/BAT rows) is
    /// captured too — hot/ret carry the *dotted* `bus.member` names pin
    /// registration and the flat class_name use, so E-PWR-001's sink lookup
    /// matches on member-bus components exactly as on bracket rows.
    #[test]
    fn decode_member_bus_spelling_captures_dotted_hot() {
        const LDO: &str = r#"component LDO.Y {
    pins = [
        psnk [1,2] = VIN{Vin, GND}::DC(5V)
        psrc [3,2] = VOUT{Vout, GND}::DC(3.3V, tol:±5%, capacity:300mA, eff:0.95)
    ]
}
module main {
}
"#;
        let pins = parse_pwr_pins(LDO, "LDO.Y");
        assert_eq!(pins.len(), 2, "both member-bus rows captured: {pins:?}");
        let sink = decode_pwr_pin(pins.iter().find(|p| p.dir == PwrDir::Snk).expect("psnk"));
        assert_eq!(
            sink.hot, "VIN.Vin",
            "dotted group.member hot matches flat class"
        );
        assert_eq!(sink.ret.as_deref(), Some("VIN.GND"));
        assert_eq!(sink.v, Some(5.0));
        assert!(sink.bad.is_none(), "member-bus sink decode: {:?}", sink.bad);
        let src = decode_pwr_pin(pins.iter().find(|p| p.dir == PwrDir::Src).expect("psrc"));
        assert_eq!(src.hot, "VOUT.Vout");
        assert_eq!(src.v, Some(3.3));
        assert_eq!(src.tol, Some(0.05));
    }
}
