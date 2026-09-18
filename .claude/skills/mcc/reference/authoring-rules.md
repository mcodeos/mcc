# 0. Authoring Rules

- Never hardcode absolute paths that embed a developer's username (for example
  `/Users/<user>/work/mo/mcc`) in source code, tests, golden data, or docs.
- Use portable forms instead: `~` in docs and shell examples, `$HOME`-derived
  paths (`PathBuf::from(home)` in Rust tests), or paths relative to the project
  root (`env!("CARGO_MANIFEST_DIR")`).
- Applies to the whole project including test code and test data.

- **Never hardcode symbol-name lists to special-case language behavior.** Do not
  gate semantics on a hardcoded list of component/method names (`"Cap"`,
  `"Pullup"`, `"Pulldown"`, …) when the behavior can be derived from the
  language itself — syntax (`_` placeholders, Sets, the `=>` prefix), the
  actual arguments, or structural shape. Detection is argument-based, never
  name-based. If a rule is specified to apply uniformly (e.g. the `=>`
  prefix-fill rule applies to ALL methods), implement it uniformly; a hardcoded
  name gate is a smell that the rule was mis-specified — re-derive the rule
  from the language rather than extending the list. (Example: P2-5 lane
  expansion decided by `fc_params_reference_bus_in_set`, not by "is this
  method called Pullup".)
- **Never infer anything from the shape of a name.** The bullet above covers
  name *lists*; this one covers name *glyphs* generally — a prefix / suffix /
  substring / case test, a digit-letter pattern (`contains('V')` plus a digit
  count), a length threshold, or a name-shaped fallback reached only when the
  real evidence is absent. A name is a label its author chose and promises
  nothing about what the thing is. Decide on a decoded value (a voltage read
  through the value engine, never a fragment of the text that wrote it), on the
  AST, on a type / role / contract, or on a declared name compared by exact
  string equality — declaring a name is what makes it identity, reading one is
  not evidence. Full statement and the worked examples:
  `AGENTS.md`, "Rule: no guessing from names".
- **Never run the full test suite by default.** Run only the test targets the
  change can actually affect (`cargo test --test <target> --test <target> …` in
  `mcc` / `mcs`). A whole-workspace `cargo test` (or `cargo nextest run` with no
  filter) is run **only when the user explicitly asks for it**. Compiling the
  `mcc` test targets plus `mcs`'s suites dominates the wall-clock, and an
  unintended full run also drags in the known-red `repro_*` family and its
  poisoned-lock cascade, drowning the signal from the suites that matter.
  `mcc`'s integration tests are grouped into eight targets, `tests/shard0` …
  `tests/shard7`; a former `tests/<file>.rs` is now the module `<file>` inside
  one of them, so reach it by name — `cargo test --test shard3 rail_rules` —
  and read `tests/shard*/main.rs` to find which shard holds a given file.
  Pick the targets from the code you touched; if you are unsure which those are,
  say so and run the nearest family rather than everything.
- **Parsing derives structure from the AST, never from string re-parsing.**
  In parsing, rely on the AST as ground truth — do not do your own string
  searching. Never re-derive language structure by rendering an `McPhrase` /
  `AstNode` back to display text (`format!("{}", phrase)`) and then splitting,
  regex-matching, or substring-searching that text for `->`, `,`, `.`, `]`,
  etc. to recover what the typed tree already holds. Walk the typed structure
  instead (`McPhrase::Member`, `FuncCall.caller`, `McInstanceRef`,
  `find_inst`, `find_pin`, `find_port`, …) and anchor diagnostics at the real
  source node (`node.span`), not at coordinates guessed from re-parsed text.
  If a decision needs `str::contains` / `.split()` on a phrase's display
  string, the shape you are looking for is (or should be) a variant already
  present in the AST — extend the parser rather than re-parsing its output.
- **Keep code comments lean.** A comment earns its place only by carrying
  something the code cannot: a non-obvious *why*, an external constraint, a
  trap. Explain why, not what (`// increment i` is noise) — if deleting it
  loses no reason, no constraint, and no warning, delete it. No change diary
  (`// previously…`, `// renamed from…`, `// an early version used to…`) and no
  commented-out code: git holds the history. No `// ───── Parse ─────` banner
  rules. Keep `///` doc comments to the contract (arguments, invariants,
  panics, units); multi-paragraph rationale goes to a design doc with at most a
  one-line pointer here — cited by bare document name, never by path. Match the
  file's local density — appending a verbose block to a terse file is a
  regression even when every sentence is true. Full rule: `AGENTS.md` §"keep
  code comments lean".
- Applies to the whole project including test code and test data.

***
