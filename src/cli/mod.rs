// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! CLI command definition layer
//!
//! This only declares command structures, does not contain any business logic.
//! Business logic is in `crate::cmds::*` modules.

pub mod config;
pub mod datadir;
pub mod manifest;
pub mod rpcclient;
pub mod servercfg;
use clap::{Parser, Subcommand, ValueEnum};

/// MCC — MCode Compiler command line tool
#[derive(Parser, Debug)]
#[command(
    name = "mcc",
    version = concat!(env!("CARGO_PKG_VERSION"), ".b", env!("MCC_BUILD_NR")),
    about = "MCode Compiler — Load, parse, analyze .mc design files",
    long_about = None,
)]
pub struct Cli {
    // Global options (corresponding to design doc §3)
    /// Verbose log: -v=info, -vv=debug, -vvv=trace
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Quiet mode, reduce output
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Suppress warning diagnostics with the given codes, comma-separated
    /// (e.g. `-i E3137,E5641`). Warning-only; errors are never suppressed.
    /// Merges over `diag.ignore_warnings` in mcc.yaml.
    #[arg(
        long = "ignore",
        short = 'i',
        global = true,
        value_name = "CODES",
        alias = "ignore-warnings"
    )]
    pub ignore_warnings: Vec<String>,

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

    /// Strict mode: strict-only diagnostics (e.g. missing required constructor
    /// parameters) are reported as warnings; dev mode ignores them
    #[arg(long, global = true)]
    pub strict: bool,

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
/// (`--lib`, `-f/--format`, `-o/--output`, `--top`, `--entry`, `--strict`,
/// `-q/--quiet`). `main` stores the parsed values once via [`set_globals`];
/// every subcommand reads them here (mirrors the `LOCAL_MODE` pattern).
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
    /// `--strict` — report strict-only diagnostics as warnings
    pub strict: bool,
    /// `-q/--quiet` — reduce output
    pub quiet: bool,
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

/// True when `--strict` was requested. Safe to call outside a CLI process
/// (library consumers, tests, RPC server without `--strict`): returns false
/// when globals are not initialized.
pub fn strict_mode() -> bool {
    GLOBAL_OPTIONS.get().map(|g| g.strict).unwrap_or(false)
}

/// Subcommands supported by first phase (MVP)
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Parse currently loaded content (corresponding to design doc §8.2)
    Parse(ParseArgs),

    /// Syntax/semantic check, output diagnostics (corresponding to design doc §8.3)
    Check(CheckArgs),

    /// Join two adjacent segments of the compile pipeline by key, and report
    /// every mismatch with its cardinality (stage-readout-design §5.3 ②)
    Join(JoinArgs),

    /// Follow one key along the whole chain — source, AST, and the three circuit
    /// segments — and print what it is at each (stage-readout-design §5.3 ③)
    Trace(TraceArgs),

    /// Compare two readings of one view and report what changed between them
    /// (stage-readout-design §5.3 ④, §6.1/§6.2)
    Diff(DiffArgs),

    /// Show detailed information for a definition (component/module/interface/enum) or its
    /// internals (pins/ports/nets/funcs/params/...)
    Show(ShowArgs),

    /// List top-level definition names (component/module/interface/enum/nets/ports/files)
    List(ListArgs),

    /// Query definitions by DSL expression or by name — `mcc search <X>` is an
    /// alias for a bare-name substring query (text/regex/fuzzy/substring)
    #[command(visible_alias = "search")]
    Query(QueryArgs),

    /// Export netlist / BOM / SPICE (text|csv|json)
    Export(ExportArgs),

    /// Manifest-driven one-click build (load dependencies + Pass1 + Pass2)
    Build(BuildArgs),

    /// System library management (list / install / load / unload / show / search / uninstall)
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

    /// Explain error codes
    Explain(ExplainArgs),

    /// Check-rule registry catalog (list / detail / severity / allow / accept)
    Rules(RulesArgs),

    /// Show compiler capabilities — self-describing API for AI
    Caps,

    /// Go-to-definition for a symbol
    Def(DefArgs),

    /// Electrical rule check — single-point nets, unconnected ports, etc.
    Erc(ErcArgs),

    /// Find all references to a symbol
    Refs(RefsArgs),

    /// Format `.mc` sources in place (whitespace only; token text is never touched)
    Fmt(FmtArgs),

    /// Blast radius of changing one def (which tops, nets, consumers)
    Impact(ImpactArgs),

    /// Read an EDA artifact back and report how it differs from the current world
    Import(ImportArgs),
}

// parse

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

    // Stage selection switches
    // Design principles:
    //   - When no stage flag is passed, default = pass1 + pass2 verbose output
    // - --viz / --viz-json is *additive*: enables drawing, but pass1/pass2 still printed by default
    // - --pass1 / --pass2 / --tree / --ast are *selectors*: after explicit specification, only run
    // checked stages
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

// check

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

    /// Run pass2 electrical net checks (driver conflict, floating inputs, etc.)
    #[arg(long)]
    pub nets: bool,

    /// Run pin usage checks (unused pins, conflicting pin options)
    #[arg(long)]
    pub pins: bool,

    /// Include the failure ledger (resolve-gate-design.md §7.1): summary
    /// counts always; `--ledger` adds per-row detail, `--ledger=audit` adds
    /// deferred / ambiguous-resolution detail.
    #[arg(long, num_args = 0..=1, default_missing_value = "detail", require_equals = true)]
    pub ledger: Option<String>,
}

// join

#[derive(Parser, Debug)]
pub struct JoinArgs {
    /// Left segment: `src` or one of the stage names (`p2` | `vec` | `viz`)
    pub a: String,

    /// Right segment. Only adjacent pairs in chain order are accepted, so the
    /// second segment is whichever one follows the first.
    pub b: String,

    /// Only print this class: carry | expand | merge | drop | synth | skip
    #[arg(long, value_name = "CLASS")]
    pub only: Option<String>,

    /// Parse directly from file (doesn't depend on loaded library/project)
    #[arg(long, short = 'F')]
    pub file: Option<String>,
}

// diff

/// Which view two readings are compared in.
///
/// The design names the face as `--view stage.*`, and each segment needs a
/// per-class key table of its own before it can be aligned across builds — a
/// view cannot be compared by a key table written for another view's classes
/// (`stages::stage_diff`). So the enum is a closed set, and the segments land in
/// it one at a time rather than as an open string that would silently accept a
/// name no key table covers (see the module doc of `cmds::diff`).
///
/// `stage.p1` is not a member, and not because it is unbuilt: it publishes no
/// items at all, so a difference over two of its readings reports zero changes,
/// which reads exactly like two identical readings.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum DiffView {
    /// `stage.viz`: the drawn circuit. Keyed per class by `stages::stage_diff`.
    #[value(name = "stage.viz")]
    StageViz,

    /// `stage.p2`: the flat electrical truth.
    #[value(name = "stage.p2")]
    StageP2,

    /// `stage.vec`: the vector graph, and the projection's own log.
    #[value(name = "stage.vec")]
    StageVec,
}

#[derive(clap::Args, Debug, Clone)]
pub struct DiffArgs {
    /// Left operand: the reference the changes are reported against. A source
    /// path (or project) read now, or a reading saved earlier by `mcc show
    /// stage <seg> -f json -o <file>`.
    pub a: String,

    /// Right operand: the reading compared against the left one. Same two
    /// kinds as the left one.
    pub b: String,

    /// Which view to compare.
    #[arg(long, value_enum, default_value = "stage.viz", value_name = "VIEW")]
    pub view: DiffView,
}

#[derive(clap::Args, Debug, Clone)]
pub struct TraceArgs {
    /// The key to follow. Its form is read off the key itself, and there are
    /// four: an in-domain handle (`N12:3`), an instance's canonical path
    /// (`top.u1.vin`), a def's canonical key (`lib/power.mc::LDO`), or a source
    /// position (`mcu.mc:23`).
    pub key: String,

    /// Parse directly from file (doesn't depend on loaded library/project)
    #[arg(long, short = 'F')]
    pub file: Option<String>,
}

// Common types

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable detailed report: definitions / instance tree /
    /// connections / nets / net summary, rendered from the envelope data
    /// (identical local ↔ RPC).
    Text,
    Json,
    JsonPretty,
    Yaml,
    Csv,
}

impl OutputFormat {
    /// Whether this format is a machine JSON dialogue: the envelope is the
    /// contract, so gates degrade the exit code instead of suppressing output.
    pub fn is_jsonish(&self) -> bool {
        matches!(self, OutputFormat::Json | OutputFormat::JsonPretty)
    }

    /// The `u8` tag the export / output protocol dispatches on.
    pub fn id(self) -> u8 {
        match self {
            OutputFormat::Text => 0,
            OutputFormat::Json => 1,
            OutputFormat::JsonPretty => 2,
            OutputFormat::Yaml => 3,
            OutputFormat::Csv => 4,
        }
    }

    /// Canonical format token, shared by the RPC `format` field and the echoed
    /// format of the emitted envelope.
    pub fn name(self) -> &'static str {
        match self {
            OutputFormat::Text => "text",
            OutputFormat::Json => "json",
            OutputFormat::JsonPretty => "json-pretty",
            OutputFormat::Yaml => "yaml",
            OutputFormat::Csv => "csv",
        }
    }

    /// Parse a format token; an unknown spelling falls back to `text`.
    pub fn from_name(s: &str) -> OutputFormat {
        match s {
            "json" => OutputFormat::Json,
            "json-pretty" | "jsonpretty" => OutputFormat::JsonPretty,
            "yaml" => OutputFormat::Yaml,
            "csv" => OutputFormat::Csv,
            _ => OutputFormat::Text,
        }
    }
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

// show

#[derive(Parser, Debug)]
pub struct ShowArgs {
    /// Type to show
    #[arg(value_enum)]
    pub target: ShowTarget,

    /// Name of the entity to show (required for detail and drill targets;
    /// name lists moved to `mcc list`). File-based targets (`all`, `dianlu`,
    /// `lapper`, `ast`) treat the positional as the target file/directory
    /// when `-F` is not given.
    pub name: Option<String>,

    /// Parse directly from file (doesn't depend on loaded library/project)
    #[arg(long, short = 'F')]
    pub file: Option<String>,

    /// Filter by instance kind (component|module|label|interface|bus|busref|list),
    /// used with `show instances <entity>`
    #[arg(long = "type", value_name = "TYPE")]
    pub r#type: Option<String>,

    /// Show source position spans in `show all` text details (hidden by default)
    #[arg(long)]
    pub span: bool,

    /// Annotate `show dianlu` with dianlu-space identity ids: every instance
    /// line / module section carries its node `NodeId` plus the `DefId` of
    /// the def it instantiates; every pin/port of an instance row carries
    /// its lane-layer physical point `N<n>:<m>` — the owning instance node
    /// `NodeId` plus the pin's stable `DefMemberId` from the def registry
    /// member ledger (design §4 D1). Text renders `name@N<n>:<m>` beside
    /// each pin; structured output adds a `"points": {pin: "N<n>:<m>"}` map
    /// (component pins) / `"ports"` map (sub-module io ports) alongside the
    /// plain pin-name list (text and JSON).
    #[arg(long)]
    pub ids: bool,

    /// `show pwrflow`: widen the rail-contract table to full detail
    /// (nominal + tol/capacity/eff/gen) instead of the compact default
    /// (`domain [hot/ret] voltage world`).
    #[arg(long)]
    pub full: bool,

    /// `show pwrflow`: unfold decoupler annotations (`(×n decaps)` → one row
    /// per decoupling two-pad element) instead of the default folded count.
    #[arg(long)]
    pub decaps: bool,

    /// Definition layers to show: file (default) | use | system | all.
    /// `file` anchors on the -F target; without a file every loaded layer is
    /// shown. `show defs` defaults to every loaded layer (all) instead.
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
    // Overview
    // Overview of all definitions in scope, layered by origin
    // (file/use/system, select with --scope; -F anchors the file layer)
    All,
    // The whole current definition space in registry form: every live def
    // with its `DefId`, kind and declaring file, in DefId order per kind.
    // The def-space twin of `dianlu`: instance `D<id>` tags printed by
    // `show dianlu --ids` resolve to their def here. Defaults to every
    // loaded layer (file/use/system); narrow with --scope.
    Defs,
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
    // Whole circuit tree after instantiation (Pass2, uses --top):
    // one section per module; same-level instances (components, labels,
    // buses, sub-modules) and connections first, then each sub-module in
    // its own nested section. Interface-typed buses are annotated with
    // their interface class, e.g. `uC.UART0{TX, RX} :: UART.TTL(DCE)`.
    Dianlu,
    // Power-intent facts after instantiation + flatten (Pass2, uses --top):
    // a recursive tree — per module its declared planes (conduit/@role,
    // domain rails), DC faces / body edges, the merged net each rail member
    // lands on (with the full point set), and the power contracts of the
    // component leaves it instantiates. `mcc show pwr US513` prints the
    // face/member nets of one module standalone; the project top shows the
    // cross-module unions (shared GND / a rail fan-out). Companion of
    // `show dianlu` (structure) — see also `show erc` for the rule findings.
    Pwr,
    // Top-level power-flow single view (Pass2, uses --top): the *generative*
    // twin of `show pwr` — one screen of "how this board's power flows":
    // world crowns (return copper × conduit role), the rail contract table,
    // and the supply tree (source / merge / trunk / converter / domain rail /
    // load), with world crosses (an edge whose producer return copper differs
    // from its own) marked where a supply edge crosses return copper. Derived
    // from one flat build + NetIslandIndex (cross-module
    // pass-through propagation — the view-only first consumer of the deferred
    // S-set step); facts stay in `show pwr`, this reports the generated flow.
    // `--full` widens the rail contract columns, `--decaps` unfolds decouplers.
    Pwrflow,
    // Dump LSP lapper intervals for a file (semantic tokens + symbols)
    Lapper,
    // Print AST tree for a file
    Ast,
    // One segment of the compile pipeline as data (stage-readout-design.md
    // §5.3): `mcc show stage <p1|p2|vec|viz>` — the `<name>` positional is the
    // segment, not an entity. Emits the projection envelope (`view` =
    // "stage.<seg>") with one sorted `items` array rendered to both faces; the
    // three views supersede the `MC_VEC_DUMP` / `MC_VIZ_DUMP` stderr prose,
    // which keep working. `p1` is a reserved placeholder (the Pass1 view is
    // not adjudicated yet), so the command family does not change shape when
    // it lands. Local-only, like `lapper` / `ast`: with a running `mcc start`
    // service pass `-L`, or the readout is delegated and prints nothing.
    Stage,

    // The organization directory of the current definition space
    // (organization-units-design.md §8; CIMP §1 U120): every unit listed under
    // its own key — the definition kinds by `(uri, ident)`, a func by its host,
    // a bus by its name and host, a clause by its position. Emits the
    // projection envelope with `view` = "org-units", one `items` array sorted
    // by `(kind, key)` and rendered to both faces. Derived, read-only, and it
    // issues no id: it is the directory of the bridge, not the bridge.
    OrgUnits,

    // Entity internals drill-down (<name> = owning entity, required)
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

impl ShowTarget {
    /// Canonical sub-face token, as typed on the command line.
    ///
    /// Shared by clap's value parsing (the variant names are the tokens, with no
    /// `#[value(name = …)]` override anywhere in the enum) and by the emitted
    /// envelope's `command` — `mcc show <token>` — which is the **only**
    /// discriminator among the 22 sub-faces sharing the `show` projection key:
    /// 16 of their payloads carry no `type` field of their own. One table, so a
    /// new sub-face cannot land a token in one place and not the other; the
    /// pairing is asserted by the test below.
    pub fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Defs => "defs",
            Self::Component => "component",
            Self::Module => "module",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::Net => "net",
            Self::Dianlu => "dianlu",
            Self::Pwr => "pwr",
            Self::Pwrflow => "pwrflow",
            Self::Lapper => "lapper",
            Self::Ast => "ast",
            Self::Stage => "stage",
            Self::OrgUnits => "org-units",
            Self::Pins => "pins",
            Self::Ports => "ports",
            Self::Labels => "labels",
            Self::Instances => "instances",
            Self::Nets => "nets",
            Self::Attrs => "attrs",
            Self::Funcs => "funcs",
            Self::Params => "params",
            Self::Roles => "roles",
            Self::Values => "values",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`ShowTarget::name`] must return the token clap accepts, because the
    /// envelope's `command` is built from it and consumers key off that string.
    #[test]
    fn show_target_name_matches_the_clap_token() {
        for t in ShowTarget::value_variants() {
            let clap = t.to_possible_value().expect("every variant is a value");
            assert_eq!(t.name(), clap.get_name(), "sub-face token drift");
        }
    }
}

// list

#[derive(Parser, Debug)]
pub struct ListArgs {
    /// What to list
    #[arg(value_enum)]
    pub target: ListTarget,

    /// Parse directly from file (doesn't depend on loaded library/project)
    #[arg(long, short = 'F')]
    pub file: Option<String>,

    /// Structured filter on the name lists (all/component/module/interface/enum/
    /// func/bus). Comma-separated key=value (key in name|kind|class). RHS
    /// supports `*`/`?` wildcards. `list clause` filters on the **host** name —
    /// a clause has no name of its own.
    #[arg(long, value_name = "EXPR")]
    pub filter: Option<String>,

    /// Definition layers for `list all` (same policy as `show all`):
    /// file (default) | use | system | all. Accepted for the other targets
    /// but ignored.
    #[arg(long, value_enum)]
    pub scope: Option<ShowScope>,
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
    // All func members of the definition space, as host members:
    // (func, host, host_kind, uri). A func is not a standalone def, so its key
    // is the (host, name) pair rather than a name of its own.
    Func,
    // All declared buses (`X{P, N}`), one row per declaration:
    // (bus, host, host_kind, members, uri). A bus carries no DefId, so its key
    // is its name plus its host.
    Bus,
    // All clause groups of the definition space, keyed by position
    // (uri, byte offset). A clause is the one unit with no declaration object
    // and no name, so it is listed by where it is, not by what it is called.
    Clause,
}

// query  (search folded in: `mcc search <X>` = alias for a bare-name query)

#[derive(Parser, Debug)]
pub struct QueryArgs {
    /// DSL query expression (e.g. 'kind=component AND name=RES*') or a def
    /// name — a value that does not compile as a DSL expression is matched as
    /// a case-insensitive substring of def names (the default).
    pub expr: String,

    /// Optional file or directory to load before querying
    pub target: Option<String>,

    /// Restrict to one kind:
    /// component|module|interface|enum|instance|net|func|bus|clause
    #[arg(long, value_enum)]
    pub kind: Option<SearchKind>,

    /// Treat <EXPR | name> as a regular expression (name search mode)
    #[arg(long, conflicts_with = "fuzzy", conflicts_with = "substring")]
    pub regex: bool,

    /// Fuzzy name match (Levenshtein distance ≤ 2)
    #[arg(long, conflicts_with = "regex", conflicts_with = "substring")]
    pub fuzzy: bool,

    /// Explicit substring name match (the default when no matcher flag is given)
    #[arg(long, conflicts_with = "regex", conflicts_with = "fuzzy")]
    pub substring: bool,

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
    // Pass2 nets of the top module (--top / target-file module; each row has
    // points). Not a definition kind — the lib search engine has no Net;
    // `query --kind net` projects the net table in cmds/query.rs.
    Net,
    // Func members, as host members (host, name). Like `net`, not a definition
    // kind the search engine holds: `query --kind func` projects the directory
    // rows in cmds/query.rs.
    Func,
    // Declared buses, keyed by name plus host. Same projection route as `func`.
    Bus,
    // Clause groups, keyed by position. Same projection route as `func`.
    Clause,
}

// export

#[derive(Parser, Debug)]
pub struct ExportArgs {
    /// What to export
    #[arg(value_enum)]
    pub kind: ExportKind,

    /// Source .mc file or project directory; defaults to the current directory
    /// when it holds a project manifest (must define a top module)
    pub file: Option<String>,

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
    // SPICE-style text netlist, human-readable -- not a simulation deck
    Spice,
    // KiCad s-expression netlist (M8)
    #[value(name = "kicad")]
    KiCad,
    // Instance list carrying the two-space identity (build-design §3.7)
    #[value(name = "inst-list")]
    InstList,
}

impl ExportKind {
    /// Every kind, in `id()` order -- the one list the outward faces agree
    /// with: `id()` tags `build_payload`, `name()` is the `KIND` token, and
    /// the RPC `features.export` array is derived from here.
    pub const ALL: [ExportKind; 5] = [
        ExportKind::Netlist,
        ExportKind::Bom,
        ExportKind::Spice,
        ExportKind::KiCad,
        ExportKind::InstList,
    ];

    /// The `u8` tag `export::build_payload` dispatches on.
    pub fn id(self) -> u8 {
        match self {
            ExportKind::Netlist => 0,
            ExportKind::Bom => 1,
            ExportKind::Spice => 2,
            ExportKind::KiCad => 3,
            ExportKind::InstList => 4,
        }
    }

    /// The `KIND` token shared by the RPC `kind` field and the reported kind
    /// of the emitted envelope (build-design §3.6 contract 3: one table, so a
    /// new product changes a single place).
    pub fn name(self) -> &'static str {
        match self {
            ExportKind::Netlist => "netlist",
            ExportKind::Bom => "bom",
            ExportKind::Spice => "spice",
            ExportKind::KiCad => "kicad-netlist",
            ExportKind::InstList => "inst-list",
        }
    }

    /// Parse a `KIND` token, accepting the `kicad` short spelling; an unknown
    /// token falls back to the netlist default.
    pub fn from_name(s: &str) -> ExportKind {
        match s {
            "bom" => ExportKind::Bom,
            "spice" => ExportKind::Spice,
            "kicad" | "kicad-netlist" => ExportKind::KiCad,
            "inst-list" => ExportKind::InstList,
            _ => ExportKind::Netlist,
        }
    }

    /// The `ExportKind`s that are also **build products** (build-design §3.3).
    ///
    /// `bom` is deliberately absent. It was retired as a product on 2026-09-14
    /// -- the pairing and the issuing belong to downstream tooling, and mcc
    /// stops at the instance list -- so accepting it here would rebuild the
    /// very artifact the boundary document hands off. The standalone command
    /// survives (`mcc export bom`), which is why the enum still carries it.
    pub const BUILD_PRODUCTS: [ExportKind; 4] = [
        ExportKind::Netlist,
        ExportKind::Spice,
        ExportKind::KiCad,
        ExportKind::InstList,
    ];

    /// The name a build product lands under in `<project-root>/build/`
    /// (build-design §3.4), for the format its body is written in. The stem
    /// follows that section's path tree; the extension follows the body, so a
    /// product written as CSV does not claim to be the text artifact.
    pub fn default_file_name(self, format: OutputFormat) -> String {
        // §3.4 names the two design-level artifacts with an extension of their
        // own, so only an explicit non-text format re-opens the question.
        if format == OutputFormat::Text {
            match self {
                ExportKind::Spice => return "design.spice".to_string(),
                ExportKind::KiCad => return "design.kicad_netlist".to_string(),
                _ => {}
            }
        }
        let ext = match format {
            OutputFormat::Csv => "csv",
            OutputFormat::Yaml => "yaml",
            OutputFormat::Json | OutputFormat::JsonPretty => "json",
            OutputFormat::Text => "txt",
        };
        let stem = match self {
            ExportKind::Spice => "design",
            ExportKind::KiCad => "design.kicad",
            _ => self.name(),
        };
        format!("{stem}.{ext}")
    }
}

// impact

#[derive(Parser, Debug)]
pub struct ImpactArgs {
    /// Symbol to assess: a def name (component / module / interface / define)
    /// or an instance path
    pub sym: String,

    /// Source .mc file or project directory; defaults to the current directory
    /// when it holds a project manifest
    pub file: Option<String>,

    /// Shorthand for `--format json`
    #[arg(long, conflicts_with = "format")]
    pub json: bool,
}

// import

#[derive(Parser, Debug)]
pub struct ImportArgs {
    /// EDA artifact to read back: the file `mcc export` produced
    pub file: String,

    /// Source .mc file or project directory the artifact is compared against;
    /// defaults to the current directory when it holds a project manifest
    pub target: Option<String>,

    /// Input EDA format. Spelled `--from` because `-f/--format` is the global
    /// output format.
    #[arg(
        long = "from",
        value_enum,
        default_value_t = ImportFormat::Netlist,
        value_name = "FORMAT"
    )]
    pub from: ImportFormat,

    /// Shorthand for `--format json`
    #[arg(long, conflicts_with = "format")]
    pub json: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum ImportFormat {
    // SPICE-like text netlist (nets -> points); the `export netlist` artifact
    Netlist,
    // KiCad s-expression netlist (M8); the `export kicad` artifact
    #[value(name = "kicad")]
    KiCad,
    // EasyEDA netlist; no exporter writes it yet, so read-back has no reference
    // side to subtract (batch of its own)
    #[value(name = "easyeda")]
    EasyEda,
}

impl ImportFormat {
    /// The `FORMAT` token carried by the report.
    pub fn name(self) -> &'static str {
        match self {
            ImportFormat::Netlist => "netlist",
            ImportFormat::KiCad => "kicad",
            ImportFormat::EasyEda => "easyeda",
        }
    }

    /// The `export` kind that produces the artifact this format reads back, or
    /// `None` where no producer exists.
    pub fn export_kind(self) -> Option<ExportKind> {
        match self {
            ImportFormat::Netlist => Some(ExportKind::Netlist),
            ImportFormat::KiCad => Some(ExportKind::KiCad),
            ImportFormat::EasyEda => None,
        }
    }

    /// Parse a `FORMAT` token; an unknown token falls back to the netlist
    /// default, exactly as `ExportKind::from_name` does.
    pub fn from_name(s: &str) -> ImportFormat {
        match s {
            "kicad" => ImportFormat::KiCad,
            "easyeda" => ImportFormat::EasyEda,
            _ => ImportFormat::Netlist,
        }
    }
}

// build

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

    /// Lock to a single layouter for viz
    /// (flow|schematic_radial|schematic_sub|hierarchical|radial|layered)
    #[arg(long, value_name = "NAME")]
    pub layouter: Option<String>,

    /// Write file products into `<project-root>/build/` (build-design §3.4).
    /// The ids are the `mcc export <KIND>` tokens, because a product's bytes
    /// must not depend on which entry produced them (§3.6 contract 1). Omitted
    /// = the default set, which is the envelope alone: no file product.
    #[arg(long = "product", value_name = "IDS", value_delimiter = ',')]
    pub products: Vec<ExportKind>,
}

// lib

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

// proj

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

// start (top-level command)

#[derive(Parser, Debug)]
pub struct StartArgs {
    /// Service address (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// Port number (default: 8080)
    #[arg(long, default_value_t = 8080)]
    pub port: u16,

    /// Output logs to file (default outputs to stderr)
    #[arg(long)]
    pub log_file: Option<String>,

    /// Run in background
    #[arg(long, short = 'b')]
    pub background: bool,
}

// stop (top-level command)

#[derive(Parser, Debug)]
pub struct StopArgs {
    /// Force stop
    #[arg(long)]
    pub force: bool,

    /// Wait timeout (seconds)
    #[arg(long, default_value_t = 10)]
    pub timeout: u64,
}

// status (top-level command)

#[derive(Parser, Debug)]
pub struct StatusArgs {
    /// Shorthand for `--format json`. An alias, never an override: it is
    /// mutually exclusive with `-f/--format`, so the global format stays the
    /// single source of truth (world-repartition-design.md §2.5).
    #[arg(long, conflicts_with = "format")]
    pub json: bool,

    /// Real-time monitoring
    #[arg(long)]
    pub watch: bool,
}

// config (configuration management)

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

// def

#[derive(Parser, Debug)]
pub struct DefArgs {
    /// Symbol name to find
    pub name: String,

    /// Parse directly from file
    #[arg(long, short = 'F')]
    pub file: Option<String>,
}

// refs

#[derive(Parser, Debug)]
pub struct RefsArgs {
    /// Symbol name to find references for
    pub name: String,

    /// Parse directly from file
    #[arg(long, short = 'F')]
    pub file: Option<String>,
}

// fmt

#[derive(Parser, Debug)]
pub struct FmtArgs {
    /// `.mc` file or directory to format; defaults to the current directory
    pub target: Option<String>,

    /// Report the files that need formatting instead of rewriting them;
    /// exits 1 when any file differs
    #[arg(long)]
    pub check: bool,
}

// erc

#[derive(Parser, Debug)]
pub struct ErcArgs {
    /// Target file or project directory
    pub target: Option<String>,
}

// explain

#[derive(Parser, Debug)]
pub struct ExplainArgs {
    /// Error code to look up (omit to list all)
    pub code: Option<u32>,
}

// rules (check-rule registry §8)

/// `mcc rules` — catalog read projection + unified override write face
/// (rule-registry design §8 / §8-5). With no subcommand, lists the catalog.
#[derive(Parser, Debug)]
pub struct RulesArgs {
    #[command(subcommand)]
    pub action: Option<RulesAction>,
}

#[derive(Subcommand, Debug)]
pub enum RulesAction {
    /// List catalog rules, filterable by the §2.3 category axes and the §2.5
    /// attributes. `-f json` emits the shared rules.list projection.
    List {
        /// Filter by execution scope:
        /// post-parse | assembly-gate | flat-erc | declaration | viz-layout
        #[arg(long, value_name = "SCOPE")]
        scope: Option<String>,

        /// Filter by content domain (e.g. wiring, structure, electrical)
        #[arg(long, value_name = "DOMAIN")]
        domain: Option<String>,

        /// Filter by default severity: hint | info | warning | error
        #[arg(long, value_name = "SEVERITY")]
        severity: Option<String>,

        /// Filter by ownership plane: core-mechanism | domain-package | sim-fulfillment
        #[arg(long, value_name = "PLANE")]
        plane: Option<String>,

        /// Filter by gate kind: advisory | blocking
        #[arg(long, value_name = "GATE")]
        gate: Option<String>,

        /// Filter suppressible rows only (true) or non-suppressible (false)
        #[arg(long, value_name = "BOOL")]
        overridable: Option<String>,

        /// Filter by fix kind: none | quick-fix | suggestion
        #[arg(long, value_name = "FIX")]
        fix: Option<String>,
    },

    /// Show one rule's full descriptor, its §8-5 override audit (configured
    /// severity / allow / accept rows per layer) and the allow syntax
    Detail {
        /// Rule code, e.g. E4101 or 4101
        code: String,
    },

    /// Set a severity override for one rule. Session-only unless --write
    /// persists it into the project `[config] diag.severities`.
    SetSeverity {
        /// Rule code, e.g. E4101 or 4101
        code: String,

        /// Severity to apply: hint | info | warning | error
        severity: String,

        /// Persist into the project project.toml `[config]` diag zone
        #[arg(long)]
        write: bool,
    },

    /// Add an allow (suppression) row for one rule. Session-only unless
    /// --write persists it into the project `[config] diag.allows`.
    Allow {
        /// Rule code, e.g. E4101 or 4101
        code: String,

        /// Path scope: project-relative file, directory prefix or glob.
        /// Omit for the project global.
        #[arg(long, value_name = "PATH")]
        path: Option<String>,

        /// Documented exception note
        #[arg(long, value_name = "TEXT")]
        reason: Option<String>,

        /// Persist into the project project.toml `[config]` diag zone
        #[arg(long)]
        write: bool,
    },

    /// Add an accept (waiver) row for one rule. Session-only unless --write
    /// persists it into the project `[config] diag.accepts`.
    Accept {
        /// Rule code, e.g. E4101 or 4101
        code: String,

        /// Path scope: project-relative file, directory prefix or glob.
        /// Omit for the project global.
        #[arg(long, value_name = "PATH")]
        path: Option<String>,

        /// When the waiver started, e.g. 2026-09-05
        #[arg(long, value_name = "DATE")]
        since: Option<String>,

        /// Persist into the project project.toml `[config]` diag zone
        #[arg(long)]
        write: bool,
    },
}
