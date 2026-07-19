// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! Configuration management command

use anyhow::Result;
use mcc::cli::config::{self, MccConfig};

fn strip_global_prefix(name: &str) -> &str {
    if name.starts_with("global.") {
        &name[7..]
    } else {
        name
    }
}

pub fn run(action: &mcc::cli::ConfigAction) -> Result<()> {
    let client = mcc::cli::rpcclient::RpcClient::probe();

    match action {
        mcc::cli::ConfigAction::Get { name } => {
            let key = strip_global_prefix(name);
            if key.starts_with("trace.") {
                if let Some(ref c) = client {
                    let result = c.call("trace.get", serde_json::json!({ "name": key }))?;
                    if let Some(value) = result.get("value") {
                        println!("{}", value);
                    }
                    return Ok(());
                }
            }
            let cfg = config::load_global_config()?;
            let value = get_config_value(&cfg, key)?;
            println!("{}", value);
        }
        mcc::cli::ConfigAction::Set { name, value, rest } => {
            let mut keys_values = vec![(name.clone(), value.clone())];
            let mut rest_iter = rest.iter();
            while let Some(k) = rest_iter.next() {
                if let Some(v) = rest_iter.next() {
                    keys_values.push((k.clone(), v.clone()));
                }
            }

            for (name, value) in keys_values {
                let key = strip_global_prefix(&name);
                if key.starts_with("trace.") {
                    if let Some(ref c) = client {
                        let bool_value = parse_bool(&value)?;
                        let _result = c.call(
                            "trace.set",
                            serde_json::json!({ "name": key, "value": bool_value }),
                        )?;
                        println!("✓ {} = {}", name, value);
                        continue;
                    }
                }
                let mut cfg = config::load_global_config().unwrap_or_default();
                set_config_value(&mut cfg, &key, &value)?;
                config::save_global_config(&cfg)?;
                println!("✓ {} = {}", name, value);
            }
        }
        mcc::cli::ConfigAction::List => {
            let cfg = config::load_global_config().unwrap_or_default();
            list_config(&cfg);
        }
        mcc::cli::ConfigAction::Reset => {
            let cfg = MccConfig::default();
            config::save_global_config(&cfg)?;
            println!("✓ config reset to defaults");
        }
    }
    Ok(())
}

fn get_config_value(config: &MccConfig, name: &str) -> Result<String> {
    let parts: Vec<&str> = name.split('.').collect();
    match parts.as_slice() {
        ["trace", "enabled"] => Ok(config
            .trace
            .enabled
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["trace", "ast"] => Ok(config
            .trace
            .ast
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["trace", "lexer"] => Ok(config
            .trace
            .lexer
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["trace", "parser"] => Ok(config
            .trace
            .parser
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["trace", "visit"] => Ok(config
            .trace
            .visit
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["trace", "pass1"] => Ok("null".to_string()), // runtime state, not stored in config file
        ["trace", "pass2"] => Ok("null".to_string()),
        ["trace", "server"] => Ok("null".to_string()),
        ["parser", "max_depth"] => Ok(config
            .parser
            .max_depth
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["parser", "strict"] => Ok(config
            .parser
            .strict
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["output", "format"] => Ok(config
            .output
            .format
            .clone()
            .unwrap_or_else(|| "null".to_string())),
        ["output", "color"] => Ok(config
            .output
            .color
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string())),
        ["libs", "load"] => Ok(format!("{:?}", config.libs.load)),
        _ => anyhow::bail!("unknown config key: {}", name),
    }
}

fn set_config_value(config: &mut MccConfig, name: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = name.split('.').collect();
    match parts.as_slice() {
        ["trace", "enabled"] => config.trace.enabled = Some(parse_bool(value)?),
        ["trace", "ast"] => config.trace.ast = Some(parse_bool(value)?),
        ["trace", "lexer"] => config.trace.lexer = Some(parse_bool(value)?),
        ["trace", "parser"] => config.trace.parser = Some(parse_bool(value)?),
        ["trace", "visit"] => config.trace.visit = Some(parse_bool(value)?),
        ["trace", "pass1"] => {} // runtime state, not stored in config file
        ["trace", "pass2"] => {}
        ["trace", "server"] => {}
        ["parser", "max_depth"] => config.parser.max_depth = Some(value.parse()?),
        ["parser", "strict"] => config.parser.strict = Some(parse_bool(value)?),
        ["output", "format"] => config.output.format = Some(value.to_string()),
        ["output", "color"] => config.output.color = Some(parse_bool(value)?),
        ["libs", "load"] => {
            // Parse comma-separated list: "mcode" or "mcode,custom_lib"
            let libs: Vec<String> = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            config.libs.load = libs;
        }
        _ => anyhow::bail!("unknown config key: {}", name),
    }
    Ok(())
}

fn list_config(config: &MccConfig) {
    println!("Global Config:");
    println!("  global.trace.enabled = {:?}", config.trace.enabled);
    println!("  global.trace.ast = {:?}", config.trace.ast);
    println!("  global.trace.lexer = {:?}", config.trace.lexer);
    println!("  global.trace.parser = {:?}", config.trace.parser);
    println!("  global.trace.visit = {:?}", config.trace.visit);
    println!("  global.parser.max_depth = {:?}", config.parser.max_depth);
    println!("  global.parser.strict = {:?}", config.parser.strict);
    println!("  global.output.format = {:?}", config.output.format);
    println!("  global.output.color = {:?}", config.output.color);
    println!("  global.libs.load = {:?}", config.libs.load);
}

fn parse_bool(value: &str) -> Result<bool> {
    match value.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" | "null" | "none" => Ok(false),
        _ => anyhow::bail!("invalid boolean value: {}", value),
    }
}
