# 8. Configuration Reference

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
- Design doc: `mcc-mcp-server-design.md`

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
