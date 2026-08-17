// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Compact `.mc`-like text renderer for entity dump data.
//!
//! Renders JSON dump output (from `show dump`) in a syntax resembling the
//! original `.mc` source file, making it easy to diff against the source
//! to spot parsing issues.
//!
//! ## Notation conventions
//!
//! - `.` (dot) and `{}` (curly braces) are **semantically equivalent** for
//!   member/sub access. The renderer uses `{}` notation exclusively.
//!   E.g. `DC2.VDD` in source → `DC2{VDD}` in dump.
//! - `[VDD1, GND1]` (square brackets) denotes an anonymous list/group.
//!
//! # Reusable entry points
//!
//! - [`render_entity`] — render a single entity JSON (has a `kind` field)
//! - [`render_all`] — render a `{"type":"dump_all","entities":[...]}` envelope

use serde_json::Value;

// ============================================================================
// Public API
// ============================================================================

/// Render a dump_all envelope (`{"type":"dump_all","entities":[...]}`).
/// An optional `file` field is shown in the header for file-scoped dumps.
/// `show_span` controls whether source position spans are rendered.
pub fn render_all(data: &Value, show_span: bool) -> String {
    let total = data["total"].as_u64().unwrap_or(0);
    let file = data["file"].as_str().unwrap_or("");
    let mut out = if file.is_empty() {
        format!("=== {} entities ===\n", total)
    } else {
        format!("=== {} entities in {} ===\n", total, file)
    };
    if let Some(entities) = data["entities"].as_array() {
        for (i, e) in entities.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&render_entity(e, show_span));
        }
    }
    out
}

/// Render a single entity JSON (has `kind` and `name` fields).
/// `show_span` controls whether source position spans are rendered.
pub fn render_entity(e: &Value, show_span: bool) -> String {
    let kind = e["kind"].as_str().unwrap_or("unknown");
    let name = e["name"].as_str().unwrap_or("?");
    let span = span_suffix(e, show_span, false);
    let mut out = format!("{} {}{}\n", kind, name, span);

    match kind {
        "component" => {
            params(&mut out, e);
            pins(&mut out, e);
            attrs(&mut out, e);
            funcs(&mut out, e);
            instances(&mut out, e, show_span);
            layout(&mut out, e);
            let cpc = e["cond_pins_count"].as_u64().unwrap_or(0);
            let cac = e["cond_attrs_count"].as_u64().unwrap_or(0);
            if cpc > 0 || cac > 0 {
                out.push_str(&format!("  cond_pins: {}, cond_attrs: {}\n", cpc, cac));
            }
        }
        "module" => {
            params(&mut out, e);
            instances(&mut out, e, show_span);
            defs(&mut out, e, show_span);
            refs(&mut out, e, show_span);
            lines(&mut out, e);
            funcs(&mut out, e);
        }
        "interface" => {
            params(&mut out, e);
            pins(&mut out, e);
            attrs(&mut out, e);
            roles(&mut out, e);
        }
        "enum" => {
            values(&mut out, e);
        }
        _ => {}
    }
    out
}

// ============================================================================
// Section renderers (pub so other modules can compose custom views)
// ============================================================================

/// Render `params` array.
pub fn params(out: &mut String, e: &Value) {
    if let Some(arr) = e["params"].as_array() {
        if !arr.is_empty() {
            let names: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
            out.push_str(&format!("  params: {}\n", names.join(", ")));
        }
    }
}

/// Render `pins` array with `pin_count`, including non-string pin values
/// (KVS, ranges, etc.) stored under the per-pin `values` field.
pub fn pins(out: &mut String, e: &Value) {
    if let Some(arr) = e["pins"].as_array() {
        out.push_str(&format!("  pins ({}):\n", arr.len()));
        for p in arr {
            let id = p["id"].as_str().unwrap_or("?");
            let iotype = p["iotype"].as_str().unwrap_or("");
            let names: Vec<&str> = p["names"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let desc = p["description"].as_str().unwrap_or("");
            let vals: Vec<&str> = p["values"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            let vals_str = if vals.is_empty() {
                String::new()
            } else {
                format!(" {}", vals.join(" "))
            };
            let extra = if desc.is_empty() {
                format!("[{}] {}{}", names.join(", "), iotype, vals_str)
            } else {
                format!("[{}] {} \"{}\"{}", names.join(", "), iotype, desc, vals_str)
            };
            out.push_str(&format!("    {}: {}\n", id, extra));
        }
    }
}

/// Render `attrs` array.
pub fn attrs(out: &mut String, e: &Value) {
    if let Some(arr) = e["attrs"].as_array() {
        if !arr.is_empty() {
            out.push_str(&format!("  attrs ({}):\n", arr.len()));
            for a in arr {
                let name = a["name"].as_str().unwrap_or("?");
                let vals: Vec<String> = a["values"]
                    .as_array()
                    .map(|arr| arr.iter().map(|v| compact_val(v)).collect())
                    .unwrap_or_default();
                out.push_str(&format!("    {} = {}\n", name, vals.join(", ")));
            }
        }
    }
}

/// Render `funcs` array (name, params, returns, body_lines).
pub fn funcs(out: &mut String, e: &Value) {
    if let Some(arr) = e["funcs"].as_array() {
        if !arr.is_empty() {
            out.push_str(&format!("  funcs ({}):\n", arr.len()));
            for f in arr {
                let fname = f["name"].as_str().unwrap_or("?");
                let p: Vec<&str> = f["params"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                let returns = f["returns"].as_str().unwrap_or("");
                out.push_str(&format!("    {}({}) -> {}\n", fname, p.join(", "), returns));
                if let Some(body) = f["body_lines"].as_array() {
                    for l in body {
                        out.push_str(&format!("      {}\n", compact_val(l)));
                    }
                }
            }
        }
    }
}

/// Render `instances` array with class name and construction params.
/// `show_span` controls the per-instance span.
pub fn instances(out: &mut String, e: &Value, show_span: bool) {
    if let Some(arr) = e["instances"].as_array() {
        if !arr.is_empty() {
            out.push_str(&format!("  instances ({}):\n", arr.len()));
            for inst in arr {
                let iname = inst["name"].as_str().unwrap_or("?");
                let kind = inst["kind"].as_str().unwrap_or("?");
                let class = inst["class"].as_str().unwrap_or("");
                let span = span_suffix(inst, show_span, false);
                // Omit the class when it repeats the instance name (e.g. a
                // label or plain bus); component/module/interface instances
                // always carry a distinct class name after resolution.
                let class_str = if class.is_empty() || class == iname {
                    String::new()
                } else {
                    format!(" {}", class)
                };
                let params_str = inst["params"]
                    .as_array()
                    .filter(|a| !a.is_empty())
                    .map(|a| {
                        let inner = a
                            .iter()
                            .filter_map(|p| p.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("({})", inner)
                    })
                    .unwrap_or_default();
                out.push_str(&format!(
                    "    {}{}: {}{}{}\n",
                    iname, span, kind, class_str, params_str
                ));
            }
        }
    }
}

/// Render `lines` array (module connection phrases).
pub fn lines(out: &mut String, e: &Value) {
    if let Some(arr) = e["lines"].as_array() {
        if !arr.is_empty() {
            out.push_str(&format!("  lines ({}):\n", arr.len()));
            for l in arr {
                out.push_str(&format!("    {}\n", compact_val(l)));
            }
        }
    }
}

/// Render `layout` object.
pub fn layout(out: &mut String, e: &Value) {
    let lo = &e["layout"];
    let l = lo["left"].as_array().map(|a| a.len()).unwrap_or(0);
    let r = lo["right"].as_array().map(|a| a.len()).unwrap_or(0);
    let t = lo["top"].as_array().map(|a| a.len()).unwrap_or(0);
    let b = lo["bottom"].as_array().map(|a| a.len()).unwrap_or(0);
    if l + r + t + b > 0 {
        out.push_str(&format!("  layout: L:{} R:{} T:{} B:{}\n", l, r, t, b));
    }
}

/// Render `roles` array.
pub fn roles(out: &mut String, e: &Value) {
    if let Some(arr) = e["roles"].as_array() {
        if !arr.is_empty() {
            out.push_str(&format!("  roles ({}):\n", arr.len()));
            for r in arr {
                let rname = r["name"].as_str().unwrap_or("?");
                let pc = r["pins"]["pin_count"].as_u64().unwrap_or(0);
                out.push_str(&format!("    {} ({} pins)\n", rname, pc));
            }
        }
    }
}

/// Render `defs` array (LSP definition spans for goto-definition).
/// `show_span` controls the per-def span.
pub fn defs(out: &mut String, e: &Value, show_span: bool) {
    if let Some(arr) = e["defs"].as_array() {
        if !arr.is_empty() {
            out.push_str(&format!("  defs ({}):\n", arr.len()));
            for d in arr {
                let name = d["name"].as_str().unwrap_or("?");
                let span = span_suffix(d, show_span, true);
                out.push_str(&format!("    {}{}\n", name, span));
            }
        }
    }
}

/// Render `refs` array (LSP reference spans in net lines).
/// `show_span` controls the per-ref span.
pub fn refs(out: &mut String, e: &Value, show_span: bool) {
    if let Some(arr) = e["refs"].as_array() {
        if !arr.is_empty() {
            out.push_str(&format!("  refs ({}):\n", arr.len()));
            for r in arr {
                let name = r["name"].as_str().unwrap_or("?");
                let scope = r["scope"].as_str().unwrap_or("");
                let span = span_suffix(r, show_span, true);
                let scope_str = if scope.is_empty() {
                    String::new()
                } else {
                    format!(" in {}", scope)
                };
                out.push_str(&format!("    {}{}{}\n", name, span, scope_str));
            }
        }
    }
}

/// Format a span as `@start:end`, prefixed with a leading space when
/// `leading_space` is set. Returns an empty string when `show_span` is false
/// or the value carries no span, so hidden spans leave no output residue.
fn span_suffix(v: &Value, show_span: bool, leading_space: bool) -> String {
    if !show_span {
        return String::new();
    }
    let Some(s) = v.get("span") else {
        return String::new();
    };
    let mut out = if leading_space { String::from(" ") } else { String::new() };
    out.push_str(&format!(
        "@{}:{}",
        s["start"].as_u64().unwrap_or(0),
        s["end"].as_u64().unwrap_or(0)
    ));
    out
}

/// Render `values` array (enum variants).
pub fn values(out: &mut String, e: &Value) {
    if let Some(arr) = e["values"].as_array() {
        if !arr.is_empty() {
            let names: Vec<&str> = arr.iter().filter_map(|v| v["name"].as_str()).collect();
            out.push_str(&format!("  values ({}): {}\n", arr.len(), names.join(", ")));
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Format a JSON value compactly for inline display.
pub fn compact_val(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => {
            let s = v.to_string();
            if s.len() > 120 {
                format!("{}...", &s[..120])
            } else {
                s
            }
        }
    }
}
