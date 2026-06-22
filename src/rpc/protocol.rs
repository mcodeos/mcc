// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! JSON-RPC 2.0 protocol implementation

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    #[serde(rename = "jsonrpc")]
    pub jsonrpc: String,

    pub method: String,

    pub params: Option<Value>,

    pub id: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    #[serde(rename = "jsonrpc")]
    pub jsonrpc: String,

    pub result: Option<Value>,

    pub error: Option<JsonRpcError>,

    pub id: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,

    pub message: String,

    pub data: Option<Value>,
}

impl JsonRpcError {
    pub fn parse_error() -> Self {
        Self {
            code: -32700,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request() -> Self {
        Self {
            code: -32600,
            message: "Invalid request".to_string(),
            data: None,
        }
    }

    pub fn method_not_found() -> Self {
        Self {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    pub fn invalid_params() -> Self {
        Self {
            code: -32602,
            message: "Invalid params".to_string(),
            data: None,
        }
    }

    pub fn internal_error() -> Self {
        Self {
            code: -32603,
            message: "Internal error".to_string(),
            data: None,
        }
    }

    pub fn custom(code: i32, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            data: None,
        }
    }
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: Option<Value>, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

pub type RpcResult = Result<Value, JsonRpcError>;

pub trait RpcHandler {
    fn handle(&self, method: &str, params: Option<Value>) -> RpcResult;
}

#[derive(Default)]
pub struct RpcMethodRegistry {
    methods: HashMap<String, Box<dyn Fn(Option<Value>) -> RpcResult + Send + Sync>>,
}

impl RpcMethodRegistry {
    pub fn new() -> Self {
        Self {
            methods: HashMap::new(),
        }
    }

    pub fn register<F>(&mut self, name: &str, handler: F)
    where
        F: Fn(Option<Value>) -> RpcResult + Send + Sync + 'static,
    {
        self.methods.insert(name.to_string(), Box::new(handler));
    }

    pub fn call(&self, method: &str, params: Option<Value>) -> RpcResult {
        match self.methods.get(method) {
            Some(handler) => handler(params),
            None => Err(JsonRpcError::method_not_found()),
        }
    }

    pub fn list_methods(&self) -> Vec<String> {
        self.methods.keys().cloned().collect()
    }
}
