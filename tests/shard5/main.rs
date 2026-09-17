// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shard binary: many former `tests/*.rs` test targets merged into one, so a
//! change in `src/` relinks a handful of binaries instead of 138.

#[path = "../common/mod.rs"]
pub mod common;

mod auto_naming_lock;
mod bitwise_cond;
mod bom_nc_hbl;
mod cond_duplicate;
mod curly_option;
mod defspace_wiring;
mod dotted_int_component_name;
mod entry_discovery;
mod eval_engine;
mod export_kind_lists;
mod gate_phase1;
mod ground_projection_declared;
mod hex_literal;
mod layout_attribute;
mod lead_crossnet_warning;
mod lock_pp_body;
mod phrase_reserved_word_subscribed;
mod port_bus_upgrade;
mod render_store_silence;
mod single_member_range;
mod u12_value_pairing_hbl;
mod u63_call_arg_binding;
mod vec_body_portcount;
mod vec_r0_operator_fidelity;
mod vec_series_rowzip;
mod vector_inst_materialize;
mod workspace_identity;
