#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GatewaySessionState {
    Opening,
    Running,
    Rejected,
}

impl GatewaySessionState {
    pub fn writable(self, gateway_ready: bool) -> bool {
        // A quiet shell can require input before it produces its first output.
        gateway_ready && self != Self::Rejected
    }

    pub fn running(self, gateway_ready: bool) -> bool {
        gateway_ready && self == Self::Running
    }

    /// The first accepted output confirms this session, rather than just the socket.
    pub fn receive_output(&mut self, gateway_ready: bool) -> Option<bool> {
        if !self.writable(gateway_ready) {
            return None;
        }
        let first = *self == Self::Opening;
        *self = Self::Running;
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::GatewaySessionState as Session;

    #[test]
    fn socket_readiness_does_not_confirm_or_revive_an_open_request() {
        let mut session = Session::Opening;
        assert!(session.writable(true));
        assert!(!session.running(true));
        assert_eq!(session.receive_output(false), None);
        assert_eq!(session.receive_output(true), Some(true));
        assert!(session.running(true));
        assert_eq!(session.receive_output(true), Some(false));
        assert!(!session.running(false));

        let mut rejected = Session::Rejected;
        assert_eq!(rejected.receive_output(true), None);
        assert!(!rejected.writable(true));
    }
}
