use futures_util::StreamExt;
use rcgen::{CertifiedKey, generate_simple_self_signed};
use rustls::{
    ClientConfig, RootCertStore, ServerConfig,
    pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
};
use tokio::{io::AsyncReadExt, net::TcpStream};
use tokio_rustls::TlsConnector;
use tonic::transport::server::Connected;

use super::*;

#[tokio::test]
async fn incoming_waits_for_a_connection_permit_before_accepting_more() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = incoming(listener, 1, 64, None);
    tokio::pin!(incoming);

    let first_client = TcpStream::connect(address).await.unwrap();
    let first = incoming.next().await.unwrap().unwrap();
    let second_client = TcpStream::connect(address).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), incoming.next())
            .await
            .is_err()
    );

    drop(first);
    let second = tokio::time::timeout(Duration::from_secs(1), incoming.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop((first_client, second_client, second));
}

#[tokio::test]
async fn incoming_rejects_connections_above_per_peer_limit_until_authentication() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = incoming_with_timeout(listener, 4, None, Duration::from_secs(1), 1);
    tokio::pin!(incoming);

    let first_client = TcpStream::connect(address).await.unwrap();
    let first = incoming.next().await.unwrap().unwrap();
    let second_client = TcpStream::connect(address).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), incoming.next())
            .await
            .is_err()
    );

    assert!(first.connect_info().mark_authenticated(
        pandar_core::TenantId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
        pandar_core::AgentId::parse("00000000-0000-0000-0000-000000000002").unwrap(),
    ));
    let third_client = TcpStream::connect(address).await.unwrap();
    let third = tokio::time::timeout(Duration::from_secs(1), incoming.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop((first_client, second_client, third_client, third));
}

#[tokio::test]
async fn silent_connection_hits_http2_preface_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = incoming_with_timeout(listener, 1, None, Duration::from_millis(50), 64);
    tokio::pin!(incoming);

    let _client = TcpStream::connect(address).await.unwrap();
    let mut connection = incoming.next().await.unwrap().unwrap();
    let error = connection.read_u8().await.unwrap_err();

    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
}

#[tokio::test]
async fn tls_incoming_negotiates_http2_alpn() {
    let CertifiedKey { cert, signing_key } =
        generate_simple_self_signed(["localhost".to_owned()]).unwrap();
    let certificate = cert.der().clone();
    let private_key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
    let mut server_config =
        ServerConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![certificate.clone()], private_key)
            .unwrap();
    server_config.alpn_protocols = vec![b"h2".to_vec()];

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let incoming = incoming(listener, 4, 4, Some(Arc::new(server_config)));
    tokio::pin!(incoming);

    let mut roots = RootCertStore::empty();
    roots.add(certificate).unwrap();
    let mut client_config =
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    client_config.alpn_protocols = vec![b"h2".to_vec()];
    let client = tokio::spawn(async move {
        TlsConnector::from(Arc::new(client_config))
            .connect(
                ServerName::try_from("localhost").unwrap(),
                TcpStream::connect(address).await.unwrap(),
            )
            .await
            .unwrap()
    });

    let connection = incoming.next().await.unwrap().unwrap();
    let client = client.await.unwrap();
    assert_eq!(client.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
    let ConnectionIo::Tls(server) = &connection.io else {
        panic!("expected TLS gRPC connection");
    };
    assert_eq!(
        server.inner.get_ref().1.alpn_protocol(),
        Some(b"h2".as_slice())
    );
}
