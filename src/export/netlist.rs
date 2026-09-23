// Copyright (c) 2026 MCode
//! Netlist export

use crate::export::NodeArena;
use crate::instant::inststore::{InstanceStore, TreeView};
use crate::instant::insttab::{InstKind, InstTable};
use crate::instant::nettab::NetTableStore;
use crate::McModuleInst;
use crate::NetPoint;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// How a net point names its owning instance.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PointNaming {
    /// The name the instance carries inside its own module (`F1.1`) — what the
    /// netlist and KiCad exports print.
    Local,
    /// The instance's hierarchical path (`main.F1.1`) — the key a flat-table
    /// lookup needs to read the instance's class back.
    Hierarchical,
}

/// An engine-generated anonymous net (`_net14`); a standalone island keeps the
/// name as its export identity (U158: dropping anonymous islands dropped real
/// copper — crystal pins, filter midpoints, series-resistor junctions — and a
/// SPICE netlist that cannot be simulated).
fn is_anon(name: &str) -> bool {
    crate::instant::mc_net::is_anon_net_name(name)
}

/// A name no export may carry: the deliberate no-connect bucket and the
/// parse-error marker.
fn is_excluded(name: &str) -> bool {
    name == "NC" || name.starts_with(crate::semantic::basic::mc_bus::McBus::ERROR_PREFIX)
}

pub fn build_netlist(table: &InstTable, top: &str, format: u8) -> (String, Value, usize) {
    let nets = island_nets(table, PointNaming::Local);
    let nets: BTreeMap<String, Vec<String>> =
        nets.into_iter().filter(|(n, _)| !is_excluded(n)).collect();
    let count = nets.len();
    if format == 1 {
        let items: Vec<Value> = nets
            .iter()
            .map(|(name, points)| json!({ "name": name, "points": points }))
            .collect();
        (String::new(), Value::Array(items), count)
    } else {
        let mut out = String::new();
        out.push_str(&format!("# Netlist: top={}\n", top));
        for (name, points) in &nets {
            out.push_str(&format!("{}: {}\n", name, points.join(" ")));
        }
        (out, Value::Null, count)
    }
}

/// The flat table's net segments folded into **copper islands** — one entry
/// per electrically distinct node, the grain a netlist export owes its
/// consumer (U158).
///
/// The flat table records one net segment per owning scope (its frozen string
/// net table), and a module-boundary point is one id on both sides (A′). A
/// junction point joins every segment wired to it. So the segments union into
/// islands by one walk: any two segments sharing a point id are one copper.
/// The old export folded the same tables into a name-keyed map instead, which
/// left the far side of every cross-scope boundary off the output and dropped
/// anonymous-keyed segments outright — hbl lost ~28 connection points, among
/// them both crystal pins and a five-pin DCDC's entire control face.
///
/// An island is named by its best member: a named segment's own name (the
/// flat table's port-name > label-name attribution), lexicographically first
/// for determinism when several members carry names. Only an island with no
/// named member keeps an engine `_netN` spelling — that island is real copper
/// no statement ever named, and the export keeps it (dropping it is a
/// connectivity loss, not a naming choice; whether the exit contract wants a
/// different spelling is S2's to rule, not this face's to guess).
///
/// Spellings can collide — the engine numbers anonymous segments per scope,
/// and two scopes may also each label a private net the same. Same-spelling
/// islands are distinct copper (islands are maximal), so they stay distinct
/// buckets: the first island in root order keeps the bare spelling, the next
/// takes `#2`, `#3`, ...
///
/// Points render per [`PointNaming`], members in ascending net id, deduplicated
/// in encounter order; the `BTreeMap` keys the islands by name so the export
/// order is the input's alone (build-design §3.7 discipline 4).
pub fn island_nets(table: &InstTable, naming: PointNaming) -> BTreeMap<String, Vec<String>> {
    let nets = table.get_nets();
    // Union-find over segment ids: a segment merges with the first segment of
    // each of its points (`nets_of` lists every segment the point sits on).
    let mut parent: BTreeMap<u32, u32> = nets.iter().map(|n| (n.id, n.id)).collect();
    fn find(parent: &mut BTreeMap<u32, u32>, mut a: u32) -> u32 {
        while parent[&a] != a {
            a = parent[&a];
        }
        a
    }
    for n in &nets {
        for &pt in &n.points {
            if let Some(&first) = table.nets_of(pt).first() {
                let (a, b) = (find(&mut parent, first), find(&mut parent, n.id));
                if a != b {
                    parent.insert(b, a);
                }
            }
        }
    }
    // Gather islands: root -> member segments, ascending by net id.
    let mut islands: BTreeMap<u32, Vec<&crate::instant::insttab::NetEntry>> = BTreeMap::new();
    for n in &nets {
        islands.entry(find(&mut parent, n.id)).or_default().push(n);
    }
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Island spellings can collide: the engine numbers anonymous segments per
    // scope, so one scope's `_net1` and another scope's `_net1` are two
    // coppers, and two scopes may each label a private net `ENABLE`. Islands
    // are maximal, so same-spelling islands are distinct copper — one bucket
    // each: the first island (ascending root id, hence input-determined)
    // keeps the bare spelling, the next distinct island with the same
    // spelling takes `#2`, `#3`, ...
    let mut spellings: BTreeMap<String, usize> = BTreeMap::new();
    for members in islands.values() {
        // Name: named members first, lexicographically first for determinism;
        // a member spelling no export may carry never names the island.
        let name = members
            .iter()
            .map(|n| n.name.as_str())
            .filter(|n| !is_excluded(n) && !is_anon(n))
            .min()
            .or_else(|| {
                members
                    .iter()
                    .map(|n| n.name.as_str())
                    .filter(|n| !is_excluded(n))
                    .min()
            })
            .unwrap_or("NC")
            .to_string();
        if is_excluded(&name) {
            continue;
        }
        let seen = spellings.entry(name.clone()).or_default();
        let key = if *seen == 0 {
            name.clone()
        } else {
            format!("{}#{}", name, *seen + 1)
        };
        *seen += 1;
        let bucket = out.entry(key).or_default();
        for n in members {
            for &pt in &n.points {
                let Some(label) = island_point_label(table, pt, naming) else {
                    continue;
                };
                if !bucket.contains(&label) {
                    bucket.push(label);
                }
            }
        }
    }
    out
}

/// Label one flat point for an export island. A pin keeps `owner.pin` under
/// local naming; a port or label keeps its own name under its owning module —
/// the same spellings the per-scope tables printed, now boundary-complete.
fn island_point_label(table: &InstTable, point: u32, naming: PointNaming) -> Option<String> {
    let entry = table.get_entry(point)?;
    match naming {
        PointNaming::Hierarchical => Some(entry.path.clone()),
        PointNaming::Local => Some(match entry.kind {
            InstKind::Pin => {
                // `main.MIC.FB_vmic.2` -> `FB_vmic.2`: the instance's local
                // name is the segment in front of the pin's own.
                let owner = entry
                    .path
                    .rsplit_once('.')
                    .and_then(|(rest, _)| rest.rsplit_once('.'))
                    .map(|(_, owner)| owner)
                    .unwrap_or(entry.path.as_str());
                let pin = entry.path.rsplit('.').next()?;
                format!("{owner}.{pin}")
            }
            _ => {
                // A port or label sits directly under its module: strip the
                // module's path prefix (`main.MIC.VMIC` -> `VMIC`).
                let parent = entry.parent_id.and_then(|id| table.get_entry(id));
                match parent {
                    Some(p) if entry.path.starts_with(&p.path) => entry.path
                        [p.path.len().min(entry.path.len())..]
                        .trim_start_matches('.')
                        .to_string(),
                    _ => entry.path.clone(),
                }
            }
        }),
    }
}

/// Collect the flat netlist from the tree-level string net tables (Phase D —
/// sourced from the frozen `net_store`, never from the tree). Walks the
/// module tree arena-first with the canonical module path (the same path the
/// store keys on: `main`, `main.ldo`, ...) and merges every module's net
/// points into one name-keyed map.
///
/// `naming` decides whether a point carries the instance's local name or its
/// hierarchical path; the walk computes the module path either way.
pub fn collect_nets(
    inst: &McModuleInst,
    arena: &NodeArena,
    inst_store: &InstanceStore,
    net_store: &NetTableStore,
    naming: PointNaming,
    out: &mut BTreeMap<String, Vec<String>>,
) {
    let view = TreeView::new(arena, inst_store);
    let mut walk = |_m: &McModuleInst, path: &str, out: &mut BTreeMap<String, Vec<String>>| {
        let Some(table) = net_store.get(path) else {
            return;
        };
        for (name, points) in table {
            for np in points {
                let pt = pin_label(np, path, naming);
                let entry = out.entry(name.clone()).or_default();
                if !entry.contains(&pt) {
                    entry.push(pt);
                }
            }
        }
    };
    collect_nets_impl(inst, &view, &inst.name.clone(), &mut walk, out);
}

fn collect_nets_impl(
    inst: &McModuleInst,
    view: &TreeView,
    path: &str,
    f: &mut impl FnMut(&McModuleInst, &str, &mut BTreeMap<String, Vec<String>>),
    out: &mut BTreeMap<String, Vec<String>>,
) {
    f(inst, path, out);
    for sub in view.sub_modules(inst) {
        let sub_path = format!("{path}.{}", sub.name);
        collect_nets_impl(sub, view, &sub_path, f, out);
    }
}

/// Label one net point for the flat map. A point with no owner (a port or a
/// label) keeps its own path under either naming.
fn pin_label(np: &NetPoint, module_path: &str, naming: PointNaming) -> String {
    let Some(owner) = &np.owner else {
        return np.path.clone();
    };
    let inst = match naming {
        PointNaming::Local => owner.clone(),
        PointNaming::Hierarchical => format!("{module_path}.{owner}"),
    };
    format!("{}.{}", inst, last_segment(&np.path))
}

fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantic::common::IOType;

    #[test]
    fn pin_label_carries_the_module_path_only_when_asked() {
        let pin = NetPoint::with_owner("C1.1", "C1", IOType::InOut, None);
        assert_eq!(pin_label(&pin, "main", PointNaming::Local), "C1.1");
        assert_eq!(
            pin_label(&pin, "main", PointNaming::Hierarchical),
            "main.C1.1"
        );
        assert_eq!(
            pin_label(&pin, "main.ldo", PointNaming::Hierarchical),
            "main.ldo.C1.1"
        );
    }

    #[test]
    fn a_point_without_an_owner_keeps_its_own_path() {
        let point = NetPoint::new("V5V", IOType::None, None);
        assert_eq!(pin_label(&point, "main", PointNaming::Local), "V5V");
        assert_eq!(pin_label(&point, "main", PointNaming::Hierarchical), "V5V");
    }
}
