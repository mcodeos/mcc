// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc diff <A> <B> --view <view>` — two readings of one view, and what
//! changed between them.
//!
//! Design: `mcd/doc/pipeline/stage-readout-design.md` §5.3 ④ (§6.1 cross-version,
//! §6.2 same-source), and `mcd/doc/viz/view-model-design.md` §5.1 for the
//! alignment law this command reports in.
//!
//! # Two worlds, not two readings of one
//!
//! The two operands are loaded in turn, and each load **resets the engine**
//! (`cmds::manifest::init_local` → `mcc_init_no_lib`), so the second reading is
//! taken over a world of its own rather than over the first one plus its own
//! additions. That reset is not an optimisation detail — without it the two
//! sides would not be two states of one thing, and every difference would be an
//! artefact of accumulation. `mcc diff hbl.mc hbl.mc` reporting nothing is the
//! reading that says the reset happened, and it is one of the acceptance
//! criteria (design §7 phase 4 ①).
//!
//! # One reading, two commands
//!
//! Each side is built by [`crate::cmds::show::build_stage_view`] — the same
//! function `show stage <seg>` calls. A difference taken over worlds built one
//! way against a `show` that builds them another would not be a difference
//! between two states of one thing (design §5.3 ruling ③).
//!
//! # What this command is not
//!
//! It does not re-implement the comparison. The alignment law — a per-class key
//! function, build-local ordinals excluded — is `mcc::stages::stage_diff`'s, one
//! [`mcc::stages::stage_diff::Law`] per view, and this command's job is to pick
//! the law the `--view` names, name two worlds, hand over their items, and carry
//! the answer in the envelope.
//!
//! # Law C
//!
//! A difference is a **readout**: a non-empty difference is the answer, not a
//! failure, so the exit code stays 0 (the caller in `main` returns success). No
//! count here is a gate.

use anyhow::Result;

use crate::output::die;

use mcc::cli::{DiffArgs, DiffView, OutputFormat};

/// The text face's first column: one word per change type, in the order the
/// rows are printed. `unaligned` is not a change type but is grouped with them
/// because it is the only other row kind a difference has.
const ROW_WORDS: [&str; 4] = ["remove", "add", "modify", "unaligned"];

/// Render `mcc diff <A> <B> --view <view>`.
pub fn run(args: &DiffArgs) -> Result<()> {
    let (seg, law): (mcc::stages::StageSeg, &mcc::stages::stage_diff::Law) = match args.view {
        DiffView::StageViz => (
            mcc::stages::StageSeg::Viz,
            &mcc::stages::stage_diff::VIZ_LAW,
        ),
        DiffView::StageP2 => (mcc::stages::StageSeg::P2, &mcc::stages::stage_diff::P2_LAW),
        DiffView::StageVec => (
            mcc::stages::StageSeg::Vec,
            &mcc::stages::stage_diff::VEC_LAW,
        ),
    };

    let a = read_one(seg, args.a.as_str())?;
    let b = read_one(seg, args.b.as_str())?;

    let diff = law.diff(&a.items, &b.items);

    if matches!(
        mcc::cli::globals().format,
        OutputFormat::Text | OutputFormat::Csv
    ) {
        // CSV falls back to the text face on purpose, as `show stage` and `join`
        // do: a fixed-width readout is not CSV-safe (a canonical path may
        // contain a comma), so a real CSV face would be a separate decision
        // rather than something to fake here.
        let rendered = mcc::stages::stage_diff::render_diff_text(law.view, &a, &b, &diff);
        return write_text(&rendered);
    }
    emit_envelope(law, &a, &b, &diff)
}

/// Load one operand and read the requested segment off it.
///
/// `init_local` first, exactly as `join` does and for the same reason: the
/// standard components are defs like any other, and without the libraries every
/// statement that uses one builds a smaller world than `show stage` reports on.
/// The reset it performs is also what makes the second call a second world.
fn read_one(seg: mcc::stages::StageSeg, target: &str) -> Result<mcc::stages::StageView> {
    crate::cmds::manifest::init_local(Some(target), &mcc::cli::globals().lib);
    match crate::cmds::show::build_stage_view(seg, Some(target)) {
        Ok(v) => Ok(v),
        Err(e) => {
            die!("mcc::diff", 1, "{e}");
        }
    }
}

/// Write the text face to `--output` or stdout.
fn write_text(rendered: &str) -> Result<()> {
    if let Some(path) = &mcc::cli::globals().output {
        std::fs::write(path, format!("{rendered}\n"))?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

/// Emit the difference through the standard envelope channel.
///
/// The view rides `result.stage` like `join` and `trace` (law B: a new `view`
/// value on the existing envelope, not a second envelope format), with
/// `view = "diff.stage.<seg>"` and the `items` being the change rows.
///
/// The envelope's own `world_ver` / `top_ver` are **side A's**, the reference
/// the changes are stated against — every other field of the view (`top`,
/// `mcc_version`, `items`) describes that same reading. A difference belongs to
/// two worlds, so one token pair cannot say what was compared: side B's pair,
/// and the parts of the answer that are not change rows, ride
/// `stage.diff.other` / `.unaligned` and whatever else the law produces, under
/// the one key [`StageViewData::carrying_second_side`] adds.
fn emit_envelope(
    law: &mcc::stages::stage_diff::Law,
    a: &mcc::stages::StageView,
    b: &mcc::stages::StageView,
    diff: &mcc::stages::stage_diff::StageDiff,
) -> Result<()> {
    // `with_view` rather than `new`: a difference's vocabulary is its own (the
    // change types), and it is already sorted by `(kind, id)` — the producer's
    // sort, which is the one the alignment law defines.
    let mut view =
        mcc::stages::StageView::with_view(law.view, &a.top, diff.changes.clone(), counts_of(diff));
    // `with_view` derives the tokens from the world that is loaded *now* — side
    // B, by the time we get here. Overwrite them with A's: the envelope makes a
    // claim about which reading its `items` are a statement about, and that is
    // the left operand.
    view.world_ver = a.world_ver.clone();
    view.top_ver = a.top_ver.clone();

    let mut builder = crate::output::builder::ResultBuilder::start("mcc diff");
    let data = crate::output::envelope::StageViewData::from(&view)
        .carrying_second_side(second_side(b, diff));
    builder.set_stage(data);
    let env = crate::output::envelope::Envelope::ok(builder.finish());
    crate::output::emit_envelope(
        &env,
        mcc::cli::globals().format,
        mcc::cli::globals()
            .output
            .as_deref()
            .map(std::path::Path::new),
        true,
    )
}

/// The second side and the answer's non-row parts, as the `stage.diff` block.
///
/// Two of those parts are the law's, and are present only when the law produces
/// them. `nameless_net_pins` is a pair of **counts** and not a list (the
/// design's ruling on nets without a cross-build key): listing them would claim
/// an identity they do not have, and a segment that does not read nets off
/// references cannot make the claim at all. `stability` is the M12 summary a
/// difference of the drawing is the first producer of, and a segment that draws
/// no boxes has no such reading — an all-zero summary would read as "every box
/// stayed put", which is not the same statement as "there are no boxes".
fn second_side(
    b: &mcc::stages::StageView,
    diff: &mcc::stages::stage_diff::StageDiff,
) -> serde_json::Value {
    let mut block = serde_json::Map::new();
    block.insert(
        "other".to_string(),
        serde_json::json!({ "world_ver": b.world_ver, "top_ver": b.top_ver }),
    );
    block.insert("unaligned".to_string(), serde_json::json!(diff.unaligned));
    if let Some((an, bn)) = diff.nameless_net_pins {
        block.insert("nameless_net_pins".to_string(), serde_json::json!([an, bn]));
    }
    if let Some(st) = &diff.stability {
        block.insert(
            "stability".to_string(),
            serde_json::json!({
                "unchanged_boxes_total": st.unchanged_boxes_total,
                "unchanged_boxes_moved": st.unchanged_boxes_moved,
                "max_unchanged_box_delta": st.max_unchanged_box_delta,
                "route_hashes_changed": st.route_hashes_changed,
                "locality_warning": st.locality_warning,
            }),
        );
    }
    serde_json::Value::Object(block)
}

/// The diff's counts block: flat word → number, the same kind of thing every
/// other view's counts block is, so the header can be rendered from it without
/// knowing which view it is.
///
/// Every word here counts **changes**. The unalignable-pin coverage
/// (`nameless_net_pins`, one number per side) is deliberately *not* one of them:
/// it is a property of the two readings rather than of the difference, and
/// folding it in would make a self-difference — which changed nothing — publish a
/// non-zero number in a block whose every other member is zero. It rides
/// `stage.diff` instead, where a pair can be a pair.
fn counts_of(diff: &mcc::stages::stage_diff::StageDiff) -> serde_json::Value {
    let n_of = |t: &str| diff.changes.iter().filter(|c| c["type"] == t).count();
    let mut map = serde_json::Map::new();
    for w in ROW_WORDS {
        let n = if w == "unaligned" {
            diff.unaligned.len()
        } else {
            n_of(w)
        };
        map.insert(w.to_string(), serde_json::json!(n));
    }
    map.insert("changes".to_string(), serde_json::json!(diff.changes.len()));
    serde_json::Value::Object(map)
}
