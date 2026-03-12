# Project Context for AI Agents

This document provides context for AI agents working on the `gemini-mcp-server`
codebase (located in `experimental/gemini-mcp-server/`).

## Purpose

This project is a standalone Rust crate that implements a
[Model Context Protocol (MCP)](https://modelcontextprotocol.io) server. It acts
as a bridge between external MCP clients (agents, IDEs) and the locally
installed `gemini` CLI.

The core functionality is to spawn a `gemini` subprocess, execute a task in
headless mode, parse the structured `stream-json` output, and return the results
to the MCP client.

## Tech Stack

- **Language**: Rust 2024 Edition.
- **Async Runtime**: Tokio
  (`tokio = { version = "1.50.0", features = ["full", "signal"] }`).
- **MCP Framework**:
  `rmcp = { version = "1.2.0", features = ["server", "transport-io", "macros"] }`.
- **Serialization**: `serde` and `serde_json` with `schemars` for JSON schema
  generation.
- **Error Handling**: `thiserror` for typed errors, `anyhow` for main entrypoint
  error propagation.

## Project Structure

```
experimental/gemini-mcp-server/
├── Cargo.toml          # Manifest and dependencies
├── src/
│   ├── main.rs           # Entry point (tokio::main), initializes tracing and stdio transport
│   ├── lib.rs            # Library exports
│   ├── server.rs         # GeminiMcpServer struct and MCP tool definitions
│   ├── config.rs         # AppConfig struct and environment variable parsing
│   ├── error.rs          # AppError enum and conversions to MCP error types
│   ├── types.rs          # Request/Response schemas (GeminiRunRequest, GeminiRunResponse, etc.)
│   └── gemini/
│       ├── mod.rs      # Module exports
│       ├── process.rs  # GeminiRunner (subprocess spawning and stdout/stderr collection)
│       └── events.rs   # parse_stream_line() for JSONL parsing logic
└── tests/              # (Not present yet, unit tests are inline in gemini/process.rs)
```

## Key Modules

### `src/server.rs` (MCP Layer)

Contains the `GeminiMcpServer` struct which implements `rmcp::ServerHandler`.

- Uses the `#[tool_router]` macro to define tool methods.
- Uses the `#[tool_handler]` macro to implement `ServerHandler` trait methods
  like `get_info()`.
- **Tools exposed**:
  1. `gemini_cli_health`: Returns info about the configured `gemini` binary.
  2. `gemini_cli_run`: Executes a prompt and returns the result.

### `src/gemini/process.rs` (Execution Layer)

Contains the `GeminiRunner` struct.

- `check_health()`: Runs `gemini --version` to validate the binary.
- `run()`: Runs `gemini -p <prompt> --output-format stream-json` inside a tokio
  async task.
  - Handles timeouts via `tokio::time::timeout`.
  - Spawns tasks to read stdout (as JSONL events) and stderr.
  - Parses events into `GeminiStreamEvent` enums.
  - Aggregates the final assistant response from `message` events.
  - Constructs the final `GeminiRunResponse`.

### `src/config.rs` (Configuration Layer)

Contains `AppConfig` struct.

- `from_env()`: Loads configuration from environment variables (`GEMINI_BIN`,
  `GEMINI_DEFAULT_TIMEOUT_SECS`, etc.).
- `resolve_binary_path()`: Resolves the gemini binary name using system `PATH`.
- `resolve_cwd()`: Validates working directory against allowlist.
- `resolve_model()`: Validates model names against `GEMINI_ALLOWED_MODELS`.

### `src/types.rs` (Schemas)

Defines strongly-typed structs for MCP tool inputs and outputs:

- `GeminiRunRequest`: Input arguments for `gemini_cli_run`.
- `GeminiRunResponse`: Output struct containing status, response, stats, events,
  etc.
- `GeminiStreamEvent`: Enum representing `stream-json` events (`Init`,
  `Message`, `ToolUse`, `ToolResult`, `Error`, `Result`).

### `src/error.rs` (Error Handling)

Defines `AppError` enum covering subprocess, parsing, and validation failures.

- `to_error_data()`: Converts application errors into `rmcp::ErrorData` for MCP
  protocol transmission (distinguishes between `invalid_params` and internal
  errors).
- `to_error_payload()`: Generates structured JSON error payloads for specific
  cases (e.g., `model_not_allowed`, `cwd_not_allowed`).

## Development

### Building

```bash
cd experimental/gemini-mcp-server
cargo build --release
```

### Testing

The crate uses inline unit tests within `src/gemini/process.rs` (under
`#[cfg(test)]`).

- Tests use `tempfile` to create mock executable scripts for `GeminiRunner`.
- Tests cover:
  - Health checks (parsing version output).
  - Run success cases (parsing stream-json events).
  - Timeout handling.
- Run tests with `cargo test`.

### Lints

The project uses a strict Clippy configuration in `Cargo.toml`:

- `uninit_assumed_init = "deny"`
- `all = { level = "warn", priority = -1 }`

## Design Decisions

- **Why `rmcp` 1.2.0?**: This is the latest stable version at the time of
  writing and provides the stdio transport and macro system for tools.
- **Why stdio transport?**: It is the most compatible transport for MCP clients
  (IDEs, orchestration layers).
- **Why `stream-json` format?**: It is the official structured output format
  from the `gemini` CLI, providing per-turn events (init, message, tool_use,
  tool_result, error, result). This allows the server to extract the final
  assistant response and stats accurately.
- **Why separate crate?**: This crate is Rust-based, while the main `gemini-cli`
  project is a Node.js monorepo. Keeping it separate in `experimental/` avoids
  build system complexity and Rust/JS interop issues at build time.

## Interaction with Parent Project

This crate depends on the parent `gemini-cli` project only at runtime (by
invoking the `gemini` binary). It does not link against any TypeScript/Node.js
code from the parent project.

- Headless mode behavior is documented in `../docs/cli/headless.md`.
- Stream JSON format is defined in `../packages/core/src/output/types.ts`.
