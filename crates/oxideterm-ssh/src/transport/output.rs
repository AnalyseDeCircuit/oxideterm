const SSH_BOOTSTRAP_OUTPUT_GATE_TIMEOUT: Duration = Duration::from_secs(2);
const SSH_BOOTSTRAP_OUTPUT_GATE_MAX_BYTES: usize = 256 * 1024;
const OXIDETERM_REMOTE_METADATA_OSC_PREFIX: &[u8] = b"\x1b]7719;";

struct SshBootstrapOutputGate {
    pending: Vec<u8>,
    deadline: Instant,
}

impl SshBootstrapOutputGate {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            deadline: Instant::now() + SSH_BOOTSTRAP_OUTPUT_GATE_TIMEOUT,
        }
    }

    fn deadline(&self) -> Instant {
        self.deadline
    }

    fn push(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        self.pending.extend_from_slice(bytes);
        if bootstrap_metadata_osc_is_complete(&self.pending)
            || self.pending.len() >= SSH_BOOTSTRAP_OUTPUT_GATE_MAX_BYTES
        {
            return self.take();
        }
        None
    }

    fn take(&mut self) -> Option<Vec<u8>> {
        (!self.pending.is_empty()).then(|| std::mem::take(&mut self.pending))
    }
}

fn bootstrap_metadata_osc_is_complete(bytes: &[u8]) -> bool {
    let Some(start) = bytes
        .windows(OXIDETERM_REMOTE_METADATA_OSC_PREFIX.len())
        .position(|window| window == OXIDETERM_REMOTE_METADATA_OSC_PREFIX)
    else {
        return false;
    };
    let payload = &bytes[start + OXIDETERM_REMOTE_METADATA_OSC_PREFIX.len()..];
    payload.contains(&0x07) || payload.windows(2).any(|window| window == b"\x1b\\")
}

fn gate_bootstrap_output(
    gate: &mut Option<SshBootstrapOutputGate>,
    bytes: &[u8],
) -> Option<Vec<u8>> {
    let Some(active_gate) = gate.as_mut() else {
        return Some(bytes.to_vec());
    };
    let released = active_gate.push(bytes);
    if released.is_some() {
        *gate = None;
    }
    released
}

fn push_shell_output(
    batcher: &mut SshOutputBatcher,
    gate: &mut Option<SshBootstrapOutputGate>,
    bytes: &[u8],
) -> bool {
    gate_bootstrap_output(gate, bytes)
        .is_some_and(|released| batcher.push(&released))
}

fn release_bootstrap_output(
    batcher: &mut SshOutputBatcher,
    gate: &mut Option<SshBootstrapOutputGate>,
) -> bool {
    let Some(mut active_gate) = gate.take() else {
        return false;
    };
    active_gate
        .take()
        .is_some_and(|released| batcher.push(&released))
}

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
            self.pending.extend_from_slice(&guarded);
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
        self.flush_deadline = Some(now + Duration::from_millis(delay));
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
    fn push(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        if bytes.is_empty() && self.residual.is_empty() {
            return None;
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

        (!combined.is_empty()).then_some(combined)
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
        assert_eq!(guard.push(&[0xa0]), Some("你".as_bytes().to_vec()));
    }

    #[test]
    fn raw_utf8_guard_flushes_invalid_bytes_unchanged() {
        let mut guard = RawUtf8ResidualGuard::default();

        assert_eq!(guard.push(&[0xff, b'a']), Some(vec![0xff, b'a']));
    }

    #[test]
    fn output_batcher_holds_utf8_tail_until_final_flush() {
        let mut batcher = SshOutputBatcher::new();

        assert!(!batcher.push(&[0xe4, 0xbd]));
        assert_eq!(batcher.take_flush(), None);
        assert_eq!(batcher.take_final_flush(), Some(vec![0xe4, 0xbd]));
    }

    #[test]
    fn output_batcher_flushes_complete_text() {
        let mut batcher = SshOutputBatcher::new();

        assert!(!batcher.push(b"abc"));
        assert_eq!(batcher.take_flush(), Some(b"abc".to_vec()));
    }

    #[test]
    fn bootstrap_output_gate_releases_motd_and_launcher_only_after_metadata() {
        let mut gate = Some(SshBootstrapOutputGate::new());

        assert_eq!(
            gate_bootstrap_output(&mut gate, b"Welcome to Ubuntu\r\nlauncher\r\n"),
            None
        );
        assert_eq!(
            gate_bootstrap_output(&mut gate, b"\x1b]7719;v=1;cwd=/home"),
            None
        );
        let released = gate_bootstrap_output(&mut gate, b"\x07prompt").unwrap();

        assert!(gate.is_none());
        assert!(released.starts_with(b"Welcome to Ubuntu"));
        assert!(released.ends_with(b"\x07prompt"));
    }

    #[test]
    fn bootstrap_output_gate_accepts_split_string_terminator() {
        let mut gate = Some(SshBootstrapOutputGate::new());

        assert_eq!(
            gate_bootstrap_output(&mut gate, b"\x1b]7719;v=1\x1b"),
            None
        );
        assert!(gate_bootstrap_output(&mut gate, b"\\").is_some());
        assert!(gate.is_none());
    }

    #[test]
    fn ssh_client_config_matches_tauri_transport_defaults() {
        let config = ssh_client_config(false);

        assert_eq!(config.inactivity_timeout, None);
        assert_eq!(config.keepalive_interval, Some(Duration::from_secs(30)));
        assert_eq!(config.keepalive_max, 3);
        assert_eq!(config.window_size, 32 * 1024 * 1024);
        assert_eq!(config.maximum_packet_size, 256 * 1024);
    }

    #[test]
    fn ssh_client_config_enables_legacy_algorithms_only_when_requested() {
        let modern = ssh_client_config(false);
        let legacy = ssh_client_config(true);

        assert!(!modern.preferred.kex.contains(&russh::kex::DH_G14_SHA1));
        assert!(legacy.preferred.kex.contains(&russh::kex::DH_G14_SHA1));
    }

    #[test]
    fn validates_proxy_chain_depth_like_tauri() {
        let chain = (0..=MAX_PROXY_CHAIN_DEPTH)
            .map(|index| ProxyHopConfig {
                host: format!("jump-{index}.example.com"),
                port: 22,
                username: "root".to_string(),
                auth: AuthMethod::Agent,
                agent_forwarding: false,
                legacy_ssh_compatibility: false,
                strict_host_key_checking: true,
                trust_host_key: None,
                expected_host_key_fingerprint: None,
            })
            .collect::<Vec<_>>();
        let error = validate_proxy_chain_depth(&chain).unwrap_err();

        assert_eq!(
            error.to_string(),
            format!(
                "SSH connection failed: proxy chain too long: {} hops (max {})",
                MAX_PROXY_CHAIN_DEPTH + 1,
                MAX_PROXY_CHAIN_DEPTH
            )
        );
    }
}
