// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use zeroize::Zeroizing;

use crate::{
    X11AuthMaterial, X11AuthSpoofRegistry, X11ForwardingError, X11Result, X11SetupRequest,
    build_auth_failure_response, inspect_setup_request, required_setup_packet_len,
    rewrite_setup_authentication,
};

pub const DEFAULT_MAX_SETUP_PACKET_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11SetupBufferState {
    WaitingForHeader { buffered: usize },
    WaitingForBody { buffered: usize, required: usize },
    Complete,
}

pub struct X11SetupRewrite {
    pub request: X11SetupRequest,
    pub rewritten_setup: Zeroizing<Vec<u8>>,
    pub trailing_data: Vec<u8>,
}

impl fmt::Debug for X11SetupRewrite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The rewritten setup contains the real local X11 cookie, so only
        // expose sizes for diagnostics.
        f.debug_struct("X11SetupRewrite")
            .field("request", &self.request)
            .field("rewritten_setup_len", &self.rewritten_setup.len())
            .field("trailing_data_len", &self.trailing_data.len())
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11SetupRejectReason {
    AuthCookieMismatch,
    UnsupportedAuthProtocol,
}

pub struct X11SetupReject {
    pub request: X11SetupRequest,
    pub reason: X11SetupRejectReason,
    pub failure_response: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for X11SetupReject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X11SetupReject")
            .field("request", &self.request)
            .field("reason", &self.reason)
            .field("failure_response_len", &self.failure_response.len())
            .finish()
    }
}

pub enum X11SetupDecision {
    Forward(X11SetupRewrite),
    Reject(X11SetupReject),
}

impl fmt::Debug for X11SetupDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forward(rewrite) => f.debug_tuple("Forward").field(rewrite).finish(),
            Self::Reject(reject) => f.debug_tuple("Reject").field(reject).finish(),
        }
    }
}

pub struct X11RegisteredSetupRewrite<ChannelId> {
    pub channel_id: ChannelId,
    pub rewrite: X11SetupRewrite,
}

impl<ChannelId: fmt::Debug> fmt::Debug for X11RegisteredSetupRewrite<ChannelId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X11RegisteredSetupRewrite")
            .field("channel_id", &self.channel_id)
            .field("rewrite", &self.rewrite)
            .finish()
    }
}

pub enum X11RegisteredSetupDecision<ChannelId> {
    Forward(X11RegisteredSetupRewrite<ChannelId>),
    Reject(X11SetupReject),
}

impl<ChannelId: fmt::Debug> fmt::Debug for X11RegisteredSetupDecision<ChannelId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Forward(rewrite) => f.debug_tuple("Forward").field(rewrite).finish(),
            Self::Reject(reject) => f.debug_tuple("Reject").field(reject).finish(),
        }
    }
}

pub struct X11SetupBuffer {
    bytes: Zeroizing<Vec<u8>>,
    max_setup_bytes: usize,
    complete: bool,
}

impl X11SetupBuffer {
    pub fn new() -> Self {
        Self::with_max_setup_bytes(DEFAULT_MAX_SETUP_PACKET_BYTES)
    }

    pub fn with_max_setup_bytes(max_setup_bytes: usize) -> Self {
        Self {
            bytes: Zeroizing::new(Vec::new()),
            max_setup_bytes,
            complete: false,
        }
    }

    pub fn state(&self) -> X11Result<X11SetupBufferState> {
        if self.complete {
            return Ok(X11SetupBufferState::Complete);
        }

        match required_setup_packet_len(self.bytes.as_slice())? {
            Some(required) => Ok(X11SetupBufferState::WaitingForBody {
                buffered: self.bytes.len(),
                required,
            }),
            None => Ok(X11SetupBufferState::WaitingForHeader {
                buffered: self.bytes.len(),
            }),
        }
    }

    pub fn push(
        &mut self,
        chunk: &[u8],
        auth: &X11AuthMaterial,
    ) -> X11Result<Option<X11SetupRewrite>> {
        match self.push_decision(chunk, auth)? {
            Some(X11SetupDecision::Forward(rewrite)) => Ok(Some(rewrite)),
            Some(X11SetupDecision::Reject(reject)) => Err(match reject.reason {
                X11SetupRejectReason::AuthCookieMismatch => X11ForwardingError::AuthCookieMismatch,
                X11SetupRejectReason::UnsupportedAuthProtocol => {
                    X11ForwardingError::UnsupportedAuthProtocol(reject.request.auth_protocol)
                }
            }),
            None => Ok(None),
        }
    }

    pub fn push_decision(
        &mut self,
        chunk: &[u8],
        auth: &X11AuthMaterial,
    ) -> X11Result<Option<X11SetupDecision>> {
        let Some((mut setup, trailing_data)) = self.push_complete_setup(chunk)? else {
            return Ok(None);
        };

        match rewrite_setup_authentication(&mut setup, auth) {
            Ok(request) => Ok(Some(X11SetupDecision::Forward(X11SetupRewrite {
                request,
                rewritten_setup: Zeroizing::new(setup),
                trailing_data,
            }))),
            Err(error @ X11ForwardingError::AuthCookieMismatch) => {
                let request = inspect_setup_request(&setup)?;
                let failure =
                    build_auth_failure_response(&request, "X11 forwarding authentication failed")?;
                Ok(Some(X11SetupDecision::Reject(X11SetupReject {
                    request,
                    reason: reject_reason_for_error(&error),
                    failure_response: failure.response,
                })))
            }
            Err(error @ X11ForwardingError::UnsupportedAuthProtocol(_)) => {
                let request = inspect_setup_request(&setup)?;
                let failure =
                    build_auth_failure_response(&request, "Unsupported X11 auth protocol")?;
                Ok(Some(X11SetupDecision::Reject(X11SetupReject {
                    request,
                    reason: reject_reason_for_error(&error),
                    failure_response: failure.response,
                })))
            }
            Err(error) => Err(error),
        }
    }

    pub fn push_registry_decision<ChannelId: Clone + Eq>(
        &mut self,
        chunk: &[u8],
        registry: &mut X11AuthSpoofRegistry<ChannelId>,
    ) -> X11Result<Option<X11RegisteredSetupDecision<ChannelId>>> {
        let Some((mut setup, trailing_data)) = self.push_complete_setup(chunk)? else {
            return Ok(None);
        };

        match registry.resolve_setup_packet(&setup) {
            Ok(Some(entry)) => {
                let request = rewrite_setup_authentication(&mut setup, &entry.auth)?;
                Ok(Some(X11RegisteredSetupDecision::Forward(
                    X11RegisteredSetupRewrite {
                        channel_id: entry.channel_id,
                        rewrite: X11SetupRewrite {
                            request,
                            rewritten_setup: Zeroizing::new(setup),
                            trailing_data,
                        },
                    },
                )))
            }
            Ok(None) => Ok(Some(X11RegisteredSetupDecision::Reject(
                reject_setup_packet(
                    &setup,
                    X11SetupRejectReason::AuthCookieMismatch,
                    "X11 forwarding authentication failed",
                )?,
            ))),
            Err(X11ForwardingError::UnsupportedAuthProtocol(_)) => Ok(Some(
                X11RegisteredSetupDecision::Reject(reject_setup_packet(
                    &setup,
                    X11SetupRejectReason::UnsupportedAuthProtocol,
                    "Unsupported X11 auth protocol",
                )?),
            )),
            Err(X11ForwardingError::InvalidAuthCookie(_)) => Ok(Some(
                X11RegisteredSetupDecision::Reject(reject_setup_packet(
                    &setup,
                    X11SetupRejectReason::AuthCookieMismatch,
                    "X11 forwarding authentication failed",
                )?),
            )),
            Err(error) => Err(error),
        }
    }

    fn push_complete_setup(&mut self, chunk: &[u8]) -> X11Result<Option<(Vec<u8>, Vec<u8>)>> {
        if self.complete {
            return Err(X11ForwardingError::InvalidSetupPacketLength);
        }

        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() > self.max_setup_bytes {
            return Err(X11ForwardingError::SetupPacketTooLarge(
                self.max_setup_bytes,
            ));
        }

        let Some(required) = required_setup_packet_len(self.bytes.as_slice())? else {
            return Ok(None);
        };

        if self.bytes.len() < required {
            return Ok(None);
        }

        let trailing_data = self.bytes.split_off(required);
        let setup = std::mem::take(self.bytes.as_mut());
        self.complete = true;
        Ok(Some((setup, trailing_data)))
    }
}

fn reject_reason_for_error(error: &X11ForwardingError) -> X11SetupRejectReason {
    match error {
        X11ForwardingError::AuthCookieMismatch => X11SetupRejectReason::AuthCookieMismatch,
        X11ForwardingError::UnsupportedAuthProtocol(_) => {
            X11SetupRejectReason::UnsupportedAuthProtocol
        }
        _ => unreachable!("only authentication errors can become X11 setup rejection packets"),
    }
}

fn reject_setup_packet(
    setup: &[u8],
    reason: X11SetupRejectReason,
    failure_message: &str,
) -> X11Result<X11SetupReject> {
    let request = inspect_setup_request(setup)?;
    let failure = build_auth_failure_response(&request, failure_message)?;
    Ok(X11SetupReject {
        request,
        reason,
        failure_response: failure.response,
    })
}

impl Default for X11SetupBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::{X11AuthCookie, X11AuthMaterial, X11AuthSpoofRegistry, X11ByteOrder};

    use super::*;

    #[test]
    fn buffers_fragmented_setup_packet_and_preserves_trailing_data() {
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let real = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = X11AuthMaterial::with_fake_cookie(fake, real);
        let mut packet = setup_packet(X11ByteOrder::LittleEndian, auth.fake_cookie.as_bytes());
        packet.extend_from_slice(b"already-x11-data");
        let split = 7;
        let mut buffer = X11SetupBuffer::new();

        assert!(buffer.push(&packet[..split], &auth).unwrap().is_none());
        assert!(matches!(
            buffer.state().unwrap(),
            X11SetupBufferState::WaitingForHeader { buffered: 7 }
        ));
        let rewrite = buffer.push(&packet[split..], &auth).unwrap().unwrap();

        assert_eq!(rewrite.request.auth_protocol, "MIT-MAGIC-COOKIE-1");
        assert_eq!(rewrite.trailing_data, b"already-x11-data");
        assert!(
            rewrite
                .rewritten_setup
                .windows(auth.local_cookie.len())
                .any(|window| window == auth.local_cookie.as_bytes())
        );
        assert!(matches!(
            buffer.state().unwrap(),
            X11SetupBufferState::Complete
        ));
    }

    #[test]
    fn setup_rewrite_debug_redacts_packet_bytes() {
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let real = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = X11AuthMaterial::with_fake_cookie(fake, real);
        let packet = setup_packet(X11ByteOrder::LittleEndian, auth.fake_cookie.as_bytes());
        let mut buffer = X11SetupBuffer::new();

        let rewrite = buffer.push(&packet, &auth).unwrap().unwrap();
        let debug = format!("{rewrite:?}");

        assert!(!debug.contains("bbbb"));
        assert!(debug.contains("rewritten_setup_len"));
    }

    #[test]
    fn setup_buffer_limits_unfinished_packet_growth() {
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let real = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = X11AuthMaterial::with_fake_cookie(fake, real);
        let mut buffer = X11SetupBuffer::with_max_setup_bytes(4);

        let error = buffer.push(b"12345", &auth).unwrap_err();

        assert_eq!(error, X11ForwardingError::SetupPacketTooLarge(4));
    }

    #[test]
    fn setup_decision_rejects_bad_fake_cookie_with_failure_response() {
        let auth = X11AuthMaterial::with_fake_cookie(
            X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        );
        let wrong = X11AuthCookie::from_hex("cccccccccccccccccccccccccccccccc").unwrap();
        let packet = setup_packet(X11ByteOrder::LittleEndian, wrong.as_bytes());
        let mut buffer = X11SetupBuffer::new();

        let decision = buffer.push_decision(&packet, &auth).unwrap().unwrap();

        let X11SetupDecision::Reject(reject) = decision else {
            panic!("bad fake cookie should reject");
        };
        assert_eq!(reject.reason, X11SetupRejectReason::AuthCookieMismatch);
        assert_eq!(reject.failure_response[0], 0);
        assert!(!format!("{reject:?}").contains("cccc"));
    }

    #[test]
    fn registry_decision_resolves_channel_and_consumes_single_connection_auth() {
        let mut registry = X11AuthSpoofRegistry::new();
        let local = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = registry.register_mit_magic_cookie("x11-channel", local, true);
        let mut packet = setup_packet(X11ByteOrder::LittleEndian, auth.fake_cookie.as_bytes());
        packet.extend_from_slice(b"first-x11-payload");
        let mut buffer = X11SetupBuffer::new();

        let decision = buffer
            .push_registry_decision(&packet, &mut registry)
            .unwrap()
            .unwrap();

        let X11RegisteredSetupDecision::Forward(forward) = decision else {
            panic!("registered fake cookie should forward");
        };
        assert_eq!(forward.channel_id, "x11-channel");
        assert_eq!(forward.rewrite.trailing_data, b"first-x11-payload");
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_decision_rejects_unknown_fake_cookie() {
        let mut registry = X11AuthSpoofRegistry::<&str>::new();
        let wrong = X11AuthCookie::from_hex("cccccccccccccccccccccccccccccccc").unwrap();
        let packet = setup_packet(X11ByteOrder::LittleEndian, wrong.as_bytes());
        let mut buffer = X11SetupBuffer::new();

        let decision = buffer
            .push_registry_decision(&packet, &mut registry)
            .unwrap()
            .unwrap();

        let X11RegisteredSetupDecision::Reject(reject) = decision else {
            panic!("unknown fake cookie should reject");
        };
        assert_eq!(reject.reason, X11SetupRejectReason::AuthCookieMismatch);
        assert_eq!(reject.failure_response[0], 0);
    }

    fn setup_packet(byte_order: X11ByteOrder, cookie: &[u8]) -> Vec<u8> {
        let protocol = b"MIT-MAGIC-COOKIE-1";
        let mut packet = Vec::new();
        packet.push(match byte_order {
            X11ByteOrder::BigEndian => b'B',
            X11ByteOrder::LittleEndian => b'l',
        });
        packet.push(0);
        push_u16(&mut packet, 11, byte_order);
        push_u16(&mut packet, 0, byte_order);
        push_u16(&mut packet, protocol.len() as u16, byte_order);
        push_u16(&mut packet, cookie.len() as u16, byte_order);
        push_u16(&mut packet, 0, byte_order);
        packet.extend_from_slice(protocol);
        packet.extend(std::iter::repeat_n(
            0,
            padded_len(protocol.len()) - protocol.len(),
        ));
        packet.extend_from_slice(cookie);
        packet.extend(std::iter::repeat_n(
            0,
            padded_len(cookie.len()) - cookie.len(),
        ));
        packet
    }

    fn push_u16(packet: &mut Vec<u8>, value: u16, byte_order: X11ByteOrder) {
        packet.extend_from_slice(&match byte_order {
            X11ByteOrder::BigEndian => value.to_be_bytes(),
            X11ByteOrder::LittleEndian => value.to_le_bytes(),
        });
    }

    fn padded_len(len: usize) -> usize {
        (len + 3) & !3
    }
}
