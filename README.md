# MCC — MCode Compiler

MCC is a Rust-based compiler and visualization tool for MCode design files. It provides a command-line interface for parsing, analyzing, and rendering circuit/module designs into interactive SVG visualizations.

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
mcc show --module ModuleName
```

## Commands

| Command | Description |
|---------|-------------|
| `parse` | Parse and display the AST of an MC design |
| `check` | Run syntax and semantic analysis |
| `extract` | Extract various targets (components, modules, etc.) |
| `show` | Display detailed information about components/modules |
| `build` | Manifest-driven build (loads dependencies + all passes) |
| `lib` | Manage system libraries (list/install/load/unload) |
| `proj` | Create and manage project workspaces |
| `start` | Start the MCC server for interactive use |
| `stop` | Stop the MCC server |
| `status` | View server status |
| `config` | Manage configuration settings |

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
