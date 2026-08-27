use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use rustls::{
    ClientConfig, HandshakeKind, ServerConfig,
    crypto::aws_lc_rs::Ticketer,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    server::{ClientHello, ResolvesServerCert, ServerSessionMemoryCache, StoresServerSessions},
    sign::CertifiedKey,
    version,
};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use super::super::{FtpsProfile, ftps_tls_config};
use crate::machine::mqtt::BambuLanCertificateVerifier;

#[derive(Debug)]
pub(super) struct FtpsServerObservation {
    pub(super) commands: Vec<String>,
    pub(super) data_session_reused: bool,
    pub(super) control_session_id_reused: bool,
    pub(super) uploaded: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DataConnectionPolicy {
    RequireControlSessionId,
    RequireAnySessionReuse,
    Reject,
}

pub(super) fn test_ftps_client_config(
    certificate_pem: &[u8],
    profile: FtpsProfile,
) -> Arc<ClientConfig> {
    let certificate = CertificateDer::from_pem_slice(certificate_pem).unwrap();
    let fingerprint: [u8; 32] = Sha256::digest(certificate.as_ref()).into();
    ftps_tls_config(
        profile,
        Arc::new(BambuLanCertificateVerifier::with_trusted_leaf_sha256(
            "test-bambu-v1",
            fingerprint,
        )),
    )
}

#[derive(Debug)]
struct TrackingSessionStore {
    inner: Arc<ServerSessionMemoryCache>,
    successful_reads: AtomicUsize,
}

impl TrackingSessionStore {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: ServerSessionMemoryCache::new(16),
            successful_reads: AtomicUsize::new(0),
        })
    }

    fn reused(&self) -> bool {
        self.successful_reads.load(Ordering::Acquire) > 0
    }
}

impl StoresServerSessions for TrackingSessionStore {
    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> bool {
        self.inner.put(key, value)
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let value = self.inner.get(key);
        if value.is_some() {
            self.successful_reads.fetch_add(1, Ordering::AcqRel);
        }
        value
    }

    fn take(&self, key: &[u8]) -> Option<Vec<u8>> {
        let value = self.inner.take(key);
        if value.is_some() {
            self.successful_reads.fetch_add(1, Ordering::AcqRel);
        }
        value
    }

    fn can_cache(&self) -> bool {
        self.inner.can_cache()
    }
}

pub(super) async fn spawn_session_reuse_ftps_server(
    policy: DataConnectionPolicy,
) -> (
    std::net::SocketAddr,
    tokio::task::JoinHandle<FtpsServerObservation>,
) {
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("../../mqtt/tests/tls/bambu-v1-cert.pem"))
            .unwrap();
    let private_key =
        PrivateKeyDer::from_pem_slice(include_bytes!("../../mqtt/tests/tls/bambu-v1-key.pem"))
            .unwrap();
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let signing_key = provider.key_provider.load_private_key(private_key).unwrap();
    let certified_key = CertifiedKey::new(vec![certificate], signing_key);
    let session_storage = TrackingSessionStore::new();
    let mut server_config = ServerConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[&version::TLS12])
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(TestCertificateResolver(Arc::new(certified_key))));
    server_config.session_storage = session_storage.clone();
    server_config.ticketer = Ticketer::new().unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (control, _) = listener.accept().await.unwrap();
        let control = acceptor.accept(control).await.unwrap();
        let mut control = BufReader::new(control);
        reply(&mut control, "220 X2D FTPS ready\r\n").await;
        let mut commands = Vec::new();
        let mut passive = None;

        loop {
            let mut line = String::new();
            assert_ne!(control.read_line(&mut line).await.unwrap(), 0);
            let line = line.trim_end();
            let (command, _) = line.split_once(' ').unwrap_or((line, ""));
            commands.push(line.to_owned());
            match command {
                "USER" => reply(&mut control, "331 Password required\r\n").await,
                "PASS" => reply(&mut control, "230 Login successful\r\n").await,
                "PBSZ" | "PROT" | "TYPE" => reply(&mut control, "200 Command okay\r\n").await,
                "PASV" => {
                    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let port = listener.local_addr().unwrap().port();
                    reply(
                        &mut control,
                        &format!(
                            "227 Entering Passive Mode (127,0,0,1,{},{})\r\n",
                            port / 256,
                            port % 256
                        ),
                    )
                    .await;
                    passive = Some(listener);
                }
                "STOR" => {
                    let (data, _) = passive.take().unwrap().accept().await.unwrap();
                    let mut data = acceptor.accept(data).await.unwrap();
                    let data_session_reused =
                        data.get_ref().1.handshake_kind() == Some(HandshakeKind::Resumed);
                    let control_session_id_reused = session_storage.reused();
                    let policy_rejected = match policy {
                        DataConnectionPolicy::RequireControlSessionId => !control_session_id_reused,
                        DataConnectionPolicy::RequireAnySessionReuse => false,
                        DataConnectionPolicy::Reject => true,
                    };
                    if !data_session_reused || policy_rejected {
                        reply(
                            &mut control,
                            "522 SSL connection failed: session reuse required\r\n",
                        )
                        .await;
                        return FtpsServerObservation {
                            commands,
                            data_session_reused,
                            control_session_id_reused,
                            uploaded: Vec::new(),
                        };
                    }
                    reply(&mut control, "150 Opening data connection\r\n").await;
                    let mut uploaded = Vec::new();
                    data.read_to_end(&mut uploaded).await.unwrap();
                    reply(&mut control, "226 Transfer complete\r\n").await;
                    passive = None;
                    if uploaded != b"abc" {
                        return FtpsServerObservation {
                            commands,
                            data_session_reused,
                            control_session_id_reused,
                            uploaded,
                        };
                    }
                }
                "SIZE" => {
                    reply(&mut control, "213 3\r\n").await;
                    return FtpsServerObservation {
                        commands,
                        data_session_reused: true,
                        control_session_id_reused: session_storage.reused(),
                        uploaded: b"abc".to_vec(),
                    };
                }
                other => panic!("unexpected FTP command {other}: {line:?}"),
            }
        }
    });
    (address, server)
}

#[derive(Debug)]
struct TestCertificateResolver(Arc<CertifiedKey>);

impl ResolvesServerCert for TestCertificateResolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

async fn reply(control: &mut BufReader<TlsStream<TcpStream>>, message: &str) {
    control
        .get_mut()
        .write_all(message.as_bytes())
        .await
        .unwrap();
    control.get_mut().flush().await.unwrap();
}
