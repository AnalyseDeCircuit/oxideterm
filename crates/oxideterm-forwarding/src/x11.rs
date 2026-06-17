// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt, future::Future, pin::Pin};

use oxideterm_ssh::{X11ForwardHandler, X11ForwardedChannel};
use oxideterm_x11_forwarding::{
    X11AuthMaterial, X11LocalEndpoint, X11RuntimeError, bridge_x11_stream_to_endpoint,
};

pub struct X11ForwardBridge {
    endpoint: X11LocalEndpoint,
    auth: X11AuthMaterial,
}

impl X11ForwardBridge {
    pub fn new(endpoint: X11LocalEndpoint, auth: X11AuthMaterial) -> Self {
        Self { endpoint, auth }
    }

    pub fn endpoint(&self) -> &X11LocalEndpoint {
        &self.endpoint
    }
}

impl fmt::Debug for X11ForwardBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X11ForwardBridge")
            .field("endpoint", &self.endpoint)
            .field("auth", &"<redacted>")
            .finish()
    }
}

impl X11ForwardHandler for X11ForwardBridge {
    fn handle_x11_forward(
        &self,
        event: X11ForwardedChannel,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let endpoint = self.endpoint.clone();
        let auth = self.auth.clone();
        Box::pin(async move {
            if let Err(error) = bridge_event(event, endpoint, auth).await {
                tracing::debug!(error = %error, "X11 channel bridge failed");
            }
        })
    }
}

async fn bridge_event(
    event: X11ForwardedChannel,
    endpoint: X11LocalEndpoint,
    auth: X11AuthMaterial,
) -> Result<(), X11RuntimeError> {
    // Each server-opened X11 channel begins with an untrusted setup packet.
    // The bridge validates the fake cookie and rewrites it before any bytes
    // reach the user's local X server.
    bridge_x11_stream_to_endpoint(event.stream, &endpoint, &auth).await
}
