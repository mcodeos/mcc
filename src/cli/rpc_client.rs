// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! `mcc::cli::rpc_client` — JSON-RPC client (CLI side)
//!
//! Thin client for `proj` / `lib` etc. subcommands:
//!   1. `RpcClient::probe()`  Probe server, return None if failed → fallback to local
//!   2. `RpcClient::call()`   HTTP POST to http://host:port/rpc
//!
//! Use curl command to implement HTTP communication

use anyhow::{anyhow, Context, Result};
use serde_json::json;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RpcClient {
    pub host: String,
    pub port: u16,
}

impl RpcClient {
    pub fn probe() -> Option<Self> {
        let Ok((pid, host, port)) = read_pid_file() else {
            return None;
        };
        if !is_process_alive(pid) {
            return None;
        }
        let client = Self { host, port };
        let result = client.call("server.info", json!({}));
        result.is_ok().then_some(client)
    }

    pub fn connect(host: &str, port: u16) -> Result<Self> {
        let client = Self {
            host: host.to_string(),
            port,
        };
        client.call("server.info", json!({}))?;
        Ok(client)
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        let url = format!("http://{}:{}/rpc", self.host, self.port);
        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
        std::env::set_var("no_proxy", "127.0.0.1,localhost");

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        let resp = client.post(&url).json(&req).send()?;

        let resp_json: Value = resp.json()?;

        if let Some(err_obj) = resp_json.get("error").and_then(|v| v.as_object()) {
            if !err_obj.is_empty() {
                let msg = err_obj
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("rpc error");
                let code = err_obj
                    .get("code")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(-32603);
                return Err(anyhow!("RPC error [{code}]: {msg}"));
            }
        }
        Ok(resp_json.get("result").cloned().unwrap_or(Value::Null))
    }
}

fn pid_file_path() -> PathBuf {
    // PID file is fixed at ~/.mcode/logs/mcc.pid, separated from MCC_SYSTEM_ROOT.
    let base = dirs::home_dir()
        .map(|h| h.join(".mcode"))
        .unwrap_or_else(|| PathBuf::from(".mcode"));
    base.join("logs").join("mcc.pid")
}

fn read_pid_file() -> Result<(u32, String, u16)> {
    let path = pid_file_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("pid file missing: {}", path.display()))?;
    let lines: Vec<&str> = content.lines().collect();
    let pid: u32 = lines
        .first()
        .ok_or_else(|| anyhow!("empty pid file"))?
        .trim()
        .parse()
        .context("invalid pid")?;
    let addr_line = lines
        .get(1)
        .ok_or_else(|| anyhow!("pid file missing addr"))?;
    let mut sp = addr_line.splitn(2, ':');
    let host = sp.next().unwrap_or("127.0.0.1").to_string();
    let port: u16 = sp.next().ok_or_else(|| anyhow!("missing port"))?.parse()?;
    Ok((pid, host, port))
}

fn is_process_alive(pid: u32) -> bool {
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system.process(sysinfo::Pid::from_u32(pid)).is_some()
}

pub fn pack_dir_as_tar_gz_b64(root: &std::path::Path, strip_root: bool) -> Result<String> {
    use base64::Engine;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    let mut buf: Vec<u8> = Vec::new();
    {
        let enc = GzEncoder::new(&mut buf, Compression::default());
        let mut tar = tar::Builder::new(enc);
        if strip_root {
            tar.append_dir_all(".", root)?;
        } else {
            let name = root
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("project"));
            tar.append_dir_all(name, root)?;
        }
        tar.finish()?;
    }
    Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
}

pub fn collect_mc_files(root: &std::path::Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    Ok(out)
}

fn walk(
    root: &std::path::Path,
    current: &std::path::Path,
    out: &mut Vec<(String, String)>,
) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "mc") {
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .to_string()
                .replace("\\", "/");
            let content = std::fs::read_to_string(&path)?;
            out.push((rel, content));
        }
    }
    Ok(())
}
