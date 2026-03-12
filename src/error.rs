use std::path::PathBuf;

use rmcp::ErrorData;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid parameter: {0}")]
    InvalidParams(String),

    #[error("configured gemini binary was not found: {configured}")]
    BinaryNotFound { configured: String },

    #[error("working directory does not exist or is not a directory: {0}")]
    InvalidWorkingDirectory(PathBuf),

    #[error("working directory is outside allowed prefixes: {cwd}")]
    WorkingDirectoryNotAllowed {
        cwd: PathBuf,
        allowed_prefixes: Vec<PathBuf>,
    },

    #[error("requested model '{model}' is not in the configured allowlist")]
    ModelNotAllowed { model: String, allowed: Vec<String> },

    #[error("requested timeout {requested_secs}s exceeds configured max {max_secs}s")]
    TimeoutTooLarge { requested_secs: u64, max_secs: u64 },

    #[error("invalid environment configuration: {0}")]
    InvalidConfiguration(String),

    #[error("failed to spawn gemini process: {0}")]
    SpawnProcess(#[source] std::io::Error),

    #[error("failed to wait for gemini process: {0}")]
    WaitProcess(#[source] std::io::Error),

    #[error("failed to terminate timed out gemini process: {0}")]
    KillProcess(#[source] std::io::Error),

    #[error("gemini process did not expose stdout")]
    MissingStdout,

    #[error("gemini process did not expose stderr")]
    MissingStderr,

    #[error("failed to join subprocess reader task: {0}")]
    JoinTask(#[from] tokio::task::JoinError),

    #[error("failed to parse stream-json line: {line}")]
    ParseStreamLine {
        line: String,
        #[source]
        source: serde_json::Error,
    },
}

impl AppError {
    pub fn is_invalid_params(&self) -> bool {
        matches!(
            self,
            Self::InvalidParams(_)
                | Self::InvalidWorkingDirectory(_)
                | Self::WorkingDirectoryNotAllowed { .. }
                | Self::ModelNotAllowed { .. }
                | Self::TimeoutTooLarge { .. }
        )
    }

    pub fn to_error_data(&self) -> ErrorData {
        if self.is_invalid_params() {
            ErrorData::invalid_params(self.to_string(), None)
        } else {
            ErrorData::internal_error(self.to_string(), None)
        }
    }

    pub fn to_error_payload(&self) -> serde_json::Value {
        match self {
            Self::ModelNotAllowed { model, allowed } => json!({
                "status": "error",
                "error": {
                    "type": "model_not_allowed",
                    "message": self.to_string(),
                    "details": {
                        "model": model,
                        "allowed": allowed,
                    }
                }
            }),
            Self::WorkingDirectoryNotAllowed {
                cwd,
                allowed_prefixes,
            } => json!({
                "status": "error",
                "error": {
                    "type": "cwd_not_allowed",
                    "message": self.to_string(),
                    "details": {
                        "cwd": cwd,
                        "allowed_prefixes": allowed_prefixes,
                    }
                }
            }),
            _ => json!({
                "status": "error",
                "error": {
                    "type": "server_error",
                    "message": self.to_string(),
                }
            }),
        }
    }
}
