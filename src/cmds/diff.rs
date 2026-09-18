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
//! # Two kinds of operand: a world, or a reading already taken
//!
//! Each operand is a source path (or project) read now, **or** a reading saved
//! earlier by `mcc show stage <seg> -f json -o <file>` (CIMP §1 U96). The kind
//! is decided by what is at the path, not by a flag.
//!
//! The second kind is what makes §6.2's same-source difference expressible at
//! all. "Same source, two compilers" cannot be asked of one process — one
//! process holds one binary — so each side has to be read by the build in
//! question and saved, and this command then only subtracts. A saved reading is
//! not a world and is never re-read: it is the same `items` the producer
//! emitted, with the identity the producer wrote, and the only thing checked is
//! that it says what it is (`view`) and what its items were aligned under
//! (`key_table`) — those two are the whole of its self-description, and a
//! difference over readings that do not agree on them would be this build's
//! table answering for readings it does not describe.
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
    let seg: mcc::stages::StageSeg = match args.view {
        DiffView::StageViz => mcc::stages::StageSeg::Viz,
        DiffView::StageP2 => mcc::stages::StageSeg::P2,
        DiffView::StageVec => mcc::stages::StageSeg::Vec,
    };
    // The table is looked up, not spelled out: which table a segment aligns
    // under is the segment's answer (`law_for`), and `show stage` publishes the
    // same one on the reading it saves. Two spellings of that mapping would let
    // a saved reading and a difference disagree about what a key is.
    let Some(law) = mcc::stages::stage_diff::law_for(seg) else {
        die!("mcc::diff", 2, "no alignment law for {}", seg.view_name());
    };

    let a = read_one(seg, law, args.a.as_str())?;
    let b = read_one(seg, law, args.b.as_str())?;

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
/// An operand is one of **two kinds**, told apart by what is at the path rather
/// than by a flag: a world to read now (a source file or a project, the kind the
/// command has always taken) or a **reading saved earlier** (`mcc show stage
/// <seg> -f json -o <file>`, CIMP §1 U96). A saved reading needs no source and
/// no world — it is the same `items` with the identity fields the producer
/// wrote — so comparing two of them is a pure data operation, which is what
/// makes "same source, two compilers" expressible: each side was read by its own
/// binary and saved, and this one only subtracts.
///
/// `init_local` first, exactly as `join` does and for the same reason: the
/// standard components are defs like any other, and without the libraries every
/// statement that uses one builds a smaller world than `show stage` reports on.
/// The reset it performs is also what makes the second call a second world. It
/// runs on the world path only — an archive is not a world, and resetting the
/// engine for one would claim it had read something.
fn read_one(
    seg: mcc::stages::StageSeg,
    law: &mcc::stages::stage_diff::Law,
    target: &str,
) -> Result<mcc::stages::StageView> {
    if let Some(saved) = saved_reading(target)? {
        return view_of_saved(seg, law, target, &saved);
    }
    crate::cmds::manifest::init_local(Some(target), &mcc::cli::globals().lib);
    match crate::cmds::show::build_stage_view(seg, Some(target)) {
        Ok(v) => Ok(v),
        Err(e) => {
            die!("mcc::diff", 1, "{e}");
        }
    }
}

/// The stage reading saved at `path`, or `None` where `path` is not one.
///
/// Recognised structurally and not by extension: a saved reading **is** a
/// projection envelope carrying `result.stage`, and a path holding anything else
/// is the source operand this command has always taken. A path that is not a
/// file, or whose bytes are not JSON, is not a reading — the world path then gets
/// it and fails its own way, with the message that operand deserves.
///
/// A JSON envelope that carries `result` but no `result.stage` dies here instead
/// of falling through: it is recognisably **a** reading and not one of these, and
/// handing it to the MCode parser would answer a question nobody asked.
fn saved_reading(path: &str) -> Result<Option<serde_json::Value>> {
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(None);
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(None);
    };
    if !text.trim_start().starts_with('{') {
        return Ok(None);
    }
    let Ok(env) = serde_json::from_str::<serde_json::Value>(text) else {
        return Ok(None);
    };
    let Some(result) = env.get("result") else {
        return Ok(None);
    };
    let Some(stage) = result.get("stage") else {
        die!(
            "mcc::diff",
            1,
            "operand '{path}' is a projection envelope, but not a stage reading \
             (no `result.stage`)\na stage reading is what `mcc show stage \
             <p1|p2|vec|viz> -f json -o <file>` writes"
        );
    };
    Ok(Some(stage.clone()))
}

/// Turn a saved reading into the same [`StageView`] its producer emitted.
///
/// The two checks here are the reading's **self-description** (CIMP §1 U96) —
/// what an archive has to state for a difference over it to be well defined:
///
/// - **which segment** (`view`): a `stage.vec` reading is not a reading of
///   `stage.p2`, and comparing one against the other with either table would
///   produce a page of `unaligned` that reads like "everything changed";
/// - **which key table**: the table is not recoverable from the items, so a
///   reading that does not name one — or names a different one — cannot be
///   compared. Refusing is the only honest answer: the alternative is to report
///   the changes *this* build's table happens to produce over readings it does
///   not describe.
///
/// The version of the producer (`mcc_version`) is carried through rather than
/// checked: two builds may differ in every way but the table and still be worth
/// subtracting, which is the whole point of the operand.
fn view_of_saved(
    seg: mcc::stages::StageSeg,
    law: &mcc::stages::stage_diff::Law,
    path: &str,
    saved: &serde_json::Value,
) -> Result<mcc::stages::StageView> {
    let written = saved.get("view").and_then(|v| v.as_str()).unwrap_or("-");
    if written != seg.view_name() {
        die!(
            "mcc::diff",
            1,
            "operand '{path}' is a {written} reading; --view names {}\n\
             a difference is between two readings of one segment",
            seg.view_name()
        );
    }
    let Some(table) = saved.get("key_table").and_then(|v| v.as_str()) else {
        die!(
            "mcc::diff",
            1,
            "operand '{path}' states no key table (written by mcc {})\n\
             a saved reading must name the table it was aligned under, or there \
             is no saying what comparing it means",
            saved
                .get("mcc_version")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
        );
    };
    if table != law.key_table {
        die!(
            "mcc::diff",
            1,
            "operand '{path}' was aligned under key table {table}; this build \
             aligns {} under {}\n\
             the two are not two readings of one thing: re-read it with the \
             build that wrote it, or compare readings of one table",
            seg.view_name(),
            law.key_table
        );
    }

    let str_of = |name: &str| saved.get(name).and_then(|v| v.as_str()).map(str::to_string);
    let items = saved
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let counts = saved
        .get("counts")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    // `with_view` and not `new`: the saved items are already in the producer's
    // order, and re-sorting them here would be this build re-answering a
    // question the reading already answered.
    let top = str_of("top").unwrap_or_default();
    let mut view = mcc::stages::StageView::with_view(seg.view_name(), &top, items, counts);
    // The identity fields are the reading's own. `with_view` derives them from
    // the world loaded *now* — which, for a difference of two saved readings, is
    // no world at all.
    view.world_ver = str_of("world_ver");
    view.top_ver = str_of("top_ver");
    view.mcc_version = str_of("mcc_version").unwrap_or_default();
    view.key_table = Some(table.to_string());
    Ok(view)
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
    // `with_view` derives the identity fields from the world that is loaded
    // *now* — side B, by the time we get here, and no world at all when both
    // operands were saved readings. Overwrite them with A's: the envelope makes
    // a claim about which reading its `items` are a statement about, and that is
    // the left operand — including which build produced it, which is not this
    // one's business to assert once an operand can be an archive.
    view.world_ver = a.world_ver.clone();
    view.top_ver = a.top_ver.clone();
    view.mcc_version = a.mcc_version.clone();
    // The table these changes were taken under. Both sides were checked against
    // it (`view_of_saved`), so it is the law's — stating it makes the answer
    // self-describing the same way a saved reading is.
    view.key_table = Some(law.key_table.to_string());

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
        serde_json::json!({
            "world_ver": b.world_ver,
            "top_ver": b.top_ver,
            // Which build read side B. Symmetric with the envelope's own
            // `mcc_version`, which is side A's, and the only place the two
            // producers can be seen side by side — the reading that says
            // "same source, two compilers" was taken.
            "mcc_version": b.mcc_version,
        }),
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
