# mew-mcp

Go MCP (Model Context Protocol) server that exposes [mew-core](../mew-core) workflow tools to external AI agents — Claude, Cursor, Hermes, or any MCP-compatible client.

## How it works

```
External Agent (Claude/Cursor/Hermes)
       ↕  MCP stdio (JSON-RPC 2.0)
   mew-mcp (Go)
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

## Architecture

```
mew-mcp/
├── cmd/mew-mcp/main.go       # Entry point — flags, env, starts stdio server
├── internal/mew/client.go    # HTTP client for mewcode-server REST API
└── internal/mcp/
    ├── server.go             # JSON-RPC 2.0 over stdio (MCP transport)
    └── tools.go              # Tool registry + handlers (workflow-level)
```

The MCP protocol is implemented from scratch using only the Go standard library — no external SDK dependency. MCP over stdio is JSON-RPC 2.0 with line-delimited messages, which is straightforward to implement correctly with `encoding/json` and `bufio`.
