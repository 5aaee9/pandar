use anyhow::{Context, anyhow, bail};

pub(super) const BRTC_MAX_FRAME_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;

pub(super) struct BrtcFrame {
    pub(super) magic: u32,
    pub(super) payload: Vec<u8>,
}

pub(super) fn checked_frame_payload_len(payload_len: u32) -> anyhow::Result<usize> {
    let payload_len = usize::try_from(payload_len).context("convert BRTC frame payload length")?;
    if payload_len > BRTC_MAX_FRAME_PAYLOAD_SIZE {
        tracing::warn!(
            payload_len,
            limit = BRTC_MAX_FRAME_PAYLOAD_SIZE,
            "rejecting oversized BRTC frame payload"
        );
        bail!(
            "BRTC frame payload length {payload_len} exceeds limit {BRTC_MAX_FRAME_PAYLOAD_SIZE}"
        );
    }
    Ok(payload_len)
}

pub(super) fn checked_chunk_end(
    offset: usize,
    chunk_size: usize,
    total: usize,
) -> anyhow::Result<usize> {
    offset
        .checked_add(chunk_size)
        .map(|end| end.min(total))
        .ok_or_else(|| {
            anyhow!("BRTC upload offset {offset} plus chunk size {chunk_size} overflowed")
        })
}

pub(super) fn checked_binary_frame_payload_len(
    body_len: usize,
    binary_len: usize,
) -> anyhow::Result<(usize, usize)> {
    let additional = 2_usize
        .checked_add(binary_len)
        .ok_or_else(|| anyhow!("BRTC binary frame payload length overflowed"))?;
    let frame_len = body_len
        .checked_add(additional)
        .ok_or_else(|| anyhow!("BRTC binary frame payload length overflowed"))?;
    u32::try_from(frame_len).context("BRTC binary frame payload exceeds u32")?;
    Ok((additional, frame_len))
}

pub(super) fn append_binary_frame_payload(body: &mut Vec<u8>, binary: &[u8]) -> anyhow::Result<()> {
    let (additional, _) = checked_binary_frame_payload_len(body.len(), binary.len())?;
    body.try_reserve_exact(additional)
        .with_context(|| format!("reserve {additional} bytes for BRTC binary frame payload"))?;
    body.extend_from_slice(b"\n\n");
    body.extend_from_slice(binary);
    Ok(())
}

pub(super) fn padded_ascii(value: &str, width: usize) -> String {
    let mut out = String::with_capacity(width);
    out.extend(value.chars().take(width));
    while out.len() < width {
        out.push('\0');
    }
    out
}

pub(super) fn json_prefix_len(bytes: &[u8]) -> Option<usize> {
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
