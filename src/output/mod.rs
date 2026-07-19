// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Output formatting layer.
//!
//! ## PR-2 changes
//!
//! Before: there was only one generic [`emit`] function, writing any `Serialize + Display` to
//! stdout or a file. Each command constructed its own report type, not reused.
//!
//! Now: added the [`emit_envelope`] path. When the user passes `--json` (or explicitly `--format json`),
//! the command should take the envelope path; otherwise continue with the [`emit`] text path (backward compatible).

pub mod builder;
pub mod compact;
pub mod diagnostic;
pub mod envelope;
pub mod renderer;

use anyhow::Result;
use mcc::cli::OutputFormat;
use serde::Serialize;
use std::fmt::Display;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

// ============================================================================
// Old API: emit (backward compatible, untouched)
// ============================================================================

/// Output any `Serialize + Display` to stdout or a file.
///
/// After PR-2, new code is recommended to use [`emit_envelope`]; [`emit`] is only for simple
/// KPI output like `--summary`, or commands not yet refactored to use envelope.
pub fn emit<T>(value: &T, format: OutputFormat, target: Option<&Path>) -> Result<()>
where
    T: Serialize + Display,
{
    let buf = render(value, format)?;
    write_out(&buf, target)
}

fn render<T>(value: &T, format: OutputFormat) -> Result<String>
where
    T: Serialize + Display,
{
    Ok(match format {
        OutputFormat::Text => format!("{}", value),
        OutputFormat::Json => serde_json::to_string(value)?,
        OutputFormat::JsonPretty => serde_json::to_string_pretty(value)?,
        OutputFormat::Yaml => serde_yaml::to_string(value)?,
        // CSV is rendered by callers (extract/export), not by emit().
        OutputFormat::Csv => format!("{}", value),
    })
}

// ============================================================================
// New API: emit_envelope - main entry of PR-2
// ============================================================================

/// Output [`Envelope`] to stdout or a file.
///
/// The caller invokes this once after assembling the result. Behavior:
///
/// - `format = Json | JsonPretty | Yaml`: serialize the entire envelope
/// - `format = Text`: go through [`render_envelope_text`] for human-readable output
///
/// **Important convention**: in command implementations, JSON mode should be **completely silent**
/// (no eprintln decoration). This function does not handle stderr itself, but your command dispatch
/// should check [`is_structured`](OutputFormat).
pub fn emit_envelope(
    env: &envelope::Envelope,
    format: OutputFormat,
    target: Option<&Path>,
    skip_diagnostics: bool,
) -> Result<()> {
    let buf = match format {
        OutputFormat::Json => serde_json::to_string(env)?,
        OutputFormat::JsonPretty => serde_json::to_string_pretty(env)?,
        OutputFormat::Yaml => serde_yaml::to_string(env)?,
        OutputFormat::Text => render_envelope_text(env, skip_diagnostics),
        // CSV is structured data emitted by individual commands (extract,
        // export), not by emit_envelope. Fall through to text here so the
        // match is exhaustive; commands that support CSV render the artifact
        // directly to stdout/file.
        OutputFormat::Csv => render_envelope_text(env, skip_diagnostics),
    };
    write_out(&buf, target)
}

// ============================================================================
// Text rendering — render envelope as a human-readable report
// ============================================================================

/// Render the envelope's result into human-readable text similar to before PR-1.
///
/// This only replicates the "top-level structure" rendering: command / workspace / brief summary of each pass.
/// The real deeply nested tables like "module instance tree / connections / nets" are output by the
/// command side using `crate::commands::print::*` directly via `eprintln!` (unified in PR-3).
///
/// What is rendered here is the **final summary on stdout**, not the progress decoration on stderr.
pub fn render_envelope_text(env: &envelope::Envelope, skip_diagnostics: bool) -> String {
    let mut out = String::new();

    if let Some(err) = &env.error {
        out.push_str(&format!("✗ Error [{}]: {}\n", err.code, err.message));
        if let Some(d) = &err.data {
            out.push_str(&format!("  data: {}\n", d));
        }
        return out;
    }

    let Some(r) = &env.result else {
        out.push_str("(empty result)\n");
        return out;
    };

    out.push_str(&format!(
        "● {} [{}: {}]\n",
        r.command,
        format!("{:?}", r.workspace.kind).to_lowercase(),
        r.workspace.name
    ));

    if !skip_diagnostics {
        if let Some(p) = &r.pass0 {
            out.push_str(&format!(
                "  pass0: {} files, {} diagnostics\n",
                p.loaded_files.len(),
                p.diagnostics.len()
            ));
            for d in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(d)));
            }
        }

        if let Some(p) = &r.pass1 {
            out.push_str(&format!(
                "  pass1: {} files, {} modules, {} components, {} interfaces, {} diagnostics\n",
                p.loaded_files.len(),
                p.definitions.modules.len(),
                p.definitions.components.len(),
                p.definitions.interfaces.len(),
                p.diagnostics.len()
            ));
            for d in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(d)));
            }
        }

        if let Some(p) = &r.pass2 {
            out.push_str(&format!(
                "  pass2: top={}, {} nets, {} connections, {} diagnostics\n",
                p.top,
                p.nets.len(),
                p.connections.len(),
                p.diagnostics.len()
            ));
            for d in &p.diagnostics {
                out.push_str(&format!("    {}\n", format_diagnostic(d)));
            }
        }
    }

    if let Some(e) = &r.extract {
        out.push_str(&format!("  extract: target={}\n", e.target));
    }

    if let Some(view) = &r.view {
        // tree/ast output: serialize data as formatted JSON for printing
        let tree_str = serde_json::to_string_pretty(&view.data).unwrap_or_default();
        out.push_str(&format!(
            "  {}: ({} nodes)\n",
            view.target,
            view.data.as_array().map(|a| a.len()).unwrap_or(0)
        ));
        for line in tree_str.lines() {
            out.push_str(&format!("    {}\n", line));
        }
    }

    if let Some(v) = &r.viz {
        out.push_str(&format!(
            "  viz: format={}, {} bytes{}\n",
            v.format,
            v.bytes,
            v.written_to
                .as_deref()
                .map(|p| format!(", written to {}", p))
                .unwrap_or_default()
        ));
    }

    if let Some(s) = &r.search {
        out.push_str(&format!(
            "  search: pattern={:?}, kind={}, regex={}, fuzzy={}, count={}\n",
            s.pattern,
            s.kind.as_deref().unwrap_or("*"),
            s.regex,
            s.fuzzy,
            s.count
        ));
    }

    if let Some(q) = &r.query {
        out.push_str(&format!("  query: expr={:?}, count={}\n", q.expr, q.count));
    }

    if let Some(e) = &r.export {
        out.push_str(&format!(
            "  export: kind={}, format={}, count={}\n",
            e.kind, e.format, e.count
        ));
    }

    let s = &r.summary;
    if skip_diagnostics {
        out.push_str("\n═══════════════════════════════════════════════════════════════\n");
        out.push_str(" Summary\n");
        out.push_str("═══════════════════════════════════════════════════════════════\n");
    }
    out.push_str(&format!(
        "  summary: errors={}, warnings={}, elapsed={}ms\n",
        s.errors, s.warnings, s.elapsed_ms
    ));

    out
}

/// Format a single [`envelope::Diagnostic`] into a rustc-style single-line text.
///
/// Looks like `error[E1309] foo.mc:10:5: message` — `2>&1 | grep E1309` can catch it directly.
///
/// When `location` is missing (rare, mostly INFO/HINT global diagnostics), degrades to
/// `error[E1309]: message`, without the file:line:col prefix.
pub fn format_diagnostic(d: &envelope::Diagnostic) -> String {
    let level = match d.severity {
        envelope::Severity::Error => "error",
        envelope::Severity::Warning => "warning",
        envelope::Severity::Info => "info",
        envelope::Severity::Hint => "hint",
    };
    let code = format!("E{:04}", d.code);
    match &d.location {
        Some(loc) => format!(
            "{}[{}] {}:{}:{}: {}",
            level, code, loc.file, loc.line, loc.column, d.message
        ),
        None => format!("{}[{}]: {}", level, code, d.message),
    }
}

// ============================================================================
// Write helper
// ============================================================================

fn write_out(buf: &str, target: Option<&Path>) -> Result<()> {
    match target {
        Some(p) => {
            let f = File::create(p)?;
            let mut w = BufWriter::new(f);
            w.write_all(buf.as_bytes())?;
            if !buf.ends_with('\n') {
                w.write_all(b"\n")?;
            }
            Ok(())
        }
        None => {
            print!("{}", buf);
            if !buf.ends_with('\n') {
                println!();
            }
            Ok(())
        }
    }
}

// ============================================================================
// OutputFormat extension
// ============================================================================

/// Write any serializable data in the specified format to stdout or a file
pub fn print_report<T: Serialize + std::fmt::Display>(
    data: &T,
    format: OutputFormat,
    output: &Option<String>,
) -> Result<()> {
    let target = output.as_ref().map(|s| Path::new(s.as_str()));
    emit(data, format, target)
}

pub trait OutputFormatExt {
    /// JSON / JsonPretty / Yaml count as structured (go through envelope), Text and Csv do not.
    fn is_structured(&self) -> bool;
}

impl OutputFormatExt for OutputFormat {
    fn is_structured(&self) -> bool {
        matches!(
            self,
            OutputFormat::Json | OutputFormat::JsonPretty | OutputFormat::Yaml
        )
    }
}

#[cfg(test)]
mod tests {
    use super::envelope::*;
    use super::*;

    #[test]
    fn structured_check() {
        assert!(OutputFormat::Json.is_structured());
        assert!(OutputFormat::JsonPretty.is_structured());
        assert!(OutputFormat::Yaml.is_structured());
        assert!(!OutputFormat::Text.is_structured());
    }

    #[test]
    fn text_renders_minimal_envelope() {
        let e = Envelope::ok(CommandResult {
            command: "mcc load".into(),
            workspace: WorkspaceRef::project("test"),
            ..Default::default()
        });
        let s = render_envelope_text(&e, false);
        assert!(s.contains("mcc load"));
        assert!(s.contains("[project: test]"));
        assert!(s.contains("summary"));
    }

    #[test]
    fn text_renders_error() {
        let e = Envelope::err(RpcError::parse_error("bad"));
        let s = render_envelope_text(&e, false);
        assert!(s.contains("✗ Error"));
        assert!(s.contains("-32001"));
    }
}
