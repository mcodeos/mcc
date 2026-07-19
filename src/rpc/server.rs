// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! RPC server implementation

use super::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, RpcMethodRegistry};
use anyhow::Result;
use axum::{
    extract::Request,
    http::{Response, StatusCode},
    routing::{get, post},
    serve, Router,
};
use std::net::SocketAddr;
use std::sync::Arc;

pub struct RpcServer {
    registry: Arc<RpcMethodRegistry>,
    host: String,
    port: u16,
}

pub struct RpcServerBuilder {
    host: String,
    port: u16,
    registry: RpcMethodRegistry,
}

impl Default for RpcServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl RpcServerBuilder {
    pub fn new() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            registry: RpcMethodRegistry::new(),
        }
    }

    pub fn host(mut self, host: &str) -> Self {
        self.host = host.to_string();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn register_method<F>(mut self, name: &'static str, handler: F) -> Self
    where
        F: Fn(Option<serde_json::Value>) -> Result<serde_json::Value, JsonRpcError>
            + Send
            + Sync
            + 'static,
    {
        self.registry.register(name, handler);
        self
    }

    pub fn build(self) -> RpcServer {
        RpcServer {
            registry: Arc::new(self.registry),
            host: self.host,
            port: self.port,
        }
    }
}

impl RpcServer {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            registry: Arc::new(RpcMethodRegistry::new()),
            host: host.to_string(),
            port,
        }
    }

    pub fn with_registry(host: &str, port: u16, registry: Arc<RpcMethodRegistry>) -> Self {
        Self {
            registry,
            host: host.to_string(),
            port,
        }
    }

    pub async fn start(self) -> Result<()> {
        let registry = self.registry.clone();
        let host = self.host.clone();
        let port = self.port;

        let app = Router::new()
            .route(
                "/rpc",
                post(move |request: Request| async move {
                    let body = request.into_body();
                    let body = axum::body::to_bytes(body, 100 * 1024 * 1024).await;

                    let body_bytes = match body {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            return Response::builder()
                                .status(StatusCode::PAYLOAD_TOO_LARGE)
                                .body(axum::body::Body::from("Payload too large"))
                                .unwrap_or_else(|_| {
                                    Response::new(axum::body::Body::from("Error"))
                                });
                        }
                    };

                    let request: JsonRpcRequest = match serde_json::from_slice(&body_bytes) {
                        Ok(req) => req,
                        Err(_) => {
                            let err = JsonRpcResponse::error(None, JsonRpcError::parse_error());
                            let body = serde_json::to_string(&err)
                                .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"}}"#.to_string());
                            return Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "application/json")
                                .body(axum::body::Body::from(body))
                                .unwrap_or_else(|_| {
                                    Response::new(axum::body::Body::from(
                                        r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Parse error"}}"#,
                                    ))
                                });
                        }
                    };

                    let id = request.id.clone();
                    let method = request.method.clone();
                    let params_summary = request
                        .params
                        .as_ref()
                        .map(|v| {
                            let s = v.to_string();
                            if s.len() > 200 {
                                let end = s
                                    .char_indices()
                                    .take(200)
                                    .last()
                                    .map(|(i, c)| i + c.len_utf8())
                                    .unwrap_or(200);
                                format!("{}...", &s[..end])
                            } else {
                                s
                            }
                        })
                        .unwrap_or_else(|| "{}".to_string());
                    let t0 = std::time::Instant::now();
                    // Run the actual RPC handler in spawn_blocking to avoid blocking
                    // the async runtime during heavy operations (build, parse, etc.)
                    let registry2 = registry.clone();
                    let method2 = method.clone();
                    let params = request.params;
                    let result = tokio::task::spawn_blocking(move || {
                        registry2.call(&method2, params)
                    })
                    .await
                    .unwrap_or_else(|_| {
                        Err(JsonRpcError::custom(
                            -32603,
                            "Internal error: handler panicked",
                        ))
                    });
                    let elapsed_ms = t0.elapsed().as_millis();

                    // server.info is a probe request, only print when there is a problem
                    let silent_probe = method == "server.info";

                    match &result {
                        Ok(_) => {
                            if silent_probe {
                                tracing::debug!(
                                    target: "mcc::rpc",
                                    "mcc {} ✓ ({}ms)",
                                    method, elapsed_ms
                                );
                            } else {
                                tracing::info!(
                                    target: "mcc::rpc",
                                    "mcc {} params={} ✓ ({}ms)",
                                    method, params_summary, elapsed_ms
                                );
                            }
                        }
                        Err(e) => tracing::warn!(
                            target: "mcc::rpc",
                            "mcc {} params={} ✗ ({}ms): {}",
                            method, params_summary, elapsed_ms, e.message
                        ),
                    }

                    let response = match result {
                        Ok(value) => JsonRpcResponse::success(id, value),
                        Err(error) => JsonRpcResponse::error(id, error),
                    };

                    let body = serde_json::to_string(&response)
                        .unwrap_or_else(|_| r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"}}"#.to_string());
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap_or_else(|_| {
                            Response::new(axum::body::Body::from(
                                r#"{"jsonrpc":"2.0","error":{"code":-32603,"message":"Internal error"}}"#,
                            ))
                        })
                }),
            )
            .route("/health", get(health_check));

        let host_addr = if host.is_empty() {
            "127.0.0.1"
        } else {
            host.as_str()
        };
        let addr: SocketAddr = format!("{host_addr}:{port}").parse()?;

        let listener = tokio::net::TcpListener::bind(&addr).await?;

        serve(listener, app.into_make_service()).await?;

        Ok(())
    }
}

async fn health_check() -> Response<axum::body::Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from("{\"status\": \"ok\"}"))
        .unwrap_or_else(|_| Response::new(axum::body::Body::from("{\"status\": \"ok\"}")))
}
