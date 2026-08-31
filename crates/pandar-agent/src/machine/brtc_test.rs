use serde::Serialize;

use super::*;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use sha2::{Digest, Sha256};
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

/// Builds the production client config with a verifier that pins the test
/// certificate, mirroring the deployed per-serial pin mechanism.
fn test_brtc_client_config() -> Arc<ClientConfig> {
    let certificate =
        CertificateDer::from_pem_slice(include_bytes!("mqtt/tests/tls/bambu-v1-cert.pem"))
            .expect("test Bambu v1 certificate is valid PEM");
    let fingerprint: [u8; 32] = Sha256::digest(certificate.as_ref()).into();
    brtc_tls_config_with_verifier(Arc::new(
        BambuLanCertificateVerifier::with_trusted_leaf_sha256("test-bambu-v1", fingerprint),
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
    let connector = TlsConnector::from(test_brtc_client_config());
    let file: Vec<u8> = (0..3072_usize).map(|index| (index % 251) as u8).collect();
    let expected_uploaded = file.clone();

    let flow = async {
        let mut session = BrtcSession::connect_on(&endpoint, ("127.0.0.1", BRTC_PORT), connector)
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
