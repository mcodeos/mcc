// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shard binary: many former `tests/*.rs` test targets merged into one, so a
//! change in `src/` relinks a handful of binaries instead of 138.

#[path = "../common/mod.rs"]
pub mod common;

mod device_layer_layout;
mod diagnostic_regressions;
mod dynamic_pin_access;
mod error_codes;
mod gap2_materialization;
mod gap3_materialization;
mod iface_role_arg_literal;
mod inst_list;
mod lock_pp_attrs_insts;
mod param_prefix_instance_receiver;
mod param_prefix_uscore_count;
mod pins_empty_declaration;
mod query_projections;
mod read_api;
mod retirement_net_classification;
mod u107_bare_port_member_source;
mod u108_formal_without_declared_port;
mod u119_port_written_order;
mod u120_org_directory;
mod u121_reverse_index;
mod u122_partition_block;
mod u124_offset_alignment;
mod u127_boundary_point_naming;
mod u31_positional_fallback;
mod use_statement_diagnostics;
mod vec_parallel_transposed_bridge;
mod vec_range_declare_equivalence;
mod vector_member_access;
