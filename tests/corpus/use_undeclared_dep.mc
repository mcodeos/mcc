// Test E800: `use` of undeclared third-party library dependency
// Expected: emit E800 Warning

// A `use` statement must be top-level (outside any `module`).
use $::nonexistent.lib@1.0

module main {
    U1::init()
}
