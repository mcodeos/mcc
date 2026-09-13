// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // Rerun when any C source / header under src/ast/c changes (cp.sh updates).
    // NOTE: cargo only reads `cargo:` directives from stdout — must use println!.
    println!("cargo:rerun-if-changed=src/ast/c");

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // Build number: the ledger repo's shared batch counter (`mcd/BUILDNr`),
    // bumped once per landed batch and committed with it, so every author
    // bakes the same value. It is 0 when mcc is cloned standalone (no sibling
    // `mcd/`) — a per-machine counter is not reproducible off this machine,
    // which is the whole point of the number. Watching the file is what keeps
    // the crate from rebuilding on every invocation: the number moves once per
    // batch, not once per build.
    let nr = batch_nr(&manifest);
    println!("cargo:rustc-env=MCC_BUILD_NR={nr}");
    if let Some(path) = batch_nr_path(&manifest) {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // add C source files
    let mut build = cc::Build::new();
    build
        .compiler("gcc")
        .define("_POSIX_C_SOURCE", "200809L")
        .file("src/ast/c/lex.c")
        .file("src/ast/c/mca.tab.c")
        .file("src/ast/c/astdef.c")
        .file("src/ast/c/common.c")
        .file("src/ast/c/print.c")
        .file("src/ast/c/astvis.c")
        .flag("-std=c17")
        .flag("-Werror=implicit-function-declaration")
        .compile("cparts");

    // add header search paths
    println!("cargo:include=src/ast/c");

    // 2. generate macros from header file
    generate_macros_from_header();

    // 3. locate the mcode install script (cp.sh) and remind the developer to
    // run it after mcode library edits. The reminder must not nag on every
    // no-op build: it prints as regular (verbose-only) build output while the
    // script is present, and only rises to a `cargo:warning` when the script
    // genuinely cannot be found in any layout we know of.
    let cp_candidates = [
        manifest.join("mc/mcode/cp.sh"), // original monorepo layout
        manifest.join("../mcode/cp.sh"), // sibling checkout: mcc next to mcode
    ];
    if let Some(cp) = cp_candidates.iter().find(|p| p.exists()) {
        println!(
            "mcc: mcode install script present at {} — run it manually to copy \
             mcode files to your user directory after mcode library edits.",
            cp.display()
        );
    } else {
        let looked = cp_candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("cargo:warning=cp.sh script not found (looked at: {looked}). Please run it after mcode edits — required for the MCODE system to function correctly.");
    }
}

fn generate_macros_from_header() {
    let header_path = "src/ast/c/astdef.h";
    println!("cargo:rerun-if-changed={}", header_path);

    let header_content = match fs::read_to_string(header_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Warning: Could not read header file {}: {}", header_path, e);
            return;
        }
    };

    let mut rust_code = String::from(
        "// Copyright (c) 2026 MCode\n\
         //\n\
         // Licensed under either of Apache License, Version 2.0 or MIT License at your option.\n\
         //\n\
         // This file is auto-generated from C headers by build.rs\n\
         // DO NOT EDIT MANUALLY - any changes will be overwritten!\n\n",
    );

    // parse each line, extract macros
    for line in header_content.lines() {
        let line = line.trim();

        // match #define MCAST_XXX number format
        if line.starts_with("#define MCAST_") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let macro_name = parts[1];
                let value = parts[2];

                // add to generated Rust code
                rust_code.push_str(&format!("pub const {}: u16 = {};\n", macro_name, value));
            }
        }
    }

    // Write the generated Rust file only when the content actually changed.
    // A no-op regeneration (identical output) must not churn the file's
    // mtime nor nag "remember to commit" on every single build — the warning
    // is meant to fire exactly when a real header change needs committing.
    let out_path = PathBuf::from("src/ast/macros.rs");
    let unchanged = fs::read_to_string(&out_path)
        .map(|existing| existing.trim_end() == rust_code.trim_end())
        .unwrap_or(false);
    if unchanged {
        return;
    }
    if let Err(e) = fs::write(&out_path, rust_code) {
        eprintln!("Error writing macros.rs: {}", e);
    } else {
        println!(
            "cargo:warning=Regenerated {} changed - remember to commit this file!",
            out_path.display()
        );
    }
}

/// Path to the ledger repo's batch number, or `None` when mcc is standalone.
/// The in-tree `mcd/` layout mirrors the `cp.sh` probe above.
fn batch_nr_path(manifest: &Path) -> Option<PathBuf> {
    [
        manifest.join("mcd/BUILDNr"),
        manifest.join("../mcd/BUILDNr"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

fn batch_nr(manifest: &Path) -> u64 {
    batch_nr_path(manifest)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
}
