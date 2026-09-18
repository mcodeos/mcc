# 7. LSP Extension (mcext)

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
