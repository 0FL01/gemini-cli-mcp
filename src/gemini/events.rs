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
    use crate::types::{GeminiStreamEvent, MessageRole};

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
}
