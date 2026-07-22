// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

/// Debug log helper — writes to /tmp/mcc_debug.log for F12 diagnostic tracing.
/// Use the `dlog!` macro throughout the codebase for temporary debug output.
use std::io::Write;
use std::sync::Mutex;

static LOG_MU: Mutex<()> = Mutex::new(());

pub fn debug_log(msg: &str) {
    let _guard = LOG_MU.lock().unwrap();
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/mcc_debug.log")
    {
        let _ = writeln!(f, "{msg}");
    }
}

#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {
        $crate::debug_log::debug_log(&format!($($arg)*))
    };
}
