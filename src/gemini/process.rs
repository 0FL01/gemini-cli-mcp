use std::{process::Stdio, time::Instant};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader},
    process::Command,
    time,
};

use crate::{
    config::AppConfig,
    error::AppError,
    gemini::events::parse_stream_line,
    types::{
        ErrorInfo, GeminiHealthResponse, GeminiRunRequest, GeminiRunResponse, GeminiStreamEvent,
        HealthStatus, MessageRole, RunStatus,
    },
};

#[derive(Debug, Clone)]
pub struct GeminiRunner {
    config: AppConfig,
}

impl GeminiRunner {
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    pub async fn check_health(&self) -> Result<GeminiHealthResponse, AppError> {
        if self.config.resolve_binary_path().is_none()
            && !self.config.gemini_bin().contains(std::path::MAIN_SEPARATOR)
        {
            return Err(AppError::BinaryNotFound {
                configured: self.config.gemini_bin().to_string(),
            });
        }

        let mut command = Command::new(self.config.gemini_bin());
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = time::timeout(self.config.default_timeout(), command.output())
            .await
            .map_err(|_| {
                AppError::InvalidConfiguration(
                    "timed out while running `gemini --version`".to_string(),
                )
            })?
            .map_err(AppError::SpawnProcess)?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let version_output = if stdout.is_empty() {
            stderr.clone()
        } else {
            stdout
        };

        if version_output.is_empty() {
            return Err(AppError::InvalidConfiguration(
                "`gemini --version` returned no output".to_string(),
            ));
        }

        Ok(GeminiHealthResponse {
            status: HealthStatus::Ok,
            gemini_bin: self.config.gemini_bin().to_string(),
            resolved_bin: self
                .config
                .resolve_binary_path()
                .map(|path| path.display().to_string()),
            version_output,
            stderr: (!stderr.is_empty()).then_some(stderr),
        })
    }

    pub async fn run(&self, request: GeminiRunRequest) -> Result<GeminiRunResponse, AppError> {
        if request.prompt.trim().is_empty() {
            return Err(AppError::InvalidParams(
                "prompt must not be empty".to_string(),
            ));
        }

        let cwd = self.config.resolve_cwd(request.cwd.as_deref())?;
        let timeout = self.config.resolve_timeout(request.timeout_secs)?;
        let model = self.config.resolve_model(request.model.as_deref())?;

        let mut invocation = vec![
            self.config.gemini_bin().to_string(),
            "-p".to_string(),
            "<prompt>".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];
        if let Some(model_name) = &model {
            invocation.push("--model".to_string());
            invocation.push(model_name.clone());
        }

        let mut command = Command::new(self.config.gemini_bin());
        command
            .arg("-p")
            .arg(&request.prompt)
            .arg("--output-format")
            .arg("stream-json")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&cwd);

        if let Some(model_name) = &model {
            command.arg("--model").arg(model_name);
        }

        let start = Instant::now();
        let mut child = command.spawn().map_err(AppError::SpawnProcess)?;
        let stdout = child.stdout.take().ok_or(AppError::MissingStdout)?;
        let stderr = child.stderr.take().ok_or(AppError::MissingStderr)?;

        let stdout_handle = tokio::spawn(read_stdout_events(stdout));
        let stderr_handle = tokio::spawn(read_stderr(stderr));

        let mut timed_out = false;
        let exit_status = match time::timeout(timeout, child.wait()).await {
            Ok(result) => result.map_err(AppError::WaitProcess)?,
            Err(_) => {
                timed_out = true;
                child.start_kill().map_err(AppError::KillProcess)?;
                child.wait().await.map_err(AppError::WaitProcess)?
            }
        };

        let events = stdout_handle.await??;
        let stderr = stderr_handle.await??;

        let mut response = String::new();
        let mut session_id = None;
        let mut model_name = None;
        let mut stats = None;
        let mut error = None;
        let mut status = RunStatus::Success;

        for event in &events {
            match event {
                GeminiStreamEvent::Init {
                    session_id: event_session_id,
                    model,
                    ..
                } => {
                    session_id = Some(event_session_id.clone());
                    model_name = Some(model.clone());
                }
                GeminiStreamEvent::Message { role, content, .. } => {
                    if matches!(role, MessageRole::Assistant) {
                        response.push_str(content);
                    }
                }
                GeminiStreamEvent::Result {
                    status: result_status,
                    stats: result_stats,
                    error: result_error,
                    ..
                } => {
                    status = result_status.clone();
                    stats = result_stats.clone();
                    error = result_error.clone();
                }
                GeminiStreamEvent::Error { message, .. } => {
                    if error.is_none() {
                        error = Some(ErrorInfo {
                            r#type: "stream_error".to_string(),
                            message: message.clone(),
                        });
                    }
                }
                GeminiStreamEvent::ToolUse { .. } | GeminiStreamEvent::ToolResult { .. } => {}
            }
        }

        let exit_code = exit_status.code().unwrap_or(if timed_out { -1 } else { 1 });
        if timed_out {
            status = RunStatus::Error;
            error = Some(ErrorInfo {
                r#type: "timeout".to_string(),
                message: format!("gemini process exceeded timeout of {}s", timeout.as_secs()),
            });
        } else if !exit_status.success() && error.is_none() {
            status = RunStatus::Error;
            error = Some(ErrorInfo {
                r#type: "process_exit".to_string(),
                message: format!("gemini process exited with status code {exit_code}"),
            });
        }

        let include_events = request.include_events.unwrap_or(false);
        let include_stderr = request.include_stderr.unwrap_or(false);
        let duration_ms = saturating_duration_ms(start.elapsed());

        Ok(GeminiRunResponse {
            status,
            response,
            session_id,
            model: model_name,
            stats,
            error,
            exit_code,
            timed_out,
            duration_ms,
            gemini_bin: self.config.gemini_bin().to_string(),
            resolved_bin: self
                .config
                .resolve_binary_path()
                .map(|path| path.display().to_string()),
            cwd: cwd.display().to_string(),
            invocation,
            stderr: if include_stderr || exit_code != 0 {
                (!stderr.trim().is_empty()).then_some(stderr)
            } else {
                None
            },
            events: include_events.then_some(events),
        })
    }
}

async fn read_stdout_events(
    stdout: impl AsyncRead + Unpin,
) -> Result<Vec<GeminiStreamEvent>, AppError> {
    let mut reader = BufReader::new(stdout).lines();
    let mut events = Vec::new();
    while let Some(line) = reader.next_line().await.map_err(AppError::WaitProcess)? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        events.push(parse_stream_line(trimmed)?);
    }
    Ok(events)
}

async fn read_stderr(stderr: impl AsyncRead + Unpin) -> Result<String, AppError> {
    let mut reader = BufReader::new(stderr);
    let mut output = String::new();
    reader
        .read_to_string(&mut output)
        .await
        .map_err(AppError::WaitProcess)?;
    Ok(output.trim().to_string())
}

fn saturating_duration_ms(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::{
        collections::BTreeSet,
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    use tempfile::TempDir;

    use super::GeminiRunner;
    use crate::{
        config::AppConfig,
        types::{GeminiRunRequest, RunStatus},
    };

    fn write_executable(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).expect("script should be written");
        let mut perms = fs::metadata(&path)
            .expect("metadata should exist")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("permissions should be updated");
        path
    }

    fn make_config(gemini_bin: PathBuf, working_dir: PathBuf) -> AppConfig {
        AppConfig::new(
            gemini_bin.display().to_string(),
            5,
            10,
            Some(BTreeSet::from(["gemini-test".to_string()])),
            Some(vec![working_dir.clone()]),
            working_dir,
        )
    }

    #[tokio::test]
    async fn health_reports_version_output() {
        let temp = TempDir::new().expect("temp dir should exist");
        let script = write_executable(
            temp.path(),
            "fake-gemini.sh",
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'gemini 9.9.9\\n'\n  exit 0\nfi\nexit 1\n",
        );
        let runner = GeminiRunner::new(make_config(script, temp.path().to_path_buf()));

        let health = runner.check_health().await.expect("health should succeed");
        assert_eq!(health.version_output, "gemini 9.9.9");
    }

    #[tokio::test]
    async fn run_collects_response_and_events() {
        let temp = TempDir::new().expect("temp dir should exist");
        let args_path = temp.path().join("args.txt");
        let script = write_executable(
            temp.path(),
            "fake-gemini.sh",
            &format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'gemini test\\n'\n  exit 0\nfi\nprintf '%s\\n' \"$@\" > '{}'\nprintf '{{\"type\":\"init\",\"timestamp\":\"2026-03-12T00:00:00Z\",\"session_id\":\"session-1\",\"model\":\"gemini-test\"}}\\n'\nprintf '{{\"type\":\"message\",\"timestamp\":\"2026-03-12T00:00:01Z\",\"role\":\"assistant\",\"content\":\"hello \",\"delta\":true}}\\n'\nprintf '{{\"type\":\"message\",\"timestamp\":\"2026-03-12T00:00:02Z\",\"role\":\"assistant\",\"content\":\"world\",\"delta\":true}}\\n'\nprintf '{{\"type\":\"result\",\"timestamp\":\"2026-03-12T00:00:03Z\",\"status\":\"success\",\"stats\":{{\"total_tokens\":3,\"input_tokens\":1,\"output_tokens\":2,\"cached\":0,\"input\":1,\"duration_ms\":10,\"tool_calls\":0}}}}\\n'\n",
                args_path.display()
            ),
        );
        let runner = GeminiRunner::new(make_config(script, temp.path().to_path_buf()));

        let result = runner
            .run(GeminiRunRequest {
                prompt: "say hello".to_string(),
                cwd: None,
                timeout_secs: Some(2),
                model: Some("gemini-test".to_string()),
                include_events: Some(true),
                include_stderr: Some(false),
            })
            .await
            .expect("run should succeed");

        assert!(matches!(result.status, RunStatus::Success));
        assert_eq!(result.response, "hello world");
        assert_eq!(result.session_id.as_deref(), Some("session-1"));
        assert_eq!(result.model.as_deref(), Some("gemini-test"));
        assert!(result
            .stats
            .as_ref()
            .expect("stats should be present")
            .models
            .is_empty());
        assert_eq!(result.events.as_ref().map(Vec::len), Some(4));

        let args = fs::read_to_string(args_path).expect("args should be captured");
        assert!(args.contains("-p"));
        assert!(args.contains("--output-format"));
        assert!(args.contains("stream-json"));
        assert!(args.contains("--model"));
    }

    #[tokio::test]
    async fn run_returns_error_for_timeout() {
        let temp = TempDir::new().expect("temp dir should exist");
        let script = write_executable(
            temp.path(),
            "fake-gemini.sh",
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'gemini test\\n'\n  exit 0\nfi\nsleep 2\n",
        );
        let runner = GeminiRunner::new(AppConfig::new(
            script.display().to_string(),
            1,
            1,
            None,
            Some(vec![temp.path().to_path_buf()]),
            temp.path().to_path_buf(),
        ));

        let result = runner
            .run(GeminiRunRequest {
                prompt: "slow".to_string(),
                cwd: None,
                timeout_secs: Some(1),
                model: None,
                include_events: Some(false),
                include_stderr: Some(false),
            })
            .await
            .expect("timeout should still return a structured response");

        assert!(matches!(result.status, RunStatus::Error));
        assert!(result.timed_out);
        assert_eq!(result.exit_code, -1);
    }
}
