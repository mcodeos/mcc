// Test: overlapping `use` exports across libraries report USE_SYMBOL_CONFLICT (2061)
// when the `use` statements sit at top level. This fixture writes them inside a
// module body, so it fails parse first (2082/2115/5459); the 2061 lock lives in
// tests/use_symbol_conflict.rs

module main {
    // Two `use` paths share the same final module name and their
    // exported symbols overlap.
    use $::lib1.power@1.0
    use $::lib2.power@1.0
}
