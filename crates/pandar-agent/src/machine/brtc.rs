use std::{sync::Arc, time::Duration};

use anyhow::{Context, anyhow, bail};
use md5::{Digest, Md5};
use rustls::{ClientConfig, pki_types::ServerName};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::machine::{BambuPrinterEndpoint, mqtt::BambuLanCertificateVerifier};

const BRTC_PORT: u16 = 6000;
const BRTC_TIMEOUT: Duration = Duration::from_secs(120);
const BRTC_LOGIN_CLIENT_MAGIC: u32 = 0x0101013f;
const BRTC_LOGIN_SERVER_MAGIC: u32 = 0x0001013f;
const BRTC_CTRL_CLIENT_MAGIC: u32 = 0x0102013f;
const BRTC_CTRL_SETUP_MTYPE: i64 = 12291;
const BRTC_CTRL_JSON_MTYPE: i64 = 12289;
const BRTC_FILE_UPLOAD_CMD: i64 = 5;

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
        let bytes = bytes.to_vec();
        timeout(BRTC_TIMEOUT, async move {
            let mut session = BrtcSession::connect(&self.endpoint).await?;
            session.upload_emmc(&dest_name, &bytes).await
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
    format!("{:x}", Md5::digest(bytes))
}

pub fn md5_upper(bytes: &[u8]) -> String {
    format!("{:X}", Md5::digest(bytes))
}

struct BrtcSession {
    stream: TlsStream<TcpStream>,
    frame_seq: u32,
    wire_seq: u32,
}

impl BrtcSession {
    async fn connect(endpoint: &BambuPrinterEndpoint) -> anyhow::Result<Self> {
        let tcp = TcpStream::connect((endpoint.host.as_str(), BRTC_PORT))
            .await
            .with_context(|| format!("connect Bambu BRTC tunnel to {}:6000", endpoint.host))?;
        let server_name = ServerName::try_from(endpoint.serial.clone())
            .with_context(|| format!("build BRTC TLS server name for {}", endpoint.serial))?;
        let connector = TlsConnector::from(brtc_tls_config());
        let stream = connector
            .connect(server_name, tcp)
            .await
            .with_context(|| format!("start Bambu BRTC TLS session with {}", endpoint.host))?;
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

        let setup = json!({
            "sequence": 0,
            "mtype": BRTC_CTRL_SETUP_MTYPE,
            "req": {
                "t_av": 1,
                "mtype": BRTC_CTRL_JSON_MTYPE,
                "peer_t": 3,
                "pid": format!("pandar-{}", endpoint.serial),
                "ver": "02.08.00.53"
            }
        });
        self.send_abi_json(&setup)
            .await
            .context("send BRTC setup")?;

        loop {
            let value = self
                .read_json_frame()
                .await
                .context("read BRTC setup ack")?;
            if value.get("mtype").and_then(Value::as_i64) == Some(BRTC_CTRL_SETUP_MTYPE)
                && value.get("result").and_then(Value::as_i64) == Some(0)
            {
                return Ok(());
            }
        }
    }

    async fn upload_emmc(&mut self, dest_name: &str, bytes: &[u8]) -> anyhow::Result<String> {
        if dest_name.is_empty() {
            bail!("BRTC upload destination name is empty");
        }

        let sequence = self.next_wire_seq();
        let init = json!({
            "cmdtype": BRTC_FILE_UPLOAD_CMD,
            "sequence": sequence,
            "req": {
                "type": "model",
                "path": dest_name,
                "total": bytes.len(),
                "storage": "emmc"
            }
        });
        self.send_abi_json(&init)
            .await
            .with_context(|| format!("start BRTC upload for emmc/{dest_name}"))?;

        let init_reply = self
            .read_matching_upload_reply(sequence)
            .await
            .context("read BRTC upload init reply")?;
        let result = init_reply
            .get("result")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
        if result != 1 && result != 19 {
            bail!("BRTC upload init failed with result {result}: {init_reply}");
        }
        let reply = init_reply.get("reply").unwrap_or(&Value::Null);
        let chunk_size = reply
            .get("chunk_size")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or_else(|| anyhow!("BRTC upload init reply did not include chunk_size"))?
            as usize
            * 1024;
        let mut offset = reply.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        if offset > bytes.len() {
            bail!(
                "BRTC upload resume offset {offset} exceeds file size {}",
                bytes.len()
            );
        }

        let digest_lower = md5_lower(bytes);
        let mut fragment = 0_u32;
        while offset < bytes.len() {
            let end = (offset + chunk_size).min(bytes.len());
            let chunk = &bytes[offset..end];
            let last = end == bytes.len();
            let chunk_request = brtc_upload_chunk_request(
                sequence,
                fragment,
                offset,
                chunk.len(),
                last.then_some(digest_lower.as_str()),
            );
            self.send_abi_json_with_binary(&chunk_request, chunk)
                .await
                .with_context(|| {
                    format!("send BRTC upload chunk {fragment} for emmc/{dest_name}")
                })?;
            offset = end;
            fragment += 1;
        }

        for _ in 0..32 {
            let value = self
                .read_matching_upload_reply(sequence)
                .await
                .context("read BRTC upload final reply")?;
            match value.get("result").and_then(Value::as_i64).unwrap_or(-1) {
                0 | 19 => return Ok(digest_lower),
                1 => continue,
                result => bail!("BRTC upload failed with result {result}: {value}"),
            }
        }

        bail!("BRTC upload did not return a final result")
    }

    fn next_wire_seq(&mut self) -> u32 {
        let sequence = self.wire_seq;
        self.wire_seq = self.wire_seq.wrapping_add(1);
        sequence
    }

    async fn send_abi_json(&mut self, value: &Value) -> anyhow::Result<()> {
        let body = wrap_ctrl_json(value)?;
        self.send_frame(BRTC_CTRL_CLIENT_MAGIC, body.as_bytes())
            .await
    }

    async fn send_abi_json_with_binary(
        &mut self,
        value: &Value,
        binary: &[u8],
    ) -> anyhow::Result<()> {
        let mut body = wrap_ctrl_json(value)?.into_bytes();
        body.extend_from_slice(b"\n\n");
        body.extend_from_slice(binary);
        self.send_frame(BRTC_CTRL_CLIENT_MAGIC, &body).await
    }

    async fn send_frame(&mut self, magic: u32, payload: &[u8]) -> anyhow::Result<()> {
        let mut header = [0_u8; 16];
        header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        header[4..8].copy_from_slice(&magic.to_le_bytes());
        header[8..12].copy_from_slice(&self.frame_seq.to_le_bytes());
        self.frame_seq = self.frame_seq.wrapping_add(1);
        self.stream.write_all(&header).await?;
        self.stream.write_all(payload).await?;
        Ok(())
    }

    async fn read_matching_upload_reply(&mut self, sequence: u32) -> anyhow::Result<Value> {
        loop {
            let value = self.read_json_frame().await?;
            if value.get("cmdtype").and_then(Value::as_i64) == Some(BRTC_FILE_UPLOAD_CMD)
                && value.get("sequence").and_then(Value::as_u64) == Some(sequence as u64)
            {
                return Ok(value);
            }
        }
    }

    async fn read_json_frame(&mut self) -> anyhow::Result<Value> {
        let frame = self.read_frame().await?;
        let json_len = json_prefix_len(&frame.payload)
            .ok_or_else(|| anyhow!("BRTC frame did not start with JSON"))?;
        serde_json::from_slice(&frame.payload[..json_len]).context("decode BRTC JSON frame")
    }

    async fn read_frame(&mut self) -> anyhow::Result<BrtcFrame> {
        let mut header = [0_u8; 16];
        self.stream.read_exact(&mut header).await?;
        let payload_len = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
        let magic = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let mut payload = vec![0_u8; payload_len];
        if payload_len > 0 {
            self.stream.read_exact(&mut payload).await?;
        }
        Ok(BrtcFrame { magic, payload })
    }
}

struct BrtcFrame {
    magic: u32,
    payload: Vec<u8>,
}

fn brtc_tls_config() -> Arc<ClientConfig> {
    let mut config =
        ClientConfig::builder_with_provider(rustls::crypto::aws_lc_rs::default_provider().into())
            .with_safe_default_protocol_versions()
            .expect("aws-lc-rs provider supports rustls safe default protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(BambuLanCertificateVerifier))
            .with_no_client_auth();
    config.alpn_protocols = Vec::new();
    Arc::new(config)
}

fn padded_ascii(value: &str, width: usize) -> String {
    let mut out = String::with_capacity(width);
    out.extend(value.chars().take(width));
    while out.len() < width {
        out.push('\0');
    }
    out
}

fn wrap_ctrl_json(value: &Value) -> anyhow::Result<String> {
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("BRTC ABI payload must be a JSON object"))?;
    object.insert("mtype".to_owned(), Value::from(BRTC_CTRL_JSON_MTYPE));
    Ok(Value::Object(object).to_string())
}

fn brtc_upload_chunk_request(
    sequence: u32,
    fragment: u32,
    offset: usize,
    size: usize,
    file_md5: Option<&str>,
) -> Value {
    let mut request = serde_json::Map::from_iter([
        ("frag_id".to_owned(), json!(fragment)),
        ("offset".to_owned(), json!(offset)),
        ("size".to_owned(), json!(size)),
    ]);
    if let Some(file_md5) = file_md5 {
        request.insert("file_md5".to_owned(), json!(file_md5));
    }

    json!({
        "cmdtype": BRTC_FILE_UPLOAD_CMD,
        "sequence": sequence,
        "req": request
    })
}

fn json_prefix_len(bytes: &[u8]) -> Option<usize> {
    if bytes.first().copied()? != b'{' {
        return None;
    }
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
#[path = "brtc_test.rs"]
mod brtc_test;
