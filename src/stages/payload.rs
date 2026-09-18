// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! A stage view as envelope payload — the one place its field list lives.
//!
//! [`crate::output::CommandResult`] carries a stage view under its own sibling
//! key, and the MCP server returns the same payload over its own transport. Both
//! spellings of "a stage view on the wire" come from here, so a field added to
//! the view is added once: a caller that spelled the list out itself would
//! serialize `null` for a field it had a value for, and nothing would fail.
//!
//! The typed struct lives here rather than beside the envelope because the MCP
//! server is a second binary: it can reach this crate's public API and not the
//! CLI's private envelope module.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::stages::StageView;

/// Stage readout — `mcc show stage <p1|p2|vec|viz>`, `mcc join <a> <b>`,
/// `mcc trace <KEY>` (design `mcd/doc/pipeline/stage-readout-design.md` §3 /
/// §5.3).
///
/// This is the **minimal projection envelope**: the fields of
/// `projection-schema-design.md` §1, carried as one more `view` value on the
/// existing envelope rather than as a second envelope format (law B). `view` is
/// `stage.<seg>` and names a *pipeline stage*, which is why it does not
/// impersonate one of the six read-side projections of the frozen world.
///
/// Every key is always emitted, `null` included. That is what makes the key set
/// identical across views, which is the readable form of law B's claim that
/// these are one envelope and not six — a consumer can ask for a field without
/// first asking which view it is reading.
#[derive(Debug, Serialize, Deserialize)]
pub struct StageViewData {
    /// Envelope schema version (`proj.1.0`).
    pub schema_version: String,
    /// Root token: the loaded world's source set as a deterministic hash, or
    /// null when it cannot be fingerprinted. See `mcc::stages::world_ver`.
    pub world_ver: Option<String>,
    /// The same material restricted to this view's top, or null.
    /// See `mcc::stages::top_ver`.
    pub top_ver: Option<String>,
    /// The compiler that produced this view.
    pub mcc_version: String,
    /// Drawing contract versions, non-null on `stage.viz` alone — the sole
    /// producer of the drawing face. Declared, not derived: see
    /// `mcc::stages::StageView::carrying_drawing_contract`.
    pub layout_version: Option<String>,
    /// See `layout_version`.
    pub render_version: Option<String>,
    /// See `layout_version`.
    pub metric_schema_version: Option<String>,
    /// `stage.p1` | `stage.p2` | `stage.vec` | `stage.viz`.
    pub view: String,
    /// The resolved top module this view is scoped to.
    pub top: String,
    /// Sorted by `(class, key)`. Every item carries its run-local key
    /// (`point`) *and* its cross-build `canon_key` — a view carrying only the
    /// former is invalidated by the next compiler change (design §3).
    pub items: Value,
    /// Per-class item counts plus the diagnostic base. A count in the header
    /// line, never a gate (law C).
    pub counts: Value,
    /// The second side of a difference, and the parts of the answer that are
    /// not change rows. `Some` only on `mcc diff`.
    ///
    /// A difference belongs to **two** worlds, so one token pair cannot say what
    /// was compared: the fields above are side A's (the reference the `items`
    /// are stated against), and this block names side B. It also carries what a
    /// change row cannot: `unaligned` (an item whose class key function produced
    /// nothing — a statement that no key existed, which a count would lose),
    /// `nameless_net_pins` (a pair of counts, deliberately not a list), and the
    /// M12 `stability` summary.
    ///
    /// Omitted everywhere else, so the four `stage.*` views and `join` / `trace`
    /// keep exactly the key set they had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<Value>,
}

/// The one place a [`StageView`] becomes envelope data.
///
/// `show stage`, `join` and `trace` each emit one, and each used to spell the
/// whole field list out: three copies of a mapping that has no per-caller
/// variation. The mapping is the envelope's, not theirs — a fourth field added
/// here would otherwise have to be added in three files, and a caller that
/// forgot would serialize `null` for a field it had a value for.
impl From<&StageView> for StageViewData {
    fn from(view: &StageView) -> Self {
        Self {
            schema_version: view.schema_version.to_string(),
            world_ver: view.world_ver.clone(),
            top_ver: view.top_ver.clone(),
            mcc_version: view.mcc_version.clone(),
            layout_version: view.layout_version.clone(),
            render_version: view.render_version.clone(),
            metric_schema_version: view.metric_schema_version.clone(),
            view: view.view.to_string(),
            top: view.top.clone(),
            items: Value::Array(view.items.clone()),
            counts: view.counts.clone(),
            diff: None,
        }
    }
}

impl StageViewData {
    /// Attach the second side of a difference.
    ///
    /// Mirrors `StageView::carrying_drawing_contract`: a field that only one
    /// producer has a value for is attached by that producer, at the one call
    /// site that has it, rather than added to the conversion every view goes
    /// through.
    pub fn carrying_second_side(mut self, diff: Value) -> Self {
        self.diff = Some(diff);
        self
    }
}

/// A view as the `result` body of one command: the same shape the CLI prints
/// under `result` for the same call.
///
/// `command` is the caller's, because the command *is* the reading — `mcc show
/// stage` and `mcc join` publish different `view` vocabularies — and `top` /
/// `items` / `counts` come from the view.
///
/// `summary` here is the one a stage-only result carries: the loaded world's
/// definition counts and a wall-clock `elapsed_ms`, with the instance and net
/// counts at zero because a stage view is not a Pass2 report. A test pins this
/// against the CLI's own builder, field for field, minus the clock.
pub fn result(command: &str, view: &StageView, elapsed_ms: u128) -> Value {
    json!({
        "command": command,
        "workspace": { "kind": "project", "name": "default" },
        "stage": serde_json::to_value(StageViewData::from(view))
            .unwrap_or(Value::Null),
        "summary": {
            "module_count": crate::mcb_module_count(),
            "component_count": crate::mcb_component_count(),
            "interface_count": crate::mcb_interface_count(),
            "instance_count": 0,
            "net_count": 0,
            "errors": 0,
            "warnings": 0,
            "elapsed_ms": elapsed_ms,
        },
    })
}
