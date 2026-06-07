use std::fs;

use super::{impl_packet_for, impl_request_id, FileAttributes, Packet, RequestId};

/// Opening flags according to the specification
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct OpenFlags(u32);

bitflags! {
    impl OpenFlags: u32 {
        const READ = 0x00000001;
        const WRITE = 0x00000002;
        const APPEND = 0x00000004;
        const CREATE = 0x00000008;
        const TRUNCATE = 0x00000010;
        const EXCLUDE = 0x00000020;
    }
}

impl From<OpenFlags> for fs::OpenOptions {
    fn from(value: OpenFlags) -> Self {
        let mut open_options = fs::OpenOptions::new();
        if value.contains(OpenFlags::READ) {
            open_options.read(true);
        }
        if value.contains(OpenFlags::WRITE) {
            open_options.write(true);
        }
        if value.contains(OpenFlags::APPEND) {
            open_options.append(true);
        }
        if value.contains(OpenFlags::CREATE) {
            // SFTPv3 spec requires the `CREATE` flag to be set if the `EXCLUDE` flag
            // is set. Rusts `OpenOptions` has different semantics: it ignores
            // whether `create` or `truncate` was set.
            // SFTPv3 spec does not say anything about read/write flags, but
            // they will be required to do anything else with the file.
            // https://datatracker.ietf.org/doc/html/draft-ietf-secsh-filexfer-02#section-6.3
            if value.contains(OpenFlags::EXCLUDE) {
                open_options.create_new(true);
            } else {
                open_options.create(true);
            }
        }
        if value.contains(OpenFlags::TRUNCATE) {
            open_options.truncate(true);
        }

        open_options
    }
}

/// Implementation for `SSH_FXP_OPEN`
#[derive(Debug, Serialize, Deserialize)]
pub struct Open {
    pub id: u32,
    pub filename: String,
    pub pflags: OpenFlags,
    pub attrs: FileAttributes,
}

impl_request_id!(Open);
impl_packet_for!(Open);
