struct SshOutputBatcher {
    pending: Vec<u8>,
    utf8_guard: RawUtf8ResidualGuard,
    flush_deadline: Option<Instant>,
    interactive_until: Option<Instant>,
}

impl SshOutputBatcher {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            utf8_guard: RawUtf8ResidualGuard::default(),
            flush_deadline: None,
            interactive_until: None,
        }
    }

    fn note_interaction(&mut self) {
        self.interactive_until =
            Some(Instant::now() + Duration::from_millis(SSH_OUTPUT_INTERACTIVE_WINDOW_MS));
        self.refresh_deadline();
    }

    fn push(&mut self, bytes: &[u8]) -> bool {
        if let Some(guarded) = self.utf8_guard.push(bytes) {
            if self.pending.is_empty() {
                self.pending = guarded.into_owned();
            } else {
                self.pending.extend_from_slice(&guarded);
            }
        }
        self.refresh_deadline();
        self.pending.len() >= SSH_OUTPUT_BATCH_MAX_BYTES
    }

    fn flush_due(&self) -> Option<Instant> {
        (!self.pending.is_empty())
            .then_some(self.flush_deadline?)
            .or(None)
    }

    fn take_flush(&mut self) -> Option<Vec<u8>> {
        if self.pending.is_empty() {
            self.flush_deadline = None;
            return None;
        }
        self.flush_deadline = None;
        Some(std::mem::take(&mut self.pending))
    }

    fn take_final_flush(&mut self) -> Option<Vec<u8>> {
        if let Some(residual) = self.utf8_guard.flush() {
            self.pending.extend_from_slice(&residual);
        }
        self.take_flush()
    }

    fn refresh_deadline(&mut self) {
        if self.pending.is_empty() {
            self.flush_deadline = None;
            return;
        }

        let now = Instant::now();
        let interactive = self
            .interactive_until
            .is_some_and(|deadline| deadline > now);
        let delay = if interactive {
            SSH_OUTPUT_INTERACTIVE_FLUSH_MS
        } else {
            SSH_OUTPUT_FLUSH_MS
        };
        // More output must not postpone bytes already waiting for the consumer.
        let deadline = now + Duration::from_millis(delay);
        self.flush_deadline = Some(
            self.flush_deadline
                .map_or(deadline, |current| current.min(deadline)),
        );
    }
}

impl SftpChannelOpener for SshConnectionHandle {
    fn open_sftp_channel(
        &self,
    ) -> impl Future<Output = Result<russh::Channel<client::Msg>, SftpError>> + Send {
        async {
            self.open_session_channel()
                .await
                .map_err(|error| SftpError::ChannelError(error.to_string()))
        }
    }
}

impl SftpExecChannelOpener for SshConnectionHandle {
    fn open_exec_channel(
        &self,
    ) -> impl Future<Output = Result<russh::Channel<client::Msg>, SftpError>> + Send {
        async {
            self.open_session_channel()
                .await
                .map_err(|error| SftpError::ChannelError(error.to_string()))
        }
    }
}

#[derive(Default)]
struct RawUtf8ResidualGuard {
    residual: Vec<u8>,
}

impl RawUtf8ResidualGuard {
    fn push<'a>(&mut self, bytes: &'a [u8]) -> Option<std::borrow::Cow<'a, [u8]>> {
        if bytes.is_empty() && self.residual.is_empty() {
            return None;
        }

        if self.residual.is_empty() {
            // Complete transport data can go straight into the batch; only an
            // incomplete scalar needs storage beyond the borrowed packet.
            let split = split_before_incomplete_utf8_tail(bytes);
            self.residual.extend_from_slice(&bytes[split..]);
            return (split > 0).then_some(std::borrow::Cow::Borrowed(&bytes[..split]));
        }

        let mut combined = Vec::with_capacity(self.residual.len() + bytes.len());
        combined.extend_from_slice(&self.residual);
        combined.extend_from_slice(bytes);
        self.residual.clear();

        let split = split_before_incomplete_utf8_tail(&combined);
        if split < combined.len() {
            self.residual.extend_from_slice(&combined[split..]);
            combined.truncate(split);
        }

        if self.residual.len() >= UTF8_RESIDUAL_MAX_BYTES {
            combined.extend_from_slice(&self.residual);
            self.residual.clear();
        }

        (!combined.is_empty()).then_some(std::borrow::Cow::Owned(combined))
    }

    fn flush(&mut self) -> Option<Vec<u8>> {
        (!self.residual.is_empty()).then(|| std::mem::take(&mut self.residual))
    }
}

fn split_before_incomplete_utf8_tail(bytes: &[u8]) -> usize {
    let len = bytes.len();
    let max_tail = len.min(UTF8_RESIDUAL_MAX_BYTES - 1);

    for tail_len in 1..=max_tail {
        let start = len - tail_len;
        let first = bytes[start];
        let width = utf8_char_width(first);
        if width == 0 {
            continue;
        }

        if width > tail_len
            && bytes[start + 1..]
                .iter()
                .all(|byte| is_utf8_continuation(*byte))
        {
            return start;
        }

        break;
    }

    len
}

fn utf8_char_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

fn is_utf8_continuation(byte: u8) -> bool {
    (0x80..=0xbf).contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_utf8_guard_keeps_incomplete_scalar_tail() {
        let mut guard = RawUtf8ResidualGuard::default();

        assert_eq!(guard.push(&[0xe4, 0xbd]), None);
        assert_eq!(guard.push(&[0xa0]).as_deref(), Some("你".as_bytes()));
    }

    #[test]
    fn raw_utf8_guard_flushes_invalid_bytes_unchanged() {
        let mut guard = RawUtf8ResidualGuard::default();

        assert_eq!(
            guard.push(&[0xff, b'a']).as_deref(),
            Some([0xff, b'a'].as_slice())
        );
    }

    #[test]
    fn output_batcher_holds_utf8_tail_until_final_flush() {
        let mut batcher = SshOutputBatcher::new();

        assert!(!batcher.push(&[0xe4, 0xbd]));
        assert_eq!(batcher.take_flush(), None);
        assert_eq!(batcher.take_final_flush(), Some(vec![0xe4, 0xbd]));
    }

    #[test]
    fn output_batcher_preserves_text_and_binary_protocol_bytes_across_chunks() {
        let fixtures: &[&[u8]] = &[
            "ASCII 中文 e\u{301} 🦀\r\n".as_bytes(),
            b"\x1b[31mred\x1b[0m\r\n::TRZSZ:TRANSFER:S:1.1.0:12345678\r\n",
            b"**\x18B00000000000000\r\n\x11\x00\xff\x80\xe4\xbd",
        ];
        for fixture in fixtures {
            for chunk_size in 1..=fixture.len() {
                for flush_each_chunk in [false, true] {
                    let mut batcher = SshOutputBatcher::new();
                    let mut received = Vec::new();
                    for chunk in fixture.chunks(chunk_size) {
                        let full = batcher.push(chunk);
                        if (full || flush_each_chunk)
                            && let Some(bytes) = batcher.take_flush()
                        {
                            received.extend(bytes);
                        }
                    }
                    if let Some(bytes) = batcher.take_final_flush() {
                        received.extend(bytes);
                    }
                    assert_eq!(
                        received, *fixture,
                        "chunk size {chunk_size}, flush each chunk {flush_each_chunk}"
                    );
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn output_batcher_flush_deadline_is_not_postponed_by_more_output() {
        let mut batcher = SshOutputBatcher::new();
        batcher.push(b"first");
        let deadline = batcher.flush_due().unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
        batcher.push(b" second");
        tokio::time::sleep_until(deadline).await;
        assert!(batcher.flush_due().unwrap() <= Instant::now());
        assert_eq!(
            batcher.take_flush().as_deref(),
            Some(b"first second".as_slice())
        );

        batcher.push(b"third");
        let normal_deadline = batcher.flush_due().unwrap();
        batcher.note_interaction();
        let interactive_deadline = batcher.flush_due().unwrap();
        assert!(interactive_deadline < normal_deadline);
        tokio::time::sleep(Duration::from_micros(500)).await;
        batcher.note_interaction();
        batcher.push(b" fourth");
        tokio::time::sleep_until(interactive_deadline).await;
        assert!(batcher.flush_due().unwrap() <= Instant::now());
        assert_eq!(
            batcher.take_flush().as_deref(),
            Some(b"third fourth".as_slice())
        );
    }

    #[tokio::test]
    async fn ssh_output_channel_releases_byte_capacity_after_consumption() {
        let (sender, mut receiver) = ssh_output_channel();
        let chunk = vec![b'x'; SSH_OUTPUT_BATCH_MAX_BYTES];
        for _ in 0..(SSH_OUTPUT_BACKLOG_BYTES / SSH_OUTPUT_BATCH_MAX_BYTES) {
            sender.send(chunk.clone()).await.unwrap();
        }

        let blocked_sender = sender.clone();
        let blocked_chunk = chunk.clone();
        let mut blocked = tokio::spawn(async move { blocked_sender.send(blocked_chunk).await });
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut blocked)
                .await
                .is_err()
        );

        drop(receiver.try_recv().unwrap());
        tokio::time::timeout(Duration::from_secs(1), blocked)
            .await
            .expect("released bytes should wake the producer")
            .unwrap()
            .unwrap();
    }

    #[test]
    fn ssh_client_config_enables_legacy_algorithms_only_when_requested() {
        let preferences = oxideterm_connections::SshAlgorithmPreferences::default();
        let modern = ssh_client_config(false, &preferences).unwrap();
        let legacy = ssh_client_config(true, &preferences).unwrap();

        assert!(!modern.preferred.kex.contains(&russh::kex::DH_G14_SHA1));
        assert!(legacy.preferred.kex.contains(&russh::kex::DH_G14_SHA1));
    }
}
