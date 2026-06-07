//! SFTP subsystem with client and server support for Russh and more!
//!
//! Crate can provide compatibility with anything that can provide the raw data
//! stream in and out of the subsystem channel.
//!
//! The client implementation contains:
//!
//! * Standard communication via [RawSftpSession](crate::client::RawSftpSession) which provides methods
//!   for sending and receiving a packet in place.
//! * [High level](crate::client::SftpSession) is similar to [`std::fs`] and has almost all the same
//!   implementations. Implements Async I/O for interaction with files. The main idea is to abstract
//!   from all the nuances and flaws of the SFTP protocol. This also takes into account the extension
//!   provided by the server provided by the server such as `limits@openssh.com` and `fsync@openssh.com`.
//!
//! You can find more examples in the repository.

#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate serde;

mod buf;
/// Client side
pub mod client;
pub mod de;
mod error;
pub mod extensions;
/// Protocol implementation
pub mod protocol;
pub mod ser;
/// Server side
pub mod server;
mod utils;
