use std::future::Future;

use super::error::Error;
use crate::protocol::{Attrs, Data, ExtendedReply, Handle, Name, Status, Version};

/// Client stream handler. This is `async_trait`
#[cfg_attr(feature = "async-trait", async_trait::async_trait)]
pub trait Handler: Sized {
    type Error: Into<Error>;

    /// Called on SSH_FXP_VERSION.
    #[allow(unused_variables)]
    fn version(
        &mut self,
        version: Version,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }

    /// Called on SSH_FXP_STATUS.
    #[allow(unused_variables)]
    fn status(&mut self, status: Status) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }

    /// Called on SSH_FXP_HANDLE.
    #[allow(unused_variables)]
    fn handle(&mut self, handle: Handle) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }

    /// Called on SSH_FXP_DATA.
    #[allow(unused_variables)]
    fn data(&mut self, data: Data) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }

    /// Called on SSH_FXP_NAME.
    #[allow(unused_variables)]
    fn name(&mut self, name: Name) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }

    /// Called on SSH_FXP_ATTRS.
    #[allow(unused_variables)]
    fn attrs(&mut self, attrs: Attrs) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }

    /// Called on SSH_EXTENDED_REPLY.
    #[allow(unused_variables)]
    fn extended_reply(
        &mut self,
        reply: ExtendedReply,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }
}
