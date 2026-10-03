// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! U392 load-phase probe: wall-clock accounting for one library-load round.
//!
//! The cold-load cost (12–16 s on the jlink probe project) has no phase
//! breakdown — library scan, file read, lex+parse, namespace resolve and
//! pass1 all collapse into the single per-library `elapsed_ms`. This probe
//! splits them into five accumulators plus a per-file ledger, so the log
//! line at the end of a load names the dominant phase and the dominant
//! files. It measures only: the accumulators reset at `mcb_load_lib` start
//! and are logged at its end, so every `mcc::lib` "load summary" line is
//! one self-contained library round.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tracing::info;

/// Library root resolution (directory walk + version bounds).
pub static SCAN_NS: AtomicU64 = AtomicU64::new(0);
/// `mcc_load` — the disk read of one source file into the C frontend.
pub static READ_NS: AtomicU64 = AtomicU64::new(0);
/// The remainder of `parse_ast`: lex + grammar parse + diagnostics collect.
pub static LEXPARSE_NS: AtomicU64 = AtomicU64::new(0);
/// `parse_nsp_from_deps` — per-file namespace resolution.
pub static NSP_NS: AtomicU64 = AtomicU64::new(0);
/// `parse_pass1_types` — per-file CMIE registration (sema pass 1).
pub static PASS1_NS: AtomicU64 = AtomicU64::new(0);
/// `mcb_parse_all_modules` — the module-level sema pass (project side).
pub static MODULES_NS: AtomicU64 = AtomicU64::new(0);
/// Files that went through a full parse in this round.
pub static FILES: AtomicU64 = AtomicU64::new(0);

/// Per-file ledger: uri → (parse_ast ns including the read, pass1 ns).
static PER_FILE: Mutex<BTreeMap<String, (u64, u64)>> = Mutex::new(BTreeMap::new());

/// Zero every accumulator and the per-file ledger. Called at library-load
/// start, so a summary line always describes exactly one round.
pub fn reset() {
    SCAN_NS.store(0, Ordering::Relaxed);
    READ_NS.store(0, Ordering::Relaxed);
    LEXPARSE_NS.store(0, Ordering::Relaxed);
    NSP_NS.store(0, Ordering::Relaxed);
    PASS1_NS.store(0, Ordering::Relaxed);
    MODULES_NS.store(0, Ordering::Relaxed);
    FILES.store(0, Ordering::Relaxed);
    PER_FILE.lock().unwrap().clear();
}

/// Run `f` with its wall time added to `sink`.
pub fn time<T>(sink: &AtomicU64, f: impl FnOnce() -> T) -> T {
    let t0 = Instant::now();
    let out = f();
    sink.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    out
}

/// Record one file's `parse_ast` duration (read + lex + parse) in the ledger.
pub fn note_parse(uri: &str, parse_ns: u64) {
    FILES.fetch_add(1, Ordering::Relaxed);
    PER_FILE
        .lock()
        .unwrap()
        .entry(uri.to_string())
        .or_insert((0, 0))
        .0 += parse_ns;
}

/// Record one file's `parse_pass1_types` duration in the ledger.
pub fn note_pass1(uri: &str, pass1_ns: u64) {
    PER_FILE
        .lock()
        .unwrap()
        .entry(uri.to_string())
        .or_insert((0, 0))
        .1 += pass1_ns;
}

/// Emit the round summary on the `mcc::lib` target: phase totals first,
/// then the ten heaviest files by parse+pass1 time.
pub fn log_summary(lib: &str) {
    let ms = |ns: &AtomicU64| ns.load(Ordering::Relaxed) / 1_000_000;
    let map = std::mem::take(&mut *PER_FILE.lock().unwrap());
    let mut files: Vec<(String, (u64, u64))> = map.into_iter().collect();
    files.sort_by_key(|(_, (p, s))| std::cmp::Reverse(p + s));
    let top: Vec<String> = files
        .iter()
        .take(10)
        .map(|(uri, (p, s))| {
            format!(
                "{}: parse {}ms pass1 {}ms",
                uri,
                p / 1_000_000,
                s / 1_000_000
            )
        })
        .collect();
    info!(
        target: "mcc::lib",
        lib = lib,
        files = FILES.load(Ordering::Relaxed),
        scan_ms = ms(&SCAN_NS),
        read_ms = ms(&READ_NS),
        lexparse_ms = ms(&LEXPARSE_NS),
        nsp_ms = ms(&NSP_NS),
        pass1_ms = ms(&PASS1_NS),
        top_files = ?top,
        "load summary (U392 probe)"
    );
}
