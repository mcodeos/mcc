// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc server` / `mcc start` / `mcc stop` / `mcc status` — Iteration B
//!
//! Key changes:
//!   - At startup, load mcode system library once (autoload)
//!   - Register all project / lib RPC methods
//!   - Daemon process keeps running, multiple clients can call concurrently

use crate::cli::{data_dir, server_config, OutputFormat, StartArgs, StatusArgs, StopArgs};
use crate::output::{self, OutputFormatExt};
use anyhow::{Context, Result};
use mcc::rpc::{handlers, RpcServerBuilder};
use serde::Serialize;
use std::fmt;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::thread;
use std::time::{Duration, Instant};
use tracing::info;

// ============================================================================
// Status report
// ============================================================================

#[derive(Serialize)]
struct ServerStatus {
    running: bool,
    pid: Option<u32>,
    host: Option<String>,
    port: Option<u16>,
    uptime: Option<String>,
}

impl fmt::Display for ServerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.running {
            writeln!(f, "Server status: Running")?;
            if let Some(pid) = self.pid {
                writeln!(f, "PID: {}", pid)?;
            }
            if let Some(host) = &self.host {
                writeln!(f, "Host: {}", host)?;
            }
            if let Some(port) = self.port {
                writeln!(f, "Port: {}", port)?;
            }
            if let Some(uptime) = &self.uptime {
                writeln!(f, "Uptime: {}", uptime)?;
            }
        } else {
            writeln!(f, "Server status: Not running")?;
        }
        Ok(())
    }
}

impl ServerStatus {
    fn not_running() -> Self {
        Self {
            running: false,
            pid: None,
            host: None,
            port: None,
            uptime: None,
        }
    }
    fn running(pid: u32, host: Option<String>, port: Option<u16>) -> Self {
        Self {
            running: true,
            pid: Some(pid),
            host,
            port,
            uptime: None,
        }
    }
}

// ============================================================================
// Dispatch subcommands for server
// ============================================================================

pub fn run_start(args: &StartArgs) -> Result<()> {
    let config = server_config::load_config().ok();
    let config_host = config.as_ref().and_then(|c| {
        let h = &c.server.host;
        if h.is_empty() {
            None
        } else {
            Some(h.clone())
        }
    });
    let config_port = config.as_ref().and_then(|c| {
        if c.server.port == 0 {
            None
        } else {
            Some(c.server.port)
        }
    });
    let host = if !args.host.is_empty() && args.host != "localhost" {
        args.host.clone()
    } else {
        config_host.unwrap_or_else(|| "127.0.0.1".to_string())
    };
    let port = if args.port != 8080 {
        args.port
    } else {
        config_port.unwrap_or(8080)
    };

    // check if server is already running
    if is_server_running()? {
        return Err(anyhow::anyhow!("Server is already running"));
    }

    // ── Foreground mode ────────────────────────────────────────────
    // Without -d/--background, run server in the current process, logging to stderr and --log-file,
    // Ctrl-C to exit.
    if !args.background {
        crate::logging::init_with_log_file_and_stderr(0, false, args.log_file.as_deref(), false);
        if let Some(ref log_file) = args.log_file {
            // Also log AST trace to the same file
            std::env::set_var("MCC_LOG_FILE", log_file);
        }
        let log_info = args
            .log_file
            .as_deref()
            .map(|p| format!("log={}", p))
            .unwrap_or_default();
        if log_info.is_empty() {
            info!(target: "mcc::server", pid = process::id(), %host, port, "Foreground mode, server started");
        } else {
            info!(target: "mcc::server", pid = process::id(), %host, port, log = log_info, "Foreground mode, server started");
        }
        return run_server_internal(&host, port, &args.load_lib);
    }

    // ── Background mode (-d/--background) ────────────────────────────────────
    // Use setsid to launch background process, detach stdout/stderr
    let exe_path = std::env::current_exe()?;
    let mut cmd = process::Command::new(&exe_path);
    cmd.arg("_server_internal")
        .arg("--host")
        .arg(&host)
        .arg("--port")
        .arg(port.to_string());

    // Preload libraries (can be called multiple times)
    for lib in &args.load_lib {
        cmd.arg("--lib").arg(lib);
    }

    // Use specified log file or default path
    let log_file = args
        .log_file
        .clone()
        .unwrap_or_else(|| data_dir::log_file().to_string_lossy().to_string());
    cmd.arg("--log-file");
    cmd.arg(&log_file);
    cmd.env("MCC_LOG_FILE", &log_file);

    // Background mode redirects stdout/stderr
    cmd.stdout(process::Stdio::null());
    cmd.stderr(process::Stdio::null());

    let mut child = cmd.spawn()?;

    // Poll PID file for up to 10 seconds, waiting for child process to complete mcode loading and write PID
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut started = false;
    while Instant::now() < deadline {
        // Child process exits prematurely -> immediate failure
        if let Ok(Some(status)) = child.try_wait() {
            return Err(anyhow::anyhow!(
                "Server startup failed: child process exited (status={})",
                status
            ));
        }
        if is_server_running()? {
            started = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    if !started {
        return Err(anyhow::anyhow!(
            "Server startup failed: PID file not written within 10 seconds"
        ));
    }

    if let Some(ref log_file) = args.log_file {
        eprintln!("Server started in background, log output to: {}", log_file);
    } else {
        eprintln!("Server started in background (host={} port={})", host, port);
    }
    Ok(())
}

// Internal startup function (invoked by child process)
pub fn run_server_internal(host: &str, port: u16, libs: &[String]) -> Result<()> {
    // Skip is_server_running check (since this is internal startup)
    mcc::mcc_set_system_root(data_dir::data_root().as_path());
    // Ensure canonical dirs exist + run one-shot migration. Idempotent.
    // (The CLI's main.rs calls ensure_dirs once at startup; doing it again
    // here is cheap and protects the server from any startup paths that
    // bypass main.)
    let _ = data_dir::ensure_dirs();
    // Load system libraries (e.g. mcode) according to config (libs.load).
    // Previously this used mcc_init_no_lib(), which skipped mcode loading and
    // caused enum PKG (and other system symbols) to be missing for LSP gotodef.
    mcc::mcc_init();
    if !libs.is_empty() {
        crate::cmds::manifest::load_libs(libs);
    }

    // Initialize logging to file (env var set by run_start)
    let log_file = std::env::var("MCC_LOG_FILE").ok();
    crate::logging::init_with_log_file_and_stderr(0, false, log_file.as_deref(), false);

    // Route C-side trace output to log file
    if let Ok(p) = std::env::var("MCC_LOG_FILE") {
        mcc::mcc_log_init(&p);
    }

    // ── 1. Register all RPC methods ──────────────────────────────────────
    let server = register_all(RpcServerBuilder::new().host(host).port(port)).build();

    // ── 2. Write PID file for client discovery ─────────────────────────
    write_pid_file(host, port)?;

    // ── 3. Blocking run ───────────────────────────────────────────────
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(server.start());

    // Clean up pid file on exit
    let _ = fs::remove_file(pid_file_path());
    result.map_err(Into::into)
}

pub fn run_stop(args: &StopArgs) -> Result<()> {
    stop_server(args.force, args.timeout)
}

pub fn run_status(args: &StatusArgs, format: OutputFormat) -> Result<()> {
    status_server(args.json, args.watch, format)
}

pub fn register_all(builder: RpcServerBuilder) -> RpcServerBuilder {
    builder
        .register_method("server.info", handlers::handle_server_info)
        .register_method("server.methods", handlers::handle_methods)
        .register_method("lib.list", handlers::handle_library_list)
        .register_method("lib.info", handlers::handle_library_show)
        .register_method("lib.load", handlers::handle_lib_load)
        .register_method("lib.unload", handlers::handle_lib_unload)
        .register_method("lib.install", handlers::handle_lib_install)
        .register_method("lib.uninstall", handlers::handle_lib_uninstall)
        .register_method("lib.search", handlers::handle_lib_search)
        .register_method("trace.set", handlers::handle_trace_set)
        .register_method("trace.get", handlers::handle_trace_get)
        .register_method("build.full", handlers::handle_build_full)
        .register_method("parse", handlers::handle_parse)
        .register_method("show.component", handlers::handle_show_component)
        .register_method("show.component.list", handlers::handle_show_component_list)
        .register_method("show.module", handlers::handle_show_module)
        .register_method("show.module.list", handlers::handle_show_module_list)
        .register_method("show.interface", handlers::handle_show_interface)
        .register_method("show.interface.list", handlers::handle_show_interface_list)
        .register_method("show.net", handlers::handle_show_net)
        .register_method("show.net.list", handlers::handle_show_net_list)
        // show — missing containers (M5 drill-down)
        .register_method("show.all", handlers::handle_show_all)
        .register_method("show.file", handlers::handle_show_file)
        .register_method("show.files", handlers::handle_show_files)
        .register_method("show.enum", handlers::handle_show_enum)
        .register_method("show.enum.list", handlers::handle_show_enum_list)
        // show — drill-down (M5)
        .register_method("show.pins", handlers::handle_show_pins)
        .register_method("show.ports", handlers::handle_show_ports)
        .register_method("show.ports.list", handlers::handle_show_ports_list)
        .register_method("show.labels", handlers::handle_show_labels)
        .register_method("show.instances", handlers::handle_show_instances)
        .register_method("show.nets", handlers::handle_show_nets)
        .register_method("show.attrs", handlers::handle_show_attrs)
        .register_method("show.funcs", handlers::handle_show_funcs)
        .register_method("show.params", handlers::handle_show_params)
        .register_method("show.roles", handlers::handle_show_roles)
        .register_method("show.values", handlers::handle_show_values)
        .register_method("show.dump", handlers::handle_show_dump)
        .register_method("show.dump.all", handlers::handle_show_dump_all)
        .register_method("check", handlers::handle_check)
        .register_method("extract", handlers::handle_extract)
        .register_method("defs.search", handlers::handle_defs_search)
        .register_method("defs.query", handlers::handle_defs_query)
        .register_method("export", handlers::handle_export)
        .register_method("sem", handlers::handle_sem)
        .register_method("explain", handlers::handle_explain)
        .register_method("def", handlers::handle_def)
        .register_method("erc", handlers::handle_erc)
        .register_method("refs", handlers::handle_refs)
        .register_method("convert", handlers::handle_convert)
        .register_method("report", handlers::handle_report)
        .register_method("caps", handlers::handle_caps)
        .register_method("diagnostics", handlers::handle_diagnostics)
        .register_method("project_symbols", handlers::handle_project_symbols)
        .register_method("set_project_root", handlers::handle_set_project_root)
        .register_method("set_system_root", handlers::handle_set_system_root)
        .register_method("init", handlers::handle_init)
        .register_method("load_project", handlers::handle_load_project)
        .register_method("add_file", handlers::handle_add_file)
        .register_method("remove_file", handlers::handle_remove_file)
}

fn pid_file_path() -> PathBuf {
    // Single source of truth: fixed at ~/.mcode/logs/mcc.pid, decoupled from MCC_SYSTEM_ROOT,
    // so start/stop/status locate the same daemon regardless of env var differences.
    data_dir::pid_file()
}

fn write_pid_file(host: &str, port: u16) -> Result<()> {
    let pid = process::id();
    let path = pid_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let mut f = File::create(&path)
        .with_context(|| format!("Failed to create PID file: {}", path.display()))?;
    writeln!(f, "{}", pid)?;
    writeln!(f, "{}:{}", host, port)?;
    Ok(())
}

// ============================================================================
// Stop / Status
// ============================================================================

fn stop_server(force: bool, timeout: u64) -> Result<()> {
    let pid_file = pid_file_path();
    if !pid_file.exists() {
        println!("Server not running");
        return Ok(());
    }
    let pid = read_pid()?;

    let signal = if force { "-KILL" } else { "-TERM" };
    let _ = process::Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .output();

    if !is_process_running(pid) {
        let _ = fs::remove_file(&pid_file);
        eprintln!("Server stopped");
        return Ok(());
    }

    if !force {
        let start = Instant::now();
        while is_process_running(pid) {
            if start.elapsed().as_secs() >= timeout {
                return Err(anyhow::anyhow!("Stop timeout, use --force to force kill"));
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = fs::remove_file(&pid_file);
    eprintln!("Server stopped");
    Ok(())
}

fn status_server(json: bool, watch: bool, format: OutputFormat) -> Result<()> {
    let render_once = || -> Result<ServerStatus> {
        if !is_server_running()? {
            return Ok(ServerStatus::not_running());
        }
        let content = fs::read_to_string(pid_file_path()).unwrap_or_default();
        let lines: Vec<&str> = content.lines().collect();
        let pid: u32 = lines
            .first()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let (host, port) = lines
            .get(1)
            .and_then(|s| {
                let mut sp = s.splitn(2, ':');
                Some((sp.next()?.to_string(), sp.next()?.parse().ok()?))
            })
            .map(|(h, p)| (Some(h), Some(p)))
            .unwrap_or((None, None));
        Ok(ServerStatus::running(pid, host, port))
    };

    let status = render_once()?;
    if json || format.is_structured() {
        output::emit(&status, format, None)?;
    } else {
        println!("{}", status);
    }

    if watch {
        loop {
            thread::sleep(Duration::from_secs(1));
            let status = render_once().unwrap_or(ServerStatus::not_running());
            if json || format.is_structured() {
                println!("{}", serde_json::to_string(&status)?);
            } else {
                let s = if status.running {
                    "Running"
                } else {
                    "Not running"
                };
                eprint!("\rServer status: {} (PID: {})", s, status.pid.unwrap_or(0));
            }
        }
    }
    Ok(())
}

fn is_server_running() -> Result<bool> {
    let pid_file = pid_file_path();
    if !pid_file.exists() {
        return Ok(false);
    }
    let pid = read_pid()?;
    Ok(is_process_running(pid))
}

fn read_pid() -> Result<u32> {
    let content = fs::read_to_string(pid_file_path())?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Err(anyhow::anyhow!("PID file is empty"));
    }
    lines[0].trim().parse().context("Invalid PID")
}

fn is_process_running(pid: u32) -> bool {
    use std::process::Command;
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
