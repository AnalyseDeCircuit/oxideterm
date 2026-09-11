struct SshOutputBatcher {
    pending: Vec<u8>,
    flush_deadline: Option<Instant>,
    interactive_until: Option<Instant>,
}

impl SshOutputBatcher {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
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
        // Transport bytes can belong to a binary protocol. Text decoders own
        // incomplete characters only after protocol consumers release display data.
        if self.pending.is_empty() {
            self.pending = bytes.to_vec();
        } else {
            self.pending.extend_from_slice(bytes);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_batcher_flushes_binary_tail_without_another_packet() {
        let mut batcher = SshOutputBatcher::new();
        batcher.push(&[0x02, 0xe4, 0xbd]);
        assert_eq!(
            batcher.take_flush().as_deref(),
            Some([0x02, 0xe4, 0xbd].as_slice())
        );
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
                    if let Some(bytes) = batcher.take_flush() {
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

    #[tokio::test]
    async fn output_cancellation_wakes_both_byte_and_message_capacity_waiters() {
        for chunk_size in [1, SSH_OUTPUT_BATCH_MAX_BYTES] {
            let (sender, receiver) = ssh_output_channel();
            let count = SSH_OUTPUT_CHANNEL_CAPACITY.min(SSH_OUTPUT_BACKLOG_BYTES / chunk_size);
            for _ in 0..count {
                sender.send(vec![b'x'; chunk_size]).await.unwrap();
            }
            let pending_sender = sender.clone();
            let mut pending =
                tokio::spawn(async move { pending_sender.send(b"blocked".to_vec()).await });
            assert!(
                tokio::time::timeout(Duration::from_millis(10), &mut pending)
                    .await
                    .is_err()
            );
            receiver.cancellation_handle().cancel();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(1), pending)
                    .await
                    .unwrap()
                    .unwrap(),
                Err(b"blocked".to_vec())
            );
            let (other, mut other_rx) = ssh_output_channel();
            other.send(b"independent".to_vec()).await.unwrap();
            assert_eq!(&*other_rx.try_recv().unwrap(), b"independent");
        }
    }

    #[tokio::test]
    async fn output_receiver_drop_wakes_sender_while_a_chunk_is_retained() {
        let (sender, mut receiver) = ssh_output_channel();
        sender
            .send(vec![b'x'; SSH_OUTPUT_BACKLOG_BYTES])
            .await
            .unwrap();
        let retained = receiver.try_recv().unwrap();
        let mut blocked = tokio::spawn(async move { sender.send(b"next".to_vec()).await });
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut blocked)
                .await
                .is_err()
        );
        drop(receiver);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), blocked)
                .await
                .unwrap()
                .unwrap(),
            Err(b"next".to_vec())
        );
        assert_eq!(&retained[..4], b"xxxx");
    }

    #[tokio::test]
    async fn output_boundary_excludes_later_packets_and_keeps_received_capacity() {
        let (sender, mut receiver) = ssh_output_channel();
        sender.send(b"first".to_vec()).await.unwrap();
        let boundary = receiver.published_sequence();
        sender.send(b"later".to_vec()).await.unwrap();
        let first = receiver.try_recv().unwrap();
        let later = receiver.try_recv().unwrap();
        assert_eq!((first.sequence(), &*first), (boundary, b"first".as_slice()));
        assert_eq!(
            (later.sequence(), &*later),
            (boundary + 1, b"later".as_slice())
        );
        assert_eq!(
            sender.byte_permits.available_permits(),
            SSH_OUTPUT_BACKLOG_BYTES - 10
        );
        drop(first);
        assert_eq!(
            sender.byte_permits.available_permits(),
            SSH_OUTPUT_BACKLOG_BYTES - 5
        );
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
