// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt::{self, Display, Formatter};

macro_rules! tmux_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(pub u64);

        impl $name {
            pub fn parse_wire(token: &[u8]) -> Option<Self> {
                parse_prefixed_decimal(token, $prefix).map(Self)
            }

            pub(crate) fn parse(token: &[u8]) -> Option<Self> {
                Self::parse_wire(token)
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}{}", $prefix as char, self.0)
            }
        }
    };
}

tmux_id!(PaneId, b'%');
tmux_id!(WindowId, b'@');
tmux_id!(SessionId, b'$');

pub(crate) fn parse_decimal(token: &[u8]) -> Option<u64> {
    if token.is_empty() {
        return None;
    }

    token.iter().try_fold(0_u64, |value, byte| {
        let digit = byte.checked_sub(b'0')?;
        (digit <= 9)
            .then_some(value)?
            .checked_mul(10)?
            .checked_add(u64::from(digit))
    })
}

fn parse_prefixed_decimal(token: &[u8], prefix: u8) -> Option<u64> {
    let digits = token.strip_prefix(&[prefix])?;
    parse_decimal(digits)
}
