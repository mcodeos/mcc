// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! MCC binary entry point.
//!
//! Many helper functions are used across cmds/ files; the compiler
//! cannot track cross-file usage within the binary crate.
#![allow(dead_code)]
//!
//! ## Initialization flow
//!
//!   1. Parse CLI (clap)
//!   2. Initialize logging (logging::init)
//!   3. Initialize data directories (ensure_dirs)
//!   4. Global initialization (mcc_init) - except start/stop
//!   5. Dispatch to subcommands (cmds::*::run)
//!
//! ## Path settings
//!
//!   - `mcc_set_system_root()`: Server startup once
//!   - `mcc_set_project_root()`: Project command once
//!
//! ## Installation
//!
//!   1. Build the project with `cargo build --release`.
//!   2. Create a symlink to the binary/bin`:
//!
//!     ```bash
//!     sudo ln -sf "$(pwd)/target/debug/mcc" /usr/local/bin/mcc
//!     ```
//!
//!     Alternatively, you can add the project directory to your `$PATH`.
//!
//!     ```bash
//!     export PATH=$PWD:$PATH
//!     ```

use anyhow::Result;
use clap::Parser;
use std::env;
use std::process::ExitCode;

mod cmds;
mod logging;
mod output;

use mcc::cli::{Cli, Command, OutputFormat};

fn main() -> ExitCode {
    // ── 0. Internal startup command (called by start subprocess)
    let raw: Vec<String> = env::args().collect();
    if raw.len() >= 2 && raw[1] == "_server_internal" {
        return run_internal_server(&raw);
    }

    // ── 1. Parse CLI ─────────────────────────────────────────────────────────
    let cli: Cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            e.print().ok();
            return ExitCode::from(code as u8);
        }
    };

    // ── 1.2 Honor global --local and store global options ────────────────
    // --local makes RpcClient::probe() return None (everything runs in-process).
    // The cross-command options (--lib / --format / --output / --top / --entry)
    // are stored once here and read by every subcommand via mcc::cli::globals().
    mcc::cli::set_local_mode(cli.local);
    mcc::cli::set_globals(mcc::cli::GlobalOptions {
        lib: cli.lib.clone(),
        format: cli.format,
        output: cli.output.clone(),
        top: cli.top.clone(),
        entry: cli.entry.clone(),
        strict: cli.strict,
    });

    // ── 2. Change working directory to (--cwd) ────────────────────────────────────
    if let Some(cwd) = &cli.cwd {
        if let Err(e) = env::set_current_dir(cwd) {
            eprintln!("error: Failed to change to directory {:?}: {}", cwd, e);
            return ExitCode::FAILURE;
        }
    }

    // ── 3. Initialize logging (before mcc_init) ──────────────────
    // Further: Some commands communicate via RPC with server, so we need logging.
    let need_logging = match &cli.command {
        // `start` initializes logging itself (foreground mode installs a
        // `--log-file` writer via `init_with_log_file_and_stderr`; background
        // mode lets the `_server_internal` child initialize its own). Eagerly
        // calling `logging::init` here would set `ALREADY_INIT`, causing the
        // file writer to be skipped and leaving `--log-file` empty.
        Some(Command::Start(_)) => false,
        Some(Command::Stop(_)) => false,
        Some(Command::Status(_)) => false,
        Some(Command::Config(_)) => false,
        Some(Command::Proj(_)) => false,
        Some(Command::Explain(_)) => false,
        Some(Command::Caps) => false,
        Some(Command::Def(_)) => false,
        Some(Command::Erc(_)) => false,
        Some(Command::Refs(_)) => false,
        Some(Command::Convert(_)) => false,
        Some(Command::Report(_)) => false,
        _ => true,
    };
    if need_logging {
        logging::init(cli.verbose, cli.quiet, cli.origin);

        // Bridge: notify logging layer when RPC trace.set changes per-target overrides.
        mcc::cli::config::set_target_applier(Box::new(|base, targets| {
            logging::set_targets(base, targets);
        }));
    }

    // ── 3.5. Ensure data directory exists ─────────────────────────────────────
    if let Err(e) = mcc::cli::datadir::ensure_dirs() {
        eprintln!("warning: Failed to create data directory: {}", e);
    }

    // ── 3.6. Load trace config from file (global + project) ──────────
    // Apply the file-configured level/targets only when the user explicitly
    // asked for debug output (`-v` / `-D`); otherwise the file's `trace.level:
    // debug` (or per-target overrides) would bury ordinary CLI results under
    // INFO/DEBUG logs. `load_trace_config` still updates runtime state so
    // `trace.get` queries and later `-D` merges see the file config.
    let project_root = std::env::current_dir().ok();
    if cli.verbose > 0 || !cli.debug_targets.is_empty() {
        mcc::init_trace_config(project_root.as_deref());
    } else {
        mcc::load_trace_config(project_root.as_deref());
    }

    // ── 3.7. Apply -D debug-target flags (CLI > config file) ─────────
    if !cli.debug_targets.is_empty() {
        let base = mcc::cli::config::base_level(cli.verbose, cli.quiet);

        // Start from config-file targets (set by init_trace_config), then
        // override with CLI -D flags.
        let mut merged = mcc::cli::config::get_debug_targets();
        for (t, l) in mcc::cli::config::resolve_debug_targets(&cli.debug_targets) {
            merged.insert(t, l);
        }

        let targets: Vec<(String, String)> = merged.into_iter().collect();
        logging::set_targets(base, &targets);
        // Keep the lib-side store in sync so trace.get RPC sees the same state.
        mcc::cli::config::set_debug_targets(base, &targets);
    }

    // ── 4. Dispatch to subcommands ────────────────────────────────────────────
    // Commands that self-initialize via `init_local` (mcc_init_no_lib + libs)
    // or a manual `mcc_init_no_lib` stay conservative here and are NOT eagerly
    // initialized. `mcc_init()` now resolves the system root once (data root)
    // before loading mcode, so eager init would be consistent — but these
    // commands control their own lib loading and are left untouched.
    let need_mcc_init = match &cli.command {
        Some(Command::Start(_)) | Some(Command::Stop(_)) | Some(Command::Status(_)) => false,
        Some(Command::Config(_)) | Some(Command::Proj(_)) => false,
        Some(Command::Show(_))
        | Some(Command::List(_))
        | Some(Command::Search(_))
        | Some(Command::Query(_)) => false,
        Some(Command::Export(_)) => false,
        Some(Command::Parse(_)) | Some(Command::Check(_)) | Some(Command::Extract(_)) => false,
        Some(Command::Verify(_)) => false,
        Some(Command::Build(_)) | Some(Command::Def(_)) | Some(Command::Erc(_)) => false,
        Some(Command::Refs(_)) | Some(Command::Convert(_)) | Some(Command::Report(_)) => false,
        None => false,
        // Lib / Explain / Caps and any future command keep the conservative
        // full initialization.
        _ => true,
    };

    if need_mcc_init {
        mcc::mcc_init();
    }

    match dispatch(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::FAILURE
        }
    }
}

fn dispatch(cli: Cli) -> Result<ExitCode> {
    // Suppress engine-level stdout traces (e.g. AST visit tree from `trace.visit`) for
    // commands that emit a structured JSON result on stdout, so a globally-enabled
    // `trace.visit` can't corrupt the JSON contract.
    let result_format = match &cli.command {
        Some(Command::Parse(_))
        | Some(Command::Check(_))
        | Some(Command::Extract(_))
        | Some(Command::Show(_))
        | Some(Command::List(_))
        | Some(Command::Build(_))
        | Some(Command::Verify(_)) => Some(mcc::cli::globals().format),
        Some(Command::Search(a)) => {
            if a.json {
                Some(OutputFormat::Json)
            } else {
                Some(mcc::cli::globals().format)
            }
        }
        Some(Command::Query(a)) => Some(if a.json {
            OutputFormat::Json
        } else {
            mcc::cli::globals().format
        }),
        Some(Command::Export(a)) => Some(if a.json {
            OutputFormat::Json
        } else {
            mcc::cli::globals().format
        }),
        _ => None,
    };
    if matches!(result_format, Some(f) if f != OutputFormat::Text) {
        mcc::set_trace_stdout_suppressed(true);
    }

    match cli.command {
        Some(Command::Parse(args)) => {
            if args.dlog {
                // --dlog: suppress engine trace so only dlog diagnostics appear on stdout
                mcc::set_trace_stdout_suppressed(true);
            }
            cmds::parse::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Check(args)) => {
            if args.dlog {
                // --dlog: suppress engine trace so only dlog diagnostics appear on stdout
                mcc::set_trace_stdout_suppressed(true);
            }
            let outcome = cmds::check::run(&args)?;
            Ok(ExitCode::from(outcome.exit_code.clamp(0, 255) as u8))
        }
        Some(Command::Verify(args)) => {
            let outcome = cmds::verify::run(&args)?;
            Ok(ExitCode::from(outcome.exit_code.clamp(0, 255) as u8))
        }
        Some(Command::Extract(args)) => {
            cmds::extract::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Show(args)) => {
            cmds::show::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::List(args)) => {
            cmds::list::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Search(args)) => {
            cmds::search::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Query(args)) => {
            cmds::query::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Export(args)) => {
            cmds::export::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Build(args)) => {
            let o = cmds::build::run(&args)?;
            Ok(ExitCode::from(o.exit_code.clamp(0, 255) as u8))
        }
        Some(Command::Lib(args)) => {
            cmds::lib::run(&args.action, mcc::cli::globals().format)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Proj(args)) => {
            cmds::proj::run(&args.action)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Start(args)) => {
            cmds::server::run_start(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Stop(args)) => {
            cmds::server::run_stop(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Status(args)) => {
            cmds::server::run_status(&args, mcc::cli::globals().format)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Config(args)) => {
            cmds::config::run(&args.action)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Explain(args)) => {
            cmds::explain::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Def(args)) => {
            cmds::def::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Erc(args)) => {
            cmds::erc::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Refs(args)) => {
            cmds::refs::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Convert(args)) => {
            mcc::set_trace_stdout_suppressed(true);
            cmds::convert::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Report(args)) => {
            mcc::set_trace_stdout_suppressed(true);
            cmds::report::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Caps) => {
            // Capabilities is self-describing; call the handler directly.
            let result =
                mcc::rpc::handlers::handle_caps(None).map_err(|e| anyhow::anyhow!("{e:?}"))?;
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(ExitCode::SUCCESS)
        }
        None => {
            print_help_hint();
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn print_help_hint() {
    eprintln!("Usage: mcc <COMMAND> [OPTIONS]");
    eprintln!();
    eprintln!("Commands:");
    eprintln!("  parse    Parse code fragment/file/project directory (Pass1 + Pass2)");
    eprintln!("  check    Syntax check, output diagnostics");
    eprintln!("  build    Manifest-driven build");
    eprintln!("  show     Show component / module / interface / net / file details");
    eprintln!("  list     List top-level definition names (component / module / interface / enum / nets / ports / files / all)");
    eprintln!("  extract  Extract instances/netlist/components/interfaces");
    eprintln!("  search   Search across loaded definitions (text/regex/fuzzy)");
    eprintln!("  query    Structured DSL query (operators, AND/OR/NOT, attr())");
    eprintln!("  export   Export netlist / BOM / SPICE (text|csv|json)");
    eprintln!(
        "  lib      System library management (list / install / load / unload / info / search)"
    );
    eprintln!("  proj     Project scaffolding (create)");
    eprintln!("  start    Start server");
    eprintln!("  stop     Stop server");
    eprintln!("  status   View server status");
    eprintln!("  config   Configuration management (get / set / list / reset)");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  mcc parse example.mc");
    eprintln!("  mcc parse example.mc --top main --viz");
    eprintln!("  mcc parse --code 'V3V3 -> RES(10k) -> GND'");
    eprintln!("  mcc parse ./my-project --top main");
    eprintln!("  mcc parse --lib mcode example.mc");
    eprintln!("  mcc build");
    eprintln!("  mcc build --top main");
    eprintln!();
    eprintln!("Config examples:");
    eprintln!("  mcc config list");
    eprintln!("  mcc config get trace.enabled");
    eprintln!("  mcc config set trace.enabled true");
    eprintln!("  mcc config set trace.ast true");
    eprintln!("  mcc config reset");
    eprintln!();
    eprintln!("Server commands:");
    eprintln!("  mcc start");
    eprintln!("  mcc start --port 9090 --background");
    eprintln!("  mcc start --lib mcode");
    eprintln!("  mcc status");
    eprintln!("  mcc stop");
    eprintln!();
    eprintln!("Run 'mcc <COMMAND> --help' for more information.");
}

fn run_internal_server(args: &[String]) -> ExitCode {
    let mut host = "127.0.0.1";
    let mut port: u16 = 8080;
    let mut log_file: Option<String> = None;
    let mut libs: Vec<String> = Vec::new();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--host" if i + 1 < args.len() => {
                host = &args[i + 1];
                i += 2;
            }
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse().unwrap_or(8080);
                i += 2;
            }
            "--log-file" if i + 1 < args.len() => {
                log_file = Some(args[i + 1].clone());
                i += 2;
            }
            "--lib" if i + 1 < args.len() => {
                libs.push(args[i + 1].clone());
                i += 2;
            }
            _ => i += 1,
        }
    }

    logging::init_with_log_file(0, true, log_file.as_deref(), false);

    // Register reload callback to enable real-time effect
    mcc::set_log_stream_applier(Box::new(|server, pass1, pass2| {
        logging::set_streams(server, pass1, pass2);
    }));

    // Bridge: notify logging layer when RPC trace.set changes per-target overrides.
    mcc::cli::config::set_target_applier(Box::new(|base, targets| {
        logging::set_targets(base, targets);
    }));

    match cmds::server::run_server_internal(host, port, &libs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("server error: {}", e);
            ExitCode::FAILURE
        }
    }
}
