use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use eventsource_stream::{EventStreamError, Eventsource};
use futures_util::StreamExt;

use crate::AiStreamEvent;

const RAW_BODY_PREVIEW_LIMIT: usize = 65_536;

pub(crate) enum StreamParseResult {
    Done,
    SawEvent,
    Empty { raw: String },
}

pub(crate) async fn stream_sse_response(
    response: reqwest::Response,
    events: &tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
    mut parse_line: impl FnMut(&str) -> ParsedStreamLine,
) -> Result<StreamParseResult> {
    stream_sse_event_source(response.bytes_stream(), events, move |line| {
        parse_line(line)
    })
    .await
}

async fn stream_sse_event_source<S, B, E>(
    source: S,
    events: &tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
    mut parse_line: impl FnMut(&str) -> ParsedStreamLine,
) -> Result<StreamParseResult>
where
    S: futures_util::Stream<Item = std::result::Result<B, E>> + Send,
    B: AsRef<[u8]> + Send,
    E: std::error::Error + Send + Sync + 'static,
{
    let raw_body = Arc::new(Mutex::new(String::new()));
    let raw_body_for_stream = Arc::clone(&raw_body);
    let stream = source.map(move |chunk| {
        if let Ok(bytes) = &chunk {
            append_raw_body_preview(&raw_body_for_stream, bytes.as_ref());
        }
        chunk
    });
    let stream = stream.eventsource();
    futures_util::pin_mut!(stream);

    let mut saw_event = false;
    let mut saw_sse_frame = false;

    while let Some(event) = stream.next().await {
        let event = event.map_err(map_sse_stream_error)?;
        let synthetic_data_line = format!("data: {}", event.data);
        let parsed = parse_line(&synthetic_data_line);
        saw_sse_frame |= parsed.saw_frame;
        for event in parsed.events {
            if event == AiStreamEvent::Done {
                return Ok(StreamParseResult::Done);
            }
            saw_event = true;
            let _ = events.send(event);
        }
    }

    if saw_event || saw_sse_frame {
        Ok(StreamParseResult::SawEvent)
    } else {
        let raw_body = raw_body.lock().map(|raw| raw.clone()).unwrap_or_default();
        Ok(StreamParseResult::Empty { raw: raw_body })
    }
}

fn append_raw_body_preview(raw_body: &Arc<Mutex<String>>, bytes: &[u8]) {
    let Ok(mut raw_body) = raw_body.lock() else {
        return;
    };
    if raw_body.len() >= RAW_BODY_PREVIEW_LIMIT {
        return;
    }
    let text = String::from_utf8_lossy(bytes);
    let remaining = RAW_BODY_PREVIEW_LIMIT - raw_body.len();
    if text.len() <= remaining {
        raw_body.push_str(&text);
    } else {
        let boundary = text
            .char_indices()
            .take_while(|(index, _)| *index <= remaining)
            .map(|(index, _)| index)
            .last()
            .unwrap_or(0);
        raw_body.push_str(&text[..boundary]);
    }
}

fn map_sse_stream_error<E>(error: EventStreamError<E>) -> anyhow::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    match error {
        EventStreamError::Transport(error) => anyhow!(error).context("AI provider stream failed"),
        EventStreamError::Utf8(error) => {
            anyhow!(error).context("AI provider stream returned invalid UTF-8")
        }
        EventStreamError::Parser(error) => {
            anyhow!("AI provider stream returned invalid SSE: {error}")
        }
    }
}

pub(crate) struct ParsedStreamLine {
    pub(crate) events: Vec<AiStreamEvent>,
    pub(crate) saw_frame: bool,
}

#[cfg(test)]
mod tests {
    use futures_util::stream;

    use super::{ParsedStreamLine, StreamParseResult, stream_sse_event_source};
    use crate::AiStreamEvent;

    #[tokio::test]
    async fn eventsource_parser_handles_split_and_multiline_events() {
        let chunks = stream::iter([
            Ok::<_, std::io::Error>("data: hel"),
            Ok("lo\n"),
            Ok("data: world\n\n"),
            Ok("data: [DONE]\n\n"),
        ]);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let result = stream_sse_event_source(chunks, &tx, parse_test_data_line)
            .await
            .unwrap();

        assert!(matches!(result, StreamParseResult::Done));
        assert_eq!(
            rx.try_recv().unwrap(),
            AiStreamEvent::Content("hello\nworld".to_string())
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn eventsource_parser_keeps_raw_body_when_no_sse_event_arrives() {
        let chunks = stream::iter([Ok::<_, std::io::Error>("{\"ok\":true}")]);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        let result = stream_sse_event_source(chunks, &tx, parse_test_data_line)
            .await
            .unwrap();

        let StreamParseResult::Empty { raw } = result else {
            panic!("expected empty stream fallback");
        };
        assert_eq!(raw, "{\"ok\":true}");
    }

    fn parse_test_data_line(line: &str) -> ParsedStreamLine {
        let Some(data) = line.strip_prefix("data: ") else {
            return ParsedStreamLine {
                events: Vec::new(),
                saw_frame: false,
            };
        };
        let event = if data == "[DONE]" {
            AiStreamEvent::Done
        } else {
            AiStreamEvent::Content(data.to_string())
        };
        ParsedStreamLine {
            events: vec![event],
            saw_frame: true,
        }
    }
}
