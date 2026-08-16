// Copyright (c) 2026 MCode
//
// Licensed under either of Apache License, Version 2.0 or MIT License at your option.

//! mcc-mcp: MCP (Model Context Protocol) server for the MCode compiler.
//!
//! Exposes mcc compiler capabilities as MCP tools so AI agents can drive the
//! design -> code -> compile -> debug -> verify loop. This binary is a thin
//! protocol adapter: every tool delegates to the existing JSON-RPC handlers /
//! libmcc API. See mcd/doc/mcp/mcc-mcp-server-design.md for the design.

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ErrorCode, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, Json, ServerHandler,
    ServiceExt,
};
use serde::Deserialize;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Tool request schemas (JSON Schema is derived automatically)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ValidateRequest {
    /// Inline .mc source snippet; no disk file needed.
    #[schemars(description = "Inline .mc source snippet to validate; no disk file needed")]
    pub content: String,
    /// System library names to load, e.g. mcode.
    #[serde(default)]
    #[schemars(description = "System library names to load, e.g. [\"mcode\"]")]
    pub libs: Vec<String>,
    /// Treat any warning as an error.
    #[serde(default)]
    pub strict: bool,
    /// Report errors only, ignore warnings.
    #[serde(default)]
    pub errors_only: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ParseRequest {
    /// Absolute path of the .mc file to parse.
    #[schemars(description = "Absolute path of the .mc file to parse")]
    pub file_path: String,
    /// Include system library definitions in the output.
    #[serde(default)]
    pub include_system: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExplainRequest {
    /// Error code, e.g. 2008. Omit to return the full error code table.
    pub code: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LoadProjectRequest {
    /// Project entry .mc file path; loads it and its use-dependencies.
    #[schemars(description = "Project entry .mc file path; loads it and its use-dependencies")]
    pub entry: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckProjectRequest {
    /// Project entry .mc file path. Required when no Project workspace is
    /// active; optional when a project was loaded via mcc_load_project.
    pub entry: Option<String>,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub errors_only: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckFileRequest {
    /// Absolute path of the .mc file to check.
    pub file_path: String,
    #[serde(default)]
    pub libs: Vec<String>,
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub errors_only: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BuildRequest {
    /// Entry .mc file path; omitted uses the active project.
    pub entry: Option<String>,
    /// Top-level module name; omitted auto-guesses the first module.
    pub top: Option<String>,
    /// Include system library definitions, default true.
    #[serde(default = "default_true")]
    pub include_system: bool,
    #[serde(default)]
    pub libs: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchDefsRequest {
    /// Text, regex, or fuzzy pattern to match against definition names.
    pub pattern: String,
    /// Definition kind filter: component | module | interface | enum | instance.
    pub kind: Option<String>,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub fuzzy: bool,
    /// Top module name to restrict the search.
    pub top: Option<String>,
    /// Maximum number of results, 0 = no limit.
    #[serde(default)]
    pub limit: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ShowDefRequest {
    /// Definition name (component / module / interface / enum / instance).
    pub name: String,
    /// Definition type: component | module | interface | enum | instance.
    pub type_filter: Option<String>,
    /// Source file of the definition.
    pub file: Option<String>,
    /// Top module name.
    pub top: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct LookupRequest {
    /// Class name to resolve, e.g. uC or uC.PA1 (compound reference).
    #[serde(rename = "className")]
    pub class_name: String,
    /// Sub-element name.
    #[serde(rename = "subName")]
    pub sub_name: Option<String>,
    /// Sub-element kind: pin | port | param | func | ...
    #[serde(rename = "subKind")]
    pub sub_kind: Option<String>,
    /// Resolve relative to this source file.
    #[serde(rename = "fromUri")]
    pub from_uri: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExportRequest {
    /// Export kind: netlist | bom | spice | kicad. Default netlist.
    #[serde(default)]
    pub kind: String,
    /// Source .mc file path.
    #[schemars(description = "Source .mc file path to export from")]
    pub entry: String,
    /// Top module name; omitted defaults to the first module.
    pub top: Option<String>,
    /// Output format: text | json | csv. Default text.
    pub format: Option<String>,
    #[serde(default)]
    pub libs: Vec<String>,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct MccMcpServer {
    // Consumed by the generated `#[tool_router]` impl at runtime.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl MccMcpServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl MccMcpServer {
    /// Validate an inline MCode snippet and return diagnostics (E2xxx/E3xxx).
    /// This is the primary AI iterate loop: generate -> validate -> fix.
    #[tool(
        description = "Validate an inline MCode snippet and return diagnostics (E2xxx/E3xxx). Primary AI loop: generate -> validate -> fix."
    )]
    fn mcc_validate_component(
        &self,
        Parameters(req): Parameters<ValidateRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "content": req.content,
            "libs": req.libs,
            "strict": req.strict,
            "errors_only": req.errors_only,
        });
        rpc_to_mcp(mcc::rpc::handlers::handle_check(Some(params)), "check")
    }

    /// Parse a .mc file and return an AST summary plus diagnostics.
    #[tool(description = "Parse a .mc file; returns AST summary and diagnostics")]
    fn mcc_parse_file(
        &self,
        Parameters(req): Parameters<ParseRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "entry": req.file_path,
            "include_system": req.include_system,
        });
        rpc_to_mcp(mcc::rpc::handlers::handle_parse(Some(params)), "parse")
    }

    /// Explain an mcc error code, or list all error codes when omitted.
    #[tool(description = "Explain an mcc error code (e.g. 2008); omit code for the full table")]
    fn mcc_explain_error(
        &self,
        Parameters(req): Parameters<ExplainRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = req.code.map(|code| json!({ "code": code }));
        rpc_to_mcp(mcc::rpc::handlers::handle_explain(params), "explain")
    }

    /// Load a project entry file and its use-dependencies into the workspace.
    #[tool(description = "Load a project entry .mc file and its dependencies into the workspace")]
    fn mcc_load_project(
        &self,
        Parameters(req): Parameters<LoadProjectRequest>,
    ) -> Result<Json<Value>, McpError> {
        // Derive the project root the same way the CLI does: walk up from the
        // entry looking for a project manifest (project.toml / manifest.toml /
        // mcc.toml); fall back to the entry's parent dir.
        let entry_path = std::path::Path::new(&req.entry);
        let mut current: Option<&std::path::Path> = if entry_path.is_dir() {
            Some(entry_path)
        } else {
            entry_path.parent()
        };
        let mut root: Option<std::path::PathBuf> = None;
        while let Some(dir) = current {
            if mcc::cli::datadir::find_manifest_in(dir).is_some() {
                root = Some(dir.to_path_buf());
                break;
            }
            current = dir.parent();
        }
        if root.is_none() {
            root = if entry_path.is_dir() {
                Some(entry_path.to_path_buf())
            } else {
                entry_path.parent().map(|p| p.to_path_buf())
            };
        }
        if let Some(project_root) = root.as_deref() {
            mcc::mcc_set_project_root(project_root);
        }
        let params = json!({ "entry": req.entry });
        rpc_to_mcp(
            mcc::rpc::handlers::handle_load_project(Some(params)),
            "load_project",
        )
    }

    /// Check a single .mc file (semantic check + diagnostics).
    #[tool(description = "Check a single .mc file; returns diagnostics")]
    fn mcc_check_file(
        &self,
        Parameters(req): Parameters<CheckFileRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "entry": req.file_path,
            "libs": req.libs,
            "strict": req.strict,
            "errors_only": req.errors_only,
        });
        rpc_to_mcp(mcc::rpc::handlers::handle_check(Some(params)), "check_file")
    }

    /// Check the whole active project (must be loaded first via mcc_load_project),
    /// or check a project entry file when `entry` is provided.
    #[tool(
        description = "Check the whole active project; or pass `entry` to check a project entry file"
    )]
    fn mcc_check_project(
        &self,
        Parameters(req): Parameters<CheckProjectRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "entry": req.entry,
            "strict": req.strict,
            "errors_only": req.errors_only,
        });
        rpc_to_mcp(
            mcc::rpc::handlers::handle_check(Some(params)),
            "check_project",
        )
    }

    /// Run Pass2 instantiation: module tree, connections, and nets.
    #[tool(description = "Run Pass2 instantiation; returns module tree, connections and nets")]
    fn mcc_build(
        &self,
        Parameters(req): Parameters<BuildRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "entry": req.entry,
            "top": req.top,
            "include_system": req.include_system,
            "libs": req.libs,
        });
        rpc_to_mcp(mcc::rpc::handlers::handle_build_full(Some(params)), "build")
    }

    /// Search loaded definitions by text / regex / fuzzy pattern.
    #[tool(
        description = "Search loaded definitions (component/module/interface/enum/instance) by pattern"
    )]
    fn mcc_search_defs(
        &self,
        Parameters(req): Parameters<SearchDefsRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "pattern": req.pattern,
            "kind": req.kind,
            "regex": req.regex,
            "fuzzy": req.fuzzy,
            "top": req.top,
            "limit": req.limit,
        });
        rpc_to_mcp(
            mcc::rpc::handlers::handle_defs_search(Some(params)),
            "search_defs",
        )
    }

    /// Show detailed definition info (pins, params, funcs, interfaces).
    #[tool(description = "Show detailed definition info: pins, params, funcs, interfaces")]
    fn mcc_show_def(
        &self,
        Parameters(req): Parameters<ShowDefRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "name": req.name,
            "type": req.type_filter,
            "file": req.file,
            "top": req.top,
        });
        rpc_to_mcp(
            mcc::rpc::handlers::handle_show_dump(Some(params)),
            "show_def",
        )
    }

    /// Resolve a symbol to its definition location (supports uC.PA1 compound refs).
    #[tool(
        description = "Resolve a symbol (supports uC.PA1 compound references) to its definition location"
    )]
    fn mcc_lookup(
        &self,
        Parameters(req): Parameters<LookupRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "className": req.class_name,
            "subName": req.sub_name,
            "subKind": req.sub_kind,
            "fromUri": req.from_uri,
        });
        rpc_to_mcp(
            mcc::rpc::handlers::handle_lookup_with_sub(Some(params)),
            "lookup",
        )
    }

    /// Electrical rule check on the active workspace: single-point nets,
    /// unconnected ports, multi-drive, floating nets.
    #[tool(description = "Electrical rule check on the active workspace")]
    fn mcc_erc(&self) -> Result<Json<Value>, McpError> {
        rpc_to_mcp(mcc::rpc::handlers::handle_erc(None), "erc")
    }

    /// Generate a netlist for the given .mc file.
    #[tool(description = "Generate a netlist (text/JSON) for a .mc file")]
    fn mcc_generate_netlist(
        &self,
        Parameters(req): Parameters<ExportRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "kind": "netlist",
            "entry": req.entry,
            "top": req.top,
            "format": req.format,
            "libs": req.libs,
        });
        rpc_to_mcp(
            mcc::rpc::handlers::handle_export(Some(params)),
            "generate_netlist",
        )
    }

    /// Export netlist / BOM / SPICE / KiCad for a .mc file.
    #[tool(description = "Export netlist / BOM / SPICE / KiCad for a .mc file")]
    fn mcc_export(
        &self,
        Parameters(req): Parameters<ExportRequest>,
    ) -> Result<Json<Value>, McpError> {
        let params = json!({
            "kind": req.kind,
            "entry": req.entry,
            "top": req.top,
            "format": req.format,
            "libs": req.libs,
        });
        rpc_to_mcp(mcc::rpc::handlers::handle_export(Some(params)), "export")
    }
}

/// `#[tool_handler]` fills in `call_tool` / `list_tools` / `get_tool` from the
/// generated `tool_router()`; `get_info` below is kept custom.
#[tool_handler(
    instructions = "MCode compiler tools for AI agents: validate, parse, check, build, search, export."
)]
impl ServerHandler for MccMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a JSON-RPC handler result onto an MCP tool result.
fn rpc_to_mcp(
    result: Result<Value, mcc::rpc::JsonRpcError>,
    op: &str,
) -> Result<Json<Value>, McpError> {
    match result {
        Ok(value) => Ok(Json(value)),
        Err(e) => Err(McpError::new(
            ErrorCode::INTERNAL_ERROR,
            format!("{op} failed: {}", e.message),
            Some(json!({ "rpc_code": e.code })),
        )),
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. System root: mcc_set_system_root handles MCC_SYSTEM_ROOT env and
    //    default probing (base/mc, base/mcode, data dir).
    mcc::mcc_set_system_root(std::path::Path::new(""));
    // 2. Init builder + load the mcode system library. mcc_init() calls
    //    mcb_init_system_lib(), which loads mcode by default unless
    //    libs.disable_mcode is set (see LibsConfig::should_load_mcode).
    mcc::mcc_init();
    // 3. Optional project binding (state model A: one process per project).
    if let Ok(project_root) = std::env::var("MCC_PROJECT_ROOT") {
        if !project_root.is_empty() {
            mcc::mcc_set_project_root(std::path::Path::new(&project_root));
        }
    }

    // 4. Serve MCP over stdio.
    let server = MccMcpServer::new().serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
