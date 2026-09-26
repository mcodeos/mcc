// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::{
    basic::mc_bus::{McBus, McList},
    basic::mc_endpoint::{McEndpoint, McInstanceRef},
    basic::mc_fcall::McFuncCall,
    basic::mc_phrase::McPhrase,
    mc_func::{GateCandidate, HasFindInst, McFunctions},
    mc_inst::{McInst, McInstance, McInstances},
};
use crate::db::context::DB;
use crate::db::diagnostic::diagnostic::{dlog_error, Position};
use crate::refdef::types::ChainSegment;
use crate::semantic::basic::mc_conds::McConds;
use crate::semantic::basic::mc_param_type::{McParamType, McParamTypeKind};
use crate::semantic::component::mc_layout::McLayout;
use crate::semantic::component::Mc2Component;
use crate::semantic::context::resolve_cmie;
use crate::semantic::mc_func::McFuncReturn;
use crate::{
    ast::{macros::*, node::AstNode},
    semantic::basic::mc_param::McParamDeclares,
    semantic::common::BlockPartitions,
    IOType, McCMIE, McIds, McParamValue, McURI, SymbolKind,
};
use std::sync::Arc;

pub(crate) mod expects;
use self::expects::Ledger;
pub(crate) mod pi;
use self::pi::McPowerDecls;

// McModule - Module definition

#[derive(Debug, Clone)]
pub struct McModule {
    pub name: McIds,
    pub params: McParamDeclares,
    /// `layout = [ ... ]` boundary-port placement hint. Takes effect where this
    /// module is instantiated as a child box (SubModule); port names are listed
    /// per edge in counterclockwise package order.
    pub layout: McLayout,
    /// `expects = [ ... ]` expectation rows declared in this module body
    /// (declaration face only; storage, no engine yet).
    pub expects: Ledger,
    pub insts: McInstances,
    pub stmts: Vec<McPhrase>,
    /// Source span for each connection stmt in `stmts` (parallel array).
    /// Used for diagnostic position reporting during instantiation.
    pub stmt_spans: Vec<crate::ast::sem::Span>,
    pub funcs: McFunctions,
    /// Power-intent declarations declared in this module body
    /// (`ref` identities + `domain`/`rail` sources; intent-design.md §5).
    pub(crate) pi: McPowerDecls,
    /// ★ U168: display-only partition table of this body (`block` groupings,
    /// [`BlockPartitions`]) — rebuilt from the AST at parse time, no id issued,
    /// no semantic rule reads it. The viz block-frame projection consumes it.
    pub blocks: BlockPartitions,
    pub uri: McURI,
    /// Source span for LSP goto-definition (byte range in `uri`).
    pub span: crate::ast::sem::Span,
    anon_counter: usize,
    /// resolve-gate §1.3: instance names / FuncCall callers noted during body
    /// parse. Module-level B-family bases (e.g. `PL3085A(...) PL.Cap()` → `PL`)
    /// never enter insts, so the ghost-bus discriminator passes them from this set.
    pub(crate) seen_callers: Vec<String>,
    /// resolve-gate §1.3 (relax-everything): ghost-bus true-miss candidates registered at
    /// parse time, rechecked at component-finish by validation::gate::GateCheck
    /// → E3137 single-use inline-net warning / resolved_late balance.
    pub(crate) gate_candidates: Vec<GateCandidate>,
    /// resolve-gate §1.6 ①: bare-identifier floating-label candidates from the
    /// module **top-level body** net statements. Module funcs register into
    /// their own `McFunction.floating_candidates`; this set covers the module
    /// body itself. Both are consumed together by `validation::floating`
    /// (E3136) — without it, module-side port spelling errors stay 0-diagnostic.
    pub(crate) floating_candidates: Vec<(String, u32, u32)>,
    /// Scratch buffer during body parse — `report_floating_label(&self, …)`
    /// pushes here (the trait hook is `&self`, and `McModule` is shared across
    /// threads behind `Arc` in the workspace table, so an `Arc<Mutex>`; the
    /// `Arc` keeps the `#[derive(Clone)]` on the struct). Drained into
    /// `floating_candidates` at the end of `parse_body`.
    floating_pending: std::sync::Arc<std::sync::Mutex<Vec<(String, u32, u32)>>>,
    /// §10.11.4 guard ③: this module's whole-referenceable domains, peeked off
    /// the `domain` clauses **before** the body walk so a bare domain name means
    /// the same thing whether its clause is written above or below the
    /// statement that uses it. Read-only: the peek builds its own list
    /// (`pi::peek_domain` / `pi::domain_pairs_of`) and never touches `pi`.
    pub(crate) domain_pairs_peek: Vec<pi::L1DomainPair>,
    /// R3 member mode (intent-reference-layer-design.md §10.4): source
    /// positions of the domain words that sit on a `@bridge(domain, domain)`-licensed
    /// chain, mapped to the single rail member (`hot` under `->`, `ret` under
    /// `<-`) the word resolves to. Filled by [`Self::scan_domain_bridges`]
    /// before the body walk — same position-free visibility as
    /// `domain_pairs_peek` — and consulted by the widening write point
    /// (`McPhrase::new`'s bare-`McOpd::Id` arm) through
    /// `HasFindInst::licensed_domain_member_at`. Keyed by word position in a
    /// `BTreeMap`: the map is built in written order and read by exact key, so
    /// no iteration order ever reaches a reading (build-design §3.7).
    licensed_members: std::collections::BTreeMap<usize, String>,
}

impl McModule {
    pub fn new(node: &AstNode, uri: &McURI) -> Option<Self> {
        // MCK_MODULE
        // |- MCAST_NAME - MCAST_PARAM (option) - MCAST_BODY
        if let Some(subnodes) = node.get_sub_node() {
            let module_name = subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))
                .and_then(|n| n.get_sub_node())
                .and_then(|n| McIds::new_with_dot(&n));

            let Some(body) = subnodes.iter().find(|x| x.is_type(MCAST_BODY)) else {
                dlog_error(
                    crate::errcodes::MODULE_MISSING_SUBNODE,
                    node,
                    &crate::errcodes::format_msg(crate::errcodes::MODULE_MISSING_SUBNODE, &[]),
                );
                return None;
            };

            let module_name = module_name?;

            // Span from the module name (MCAST_NAME → MCAST_IDS), not the whole node
            let ids_node = subnodes
                .iter()
                .find(|x| x.is_type(MCAST_NAME))
                .and_then(|n| n.get_sub_node())?;
            let start = ids_node.get_pos() as usize;
            let end = start + ids_node.get_len() as usize;
            let mut module = Self {
                name: module_name,
                params: McParamDeclares::new(),
                layout: McLayout::default(),
                expects: Ledger::default(),
                funcs: McFunctions::new(),
                pi: McPowerDecls::new(),
                blocks: BlockPartitions {
                    uri: uri.clone(),
                    roots: Vec::new(),
                },
                insts: McInstances::new(),
                stmts: Vec::new(),
                stmt_spans: Vec::new(),
                uri: uri.clone(),
                span: crate::ast::sem::Span { start, end },
                anon_counter: 1,
                seen_callers: Vec::new(),
                gate_candidates: Vec::new(),
                floating_candidates: Vec::new(),
                floating_pending: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
                domain_pairs_peek: Vec::new(),
                licensed_members: std::collections::BTreeMap::new(),
            };

            // 2. Parse parameters
            if let Some(param_node) = subnodes.iter().find(|x| x.is_type(MCAST_PARAMS)) {
                module.parse_params(&param_node);
            }

            // 3. Parse body
            module.parse_body(&body);

            // ★ P4.1 ("Pass1b" hook): the whole body — including all `func`
            // definitions — is now parsed, so every FuncCall's return shape
            // (eval.md §8.1) can be resolved against the complete funcs table.
            // `this`/implicit → caller shape preserved; `return <expr>` → [0|N].
            {
                let stmts = std::mem::take(&mut module.stmts);
                for mut stmt in stmts {
                    McFuncCall::fill_return_shapes(&mut stmt, &module);
                    module.stmts.push(stmt);
                }
            }
            Some(module)
        } else {
            dlog_error(
                crate::errcodes::MODULE_MISSING_SUBNODE,
                node,
                &crate::errcodes::format_msg(crate::errcodes::MODULE_MISSING_SUBNODE, &[]),
            );
            None
        }
    }

    /// Test-only stub constructor: builds a minimal module with no parsed
    /// body. Available only under `#[cfg(test)]` so instance-layer scope
    /// unit tests can construct a [`McModuleInst`] without an AST.
    #[cfg(test)]
    pub fn test_stub(name: &str) -> Self {
        Self {
            name: McIds::from(name),
            params: McParamDeclares::new(),
            layout: McLayout::default(),
            expects: Ledger::default(),
            insts: McInstances::new(),
            stmts: Vec::new(),
            stmt_spans: Vec::new(),
            funcs: McFunctions::new(),
            pi: McPowerDecls::new(),
            blocks: BlockPartitions::default(),
            uri: McURI::default(),
            span: crate::ast::sem::Span {
                start: 0,
                end: name.len(),
            },
            anon_counter: 1,
            seen_callers: Vec::new(),
            gate_candidates: Vec::new(),
            floating_candidates: Vec::new(),
            floating_pending: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            domain_pairs_peek: Vec::new(),
            licensed_members: std::collections::BTreeMap::new(),
        }
    }

    pub(crate) fn parse_params(&mut self, decl_node: &AstNode) {
        // Parameters divided into 2 categories: data and inst, each parsed separately
        // MCAST_PARAMS
        //   |- MCAST_PARAM
        //      |- MCAST_ROLE               : parse as params: McParamDeclares
        //      |- MCAST_IDS                : parse as params: McParamDeclares
        //      |- MCAST_SQUARE_VEC         : parse as params: McParamDeclares
        //      |- MCAST_DECLARE_UV         : parse as params: McParamDeclares

        //      |- MCAST_DECLARE            : parse as insts: McInstances

        if let Some(subnodes) = decl_node.get_sub_node() {
            for param_node in subnodes.iter() {
                // Each MCAST_PARAM child node determines its type
                let Some(subnode) = param_node.get_sub_node() else {
                    continue;
                };

                match subnode.get_type() {
                    // Data parameter -> params
                    MCAST_ROLE | MCAST_IDS | MCAST_SQUARE_VEC | MCAST_DECLARE_UV => {
                        self.params.parse(&param_node);
                    }
                    // Instance parameter -> insts, or enum-class/interface data param
                    MCAST_DECLARE => {
                        // Check if CLASS is an enum → data param (B5/B6)
                        let is_enum = McParamType::extract_class_name_from_declare(&subnode)
                            .map(|cn| crate::db::cmie::cmie::is_enum_class_name(&cn))
                            .unwrap_or(false);
                        // Check if CLASS is an interface → port param (A3/A4)
                        // e.g., USB_VBUS_1{VDD_3V, GND}::DC(3.3V) has name prefix
                        // and is parsed as MCAST_DECLARE (not MCAST_SQUARE_VEC).
                        let pt = McParamType::from_ast(&subnode);
                        let is_interface = matches!(
                            pt.kind,
                            McParamTypeKind::Interface { .. }
                                | McParamTypeKind::InterfaceWithRole { .. }
                        );
                        // `id::Class(k = v)` inline-attr declares classify as
                        // plain interface params — the former A5 arm retired
                        // together with E3112 (U271).
                        if is_enum || is_interface {
                            self.params.parse(&param_node);
                            // ★ LSP: register the interface class ref of a
                            // module port (`[VDD_3V3,GND]::DC(3.3V)` → `DC`) so
                            // goto-def lands on the library `interface DC`
                            // definition (same path as component pin ::ifaces).
                            if is_interface {
                                // A MCAST_DECLARE clause typed as an interface
                                // reaches this arm only because it carries NO
                                // leading direction word (a `psrc`/`psnk`/`psbi`
                                // direction routes the clause to the MCAST_IOTYPE
                                // arm above). Design removed the no-direction
                                // header sugar for power/DC supply params: every
                                // such declare must state its energy direction
                                // explicitly (E3055), no legacy tolerance.
                                let extracted = Self::extract_declare_class_span(&subnode);
                                let module_name = self.name.to_string();
                                let class_name = extracted
                                    .as_ref()
                                    .map(|(cn, _)| cn.base_name())
                                    .unwrap_or_default();
                                dlog_error(
                                    crate::errcodes::MODULE_HEADER_IFACE_NEEDS_DIRECTION,
                                    &subnode,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::MODULE_HEADER_IFACE_NEEDS_DIRECTION,
                                        &[&module_name, &class_name],
                                    ),
                                );
                                // Keep registering (LSP + curly bus def) so
                                // goto-def stays live while the author fixes
                                // the direction — fewer cascading errors.
                                if let Some((class_name, class_span)) = extracted {
                                    tracing::info!(target: "mcc::lsp::audit",
                                        "[AUDIT-ModulePort-Iface] class={class_name} span={class_span:?} uri={}",
                                        self.uri);
                                    crate::query::refs::mcb_register_declare_class(
                                        &self.uri,
                                        &class_name,
                                        class_span,
                                    );
                                } else {
                                    tracing::info!(target: "mcc::lsp::audit",
                                        "[AUDIT-ModulePort-Iface] extract failed, subnode_type={}",
                                        subnode.get_type());
                                }
                                // ★ LSP: curly interface params
                                // (`dc{VDD_3V3, GND}::DC(3.3V)`) register a
                                // BusDef with declaration-site member spans so
                                // `dc.GND` / `dc.VDD_3V3` resolve via
                                // bus_member_hit to the member text in THIS
                                // file instead of a use-site span.
                                Self::register_curly_param_bus_def(&subnode, &mut self.insts);
                            }
                        } else {
                            self.insts.parse(&subnode, &self.uri);
                        }
                    }
                    // IOTYPE-prefix parameter -> insts + params (e.g. psnk dc24v, in GPIO[1:2])
                    MCAST_IOTYPE => {
                        self.insts.parse(&param_node, &self.uri);
                        self.params.parse(&param_node); // also register for unused detection

                        // A header power row is the same declaration the body
                        // form writes, only written in the parameter list — the
                        // spelling the corpus uses for every module supply face
                        // (`module MIC_SIP(psnk dc{VDD_3V3, GND}::DC(3.3V))`).
                        // Capture its contract here too, or `pwr_ports` stays
                        // empty for it and every consumer goes blind (the
                        // direction judge, the §8.5 budget face, the Power axis).
                        self.pi.parse_port_pwr(&param_node);
                    }
                    _ => {
                        // Unknown type, try to parse as data parameter
                        dlog_error(
                            crate::errcodes::MODULE_PARAM_TYPE_UNEXPECTED,
                            &subnode,
                            &crate::errcodes::format_msg(
                                crate::errcodes::MODULE_PARAM_TYPE_UNEXPECTED,
                                &[],
                            ),
                        );
                    }
                }
            }
        }
    }

    /// §10.11.3 ①: did reading this connection statement already report its own
    /// concrete failure? These are every code the chain-shape gate raises *at
    /// the operand that failed* — `->` / `+` / `<-` width mismatch, and the
    /// column-width mix the same gate checks just before them. A statement the
    /// gate rejected was read to the end; calling it a parse failure
    /// (`CONN_STMT_PARSE_FAILED`'s own words) restates the same defect in the
    /// wrong vocabulary and buries the real one.
    ///
    /// Deliberately *shape* only. `CONN_OPERATOR_UNSUPPORTED` (4008) is the
    /// control: a statement that met a construct the grammar has no reading for
    /// genuinely failed to parse, carries no shape fact, and keeps the wrapper.
    ///
    /// Read off the diagnostic ledger by span rather than returned out of
    /// `McPhrase::new`: the failing arm is nested (a series leg inside a series
    /// leg), so a return flag would only carry the innermost verdict to one
    /// caller, while the span query sees every code the statement raised.
    /// Conditional body clauses (`if (cond) { expects += [ ... ] }`,
    /// circuit-intent-acceptance-design.md §3). Branches judge against this
    /// module's formal params with their defaults; the first statically true
    /// branch wins, exactly like [`McConds::evaluate`]. A branch whose
    /// condition cannot be judged statically (a runtime quantity, an unknown
    /// name) defers every row it declares — `deferred` rows reach the engine
    /// as DEFER verdicts, never diagnostics (§5.1). The else arm is walked
    /// only when nothing matched.
    fn read_cond_expects(&mut self, clause: &AstNode) {
        let params = self.params.get_cond_params_with_defaults();
        let mut else_block: Option<AstNode> = None;
        for (condition, block) in McConds::raw_branches(clause) {
            match condition {
                Some(cond) => match McConds::check_condition_result(&cond, &params, None) {
                    Ok(true) => {
                        self.read_cond_block(&block, false);
                        return;
                    }
                    Ok(false) => {}
                    Err(_) => {
                        // Not statically decidable: the whole entry defers.
                        self.read_cond_block(&block, true);
                        return;
                    }
                },
                None => else_block = Some(block),
            }
        }
        if let Some(block) = else_block {
            self.read_cond_block(&block, false);
        }
    }

    /// One conditional branch body: the bare clause form or the braced form
    /// (grammar `mc_cond_block`).
    fn read_cond_block(&mut self, block: &AstNode, deferred: bool) {
        if block.is_type(MCAST_BODY) {
            if let Some(sub) = block.get_sub_node() {
                for clause in sub.iter() {
                    self.read_cond_clause(&clause, deferred);
                }
            }
            return;
        }
        self.read_cond_clause(block, deferred);
    }

    /// One clause inside a conditional branch. Only expectation rows carry
    /// the branch's verdict; every other clause kind keeps the top-level
    /// unexpected-clause diagnostic — a branch is not a place where module
    /// bodies accept clauses they otherwise reject.
    fn read_cond_clause(&mut self, clause: &AstNode, deferred: bool) {
        match clause.get_type() {
            MCAST_COND_IF => self.read_cond_expects(clause),
            MCAST_ATTRIBUTE | MCAST_ATTRIBUTE_ADD => match Ledger::new(clause) {
                Some(mut ledger) => {
                    if deferred {
                        for row in &mut ledger.rows {
                            row.deferred = true;
                        }
                    }
                    self.expects.rows.extend(ledger.rows);
                }
                None => dlog_error(
                    crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                    clause,
                    &crate::errcodes::format_msg(
                        crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                        &[&"an attribute row inside a conditional branch is not accepted here"],
                    ),
                ),
            },
            _ => {
                // U300 M9a: name the actual rule so the author can self-check —
                // a conditional branch in a module body reads expectation rows
                // only; connections and other clause kinds are not read from it.
                dlog_error(
                    crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                    clause,
                    &crate::errcodes::format_msg(
                        crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                        &[&"a clause inside a conditional branch is not accepted here — branches only carry expectation rows (`expects = [ ... ]`)"],
                    ),
                )
            }
        }
    }

    fn stmt_reports_own_failure(&self, clause: &AstNode) -> bool {
        const PASS1_SHAPE_CODES: [u32; 4] = [
            crate::errcodes::CONN_SERIES_SHAPE_MISMATCH,
            crate::errcodes::CONN_PARALLEL_SHAPE_MISMATCH,
            crate::errcodes::CONN_LEFT_ARROW_SHAPE_MISMATCH,
            crate::errcodes::SHAPE_COLUMN_WIDTH_MIXED,
        ];
        let uri = crate::current_uri::get();
        let start = clause.get_pos();
        let end = start + clause.get_len();
        PASS1_SHAPE_CODES.iter().any(|&code| {
            crate::db::diagnostic::diagnostic::has_code_in_range(code, &uri, start, end)
        })
    }

    /// R3 pre-scan (intent-reference-layer-design.md §10.4): one read-only walk
    /// over the body's connection clauses, before the body walk below, that
    /// (a) fills `licensed_members` — the domain-word positions whose
    /// `@bridge(domain, domain)` license resolves them to a *single* directed rail
    /// member instead of the whole `[hot, ret]` pair; (b) reports the family's
    /// three static defects — 6046 mixed bridge identity, 6047 reversed
    /// argument order, 6048 leg inconsistency; (c) reports 6049 for a named
    /// pair no chain in the module witnesses.
    ///
    /// Everything here is judged off the AST (attributes through
    /// `pi::collect_attrs`, words through the same `McOpd::new` funnel the
    /// widening write point reads), so the scan's keys and the write point's
    /// lookups cannot drift. A domain word written before its `domain` clause
    /// licenses exactly as one written after — the pair table is the position
    /// -free peek, like `domain_pairs_peek` above.
    fn scan_domain_bridges(&mut self, clauses: &[AstNode]) {
        if self.domain_pairs_peek.is_empty() {
            return; // no whole-referenceable domain: nothing can license
        }
        let pairs = self.domain_pairs_peek.clone();
        let uri = crate::current_uri::get();
        let mut licensed: std::collections::BTreeMap<usize, String> =
            std::collections::BTreeMap::new();
        // Sorted (a, b) domain-pair key → (witnessed side, first naming span).
        // `None` side = named but unwitnessed, 6049's object after the walk.
        let mut named: std::collections::BTreeMap<(String, String), (Option<bool>, (u32, u32))> =
            std::collections::BTreeMap::new();
        for clause in clauses {
            if !clause.is_type(MCAST_NET) {
                continue;
            }
            let Some(head) = clause.get_sub_node() else {
                continue;
            };
            for attr in pi::collect_attrs(&head).iter() {
                if attr.id.to_string().as_str() != "bridge" {
                    continue;
                }
                let texts = pi::value_texts(attr);
                let Some((pa, pb)) = pi::domain_bridge_of(&texts, &pairs) else {
                    // Not licensed. Exactly one side naming a whole-referenceable
                    // domain is the mixed identity — reported, and the statement
                    // keeps today's reading (the net-level pair of raw texts).
                    if texts.len() == 2 {
                        let mixed: Vec<&pi::L1DomainPair> = pairs
                            .iter()
                            .filter(|p| p.domain == texts[0] || p.domain == texts[1])
                            .collect();
                        if let [p] = mixed[..] {
                            report_domain_bridge_code(
                                crate::errcodes::DOMAIN_NET_MIXED_BRIDGE,
                                clause,
                                &[&p.domain, &p.hot, &p.ret],
                            );
                        }
                    }
                    continue;
                };
                let mut words: Vec<DomainBridgeWord> = Vec::new();
                collect_domain_bridge_words(&head, None, &mut words);
                words.sort_by_key(|w| w.pos);
                // Member take (§10.4): each domain word resolves to the member
                // its own arrow direction names — `->` takes the hot member,
                // `<-` the return member. The resolution is per word, so it does
                // not depend on the argument order; a reversed writing is
                // reported (below) *and* stays licensed, because re-reading the
                // chain from the other end to make the orders agree would be
                // the silent rewrite this layer forbids.
                for w in &words {
                    if w.name != pa.domain && w.name != pb.domain {
                        continue;
                    }
                    let Some(hot) = w.dir else { continue };
                    let p = if w.name == pa.domain { &pa } else { &pb };
                    licensed.insert(w.pos, if hot { p.hot.clone() } else { p.ret.clone() });
                }
                // 6047: the written left-to-right order of the two domain words
                // must agree with the argument order — the mirrored writing of
                // the same crossing is not silently re-read.
                let first_at = |dom: &str| words.iter().find(|w| w.name == dom).map(|w| w.pos);
                if let (Some(a_at), Some(b_at)) = (first_at(&pa.domain), first_at(&pb.domain)) {
                    if a_at > b_at {
                        report_domain_bridge_code(
                            crate::errcodes::DOMAIN_BRIDGE_DIRECTION_REVERSED,
                            clause,
                            &[&pa.domain, &pb.domain],
                        );
                    }
                }
                // Witness (§10.4): a chain witnesses a leg when both its end
                // words land on one side of the pair — a domain word takes its
                // arrow's side; a literal end landing on a member name takes
                // that member's side. Ends on opposite sides are the
                // inconsistent chain (6048); an end on neither member
                // witnesses nothing (silence, per §10.4's hot-vs-return
                // criterion — a dangling pair is 6049's object, not this).
                let side_of = |w: &DomainBridgeWord| -> Option<bool> {
                    if w.name == pa.domain || w.name == pb.domain {
                        return w.dir;
                    }
                    if w.name == pa.hot || w.name == pb.hot {
                        return Some(true);
                    }
                    if w.name == pa.ret || w.name == pb.ret {
                        return Some(false);
                    }
                    None
                };
                let witness = match (words.first(), words.last()) {
                    (Some(f), Some(l)) if f.pos != l.pos => match (side_of(f), side_of(l)) {
                        (Some(s), Some(t)) if s != t => {
                            report_domain_bridge_code(
                                crate::errcodes::DOMAIN_BRIDGE_LEG_INCONSISTENT,
                                clause,
                                &[&pa.domain, &pb.domain],
                            );
                            None
                        }
                        (Some(s), Some(_)) => Some(s),
                        _ => None,
                    },
                    _ => None,
                };
                let mut key = (pa.domain.clone(), pb.domain.clone());
                if key.0 > key.1 {
                    std::mem::swap(&mut key.0, &mut key.1);
                }
                let entry = named
                    .entry(key)
                    .or_insert((None, (clause.get_pos(), clause.get_len())));
                if witness.is_some() && entry.0.is_none() {
                    entry.0 = witness;
                }
            }
        }
        // 6049: a named pair with zero witnessed legs anywhere in the module.
        for ((a, b), (witness, (pos, len))) in &named {
            if witness.is_some()
                || crate::db::diagnostic::diagnostic::has_code_at(
                    crate::errcodes::DOMAIN_BRIDGE_DANGLING,
                    &uri,
                    *pos,
                )
            {
                continue;
            }
            crate::db::diagnostic::diagnostic::diagnostic_log(
                crate::errcodes::DOMAIN_BRIDGE_DANGLING,
                crate::db::diagnostic::diagnostic::DiagnosticLevel::Error,
                *pos,
                *len,
                &crate::errcodes::format_msg(crate::errcodes::DOMAIN_BRIDGE_DANGLING, &[a, b]),
                &[],
            );
        }
        self.licensed_members = licensed;
    }

    pub(crate) fn parse_body(&mut self, body: &AstNode) {
        // ★ LSP: Set scope for instance registration
        self.insts.scope = Some(self.name.to_string());
        // ★ U168: record the body's partition tree before the transparent walk
        // below. `clause_list` flattens partitions away — that is the semantic
        // ruling (group-only) and stays untouched; this side table is the
        // display-only projection of what was written.
        self.blocks.roots = crate::semantic::common::collect_block_partitions(body);
        // `clause_list` makes an in-body partition transparent: a `block`'s
        // clauses are dispatched exactly as if they had been written here.
        let clauses = body.clause_list();
        if !clauses.is_empty() {
            // ── §10.11.4 guard ③: declaration visibility is position-free ──
            // A bare domain name must mean the same thing above and below its
            // own `domain` clause. `pi` fills source-order (the walk below is
            // one pass), so the table the walk consults is taken here, up
            // front, off the same clauses — read-only (`pi::peek_domain`), so
            // `pi` state and every diagnostic the real `parse_domain` would
            // raise are untouched; the walk still does the one real parse.
            self.domain_pairs_peek = pi::domain_pairs_of(
                &clauses
                    .iter()
                    .filter(|c| c.is_type(MCAST_DOMAIN))
                    .filter_map(|c| McPowerDecls::peek_domain(&c))
                    .collect::<Vec<_>>(),
            );
            // ── R3 domain-level @bridge (intent-reference-layer-design.md
            // §10.4) ── same position-free pre-read as the peek above: the
            // license scan classifies every `@bridge(domain, domain)` statement and
            // fills the word-position map the widening write point consults,
            // so a statement means the same thing wherever its `domain`
            // clauses were written.
            self.scan_domain_bridges(&clauses);
            for clause in clauses {
                let ct = clause.get_type();
                match ct {
                    MCAST_NET_PORTS => {
                        self.insts.parse(&clause, &self.uri);
                        // Power-intent identity words trail module-interface
                        // port rows (`io MIC{P,N} @class(analog) @return(GNDA)`,
                        // `out … @bind_role(earth)`, `io … @exposed(…)` —
                        // design §5.1 unified slot). The net reader above
                        // registers the port operands and drops the trailing
                        // words, so identity-bearing rows are re-captured here
                        // (design §13 groundwork; no rule consumes them yet).
                        self.pi.parse_port(&clause);
                        // Power-output port rows (`psrc NAME{hot,ret}::DC(…)`)
                        // carry a full ::DC contract the flatten layer records
                        // only as a written pair — capture capacity/eff here for
                        // the PWR-4 budget axis (§8.5 budget face).
                        self.pi.parse_port_pwr(&clause);
                    }

                    MCAST_NET => {
                        if let Some(subnode) = clause.get_sub_node() {
                            if subnode.get_type() == MCAST_DECLARE {
                                // ★ LSP: instance declarations also reference their ctor args
                                // (`speaker(V3V3)`) — record
                                // them like MCAST_NET operands so F12 works.
                                self.collect_declare_ctor_refs(&subnode);
                                // ★ NC layer ③: the declaration's trailing `@ncpin(…)`
                                // marker rides as a *sibling* of this node
                                // (`mc_net: mc_phrase mc_tattrs_opt`), so it is
                                // staged for `parse` instead of being read from
                                // inside the declare node.
                                self.insts
                                    .set_nc_pins(crate::semantic::nc_pin::read_nc_pins(&subnode));
                                self.insts.parse(&subnode, &self.uri);
                                continue;
                            }
                            // `return` is dead syntax in a module body — only a
                            // function body has a receiver. Without this the
                            // statement dies as a generic connection-parse failure.
                            if subnode.get_type() == MCAST_IOTYPE_RETURN {
                                dlog_error(
                                    crate::errcodes::MODULE_RETURN_NOT_ALLOWED,
                                    &clause,
                                    &crate::errcodes::format_msg(
                                        crate::errcodes::MODULE_RETURN_NOT_ALLOWED,
                                        &[],
                                    ),
                                );
                                continue;
                            }
                            // Power-intent relation-edge attributes (`@bridge(a,b)`
                            // …) trail this connection net; the net reader consumes
                            // only the phrase head, so capture the edges here
                            // (design §13 landing 1 groundwork).
                            self.pi.parse_net(&clause);

                            // Collect port reference spans before parsing the net
                            let scope = self.name.to_string();
                            Self::collect_net_refs_in_node(
                                &subnode,
                                &mut self.insts,
                                &mut self.params,
                                &scope,
                            );
                            match McPhrase::new(&subnode, self) {
                                Some(net) => {
                                    // Store definition spans + LSP lapper entries for inline ports
                                    Self::collect_net_def_spans(
                                        &subnode,
                                        &mut self.insts,
                                        &self.uri,
                                        &self.name.to_string(),
                                    );
                                    // Track source span for diagnostic position reporting
                                    let stmt_start = subnode.get_pos() as usize;
                                    let stmt_end = stmt_start + subnode.get_len() as usize;
                                    self.stmt_spans.push(crate::ast::sem::Span {
                                        start: stmt_start,
                                        end: stmt_end,
                                    });
                                    self.stmts.push(net);
                                }
                                None => {
                                    // ── §10.11.3 ①: the generic wrapper states
                                    // nothing when the statement's own span
                                    // already carries the concrete failure.
                                    // `None` from the series / parallel /
                                    // left-arrow arms means the phrase was read
                                    // to the end and failed the chain-shape gate
                                    // (E4007 / E4005 / E4002, reported at that
                                    // point); stacking "A connection statement
                                    // failed to parse." on top reads as a parse
                                    // error where the real defect is a width
                                    // mismatch.
                                    if !self.stmt_reports_own_failure(&clause) {
                                        dlog_error(
                                            crate::errcodes::CONN_STMT_PARSE_FAILED,
                                            &clause,
                                            &crate::errcodes::format_msg(
                                                crate::errcodes::CONN_STMT_PARSE_FAILED,
                                                &[],
                                            ),
                                        );
                                    }
                                }
                            }
                        } else {
                            dlog_error(
                                crate::errcodes::FUNC_EMPTY_NET,
                                &clause,
                                &crate::errcodes::format_msg(crate::errcodes::FUNC_EMPTY_NET, &[]),
                            );
                        }
                    }

                    MCAST_FUNCTION => {
                        let context = unsafe { &mut *(self as *mut McModule) };
                        // ★ LSP: register interface class refs from the func
                        // header (`func power(V3V3::DC(3.3V))` → `DC`) so
                        // goto-def / hover resolve them (same path as module
                        // ports). Must run before create_lapper consumes
                        // declare_class_refs.
                        crate::query::refs::register_func_header_iface_refs(&clause, &self.uri);
                        self.funcs.parse(&clause, context);
                    }

                    MCAST_REF => {
                        // Power-intent conductor-identity declaration
                        // (`conduit GND @role(main) @star`; keyword `ref` is a
                        // legacy alias). Capture only — the identity/role
                        // semantics land with the flatten/ERC block
                        // (intent-design.md §13 landing 1).
                        self.pi.parse_ref(&clause);
                    }

                    MCAST_DOMAIN => {
                        // Power-intent domain/rail source block
                        // (`domain DVDD { rail ... }`). Capture the domain and
                        // its rails; rail supply semantics land later.
                        self.pi.parse_domain(&clause);
                    }

                    MCAST_DECLARE => {
                        // ★ LSP: record ctor-arg refs for goto-def (see collect_declare_ctor_refs).
                        self.collect_declare_ctor_refs(&clause);
                        self.insts.parse(&clause, &self.uri);
                    }

                    MCAST_ROLE => {
                        dlog_error(
                            crate::errcodes::MODULE_ROLE_UNSUPPORTED,
                            &clause,
                            &crate::errcodes::format_msg(
                                crate::errcodes::MODULE_ROLE_UNSUPPORTED,
                                &[],
                            ),
                        );
                    }
                    MCAST_COND_IF => {
                        // `if (cond) { expects += [ ... ] }` (design §3): only
                        // expectation rows are read out of a conditional
                        // branch; every other clause kind inside the branch
                        // keeps the unexpected-clause diagnostic it gets at
                        // the top level.
                        self.read_cond_expects(&clause);
                    }
                    MCAST_ATTRIBUTE | MCAST_ATTRIBUTE_ADD => {
                        // `layout = [ ... ]` — boundary-port placement for this
                        // module when it is instantiated as a child box. Any
                        // other attribute in a module body stays an unexpected
                        // clause (E3081), as before.
                        if let Some(layout) = McLayout::new(&clause) {
                            self.layout = layout;
                        } else if let Some(ledger) = Ledger::new(&clause) {
                            // `expects = [ ... ]` — rows append so repeated
                            // clauses accumulate.
                            self.expects.rows.extend(ledger.rows);
                        } else {
                            dlog_error(
                                crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                                &clause,
                                &crate::errcodes::format_msg(
                                    crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                                    &[&"this attribute row is not accepted in a module body"],
                                ),
                            );
                        }
                    }
                    MCAST_ATTRIBUTE_PIN | MCAST_ATTRIBUTE_PINADD => {
                        dlog_error(
                            crate::errcodes::MODULE_PINS_UNSUPPORTED,
                            &clause,
                            &crate::errcodes::format_msg(
                                crate::errcodes::MODULE_PINS_UNSUPPORTED,
                                &[],
                            ),
                        );
                    }
                    _ => {
                        dlog_error(
                            crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                            &clause,
                            &crate::errcodes::format_msg(
                                crate::errcodes::UNEXPECTED_CLAUSE_TYPE,
                                &[&"this clause type is not accepted in a module body"],
                            ),
                        );
                    }
                }
            }

            // ★ Smart Param (M5): Check both formal params and body ports.
            let mod_name = self.name.to_string();
            let diags = self.params.finalize(Some(body), &mod_name);
            let mut warned: std::collections::HashSet<String> =
                diags.iter().map(|d| d.param_name.clone()).collect();
            // finalize names params by their declared form (e.g. "GPIO[1:2]",
            // "DC1{VDD, GND}", "[VDD1, GND1]") while the instance table below
            // uses normalized keys ("GPIO1", "DC1", "@3"); fold every warned
            // declare's name forms into the set so the sweep below does not
            // re-report the same port (E5641 + E5642 duplicates).
            for declare in self.params.iter() {
                if warned.contains(&declare.display_name()) {
                    warned.extend(declare.all_name_forms());
                }
            }
            for d in diags {
                crate::mcc_log_global_diag(&d);
            }
            for port_name in self.insts.iter_port_names() {
                let all_forms = self.insts.all_name_forms_for(port_name);
                if all_forms.iter().any(|form| warned.contains(form)) {
                    continue;
                }
                let mut span = self
                    .insts
                    .port_spans()
                    .get(port_name)
                    .and_then(|s| s.first().cloned());
                // T7 (G8): never borrow a sibling's span. When an IDX/list
                // member key has no span of its own, derive the span from its
                // declaring structure — the structural key whose name forms
                // cover this port (e.g. member `GPIO1` -> list `GPIO[1:2]`),
                // then the formal-parameter declaration as a last resort.
                if span.is_none() || span.as_ref().is_some_and(|s| s.is_empty()) {
                    span = self
                        .insts
                        .iter_instance_names()
                        .filter(|k| *k != port_name)
                        .find(|k| {
                            self.insts
                                .all_name_forms_for(k)
                                .contains(&port_name.to_string())
                        })
                        .and_then(|k| {
                            self.insts
                                .port_spans()
                                .get(k)
                                .and_then(|v| v.first())
                                .cloned()
                        });
                }
                if span.is_none() {
                    span = self
                        .params
                        .find(port_name)
                        .and_then(|declare| self.params.get_def_span(&declare.display_name()));
                }
                let span = span.unwrap_or(0..1);
                let has_recorded_ref = self
                    .insts
                    .iter_net_refs()
                    .any(|(_, name, _)| all_forms.iter().any(|form| form == name));
                let has_ast_usage = all_forms.iter().any(|form| {
                    crate::semantic::basic::mc_param_infer::collect_usages(form, body)
                        .iter()
                        .any(|usage| usage.pos != span.start)
                });
                if !has_recorded_ref && !has_ast_usage {
                    crate::db::diagnostic::diagnostic::diagnostic_log(
                        crate::errcodes::PORT_NEVER_USED,
                        crate::db::diagnostic::diagnostic::DiagnosticLevel::Warning,
                        span.start as u32,
                        (span.end - span.start) as u32,
                        &crate::errcodes::format_msg(
                            crate::errcodes::PORT_NEVER_USED,
                            &[&port_name, &mod_name],
                        ),
                        &[],
                    );
                }
            }

            // ★ Inline labels: register bare names referenced in net stmts that
            // are not ports/params/instances as Inline labels, so `show
            // instances` lists them (e.g. `GND` in `... -> GND`).
            let mut net_labels: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for stmt in &self.stmts {
                crate::semantic::validation::body::collect_net_label_names(stmt, &mut net_labels);
            }
            for name in net_labels {
                if !Self::is_plain_label_candidate(&name) {
                    continue;
                }
                if self.insts.contains(&name) || self.params.contains(&name) {
                    continue;
                }
                self.insts
                    .create_inst(&name, McInstance::Label(name.clone()));
                self.insts
                    .set_label_kind(&name, crate::semantic::mc_inst::LabelKind::Inline);
            }
        }

        // resolve-gate §1.6 ①: drain the body-parse floating candidates
        // (`report_floating_label` pushed into `floating_pending`) into the
        // PostParse-visible set consumed by `validation::floating`.
        self.floating_candidates = std::mem::take(&mut *self.floating_pending.lock().unwrap());
    }

    /// ★ LSP: Record net refs for instance-declaration constructor arguments
    /// (`speaker(V3V3)`, `mcu(V3V3, V1V2)`). Instance declarations only walk
    /// `insts.parse` → `parse_declare`, which binds ctor args but never records
    /// lapper refs — unlike MCAST_NET operands (`collect_net_refs_in_node`).
    /// Route each `MCAST_PARAM` under every `MCAST_INSTANCE` through the same
    /// collector so argument identifiers get a LabelRef/PortRef and F12 finds
    /// their def (consistent with the MCAST_NET path).
    fn collect_declare_ctor_refs(&mut self, clause: &AstNode) {
        let scope = self.name.to_string();
        let Some(sub) = clause.get_sub_node() else {
            return;
        };
        for child in sub.iter() {
            if child.get_type() != MCAST_INSTANCE {
                continue;
            }
            // The ctor PARAMS node is the next sibling of the instance id node
            // (or of the instance node when the id has no sub) — mirror
            // collect_ctor_params in mc_inst.rs.
            let inst_id = child.get_sub_node().unwrap_or_else(|| child.clone());
            for cand in [inst_id.get_next(), child.get_next()] {
                let Some(n) = cand else { continue };
                if n.get_type() != MCAST_PARAMS {
                    continue;
                }
                if let Some(psub) = n.get_sub_node() {
                    for p in psub.iter() {
                        if p.get_type() == MCAST_PARAM {
                            Self::collect_net_refs_in_node(
                                &p,
                                &mut self.insts,
                                &mut self.params,
                                &scope,
                            );
                        }
                    }
                }
                break;
            }
        }
    }

    /// A bare identifier eligible to become an inline net label: no member
    /// separators (`.`/`{`), not an anonymous or bracketed name, not a
    /// reserved keyword.
    fn is_plain_label_candidate(name: &str) -> bool {
        if name.is_empty()
            || name == "this"
            || name == "lead"
            || name.starts_with('@')
            || name.starts_with('[')
            || name.starts_with('(')
            || name.contains('.')
            || name.contains('{')
            || name.contains('(')
            || name.contains(',')
            || name.contains(char::is_whitespace)
        {
            return false;
        }
        true
    }
    /// Extract the interface class name and its source span from an
    /// interface-typed module port parameter, e.g. `[VDD_3V3,GND]::DC(3.3V)`
    /// → (`McIds(DC)`, <span of "DC">). Mirrors `McParamType::classify_declare`.
    /// The `McIds` is taken directly from the AST node so the multi-segment
    /// structure is preserved for downstream registration / resolution.
    pub(crate) fn extract_declare_class_span(
        node: &AstNode,
    ) -> Option<(McIds, std::ops::Range<usize>)> {
        let first_child = node.get_sub_node()?;
        for child in first_child.iter() {
            if child.get_type() != MCAST_CLASS {
                continue;
            }
            let Some(name_node) = child.get_sub_node() else {
                continue;
            };
            // The class-name IDS can over-span the real name when the declare
            // carries ctor args in the func-header grammar (`::DC(3.3V)` yields
            // an IDS covering `DC(3.3V)` with only `DC` as a child), so compute
            // the span from the name-constituent children (ID/IDA/dot members)
            // and skip the ctor-arg container (MCAST_PARAMS). Falls back to the
            // IDS node's own span for leaf IDS nodes (plain names).
            let mut span_start: Option<Position> = None;
            let mut span_end: Option<Position> = None;
            let mut cur = name_node.get_sub_node();
            while let Some(n) = cur {
                if n.get_type() != MCAST_PARAMS {
                    if span_start.is_none() {
                        span_start = Some(n.get_pos());
                    }
                    span_end = Some(n.get_pos() + n.get_len());
                }
                cur = n.get_next();
            }
            let span = match (span_start, span_end) {
                (Some(s), Some(e)) => (s as usize)..(e as usize),
                _ => {
                    (name_node.get_pos() as usize)
                        ..((name_node.get_pos() + name_node.get_len()) as usize)
                }
            };
            if let Some(ids) = McIds::new(&name_node) {
                return Some((ids, span));
            }
        }
        None
    }

    /// ★ LSP: register a BusDef for curly interface module params such as
    /// `dc{VDD_3V3, GND}::DC(3.3V)`. The whole span covers the base identifier
    /// and each member span points at the member text, so member refs
    /// (`dc.GND`, `dc.VDD_3V3`) resolve to the declaration in THIS file via
    /// `bus_member_hit` rather than a first-use site span.
    fn register_curly_param_bus_def(node: &AstNode, insts: &mut McInstances) {
        // MCAST_DECLARE → MCAST_INSTANCE → MCAST_OPD → MCAST_IDS[base, opd_curly[...]]
        let Some(sub) = node.get_sub_node() else {
            return;
        };
        let mut cur = sub;
        let ids_node = loop {
            if cur.get_type() == MCAST_INSTANCE {
                let mut inner = cur.get_sub_node();
                let ids = loop {
                    match inner {
                        Some(n) if n.get_type() == MCAST_IDS => break n,
                        Some(n) => inner = n.get_sub_node(),
                        None => return,
                    }
                };
                break ids;
            }
            match cur.get_next() {
                Some(nx) => cur = nx,
                None => return,
            }
        };
        let Some((busname, members)) = McIds::new(&ids_node).and_then(|ids| ids.as_bus()) else {
            return;
        };
        if members.is_empty() {
            return;
        }
        let whole_span = ids_node
            .get_sub_node()
            .filter(|n| n.get_type() == MCAST_ID)
            .map(|n| {
                let p = n.get_pos() as usize;
                p..(p + busname.len())
            })
            .unwrap_or_else(|| {
                let p = ids_node.get_pos() as usize;
                p..(p + busname.len())
            });
        let mut member_spans: Vec<(String, std::ops::Range<usize>)> = Vec::new();
        let mut mcur = ids_node.get_sub_node();
        while let Some(child) = mcur {
            if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                let mut mc = child.get_sub_node();
                while let Some(m) = mc {
                    if let Some(mname) = m.to_string() {
                        let mstart = m.get_pos() as usize;
                        let mlen = mname.len();
                        member_spans.push((mname, mstart..(mstart + mlen)));
                    }
                    mc = m.get_next();
                }
            }
            mcur = child.get_next();
        }
        if !member_spans.is_empty() {
            tracing::info!(target: "mcc::lsp::audit",
                "[AUDIT-ParamBusDef] bus={busname} span={whole_span:?} members={member_spans:?}");
            insts.register_bus_def(&busname, whole_span, member_spans);
        }
        // Also register the actual bus instance with the FULL member set. Without
        // it the first member ref in a net line (`dc.GND`) auto-creates a
        // single-member bus via `McPhrase::add_bus`, so `show` lists only the
        // referenced member instead of the declared `dc{VDD_3V3, GND}`.
        insts.create_inst(
            &busname,
            McInstance::Bus(McBus::new_with_members(&busname, members)),
        );
    }

    pub(crate) fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.insts.get(id).cloned()
    }

    /// Add label to symbol table
    /// If instance exists, return reference to existing instance
    /// If not found, check members in anonymous List/Bus
    pub(crate) fn add_label(&mut self, name: String) -> McPhrase {
        if let Some(existing_inst) = self.insts.get(&name) {
            return McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                existing_inst.clone(),
            )));
        }
        if let Some(member_ref) = self.find_member_in_anon_insts(&name) {
            return member_ref;
        }
        self.insts
            .create_inst(&name, McInstance::Label(name.clone()));
        self.insts
            .set_label_kind(&name, crate::semantic::mc_inst::LabelKind::Inline);
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(McInstance::Label(
            name,
        ))))
    }

    /// Find member in anonymous List/Bus/Interface
    /// Anonymous instance: name starts with @, or [member1, member2] format (no total name)
    fn find_member_in_anon_insts(&self, member_name: &str) -> Option<McPhrase> {
        for (inst_name, inst) in self.insts.iter() {
            let is_anon = inst_name.starts_with('@')
                || (inst_name.starts_with('[') && inst_name.contains(','));
            if !is_anon {
                continue;
            }
            match inst {
                McInstance::List(list) => {
                    if list.member.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                McInstance::Bus(bus) => {
                    if bus.full_members.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                McInstance::Interface(iface) => {
                    if iface.base.pins.names_to_id.contains_key(member_name) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                    let iface_members = iface.name.expand();
                    if iface_members.len() > 1 && iface_members.contains(&member_name.to_string()) {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Label(member_name.to_string()),
                        ))));
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Add component instance to symbol table
    pub(crate) fn add_component(&mut self, name: String, comp: Mc2Component) -> McPhrase {
        let inst = McInstance::Component(Arc::new(comp));
        // ── P2-10: anonymous components (names starting with @) are created
        // inline in connection stmts. They must NOT be stored in insts,
        // otherwise instantiate_declarations_resilient will create them as
        // declarations with no connections, duplicating the stmt-created ones.
        if !name.starts_with('@') {
            self.insts.create_inst(&name, inst.clone());
        }
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Add module instance to symbol table
    pub(crate) fn add_module(&mut self, name: String, module: Mc2Module) -> McPhrase {
        let inst = McInstance::Module(Arc::new(module));
        self.insts.create_inst(&name, inst.clone());
        McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(inst)))
    }

    /// Get all input ports' McBus
    pub fn get_input_elements(&self) -> Vec<McBus> {
        self.insts
            .get_all_inputs()
            .iter()
            .map(|p| p.to_node_element())
            .collect()
    }

    /// Get all output ports' McBus
    pub fn get_output_elements(&self) -> Vec<McBus> {
        self.insts
            .get_all_outputs()
            .iter()
            .map(|p| p.to_node_element())
            .collect()
    }

    /// Source span of the declaration that introduced port `name`.
    ///
    /// Two declaration shapes introduce a port: a signature / body item
    /// (`insts`, the common case) and an interface-typed signature parameter
    /// that the item loop never materialized (`params`). Both readers live
    /// here so callers agree on where a port was declared.
    pub(crate) fn port_decl_span(&self, name: &str) -> Option<std::ops::Range<usize>> {
        self.insts
            .get_port_span(name)
            .or_else(|| self.params.get_def_span(name))
    }
}

impl HasFindInst for McModule {
    fn find_inst(&self, id: &str) -> Option<McInstance> {
        self.find_inst_with_span(id).map(|(inst, _)| inst)
    }

    // ── resolve-gate §1.3 entry gate: module-level discriminator ──
    fn is_declared_instance_name(&self, base: &str) -> bool {
        if self.seen_callers.iter().any(|s| s == base) {
            return true;
        }
        self.find_inst(base).is_some()
    }

    fn note_func_call_caller(&mut self, name: &str) {
        if !self.seen_callers.iter().any(|s| s == name) {
            self.seen_callers.push(name.to_string());
        }
    }

    // resolve-gate §1.6 ①: module top-level body bare misses register as
    // floating-label candidates (drained into `floating_candidates` at the end
    // of `parse_body`; consumed by validation::floating → E3136). Without this,
    // module-side port spelling errors were 0-diagnostic.
    fn report_floating_label(&self, name: &str, node: &AstNode) {
        self.floating_pending.lock().unwrap().push((
            name.to_string(),
            node.get_pos(),
            node.get_len(),
        ));
    }

    fn register_gate_candidate(&mut self, base: &str, form: &str, pos: u32, len: u32) {
        self.gate_candidates.push(GateCandidate {
            base: base.to_string(),
            form: form.to_string(),
            pos,
            len,
        });
    }

    fn find_inst_mut(&mut self, id: &str) -> Option<&mut crate::McInstance> {
        self.insts.get_mut(id)
    }

    fn get_vector_members(&self, base: &str) -> Option<Vec<String>> {
        self.insts
            .get_vector_members(base)
            .map(|members| members.to_vec())
    }

    fn find_inst_with_span(
        &self,
        id: &str,
    ) -> Option<(McInstance, Option<std::ops::Range<usize>>)> {
        // P2 container category chain (§3.3): param ports → param defs →
        // ports → labels → non-port insts (uniform Bus/List/Interface/
        // Component coverage) → funcs. Each category is an independent scope
        // unit in semantic::scope with the same hit logic (and stored spans)
        // as the original hand-written chain it replaced.
        crate::semantic::scope::module_scope(self)
            .resolve(id)
            .map(|r| (r.inst, r.span))
    }

    fn is_declared_port(&self, name: &str) -> bool {
        // A declared port carries a concrete IOType (`io` / `in` / `out`);
        // internal labels and params are registered with IOType::None.
        self.insts
            .get_with_iotype(name)
            .is_some_and(|(io, _)| !matches!(io, IOType::None))
    }

    fn declared_port_members(&self, base: &str) -> Option<Vec<String>> {
        // The declaration is authoritative: the module port's member set is
        // fixed at the `io`/`in`/`out` declaration site and must never be
        // widened by body usage. Only member-capable port directions
        // (io/in/out) carry a shape; `psnk`/`analog`/`label`/component/module
        // instances and internal nets (IOType::None) are usage-defined and are
        // not gated here.
        let (io, inst) = self.insts.get_with_iotype(base)?;
        if !matches!(io, IOType::In | IOType::Out | IOType::InOut) {
            return None;
        }
        match inst {
            McInstance::Label(_) => Some(Vec::new()),
            McInstance::Bus(b) => Some(b.member.clone()),
            McInstance::List(l) => Some(l.member.clone()),
            _ => None,
        }
    }

    fn interface_param_members(&self, name: &str) -> Option<Vec<String>> {
        // Interface-class module params (e.g. `psnk dc{VDD_3V3, GND}::DC(3.3V)`)
        // are routed by parse_params into the param table only — never into
        // insts — so a bare reference falls to the 1*1 label fallback in the
        // Pass1 opcheck. Present the declared member width instead, matching
        // Pass2's expand_port_lanes upgrade for the same bare reference.
        self.params.iter().find_map(|d| {
            if !d.param_type.is_port() {
                return None;
            }
            match &d.kind {
                crate::semantic::basic::mc_param::McParamDeclareKind::Single(ids) => {
                    let (base, members) = ids.as_bus()?;
                    (base == name && members.len() >= 2).then_some(members)
                }
                _ => None,
            }
        })
    }

    fn add_label_at(
        &mut self,
        name: String,
        span: Option<std::ops::Range<usize>>,
    ) -> Option<McPhrase> {
        if let Some(s) = span {
            self.insts.store_port_span(&name, s);
        }
        Some(self.add_label(name))
    }

    fn add_bus(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        // An inlined ghost-bus (resolve-gate relax-everything) is a statement-tree net
        // node, NOT a declaration — never register it into `insts`, or the
        // finish recheck (gate.rs `base_declared_by_finish`) would mistake the
        // base for a late-declared instance and skip E3137. Net joining in
        // pass2 is driven by the bus name in the statement tree.
        let bus = McBus::new_with_members(&name, members);
        let inst = McInstance::Bus(bus);
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_list(&mut self, name: String, members: Vec<String>) -> Option<McPhrase> {
        let list = McList::new_with_members(&name, members);
        let inst = McInstance::List(list);
        self.insts.create_inst(&name, inst.clone());
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            inst,
        ))))
    }

    fn add_bus_member(&mut self, base: &str, member: String) -> Option<McPhrase> {
        // A declared module port (`io`/`in`/`out`) is authoritative — never
        // widen it here (that would be the banned usage auto-expansion).
        // Diagnostics (E3183 for scalar, E3181 for an undeclared member) are
        // emitted by the caller gate (`enforce_declared_port_shape`); this
        // guard only stops the mutation and returns a plain member-ref lane.
        if self.insts.get_with_iotype(base).is_some_and(|(io, inst)| {
            matches!(io, IOType::In | IOType::Out | IOType::InOut)
                && matches!(
                    inst,
                    McInstance::Label(_) | McInstance::Bus(_) | McInstance::List(_)
                )
        }) {
            let member_ref = McBus::member_ref(base, member);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(member_ref),
            ))));
        }

        let is_component_with_bus = self
            .insts
            .get(base)
            .map(|inst| {
                if let McInstance::Component(comp) = inst {
                    comp.base.pins.is_bus(&member)
                } else {
                    false
                }
            })
            .unwrap_or(false);

        if is_component_with_bus {
            let full_name = format!("{base}.{member}");
            if !self.insts.contains(&full_name) {
                let members = if let Some(inst) = self.insts.get(base) {
                    if let McInstance::Component(comp) = inst {
                        comp.base.pins.get_bus_members(&member).unwrap_or_default()
                    } else {
                        vec![member.clone()]
                    }
                } else {
                    vec![member.clone()]
                };
                let mut new_bus = McBus::new_with_members(&full_name, members);
                new_bus.add_member(&member);
                self.insts.create_inst(&full_name, McInstance::Bus(new_bus));
            } else if let Some(existing_inst) = self.insts.get_mut(&full_name) {
                if let McInstance::Bus(bus) = existing_inst {
                    if !bus.full_members.iter().any(|m| m == &member) {
                        bus.add_member(&member);
                    }
                }
            }
            let member_ref = McBus::member_ref(&full_name, member);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(member_ref),
            ))));
        }

        if let Some(inst) = self.insts.get_mut(base) {
            if let McInstance::Bus(bus) = inst {
                let fn_base = base.to_string();
                bus.add_member(&member);
                let full_members_clone = bus.full_members.clone();
                if !self.insts.contains(&fn_base) {
                    let bus_to_add = McBus::new_with_members(&fn_base, full_members_clone);
                    self.insts
                        .create_inst(&fn_base, McInstance::Bus(bus_to_add));
                }
                let member_ref = McBus::member_ref(&fn_base, member);
                return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                    McInstance::Bus(member_ref),
                ))));
            }
        }

        let bus = McBus::new_with_members(base, vec![member.clone()]);
        let inst = McInstance::Bus(bus);
        self.insts.create_inst(base, inst.clone());
        let member_ref = McBus::member_ref(base, member);
        Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
            McInstance::Bus(member_ref),
        ))))
    }

    fn add_interface_member(
        &mut self,
        component: &str,
        interface: &str,
        members: Vec<String>,
    ) -> Option<McPhrase> {
        let full_name = format!("{component}.{interface}");
        if let Some(comp_inst) = self.insts.get(component) {
            if let McInstance::Component(comp) = comp_inst {
                if comp.base.pins.is_interface(interface) {
                    let iface_ref = McBus::new_with_members(&full_name, members);
                    return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                        McInstance::Bus(iface_ref),
                    ))));
                }
            }
        }
        if let Some(McCMIE::Interface(_)) = resolve_cmie(&DB, &McIds::from("ADC.DIFF"), self.uri())
        {
            let iface_ref = McBus::new_with_members(&full_name, members);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(iface_ref),
            ))));
        }
        if let Some(McCMIE::Interface(_)) = resolve_cmie(
            &DB,
            &McIds::from(&format!("{component}.{interface}") as &str),
            self.uri(),
        ) {
            let iface_ref = McBus::new_with_members(&full_name, members);
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(iface_ref),
            ))));
        }
        None
    }

    fn check_bus_member(&mut self, base: &str, member: &str) -> Option<(String, String)> {
        if let Some(inst) = self.insts.get(base) {
            if let McInstance::Component(comp) = inst {
                if comp.base.pins.is_bus(member) {
                    return Some((format!("{base}.{member}"), member.to_string()));
                }
            }
        }
        None
    }

    fn is_component_bus(&self, base: &str, member: &str) -> bool {
        if let Some(inst) = self.insts.get(base) {
            if let McInstance::Component(comp) = inst {
                return comp.base.pins.is_bus(member);
            }
        }
        false
    }

    fn uri(&self) -> &McURI {
        &self.uri
    }

    fn parse_declare(&mut self, node: &AstNode) -> Vec<McInstance> {
        let before: Vec<String> = self.insts.get_all_names();
        self.insts.parse(node, &self.uri);
        // Collect newly created instances to return to callers (mc_phrase.rs, mc_fcall.rs)
        self.insts
            .get_all_names()
            .into_iter()
            .filter(|k| !before.contains(k))
            .filter_map(|k| self.insts.get(&k).cloned())
            .collect()
    }

    fn add_component(
        &mut self,
        name: String,
        comp: crate::semantic::component::Mc2Component,
    ) -> Option<McPhrase> {
        Some(self.add_component(name, comp))
    }

    fn add_module(
        &mut self,
        name: String,
        module: crate::semantic::module::Mc2Module,
    ) -> Option<McPhrase> {
        Some(self.add_module(name, module))
    }

    /// Generate an anonymous instance name: `@{classname}{counter}` (e.g. `@RES1`, `@CAP2`).
    ///
    /// # Design rule
    /// Anonymous instances are created inline in connection statements
    /// (e.g. `-> RES(10kΩ) ->`). Their declaration position **is** their usage
    /// position — they exist solely as part of a connection chain and do not
    /// need to be referenced from elsewhere.
    ///
    /// Diagnostics that check for "unused" ports/instances must skip names
    /// produced by this function. See [`McInstances::iter_port_names`].
    fn gen_anon_name(&mut self, classname: &str) -> String {
        let name = format!("@{}{}", classname, self.anon_counter);
        self.anon_counter += 1;
        name
    }

    fn store_inst_span(&mut self, name: &str, span: std::ops::Range<usize>) {
        self.insts.store_port_span(name, span);
    }

    fn record_declareb_def(
        &mut self,
        name: &str,
        kind: crate::refdef::types::SymbolKind,
        span: std::ops::Range<usize>,
    ) {
        self.insts.record_declareb_def(name, kind, span);
    }

    fn upgrade_label_to_bus(&mut self, name: &str) -> bool {
        // A declared scalar port (`io X`) is shape-locked — never promote it to
        // a Bus here (that is the banned usage auto-expansion). Internal labels
        // (IOType::None) keep the old shape-by-use upgrade.
        if self
            .insts
            .get_with_iotype(name)
            .is_some_and(|(io, _)| matches!(io, IOType::In | IOType::Out | IOType::InOut))
        {
            return false;
        }
        if let Some(inst) = self.insts.get_mut(name) {
            if matches!(inst, McInstance::Label(_)) {
                let new_bus = McBus::new(name);
                *inst = McInstance::Bus(new_bus);
                return true;
            }
        }
        false
    }

    fn find_func_return(&self, name: &str) -> Option<McFuncReturn> {
        self.funcs.find(name).map(|f| f.returns.clone())
    }

    fn domain_pair_named(&self, name: &str) -> Option<pi::L1DomainPair> {
        // The peek, not `self.pi`: the body walk fills `pi` in source order,
        // so reading it here would make a name written above its `domain`
        // clause miss silently (guard ③). The two agree once the walk ends.
        self.domain_pairs_peek
            .iter()
            .find(|p| p.domain == name)
            .cloned()
    }

    fn declared_endpoint_named(&self, name: &str) -> bool {
        // Ports, instances, labels/nets, buses, lists and vector-group members
        // are all instance-table entries, so one scope-chain walk covers them.
        // A `conduit` (the `MCAST_REF` clause, keyword alias `ref`) is *not* a
        // name-resolution entry — it is a copper-identity declaration read by
        // the ERC face, never by `find_inst` — so it is asked here by hand.
        // Without that second half a name that is both a conduit and a
        // whole-referenceable domain would be widened into the pair with no
        // diagnostic: the conduit reading would be lost in silence.
        self.insts.get(name).is_some() || self.pi.refs.iter().any(|r| r.name == name)
    }

    fn scope_name(&self) -> Option<String> {
        Some(self.name.to_string())
    }

    fn licensed_domain_member_at(&self, pos: usize) -> Option<String> {
        self.licensed_members.get(&pos).cloned()
    }
}

/// One bare identifier word of a connection statement's phrase tree, with the
/// connection direction of the nearest enclosing arrow: `Some(true)` under
/// `->` (hot take), `Some(false)` under `<-` (return take), `None` under a
/// non-arrow chain (`-` / `+`) or no chain. The recorded position is the exact
/// node the widening write point queries — `McPhrase::new`'s bare-`McOpd::Id`
/// arm reads `McOpd::new` off the same subnode — so the pre-scan's keys and
/// the write point's lookups cannot drift.
struct DomainBridgeWord {
    pos: usize,
    name: String,
    dir: Option<bool>,
}

/// Walk a phrase subtree collecting its bare identifier words (`R3` pre-scan
/// input, [`McModule::scan_domain_bridges`]). Arrow nodes override the
/// inherited direction for their whole subtree (a chain has one direction);
/// every other node passes it through. Names render through `McIds`' `Display`
/// — the whole written word — so a dotted / bracketed spelling simply never
/// equals a domain name: §10.11.4 guard ① holds by word identity here too.
fn collect_domain_bridge_words(node: &AstNode, dir: Option<bool>, out: &mut Vec<DomainBridgeWord>) {
    let dir = match node.get_type() {
        MCAST_OPD_RIGHTARROW => Some(true),
        MCAST_OPD_LEFTARROW => Some(false),
        _ => dir,
    };
    if node.is_type(MCAST_OPD) {
        if let Some(sub) = node.get_sub_node() {
            if let Some(crate::semantic::basic::mc_opd::McOpd::Id(ids)) =
                crate::semantic::basic::mc_opd::McOpd::new(&sub)
            {
                out.push(DomainBridgeWord {
                    pos: sub.get_pos() as usize,
                    name: ids.to_string(),
                    dir,
                });
            }
        }
    }
    let mut kid = node.get_sub_node();
    while let Some(k) = kid {
        collect_domain_bridge_words(&k, dir, out);
        kid = k.get_next();
    }
}

/// Report one domain-bridge code at a statement, idempotent per position like
/// every span-anchored fact in this tree (repeated parse runs must not
/// multiply it).
fn report_domain_bridge_code(code: u32, clause: &AstNode, args: &[&dyn std::fmt::Display]) {
    let uri = crate::current_uri::get();
    let pos = clause.get_pos();
    if crate::db::diagnostic::diagnostic::has_code_at(code, &uri, pos) {
        return;
    }
    dlog_error(code, clause, &crate::errcodes::format_msg(code, args));
}

impl McModule {
    /// Recursively scan AST nodes in a net expression for identifiers that match
    /// known port names (both from body insts and params), and record their spans for LSP
    /// goto-definition.
    /// Walk AST nodes in a net phrase and store definition spans + LSP lapper
    /// entries for any identifier that becomes an inline port instance.
    fn collect_net_def_spans(node: &AstNode, insts: &mut McInstances, uri: &McURI, scope: &str) {
        match node.get_type() {
            MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC
            | MCAST_OPD_CURLY => {
                if let Some(text) = node.to_string() {
                    // ★ §3.4.3 (rev) check-before-register: member chains
                    // (`USB_VBUS_1.GND`, `dc.VDD_3V3`) are REFS to members of an
                    // already-declared bus; the member defs were registered at the
                    // declaration site (module param / io line) via register_bus_def
                    // → BusMemberDef. Skipping them here prevents the whole-chain
                    // span from being stored as the base bus's port span — which
                    // would register spurious BusDef/LabelDef at the use site and
                    // make F12 on the member self-locate (def == ref span).
                    //
                    // Two shapes slip through a plain dotted-text check:
                    //   - the chain node itself (`USB_VBUS_1.GND`); and
                    //   - its first MCAST_ID segment (`USB_VBUS_1`), whose
                    //     `get_len()` is extended to the whole chain by
                    //     mc_value_link (§5.1: never trust get_len() for ids
                    //     chains) while `to_string()` returns only the segment.
                    // `node_len > text.len()` detects the latter.
                    let node_len = node.get_len() as usize;
                    let is_member_chain = text.contains('.') || node_len > text.len();
                    if is_member_chain {
                        // member-chain ref: def already exists at declaration site
                    } else {
                        let start = node.get_pos() as usize;
                        let span = start..(start + node_len);
                        let key = insts.resolve_idx(&text).unwrap_or(text);
                        if insts.get(&key).is_some() && insts.port_spans().get(&key).is_none() {
                            insts.store_port_span(&key, span.clone());
                            // Register in name_to_declare_id so goto-def can find this inline port
                            if let Some(mcode) = crate::db::cmie::tables::WORKSPACE.mcodes.get(uri)
                            {
                                if let Ok(mut sem) = mcode.symbols.lock() {
                                    // The location must name this file (global
                                    // `UriId`, CIMP U81 ①): a zero file_id would
                                    // alias the inline port with every other
                                    // file's inline port of the same key.
                                    let loc = crate::ast::sem::SourceLocation {
                                        file_id: crate::refdef::types::intern_uri(uri.as_str()),
                                        ..crate::ast::sem::SourceLocation::from_span(&span)
                                    };
                                    sem.local_table.add_declare_with_name(
                                        loc,
                                        &key,
                                        scope,
                                        SymbolKind::PortDef,
                                        crate::ScopePath::module(uri, scope).priority(),
                                    );
                                }
                            }
                        } // end else (non-member-chain def registration)
                    }
                }
            }
            _ => {}
        }
        if let Some(sub) = node.get_sub_node() {
            let mut cur = sub;
            loop {
                Self::collect_net_def_spans(&cur, insts, uri, scope);
                match cur.get_next() {
                    Some(next) => cur = next,
                    None => break,
                }
            }
        }
    }

    pub(crate) fn collect_net_refs_in_node(
        node: &AstNode,
        insts: &mut McInstances,
        params: &mut McParamDeclares,
        scope: &str,
    ) {
        let handled = match node.get_type() {
            MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN => {
                Self::record_scoped_net_ref(node, insts, params, scope);
                true
            }
            // ★ SQUARE_VEC / OPD_SQUARE_VEC (e.g. [VDD_3V3,GND]):
            //   text starts with `[` so split-by-`[` gives empty base.
            //   Iterate members and look up each individually — matching how
            //   McParamDeclares::parse stores them as individual keys in def_spans.
            MCAST_SQUARE_VEC | MCAST_OPD_SQUARE_VEC => {
                tracing::info!(target: "mcc::lsp",
                    "SQUARE_VEC_REF node_type={} pos={} len={}",
                    node.get_type(),
                    node.get_pos(),
                    node.get_len()
                );
                let mut current = node.get_sub_node();
                while let Some(phrase_node) = current {
                    let ids_node = phrase_node
                        .get_sub_node()
                        .unwrap_or_else(|| phrase_node.clone());
                    let mut handled = false;
                    if let Some(ids) = crate::semantic::basic::mc_ids::McIds::new(&ids_node) {
                        let name = ids.to_string();
                        let member_span = (ids_node.get_pos() as usize)
                            ..((ids_node.get_pos() + ids_node.get_len()) as usize);
                        // ★ Dot-chain members (`dc.VDD_3V3`, `lpa.IN.N`) must
                        // NOT be folded into their base key here — resolve_idx
                        // would map them to `dc`/`lpa` and lose the member
                        // context. Route them through the chain path below.
                        let is_chain = name.contains('.');
                        if !is_chain {
                            // Resolve the member to its owning port (e.g. `VDD_3V3`
                            // inside `[VDD_3V3, GND]::DC(3.3V)` maps to the bracket
                            // key) so usage of bracket members counts as usage of
                            // the whole bracket port.
                            let matched_key = insts.resolve_idx(&name);
                            let in_params = params.is_defined(&name);
                            tracing::info!(
                                "SQUARE_VEC_REF member='{name}' span=[{},{}] key={:?} in_params={in_params} scope='{scope}'",
                                member_span.start, member_span.end, matched_key
                            );
                            if let Some(key) = matched_key {
                                insts.record_net_ref(member_span, &key, scope);
                                handled = true;
                            } else if in_params {
                                params.record_net_ref(member_span, &name, scope);
                                handled = true;
                            }
                        }
                    }
                    if !handled {
                        // ★ Unmatched / chain members (e.g. `dc.VDD_3V3`,
                        // `RES(..) -> (lpa.VO1 + spk.N)`) must recurse instead:
                        // without it they are dropped entirely and never reach
                        // the chain path (has_dot_chain → try_record_chain_ref).
                        // Walk next-siblings too: a member may
                        // be a full connection (`dc.VDD_3V3 -> wm7121.VCC`)
                        // whose arrow's right operand hangs off the left
                        // operand's get_next() — without the walk it is
                        // swallowed and never registered as a pin ref.
                        let mut cur = Some(ids_node.clone());
                        while let Some(c) = cur {
                            Self::collect_net_refs_in_node(&c, insts, params, scope);
                            cur = c.get_next();
                        }
                    }
                    current = phrase_node.get_next();
                }
                true
            }
            MCAST_OPD => {
                // If this OPD contains dot separators between identifiers
                // (e.g., `uC.i2c(0x36).I2C0`), return false so that
                // try_record_chain_ref handles it with AST-structured segments
                // instead of falling through to simple name recording.
                if Self::has_dot_chain(node) {
                    false
                } else if let Some(sub) = node.get_sub_node() {
                    let inner_type = sub.get_type();
                    if matches!(
                        inner_type,
                        MCAST_ID | MCAST_IDA | MCAST_IDS | MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN
                    ) {
                        Self::record_scoped_net_ref(&sub, insts, params, scope);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        };

        if !handled {
            // ★ Chain detection: an MCAST_OPD / MCAST_OPD_DOT with dotted
            // segments like `uC.i2c(0x36).I2C0` (root is MCAST_OPD_DOT whose
            // sub is the fcall and next is the member). Record the full chain
            // as a net-ref so the chain resolver can find the cross-container
            // member def (e.g., the MCU pin I2C0::I2C(Master) rather than the
            // local module port I2C0).
            if matches!(node.get_type(), MCAST_OPD | MCAST_OPD_DOT)
                && Self::try_record_chain_ref(node, insts, scope)
            {
                return;
            }
            let Some(sub) = node.get_sub_node() else {
                return;
            };
            let mut current = sub;
            loop {
                Self::collect_net_refs_in_node(&current, insts, params, scope);
                match current.get_next() {
                    Some(next) => current = next,
                    None => break,
                }
            }
        }
    }

    /// Check whether an MCAST_OPD node contains dot-separated identifiers
    /// (i.e., it's a member-chain expression like `uC.i2c(0x36).I2C0`).
    /// Returns `true` if an MCAST_OPD_DOT appears as a flat child, inside a
    /// nested MCAST_OPD (chain-tail operands), or merged into an ID/IDA/IDS
    /// sub_node (`uC.ADC{P,N}` is one MCAST_IDS with a dotted sub_node).
    fn has_dot_chain(node: &AstNode) -> bool {
        let mut current = node.get_sub_node();
        while let Some(n) = current {
            if n.get_type() == MCAST_OPD_DOT {
                return true;
            }
            // Chain-tail operands wrap their children in a nested MCAST_OPD
            // (e.g. `MIC -> uC.ADC{P,N}` — the tail is nested), so recurse.
            if n.get_type() == MCAST_OPD && Self::has_dot_chain(&n) {
                return true;
            }
            // ID/IDA/IDS may merge the dot chain into their sub_node:
            // `uC.ADC{P,N}` is one MCAST_IDS whose sub_node is
            // [MCAST_ID "uC", MCAST_OPD_DOT "ADC", MCAST_OPD_CURLY "P{N}"].
            if matches!(n.get_type(), MCAST_ID | MCAST_IDA | MCAST_IDS) {
                if let Some(sub) = n.get_sub_node() {
                    let mut sc = sub;
                    loop {
                        if sc.get_type() == MCAST_OPD_DOT {
                            return true;
                        }
                        match sc.get_next() {
                            Some(nx) => sc = nx,
                            None => break,
                        }
                    }
                }
            }
            current = n.get_next();
        }
        false
    }

    /// Walk the AST children of a chain expression (MCAST_OPD / MCAST_OPD_DOT)
    /// and extract structured [`ChainSegment`]s. Records the chain via
    /// [`McInstances::record_chain_ref`] so the chain resolver can use the
    /// already-parsed structure instead of re-parsing brackets from raw text.
    /// Returns `true` if the chain was recorded (≥2 segments).
    fn try_record_chain_ref(node: &AstNode, insts: &mut McInstances, scope: &str) -> bool {
        let mut segments: Vec<ChainSegment> = Vec::new();
        let mut chain_end: Option<usize> = None;
        // Whether the last recorded segment came from a bracketed AST node
        // (curly group or fcall). Decided by the AST node type, not by
        // string content — the parser excludes the closing delimiter from
        // node spans, so a curly group always needs one extra byte for `}`.
        let mut closing_delim = false;

        // A chain whose root is MCAST_OPD_DOT has the receiver/fcall as its
        // `sub` and the member as the fcall's `next` (e.g. `uC.i2c(0x36).I2C0`
        // parses as DOT(sub=FCALL(uC.i2c(0x36)), next=IDS(I2C0))). Start the
        // walk at the sub node; the fcall's `next` is traversed as siblings.
        let mut current = node.get_sub_node();
        while let Some(n) = current {
            let ty = n.get_type();

            // Stop at connection operators `->` or `-`.
            if ty == MCAST_OPD_LEFTARROW || ty == MCAST_OPD_MINUS {
                break;
            }

            // Handle DOT member references:
            //   - `.19`  → DOT wraps MCAST_INT    → extract "19"
            //   - `.ADC` → DOT wraps MCAST_IDA/IDS → extract identifier text
            //   - `.ADC{P,N}` → DOT wraps "ADC" + sibling MCAST_OPD_CURLY "P{N}"
            //     (merged dotted form is handled inside collect_ident_segments)
            if ty == MCAST_OPD_DOT {
                if let Some(sub) = n.get_sub_node() {
                    if sub.get_type() == MCAST_INT {
                        if let Some(num) = sub.to_string() {
                            segments.push(ChainSegment::Ident(num));
                            chain_end = Some(sub.get_pos() as usize + sub.get_len() as usize);
                            closing_delim = false;
                        }
                    } else {
                        // IDA / IDS — reuse the ident walker so curly groups
                        // report their closing delimiter from the AST type.
                        Self::collect_ident_segments(
                            &sub,
                            &mut segments,
                            &mut chain_end,
                            &mut closing_delim,
                        );
                    }
                }
                current = n.get_next();
                continue;
            }
            if ty == MCAST_OPD_COLON || ty == MCAST_OPD_DBCOLON {
                current = n.get_next();
                continue;
            }

            if ty == MCAST_ID || ty == MCAST_IDA || ty == MCAST_IDS {
                Self::collect_ident_segments(&n, &mut segments, &mut chain_end, &mut closing_delim);
            } else if ty == MCAST_OPD_FCALL {
                Self::collect_fcall_segments(&n, &mut segments, &mut chain_end, &mut closing_delim);
            } else if ty == MCAST_OPD {
                // Nested MCAST_OPD wrapping the chain — recurse into it.
                Self::walk_chain_children(&n, &mut segments, &mut chain_end, &mut closing_delim);
                break;
            }

            current = n.get_next();
        }

        // Need at least 2 segments for a cross-container chain (e.g., `uC.I2C0`).
        if segments.len() < 2 {
            return false;
        }
        let mut chain_end = match chain_end {
            Some(e) => e,
            None => return false,
        };

        // ★ The parser excludes closing delimiters from AST node spans: the
        // curly node for `uC.ADC{P,N}` covers the members only and the `}`
        // lands right after them; an fcall's `)` lands right after its last
        // argument. When the final segment came from such a node (decided by
        // AST type above), extend the recorded span by one byte so
        // hover/tooltip shows the whole `uC.ADC{P,N}` instead of `uC.ADC{P,N`.
        if closing_delim {
            chain_end += 1;
        }

        let start = node.get_pos() as usize;
        let span = start..chain_end;
        insts.record_chain_ref(span, segments, scope);
        true
    }

    /// Collect chain segments from a `MCAST_INSTANCE` node (the receiver of a
    /// method call, e.g. `uC` in `uC.i2c(0x36)`). The instance wraps an
    /// MCAST_OPD whose sub is the identifier(s), so delegate to the ident
    /// walker.
    fn collect_instance_segments(
        n: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        if let Some(opd) = n.get_sub_node() {
            if let Some(ids) = opd.get_sub_node() {
                Self::collect_ident_segments(&ids, segments, chain_end, closing_delim);
            }
        }
    }

    /// Collect chain segments from an `MCAST_OPD_FCALL` node. A method call
    /// `uC.i2c(0x36)` has children [MCAST_INSTANCE uC, MCAST_NAME i2c,
    /// MCAST_PARAMS 0x36]: push the receiver instance as an Ident segment and
    /// the function name as an Fcall segment (the resolver treats Fcall as a
    /// transparent hop since the function returns `this`).
    fn collect_fcall_segments(
        n: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        let end = n.get_pos() as usize + n.get_len() as usize;
        let mut child = n.get_sub_node();
        while let Some(c) = child {
            match c.get_type() {
                MCAST_INSTANCE => {
                    Self::collect_instance_segments(&c, segments, chain_end, closing_delim);
                }
                MCAST_NAME => {
                    if let Some(name) = c.to_string() {
                        if !name.is_empty() {
                            segments.push(ChainSegment::Fcall(name));
                            *chain_end = Some(end);
                            *closing_delim = true;
                        }
                    }
                }
                _ => {}
            }
            child = c.get_next();
        }
    }

    /// Walk children of a nested MCAST_OPD node, collecting chain segments.
    fn walk_chain_children(
        node: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        let mut current = node.get_sub_node();
        while let Some(n) = current {
            let ty = n.get_type();

            if ty == MCAST_OPD_LEFTARROW || ty == MCAST_OPD_MINUS {
                break;
            }

            // Handle DOT member references:
            //   - `.19`  → DOT wraps MCAST_INT    → extract "19"
            //   - `.ADC` → DOT wraps MCAST_IDA/IDS → extract identifier text
            //   - `.ADC{P,N}` → DOT wraps "ADC" + sibling MCAST_OPD_CURLY "P{N}"
            //     (merged dotted form is handled inside collect_ident_segments)
            if ty == MCAST_OPD_DOT {
                if let Some(sub) = n.get_sub_node() {
                    if sub.get_type() == MCAST_INT {
                        if let Some(num) = sub.to_string() {
                            segments.push(ChainSegment::Ident(num));
                            *chain_end = Some(sub.get_pos() as usize + sub.get_len() as usize);
                            *closing_delim = false;
                        }
                    } else {
                        Self::collect_ident_segments(&sub, segments, chain_end, closing_delim);
                    }
                }
                current = n.get_next();
                continue;
            }
            if ty == MCAST_OPD_COLON || ty == MCAST_OPD_DBCOLON {
                current = n.get_next();
                continue;
            }

            if ty == MCAST_ID || ty == MCAST_IDA || ty == MCAST_IDS {
                Self::collect_ident_segments(&n, segments, chain_end, closing_delim);
            } else if ty == MCAST_OPD_FCALL {
                Self::collect_fcall_segments(&n, segments, chain_end, closing_delim);
            }

            current = n.get_next();
        }
    }

    /// Extract chain segments from an identifier node, handling the merged
    /// dotted form where `uC.ADC{P,N}` is a single MCAST_IDS whose sub_node
    /// is `[MCAST_ID "uC", MCAST_OPD_DOT "ADC", MCAST_OPD_CURLY "P{N}"]`.
    fn collect_ident_segments(
        n: &AstNode,
        segments: &mut Vec<ChainSegment>,
        chain_end: &mut Option<usize>,
        closing_delim: &mut bool,
    ) {
        let end = n.get_pos() as usize + n.get_len() as usize;
        if let Some(sub) = n.get_sub_node() {
            let mut pending_dot: Option<String> = None;
            let mut cur = sub;
            loop {
                let st = cur.get_type();
                if st == MCAST_OPD_DOT {
                    if let Some(t) = cur.to_string() {
                        // ★ Consecutive dots (`lpa.IN.N` → [DOT IN, DOT N]):
                        // flush the previous member first so the middle
                        // segment is not dropped (segments would become
                        // [lpa, N] instead of [lpa, IN, N]).
                        if let Some(prev) = pending_dot.take() {
                            segments.push(ChainSegment::Ident(prev));
                        }
                        pending_dot = Some(t);
                    }
                } else if st == MCAST_OPD_CURLY || st == MCAST_OPD_CURLY_MN {
                    // Combine the pending dot member with the group members:
                    // "ADC" + [P, N] → Group { base: "ADC", members: [P, N] }.
                    let members = Self::collect_curly_members(&cur);
                    let member = pending_dot.take().unwrap_or_default();
                    segments.push(ChainSegment::Group {
                        base: member,
                        members,
                    });
                    // A curly node is a bracketed AST node: its `}` is
                    // excluded from the node span, so the chain needs +1.
                    *closing_delim = true;
                } else if let Some(t) = cur.to_string() {
                    // Base identifier (first child) or other member.
                    segments.push(ChainSegment::Ident(t));
                    *closing_delim = false;
                }
                match cur.get_next() {
                    Some(nx) => cur = nx,
                    None => break,
                }
            }
            // Flush a pending dot member with no following group (e.g. `uC.ADC`).
            if let Some(d) = pending_dot {
                segments.push(ChainSegment::Ident(d));
                *closing_delim = false;
            }
            *chain_end = Some(end);
        } else if let Some(text) = n.to_string() {
            segments.push(ChainSegment::Ident(text));
            *chain_end = Some(end);
            *closing_delim = false;
        }
    }

    /// Collect the member names of a curly group node (`{P,N}` → `["P", "N"]`).
    /// Numeric ranges (`{1:3}`) are expanded to their individual members.
    fn collect_curly_members(curly: &AstNode) -> Vec<String> {
        let mut members: Vec<String> = Vec::new();
        if let Some(sub) = curly.get_sub_node() {
            let mut cur = sub;
            loop {
                if cur.get_type() == MCAST_OPD_COLON {
                    // `{1:3}` range — expand to individual members.
                    if let Some((from, to)) = Self::curly_range(&cur) {
                        for i in from..=to {
                            members.push(i.to_string());
                        }
                    }
                } else if let Some(t) = cur.to_string() {
                    if !t.is_empty() && t != "," {
                        members.push(t);
                    }
                }
                match cur.get_next() {
                    Some(nx) => cur = nx,
                    None => break,
                }
            }
        }
        members
    }

    /// Parse a colon-range child of a curly group (`1:3`) into its bounds.
    fn curly_range(node: &AstNode) -> Option<(i64, i64)> {
        let sub = node.get_sub_node()?;
        let from = sub.to_string()?.parse::<i64>().ok()?;
        let to = sub.get_next()?.to_string()?.parse::<i64>().ok()?;
        (from <= to).then_some((from, to))
    }

    fn record_scoped_net_ref(
        node: &AstNode,
        insts: &mut McInstances,
        params: &mut McParamDeclares,
        scope: &str,
    ) {
        let Some(text) = node.to_string() else {
            return;
        };
        let ids = McIds::new(node);
        let root = ids.as_ref().and_then(McIds::root_name).unwrap_or_else(|| {
            text.split(|c: char| c == '.' || c == '{' || c == '[')
                .next()
                .unwrap_or(&text)
                .to_string()
        });
        if root.is_empty() {
            return;
        }

        let start = node.get_pos() as usize;
        let span = start..(start + root.len().min(node.get_len() as usize));
        let matched_key = if insts.contains(&root) {
            Some(root.clone())
        } else {
            insts
                .resolve_idx(&text)
                .or_else(|| insts.resolve_idx(&root))
        };

        if let Some(key) = matched_key {
            insts.record_net_ref(span, &key, scope);
        } else if params.is_defined(&root) {
            params.record_net_ref(span, &root, scope);
        } else {
            insts.record_net_ref(span, &root, scope);
        }

        // ★ §3.4.3 (rev): per-segment member refs — curly-bus members and
        // dot members get their own refs so F12 lands on the member text.
        // Member node pos is reliable; len is not (mc_value_link extension).
        //
        // Two curly forms are handled:
        //   - `MIC{P,N}`        — McIds parses the whole bus (`as_bus` hits);
        //   - `U_MCU{I2C0.SCL}` — McIds only parses the leading segment, so the
        //     curly child carries the members; base = first segment (`root`).
        if let Some(ids) = ids {
            let base_from_bus = ids.as_bus().map(|(b, _members)| b);
            let has_curly_child = node.get_sub_node().map_or(false, |sub| {
                let mut cur = Some(sub);
                loop {
                    let Some(child) = cur else { break false };
                    if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                        break true;
                    }
                    cur = child.get_next();
                }
            });
            if base_from_bus.is_some() || has_curly_child {
                // Register each curly member as `<base>.<member>` so F12 lands
                // on the member text (`MIC.P`, `U_MCU.I2C0.SCL`).
                let bus = base_from_bus.unwrap_or_else(|| root.clone());
                let mut cur = node.get_sub_node();
                while let Some(child) = cur {
                    if matches!(child.get_type(), MCAST_OPD_CURLY | MCAST_OPD_CURLY_MN) {
                        let mut mc = child.get_sub_node();
                        while let Some(m) = mc {
                            if let Some(mname) = m.to_string() {
                                let mstart = m.get_pos() as usize;
                                insts.record_net_ref(
                                    mstart..(mstart + mname.len()),
                                    &format!("{bus}.{mname}"),
                                    scope,
                                );
                            }
                            mc = m.get_next();
                        }
                    }
                    cur = child.get_next();
                }
            } else if ids.count() >= 2 {
                // `MIC.P`: register the member segment (text after the dot).
                let member_start = start + root.len() + 1;
                let member_len = text.len().saturating_sub(root.len() + 1);
                if member_len > 0 {
                    insts.record_net_ref(member_start..(member_start + member_len), &text, scope);
                }
            } else {
                // ★ Dot chain: `MIC.P`, `U_MCU.I2C0.SCL`. `node.to_string()`
                // returns only the base segment (`U_MCU`), while `ids.to_string()`
                // carries the full chain. Register the member segment(s) after
                // the first dot so F12 lands on the member text and lapper can
                // resolve it via the member chain (Phase 3).
                let full = ids.to_string();
                if !ids.is_square_only() && full.contains('.') {
                    let member_start = start + root.len() + 1;
                    let member_end = start + full.len();
                    if member_end > member_start {
                        insts.record_net_ref(member_start..member_end, &full, scope);
                    }
                }
            }
        }
    }
}

// Mc2Module - Module instance wrapper

#[derive(Debug, Clone)]
pub struct Mc2Module {
    pub base: Arc<McModule>,
    pub name: McIds,
    pub args: Vec<McParamValue>,
    pub insts: Vec<McInst>,
    /// ★ NC layer ③: the declaration clause's `@ncpin(…)` trailer, in written
    /// form (see [`crate::semantic::nc_pin`]) — the module-instance sibling of
    /// [`Mc2Component::nc_pins`].
    pub(crate) nc_pins: Vec<crate::semantic::nc_pin::NcPinSpec>,
}

impl Mc2Module {
    pub fn new(name: &str, base: Arc<McModule>) -> Self {
        Self {
            base,
            name: McIds::from(name),
            args: Vec::new(),
            insts: Vec::new(),
            nc_pins: Vec::new(),
        }
    }

    pub fn with_params(name: &str, base: Arc<McModule>, args: Vec<McParamValue>) -> Self {
        Self {
            base,
            name: McIds::from(name),
            args,
            insts: Vec::new(),
            nc_pins: Vec::new(),
        }
    }

    /// Find externally exposed ports
    pub fn find_port(&self, id: &str) -> Option<McPhrase> {
        // 1. Find in interface definitions
        if let Some(_port) = self.base.insts.find_port(id) {
            return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                McInstance::Bus(McBus::new_with_members(
                    &self.name.to_string(),
                    vec![id.to_string()],
                )),
            ))));
        }

        // 2. Support dot-path lookup (e.g. "in.data")
        if let Some((first, rest)) = id.split_once('.') {
            if let Some((_iotype, port)) = self.base.insts.get_with_iotype(first) {
                // Find in port's sub-members
                for member_name in port.members() {
                    if member_name == rest {
                        return Some(McPhrase::Endpoint(McEndpoint::Single(McInstanceRef::new(
                            McInstance::Bus(McBus::new(&format!(
                                "{}.{}.{}",
                                self.name, first, rest
                            ))),
                        ))));
                    }
                }
            }
        }

        // 3. Find in functions (supports method calls)
        // TODO: phase 2 implementation

        None
    }

    /// Get all input ports
    pub fn get_input_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_inputs()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }

    /// Get all output ports
    pub fn get_output_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_outputs()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }

    /// Get all ports
    pub fn get_all_ports(&self) -> Vec<McBus> {
        self.base
            .insts
            .get_all_ports()
            .iter()
            .map(|p| p.to_node_element_with_prefix(&self.name.to_string()))
            .collect()
    }
}

// Display implementation - concise format output

impl std::fmt::Display for McModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Module {}", self.name)?;
        writeln!(f, "  Insts:")?;

        // Collect inst data for alignment calculation
        #[derive(Debug)]
        struct InstRow {
            io: String,
            name: String,
            inst: String,
            inst_type: String,
            has_io: bool,
            type_order: u8, // 0=Component/Module, 1=Interface, 2=Label, 3=Bus, 4=other
        }

        let mut rows: Vec<InstRow> = Vec::new();
        for (name, (io, inst)) in self.insts.iter_with_iotype() {
            let has_io = !matches!(*io, IOType::None);
            let io_str = if has_io {
                format!("{io:?}")
            } else {
                String::new()
            };
            // Strip type prefixes from instance display and collect type separately
            let (inst_str, type_str, type_order) = match inst {
                McInstance::Component(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Component:").to_string(),
                        "Component".to_string(),
                        0,
                    )
                }
                McInstance::Module(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Module:").to_string(),
                        "Module".to_string(),
                        0,
                    )
                }
                McInstance::Label(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("L:").to_string(),
                        "Label".to_string(),
                        2,
                    )
                }
                McInstance::Interface(_) => (inst.to_string(), "Interface".to_string(), 1),
                McInstance::Bus(_) => (inst.to_string(), "Bus".to_string(), 3),
                McInstance::BusRef { .. } => (inst.to_string(), "Ref".to_string(), 4),
                McInstance::List(_) | McInstance::Unresolved { .. } => {
                    (inst.to_string(), "Unresolved".to_string(), 5)
                }
                McInstance::Pins => ("pins".to_string(), "Pins".to_string(), 6),
                McInstance::PinId(id) => (id.clone(), "PinId".to_string(), 6),
                McInstance::Attr(_) => (inst.to_string(), "Attr".to_string(), 7),
                McInstance::Func(_) => {
                    let s = inst.to_string();
                    (
                        s.trim_start_matches("Func:").to_string(),
                        "Func".to_string(),
                        8,
                    )
                }
                McInstance::EnumVal { .. } => (inst.to_string(), "EnumVal".to_string(), 9),
            };
            rows.push(InstRow {
                io: io_str,
                name: name.to_string(),
                inst: inst_str,
                inst_type: type_str,
                has_io,
                type_order,
            });
        }

        // Sort: 1. has_io=true first, 2. type_order, 3. name
        rows.sort_by(|a, b| {
            let io_cmp = b.has_io.cmp(&a.has_io);
            if io_cmp != std::cmp::Ordering::Equal {
                return io_cmp;
            }
            let type_cmp = a.type_order.cmp(&b.type_order);
            if type_cmp != std::cmp::Ordering::Equal {
                return type_cmp;
            }
            a.name.cmp(&b.name)
        });

        // Calculate column widths
        let io_width = rows.iter().map(|r| r.io.len()).max().unwrap_or(0);
        let name_width = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
        let inst_width = rows.iter().map(|r| r.inst.len()).max().unwrap_or(0);

        // Output with alignment
        for row in &rows {
            if row.io.is_empty() {
                if row.inst_type.is_empty() {
                    writeln!(
                        f,
                        "    {:<width$} {:<name_width$} = {:<inst_width$}",
                        "",
                        row.name,
                        row.inst,
                        width = io_width,
                        name_width = name_width,
                        inst_width = inst_width
                    )?;
                } else {
                    writeln!(
                        f,
                        "    {:<width$} {:<name_width$} = {:<inst_width$}  {}",
                        "",
                        row.name,
                        row.inst,
                        row.inst_type,
                        width = io_width,
                        name_width = name_width,
                        inst_width = inst_width
                    )?;
                }
            } else if row.inst_type.is_empty() {
                writeln!(
                    f,
                    "    {:<width$} {:<name_width$} = {:<inst_width$}",
                    row.io,
                    row.name,
                    row.inst,
                    width = io_width,
                    name_width = name_width,
                    inst_width = inst_width
                )?;
            } else {
                writeln!(
                    f,
                    "    {:<width$} {:<name_width$} = {:<inst_width$}  {}",
                    row.io,
                    row.name,
                    row.inst,
                    row.inst_type,
                    width = io_width,
                    name_width = name_width,
                    inst_width = inst_width
                )?;
            }
        }

        writeln!(f, "  Stmts:")?;
        for stmt in &self.stmts {
            writeln!(f, "    {stmt}")?;
        }
        Ok(())
    }
}
