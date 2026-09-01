use serde::Serialize;

use super::*;
use openssl::ssl::{SslContext, SslMethod, SslVerifyMode, SslVersion};
use openssl::x509::X509;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::server::TlsStream;

#[derive(Serialize)]
struct ExpectedUploadChunkRequest<'a> {
    cmdtype: i64,
    sequence: u32,
    req: ExpectedUploadChunkBody<'a>,
}

#[derive(Serialize)]
struct ExpectedUploadChunkBody<'a> {
    frag_id: u32,
    offset: usize,
    size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_md5: Option<&'a str>,
}

#[test]
fn md5_helpers_match_bambu_case_usage() {
    assert_eq!(md5_lower(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(md5_upper(b"abc"), "900150983CD24FB0D6963F7D28E17F72");
}

#[test]
fn json_prefix_stops_before_binary_separator() {
    let body =
        br#"{"result":0}"#.iter().copied().chain(b"\n\nabc".iter().copied()).collect::<Vec<_>>();
    assert_eq!(json_prefix_len(&body), Some(12));
}

#[test]
fn upload_chunk_request_only_includes_md5_for_final_chunk() {
    assert_eq!(
        serde_json::to_value(protocol::upload_chunk_request(7, 0, 0, 1024, None)).unwrap(),
        expected_upload_chunk_request(7, 0, 0, 1024, None)
    );
    assert_eq!(
        serde_json::to_value(protocol::upload_chunk_request(
            7,
            1,
            1024,
            512,
            Some("abc123")
        ))
        .unwrap(),
        expected_upload_chunk_request(7, 1, 1024, 512, Some("abc123"))
    );
}

#[test]
fn frame_payload_length_rejects_values_above_limit() {
    assert_eq!(
        checked_frame_payload_len(frames::BRTC_MAX_FRAME_PAYLOAD_SIZE as u32).unwrap(),
        frames::BRTC_MAX_FRAME_PAYLOAD_SIZE
    );
    let (logs, error) = crate::test_tracing::capture_logs(|| {
        checked_frame_payload_len(frames::BRTC_MAX_FRAME_PAYLOAD_SIZE as u32 + 1).unwrap_err()
    });
    assert!(format!("{error:#}").contains("exceeds limit"));
    let captured = logs.contents();
    assert!(captured.contains("rejecting oversized BRTC frame payload"));
    assert!(captured.contains("payload_len=16777217"));
    assert!(captured.contains("limit=16777216"));
}

#[test]
fn upload_reply_rejects_overflowing_chunk_size() {
    let reply = serde_json::from_value(serde_json::json!({
        "cmdtype": BRTC_FILE_UPLOAD_CMD,
        "sequence": 7,
        "result": 1,
        "reply": {"chunk_size": u64::MAX, "offset": 0}
    }))
    .unwrap();
    let frame = protocol::upload_reply("reply".to_owned(), reply, 7).unwrap();

    let error = frame.chunk_size_bytes().unwrap_err();
    assert!(format!("{error:#}").contains("chunk_size"));
}

#[test]
fn upload_reply_rejects_chunk_size_above_limit() {
    let chunk_size_kib = BRTC_MAX_UPLOAD_CHUNK_SIZE as u64 / 1024 + 1;
    let reply = serde_json::from_value(serde_json::json!({
        "cmdtype": BRTC_FILE_UPLOAD_CMD,
        "sequence": 7,
        "result": 1,
        "reply": {"chunk_size": chunk_size_kib, "offset": 0}
    }))
    .unwrap();
    let frame = protocol::upload_reply("reply".to_owned(), reply, 7).unwrap();

    let (logs, error) = crate::test_tracing::capture_logs(|| frame.chunk_size_bytes().unwrap_err());
    assert!(format!("{error:#}").contains("exceeds limit"));
    let captured = logs.contents();
    assert!(captured.contains("rejecting oversized BRTC upload chunk size"));
    assert!(captured.contains("chunk_size=16778240"));
    assert!(captured.contains("limit=16777216"));
}

#[test]
fn chunk_end_rejects_integer_overflow() {
    let error = checked_chunk_end(usize::MAX, 1, usize::MAX).unwrap_err();
    assert!(format!("{error:#}").contains("offset"));
}

#[test]
fn binary_frame_payload_uses_checked_length_and_delimiter() {
    assert_eq!(
        append_binary_frame_payload(b"{}".to_vec(), b"abc").unwrap(),
        b"{}\n\nabc"
    );

    let error = frames::checked_binary_frame_payload_len(usize::MAX, 1).unwrap_err();
    assert!(format!("{error:#}").contains("overflowed"));
}

/// One machine printer conversation observed by the hermetic :6000 peer.
struct BrtcPeerObservation {
    init: serde_json::Value,
    chunks: Vec<serde_json::Value>,
    uploaded: Vec<u8>,
}

struct PeerFrame {
    magic: u32,
    payload: Vec<u8>,
}

#[derive(Debug)]
struct StaticPeerCertificate(Arc<rustls::sign::CertifiedKey>);

impl rustls::server::ResolvesServerCert for StaticPeerCertificate {
    fn resolve(
        &self,
        _hello: rustls::server::ClientHello<'_>,
    ) -> Option<Arc<rustls::sign::CertifiedKey>> {
        Some(Arc::clone(&self.0))
    }
}

/// Builds the production client verifier with a pin on the test certificate,
/// mirroring the deployed per-serial pin mechanism.
fn test_brtc_verifier() -> Arc<BambuLanCertificateVerifier> {
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("mqtt/tests/tls/bambu-v1-cert.pem"))
            .expect("test Bambu v1 certificate is valid PEM");
    let fingerprint: [u8; 32] = Sha256::digest(certificate.as_ref()).into();
    Arc::new(BambuLanCertificateVerifier::with_trusted_leaf_sha256(
        "test-bambu-v1",
        fingerprint,
    ))
}

async fn spawn_brtc_peer() -> tokio::task::JoinHandle<BrtcPeerObservation> {
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("mqtt/tests/tls/bambu-v1-cert.pem"))
            .expect("test Bambu v1 certificate is valid PEM");
    let private_key =
        PrivateKeyDer::from_pem_slice(include_bytes!("mqtt/tests/tls/bambu-v1-key.pem"))
            .expect("test Bambu v1 key is valid PEM");
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let signing_key = provider
        .key_provider
        .load_private_key(private_key)
        .expect("test Bambu v1 key loads");
    let config = rustls::ServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .expect("rustls safe versions for the BRTC test peer")
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(StaticPeerCertificate(Arc::new(
            rustls::sign::CertifiedKey::new(vec![certificate], signing_key),
        ))));
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind(("127.0.0.1", BRTC_PORT))
        .await
        .expect("bind hermetic Bambu BRTC peer on 127.0.0.1:6000");
    tokio::spawn(async move { serve_brtc_peer(acceptor, listener).await })
}

async fn serve_brtc_peer(acceptor: TlsAcceptor, listener: TcpListener) -> BrtcPeerObservation {
    let (socket, _) = listener.accept().await.expect("peer accepts one session");
    let mut stream = acceptor.accept(socket).await.expect("peer TLS handshake");

    let login = read_peer_frame(&mut stream).await;
    assert_eq!(login.magic, BRTC_LOGIN_CLIENT_MAGIC);
    assert_eq!(
        login.payload,
        format!("{}{}", padded_ascii("bblp", 8), padded_ascii("12345678", 8)).into_bytes()
    );
    send_peer_frame(&mut stream, BRTC_LOGIN_SERVER_MAGIC, &[]).await;

    let setup = read_peer_json(&mut stream).await;
    assert_eq!(setup["mtype"], serde_json::json!(BRTC_CTRL_SETUP_MTYPE));
    send_peer_json(
        &mut stream,
        serde_json::json!({"mtype": BRTC_CTRL_SETUP_MTYPE, "result": 0}),
    )
    .await;

    let init = read_peer_json(&mut stream).await;
    assert_eq!(init["cmdtype"], serde_json::json!(BRTC_FILE_UPLOAD_CMD));
    let sequence = init["sequence"].as_u64().expect("upload init sequence");
    send_peer_json(
        &mut stream,
        serde_json::json!({
            "cmdtype": BRTC_FILE_UPLOAD_CMD,
            "sequence": sequence,
            "result": 1,
            "reply": {"chunk_size": 1, "offset": 0}
        }),
    )
    .await;

    let mut chunks = Vec::new();
    let mut uploaded = Vec::new();
    loop {
        let frame = read_peer_frame(&mut stream).await;
        assert_eq!(frame.magic, BRTC_CTRL_CLIENT_MAGIC);
        let json_len = json_prefix_len(&frame.payload).expect("chunk JSON prefix");
        let chunk: serde_json::Value =
            serde_json::from_slice(&frame.payload[..json_len]).expect("chunk JSON");
        uploaded.extend_from_slice(&frame.payload[json_len + 2..]);
        let is_final = chunk["req"]["file_md5"].is_string();
        chunks.push(chunk);
        if is_final {
            break;
        }
    }

    send_peer_json(
        &mut stream,
        serde_json::json!({
            "cmdtype": BRTC_FILE_UPLOAD_CMD,
            "sequence": sequence,
            "result": 0
        }),
    )
    .await;

    BrtcPeerObservation {
        init,
        chunks,
        uploaded,
    }
}

async fn read_peer_frame(stream: &mut TlsStream<TcpStream>) -> PeerFrame {
    let mut header = [0_u8; 16];
    stream
        .read_exact(&mut header)
        .await
        .expect("peer reads frame header");
    let payload_len = u32::from_le_bytes(header[0..4].try_into().expect("payload length"));
    let magic = u32::from_le_bytes(header[4..8].try_into().expect("frame magic"));
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(payload_len as usize)
        .expect("peer frame payload");
    payload.resize(payload_len as usize, 0);
    if payload_len > 0 {
        stream
            .read_exact(&mut payload)
            .await
            .expect("peer reads frame payload");
    }
    PeerFrame { magic, payload }
}

async fn read_peer_json(stream: &mut TlsStream<TcpStream>) -> serde_json::Value {
    let frame = read_peer_frame(stream).await;
    let json_len = json_prefix_len(&frame.payload).expect("peer frame starts with JSON");
    serde_json::from_slice(&frame.payload[..json_len]).expect("peer frame JSON")
}

async fn send_peer_frame(stream: &mut TlsStream<TcpStream>, magic: u32, payload: &[u8]) {
    let mut header = [0_u8; 16];
    header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[4..8].copy_from_slice(&magic.to_le_bytes());
    header[8..12].copy_from_slice(&1_u32.to_le_bytes());
    stream
        .write_all(&header)
        .await
        .expect("peer writes frame header");
    stream
        .write_all(payload)
        .await
        .expect("peer writes frame payload");
}

async fn send_peer_json(stream: &mut TlsStream<TcpStream>, value: serde_json::Value) {
    let body = serde_json::to_vec(&value).expect("peer reply JSON");
    send_peer_frame(stream, BRTC_CTRL_CLIENT_MAGIC, &body).await;
}

#[tokio::test]
async fn session_exchanges_full_brtc_emmc_upload_with_port_6000_peer() {
    let server = spawn_brtc_peer().await;
    let endpoint = BambuPrinterEndpoint {
        host: "127.0.0.1".to_owned(),
        serial: "test-bambu-v1".to_owned(),
        access_code: "12345678".to_owned(),
        model: Some("H2D".to_owned()),
        name: None,
    };
    let verifier = test_brtc_verifier();
    let file: Vec<u8> = (0..3072_usize).map(|index| (index % 251) as u8).collect();
    let expected_uploaded = file.clone();

    let flow = async {
        let mut session = BrtcSession::connect_on(&endpoint, ("127.0.0.1", BRTC_PORT), &verifier)
            .await
            .expect("BRTC session connects to the :6000 peer");
        let digest = session
            .upload_emmc("Metadata/plate.gcode.3mf", &file)
            .await
            .expect("BRTC eMMC upload succeeds");
        assert_eq!(digest, md5_lower(&expected_uploaded));
        drop(session);
        server.await.expect("peer conversation completes")
    };
    let observation = timeout(Duration::from_secs(20), flow)
        .await
        .expect("hermetic peer conversation finishes inside 20 seconds");

    assert_eq!(observation.uploaded, expected_uploaded);
    assert_eq!(observation.init["req"]["type"], "model");
    assert_eq!(observation.init["req"]["storage"], "emmc");
    assert_eq!(observation.init["req"]["path"], "Metadata/plate.gcode.3mf");
    assert_eq!(observation.init["req"]["total"], expected_uploaded.len());
    assert_eq!(observation.chunks.len(), 3);
    for (fragment, chunk) in observation.chunks.iter().enumerate() {
        assert_eq!(chunk["cmdtype"], BRTC_FILE_UPLOAD_CMD);
        assert_eq!(chunk["req"]["frag_id"], fragment as u8);
        assert_eq!(chunk["req"]["offset"], serde_json::json!(fragment * 1024));
        assert_eq!(chunk["req"]["size"], 1024);
        if fragment == observation.chunks.len() - 1 {
            assert_eq!(
                chunk["req"]["file_md5"],
                serde_json::json!(md5_lower(&expected_uploaded))
            );
        } else {
            assert!(chunk["req"].get("file_md5").is_none());
        }
    }
}

fn expected_upload_chunk_request(
    sequence: u32,
    fragment: u32,
    offset: usize,
    size: usize,
    file_md5: Option<&str>,
) -> serde_json::Value {
    serde_json::to_value(ExpectedUploadChunkRequest {
        cmdtype: BRTC_FILE_UPLOAD_CMD,
        sequence,
        req: ExpectedUploadChunkBody {
            frag_id: fragment,
            offset,
            size,
            file_md5,
        },
    })
    .unwrap()
}

/// Blocking reader for the static-RSA test peer; mirrors `read_peer_frame`
/// over OpenSSL's synchronous stream.
fn read_rsa_peer_frame(
    stream: &mut openssl::ssl::SslStream<std::net::TcpStream>,
) -> std::io::Result<PeerFrame> {
    use std::io::Read as _;
    let mut header = [0_u8; 16];
    stream.read_exact(&mut header)?;
    let payload_len = u32::from_le_bytes(header[0..4].try_into().expect("payload length"));
    let magic = u32::from_le_bytes(header[4..8].try_into().expect("frame magic"));
    let mut payload = vec![0_u8; payload_len as usize];
    if payload_len > 0 {
        stream.read_exact(&mut payload)?;
    }
    Ok(PeerFrame { magic, payload })
}

fn send_rsa_peer_frame(
    stream: &mut openssl::ssl::SslStream<std::net::TcpStream>,
    magic: u32,
    payload: &[u8],
) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut header = [0_u8; 16];
    header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[4..8].copy_from_slice(&magic.to_le_bytes());
    header[8..12].copy_from_slice(&1_u32.to_le_bytes());
    stream.write_all(&header)?;
    stream.write_all(payload)?;
    Ok(())
}

fn send_rsa_peer_json(
    stream: &mut openssl::ssl::SslStream<std::net::TcpStream>,
    value: serde_json::Value,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(&value).expect("peer reply JSON");
    send_rsa_peer_frame(stream, BRTC_CTRL_CLIENT_MAGIC, &body)
}

/// An OpenSSL :6000 peer restricted to the exact profile real printers use:
/// TLS 1.2 only, with the static-RSA `AES256-GCM-SHA384` suite as the only
/// offer. rustls clients cannot negotiate this profile at all.
fn spawn_static_rsa_peer(
    cert_der: &[u8],
    key_pem: &[u8],
) -> (std::sync::mpsc::Receiver<u16>, std::thread::JoinHandle<()>) {
    let (bound_sender, bound_receiver) = std::sync::mpsc::channel();
    let cert_owned = cert_der.to_vec();
    let key_owned = key_pem.to_vec();
    let handle = std::thread::spawn(move || {
        let certificate = X509::from_pem(&cert_owned).expect("peer RSA suite loads test leaf");
        let private_key = openssl::pkey::PKey::private_key_from_pem(&key_owned)
            .expect("peer RSA suite loads test key");
        let mut builder = SslContext::builder(SslMethod::tls()).expect("peer SSL context");
        builder
            .set_certificate(&certificate)
            .expect("peer RSA suite sets test leaf");
        builder
            .set_private_key(&private_key)
            .expect("peer RSA suite sets test key");
        builder
            .set_min_proto_version(Some(SslVersion::TLS1_2))
            .expect("peer minimum version");
        builder
            .set_max_proto_version(Some(SslVersion::TLS1_2))
            .expect("peer maximum version");
        builder
            .set_cipher_list(BRTC_STATIC_RSA_CIPHER_LIST)
            .expect("static RSA cipher list");
        builder.set_verify(SslVerifyMode::NONE);
        let context = builder.build();

        let listener = std::net::TcpListener::bind(("127.0.0.1", BRTC_STATIC_RSA_PORT))
            .expect("bind static-RSA test peer");
        let bound_addr = listener
            .local_addr()
            .expect("static-RSA test peer local address");
        bound_sender
            .send(bound_addr.port())
            .expect("announce static-RSA peer port");
        let (socket, _) = listener
            .accept()
            .expect("static-RSA peer accepts one session");
        let ssl = openssl::ssl::Ssl::new(&context).expect("peer SSL handle");
        let mut session = openssl::ssl::SslStream::new(ssl, socket)
            .expect("peer ssl stream over accepted socket");
        openssl::ssl::SslStream::accept(&mut session).expect("peer TLS handshake");
        assert_eq!(
            session.ssl().version_str(),
            "TLSv1.2",
            "machine profile is TLS 1.2 only"
        );

        let login = read_rsa_peer_frame(&mut session).expect("peer reads login");
        assert_eq!(login.magic, BRTC_LOGIN_CLIENT_MAGIC);
        assert_eq!(
            login.payload,
            format!("{}{}", padded_ascii("bblp", 8), padded_ascii("12345678", 8)).into_bytes()
        );
        let mut header = [0_u8; 16];
        header[0..4].copy_from_slice(&0_u32.to_le_bytes());
        header[4..8].copy_from_slice(&BRTC_LOGIN_SERVER_MAGIC.to_le_bytes());
        header[8..12].copy_from_slice(&1_u32.to_le_bytes());
        std::io::Write::write_all(&mut session, &header).expect("peer writes login ack");
        std::io::Write::flush(&mut session).expect("peer flushes");

        let setup = read_rsa_peer_frame(&mut session).expect("peer reads setup");
        assert_eq!(setup.magic, BRTC_CTRL_CLIENT_MAGIC);
        send_rsa_peer_json(
            &mut session,
            serde_json::json!({"mtype": BRTC_CTRL_SETUP_MTYPE, "result": 0}),
        )
        .expect("peer writes setup ack");

        // The client drops the session after the BRTC handshake; read until EOF.
        let _ = read_rsa_peer_frame(&mut session);
    });
    (bound_receiver, handle)
}

const BRTC_STATIC_RSA_PORT: u16 = 6001;
const BRTC_STATIC_RSA_CIPHER_LIST: &str = "AES256-GCM-SHA384";

#[tokio::test]
async fn session_negotiates_static_rsa_tls12_with_machine_profile() {
    let cert_der = include_bytes!("mqtt/tests/tls/bambu-v1-cert.pem") as &[u8];
    let key_pem = include_bytes!("mqtt/tests/tls/bambu-v1-key.pem") as &[u8];
    let (bound_receiver, peer) = spawn_static_rsa_peer(cert_der, key_pem);
    let port = bound_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("static-RSA peer binds before the client connects");
    let endpoint = BambuPrinterEndpoint {
        host: "127.0.0.1".to_owned(),
        serial: "test-bambu-v1".to_owned(),
        access_code: "12345678".to_owned(),
        model: Some("H2D".to_owned()),
        name: None,
    };
    let verifier = test_brtc_verifier();
    timeout(
        Duration::from_secs(20),
        BrtcSession::connect_on(&endpoint, ("127.0.0.1", port), &verifier),
    )
    .await
    .expect("static-RSA machine profile connects inside 20 seconds")
    .expect("client negotiates the static-RSA TLS 1.2 machine profile");
    peer.join().expect("static-RSA peer conversation completes");
}
