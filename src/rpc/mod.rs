// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! RPC Server Module (PR-4 Stage C)
//!
//! Implements JSON-RPC 2.0 protocol over HTTP.

pub mod handlers;
pub mod protocol;
pub mod server;

pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::RpcServer;
pub use server::RpcServerBuilder;
