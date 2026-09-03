// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shared helpers for local CLI commands: unified target loading (file or
//! directory project mode), top-module resolution, and guarded Pass2 builds.

use crate::cmds::manifest;
use mcc::vector::model::trunk::{TrunkCtx, TrunkKind};
use std::path::{Path, PathBuf};

/// Load the CLI target into the engine and return the entry URI plus the
/// resolved top module when the target kind supplies one.
///
/// - File target: loaded directly; no top is implied.
/// - Directory target: project mode — `project.toml` drives the entry,
///   dependency libraries and top module ([`manifest::build_from_manifest`]);
///   without a usable manifest, browse mode selects the unique `module main`
///   entry ([`manifest::select_browse_entry`]). The returned top honors
///   `--top` / `--entry` and falls back to the browse entry's module.
pub fn load_target(
    target: Option<&str>,
    cli_top: Option<&str>,
    cli_entry: Option<&str>,
) -> anyhow::Result<(String, Option<String>)> {
    let Some(t) = target else {
        return Ok((String::new(), None));
    };
    let p = Path::new(t);
    if p.is_dir() {
        // Absolutize the root like `build` does: a relative root would make
        // the manifest entry URI relative too, and the module registry keys
        // on the canonical URI, so the top-module lookup would come up empty.
        let root = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(p)
        };
        match manifest::build_from_manifest(&root, cli_top, cli_entry) {
            Ok((entry_uri, top)) => Ok((entry_uri, Some(top))),
            Err(manifest_err) => {
                let entry_path =
                    manifest::select_browse_entry(&root, cli_entry).map_err(|browse_err| {
                        anyhow::anyhow!("{} (manifest: {:#})", browse_err, manifest_err)
                    })?;
                mcc::mcc_set_project_root(&root);
                let entry_uri = entry_path.to_string_lossy().to_string();
                mcc::mcc_load_project(&entry_uri);
                let top = cli_top
                    .map(str::to_string)
                    .or_else(|| mcc::mcb_get_module_name_by_uri(&entry_uri));
                Ok((entry_uri, top))
            }
        }
    } else {
        let path = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(p)
        };
        let entry_uri = path.to_string_lossy().to_string();
        mcc::mcc_load_project(&entry_uri);
        Ok((entry_uri, None))
    }
}

/// Resolve the top module: an explicit top (manifest top_module, or `--top` /
/// `--entry` override) first, else the module declared in the entry file,
/// else the first loaded module.
pub fn resolve_top_module(entry_uri: &str, explicit_top: Option<String>) -> Option<String> {
    explicit_top
        .or_else(|| mcc::cli::globals().top.clone())
        .or_else(|| mcc::mcb_get_module_name_by_uri(&entry_uri.to_string()))
        .or_else(mcc::mcb_get_first_module_name)
}

/// Run Pass2 for `top` in `uri`, converting an engine panic into an error so
/// a Pass2 bug cannot abort the CLI process.
pub fn build_pass2(top: &str, uri: &str) -> Result<mcc::McModuleInst, String> {
    let ident = mcc::McIds::from(top);
    let mcc_uri = mcc::McURI::from(uri);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build(&ident, &mcc_uri)
    })) {
        Ok(Ok(inst)) => Ok(inst),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".to_string()),
    }
}

/// Like [`build_pass2`], but also returns the Phase C companion arena + Phase C
/// S3 instance store so the tree-rendering consumer walks arena `children`
/// edges through the store-backed `TreeView` (design §4, plan §9 C item 3)
/// instead of the tree's `sub_modules` Vec, plus the Phase D frozen string
/// net-table store for the tree-level net consumers.
pub fn build_pass2_with_arena(
    top: &str,
    uri: &str,
) -> Result<
    (
        mcc::McModuleInst,
        mcc::NodeArena,
        mcc::InstanceStore,
        mcc::NetTableStore,
    ),
    String,
> {
    let ident = mcc::McIds::from(top);
    let mcc_uri = mcc::McURI::from(uri);
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        mcc::mcc_build_with_arena(&ident, &mcc_uri)
    })) {
        Ok(Ok(quad)) => Ok(quad),
        Ok(Err(e)) => Err(format!("build failed: {}", e)),
        Err(_) => Err("build panicked (engine Pass2 bug)".to_string()),
    }
}

// ============================================================================
// §8.9.5 layered connection rendering (shared by `show dianlu` and `verify`)
// ============================================================================

/// One connection row for layered rendering. `dir` is the source direction
/// tag (`"LtoR"` / `"RtoL"` / anything else = undirected, the `{:?}` form of
/// `ConnDir`); `trunk` is the structured group context (§8.9.6) decided
/// at the AST layer — `None` (or `kind == Plain`) means the connection is an
/// independent **wire**, `Some` with a member name marks a bus/interface
/// member lane (a **trunk** candidate that only renders as a trunk when it
/// converges with sibling lanes to one two-end mate).
pub struct ConnView {
    pub net: String,
    pub points: Vec<String>,
    pub dir: String,
    pub trunk: Option<TrunkCtx>,
}

/// Join connection endpoints with the separator that reflects the source
/// connector direction: `->` (LtoR), `<-` (RtoL), `-` (undirected).
fn join_conn_points(points: &[String], dir: &str) -> String {
    let sep = match dir {
        "LtoR" => " -> ",
        "RtoL" => " <- ",
        _ => " - ",
    };
    points.join(sep)
}

/// §8.9.5 layered connection rendering (shared by `show dianlu` and `verify`).
///
/// Connections render in two tiers (vocabulary: trunk / lane / wire):
///
/// - **trunk** (aggregate layer): bus/interface/list member lanes that mate
///   the same two ends. Renders as one `[trunk] left{member, ...} <-> right`
///   header line (the member set is spelled out on the left end; anonymous
///   lists use the bare member set as their left end) followed by one
///   indented numbered lane line per member (`SCLK#0 : mcu513.SPI.SCLK ->
///   flash.6`). All coarse kinds land here; an interface class, when
///   present, is annotated `:: class` on the header.
/// - **wire** (independent connection): every single line that is not part
///   of a two-end mate — scalar/plain connections plus bus member lanes
///   that do not converge. Renders as one `[wire] net : points` line.
///
/// A member lane is promoted to a trunk only when at least two lanes share
/// the same group name and, after stripping the leaf segment off each lane's
/// two outermost endpoints, all lanes converge to one distinct left end and
/// one distinct right end. Non-converging groups decompose into wire lines
/// instead of faking a bus header. `indent` prefixes every trunk / wire
/// line (lane lines get one level more).
pub fn render_layered_conns(conns: &[ConnView], indent: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    // First-seen ordered lane groups: (net, member, points, dir, iface_class)
    let mut groups: Vec<(
        String,
        Vec<(String, String, Vec<String>, String, Option<String>)>,
    )> = Vec::new();
    for c in conns {
        match &c.trunk {
            Some(pg) if pg.kind != TrunkKind::Plain && pg.member.is_some() => {
                let base = pg.name.clone().unwrap_or_else(|| c.net.clone());
                let member = pg.member.clone().unwrap_or_else(|| c.net.clone());
                match groups.iter_mut().find(|(b, _)| *b == base) {
                    Some((_, members)) => members.push((
                        c.net.clone(),
                        member,
                        c.points.clone(),
                        c.dir.clone(),
                        pg.iface_class.clone(),
                    )),
                    None => groups.push((
                        base,
                        vec![(
                            c.net.clone(),
                            member,
                            c.points.clone(),
                            c.dir.clone(),
                            pg.iface_class.clone(),
                        )],
                    )),
                }
            }
            _ => lines.push(format!(
                "{indent}[wire] {} : {}",
                c.net,
                join_conn_points(&c.points, &c.dir)
            )),
        }
    }
    for (_, members) in groups {
        match two_end_trunk(&members) {
            // Converged two-end mate -> one trunk header with numbered lanes.
            // The header spells the whole member set out on the left end
            // (`modldo.vout{VCC, GND}`); anonymous lists have no name, so
            // their left end is just the member set itself (`{VCC, GND}`).
            Some((left, right, iface)) => {
                let mut left_label = left;
                let leaves = member_leaves_ordered(&members);
                if !leaves.is_empty() {
                    if left_label.is_empty() {
                        left_label = format!("{{{}}}", leaves.join(", "));
                    } else {
                        left_label.push_str(&format!("{{{}}}", leaves.join(", ")));
                    }
                }
                let mut header = format!("{indent}[trunk] {left_label} <-> {right}");
                if let Some(cls) = iface {
                    header.push_str(&format!(" :: {cls}"));
                }
                lines.push(header);
                // Lane labels show the member leaf (`SPI.SCLK` -> `SCLK`),
                // numbered in appearance order within the trunk.
                let labels: Vec<String> = members
                    .iter()
                    .enumerate()
                    .map(|(k, (_, member, _, _, _))| format!("{}#{k}", member_leaf(member)))
                    .collect();
                let width = labels.iter().map(|l| l.len()).max().unwrap_or(0);
                for (label, (_, _, points, dir, _)) in labels.iter().zip(&members) {
                    lines.push(format!(
                        "{indent}  {label:<width$} : {}",
                        join_conn_points(points, dir),
                        width = width
                    ));
                }
            }
            // No unique two ends -> every lane is an independent wire.
            None => {
                for (net, _, points, dir, _) in members {
                    lines.push(format!(
                        "{indent}[wire] {} : {}",
                        net,
                        join_conn_points(&points, &dir)
                    ));
                }
            }
        }
    }
    lines
}

/// The last dotted segment of a member path (`SPI.SCLK` -> `SCLK`, `VCC` ->
/// `VCC`), the leaf name of a bus member lane.
fn member_leaf(member: &str) -> &str {
    match member.rfind('.') {
        Some(d) if d + 1 < member.len() => &member[d + 1..],
        _ => member,
    }
}

/// The ordered, de-duplicated member leaves of a lane group, used for the
/// `{...}` member set on a trunk header.
fn member_leaves_ordered(
    members: &[(String, String, Vec<String>, String, Option<String>)],
) -> Vec<String> {
    let mut leaves: Vec<String> = Vec::new();
    for (_, member, _, _, _) in members {
        let leaf = member_leaf(member).to_string();
        if !leaves.contains(&leaf) {
            leaves.push(leaf);
        }
    }
    leaves
}

/// Strip the leaf segment off an endpoint path (`mcu513.SPI.SCLK` ->
/// `mcu513.SPI`, `flash.6` -> `flash`), leaving the owning port object.
/// Leaf-only paths stay whole.
fn endpoint_port(p: &str) -> &str {
    match p.rfind('.') {
        Some(d) if d > 0 => &p[..d],
        _ => p,
    }
}

/// Promote a member-lane group to a two-end trunk: at least two lanes whose
/// two outermost endpoints, with the leaf segment stripped, converge to one
/// distinct left end and one distinct right end. Returns the two ends and
/// the interface class (when every lane agrees on one). `None` means the
/// group is not a trunk and its lanes render as wires.
fn two_end_trunk(
    members: &[(String, String, Vec<String>, String, Option<String>)],
) -> Option<(String, String, Option<String>)> {
    if members.len() < 2 {
        return None;
    }
    let mut left: Option<String> = None;
    let mut right: Option<String> = None;
    let mut iface: Option<String> = None;
    for (_, _, points, _, pg_iface) in members {
        if points.len() != 2 {
            return None;
        }
        let l = endpoint_port(&points[0]).to_string();
        let r = endpoint_port(&points[1]).to_string();
        match &left {
            Some(prev) if prev != &l => return None,
            None => left = Some(l),
            _ => {}
        }
        match &right {
            Some(prev) if prev != &r => return None,
            None => right = Some(r),
            _ => {}
        }
        if pg_iface.is_some() {
            iface = pg_iface.clone();
        }
    }
    let left = left?;
    let right = right?;
    if left == right {
        return None;
    }
    Some((left, right, iface))
}
