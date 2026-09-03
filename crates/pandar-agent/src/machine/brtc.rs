use std::{pin::Pin, time::Duration};

use anyhow::{Context, anyhow, bail};
use md5::{Digest, Md5};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode, SslVersion};
use rustls::{
    client::danger::ServerCertVerifier,
    pki_types::{CertificateDer, ServerName as PkiServerName, UnixTime},
};
use serde::{Serialize, de::DeserializeOwned};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use tokio_openssl::SslStream;

use crate::machine::{BambuPrinterEndpoint, mqtt::BambuLanCertificateVerifier};

mod frames;
mod protocol;
use frames::{
    BrtcFrame, append_binary_frame_payload, checked_chunk_end, checked_frame_payload_len,
    json_prefix_len, padded_ascii,
};

const BRTC_PORT: u16 = 6000;
const BRTC_TIMEOUT: Duration = Duration::from_secs(120);
const BRTC_LOGIN_CLIENT_MAGIC: u32 = 0x0101013f;
const BRTC_LOGIN_SERVER_MAGIC: u32 = 0x0001013f;
const BRTC_CTRL_CLIENT_MAGIC: u32 = 0x0102013f;
const BRTC_CTRL_SETUP_MTYPE: i64 = 12291;
const BRTC_CTRL_JSON_MTYPE: i64 = 12289;
const BRTC_FILE_UPLOAD_CMD: i64 = 5;
const BRTC_MAX_UPLOAD_CHUNK_SIZE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BrtcMachineFileTransfer {
    endpoint: BambuPrinterEndpoint,
}

impl BrtcMachineFileTransfer {
    pub fn new(endpoint: BambuPrinterEndpoint) -> Self {
        Self { endpoint }
    }

    pub async fn upload_emmc(&self, dest_name: &str, bytes: &[u8]) -> anyhow::Result<String> {
        let dest_name = dest_name.to_owned();
        timeout(BRTC_TIMEOUT, async move {
            let mut session = BrtcSession::connect(&self.endpoint).await?;
            session.upload_emmc(&dest_name, bytes).await
        })
        .await
        .with_context(|| {
            format!(
                "Bambu BRTC file upload timed out for {}",
                self.endpoint.host
            )
        })?
    }
}

pub fn md5_lower(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn md5_upper(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

struct BrtcSession {
    stream: SslStream<TcpStream>,
    frame_seq: u32,
    wire_seq: u32,
}

impl BrtcSession {
    async fn connect(endpoint: &BambuPrinterEndpoint) -> anyhow::Result<Self> {
        let verifier = BambuLanCertificateVerifier::new(&endpoint.serial);
        Self::connect_on(endpoint, (endpoint.host.as_str(), BRTC_PORT), &verifier).await
    }

    async fn connect_on(
        endpoint: &BambuPrinterEndpoint,
        address: (&str, u16),
        verifier: &BambuLanCertificateVerifier,
    ) -> anyhow::Result<Self> {
        let tcp = TcpStream::connect(address)
            .await
            .with_context(|| format!("connect Bambu BRTC tunnel to {}", endpoint.host))?;
        let connector = brtc_ssl_connector()?;
        let config = connector
            .configure()
            .context("configure Bambu BRTC TLS session")?;
        // Bambu tunnel certificates carry the printer serial as the common
        // name, so the SNI name is the serial (same choice as the rustls
        // client this replaces).
        let ssl = config
            .into_ssl(endpoint.serial.as_str())
            .context("build Bambu BRTC TLS session")?;
        let mut stream = SslStream::new(ssl, tcp)?;
        Pin::new(&mut stream)
            .connect()
            .await
            .with_context(|| format!("start Bambu BRTC TLS session with {}", endpoint.host))?;
        verify_brtc_peer_certificate(&stream, verifier, endpoint)?;
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos())
            .unwrap_or(1);
        let mut session = Self {
            stream,
            frame_seq: seed,
            wire_seq: 1,
        };
        session.handshake(endpoint).await?;
        Ok(session)
    }

    async fn handshake(&mut self, endpoint: &BambuPrinterEndpoint) -> anyhow::Result<()> {
        let login = padded_ascii("bblp", 8) + &padded_ascii(&endpoint.access_code, 8);
        self.send_frame(BRTC_LOGIN_CLIENT_MAGIC, login.as_bytes())
            .await
            .context("send BRTC login frame")?;

        let login_ack = self.read_frame().await.context("read BRTC login ack")?;
        if login_ack.magic != BRTC_LOGIN_SERVER_MAGIC {
            bail!("unexpected BRTC login ack magic 0x{:08x}", login_ack.magic);
        }

        let setup = protocol::setup_request(&endpoint.serial);
        self.send_abi_json(&setup)
            .await
            .context("send BRTC setup")?;

        loop {
            let ack = self
                .read_json_frame::<protocol::BrtcSetupAck>()
                .await
                .context("read BRTC setup ack")?;
            if protocol::setup_ack_success(ack) {
                return Ok(());
            }
        }
    }

    async fn upload_emmc(&mut self, dest_name: &str, bytes: &[u8]) -> anyhow::Result<String> {
        if dest_name.is_empty() {
            bail!("BRTC upload destination name is empty");
        }

        let sequence = self.next_wire_seq();
        let init = protocol::upload_init_request(sequence, dest_name, bytes.len());
        self.send_abi_json(&init)
            .await
            .with_context(|| format!("start BRTC upload for emmc/{dest_name}"))?;

        let init_reply = self
            .read_matching_upload_reply(sequence)
            .await
            .context("read BRTC upload init reply")?;
        let result = init_reply.result();
        if result != 1 && result != 19 {
            bail!(
                "BRTC upload init failed with result {result}: {}",
                init_reply.raw()
            );
        }
        let chunk_size = init_reply.chunk_size_bytes()?;
        let mut offset = init_reply.offset()?;
        if offset > bytes.len() {
            bail!(
                "BRTC upload resume offset {offset} exceeds file size {}",
                bytes.len()
            );
        }

        let digest_lower = md5_lower(bytes);
        let mut fragment = 0_u32;
        let mut frame_body = Vec::new();
        while offset < bytes.len() {
            let end = checked_chunk_end(offset, chunk_size, bytes.len())?;
            let chunk = &bytes[offset..end];
            let last = end == bytes.len();
            let chunk_request = protocol::upload_chunk_request(
                sequence,
                fragment,
                offset,
                chunk.len(),
                last.then_some(digest_lower.as_str()),
            );
            self.send_abi_json_with_binary(&chunk_request, chunk, &mut frame_body)
                .await
                .with_context(|| {
                    format!("send BRTC upload chunk {fragment} for emmc/{dest_name}")
                })?;
            offset = end;
            if !last {
                fragment = fragment
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("BRTC upload fragment id overflow"))?;
            }
        }

        for _ in 0..32 {
            let value = self
                .read_matching_upload_reply(sequence)
                .await
                .context("read BRTC upload final reply")?;
            match value.result() {
                0 | 19 => return Ok(digest_lower),
                1 => continue,
                result => bail!("BRTC upload failed with result {result}: {}", value.raw()),
            }
        }

        bail!("BRTC upload did not return a final result")
    }

    fn next_wire_seq(&mut self) -> u32 {
        let sequence = self.wire_seq;
        self.wire_seq = self.wire_seq.wrapping_add(1);
        sequence
    }

    async fn send_abi_json(&mut self, value: &impl Serialize) -> anyhow::Result<()> {
        let body = protocol::wrap_ctrl_json(value)?;
        self.send_frame(BRTC_CTRL_CLIENT_MAGIC, body.as_bytes())
            .await
    }

    async fn send_abi_json_with_binary(
        &mut self,
        value: &impl Serialize,
        binary: &[u8],
        body: &mut Vec<u8>,
    ) -> anyhow::Result<()> {
        let json = protocol::wrap_ctrl_json(value)?;
        body.clear();
        body.extend_from_slice(json.as_bytes());
        append_binary_frame_payload(body, binary)?;
        self.send_frame(BRTC_CTRL_CLIENT_MAGIC, body).await
    }

    async fn send_frame(&mut self, magic: u32, payload: &[u8]) -> anyhow::Result<()> {
        let mut header = [0_u8; 16];
        let payload_len = u32::try_from(payload.len()).context("BRTC frame payload exceeds u32")?;
        header[0..4].copy_from_slice(&payload_len.to_le_bytes());
        header[4..8].copy_from_slice(&magic.to_le_bytes());
        header[8..12].copy_from_slice(&self.frame_seq.to_le_bytes());
        self.frame_seq = self.frame_seq.wrapping_add(1);
        self.stream.write_all(&header).await?;
        self.stream.write_all(payload).await?;
        Ok(())
    }

    async fn read_matching_upload_reply(
        &mut self,
        sequence: u32,
    ) -> anyhow::Result<protocol::BrtcUploadReplyFrame> {
        loop {
            let (reply, raw) = self.read_json_frame_with_raw().await?;
            if let Some(reply) = protocol::upload_reply(raw, reply, sequence) {
                return Ok(reply);
            }
        }
    }

    async fn read_json_frame<T>(&mut self) -> anyhow::Result<T>
    where
        T: DeserializeOwned,
    {
        let (value, _) = self.read_json_frame_with_raw().await?;
        Ok(value)
    }

    async fn read_json_frame_with_raw<T>(&mut self) -> anyhow::Result<(T, String)>
    where
        T: DeserializeOwned,
    {
        let frame = self.read_frame().await?;
        let json_len = json_prefix_len(&frame.payload)
            .ok_or_else(|| anyhow!("BRTC frame did not start with JSON"))?;
        let raw = std::str::from_utf8(&frame.payload[..json_len])
            .context("decode BRTC JSON frame text")?
            .to_owned();
        let value = serde_json::from_str(&raw).context("decode BRTC JSON frame")?;
        Ok((value, raw))
    }

    async fn read_frame(&mut self) -> anyhow::Result<BrtcFrame> {
        let mut header = [0_u8; 16];
        self.stream.read_exact(&mut header).await?;
        let payload_len =
            checked_frame_payload_len(u32::from_le_bytes(header[0..4].try_into().unwrap()))?;
        let magic = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(payload_len)
            .with_context(|| format!("reserve {payload_len} bytes for BRTC frame payload"))?;
        payload.resize(payload_len, 0);
        if payload_len > 0 {
            self.stream.read_exact(&mut payload).await?;
        }
        Ok(BrtcFrame { magic, payload })
    }
}

fn brtc_ssl_connector() -> anyhow::Result<SslConnector> {
    let mut builder = SslConnector::builder(SslMethod::tls())?;
    // Pandar protected-transport floor: printers negotiate TLS 1.2 on this
    // tunnel today and newer stacks can pick TLS 1.3.
    builder
        .set_min_proto_version(Some(SslVersion::TLS1_2))
        .context("set Bambu BRTC TLS minimum version")?;
    // The default OpenSSL cipher list includes the static-RSA TLS 1.2 key
    // exchange the printer's :6000 stack requires; rustls cannot offer it.
    // Certificate trust is enforced after the handshake through the shared
    // Bambu certificate verifier (same rules as the MQTT and FTPS tunnels).
    builder.set_verify(SslVerifyMode::NONE);
    Ok(builder.build())
}

/// Verifies the :6000 tunnel certificate with the same rules as every other
/// Bambu transport: leaf CN equals the printer serial, then the pinned-leaf,
/// legacy-V1, or CA-chain policy of `BambuLanCertificateVerifier`.
fn verify_brtc_peer_certificate(
    stream: &SslStream<TcpStream>,
    verifier: &BambuLanCertificateVerifier,
    endpoint: &BambuPrinterEndpoint,
) -> anyhow::Result<()> {
    let leaf = stream
        .ssl()
        .peer_certificate()
        .with_context(|| {
            format!(
                "Bambu BRTC tunnel at {} presented no TLS certificate",
                endpoint.host
            )
        })?
        .to_der()
        .context("encode Bambu BRTC tunnel leaf certificate")?;
    let mut intermediates = Vec::new();
    if let Some(chain) = stream.ssl().peer_cert_chain() {
        for certificate in chain {
            let der = certificate
                .to_der()
                .context("encode Bambu BRTC tunnel chain certificate")?;
            if der != leaf {
                intermediates.push(CertificateDer::from(der));
            }
        }
    }
    let server_name = PkiServerName::try_from(endpoint.serial.clone())
        .context("build Bambu BRTC tunnel verification name")?;
    verifier
        .verify_server_cert(
            &CertificateDer::from(leaf),
            &intermediates,
            &server_name,
            &[],
            UnixTime::now(),
        )
        .map_err(|error| {
            anyhow!(
                "verify Bambu BRTC tunnel TLS certificate for {}: {error}",
                endpoint.host
            )
        })?;
    Ok(())
}

#[cfg(test)]
#[path = "brtc_test.rs"]
mod brtc_test;
