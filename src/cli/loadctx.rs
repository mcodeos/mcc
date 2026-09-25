// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The loading context — use-design.md §19.10 D6 phase 1.
//!
//! The five loading channels (global config `[libs].load`, manifest
//! `[dependencies]`, CLI `--lib`, the mcode default, and the RPC request
//! list) resolve through one [`LoadContext`] and load through one
//! [`load_all`]. The CLI resolves with the canonical union
//! ([`resolve_load_context`]); the RPC keeps its documented "explicit
//! request list replaces the config list" policy (see
//! `resolve_libs_rpc`) while sharing [`load_all`]. Converging the two
//! resolution policies is phase 2 (design §19.10 convergence table).

use std::path::{Path, PathBuf};

/// How a workspace was entered — the D6 `workspace_kind` axis.
///
/// Project mode resolves the manifest `[dependencies]` and scopes the
/// config read to the project root; anonymous mode has neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceKind {
    /// A manifest root was found (`project.toml` or an alias).
    Project,
    /// No manifest anywhere up the walk — libraries come from config,
    /// CLI and the mcode default only.
    #[default]
    Anonymous,
}

/// The one resolved loading context (design §19.10 D6 construct 1).
#[derive(Debug, Clone, Default)]
pub struct LoadContext {
    pub workspace_kind: WorkspaceKind,
    pub project_root: Option<PathBuf>,
    /// Manifest `[dependencies]` names (Project mode only).
    pub deps: Vec<String>,
    /// `--lib` (or the RPC request list) names.
    pub cli_libs: Vec<String>,
    /// Global/project config `[libs].load` names.
    pub config_libs: Vec<String>,
    /// The default system libraries (`mcode` unless disabled).
    pub system_libs: Vec<String>,
}

impl LoadContext {
    /// A context that carries already-resolved names — the escape hatch
    /// for a caller whose resolution policy is not the canonical union
    /// (the RPC request list). The names load verbatim, dedup only.
    pub fn from_resolved(names: Vec<String>) -> LoadContext {
        LoadContext {
            workspace_kind: WorkspaceKind::Anonymous,
            cli_libs: names,
            ..LoadContext::default()
        }
    }

    /// The load order: config ∪ deps ∪ cli ∪ system, first occurrence
    /// wins. Deduplication is by exact name, config-first, so a name
    /// given twice across channels loads once.
    pub fn lib_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let push = |name: &str, out: &mut Vec<String>| {
            if !out.iter().any(|l| l == name) {
                out.push(name.to_string());
            }
        };
        for l in &self.config_libs {
            push(l, &mut out);
        }
        for d in &self.deps {
            push(d, &mut out);
        }
        for l in &self.cli_libs {
            push(l, &mut out);
        }
        for s in &self.system_libs {
            push(s, &mut out);
        }
        out
    }
}

/// Resolve one loading context from the project root (if any) and the
/// explicit library names (design §19.10 D6 construct 1).
///
/// This is the CLI's canonical union policy, extracted verbatim from the
/// former `cmds::manifest::collect_libs`: global/project config
/// `[libs].load`, then manifest `[dependencies]`, then the explicit
/// names, each deduplicated against the accumulated list, then the
/// mcode default unless `libs.disable_mcode` suppresses it.
pub fn resolve_load_context(project_root: Option<&Path>, cli_libs: &[String]) -> LoadContext {
    let config_libs = crate::cli::config::get_libs_load_list(project_root).to_vec();
    let mut deps: Vec<String> = Vec::new();
    if let Some(root) = project_root {
        if let Some(path) = crate::cli::manifest::Manifest::find_in(root) {
            if let Ok(manifest) = crate::cli::manifest::Manifest::load(&path) {
                for dep in manifest.dependencies.keys() {
                    if !config_libs.contains(dep) {
                        deps.push(dep.clone());
                    }
                }
            }
        }
    }
    let system_libs = if crate::cli::config::should_load_mcode(project_root) {
        vec!["mcode".to_string()]
    } else {
        Vec::new()
    };
    let workspace_kind = if project_root.is_some() {
        WorkspaceKind::Project
    } else {
        WorkspaceKind::Anonymous
    };
    LoadContext {
        workspace_kind,
        project_root: project_root.map(|p| p.to_path_buf()),
        deps,
        cli_libs: cli_libs.to_vec(),
        config_libs,
        system_libs,
    }
}

/// Load exactly the context's libraries, in [`LoadContext::lib_names`]
/// order, through the shared name loader (design §19.10 D6 construct 2).
///
/// `mcb_load_lib_by_name` skips libraries that are already loaded, so a
/// repeated context is idempotent.
pub fn load_all(ctx: &LoadContext) {
    for name in ctx.lib_names() {
        crate::db::infra::libmgr::mcb_load_lib_by_name(&name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_loadctx__union_order_and_dedup() {
        let ctx = LoadContext {
            config_libs: vec!["a".into(), "b".into()],
            deps: vec!["b".into(), "c".into()],
            cli_libs: vec!["c".into(), "a".into(), "d".into()],
            system_libs: vec!["mcode".into()],
            ..LoadContext::default()
        };
        assert_eq!(
            ctx.lib_names(),
            vec!["a", "b", "c", "d", "mcode"],
            "config first, deps next, cli last, each name once"
        );
    }

    #[test]
    fn cli_loadctx__no_system_libs_when_suppressed() {
        let ctx = LoadContext {
            config_libs: vec!["a".into()],
            system_libs: vec![],
            ..LoadContext::default()
        };
        assert_eq!(ctx.lib_names(), vec!["a"], "disable_mcode leaves no default");
    }

    #[test]
    fn cli_loadctx__resolved_names_load_verbatim() {
        let ctx = LoadContext::from_resolved(vec!["x".into(), "x".into(), "y".into()]);
        assert_eq!(
            ctx.lib_names(),
            vec!["x", "y"],
            "the RPC escape hatch dedups but adds nothing"
        );
        assert_eq!(ctx.workspace_kind, WorkspaceKind::Anonymous);
    }

    #[test]
    fn cli_loadctx__workspace_kind_follows_root() {
        let proj = resolve_load_context(Some(Path::new("/nonexistent-root-for-test")), &[]);
        assert_eq!(proj.workspace_kind, WorkspaceKind::Project);
        let anon = resolve_load_context(None, &[]);
        assert_eq!(anon.workspace_kind, WorkspaceKind::Anonymous);
        assert!(
            anon.deps.is_empty(),
            "anonymous mode reads no manifest dependencies"
        );
    }
}
