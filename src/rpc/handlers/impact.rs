// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use super::*;

// === handle_impact (design world-repartition-design.md §6.1) ===

/// Why an `impact` request produced no report. The two arms map to the two
/// documented exit codes (`1` sym unresolved, `2` world build failed), so the
/// distinction is kept instead of being flattened into one message.
pub enum ImpactError {
    /// The symbol names no live def and no instance path.
    Unresolved(String),
    /// The scope world could not be built or projected.
    Build(String),
}

/// The resolved subject of an impact question: the def whose change is being
/// assessed, plus the identity instances are matched on.
struct ImpactTarget {
    def_id: crate::DefId,
    uri: String,
    ident: String,
}

/// MCP face of `mcc impact` (design §6.1: "MCP and CLI, one engine").
pub fn handle_impact(params: Option<Value>) -> RpcResult {
    #[derive(Deserialize)]
    struct ImpactParams {
        sym: String,
        entry: String,
        #[serde(default)]
        top: Option<String>,
        #[serde(default)]
        libs: Vec<String>,
    }
    let p: ImpactParams = parse_strict(params)?;
    impact_report(&p.sym, &p.entry, p.top.as_deref(), &p.libs).map_err(|e| match e {
        ImpactError::Unresolved(s) => JsonRpcError::custom(-32602, &s),
        ImpactError::Build(s) => JsonRpcError::custom(-32603, &s),
    })
}

/// The blast radius of changing `sym`: which top circuits were instantiated
/// over it, which nets its instances sit on, and how many instances reference
/// it (design §6.1). The def->circuits half reads the world's invalidation
/// index, never a fresh scan.
pub fn impact_report(
    sym: &str,
    entry: &str,
    top: Option<&str>,
    libs: &[String],
) -> Result<Value, ImpactError> {
    let _ = libs;
    let uri = crate::McURI::from(entry);
    let _ = crate::mcc_load_project(&uri);

    let top = match top {
        Some(t) => t.to_string(),
        None => crate::mcb_get_first_module_name()
            .ok_or_else(|| ImpactError::Build("no module found in file (use --top)".into()))?,
    };

    let (mut world, key, synthetic) = crate::mcc_virtual_build_world(&top, &uri, 1000)
        .map_err(|e| ImpactError::Build(format!("{}", e)))?;
    match synthetic {
        Some(prefix) => world.flatten_with_prefix(&key, &prefix),
        None => world.flatten(&key),
    }
    .map_err(|e| ImpactError::Build(format!("flatten failed: {}", e)))?;

    let dl = world
        .circuit(&key)
        .ok_or_else(|| ImpactError::Build("world build produced no circuit".into()))?;
    let table = dl
        .table()
        .ok_or_else(|| ImpactError::Build("no flat table after flatten".into()))?;

    let target = resolve_impact_target(sym, table)?;

    let circuits: Vec<Value> = world
        .invalidated(target.def_id)
        .iter()
        .map(|k| json!({ "entry": k.entry_uri, "top": k.top }))
        .collect();

    let mut nets: Vec<String> = Vec::new();
    let mut consumers = 0usize;
    for comp in table.get_components() {
        if !inst_matches(comp, &target.ident) {
            continue;
        }
        consumers += 1;
        for pin in table.get_pins_of(comp.id) {
            for &net_id in table.nets_of(pin.id) {
                if let Some(net) = table.get_net(net_id) {
                    if !nets.iter().any(|n| n == &net.name) {
                        nets.push(net.name.clone());
                    }
                }
            }
        }
    }
    nets.sort();

    Ok(json!({
        "schema_version": "impact.1.0",
        "sym": sym,
        "defId": target.def_id,
        "uri": target.uri,
        "impacted": {
            "circuits": circuits,
            "nets": nets,
            "consumers": consumers,
        },
        "world_ver": crate::stages::world_ver::world_ver(),
    }))
}

/// A def named directly, or the class of an instance addressed by path.
/// `sym` is a name, so every comparison here is exact string equality.
fn resolve_impact_target(sym: &str, table: &crate::InstTable) -> Result<ImpactTarget, ImpactError> {
    if let Some((ident, uri)) = def_by_name(sym) {
        let sn = crate::McSpaceName::new(
            &crate::McIds::from(ident.as_str()),
            crate::McURI::from(uri.as_str()),
        );
        let kind = crate::kind_of(&sn)
            .ok_or_else(|| ImpactError::Unresolved(format!("impact: no live def for '{}'", sym)))?;
        let def_id = crate::def_id(&sn, kind)
            .ok_or_else(|| ImpactError::Unresolved(format!("impact: no live def for '{}'", sym)))?;
        return Ok(ImpactTarget { def_id, uri, ident });
    }
    for entry in table
        .get_components()
        .into_iter()
        .chain(table.get_modules())
    {
        if entry.path == sym {
            if let Some(cd) = &entry.class_def {
                let ident = cd.ident.to_string();
                let uri = cd.uri.to_string();
                if let Some(kind) = crate::kind_of(cd) {
                    if let Some(def_id) = crate::def_id(cd, kind) {
                        return Ok(ImpactTarget { def_id, uri, ident });
                    }
                }
            }
            return Err(ImpactError::Unresolved(format!(
                "impact: instance '{}' names no live def",
                sym
            )));
        }
    }
    Err(ImpactError::Unresolved(format!(
        "impact: unresolved symbol '{}'",
        sym
    )))
}

/// The declaring `(ident, uri)` of a live def named `sym`, any kind. Matches
/// are ordered by uri so the choice is deterministic when a name repeats.
fn def_by_name(sym: &str) -> Option<(String, String)> {
    let ds = crate::definition_space();
    let mut spaces: Vec<crate::McSpaceName> = Vec::new();
    spaces.extend(ds.all_components().into_iter().map(|(sn, _)| sn));
    spaces.extend(ds.all_modules().into_iter().map(|(sn, _)| sn));
    spaces.extend(ds.all_interfaces().into_iter().map(|(sn, _)| sn));
    spaces.extend(ds.all_enums().into_iter().map(|(sn, _)| sn));
    let mut hits: Vec<(String, String)> = spaces
        .into_iter()
        .filter(|sn| sn.ident.to_string() == sym)
        .map(|sn| (sn.ident.to_string(), sn.uri.to_string()))
        .collect();
    hits.sort_by(|a, b| a.1.cmp(&b.1));
    hits.into_iter().next()
}

/// An instance is a consumer when its class is the def being assessed.
fn inst_matches(entry: &crate::InstEntry, ident: &str) -> bool {
    entry
        .class_def
        .as_ref()
        .is_some_and(|cd| cd.ident.to_string() == ident)
}
