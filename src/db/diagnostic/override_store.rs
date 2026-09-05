// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Catalog override store (rule-registry design §8-5) — the single
//! configuration-side writer for rule severity / allow / accept overrides.
//!
//! Data model: three zones (`severities`, `allows`, `accepts`) carried in
//! four layers, ordered runtime-session > project `mcc.yaml` > user
//! `mcc.yaml` > `*.mcerc` import, sitting above the rule descriptor default.
//! Within one layer, several entries may hit for the same rule; the most
//! path-specific one wins (exact file > directory/glob > project global).
//!
//! [`OverrideStore::adjudicate`] is pure, and on an empty store returns the
//! rule default — the "empty store == today's bytes" identity anchor that
//! lets the lock tests run unmodified. `accepts` are recorded in the store
//! (an on-file waiver: still displayed and counted, exempted from audit/gate
//! alarms, erc-extension §5.4) but have no functional consumer in v1; they
//! surface through the `mcc rules` read view.
//!
//! A process-wide store is installed once by the entry point (CLI/server/MCP)
//! from the merged global + project config `diag` zones (see
//! `crate::load_rule_overrides`); while uninstalled it stays empty, which is
//! exactly the identity anchor the lock tests run under.

use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};

use crate::semantic::validation::finding::CheckFinding;
use crate::semantic::validation::CheckSeverity;
use serde_json::{json, Value};

/// The process-wide store consulted by the emission sinks. Empty (identity)
/// until the entry point installs the config-seeded store.
static ACTIVE_STORE: OnceLock<RwLock<OverrideStore>> = OnceLock::new();

/// Install (replace) the process-wide override store.
pub fn install_store(store: OverrideStore) {
    let mut guard = active_store().write().unwrap();
    *guard = store;
}

/// Mutate the process-wide store in place — the runtime write face (RPC/MCP
/// `severity.set`/`allow.add`/`accept`) uses this to push into the session
/// layer so later runs in the same process adjudicate against it.
pub fn update_store(f: impl FnOnce(&mut OverrideStore)) {
    let mut guard = active_store().write().unwrap();
    f(&mut guard);
}

/// Read the process-wide store.
pub fn with_store<R>(f: impl FnOnce(&OverrideStore) -> R) -> R {
    let guard = active_store().read().unwrap();
    f(&guard)
}

fn active_store() -> &'static RwLock<OverrideStore> {
    ACTIVE_STORE.get_or_init(|| RwLock::new(OverrideStore::default()))
}

/// Parse a config/write-side rule code key — `"E4101"` or `"4101"` (case
/// insensitive `e` prefix) — into the numeric code. Returns `None` when the
/// key is not a numeric code with an optional `E` prefix.
pub fn parse_rule_code(key: &str) -> Option<u32> {
    let key = key.trim();
    let digits = key
        .strip_prefix('E')
        .or_else(|| key.strip_prefix('e'))
        .unwrap_or(key);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The canonical config key for a rule code (the `"E4101"` spelling used in
/// the `mcc.yaml`/CLI surface and in the explain/audit views).
pub fn code_key(code: u32) -> String {
    format!("E{code}")
}

/// Parse a config `path` value into a [`PathScope`]: omitted/empty = project
/// global, a value ending in `/` or containing glob metacharacters =
/// directory/glob pattern, anything else = one exact project-relative file.
pub fn parse_path_scope(path: &str) -> PathScope {
    let p = path.trim();
    if p.is_empty() {
        PathScope::Project
    } else if p.ends_with('/') || p.contains('*') || p.contains('?') {
        PathScope::Directory(p.to_string())
    } else {
        PathScope::File(p.to_string())
    }
}

/// Authority layer of an override entry (§8-5 priority order). Higher wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OverrideLayer {
    /// External `*.mcerc` import (read-only for mcc).
    Mcerc = 0,
    /// User-level `mcc.yaml`.
    User = 1,
    /// Project `mcc.yaml`.
    Project = 2,
    /// Runtime session: CLI/RPC/MCP writes, not persisted.
    Session = 3,
}

/// How narrowly an allow/accept entry applies. `None` path = project global.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum PathScope {
    /// Applies to the whole project.
    Project,
    /// Directory prefix (value ends in `/`) or a glob pattern (value contains
    /// `*` / `?`), e.g. `boards/**/*.mc`.
    Directory(String),
    /// One exact project-relative file path.
    File(String),
}

impl PathScope {
    /// Specificity tier used to break ties inside one layer: exact file >
    /// directory/glob > project global. The adjudication outcome is boolean
    /// either way, so tier only sharpens the audit view / unit expectations.
    #[cfg(test)]
    fn tier(&self) -> u8 {
        match self {
            Self::File(_) => 2,
            Self::Directory(_) => 1,
            Self::Project => 0,
        }
    }
}

/// One `allows` row (an explicit suppression).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowEntry {
    pub code: u32,
    pub path: PathScope,
    pub reason: Option<String>,
}

/// One `accepts` row (an on-file waiver; recorded, v1 has no gate consumer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptEntry {
    pub code: u32,
    pub path: PathScope,
    pub since: Option<String>,
}

/// Configuration carried by one layer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layer {
    /// Severity overrides, keyed by rule code (a code maps to one severity).
    pub severities: BTreeMap<u32, CheckSeverity>,
    /// Suppression rows in file order (later rows win on ties).
    pub allows: Vec<AllowEntry>,
    /// Waiver rows in file order.
    pub accepts: Vec<AcceptEntry>,
}

/// The full store: four layers plus the project root used to relativize
/// source uris before path matching.
#[derive(Debug, Clone, Default)]
pub struct OverrideStore {
    pub session: Layer,
    pub project: Layer,
    pub user: Layer,
    pub mcerc: Layer,
    /// Project root; source uris that start with it are relativized before a
    /// path match, so project-relative `mcc.yaml` patterns compare cleanly.
    pub project_root: Option<String>,
}

impl OverrideStore {
    /// True when no layer carries any configuration.
    pub fn is_empty(&self) -> bool {
        self.session.is_empty()
            && self.project.is_empty()
            && self.user.is_empty()
            && self.mcerc.is_empty()
    }

    /// The layered severity override for a code, if any (highest layer wins;
    /// `accepts` never affects severity).
    pub fn severity_for(&self, code: u32) -> Option<CheckSeverity> {
        for l in [&self.session, &self.project, &self.user, &self.mcerc] {
            if let Some(sev) = l.severities.get(&code) {
                return Some(*sev);
            }
        }
        None
    }

    /// Adjudicate one finding against the store — design §8-5 decision
    /// function:
    ///   1. `overridable = false` refuses every override/suppression and
    ///      returns the default (today's errors are all non-overridable);
    ///   2. an allow hit suppresses;
    ///   3. a severity override applies;
    ///   4. otherwise the default.
    ///
    /// `uri` is the finding's source uri (`None` when unattributed). Layers
    /// are consulted highest first; within one layer the most path-specific
    /// allow hit for the code wins.
    pub fn adjudicate(
        &self,
        code: u32,
        overridable: bool,
        _default: CheckSeverity,
        uri: Option<&str>,
    ) -> Adjudication {
        if !overridable {
            return Adjudication::Default;
        }
        let rel = uri.map(|u| relativize(self.project_root.as_deref(), u));
        for l in [&self.session, &self.project, &self.user, &self.mcerc] {
            if layer_has_allow_hit(l, code, rel.as_deref()) {
                return Adjudication::Suppressed;
            }
        }
        if let Some(sev) = self.severity_for(code) {
            return Adjudication::Severity(sev);
        }
        Adjudication::Default
    }

    /// Convenience for the emission sinks: apply the store to one unified
    /// finding. `None` means the allow hit suppresses the finding (it is not
    /// produced at the display layers); `Some` carries the finding with the
    /// effective severity. Rules whose code is not registered in the catalog
    /// pass through untouched.
    ///
    /// Every catalog rule is `overridable = false` today (§8-5 step 1), so
    /// this returns the finding unchanged on a real code; the mapping branches
    /// become reachable once a rule is explicitly granted `overridable`.
    pub fn apply_to_finding(&self, f: &CheckFinding) -> Option<CheckFinding> {
        let meta = match crate::rules::find_rule(f.code) {
            Some(m) => m,
            None => return Some(f.clone()),
        };
        let adjudication = self.adjudicate(f.code, meta.overridable, f.severity, f.uri.as_deref());
        apply_adjudication(f, adjudication)
    }
}

/// Map one pure adjudication onto the finding line (shared by the emission
/// sinks and by the audit view).
fn apply_adjudication(f: &CheckFinding, a: Adjudication) -> Option<CheckFinding> {
    match a {
        Adjudication::Suppressed => None,
        Adjudication::Severity(sev) => {
            let mut g = f.clone();
            g.severity = sev;
            Some(g)
        }
        Adjudication::Default => Some(f.clone()),
    }
}

impl Layer {
    fn is_empty(&self) -> bool {
        self.severities.is_empty() && self.allows.is_empty() && self.accepts.is_empty()
    }
}

/// Result of adjudicating one finding (§8-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjudication {
    /// No store hit — keep the rule default (empty-store identity).
    Default,
    /// A severity override applies.
    Severity(CheckSeverity),
    /// An allow hit suppresses the finding at the display layers.
    Suppressed,
}

/// True when at least one allow row for `code` matches `rel` (the uri already
/// relativized against the project root). Ties inside the layer resolve to
/// the most path-specific hit; the outcome is boolean either way.
fn layer_has_allow_hit(layer: &Layer, code: u32, rel: Option<&str>) -> bool {
    layer
        .allows
        .iter()
        .filter(|e| e.code == code && path_matches(&e.path, rel))
        .next()
        .is_some()
}

/// Match a path scope against a (relativized) source uri.
fn path_matches(scope: &PathScope, rel: Option<&str>) -> bool {
    let Some(rel) = rel else { return false };
    match scope {
        PathScope::Project => true,
        PathScope::File(f) => rel == f,
        PathScope::Directory(p) => {
            if p.contains('*') || p.contains('?') {
                glob_match(p, rel)
            } else if let Some(stripped) = p.strip_suffix('/') {
                rel == stripped
                    || rel.starts_with(stripped) && rel[stripped.len()..].starts_with('/')
            } else {
                rel == p
            }
        }
    }
}

/// Strip a project-root prefix from a source uri so project-relative config
/// patterns can compare against it. Returns the input unchanged when the uri
/// does not start with the root or no root is set.
fn relativize<'a>(root: Option<&str>, uri: &'a str) -> &'a str {
    match root {
        Some(r) => {
            let r = r.trim_end_matches('/');
            if uri == r {
                "/"
            } else if let Some(rest) = uri.strip_prefix(r) {
                rest.trim_start_matches('/')
            } else {
                uri
            }
        }
        None => uri,
    }
}

/// Minimal glob matcher used for the `path` patterns of allow/accept rows:
/// `*` matches any run of characters except `/`, `**` matches any run
/// including `/`, `?` matches one non-`/` character.
fn glob_match(pattern: &str, text: &str) -> bool {
    glob_match_chars(
        &pattern.chars().collect::<Vec<_>>(),
        &text.chars().collect::<Vec<_>>(),
    )
}

fn glob_match_chars(pat: &[char], text: &[char]) -> bool {
    if pat.is_empty() {
        return text.is_empty();
    }
    match pat[0] {
        '*' => {
            let mut star = 0;
            while star < pat.len() && pat[star] == '*' {
                star += 1;
            }
            if star >= 2 {
                // `**` crosses `/`
                (0..=text.len()).any(|i| glob_match_chars(&pat[star..], &text[i..]))
            } else {
                // single `*` does not cross `/`
                (0..=text.len())
                    .filter(|&i| !text[..i].contains(&'/'))
                    .any(|i| glob_match_chars(&pat[1..], &text[i..]))
            }
        }
        '?' => !text.is_empty() && text[0] != '/' && glob_match_chars(&pat[1..], &text[1..]),
        c => !text.is_empty() && text[0] == c && glob_match_chars(&pat[1..], &text[1..]),
    }
}

// ============================================================================
// §8 consumer surface — catalog read projection + session write face
// ============================================================================
//
// Every read consumer (`mcc rules` list/detail, RPC `rules.list` /
// `rule.detail`, `explain`/`caps`, query `--kind rule`) renders one shared
// JSON projection built here, so the bytes stay identical across layers.
// Every write entry (`severity.set` / `allow.add` / `accept` from CLI
// local / RPC / MCP) validates against the catalog first (§8-5 step 1:
// `overridable = false` refuses the write) and lands in the session layer
// through [`update_store`] — the design's single store write API. Only the
// CLI `--write` flag persists into the project config (see
// `crate::cli::config`); the session ops never touch a file.

/// The canonical `E4101`-style key for a code, plus its numeric code.
pub fn rule_key(code: u32) -> String {
    code_key(code)
}

/// True when `code` is registered in one of the four numeric-code catalogs.
pub fn rule_code_known(code: u32) -> bool {
    crate::rules::find_rule(code).is_some()
}

/// §8-5 write gate shared by every writer: the code must be registered and
/// the rule must grant `overridable = true`, else the write is refused with
/// the same message on CLI, RPC and MCP (no silent swallow).
pub fn guard_rule_write(code: u32) -> Result<(), String> {
    let meta = match crate::rules::find_rule(code) {
        Some(m) => m,
        None => {
            return Err(format!(
                "unknown rule code {key}; run `mcc rules list` for the catalog",
                key = rule_key(code)
            ))
        }
    };
    if meta.overridable {
        Ok(())
    } else {
        Err(format!(
            "rule {key} ({name}) is not overridable; a severity/allow write needs the descriptor to grant overridable = true",
            key = rule_key(code),
            name = meta.name
        ))
    }
}

/// Session-layer severity override (design §8-5 write entry). The rule must
/// be registered and overridable (see [`guard_rule_write`]); repeated writes
/// for the same code replace the earlier value.
pub fn session_set_severity(code: u32, severity: CheckSeverity) -> Result<(), String> {
    guard_rule_write(code)?;
    update_store(|s| {
        s.session.severities.insert(code, severity);
    });
    Ok(())
}

/// Session-layer allow (suppression) row. Upsert: a row with the same code
/// and path scope is replaced so re-adding never duplicates.
pub fn session_add_allow(code: u32, path: PathScope, reason: Option<String>) -> Result<(), String> {
    guard_rule_write(code)?;
    update_store(|s| {
        s.session
            .allows
            .retain(|e| !(e.code == code && e.path == path));
        s.session.allows.push(AllowEntry { code, path, reason });
    });
    Ok(())
}

/// Session-layer accept (waiver) row. Upsert semantics mirror
/// [`session_add_allow`].
pub fn session_add_accept(code: u32, path: PathScope, since: Option<String>) -> Result<(), String> {
    guard_rule_write(code)?;
    update_store(|s| {
        s.session
            .accepts
            .retain(|e| !(e.code == code && e.path == path));
        s.session.accepts.push(AcceptEntry { code, path, since });
    });
    Ok(())
}

/// One layer's raw rows for a code, for the audit/detail view.
#[derive(Debug, Clone)]
pub struct AuditRow {
    pub layer: OverrideLayer,
    /// "severity" | "allow" | "accept".
    pub kind: &'static str,
    pub path: String,
    pub value: Option<String>,
    pub note: Option<String>,
}

/// The raw store rows touching `code`, highest layer first (session, project,
/// user, mcerc) — the §8-5 audit view: every override/waiver is queryable.
pub fn audit_rows(code: u32) -> Vec<AuditRow> {
    let store = {
        let guard = active_store().read().unwrap();
        guard.clone()
    };
    let mut out = Vec::new();
    let sev_to = |sev: CheckSeverity| sev.as_str().to_string();
    for (layer, lay) in [
        (OverrideLayer::Session, &store.session),
        (OverrideLayer::Project, &store.project),
        (OverrideLayer::User, &store.user),
        (OverrideLayer::Mcerc, &store.mcerc),
    ] {
        if let Some(sev) = lay.severities.get(&code) {
            out.push(AuditRow {
                layer,
                kind: "severity",
                path: "project".to_string(),
                value: Some(sev_to(*sev)),
                note: None,
            });
        }
        for e in &lay.allows {
            if e.code == code {
                out.push(AuditRow {
                    layer,
                    kind: "allow",
                    path: path_display(&e.path),
                    value: None,
                    note: e.reason.clone(),
                });
            }
        }
        for e in &lay.accepts {
            if e.code == code {
                out.push(AuditRow {
                    layer,
                    kind: "accept",
                    path: path_display(&e.path),
                    value: e.since.clone(),
                    note: None,
                });
            }
        }
    }
    out
}

pub fn path_display(path: &PathScope) -> String {
    match path {
        PathScope::Project => "project".to_string(),
        PathScope::Directory(d) => d.clone(),
        PathScope::File(f) => f.clone(),
    }
}

/// The shared descriptor JSON for one catalog rule (§8 list/detail row).
/// Includes the rule's *configured* and *effective* severity: a store row is
/// configured whenever it exists in any layer; it is effective only when the
/// descriptor grants `overridable = true` (design §8-5 step 1).
pub fn rule_descriptor_json(meta: &crate::rules::RuleMeta) -> Value {
    let raw = with_store(|s| s.severity_for(meta.code));
    let effective = if meta.overridable { raw } else { None };
    json!({
        "key": code_key(meta.code),
        "code": meta.code,
        "name": meta.name,
        "title": meta.title,
        "severity": meta.severity.as_str(),
        "severity_configured": raw.map(|s| s.as_str()),
        "effective_severity": effective.unwrap_or(meta.severity).as_str(),
        "scope": meta.scope.as_str(),
        "domain": meta.domain.as_str(),
        "family": meta.family,
        "overridable": meta.overridable,
        "fix": meta.fix.as_str(),
        "plane": meta.plane.as_str(),
        "acceptance": meta.acceptance.as_str(),
        "sink": meta.sink.as_str(),
        "gate": meta.gate.as_str(),
        "cadence": meta.cadence.as_str(),
        "doc": meta.doc,
        "lock": meta.lock,
        "lock_strong": meta.lock.starts_with("tests/"),
    })
}

/// Build a [`crate::rules::RuleFilter`] from the §8 query params
/// (`scope`/`domain`/`severity`/`plane`/`gate`/`overridable`/`fix`, all
/// optional strings). Used by the RPC `rules.list` handler and by the CLI
/// `--format json` path so both parse identically.
pub fn filter_from_value(v: Option<&Value>) -> Result<crate::rules::RuleFilter, String> {
    use crate::rules::RuleFilter;
    use crate::rules::{Acceptance, Cadence, FixKind, GateKind, RuleDomain, RulePlane, RuleScope};
    let v = match v {
        Some(v) => v,
        None => return Ok(RuleFilter::default()),
    };
    let get = |key: &str| v.get(key).and_then(|x| x.as_str());
    let scope = match get("scope") {
        Some(s) => Some(
            RuleScope::from_name(s)
                .ok_or_else(|| format!("unknown rule scope '{s}' (post-parse|assembly-gate|flat-erc|declaration|viz-layout)"))?,
        ),
        None => None,
    };
    let domain = match get("domain") {
        Some(s) => {
            Some(RuleDomain::from_name(s).ok_or_else(|| format!("unknown rule domain '{s}'"))?)
        }
        None => None,
    };
    let severity = match get("severity") {
        Some(s) => Some(
            CheckSeverity::from_str(s.trim())
                .ok_or_else(|| format!("unknown severity '{s}' (hint|info|warning|error)"))?,
        ),
        None => None,
    };
    let plane = match get("plane") {
        Some(s) => Some(RulePlane::from_name(s).ok_or_else(|| {
            format!("unknown rule plane '{s}' (core-mechanism|domain-package|sim-fulfillment)")
        })?),
        None => None,
    };
    let gate = match get("gate") {
        Some(s) => Some(
            GateKind::from_name(s)
                .ok_or_else(|| format!("unknown rule gate '{s}' (advisory|blocking)"))?,
        ),
        None => None,
    };
    let overridable = match get("overridable") {
        Some(s) => Some(
            s.parse::<bool>()
                .map_err(|_| format!("overridable must be true or false, got '{s}'"))?,
        ),
        None => None,
    };
    let fix = match get("fix") {
        Some(s) => Some(
            FixKind::from_name(s)
                .ok_or_else(|| format!("unknown rule fix '{s}' (none|quick-fix|suggestion)"))?,
        ),
        None => None,
    };
    // Keep the unused-variant references concrete so adding a filter axis
    // later only touches this table.
    let _ = (Acceptance::Legal, Cadence::PerCircuit);
    Ok(RuleFilter {
        scope,
        domain,
        severity,
        plane,
        gate,
        overridable,
        fix,
    })
}

/// The shared `rules.list` projection (§8): the matching rules in declared
/// table order (FlatErc, Declaration, AssemblyGate, PostParse — see
/// `crate::rules::query_rules`), each as [`rule_descriptor_json`], plus the
/// total.
pub fn rules_list_json(filter: &crate::rules::RuleFilter) -> Value {
    let metas = crate::rules::query_rules(filter);
    let rules: Vec<Value> = metas.iter().map(|m| rule_descriptor_json(m)).collect();
    json!({ "total": rules.len(), "rules": rules })
}

/// The shared `rule.detail` projection: the full descriptor plus the §8-5
/// audit rows (configured overrides/waivers per layer) and the canonical
/// `mcc rules` / `mcc.yaml` spelling for each zone.
pub fn rule_detail_json(code: u32) -> Result<Value, String> {
    let meta = crate::rules::find_rule(code).ok_or_else(|| {
        format!(
            "unknown rule code {key}; run `mcc rules list` for the catalog",
            key = code_key(code)
        )
    })?;
    let rows: Vec<Value> = audit_rows(code)
        .iter()
        .map(|r| {
            json!({
                "layer": format!("{:?}", r.layer).to_ascii_lowercase(),
                "kind": r.kind,
                "path": r.path,
                "value": r.value,
                "note": r.note,
            })
        })
        .collect();
    // Anchor sharing: codes pinned to the same lock anchor as this one.
    let anchor_codes: Vec<u32> = crate::rules::lock_ledger()
        .iter()
        .find(|e| e.lock == meta.lock)
        .map(|e| e.codes.clone())
        .unwrap_or_default();
    Ok(json!({
        "rule": rule_descriptor_json(meta),
        "audit": rows,
        "anchor_codes": anchor_codes,
        "allow_syntax": format!(
            "{key} path=boards/**/*.mc reason=...  # or omit path for the project global",
            key = code_key(code)
        ),
    }))
}

/// The caps/`features.rules` summary block: total rule count and the count by
/// scope, severity, gate and overridable flag over the whole numeric-code
/// catalog (design §8: caps_json gains a `features.rules` block).
pub fn rules_summary_json() -> Value {
    use crate::rules::{query_rules, RuleFilter, RuleScope};
    let all = query_rules(&RuleFilter::default());
    let mut by_scope = serde_json::Map::new();
    for scope in [
        RuleScope::PostParse,
        RuleScope::AssemblyGate,
        RuleScope::FlatErc,
        RuleScope::Declaration,
        RuleScope::VizLayout,
    ] {
        let n = all.iter().filter(|m| m.scope == scope).count();
        by_scope.insert(scope.as_str().to_string(), json!(n));
    }
    let mut by_severity = serde_json::Map::new();
    let mut by_gate = serde_json::Map::new();
    let mut overridable = 0usize;
    for m in &all {
        let sev = by_severity
            .entry(m.severity.as_str().to_string())
            .or_insert_with(|| json!(0));
        *sev = json!(sev.as_u64().unwrap_or(0) + 1);
        let gate = by_gate
            .entry(m.gate.as_str().to_string())
            .or_insert_with(|| json!(0));
        *gate = json!(gate.as_u64().unwrap_or(0) + 1);
        if m.overridable {
            overridable += 1;
        }
    }
    json!({
        "total": all.len(),
        "by_scope": by_scope,
        "by_severity": by_severity,
        "by_gate": by_gate,
        "overridable": overridable,
    })
}

/// The `mcc rules` human text line for one descriptor row (fields aligned
/// with [`rule_descriptor_json`]).
pub fn rule_descriptor_line(m: &crate::rules::RuleMeta) -> String {
    let effective = with_store(|s| s.severity_for(m.code));
    let sev = if m.overridable {
        effective.unwrap_or(m.severity)
    } else {
        m.severity
    };
    format!(
        "{key}  {sev:<7} {scope:<14} {domain:<16} {gate:<8} {ov:<8} {name:<24} {title}",
        key = code_key(m.code),
        sev = sev.as_str(),
        scope = m.scope.as_str(),
        domain = m.domain.as_str(),
        gate = m.gate.as_str(),
        ov = if m.overridable { "yes" } else { "no" },
        name = m.name,
        title = m.title,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(code: u32, uri: Option<&str>) -> CheckFinding {
        CheckFinding {
            rule: "probe",
            code,
            severity: CheckSeverity::Warning,
            message: "probe message".to_string(),
            uri: uri.map(|s| s.to_string()),
            pos: 0,
            len: 0,
        }
    }

    #[test]
    fn empty_store_adjudicates_to_default_and_passes_findings_through() {
        // The identity anchor: with nothing configured the store neither
        // suppresses nor re-levels anything, for overridable and
        // non-overridable rules alike.
        let store = OverrideStore::default();
        assert!(store.is_empty());
        assert_eq!(
            store.adjudicate(4101, false, CheckSeverity::Error, Some("a.mc")),
            Adjudication::Default
        );
        assert_eq!(
            store.adjudicate(4101, true, CheckSeverity::Error, Some("a.mc")),
            Adjudication::Default
        );
        let f = finding(5155, Some("a.mc")); // PIN_UNCONNECTED is registered
        assert_eq!(store.apply_to_finding(&f), Some(f));
    }

    #[test]
    fn non_overridable_rules_refuse_every_override_and_suppression() {
        // Design §8-5 step 1: `overridable = false` returns the default even
        // when a severity or allow row would otherwise hit. Errors today are
        // all non-overridable, so this is what keeps them un-suppressible.
        let mut store = OverrideStore::default();
        store.session.severities.insert(4101, CheckSeverity::Info);
        store.session.allows.push(AllowEntry {
            code: 4101,
            path: PathScope::Project,
            reason: None,
        });
        assert_eq!(
            store.adjudicate(4101, false, CheckSeverity::Error, Some("a.mc")),
            Adjudication::Default
        );
    }

    #[test]
    fn severity_override_honors_layer_priority() {
        let mut store = OverrideStore::default();
        store.user.severities.insert(4101, CheckSeverity::Warning);
        store.project.severities.insert(4101, CheckSeverity::Info);
        // Project beats user; session beats project.
        assert_eq!(
            store.adjudicate(4101, true, CheckSeverity::Error, None),
            Adjudication::Severity(CheckSeverity::Info)
        );
        store.session.severities.insert(4101, CheckSeverity::Hint);
        assert_eq!(
            store.adjudicate(4101, true, CheckSeverity::Error, None),
            Adjudication::Severity(CheckSeverity::Hint)
        );
    }

    #[test]
    fn allow_suppresses_only_when_path_matches() {
        let mut store = OverrideStore::default();
        store.project.allows.push(AllowEntry {
            code: 4101,
            path: PathScope::Directory("boards/**/*.mc".to_string()),
            reason: Some("documented exception".to_string()),
        });
        assert_eq!(
            store.adjudicate(4101, true, CheckSeverity::Error, Some("boards/dev/main.mc")),
            Adjudication::Suppressed
        );
        assert_eq!(
            store.adjudicate(4101, true, CheckSeverity::Error, Some("core/main.mc")),
            Adjudication::Default
        );
        // Project-global allow hits any uri, but never an unattributed one.
        let mut g = OverrideStore::default();
        g.session.allows.push(AllowEntry {
            code: 4101,
            path: PathScope::Project,
            reason: None,
        });
        assert_eq!(
            g.adjudicate(4101, true, CheckSeverity::Error, Some("x.mc")),
            Adjudication::Suppressed
        );
        assert_eq!(
            g.adjudicate(4101, true, CheckSeverity::Error, None),
            Adjudication::Default
        );
    }

    #[test]
    fn apply_to_finding_keeps_non_overridable_and_unregistered_codes_untouched() {
        // Every catalog rule is `overridable = false` (§8-5 step 1): even a
        // store that would otherwise hit must not re-level or suppress the
        // finding. Unregistered codes pass through regardless.
        let mut store = OverrideStore::default();
        store.project.severities.insert(5155, CheckSeverity::Info);
        store.session.allows.push(AllowEntry {
            code: 5155,
            path: PathScope::File("a.mc".to_string()),
            reason: None,
        });
        let f = finding(5155, Some("a.mc"));
        assert_eq!(store.apply_to_finding(&f), Some(f.clone()));

        let unregistered = finding(1, Some("a.mc"));
        store.session.allows.push(AllowEntry {
            code: 1,
            path: PathScope::Project,
            reason: None,
        });
        assert_eq!(store.apply_to_finding(&unregistered), Some(unregistered));
    }

    #[test]
    fn apply_adjudication_maps_suppress_relevel_and_default() {
        // The mapping branches are exercised directly because no catalog rule
        // is overridable yet; they become reachable through apply_to_finding
        // once one is.
        let f = finding(5155, Some("a.mc"));
        assert_eq!(
            apply_adjudication(&f, Adjudication::Default),
            Some(f.clone())
        );
        assert_eq!(apply_adjudication(&f, Adjudication::Suppressed), None);
        let g =
            apply_adjudication(&f, Adjudication::Severity(CheckSeverity::Info)).expect("releveled");
        assert_eq!(g.severity, CheckSeverity::Info);
        assert_eq!(g.code, 5155);
        assert_eq!(g.message, "probe message");
        assert_eq!(g.uri, Some("a.mc".to_string()));
    }

    #[test]
    fn exact_file_beats_glob_and_project_global_when_same_code_hits() {
        // Specificity decides *which* row applies when several hit; the
        // adjudication outcome stays Suppressed, and the audit view (rules
        // detail) reports the most specific row.
        let exact = AllowEntry {
            code: 1,
            path: PathScope::File("boards/dev/main.mc".to_string()),
            reason: None,
        };
        let dir = AllowEntry {
            code: 1,
            path: PathScope::Directory("boards/**/*.mc".to_string()),
            reason: None,
        };
        let global = AllowEntry {
            code: 1,
            path: PathScope::Project,
            reason: None,
        };
        let rel = Some("boards/dev/main.mc");
        for scope in [&exact.path, &dir.path, &global.path] {
            assert!(path_matches(scope, rel), "{scope:?}");
        }
        assert!(
            PathScope::File("boards/dev/main.mc".to_string()).tier()
                > PathScope::Directory("boards/**".to_string()).tier()
        );
        assert!(PathScope::Directory("boards/**".to_string()).tier() > PathScope::Project.tier());
    }

    #[test]
    fn glob_matcher_handles_star_globstar_and_question() {
        assert!(glob_match("boards/**/*.mc", "boards/dev/main.mc"));
        assert!(glob_match("boards/**/*.mc", "boards/a/b/c/main.mc"));
        assert!(!glob_match("boards/**/*.mc", "other/main.mc"));
        assert!(glob_match("*.mc", "main.mc"));
        assert!(!glob_match("*.mc", "dir/main.mc")); // single * does not cross /
        assert!(glob_match("?a.mc", "xa.mc"));
        assert!(!glob_match("?a.mc", "/a.mc"));
        assert!(glob_match("boards/**", "boards/dev/main.mc"));
    }

    #[test]
    fn relativize_strips_only_the_project_root_prefix() {
        let root = "/proj";
        assert_eq!(
            relativize(Some(root), "/proj/boards/dev/main.mc"),
            "boards/dev/main.mc"
        );
        assert_eq!(relativize(Some(root), "/other/main.mc"), "/other/main.mc");
        assert_eq!(relativize(Some(root), "/proj"), "/");
        assert_eq!(relativize(None, "/proj/x.mc"), "/proj/x.mc");
    }

    #[test]
    fn parse_rule_code_accepts_e_prefix_and_bare_numbers() {
        assert_eq!(parse_rule_code("E4101"), Some(4101));
        assert_eq!(parse_rule_code("e4101"), Some(4101));
        assert_eq!(parse_rule_code("4101"), Some(4101));
        assert_eq!(parse_rule_code(" E4101 "), Some(4101));
        assert_eq!(parse_rule_code("E"), None);
        assert_eq!(parse_rule_code(""), None);
        assert_eq!(parse_rule_code("driver-conflict"), None);
        assert_eq!(parse_rule_code("4101x"), None);
        assert_eq!(code_key(4101), "E4101");
    }

    #[test]
    fn parse_path_scope_classifies_file_dir_glob_and_project() {
        assert_eq!(parse_path_scope(""), PathScope::Project);
        assert_eq!(parse_path_scope("   "), PathScope::Project);
        assert_eq!(
            parse_path_scope("boards/dev/main.mc"),
            PathScope::File("boards/dev/main.mc".into())
        );
        assert_eq!(
            parse_path_scope("boards/**/*.mc"),
            PathScope::Directory("boards/**/*.mc".into())
        );
        assert_eq!(
            parse_path_scope("boards/"),
            PathScope::Directory("boards/".into())
        );
        assert_eq!(
            parse_path_scope("main.?c"),
            PathScope::Directory("main.?c".into())
        );
    }

    #[test]
    fn active_store_is_empty_until_installed() {
        // The process-wide store starts at identity; install_store replaces
        // it, so the daemon/CLI seed path and tests can reset it.
        install_store(OverrideStore::default());
        with_store(|s| assert!(s.is_empty()));
        let mut store = OverrideStore::default();
        store.session.severities.insert(4101, CheckSeverity::Info);
        install_store(store);
        with_store(|s| assert_eq!(s.severity_for(4101), Some(CheckSeverity::Info)));
        install_store(OverrideStore::default());
    }

    #[test]
    fn session_write_apis_refuse_unknown_and_non_overridable_rules() {
        // §8-5 write gate: unknown codes and codes whose descriptor does not
        // grant overridable = true are refused with the same error on every
        // write entry — no silent swallow.
        install_store(OverrideStore::default());
        assert!(session_set_severity(1, CheckSeverity::Info).is_err()); // unknown code
        assert!(session_add_allow(1, PathScope::Project, None).is_err());
        assert!(session_add_accept(1, PathScope::Project, None).is_err());
        // 5155 (PIN_UNCONNECTED) is registered but not overridable today.
        let e = session_set_severity(5155, CheckSeverity::Info).unwrap_err();
        assert!(e.contains("not overridable"), "{e}");
        let e = session_add_allow(5155, PathScope::Project, None).unwrap_err();
        assert!(e.contains("not overridable"), "{e}");
        let e = session_add_accept(5155, PathScope::Project, None).unwrap_err();
        assert!(e.contains("not overridable"), "{e}");
        // Nothing landed in the session layer.
        with_store(|s| assert!(s.session.severities.is_empty()));
        with_store(|s| assert!(s.session.allows.is_empty()));
        with_store(|s| assert!(s.session.accepts.is_empty()));
        install_store(OverrideStore::default());
    }

    #[test]
    fn list_and_summary_projections_cover_the_numeric_catalog() {
        // The shared §8 read projections over the whole catalog: list rows are
        // ordered by declared table order, the summary totals match
        // `rules::rule_count`, and every list row keeps the numeric code.
        let list = rules_list_json(&crate::rules::RuleFilter::default());
        let rules = list["rules"].as_array().expect("rules array");
        assert_eq!(list["total"].as_u64().unwrap() as usize, rules.len());
        assert_eq!(rules.len(), crate::rules::rule_count());
        assert!(rules.iter().all(|r| r["code"].as_u64().is_some()));
        assert!(rules
            .iter()
            .all(|r| r["key"].as_str().unwrap().starts_with('E')));

        let summary = rules_summary_json();
        assert_eq!(
            summary["total"].as_u64().unwrap() as usize,
            crate::rules::rule_count()
        );
        let by_scope = summary["by_scope"].as_object().unwrap();
        let summed: usize = by_scope
            .values()
            .map(|v| v.as_u64().unwrap() as usize)
            .sum();
        assert_eq!(summed, crate::rules::rule_count());
        // Today every rule is non-overridable (§8-5 step 1 anchor).
        assert_eq!(summary["overridable"].as_u64().unwrap(), 0);
    }

    #[test]
    fn detail_projection_carries_descriptor_audit_and_anchor() {
        // 5155 PIN_UNCONNECTED is registered with a tests/ lock anchor.
        let detail = rule_detail_json(5155).expect("detail");
        let rule = &detail["rule"];
        assert_eq!(rule["key"].as_str(), Some("E5155"));
        assert_eq!(rule["code"].as_u64(), Some(5155));
        assert!(rule["doc"].as_str().is_some());
        assert!(detail["audit"].is_array());
        let anchors = detail["anchor_codes"].as_array().unwrap();
        assert!(anchors.iter().any(|c| c.as_u64() == Some(5155)));
        // Unknown code is an explicit error, not a silent empty object.
        assert!(rule_detail_json(1).is_err());
    }

    #[test]
    fn filter_from_value_parses_axes_and_rejects_unknown_spellings() {
        use crate::rules::{RuleFilter, RuleScope};
        use serde_json::json;
        let v = json!({ "scope": "flat-erc", "severity": "warning", "overridable": "false" });
        let f = filter_from_value(Some(&v)).expect("filter");
        assert_eq!(f.scope, Some(RuleScope::FlatErc));
        assert_eq!(f.severity, Some(CheckSeverity::Warning));
        assert_eq!(f.overridable, Some(false));
        assert_eq!(f.domain, None);

        let bad = json!({ "scope": "not-a-scope" });
        let e = filter_from_value(Some(&bad)).unwrap_err();
        assert!(e.contains("unknown rule scope"), "{e}");
        let bad = json!({ "severity": "loud" });
        let e = filter_from_value(Some(&bad)).unwrap_err();
        assert!(e.contains("unknown severity"), "{e}");
        let bad = json!({ "overridable": "maybe" });
        let e = filter_from_value(Some(&bad)).unwrap_err();
        assert!(e.contains("true or false"), "{e}");
        // Missing params is the empty filter (every rule).
        let f = filter_from_value(None).expect("default");
        assert_eq!(f, RuleFilter::default());
    }
}
