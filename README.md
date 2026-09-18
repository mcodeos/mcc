# MCC — MCode Compiler

MCC is a Rust-based compiler and visualization tool for MCode design files. It provides a command-line interface for parsing, analyzing, and rendering circuit/module designs into interactive SVG visualizations.

MCode is an industrial-grade circuit programming language aimed at precise and efficient circuit programming. MCC is the compiler and visualization toolchain for that language, built around this design tenet.

## Features

- **Parsing & Analysis**: Parse `.mc` design files and perform syntax/semantic checks
- **Visualization**: Render designs as SVG with intelligent layout and routing algorithms
- **Interactive Server**: Start a server mode for interactive design exploration
- **Library Management**: Built-in system library management for reusable components
- **Project Workspace**: Project workspace management for multi-module designs

## Installation

```bash
# Build from source
cargo build --release

# Create a symlink (optional)
sudo ln -sf "$(pwd)/target/release/mcc" /usr/local/bin/mcc

# Or add to PATH
export PATH=$PWD/target/release:$PATH
```

## Quick Start

```bash
# Parse a design file
mcc parse design.mc

# Check for errors
mcc check design.mc

# Build a project
mcc build

# Start the interactive server
mcc start

# Show detailed information
mcc show module ModuleName
```

## Commands

25 subcommands, grouped by what they operate on. `mcc <cmd> --help` is authoritative;
the offline manual lives in the sibling docs repo (`mcd/doc/cli/manual.md`).

**Source editing loop**

| Command | Description |
|---------|-------------|
| `parse` | Parse and display the AST of an MC design |
| `check` | Run syntax and semantic analysis |
| `build` | Manifest-driven build (loads dependencies + Pass1 + Pass2) |

**Electrical checks**

| Command | Description |
|---------|-------------|
| `erc` | Electrical rule check — single-point nets, unconnected ports, etc. |
| `rules` | Check-rule registry catalog (list / detail / severity / allow / accept) |

**Reading a projection**

| Command | Description |
|---------|-------------|
| `show` | Show a definition or its internals (pins/ports/nets/funcs/params/...), or a pipeline stage |
| `list` | List top-level definition names (component/module/interface/enum/nets/ports/files) |
| `query` | Query definitions by DSL expression or by name (`search` is an alias) |
| `join` | Join two adjacent pipeline segments by key and report every mismatch |
| `trace` | Follow one key along the whole chain (source → AST → three circuit segments) |
| `diff` | Compare two readings of one view and report what changed |

**Navigation**

| Command | Description |
|---------|-------------|
| `def` | Go-to-definition for a symbol |
| `refs` | Find all references to a symbol |
| `explain` | Explain an error code |
| `impact` | Blast radius of changing one def (which tops, nets, consumers) |

**Producing and reading back artifacts**

| Command | Description |
|---------|-------------|
| `export` | Export netlist / BOM / SPICE (text\|csv\|json) |
| `import` | Read an EDA artifact back and report how it differs from the current world |

**Session, library and scaffolding**

| Command | Description |
|---------|-------------|
| `lib` | System library management (list / install / load / unload / show / search / uninstall) |
| `proj` | Project workspace management (create) |
| `start` | Start the MCC server for interactive use |
| `stop` | Stop the MCC server |
| `status` | View server status |
| `config` | Manage configuration settings |
| `caps` | Show compiler capabilities — self-describing API for AI |
| `fmt` | Format `.mc` sources in place (whitespace only; token text is never touched) |

## Architecture

```
src/
├── ast/           # AST parsing (C syntax definitions)
├── builder/       # Core builder and project management
├── cli/           # Command-line interface definitions
├── cmds/          # Command implementations
├── core/          # Core data models (bus, endpoint, module, etc.)
├── instant/       # Instantiation handling
├── output/        # Output formatting
├── rpc/           # RPC protocol for server mode
├── vector/        # Vector graph building for visualization
└── viz/           # Visualization (layout, routing, rendering)
```

## Visualization

MCC includes sophisticated visualization capabilities:

- **Layout Algorithms**: Hierarchical, radial, grid-based layouts
- **Routing**: Orthogonal, star, and bus bundle routing
- **Rendering**: SVG output with HTML wrapper for interactivity
- **Component Support**: Specialized rendering for resistors, capacitors, inductors, ICs, and more

## Configuration

```bash
# View current configuration
mcc config list

# Set a configuration value
mcc config set <key> <value>

# Reset to defaults
mcc config reset
```

## Development

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

### Code Standards

- All comments must be in English
- Follow Rust idioms and conventions
- Run `cargo fmt` for formatting
- Run `cargo clippy` for linting

## License

Licensed under either of:
- Apache License, Version 2.0
- MIT License

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT) for details.
