# 2. CLI Commands

> Corrected against the real `mcc --help` surface (25 words + `search` alias),
> U379 (2026-10-02). When this file and `mcc <word> --help` disagree, the
> binary wins.

### Global Flags

```
  -v, --verbose...                    Verbose: -v=info, -vv=debug, -vvv=trace
  -q, --quiet                         Quiet mode, reduce output
  -g, --origin                        Log lines include timestamp, module and file:line
  -c, --cwd <DIR>                     Change working directory before running
  -d, --debug <TARGET[=LEVEL]>        Enable debug output for a target (repeatable)
  -L, --local                         Run in this process; skip RPC delegation to a running `mcc start` server
  -l, --lib <NAME>                    Load a library before running (repeatable)
  -f, --format <FORMAT>               Output format: text | json | json-pretty | yaml | csv
  -o, --output <FILE>                 Write the command result to FILE instead of stdout
  -t, --top <NAME>                    Top-level module name (auto-guess first module if omitted)
  -e, --entry <FILE>                  Entry file for a directory target without a manifest
  -i, --ignore <CODES>                Suppress warning diagnostics by code, comma-separated (warning-only; errors are never suppressed)
      --strict                        Strict mode: strict-only diagnostics are reported as warnings
  -V, --version                       Print version
```

Global flags may appear before or after the subcommand:
`mcc --lib mcode -f json parse example.mc --top main` ≡ `mcc parse example.mc --top main --lib mcode -f json`

### Debug Targets (`-d` flag)

Runtime-controllable per-module debug output via `mcc_dbg!` macro (20 tracing targets):

| Alias    | Expands to                            |
| -------- | ------------------------------------- |
| `pass1`  | `mcc::parse::*`, `mcc::sem::*`        |
| `pass2`  | `mcc::inst::*`                        |
| `fcall`  | `mcc::sem::fcall`, `mcc::inst::fcall` |
| `lapper` | `mcc::sem::class`, `mcc::lsp::lapper` |
| `vec`    | `mcc::vec`                            |
| `viz`    | `mcc::viz`                            |
| `lsp`    | `mcc::lsp::*`                         |
| `all`    | `*` (everything)                      |

```bash
# Example: enable function-call resolution debug
mcc parse example.mc -d fcall=debug -vv

# Example: enable multiple targets at different levels
mcc parse example.mc -d pass1=trace -d inst::dump=debug
```

**Default logging is quiet.** Plain CLI runs (no `-v` / `-d`) emit warnings only
(`warn` level). The file-configured `trace.level` / `trace.targets` in
`~/.mcode/config/mcc.yaml` are loaded into runtime state for `trace.get` but are
**not applied to CLI runs** — otherwise a file-configured `level: debug` would
bury command results under INFO/DEBUG logs. File config takes effect only when
you explicitly pass `-v` / `-d`, or via RPC `trace.set` on a server.

### RPC Debug Control

```json
// Enable per-target debug at runtime (no rebuild needed)
{"method":"trace.set","params":{"name":"mcc::sem::fcall","level":"debug"}}
{"method":"trace.set","params":{"name":"pass1","level":"trace"}}
{"method":"trace.set","params":{"name":"mcc::inst::dump","level":"off"}}

// Query active targets
{"method":"trace.get"}
→ {"legacy": {...}, "targets": {"mcc::sem::fcall": "debug", ...}}
```

### Parse a file with an explicit top module

```bash
mcc parse example.mc --top main --viz
```

Note: `mcc <file> <top>` legacy shorthand is not supported; always pass the
`parse` subcommand.

***

### 2.1 `parse` — Parse & Analyze

```bash
# Parse a single file / project directory (auto-detects project.toml)
mcc parse path/to/file.mc
mcc parse ./my-project

# Parse a code snippet directly
mcc parse --code "RES(100Ω, 250V)" --lib mcode

# Parse-only, no instantiation
mcc parse example.mc --pass1

# Parse + instantiate, or all the way through visualization
mcc parse example.mc --pass2 --top main
mcc parse example.mc --top main --viz
mcc parse example.mc --all          # = --pass1 --pass2 --viz
```

Key flags (see also Global Flags above; `--lib`/`--top`/`-f`/`-o` are global):

| Flag                          | Purpose                                                          |
| ----------------------------- | ---------------------------------------------------------------- |
| `--code CODE`                 | Parse inline code (mutually exclusive with positional TARGET)    |
| `-l, --lib NAME`              | Load a library (global, repeatable)                              |
| `-t, --top NAME`              | Top-level module name (global)                                   |
| `--dlog`                      | Only output diagnostics as `file:line:col: level[code]: message` |
| `-i, --ignore CODES`          | Suppress warning-level diagnostics by code (global); errors are never suppressed |
| `--sort {pin-id\|interface}`  | Instance-tree pin sorting (default `pin-id`)                     |
| `--pass1`                     | Detailed Pass1 print (loaded files / definitions / ports)        |
| `--pass2`                     | Parse + instantiate, print module tree / connections / nets      |
| `--all`                       | Equivalent to `--pass1 --pass2 --viz`                            |
| `--viz`                       | Generate visualization HTML (default `<project-root>/build/circuit.html`) |
| `--viz-json`                  | Generate visualization JSON instead of HTML                      |
| `--ast`                       | Print AST                                                        |
| `--tree`                      | Print tree representation                                        |
| `--depth N`                   | Tree depth limit (`--tree`/`--ast` only, 0=unlimited)            |
| `-f FORMAT` / `-o FILE`       | Output format / file (global)                                    |

> `parse` always exits 0 — it reports diagnostics but is not an exit-code gate.
> For CI-style pass/fail use `check` (exit 1 on errors).

***

### 2.2 `check` — Validate

```bash
# Check a file and print diagnostics
mcc check path/to/file.mc

# Check entire project directory
mcc check ./my-project

# Errors only
mcc check example.mc --errors-only

# Strict mode (strict-only diagnostics reported as warnings)
mcc check example.mc --strict

# Include netlist checks (driver conflict, floating inputs, ...)
mcc check example.mc --nets

# Pin usage checks (unused pins, conflicting pin options)
mcc check example.mc --pins

# Append the resolve-gate failure ledger (summary; `--ledger=audit` adds deferred/ambiguous detail)
mcc check example.mc --ledger

# JSON output / only file:line:col lines
mcc check example.mc -f json-pretty
mcc check example.mc --dlog
```

Exit code 1 when errors are present — `check` (unlike `parse`) is the
exit-code gate.

***

### 2.3 `build` — Manifest-driven Build

```bash
# Build from project.toml in current directory (or with explicit entry)
mcc build
mcc build path/to/main.mc

# Build with library and top module
mcc build path/to/main.mc --lib mcode --top my_top_module

# With visualization
mcc build --viz

# JSON output / include system library in output
mcc build path/to/main.mc -f json -o output.json
mcc build --include-system
```

Extra flags:

| Flag                  | Purpose                                                                                                          |
| --------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `--layouter flow`     | Lock viz to one layouter (only `flow` exists today)                                                              |
| `--viz-frames`        | Draw scope dashed frames in `--viz` output (grouping annotation, default off)                                    |
| `--product IDS`       | Write file products into `<project-root>/build/`; ids are the `mcc export <KIND>` tokens (`netlist`,`bom`,`spice`,`kicad`,`kicad-sch`,`inst-list`). Omitted = envelope alone |

Exit code follows the same criterion as `check` (1 on diagnostics errors).

Uses `project.toml`:

```toml
[project]
name = "hbl"
version = "0.1.0"
entry = "src/hbl.mc"
top_module = "main"

[dependencies]
mcode = "*"
```

***

### 2.4 `list` / `show` — Inspect Definitions

- `mcc list <KIND>` — top-level definition **name lists**
- `mcc show <TARGET> [NAME]` — **detailed content** of one entity / an overview

```
mcc list <KIND> [OPTIONS]          # KIND: all | component | module | interface | enum | nets | ports | files | func | bus | clause
mcc show <TARGET> [NAME] [OPTIONS] # NAME required for detail/drill targets; file-based targets take the file via -F
```

#### `mcc list` — top-level lists (names only)

Text is the human-readable default; `-f json` prints the full structured
object shown in the table (kind-tagged rows, uris, etc.).

| command                                                | output (text)                                     | output (`-f json`)                                                                                                                                   |
| ------------------------------------------------------ | ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcc list all`                                         | `count: N` + one `kind: name` line per definition | flat aggregate, kind-tagged: `{type:"all", count, list:[{name, kind}]}`; same `--scope` default policy as `show all` (`-F` anchors the `file` layer) |
| `mcc list component` / `module` / `interface` / `enum` | `count: N` + one name per line                    | flat name list `{type, count, list}` — scripting-friendly                                                                                            |
| `mcc list func` / `bus` / `clause`                     | `count: N` + one name per line                    | same flat shape; `list clause` filters on the **host** name (a clause has no name of its own)                                                        |
| `mcc list nets`                                        | `count: N` + `name: point, point` per net         | all Pass2 nets of the top module (`--top` overrides; each entry includes its points)                                                                 |
| `mcc list ports`                                       | `count: N` + `name: iotype (module)` per port     | all module ports                                                                                                                                     |
| `mcc list files`                                       | one `uri: counts` line per file                   | every loaded file with per-file def counts                                                                                                           |

Options: `--filter EXPR` (structured filter, keys `name|kind|class`, `*`/`?`
wildcards; applies to all/component/module/interface/enum/func/bus),
`-F/--file`, `-l/--lib`, `-t/--top` (nets), `-f/-o`, `-L`, `-c`.

#### `mcc show` — detailed content

Targets (29):

| group    | targets |
| -------- | ------- |
| Overview / registry | `all` (layered overview, `--scope`), `defs` (whole def space in registry form: DefId + kind + declaring file) |
| Entity details | `component`, `module`, `interface`, `enum` |
| Pass2 circuit / power | `dianlu` (whole instantiated circuit; `--ids` annotates `NodeId`/`DefId`/physical points), `pwr` (power-intent facts tree), `pwrflow` (one-screen power flow; `--full` widens the rail-contract table, `--decaps` unfolds decoupler counts), `netlist` (flattening connectivity projection), `net` / `nets` (one net's points / module netlist) |
| Projections (readout, never a gate) | `project` (hierarchical module/instance tree), `core-erc` (extension-tool check-model snapshot), `expectation` (acceptance ledger verdicts), `diagnostics` (collected diagnostics of the loaded world), `sim` (sim model-profile registry joined to the built world), `org-units` (organization directory) |
| Debug | `lapper` (LSP intervals + RefDefMap), `ast` (parser AST tree), `stage <p1\|p2\|vec\|viz>` (pipeline segment; `--select`/`--exclude` slice stage viz by query-DSL predicate) |
| Drill-downs (NAME = owning entity) | `pins`, `ports`, `labels`, `instances` (`--type KIND`), `nets`, `attrs`, `funcs`, `params`, `roles`, `values` |

> `show nets` / `show params` accept `OWNER.FUNC` (dot-qualified func inside a
> module/component; dotted class names work too, e.g. `MCU.US513_20_F.i2c`).
> `show nets <func>` reports func-body connection-line nets named `line_N`
> (no Pass2 — funcs depend on parameters and a calling context).
> `show lapper` — see §6.6 for the full debug workflow.

#### Parameter matrix

| parameter                     | `mcc list`                          | `mcc show`    | effect                                                                                                                                |
| ----------------------------- | ----------------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `--scope S`                   | `all`                               | `all` / `defs` | definition layers: `file` (default) / `use` / `system` / `all`; `show defs` defaults to every loaded layer                            |
| `--filter EXPR`               | all/component/module/interface/enum/func/bus | —    | structured filter (`name=RES*`, `*` / `?` wildcards)                                                                                  |
| `-F, --file FILE`             | all                                 | all           | parse directly from a file instead of the loaded library/project; anchors the `show all` / `list all` file layer                      |
| `-t, --top NAME`              | nets                                | nets / dianlu / pwr / pwrflow / sim | Pass2 top module for instantiation (auto-guesses the first module in the file if omitted)                       |
| `--type KIND`                 | —                                   | instances     | filter sub-instances by kind (component\|module\|label\|interface\|bus\|busref\|list)                                                 |
| `--span`                      | —                                   | show all text | append `@start:end` source spans to `show all` file-layer details (hidden by default)                                                 |
| `--ids`                       | —                                   | dianlu        | annotate instances/pins with node `NodeId` + def `DefId` + physical point ids (`N<n>:<m>`)                                            |
| `--full` / `--decaps`         | —                                   | pwrflow       | widen the rail-contract table / unfold decoupler annotations                                                                          |
| `--select` / `--exclude EXPR` | —                                   | stage viz     | slice the drawing to nets the query-DSL predicate selects / subtracts (repeatable, union)                                             |
| `-l, --lib NAME` (repeatable) | all                                 | all           | load a library into scope (mcode, installed, or project)                                                                              |

> Target-specific parameters (`--scope`, `--filter`, `--top`, `--type`,
> `--span`, `--ids`, `--full`, `--decaps`, `--select`, `--exclude`) are silently
> ignored on targets they don't apply to. Orthogonal flags `-f`/`-o`
> (format/output), `-L` (local), `-c` (cwd), `-e` (entry) apply to every
> `list` / `show` target.

#### Common queries

```bash
# Lists
mcc list all -l mcode                       # every def, kind-tagged, flat
mcc list all -F example.mc                  # defs in the file (file layer, same default as show all)
mcc list component -l mcode --filter "name=RES*"
mcc list func -F example.mc
mcc list files
mcc list nets -F example.mc --top net1_simple_port

# Overview / registry
mcc show all -F example.mc                  # entities in the file (file layer)
mcc show all -F example.mc --scope system   # one layer only
mcc show defs -l mcode                      # whole def space, registry form

# Entity details / drill-down
mcc show component RES -l mcode
mcc show pins RES -l mcode
mcc show instances LP322DCDC --type component -F example.mc
mcc show nets LP322DCDC --top LP322DCDC -F example.mc
mcc show params US513.loadFlash -F example.mc   # nested func (OWNER.FUNC)

# Pass2 circuit / power
mcc show dianlu --top main -F example.mc --ids
mcc show pwrflow --top main -F example.mc --full --decaps

# Projections (JSON readouts)
mcc show project -f json
mcc show expectation -f json
mcc show core-erc -f json

# Debug
mcc show all -F example.mc --span                     # file layer with @start:end spans
mcc show ast -F example.mc
mcc show lapper -F example.mc -f json-pretty
```

Choosing a query: name list → `mcc list <kind>`; one entity → `mcc show <kind>
NAME`; internals → drill-down (`pins`, `instances`, ...); file contents →
`mcc show all -F FILE`; module netlist → `mcc show nets MODULE -F file.mc --top MODULE`; parser/semantic debug → `lapper` / `ast`.

***

### 2.4b Stage comparison — `join` / `trace` / `diff` / `show stage`

Cross-segment audit of the compile pipeline: `src → p2 (Pass2 circuit) →
vec → viz`. Semantics and acceptance language live in
`mcd/doc/pipeline/stage-readout-design.md` §5.3 (② join / ③ trace / ④ diff).

```bash
# Join two ADJACENT segments by key; reports every mismatch with its
# cardinality, in six classes:
#   carry (passed through) | expand | merge | drop | synth | skip
# drop = the segment lost it, synth = the segment invented it — the two
# suspect classes for lost/extra objects.
mcc join src p2  -F hbl.mc        # source -> Pass2 circuit
mcc join p2  vec -F hbl.mc        # Pass2 -> vec space
mcc join vec viz -F hbl.mc        # vec -> viz objects
mcc join src p2 -F hbl.mc --only drop   # one class only

# Follow ONE key along the whole chain and print what it is at each stage.
# The key's form is read off the key itself — four shapes:
mcc trace top.u1.vin -F hbl.mc        # instance canonical path
mcc trace "lib/power.mc::LDO" -F hbl.mc  # def canonical key
mcc trace dc.VDD_3V3 -F hbl.mc        # net name
mcc trace mcu.mc:23 -F hbl.mc         # source position
mcc trace N12:3 -F hbl.mc             # in-domain handle

# Stage readouts (the segments themselves):
mcc show stage p2 -F hbl.mc             # text
mcc show stage viz -F hbl.mc -f json    # structured, saveable

# Diff two readings of one view. Two modes:
mcc show stage viz -F hbl.mc -f json -o /tmp/viz_now.json
mcc diff hbl.mc /tmp/viz_now.json --view stage.viz
        # identity mode (default): NodeId-keyed alignment over a stage view;
        # operands = a source path read now + a saved reading
mcc diff ./projA ./projB --mode functional
        # functional mode: shared-expects alignment over two worlds;
        # both operands are source paths/projects, --view does not apply
# --view: stage.viz (default) | stage.p2 | stage.vec
```

AST faces (feeds of the whole chain):

```bash
mcc show ast -F hbl.mc -f json          # CST face: every node span:{start,end}
mcc parse hbl.mc --ast -f json          # phrase face: statement roots + stmt spans
```

`scripts/check-ast-roundtrip.py` gates both faces against the source
(gap = a non-trivia byte no node span covers; invented / overlap = leaf
evidence). rc 0 clean / 1 error / 2 findings:

```bash
python3 scripts/check-ast-roundtrip.py --mcc target-slots/b/debug/mcc \
    /path/to/project/src/*.mc
```

#### Worked example — full-chain audit of a project

The five-step recipe used to audit `mcs/hbl` end to end (`$MCC` is the
mcc binary, `$HBL` the project's top file, e.g.
`~/work/mo/mcs/hbl/src/hbl.mc`; a project directory works too):

```bash
MCC=mcc
HBL=~/work/mo/mcs/hbl/src/hbl.mc

# 1. Segment joins — audit each seam; drop/synth are the suspect classes
$MCC join src p2  -F $HBL          # source -> Pass2 circuit
$MCC join p2  vec -F $HBL          # Pass2 -> vec
$MCC join vec viz -F $HBL          # vec -> viz
$MCC join vec viz -F $HBL --only drop   # zoom into one class

# 2. Trace one object that looks wrong at some stage
$MCC trace top.u1.vin -F $HBL      # instance port / def key / net name / mcu.mc:23 / N12:3

# 3. Diff two readings of one view (before/after an edit)
$MCC show stage viz -F $HBL -f json -o /tmp/viz_now.json
$MCC diff $HBL /tmp/viz_now.json --view stage.viz
$MCC diff $HBL ~/other/worktree/src/hbl.mc --mode functional   # two worlds

# 4. AST <-> source round-trip gate (every non-comment byte accounted for)
python3 scripts/check-ast-roundtrip.py --mcc $MCC ~/work/mo/mcs/hbl/src/*.mc

# 5. Diagnostics floor — syntax/semantic errors and warnings for the target
$MCC check $HBL                    # positional TARGET; --local belongs to `build`
```

Reading order: 4 must be clean before anything downstream is trusted;
1's `drop`/`synth` rows name what to feed into 2; 3 closes the loop
after any fix; 5 is the standing diagnostics floor, not a comparison.

***

### 2.5 `search` & `query` — Find Definitions

```bash
# Text search (substring is the default matcher)
mcc search RES
mcc search RES --substring          # explicit substring

# Regex / fuzzy search
mcc search "CAP\..*" --regex
mcc search "amplifir" --fuzzy

# Filter by kind: component|module|interface|enum|instance|net|func|bus|clause
mcc search SPI --kind interface

# Limit results
mcc search RES --limit 10

# JSON output
mcc search RES --json
```

```bash
# Structured DSL query
mcc query "kind=component AND name=RES*"

# Query with filters
mcc query "kind=interface AND port_count>2" --json
```

> A query value that does not compile as a DSL expression falls back to a
> case-insensitive substring match on def names. `--kind` has no `def` value —
> def-like kinds are spelled out (`component|module|interface|enum|...`).

***

### 2.6 `export` — Generate Outputs

Kinds: `netlist | bom | spice | kicad | kicad-sch | inst-list`.

```bash
# Netlist
mcc export netlist example.mc --top main --lib mcode

# BOM (Bill of Materials)
mcc export bom example.mc --top main --lib mcode

# SPICE netlist
mcc export spice example.mc --top main --lib mcode

# KiCad NETLIST (the connectivity exchange format)
mcc export kicad example.mc --top main --lib mcode -o output.net

# KiCad SCHEMATIC (hierarchical sheets; --flat tiles everything on one sheet)
mcc export kicad-sch example.mc --top main --lib mcode -o output.kicad_sch
mcc export kicad-sch example.mc --flat

# Instance list
mcc export inst-list example.mc --top main

# Format options
mcc export netlist example.mc --top main -f json
mcc export bom example.mc --top main -f csv
```

> `kicad` and `kicad-sch` are different products: `kicad` is the KiCad
> **netlist**, `kicad-sch` is the drawing. `--flat` is `kicad-sch`-only.

***

### 2.7 `rules` — Check-Rule Registry Catalog

```bash
# List catalog rules; -f json emits the shared rules.list projection
mcc rules list
mcc rules list --scope flat-erc --severity warning

# One rule's full descriptor + its override audit (severity/allow/accept rows per layer)
mcc rules detail FLAT_R02

# Set a severity override (session-only; --write persists into project [config] diag.severities)
mcc rules set-severity FLAT_R02 error

# Allow (suppression) row / Accept (waiver) row (same --write persistence pattern)
mcc rules allow FLAT_R03 --path "src/power.mc"
mcc rules accept FLAT_R03
```

`rules list` filter flags: `--scope` (post-parse | assembly-gate | flat-erc |
declaration | viz-layout), `--domain`, `--severity` (hint|info|warning|error),
`--plane`, `--gate`, `--fix`, `--overridable`. Without a subcommand the
catalog summary prints.

***

### 2.8 `impact` & `import` — Change Radius / Read-Back

```bash
# Blast radius of changing one def (which tops, nets, consumers)
mcc impact LDO ./my-project
mcc impact main -f json             # versioned payload (schema_version impact.1.0)

# Read an EDA artifact back and report how it differs from the current world
mcc export netlist ./my-project -o /tmp/hbl.net
mcc import /tmp/hbl.net ./my-project --from netlist
mcc import /tmp/hbl.net ./my-project --from netlist -f json   # schema_version import.1.0
```

Exit codes: `impact` exits 1 when the sym names nothing, 2 when the world
cannot be built. `import` exits 2 on artifact/world construction failure,
1 when the artifact differs (`count > 0`) or the world is not
diagnostic-clean, 0 on agreement. Both answers are versioned payloads of
their own (`schema_version`), not the shared command envelope (design §7 D4).

***

### 2.9 `fmt` — Format `.mc` Sources

```bash
mcc fmt path/to/file.mc             # format in place (whitespace only)
mcc fmt ./src                       # whole directory; default target = cwd
mcc fmt --check ./src               # report files needing formatting; exit 1 if any differ
mcc fmt --rename ./src              # also rewrite style-gate names (E5070) to corrected spelling
```

> `--rename` is off by default and is not a whitespace pass: it rewrites the
> token sequence, and only touches names whose consumer set is provably
> complete (a name with invisible consumers is left as-is).

***

### 2.10 `lib` — Library Management

```bash
# List loaded libraries
mcc lib list

# Show library info
mcc lib show mcode

# Install a library from source
mcc lib install mcode --from /path/to/mcode

# Search available libraries
mcc lib search mcode

# Uninstall
mcc lib uninstall mylib

# Load/unload at runtime
mcc lib load mylib
mcc lib unload mylib
```

***

### 2.11 `start` / `stop` / `status` — RPC Server

```bash
# Start foreground server
mcc start --host 127.0.0.1 --port 8080 --lib mcode

# Start background daemon
mcc start -b --port 8080 --lib mcode

# Server log file
mcc start -b --log-file /tmp/mcc-server.log

# Check status
mcc status
mcc status --json
mcc status --watch                  # real-time monitoring

# Stop gracefully (wait up to --timeout seconds, default 10)
mcc stop

# Force stop
mcc stop --force
```

> There is no `--log-level` flag: log verbosity is the global `-v`/`-q` ladder
> (default `warn`, `-v` info, `-vv` debug, `-vvv` trace).

***

### 2.12 Other Commands

```bash
# Create a new project
mcc proj create my-project

# Explain an error code (omit the code to list all)
mcc explain 1100

# Go-to-definition (verify F12 jump target)
mcc def DC --lib mcode

# Find references (source spans by default; --circuit answers in the built board's rows)
mcc refs DC --lib mcode
mcc refs LDO --circuit --top main ./my-project

# Electrical rule check
mcc erc ./my-project --lib mcode
mcc erc ./my-project --top main --lib mcode

# Self-describing capabilities (AI discovery)
mcc caps

# Config management
mcc config list
mcc config get trace.parser
mcc config set trace.pass1 true     # runtime log-stream toggles (pass1/pass2/server), not stored in the config file
mcc config reset
```

> **Warning suppression.** Warning-only diagnostics can be suppressed per code,
> either on the CLI (`-i E3137`; alias `--ignore-warnings E3137`) or via the config key
> `diag.ignore_warnings` (project `project.toml`:
> `[config.diag] ignore_warnings = ["E3137"]`, or global `~/.mcode/config/mcc.yaml`).
> Errors are never suppressed. E3137 (`SINGLE_USE_INLINE_NET`) is the
> resolve-gate relax-everything single-use inline ghost-net warning — an undeclared
> structured base (`uC.ADC.P`) referenced exactly once; a shared/multi-use ghost
> net is left to the net layer (netcheck R03 flags a net holding both a supply
> and a ground).

***
