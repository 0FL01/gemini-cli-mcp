# Gemini MCP Server

A standalone [Model Context Protocol (MCP)](https://modelcontextprotocol.io)
server implementation in Rust that proxies tasks to a locally installed `gemini`
CLI binary.

This server enables external AI agents or MCP-compatible tools (e.g., in IDEs or
orchestration layers) to call the Gemini CLI as a backend service, leveraging
its official headless mode (`--output-format stream-json`) for structured
output.

## Architecture

```
[MCP Client / IDE / Agent] <---(stdio)--->
         [gemini-mcp-server] <---(subprocess)--->
             [system gemini CLI]
```

- **Transport**: Stdio (stdin/stdout).
- **Stack**: Rust 2024 Edition,
  [rmcp](https://github.com/modelcontextprotocol/rust-sdk) 1.2.0, Tokio.
- **Subprocess Integration**: Spawns the system `gemini` binary with `-p` and
  `--output-format stream-json`, parses the newline-delimited JSON (JSONL)
  events, and returns a structured response to the MCP client.

## Features

- **Headless Execution**: Uses `gemini -p <prompt> --output-format stream-json`
  to run tasks non-interactively.
- **Structured I/O**: Tools accept typed arguments and return structured JSON
  payloads with session metadata, stats, and optional raw events.
- **Configurable**:
  - Custom timeout limits (default 600s, max 1800s).
  - Model allowlist via `GEMINI_ALLOWED_MODELS`.
  - Working directory restrictions via `GEMINI_ALLOWED_CWD_PREFIXES`.
  - Configurable gemini binary path via `GEMINI_BIN`.
- **Safety**:
  - Does not expose arbitrary CLI flags to the MCP client.
  - Respects `cwd` restrictions.
  - Validates model names against an allowlist (if configured).
  - Graceful handling of process timeouts and errors.

## Configuration

Configure the server via environment variables:

| Variable                      | Description                                                                                                                                | Default  |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | -------- |
| `GEMINI_BIN`                  | Path to the gemini binary to use.                                                                                                          | `gemini` |
| `GEMINI_DEFAULT_TIMEOUT_SECS` | Default timeout for `gemini_cli_run` tasks.                                                                                                | `600`    |
| `GEMINI_MAX_TIMEOUT_SECS`     | Maximum allowed timeout for `gemini_cli_run` tasks.                                                                                        | `1800`   |
| `GEMINI_ALLOWED_MODELS`       | Comma-separated list of allowed model names (e.g., `gemini-2.5-pro,gemini-2.5-flash`). If unset, all models are allowed.                   | -        |
| `GEMINI_ALLOWED_CWD_PREFIXES` | Path separator (`:` on Unix, `;` on Windows) separated list of allowed working directory prefixes. If unset, current directory is allowed. | -        |

## Usage

### Building

```bash
cargo build --release
```

The compiled binary will be located at `target/release/gemini-mcp-server`.

### Running as an MCP Server

This server uses the stdio transport, meaning it communicates over standard
input and output. To use it:

1. **In an MCP Client (e.g., Cursor, Claude Desktop, etc.)**: Configure the
   server in your client's MCP settings, pointing to the `gemini-mcp-server`
   binary:

   ```json
   {
     "mcpServers": {
       "gemini-local": {
         "command": "/path/to/gemini-mcp-server",
         "args": [],
         "env": {
           "GEMINI_BIN": "/path/to/gemini"
         }
       }
     }
   }
   ```

2. **Manually (for testing)**: Since the server expects MCP protocol messages on
   stdin, running it directly in a terminal without an MCP client will not work
   interactively. However, you can pipe JSONL requests if you are implementing a
   custom client.

    For simple validation that the server starts and the `gemini` binary is
    accessible, you can run a health check if your client supports testing tools
    individually. The `gemini_cli_health` tool is designed for this.

### OpenCode Configuration

For using with [OpenCode](https://opencode.ai), add this to your `opencode.jsonc`:

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "gemini-mcp": {
      "type": "local",
      "command": [
        "sh", "-c",
        "GEMINI_BIN=gemini GEMINI_DEFAULT_TIMEOUT_SECS=600 GEMINI_MAX_TIMEOUT_SECS=1800 ./target/release/gemini-mcp-server"
      ],
      "enabled": true
    }
  }
}
```

**Configuration options:**
- `GEMINI_BIN`: Path to gemini binary (default: `gemini` from PATH)
- `GEMINI_DEFAULT_TIMEOUT_SECS`: Default timeout in seconds (default: 600)
- `GEMINI_MAX_TIMEOUT_SECS`: Maximum allowed timeout (default: 1800)
- `GEMINI_ALLOWED_MODELS`: Comma-separated allowlist of model names
- `GEMINI_ALLOWED_CWD_PREFIXES`: Path separator (`:` on Unix, `;` on Windows) separated list of allowed working directory prefixes

**Windows example:**
```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "gemini-mcp": {
      "type": "local",
      "command": [
        "cmd", "/c",
        "set GEMINI_BIN=gemini && set GEMINI_DEFAULT_TIMEOUT_SECS=600 && set GEMINI_MAX_TIMEOUT_SECS=1800 && .\\target\\release\\gemini-mcp-server.exe"
      ],
      "enabled": true
    }
  }
}
```

## Available Tools

### `gemini_cli_health`

Checks if the configured `gemini` binary is available and returns its version
output.

**Arguments**: None.

**Returns**:

```json
{
  "status": "ok",
  "gemini_bin": "gemini",
  "resolved_bin": "/home/user/.npm-global/bin/gemini",
  "version_output": "gemini 9.9.9",
  "stderr": null
}
```

### `gemini_cli_run`

Executes a single headless task using `gemini -p` and returns the final
assistant response, plus optional metadata and events.

**Arguments**:

| Field | Type | Required | Description |
|-------|--------|-----------|-------------|
| `prompt` | string | Yes | The prompt to send to `gemini -p`. |
| `cwd` | string or null | No | Working directory for the subprocess (relative paths resolved against the server's working dir). |
| `timeout_secs` | integer (uint64) or null | No | Timeout override for this specific invocation (must be <= configured max). |
| `model` | string or null | No | Optional `--model` override. Must be in the `GEMINI_ALLOWED_MODELS` allowlist if configured. |
| `include_events` | boolean | No | If true, includes the parsed `stream-json` events in the `events` field. |
| `include_stderr` | boolean | No | If true, includes stderr output even on success. |

**Returns** (Success):

```json
{
  "status": "success",
  "response": "The assistant's final response text.",
  "session_id": "session-abc123",
  "model": "gemini-2.5-flash",
  "stats": {
    "total_tokens": 100,
    "input_tokens": 50,
    "output_tokens": 50,
    "cached": 0,
    "input": 50,
    "duration_ms": 1200,
    "tool_calls": 0,
    "models": {}
  },
  "error": null,
  "exit_code": 0,
  "timed_out": false,
  "duration_ms": 1250,
  "gemini_bin": "gemini",
  "resolved_bin": "/home/user/.npm-global/bin/gemini",
  "cwd": "/home/user/project",
  "invocation": ["gemini", "-p", "my prompt", "--output-format", "stream-json"],
  "stderr": null,
  "events": [...]
}
```

**Returns** (Error):

```json
{
  "status": "error",
  "response": "",
  "session_id": null,
  "model": null,
  "stats": null,
  "error": {
    "type": "timeout",
    "message": "gemini process exceeded timeout of 600s"
  },
  "exit_code": -1,
  "timed_out": true,
  "duration_ms": 600000,
  "gemini_bin": "gemini",
  "resolved_bin": "/home/user/.npm-global/bin/gemini",
  "cwd": "/home/user/project",
  "invocation": ["gemini", "-p", "...", "--output-format", "stream-json"],
  "stderr": "Error: ...\n",
  "events": [...]
}
```

## Development & Testing

This crate uses `cargo` for dependency management and testing.

- **Run tests**:

  ```bash
  cargo test
  ```

  Tests include unit tests for stream-json event parsing and subprocess
  simulation using the `tempfile` crate.

- **Run lints**:
  ```bash
  cargo clippy
  ```

## Project Context

This server provides a bridge between MCP clients and the powerful capabilities of the
`gemini` CLI without requiring direct integration into gemini-cli's Node.js/TypeScript
build system.

For more details on the `gemini` CLI's headless mode and `stream-json` format,
refer to the gemini-cli project:

- https://github.com/agent-frameworks/gemini-cli

## License

Apache-2.0
