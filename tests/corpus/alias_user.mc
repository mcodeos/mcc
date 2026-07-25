// Test: `as` alias renames the target module's spacename.
// The alias `tgt` should be registered in spacenames.
// The original name `target` should NOT be directly accessible; only `tgt` should work.

use ./alias_target.mc as tgt

module main {
}
