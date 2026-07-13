// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CLI command definition layer
//!
//! This only declares command structures, does not contain any business logic.
//! Business logic is in `crate::cmds::*` modules.

pub mod config;
pub mod data_dir;
pub mod rpc_client;
pub mod server_config;
use clap::{Parser, Subcommand, ValueEnum};

/// MCC — MCode Compiler command line tool
#[derive(Parser, Debug)]
#[command(
    name = "mcc",
    version,
    about = "MCode Compiler — Load, parse, analyze .mc design files",
    long_about = None,
)]
pub struct Cli {
    // ---------- Global options (corresponding to design doc §3) ----------
    /// Verbose log: -v=info, -vv=debug, -vvv=trace
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Quiet mode, reduce output
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Show log target (module name, e.g., mcc::builder)
    #[arg(long, short = 't', global = true)]
    pub show_target: bool,

    /// Set working directory (corresponds to --cwd)
    #[arg(long, global = true, value_name = "DIR")]
    pub cwd: Option<String>,

    /// Generate shell auto-completion script
    #[arg(long = "completion", global = true, value_name = "SHELL")]
    pub completion: Option<String>,

    /// Subcommand. If no subcommand specified, falls back to legacy compatible behavior
    /// (historical usage `mcc <file> <module> [--viz]`)
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Subcommands supported by first phase (MVP)
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Parse currently loaded content (corresponding to design doc §8.2)
    Parse(ParseArgs),

    /// Syntax/semantic check, output diagnostics (corresponding to design doc §8.3)
    Check(CheckArgs),

    /// Extract various targets (corresponding to design doc §9)
    Extract(ExtractArgs),

    /// Show detailed information for specified component/module/interface
    Show(ShowArgs),

    /// Search across loaded definitions (text/regex/fuzzy)
    Search(SearchArgs),

    /// Query top-level definitions with the structured DSL
    Query(QueryArgs),

    /// Manifest-driven one-click build (load dependencies + Pass1 + Pass2)
    Build(BuildArgs),

    /// System library management (list / install / load / unload / info)
    Lib(LibArgs),

    /// Project workspace management (create)
    Proj(ProjArgs),

    /// Start service (corresponding to design doc §4.1)
    Start(StartArgs),

    /// Stop service (corresponding to design doc §4.2)
    Stop(StopArgs),

    /// View service status (corresponding to design doc §4.3)
    Status(StatusArgs),

    /// Configuration management (get / set / list / reset)
    Config(ConfigArgs),
}

// ============================================================================
// parse
// ============================================================================

#[derive(Parser, Debug)]
pub struct ParseArgs {
    /// Target file to parse
    pub target: Option<String>,

    /// Parse code snippet directly (mutually exclusive with position argument <target>)
    #[arg(long, value_name = "CODE", conflicts_with = "target")]
    pub code: Option<String>,

    /// Load system library (can be specified multiple times), applicable to both local / server
    #[arg(long = "lib", value_name = "NAME")]
    pub lib: Vec<String>,

    /// Top-level module name (auto-guess first module in file if omitted)
    #[arg(long, value_name = "NAME")]
    pub top: Option<String>,

    /// Instance Tree pin sorting: `pinid` (default, sort by pinid number ascending) or
    /// `interface` (sort by interface name grouping)
    #[arg(long, value_enum, default_value_t = PinSortMode::PinId)]
    pub sort: PinSortMode,

    // ── Old main.rs behavior switches ──────────────────────────────────────────
    // Design principles:
    //   - When no stage flag is passed, default = pass1 + pass2 verbose output (matches old mcc <file> <top>)
    //   - --viz / --viz-json is *additive*: enables drawing, but pass1/pass2 still printed by default
    //   - --pass1 / --pass2 / --tree / --ast are *selectors*: after explicit specification, only run checked stages
    //   - --all is shortcut, equivalent to --pass1 --pass2 --viz
    /// Detailed print Pass1 (loaded files / all definitions / top module's ports / symbols / lines)
    #[arg(long)]
    pub pass1: bool,

    /// Run Pass2 instantiation, print module tree / connections / nets
    #[arg(long)]
    pub pass2: bool,

    /// Generate visualization HTML (default circuit.html)
    #[arg(long)]
    pub viz: bool,

    /// Generate visualization JSON instead of HTML
    #[arg(long = "viz-json")]
    pub viz_json: bool,

    /// Equivalent to --pass1 --pass2 --viz
    #[arg(long)]
    pub all: bool,

    /// Output AST node structure (similar to --tree, current implementation shares same TreeNode)
    #[arg(long)]
    pub ast: bool,

    /// Output syntax tree (Lines / Phrases tree structure, JSON friendly)
    #[arg(long)]
    pub tree: bool,

    /// Output depth limit (only applies to --tree / --ast, 0 = unlimited)
    #[arg(long, default_value_t = 0)]
    pub depth: usize,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Output file:
    /// * For --viz / --viz-json means drawing result output path (default circuit.html / stdout JSON)
    /// * For --tree / --ast means report output path
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<String>,
}

// ============================================================================
// check
// ============================================================================

#[derive(Parser, Debug)]
pub struct CheckArgs {
    /// Target file to check
    pub target: Option<String>,

    /// Load system library (can be specified multiple times), applicable to both local / server
    #[arg(long = "lib", value_name = "NAME")]
    pub lib: Vec<String>,

    /// Show errors only, ignore warnings
    #[arg(long)]
    pub errors_only: bool,

    /// Strict mode (any warning also exits with non-zero exit code)
    #[arg(long)]
    pub strict: bool,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

// ============================================================================
// Common types
// ============================================================================

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    JsonPretty,
    Yaml,
}

/// Instance Tree pin list sorting mode
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum PinSortMode {
    /// Sort by pinid number ascending (default). Example: 1, 2, 3, ..., 25, 26
    PinId,
    /// Sort by interface name grouping. Example: all I2C first, then all SPI, then all GPIO ...
    /// Within same interface, still sort by pinid ascending
    Interface,
}

// ============================================================================
// extract
// ============================================================================

#[derive(Parser, Debug)]
pub struct ExtractArgs {
    /// Type of extraction target
    #[arg(value_enum)]
    pub target: ExtractTarget,

    /// Target file to extract
    #[arg(value_name = "FILE")]
    pub file: Option<String>,

    /// Load system library (can be specified multiple times), applicable to both local / server
    #[arg(long = "lib", value_name = "NAME")]
    pub lib: Vec<String>,

    /// Top-level module name
    #[arg(long, value_name = "NAME")]
    pub top: Option<String>,

    /// Filter by name
    #[arg(long, value_name = "PATTERN")]
    pub name: Option<String>,

    /// Filter by type (RES|CAP|DIO|MCU|...)
    #[arg(long, value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Structured filter: comma-separated key=value (key in name|kind|class).
    /// RHS supports `*`/`?` wildcards (converted to regex).
    #[arg(long, value_name = "EXPR")]
    pub filter: Option<String>,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Output to file
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ExtractTarget {
    /// Extract all instances
    Instances,
    /// Extract netlist
    Nets,
    /// Extract component definitions
    Components,
    /// Extract interface definitions
    Interfaces,
}

// ============================================================================
// show
// ============================================================================

#[derive(Parser, Debug)]
pub struct ShowArgs {
    /// Type to show
    #[arg(value_enum)]
    pub target: ShowTarget,

    /// Name to show (list all if omitted)
    pub name: Option<String>,

    /// Load system library (can be specified multiple times), applicable to both local / server
    #[arg(long = "lib", value_name = "NAME")]
    pub lib: Vec<String>,

    /// Parse directly from file (doesn't depend on loaded library/project)
    #[arg(long, short = 'F')]
    pub file: Option<String>,

    /// Top-level module name (used when show net builds netlist)
    #[arg(long, short = 'T', value_name = "NAME")]
    pub top: Option<String>,

    /// Filter by instance kind (component|module|label|interface|bus|busref|list),
    /// used with `show instances <entity>`
    #[arg(long = "type", value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Structured filter (used with `--list` targets).
    /// Comma-separated key=value (key in name|kind|class). RHS supports `*`/`?` wildcards.
    #[arg(long, value_name = "EXPR")]
    pub filter: Option<String>,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Output to file
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ShowTarget {
    // ── Overview / containers ──────────────────────────────────────────────
    /// Overview of all definitions in scope (counts + name lists)
    All,
    /// Show all elements in a file (<name> is the file path)
    File,
    /// List all components, or show one component's details
    Component,
    /// List all modules, or show one module's details
    Module,
    /// List all interfaces, or show one interface's details
    Interface,
    /// List all enums, or show one enum's details
    Enum,
    /// List/show net details (Pass2, uses --top)
    Net,

    // ── Entity internals drill-down (<name> = owning entity, required) ──────
    /// Pins of a component / interface
    Pins,
    /// Ports (in/out/io) of a module
    Ports,
    /// Labels of a module
    Labels,
    /// Sub-instances of a component / module (filter with --type)
    Instances,
    /// Netlist of a module (Pass2)
    Nets,
    /// Attributes of a component / interface
    Attrs,
    /// Functions of a component / module
    Funcs,
    /// Parameter declarations of a component / module / interface
    Params,
    /// Roles of an interface
    Roles,
    /// Values of an enum
    Values,
}

// ============================================================================
// search
// ============================================================================

#[derive(Parser, Debug)]
pub struct SearchArgs {
    /// Pattern to match (substring by default; regex with --regex; fuzzy with --fuzzy)
    pub pattern: String,

    /// Optional file or directory to load before searching (required for
    /// `--kind instance` together with `--top`, so the target module is in
    /// scope for this invocation).
    pub target: Option<String>,

    /// Restrict to one kind: component|module|interface|enum|instance
    #[arg(long, value_enum)]
    pub kind: Option<SearchKind>,

    /// Treat pattern as a regular expression
    #[arg(long)]
    pub regex: bool,

    /// Fuzzy match (Levenshtein distance ≤ 2)
    #[arg(long)]
    pub fuzzy: bool,

    /// Load system library (can be specified multiple times)
    #[arg(long = "lib", value_name = "NAME")]
    pub lib: Vec<String>,

    /// When set, also drill into the instances of this top module (Pass2)
    #[arg(long, value_name = "NAME")]
    pub top: Option<String>,

    /// Cap on result count (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    pub limit: usize,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Shorthand for `--format json`
    #[arg(long, conflicts_with = "format")]
    pub json: bool,

    /// Output to file
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum SearchKind {
    /// Component definitions
    Component,
    /// Module definitions
    Module,
    /// Interface definitions
    Interface,
    /// Enum definitions
    Enum,
    /// Instances inside a top module (requires --top)
    Instance,
}

// ============================================================================
// query
// ============================================================================

#[derive(Parser, Debug)]
pub struct QueryArgs {
    /// Structured query expression (e.g. 'kind=component AND name=RES*')
    pub expr: String,

    /// Optional file or directory to load before querying
    pub target: Option<String>,

    /// Load system library (can be specified multiple times)
    #[arg(long = "lib", value_name = "NAME")]
    pub lib: Vec<String>,

    /// Cap on result count (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    pub limit: usize,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Shorthand for `--format json`
    #[arg(long, conflicts_with = "format")]
    pub json: bool,

    /// Output to file
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<String>,
}

// ============================================================================
// build
// ============================================================================

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Entry file (can be omitted, use entry in manifest)
    pub entry: Option<String>,

    /// Load system library (can be specified multiple times), applicable to both local / server
    #[arg(long = "lib", value_name = "NAME")]
    pub lib: Vec<String>,

    /// Top-level module name (can be omitted, use top_module in manifest)
    #[arg(long, value_name = "NAME")]
    pub top: Option<String>,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Generate circuit visualization (HTML)
    #[arg(long)]
    pub viz: bool,

    /// Output file path
    #[arg(long, short = 'o', value_name = "FILE")]
    pub output: Option<String>,

    /// Whether to include system library definitions, default false
    #[arg(long, default_value_t = false)]
    pub include_system: bool,

    /// Lock to a single layouter for viz (flow|schematic_radial|schematic_sub|hierarchical|radial|layered)
    #[arg(long, value_name = "NAME")]
    pub layouter: Option<String>,
}

// ============================================================================
// lib
// ============================================================================

#[derive(Parser, Debug)]
pub struct LibArgs {
    #[command(subcommand)]
    pub action: LibAction,

    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text, global = true)]
    pub format: OutputFormat,
}

#[derive(Subcommand, Debug)]
pub enum LibAction {
    /// List loaded and installed libraries
    List,

    /// Install library to system directory
    Install {
        /// Library name
        name: String,

        /// Source path (library root directory)
        #[arg(long)]
        from: String,

        /// Version number (optional)
        #[arg(long)]
        version: Option<String>,
    },

    /// Load library into memory
    Load {
        /// Library name
        name: String,
    },

    /// Unload library from memory
    Unload {
        /// Library name
        name: String,
    },

    /// Show library detailed information
    Show {
        /// Library name
        name: String,
    },

    /// Search installed libraries
    Search {
        /// Search keyword (library name or description)
        pattern: String,
    },

    /// Uninstall installed library from disk
    Uninstall {
        /// Library name
        name: String,

        /// Force uninstall (even if loaded into memory)
        #[arg(long, short = 'f')]
        force: bool,
    },
}

// ============================================================================
// proj
// ============================================================================

#[derive(Parser, Debug)]
pub struct ProjArgs {
    #[command(subcommand)]
    pub action: ProjAction,

    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text, global = true)]
    pub format: OutputFormat,
}

#[derive(Subcommand, Debug)]
pub enum ProjAction {
    /// Create project directory and project.toml
    Create {
        /// Project path
        path: String,
    },
}

// ============================================================================
// start (top-level command)
// ============================================================================

#[derive(Parser, Debug)]
pub struct StartArgs {
    /// Service address (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port number (default: 8080)
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Enable TLS
    #[arg(long)]
    pub tls: bool,

    /// TLS certificate file
    #[arg(long)]
    pub cert: Option<String>,

    /// TLS private key file
    #[arg(long)]
    pub key: Option<String>,

    /// Authentication type (none|basic|token)
    #[arg(long, default_value = "none")]
    pub auth: String,

    /// Maximum connections
    #[arg(long, default_value_t = 100)]
    pub max_conn: usize,

    /// Timeout (seconds)
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,

    /// Log level (debug|info|warn|error)
    #[arg(long, default_value = "info")]
    pub log_level: String,

    /// Output logs to file (default outputs to stderr)
    #[arg(long, short = 'l')]
    pub log_file: Option<String>,

    /// Run in background
    #[arg(long, short = 'd')]
    pub background: bool,

    /// PID file location
    #[arg(long)]
    pub pid_file: Option<String>,

    /// Pre-load system library on startup (can be specified multiple); if not specified, don't load any library
    #[arg(long = "lib", value_name = "NAME")]
    pub load_lib: Vec<String>,
}

// ============================================================================
// stop (top-level command)
// ============================================================================

#[derive(Parser, Debug)]
pub struct StopArgs {
    /// Force stop
    #[arg(long, short = 'f')]
    pub force: bool,

    /// Wait timeout (seconds)
    #[arg(long, default_value_t = 10)]
    pub timeout: u64,
}

// ============================================================================
// status (top-level command)
// ============================================================================

#[derive(Parser, Debug)]
pub struct StatusArgs {
    /// JSON format output
    #[arg(long)]
    pub json: bool,

    /// Real-time monitoring
    #[arg(long)]
    pub watch: bool,
}

// ============================================================================
// config (configuration management)
// ============================================================================

#[derive(Parser, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Get configuration item value
    Get {
        /// Configuration item name (e.g., trace.enabled, parser.strict)
        name: String,
    },

    /// Set configuration item value
    Set {
        /// Configuration item name (e.g., trace.enabled, parser.strict)
        name: String,

        /// Configuration value
        value: String,

        /// Additional configuration item names and values (optional)
        #[arg(last = true)]
        rest: Vec<String>,
    },

    /// List all configuration items
    List,

    /// Reset to default values
    Reset,
}
