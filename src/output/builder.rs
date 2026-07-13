// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Fluent builder for [`CommandResult`].
//!
//! Reduce repetitive code in command files by encapsulating common tasks:
//!
//! - Command name / workspace
//! - Timing (elapsed_ms)
//! - Filling in pass1 / pass2 / extract / view / viz
//! - Final aggregation of summary (counts, errors, warnings, elapsed)
//!
//! These tasks are centralized in one place, so command implementations only need to handle
//! "what data do I have to put in".

use super::envelope::*;
use std::time::Instant;

pub struct ResultBuilder {
    started: Instant,
    result: CommandResult,
}

impl ResultBuilder {
    /// Start building a result. Command names look like "mcc parse", "mcc build", "mcc extract instances".
    pub fn start(command: impl Into<String>) -> Self {
        Self {
            started: Instant::now(),
            result: CommandResult {
                command: command.into(),
                workspace: WorkspaceRef::project("default"),
                ..Default::default()
            },
        }
    }

    pub fn workspace(mut self, ws: WorkspaceRef) -> Self {
        self.result.workspace = ws;
        self
    }

    pub fn set_pass0(&mut self, p: Pass0Report) -> &mut Self {
        self.result.pass0 = Some(p);
        self
    }

    pub fn set_pass1(&mut self, p: Pass1Report) -> &mut Self {
        self.result.pass1 = Some(p);
        self
    }

    pub fn set_pass2(&mut self, p: Pass2Report) -> &mut Self {
        self.result.pass2 = Some(p);
        self
    }

    pub fn set_extract(&mut self, e: ExtractData) -> &mut Self {
        self.result.extract = Some(e);
        self
    }

    pub fn set_view(&mut self, v: ViewData) -> &mut Self {
        self.result.view = Some(v);
        self
    }

    pub fn set_viz(&mut self, v: VizData) -> &mut Self {
        self.result.viz = Some(v);
        self
    }

    pub fn set_search(&mut self, s: SearchData) -> &mut Self {
        self.result.search = Some(s);
        self
    }

    pub fn set_query(&mut self, q: QueryData) -> &mut Self {
        self.result.query = Some(q);
        self
    }

    pub fn set_export(&mut self, e: ExportData) -> &mut Self {
        self.result.export = Some(e);
        self
    }

    /// Print a summary of collected diagnostics (without consuming self).
    /// Useful for displaying diagnostics before Net Summary.
    pub fn print_diagnostics_summary(&self) {
        println!("\n═══════════════════════════════════════════════════════════════");
        println!(" Diagnostics");
        println!("═══════════════════════════════════════════════════════════════");
        println!(
            "● {} [{}: {}]",
            self.result.command,
            format!("{:?}", self.result.workspace.kind).to_lowercase(),
            self.result.workspace.name
        );

        if let Some(p) = &self.result.pass0 {
            println!(
                "  pass0: {} files, {} diagnostics",
                p.loaded_files.len(),
                p.diagnostics.len()
            );
            for d in &p.diagnostics {
                println!("    {}", crate::output::format_diagnostic(d));
            }
        }

        if let Some(p) = &self.result.pass1 {
            println!(
                "  pass1: {} files, {} modules, {} components, {} interfaces, {} diagnostics",
                p.loaded_files.len(),
                p.definitions.modules.len(),
                p.definitions.components.len(),
                p.definitions.interfaces.len(),
                p.diagnostics.len()
            );
            for d in &p.diagnostics {
                println!("    {}", crate::output::format_diagnostic(d));
            }
        }
    }

    /// Calculate the current cumulative error count (does not consume self).
    /// Used for build command exit_code.
    pub fn error_count(&self) -> usize {
        let mut errors = 0usize;
        if let Some(p1) = &self.result.pass1 {
            let (e, _) = super::diagnostic::count_severity(&p1.diagnostics);
            errors += e;
        }
        if let Some(p2) = &self.result.pass2 {
            let (e, _) = super::diagnostic::count_severity(&p2.diagnostics);
            errors += e;
        }
        errors
    }

    /// Finish building and automatically populate Summary.
    ///
    /// summary prefers to use our existing pass1/pass2 fields for aggregation (counts), then falls
    /// back to the library's `mcb_*_count`. The sum of errors/warnings comes from the diagnostics of all phases.
    pub fn finish(mut self) -> CommandResult {
        let mut summary = Summary {
            elapsed_ms: self.started.elapsed().as_millis(),
            ..Default::default()
        };

        // ── Aggregate errors / warnings (across all phase) ──
        if let Some(p0) = &self.result.pass0 {
            let (e, w) = super::diagnostic::count_severity(&p0.diagnostics);
            summary.errors += e;
            summary.warnings += w;
        }

        if let Some(p1) = &self.result.pass1 {
            let (e, w) = super::diagnostic::count_severity(&p1.diagnostics);
            summary.errors += e;
            summary.warnings += w;
            summary.module_count = p1.definitions.modules.len();
            summary.component_count = p1.definitions.components.len();
            summary.interface_count = p1.definitions.interfaces.len();
        } else if self.result.pass0.is_none() {
            summary.module_count = mcc::mcb_module_count();
            summary.component_count = mcc::mcb_component_count();
            summary.interface_count = mcc::mcb_interface_count();
        }

        if let Some(p2) = &self.result.pass2 {
            let (e, w) = super::diagnostic::count_severity(&p2.diagnostics);
            summary.errors += e;
            summary.warnings += w;
            summary.instance_count = count_instances(p2.instances.as_ref());
            summary.net_count = p2.nets.len();
        }

        self.result.summary = summary;
        self.result
    }
}

/// Recursively count total instances in the instance tree (including submodules).
fn count_instances(node: Option<&InstanceNode>) -> usize {
    let Some(n) = node else { return 0 };
    let mut total = 1;
    total += n.components.len();
    for sub in &n.sub_modules {
        total += count_instances(Some(sub));
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_finish_has_zero_counts_and_some_elapsed() {
        let r = ResultBuilder::start("mcc test").finish();
        assert_eq!(r.command, "mcc test");
        assert_eq!(r.summary.errors, 0);
        assert!(r.summary.elapsed_ms <= 60_000);
    }

    #[test]
    fn pass2_instance_count_walks_subtree() {
        let n = InstanceNode {
            name: "main".into(),
            kind: "module".into(),
            class_name: "Main".into(),
            ports: vec![],
            components: vec![ComponentInfo {
                name: "R1".into(),
                class_name: "RES".into(),
                pins: vec![
                    PinInfo {
                        id: "1".into(),
                        name: "1".into(),
                    },
                    PinInfo {
                        id: "2".into(),
                        name: "2".into(),
                    },
                ],
                nc: false,
            }],
            sub_modules: vec![InstanceNode {
                name: "child".into(),
                kind: "module".into(),
                class_name: "Sub".into(),
                ports: vec![],
                components: vec![],
                sub_modules: vec![],
            }],
        };
        // root + 1 component + 1 submodule = 3
        assert_eq!(count_instances(Some(&n)), 3);
    }
}
