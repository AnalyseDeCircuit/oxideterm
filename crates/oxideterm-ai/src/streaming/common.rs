use anyhow::{Context, Result};
use futures_util::StreamExt;

use crate::AiStreamEvent;

pub(crate) enum StreamParseResult {
    Done,
    SawEvent,
    Empty { raw: String },
}

pub(crate) async fn stream_sse_response(
    response: reqwest::Response,
    events: &tokio::sync::mpsc::UnboundedSender<AiStreamEvent>,
    parse_line: fn(&str) -> ParsedStreamLine,
) -> Result<StreamParseResult> {
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut raw_body = String::new();
    let mut saw_event = false;
    let mut saw_sse_frame = false;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("AI provider stream failed")?;
        let text = String::from_utf8_lossy(&chunk);
        if raw_body.len() < 65_536 {
            raw_body.push_str(&text);
        }
        buffer.push_str(&text);
        while let Some(newline) = buffer.find('\n') {
            let line = buffer[..newline].trim_end_matches('\r').to_string();
            buffer.drain(..=newline);
            let parsed = parse_line(&line);
            saw_sse_frame |= parsed.saw_frame;
            for event in parsed.events {
                if event == AiStreamEvent::Done {
                    return Ok(StreamParseResult::Done);
                }
                saw_event = true;
                let _ = events.send(event);
            }
        }
    }

    if !buffer.trim().is_empty() {
        let parsed = parse_line(buffer.trim());
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
        Ok(StreamParseResult::Empty { raw: raw_body })
    }
}

pub(crate) struct ParsedStreamLine {
    pub(crate) events: Vec<AiStreamEvent>,
    pub(crate) saw_frame: bool,
}
