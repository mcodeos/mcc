// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Error code catalog (M6).
//!
//! Central registry of diagnostic codes emitted by the mcc compiler.
//! Each code has a symbolic constant, a human-readable name, and a description.
//! Used by `mcc explain` and by AI tools consuming the JSON-RPC envelope.
//!
//! ## Adding a new code
//!
//! 1. Add a `pub const` in the appropriate section below.
//! 2. Add a match arm in [`describe()`].
//! 3. Update the `ALL_CODES` array.

// ============================================================================
// Pass1a: Type collection (interface / component / enum)
// ============================================================================

/// Duplicate interface definition in the same file.
pub const DUPLICATE_INTERFACE: u32 = 1001;
/// Duplicate component definition in the same file.
pub const DUPLICATE_COMPONENT: u32 = 1002;
/// Duplicate enum definition in the same file.
pub const DUPLICATE_ENUM: u32 = 1004;

// ============================================================================
// Pass1b: Module body resolution
// ============================================================================

/// Duplicate module definition (same name in same file).
pub const DUPLICATE_MODULE: u32 = 1503;
/// Definition not found — a name reference could not be resolved.
pub const DEFINITION_NOT_FOUND: u32 = 1504;

// ============================================================================
// Name resolution / instance lookup
// ============================================================================

/// Component name could not be resolved to a definition.
pub const COMPONENT_NOT_FOUND: u32 = 1100;
/// Module name could not be resolved to a definition.
pub const MODULE_NOT_FOUND: u32 = 1101;
/// Interface name could not be resolved to a definition.
pub const INTERFACE_NOT_FOUND: u32 = 1102;
/// Enum name could not be resolved to a definition.
pub const ENUM_NOT_FOUND: u32 = 1103;
/// Instance reference not found in symbol table.
pub const INSTANCE_NOT_FOUND: u32 = 1105;
/// IDS (dotted name) resolution failed.
pub const IDS_RESOLVE_FAILED: u32 = 1106;

// ============================================================================
// Module body: connection / net errors
// ============================================================================

/// Cannot connect — shape mismatch between left and right sides.
pub const SHAPE_MISMATCH: u32 = 301;
/// Invalid IO type for a port declaration.
pub const INVALID_IO_TYPE: u32 = 302;
/// Transpose operation is not allowed at this position.
pub const CANNOT_TRANSPOSE: u32 = 303;
/// Port name not found in the module's symbol table.
pub const PORT_NOT_FOUND: u32 = 1200;
/// Duplicate port name in module.
pub const DUPLICATE_PORT: u32 = 1201;
/// Bus reference refers to a non-existent component.
pub const BUS_REF_NOT_FOUND: u32 = 1202;

// ============================================================================
// Component definition errors
// ============================================================================

/// Invalid pin definition in component body.
pub const INVALID_PIN: u32 = 1800;
/// Invalid attribute definition in component body.
pub const INVALID_ATTRIBUTE: u32 = 1801;
/// Two-pin component requires exactly 2 pins.
pub const TWOPIN_REQUIRES_TWO_PINS: u32 = 1804;

// ============================================================================
// Catalog infrastructure
// ============================================================================

/// A human-readable error code entry.
#[derive(Clone)]
pub struct ErrorCodeInfo {
    pub code: u32,
    pub name: &'static str,
    pub description: &'static str,
}

/// All registered error codes (used by `mcc explain` without arguments).
pub fn all_codes() -> &'static [ErrorCodeInfo] {
    &ALL_CODES
}

/// Look up a single error code. Returns `None` if unknown.
pub fn describe(code: u32) -> Option<ErrorCodeInfo> {
    ALL_CODES.iter().find(|e| e.code == code).cloned()
}

macro_rules! entry {
    ($const:ident, $desc:expr) => {
        ErrorCodeInfo {
            code: $const,
            name: stringify!($const),
            description: $desc,
        }
    };
}

static ALL_CODES: &[ErrorCodeInfo] = &[
    // Pass1a
    entry!(DUPLICATE_INTERFACE, "An interface with the same name already exists in this file."),
    entry!(DUPLICATE_COMPONENT, "A component with the same name already exists in this file."),
    entry!(DUPLICATE_ENUM, "An enum with the same name already exists in this file."),
    // Pass1b
    entry!(DUPLICATE_MODULE, "A module with the same name already exists in this file."),
    entry!(DEFINITION_NOT_FOUND, "A name reference could not be resolved to any known definition."),
    // Name resolution
    entry!(COMPONENT_NOT_FOUND, "Component name could not be resolved to a definition."),
    entry!(MODULE_NOT_FOUND, "Module name could not be resolved to a definition."),
    entry!(INTERFACE_NOT_FOUND, "Interface name could not be resolved to a definition."),
    entry!(ENUM_NOT_FOUND, "Enum name could not be resolved to a definition."),
    entry!(INSTANCE_NOT_FOUND, "Instance reference not found in the module's symbol table."),
    entry!(IDS_RESOLVE_FAILED, "Dotted name (IDS) resolution failed — a segment could not be found."),
    // Connection / net
    entry!(SHAPE_MISMATCH, "Cannot connect — the left and right sides have different shapes (e.g., pin count mismatch)."),
    entry!(INVALID_IO_TYPE, "Invalid IO type for a port declaration. Allowed: in, out, inout, power, analog."),
    entry!(CANNOT_TRANSPOSE, "The transpose operator (apostrophe) is not allowed at this position."),
    entry!(PORT_NOT_FOUND, "Port name not found in the module's symbol table."),
    entry!(DUPLICATE_PORT, "Duplicate port name in module declaration."),
    entry!(BUS_REF_NOT_FOUND, "A bus reference points to a non-existent component."),
    // Component
    entry!(INVALID_PIN, "Invalid pin definition in a component body."),
    entry!(INVALID_ATTRIBUTE, "Invalid attribute definition in a component body."),
    entry!(TWOPIN_REQUIRES_TWO_PINS, "A two-pin component requires exactly 2 pins to be defined."),
];
