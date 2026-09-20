// Test: `use` of an undeclared third-party library dependency
// Expected codes: USE_LIB_NOT_FOUND (2052) plus follow-on unresolved-class
// diagnostics (2003, 3157, 2006, 5256); see golden/use_undeclared_dep.expected.json

// A `use` statement must be top-level (outside any `module`).
use $::nonexistent.lib@1.0

module main {
    U1::init()
}
