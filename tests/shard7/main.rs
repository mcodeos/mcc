// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shard binary: many former `tests/*.rs` test targets merged into one, so a
//! change in `src/` relinks a handful of binaries instead of 138.

#[path = "../common/mod.rs"]
pub mod common;

mod ac_face_gates;
mod bom_nc_classified;
mod build_dir_net_checks;
mod build_products;
mod dedup_id_coverage;
mod det_probe;
mod diff_saved_readings;
mod equi_e2e;
mod erc_single_ruler;
mod expr_dot_curly;
mod flatten_net_check_diagnostics;
mod floating_label;
mod iface_connect_rule;
mod iface_exclusive_peer;
mod iface_pin_number_binding;
mod lock_pp_conds;
mod lock_pp_defs;
mod lock_pp_naming_ports;
mod mcode_auto_load;
mod module_port_interface_ref;
mod net_island_l1;
mod output_path_flag;
mod param_call_site_key_binding;
mod param_group_prefix;
mod param_pin_same_name;
mod point_identity_stage_key;
mod port_row_with_connection;
mod product_order;
mod pwrflow_l1;
mod replication_operator_u155;
mod show_target_law;
mod stage_diff_command;
mod stage_join;
mod stage_mcp_mirror;
mod stage_p2_diff;
mod stage_p2_view;
mod stage_top_ver;
mod stage_trace;
mod stage_vec_diff;
mod stage_vec_view;
mod stage_viz_diff;
mod stage_viz_view;
mod tablea_dispatch_regression;
mod u131_named_func_args;
mod u138_iface_return_face;
mod u141_parsed_pins_boundary;
mod u216_canon_names;
mod u54_parameter_default;
mod ghost_port_boundary;
mod vec_group_expansion_equivalence;
mod vec_lane_chain_width;
mod vec_net_crossnet;
mod vec_r0_operator_encoding;
mod vec_rule_discrimination;
