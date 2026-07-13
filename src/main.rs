// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! MCC binary entry point.
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

#![allow(dead_code)]

use anyhow::Result;
use clap::Parser;
use std::env;
use std::process::ExitCode;

mod cli;
mod cmds;
mod logging;
mod output;

use cli::{Cli, Command, OutputFormat};

fn main() -> ExitCode {
    // ── 0. Internal startup command (called by start subprocess)
    let raw: Vec<String> = env::args().collect();
    if raw.len() >= 2 && raw[1] == "_server_internal" {
        return run_internal_server(&raw);
    }

    // ── 1. Parse CLI (legacy compatibility) ─────────────────────────────────
    let cli: Cli = if should_legacy_rewrite(&raw) {
        match Cli::try_parse_from(rewrite_legacy_args(&raw)) {
            Ok(c) => c,
            Err(e) => {
                e.print().ok();
                return ExitCode::from(2);
            }
        }
    } else {
        match Cli::try_parse() {
            Ok(c) => c,
            Err(e) => {
                let code = if e.use_stderr() { 2 } else { 0 };
                e.print().ok();
                return ExitCode::from(code as u8);
            }
        }
    };

    // ── 1.5 Generate completion ───────────────────────────────────────────
    if let Some(shell) = &cli.completion {
        return generate_completion(shell);
    }

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
        logging::init(cli.verbose, cli.quiet, cli.show_target);
    }

    // ── 3.5. Ensure data directory exists ─────────────────────────────────────
    if let Err(e) = cli::data_dir::ensure_dirs() {
        eprintln!("warning: Failed to create data directory: {}", e);
    }

    // ── 3.6. Load trace config from file ─────────────────────────────
    mcc::init_trace_config();

    // ── 4. Dispatch to subcommands ────────────────────────────────────────────
    let need_mcc_init = match &cli.command {
        Some(Command::Start(_)) => false,
        Some(Command::Stop(_)) => false,
        Some(Command::Status(_)) => false,
        Some(Command::Config(_)) => false,
        Some(Command::Proj(_)) => false,
        Some(Command::Show(_)) => false,
        Some(Command::Search(_)) => false,
        Some(Command::Query(_)) => false,
        Some(Command::Export(_)) => false,
        Some(Command::Parse(_)) => false,
        Some(Command::Check(_)) => false,
        Some(Command::Extract(_)) => false,
        None => false,
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
        Some(Command::Parse(a)) => Some(a.format),
        Some(Command::Check(a)) => Some(a.format),
        Some(Command::Extract(a)) => Some(a.format),
        Some(Command::Show(a)) => Some(a.format),
        Some(Command::Search(a)) => {
            if a.json {
                Some(OutputFormat::Json)
            } else {
                Some(a.format)
            }
        }
        Some(Command::Query(a)) => Some(if a.json { OutputFormat::Json } else { a.format }),
        Some(Command::Export(a)) => Some(if a.json { OutputFormat::Json } else { a.format }),
        Some(Command::Build(a)) => Some(a.format),
        _ => None,
    };
    if matches!(result_format, Some(f) if f != OutputFormat::Text) {
        mcc::set_trace_stdout_suppressed(true);
    }

    match cli.command {
        Some(Command::Parse(args)) => {
            cmds::parse::run(&args)?;
            Ok(ExitCode::SUCCESS)
        }
        Some(Command::Check(args)) => {
            let outcome = cmds::check::run(&args)?;
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
            cmds::lib::run(&args.action, args.format)?;
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
            cmds::server::run_status(&args, OutputFormat::Text)?;
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
            let result = mcc::rpc::handlers::handle_caps(None)
                .map_err(|e| anyhow::anyhow!("{e:?}"))?;
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
    eprintln!("Auto-completion:");
    eprintln!("  mcc --completion bash > /etc/bash_completion.d/mcc");
    eprintln!("  mcc --completion zsh > ~/.zsh/completions/_mcc");
    eprintln!("  mcc --completion fish > ~/.config/fish/completions/mcc.fish");
    eprintln!();
    eprintln!("Legacy form (auto-rewritten to `mcc parse`):");
    eprintln!("  mcc example.mc main --viz");
    eprintln!();
    eprintln!("Run 'mcc <COMMAND> --help' for more information.");
}

fn generate_completion(shell: &str) -> ExitCode {
    use clap::CommandFactory;
    let mut cmd = Cli::command();

    match shell.to_lowercase().as_str() {
        "bash" => {
            clap_complete::generate(
                clap_complete::Shell::Bash,
                &mut cmd,
                "mcc",
                &mut std::io::stdout(),
            );
        }
        "zsh" => {
            clap_complete::generate(
                clap_complete::Shell::Zsh,
                &mut cmd,
                "mcc",
                &mut std::io::stdout(),
            );
        }
        "fish" => {
            clap_complete::generate(
                clap_complete::Shell::Fish,
                &mut cmd,
                "mcc",
                &mut std::io::stdout(),
            );
        }
        "powershell" => {
            clap_complete::generate(
                clap_complete::Shell::PowerShell,
                &mut cmd,
                "mcc",
                &mut std::io::stdout(),
            );
        }
        _ => {
            eprintln!(
                "error: Unsupported shell type: {}. Supported types: bash, zsh, fish, powershell",
                shell
            );
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
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

    match cmds::server::run_server_internal(host, port, &libs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("server error: {}", e);
            ExitCode::FAILURE
        }
    }
}

// ============================================================================
// Legacy compatibility: `mcc <file>.mc <top> [--viz|--json|-o <path>]`
//   → Automatically rewritten to `mcc parse <file>.mc --top <top> [--viz|--json -o <path>]`
//
// Trigger condition: The first non-flag position argument ends with ".mc",
// and is not a known subcommand.
// This allows old scripts / command line history to transition seamlessly,
// while not interfering with clap's normal parsing.
// ============================================================================

const KNOWN_SUBCMDS: &[&str] = &[
    "parse",
    "check",
    "build",
    "show",
    "search",
    "query",
    "export",
    "extract",
    "lib",
    "proj",
    "start",
    "stop",
    "status",
    "config",
    "help",
    "-h",
    "--help",
    "-V",
    "--version",
];

fn should_legacy_rewrite(args: &[String]) -> bool {
    // args[0] = program name
    // find the first non-flag position argument
    for a in args.iter().skip(1) {
        if a.starts_with('-') {
            continue;
        }
        if KNOWN_SUBCMDS.contains(&a.as_str()) {
            return false;
        }
        // first position argument looks like .mc → use legacy
        return a.ends_with(".mc");
    }
    false
}

fn rewrite_legacy_args(args: &[String]) -> Vec<String> {
    // legacy position arguments: <file> [top]
    // legacy flag: --viz, --json, -o <path>
    // new usage: parse <file> [--top <top>] [--viz] [--json] [-o <path>]
    let mut out: Vec<String> = Vec::with_capacity(args.len() + 2);
    out.push(args[0].clone()); // program name
    out.push("parse".to_string());

    let mut positional: Vec<String> = Vec::new();
    let mut tail: Vec<String> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "-o" => {
                tail.push(a.clone());
                if let Some(v) = args.get(i + 1) {
                    tail.push(v.clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--viz" | "--json" => {
                tail.push(a.clone());
                i += 1;
            }
            _ if a.starts_with('-') => {
                // Pass-through other flags (--verbose / --quiet / --cwd ...)
                tail.push(a.clone());
                i += 1;
            }
            _ => {
                positional.push(a.clone());
                i += 1;
            }
        }
    }

    // positional[0] = file (required)
    if let Some(file) = positional.first() {
        out.push(file.clone());
    }
    // positional[1] = top module → --top
    if let Some(top) = positional.get(1) {
        out.push("--top".to_string());
        out.push(top.clone());
    }

    out.extend(tail);
    out
}
