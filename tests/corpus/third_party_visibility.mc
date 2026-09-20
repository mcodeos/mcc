// Test third-party library visibility
// Expected: symbols from a non-`use`-d library simply fail to resolve
// (INST_CLASS_UNRESOLVED 3157, INST_CLASS_NOT_LOADED 5256); the system library
// is not pre-scanned into the workspace (see 19-decisions.md section 13)

module main {
    // Try to use a third-party library symbol directly (no `use` written)
    // Should report E1304 (or equivalent unresolved-class diagnostic)
    TI_MCU::init()
}
