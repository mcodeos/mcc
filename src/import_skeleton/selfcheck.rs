// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Compile self-check for generated skeletons: write `main.mc` into a scratch
//! dir, run it through the same tree builder the export face uses, and count
//! diagnostics from **both** layers — the flatten pass (`build_tree_diags`,
//! net-level) and the workspace diagnostics the parse/semantic passes left on
//! the file (E2116 empty-pins lives there, not in flatten). Zero errors is the
//! delivery gate: the generator must never hand out a project that does not
//! compile (bad net ident, bad builtin args, pin names colliding with keywords
//! all detonate here, before any snapshot diff could).

use std::collections::BTreeMap;

pub struct Check {
    pub errors: usize,
    pub warnings: usize,
    /// human-readable digest (diagnostic lines), for stderr / bail messages
    pub detail: String,
}

/// Build `files["main.mc"]` in a fresh scratch project and count diagnostics.
pub fn check(files: &BTreeMap<String, String>) -> Check {
    let Some(main_mc) = files.get("main.mc") else {
        return Check {
            errors: 1,
            warnings: 0,
            detail: "selfcheck: no main.mc in generated files".to_string(),
        };
    };
    let dir = std::env::temp_dir().join(format!(
        "mcc-skeleton-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Check { errors: 1, warnings: 0, detail: format!("selfcheck: scratch dir: {e}") };
    }
    let path = dir.join("main.mc");
    let result = std::fs::write(&path, main_mc)
        .map_err(|e| format!("selfcheck: write: {e}"))
        .and_then(|_| {
            crate::export::build_tree_diags(path.to_string_lossy().as_ref(), Some("main"), &[])
                .map_err(|e| format!("selfcheck: build: {e}"))
        });
    let _ = std::fs::remove_dir_all(&dir);
    let uri = crate::McURI::from(path.to_string_lossy().as_ref());

    match result {
        Ok((_, _, _, _, diags)) => {
            let mut errors = 0;
            let mut warnings = 0;
            let mut detail = String::new();
            for dg in &diags {
                match dg.level {
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Error => errors += 1,
                    _ => warnings += 1,
                }
                detail.push_str(&format!("  [E{} {:?}] {}\n", dg.code, dg.level, dg.msg));
            }
            // Parse/semantic-layer diagnostics live on the workspace store
            // (E2116 and friends) — flatten does not carry them
            let mgr = crate::db::cmie::tables::WORKSPACE.diagnostics.lock().unwrap();
            let stored: Vec<_> = mgr.get_diagnostics_for_file(&uri);
            let stored: Vec<(u32, crate::db::diagnostic::diagnostic::DiagnosticLevel, String)> =
                stored.iter().map(|d| (d.code, d.level, d.msg.clone())).collect();
            drop(mgr);
            for (code, level, msg) in stored {
                match level {
                    crate::db::diagnostic::diagnostic::DiagnosticLevel::Error => errors += 1,
                    _ => warnings += 1,
                }
                detail.push_str(&format!("  [E{code} {level:?}] {msg}\n"));
            }
            Check { errors, warnings, detail }
        }
        Err(m) => Check { errors: 1, warnings: 0, detail: m },
    }
}
