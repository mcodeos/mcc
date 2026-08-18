// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CLI command definition layer
//!
//! This only declares command structures, does not contain any business logic.
//! Business logic is in `crate::cmds::*` modules.

pub mod config;
pub mod datadir;
pub mod rpcclient;
pub mod servercfg;
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

    /// Log lines include timestamp, module and file:line
    #[arg(long = "origin", short = 'g', global = true)]
    pub origin: bool,

    /// Change working directory before running
    #[arg(long, short = 'c', global = true, value_name = "DIR")]
    pub cwd: Option<String>,

    /// Enable debug output for a target; aliases: pass1, pass2, fcall, lapper, vec, viz, lsp, all
    #[arg(
        short = 'd',
        long = "debug",
        global = true,
        value_name = "TARGET[=LEVEL]"
    )]
    pub debug_targets: Vec<String>,

    /// Run locally in this process; skip delegation to a running `mcc start` server
    #[arg(long, short = 'L', global = true)]
    pub local: bool,

    /// Load a library before running (can be specified multiple times)
    #[arg(long = "lib", short = 'l', value_name = "NAME", global = true)]
    pub lib: Vec<String>,

    /// Output format
    #[arg(long, short = 'f', value_enum, default_value_t = OutputFormat::Text, global = true)]
    pub format: OutputFormat,

    /// Output to file
    #[arg(long, short = 'o', value_name = "FILE", global = true)]
    pub output: Option<String>,

    /// Top-level module name (auto-guess first module in file if omitted)
    #[arg(long, short = 't', value_name = "NAME", global = true)]
    pub top: Option<String>,

    /// Entry file for a directory target without a manifest (browse mode)
    #[arg(long, short = 'e', value_name = "FILE", global = true)]
    pub entry: Option<String>,

    /// Subcommand. If omitted, prints a usage hint.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Process-wide "run locally" switch, set by `main` from the global `--local` flag.
/// `RpcClient::probe()` honors it so every command skips RPC delegation at one
/// choke point instead of each command carrying its own flag.
pub static LOCAL_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Enable/disable forced local execution for the whole process.
pub fn set_local_mode(enabled: bool) {
    LOCAL_MODE.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// True when `--local` was requested (all commands run in-process, no RPC server).
pub fn local_mode() -> bool {
    LOCAL_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Cross-command option values promoted from per-subcommand fields
/// (`--lib`, `-f/--format`, `-o/--output`, `--top`, `--entry`). `main` stores
/// the parsed values once via [`set_globals`]; every subcommand reads them here
/// (mirrors the `LOCAL_MODE` pattern).
#[derive(Debug, Clone)]
pub struct GlobalOptions {
    /// `--lib NAME` — libraries to load (repeatable)
    pub lib: Vec<String>,
    /// `-f/--format` — output format (clap default: Text)
    pub format: OutputFormat,
    /// `-o/--output FILE` — output file path
    pub output: Option<String>,
    /// `--top NAME` — top-level module name
    pub top: Option<String>,
    /// `--entry FILE` — entry file for browse-mode directory targets
    pub entry: Option<String>,
}

/// Storage for the global option values, filled once by `main` right after CLI parsing.
pub static GLOBAL_OPTIONS: once_cell::sync::OnceCell<GlobalOptions> =
    once_cell::sync::OnceCell::new();

/// Store the parsed global option values (called once by `main`).
pub fn set_globals(g: GlobalOptions) {
    let _ = GLOBAL_OPTIONS.set(g);
}

/// Read the global option values. Only valid after `main` ran [`set_globals`].
pub fn globals() -> &'static GlobalOptions {
    GLOBAL_OPTIONS
        .get()
        .expect("mcc cli globals not initialized by main")
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

    /// Show detailed information for a definition (component/module/interface/enum) or its internals (pins/ports/nets/funcs/params/...)
    Show(ShowArgs),

    /// List top-level definition names (component/module/interface/enum/nets/ports/files)
    List(ListArgs),

    /// Search across loaded definitions (text/regex/fuzzy)
    Search(SearchArgs),

    /// Query top-level definitions with the structured DSL
    Query(QueryArgs),

    /// Export netlist / BOM / SPICE (text|csv|json)
    Export(ExportArgs),

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

    /// Explain error codes (M6)
    Explain(ExplainArgs),

    /// Show compiler capabilities (M6) — self-describing API for AI
    Caps,

    /// Go-to-definition for a symbol (M6)
    Def(DefArgs),

    /// Electrical rule check (M6) — single-point nets, unconnected ports, etc.
    Erc(ErcArgs),

    /// Find all references to a symbol (M6)
    Refs(RefsArgs),

    /// Convert .mc files to/from other formats (M5b)
    Convert(ConvertArgs),

    /// Generate structured design report (M5b)
    Report(ReportArgs),
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

    /// Only output diagnostics (errors and warnings) as `file:line:col: level[code]: message`
    #[arg(long)]
    pub dlog: bool,

    /// Instance Tree pin sorting: `pinid` (default, sort by pinid number ascending) or
    /// `interface` (sort by interface name grouping)
    #[arg(long, value_enum, default_value_t = PinSortMode::PinId)]
    pub sort: PinSortMode,

    // ── Stage selection switches ─────────────────────────────────────────────
    // Design principles:
    //   - When no stage flag is passed, default = pass1 + pass2 verbose output
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
}

// ============================================================================
// check
// ============================================================================

#[derive(Parser, Debug)]
pub struct CheckArgs {
    /// Target file to check
    pub target: Option<String>,

    /// Only output diagnostics (errors and warnings) as `file:line:col: level[code]: message`
    #[arg(long)]
    pub dlog: bool,

    /// Show errors only, ignore warnings
    #[arg(long)]
    pub errors_only: bool,

    /// Strict mode (any warning also exits with non-zero exit code)
    #[arg(long)]
    pub strict: bool,

    /// Run pass2 electrical net checks (driver conflict, floating inputs, etc.)
    #[arg(long)]
    pub nets: bool,

    /// Run pin usage checks (unused pins, conflicting pin options)
    #[arg(long)]
    pub pins: bool,
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
    Csv,
}

/// Instance Tree pin list sorting mode
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum PinSortMode {
    // Sort by pinid number ascending (default). Example: 1, 2, 3, ..., 25, 26
    PinId,
    // Sort by interface name grouping. Example: all I2C first, then all SPI, then all GPIO ...
    // Within same interface, still sort by pinid ascending
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
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ExtractTarget {
    // Extract all instances
    Instances,
    // Extract netlist
    Nets,
    // Extract component definitions
    Components,
    // Extract interface definitions
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

    /// Name of the entity to show (required for detail and drill targets;
    /// name lists moved to `mcc list`)
    pub name: Option<String>,

    /// Parse directly from file (doesn't depend on loaded library/project)
    #[arg(long, short = 'F')]
    pub file: Option<String>,

    /// Filter by instance kind (component|module|label|interface|bus|busref|list),
    /// used with `show instances <entity>`
    #[arg(long = "type", value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Structured filter (accepted but ignored here; name lists moved to
    /// `mcc list`, where --filter takes effect). Comma-separated key=value
    /// (key in name|kind|class). RHS supports `*`/`?` wildcards.
    #[arg(long, value_name = "EXPR")]
    pub filter: Option<String>,

    /// Show source position spans in `show all` text details (hidden by default)
    #[arg(long)]
    pub span: bool,

    /// Definition layers to show: file (default) | use | system | all.
    /// `file` anchors on the -F target; without a file every loaded layer is shown.
    #[arg(long, value_enum)]
    pub scope: Option<ShowScope>,
}

/// Definition layers for `show all` (`--scope`).
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ShowScope {
    // Definitions declared in the target file (-F)
    File,
    // Definitions from use-imported / project libraries
    Use,
    // Definitions from system libraries (mcode and installed libs)
    System,
    // All layers: file + use + system
    All,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ShowTarget {
    // ── Overview ────────────────────────────────────────────────────────────
    // Overview of all definitions in scope, layered by origin
    // (file/use/system, select with --scope; -F anchors the file layer)
    All,
    // One component's details (pins table)
    Component,
    // One module's details (summary + sub-instances)
    Module,
    // One interface's details (pins, roles, params)
    Interface,
    // One enum's details (values)
    Enum,
    // One net's points (Pass2, uses --top)
    Net,
    // Dump LSP lapper intervals for a file (semantic tokens + symbols)
    Lapper,
    // Print AST tree for a file
    Ast,

    // ── Entity internals drill-down (<name> = owning entity, required) ──────
    // Pins of a component / interface
    Pins,
    // Ports (in/out/io) of a module
    Ports,
    // Labels of a module
    Labels,
    // Sub-instances of a component / module (filter with --type)
    Instances,
    // Netlist of a module (Pass2), or connection-line nets of a func body
    // (dot-qualified `OWNER.FUNC`, no Pass2)
    Nets,
    // Attributes of a component / interface
    Attrs,
    // Functions of a component / module
    Funcs,
    // Parameter declarations of a component / module / interface / func
    // (funcs are dot-qualified `OWNER.FUNC`)
    Params,
    // Roles of an interface
    Roles,
    // Values of an enum
    Values,
}

// ============================================================================
// list
// ============================================================================

#[derive(Parser, Debug)]
pub struct ListArgs {
    /// What to list
    #[arg(value_enum)]
    pub target: ListTarget,

    /// Parse directly from file (doesn't depend on loaded library/project)
    #[arg(long, short = 'F')]
    pub file: Option<String>,

    /// Structured filter on the name lists (all/component/module/interface/enum).
    /// Comma-separated key=value (key in name|kind|class). RHS supports `*`/`?` wildcards.
    #[arg(long, value_name = "EXPR")]
    pub filter: Option<String>,

    /// Definition layers for `list all` (same policy as `show all`):
    /// file (default) | use | system | all. Accepted for the other targets
    /// but ignored.
    #[arg(long, value_enum)]
    pub scope: Option<ShowScope>,

    /// Filter by instance kind (accepted but ignored here; used by `show instances`)
    #[arg(long = "type", value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Show source position spans (accepted but ignored here; used by `show all`)
    #[arg(long)]
    pub span: bool,
}

/// What to list
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ListTarget {
    // Flat aggregate of every definition in scope, kind-tagged
    // ({type:"all", count, list:[{name, kind}]})
    All,
    // All component names
    Component,
    // All module names
    Module,
    // All interface names
    Interface,
    // All enum names
    Enum,
    // All Pass2 nets of the top module (--top overrides; each entry has points)
    Nets,
    // All module ports
    Ports,
    // All loaded files with per-file definition counts
    Files,
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

    /// Cap on result count (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    pub limit: usize,

    /// Shorthand for `--format json`
    #[arg(long, conflicts_with = "format")]
    pub json: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum SearchKind {
    // Component definitions
    Component,
    // Module definitions
    Module,
    // Interface definitions
    Interface,
    // Enum definitions
    Enum,
    // Instances inside a top module (requires --top)
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

    /// Cap on result count (0 = unlimited)
    #[arg(long, default_value_t = 0)]
    pub limit: usize,

    /// Shorthand for `--format json`
    #[arg(long, conflicts_with = "format")]
    pub json: bool,
}

// ============================================================================
// export
// ============================================================================

#[derive(Parser, Debug)]
pub struct ExportArgs {
    /// What to export
    #[arg(value_enum)]
    pub kind: ExportKind,

    /// Source .mc file (must define a top module)
    pub file: String,

    /// Shorthand for `--format json`
    #[arg(long, conflicts_with = "format")]
    pub json: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ExportKind {
    // SPICE-like text netlist (nets → points)
    Netlist,
    // Bill of materials (CSV / text / JSON)
    Bom,
    // SPICE deck (hierarchical .SUBCKT + X lines)
    Spice,
    // KiCad s-expression netlist (M8)
    #[value(name = "kicad")]
    KiCad,
}

// ============================================================================
// build
// ============================================================================

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Entry file (can be omitted, use entry in manifest or the global --entry)
    pub file: Option<String>,

    /// Generate circuit visualization (HTML)
    #[arg(long)]
    pub viz: bool,

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
        #[arg(long)]
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
    #[arg(long)]
    pub log_file: Option<String>,

    /// Run in background
    #[arg(long, short = 'b')]
    pub background: bool,

    /// PID file location
    #[arg(long)]
    pub pid_file: Option<String>,
}

// ============================================================================
// stop (top-level command)
// ============================================================================

#[derive(Parser, Debug)]
pub struct StopArgs {
    /// Force stop
    #[arg(long)]
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

// ============================================================================
// def (M6)
// ============================================================================

#[derive(Parser, Debug)]
pub struct DefArgs {
    /// Symbol name to find
    pub name: String,

    /// Parse directly from file
    #[arg(long, short = 'F')]
    pub file: Option<String>,
}

// ============================================================================
// refs (M6)
// ============================================================================

#[derive(Parser, Debug)]
pub struct RefsArgs {
    /// Symbol name to find references for
    pub name: String,

    /// Parse directly from file
    #[arg(long, short = 'F')]
    pub file: Option<String>,
}

// ============================================================================
// report (M5b)
// ============================================================================

#[derive(Parser, Debug)]
pub struct ReportArgs {
    /// Target file or project (optional — uses current workspace if omitted)
    pub target: Option<String>,
}

// ============================================================================
// convert (M5b)
// ============================================================================

#[derive(Parser, Debug)]
pub struct ConvertArgs {
    /// Source .mc file
    pub file: String,

    /// Target format: json, yaml
    #[arg(long, default_value = "json")]
    pub to: String,
}

// ============================================================================
// erc (M6)
// ============================================================================

#[derive(Parser, Debug)]
pub struct ErcArgs {
    /// Target file or project directory
    pub target: Option<String>,
}

// ============================================================================
// explain (M6)
// ============================================================================

#[derive(Parser, Debug)]
pub struct ExplainArgs {
    /// Error code to look up (omit to list all)
    pub code: Option<u32>,
}
