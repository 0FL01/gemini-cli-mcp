use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use serde_json::to_value;

use crate::{
    config::AppConfig,
    gemini::process::GeminiRunner,
    types::{GeminiHealthResponse, GeminiRunRequest},
};

#[derive(Debug, Clone)]
pub struct GeminiMcpServer {
    runner: GeminiRunner,
    tool_router: ToolRouter<Self>,
}

impl GeminiMcpServer {
    pub fn new(config: AppConfig) -> Self {
        Self {
            runner: GeminiRunner::new(config),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl GeminiMcpServer {
    #[tool(
        name = "gemini_cli_health",
        description = "Check the configured system gemini binary and report its resolved path and version output."
    )]
    pub async fn gemini_cli_health(&self) -> Result<rmcp::Json<GeminiHealthResponse>, ErrorData> {
        let response = self
            .runner
            .check_health()
            .await
            .map_err(|error| error.to_error_data())?;
        Ok(rmcp::Json(response))
    }

    #[tool(
        name = "gemini_cli_run",
        description = "Run one headless Gemini CLI task via `gemini -p ... --output-format stream-json` and return the final response plus optional events."
    )]
    pub async fn gemini_cli_run(
        &self,
        Parameters(request): Parameters<GeminiRunRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        match self.runner.run(request).await {
            Ok(response) => {
                let value = to_value(&response).map_err(|error| {
                    ErrorData::internal_error(
                        format!("failed to serialize tool response: {error}"),
                        None,
                    )
                })?;
                let result = match response.status {
                    crate::types::RunStatus::Success => CallToolResult::structured(value),
                    crate::types::RunStatus::Error => CallToolResult::structured_error(value),
                };
                Ok(result)
            }
            Err(error) if error.is_invalid_params() => Err(error.to_error_data()),
            Err(error) => Ok(CallToolResult::structured_error(error.to_error_payload())),
        }
    }
}

#[tool_handler]
impl ServerHandler for GeminiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "This server proxies a local system `gemini` CLI binary through MCP. Use `gemini_cli_run` for task execution and prefer structured inputs/outputs.",
            )
    }
}
