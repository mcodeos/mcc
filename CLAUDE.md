# mcc -- working conventions

> Authoring and commit rules live in `AGENTS.md` (12 authoring rules: English
> only, diagnostics must cite a real source location, exact name comparison,
> no guessing, never commit automatically, build only in a private slot).
> **Those rules take precedence over this file.**

## Look in the index before grepping the source

Before answering "where is X defined" or "who calls this", check
**`SRC-INDEX.md`** -- an index of 389 `.rs` files and 10,356 declarations, in
the form `path#Lline  kind name`, one self-contained entry per line:

```sh
grep -n 'fn resolve_gate' SRC-INDEX.md
grep -n 'impl .*McIds' SRC-INDEX.md
```

A hit gives you the path, the line, and the name at once. Read the line range
directly:

```
Read(file_path="src/semantic/basic/mc_ids.rs", offset=55, limit=80)
```

Only then fall back to `rg` over the source tree.

## Do not read large files whole

The read-guard hook **refuses** to read a file larger than 40KB without an
`offset`/`limit`. mcc has **18 `.rs` files over 100KB**:

| File | Size | Declarations |
|---|---|---|
| `src/db/infra/mc_code.rs` | 338 KB | 94 |
| `src/viz/layout/equipotential_tree.rs` | 332 KB | 173 |
| `src/semantic/basic/mc_phrase.rs` | 296 KB | 61 |
| `src/semantic/component/mc_pins/mod.rs` | 215 KB | 101 |
| `src/rules.rs` | 172 KB | 83 |

Reading `mc_code.rs` whole costs roughly 100k tokens, and it is re-sent on
**every turn** of the session. Grep the index first, then read a line range.
The full list is in the "High risk" section of `SRC-INDEX.md`.

## Hand broad searches to a subagent

"Find every call site of X" or "which files assign this field" belongs in an
Explore subagent: what it reads **never enters the main context**, only the
conclusion comes back.

## Generated artifacts (do not hand-edit)

`SRC-INDEX.md`: `python3 scripts/src-index.py --write SRC-INDEX.md`. Re-run it
after changing source, or the line numbers drift.

**It is an approximate index, not a compiler**: the regex does not recognise
macro-generated declarations or declarations split across lines, and does not
look at `#[cfg]`. Use it to **locate**, then confirm with `rg` or `mcc show` --
consistent with the "no guessing" rule in `AGENTS.md`.

## Self-check scripts

Already in `scripts/`: `check-attr-keys.py`, `check-cjk.py`, `check-comments.py`,
`check-errcodes.py`, `check-name-case.py`, `check-paths.py`, `check.sh`. Before
changing a face they cover, check whether a gate already exists.
