use std::{
    ffi::c_void,
    net::TcpListener,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use anyhow::{Context, bail};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::{
    PluginHttpResult,
    connection::StudioRequestSnapshot,
    http::{hub_client, send_hub_request},
    invalid_input, read_utf8, result, stable_error_body,
};

const RELAY_ACCEPT_TIMEOUT: Duration = Duration::from_secs(15);
const RELAY_FRAME_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RELAY_FRAME_BYTES: usize = 10 * 1024 * 1024;
const RELAY_AUTH_BYTES: usize = 32;
const MAX_ACTIVE_RELAYS: usize = 16;

static ACTIVE_RELAYS: AtomicUsize = AtomicUsize::new(0);

struct ActiveRelay;

impl ActiveRelay {
    fn acquire() -> anyhow::Result<Self> {
        ACTIVE_RELAYS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                (active < MAX_ACTIVE_RELAYS).then_some(active + 1)
            })
            .map_err(|_| anyhow::anyhow!("Studio camera relay capacity exhausted"))?;
        Ok(Self)
    }
}

impl Drop for ActiveRelay {
    fn drop(&mut self) {
        ACTIVE_RELAYS.fetch_sub(1, Ordering::AcqRel);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pandar_plugin_camera_url(
    session_ptr: *mut c_void,
    dev_id_ptr: *const u8,
    dev_id_len: usize,
) -> PluginHttpResult {
    let Some(session) = crate::connection::ffi::session(session_ptr) else {
        return result(-1, 0, stable_error_body("invalid_handle"));
    };
    let Some(dev_id) = read_utf8(dev_id_ptr, dev_id_len) else {
        return invalid_input("camera_unavailable");
    };
    let Some(snapshot) = session.studio_camera_snapshot(dev_id) else {
        return result(-19, 0, stable_error_body("camera_unavailable"));
    };
    match start_relay(snapshot) {
        Ok(url) => result(0, 200, url),
        Err(error) => {
            eprintln!("pandar Studio local camera relay failed to start: {error:#}");
            result(-2, 0, stable_error_body("camera_unavailable"))
        }
    }
}

fn start_relay(snapshot: StudioRequestSnapshot) -> anyhow::Result<String> {
    let active_relay = ActiveRelay::acquire()?;
    let listener = TcpListener::bind("127.0.0.1:0").context("bind Studio camera relay")?;
    listener
        .set_nonblocking(true)
        .context("configure Studio camera relay listener")?;
    let port = listener
        .local_addr()
        .context("read Studio camera relay address")?
        .port();
    let auth = Uuid::new_v4().simple().to_string();
    let relay_auth = auth.clone();
    std::thread::Builder::new()
        .name("pandar-studio-camera".to_owned())
        .spawn(move || {
            let _active_relay = active_relay;
            let relay = crate::runtime().block_on(relay_camera(listener, relay_auth, snapshot));
            if let Err(error) = relay {
                eprintln!("pandar Studio local camera relay ended: {error:#}");
            }
        })
        .context("spawn Studio camera relay")?;
    Ok(format!("bambu:///local/127.0.0.1?port={port}&auth={auth}"))
}

async fn relay_camera(
    listener: TcpListener,
    auth: String,
    snapshot: StudioRequestSnapshot,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::from_std(listener)
        .context("adopt Studio camera relay listener")?;
    let (mut source, _) = tokio::time::timeout(RELAY_ACCEPT_TIMEOUT, listener.accept())
        .await
        .context("Studio camera source did not connect before relay expiry")?
        .context("accept Studio camera source")?;
    let mut presented_auth = [0_u8; RELAY_AUTH_BYTES];
    source
        .read_exact(&mut presented_auth)
        .await
        .context("read Studio camera relay authentication")?;
    if presented_auth != auth.as_bytes() {
        bail!("Studio camera relay authentication failed");
    }

    let response = send_hub_request(
        hub_client()
            .get(camera_stream_url(&snapshot)?)
            .bearer_auth(&snapshot.token),
        "open Hub Studio camera stream",
    )
    .await?;
    if !response.status().is_success() {
        bail!(
            "Hub Studio camera stream returned HTTP {}",
            response.status()
        );
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    loop {
        let chunk = tokio::time::timeout(RELAY_FRAME_TIMEOUT, stream.next())
            .await
            .context("Hub Studio camera stream frame timed out")?;
        let Some(chunk) = chunk else {
            return Ok(());
        };
        let chunk = chunk.context("read Hub Studio camera stream")?;
        buffer.extend_from_slice(&chunk);
        while let Some(frame) = take_jpeg_frame(&mut buffer)? {
            source
                .write_all(&u32::try_from(frame.len())?.to_le_bytes())
                .await
                .context("write Studio camera frame length")?;
            source
                .write_all(&frame)
                .await
                .context("write Studio camera frame")?;
        }
        trim_unframed_input(&mut buffer)?;
    }
}

fn camera_stream_url(snapshot: &StudioRequestSnapshot) -> anyhow::Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(&snapshot.hub_url).context("parse Hub camera URL")?;
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("Hub camera URL cannot carry path segments"))?
        .extend([
            "api",
            "v1",
            "plugin",
            "printers",
            &snapshot.printer_id,
            "camera.mjpeg",
        ]);
    Ok(url)
}

fn take_jpeg_frame(buffer: &mut Vec<u8>) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(start) = buffer.windows(2).position(|bytes| bytes == [0xff, 0xd8]) else {
        return Ok(None);
    };
    if start > 0 {
        buffer.drain(..start);
    }
    let Some(end) = buffer
        .windows(2)
        .skip(2)
        .position(|bytes| bytes == [0xff, 0xd9])
        .map(|position| position + 3)
    else {
        return Ok(None);
    };
    if end + 1 > MAX_RELAY_FRAME_BYTES {
        bail!("Hub Studio camera JPEG exceeds relay frame limit");
    }
    let frame = buffer[..=end].to_vec();
    buffer.drain(..=end);
    Ok(Some(frame))
}

fn trim_unframed_input(buffer: &mut Vec<u8>) -> anyhow::Result<()> {
    if buffer.len() <= MAX_RELAY_FRAME_BYTES {
        return Ok(());
    }
    if buffer.windows(2).any(|bytes| bytes == [0xff, 0xd8]) {
        bail!("Hub Studio camera JPEG exceeds relay frame limit");
    }
    let trailing = buffer.last().copied().filter(|byte| *byte == 0xff);
    buffer.clear();
    if let Some(byte) = trailing {
        buffer.push(byte);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_admission_is_bounded() {
        let relays = (0..MAX_ACTIVE_RELAYS)
            .map(|_| ActiveRelay::acquire().unwrap())
            .collect::<Vec<_>>();
        assert!(ActiveRelay::acquire().is_err());
        drop(relays);
        assert!(ActiveRelay::acquire().is_ok());
    }

    #[test]
    fn relay_extracts_jpegs_from_multipart_chunks() {
        let first = [0xff, 0xd8, 1, 0xff, 0xd9];
        let second = [0xff, 0xd8, 2, 0xff, 0xd9];
        let mut buffer = b"--frame\r\nContent-Type: image/jpeg\r\n\r\n".to_vec();
        buffer.extend_from_slice(&first);
        buffer.extend_from_slice(b"\r\n--frame\r\n\r\n");
        buffer.extend_from_slice(&second);

        assert_eq!(take_jpeg_frame(&mut buffer).unwrap().unwrap(), first);
        assert_eq!(take_jpeg_frame(&mut buffer).unwrap().unwrap(), second);
        assert!(take_jpeg_frame(&mut buffer).unwrap().is_none());
    }

    #[test]
    fn relay_url_contains_no_hub_or_printer_credentials() {
        let snapshot = StudioRequestSnapshot {
            hub_url: "https://hub.example.test".to_owned(),
            token: "tenant-bearer-secret".to_owned(),
            printer_id: "printer-1".to_owned(),
        };
        let url = camera_stream_url(&snapshot).unwrap();

        assert_eq!(
            url.as_str(),
            "https://hub.example.test/api/v1/plugin/printers/printer-1/camera.mjpeg"
        );
        assert!(!url.as_str().contains("tenant-bearer-secret"));
    }
}
