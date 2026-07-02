// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! MCC logging initialization
//!
//! Design highlights:
//!
//!   1. **Background daemon logs only to file**: old version `Writer` writes to stderr, and `server.rs` does not close child process stderr.
//!      This leads to debug logs being printed to the caller's terminal.
//!
//!   2. **Actually honor `-q` / `-v`** Here, `_verbose` / `_quiet` are unused.
//!      They are replaced with `quiet` / `verbose` for default levels.
//!
//!   3. **filter runtime adjustable** Here, `EnvFilter` is wrapped in a `reload::Handle`,
//!      using [`set_streams`] / [`reload_filter`] to dynamically switch output streams.
//!        - server runtime (default level is `-q/-v`)
//!        - `mcc::pass1` report (default: off)
//!        - `mcc::pass2` report (default: off)
//!
//! Log target:
//!   - server runtime (using `mcc::server` module)
//!   - pass1 report (using `mcc::pass1` module)
//!   - pass2 report (using `mcc::pass2` module)
//!
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::{fmt, prelude::*, reload, EnvFilter, Registry};

static ALREADY_INIT: OnceLock<bool> = OnceLock::new();

/// Runtime reloadable filter handle. Daemon can dynamically switch output streams
/// (server logs, pass1/pass2 reports) using this handle.
static RELOAD_HANDLE: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

// ============================================================================
// Time format for logging
// ============================================================================

pub struct ShortTime;

impl FormatTime for ShortTime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let secs = now.as_secs();

        // Local timezone offset (seconds)
        let tz_offset = get_timezone_offset();
        let local_secs = (secs as i64 + tz_offset).max(0) as u64;

        let hours = (local_secs / 3600) % 24;
        let mins = (local_secs / 60) % 60;
        let secs = local_secs % 60;

        let (_year, month, day) = get_local_date();
        write!(
            w,
            "{:02}-{:02} {:02}:{:02}:{:02}",
            month, day, hours, mins, secs
        )
    }
}

fn get_timezone_offset() -> i64 {
    // Default UTC+8 (Beijing Time). Dynamic detection required for chrono.
    8 * 3600
}

fn get_local_date() -> (u32, u32, u32) {
    let now = SystemTime::now();
    let secs_since_epoch = now.duration_since(UNIX_EPOCH).unwrap().as_secs();

    let mut days = secs_since_epoch / 86400;
    let mut year = 1970u64;
    let mut month = 1u32;
    let mut day = 1u32;

    loop {
        let days_in_year = if is_leap_year(year as u32) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let days_in_months: &[u32] = if is_leap_year(year as u32) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    for i in 0..12 {
        if days < days_in_months[i] as u64 {
            month = i as u32 + 1;
            day = days as u32 + 1;
            break;
        }
        days -= days_in_months[i] as u64;
    }

    (year as u32, month, day)
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ============================================================================
// filter construction / runtime adjustment
// ============================================================================

/// Construct default filter: `RUST_LOG` takes precedence; otherwise, assign levels based on `-q` / `-v`,
/// and disable pass1 / pass2 reports by default.
fn base_filter(verbose: u8, quiet: bool) -> EnvFilter {
    if let Ok(f) = EnvFilter::try_from_default_env() {
        return f;
    }

    let level = if quiet {
        "warn"
    } else {
        match verbose {
            0 => "info",
            1 => "debug",
            _ => "trace",
        }
    };

    // server runtime logs use `level`; pass1 / pass2 reports are off by default.
    EnvFilter::new(format!("{level},mcc::pass1=off,mcc::pass2=off"))
}

/// Reload the filter with an arbitrary EnvFilter directive string (advanced usage).
///
/// Returns `true` if the filter was successfully applied; `false` if the logging system
/// has not been initialized or the daemon does not exist.
pub fn reload_filter(spec: &str) -> bool {
    match RELOAD_HANDLE.get() {
        Some(h) => h.reload(EnvFilter::new(spec)).is_ok(),
        None => false,
    }
}

/// Set the output streams for the daemon.
///
///   - `server_level`: server runtime level (e.g., `"info"` / `"warn"` / `"debug"` / `"off"`)
///   - `pass1`: Whether to output `mcc::pass1` report
///   - `pass2`: Whether to output `mcc::pass2` report
///
/// Typical usage (in `trace.set` RPC handler):
/// ```ignore
/// logging::set_streams("warn", true, false); // Only show pass1, suppress server noise
/// ```
pub fn set_streams(server_level: &str, pass1: bool, pass2: bool) -> bool {
    let spec = format!(
        "{server_level},mcc::pass1={},mcc::pass2={}",
        if pass1 { "info" } else { "off" },
        if pass2 { "info" } else { "off" },
    );
    reload_filter(&spec)
}

// ============================================================================
// Initialization
// ============================================================================

/// Initialize logging to stderr (foreground / no log file scenario).
///
/// `show_target`: Whether to display log target (e.g., `mcc::builder`). Default is false.
pub fn init(verbose: u8, quiet: bool, show_target: bool) {
    if ALREADY_INIT.get().is_some() {
        return;
    }
    let _ = ALREADY_INIT.set(true);

    let (filter_layer, handle) = reload::Layer::new(base_filter(verbose, quiet));
    let _ = RELOAD_HANDLE.set(handle);

    // Choose on show_target to display timestamp/target/file line number
    let fmt_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = if show_target {
        Box::new(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(true)
                .with_target(true)
                .with_thread_ids(false)
                .with_file(true)
                .with_line_number(true)
                .with_timer(ShortTime),
        )
    } else {
        // Do not display timestamp/target/file line number
        let format = fmt::format().with_timer(()).with_target(false);
        Box::new(
            fmt::layer()
                .event_format(format)
                .with_writer(std::io::stderr)
                .with_ansi(true),
        )
    };

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init()
        .map_err(|e| {
            eprintln!("[logging] Failed to init: {}", e);
        })
        .ok();
}

/// Initialize logging to a file (daemon mode).
///
/// Unlike the old version: **only write to the log file, no longer copy to stderr**.
/// Residual stdout/stderr (println!/panic etc. non-tracing outputs) are redirected
/// to the same file by `server.rs::run_start` when spawned.
///
/// `show_target`: Whether to show log target (e.g., `mcc::builder`). Default is false.
pub fn init_with_log_file(verbose: u8, quiet: bool, log_file: Option<&str>, show_target: bool) {
    if ALREADY_INIT.get().is_some() {
        return;
    }

    let Some(path) = log_file else {
        // No log file → fallback to stderr initialization
        init(verbose, quiet, show_target);
        return;
    };

    let _ = ALREADY_INIT.set(true);

    let log_path = PathBuf::from(path);
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let (filter_layer, handle) = reload::Layer::new(base_filter(verbose, quiet));
    let _ = RELOAD_HANDLE.set(handle);

    let writer_path = log_path.clone();
    let fmt_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = if show_target {
        Box::new(
            fmt::layer()
                .with_writer(move || {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&writer_path)
                        .expect("Failed to open log file")
                })
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(false)
                .with_file(true)
                .with_line_number(true)
                .with_timer(ShortTime),
        )
    } else {
        let format = fmt::format().with_timer(()).with_target(false);
        Box::new(
            fmt::layer()
                .event_format(format)
                .with_writer(move || {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&writer_path)
                        .expect("Failed to open log file")
                })
                .with_ansi(false),
        )
    };

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init()
        .ok();
}

/// Initialize logging: **both** to stderr and a file (foreground mode).
///
/// Unlike `init_with_log_file`, this function also adds a stderr fmt layer,
/// allowing users to observe server runtime in real-time in the terminal.
///
/// `show_target`: Whether to display log target (e.g., `mcc::builder`). Default is false.
pub fn init_with_log_file_and_stderr(
    verbose: u8,
    quiet: bool,
    log_file: Option<&str>,
    show_target: bool,
) {
    if ALREADY_INIT.get().is_some() {
        return;
    }

    let Some(path) = log_file else {
        init(verbose, quiet, show_target);
        return;
    };

    let _ = ALREADY_INIT.set(true);

    let log_path = PathBuf::from(path);
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let (filter_layer, handle) = reload::Layer::new(base_filter(verbose, quiet));
    let _ = RELOAD_HANDLE.set(handle);

    let writer_path = log_path.clone();
    let file_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = if show_target {
        Box::new(
            fmt::layer()
                .with_writer(move || {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&writer_path)
                        .expect("Failed to open log file")
                })
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(false)
                .with_file(true)
                .with_line_number(true)
                .with_timer(ShortTime),
        )
    } else {
        let format = fmt::format().with_timer(()).with_target(false);
        Box::new(
            fmt::layer()
                .event_format(format)
                .with_writer(move || {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&writer_path)
                        .expect("Failed to open log file")
                })
                .with_ansi(false),
        )
    };

    let stderr_layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = if show_target {
        Box::new(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(true)
                .with_target(true)
                .with_thread_ids(false)
                .with_file(true)
                .with_line_number(true)
                .with_timer(ShortTime),
        )
    } else {
        let format = fmt::format().with_timer(()).with_target(false);
        Box::new(
            fmt::layer()
                .event_format(format)
                .with_writer(std::io::stderr)
                .with_ansi(true),
        )
    };

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(file_layer)
        .with(stderr_layer)
        .try_init()
        .ok();
}

pub fn server_log_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mcc")
        .join("server.log")
}

#[cfg(test)]
#[allow(dead_code)]
pub fn init_for_test() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::new("off"))
        .try_init();
}
