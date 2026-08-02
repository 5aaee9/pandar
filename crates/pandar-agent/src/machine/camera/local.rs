use std::time::Duration;

use anyhow::{Context, bail};
use pandar_core::compatibility::studio_local_camera_supported;
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
};
use tokio_rustls::TlsConnector;

use crate::machine::{BambuPrinterEndpoint, mqtt::bambu_lan_client_config};

const LOCAL_CAMERA_PORT: u16 = 6000;
const LOCAL_CAMERA_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_LOCAL_CAMERA_FRAME_BYTES: usize = 10 * 1024 * 1024;
const CAMERA_CHUNK_BYTES: usize = 32 * 1024;

pub(super) async fn stream_camera_mjpeg(
    endpoint: BambuPrinterEndpoint,
    sender: mpsc::Sender<Vec<u8>>,
) -> anyhow::Result<()> {
    if !studio_local_camera_supported(endpoint.model.as_deref()) {
        bail!(
            "Studio local camera is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }

    let server_name = ServerName::try_from(endpoint.host.clone())
        .map_err(|_| anyhow::anyhow!("invalid Bambu local camera TLS server name"))?;
    let stream = tokio::time::timeout(
        LOCAL_CAMERA_TIMEOUT,
        TcpStream::connect((endpoint.host.as_str(), LOCAL_CAMERA_PORT)),
    )
    .await
    .context("Bambu local camera TCP connection timed out")?
    .with_context(|| {
        format!(
            "connect to Bambu local camera at {}:{LOCAL_CAMERA_PORT}",
            endpoint.host
        )
    })?;
    let mut stream = tokio::time::timeout(
        LOCAL_CAMERA_TIMEOUT,
        TlsConnector::from(bambu_lan_client_config(&endpoint.serial)).connect(server_name, stream),
    )
    .await
    .context("Bambu local camera TLS handshake timed out")?
    .context("complete Bambu local camera TLS handshake")?;
    tokio::time::timeout(
        LOCAL_CAMERA_TIMEOUT,
        stream.write_all(&local_camera_auth_payload(&endpoint.access_code)?),
    )
    .await
    .context("Bambu local camera authentication timed out")?
    .context("send Bambu local camera authentication")?;

    loop {
        let frame =
            tokio::time::timeout(LOCAL_CAMERA_TIMEOUT, read_local_camera_frame(&mut stream))
                .await
                .context("Bambu local camera frame timed out")??;
        let part = super::mjpeg_part(&frame);
        for chunk in part.chunks(CAMERA_CHUNK_BYTES) {
            if sender.send(chunk.to_vec()).await.is_err() {
                return Ok(());
            }
        }
    }
}

fn local_camera_auth_payload(access_code: &str) -> anyhow::Result<[u8; 80]> {
    let access_code = access_code.as_bytes();
    if access_code.len() > 32 {
        bail!("Bambu local camera access code exceeds 32 bytes");
    }
    let mut payload = [0_u8; 80];
    payload[..4].copy_from_slice(&0x40_u32.to_le_bytes());
    payload[4..8].copy_from_slice(&0x3000_u32.to_le_bytes());
    payload[16..20].copy_from_slice(b"bblp");
    payload[48..48 + access_code.len()].copy_from_slice(access_code);
    Ok(payload)
}

async fn read_local_camera_frame(reader: &mut (impl AsyncRead + Unpin)) -> anyhow::Result<Vec<u8>> {
    let mut header = [0_u8; 16];
    reader
        .read_exact(&mut header)
        .await
        .context("read Bambu local camera frame header")?;
    let payload_len = u32::from_le_bytes(header[..4].try_into().expect("four-byte frame length"));
    let payload_len = usize::try_from(payload_len).expect("u32 fits usize on supported targets");
    if payload_len == 0 || payload_len > MAX_LOCAL_CAMERA_FRAME_BYTES {
        bail!("Bambu local camera frame length {payload_len} is invalid");
    }
    let mut frame = vec![0_u8; payload_len];
    reader
        .read_exact(&mut frame)
        .await
        .context("read Bambu local camera frame payload")?;
    if !frame.starts_with(&[0xff, 0xd8]) || !frame.ends_with(&[0xff, 0xd9]) {
        bail!("Bambu local camera frame is not a complete JPEG");
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_exact_local_camera_authentication_payload() {
        let payload = local_camera_auth_payload("secret").unwrap();
        assert_eq!(&payload[..8], &[0x40, 0, 0, 0, 0, 0x30, 0, 0]);
        assert_eq!(&payload[16..20], b"bblp");
        assert_eq!(&payload[48..54], b"secret");
        assert!(payload[54..].iter().all(|byte| *byte == 0));
        assert!(local_camera_auth_payload(&"x".repeat(33)).is_err());
    }

    #[test]
    fn local_camera_gate_is_exact() {
        for model in ["N1", "N2S", "C12", "N9"] {
            assert!(studio_local_camera_supported(Some(model)), "{model}");
        }
        for model in ["C11", "BL-P001", "N6", "O1C2", "unknown"] {
            assert!(!studio_local_camera_supported(Some(model)), "{model}");
        }
    }

    #[tokio::test]
    async fn reads_length_prefixed_local_camera_jpeg() {
        let jpeg = [0xff, 0xd8, 1, 2, 3, 0xff, 0xd9];
        let (mut writer, mut reader) = tokio::io::duplex(128);
        writer
            .write_all(&(jpeg.len() as u32).to_le_bytes())
            .await
            .unwrap();
        writer.write_all(&[0_u8; 12]).await.unwrap();
        writer.write_all(&jpeg).await.unwrap();

        assert_eq!(read_local_camera_frame(&mut reader).await.unwrap(), jpeg);
    }

    #[tokio::test]
    async fn rejects_invalid_local_camera_frames() {
        for payload in [Vec::new(), vec![0_u8; 4]] {
            let (mut writer, mut reader) = tokio::io::duplex(128);
            writer
                .write_all(&(payload.len() as u32).to_le_bytes())
                .await
                .unwrap();
            writer.write_all(&[0_u8; 12]).await.unwrap();
            writer.write_all(&payload).await.unwrap();
            drop(writer);

            assert!(read_local_camera_frame(&mut reader).await.is_err());
        }
    }
}
