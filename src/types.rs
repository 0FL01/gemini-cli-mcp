use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GeminiRunRequest {
    #[schemars(description = "Prompt to send to `gemini -p`.")]
    pub prompt: String,

    #[schemars(description = "Working directory for the gemini subprocess.")]
    pub cwd: Option<String>,

    #[schemars(description = "Optional timeout override in seconds.")]
    pub timeout_secs: Option<u64>,

    #[schemars(description = "Optional `--model` override.")]
    pub model: Option<String>,

    #[schemars(description = "Include parsed stream-json events in the response.")]
    pub include_events: Option<bool>,

    #[schemars(description = "Always include stderr in the response, even on success.")]
    pub include_stderr: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ErrorInfo {
    pub r#type: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ModelStreamStats {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached: u64,
    pub input: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct StreamStats {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached: u64,
    pub input: u64,
    pub duration_ms: u64,
    pub tool_calls: u64,
    pub models: BTreeMap<String, ModelStreamStats>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolResultStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GeminiStreamEvent {
    Init {
        timestamp: String,
        session_id: String,
        model: String,
    },
    Message {
        timestamp: String,
        role: MessageRole,
        content: String,
        delta: Option<bool>,
    },
    ToolUse {
        timestamp: String,
        tool_name: String,
        tool_id: String,
        parameters: serde_json::Value,
    },
    ToolResult {
        timestamp: String,
        tool_id: String,
        status: ToolResultStatus,
        output: Option<String>,
        error: Option<ErrorInfo>,
    },
    Error {
        timestamp: String,
        severity: ErrorSeverity,
        message: String,
    },
    Result {
        timestamp: String,
        status: RunStatus,
        error: Option<ErrorInfo>,
        stats: Option<StreamStats>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GeminiRunResponse {
    pub status: RunStatus,
    pub response: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub stats: Option<StreamStats>,
    pub error: Option<ErrorInfo>,
    pub exit_code: i32,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub gemini_bin: String,
    pub resolved_bin: Option<String>,
    pub cwd: String,
    pub invocation: Vec<String>,
    pub stderr: Option<String>,
    pub events: Option<Vec<GeminiStreamEvent>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GeminiHealthResponse {
    pub status: HealthStatus,
    pub gemini_bin: String,
    pub resolved_bin: Option<String>,
    pub version_output: String,
    pub stderr: Option<String>,
}
