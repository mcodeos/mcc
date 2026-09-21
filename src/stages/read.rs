// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The three chain verbs' readings, built in one place.
//!
//! `show stage <seg>`, `join <a> <b>` and `trace <KEY>` each read one operand's
//! pipeline. Two callers read them: the CLI (`cmds/show.rs` / `cmds/join.rs` /
//! `cmds/trace.rs`) and the MCP server (`bin/mcp.rs`). They differ in how they
//! *name and load* the operand — the CLI resolves `-F` / the cwd manifest / the
//! loaded workspace, the MCP server takes an entry path in the request — and in
//! nothing else. The construction therefore lives here: a second one would not
//! fail, it would answer the same question with a different reading.
//!
//! Why these are lib functions and not RPC methods: a stage view is read off the
//! **caller's** loaded source set, and a daemon's world is not the caller's
//! (the axis `U90` settles — see `cmds/show.rs::rpc_mapping`, which refuses to
//! delegate `show stage` for the same reason). The MCP server runs one process
//! per project and names the entry in the request, so it reads in-process;
//! publishing a daemon method would add a reachable surface whose answer nobody
//! compares against the local one.

use crate::instant::arena::NodeArena;
use crate::instant::inststore::InstanceStore;
use crate::instant::inststore::TreeView;
use crate::instant::insttab::InstTable;
use crate::stages::join;
use crate::stages::{trace, StageSeg, StageView};
use crate::vector::graph::{build_mc_vec_graph_with_log, McVecGraph};
use crate::viz::api::{render_with_metrics_and_sink, RenderOpts};
use crate::McModuleInst;

/// One operand, loaded: everything a segment view is built out of.
///
/// The pieces are kept together because every reading below needs the same set,
/// and because "was this loaded the same way" is a question about the whole set
/// rather than about any one part of it. `top` is part of that set: a view is
/// scoped to one resolved top module, and taking it from anywhere else is how
/// two readings of one load end up scoped differently.
pub struct Loaded {
    pub tree: McModuleInst,
    pub table: InstTable,
    pub arena: NodeArena,
    pub store: InstanceStore,
    /// The resolved top module this load was scoped to.
    pub top: String,
    /// How many diagnostics the load produced. A view *counts* them; it never
    /// gates on them (design §5.3, law C).
    pub diagnostics: usize,
}

impl Loaded {
    /// Pair a load result with its diagnostic count, in the one place that knows
    /// the count is a length.
    pub fn new(
        tree: McModuleInst,
        table: InstTable,
        arena: NodeArena,
        store: InstanceStore,
        top: &str,
        diagnostics: usize,
    ) -> Self {
        Self {
            tree,
            table,
            arena,
            store,
            top: top.to_string(),
            diagnostics,
        }
    }

    /// The vec block and its projection log — what both inner hops and the
    /// `vec` segment are read from.
    fn vec_block(&self) -> (McVecGraph, crate::viz::project::ProjectionLog) {
        let block =
            crate::build_mc_vec_with_arena(&self.tree, &self.table, &self.arena, &self.store);
        build_mc_vec_graph_with_log(&block, &self.table)
    }
}

/// Load one operand for a reading of it.
///
/// This is the load the MCP server uses. It is deliberately the same reset the
/// CLI's `init_local` performs (`mcc_init_no_lib` rather than the weaker
/// `mcc_clear_workspace`): a server answers many requests in one process, and
/// the workspace tables are append-only, so a weaker reset would let the
/// previous request's source set into this one's `world_ver`.
///
/// `top` resolved here rather than at the call site, so that a caller which
/// named no top and a caller which named one get the same answer the views
/// below are scoped to.
pub fn load(entry: &str, top: Option<&str>, libs: &[String]) -> Result<Loaded, String> {
    crate::mcc_init_no_lib();
    crate::rpc::handlers::load_libs_rpc(libs);
    let (tree, table, arena, store, diags) = crate::export::build_tree_diags(entry, top, libs)?;
    let resolved = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_first_module_name()
            .ok_or_else(|| "no module found in file (use top)".to_string())?,
    };
    Ok(Loaded::new(
        tree,
        table,
        arena,
        store,
        &resolved,
        diags.len(),
    ))
}

/// Read one segment of the chain.
pub fn build_segment(seg: StageSeg, loaded: &Loaded) -> StageView {
    let top = loaded.top.as_str();
    // `stage.p1` is the reserved slot: the command family is fixed now so that
    // the Pass1 view (design §8 O2/O3) can land without reshaping it. Its body
    // stays empty rather than reusing `show ast`'s output, which carries none of
    // the chain's keys (design §5.2 ①: the chain's identity starts at Pass2).
    match seg {
        StageSeg::P1 => StageView::new(seg, top, Vec::new(), loaded.diagnostics),
        StageSeg::P2 => crate::stages::p2::build_p2(&loaded.table, top, loaded.diagnostics),
        StageSeg::Vec => {
            let (graph, log) = loaded.vec_block();
            crate::stages::vec::build_vec(&graph, &log, &loaded.table, top, loaded.diagnostics)
        }
        StageSeg::Viz => {
            // The render pipeline consumes the graph it lays out, so the view
            // reads it back through the observation sink: this is the one
            // segment whose objects have positions, and they exist nowhere but
            // in that graph (never in the SVG — design §11.1 / M5).
            let block = crate::build_mc_vec_with_arena(
                &loaded.tree,
                &loaded.table,
                &loaded.arena,
                &loaded.store,
            );
            let graph = crate::vector::graph::build_mc_vec_graph(&block, &loaded.table);
            let mut layers = Vec::new();
            let (_doc, metrics) =
                render_with_metrics_and_sink(graph, RenderOpts::default(), Some(&mut layers));
            let quality = metrics.finish_quality(None);
            crate::stages::viz::build_viz(&layers, &quality, &loaded.table, top, loaded.diagnostics)
        }
    }
}

/// The chain's hops, adjacent pairs only.
///
/// A hop would skip the segment in between and lose its `merge` criterion, so
/// this is a closed set of ordered pairs rather than an N×N matrix (design
/// §5.3 ②). The order carries information: `join p2 src` is a different question
/// (it is `trace`).
pub fn is_adjacent_pair(a: &str, b: &str) -> bool {
    matches!((a, b), ("src", "p2") | ("p2", "vec") | ("vec", "viz"))
}

/// Read one hop of the chain.
pub fn build_join_pair(a: &str, b: &str, loaded: &Loaded) -> Result<StageView, String> {
    if !is_adjacent_pair(a, b) {
        return Err(format!("'{a}' and '{b}' are not adjacent segments"));
    }
    let top = loaded.top.as_str();
    Ok(match (a, b) {
        ("src", "p2") => join::build_join_src_p2(
            &loaded.table,
            &loaded.tree,
            &TreeView::new(&loaded.arena, &loaded.store),
            top,
            loaded.diagnostics,
        ),
        ("p2", "vec") => {
            let (graph, log) = loaded.vec_block();
            join::build_join_p2_vec(&graph, &log, &loaded.table, top, loaded.diagnostics)
        }
        _ => {
            let (graph, log) = loaded.vec_block();
            join::build_join_vec_viz(graph, &log, &loaded.table, top, loaded.diagnostics)
        }
    })
}

/// Keep the rows of one class word, or every row when `only` is `None`.
///
/// It filters the **rows** and nothing else: the counts keep describing the
/// whole hop, so a filtered readout cannot be mistaken for a world with nothing
/// else in it (design §5.3 ②).
pub fn filter_join_items(view: &mut StageView, only: Option<&str>) -> Result<(), String> {
    let Some(only) = only else { return Ok(()) };
    if !join::is_class_word(only) {
        let words: Vec<&str> = join::CLASS_WORDS
            .iter()
            .chain(join::DIAG_WORDS)
            .copied()
            .collect();
        return Err(format!(
            "unknown class '{only}'\nexpected one of: {}",
            words.join(" | ")
        ));
    }
    view.items.retain(|i| i["class"] == only);
    Ok(())
}

/// Follow one object along all three hops of one build.
pub fn build_trace_view(key: &str, loaded: &Loaded) -> Result<StageView, String> {
    // All three hops of one build: the source hop is generated from the table,
    // and the two circuit hops from one vector graph — cloned rather than built
    // twice, because the second hop's builder consumes the graph it renders.
    let top = loaded.top.as_str();
    let src_p2 = join::build_join_src_p2(
        &loaded.table,
        &loaded.tree,
        &TreeView::new(&loaded.arena, &loaded.store),
        top,
        loaded.diagnostics,
    );
    let (graph, log) = loaded.vec_block();
    let p2_vec =
        join::build_join_p2_vec_with_sides(&graph, &log, &loaded.table, top, loaded.diagnostics);
    let vec_viz =
        join::build_join_vec_viz_with_sides(graph, &log, &loaded.table, top, loaded.diagnostics);
    trace::build_trace(key, &src_p2, &p2_vec, &vec_viz, top)
}
