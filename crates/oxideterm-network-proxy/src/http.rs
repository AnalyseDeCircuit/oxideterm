// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::net::IpAddr;

use reqwest::{ClientBuilder, NoProxy, Proxy};

use crate::{
    ApplicationProxyAuth, ApplicationProxyError, ApplicationProxyPolicy, ApplicationProxyProtocol,
    CustomApplicationProxy,
};

pub(crate) fn configure_http_client_builder(
    mut builder: ClientBuilder,
    policy: &ApplicationProxyPolicy,
) -> Result<ClientBuilder, ApplicationProxyError> {
    match policy {
        ApplicationProxyPolicy::Direct => Ok(builder.no_proxy()),
        ApplicationProxyPolicy::System => Ok(builder),
        ApplicationProxyPolicy::Custom(config) => {
            let mut proxy = Proxy::all(proxy_url(config)?)?;
            if !config.no_proxy.trim().is_empty() {
                proxy = proxy.no_proxy(NoProxy::from_string(&config.no_proxy));
            }
            if let ApplicationProxyAuth::Password { username, password } = &config.auth {
                proxy = proxy.basic_auth(username, password.as_str());
            }
            builder = builder.no_proxy().proxy(proxy);
            Ok(builder)
        }
        ApplicationProxyPolicy::Unavailable { reason } => {
            Err(ApplicationProxyError::Unavailable(reason.clone()))
        }
    }
}

pub(crate) fn proxy_url(config: &CustomApplicationProxy) -> Result<String, ApplicationProxyError> {
    if config.host.trim().is_empty() || config.port == 0 {
        return Err(ApplicationProxyError::InvalidConfiguration(
            "proxy host and port are required".to_string(),
        ));
    }
    let scheme = match config.protocol {
        ApplicationProxyProtocol::HttpConnect => "http",
        ApplicationProxyProtocol::HttpsConnect => "https",
        ApplicationProxyProtocol::Socks5 if config.remote_dns => "socks5h",
        ApplicationProxyProtocol::Socks5 => "socks5",
    };
    let host = proxy_url_host(&config.host);
    let url = format!("{scheme}://{host}:{}", config.port);
    Proxy::all(&url)
        .map(|_| url)
        .map_err(ApplicationProxyError::Client)
}

fn proxy_url_host(host: &str) -> String {
    let trimmed = host.trim().trim_matches(['[', ']']);
    if trimmed
        .parse::<IpAddr>()
        .is_ok_and(|address| address.is_ipv6())
    {
        format!("[{trimmed}]")
    } else {
        trimmed.to_string()
    }
}
