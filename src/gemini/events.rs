use crate::{error::AppError, types::GeminiStreamEvent};

pub fn parse_stream_line(line: &str) -> Result<GeminiStreamEvent, AppError> {
    serde_json::from_str::<GeminiStreamEvent>(line).map_err(|source| AppError::ParseStreamLine {
        line: line.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_stream_line;
    use crate::types::{GeminiStreamEvent, MessageRole, RunStatus};

    #[test]
    fn parses_message_event() {
        let event = parse_stream_line(
            r#"{"type":"message","timestamp":"2026-03-12T00:00:00Z","role":"assistant","content":"hello","delta":true}"#,
        )
        .expect("message event should parse");

        match event {
            GeminiStreamEvent::Message {
                role,
                content,
                delta,
                ..
            } => {
                assert!(matches!(role, MessageRole::Assistant));
                assert_eq!(content, "hello");
                assert_eq!(delta, Some(true));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parses_result_event_without_models_breakdown() {
        let event = parse_stream_line(
            r#"{"type":"result","timestamp":"2026-03-12T00:00:03Z","status":"success","stats":{"total_tokens":60964,"input_tokens":59566,"output_tokens":551,"cached":36002,"input":23564,"duration_ms":23868,"tool_calls":6}}"#,
        )
        .expect("result event without models should parse");

        match event {
            GeminiStreamEvent::Result { status, stats, .. } => {
                assert!(matches!(status, RunStatus::Success));
                let stats = stats.expect("stats should be present");
                assert_eq!(stats.tool_calls, 6);
                assert!(stats.models.is_empty());
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
