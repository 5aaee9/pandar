use std::{net::SocketAddr, str::FromStr};

pub(crate) struct RelayConfig {
    pub(crate) address: SocketAddr,
    pub(crate) auth: [u8; 32],
}

pub(crate) fn parse_relay_url(url: &str) -> Option<RelayConfig> {
    let query = url.strip_prefix("bambu:///local/127.0.0.1?")?;
    let mut port = None;
    let mut auth = None;
    for field in query.split('&') {
        let Some((name, value)) = field.split_once('=') else {
            continue;
        };
        match name {
            "port" if port.is_none() => port = u16::from_str(value).ok().filter(|port| *port > 0),
            "auth" if auth.is_none() && value.len() == 32 => {
                let bytes: [u8; 32] = value.as_bytes().try_into().ok()?;
                if bytes.iter().all(u8::is_ascii_hexdigit) {
                    auth = Some(bytes);
                }
            }
            _ => {}
        }
    }
    Some(RelayConfig {
        address: SocketAddr::from(([127, 0, 0, 1], port?)),
        auth: auth?,
    })
}

pub(crate) fn jpeg_dimensions(frame: &[u8]) -> Option<(i32, i32)> {
    if !frame.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut offset = 2;
    while offset + 4 <= frame.len() {
        if frame[offset] != 0xff {
            offset += 1;
            continue;
        }
        while offset < frame.len() && frame[offset] == 0xff {
            offset += 1;
        }
        let marker = *frame.get(offset)?;
        offset += 1;
        if matches!(marker, 0x01 | 0xd8 | 0xd9 | 0xd0..=0xd7) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes([
            *frame.get(offset)?,
            *frame.get(offset + 1)?,
        ]));
        if length < 2 || offset + length > frame.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if length < 7 {
                return None;
            }
            let height = i32::from(u16::from_be_bytes([frame[offset + 3], frame[offset + 4]]));
            let width = i32::from(u16::from_be_bytes([frame[offset + 5], frame[offset + 6]]));
            return (width > 0 && height > 0).then_some((width, height));
        }
        offset += length;
    }
    None
}
