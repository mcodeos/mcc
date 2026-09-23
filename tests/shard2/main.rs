// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Shard binary: many former `tests/*.rs` test targets merged into one, so a
//! change in `src/` relinks a handful of binaries instead of 138.

#[path = "../common/mod.rs"]
pub mod common;

mod attr_as_endpoint_e4025;
mod attr_key_duplicate;
mod attr_value_vocabulary;
mod adopt_dotted_numeric_tail;
mod computed_pin_names;
mod cond_branch_dynamic_pins;
mod ctor_arg_family;
mod defspace_golden;
mod dynamic_pin_expansion;
mod export_spice_refdes;
mod ground_net_unification;
mod lead_classification;
mod lock_pp_duplicates;
mod lock_pp_extra;
mod member_role_declared_identity;
mod pin_member_spelling;
mod pins_index_access;
mod pins_self_face_phrase;
mod resolve_policy;
mod use_import_codes;
mod vec_array_fold_equivalence;
mod vec_per_edge_truth;
mod world_projection;
