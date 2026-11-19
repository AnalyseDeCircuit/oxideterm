use super::*;
pub(in crate::workspace) use oxideterm_sftp::FtpRuntime;
pub(super) use oxideterm_sftp::MutationSession;

impl WorkspaceApp {
    pub(super) fn transfer_protocol_for_remote(
        &self,
        remote_id: &SftpRemoteId,
    ) -> RemoteTransferProtocol {
        if let SftpRemoteId::Ftp(id) = remote_id {
            return self
                .ftp_sessions
                .get(id)
                .map_or(RemoteTransferProtocol::Ftp, |runtime| {
                    runtime.transfer_protocol()
                });
        }
        configured_transfer_protocol(self.settings_store.settings().sftp.transfer_protocol)
    }
}

impl SftpRemoteBackend {
    pub(super) async fn mutation_session(&self) -> Result<MutationSession, String> {
        match self {
            Self::Ftp { runtime } => Ok(MutationSession::Ftp {
                session: tokio::sync::Mutex::new(runtime.transfer().await?),
                cancel: runtime.cancel.clone(),
            }),
            _ => self
                .acquire_transfer_sftp()
                .await
                .map(MutationSession::Sftp),
        }
    }
}
