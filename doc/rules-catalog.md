# Rule catalog

The open index of every check rule and diagnostic code mcc emits — and the place
where their **conclusions** live. This file is deliberately a pointer, not a copy:
the two registries below are the single sources, and the CLI projects them.

## 1. Enumerate the table

Nothing here has to be read from source:

```bash
mcc rules           # every registered check rule, in execution order
mcc rules <code>    # one rule: descriptor, gate, acceptance, allow syntax
mcc explain         # every diagnostic code, name + description
mcc explain <code>  # one code, deepened with its rule descriptor
```

## 2. Sources of truth

| What | File |
|---|---|
| Code ↔ const ↔ description ↔ message template | `src/db/diagnostic/errcodes.rs` |
| Check-rule registration: execution order, descriptors, gates | `src/rules.rs` |

Every code is declared in both (a `pub const` plus an `ALL_CODES` row); every
check rule is declared once in `src/rules.rs` and driven from that table, so a
rule's identity lives in one place. `mcc rules` / `mcc explain` are projections
of those two tables, not a second copy.

## 3. Code ranges

Thousands + hundreds = pipeline stage / semantic cluster.

| Range | Stage | Meaning | Declared |
|---|---|---|---|
| 1xxx | Pass1a | type collection / definition structure | |
| 2xxx | Pass1b | use statements / parser / name resolution | |
| 3xxx | Pass1c | component / module / params / instances | |
| 4xxx | Pass2 | connection / netlist / interface binding | 98 |
| 5xxx | Pass3 | validation checks | |
| 6xxx | ERC | electrical rule checks | 28 |
| 9xxx | — | reserved | |

## 4. Where the checks are implemented

| Area | Module |
|---|---|
| Flat net electrical truth (4xxx) | `src/instant/netcheck.rs` |
| ERC rule bodies (6xxx) | `src/semantic/validation/nets/` |
| Net-island attribution index (copper / role / resolvable per flat net) | `src/instant/island.rs` |
| Nominal reach — 6011 (mandatory nominal) / 6019 (no-source kernel) | `src/semantic/validation/nets/reach.rs` |
| Supply budget — 6021 | `src/semantic/validation/nets/budget.rs` |
| Rail windows — 6023/6024/6025 | `src/semantic/validation/nets/window.rs` |
| Unified failure ledger (misses that resolve silently) | `src/semantic/validation/ledger.rs` |

## 5. Regression guards

| Guard | File |
|---|---|
| Netcheck rule case set — every R-series rule has a known true positive and a known false positive | `doc/netcheck-cases.md` |
| Rule-registry / ordering locks | `src/rules.rs` (unit tests in-module) |
| Per-rule acceptance surfaces | `tests/` |

## 6. Where the reasoning lives

The design drafts behind these rules — rulings, rejected alternatives, revision
history — are **process**, not product, and are not part of this repository.

Two consequences, both enforced by convention:

1. **Citations are by bare document name, never by path.** Code and tests write
   `island-attribution-design.md §2/§3`, not a repository path to it. A path
   would be a dead link for every outside reader and would rot on every
   reorganization of the doc tree.
2. **The conclusion must be self-sufficient.** Whatever an outside reader needs
   to judge a check is required to be readable in the code itself: a code
   (`6022`), an error code (`E4183`), or 1–3 lines of inline rationale. "See the
   design doc" is never the only justification.

See `AGENTS.md` for the same rule applied to new code.
