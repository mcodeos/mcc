// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! MCC configuration management
//!
//! Configuration hierarchy:
//! 1. Global configuration `~/.mcode/config/mcc.yaml`
//! 2. Project configuration `project.toml` section `[config]`
//!
//! Priority: Project > Global

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex as StdMutex, RwLock};

// Use LazyLock to ensure only one initialization and global sharing
static RUNTIME_TRACE: LazyLock<RwLock<TraceConfig>, fn() -> RwLock<TraceConfig>> =
    LazyLock::new(|| RwLock::new(TraceConfig::default()));

pub fn get_runtime_trace() -> &'static RwLock<TraceConfig> {
    &RUNTIME_TRACE
}

static SYSTEM_LIB_LOADING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn is_system_lib_loading() -> bool {
    SYSTEM_LIB_LOADING.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn set_system_lib_loading(loading: bool) {
    SYSTEM_LIB_LOADING.store(loading, std::sync::atomic::Ordering::SeqCst);
}

/// When true, engine-level stdout traces (e.g. AST visit tree) are suppressed even if
/// `trace.visit` is configured on. Set by CLI commands emitting a structured JSON result
/// on stdout, so a globally-enabled `trace.visit` can't corrupt the JSON contract.
static SUPPRESS_TRACE_STDOUT: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn is_trace_stdout_suppressed() -> bool {
    SUPPRESS_TRACE_STDOUT.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn set_trace_stdout_suppressed(suppress: bool) {
    SUPPRESS_TRACE_STDOUT.store(suppress, std::sync::atomic::Ordering::SeqCst);
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MccConfig {
    #[serde(default)]
    pub trace: TraceConfig,

    #[serde(default)]
    pub parser: ParserConfig,

    #[serde(default)]
    pub output: OutputConfig,

    #[serde(default)]
    pub libs: LibsConfig,

    #[serde(default)]
    pub diag: DiagConfig,
}

/// Diagnostic rendering configuration.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DiagConfig {
    /// Warning diagnostic codes to suppress from output, e.g. `["E3137"]`.
    /// Mirrored by the CLI `-i/--ignore` flag (CLI merges over config).
    #[serde(default)]
    pub ignore_warnings: Vec<String>,

    /// Rule severity overrides (rule-registry design §8-5, zone 1), keyed by
    /// the stable rule code string `"E4101"`/`"4101"`. A value takes effect
    /// only when the descriptor grants `overridable = true`.
    #[serde(default)]
    pub severities: BTreeMap<String, String>,

    /// Explicit suppressions (§8-5 zone 2). Each row suppresses the rule when
    /// `path` matches; an omitted `path` is the project global.
    #[serde(default)]
    pub allows: Vec<AllowRow>,

    /// On-file waivers (§8-5 zone 3): still displayed and counted, recorded
    /// for audit/gate exemption. v1 has no gate consumer.
    #[serde(default)]
    pub accepts: Vec<AcceptRow>,
}

/// One `diag.allows` row (`rule` + optional `path`/`reason`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AllowRow {
    /// Stable rule code string, e.g. `"E4101"`.
    pub rule: String,
    /// Project-relative path / directory prefix / glob. Omitted = project
    /// global.
    #[serde(default)]
    pub path: Option<String>,
    /// Documented exception note.
    #[serde(default)]
    pub reason: Option<String>,
}

/// One `diag.accepts` row (`rule` + optional `path`/`since`).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AcceptRow {
    /// Stable rule code string, e.g. `"E3101"`.
    pub rule: String,
    /// Project-relative path / directory prefix / glob. Omitted = project
    /// global.
    #[serde(default)]
    pub path: Option<String>,
    /// When the waiver started, e.g. `"2026-09-05"`.
    #[serde(default)]
    pub since: Option<String>,
}

impl DiagConfig {
    /// Project this config's three override zones into one override-store
    /// layer. Rows whose code is unparseable or whose severity string is
    /// unknown are skipped: config is advisory, and the adjudicator is what
    /// enforces `overridable` on the actual catalog rules.
    pub fn to_override_layer(&self) -> crate::db::diagnostic::override_store::Layer {
        use crate::db::diagnostic::override_store::{
            parse_path_scope, parse_rule_code, AcceptEntry, AllowEntry, Layer,
        };
        use crate::semantic::validation::CheckSeverity;
        let severities = self
            .severities
            .iter()
            .filter_map(|(k, v)| {
                let code = parse_rule_code(k)?;
                let sev = CheckSeverity::from_str(v.trim())?;
                Some((code, sev))
            })
            .collect();
        let allows = self
            .allows
            .iter()
            .filter_map(|r| {
                let code = parse_rule_code(&r.rule)?;
                Some(AllowEntry {
                    code,
                    path: parse_path_scope(r.path.as_deref().unwrap_or("")),
                    reason: r.reason.clone(),
                })
            })
            .collect();
        let accepts = self
            .accepts
            .iter()
            .filter_map(|r| {
                let code = parse_rule_code(&r.rule)?;
                Some(AcceptEntry {
                    code,
                    path: parse_path_scope(r.path.as_deref().unwrap_or("")),
                    since: r.since.clone(),
                })
            })
            .collect();
        Layer {
            severities,
            allows,
            accepts,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct TraceConfig {
    #[serde(default)]
    pub enabled: Option<bool>,

    #[serde(default)]
    pub ast: Option<bool>,

    #[serde(default)]
    pub lexer: Option<bool>,

    #[serde(default)]
    pub parser: Option<bool>,

    #[serde(default)]
    pub visit: Option<bool>,

    /// Base tracing level (off | error | warn | info | debug | trace).
    /// Overridden by CLI `-v`/`-q`.
    #[serde(default)]
    pub level: Option<String>,

    /// Per-target level overrides: `"mcc::sem::fcall" = "debug"`.
    /// Aliases (pass1, pass2, …) are NOT resolved here — use `resolve_debug_targets`.
    #[serde(default)]
    pub targets: HashMap<String, String>,
}

impl TraceConfig {
    pub fn has_any_value(&self) -> bool {
        self.enabled.is_some()
            || self.ast.is_some()
            || self.lexer.is_some()
            || self.parser.is_some()
            || self.visit.is_some()
            || self.level.is_some()
            || !self.targets.is_empty()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }

    pub fn is_ast(&self) -> bool {
        self.ast.unwrap_or(false)
    }

    pub fn is_lexer(&self) -> bool {
        self.lexer.unwrap_or(false)
    }

    pub fn is_parser(&self) -> bool {
        self.parser.unwrap_or(false)
    }

    pub fn is_visit(&self) -> bool {
        self.visit.unwrap_or(false)
    }

    pub fn get_flag(&self) -> u8 {
        // Correspond to the C code common.h
        // MCC_LOG_TOKEN = (1 << 0) = 0x01
        // MCC_LOG_SEM = (1 << 1) = 0x02
        // MCC_LOG_AST = (1 << 2) = 0x04
        // MCC_LOG_VISIT = (1 << 3) = 0x08
        // MCC_LOG_ERROR = (1 << 4) = 0x10
        let mut flag = 0u8;
        if self.is_enabled() {
            flag = 0xFF; // enabled all logs
        }
        if self.is_ast() {
            flag |= 0x04; // MCC_LOG_AST
        } else if self.enabled.is_some() {
            flag &= !0x04; // enabled=true but ast=false, exclude
        }
        if self.is_lexer() {
            flag |= 0x01; // MCC_LOG_TOKEN
        } else if self.enabled.is_some() {
            flag &= !0x01; // enabled=true but lexer=false, exclude
        }
        if self.is_parser() {
            flag |= 0x02; // MCC_LOG_SEM
        } else if self.enabled.is_some() {
            flag &= !0x02; // enabled=true but parser=false, exclude
        }
        if self.is_visit() {
            flag |= 0x08; // MCC_LOG_VISIT
        } else if self.enabled.is_some() {
            flag &= !0x08; // enabled=true but visit=false, exclude
        }
        flag
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ParserConfig {
    #[serde(default)]
    pub max_depth: Option<usize>,

    #[serde(default)]
    pub strict: Option<bool>,
}

impl ParserConfig {
    pub fn get_max_depth(&self) -> usize {
        self.max_depth.unwrap_or(0)
    }

    pub fn is_strict(&self) -> bool {
        self.strict.unwrap_or(false)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct OutputConfig {
    #[serde(default)]
    pub format: Option<String>,

    #[serde(default)]
    pub color: Option<bool>,
}

impl OutputConfig {
    pub fn get_format(&self) -> String {
        self.format.clone().unwrap_or_else(|| "text".to_string())
    }

    pub fn is_color(&self) -> bool {
        self.color.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LibsConfig {
    /// List of additional system libraries to load on top of mcode.
    /// Example: ["mcode"] or ["mcode", "/path/to/custom_lib"]
    #[serde(default)]
    pub load: Vec<String>,
    /// Set true to disable the default auto-loading of the mcode standard library.
    /// mcode is loaded by default in every mode; this flag is the only opt-out.
    /// None means unset (project config falls back to global config).
    #[serde(default)]
    pub disable_mcode: Option<bool>,
}

impl LibsConfig {
    /// Check if the mcode standard library should be loaded.
    /// Returns false only when `disable_mcode` is explicitly true (default is true).
    pub fn should_load_mcode(&self) -> bool {
        !self.disable_mcode.unwrap_or(false)
    }

    /// Get the list of libraries to load.
    pub fn get_load_list(&self) -> &[String] {
        &self.load
    }
}

fn default_config_path() -> PathBuf {
    crate::cli::datadir::config_dir().join("mcc.yaml")
}

pub fn global_config_path() -> PathBuf {
    default_config_path()
}

pub fn load_global_config() -> Result<MccConfig> {
    let path = global_config_path();

    if !path.exists() {
        return Ok(MccConfig::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: MccConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Invalid config file format: {}", path.display()))?;

    Ok(config)
}

pub fn save_global_config(config: &MccConfig) -> Result<()> {
    let path = global_config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = serde_yaml::to_string(config)?;
    fs::write(&path, content)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    Ok(())
}

pub fn load_project_config(project_root: &Path) -> Result<Option<MccConfig>> {
    let Some(path) = crate::cli::datadir::find_manifest_in(project_root) else {
        return Ok(None);
    };

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read project config: {}", path.display()))?;

    let toml: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Invalid project config format: {}", path.display()))?;

    if let Some(config_table) = toml.get("config").and_then(|v| v.as_table()) {
        let config: MccConfig = serde_json::to_string(&config_table)
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_default();
        return Ok(Some(config));
    }

    Ok(None)
}

/// Merge a modified `diag` zone into the project config file
/// (`project.toml` `[config]`), replacing only the `diag` subsection and
/// leaving every other section/zone untouched (design §8-5 persistence
/// discipline: merge, never overwrite). Returns the manifest path written.
///
/// This is the only writer of the project `diag` zones — `mcc config set
/// diag.*` refuses, and the `mcc rules ... --write` entry delegates here.
pub fn save_project_diag_config(project_root: &Path, diag: &DiagConfig) -> Result<PathBuf> {
    use std::io::Write;
    let manifest = crate::cli::datadir::find_manifest_in(project_root)
        .ok_or_else(|| anyhow::anyhow!(
            "no project.toml at {}; `mcc rules ... --write` persists into the project config (rule-registry design §8-5)",
            project_root.display()
        ))?;
    let content = fs::read_to_string(&manifest)
        .with_context(|| format!("Failed to read project config: {}", manifest.display()))?;
    let mut doc: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Invalid project config format: {}", manifest.display()))?;
    // Ensure `[config]` exists, then replace its `diag` subsection only
    // (toml::Value table keys are inserted through `as_table_mut`, never
    // through index assignment, which panics on a missing key).
    if !doc.get("config").map(|v| v.is_table()).unwrap_or(false) {
        if let Some(root) = doc.as_table_mut() {
            root.insert("config".into(), toml::Value::Table(toml::map::Map::new()));
        }
    }
    let diag_value = toml::Value::try_from(diag)
        .with_context(|| "Failed to serialize diag config zone".to_string())?;
    if let Some(config) = doc.get_mut("config").and_then(|v| v.as_table_mut()) {
        config.insert("diag".into(), diag_value);
    }
    let out = toml::to_string(&doc)
        .with_context(|| format!("Failed to serialize project config: {}", manifest.display()))?;
    let mut file = fs::File::create(&manifest)
        .with_context(|| format!("Failed to open project config: {}", manifest.display()))?;
    file.write_all(out.as_bytes())
        .with_context(|| format!("Failed to write project config: {}", manifest.display()))?;
    Ok(manifest)
}

/// Load just the project `[config]` diag zone (if any) — the read twin of
/// [`save_project_diag_config`].
pub fn load_project_diag_config(project_root: &Path) -> Result<Option<DiagConfig>> {
    Ok(load_project_config(project_root)?.map(|c| c.diag))
}

pub fn merge_configs(global: &MccConfig, local: Option<&MccConfig>) -> MccConfig {
    match local {
        Some(local) => {
            let trace = TraceConfig {
                enabled: local.trace.enabled.or(global.trace.enabled),
                ast: local.trace.ast.or(global.trace.ast),
                lexer: local.trace.lexer.or(global.trace.lexer),
                parser: local.trace.parser.or(global.trace.parser),
                visit: local.trace.visit.or(global.trace.visit),
                level: local.trace.level.clone().or(global.trace.level.clone()),
                targets: {
                    let mut merged = global.trace.targets.clone();
                    for (k, v) in &local.trace.targets {
                        merged.insert(k.clone(), v.clone());
                    }
                    merged
                },
            };

            let parser = ParserConfig {
                max_depth: local.parser.max_depth.or(global.parser.max_depth),
                strict: local.parser.strict.or(global.parser.strict),
            };

            let output = OutputConfig {
                format: local.output.format.clone().or(global.output.format.clone()),
                color: local.output.color.or(global.output.color),
            };

            let libs = LibsConfig {
                load: if local.libs.load.is_empty() {
                    global.libs.load.clone()
                } else {
                    local.libs.load.clone()
                },
                disable_mcode: local.libs.disable_mcode.or(global.libs.disable_mcode),
            };

            let diag = DiagConfig {
                ignore_warnings: if local.diag.ignore_warnings.is_empty() {
                    global.diag.ignore_warnings.clone()
                } else {
                    local.diag.ignore_warnings.clone()
                },
                severities: if local.diag.severities.is_empty() {
                    global.diag.severities.clone()
                } else {
                    local.diag.severities.clone()
                },
                allows: if local.diag.allows.is_empty() {
                    global.diag.allows.clone()
                } else {
                    local.diag.allows.clone()
                },
                accepts: if local.diag.accepts.is_empty() {
                    global.diag.accepts.clone()
                } else {
                    local.diag.accepts.clone()
                },
            };

            MccConfig {
                trace,
                parser,
                output,
                libs,
                diag,
            }
        }
        None => global.clone(),
    }
}

pub fn get_trace_flag(project_root: Option<&Path>) -> u8 {
    if is_system_lib_loading() {
        return 0;
    }

    if let Some(flag) = get_runtime_trace_flag() {
        return flag;
    }

    let global = load_global_config().unwrap_or_default();

    let local = project_root.and_then(|p| load_project_config(p).ok().flatten());

    let merged = merge_configs(&global, local.as_ref());
    merged.trace.get_flag()
}

pub fn get_trace_enabled() -> Option<bool> {
    get_runtime_trace().read().ok()?.enabled
}

pub fn set_trace_enabled(value: bool) {
    if let Ok(mut trace) = get_runtime_trace().write() {
        trace.enabled = Some(value);
    }
    if let Err(e) = save_trace_config_to_file() {
        eprintln!("Warning: Failed to save trace config to file: {e}");
    }
}

pub fn get_trace_ast() -> Option<bool> {
    get_runtime_trace().read().ok()?.ast
}

pub fn set_trace_ast(value: bool) {
    if let Ok(mut trace) = get_runtime_trace().write() {
        trace.ast = Some(value);
    }
    if let Err(e) = save_trace_config_to_file() {
        eprintln!("Warning: Failed to save trace config to file: {e}");
    }
}

pub fn get_trace_lexer() -> Option<bool> {
    get_runtime_trace().read().ok()?.lexer
}

pub fn set_trace_lexer(value: bool) {
    if let Ok(mut trace) = get_runtime_trace().write() {
        trace.lexer = Some(value);
    }
    if let Err(e) = save_trace_config_to_file() {
        eprintln!("Warning: Failed to save trace config to file: {e}");
    }
}

pub fn get_trace_parser() -> Option<bool> {
    get_runtime_trace().read().ok()?.parser
}

pub fn set_trace_parser(value: bool) {
    if let Ok(mut trace) = get_runtime_trace().write() {
        trace.parser = Some(value);
    }
    if let Err(e) = save_trace_config_to_file() {
        eprintln!("Warning: Failed to save trace config to file: {e}");
    }
}

pub fn get_trace_visit() -> Option<bool> {
    get_runtime_trace().read().ok()?.visit
}

pub fn set_trace_visit(value: bool) {
    if let Ok(mut trace) = get_runtime_trace().write() {
        trace.visit = Some(value);
    }
    if let Err(e) = save_trace_config_to_file() {
        eprintln!("Warning: Failed to save trace config to file: {e}");
    }
}

/// Save runtime trace config to global config file
fn save_trace_config_to_file() -> Result<()> {
    let trace = get_runtime_trace()
        .read()
        .ok()
        .ok_or_else(|| anyhow::anyhow!("Failed to read trace config"))?;

    let mut config = load_global_config().unwrap_or_default();
    config.trace = trace.clone();

    save_global_config(&config)?;
    Ok(())
}

pub fn get_runtime_trace_flag() -> Option<u8> {
    get_runtime_trace().read().ok().and_then(|t| {
        if t.has_any_value() {
            Some(t.get_flag())
        } else {
            None
        }
    })
}

// ============================================================================
// Rust log three-way switch runtime state + reload callback bridge
//   reload handle lives in the binary's logging.rs, the lib side only stores "apply callback", registered by the binary.
// ============================================================================

#[derive(Clone)]
struct LogStreams {
    server_level: String,
    pass1: bool,
    pass2: bool,
}
impl Default for LogStreams {
    fn default() -> Self {
        Self {
            server_level: "info".into(),
            pass1: false,
            pass2: false,
        }
    }
}

static LOG_STREAMS: LazyLock<RwLock<LogStreams>, fn() -> RwLock<LogStreams>> =
    LazyLock::new(|| {
        RwLock::new(LogStreams {
            server_level: "info".into(),
            pass1: false,
            pass2: false,
        })
    });

fn get_log_streams() -> &'static RwLock<LogStreams> {
    &LOG_STREAMS
}

type LogApplier = Box<dyn Fn(&str, bool, bool) + Send + Sync>;
static mut LOG_APPLIER: Option<LogApplier> = None;

/// Registered by the binary after log initialization: applies (server_level, pass1, pass2) to reload filter.
pub fn set_log_stream_applier(f: LogApplier) {
    unsafe {
        LOG_APPLIER = Some(f);
    }
}

#[allow(static_mut_refs)]
fn apply_log_streams() {
    if let Ok(s) = get_log_streams().read() {
        if let Some(ref f) = unsafe { LOG_APPLIER.as_ref() } {
            f(&s.server_level, s.pass1, s.pass2);
        }
    }
}

pub fn set_log_server(on: bool) {
    if let Ok(mut s) = get_log_streams().write() {
        s.server_level = if on { "info".into() } else { "warn".into() };
    }
    apply_log_streams();
}
pub fn get_log_server() -> Option<bool> {
    get_log_streams()
        .read()
        .ok()
        .map(|s| s.server_level == "info")
}
pub fn set_log_pass1(on: bool) {
    if let Ok(mut s) = get_log_streams().write() {
        s.pass1 = on;
    }
    apply_log_streams();
}
pub fn get_log_pass1() -> Option<bool> {
    get_log_streams().read().ok().map(|s| s.pass1)
}
pub fn set_log_pass2(on: bool) {
    if let Ok(mut s) = get_log_streams().write() {
        s.pass2 = on;
    }
    apply_log_streams();
}
pub fn get_log_pass2() -> Option<bool> {
    get_log_streams().read().ok().map(|s| s.pass2)
}

/// Check if the mcode standard library should be loaded based on config.
/// Returns true by default; only `libs.disable_mcode: true` disables it.
pub fn should_load_mcode(project_root: Option<&Path>) -> bool {
    let global = load_global_config().unwrap_or_default();
    let local = project_root.and_then(|p| load_project_config(p).ok().flatten());
    let merged = merge_configs(&global, local.as_ref());
    merged.libs.should_load_mcode()
}

/// Get the list of libraries to load from config.
pub fn get_libs_load_list(project_root: Option<&Path>) -> Vec<String> {
    let global = load_global_config().unwrap_or_default();
    let local = project_root.and_then(|p| load_project_config(p).ok().flatten());
    let merged = merge_configs(&global, local.as_ref());
    merged.libs.get_load_list().to_vec()
}

// ============================================================================
// Debug target aliases & resolution
// ============================================================================

/// Known debug-target aliases.
/// Each alias expands to one or more tracing targets.
static DEBUG_ALIASES: &[(&str, &[&str])] = &[
    ("pass1", &["mcc::parse::*", "mcc::sem::*"]),
    ("pass2", &["mcc::inst::*"]),
    ("fcall", &["mcc::sem::fcall", "mcc::inst::fcall"]),
    ("lapper", &["mcc::sem::class", "mcc::lsp::lapper"]),
    ("vec", &["mcc::vec"]),
    ("viz", &["mcc::viz"]),
    ("lsp", &["mcc::lsp::*"]),
    ("all", &["*"]),
];

/// Resolve CLI `-D` flags into `(target, level)` pairs.
///
/// Each raw flag has the form `target[=level]` (level defaults to `"debug"`).
/// Aliases (pass1, pass2, fcall, …) are expanded to their constituent targets.
pub fn resolve_debug_targets(raw: &[String]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();

    for raw_flag in raw {
        let (key, level) = match raw_flag.split_once('=') {
            Some((k, l)) => (k.trim(), l.trim().to_string()),
            None => (raw_flag.trim(), "debug".to_string()),
        };
        if key.is_empty() {
            continue;
        }

        // Check alias expansion
        let alias = key.to_lowercase();
        if let Some(targets) = DEBUG_ALIASES
            .iter()
            .find(|(name, _)| *name == alias)
            .map(|(_, targets)| *targets)
        {
            for t in targets {
                out.push((t.to_string(), level.clone()));
            }
        } else {
            out.push((key.to_string(), level));
        }
    }

    out
}

/// Build the base log level string from CLI verbosity flags.
pub fn base_level(verbose: u8, quiet: bool) -> &'static str {
    if quiet {
        "error"
    } else {
        match verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    }
}

// ============================================================================
// Per-target debug level overrides (bridge between lib ↔ binary logging)
// ============================================================================

/// In-library storage for per-target debug overrides.
/// Mirrors `logging::TARGETS` in the binary so RPC handlers can read/write.
static TARGET_OVERRIDES: LazyLock<StdMutex<HashMap<String, String>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

type TargetApplier = Box<dyn Fn(&str, &[(String, String)]) + Send + Sync>;
static mut TARGET_APPLIER: Option<TargetApplier> = None;

/// Register a callback that bridges target overrides to the binary's `logging::set_targets`.
///
/// Called by `main.rs` after `logging::init()`.
pub fn set_target_applier(f: TargetApplier) {
    unsafe {
        TARGET_APPLIER = Some(f);
    }
}

#[allow(static_mut_refs)]
fn apply_target_overrides(base_level: &str) {
    let targets: Vec<(String, String)> = TARGET_OVERRIDES
        .lock()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if let Some(ref f) = unsafe { TARGET_APPLIER.as_ref() } {
        f(base_level, &targets);
    }
}

/// Set per-target debug levels.  `targets` is a list of `(target, level)` pairs
/// where level is one of `off | error | warn | info | debug | trace`.
///
/// Aliases are NOT resolved here — the caller (`trace.set` handler or `-D` CLI)
/// must expand aliases before calling.  An empty slice clears all overrides.
pub fn set_debug_targets(base_level: &str, targets: &[(String, String)]) {
    {
        let mut map = TARGET_OVERRIDES.lock().unwrap();
        map.clear();
        for (t, l) in targets {
            map.insert(t.clone(), l.clone());
        }
    }
    apply_target_overrides(base_level);
}

/// Return all current per-target overrides (empty if none have been set).
pub fn get_debug_targets() -> HashMap<String, String> {
    TARGET_OVERRIDES.lock().unwrap().clone()
}

/// Return the list of known debug-target aliases for `server.methods` discovery.
pub fn get_debug_aliases() -> Vec<(&'static str, &'static [&'static str])> {
    DEBUG_ALIASES.iter().map(|(n, t)| (*n, *t)).collect()
}

/// Return the list of known debug targets for `server.methods` discovery.
pub fn get_known_debug_targets() -> Vec<&'static str> {
    vec![
        "mcc::parse::ast",
        "mcc::parse::phrase",
        "mcc::sem::fcall",
        "mcc::sem::conds",
        "mcc::sem::class",
        "mcc::sem::inst",
        "mcc::sem::module",
        "mcc::sem::comp",
        "mcc::inst::mod",
        "mcc::inst::comp",
        "mcc::inst::fcall",
        "mcc::inst::points",
        "mcc::inst::table",
        "mcc::inst::dump",
        "mcc::vec",
        "mcc::viz",
        "mcc::lsp::query",
        "mcc::lsp::lapper",
        "mcc::build",
        "mcc::config",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::diagnostic::override_store::{Adjudication, OverrideStore, PathScope};
    use crate::semantic::validation::CheckSeverity;

    fn sample_config() -> MccConfig {
        // The same schema serves the user-level `mcc.yaml` and the project
        // `[config]` section (project config is deserialized through the same
        // MccConfig shape).
        serde_yaml::from_str(
            r#"
diag:
  severities:
    E5155: info
    4101: warning
    not-a-code: error
    E4101x: error
  allows:
    - rule: E5155
      path: boards/**/*.mc
      reason: documented exception
    - rule: 4102
  accepts:
    - rule: E5156
      path: boards/dev/main.mc
      since: "2026-09-05"
"#,
        )
        .expect("yaml parse")
    }

    #[test]
    fn diag_zones_project_into_an_override_layer() {
        let cfg = sample_config();
        let layer = cfg.diag.to_override_layer();
        // Invalid code keys and unknown severities are skipped.
        assert_eq!(layer.severities.len(), 2);
        assert_eq!(layer.severities.get(&5155), Some(&CheckSeverity::Info));
        assert_eq!(layer.severities.get(&4101), Some(&CheckSeverity::Warning));
        assert_eq!(layer.allows.len(), 2);
        assert_eq!(layer.allows[0].code, 5155);
        assert!(matches!(layer.allows[0].path, PathScope::Directory(_)));
        assert_eq!(layer.allows[1].code, 4102);
        assert_eq!(layer.accepts.len(), 1);
        assert_eq!(layer.accepts[0].code, 5156);
        assert_eq!(layer.accepts[0].since.as_deref(), Some("2026-09-05"));
    }

    #[test]
    fn config_layer_and_store_adjudicate_together() {
        let cfg = sample_config();
        let store = OverrideStore {
            project: cfg.diag.to_override_layer(),
            ..Default::default()
        };
        // The store refuses the override while the rule is non-overridable.
        assert_eq!(
            store.adjudicate(
                5155,
                false,
                CheckSeverity::Warning,
                Some("boards/dev/main.mc")
            ),
            Adjudication::Default
        );
        // Overridable + allow hit: the path-scoped allow suppresses (allow
        // outranks the severity row when both hit).
        assert_eq!(
            store.adjudicate(
                5155,
                true,
                CheckSeverity::Warning,
                Some("boards/dev/main.mc")
            ),
            Adjudication::Suppressed
        );
        // Overridable + no allow hit: the severity row re-levels.
        assert_eq!(
            store.adjudicate(5155, true, CheckSeverity::Warning, Some("core/main.mc")),
            Adjudication::Severity(CheckSeverity::Info)
        );
        // A path the allow matches but the severity row does not cover: the
        // project-global allow row for 4102 hits any uri.
        assert_eq!(
            store.adjudicate(4102, true, CheckSeverity::Warning, Some("any/main.mc")),
            Adjudication::Suppressed
        );
    }

    #[test]
    fn merge_configs_prefers_project_diag_zones() {
        let mut global = MccConfig::default();
        global.diag.severities.insert("E4101".into(), "info".into());
        global.diag.allows.push(AllowRow {
            rule: "E5155".into(),
            path: None,
            reason: None,
        });
        let mut local = MccConfig::default();
        local.diag.severities.insert("E5155".into(), "hint".into());
        let merged = merge_configs(&global, Some(&local));
        assert_eq!(merged.diag.severities.len(), 1);
        assert_eq!(
            merged.diag.severities.get("E5155"),
            Some(&"hint".to_string())
        );
        // No local allows: the global rows carry over.
        assert_eq!(merged.diag.allows.len(), 1);
    }

    #[test]
    fn project_diag_config_roundtrips_and_preserves_other_zones() {
        // §8-5 persistence discipline: `save_project_diag_config` replaces
        // only the `[config] diag` subsection of project.toml and leaves
        // every other section/zone untouched.
        let base = std::env::temp_dir().join(format!(
            "mcc-config-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let proj = base.join("p");
        fs::create_dir_all(&proj).unwrap();
        fs::write(
            proj.join("project.toml"),
            "[project]\nname = \"t\"\n\n[config]\ntrace = { enabled = true }\n",
        )
        .unwrap();

        let mut diag = DiagConfig::default();
        diag.severities.insert("E5155".into(), "info".into());
        diag.allows.push(AllowRow {
            rule: "E4101".into(),
            path: Some("boards/**/*.mc".into()),
            reason: Some("documented exception".into()),
        });
        let written = save_project_diag_config(&proj, &diag).unwrap();
        assert_eq!(written, proj.join("project.toml"));

        let content = fs::read_to_string(&written).unwrap();
        assert!(
            content.contains("enabled = true"),
            "other zone clobbered: {content}"
        );
        assert!(content.contains("severities"), "{content}");

        let back = load_project_diag_config(&proj)
            .unwrap()
            .expect("diag zone present");
        assert_eq!(
            back.severities.get("E5155").map(|s| s.as_str()),
            Some("info")
        );
        assert_eq!(back.allows.len(), 1);
        assert_eq!(back.allows[0].path.as_deref(), Some("boards/**/*.mc"));
        assert_eq!(
            back.allows[0].reason.as_deref(),
            Some("documented exception")
        );
        assert!(back.accepts.is_empty());

        // Without a `[config]` diag zone the read twin is `None`.
        let bare = base.join("bare");
        fs::create_dir_all(&bare).unwrap();
        fs::write(bare.join("project.toml"), "[project]\nname = \"t\"\n").unwrap();
        assert!(load_project_diag_config(&bare).unwrap().is_none());

        // Writing into a directory without a manifest is an explicit error.
        let no_manifest = base.join("none");
        fs::create_dir_all(&no_manifest).unwrap();
        assert!(save_project_diag_config(&no_manifest, &DiagConfig::default()).is_err());

        fs::remove_dir_all(&base).ok();
    }
}
