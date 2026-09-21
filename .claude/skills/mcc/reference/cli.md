# 2. CLI Commands

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

# Output as JSON / show AST / limit tree depth
mcc parse example.mc -f json-pretty -o result.json
mcc parse example.mc --ast
mcc parse example.mc --top main --depth 3
```

Key flags (see also Global Flags above; `--lib`/`--top`/`-f`/`-o` are global):

| Flag                        | Purpose                                                          |
| --------------------------- | ---------------------------------------------------------------- |
| `--code CODE`               | Parse inline code                                                |
| `-l, --lib NAME`            | Load a library (global, repeatable)                              |
| `-t, --top NAME`            | Top-level module name (global)                                   |
| `--dlog`                    | Only output diagnostics as `file:line:col: level[code]: message` |
| `-i, --ignore CODES`        | Suppress warning-level diagnostics by code (global, e.g. `E3137[,E…]`); errors are never suppressed. Alias: `--ignore-warnings` |
| `--sort {pinid\|interface}` | Pin sorting mode                                                 |
| `--pass1`                   | Parse only (no instantiation)                                    |
| `--pass2`                   | Parse + instantiate                                              |
| `--viz`                     | Generate HTML visualization                                      |
| `--viz-json`                | Generate JSON visualization data                                 |
| `--ast`                     | Print AST                                                        |
| `--tree`                    | Print tree representation                                        |
| `--depth N`                 | Tree depth limit (0=unlimited)                                   |
| `-f FORMAT`                 | Output format (global)                                           |
| `-o FILE`                   | Output file (global)                                             |

***

### 2.2 `check` — Validate

```bash
# Check a file and print diagnostics
mcc check path/to/file.mc

# Check entire project directory
mcc check ./my-project

# Errors only
mcc check example.mc --errors-only

# Strict mode (warnings become errors)
mcc check example.mc --strict

# Include netlist checks
mcc check example.mc --nets

# JSON output
mcc check example.mc -f json-pretty

# Only diagnostics as file:line:col lines
mcc check example.mc --dlog
```

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

The old `show` command was split into two:

- `mcc list <KIND>` — top-level definition **name lists**
- `mcc show <TARGET> [NAME]` — **detailed content** of one entity / an overview

```
mcc list <KIND> [OPTIONS]          # KIND: all | component | module | interface | enum | nets | ports | files
mcc show <TARGET> [NAME] [OPTIONS] # NAME required except for `all`
```

#### `mcc list` — top-level lists (names only)

Text is the human-readable default; `-f json` prints the full structured
object shown in the table (kind-tagged rows, uris, etc.).

| command                                                | output (text)                                     | output (`-f json`)                                                                                                                                   |
| ------------------------------------------------------ | ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcc list all`                                         | `count: N` + one `kind: name` line per definition | flat aggregate, kind-tagged: `{type:"all", count, list:[{name, kind}]}`; same `--scope` default policy as `show all` (`-F` anchors the `file` layer) |
| `mcc list component` / `module` / `interface` / `enum` | `count: N` + one name per line                    | flat name list `{type, count, list}` — scripting-friendly                                                                                            |
| `mcc list nets`                                        | `count: N` + `name: point, point` per net         | all Pass2 nets of the top module (`--top` overrides; each entry includes its points)                                                                 |
| `mcc list ports`                                       | `count: N` + `name: iotype (module)` per port     | all module ports                                                                                                                                     |
| `mcc list files`                                       | one `uri: counts` line per file                   | every loaded file with per-file def counts                                                                                                           |

Options: `--filter EXPR` (component/module/interface/enum), `-F/--file`,
`-l/--lib`, `-t/--top` (nets), `-f/-o`, `-L`, `-c`.

#### `mcc show` — detailed content

**Overview:**

| command                              | output                                                                                                                                                                                                                             |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `mcc show all [-F FILE] [--scope S]` | layered overview (file/use/system); `-F` anchors the `file` layer (default) and renders each entity in that file as a compact `.mc`-style detail block (pins/attrs/funcs/instances/...) — the former `show file` / whole-file dump |

**Entity details:**

| command                   | output                              |
| ------------------------- | ----------------------------------- |
| `mcc show component NAME` | pins table (id/io/names/interfaces) |
| `mcc show module NAME`    | module summary + sub-instances      |
| `mcc show interface NAME` | pin\_count, roles, params           |
| `mcc show enum NAME`      | values                              |

**Drill-downs** (NAME = owning entity):

| command                   | output                                                                           |
| ------------------------- | -------------------------------------------------------------------------------- |
| `mcc show pins NAME`      | pins of a component / interface                                                  |
| `mcc show ports NAME`     | ports (in/out/io) of a module                                                    |
| `mcc show labels NAME`    | labels of a module                                                               |
| `mcc show instances NAME` | sub-instances of a component / module; `--type KIND` filters kind                |
| `mcc show nets NAME`      | Pass2 netlist of module `NAME` (or `OWNER.FUNC` → func-body line nets, no Pass2) |
| `mcc show net NAME`       | points of one Pass2 net                                                          |
| `mcc show attrs NAME`     | attributes of a component / interface                                            |
| `mcc show funcs NAME`     | functions of a component / module                                                |
| `mcc show params NAME`    | parameter declarations of a component / module / interface / func                |
| `mcc show roles NAME`     | roles of an interface                                                            |
| `mcc show values NAME`    | values of an enum                                                                |

**Debug output** (raw parser / semantic data):

| command                   | output                                                   |
| ------------------------- | -------------------------------------------------------- |
| `mcc show lapper -F FILE` | LSP symbol intervals + RefDefMap (goto-def debug, local) |
| `mcc show ast -F FILE`    | AST tree (parser debug); `-f json` adds per-node source spans (see §2.4b) |

**Pass2 circuit tree** (`--top`):

| command                | output                                                                                              |
| ---------------------- | --------------------------------------------------------------------------------------------------- |
| `mcc show dianlu`      | whole instantiated circuit, one section per module: same-level instances (`[C]` component, `[M]` sub-module, `[L]` label, `[B]` bus) then per-connection lines; sub-modules recurse into nested sections; component interface buses are annotated with their interface class (e.g. `uC.UART0{TX, RX} :: UART.TTL(DCE)`) |

> `show nets` / `show params` accept `OWNER.FUNC` (dot-qualified func inside a
> module/component; dotted class names work too, e.g. `MCU.US513_20_F.i2c`).
> `show nets <func>` reports func-body connection-line nets named `line_N`
> (no Pass2 — funcs depend on parameters and a calling context).
> `show lapper` — see §6.6 for the full debug workflow.
> `show sem` — RPC-based equivalent of lapper:
> `curl -s -X POST http://localhost:8080/rpc -H "Content-Type: application/json" -d '{"jsonrpc":"2.0","method":"sem","params":{"uri":"<path>"},"id":1}'`

#### Parameter matrix

| parameter                     | `mcc list`                          | `mcc show`    | effect                                                                                                                                |
| ----------------------------- | ----------------------------------- | ------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `--scope S`                   | `all`                               | `all`         | definition layers: `file` (default) / `use` / `system` / `all`; `show all` text renders one `------ <layer> ------` section per layer |
| `--filter EXPR`               | all/component/module/interface/enum | —             | name filter on the list (`name=RES*`, `*` / `?` wildcards)                                                                            |
| `-F, --file FILE`             | all                                 | all           | parse directly from a file instead of the loaded library/project; anchors the `show all` / `list all` file layer                      |
| `-t, --top NAME`              | nets                                | nets / dianlu | Pass2 top module for instantiation (auto-guesses the first module in the file if omitted)                                             |
| `--type KIND`                 | —                                   | instances     | filter sub-instances by kind (component\|module\|label\|interface\|bus\|busref\|list)                                                 |
| `--span`                      | —                                   | show all text | append `@start:end` source spans to `show all` file-layer details (hidden by default)                                                 |
| `-l, --lib NAME` (repeatable) | all                                 | all           | load a library into scope (mcode, installed, or project)                                                                              |

> Target-specific parameters (`--scope`, `--filter`, `--top`, `--type`,
> `--span`) are silently ignored on targets they don't apply to. Orthogonal
> flags `-f`/`-o` (format/output), `-L` (local), `-c` (cwd), `-e` (entry) apply
> to every `list` / `show` target.

#### Common queries

```bash
# Lists
mcc list all -l mcode                       # every def, kind-tagged, flat
mcc list all -F example.mc                  # defs in the file (file layer, same default as show all)
mcc list all -F example.mc --scope system   # defs in system libraries only
mcc list component -l mcode
mcc list component -l mcode --filter "name=RES*"
mcc list interface -l mcode
mcc list files
mcc list nets -F example.mc --top net1_simple_port

# Overview / file scope (by origin layer, not kind)
mcc show all -F example.mc                  # entities in the file (file layer)
mcc show all -F example.mc --scope all      # system/use/file sections
mcc show all -F example.mc --scope system   # one layer only

# Entity details
mcc show component RES -l mcode
mcc show enum CAP -l mcode
mcc show module LP322DCDC -F example.mc

# Drill-down
mcc show pins RES -l mcode
mcc show ports LP322DCDC -F example.mc
mcc show labels LP322DCDC -F example.mc
mcc show instances LP322DCDC -F example.mc
mcc show instances LP322DCDC --type component -F example.mc
mcc show nets LP322DCDC --top LP322DCDC -F example.mc
mcc show net left -F example.mc             # points of one net
mcc show attrs RES -l mcode
mcc show funcs CAP -l mcode
mcc show params CAP -l mcode
mcc show roles SPI -l mcode
mcc show values CAP -l mcode

# Nested funcs (OWNER.FUNC)
mcc show params US513.loadFlash -F example.mc        # func parameters
mcc show nets US513.loadFlash -F example.mc          # func body line nets
mcc show funcs US513 -F example.mc                   # list funcs of an entity

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
# Key forms are read off the key itself (four shapes):
mcc trace top.u1.vin -F hbl.mc    # instance port canonical path
mcc trace dc.VDD_3V3 -F hbl.mc    # net name
mcc trace mcu.mc:23 -F hbl.mc     # source position
mcc trace N12:3 -F hbl.mc         # in-domain handle

# Stage readouts (the segments themselves):
mcc show stage p2 -F hbl.mc             # text
mcc show stage viz -F hbl.mc -f json    # structured, saveable

# Diff two readings of one view (a source path read now, or a saved
# `show stage ... -o FILE` reading):
mcc show stage viz -F hbl.mc -f json -o /tmp/viz_now.json
mcc diff hbl.mc /tmp/viz_now.json --view stage.viz
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
$MCC trace top.u1.vin -F $HBL      # instance port / net name / mcu.mc:23 / N12:3

# 3. Diff two readings of one view (before/after an edit)
$MCC show stage viz -F $HBL -f json -o /tmp/viz_now.json
$MCC diff $HBL /tmp/viz_now.json --view stage.viz

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
# Text search
mcc search RES

# Regex search
mcc search "CAP\..*" --regex

# Fuzzy search
mcc search "amplifir" --fuzzy

# Filter by kind
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

***

### 2.6 `export` — Generate Outputs

```bash
# Netlist
mcc export netlist example.mc --top main --lib mcode

# BOM (Bill of Materials)
mcc export bom example.mc --top main --lib mcode

# SPICE netlist
mcc export spice example.mc --top main --lib mcode

# KiCad schematic
mcc export kicad example.mc --top main --lib mcode -o output.kicad_sch

# Format options
mcc export netlist example.mc --top main -f json
mcc export bom example.mc --top main -f csv
```

***

### 2.7 `extract` — Extract Entities

```bash
# All instances / nets / components / interfaces
mcc extract instances example.mc --top main --lib mcode
mcc extract nets example.mc --top main --lib mcode
mcc extract components example.mc --lib mcode
mcc extract interfaces example.mc --lib mcode

# Filter by name pattern
mcc extract instances example.mc --name "C*" --lib mcode
```

***

### 2.8 `lib` — Library Management

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

### 2.9 `start` / `stop` / `status` — RPC Server

```bash
# Start foreground server
mcc start --host 127.0.0.1 --port 8080 --lib mcode

# Start background daemon
mcc start -b --port 8080 --lib mcode

# With logging
mcc start --log-level debug --log-file /tmp/mcc-server.log

# Check status
mcc status
mcc status --json

# Stop gracefully
mcc stop

# Force stop
mcc stop --force
```

***

### 2.10 Other Commands

```bash
# Create a new project
mcc proj create my-project

# Explain an error code
mcc explain 1100

# Go-to-definition (verify F12 jump target)
mcc def DC --lib mcode
mcc def CAP --lib mcode
mcc def RES --lib mcode

# Find references (verify reference lookup)
mcc refs DC --lib mcode
mcc refs CAP --lib mcode

# Electrical rule check
mcc erc ./my-project --lib mcode
mcc erc ./my-project --top main --lib mcode

# Convert .mc to JSON/YAML
mcc convert example.mc --to json -o example.json
mcc convert example.mc --to yaml -o example.yaml

# Generate design report
mcc report ./my-project

# Self-describing capabilities (AI discovery)
mcc caps

# Config management
mcc config list
mcc config get trace.parser
mcc config set trace.pass1 true
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
