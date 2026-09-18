# 4. Compiler Pipeline

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
