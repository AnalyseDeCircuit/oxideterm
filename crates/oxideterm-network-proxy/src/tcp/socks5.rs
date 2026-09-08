use super::*;

pub(super) async fn negotiate_socks5_auth(
    stream: &mut TcpStream,
    auth: &UpstreamProxyAuth,
) -> Result<(), TcpProxyError> {
    match auth {
        UpstreamProxyAuth::None => {
            stream
                .write_all(&[SOCKS_VERSION, 1, SOCKS_METHOD_NO_AUTH])
                .await
                .map_err(socks_io_error)?;
        }
        UpstreamProxyAuth::Password { .. } => {
            stream
                .write_all(&[
                    SOCKS_VERSION,
                    2,
                    SOCKS_METHOD_NO_AUTH,
                    SOCKS_METHOD_PASSWORD,
                ])
                .await
                .map_err(socks_io_error)?;
        }
    }

    let mut response = [0_u8; 2];
    stream
        .read_exact(&mut response)
        .await
        .map_err(socks_io_error)?;
    if response[0] != SOCKS_VERSION {
        return proxy_error(UpstreamProxyError::SocksInvalidGreeting);
    }

    match response[1] {
        SOCKS_METHOD_NO_AUTH => Ok(()),
        SOCKS_METHOD_PASSWORD => authenticate_socks5_password(stream, auth).await,
        SOCKS_METHOD_NO_ACCEPTABLE => proxy_error(UpstreamProxyError::SocksRejectedAuthMethods),
        method => proxy_error(UpstreamProxyError::SocksUnsupportedAuthMethod(method)),
    }
}

async fn authenticate_socks5_password(
    stream: &mut TcpStream,
    auth: &UpstreamProxyAuth,
) -> Result<(), TcpProxyError> {
    let UpstreamProxyAuth::Password { username, password } = auth else {
        return proxy_error(UpstreamProxyError::SocksMissingCredentials);
    };
    let username = username.as_bytes();
    let password = password.as_bytes();
    if username.len() > u8::MAX as usize || password.len() > u8::MAX as usize {
        return proxy_error(UpstreamProxyError::SocksCredentialsTooLong);
    }

    let mut request = Zeroizing::new(Vec::with_capacity(3 + username.len() + password.len()));
    request.push(SOCKS_AUTH_VERSION);
    request.push(username.len() as u8);
    request.extend_from_slice(username);
    request.push(password.len() as u8);
    request.extend_from_slice(password);
    stream.write_all(&request).await.map_err(socks_io_error)?;

    let mut response = [0_u8; 2];
    stream
        .read_exact(&mut response)
        .await
        .map_err(socks_io_error)?;
    if response[0] != SOCKS_AUTH_VERSION {
        return proxy_error(UpstreamProxyError::SocksInvalidAuthReply);
    }
    if response[1] != 0 {
        return proxy_error(UpstreamProxyError::SocksAuthFailed);
    }
    Ok(())
}

pub(super) async fn send_socks5_connect(
    stream: &mut TcpStream,
    target_host: &str,
    target_port: u16,
    remote_dns: bool,
) -> Result<(), TcpProxyError> {
    let mut request = Vec::new();
    request.extend_from_slice(&[SOCKS_VERSION, SOCKS_COMMAND_CONNECT, 0x00]);
    let resolved;
    let target_host = if !remote_dns && target_host.parse::<IpAddr>().is_err() {
        resolved = resolve_socket_addrs(target_host, target_port).await?[0]
            .ip()
            .to_string();
        resolved.as_str()
    } else {
        target_host
    };
    append_socks5_target(&mut request, target_host, target_port)?;
    stream.write_all(&request).await.map_err(socks_io_error)?;

    let mut header = [0_u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(socks_io_error)?;
    if header[0] != SOCKS_VERSION || header[2] != 0x00 {
        return proxy_error(UpstreamProxyError::SocksInvalidReply);
    }
    if header[1] != 0x00 {
        return proxy_error(UpstreamProxyError::SocksReplyCode(header[1]));
    }

    drain_socks5_bind_address(stream, header[3]).await
}

pub(super) fn append_socks5_target(
    request: &mut Vec<u8>,
    target_host: &str,
    target_port: u16,
) -> Result<(), TcpProxyError> {
    if let Ok(ip) = target_host.parse::<IpAddr>() {
        append_socks5_ip(request, ip);
    } else {
        let host = target_host.as_bytes();
        if host.len() > u8::MAX as usize {
            return proxy_error(UpstreamProxyError::SocksTargetHostTooLong);
        }
        request.push(SOCKS_ATYP_DOMAIN);
        request.push(host.len() as u8);
        request.extend_from_slice(host);
    }
    request.extend_from_slice(&target_port.to_be_bytes());
    Ok(())
}

pub(super) fn append_socks5_ip(request: &mut Vec<u8>, ip: IpAddr) {
    match ip {
        IpAddr::V4(ip) => {
            request.push(SOCKS_ATYP_IPV4);
            request.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            request.push(SOCKS_ATYP_IPV6);
            request.extend_from_slice(&ip.octets());
        }
    }
}

async fn drain_socks5_bind_address(stream: &mut TcpStream, atyp: u8) -> Result<(), TcpProxyError> {
    let address_len = match atyp {
        SOCKS_ATYP_IPV4 => 4,
        SOCKS_ATYP_DOMAIN => {
            let mut len = [0_u8; 1];
            stream.read_exact(&mut len).await.map_err(socks_io_error)?;
            len[0] as usize
        }
        SOCKS_ATYP_IPV6 => 16,
        _ => return proxy_error(UpstreamProxyError::SocksUnknownAddressType),
    };
    let mut sink = vec![0_u8; address_len + 2];
    stream.read_exact(&mut sink).await.map_err(socks_io_error)?;
    Ok(())
}
