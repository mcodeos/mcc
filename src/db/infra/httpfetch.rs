// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! The HTTP read face of the registry client (registry-p3-protocol.md §1/§2).
//!
//! Thin `reqwest::blocking` wrapper pinning the protocol contract: connect
//! 10s / read 60s (constants, no config surface), at most 5 redirects, an
//! identifying User-Agent, and **no credentials** — open-pack anonymity law
//! (§3.3); the token belongs to the phase-two publish API only, which does
//! not exist yet. 404 is an existence *answer* (the name is unknown), not a
//! failure — it gets its own variant so the caller maps it to `Ok(None)`.

use std::path::Path;
use std::time::Duration;

/// The metadata contract (§2.1): dial 10s, read 60s — constants by ruling
/// (§6: no config surface is pre-built).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// §2.1: at most 5 hops, http(s) only (reqwest only ever speaks http(s)).
const MAX_REDIRECTS: usize = 5;

fn agent() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .user_agent(format!("mcc/{}", env!("CARGO_PKG_VERSION")))
        // Never honor env/system proxies: a macOS system HTTP proxy would
        // otherwise capture even http://127.0.0.1/ registries and answer 502
        // for a host only this machine can reach (observed live). A registry
        // URL is user-configured and direct; cargo/npm bypass proxies for
        // loopback for the same reason.
        .no_proxy()
        .build()
        .expect("reqwest client builder never fails with these options")
}

/// A completed fetch: the status is the caller's to judge (any non-404
/// status outside 2xx/304 is the caller's error to name).
pub struct FetchResponse {
    pub status: u16,
    pub etag: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum FetchError {
    /// 404 — the name (or artifact) is not there. An answer, not a failure.
    NotFound,
    /// Any other status: named with the URL, never silently swallowed.
    Status { url: String, status: u16 },
    /// Dial/read failure: the endpoint is unreachable (offline face).
    Transport { url: String, message: String },
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::NotFound => write!(f, "not found (404)"),
            FetchError::Status { url, status } => write!(f, "HTTP {status} from {url}"),
            FetchError::Transport { url, message } => {
                write!(f, "cannot reach {url}: {message}")
            }
        }
    }
}

/// `GET url` with an optional `If-None-Match` revalidation. The ETag response
/// header (if any) travels back so the caller can park it beside the cache
/// row (§2.2).
pub fn get(url: &str, if_none_match: Option<&str>) -> Result<FetchResponse, FetchError> {
    let mut req = agent().get(url);
    if let Some(etag) = if_none_match {
        req = req.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    let resp = req
        .send()
        .map_err(|e| FetchError::Transport { url: url.to_string(), message: e.to_string() })?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Err(FetchError::NotFound);
    }
    let etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body = resp
        .bytes()
        .map_err(|e| FetchError::Transport { url: url.to_string(), message: e.to_string() })?
        .to_vec();
    Ok(FetchResponse { status, etag, body })
}

/// Stream `url` into `dest` through a `.part` sibling, then rename — a
/// half-written artifact must never exist under its real name (§2.1). The
/// caller verifies sha256 afterwards; nothing here trusts the transport.
pub fn download(url: &str, dest: &Path) -> Result<(), FetchError> {
    let resp = agent()
        .get(url)
        .send()
        .map_err(|e| FetchError::Transport { url: url.to_string(), message: e.to_string() })?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Err(FetchError::NotFound);
    }
    if !(200..300).contains(&status) {
        return Err(FetchError::Status { url: url.to_string(), status });
    }
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "artifact".to_string());
    let part = dest.with_file_name(format!("{name}.part"));
    {
        let mut reader = resp;
        let mut file = std::fs::File::create(&part).map_err(|e| FetchError::Transport {
            url: url.to_string(),
            message: format!("cannot create {}: {e}", part.display()),
        })?;
        std::io::copy(&mut reader, &mut file).map_err(|e| FetchError::Transport {
            url: url.to_string(),
            message: format!("download interrupted: {e}"),
        })?;
    }
    std::fs::rename(&part, dest).map_err(|e| FetchError::Transport {
        url: url.to_string(),
        message: format!("cannot finalize {}: {e}", dest.display()),
    })?;
    Ok(())
}
