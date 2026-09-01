// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

use crate::output::OutputFormatExt;
use mcc::cli::{OutputFormat, PinSortMode};
use mcc::{McModule, MccProjectTree, NodeArena};
use tracing::info;

pub trait OutputRenderer {
    fn pass1_header(&self, uri: &str);
    fn pass1_definitions(&self, modules: usize, components: usize, interfaces: usize);
    fn module_ports(&self, module_def: &McModule);
    fn module_symbols(&self, module_def: &McModule);
    fn module_stmts(&self, module_def: &McModule);

    fn pass2_header(&self, top_name: &str);
    fn pass2_failed(&self, err: &str);
    fn instances(&self, inst: &MccProjectTree, depth: usize, arena: Option<&NodeArena>);
    fn connections(&self, inst: &MccProjectTree, depth: usize, arena: Option<&NodeArena>);
    fn nets(&self, inst: &MccProjectTree, depth: usize, arena: Option<&NodeArena>);
    fn net_summary(&self, inst: &MccProjectTree, arena: Option<&NodeArena>);

    fn viz_written(&self, path: &str, bytes: usize);
}

pub fn for_format_with_sort(format: OutputFormat, sort: PinSortMode) -> Box<dyn OutputRenderer> {
    if format.is_structured() {
        Box::new(SilentRenderer)
    } else {
        Box::new(TextRenderer::with_sort(sort))
    }
}

pub struct SilentRenderer;

impl OutputRenderer for SilentRenderer {
    fn pass1_header(&self, _: &str) {}
    fn pass1_definitions(&self, _: usize, _: usize, _: usize) {}
    fn module_ports(&self, _: &McModule) {}
    fn module_symbols(&self, _: &McModule) {}
    fn module_stmts(&self, _: &McModule) {}
    fn pass2_header(&self, _: &str) {}
    fn pass2_failed(&self, _: &str) {}
    fn instances(&self, _: &MccProjectTree, _: usize, _: Option<&NodeArena>) {}
    fn connections(&self, _: &MccProjectTree, _: usize, _: Option<&NodeArena>) {}
    fn nets(&self, _: &MccProjectTree, _: usize, _: Option<&NodeArena>) {}
    fn net_summary(&self, _: &MccProjectTree, _: Option<&NodeArena>) {}
    fn viz_written(&self, _: &str, _: usize) {}
}

struct TextRenderer {
    sort: PinSortMode,
}

impl TextRenderer {
    fn with_sort(sort: PinSortMode) -> Self {
        Self { sort }
    }
}

impl OutputRenderer for TextRenderer {
    fn pass1_header(&self, uri: &str) {
        println!("\n═══════════════════════════════════════════════════════════════");
        println!(" Pass 1");
        println!("═══════════════════════════════════════════════════════════════");
        println!("loading: {}", uri);
    }

    fn pass1_definitions(&self, modules: usize, components: usize, interfaces: usize) {
        println!(
            "definitions: {} modules, {} components, {} interfaces",
            modules, components, interfaces
        );
    }

    fn module_ports(&self, module_def: &McModule) {
        println!("\n───────────────────────────────────────────────────────────────");
        println!(" Module: {}", module_def.name);
        println!("───────────────────────────────────────────────────────────────");
        println!(" ports:");
        println!(
            "   inputs:  {:?}",
            module_def
                .insts
                .inputs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        println!(
            "   outputs: {:?}",
            module_def
                .insts
                .outputs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        println!(
            "   bidirs:  {:?}",
            module_def
                .insts
                .bidirs_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
        println!(
            "   powers:  {:?}",
            module_def
                .insts
                .powers_with_name()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        );
    }

    fn module_symbols(&self, module_def: &McModule) {
        println!(" symbols ({}):", module_def.insts.iter().count());
        for (key, ident) in module_def.insts.iter() {
            let type_name = ident.type_name();
            println!("   {:<15} {}", type_name, key);
        }
    }

    fn module_stmts(&self, module_def: &McModule) {
        println!(" stmts ({}):", module_def.stmts.len());
        if module_def.stmts.is_empty() {
            println!("   (none)");
        } else {
            for (i, stmt) in module_def.stmts.iter().enumerate() {
                println!("   Series[{}]:", i);
                crate::cmds::print::print_phrase_members(stmt, "     ");
            }
        }
    }

    fn pass2_header(&self, top_name: &str) {
        println!("\n═══════════════════════════════════════════════════════════════");
        println!(" Pass 2");
        println!("═══════════════════════════════════════════════════════════════");
        println!("instantiating: {}", top_name);
    }

    fn pass2_failed(&self, err: &str) {
        println!("! instantiation failed: {}", err);
        println!("? hint: dependent component/module may be undefined");
    }

    fn instances(&self, inst: &MccProjectTree, depth: usize, arena: Option<&NodeArena>) {
        println!("\n───────────────────────────────────────────────────────────────");
        println!(" Instance Tree");
        println!("───────────────────────────────────────────────────────────────");
        crate::cmds::print::print_module_inst(inst, depth, self.sort, arena);
    }

    fn connections(&self, inst: &MccProjectTree, depth: usize, arena: Option<&NodeArena>) {
        println!("\n───────────────────────────────────────────────────────────────");
        println!(" Connections");
        println!("───────────────────────────────────────────────────────────────");
        crate::cmds::print::print_connections(inst, depth, arena);
    }

    fn nets(&self, inst: &MccProjectTree, depth: usize, arena: Option<&NodeArena>) {
        println!("\n───────────────────────────────────────────────────────────────");
        println!(" Nets");
        println!("───────────────────────────────────────────────────────────────");
        crate::cmds::print::print_nets(inst, depth, arena);
    }

    fn net_summary(&self, inst: &MccProjectTree, arena: Option<&NodeArena>) {
        crate::cmds::print::print_net_summary(inst, arena);
    }

    fn viz_written(&self, path: &str, bytes: usize) {
        info!(target: "mcc::parse", "✓ wrote {} ({} bytes)", path, bytes);
    }
}
