use chrono::{DateTime, Utc};
use std::time::{Duration, UNIX_EPOCH};

use super::FileAttributes;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub filename: String,
    pub longname: String,
    pub attrs: FileAttributes,
}

impl File {
    /// Omits `longname` and set dummy `attributes`. This is mainly used for [`crate::server::Handler::realpath`] as per the standard
    pub fn dummy<S: Into<String>>(filename: S) -> Self {
        Self {
            filename: filename.into(),
            longname: "".to_string(),
            attrs: FileAttributes::default(),
        }
    }

    /// Implies the use of longname
    pub fn new<S: Into<String>>(filename: S, attrs: FileAttributes) -> Self {
        let mut file = Self {
            filename: filename.into(),
            longname: "".to_string(),
            attrs,
        };
        file.longname = file.longname();
        file
    }

    /// Get formed longname
    pub fn longname(&self) -> String {
        let directory = if self.attrs.is_dir() { "d" } else { "-" };
        let permissions = self.attrs.permissions().to_string();

        let size = self.attrs.size.unwrap_or(0);
        let mtime = self.attrs.mtime.unwrap_or(0);

        let datetime = DateTime::<Utc>::from(UNIX_EPOCH + Duration::from_secs(mtime as u64));
        let delayed = datetime.format("%b %d %Y %H:%M");

        format!(
            "{directory}{permissions} 0 {} {} {size} {delayed} {}",
            if let Some(user) = &self.attrs.user {
                user.to_string()
            } else {
                self.attrs.uid.unwrap_or(0).to_string()
            },
            if let Some(group) = &self.attrs.group {
                group.to_string()
            } else {
                self.attrs.gid.unwrap_or(0).to_string()
            },
            self.filename
        )
    }
}
