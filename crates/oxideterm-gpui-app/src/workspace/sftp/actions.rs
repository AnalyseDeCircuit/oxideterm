use super::*;

// Keep action responsibilities isolated while their shared API remains private to SFTP.
mod dialog_lifecycle;
mod external;
mod menus_conflicts;
mod navigation;
mod preview_editor;
mod transfers;

pub(in crate::workspace::sftp) use menus_conflicts::sftp_extract_archive_kind;
pub(in crate::workspace::sftp) use transfers::SftpTransferLaunch;
