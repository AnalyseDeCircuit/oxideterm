use crate::protocol::StatusCode;

/// Response sent by [`Handler`](super::Handler) for any request that completes
/// via the `Err` arm. Mapped to an `SSH_FXP_STATUS` packet.
///
/// `error_message` and `language_tag` fall back to `status_code.to_string()`
/// and `"en-US"` respectively when left as `None`, so simple cases that only
/// carry a [`StatusCode`] stay allocation-free.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusReply {
    pub status_code: StatusCode,
    pub error_message: Option<String>,
    pub language_tag: Option<String>,
}

impl StatusReply {
    pub fn new(status_code: StatusCode) -> Self {
        Self {
            status_code,
            error_message: None,
            language_tag: None,
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.error_message = Some(message.into());
        self
    }

    pub fn with_language_tag(mut self, tag: impl Into<String>) -> Self {
        self.language_tag = Some(tag.into());
        self
    }
}

impl From<StatusCode> for StatusReply {
    fn from(status_code: StatusCode) -> Self {
        Self::new(status_code)
    }
}

// Lives here, not in `protocol/status.rs`, to keep `protocol` free of a
// dependency on `server`-side types.
impl StatusCode {
    /// Attach a custom message to this status code and produce a [`StatusReply`].
    ///
    /// Shorthand for `StatusReply::new(code).with_message(msg)`.
    pub fn with_message(self, message: impl Into<String>) -> StatusReply {
        StatusReply::new(self).with_message(message)
    }
}
