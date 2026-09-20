// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Power-intent declarations — typed capture (intent-design.md §5).
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
//! intent-design.md §3/§6 iron rule 2) trail the **connection** net
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
use crate::eval;
use crate::semantic::basic::attr_keys;
use crate::semantic::basic::mc_uval::McUnit;
use crate::semantic::component::mc_attr::{McAttrVal, McAttribute, McAttributes};
use crate::semantic::component::mc_pins::{McPwrPin, PwrDir, PwrParam};
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
    /// Module **power-output port rows** — `psrc/psnk/psbi NAME{hot,ret}::DC(…)`
    /// declared in the module body (a bounded/exported supply face such as
    /// `psrc vin{VBUS_5V,GND}::DC(5V, capacity:…)`). The flatten layer records
    /// only the written DC pair on the resulting `PortInst`; the full contract
    /// (capacity / eff budget params) is captured here so the PWR-4 budget axis
    /// can treat the exported supply face as an explicit capacity root
    /// (rail-contract-design.md §8.5). Deliberately a separate vector from
    /// [`McPowerDecls::ports`] — `l1_ports()` is the stable JSON identity view
    /// and must not see power rows.
    pub pwr_ports: Vec<McPortPwr>,

    /// Written `[hot, ret]` pair of every module-body port row carrying a
    /// `::DC(…)` contract, whatever its direction word (`in [VDD_3V3, GND]::DC(3.3V)`
    /// as well as `psnk …`). [`Self::pwr_ports`] holds the same rows only for the
    /// `psrc/psnk/psbi` budget face; this one exists for the **identity**
    /// question — "did this scope declare this name?" — which has to hold for
    /// every spelling of a DC port row, since a bare reference to such a member
    /// is a declared reference and not a dangling label.
    pub dc_port_pairs: Vec<(String, Option<String>)>,
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

    /// The pure AST read behind [`Self::parse_domain`] — the same decode without
    /// the mutation, so Pass1's whole-reference peek can learn a module's
    /// domains before the body walk has reached their clauses
    /// (intent-reference-layer-design.md §10.11.4 guard ③). A peeked list is
    /// read through [`domain_pairs_of`], never pushed into `self.domains`.
    pub fn peek_domain(node: &AstNode) -> Option<McDomainDecl> {
        // The peek runs beside the same clauses the real `parse_domain` walk
        // will parse, so the declaration-side attr diagnostics fire exactly
        // once — there; the peek's own decode stays quiet.
        McDomainDecl::from_node_quiet(node)
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

    /// Capture a module power-output port row (`psrc/psnk/psbi NAME{hot,ret}
    /// ::DC(…)`) — the §8.5 budget-face capture complementing [`Self::parse_port`]
    /// (identity rows). The flatten layer keeps only the written pair on the
    /// `PortInst`; this keeps the full `::DC` contract (capacity / eff).
    pub fn parse_port_pwr(&mut self, node: &AstNode) {
        // Identity face: every `::DC` port row, any direction word.
        if let Some(head) = node.get_sub_node() {
            if let Some(pair) = dc_port_members(&head) {
                self.dc_port_pairs.push(pair);
            }
        }
        // Budget face: the `psrc/psnk/psbi` rows, with their ctor params.
        if let Some(p) = McPortPwr::from_node(node) {
            self.pwr_ports.push(p);
        }
    }

    /// Project the module's declared power-output (Src/Bi) port rows into typed
    /// supply contracts — `(hot member, decoded)`, hot member verbatim (the
    /// label the flat Port point's path tail carries). Sink (`psnk`) port rows
    /// are captured structurally but are demand, never a budget source, so they
    /// are excluded here (rail-contract-design.md §8.1: a source declares
    /// capacity; its own input draw is derived).
    pub fn l1_port_sources(&self) -> Vec<(String, L1PwrPin)> {
        let mut out = Vec::new();
        for p in &self.pwr_ports {
            if p.dir == PwrDir::Snk {
                continue;
            }
            out.push((p.hot.clone(), decode_port_pwr(p)));
        }
        out
    }

    /// Decode the module's `conduit` declarations into the L1 identity view
    /// the FlatErc checks consume (design §3.2): name + `@role` value + `@star`
    /// flag. A conduit with no `@role` defaults to `main` (default per §5.2).
    pub fn l1_refs(&self) -> Vec<L1Ref> {
        self.refs
            .iter()
            .map(|r| L1Ref {
                name: r.name.clone(),
                role: attr_texts(&r.attrs, attr_keys::KEY_ROLE).into_iter().next(),
                star: has_attr(&r.attrs, attr_keys::KEY_STAR),
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
    /// an AC rail is [`Self::l1_ac_rails`]'s list, so the two readers partition
    /// the rail rows and the DC consumers keep seeing exactly DC. A rail whose
    /// ctor args fail to decode still lists with `bad: Some(..)` so the
    /// two-root check stays independent of value decode.
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

    /// Decode every AC `rail [L, N]::AC(v, f)` guarantee into a typed L1 contract
    /// (§3.2/§3.3) — the AC axis's projection beside [`Self::l1_rails`]. Only
    /// rows whose iface is a registered `::AC*` variant are AC rails; any other
    /// iface belongs to neither reader. A row whose ctor args fail to decode
    /// still lists with `bad: Some(..)`, like the DC side.
    pub fn l1_ac_rails(&self) -> Vec<L1AcRail> {
        let mut out = Vec::new();
        for d in &self.domains {
            for r in &d.rails {
                if ac_terminal_group(&r.iface).is_some() {
                    out.push(decode_ac_rail(&d.name, r));
                }
            }
        }
        out
    }

    /// Decode every domain's `@nature(ac|dc)` word beside the iface of each rail
    /// row it declares (§3.1) — the AC/DC consistency rule's read. Both sides
    /// are carried exactly as written; mapping them onto an axis is the rule's
    /// step ([`RailAxis`]), not this projection's. A domain with no `nature`
    /// word still lists (`nature: None`): its rail contracts stand alone.
    pub fn l1_domain_natures(&self) -> Vec<L1DomainNature> {
        self.domains
            .iter()
            .map(|d| L1DomainNature {
                name: d.name.clone(),
                nature: first_text(&d.attrs, attr_keys::KEY_NATURE),
                rails: d
                    .rails
                    .iter()
                    .map(|r| L1RailAxisRow {
                        iface: r.iface.clone(),
                        hot: r.hot.clone(),
                        span: r.span.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Decode every domain's identity-axis words — §1.1's domain-face
    /// projection, read from the same `self.domains` as [`Self::l1_domain_natures`]
    /// and shaped like it. A domain writing no word still lists (`None`): the
    /// §1.4 quiet/sensitive face is then simply not claimed by it.
    pub fn l1_domain_faces(&self) -> Vec<L1DomainFace> {
        self.domains
            .iter()
            .map(|d| L1DomainFace {
                name: d.name.clone(),
                class: first_text(&d.attrs, attr_keys::KEY_CLASS),
                noise: first_text(&d.attrs, attr_keys::KEY_NOISE),
                nature: first_text(&d.attrs, attr_keys::KEY_NATURE),
            })
            .collect()
    }

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
                    class: first_text(&p.attrs, attr_keys::KEY_CLASS),
                    nature: first_text(&p.attrs, attr_keys::KEY_NATURE),
                    noise: first_text(&p.attrs, attr_keys::KEY_NOISE),
                    ret: first_text(&p.attrs, attr_keys::KEY_RETURN),
                    exposed: attr_texts(&p.attrs, attr_keys::KEY_EXPOSED),
                    bind_role: first_text(&p.attrs, attr_keys::KEY_BIND_ROLE),
                    span: p.span.clone(),
                });
            }
        }
        out
    }

    /// Every whole-referenceable domain — the R1 reference rule's data face
    /// (intent-reference-layer §10.2 D1). A domain lists iff it declares
    /// **exactly one** `::DC` rail row: that alone determines a pair, so a bare
    /// domain name in a pair position has one answer. Zero DC rows (a pure AC
    /// domain, or an empty one) and two or more both list nothing — the rule
    /// then leaves the written name alone instead of guessing which rail was
    /// meant. Shaped beside [`Self::l1_domain_natures`] /
    /// [`Self::l1_domain_faces`], read from the same `self.domains`; that the
    /// name resolves in the *owning* module's scope is the rule's step, not
    /// this projection's.
    pub fn l1_domain_pairs(&self) -> Vec<L1DomainPair> {
        domain_pairs_of(&self.domains)
    }

    /// Project the module's **domain-level** bridges (design §10.4, R3): one row
    /// per `@bridge` attribute whose two arguments both name whole-referenceable
    /// domains ([`domain_bridge_of`]). Mixed arguments (one domain, one plain
    /// endpoint) and net-level pairs stay out — they belong to 6046 and to the
    /// unchanged net-level [`Self::l1_edges`] respectively. Ruling E (§10.4)
    /// keeps this a separate table: [`L1Edge`] and its eleven consumers are
    /// untouched.
    pub fn l1_domain_edges(&self) -> Vec<L1DomainEdge> {
        let mut out = Vec::new();
        for e in &self.net_edges {
            for a in e.attrs.iter() {
                if a.id.to_string().as_str() != "bridge" {
                    continue;
                }
                if let Some((pa, pb)) =
                    domain_bridge_of(&value_texts(a), &domain_pairs_of(&self.domains))
                {
                    out.push(L1DomainEdge {
                        a: pa.domain,
                        b: pb.domain,
                        span: e.span.clone(),
                    });
                }
            }
        }
        out
    }
}

/// [`McPowerDecls::l1_domain_pairs`] over a bare domain list — the same
/// projection, callable on a list that was *not* produced by `parse_domain`.
/// Pass1's whole-reference peek needs exactly that: the owning module answers a
/// bare domain name's meaning before its body walk has reached the `domain`
/// clause, and re-running `parse_domain` would re-enter a mutating path the peek
/// must not touch (§10.11.4 guard ③).
pub fn domain_pairs_of(domains: &[McDomainDecl]) -> Vec<L1DomainPair> {
    let mut out = Vec::new();
    for d in domains {
        let mut dc = d.rails.iter().filter(|r| r.iface == "DC");
        let Some(row) = dc.next() else { continue };
        // Two DC rails state two pairs: naming the domain would be a guess.
        if dc.next().is_some() {
            continue;
        }
        out.push(L1DomainPair {
            domain: d.name.clone(),
            hot: row.hot.clone(),
            ret: row.ret.clone(),
            span: row.span.clone(),
        });
    }
    out
}

/// One domain-level bridge — a `@bridge(X, Y)` whose **both** arguments name
/// whole-referenceable domains of the owning module (intent-reference-layer
/// design §10.4, R3; ruling E takes a separate table over upgrading
/// [`L1Edge`]'s net-string endpoints, so every net-level consumer keeps its
/// exact reading). Identity is the two domain names as written; the witnessed
/// legs are the licensed chains' own face and live with the Pass1 scan
/// (`McModule::scan_domain_bridges`), not in this store row.
#[derive(Debug, Clone)]
pub struct L1DomainEdge {
    pub a: String,
    pub b: String,
    /// The statement clause's span — the same indexing the net-level
    /// `DeclEdge` spans use, so a leg's wiring site can be tested against it.
    pub span: Span,
}

/// The R3 license classifier over one `@bridge` attribute's endpoint texts:
/// `Some((pair_a, pair_b))` iff exactly two texts, each naming a
/// whole-referenceable domain of `pairs` ([`domain_pairs_of`]'s output, so a
/// zero-rail / multi-rail / non-DC domain never licenses), any other shape —
/// net-level bridge, mixed pair (6046's object, judged by the caller), wrong
/// arity — is `None`. Argument order is preserved: the caller checks it
/// against the chain's written order (6047).
pub fn domain_bridge_of(
    texts: &[String],
    pairs: &[L1DomainPair],
) -> Option<(L1DomainPair, L1DomainPair)> {
    if texts.len() != 2 {
        return None;
    }
    let a = pairs.iter().find(|p| p.domain == texts[0])?.clone();
    let b = pairs.iter().find(|p| p.domain == texts[1])?.clone();
    Some((a, b))
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
                match eval::quantity_in(&p.text, &McUnit::Volt) {
                    Some(x) => out.v = Some(x),
                    None => flag_bad(
                        &mut out.bad,
                        format!("nominal '{}' is not a DC volts value", p.text),
                    ),
                }
            }
            Some("tol") => match eval::percent_of(&p.text).map(f64::abs) {
                Some(x) => out.tol = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("tol '{}' is not a ±percent window", p.text),
                ),
            },
            Some("capacity") => match eval::quantity_in(&p.text, &McUnit::Amp) {
                Some(x) => out.capacity_amps = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("capacity '{}' is not a DC current", p.text),
                ),
            },
            Some("eff") => match eval::ratio_of(&p.text) {
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

// §3.2/§3.3 AC axis — the `::AC*` terminal groups and their typed rail read

/// One terminal of an `::AC*` variant's group. The role is the variant's, read
/// off the position in the group the registry declares — never off the spelling
/// of the net name the row happens to write there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcTerminal {
    /// `L` / `U` — a phase conductor.
    Phase,
    /// `N` — the neutral: the member the pair's return runs on (§3.3, model A).
    Neutral,
    /// `PE` — the protective conductor. Not a return: it belongs to the
    /// protective-earth identity net (§3.6).
    Protective,
}

/// The `::AC*` variants and the terminal group each one declares, in the order
/// it declares them (ac-axis-interface-design.md §3.2). The table is the AC
/// axis's identity anchor: an iface name absent from it is not an AC contract,
/// and a row's written members take their roles by position — the shape comes
/// from the variant, never from counting or reading the names.
const AC_VARIANTS: &[(&str, &[AcTerminal])] = &[
    ("AC", &[AcTerminal::Phase, AcTerminal::Neutral]),
    (
        "AC_1P3W",
        &[
            AcTerminal::Phase,
            AcTerminal::Neutral,
            AcTerminal::Protective,
        ],
    ),
    (
        "AC_3P3W",
        &[AcTerminal::Phase, AcTerminal::Phase, AcTerminal::Phase],
    ),
    (
        "AC_3P4W",
        &[
            AcTerminal::Phase,
            AcTerminal::Phase,
            AcTerminal::Phase,
            AcTerminal::Neutral,
        ],
    ),
    (
        "AC_3P5W",
        &[
            AcTerminal::Phase,
            AcTerminal::Phase,
            AcTerminal::Phase,
            AcTerminal::Neutral,
            AcTerminal::Protective,
        ],
    ),
];

/// The terminal group an `::AC*` variant declares, or `None` for any iface the
/// registry does not hold (`DC` included) — the AC reader's one test.
pub fn ac_terminal_group(iface: &str) -> Option<&'static [AcTerminal]> {
    AC_VARIANTS
        .iter()
        .find(|(name, _)| *name == iface)
        .map(|(_, group)| *group)
}

/// The supply axis a domain's `@nature` word and a rail row's `::` contract both
/// name (§3.1). The two sides are written in different vocabularies (a lowercase
/// value word / a `::` iface name), so a comparison has to bring them onto one
/// axis first — there is no spelling shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailAxis {
    Ac,
    Dc,
}

impl RailAxis {
    /// The axis a `@nature` value word names. §5.2 registers exactly `ac` and
    /// `dc`, which the registry's `nature` row holds; any other word is outside
    /// the closed vocabulary and names no axis, so the rule has no verdict for
    /// it — the declaration itself is reported where it is written (5360).
    pub fn of_nature_word(word: &str) -> Option<Self> {
        match word {
            attr_keys::WORD_AC => Some(Self::Ac),
            attr_keys::WORD_DC => Some(Self::Dc),
            _ => None,
        }
    }

    /// The axis a rail row's iface names: `::DC`, or one of the registered
    /// `::AC*` variants (§3.2). An iface that names neither — a bare row, a
    /// foreign contract — names no axis.
    pub fn of_iface(iface: &str) -> Option<Self> {
        if iface == "DC" {
            Some(Self::Dc)
        } else if ac_terminal_group(iface).is_some() {
            Some(Self::Ac)
        } else {
            None
        }
    }
}

/// One domain's `@nature` word beside every rail row it declares (§3.1): the
/// declaration-local pair the AC/DC consistency rule reads. `nature` is the
/// written word verbatim (`None` when the domain writes none, which leaves its
/// rail contracts as the only statement of the axis); each rail carries its own
/// span so a contradicting row is reported on itself.
#[derive(Debug, Clone)]
pub struct L1DomainNature {
    pub name: String,
    pub nature: Option<String>,
    pub rails: Vec<L1RailAxisRow>,
}

/// One domain's identity-axis words (§1.1) — the face a rule reads to decide
/// whether a domain is a **quiet / sensitive** one (§1.4: `@class(analog)`, or
/// `@noise(quiet)` / `@noise(sensitive)`). Words are carried exactly as written,
/// beside [`L1DomainNature`] and [`L1Port`]; which of them makes a face quiet is
/// the rule's step, and their value legality belongs to the §5.2 registry, not
/// to this projection.
#[derive(Debug, Clone)]
pub struct L1DomainFace {
    pub name: String,
    pub class: Option<String>,
    pub noise: Option<String>,
    pub nature: Option<String>,
}

/// One rail row as the §3.1 consistency rule reads it: the `::` contract it
/// writes, the net its first member opens (the row's subject, like [`L1Rail`]'s
/// hot), and the row's own span (the rule's anchor).
#[derive(Debug, Clone)]
pub struct L1RailAxisRow {
    pub iface: String,
    pub hot: String,
    pub span: Span,
}

/// One **whole-referenceable** domain — a domain whose name, written bare in a
/// position that expects a DC pair, denotes its declared `[hot, ret]` rail
/// (intent-reference-layer §10.2 D1, the R1 reference rule's data face).
///
/// The predicate is *exactly one* `::DC` row, and nothing else: a domain with
/// no DC rail (pure AC, or empty) states no pair to stand for, and one with two
/// or more DC rails states no *single* pair — naming either of those would be a
/// guess, so neither lists. That is why this is a filtered projection rather
/// than a lookup that can fail: absence from the list is the predicate.
///
/// Scope is the rule's step, not this projection's: a consumer resolves the
/// name in the **owning module's** own `domains`, never up or down the instance
/// tree.
#[derive(Debug, Clone)]
pub struct L1DomainPair {
    pub domain: String,
    /// The row's first member — the pair's hot net (`McRailDecl::hot`).
    pub hot: String,
    /// The row's second member — the pair's return net (`McRailDecl::ret`).
    pub ret: String,
    /// The row's own span (the rule's anchor).
    pub span: Span,
}

/// One decoded AC rail guarantee — the AC axis's typed rail read, beside the
/// DC-only [`L1Rail`]. `v_rms` is `None` only when the written nominal does not
/// decode to a volts value; that failure is kept as `bad` (reported by the
/// rail-contract check) rather than silently dropped. No window is derived: an
/// AC tolerance (grid swing) is a power-quality question for sim, so the
/// schematic-ERC face reads the RMS nominal and the frequency (§3.3).
#[derive(Debug, Clone)]
pub struct L1AcRail {
    pub domain: String,
    /// The `::AC*` variant — the identity of the terminal group below.
    pub variant: String,
    /// Written member net names in declaration order (`[L, N]` → `["L", "N"]`).
    pub members: Vec<String>,
    /// The leading member — the phase the row opens with.
    pub hot: String,
    /// The member the variant's group puts in the neutral slot, when the shape
    /// has one and the row writes it. A delta three-wire shape has none: its
    /// return runs phase-to-phase, which this read leaves blank (§8 boundary 2).
    pub ret: Option<String>,
    /// Verbatim nominal text as written (`230V`) — for messages.
    pub v_text: String,
    /// RMS volts; `None` when `v_text` is not a volts value.
    pub v_rms: Option<f64>,
    /// Line frequency in Hz (`50Hz`); `None` when the row writes none.
    pub f: Option<f64>,
    /// First decode problem, if any (a member count the variant's group does not
    /// declare, a non-volts RMS, a non-hertz frequency, an unknown key).
    pub bad: Option<String>,
    pub span: Span,
}

/// Decode one rail row whose iface is a registered `::AC*` variant: the two
/// scalars are positional (`::AC(230V, 50Hz)` — RMS volts, then hertz), and the
/// written members must match the variant's declared group size.
fn decode_ac_rail(domain: &str, r: &McRailDecl) -> L1AcRail {
    let group = ac_terminal_group(&r.iface).unwrap_or(&[]);
    let mut out = L1AcRail {
        domain: domain.to_string(),
        variant: r.iface.clone(),
        members: r.members.clone(),
        hot: r.members.first().cloned().unwrap_or_default(),
        ret: group
            .iter()
            .position(|t| *t == AcTerminal::Neutral)
            .and_then(|i| r.members.get(i).cloned()),
        v_text: String::new(),
        v_rms: None,
        f: None,
        bad: None,
        span: r.span.clone(),
    };
    if r.members.len() != group.len() {
        flag_bad(
            &mut out.bad,
            format!(
                "iface '{}' declares a {}-member terminal group but the row writes {}",
                r.iface,
                group.len(),
                r.members.len()
            ),
        );
    }
    let mut positional = 0usize;
    for p in &r.params {
        match p.key.as_deref() {
            None => {
                positional += 1;
                match positional {
                    1 => {
                        out.v_text = p.text.clone();
                        match eval::quantity_in(&p.text, &McUnit::Volt) {
                            Some(x) => out.v_rms = Some(x),
                            None => flag_bad(
                                &mut out.bad,
                                format!("RMS nominal '{}' is not a volts value", p.text),
                            ),
                        }
                    }
                    2 => match eval::quantity_in(&p.text, &McUnit::Hz) {
                        Some(x) => out.f = Some(x),
                        None => flag_bad(
                            &mut out.bad,
                            format!("frequency '{}' is not a hertz value", p.text),
                        ),
                    },
                    _ => flag_bad(
                        &mut out.bad,
                        format!(
                            "the AC contract carries two scalars (RMS volts, hertz); '{}' is a third",
                            p.text
                        ),
                    ),
                }
            }
            Some(k) => flag_bad(
                &mut out.bad,
                format!("unknown AC rail contract parameter '{k}'"),
            ),
        }
    }
    if positional == 0 {
        flag_bad(&mut out.bad, "missing nominal RMS 'v'".to_string());
    }
    out
}

// §5.2 direction-word family — component `psrc/psnk/psbi` pin `::DC(...)` decode

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
                match eval::quantity_in(&p.text, &McUnit::Volt) {
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
            Some("amp") => match eval::quantity_in(&p.text, &McUnit::Amp) {
                Some(x) => out.amp = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("amp '{}' is not a DC current", p.text),
                ),
            },
            Some("tol") => match eval::percent_of(&p.text).map(f64::abs) {
                Some(x) => out.tol = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("tol '{}' is not a ±percent window", p.text),
                ),
            },
            Some("capacity") => match eval::quantity_in(&p.text, &McUnit::Amp) {
                Some(x) => out.capacity_amps = Some(x),
                None => flag_bad(
                    &mut out.bad,
                    format!("capacity '{}' is not a DC current", p.text),
                ),
            },
            Some("eff") => match eval::ratio_of(&p.text) {
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

/// Text values of every attribute whose key is `key` (`role`, …). One reader
/// for both projections of a declaration row: the L1 identity face below and
/// the flat pin-row carry (`instant::insttab::exposed_of_pin`), so a key's
/// spelling has exactly one decoder.
pub(crate) fn attr_texts(attrs: &McAttributes, key: &str) -> Vec<String> {
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
pub(crate) fn value_texts(attr: &McAttribute) -> Vec<String> {
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

/// One module power-output port row — `psrc NAME{hot, ret}::DC(…)` /
/// `psbi …` / `psnk …` declared in the module body (design §5.2 direction
/// family, rail-contract-design.md §8.5 budget face). Structurally mirrors the
/// component [`McPwrPin`] (an `McPwrPin`-shaped slice is enough to reuse
/// [`decode_pwr_pin`]): direction keyword, written hot/ret member labels
/// verbatim (the flat `PortInst.dc_pair` spelling — bus prefix dropped), and the
/// `::DC` ctor params held as [`McRailParam`] text for typed decode.
#[derive(Debug, Clone)]
pub struct McPortPwr {
    pub dir: PwrDir,
    /// Written hot member label (`VBUS_5V`), verbatim.
    pub hot: String,
    /// Written return member label (`GND`), when the row declares a pair.
    pub ret: Option<String>,
    pub params: Vec<McRailParam>,
    pub span: Span,
}

impl McPortPwr {
    fn from_node(node: &AstNode) -> Option<Self> {
        // MCAST_NET_PORTS.sub = [ IOTYPE keyword carrier,
        //                         (MCAST_DECLARE ::<iface>(params))*,
        //                         (INSTANCE name side {hot,ret} | [hot,ret])* ]
        let head = node.get_sub_node()?;
        // Direction keyword: carrier sub type 97/98/99 (psrc/psnk/psbi).
        let dir = head
            .iter()
            .find(|c| c.get_type() == MCAST_IOTYPE)?
            .get_sub_node()
            .and_then(|sub| match sub.get_type() {
                MCAST_IOTYPE_PSRC => Some(PwrDir::Src),
                MCAST_IOTYPE_PSNK => Some(PwrDir::Snk),
                MCAST_IOTYPE_PSBI => Some(PwrDir::Bi),
                _ => None,
            })?;

        // The `::DC(params)` contract (only the DC axis decodes here — AC/nature
        // port contracts belong to the later AC-axis step, as with component pins).
        let (iface, params) = dc_declare(&head)?;
        if iface != "DC" {
            return None;
        }

        let (hot, ret) = dc_port_members(&head)?;

        Some(Self {
            dir,
            hot,
            ret,
            params,
            span: clause_span(node),
        })
    }
}

/// The `::DC` tail of a port row's declare, decoded as `(iface, ctor params)`.
fn dc_declare(head: &AstNode) -> Option<(String, Vec<McRailParam>)> {
    let declare = head.iter().find(|c| c.get_type() == MCAST_DECLARE)?;
    let mut iface = String::new();
    let mut params = Vec::new();
    if let Some(class) = child_of_type(&declare, MCAST_CLASS) {
        if let Some(ch) = class.get_sub_node() {
            for c in ch.iter() {
                if c.is_type(MCAST_IDS) {
                    iface = id_text(&c)?;
                } else if c.is_type(MCAST_PARAMS) {
                    params = read_params(&c);
                }
            }
        }
    }
    Some((iface, params))
}

/// The written `[hot, ret]` pair of a module port row that carries a `::DC(…)`
/// contract — first member is the supply face, the second the return, in source
/// order (the same positional rule the flat layer's `dc_pair` uses). `None` for
/// a row with no declare, a non-`DC` iface, or no written member.
///
/// Independent of the row's direction word on purpose. `McPortPwr` above is the
/// **budget face** (`psrc/psnk/psbi` only); this is the **identity face**: it
/// answers "which names does this scope declare as DC members?" for the rows the
/// corpus actually writes in a module body —
/// `in [VDD_3V3, GND]::DC(3.3V)` declares both names just as
/// `psnk [VDD_3V3, GND]::DC(3.3V)` does.
fn dc_port_members(head: &AstNode) -> Option<(String, Option<String>)> {
    let (iface, _) = dc_declare(head)?;
    if iface != "DC" {
        return None;
    }
    let mut members = Vec::new();
    for c in head.iter() {
        collect_member_labels(&c, &mut members);
        if members.len() >= 2 {
            break;
        }
    }
    let hot = members.first()?.clone();
    let ret = members.get(1).cloned();
    Some((hot, ret))
}

/// Collect written member labels from the first curly-bus / square-vector group
/// reachable under `node` (bounded descent past OPD / INSTANCE wrappers).
/// Curly members (`{VBUS_5V, GND}`) are read verbatim; square operands
/// (`[A, B]`) via [`net_name`]. A row whose name side is a scalar (no written
/// group) yields nothing.
fn collect_member_labels(node: &AstNode, out: &mut Vec<String>) {
    if out.len() >= 2 {
        return;
    }
    let Some(first) = node.get_sub_node() else {
        return;
    };
    for c in first.iter() {
        match c.get_type() {
            MCAST_OPD_CURLY => {
                if let Some(m0) = c.get_sub_node() {
                    for m in m0.iter() {
                        if let Some(t) = leaf_text(&m) {
                            out.push(t);
                        }
                    }
                }
            }
            MCAST_OPD_SQUARE_VEC => {
                if let Some(m0) = c.get_sub_node() {
                    for m in m0.iter() {
                        if let Some(t) = net_name(&m) {
                            out.push(t);
                        }
                    }
                }
            }
            _ => collect_member_labels(&c, out),
        }
        if out.len() >= 2 {
            return;
        }
    }
}

/// Decode one captured [`McPortPwr`] through the shared pin-contract reader by
/// bridging it onto a temporary [`McPwrPin`] (same value language as rails —
/// [`decode_pwr_pin`] already owns `tol`/`capacity`/`eff`/`amp` discipline).
fn decode_port_pwr(p: &McPortPwr) -> L1PwrPin {
    let pin = McPwrPin {
        dir: p.dir,
        iface: "DC".to_string(),
        hot: p.hot.clone(),
        ret: p.ret.clone(),
        params: p
            .params
            .iter()
            .map(|r| PwrParam {
                key: r.key.clone(),
                text: r.text.clone(),
            })
            .collect(),
        span: (p.span.start as usize)..(p.span.end as usize),
        attrs: McAttributes::new(),
    };
    decode_pwr_pin(&pin)
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

        // Rails live under the domain's MCAST_BODY child, with an in-body
        // partition read as transparent (`clause_list`).
        let rails = head
            .iter()
            .find(|c| c.is_type(MCAST_BODY))
            .map(|body| body.clause_list())
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

    /// The decode [`Self::from_node`] performs, with the declaration-side
    /// attribute diagnostics suppressed — the shape [`McPowerDecls::peek_domain`]
    /// must read through, since the real `parse_domain` walk parses the same
    /// clauses again and the diagnostics fire exactly once, there.
    fn from_node_quiet(node: &AstNode) -> Option<Self> {
        let head = node.get_sub_node()?;
        let name_node = head.iter().next()?;
        if name_node.get_type() != MCAST_IDS {
            return None;
        }
        let name = id_text(&name_node)?;

        let attrs = collect_attrs_quiet(&head);

        let rails = head
            .iter()
            .find(|c| c.is_type(MCAST_BODY))
            .map(|body| body.clause_list())
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

/// One `rail [hot, ret]::iface(params)` line (the domain's power pair).
/// `hot`/`ret` are the member net names; `iface` e.g. `DC`. Params are the
/// iface constructor arguments (3.3V, tol:±5%, …) held as text for now.
#[derive(Debug, Clone)]
pub struct McRailDecl {
    pub hot: String,
    pub ret: String,
    pub iface: String,
    /// Every written member of the row's square vector, in declaration order.
    /// `hot`/`ret` are its first two for the two-member shapes; a multi-terminal
    /// AC group (`::AC_3P5W`) declares five, and only the full list states the
    /// group the variant's registry entry describes (§3.2).
    pub members: Vec<String>,
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
        let mut members: Vec<String> = Vec::new();
        if let Some(inst) = child_of_type(&declare, MCAST_INSTANCE) {
            if let Some(sq) = find_square_vec(&inst) {
                if let Some(first) = sq.get_sub_node() {
                    members.extend(first.iter().filter_map(|opd| net_name(&opd)));
                }
            }
        }
        let hot = members.first().cloned().unwrap_or_default();
        let ret = members.get(1).cloned().unwrap_or_default();

        Some(Self {
            hot,
            ret,
            iface,
            members,
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
        // A named argument reads the same either way it is spelled, `k: v` or
        // `k = v` -- the grammar gives both one shape. `leaf_text` already
        // returns None for empty identifiers.
        if let Some((key_node, val_node)) = value.named_arg_parts() {
            return Some(Self {
                key: leaf_text(&key_node),
                text: value_text(&val_node),
                span,
            });
        }
        Some(Self {
            key: None,
            text: value_text(&value),
            span,
        })
    }
}

// helpers

pub(crate) fn collect_attrs(head: &AstNode) -> McAttributes {
    let mut attrs = McAttributes::new();
    for c in head.iter() {
        if c.is_type(MCAST_ATTRIBUTE) {
            attrs.parse(&c);
        }
    }
    attrs
}

/// The same read with the declaration diagnostics suppressed. The peek and
/// license-scan pre-reads (`peek_domain`, `scan_domain_bridges`) walk the same
/// clauses the real `parse_domain` walk will parse again, so a noisy
/// `collect_attrs` would report every declaration-side key defect twice.
pub(crate) fn collect_attrs_quiet(head: &AstNode) -> McAttributes {
    let mut attrs = McAttributes::new();
    for c in head.iter() {
        if c.is_type(MCAST_ATTRIBUTE) {
            if let Some(attribute) = McAttribute::new(&c) {
                attrs.push(attribute);
            }
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

    /// §3.2 the registry is the AC axis's identity anchor: the five variants and
    /// their terminal groups, and nothing else. `DC` is not an AC variant, and a
    /// name the table does not hold is not a contract however AC-shaped it looks.
    #[test]
    fn ac_variant_registry_holds_the_terminal_group_table() {
        use AcTerminal::{Neutral, Phase, Protective};
        assert_eq!(ac_terminal_group("AC").unwrap(), &[Phase, Neutral]);
        assert_eq!(
            ac_terminal_group("AC_1P3W").unwrap(),
            &[Phase, Neutral, Protective]
        );
        assert_eq!(
            ac_terminal_group("AC_3P3W").unwrap(),
            &[Phase, Phase, Phase],
            "a delta three-wire group has no neutral member"
        );
        assert_eq!(
            ac_terminal_group("AC_3P4W").unwrap(),
            &[Phase, Phase, Phase, Neutral]
        );
        assert_eq!(
            ac_terminal_group("AC_3P5W").unwrap(),
            &[Phase, Phase, Phase, Neutral, Protective]
        );
        assert!(ac_terminal_group("DC").is_none());
        assert!(ac_terminal_group("AC_2P").is_none(), "unregistered variant");
    }

    /// §3.1 the two vocabularies meet on one axis: the registered `{ac, dc}`
    /// value words and the rail ifaces (`::DC` / the `::AC*` registry), each
    /// matched exactly and anything else naming no axis.
    #[test]
    fn rail_axis_maps_words_and_ifaces_onto_the_two_axes() {
        assert_eq!(RailAxis::of_nature_word("ac"), Some(RailAxis::Ac));
        assert_eq!(RailAxis::of_nature_word("dc"), Some(RailAxis::Dc));
        assert_eq!(RailAxis::of_nature_word("AC"), None, "words are exact");
        assert_eq!(RailAxis::of_nature_word("unsure"), None);
        assert_eq!(RailAxis::of_iface("DC"), Some(RailAxis::Dc));
        assert_eq!(RailAxis::of_iface("AC_3P5W"), Some(RailAxis::Ac));
        assert_eq!(RailAxis::of_iface("AC_2P"), None, "unregistered variant");
        assert_eq!(RailAxis::of_iface(""), None);
    }

    /// §3.1 the consistency rule's read: each domain's written word verbatim
    /// (`None` when absent) beside every rail row's contract and span.
    #[test]
    fn l1_domain_natures_carry_the_word_and_every_rail_row() {
        let pi = parse_pi(SRC_AC);
        let doms = pi.l1_domain_natures();
        assert_eq!(doms.len(), 3, "domains: {doms:?}");
        let mains = doms.iter().find(|d| d.name == "MAINS").expect("MAINS");
        assert_eq!(mains.nature.as_deref(), Some("ac"));
        assert_eq!(mains.rails.len(), 1);
        assert_eq!(mains.rails[0].iface, "AC");
        assert_eq!(mains.rails[0].hot, "L");
        assert!(mains.rails[0].span.end > mains.rails[0].span.start);
        let vbulk = doms.iter().find(|d| d.name == "VBULK").expect("VBULK");
        assert_eq!(vbulk.nature, None, "a face writing no word states no axis");
        assert_eq!(vbulk.rails[0].iface, "DC");
    }

    /// §1.1 the domain-face projection: the identity-axis words of each domain,
    /// carried verbatim (which of them makes a face quiet is §1.4's rule, not
    /// this projection's). A domain writing none still lists.
    #[test]
    fn l1_domain_faces_carry_the_identity_words() {
        let pi = parse_pi(SRC_FACES);
        let faces = pi.l1_domain_faces();
        assert_eq!(faces.len(), 4, "faces: {faces:?}");
        let avdd = faces.iter().find(|f| f.name == "AVDD").expect("AVDD");
        assert_eq!(avdd.class.as_deref(), Some("analog"));
        assert_eq!(avdd.noise, None);
        let avaud = faces.iter().find(|f| f.name == "AVAUD").expect("AVAUD");
        assert_eq!(avaud.class.as_deref(), Some("analog"));
        assert_eq!(avaud.noise.as_deref(), Some("sensitive"));
        let dvdd = faces.iter().find(|f| f.name == "DVDD").expect("DVDD");
        assert_eq!(dvdd.class.as_deref(), Some("digital"));
        let plain = faces.iter().find(|f| f.name == "PLAIN").expect("PLAIN");
        assert_eq!(plain.class, None, "a face writing no word claims nothing");
        assert_eq!(plain.noise, None);
        assert_eq!(plain.nature, None);
    }

    const SRC_FACES: &str = r#"module main {
    conduit GNDA @role(quiet)
    domain AVDD  @class(analog)  { rail [VDDA, GNDA]::DC(3.3V) }
    domain AVAUD @class(analog) @noise(sensitive) { rail [V3A, GNDA]::DC(3.3V) }
    domain DVDD  @class(digital) { rail [VDD_3V3, GND]::DC(3.3V) }
    domain PLAIN { rail [Vx, GND]::DC(1V) }
}
"#;

    const SRC_PAIRS: &str = r#"module main {
    conduit GND  @role(main)
    conduit GNDA @role(quiet)
    domain DVDD  @class(digital) { rail [VDD_3V3, GND]::DC(3.3V) }
    domain AVDD  @class(analog)  { rail [VDDA, GNDA]::DC(3.3V) }
    domain MAINS @nature(ac) { rail [L, N]::AC(230V, 50Hz) }
    domain MAINS3 @nature(ac) { rail [U1, U2, U3]::AC_3P3W(400V, 50Hz) }
    domain DUALA { rail [VDD_1V8, GND]::DC(1.8V)
                   rail [VDD_1V2, GND]::DC(1.2V) }
    domain DUALB { rail [VAA, GNDA]::DC(3.3V)
                   rail [VBB, GNDA]::DC(5V) }
    domain BARE_A {}
    domain BARE_B {}
}
"#;

    /// §10.2 D1 the whole-reference predicate: a domain lists iff it declares
    /// **exactly one** `::DC` rail. Both rejection branches are filled twice —
    /// no rail at all (`BARE_*`), a rail but none DC (`MAINS` / `MAINS3`), and
    /// two DC rails (`DUALA` / `DUALB`) — so the projection cannot pass by
    /// listing nothing, and cannot pass by listing everything.
    #[test]
    fn l1_domain_pairs_list_only_single_dc_rail_domains() {
        let pi = parse_pi(SRC_PAIRS);
        // Pin the fixture first: every negative below is a domain that must be
        // *present and rejected*, not one the parser dropped — an absent domain
        // would make those assertions pass vacuously.
        assert_eq!(pi.domains.len(), 8, "domains: {:?}", pi.domains);
        let rail_counts: Vec<(&str, usize)> = pi
            .domains
            .iter()
            .map(|d| (d.name.as_str(), d.rails.len()))
            .collect();
        assert_eq!(
            rail_counts,
            vec![
                ("DVDD", 1),
                ("AVDD", 1),
                ("MAINS", 1),
                ("MAINS3", 1),
                ("DUALA", 2),
                ("DUALB", 2),
                ("BARE_A", 0),
                ("BARE_B", 0),
            ],
            "the fixture must carry every branch with its stated rail count"
        );
        let pairs = pi.l1_domain_pairs();
        let names: Vec<&str> = pairs.iter().map(|p| p.domain.as_str()).collect();
        assert_eq!(names, vec!["DVDD", "AVDD"], "pairs: {pairs:?}");

        let dvdd = pairs.iter().find(|p| p.domain == "DVDD").expect("DVDD");
        assert_eq!(dvdd.hot, "VDD_3V3");
        assert_eq!(dvdd.ret, "GND");
        assert!(dvdd.span.end > dvdd.span.start, "the rule needs an anchor");
        let avdd = pairs.iter().find(|p| p.domain == "AVDD").expect("AVDD");
        assert_eq!(avdd.hot, "VDDA");
        assert_eq!(avdd.ret, "GNDA");

        for absent in ["MAINS", "MAINS3"] {
            assert!(
                !names.contains(&absent),
                "{absent} declares no DC rail, so it stands for no pair"
            );
        }
        for dual in ["DUALA", "DUALB"] {
            assert!(
                !names.contains(&dual),
                "{dual} declares two DC rails, so it stands for no single pair"
            );
        }
        for bare in ["BARE_A", "BARE_B"] {
            assert!(
                !names.contains(&bare),
                "{bare} declares nothing to stand for"
            );
        }
    }

    const SRC_AC: &str = r#"module main {
    ref GND @role(main)
    domain MAINS  @nature(ac) { rail [L, N]::AC(230V, 50Hz) }
    domain MAINS3 @nature(ac) { rail [U1, U2, U3]::AC_3P3W(400V, 50Hz) }
    domain VBULK             { rail [Vb, N]::DC(310V) }
}
"#;

    /// §3.3 `::AC(v, f)` decodes into the AC read — RMS nominal and hertz — and
    /// the two readers partition the rail rows: `l1_rails` still holds exactly
    /// the DC pair, so every DC consumer keeps seeing DC only.
    #[test]
    fn l1_ac_rails_decode_rms_and_frequency_beside_the_dc_axis() {
        let pi = parse_pi(SRC_AC);
        let dc = pi.l1_rails();
        assert_eq!(dc.len(), 1, "the DC reader must not see AC rails: {dc:?}");
        assert_eq!(dc[0].hot, "Vb");

        let ac = pi.l1_ac_rails();
        assert_eq!(ac.len(), 2, "ac rails: {ac:?}");
        let mains = ac.iter().find(|r| r.domain == "MAINS").expect("MAINS");
        assert_eq!(mains.variant, "AC");
        assert_eq!(mains.members, vec!["L".to_string(), "N".to_string()]);
        assert_eq!(mains.hot, "L");
        assert_eq!(
            mains.ret.as_deref(),
            Some("N"),
            "the neutral slot is the pair's return (§3.3, model A)"
        );
        assert_eq!(mains.v_rms, Some(230.0));
        assert_eq!(mains.f, Some(50.0));
        assert!(mains.bad.is_none(), "MAINS: {:?}", mains.bad);

        let three = ac.iter().find(|r| r.domain == "MAINS3").expect("MAINS3");
        assert_eq!(three.variant, "AC_3P3W");
        assert_eq!(three.v_rms, Some(400.0));
        assert_eq!(
            three.ret, None,
            "a delta three-wire group declares no neutral, so no member is the return"
        );
        assert!(three.bad.is_none(), "MAINS3: {:?}", three.bad);
    }

    /// A row whose shape or values contradict the contract it names is flagged
    /// rather than silently accepted: a member count the variant's group does
    /// not declare, a nominal that is not volts, a second scalar that is not
    /// hertz, and an unknown key.
    #[test]
    fn l1_ac_rails_flag_shape_and_value_failures() {
        const BAD: &str = r#"module main {
    domain A { rail [L, N, PE]::AC(230V, 50Hz) }
    domain B { rail [L, N]::AC(50Hz) }
    domain C { rail [L, N]::AC(230V, 5A) }
    domain D { rail [L, N]::AC(230V, tol:±10%) }
}
"#;
        let pi = parse_pi(BAD);
        let ac = pi.l1_ac_rails();
        assert_eq!(ac.len(), 4, "ac rails: {ac:?}");
        let bad = |d: &str| {
            ac.iter()
                .find(|r| r.domain == d)
                .unwrap_or_else(|| panic!("domain {d}"))
                .bad
                .clone()
        };
        assert!(bad("A").is_some(), "three members on a two-member group");
        assert!(bad("B").is_some(), "50Hz is not a volts RMS");
        assert!(bad("C").is_some(), "5A is not a hertz frequency");
        assert!(bad("D").is_some(), "an AC contract carries no tol key");
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

    // §5.2 direction-word family pin-DC decode (design §4.1 / §4.4)

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

    // module power-output port capture (rail-contract-design.md §8.5)

    const SRC_PWR_PORTS: &str = r#"module main {
    psrc vin{VBUS_5V, GND}::DC(5V, capacity:2A, eff:0.9)
    psrc [C, D]::DC(5V, capacity:1A)
    psbi pb{B, GND}::DC(5V, capacity:300mA)
    psnk pin{PVIN, GND}::DC(5V)
    out shield_to_earth @bind_role(earth)   // non-power row untouched
    io  SEL
}
"#;

    #[test]
    fn captures_module_power_port_rows() {
        // §8.5 budget face: psrc/psbi/psnk module port rows are captured with
        // their full ::DC contract — the flatten layer keeps only the pair.
        let pi = parse_pi(SRC_PWR_PORTS);
        assert_eq!(pi.pwr_ports.len(), 4, "pwr_ports: {:#?}", pi.pwr_ports);
        // The ordinary identity rows stay in `ports`, none leak in.
        assert_eq!(pi.ports.len(), 1, "identity rows only");
        assert_eq!(pi.ports[0].names, vec!["shield_to_earth".to_string()]);

        let vin = pi
            .pwr_ports
            .iter()
            .find(|p| p.hot == "VBUS_5V")
            .expect("psrc vin row");
        assert_eq!(vin.dir, PwrDir::Src);
        assert_eq!(vin.ret.as_deref(), Some("GND"));
        // params kept as text: [nominal 5V, capacity, eff]
        assert_eq!(vin.params.len(), 3, "params: {:?}", vin.params);

        let cd = pi
            .pwr_ports
            .iter()
            .find(|p| p.hot == "C")
            .expect("bare bracket row");
        assert_eq!(cd.dir, PwrDir::Src);
        assert_eq!(cd.ret.as_deref(), Some("D"));
        assert_eq!(cd.params.len(), 2);

        let pb = pi.pwr_ports.iter().find(|p| p.hot == "B").expect("psbi");
        assert_eq!(pb.dir, PwrDir::Bi);
        let pin = pi
            .pwr_ports
            .iter()
            .find(|p| p.hot == "PVIN")
            .expect("psnk row captured structurally");
        assert_eq!(pin.dir, PwrDir::Snk);
    }

    #[test]
    fn l1_port_sources_decode_supply_contracts_only() {
        // psrc/psbi decode their source budget (capacity/eff); psnk and plain
        // identity rows never appear as a budget source.
        let pi = parse_pi(SRC_PWR_PORTS);
        let sources = pi.l1_port_sources();
        assert_eq!(sources.len(), 3, "Src/Bi only: {sources:#?}");

        let (hot, vin) = sources
            .iter()
            .find(|(h, _)| h == "VBUS_5V")
            .expect("vin source");
        assert_eq!(hot, "VBUS_5V");
        assert_eq!(vin.dir, PwrDir::Src);
        assert_eq!(vin.v, Some(5.0));
        assert_eq!(vin.capacity_amps, Some(2.0), "capacity:2A → 2.0");
        assert_eq!(vin.eff, Some(0.9));
        assert!(vin.bad.is_none(), "vin decode: {:?}", vin.bad);

        let cd = sources
            .iter()
            .find(|(h, _)| h == "C")
            .expect("bare bracket source");
        assert_eq!(cd.1.capacity_amps, Some(1.0));

        let pb = sources
            .iter()
            .find(|(h, _)| h == "B")
            .expect("psbi conditional source");
        assert_eq!(pb.1.dir, PwrDir::Bi);
        assert_eq!(pb.1.capacity_amps, Some(0.3), "300mA → 0.3A");

        assert!(
            sources.iter().all(|(_, s)| s.dir != PwrDir::Snk),
            "psnk port rows must not project as budget sources"
        );
    }

    #[test]
    fn decode_port_pwr_source_budget_flags_mirror_pins() {
        // Non-current capacity / source-exclusive discipline decode exactly like
        // component pin rows (same decode_pwr_pin value language).
        const BAD: &str = r#"module main {
    psrc vin{VBUS_5V, GND}::DC(5V, capacity:1.5V)
}
"#;
        let pi = parse_pi(BAD);
        assert_eq!(pi.pwr_ports.len(), 1);
        let src = pi.l1_port_sources();
        assert_eq!(src.len(), 1);
        let bad = src[0].1.bad.as_deref().expect("bad capacity flagged");
        assert!(bad.contains("not a DC current"), "got: {bad}");
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
