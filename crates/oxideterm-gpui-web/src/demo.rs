mod shell;
mod storage;
use crate::transport::*;
use shell::Shell;
use std::collections::HashMap;
pub use storage::*;
pub use storage::transfer::TransferDestination;
use zeroize::Zeroizing;

pub struct DemoTransport {
    drives: DemoDrives,
    terminals: HashMap<u64, Shell>,
    events: Events,
}
impl DemoTransport {
    pub fn new(drives: DemoDrives, events: Events) -> Self {
        Self {
            drives,
            events,
            terminals: HashMap::new(),
        }
    }
}
impl WebTransport for DemoTransport {
    fn directory(&self, session: u64) -> Option<&str> {
        self.terminals.get(&session).map(|s| s.cwd.as_str())
    }

    fn change_directory(&mut self, session: u64, path: &str) -> Result<(), &'static str> {
        let shell = self
            .terminals
            .get_mut(&session)
            .ok_or("sftp.toolbar.terminal_directory_unavailable")?;
        let bytes = shell.change_directory(path)?;
        let _ = self
            .events
            .unbounded_send(GatewayEvent::DemoDirectoryChanged { session });
        self.events
            .unbounded_send(GatewayEvent::Output { session, bytes })
            .map_err(|_| "sftp.toast.sync_terminal_failed")
    }
    fn send(&mut self, request: GatewayRequest) -> Result<(), ()> {
        let event = match request {
            GatewayRequest::Open { session, node, .. } => {
                let shell = Shell::new(
                    self.drives.files(Drive::Node(node.clone())),
                    if node == "local" { LOCAL } else { REMOTE },
                );
                let bytes = Zeroizing::new(shell.prompt().into_bytes());
                self.terminals.insert(session, shell);
                GatewayEvent::Output { session, bytes }
            }
            GatewayRequest::Input { session, bytes } => {
                let shell = self.terminals.get_mut(&session).ok_or(())?;
                let before = bytes
                    .iter()
                    .any(|b| matches!(b, b'\r' | b'\n'))
                    .then(|| shell.cwd.clone());
                let bytes = shell.input(&bytes);
                if before.is_some_and(|cwd| cwd != shell.cwd) {
                    let _ = self
                        .events
                        .unbounded_send(GatewayEvent::DemoDirectoryChanged { session });
                }
                GatewayEvent::Output { session, bytes }
            }
            GatewayRequest::Unsubscribe { session } => {
                self.terminals.remove(&session);
                return Ok(());
            }
            GatewayRequest::Copy {
                from,
                to,
                transfer,
                node,
            } => {
                let fs = self.drives.files(Drive::Node(node));
                if copy_file(&fs, &from, &fs, &to).is_ok() {
                    GatewayEvent::Copied { transfer }
                } else {
                    GatewayEvent::Rejected { request: transfer }
                }
            }
            GatewayRequest::Resize { .. } | GatewayRequest::Cancel { .. } => return Ok(()),
        };
        self.events.unbounded_send(event).map_err(|_| ())
    }
}
