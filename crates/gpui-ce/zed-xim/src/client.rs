mod attribute_builder;

pub use self::attribute_builder::AttributeBuilder;
use crate::AHashMap;
use xim_parser::{
    Attr, Attribute, AttributeName, CaretDirection, CaretStyle, CommitData, Extension, Feedback,
    ForwardEventFlag, PreeditDrawStatus, Request,
};

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    ReadProtocol(xim_parser::ReadError),
    InvalidCompoundText(xim_ctext::DecodeError),
    XimError(xim_parser::ErrorCode, String),
    UnsupportedTransport,
    InvalidReply,
    NoXimServer,
    #[cfg(feature = "std")]
    Other(alloc::boxed::Box<dyn std::error::Error + Send + Sync>),
}

impl From<xim_parser::ReadError> for ClientError {
    fn from(e: xim_parser::ReadError) -> Self {
        Self::ReadProtocol(e)
    }
}

impl From<xim_ctext::DecodeError> for ClientError {
    fn from(error: xim_ctext::DecodeError) -> Self {
        Self::InvalidCompoundText(error)
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientError::ReadProtocol(e) => write!(f, "Can't read xim message: {}", e),
            ClientError::InvalidCompoundText(e) => {
                write!(f, "Can't decode XIM compound text: {}", e)
            }
            ClientError::XimError(code, detail) => {
                write!(f, "Server send error code: {:?}, detail: {}", code, detail)
            }
            ClientError::UnsupportedTransport => write!(f, "Server Transport is not supported"),
            ClientError::InvalidReply => write!(f, "Invalid reply from server"),
            ClientError::NoXimServer => write!(f, "Can't connect xim server"),
            #[cfg(feature = "std")]
            ClientError::Other(e) => write!(f, "Other error: {}", e),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ClientError {}

pub fn handle_request<C: ClientCore>(
    client: &mut C,
    handler: &mut impl ClientHandler<C>,
    req: Request,
) -> Result<(), ClientError> {
    if log::log_enabled!(log::Level::Trace) {
        log::trace!("<-: {:?}", req);
    } else {
        log::debug!("<-: {}", req.name());
    }

    match req {
        Request::ConnectReply {
            server_major_protocol_version: _,
            server_minor_protocol_version: _,
        } => handler.handle_connect(client),
        Request::OpenReply {
            input_method_id,
            im_attrs,
            ic_attrs,
        } => {
            log::debug!("im_attrs: {:#?}", im_attrs);
            log::debug!("ic_attrs: {:#?}", ic_attrs);
            client.set_attrs(im_attrs, ic_attrs);
            // Require for uim
            client.send_req(Request::EncodingNegotiation {
                encodings: vec!["COMPOUND_TEXT".into()],
                encoding_infos: vec![],
                input_method_id,
            })
        }
        Request::EncodingNegotiationReply {
            input_method_id,
            index: _,
            category: _,
        } => handler.handle_open(client, input_method_id),
        Request::QueryExtensionReply {
            input_method_id: _,
            extensions,
        } => handler.handle_query_extension(client, &extensions),
        Request::GetImValuesReply {
            input_method_id,
            im_attributes,
        } => handler.handle_get_im_values(
            client,
            input_method_id,
            im_attributes
                .into_iter()
                .filter_map(|attr| {
                    client
                        .im_attributes()
                        .iter()
                        .find(|(_, v)| **v == attr.id)
                        .map(|(n, _)| (*n, attr.value))
                })
                .collect(),
        ),
        Request::SetIcValuesReply {
            input_method_id,
            input_context_id,
        } => handler.handle_set_ic_values(client, input_method_id, input_context_id),
        Request::CreateIcReply {
            input_method_id,
            input_context_id,
        } => handler.handle_create_ic(client, input_method_id, input_context_id),
        Request::DestroyIcReply {
            input_method_id,
            input_context_id,
        } => handler.handle_destroy_ic(client, input_method_id, input_context_id),
        Request::SetEventMask {
            input_method_id,
            input_context_id,
            forward_event_mask,
            synchronous_event_mask,
        } => handler.handle_set_event_mask(
            client,
            input_method_id,
            input_context_id,
            forward_event_mask,
            synchronous_event_mask,
        ),
        Request::CloseReply { input_method_id } => handler.handle_close(client, input_method_id),
        Request::DisconnectReply {} => {
            handler.handle_disconnect();
            Ok(())
        }
        Request::ResetIcReply {
            input_method_id,
            input_context_id,
            preedit_string,
        } => {
            let preedit_string = decode_compound_text(&preedit_string)?;
            handler.handle_reset_ic(client, input_method_id, input_context_id, &preedit_string)?;
            Ok(())
        }
        Request::Error { code, detail, .. } => Err(ClientError::XimError(code, detail)),
        Request::ForwardEvent {
            xev,
            input_method_id,
            input_context_id,
            flag,
            ..
        } => {
            handler.handle_forward_event(
                client,
                input_method_id,
                input_context_id,
                flag,
                client.deserialize_event(&xev),
            )?;

            if flag.contains(ForwardEventFlag::SYNCHRONOUS) {
                client.send_req(Request::SyncReply {
                    input_method_id,
                    input_context_id,
                })?;
            }

            Ok(())
        }
        Request::Commit {
            input_method_id,
            input_context_id,
            data,
        } => match data {
            CommitData::Keysym { keysym: _, .. } => {
                log::warn!("Keysym commit is not supported");
                Ok(())
            }
            CommitData::Chars {
                commited,
                syncronous,
            } => {
                handler.handle_commit(
                    client,
                    input_method_id,
                    input_context_id,
                    &decode_compound_text(&commited)?,
                )?;

                if syncronous {
                    client.send_req(Request::SyncReply {
                        input_method_id,
                        input_context_id,
                    })?;
                }

                Ok(())
            }
            CommitData::Both { .. } => {
                log::warn!("Both commit data is not supported");
                Ok(())
            }
        },
        Request::Sync {
            input_method_id,
            input_context_id,
        } => client.send_req(Request::SyncReply {
            input_method_id,
            input_context_id,
        }),
        Request::SyncReply { .. } => {
            // Nothing to do
            Ok(())
        }
        Request::PreeditStart {
            input_method_id,
            input_context_id,
        } => handler.handle_preedit_start(client, input_method_id, input_context_id),
        Request::PreeditDone {
            input_method_id,
            input_context_id,
        } => handler.handle_preedit_done(client, input_method_id, input_context_id),
        Request::PreeditDraw {
            input_method_id,
            input_context_id,
            caret,
            chg_first,
            chg_length,
            preedit_string,
            status,
            feedbacks,
        } => {
            let preedit_string = decode_compound_text(&preedit_string)?;
            handler.handle_preedit_draw(
                client,
                input_method_id,
                input_context_id,
                caret,
                chg_first,
                chg_length,
                status,
                &preedit_string,
                feedbacks,
            )
        }
        Request::PreeditCaret {
            input_method_id,
            input_context_id,
            mut position,
            direction,
            style,
        } => {
            // Handle the request.
            handler.handle_preedit_caret(
                client,
                input_method_id,
                input_context_id,
                &mut position,
                direction,
                style,
            )?;

            // Send the reply.
            client.send_req(Request::PreeditCaretReply {
                input_method_id,
                input_context_id,
                position,
            })
        }
        _ => {
            log::warn!("Unknown request {:?}", req);
            Ok(())
        }
    }
}

fn decode_compound_text(bytes: &[u8]) -> Result<String, ClientError> {
    // Input methods are external processes, so malformed text must remain a recoverable error.
    Ok(xim_ctext::compound_text_to_utf8(bytes)?)
}

pub trait ClientCore {
    type XEvent;

    fn set_attrs(&mut self, ic_attrs: Vec<Attr>, im_attrs: Vec<Attr>);
    fn ic_attributes(&self) -> &AHashMap<AttributeName, u16>;
    fn im_attributes(&self) -> &AHashMap<AttributeName, u16>;
    fn serialize_event(&self, xev: &Self::XEvent) -> xim_parser::XEvent;
    fn deserialize_event(&self, xev: &xim_parser::XEvent) -> Self::XEvent;
    fn send_req(&mut self, req: Request) -> Result<(), ClientError>;
}

pub trait Client {
    type XEvent;

    fn build_ic_attributes(&self) -> AttributeBuilder<'_>;
    fn build_im_attributes(&self) -> AttributeBuilder<'_>;

    fn disconnect(&mut self) -> Result<(), ClientError>;
    fn open(&mut self, locale: &str) -> Result<(), ClientError>;
    fn close(&mut self, input_method_id: u16) -> Result<(), ClientError>;
    fn quert_extension(
        &mut self,
        input_method_id: u16,
        extensions: &[&str],
    ) -> Result<(), ClientError>;
    fn get_im_values(
        &mut self,
        input_method_id: u16,
        names: &[AttributeName],
    ) -> Result<(), ClientError>;
    fn set_ic_values(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError>;
    fn create_ic(
        &mut self,
        input_method_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError>;
    fn destroy_ic(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError>;
    fn forward_event(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        xev: &Self::XEvent,
    ) -> Result<(), ClientError>;
    fn set_focus(&mut self, input_method_id: u16, input_context_id: u16)
        -> Result<(), ClientError>;
    fn unset_focus(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError>;
    fn reset_ic(&mut self, input_method_id: u16, input_context_id: u16) -> Result<(), ClientError>;
}

impl<C> Client for C
where
    C: ClientCore,
{
    type XEvent = C::XEvent;

    fn build_ic_attributes(&self) -> AttributeBuilder<'_> {
        AttributeBuilder::new(self.ic_attributes())
    }

    fn build_im_attributes(&self) -> AttributeBuilder<'_> {
        AttributeBuilder::new(self.im_attributes())
    }

    fn open(&mut self, locale: &str) -> Result<(), ClientError> {
        self.send_req(Request::Open {
            locale: locale.into(),
        })
    }

    fn quert_extension(
        &mut self,
        input_method_id: u16,
        extensions: &[&str],
    ) -> Result<(), ClientError> {
        self.send_req(Request::QueryExtension {
            input_method_id,
            extensions: extensions.iter().map(|&e| e.into()).collect(),
        })
    }

    fn get_im_values(
        &mut self,
        input_method_id: u16,
        names: &[AttributeName],
    ) -> Result<(), ClientError> {
        self.send_req(Request::GetImValues {
            input_method_id,
            im_attributes: names
                .iter()
                .filter_map(|name| self.im_attributes().get(name).copied())
                .collect(),
        })
    }

    fn set_ic_values(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError> {
        self.send_req(Request::SetIcValues {
            input_method_id,
            input_context_id,
            ic_attributes,
        })
    }

    fn create_ic(
        &mut self,
        input_method_id: u16,
        ic_attributes: Vec<Attribute>,
    ) -> Result<(), ClientError> {
        self.send_req(Request::CreateIc {
            input_method_id,
            ic_attributes,
        })
    }

    fn forward_event(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        xev: &Self::XEvent,
    ) -> Result<(), ClientError> {
        let ev = self.serialize_event(xev);
        self.send_req(Request::ForwardEvent {
            input_method_id,
            input_context_id,
            flag,
            serial_number: ev.sequence,
            xev: ev,
        })
    }

    fn disconnect(&mut self) -> Result<(), ClientError> {
        self.send_req(Request::Disconnect {})
    }

    fn close(&mut self, input_method_id: u16) -> Result<(), ClientError> {
        self.send_req(Request::Close { input_method_id })
    }

    fn destroy_ic(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        self.send_req(Request::DestroyIc {
            input_method_id,
            input_context_id,
        })
    }

    fn set_focus(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        self.send_req(Request::SetIcFocus {
            input_method_id,
            input_context_id,
        })
    }
    fn unset_focus(
        &mut self,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        self.send_req(Request::UnsetIcFocus {
            input_method_id,
            input_context_id,
        })
    }
    fn reset_ic(&mut self, input_method_id: u16, input_context_id: u16) -> Result<(), ClientError> {
        self.send_req(Request::ResetIc {
            input_method_id,
            input_context_id,
        })
    }
}

#[allow(unused_variables)]
pub trait ClientHandler<C: Client> {
    fn handle_connect(&mut self, client: &mut C) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_disconnect(&mut self) {}
    fn handle_open(&mut self, client: &mut C, input_method_id: u16) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_close(&mut self, client: &mut C, input_method_id: u16) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_query_extension(
        &mut self,
        client: &mut C,
        extensions: &[Extension],
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_get_im_values(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        attributes: AHashMap<AttributeName, Vec<u8>>,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_set_ic_values(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_create_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_destroy_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_commit(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        text: &str,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_forward_event(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        flag: ForwardEventFlag,
        xev: C::XEvent,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_set_event_mask(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        forward_event_mask: u32,
        synchronous_event_mask: u32,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_preedit_start(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_preedit_draw(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        caret: i32,
        chg_first: i32,
        chg_len: i32,
        status: PreeditDrawStatus,
        preedit_string: &str,
        feedbacks: Vec<Feedback>,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_preedit_caret(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        position: &mut i32,
        direction: CaretDirection,
        style: CaretStyle,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_preedit_done(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
    ) -> Result<(), ClientError> {
        Ok(())
    }
    fn handle_reset_ic(
        &mut self,
        client: &mut C,
        input_method_id: u16,
        input_context_id: u16,
        preedit_text: &str,
    ) -> Result<(), ClientError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_compound_text, ClientError};

    #[test]
    fn unsupported_compound_text_is_returned_as_an_error() {
        let result = decode_compound_text(&[0x1B, 0x24, 0x28, 0x7F, 0x41, 0x42]);

        assert!(matches!(result, Err(ClientError::InvalidCompoundText(_))));
    }
}
