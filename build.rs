// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use std::fs;
use std::path::PathBuf;

fn main() {
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

    // 3. check zcp.sh script exists and add warning message
    let zcp_path = PathBuf::from("mc/mcode/zcp.sh");
    if zcp_path.exists() {
        eprintln!("cargo:warning=Please run 'bash mc/mcode/zcp.sh' manually to copy mcode files to your user directory.");
        eprintln!(
            "cargo:warning=This step is required for the MCODE system to function correctly."
        );
    } else {
        eprintln!(
            "cargo:warning=zcp.sh script not found. Please ensure it exists at mc/mcode/zcp.sh"
        );
    }
}

fn generate_macros_from_header() {
    let header_path = "src/ast/c/astdef.h";
    eprintln!("cargo:rerun-if-changed={}", header_path);

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

    // write generated Rust file
    let out_path = PathBuf::from("src/ast/c_macros.rs");
    if let Err(e) = fs::write(&out_path, rust_code) {
        eprintln!("Error writing c_macros.rs: {}", e);
    } else {
        eprintln!(
            "cargo:warning=Generated {} - remember to commit this file!",
            out_path.display()
        );
    }
}
