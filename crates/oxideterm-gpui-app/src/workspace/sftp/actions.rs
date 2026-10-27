use super::*;

// Keep action responsibilities isolated while their shared API remains private to SFTP.
mod dialog_lifecycle;
mod external;
mod menus_conflicts;
mod navigation;
mod preview_editor;
mod transfers;

pub(in crate::workspace::sftp) use menus_conflicts::{
    build_sftp_pending_uploads, sftp_extract_archive_kind, sftp_terminal_drop_conflict_resolution,
};
pub(in crate::workspace::sftp) use transfers::SftpTransferLaunch;
