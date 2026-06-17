// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use crate::{
    X11AuthCookie, X11AuthMaterial, X11AuthProtocol, X11Result, inspect_setup_authentication,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11SpoofedAuth<ChannelId> {
    pub channel_id: ChannelId,
    pub auth: X11AuthMaterial,
    pub single_connection: bool,
}

pub struct X11AuthSpoofRegistry<ChannelId> {
    entries: Vec<X11SpoofedAuth<ChannelId>>,
}

impl<ChannelId> X11AuthSpoofRegistry<ChannelId> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<ChannelId> Default for X11AuthSpoofRegistry<ChannelId> {
    fn default() -> Self {
        Self::new()
    }
}

impl<ChannelId> fmt::Debug for X11AuthSpoofRegistry<ChannelId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Fake X11 cookies are bearer credentials, so registry Debug exposes
        // only the count and never the stored auth material.
        f.debug_struct("X11AuthSpoofRegistry")
            .field("entries_len", &self.entries.len())
            .finish()
    }
}

impl<ChannelId: Clone + Eq> X11AuthSpoofRegistry<ChannelId> {
    pub fn register_mit_magic_cookie(
        &mut self,
        channel_id: ChannelId,
        local_cookie: X11AuthCookie,
        single_connection: bool,
    ) -> X11AuthMaterial {
        let auth = X11AuthMaterial::mit_magic_cookie(local_cookie);
        self.insert(X11SpoofedAuth {
            channel_id,
            auth: auth.clone(),
            single_connection,
        });
        auth
    }

    pub fn insert(
        &mut self,
        entry: X11SpoofedAuth<ChannelId>,
    ) -> Option<X11SpoofedAuth<ChannelId>> {
        let replaced = self
            .entries
            .iter()
            .position(|existing| existing.auth.fake_cookie == entry.auth.fake_cookie)
            .map(|index| self.entries.remove(index));
        self.entries.push(entry);
        replaced
    }

    pub fn resolve(
        &mut self,
        protocol: X11AuthProtocol,
        fake_cookie: &X11AuthCookie,
    ) -> Option<X11SpoofedAuth<ChannelId>> {
        let index = self.entries.iter().position(|entry| {
            entry.auth.protocol == protocol && entry.auth.fake_cookie.constant_time_eq(fake_cookie)
        })?;

        if self.entries[index].single_connection {
            Some(self.entries.remove(index))
        } else {
            Some(self.entries[index].clone())
        }
    }

    pub fn resolve_setup_packet(
        &mut self,
        packet: &[u8],
    ) -> X11Result<Option<X11SpoofedAuth<ChannelId>>> {
        let setup_auth = inspect_setup_authentication(packet)?;
        Ok(self.resolve(setup_auth.protocol, &setup_auth.fake_cookie))
    }

    pub fn remove_channel(&mut self, channel_id: &ChannelId) -> Vec<X11SpoofedAuth<ChannelId>> {
        let mut removed = Vec::new();
        let mut index = 0;
        while index < self.entries.len() {
            if &self.entries[index].channel_id == channel_id {
                removed.push(self.entries.remove(index));
            } else {
                index += 1;
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use crate::{X11AuthCookie, X11AuthMaterial};

    use super::*;

    #[test]
    fn registry_reuses_multi_connection_entries() {
        let mut registry = X11AuthSpoofRegistry::new();
        let local = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = registry.register_mit_magic_cookie("channel-1", local, false);

        let first = registry
            .resolve(auth.protocol, &auth.fake_cookie)
            .expect("registered fake cookie should resolve");
        let second = registry
            .resolve(auth.protocol, &auth.fake_cookie)
            .expect("multi-use fake cookie should stay registered");

        assert_eq!(first.channel_id, "channel-1");
        assert_eq!(second.channel_id, "channel-1");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn registry_consumes_single_connection_entries() {
        let mut registry = X11AuthSpoofRegistry::new();
        let local = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = registry.register_mit_magic_cookie(7u64, local, true);

        assert!(registry.resolve(auth.protocol, &auth.fake_cookie).is_some());
        assert!(registry.resolve(auth.protocol, &auth.fake_cookie).is_none());
        assert!(registry.is_empty());
    }

    #[test]
    fn registry_can_insert_prebuilt_auth_material() {
        let mut registry = X11AuthSpoofRegistry::new();
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let local = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = X11AuthMaterial::with_fake_cookie(fake.clone(), local);

        registry.insert(X11SpoofedAuth {
            channel_id: "channel-1",
            auth,
            single_connection: true,
        });

        assert!(
            registry
                .resolve(X11AuthProtocol::MitMagicCookie1, &fake)
                .is_some()
        );
    }

    #[test]
    fn registry_debug_does_not_leak_cookie_material() {
        let mut registry = X11AuthSpoofRegistry::new();
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let local = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();

        registry.insert(X11SpoofedAuth {
            channel_id: "channel-1",
            auth: X11AuthMaterial::with_fake_cookie(fake, local),
            single_connection: false,
        });

        let debug = format!("{registry:?}");

        assert!(debug.contains("entries_len"));
        assert!(!debug.contains("aaaaaaaa"));
        assert!(!debug.contains("bbbbbbbb"));
    }
}
