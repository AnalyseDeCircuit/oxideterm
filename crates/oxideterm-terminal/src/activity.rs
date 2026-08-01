use async_channel::{Receiver, Sender, bounded};

/// Coalesces terminal activity so producers never block on UI scheduling.
#[derive(Clone)]
pub(crate) struct TerminalActivitySender {
    sender: Sender<()>,
}

/// Lets a UI owner sleep until terminal state may have changed.
#[derive(Clone)]
pub struct TerminalActivityReceiver {
    receiver: Receiver<()>,
}

pub(crate) fn terminal_activity_channel() -> (TerminalActivitySender, TerminalActivityReceiver) {
    let (sender, receiver) = bounded(1);
    (
        TerminalActivitySender { sender },
        TerminalActivityReceiver { receiver },
    )
}

impl TerminalActivitySender {
    pub(crate) fn notify(&self) {
        // A full channel already represents pending work, so dropping this edge is intentional.
        let _ = self.sender.try_send(());
    }
}

impl TerminalActivityReceiver {
    /// Returns false after the owning terminal session has been dropped.
    pub async fn notified(&self) -> bool {
        self.receiver.recv().await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_notifications_coalesce_without_blocking() {
        let (sender, receiver) = terminal_activity_channel();
        sender.notify();
        sender.notify();

        assert!(receiver.receiver.try_recv().is_ok());
        assert!(receiver.receiver.try_recv().is_err());
    }
}
