// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::OnceLock;

use parking_lot::RwLock;
use reqwest::{Client, ClientBuilder};

use crate::{ApplicationProxyError, ApplicationProxyPolicy, http::configure_http_client_builder};

struct ApplicationProxyRuntime {
    policy: ApplicationProxyPolicy,
    default_client: Option<Client>,
    client_error: Option<String>,
}

impl ApplicationProxyRuntime {
    fn new(policy: ApplicationProxyPolicy) -> Self {
        let client = configure_http_client_builder(Client::builder(), &policy)
            .and_then(|builder| builder.build().map_err(Into::into));
        match client {
            Ok(default_client) => Self {
                policy,
                default_client: Some(default_client),
                client_error: None,
            },
            Err(error) => Self {
                policy,
                default_client: None,
                client_error: Some(error.to_string()),
            },
        }
    }
}

static APPLICATION_PROXY_RUNTIME: OnceLock<RwLock<ApplicationProxyRuntime>> = OnceLock::new();

fn runtime_store() -> &'static RwLock<ApplicationProxyRuntime> {
    APPLICATION_PROXY_RUNTIME
        .get_or_init(|| RwLock::new(ApplicationProxyRuntime::new(ApplicationProxyPolicy::System)))
}

pub fn set_application_proxy_policy(policy: ApplicationProxyPolicy) {
    // Replacing the application-owned runtime drops the previous zeroizing
    // policy and pooled client together.
    *runtime_store().write() = ApplicationProxyRuntime::new(policy);
}

pub fn application_http_client() -> Result<Client, ApplicationProxyError> {
    let runtime = runtime_store().read();
    runtime.default_client.clone().ok_or_else(|| {
        ApplicationProxyError::Unavailable(
            runtime
                .client_error
                .clone()
                .unwrap_or_else(|| "proxy client is unavailable".to_string()),
        )
    })
}

pub fn application_http_client_builder() -> Result<ClientBuilder, ApplicationProxyError> {
    configure_application_http_client_builder(Client::builder())
}

pub fn configure_application_http_client_builder(
    builder: ClientBuilder,
) -> Result<ClientBuilder, ApplicationProxyError> {
    // Apply credentials while borrowing the runtime policy so callers do not
    // create another owned password copy.
    configure_http_client_builder(builder, &runtime_store().read().policy)
}
