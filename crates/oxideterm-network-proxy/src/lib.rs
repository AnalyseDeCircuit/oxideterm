// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Application-wide HTTP proxy policy, credential hydration, and client adapters.

mod credentials;
mod http;
mod policy;
mod runtime;
mod settings;

pub use credentials::ApplicationProxyCredentialProvider;
pub use policy::{
    ApplicationProxyAuth, ApplicationProxyError, ApplicationProxyPolicy, ApplicationProxyProtocol,
    CustomApplicationProxy,
};
pub use runtime::{
    application_http_client, application_http_client_builder,
    configure_application_http_client_builder, set_application_proxy_policy,
};
pub use settings::{
    application_proxy_policy_from_settings, configure_update_http_client_builder,
    install_application_proxy_policy_from_settings,
};

#[cfg(test)]
mod tests;
