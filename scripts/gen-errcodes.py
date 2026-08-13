#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Regenerate src/db/diagnostic/errcodes.rs from scripts/error-code-mapping.json.

Phase 1 of the error-code unification plan: the registry becomes the single
source of truth for all diagnostic codes. Run from the mcc repo root:

    python3 scripts/gen-errcodes.py
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
MAP = os.path.join(HERE, "error-code-mapping.json")
OUT = os.path.join(HERE, "..", "src", "db", "diagnostic", "errcodes.rs")

SECTIONS = [
    (1000, 1049, "Pass1a: duplicate definitions"),
    (1050, 1099, "Pass1a: definition structure / CMIE load"),
    (2000, 2049, "Pass1b: use statements"),
    (2050, 2079, "Pass1b: use-stage diagnostics"),
    (2080, 2119, "Pass1b: parser / AST messages"),
    (2120, 2199, "Pass1b: name resolution"),
    (3000, 3049, "Pass1c: component definition (pins / attrs / units)"),
    (3050, 3099, "Pass1c: module body"),
    (3100, 3149, "Pass1c: params / functions"),
    (3150, 3199, "Pass1c: instance declaration / reference"),
    (4000, 4049, "Pass2: connection / shape"),
    (4050, 4099, "Pass2: netlist heuristics (D-series / layout)"),
    (4100, 4149, "Pass2: netlist / interface binding"),
    (4150, 4199, "Pass2: instantiation checks"),
    (5000, 5049, "Pass3: duplicate validation"),
    (5050, 5099, "Pass3: naming / style"),
    (5100, 5149, "Pass3: reference integrity"),
    (5150, 5199, "Pass3: ports / pins"),
    (5200, 5249, "Pass3: functions / roles / defaults"),
    (5250, 5299, "Pass3: definition structure (M-series)"),
    (5300, 5349, "Pass3: .int class checks"),
    (5350, 5399, "Pass3: instance / attribute checks"),
    (5400, 5449, "Pass3: enum / expression checks"),
    (5450, 5499, "Pass3: condition blocks"),
    (5500, 5549, "Pass3: hardware checks"),
    (5550, 5599, "Pass3: type / unit compatibility"),
    (5600, 5649, "Pass3: global diagnostics"),
    (6000, 6099, "ERC (electrical rule check)"),
]

HEADER = """// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Error code catalog (M6) — SINGLE SOURCE OF TRUTH.
//!
//! Every diagnostic code emitted by mcc must be declared here, with a symbolic
//! constant, a name, a description, and a message template. The message
//! template is the canonical emission text (with `{0}`, `{1}`… placeholders);
//! emission points render it via [`format_msg()`]. Generated from
//! `scripts/error-code-mapping.json` (see `scripts/gen-errcodes.py`); do not
//! edit by hand — regenerate instead.
//!
//! Numbering follows `mcd/doc/mcc-error-code-unification-plan.md` §3.2:
//! thousands+hundreds = pipeline stage / semantic cluster.
//!   - 1xxx  Pass1a  type collection / definition structure
//!   - 2xxx  Pass1b  use statements / parser / name resolution
//!   - 3xxx  Pass1c  component/module/params/instances
//!   - 4xxx  Pass2   connection / netlist / interface binding
//!   - 5xxx  Pass3   validation checks
//!   - 6xxx  ERC
//!   - 9xxx  reserved
//!
//! ## Adding a new code
//!
//! 1. Add a `pub const` in the appropriate section below.
//! 2. Add a match arm / entry in [`describe()`] / `ALL_CODES`.
//! 3. Regenerate via `python3 scripts/gen-errcodes.py` to keep this file in sync.

// ============================================================================
// Infrastructure
// ============================================================================

/// A human-readable error code entry.
#[derive(Clone)]
pub struct ErrorCodeInfo {
    pub code: u32,
    pub name: &'static str,
    pub description: &'static str,
    /// Canonical emission message template (`{0}`, `{1}`, … placeholders).
    pub message: &'static str,
}

/// All registered error codes (used by `mcc explain` without arguments).
pub fn all_codes() -> &'static [ErrorCodeInfo] {
    &ALL_CODES
}

/// Look up a single error code. Returns `None` if unknown.
pub fn describe(code: u32) -> Option<ErrorCodeInfo> {
    ALL_CODES.iter().find(|e| e.code == code).cloned()
}

/// Render the canonical emission message for `code` by substituting `{i}`
/// placeholders with `args[i]`. Placeholders without a matching argument are
/// left verbatim; unknown codes render an empty string.
pub fn format_msg(code: u32, args: &[&dyn std::fmt::Display]) -> String {
    let Some(tmpl) = ALL_CODES.iter().find(|e| e.code == code) else {
        return String::new();
    };
    let mut out = String::with_capacity(tmpl.message.len());
    let mut rest = tmpl.message;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end_rel) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let end = start + end_rel;
        let inner = &rest[start + 1..end];
        if let Ok(i) = inner.parse::<usize>() {
            if let Some(a) = args.get(i) {
                out.push_str(&a.to_string());
            } else {
                out.push_str(&rest[start..=end]);
            }
        } else {
            out.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Lowest C-parser warning code — used by `mc_code.rs` to dedup overlapping
/// parser diagnostics (warnings are more specific than syntax errors).
pub const PARSER_WARNING_CODE_BASE: u32 = 2111;

macro_rules! entry {
    ($const:ident, $desc:expr, $msg:expr) => {
        ErrorCodeInfo {
            code: $const,
            name: stringify!($const),
            description: $desc,
            message: $msg,
        }
    };
}
"""


def esc(s):
    return s.replace("\\", "\\\\").replace('"', '\\"')


def main():
    with open(MAP, encoding="utf-8") as f:
        mapping = json.load(f)

    # Dedupe by (new code, const name); keep first description/message.
    # message priority: mapping entry `msg` > extracted emission template > description
    tmpl_path = os.path.join(HERE, "emission-templates.json")
    emission_tmpl = {}
    if os.path.exists(tmpl_path):
        with open(tmpl_path, encoding="utf-8") as f:
            emission_tmpl = {int(k): v for k, v in json.load(f).items()}
    by_new = {}
    for e in mapping["mapping"]:
        if e["new"] is None:
            continue
        key = (e["new"], e["name"])
        desc = e.get("desc", "")
        msg = e.get("msg") or emission_tmpl.get(e["new"]) or desc
        by_new.setdefault(key, (desc, msg))

    lines = [HEADER]

    # Group constants by section
    for lo, hi, title in SECTIONS:
        items = sorted(
            ((new, name, d, m) for (new, name), (d, m) in by_new.items() if lo <= new <= hi),
            key=lambda t: t[0],
        )
        if not items:
            continue
        lines.append("// ============================================================================")
        lines.append(f"// {title} ({lo}-{hi})")
        lines.append("// ============================================================================")
        lines.append("")
        for new, name, d, _m in items:
            lines.append(f"/// {d}")
            lines.append(f"pub const {name}: u32 = {new};")
            lines.append("")
        lines.append("")

    lines.append("static ALL_CODES: &[ErrorCodeInfo] = &[")
    for lo, hi, _title in SECTIONS:
        items = sorted(
            ((new, name, d, m) for (new, name), (d, m) in by_new.items() if lo <= new <= hi),
            key=lambda t: t[0],
        )
        if not items:
            continue
        lines.append("    // ---- section ----")
        for new, name, d, m in items:
            lines.append(
                f"    entry!({name}, {json.dumps(d, ensure_ascii=False)}, "
                f"{json.dumps(m, ensure_ascii=False)}),"
            )
    lines.append("];")
    lines.append("")

    with open(os.path.abspath(OUT), "w", encoding="utf-8") as f:
        f.write("\n".join(lines))
    print(f"wrote {os.path.abspath(OUT)}  ({len(by_new)} codes)")


if __name__ == "__main__":
    main()
