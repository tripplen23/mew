# mew-mcp

Go MCP (Model Context Protocol) server that exposes [mew-core](../mew-core) workflow tools to external AI agents — Claude, Cursor, Hermes, or any MCP-compatible client.

Built with the [official MCP Go SDK](https://github.com/modelcontextprotocol/go-sdk) (`github.com/modelcontextprotocol/go-sdk`), maintained in collaboration with Google. Implements MCP protocol version `2025-06-18` (negotiated automatically by the SDK).

## How it works

```
External Agent (Claude/Cursor/Hermes)
       ↕  MCP stdio (JSON-RPC 2.0, newline-delimited)
   mew-mcp (Go, official MCP Go SDK)
       ↕  HTTP / SSE
  mewcode-server (Rust)
```

`mew-mcp` is a thin adapter: it translates MCP tool calls into HTTP requests against the running `mewcode-server` REST API. No FFI, no embedded engine — the Rust server remains the single source of truth.

## Tools

| Tool | Description |
|------|-------------|
| `mew_health` | Check if mewcode-server is running. Returns service name and version. |
| `ask_mew` | Send a one-shot prompt to mew in BUILD mode. Creates a new session and returns the full assistant reply. |
| `continue_mew_session` | Send a follow-up message to an existing session. Returns the full assistant reply. |
| `list_mew_sessions` | List all sessions (newest first) with id, title, model, mode, and creation time. |
| `get_mew_session` | Get full session details including complete message history. |

Tool input/output schemas are auto-inferred from Go struct `jsonschema` tags by the MCP Go SDK — no manual JSON Schema authoring required. The SDK handles argument validation, marshaling, and protocol negotiation.

## Quick start

1. **Start mewcode-server** (from `mew-core/`):
   ```bash
   cd mew-core && cargo run --bin mewcode-server
   ```

2. **Build mew-mcp**:
   ```bash
   cd mew-mcp && go build -o mew-mcp ./cmd/mew-mcp
   ```

3. **Configure your agent** to launch `mew-mcp` as an MCP stdio server. For example, in Hermes `config.yaml`:
   ```yaml
   mcp_servers:
     mew:
       command: "/path/to/mew-mcp"
       env:
         MEWCODE_API_URL: "http://127.0.0.1:3737"
   ```

   Or for Claude Desktop (`claude_desktop_config.json`):
   ```json
   {
     "mcpServers": {
       "mew": {
         "command": "/path/to/mew-mcp",
         "env": { "MEWCODE_API_URL": "http://127.0.0.1:3737" }
       }
     }
   }
   ```

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `MEWCODE_API_URL` | `http://127.0.0.1:3737` | Base URL of the mewcode-server instance |

Or pass `--url` explicitly:
```bash
mew-mcp --url http://localhost:3737
```

## Development

```bash
cd mew-mcp
go test -v ./...    # run tests
go build ./cmd/mew-mcp   # build binary
go vet ./...        # static analysis
```

Requires Go 1.26+ (latest stable). The MCP Go SDK (`v1.6.1+`) requires Go 1.25+.

## Architecture

```
mew-mcp/
├── cmd/mew-mcp/main.go       # Entry point — flags, env, starts stdio server
├── internal/mew/client.go    # HTTP client for mewcode-server REST API
└── internal/mcp/tools.go     # Tool registration via official MCP Go SDK
```

### Protocol compliance

This server uses the [official MCP Go SDK](https://github.com/modelcontextprotocol/go-sdk) which implements the full [MCP specification](https://modelcontextprotocol.io/specification):
- JSON-RPC 2.0 over stdio (newline-delimited JSON)
- Protocol version negotiation (`initialize` handshake)
- Tool discovery (`tools/list`) with auto-inferred JSON Schema
- Tool invocation (`tools/call`) with argument validation
- Error handling per spec (`isError` field in tool results)

## License

MIT
