---
name: mcc
description: CLI reference, RPC protocol, and debugging workflows for the mcc
  compiler and mcode projects. Use when running mcc/mcode CLI commands, driving
  the RPC server, debugging a parse/check/build failure, reading diagnostics,
  decoding error codes, configuring mcc.yaml or project.toml, or working on the
  LSP extension and MCP server.
---

# mcc Compiler

> CLI reference, RPC protocol, and debugging workflows for the mcc compiler and
> mcode projects.

This file is the index and the always-needed part. The bulk is split into
`reference/` and read only when needed:

| File | Covers |
|---|---|
| `reference/authoring-rules.md` | Rules for editing this repo (no name-guessing, no hardcoded paths, test scope) |
| `reference/cli.md` | Every subcommand and flag: `parse`, `check`, `build`, `list`/`show`, `search`/`query`, `export`, `extract`, `lib`, `start`/`stop`/`status` |
| `reference/rpc.md` | JSON-RPC protocol, method tables, error codes |
| `reference/pipeline.md` | Compiler passes, and how to debug mcc itself (VS Code configs, logging, server debugging, trace config, test commands) |
| `reference/debugging.md` | Debugging mcode projects: project layout, common workflows, diagnosing errors, error codes, lapper/refdefmap dumps |
| `reference/lsp-mcext.md` | The `mcext` VS Code LSP extension |
| `reference/config.md` | `mcc.yaml`, `project.toml`, `server.yaml`, and the `mcc-mcp` MCP server |

Read the matching reference file before acting on one of those areas.

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
