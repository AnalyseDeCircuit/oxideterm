use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use serde_json::Value;

pub(crate) async fn read_text_events<B: AsRef<[u8]>, E>(
    bytes: impl Stream<Item = Result<B, E>>,
    anthropic: bool,
    mut text: impl FnMut(String),
) -> Result<(), &'static str> {
    let frames = bytes.eventsource();
    futures::pin_mut!(frames);
    while let Some(frame) = frames.next().await {
        let frame = frame.map_err(|_| "settings_view.ai.stream_interrupted")?;
        if !anthropic && frame.data.trim() == "[DONE]" {
            return Ok(());
        }
        let value: Value =
            serde_json::from_str(&frame.data).map_err(|_| "settings_view.ai.chat_invalid_stream")?;
        if value.get("error").is_some() || frame.event == "error" {
            return Err("settings_view.ai.chat_rejected");
        }
        let delta = if anthropic {
            match value["type"].as_str() {
                Some("message_stop") => return Ok(()),
                Some("content_block_start") => value.pointer("/content_block/text"),
                Some("content_block_delta") => value.pointer("/delta/text"),
                _ => None,
            }
        } else {
            value.pointer("/choices/0/delta/content")
        };
        if let Some(s) = delta.and_then(Value::as_str).filter(|s| !s.is_empty()) {
            text(s.to_owned());
        }
    }
    Err("settings_view.ai.stream_interrupted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{FutureExt, channel::mpsc};
    use std::cell::RefCell;

    #[test]
    fn delivers_text_before_eof_and_distinguishes_completion_from_disconnect() {
        for (anthropic, chunk, end) in [
            (
                false,
                r#"{"choices":[{"delta":{"content":"中文"}}]}"#,
                "[DONE]",
            ),
            (
                true,
                r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"中文"}}"#,
                r#"{"type":"message_stop"}"#,
            ),
        ] {
            let (tx, rx) = mpsc::unbounded::<Result<Vec<u8>, ()>>();
            let seen = RefCell::new(String::new());
            let read = read_text_events(rx, anthropic, |s| seen.borrow_mut().push_str(&s));
            futures::pin_mut!(read);
            let frame = format!("data: {chunk}\r\n\r\n");
            let (a, b) = frame.as_bytes().split_at(frame.find('中').unwrap() + 1);
            tx.unbounded_send(Ok(a.to_vec())).unwrap();
            tx.unbounded_send(Ok(b.to_vec())).unwrap();
            assert!(read.as_mut().now_or_never().is_none());
            assert_eq!(&*seen.borrow(), "中文");
            tx.unbounded_send(Ok(format!("data: {end}\n\n").into_bytes()))
                .unwrap();
            assert_eq!(read.now_or_never(), Some(Ok(())));

            let partial = futures::stream::iter([Ok::<_, ()>(frame.as_bytes())]);
            assert_eq!(
                read_text_events(partial, anthropic, |_| {}).now_or_never(),
                Some(Err("settings_view.ai.stream_interrupted"))
            );
        }
    }
}
