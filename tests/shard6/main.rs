// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shard binary: many former `tests/*.rs` test targets merged into one, so a
//! change in `src/` relinks a handful of binaries instead of 138.

#[path = "../common/mod.rs"]
pub mod common;

mod attr_dotted_name_n2;
mod barrier_isolation;
mod bom_binding;
mod check_top_pick;
mod dianlu_core;
mod enum_component_same_name;
mod error_does_not_block_instantiation;
mod fcall_return_shape;
mod flat_driver_conflict_vantage;
mod golden_ledger;
mod goto_def_connection_refs;
mod iface_mixed_group_binding;
mod instance_dnp_marker;
mod instance_nc_pin_marker;
mod lock_pp_exprs;
mod lock_pp_hw;
mod member_lane_alias;
mod nested_call_arg;
mod net_report_consistency;
mod pin_groups;
mod port_member_declared;
mod power_intent_l1;
mod rail_identity_declared;
mod root_layer_anchor;
mod single_port_representative;
mod top_series_passive_kept;
mod u79_r3_domain_bridge;
mod u97_declared_member_port;
mod vec_caret_reverse;
mod vec_degenerate_side_face;
mod vec_p25_expansion_equivalence;
mod vec_r4_column_width;
mod vec_r4_element_row;
