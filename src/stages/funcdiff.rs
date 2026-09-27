// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The `diff` projection — functional alignment, the second diff mode.
//!
//! The sixth and last carried canonical `view-name` word
//! (`schema/projection.cddl` §2.5, group `functional-change`). The identity
//! diff (`stage_diff`) answers "did this design change correctly" on NodeId;
//! this one answers "how do two implementations of one intent compare" on the
//! **shared `expects` semantic keys** (kind + target + bound) — never on
//! NodeId or source position, which two implementations do not share
//! (circuit-intent-acceptance-design.md §6, projection-schema-design.md §2.5).
//!
//! The block-level judgment is §6.1's, read off the shared rows' verdicts:
//!
//! * every shared key PASS on both sides → `module-replace` (boundary
//!   equivalent, replaceable);
//! * any shared key FAIL on either side → `verdict-divergence` (one side did
//!   not honor a shared expectation);
//! * shared keys present but none resolvable statically (DEFER) →
//!   `deferred`, handed to the dynamic arm (design §6.2). The word is this
//!   batch's, recorded on the CDDL group: §6.1 ruled the marking ("hand to
//!   the dynamic arm") but the 2026-09-04 enum predates the engine and
//!   carries no word for it;
//! * no shared key at all → `none`, `delta` omitted: the two sides expose
//!   different interfaces, no equivalence judgment is made, and each side's
//!   own expectations are accounted separately (§6's M2 precondition).
//!
//! Each side's verdicts come from one acceptance run
//! ([`crate::check::expectation::run`]) — the same engine the `check` gate
//! judges with, so the comparison cannot read a verdict the gate would not
//! have spelled. Rows that are not shared never enter the judgment; the text
//! face lists them per side as the separate accounting.

use serde::Serialize;
use serde_json::Value;

use super::StageView;
use crate::semantic::module::expects::{Kind, Ledger};
use crate::semantic::validation::expectation::{ExpectationReport, Verdict};

/// The canonical word this face publishes (CDDL `view-name`).
pub const DIFF_VIEW: &str = "diff";

/// The `kind` every functional-change carries: the comparison is block-level
/// (the CDDL group pins it to this one word).
pub const FUNCTIONAL_KIND: &str = "module-replace";

/// CDDL `functional-change` — one block's account, member names verbatim from
/// `schema/projection.cddl`. `delta` is present exactly when there is a
/// shared face to judge (`none` omits it, never serializes an empty list).
#[derive(Debug, Clone, Serialize)]
pub struct FunctionalChange {
    /// `module-replace` | `verdict-divergence` | `deferred` | `none` — the
    /// §6.1 block judgment (the last word this batch's, see the module doc).
    #[serde(rename = "type")]
    pub change_type: String,
    /// Always [`FUNCTIONAL_KIND`]: the alignment is whole-block.
    pub kind: &'static str,
    /// The shared rows' both-side verdicts, in the A side's ledger order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<SharedKeys>,
}

/// CDDL `delta` — the shared-face payload.
#[derive(Debug, Clone, Serialize)]
pub struct SharedKeys {
    pub shared_keys: Vec<SharedKeyVerdict>,
}

/// CDDL `shared-key-verdict` — one shared row, both sides' verdicts. The
/// member names are the CDDL's (`A`/`B`, capital), hence the renames.
#[derive(Debug, Clone, Serialize)]
pub struct SharedKeyVerdict {
    /// The alignment key: kind + target + bound in one spelling
    /// ([`semantic_key`]).
    pub key: String,
    /// The A (reference) side's verdict word.
    #[serde(rename = "A")]
    pub a: String,
    /// The B side's verdict word.
    #[serde(rename = "B")]
    pub b: String,
}

/// One side's rows as alignment keys plus verdict words, ledger order.
pub struct SideVerdicts {
    /// `(semantic key, verdict word)`, in the ledger's own source order.
    pub rows: Vec<(String, String)>,
}

/// The alignment key for one row: `kind:target`, plus the window sides for a
/// value-bound row — the author's notation verbatim, an absent side a `-`.
///
/// Notation differences (`[3.2V:3.4V]` vs `3.2V ~ 3.4V`) are different keys
/// by design: the alignment is on what each implementation declared, not on
/// what a normalizer would decide they meant — the same verbatim rule the
/// `expectation` view's `bound` already keeps.
pub fn semantic_key(kind_word: &str, target: &str, kind: &Kind) -> String {
    match kind {
        Kind::Window { low, high } => format!(
            "{kind_word}:{target}[{}~{}]",
            low.as_deref().unwrap_or("-"),
            high.as_deref().unwrap_or("-")
        ),
        _ => format!("{kind_word}:{target}"),
    }
}

fn verdict_word(v: Verdict) -> &'static str {
    match v {
        Verdict::Pass => "PASS",
        Verdict::Fail => "FAIL",
        Verdict::Defer => "DEFER",
    }
}

/// One side's alignment keys and verdicts, off the same engine run the
/// `check` gate judged with. The row/outcome count assertion is the same
/// invariant [`super::expectview::expectation_items`] guards.
pub fn side_verdicts(ledger: &Ledger, report: &ExpectationReport) -> SideVerdicts {
    assert_eq!(
        ledger.rows.len(),
        report.outcomes.len(),
        "the engine owes one outcome per ledger row"
    );
    SideVerdicts {
        rows: ledger
            .rows
            .iter()
            .zip(report.outcomes.iter())
            .map(|(row, outcome)| {
                (
                    semantic_key(&outcome.kind.to_string(), &outcome.target, &row.kind),
                    verdict_word(outcome.verdict).to_string(),
                )
            })
            .collect(),
    }
}

/// The §6.1 block judgment over two sides' shared keys. The judgment reads
/// the shared rows only; a key only one side declares is the M2
/// incomparability, and the sides are accounted separately by the faces.
pub fn functional_change(a: &SideVerdicts, b: &SideVerdicts) -> FunctionalChange {
    let b_index: std::collections::HashMap<&str, &str> = b
        .rows
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let shared: Vec<SharedKeyVerdict> = a
        .rows
        .iter()
        .filter(|(k, _)| b_index.contains_key(k.as_str()))
        .map(|(k, av)| SharedKeyVerdict {
            key: k.clone(),
            a: av.clone(),
            b: b_index[k.as_str()].to_string(),
        })
        .collect();
    if shared.is_empty() {
        return FunctionalChange {
            change_type: "none".to_string(),
            kind: FUNCTIONAL_KIND,
            delta: None,
        };
    }
    let change_type = if shared
        .iter()
        .any(|s| s.a == "FAIL" || s.b == "FAIL")
    {
        "verdict-divergence"
    } else if shared.iter().any(|s| s.a == "DEFER" || s.b == "DEFER") {
        "deferred"
    } else {
        "module-replace"
    };
    FunctionalChange {
        change_type: change_type.to_string(),
        kind: FUNCTIONAL_KIND,
        delta: Some(SharedKeys {
            shared_keys: shared,
        }),
    }
}

/// Per-face counts: the four type words — computed from the **items** so the
/// header and the rows cannot disagree. Every word is printed even when zero.
pub fn functional_counts(items: &[Value]) -> Value {
    let count = |word: &str| {
        items
            .iter()
            .filter(|i| i["type"].as_str() == Some(word))
            .count()
    };
    serde_json::json!({
        "module-replace": count("module-replace"),
        "verdict-divergence": count("verdict-divergence"),
        "deferred": count("deferred"),
        "none": count("none"),
    })
}

/// Assemble the projection. Goes through [`StageView::with_view`] like every
/// carried view — the envelope carries the canonical word and its own counts.
pub fn functional_view(a_path: &str, b_path: &str, change: &FunctionalChange) -> StageView {
    let items = vec![serde_json::to_value(change).unwrap_or(Value::Null)];
    let counts = functional_counts(&items);
    StageView::with_view(
        DIFF_VIEW,
        &format!("{a_path} vs {b_path}"),
        items,
        counts,
    )
}

/// One side's keys the other side does not declare — the separate accounting
/// §6 gives them: out of the judgment, listed by the text face, sorted so the
/// readout is stable.
fn only_keys<'a>(side: &'a SideVerdicts, other: &std::collections::BTreeSet<&str>) -> Vec<&'a str> {
    let mut keys: Vec<&str> = side
        .rows
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !other.contains(k))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys
}

/// The text face, rendered from the **same** item the envelope carries: the
/// block's type and what ruled it, then one row per shared key with both
/// verdicts, then each side's unshared keys — the separate accounting §6
/// gives them. A `none` block says what it means: no shared expectation, no
/// equivalence judgment.
pub fn render_functional_text(
    view: &StageView,
    a: &SideVerdicts,
    b: &SideVerdicts,
) -> String {
    let mut lines = vec![view.header_line()];
    let words: Vec<String> = view
        .counts
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k} {}", v.as_u64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    lines.push(format!("# {}", words.join("  ")));
    for it in &view.items {
        let kind = it["type"].as_str().unwrap_or("-");
        match kind {
            "module-replace" => {
                lines.push("module-replace  boundary equivalent (shared expectations all PASS)".into());
            }
            "verdict-divergence" => {
                lines.push("verdict-divergence  a shared expectation fails on one side".into());
            }
            "deferred" => {
                lines.push("deferred  shared expectations await sim (dynamic arm, acceptance §6.2)".into());
            }
            _ => {
                lines.push("none  no shared expectation — no equivalence judgment".into());
            }
        }
        if let Some(delta) = it["delta"].as_object() {
            let keys = delta["shared_keys"].as_array().cloned().unwrap_or_default();
            let key_col = keys
                .iter()
                .map(|k| k["key"].as_str().unwrap_or("-").len())
                .max()
                .unwrap_or(3)
                .max(3);
            lines.push(format!(
                "  {:<key_col$}  A     B",
                "key"
            ));
            for k in &keys {
                lines.push(format!(
                    "  {:<key_col$}  {:<5} {}",
                    k["key"].as_str().unwrap_or("-"),
                    k["A"].as_str().unwrap_or("-"),
                    k["B"].as_str().unwrap_or("-"),
                ));
            }
        }
    }
    let b_keys: std::collections::BTreeSet<&str> =
        b.rows.iter().map(|(k, _)| k.as_str()).collect();
    let a_keys: std::collections::BTreeSet<&str> =
        a.rows.iter().map(|(k, _)| k.as_str()).collect();
    for (label, side, other) in [
        ("only in A", a, &b_keys),
        ("only in B", b, &a_keys),
    ] {
        let keys = only_keys(side, other);
        if !keys.is_empty() {
            lines.push(format!("{label} (accounted separately): {}", keys.join(", ")));
        }
    }
    lines.join("\n")
}
