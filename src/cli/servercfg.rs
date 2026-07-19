// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Server configuration file handling
//!
//! Supports loading server config from `~/.mcc/server.yaml`

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ServerConfig {
    #[serde(default)]
    pub server: ServerSettings,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub tls: TlsSettings,

    #[serde(default)]
    pub auth: AuthSettings,

    #[serde(default)]
    pub limits: LimitsSettings,

    #[serde(default)]
    pub logging: LoggingSettings,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct TlsSettings {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub cert: String,

    #[serde(default)]
    pub key: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct AuthSettings {
    #[serde(default = "default_auth_type")]
    pub r#type: String,

    #[serde(default)]
    pub token_file: String,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LimitsSettings {
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LoggingSettings {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default)]
    pub file: String,
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_auth_type() -> String {
    "none".to_string()
}

fn default_max_connections() -> usize {
    100
}

fn default_request_timeout() -> u64 {
    300
}

fn default_log_level() -> String {
    "info".to_string()
}

pub fn config_path() -> PathBuf {
    crate::cli::datadir::config_dir().join("server.yaml")
}

pub fn load_config() -> Result<ServerConfig> {
    let path = config_path();

    if !path.exists() {
        return Ok(ServerConfig::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let config: ServerConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Invalid config file format: {}", path.display()))?;

    Ok(config)
}

pub fn save_config(config: &ServerConfig) -> Result<()> {
    let path = config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = serde_yaml::to_string(&config)?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;

    Ok(())
}

pub fn create_default_config() -> Result<()> {
    let config = ServerConfig::default();
    save_config(&config)
}
