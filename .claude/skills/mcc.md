# mcc Compiler

> CLI reference, RPC protocol, and debugging workflows for the mcc compiler and mcode projects.

***

## 0. Authoring Rules

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
- Applies to the whole project including test code and test data.

***

## 1. Quick Reference

### Build & Run

```bash
cd ~/work/mo/mcc
cargo build
```

### Key Paths

| Path                          | Purpose                                             |
| ----------------------------- | --------------------------------------------------- |
| `~/work/mo/mcc`               | Compiler source                                     |
| `~/work/mo/mcode`             | Standard library (components, interfaces, packages) |
| `~/work/mo/mcd`               | Workspace: test projects, libraries, docs           |
| `~/work/mo/mcext`             | VS Code extension + LSP server (`mcodels`)          |
| `~/.mcode/`                   | Runtime data: config, logs, PID file                |
| `~/.mcode/config/mcc.yaml`    | Global compiler config                              |
| `~/.mcode/config/server.yaml` | RPC server config                                   |
| `~/.mcode/logs/mcc.pid`       | Server PID file                                     |

### Environment Variables

| Variable             | Purpose                                      |
| -------------------- | -------------------------------------------- |
| `MCC_SYSTEM_ROOT`    | Override data directory (default `~/.mcode`) |
| `RUST_LOG`           | Tracing filter (overrides `-v`/`-q`)         |
| `MCC_LOG_FILE`       | Redirect C-parser trace to file              |
| `MCC_GOLDEN_PROJECT` | Golden-test project root                     |
| `MCC_GOLDEN_ENTRY`   | Golden-test entry file                       |
| `MCC_GOLDEN_TOP`     | Golden-test top module                       |
| `UPDATE_GOLDEN`      | Write golden baseline instead of comparing   |
| `MC_VIZ_DUMP`        | Enable visualization debug dump              |

***

## 2. CLI Commands

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
| `--ignore-warnings CODES`   | Suppress warning-level diagnostics by code (global, e.g. `E3137[,E…]`); errors are never suppressed |
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

Uses `project.toml` / `manifest.toml` / `mcc.toml`:

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
| `mcc show ast -F FILE`    | AST tree (parser debug)                                  |

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
> either on the CLI (`--ignore-warnings E3137`) or via the config key
> `diag.ignore_warnings` (project `project.toml`:
> `[config.diag] ignore_warnings = ["E3137"]`, or global `~/.mcode/config/mcc.yaml`).
> Errors are never suppressed. E3137 (`SINGLE_USE_INLINE_NET`) is the
> resolve-gate relax-everything single-use inline ghost-net warning — an undeclared
> structured base (`uC.ADC.P`) referenced exactly once; a shared/multi-use ghost
> net is left to the net layer (netcheck R03 flags a net holding both a supply
> and a ground).

***

## 3. RPC Protocol

### Overview

JSON-RPC 2.0 over HTTP. Server listens on `127.0.0.1:{port}` (default 8080).

| Endpoint  | Method | Purpose                           |
| --------- | ------ | --------------------------------- |
| `/rpc`    | POST   | Main JSON-RPC handler             |
| `/health` | POST   | Health check → `{"status": "ok"}` |

Request format:

```json
{"jsonrpc": "2.0", "method": "server.info", "params": {}, "id": 1}
```

Response format:

```json
{"jsonrpc": "2.0", "result": {...}, "id": 1}
```

Error format:

```json
{"jsonrpc": "2.0", "error": {"code": -32601, "message": "Method not found"}, "id": 1}
```

### Client Usage (curl)

```bash
# Health check
curl -s -X POST http://127.0.0.1:8080/health

# JSON-RPC: one canonical call — swap method/params as needed (full reference below)
curl -s -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"check","params":{"uri":"/path/to/file.mc","libs":["mcode"]}}'

# Representative params (method tables below are authoritative; server.methods/caps list live methods):
#   parse / check   {"uri":"/path/to/file.mc","libs":["mcode"]}   # content: for inline snippets
#   sem             {"uri":"/path/to/file.mc"}                     # LSP semantic tokens+symbols
#   diagnostics     {"uri":"/path/to/file.mc"}
#   def / refs      {"name":"RES"}                                 # go-to-def / references
#   build.full      {"entry":"/path/to/file.mc","top":"TOP","libs":["mcode"]}
#   show.*          {"name":"RES"}                                 # show.component/dump/all
#   lib.*           {}                                              # lib.list / info / load / unload
```

### RPC Methods Reference

#### Discovery

| Method           | Params | Returns                             |
| ---------------- | ------ | ----------------------------------- |
| `server.info`    | —      | Server version, uptime, loaded libs |
| `server.methods` | —      | List of all registered methods      |
| `caps`           | —      | Self-describing capabilities        |

#### Workspace

| Method             | Params | Returns                    |
| ------------------ | ------ | -------------------------- |
| `init`             | —      | Initialize workspace       |
| `load_project`     | `uri`  | Load project entry file    |
| `add_file`         | `uri`  | Add file to workspace      |
| `remove_file`      | `uri`  | Remove file from workspace |
| `set_project_root` | `path` | Set project root directory |
| `set_system_root`  | `path` | Set system library root    |

#### Parse / Build

| Method       | Params                         | Returns            |
| ------------ | ------------------------------ | ------------------ |
| `parse`      | `uri`, `code?`, `libs?`        | Parse result       |
| `check`      | `uri`                          | Diagnostics        |
| `build.full` | `uri`, `top?`                  | Full build result  |
| `extract`    | `kind`, `uri`, `top?`, `name?` | Extracted entities |

#### Show / Inspect

| Method                | Params          | Returns                     |
| --------------------- | --------------- | --------------------------- |
| `show.all`            | `file?`         | All entities (list only)    |
| `show.component`      | `name?`         | Component list or detail    |
| `show.component.list` | —               | Flat list of all components |
| `show.module`         | `name?`         | Module list or detail       |
| `show.module.list`    | —               | Flat list of all modules    |
| `show.interface`      | `name?`         | Interface list or detail    |
| `show.interface.list` | —               | Flat list of all interfaces |
| `show.enum`           | `name?`         | Enum list or detail         |
| `show.enum.list`      | —               | Flat list of all enums      |
| `show.net`            | `name?`         | Net list or detail          |
| `show.net.list`       | —               | Flat list of all nets       |
| `show.pins`           | `name`          | Pin definitions             |
| `show.ports`          | `name`          | Port definitions            |
| `show.ports.list`     | —               | Flat list of all ports      |
| `show.labels`         | `name`          | Labels of a module          |
| `show.instances`      | `name`, `file?` | Instance list with kinds    |
| `show.nets`           | `name`, `file?` | Net list                    |
| `show.attrs`          | `name`          | Attribute list              |
| `show.funcs`          | `name`          | Function list               |
| `show.params`         | `name`          | Parameter list              |
| `show.roles`          | `name`          | Role definitions            |
| `show.values`         | `name`          | Enum values                 |
| `show.dump`           | `name`          | Full entity dump            |
| `show.dump.all`       | —               | Dump all loaded entities    |
| `show.file`           | `uri`           | All definitions in file     |
| `show.files`          | —               | All loaded files            |

#### Semantics / LSP

| Method            | Params                                           | Returns                                                    |
| ----------------- | ------------------------------------------------ | ---------------------------------------------------------- |
| `sem`             | `uri`, `content?`                                | Semantic tokens + symbols                                  |
| `diagnostics`     | `uri`                                            | File diagnostics                                           |
| `project_symbols` | —                                                | Project-wide symbol index                                  |
| `def`             | `name`                                           | Go-to-definition by name                                   |
| `refs`            | `name`                                           | Find all references by name                                |
| `hover`           | `name`, `uri`                                    | Hover tooltip info                                         |
| `completion`      | `uri`, `line`, `column`                          | Code completions at position                               |
| `defs.search`     | `pattern`, `kind?`, `regex?`, `fuzzy?`, `limit?` | Text/regex/fuzzy search across definitions                 |
| `defs.query`      | `expr`, `limit?`                                 | Structured DSL query (e.g. `kind=component AND name=RES*`) |
| `lookup`          | `name`                                           | Lookup by name                                             |
| `lookup_sub`      | `parentUri`, `kind`, `name`                      | Scoped lookup                                              |
| `lookup_all`      | —                                                | All lookup entries                                         |
| `erc`             | `uri?`, `top?`                                   | Electrical rule check                                      |

#### Library

| Method          | Params  | Returns               |
| --------------- | ------- | --------------------- |
| `lib.list`      | —       | Loaded libraries      |
| `lib.info`      | `name`  | Library metadata      |
| `lib.load`      | `name`  | Load a library        |
| `lib.unload`    | `name`  | Unload a library      |
| `lib.install`   | `path`  | Install library       |
| `lib.uninstall` | `name`  | Uninstall library     |
| `lib.search`    | `query` | Search installed libs |

#### Export / Utility

| Method      | Params               | Returns                |
| ----------- | -------------------- | ---------------------- |
| `export`    | `kind`, `uri`, `top` | Export result          |
| `convert`   | `uri`, `format`      | Convert file           |
| `report`    | `uri?`               | Design report          |
| `explain`   | `code?`              | Error code description |
| `trace.set` | `config`             | Update trace config    |
| `trace.get` | —                    | Current trace config   |

### Error Codes

| Code   | Meaning                    |
| ------ | -------------------------- |
| -32700 | Parse error (invalid JSON) |
| -32600 | Invalid request            |
| -32601 | Method not found           |
| -32602 | Invalid params             |
| -32603 | Internal error             |
| 32100  | I/O or filesystem error    |
| 32101  | Workspace conflict         |
| 32102  | Workspace not found        |
| 32103  | Archive decode failed      |
| 32104  | Unsupported format         |
| 32105  | Entry file not found       |
| 32106  | Dependency not loaded      |
| 32107  | Pass1 or Pass2 failed      |
| 32108  | Build panic                |

***

## 4. Compiler Pipeline

```
Pass 0 — Manifest
  Read project.toml → load dependencies → resolve entry file

Pass 1 — Parse
  C lexer + yacc parser → AST → type resolution → cross-file references
  Output: definitions by URI + span

Pass 2 — Instantiate
  Top module → recursive instantiation → McProjectTree + InstTable
  Output: ports, components, submodules, connections, nets

Pass 3 — Vector
  build_mc_vec → McVecBlock → build_mc_vec_graph → McVecGraph
  D1-d8 detectors run here (codes 2001-2008)

Pass 4 — Layout + Render
  Layout algorithms → wire routing → SVG render → HTML template
```

```bash
# Run specific passes
mcc parse example.mc --pass1              # Pass 1 only
mcc parse example.mc --pass2 --top main   # Pass 1 + 2
mcc parse example.mc --viz --top main     # All passes (visualization)

# Trace pass execution (via RPC)
curl -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"trace.set","params":{"pass1":true,"pass2":true},"id":1}'
```

***

## 5. Debugging mcc Itself

### 5.1 VS Code Debug Configurations

In `~/work/mo/mcc/.vscode/launch.json`:

**"mcc"** — Debug a one-shot CLI run:

- Program: `target/debug/mcc`
- Args: `parse ../mcs/hbl/src/hbl.mc` (hbl project lives in `~/work/mo/mcs/hbl`)
- Env: `RUST_BACKTRACE=1`
- cwd: `${workspaceFolder}`

```bash
# Equivalent command line
cd ~/work/mo/mcc
RUST_BACKTRACE=1 cargo run -- parse ../mcs/hbl/src/hbl.mc
```

### 5.2 Logging

```bash
# Default (no flags): warnings only. File-configured trace.level/targets are
# NOT applied to CLI runs — debug output is explicit opt-in:
mcc show pins RES --lib mcode            # clean result, no engine logs
mcc -d sem:class show pins RES --lib mcode   # one module at debug level
mcc -d pass1=trace parse example.mc      # alias + file config, explicit

# Increasing verbosity (overrides the default warn level)
mcc parse example.mc -v            # info
mcc parse example.mc -vv           # debug
mcc parse example.mc -vvv          # trace (very verbose)

# Target-specific logging
RUST_LOG="mcc::pass1=trace,mcc::pass2=debug" mcc parse example.mc

# With origin (timestamp, module, file:line) shown
mcc parse example.mc -vvv -g

# C parser trace
MCC_LOG_FILE=/tmp/cparse.log mcc parse example.mc -vvv

# Visualization debug dump
MC_VIZ_DUMP=1 mcc parse example.mc --viz --top main
```

### 5.3 Server Debugging

> **⚠️ Stale server = stale results (bites repeatedly — read before debugging).**
> A running `mcc start` server holds the **library and all parsed definitions
> in memory from the moment it started**. It does NOT re-read `~/.mcode/mcode`
> or pick up a rebuilt `mcc` binary on its own.
>
> After you edit an mcode library file or `cargo build` mcc, a still-running
> server keeps serving the OLD library / OLD binary — so CLI commands that
> default to RPC delegation (no `-L`) report phantom diagnostics like
> `E4176 Too many arguments: expected 1, got 3` on a signature you already
> fixed. Symptoms: a change verifies fine with `--local` but "still broken"
> in the default/IDE path.
>
> First line of defense — verify locally, bypassing any server:
> ```bash
> mcc check file.mc -L        # -L / --local: run in this process, skip RPC
> ```
> If local is clean but the default path errors → a stale server is up. Then:
> ```bash
> mcc status                  # is a server running? (also: lsof -i :8080)
> mcc stop                    # graceful stop; kill $(cat ~/.mcode/logs/mcc.pid) if hung
> # restart fresh AFTER library edits / rebuilds so it loads new state:
> mcc start -b --port 8080 --lib mcode
> ```
> The IDE LSP (`mcodels`) spawns/attaches to this server; after a restart it
> reconnects on the next request (reload the VS Code window if old
> diagnostics linger).

```bash
# Background daemon with library preload (most common)
mcc start -b --port 8080 --lib mcode
mcc start -b --log-file /tmp/mcc.log --lib mcode    # with log file

# Foreground server with full tracing (debug mode)
mcc start --port 8080 -vv --lib mcode

# Check server health
curl -s -X POST http://127.0.0.1:8080/health

# Check if server is running
mcc status
mcc status --json

# View PID
cat ~/.mcode/logs/mcc.pid

# Kill orphaned server
kill $(cat ~/.mcode/logs/mcc.pid)

# Force stop if hung
mcc stop --force
```

### 5.4 Trace Configuration (runtime)

```bash
# Enable pass1 tracing via RPC
curl -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"trace.set","params":{"enabled":true,"pass1":true},"id":1}'

# Check current trace config
curl -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"trace.get","params":{},"id":1}'
```

### 5.5 Test Commands

```bash
# Run all tests
cd ~/work/mo/mcc
cargo test

# Run specific test
cargo test --lib cmds::build::tests

# Run golden tests
MCC_GOLDEN_PROJECT=/path/to/project cargo test golden

# Update golden baselines
UPDATE_GOLDEN=1 MCC_GOLDEN_PROJECT=/path/to/project cargo test golden

# Run with backtrace
RUST_BACKTRACE=full cargo test
```

***

## 6. Debugging mcode Projects

### 6.1 Project Structure

```
my-project/
├── project.toml          # Required: [project] + [dependencies]
├── src/
│   ├── main.mc           # Entry file (referenced in project.toml)
│   └── sub_module.mc     # Other .mc files
```

### 6.2 Common Workflows

```bash
# Create a new project
mcc proj create my-project

# Quick syntax/diagnostic check
mcc check ./my-project

# Parse and show structure
mcc parse ./my-project -f json-pretty

# Build and visualize
mcc build --viz
# Opens circuit.html in browser

# Show what's defined in a file
mcc show all -F src/main.mc

# Find a component definition
mcc show component RES --lib mcode

# Search for components matching a pattern
mcc search "CAP" --kind component

# Show instances (what's actually used)
mcc show instances TOP_MODULE --top TOP_MODULE -F src/main.mc

# Export netlist
mcc export netlist src/main.mc --top main --json
```

### 6.3 Diagnosing Errors

```bash
# Get all diagnostics for a file
mcc check path/to/file.mc

# With strict checking
mcc check path/to/file.mc --strict

# Explain a specific error code
mcc explain 1100

# Full diagnostics via RPC (with server running)
curl -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"diagnostics","params":{"uri":"file:///absolute/path/to/file.mc"},"id":1}'

# Get semantic tokens + symbols for a file
curl -X POST http://127.0.0.1:8080/rpc \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"sem","params":{"uri":"file:///absolute/path/to/file.mc"},"id":1}'
```

### 6.4 Common Error Codes

| Code               | Meaning                          | Typical Cause                                                 |
| ------------------ | -------------------------------- | ------------------------------------------------------------- |
| 1001-1005          | Duplicate definition             | Same name used twice in scope                                 |
| 1051-1060          | Definition structure / CMIE load | Missing subnodes, malformed IO type, define-as-CMIE           |
| 2001-2010          | `use` statement errors           | Bad path, target not found, self import, alias collision      |
| 2051 / 2061 / 2071 | Use-stage dependency errors      | Undeclared dep, symbol conflict, import not found             |
| 2080-2119          | Parser errors                    | Syntax error, invalid clause / pin / net / conds              |
| 2121-2127          | Name / declaration parse errors  | Missing subnode, failed name extraction                       |
| 2171-2172          | Unsupported / unresolved symbol  | P1-P5 lookup failed, not supported yet                        |
| 2901-2906          | Vector shape validation          | Shape mismatch, transpose limit, expand mismatch              |
| 3001-3008          | Pin/port definition              | Pin ID/name mismatch, count errors                            |
| 3021-3023          | Attribute errors                 | Type mismatch, unsupported type, missing subnode              |
| 3041-3049          | Unit value (UVAL) errors         | Invalid/unsupported unit, bad value format                    |
| 3051-3054          | Module body errors               | PINS unsupported, role unsupported, unexpected param          |
| 3071 / 3081        | Module method / clause           | Method not found, unexpected clause type                      |
| 3101-3111          | Params / functions               | Invalid param, class/instance expected                        |
| 3131-3135          | Function calls / lines           | Missing name, parse failure, dropped line                     |
| 3151-3180          | Instance / interface reference   | Class unresolved, member / pin / port not found               |
| 4001-4026          | Connection / shape               | Transpose mismatch, parallel/series invalid, dot misuse       |
| 4050-4058          | Netlist heuristics (D-series)    | Ghost port, merged short, sort hazard, floating `_`           |
| 4081-4098          | Layout attribute errors          | Missing subnode, type mismatch, malformed edge                |
| 4101-4118          | Netlist / interface binding      | Multi-drive, no driver, unconnected, backfeed risk            |
| 4150-4178          | Instantiation checks             | Chain link skipped, arg count mismatch, bind failed           |
| 5001-5003          | Cross-file duplicates            | Same name defined in another file                             |
| 5050-5099          | Naming / style                   | Lowercase component, single-char instance, shadows CMIE       |
| 5101-5104          | Reference integrity              | Undeclared spec key, function without body                    |
| 5151-5163          | Ports / pins                     | Duplicate port, unused pin/port, conflicting options          |
| 5201-5206          | Functions / roles / defaults     | Bad param default, enum single value                          |
| 5251-5267          | Definition structure (M-series)  | Empty body, no pins, duplicate spec key                       |
| 5301-5304          | `.int` class checks              | Ambiguous name, class not loaded, unconventional suffix       |
| 5351-5357          | Instance / attribute checks      | Reserved keyword, arg count, nesting too deep                 |
| 5401-5412          | Enum / expression checks         | Duplicate value, reversed range, `this` at top level          |
| 5451-5459          | Condition blocks                 | Empty body, if without else, NC at component level            |
| 5501-5510          | Hardware checks                  | Power pins excess, pin number gaps, NC contiguous             |
| 5551-5552          | Type / unit compatibility        | Free closure variable, incompatible types                     |
| 5641-5643          | Global diagnostics               | Unused param/port, untyped param                              |
| 6001-6004          | ERC                              | Single-point net, unconnected port, multi-drive, floating net |

The D1-d7 detector codes referenced by build.rs tests map as follows:

| Detector | Code | Constant                  |
| -------- | ---- | ------------------------- |
| D1       | 4053 | SORT\_HAZARD              |
| D2       | 4054 | FLOATING\_PLACEHOLDER     |
| D3       | 4051 | NET\_MERGED\_SHORT        |
| D4       | 4050 | GHOST\_PORT\_BOX          |
| D5       | 4052 | NET\_BUS\_ORDER\_MISMATCH |
| D6       | 4057 | NET\_DROPPED\_STATEMENT   |
| D7       | 4056 | PULLUP\_DEGENERATE        |

### 6.5 Validating Library Changes

When modifying mcode library files:

```bash
# 0. Sync the repo library into the runtime dir, then restart any stale server.
#    mcc reads ~/.mcode/mcode at RUNTIME, and a running `mcc start` server holds
#    its own in-memory copy — it does not pick up file edits. Skip this step and
#    you will chase phantom diagnostics that vanish under --local (§5.3).
bash ~/work/mo/mcode/cp.sh        # repo mcode -> ~/.mcode/mcode
mcc stop || true                  # kill any stale server (also: lsof -i :8080)
mcc check ./path/to/changed.mc -L # -L forces local: verify against on-disk state

# 1. Check modified file for syntax errors
mcc check ./path/to/changed.mc

# 2. Parse with library loaded
mcc parse ./path/to/changed.mc --lib mcode

# 3. Build a test project that uses the changed component
cd ~/work/mo/mcs/hbl
mcc build

# 4. Full rebuild with visualization
mcc build --viz

# 5. Run mcc's internal test suite
cd ~/work/mo/mcc
cargo test
```

After editing a library file, always re-sync (`cp.sh`) AND restart the server
before trusting a default `mcc check` / IDE diagnostic. A clean result from a
`-L` run proves the source is fine; a still-failing default run means the
server is serving stale state (§5.3).

### 6.6 Lapper / RefDefMap Debug Dump

```bash
# Local mode (no server needed) — F12_DIAG text format (default)
mcc show lapper path/to/file.mc

# Load library first
mcc show lapper --lib mcode path/to/file.mc

# JSON output
mcc show lapper path/to/file.mc -f json-pretty

# Save to file (suppress AST tree noise)
mcc show lapper path/to/file.mc 2>/dev/null > dump.txt
```

**Text output sections:**

| Section          | Content                                               |
| ---------------- | ----------------------------------------------------- |
| `LAPPER ENTRIES` | All symbol intervals: kind, id, span, name, file      |
| `DECLARES`       | name\_to\_declare\_id entries: id, span, scope, name  |
| `REFERENCES`     | inst\_id\_to\_span entries: id, span, declare\_id     |
| `DEF_MAP`        | (def\_kind, decl\_id) → SourceLocation                |
| `REF_ENTRIES`    | Pre-collected refs: (ref\_kind, decl\_id, span, name) |
| `REF_DEF_MAP`    | **Core**: Ref→Def resolution with kind\_names legend  |

**Quick analysis:**

```bash
# Count MAP entries by ref→def kind pair
grep "F12_DIAG MAP:" dump.txt | sed 's/.*Ref(//;s/).*=> Def(/ -> /;s/,.*//' | sort | uniq -c | sort -rn

# Check all ClassRef resolutions
grep "Ref(ClassRef" dump.txt

# Check all FuncParamRef resolutions
grep "Ref(FuncParamRef" dump.txt
```

**Common debugging workflow:**

```bash
# 1. Without library — check local def/ref mappings
mcc show lapper src/us513.mc 2>/dev/null | grep "^F12_DIAG MAP:"

# 2. With library — check cross-file library class resolution
mcc show lapper --lib mcode src/us513.mc 2>/dev/null | grep "^F12_DIAG MAP:"

# 3. Drill into specific ref type
mcc show lapper --lib mcode src/us513.mc 2>/dev/null | grep "Ref(ClassRef"

# 4. Compare two runs
diff <(mcc show lapper file.mc 2>/dev/null | grep MAP) \
     <(mcc show lapper --lib mcode file.mc 2>/dev/null | grep MAP)
```

**JSON analysis — F12 failure root cause (Lapper ID vs RefDefMap ID match):**

```bash
mcc show lapper us513.mc --lib mcode -f json | python3 -c "
import json,sys
d=json.load(sys.stdin)
l={e['id'] for e in d['lapper'] if e['kind']==1}          # ClassRef kind=1
r={e['ref_id'] for e in d['ref_def_map']['entries'] if e['ref_kind']==1}
print('MISMATCHED:', sorted(l - r))                       # non-empty = F12 broken
"
```

**Quick debug flow (F12 goto-def issues):**

```bash
mcc show interface DC --lib mcode && mcc show component CAP --lib mcode  # def exists?
mcc def DC --lib mcode && mcc def CAP --lib mcode                        # correct target?
mcc show lapper us513.mc --lib mcode -f json | python3 -m json.tool      # raw data
```

## 7. LSP Extension (mcext)

### Architecture

```
VS Code  ←LSP→  mcodels (Rust)  ←HTTP JSON-RPC→  mcc server
(extension)     (tower-lsp)                       (axum :8080)
```

### Debug Configurations

In `~/work/mo/mcext/.vscode/launch.json`:

| Config                           | Purpose                                       |
| -------------------------------- | --------------------------------------------- |
| **Debug LSP Server**             | Launch `mcodels` with `RUST_LOG=trace`        |
| **Debug VS Code Extension**      | Open new VS Code window with extension loaded |
| **Attach to LSP Server**         | Attach debugger to running mcodels process    |
| **Debug Extension + LSP Server** | Compound: launch both simultaneously          |

```bash
# Build extension
cd ~/work/mo/mcext
cargo build

# Run LSP server standalone (stdin/stdout)
RUST_LOG=trace cargo run --bin mcodels

# Start extension development host
# Use "Debug Extension + LSP Server" launch config, or:
code --extensionDevelopmentPath=~/work/mo/mcext ~/work/mo/mcs/hbl
```

### Key LSP Features

| Feature          | RPC Method Used              | Source Module         |
| ---------------- | ---------------------------- | --------------------- |
| Semantic tokens  | `sem`                        | `features/semtok.rs`  |
| Go-to-definition | `def`                        | `features/gotodef.rs` |
| Find references  | `refs`                       | `features/refs.rs`    |
| Completions      | `project_symbols` + `show.*` | `features/comp.rs`    |
| Hover            | `show.dump`                  | `features/hover.rs`   |
| Diagnostics      | `diagnostics`                | `features/diag.rs`    |
| Formatting       | (internal)                   | `features/fmt.rs`     |
| Inlay hints      | (internal)                   | `features/inhint.rs`  |

### Health Checks

```bash
# Check mcc server status
curl -X POST http://127.0.0.1:8080/health

# Check if mcodels is running
ps aux | grep mcodels

# View mcc server log
tail -f ~/.mcode/logs/mcc-server.log

# Check extension output in VS Code
# View → Output → "MCode" channel
```

***

## 8. Configuration Reference

### Global Config (`~/.mcode/config/mcc.yaml`)

```yaml
trace:
  # Base level: off | error | warn | info | debug | trace
  # Applied only when CLI runs with -v / -d (or via RPC trace.set).
  # Plain CLI commands (mcc show/parse/check ...) stay at the -v/-q-derived
  # default (warn) so results are not buried under INFO/DEBUG logs.
  level: warn
  # Per-module overrides (mcc::sem::fcall: debug). Enabled per-run with -d:
  #   mcc -d sem:class parse example.mc
  # targets:
  #   mcc::sem::fcall: debug
  enabled: null
  ast: null
  lexer: null
  parser: null
  visit: null

parser:
  strict: false       # treat warnings as errors in check
  max_depth: null     # 0/unset = unlimited tree depth

output:
  format: "text"       # text | json | yaml

libs:
  preload:
    - mcode
```

### Project Config (`project.toml`)

```toml
[project]
name = "my-project"
version = "0.1.0"
entry = "src/main.mc"
top_module = "main"

[dependencies]
mcode = "*"

[config.trace]
enabled = false
pass1 = false
pass2 = false
```

### Server Config (`~/.mcode/config/server.yaml`)

```yaml
server:
  host: "127.0.0.1"
  port: 8080
  tls: false
  auth: "none"     # none | basic | token
  max_connections: 100
  request_timeout_ms: 30000

logging:
  level: "info"    # debug | info | warn | error
  file: ""         # empty = stderr only
```

***

## 9. MCP Server (mcc-mcp)

`mcc-mcp` is an MCP (Model Context Protocol) server binary that exposes the
compiler to AI agents over stdio. Every tool delegates to the existing
JSON-RPC handlers / libmcc API, so there is no duplicated business logic.
The AI client discovers the tools automatically via `tools/list`; the tool
name, description, and JSON schema are self-describing.

- Binary: `target/debug/mcc-mcp` (source: `src/bin/mcc_mcp.rs`)
- Design doc: `mcd/doc/mcp/mcc-mcp-server-design.md`

### 9.1 Connection Configuration

The server speaks MCP over stdio and is launched as a subprocess by the AI
client. Bind one process to one project via `MCC_PROJECT_ROOT` (state model A).

Claude Desktop (`~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "mcc": {
      "command": "~/work/mo/mcc/target/debug/mcc-mcp",
      "env": {
        "MCC_PROJECT_ROOT": "~/work/mo/mcs/hbl"
      }
    }
  }
}
```

Claude Code:

```bash
claude mcp add mcc -- ~/work/mo/mcc/target/debug/mcc-mcp
# with a project binding:
claude mcp add mcc --env MCC_PROJECT_ROOT=~/work/mo/mcs/hbl \
  -- ~/work/mo/mcc/target/debug/mcc-mcp
```

Cursor / Trae: Settings → MCP → Add server with `command` + `env` (same shape).

Environment variables:

| Variable           | Purpose                                                                                      |
| ------------------ | -------------------------------------------------------------------------------------------- |
| `MCC_PROJECT_ROOT` | Project root this instance is bound to (optional; `mcc_load_project` also binds per call)    |
| `MCC_SYSTEM_ROOT`  | Override the system root that contains the `mcode` library (optional; auto-probed otherwise) |

Notes:

- The `mcode` system library is force-loaded at startup, so `mcc_search_defs` /
  `mcc_show_def` resolve mcode symbols without extra `libs` arguments.
- Tool failures come back as MCP errors (`INTERNAL_ERROR`) with the original
  RPC code in the `data.rpc_code` field.

### 9.2 Tools (13)

| Tool                     | Params (required first)                                                     | Purpose                                                                                                                                |
| ------------------------ | --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `mcc_validate_component` | `content`, `libs?`, `strict?`, `errors_only?`                               | Validate an inline MCode snippet; returns diagnostics (E2xxx/E3xxx). Primary AI loop: generate → validate → fix.                       |
| `mcc_parse_file`         | `file_path`, `include_system?`                                              | Parse a `.mc` file; returns AST summary and diagnostics.                                                                               |
| `mcc_explain_error`      | `code?`                                                                     | Explain an error code (e.g. 2008); omit `code` for the full error table.                                                               |
| `mcc_load_project`       | `entry`                                                                     | Load a project entry `.mc` file and its use-dependencies into the workspace; derives the project root by walking up to `project.toml`. |
| `mcc_check_file`         | `file_path`, `libs?`, `strict?`, `errors_only?`                             | Check a single `.mc` file; returns diagnostics.                                                                                        |
| `mcc_check_project`      | `entry?`, `strict?`, `errors_only?`                                         | Check the whole active project (load it first via `mcc_load_project`); `entry` is required when no project is loaded.                  |
| `mcc_build`              | `entry?`, `top?`, `include_system?`, `libs?`                                | Run Pass2 instantiation; returns module tree, connections, and nets.                                                                   |
| `mcc_search_defs`        | `pattern`, `kind?`, `regex?`, `fuzzy?`, `top?`, `limit?`                    | Search loaded definitions (component / module / interface / enum / instance).                                                          |
| `mcc_show_def`           | `name`, `type_filter?`, `file?`, `top?`                                     | Show detailed definition info: pins, params, funcs, interfaces.                                                                        |
| `mcc_lookup`             | `className`, `subName?`, `subKind?`, `fromUri?`                             | Resolve a symbol (supports `uC.PA1` compound references) to its definition location.                                                   |
| `mcc_erc`                | —                                                                           | Electrical rule check on the active workspace: single-point nets, unconnected ports, multi-drive, floating nets.                       |
| `mcc_generate_netlist`   | `entry`, `top?`, `format?`, `libs?`                                         | Generate a netlist (text / JSON) for a `.mc` file.                                                                                     |
| `mcc_export`             | `kind` (netlist / bom / spice / kicad), `entry`, `top?`, `format?`, `libs?` | Export netlist / BOM / SPICE / KiCad for a `.mc` file.                                                                                 |

### 9.3 Typical Workflow

Follow the design → code → compile → debug → verify loop:

```text
1. Load project        mcc_load_project(entry="src/main.mc")
2. Draft / iterate     mcc_validate_component(content="...", libs=["mcode"])
3. Check               mcc_check_file(file_path="src/main.mc")          # single file
                       mcc_check_project()                              # whole project (after 1)
4. Understand errors   mcc_explain_error(code=2008)
5. Find definitions    mcc_search_defs(pattern="RES", kind="component")
                       mcc_show_def(name="RES")
6. Resolve symbols     mcc_lookup(className="uC.PA1")
7. Build               mcc_build(top="main")                            # Pass2 tree + nets
8. Electrical check    mcc_erc                                          # after a build/load
9. Export              mcc_generate_netlist(entry="src/main.mc", top="main")
                       mcc_export(kind="bom", entry="src/main.mc", top="main")
```

Iteration loop: `mcc_validate_component` (or `mcc_check_file`) → fix the
snippet / file → re-check until diagnostics are clean → `mcc_build` →
`mcc_erc` → `mcc_export`.

Notes:

- `mcc_check_project` and `mcc_erc` need an active workspace: call
  `mcc_load_project` (or `mcc_build`) first.
- `mcc_search_defs` searches everything loaded so far; system mcode symbols
  are always available because mcode is loaded at startup.

