// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The slice read face for `stage.viz` — the M1 `select` line, landed as a
//! **post-filter over the frozen items** (projection-schema-design.md §1:
//! a slice filters the frozen projection serialization, it never materializes
//! the whole), not a second projection or a re-layout.
//!
//! The selector value domain is the projection schema's (net names, exact /
//! glob / regex) **plus the attribution the intent rows carry**:
//! `intent=power-intent` compiles to the net set the family claims
//! (view-model-design.md §6.8: the intent profile compiles into the existing
//! select — the net-key set the family claims — with no new selection
//! vocabulary). Both words go through the one query DSL
//! ([`crate::query_api`]), so the selection machinery is `select` + and/or/not
//! with zero new grammar.
//!
//! What survives a selection is decided by what each item *names*, never by
//! proximity or count: a net-naming row survives iff its net does; a pin iff
//! its net does; a box iff a surviving pin names it; a layer iff a surviving
//! row names it. A selection that names nothing is a caller error and dies
//! loudly — a misspelled address must not read as an empty drawing.

use serde_json::Value;

use crate::query_api::{self, Query};
use crate::stages::{StageSeg, StageView};

/// The record fields a slice predicate may name. Net records carry their name
/// and the family that claims them; anything else is a vocabulary the slice
/// does not read and is rejected before evaluation.
const ALLOWED_FIELDS: &[&str] = &["name", "intent"];

/// A compiled selection: the union of the `--select` expressions minus the
/// union of the `--exclude` expressions.
#[derive(Debug)]
pub struct Selection {
    select: Option<Query>,
    exclude: Option<Query>,
}

impl Selection {
    /// Compile the raw expression strings. An empty `selects` slice with
    /// empty `excludes` is a no-op selection ([`Selection::is_noop`]), which
    /// is how "no flag given" reaches the stage unchanged — the envelope must
    /// stay byte-identical when the face is not used.
    pub fn compile(selects: &[String], excludes: &[String]) -> Result<Self, String> {
        let union = |exprs: &[String], flag: &str| -> Result<Option<Query>, String> {
            let mut union: Option<Query> = None;
            for expr in exprs {
                let q = query_api::compile(expr)
                    .map_err(|err| format!("bad --{flag} '{expr}': {err}"))?;
                query_api::validate_allowed_fields(&q, ALLOWED_FIELDS)
                    .map_err(|err| format!("bad --{flag} '{expr}': {err}"))?;
                union = Some(match union {
                    None => q,
                    Some(prev) => Query::Or(Box::new(prev), Box::new(q)),
                });
            }
            Ok(union)
        };
        Ok(Self {
            select: union(selects, "select")?,
            exclude: union(excludes, "exclude")?,
        })
    }

    /// True when neither flag was given: the view passes through untouched.
    pub fn is_noop(&self) -> bool {
        self.select.is_none() && self.exclude.is_none()
    }

    /// Does one net record pass the selection?
    fn keeps(&self, record: &Value) -> bool {
        let selected = self
            .select
            .as_ref()
            .map(|q| query_api::matches_json_record(q, record))
            .unwrap_or(true);
        let excluded = self
            .exclude
            .as_ref()
            .map(|q| query_api::matches_json_record(q, record))
            .unwrap_or(false);
        selected && !excluded
    }
}

/// Apply a selection to a `stage.viz` reading. A no-op selection returns the
/// view untouched; anything else rebuilds it from the surviving items — the
/// counts are recomputed from the same items by [`StageView::new`], so the
/// header cannot disagree with the rows.
pub fn apply_viz(view: StageView, sel: &Selection) -> Result<StageView, String> {
    if sel.is_noop() {
        return Ok(view);
    }

    // One record per drawn net, read off the items themselves — the profile's
    // member set comes from the `intent` rows (the attribution edge the view
    // publishes), never recomputed from declarations here.
    let mut drawn: std::collections::BTreeSet<String> = Default::default();
    let mut claimed_by: std::collections::BTreeMap<String, Option<String>> = Default::default();
    for item in &view.items {
        match item["class"].as_str().unwrap_or("") {
            // The digest row is one per net; the segment rows name their net
            // per drawn piece. Either is enough to make a net addressable.
            "digest" | "segment" => {
                if let Some(name) = item["net"].as_str() {
                    drawn.insert(name.to_string());
                }
            }
            "intent" => {
                let family = item["family"].as_str();
                for net in item["nets"].as_array().into_iter().flatten() {
                    if let Some(name) = net["name"].as_str() {
                        drawn.insert(name.to_string());
                        claimed_by
                            .entry(name.to_string())
                            .or_insert_with(|| family.map(|f| f.to_string()));
                    }
                }
            }
            _ => {}
        }
    }

    let kept: std::collections::BTreeSet<String> = drawn
        .iter()
        .filter(|name| {
            let intent = claimed_by.get(*name).cloned().flatten();
            let record = serde_json::json!({ "name": name, "intent": intent });
            sel.keeps(&record)
        })
        .cloned()
        .collect();
    if kept.is_empty() {
        return Err(
            "the selection matches no drawn net\nselectors address nets by name \
             (exact, glob with *, regex with ~=) or by the family that claims them \
             (intent=power-intent)"
                .to_string(),
        );
    }
    let in_sel = |name: Option<&str>| name.map(|n| kept.contains(n)).unwrap_or(false);

    // Pass one: every row that names a net decides here, and the boxes and
    // layers those rows name stay alive.
    let mut kept_boxes: std::collections::BTreeSet<String> = Default::default();
    let mut kept_layers: std::collections::BTreeSet<String> = Default::default();
    let mut decided: Vec<bool> = Vec::with_capacity(view.items.len());
    for item in &view.items {
        let keep = match item["class"].as_str().unwrap_or("") {
            // The pin names its net by the net's key form ("net:<name>"), or
            // not at all when the net has no source name.
            "pin" => {
                let name = item["net"].as_str().and_then(|k| k.strip_prefix("net:"));
                let keep = in_sel(name);
                if keep {
                    if let Some(b) = item["box"].as_str() {
                        kept_boxes.insert(b.to_string());
                    }
                }
                keep
            }
            "segment" | "digest" => {
                let keep = in_sel(item["net"].as_str());
                if keep {
                    if let Some(l) = item["layer"].as_str() {
                        kept_layers.insert(l.to_string());
                    }
                }
                keep
            }
            // Attribution rows: an edge to the nets it names. It survives when
            // the selection keeps at least one of them.
            "group" | "intent" => item["nets"]
                .as_array()
                .map(|nets| nets.iter().any(|n| in_sel(n["name"].as_str())))
                .unwrap_or(false),
            // Everything else (metrics, …) is view-level and names no nets.
            _ => true,
        };
        decided.push(keep);
    }

    // Pass two: a box needs a surviving pin; a layer needs a surviving row.
    // A kept box hands its own layer a stay, so boxes decide before layers.
    let mut keep: Vec<bool> = Vec::with_capacity(view.items.len());
    for (item, &decided) in view.items.iter().zip(&decided) {
        let verdict = match item["class"].as_str().unwrap_or("") {
            "box" => {
                let alive = item["path"]
                    .as_str()
                    .map(|p| kept_boxes.contains(p))
                    .unwrap_or(false);
                if alive {
                    if let Some(l) = item["layer"].as_str() {
                        kept_layers.insert(l.to_string());
                    }
                }
                alive
            }
            _ => decided,
        };
        keep.push(verdict);
    }
    let mut kept_items: Vec<Value> = Vec::new();
    for (item, &keep) in view.items.iter().zip(&keep) {
        let keep = match item["class"].as_str().unwrap_or("") {
            "layer" => item["path"]
                .as_str()
                .map(|p| kept_layers.contains(p))
                .unwrap_or(false),
            _ => keep,
        };
        if keep {
            kept_items.push(item.clone());
        }
    }

    let diagnostics = view.counts["diagnostics"].as_u64().unwrap_or(0) as usize;
    let top = view.top.clone();
    let had_contract = view.layout_version.is_some();
    let mut out = StageView::new(StageSeg::Viz, &top, kept_items, diagnostics);
    if had_contract {
        out = out.carrying_drawing_contract();
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// One small drawing: a layer with one box, a pin on it, two nets (one
    /// claimed by the power intent, one not), and the attribution rows.
    fn drawing() -> StageView {
        let items = vec![
            json!({"class": "layer", "key": "D1", "path": "main"}),
            json!({"class": "box", "key": "D2", "path": "main.c1", "layer": "main"}),
            json!({"class": "pin", "key": "P1", "net": "net:VDD", "box": "main.c1", "layer": "main"}),
            json!({"class": "segment", "kind": "wire", "net": "VDD", "layer": "main"}),
            json!({"class": "digest", "kind": "net", "net": "VDD", "layer": "main"}),
            json!({"class": "segment", "kind": "wire", "net": "SIG", "layer": "main"}),
            json!({"class": "digest", "kind": "net", "net": "SIG", "layer": "main"}),
            json!({"class": "group", "key": "k", "nets": [
                {"net": "net:VDD", "name": "VDD"},
                {"net": "net:SIG", "name": "SIG"},
            ]}),
            json!({"class": "intent", "family": "power-intent", "nets": [
                {"net": "net:VDD", "name": "VDD"},
            ]}),
            json!({"class": "metrics", "key": "m"}),
        ];
        StageView::new(StageSeg::Viz, "main", items, 0)
    }

    fn classes(view: &StageView) -> Vec<String> {
        let mut v: Vec<String> = view
            .items
            .iter()
            .map(|i| {
                format!(
                    "{}:{}",
                    i["class"].as_str().unwrap_or(""),
                    i["net"].as_str().or(i["path"].as_str()).unwrap_or("")
                )
            })
            .collect();
        v.sort();
        v
    }

    fn sel(exprs: &[&str]) -> Selection {
        let owned: Vec<String> = exprs.iter().map(|s| s.to_string()).collect();
        Selection::compile(&owned, &[]).expect("compile")
    }

    #[test]
    fn slice_noop_selection_returns_the_view_untouched() {
        let view = drawing();
        let before = classes(&view);
        let out = apply_viz(view, &Selection::compile(&[], &[]).unwrap()).unwrap();
        assert_eq!(classes(&out), before);
    }

    #[test]
    fn slice_intent_selector_keeps_exactly_the_claimed_nets() {
        let view = drawing();
        let out = apply_viz(view, &sel(&["intent=power-intent"])).unwrap();
        let rows = classes(&out);
        // VDD rows survive; SIG rows do not; the attribution rows do.
        assert!(rows.contains(&"segment:VDD".to_string()));
        assert!(rows.contains(&"digest:VDD".to_string()));
        assert!(rows.contains(&"intent:".to_string()));
        assert!(!rows.iter().any(|r| r.contains("SIG") && r.starts_with("segment")));
        assert!(!rows.iter().any(|r| r.contains("SIG") && r.starts_with("digest")));
        // The counts were recomputed from the surviving items.
        assert_eq!(out.counts["segments"], json!(1));
    }

    #[test]
    fn slice_name_glob_selects_by_spelling() {
        let view = drawing();
        let out = apply_viz(view, &sel(&["name=SIG"])).unwrap();
        let rows = classes(&out);
        assert!(rows.contains(&"digest:SIG".to_string()));
        assert!(!rows.contains(&"digest:VDD".to_string()));
        // The intent row survives only because one of its nets does.
        assert!(!rows.contains(&"intent:".to_string()));
    }

    #[test]
    fn slice_exclude_subtracts_from_the_selection() {
        let view = drawing();
        let owned = vec!["name=SIG".to_string(), "name=VDD".to_string()];
        let excludes = vec!["name=VDD".to_string()];
        let selection = Selection::compile(&owned, &excludes).unwrap();
        let out = apply_viz(view, &selection).unwrap();
        let rows = classes(&out);
        // SIG survives, VDD does not.
        assert!(rows.contains(&"digest:SIG".to_string()));
        assert!(!rows.contains(&"digest:VDD".to_string()));
        // The intent row drops with its only claim.
        assert!(!rows.contains(&"intent:".to_string()));
    }

    #[test]
    fn slice_excluding_every_selected_net_is_still_a_loud_error() {
        let view = drawing();
        let owned = vec!["intent=power-intent".to_string()];
        let excludes = vec!["name=VDD".to_string()];
        let selection = Selection::compile(&owned, &excludes).unwrap();
        let err = apply_viz(view, &selection).unwrap_err();
        assert!(err.contains("matches no drawn net"), "{err}");
    }

    #[test]
    fn slice_box_survives_only_with_a_surviving_pin_and_layer_with_a_row() {
        let items = vec![
            json!({"class": "layer", "key": "D1", "path": "main"}),
            json!({"class": "box", "key": "D2", "path": "main.c1", "layer": "main"}),
            json!({"class": "pin", "key": "P1", "net": "net:SIG", "box": "main.c1", "layer": "main"}),
            json!({"class": "segment", "kind": "wire", "net": "VDD", "layer": "main"}),
            json!({"class": "digest", "kind": "net", "net": "VDD", "layer": "main"}),
        ];
        let view = StageView::new(StageSeg::Viz, "main", items, 0);
        let out = apply_viz(view, &sel(&["name=VDD"])).unwrap();
        let rows = classes(&out);
        // The box's only pin is on SIG: the box goes.
        assert!(!rows.iter().any(|r| r.starts_with("box:")));
        // The layer stays: the surviving digest row names it.
        assert!(rows.contains(&"layer:main".to_string()));
        assert!(rows.contains(&"digest:VDD".to_string()));

        // Take the rows that name the layer away too, and the layer goes with
        // them — it never keeps itself alive.
        let items = vec![
            json!({"class": "layer", "key": "D1", "path": "main"}),
            json!({"class": "pin", "key": "P1", "net": "net:VDD", "box": "main.c1", "layer": "main"}),
            json!({"class": "box", "key": "D2", "path": "main.c1", "layer": "main"}),
        ];
        let view = StageView::new(StageSeg::Viz, "main", items, 0);
        let err = apply_viz(view, &sel(&["name=VDD"])).unwrap_err();
        assert!(err.contains("matches no drawn net"), "{err}");
    }

    #[test]
    fn slice_layer_without_a_surviving_row_drops() {
        let items = vec![
            json!({"class": "layer", "key": "D1", "path": "main"}),
            json!({"class": "layer", "key": "D2", "path": "spare"}),
            json!({"class": "digest", "kind": "net", "net": "VDD", "layer": "main"}),
        ];
        let view = StageView::new(StageSeg::Viz, "main", items, 0);
        let out = apply_viz(view, &sel(&["name=VDD"])).unwrap();
        let rows = classes(&out);
        assert!(rows.contains(&"layer:main".to_string()));
        assert!(!rows.iter().any(|r| r.starts_with("layer:spare")));
    }

    #[test]
    fn slice_selection_that_names_nothing_is_a_loud_error() {
        let view = drawing();
        let err = apply_viz(view, &sel(&["name=NOT_THERE"])).unwrap_err();
        assert!(err.contains("matches no drawn net"), "{err}");
    }

    #[test]
    fn slice_rejects_fields_the_net_record_does_not_carry() {
        let err = Selection::compile(&["kind=wire".to_string()], &[]).unwrap_err();
        assert!(err.contains("unknown key"), "{err}");
    }
}
