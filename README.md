# mewcode

A hyper-sick terminal coding agent, written in Rust.

## Goals

- Learn Rust by porting a real, well-designed TypeScript project.
- Native, single-binary CLI with no JS runtime dependency.
- Streaming chat, local-project tools, and persistent sessions.
- A UI that actually feels good to use.

## Architecture

```
crates/
  protocol/  protocol types (no I/O)
  engine/    rig-based agent harness, tools, streaming
  server/    axum backend + local session store
  client/    ratatui terminal UI
```

One binary, two main subcommands:

```
mewcode server   # axum server on $MEWCODE_PORT (default 3737)
mewcode tui      # ratatui client
```

## Getting started

Prerequisites:
- Rust stable (1.85+), edition 2024

```bash
cp .env.example .env
# fill in OPENCODE_GO_API_KEY

cargo run -- tui
```

## Server API

- **Run server with command**:

```bash
cargo run -p mewcode-server
```

- **Swagger UI** — http://127.0.0.1:3737/swagger-ui/

## License

MIT
