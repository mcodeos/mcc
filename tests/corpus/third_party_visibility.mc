// Test third-party library visibility (§15)
// Expected: third-party library symbols are not visible without `use`,
// producing E1304 / E1601 / E1401 / E2606

module main {
    // Try to use a third-party library symbol directly (no `use` written)
    // Should report E1304 (or equivalent unresolved-class diagnostic)
    TI_MCU::init()
}
