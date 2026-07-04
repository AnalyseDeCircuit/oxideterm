#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Integration tests for sntrup761 hybrid key exchange.
#![cfg(not(windows))]

use std::borrow::Cow;
use std::sync::Arc;

use russh::keys::PrivateKeyWithHashAlg;
use russh::*;
use ssh_key::PrivateKey;

#[tokio::test]
async fn test_sntrup761x25519_handshake_with_standard_name() {
    run_sntrup_handshake(kex::SNTRUP761X25519_SHA512).await;
}

#[tokio::test]
async fn test_sntrup761x25519_handshake_with_openssh_alias() {
    run_sntrup_handshake(kex::SNTRUP761X25519_SHA512_OPENSSH).await;
}

async fn run_sntrup_handshake(kex_algorithm: kex::Name) {
    let _ = env_logger::try_init();

    let client_key = PrivateKey::random(&mut rand::rng(), ssh_key::Algorithm::Ed25519).unwrap();

    let mut server_config = server::Config::default();
    server_config.inactivity_timeout = None;
    server_config.auth_rejection_time = std::time::Duration::from_secs(3);
    server_config
        .keys
        .push(PrivateKey::random(&mut rand::rng(), ssh_key::Algorithm::Ed25519).unwrap());

    server_config.preferred = {
        let mut p = Preferred::default();
        p.kex = Cow::Owned(vec![kex_algorithm]);
        p
    };

    let server_config = Arc::new(server_config);
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();

    tokio::spawn(async move {
        let (socket, _) = socket.accept().await.unwrap();
        server::run_stream(server_config, socket, TestServer {})
            .await
            .unwrap();
    });

    let mut client_config = client::Config::default();
    client_config.preferred = {
        let mut p = Preferred::default();
        p.kex = Cow::Owned(vec![kex_algorithm]);
        p
    };
    let client_config = Arc::new(client_config);

    let mut session = client::connect(client_config, addr, TestClient {})
        .await
        .unwrap();

    let authenticated = session
        .authenticate_publickey(
            std::env::var("USER").unwrap_or("user".to_owned()),
            PrivateKeyWithHashAlg::new(Arc::new(client_key), None),
        )
        .await
        .unwrap()
        .success();
    assert!(
        authenticated,
        "Authentication should succeed with sntrup KEX"
    );

    let mut channel = session.channel_open_session().await.unwrap();
    channel.data(&b"test data with sntrup"[..]).await.unwrap();

    match channel.wait().await.unwrap() {
        ChannelMsg::Data { data } => assert_eq!(&*data, b"test data with sntrup"),
        msg => panic!("Unexpected message: {msg:?}"),
    }

    channel.eof().await.unwrap();
    session
        .disconnect(Disconnect::ByApplication, "", "")
        .await
        .unwrap();
}

#[derive(Clone)]
struct TestServer {}

impl server::Handler for TestServer {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _public_key: &ssh_key::PublicKey,
    ) -> Result<server::Auth, Self::Error> {
        Ok(server::Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<server::Msg>,
        _session: &mut server::Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, data.to_vec())?;
        Ok(())
    }
}

struct TestClient {}

impl client::Handler for TestClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
