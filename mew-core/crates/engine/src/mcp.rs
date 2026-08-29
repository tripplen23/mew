//! MCP **client**: connect to external MCP servers declared in `mew.toml`
//! and expose their tools as ordinary Mew tools.
//!
//! Each remote tool is wrapped in a [`ToolContracts`] impl and registered in
//! the normal [`ToolRegistry`](crate::tools::ToolRegistry), so it inherits the
//! approval gate, Plan/Build mode policy, the system-prompt tool listing, and
//! tracing for free. Rig's own `rmcp_tools` agent builder would bypass all of
//! that.
//!
//! This is the mirror image of `mew-mcp`, the Go MCP *server* that exposes Mew
//! to other agents.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use mewcode_protocol::{ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolOutput};
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool as RemoteTool};
use rmcp::service::{RoleClient, RunningService, ServerSink};
use rmcp::transport::TokioChildProcess;
use serde::Deserialize;
use serde_json::{Value, json};

// Provider-side cap on tool names (OpenAI and Anthropic both stop at 64).
const MAX_TOOL_NAME: usize = 64;

/// One external MCP server, keyed by name in the `[mcp]` config table.
///
/// ```toml
/// [default.mcp.firecrawl]
/// command = ["npx", "-y", "firecrawl-mcp"]
/// environment = { FIRECRAWL_API_KEY = "${FIRECRAWL_API_KEY}" }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    /// argv of a stdio MCP server. The first element is the executable.
    pub command: Vec<String>,
    /// Extra environment variables for the child process.
    #[serde(default, alias = "env")]
    pub environment: BTreeMap<String, String>,
    /// Set `false` to keep the entry in config but skip connecting.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

/// Live connections to every reachable configured MCP server.
///
/// Keep this alive for as long as the tools are in use: dropping a
/// `RunningService` kills its child process.
pub struct McpClients {
    // Held only for their `Drop`; the tools talk over cloned peer handles.
    _services: Vec<RunningService<RoleClient, ()>>,
    tools: Vec<Arc<dyn ToolContracts>>,
}

impl McpClients {
    /// Spawn and handshake every enabled server, then discover its tools.
    ///
    /// A server that fails to start or handshake is logged and skipped; Mew
    /// stays usable without it.
    // ponytail: connect-once at startup — no reconnect, no
    // `tools/list_changed` handling. A server that dies mid-session makes its
    // tools fail until Mew restarts. Upgrade path: a supervisor task that
    // re-serves the transport with backoff and swaps the peer handle.
    pub async fn connect(servers: &BTreeMap<String, McpServerConfig>) -> Self {
        let mut clients = Self {
            _services: Vec::new(),
            tools: Vec::new(),
        };
        for (name, config) in servers.iter().filter(|(_, c)| c.enabled) {
            match connect_one(name, config).await {
                Ok((service, tools)) => {
                    tracing::info!(server = %name, tools = tools.len(), "MCP server connected");
                    clients._services.push(service);
                    clients.tools.extend(tools);
                }
                Err(error) => {
                    tracing::warn!(server = %name, %error, "MCP server unavailable, skipping");
                }
            }
        }
        clients
    }

    /// Tools discovered across all connected servers.
    pub fn tools(&self) -> &[Arc<dyn ToolContracts>] {
        &self.tools
    }
}

async fn connect_one(
    server: &str,
    config: &McpServerConfig,
) -> anyhow::Result<(RunningService<RoleClient, ()>, Vec<Arc<dyn ToolContracts>>)> {
    let (program, args) = config
        .command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("`command` is empty"))?;

    let mut command = tokio::process::Command::new(program);
    command.args(args);
    // ponytail: the child inherits Mew's environment plus the declared vars,
    // matching OpenCode and Claude Code. Every secret in Mew's env is therefore
    // visible to the server. Upgrade path: `env_clear()` plus an allowlist.
    command.envs(&config.environment);

    let service = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        ().serve(TokioChildProcess::new(command)?),
    )
    .await
    .map_err(|_| anyhow::anyhow!("MCP handshake timed out after 10s"))??;
    let peer = service.peer().to_owned();
    let tools = tokio::time::timeout(std::time::Duration::from_secs(10), service.list_all_tools())
        .await
        .map_err(|_| anyhow::anyhow!("MCP tool discovery timed out after 10s"))??;
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::new();
    for remote in tools {
        let qualified = qualified_name(server, &remote.name);
        if !seen.insert(qualified.clone()) {
            tracing::warn!(server = %server, tool = %remote.name, qualified = %qualified, "MCP tool name collision after normalization, skipping duplicate");
            continue;
        }
        deduped.push(Arc::new(McpTool::new_with_name(
            server,
            Box::leak(qualified.into_boxed_str()),
            remote,
            peer.clone(),
        )) as Arc<dyn ToolContracts>);
    }
    Ok((service, deduped))
}

// One remote MCP tool, callable through Mew's tool registry.
struct McpTool {
    // `mcp_<server>_<tool>`. Leaked once at startup because `ToolContracts::name` needs `&'static str`.
    name: &'static str,
    // The tool's name on the remote server.
    remote_name: Cow<'static, str>,
    peer: ServerSink,
    descriptor: ToolDescriptor,
}

impl McpTool {
    #[allow(dead_code)]
    fn new(server: &str, remote: RemoteTool, peer: ServerSink) -> Self {
        let name: &'static str = Box::leak(qualified_name(server, &remote.name).into_boxed_str());
        Self::new_with_name(server, name, remote, peer)
    }

    fn new_with_name(
        server: &str,
        name: &'static str,
        remote: RemoteTool,
        peer: ServerSink,
    ) -> Self {
        Self {
            name,
            descriptor: descriptor_for(server, name, &remote),
            remote_name: remote.name,
            peer,
        }
    }
}

#[async_trait]
impl ToolContracts for McpTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn descriptor(&self) -> ToolDescriptor {
        self.descriptor.clone()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let mut request = CallToolRequestParams::new(self.remote_name.clone());
        request.arguments = match input {
            Value::Object(arguments) => Some(arguments),
            Value::Null => None,
            other => {
                return Err(ToolError::invalid_input(
                    format!("expected a JSON object of arguments, got {other}"),
                    "pass the tool arguments as a JSON object",
                ));
            }
        };
        let result = self
            .peer
            .call_tool(request)
            .await
            .map_err(|error| ToolError::Other {
                message: format!("MCP call to `{}` failed: {error}", self.remote_name),
                hint: Some("the MCP server may have exited; restart Mew to reconnect".into()),
            })?;
        if result.is_error.unwrap_or(false) {
            return Err(ToolError::Other {
                message: result_text(&result),
                hint: None,
            });
        }
        Ok(tool_output(result))
    }
}

// Map an MCP tool declaration onto Mew's descriptor. Missing annotation
// hints default to the cautious reading — mutating, non-idempotent,
// open-world — so an unannotated tool ends up behind the approval gate.
fn descriptor_for(server: &str, name: &'static str, remote: &RemoteTool) -> ToolDescriptor {
    let hints = remote.annotations.as_ref();
    let read_only = hints.and_then(|a| a.read_only_hint).unwrap_or(false);
    ToolDescriptor {
        name: name.to_string(),
        description: format!(
            "[MCP: {server}] {}",
            remote.description.as_deref().unwrap_or(name)
        ),
        input_schema: Value::Object(remote.input_schema.as_ref().clone()),
        annotations: ToolAnnotations {
            read_only,
            destructive: hints.and_then(|a| a.destructive_hint).unwrap_or(!read_only),
            open_world: hints.and_then(|a| a.open_world_hint).unwrap_or(true),
            idempotent: hints.and_then(|a| a.idempotent_hint).unwrap_or(false),
            approval_exempt: false,
        },
        examples: Vec::new(),
        max_response_chars: mewcode_protocol::DEFAULT_MAX_RESPONSE_CHARS,
    }
}

// Prefix a remote tool name with `mcp_<server>_` so it can never collide with
// a built-in tool, and sanitise it to `[A-Za-z0-9_-]{1,64}`.
fn qualified_name(server: &str, remote: &str) -> String {
    let mut name: String = format!("mcp_{server}_{remote}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    name.truncate(MAX_TOOL_NAME);
    name
}

// Prefer the server's structured result; otherwise hand the model the concatenated text blocks.
fn tool_output(result: CallToolResult) -> ToolOutput {
    if let Some(structured) = result.structured_content {
        return ToolOutput(structured);
    }
    let text = result_text(&result);
    if text.is_empty() {
        // Non-text content (image, audio, resource link) has no model-facing
        // text form here; say so rather than returning a silent empty string.
        return ToolOutput(json!({ "content_blocks": result.content.len() }));
    }
    ToolOutput::text(text)
}

fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text().map(|text| text.text.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(name: &'static str, annotations: Option<rmcp::model::ToolAnnotations>) -> RemoteTool {
        let schema = Arc::new(serde_json::Map::from_iter([(
            "type".to_string(),
            json!("object"),
        )]));
        let mut tool = RemoteTool::new(name, "does a thing", schema);
        tool.annotations = annotations;
        tool
    }

    #[test]
    fn names_are_prefixed_sanitised_and_bounded() {
        assert_eq!(
            qualified_name("firecrawl", "firecrawl_scrape"),
            "mcp_firecrawl_firecrawl_scrape"
        );
        assert_eq!(qualified_name("my.server", "do/it"), "mcp_my_server_do_it");
        assert_eq!(qualified_name("s", &"x".repeat(100)).len(), MAX_TOOL_NAME);
    }

    #[test]
    fn unannotated_tool_is_treated_as_mutating() {
        let descriptor = descriptor_for("fc", "mcp_fc_scrape", &remote("scrape", None));
        assert!(!descriptor.annotations.read_only);
        assert!(descriptor.annotations.destructive);
        assert!(descriptor.annotations.open_world);
        assert!(!descriptor.annotations.approval_exempt);
    }

    #[test]
    fn read_only_hint_is_honoured() {
        let hints = rmcp::model::ToolAnnotations::new().read_only(true);
        let descriptor = descriptor_for("fc", "mcp_fc_search", &remote("search", Some(hints)));
        assert!(descriptor.annotations.read_only);
        assert!(!descriptor.annotations.destructive);
        assert!(descriptor.description.starts_with("[MCP: fc] "));
    }
}
