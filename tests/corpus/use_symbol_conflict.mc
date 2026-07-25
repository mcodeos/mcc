// Test E801: `use` symbol conflict detection
// Expected: emit E801 Error

module main {
    // Two `use` paths share the same final module name and their
    // exported symbols overlap.
    use $::lib1.power@1.0
    use $::lib2.power@1.0
}
