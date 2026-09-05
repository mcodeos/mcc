// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc rules` — check-rule registry catalog read view + unified override
//! write face (rule-registry design §8 / §8-5).
//!
//! ```text
//! mcc rules                              # list the whole catalog (text)
//! mcc rules list --scope flat-erc        # filter by a §2.3/§2.5 axis
//! mcc rules list -f json                 # shared rules.list projection bytes
//! mcc rules detail E4101                 # descriptor + override audit
//! mcc rules set-severity E4101 info --write
//! mcc rules allow E4101 --path 'boards/**/*.mc' --reason 'doc note' --write
//! mcc rules accept E3101 --path boards/dev/main.mc --since 2026-09-05 --write
//! ```
//!
//! Every read path renders one shared projection built in
//! [`mcc::override_store`] (the same bytes the RPC `rules.list` /
//! `rule.detail` and the MCP tools emit). Every write path goes through the
//! process store API: session layer by default, and only the explicit
//! `--write` flag persists into the project `[config]` diag zone
//! (design §8-5 persistence discipline). A write is refused whenever the
//! descriptor does not grant `overridable = true`.

use anyhow::Result;
use mcc::check::CheckSeverity;
use mcc::cli::{OutputFormat, RulesAction};
use mcc::override_store as store;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub fn run(action: Option<&RulesAction>, format: OutputFormat) -> Result<()> {
    match action {
        None => cmd_list(
            &mcc::rules::RuleFilter::default(),
            format,
            "no subcommand: full catalog",
        ),
        Some(RulesAction::List {
            scope,
            domain,
            severity,
            plane,
            gate,
            overridable,
            fix,
        }) => {
            let mut v = serde_json::Map::new();
            if let Some(s) = scope {
                v.insert("scope".into(), json!(s));
            }
            if let Some(s) = domain {
                v.insert("domain".into(), json!(s));
            }
            if let Some(s) = severity {
                v.insert("severity".into(), json!(s));
            }
            if let Some(s) = plane {
                v.insert("plane".into(), json!(s));
            }
            if let Some(s) = gate {
                v.insert("gate".into(), json!(s));
            }
            if let Some(s) = overridable {
                v.insert("overridable".into(), json!(s));
            }
            if let Some(s) = fix {
                v.insert("fix".into(), json!(s));
            }
            let filter = store::filter_from_value(Some(&Value::Object(v)))
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            cmd_list(&filter, format, "")
        }
        Some(RulesAction::Detail { code }) => cmd_detail(code, format),
        Some(RulesAction::SetSeverity {
            code,
            severity,
            write,
        }) => cmd_set_severity(code, severity, *write),
        Some(RulesAction::Allow {
            code,
            path,
            reason,
            write,
        }) => cmd_add_allow(code, path, reason, *write),
        Some(RulesAction::Accept {
            code,
            path,
            since,
            write,
        }) => cmd_add_accept(code, path, since, *write),
    }
}

// ============================================================================
// read view
// ============================================================================

fn cmd_list(filter: &mcc::rules::RuleFilter, format: OutputFormat, note: &str) -> Result<()> {
    match format {
        OutputFormat::Text => {
            if !note.is_empty() {
                println!("# {note}");
            }
            println!(
                "{:<6} {:<7} {:<14} {:<16} {:<8} {:<10} {:<24} {}",
                "KEY", "SEV", "SCOPE", "DOMAIN", "GATE", "OVERRIDE", "NAME", "TITLE"
            );
            for m in mcc::rules::query_rules(filter) {
                println!("{}", store::rule_descriptor_line(m));
            }
            Ok(())
        }
        _ => {
            let report = store::rules_list_json(filter);
            print_json(&report)
        }
    }
}

fn cmd_detail(code: &str, format: OutputFormat) -> Result<()> {
    let code = parse_code(code)?;
    match format {
        OutputFormat::Text => {
            let meta = mcc::rules::find_rule(code).ok_or_else(|| unknown_code_error(code))?;
            let mut audit = store::audit_rows(code);
            let sev_conf = audit
                .iter()
                .find(|r| r.kind == "severity")
                .map(|r| r.value.clone().unwrap_or_default());
            audit.retain(|r| r.kind != "severity");
            let effective = if meta.overridable {
                sev_conf.as_deref().unwrap_or(meta.severity.as_str())
            } else {
                meta.severity.as_str()
            };
            println!("{}  {}", store::rule_key(code), meta.name);
            println!("  title:        {}", meta.title);
            println!(
                "  severity:     {} (default) -> {} (effective{}",
                meta.severity.as_str(),
                effective,
                if sev_conf.is_some() {
                    ")"
                } else {
                    "; none configured)"
                }
            );
            println!("  scope:        {}", meta.scope.as_str());
            println!("  domain:       {}", meta.domain.as_str());
            if let Some(fam) = meta.family {
                println!("  family:       {fam}");
            }
            println!(
                "  gate:         {} ({})",
                meta.gate.as_str(),
                meta.sink.as_str()
            );
            println!("  plane:        {}", meta.plane.as_str());
            println!("  acceptance:   {}", meta.acceptance.as_str());
            println!("  cadence:      {}", meta.cadence.as_str());
            println!("  fix:          {}", meta.fix.as_str());
            println!(
                "  overridable:  {}",
                if meta.overridable { "yes" } else { "no" }
            );
            println!("  lock:         {}", meta.lock);
            println!("  doc:          {}", meta.doc);
            if !audit.is_empty() {
                println!("  audit:");
                for r in &audit {
                    let val = match (&r.value, &r.note) {
                        (Some(v), _) => v.clone(),
                        (None, Some(n)) => format!("reason: {n}"),
                        (None, None) => String::new(),
                    };
                    println!(
                        "    - {} layer={:?} path={} {}",
                        r.kind, r.layer, r.path, val
                    );
                }
            } else {
                println!("  audit:        (no overrides or waivers configured)");
            }
            if meta.overridable {
                println!("  allow syntax: {}", store::rule_key(code));
                println!(
                    "    `mcc rules allow {} --path 'boards/**/*.mc' --reason ...`",
                    store::rule_key(code)
                );
                println!(
                    "    `mcc rules accept {} --path boards/dev/main.mc --since 2026-09-05`",
                    store::rule_key(code)
                );
                println!(
                    "    `mcc rules set-severity {} <hint|info|warning|error>`",
                    store::rule_key(code)
                );
            } else {
                println!(
                    "  allow syntax: rule {} is not overridable; severity/allow writes are refused",
                    store::rule_key(code)
                );
            }
            Ok(())
        }
        _ => {
            let detail = store::rule_detail_json(code).map_err(|e| anyhow::anyhow!("{e}"))?;
            print_json(&detail)
        }
    }
}

// ============================================================================
// write face (session store + optional project persistence)
// ============================================================================

fn cmd_set_severity(code: &str, severity: &str, write: bool) -> Result<()> {
    let code = parse_code(code)?;
    let sev = CheckSeverity::from_str(severity.trim()).ok_or_else(|| {
        anyhow::anyhow!("unknown severity '{severity}' (hint|info|warning|error)")
    })?;
    store::session_set_severity(code, sev).map_err(|e| anyhow::anyhow!("{e}"))?;
    if write {
        let root = project_root_for_write()?;
        let mut diag = mcc::cli::config::load_project_diag_config(&root)?.unwrap_or_default();
        diag.severities
            .insert(store::rule_key(code), sev.as_str().to_string());
        let path = mcc::cli::config::save_project_diag_config(&root, &diag)?;
        println!(
            "rule {} severity -> {} (session + project {})",
            store::rule_key(code),
            sev.as_str(),
            path.display()
        );
    } else {
        println!(
            "rule {} severity -> {} (session only; re-run with --write to persist)",
            store::rule_key(code),
            sev.as_str()
        );
    }
    Ok(())
}

fn cmd_add_allow(
    code: &str,
    path: &Option<String>,
    reason: &Option<String>,
    write: bool,
) -> Result<()> {
    let code = parse_code(code)?;
    let scope = store::parse_path_scope(path.as_deref().unwrap_or(""));
    store::session_add_allow(code, scope.clone(), reason.clone())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if write {
        let root = project_root_for_write()?;
        let mut diag = mcc::cli::config::load_project_diag_config(&root)?.unwrap_or_default();
        upsert_allow(&mut diag, code, path, reason);
        let path = mcc::cli::config::save_project_diag_config(&root, &diag)?;
        println!(
            "rule {} allowed at '{}' (session + project {})",
            store::rule_key(code),
            store::path_display(&scope),
            path.display()
        );
    } else {
        println!(
            "rule {} allowed at '{}' (session only; re-run with --write to persist)",
            store::rule_key(code),
            store::path_display(&scope)
        );
    }
    Ok(())
}

fn cmd_add_accept(
    code: &str,
    path: &Option<String>,
    since: &Option<String>,
    write: bool,
) -> Result<()> {
    let code = parse_code(code)?;
    let scope = store::parse_path_scope(path.as_deref().unwrap_or(""));
    store::session_add_accept(code, scope.clone(), since.clone())
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if write {
        let root = project_root_for_write()?;
        let mut diag = mcc::cli::config::load_project_diag_config(&root)?.unwrap_or_default();
        upsert_accept(&mut diag, code, path, since);
        let path = mcc::cli::config::save_project_diag_config(&root, &diag)?;
        println!(
            "rule {} accepted at '{}' (session + project {})",
            store::rule_key(code),
            store::path_display(&scope),
            path.display()
        );
    } else {
        println!(
            "rule {} accepted at '{}' (session only; re-run with --write to persist)",
            store::rule_key(code),
            store::path_display(&scope)
        );
    }
    Ok(())
}

fn upsert_allow(
    diag: &mut mcc::cli::config::DiagConfig,
    code: u32,
    path: &Option<String>,
    reason: &Option<String>,
) {
    use mcc::cli::config::AllowRow;
    let key = store::rule_key(code);
    let path_val = path.as_deref().unwrap_or("");
    diag.allows
        .retain(|r| !(r.rule == key && r.path.as_deref().unwrap_or("") == path_val));
    diag.allows.push(AllowRow {
        rule: key,
        path: path.clone(),
        reason: reason.clone(),
    });
}

fn upsert_accept(
    diag: &mut mcc::cli::config::DiagConfig,
    code: u32,
    path: &Option<String>,
    since: &Option<String>,
) {
    use mcc::cli::config::AcceptRow;
    let key = store::rule_key(code);
    let path_val = path.as_deref().unwrap_or("");
    diag.accepts
        .retain(|r| !(r.rule == key && r.path.as_deref().unwrap_or("") == path_val));
    diag.accepts.push(AcceptRow {
        rule: key,
        path: path.clone(),
        since: since.clone(),
    });
}

// ============================================================================
// helpers
// ============================================================================

fn parse_code(code: &str) -> Result<u32> {
    store::parse_rule_code(code)
        .ok_or_else(|| anyhow::anyhow!("invalid rule code '{code}' (use e.g. E4101 or 4101)"))
}

fn unknown_code_error(code: u32) -> anyhow::Error {
    anyhow::anyhow!(
        "unknown rule code {}; run `mcc rules list` for the catalog",
        store::rule_key(code)
    )
}

/// Walk up from the current directory to the project root containing
/// `project.toml`; `--write` persistence requires one (design §8-5: only the
/// CLI `--write` merges back into the project config).
fn project_root_for_write() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let mut dir: Option<&Path> = Some(&cwd);
    while let Some(d) = dir {
        if mcc::cli::datadir::find_manifest_in(d).is_some() {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }
    Err(anyhow::anyhow!(
        "no project.toml under {}; `mcc rules ... --write` persists into the project config (rule-registry design §8-5)",
        cwd.display()
    ))
}

fn print_json(v: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(v)?);
    Ok(())
}
