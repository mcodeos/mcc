// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::builder::*;
use crate::db::cmie::tables as workspace;
use crate::db::infra::global;
use crate::query::lookup::{find_by_name_in_project_tables, find_in_project_tables};
use crate::{McCMIE, McIds, McSpaceName, McURI};

// === pub fn mcb_print() { ===
pub fn mcb_print() {
    // Print system-level Interfaces (mcode directory)
    global::mcc_interfaces
        .borrow()
        .iter()
        .for_each(|interface| {
            println!("{}", interface.value().as_ref());
        });

    // Print project-level Interfaces
    workspace::WORKSPACE
        .interfaces
        .borrow()
        .iter()
        .for_each(|interface| {
            println!("{}", interface.value().as_ref());
        });

    workspace::WORKSPACE
        .components
        .borrow()
        .iter()
        .for_each(|component| {
            println!("{}", component.value().as_ref());
        });

    workspace::WORKSPACE
        .modules
        .borrow()
        .iter()
        .for_each(|module| {
            println!("{}", module.value().as_ref());
        });

    // global::mcc_enums.borrow().iter().for_each(|enum_def| {
    //     println!("{:#?}", enum_def.value().as_ref());
    // });

    workspace::WORKSPACE
        .enums
        .borrow()
        .iter()
        .for_each(|enum_def| {
            println!("{}", enum_def.value().as_ref());
        });
}

// === pub fn mcb_print_lines() { ===
/// Print Lines information for all modules (used for drawing-side debugging)
pub fn mcb_print_lines() {
    let modules = workspace::WORKSPACE.modules.borrow();

    if modules.is_empty() {
        println!("⚠️  prj_modules is empty, no module definitions found");
        return;
    }

    println!("╠════════════════════════════════════════════════════════════════╣");
    println!(
        "║  Found {} modules                                              ",
        modules.len()
    );

    for entry in modules.iter() {
        let space_name = entry.key();
        let module_def = entry.value();

        println!("┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("┃ Module: {}", module_def.name);
        println!("┃ URI: {}", space_name.uri);
        println!("┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        // Interface
        println!("┃ Interface:");
        println!(
            "┃   inputs:  {:?}",
            module_def
                .insts
                .inputs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        println!(
            "┃   outputs: {:?}",
            module_def
                .insts
                .outputs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        println!(
            "┃   bidirs:  {:?}",
            module_def
                .insts
                .bidirs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );

        // Symbol Table
        println!(
            "┃ Symbol Table ({} symbols):",
            module_def.insts.iter().count()
        );
        for (key, ident) in module_def.insts.iter() {
            println!("┃   - {} : {}", key, ident.get_name());
        }

        // Lines
        println!("┃ Lines ({} connections):", module_def.lines.len());

        for (i, line) in module_def.lines.iter().enumerate() {
            println!("┃");
            println!("┃   ┌─── Line[{i}] ───────────────────────────────");
            print_phrase_internal(line, "┃   │  ");
            println!("┃   └──────────────────────────────────────────────");
        }

        println!("┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }
}

// === fn print_phrase_internal(phrase: &crate::semantic::basic::mc_phrase::McPhrase, p ===
/// Print an McPhrase
pub(crate) fn print_phrase_internal(
    phrase: &crate::semantic::basic::mc_phrase::McPhrase,
    prefix: &str,
) {
    use crate::semantic::basic::mc_endpoint::McEndpoint;
    use crate::semantic::basic::mc_phrase::McPhrase;
    match phrase {
        McPhrase::Series(phrases) => {
            if phrases.is_empty() {
                println!("{prefix}(empty seq)");
                return;
            }
            for (i, p) in phrases.iter().enumerate() {
                if i > 0 {
                    println!("{prefix}    │");
                    println!("{prefix}    â–¼");
                }
                print_phrase_internal(p, prefix);
            }
        }
        McPhrase::Parallel(phrases) => {
            println!("{}(Parallel {})", prefix, phrases.len());
            for (i, p) in phrases.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  [{i}]:"));
            }
        }
        McPhrase::Closure(c) => {
            println!("{}(closure {} lines)", prefix, c.body.len());
            for (i, p) in c.body.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  body[{i}]:"));
            }
        }
        McPhrase::Group(g) => {
            println!("{}(group {} items)", prefix, g.opds.len());
            for (i, p) in g.opds.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  [{i}]:"));
            }
        }
        McPhrase::FuncCall(f) => {
            // Check if it is a pre-closure pattern
            let is_pre_closure = if let Some(c) = &f.caller {
                if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                    let func_name_str = inner_fc.func_name.to_string();
                    func_name_str
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_uppercase())
                } else {
                    false
                }
            } else {
                false
            };

            print!("{prefix}(funcall: ");
            if let Some(c) = &f.caller {
                if is_pre_closure {
                    // Pre-closure: print ClassName(params) -> MethodName
                    if let McPhrase::FuncCall(inner_fc) = c.as_ref() {
                        print!("{}(", inner_fc.func_name);
                        let inner_params: Vec<String> =
                            inner_fc.params.iter().map(|p| format!("{p}")).collect();
                        print!("{})", inner_params.join(", "));
                    }
                    print!(" -> ");
                } else {
                    print_phrase_internal(c, "");
                    print!(".");
                }
            }
            print!("{}", f.func_name);
            let param_strs: Vec<String> = f.params.iter().map(|p| format!("{p}")).collect();
            // If in pre-closure mode, skip the leading "_" placeholder
            let display_params = if is_pre_closure && param_strs.first() == Some(&"_".to_string()) {
                &param_strs[1..]
            } else {
                &param_strs
            };
            print!("({})", display_params.join(", "));
            println!(")");
        }
        McPhrase::Member(inner, endpoint) => {
            print_phrase_internal(inner, prefix);
            println!("{prefix}    .{endpoint}");
        }
        McPhrase::Endpoint(McEndpoint::Node { input, output }) => {
            let input_str: Vec<String> = input.iter().map(|e| format!("{e}")).collect();
            let output_str: Vec<String> = output.iter().map(|e| format!("{e}")).collect();
            println!(
                "{}(node: {{{} | {}}})",
                prefix,
                input_str.join(", "),
                output_str.join(", ")
            );
        }
        McPhrase::Endpoint(ep) => {
            println!("{prefix}(endpoint: {ep})");
        }
        McPhrase::Multiple(phrases) => {
            println!("{}(multiple {} items)", prefix, phrases.len());
            for (i, p) in phrases.iter().enumerate() {
                print_phrase_internal(p, &format!("{prefix}  [{i}]:"));
            }
        }
        McPhrase::Transposed(inner) => {
            print!("{prefix}(transposed: ");
            print_phrase_internal(inner, "");
            println!(")");
        }
        McPhrase::Lead => {
            println!("{prefix}(lead)");
        }
    }
}

// === pub fn mcb_debug_get_cmie(class_name: &McIds, uri: &McURI) { ===
pub fn mcb_debug_get_cmie(class_name: &McIds, uri: &McURI) {
    let name_str = class_name.to_string();
    eprintln!("╔══════════════════════════════════════════════════════╗");
    eprintln!("â•' DEBUG mcb_get_cmie                                  â•'");
    eprintln!("â•' class_name: {name_str:40} â•'");
    eprintln!("â•' uri:        {uri:40} â•'");
    eprintln!("╠══════════════════════════════════════════════════════╣");

    // Step 1: system lib
    let mcode_found = crate::db::infra::lib_mgr::mcc_blibs
        .borrow()
        .get(&"mcode".to_string())
        .is_some();
    eprintln!("â•' Step 1: mcode system lib exists = {mcode_found}");
    // [Diagnostic] Step 1: search in mcode base library
    if let Some(mcode) = crate::db::infra::lib_mgr::mcc_blibs
        .borrow()
        .get(&"mcode".to_string())
    {
        let has_entry = mcode.spacenames.get(class_name).is_some();
        eprintln!("â•'   spacenames.get({name_str}) = {has_entry}");
        if has_entry {
            eprintln!(
                "║   ⚠️  System library hit! may return system library version (empty module)"
            );
        }
    }

    // Step 2: prj_mcodes
    let mcodes_has_uri = workspace::WORKSPACE.mcodes.borrow().get(uri).is_some();
    eprintln!("â•' Step 2: prj_mcodes.get(uri) = {mcodes_has_uri}");
    if let Some(mcfile) = workspace::WORKSPACE.mcodes.borrow().get(uri) {
        let has_spacename = mcfile.value().spacenames.get(class_name).is_some();
        eprintln!("â•'   spacenames.get({name_str}) = {has_spacename}");
        if let Some(sn) = mcfile.value().spacenames.get(class_name) {
            let sn_val = sn.clone();
            eprintln!("â•'   SpaceName.ident = {}", sn_val.ident);
            eprintln!("â•'   SpaceName.uri   = {}", sn_val.uri);
            let found = find_in_project_tables(&sn_val);
            eprintln!("â•'   find_in_project_tables = {}", found.is_some());
            if let Some(McCMIE::Module(m)) = &found {
                eprintln!(
                    "║   ✅ Module found! lines={}, symbols={}",
                    m.lines.len(),
                    m.insts.iter().count()
                );
            }
        }
    }

    // Step 3: direct construct
    let direct_sn = McSpaceName::new(&class_name.clone(), uri.clone());
    let direct_found = find_in_project_tables(&direct_sn);
    eprintln!(
        "â•' Step 3: direct SpaceName({}, {}) = {}",
        name_str,
        uri,
        direct_found.is_some()
    );

    // Step 5: by name
    let by_name = find_by_name_in_project_tables(class_name);
    eprintln!("â•' Step 5: find_by_name = {}", by_name.is_some());
    if let Some(McCMIE::Module(m)) = &by_name {
        eprintln!(
            "║   ✅ Module found! lines={}, symbols={}",
            m.lines.len(),
            m.insts.iter().count()
        );
    }

    // Full prj_modules state
    let modules = workspace::WORKSPACE.modules.borrow();
    eprintln!("╠══════════════════════════════════════════════════════╣");
    eprintln!("║ prj_modules status: {} modules", modules.len());
    for entry in modules.iter() {
        let key = entry.key();
        let val = entry.value();
        eprintln!(
            "║   {} (uri={}) → lines={}, symbols={}",
            key.ident,
            key.uri,
            val.lines.len(),
            val.insts.iter().count()
        );
    }
    eprintln!("╚══════════════════════════════════════════════════════╝");
}
