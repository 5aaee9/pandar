use std::process::Stdio;

use anyhow::{Context, bail};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::mpsc,
};

use crate::machine::BambuPrinterEndpoint;

mod local;

const BAMBU_RTSP_PORT: u16 = 322;
const CAMERA_BOUNDARY: &[u8] = b"--frame\r\n";
const FFMPEG_PATH_VAR: &str = "PANDAR_FFMPEG_PATH";

pub async fn stream_camera_mjpeg(
    endpoint: BambuPrinterEndpoint,
    sender: mpsc::Sender<Vec<u8>>,
) -> anyhow::Result<()> {
    if pandar_core::compatibility::studio_local_camera_supported(endpoint.model.as_deref()) {
        return local::stream_camera_mjpeg(endpoint, sender).await;
    }
    if !supports_rtsp(endpoint.model.as_deref()) {
        bail!(
            "camera streaming is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }

    let camera_url = build_camera_url(&endpoint.host, &endpoint.access_code);
    let mut process = spawn_ffmpeg_command(ffmpeg_mjpeg_command(), &camera_url)
        .await
        .context("spawn ffmpeg for Bambu camera stream")?;
    let mut stdout = process
        .stdout
        .take()
        .context("ffmpeg stdout should be piped")?;
    let mut buffer = Vec::new();
    let mut chunk = vec![0_u8; 8192];

    loop {
        let read = stdout
            .read(&mut chunk)
            .await
            .context("read ffmpeg camera output")?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        while let Some(frame) = take_jpeg_frame(&mut buffer) {
            if sender.send(mjpeg_part(&frame)).await.is_err() {
                return Ok(());
            }
        }
    }

    let status = process
        .wait()
        .await
        .context("wait for ffmpeg camera stream")?;
    if status.success() {
        Ok(())
    } else {
        bail!("ffmpeg camera stream exited with {status}")
    }
}

pub async fn stream_camera_fragmented_mp4(
    endpoint: BambuPrinterEndpoint,
    sender: mpsc::Sender<Vec<u8>>,
) -> anyhow::Result<()> {
    if !supports_rtsp(endpoint.model.as_deref()) {
        bail!(
            "camera streaming is not supported for model {}",
            endpoint.model.as_deref().unwrap_or("unknown")
        );
    }

    let camera_url = build_camera_url(&endpoint.host, &endpoint.access_code);
    let mut process = spawn_ffmpeg_command(ffmpeg_fragmented_mp4_command(), &camera_url)
        .await
        .context("spawn ffmpeg for Bambu camera stream")?;
    let mut stdout = process
        .stdout
        .take()
        .context("ffmpeg stdout should be piped")?;
    let mut chunk = vec![0_u8; 16384];

    loop {
        let read = stdout
            .read(&mut chunk)
            .await
            .context("read ffmpeg camera output")?;
        if read == 0 {
            break;
        }
        if sender.send(chunk[..read].to_vec()).await.is_err() {
            return Ok(());
        }
    }

    let status = process
        .wait()
        .await
        .context("wait for ffmpeg camera stream")?;
    if status.success() {
        Ok(())
    } else {
        bail!("ffmpeg camera stream exited with {status}")
    }
}

pub fn supports_rtsp(model: Option<&str>) -> bool {
    let Some(model) = model else {
        return false;
    };
    let model = model.to_ascii_uppercase();
    model.contains("X1")
        || model.contains("X2")
        || model.contains("H2")
        || model.contains("P2")
        || matches!(
            model.as_str(),
            "BL-P001" | "C13" | "N6" | "O1D" | "O1C" | "O1C2" | "O1S" | "O1E" | "O2D" | "N7"
        )
}

pub fn build_camera_url(host: &str, access_code: &str) -> String {
    format!(
        "rtsps://bblp:{}@{host}:{BAMBU_RTSP_PORT}/streaming/live/1",
        percent_encode_userinfo(access_code)
    )
}

fn ffmpeg_mjpeg_command() -> Command {
    let mut command = Command::new(ffmpeg_executable());
    command
        .arg("-fflags")
        .arg("nobuffer")
        .arg("-flags")
        .arg("low_delay")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-protocol_whitelist")
        .arg("file,pipe,tcp,tls,rtp,rtsp,crypto")
        .arg("-i")
        .arg("pipe:0")
        .arg("-f")
        .arg("mjpeg")
        .arg("-q:v")
        .arg("5")
        .arg("-r")
        .arg("5")
        .arg("-an")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
}

fn ffmpeg_fragmented_mp4_command() -> Command {
    let mut command = Command::new(ffmpeg_executable());
    command
        .arg("-fflags")
        .arg("nobuffer")
        .arg("-flags")
        .arg("low_delay")
        .arg("-f")
        .arg("concat")
        .arg("-safe")
        .arg("0")
        .arg("-protocol_whitelist")
        .arg("file,pipe,tcp,tls,rtp,rtsp,crypto")
        .arg("-i")
        .arg("pipe:0")
        .arg("-an")
        .arg("-c:v")
        .arg("libx264")
        .arg("-preset")
        .arg("ultrafast")
        .arg("-tune")
        .arg("zerolatency")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg("-g")
        .arg("30")
        .arg("-keyint_min")
        .arg("30")
        .arg("-sc_threshold")
        .arg("0")
        .arg("-movflags")
        .arg("frag_keyframe+empty_moov+default_base_moof")
        .arg("-f")
        .arg("mp4")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
}

fn ffmpeg_executable() -> String {
    ffmpeg_executable_from_env(std::env::var(FFMPEG_PATH_VAR).ok())
}

fn ffmpeg_executable_from_env(value: Option<String>) -> String {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "ffmpeg".to_owned())
}

async fn spawn_ffmpeg_command(
    mut command: Command,
    camera_url: &str,
) -> anyhow::Result<tokio::process::Child> {
    let program = command
        .as_std()
        .get_program()
        .to_string_lossy()
        .into_owned();
    let mut child = command
        .spawn()
        .with_context(|| format!("spawn ffmpeg executable {program}"))?;
    let mut stdin = child.stdin.take().context("ffmpeg stdin should be piped")?;
    stdin
        .write_all(ffconcat_camera_input(camera_url).as_bytes())
        .await
        .context("write protected ffmpeg camera input")?;
    stdin
        .shutdown()
        .await
        .context("close ffmpeg camera input")?;
    Ok(child)
}

fn ffconcat_camera_input(camera_url: &str) -> String {
    format!(
        "ffconcat version 1.0\n\
         file '{camera_url}'\n\
         option rtsp_transport tcp\n\
         option rtsp_flags prefer_tcp\n\
         option timeout 30000000\n\
         option buffer_size 1024000\n\
         option max_delay 500000\n"
    )
}

fn percent_encode_userinfo(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn take_jpeg_frame(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let start = buffer.windows(2).position(|bytes| bytes == [0xff, 0xd8])?;
    if start > 0 {
        buffer.drain(..start);
    }
    let end = buffer
        .windows(2)
        .skip(2)
        .position(|bytes| bytes == [0xff, 0xd9])?
        + 3;
    let frame = buffer[..=end].to_vec();
    buffer.drain(..=end);
    Some(frame)
}

fn mjpeg_part(frame: &[u8]) -> Vec<u8> {
    let mut part = Vec::with_capacity(frame.len() + 80);
    part.extend_from_slice(CAMERA_BOUNDARY);
    part.extend_from_slice(b"Content-Type: image/jpeg\r\nContent-Length: ");
    part.extend_from_slice(frame.len().to_string().as_bytes());
    part.extend_from_slice(b"\r\n\r\n");
    part.extend_from_slice(frame);
    part.extend_from_slice(b"\r\n");
    part
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_camera_url() {
        assert_eq!(
            build_camera_url("10.1.61.124", "secret"),
            "rtsps://bblp:secret@10.1.61.124:322/streaming/live/1"
        );
        assert_eq!(
            build_camera_url("10.1.61.124", "secret'\n"),
            "rtsps://bblp:secret%27%0A@10.1.61.124:322/streaming/live/1"
        );
    }

    #[test]
    fn supports_display_model_names() {
        assert!(supports_rtsp(Some("Bambu Lab X2D")));
        assert!(supports_rtsp(Some("Bambu Lab X1 Carbon")));
    }

    #[test]
    fn wraps_jpeg_frame_as_mjpeg_part() {
        let part = mjpeg_part(&[0xff, 0xd8, 0xff, 0xd9]);
        assert!(part.starts_with(b"--frame\r\nContent-Type: image/jpeg\r\n"));
        assert!(part.ends_with(b"\xff\xd8\xff\xd9\r\n"));
    }

    #[test]
    fn builds_fragmented_mp4_ffmpeg_command() {
        let command = ffmpeg_fragmented_mp4_command();
        let args = command
            .as_std()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.windows(2).any(|args| args == ["-c:v", "libx264"]));
        assert!(args.windows(2).any(|args| args == ["-g", "30"]));
        assert!(args.windows(2).any(|args| args == ["-f", "mp4"]));
        assert!(!args.iter().any(|arg| arg.contains("rtsps://")));
        assert!(
            args.windows(2).any(|args| {
                args == ["-movflags", "frag_keyframe+empty_moov+default_base_moof"]
            })
        );
    }

    #[test]
    fn scopes_rtsp_options_to_the_concat_file() {
        for command in [ffmpeg_mjpeg_command(), ffmpeg_fragmented_mp4_command()] {
            let args = command
                .as_std()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(!args.iter().any(|arg| arg == "-rtsp_transport"));
        }

        let input = ffconcat_camera_input("rtsps://example.test/stream");
        for option in [
            "option rtsp_transport tcp",
            "option rtsp_flags prefer_tcp",
            "option timeout 30000000",
            "option buffer_size 1024000",
            "option max_delay 500000",
        ] {
            assert!(input.lines().any(|line| line == option));
        }
    }

    #[test]
    fn ffmpeg_executable_defaults_to_path_lookup() {
        assert_eq!(ffmpeg_executable_from_env(None), "ffmpeg");
        assert_eq!(ffmpeg_executable_from_env(Some("  ".to_owned())), "ffmpeg");
    }

    #[test]
    fn ffmpeg_executable_accepts_explicit_path() {
        assert_eq!(
            ffmpeg_executable_from_env(Some(" C:\\tools\\ffmpeg.exe ".to_owned())),
            "C:\\tools\\ffmpeg.exe"
        );
    }

    #[tokio::test]
    async fn ffmpeg_spawn_error_preserves_executable_context() {
        let mut command = Command::new("C:\\definitely-missing-pandar-ffmpeg\\ffmpeg.exe");
        command.stdout(Stdio::piped()).stderr(Stdio::null());

        let err = spawn_ffmpeg_command(command, "rtsps://example.test/stream")
            .await
            .unwrap_err();
        let formatted = format!("{err:#}");

        assert!(formatted.contains("spawn ffmpeg executable"));
        assert!(formatted.contains("definitely-missing-pandar-ffmpeg"));
        assert!(formatted.contains("os error") || formatted.contains("The system cannot find"));
    }
}
