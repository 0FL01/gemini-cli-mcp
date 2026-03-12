use anyhow::Context;
use gemini_mcp_server::{AppConfig, GeminiMcpServer};
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let config = AppConfig::from_env().context("failed to load server configuration")?;
    let server = GeminiMcpServer::new(config);
    let transport = stdio();
    let running = server
        .serve(transport)
        .await
        .context("failed to start MCP server")?;

    running
        .waiting()
        .await
        .context("MCP server exited with an error")?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,gemini_mcp_server=info"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .try_init();
}
