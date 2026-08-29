// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Name resolution trait decoupling `semantic/` from the global state in `db/`.
//!
//! [`NameResolver`] — resolve class names to CMIE definitions (replaces `mcb_get_cmie`),
//! with [`resolve_cmie`] as the drop-in bridge used by semantic passes.

use crate::{McCMIE, McIds, McURI};

// ============================================================================
// Free function — bridge from global mcb_get_cmie to trait injection
// ============================================================================

/// Resolve a CMIE definition using the provided resolver.
/// Drop-in replacement for `mcb_get_cmie(&ids, &uri)`.
///
/// ## Migration example
///
/// ```ignore
/// // Before (global state):
/// let cmie = mcb_get_cmie(&ids, uri);
///
/// // After (trait injection):
/// let cmie = resolve_cmie(ctx, &ids, uri);
/// ```
pub fn resolve_cmie(
    ctx: &impl NameResolver,
    class_name: &McIds,
    from_uri: &McURI,
) -> Option<McCMIE> {
    ctx.resolve(class_name, from_uri).map(|(cmie, _)| cmie)
}

// ============================================================================
// NameResolver — resolve class names to definitions
// ============================================================================

/// Resolves class names (components, modules, interfaces, enums) to their
/// CMIE definitions. Abstracts over the double-layer (workspace → global)
/// lookup in `db/cmie/cmie.rs`.
pub trait NameResolver {
    /// Resolve a class name in the context of a source URI.
    /// Returns the CMIE and the URI where it was defined.
    fn resolve(&self, class_name: &McIds, from_uri: &McURI) -> Option<(McCMIE, McURI)>;

    /// Look up a class name in system tables only (no workspace layer).
    /// Only exercised via `MockResolver` in tests; production resolution goes
    /// through `Resolver::resolve_system` directly.
    #[allow(dead_code)]
    fn resolve_system(&self, class_name: &McIds) -> Option<McCMIE>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that a mock resolver can be used without global state.
    #[test]
    fn mock_resolver_no_global_state() {
        struct MockResolver;
        impl NameResolver for MockResolver {
            fn resolve(&self, _: &McIds, _: &McURI) -> Option<(McCMIE, McURI)> {
                None
            }
            fn resolve_system(&self, _: &McIds) -> Option<McCMIE> {
                None
            }
        }
        let ids = McIds::from("RES");
        assert!(MockResolver.resolve(&ids, &McURI::default()).is_none());
        assert!(MockResolver.resolve_system(&ids).is_none());
    }
}
