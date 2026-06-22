use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use anyhow::Context;
use serde::Serialize;
use tokio::{net::UdpSocket, time::Instant};

const SSDP_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 255, 255, 250)), 2021);
const SSDP_ST: &str = "urn:bambulab-com:device:3dprinter:1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrinterDiscoveryResult {
    #[serde(rename = "type")]
    pub result_type: &'static str,
    pub printers: Vec<DiscoveredPrinter>,
}

impl PrinterDiscoveryResult {
    pub fn new(printers: Vec<DiscoveredPrinter>) -> Self {
        Self {
            result_type: "printer_discovery",
            printers,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoveredPrinter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub source: &'static str,
}

pub async fn discover_printers(timeout_seconds: u32) -> anyhow::Result<PrinterDiscoveryResult> {
    let timeout_seconds = timeout_seconds.clamp(1, 15);
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .context("bind SSDP discovery UDP socket")?;
    socket
        .set_broadcast(true)
        .context("enable SSDP UDP broadcast")?;

    let request = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {SSDP_ADDR}\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {SSDP_ST}\r\n\r\n"
    );
    socket
        .send_to(request.as_bytes(), SSDP_ADDR)
        .await
        .context("send Bambu SSDP discovery request")?;

    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.into());
    let mut buf = [0u8; 4096];
    let mut printers = Vec::new();

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, source))) => {
                if let Some(printer) = parse_ssdp_response(&buf[..len], source) {
                    printers.push(printer);
                }
            }
            Ok(Err(err)) => return Err(err).context("receive Bambu SSDP discovery response"),
            Err(_) => break,
        }
    }

    Ok(PrinterDiscoveryResult::new(deduplicate_printers(printers)))
}

pub fn parse_ssdp_response(bytes: &[u8], source: SocketAddr) -> Option<DiscoveredPrinter> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut headers = Vec::new();
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.push((
            name.trim().to_ascii_lowercase(),
            value.trim().trim_matches('"').to_owned(),
        ));
    }

    let has_bambu_target = headers.iter().any(|(name, value)| {
        matches!(name.as_str(), "st" | "nt") && value.contains("bambulab-com:device:3dprinter")
    });
    let has_bambu_header = headers.iter().any(|(name, _)| name.ends_with(".bambu.com"));
    let serial_number = header_value(&headers, "usn").and_then(serial_from_usn);
    if !has_bambu_target && !has_bambu_header {
        return None;
    }

    let model = header_value(&headers, "devmodel.bambu.com")
        .map(ToOwned::to_owned)
        .or_else(|| header_value(&headers, "nt").and_then(model_from_nt));

    Some(DiscoveredPrinter {
        serial_number,
        host: source.ip().to_string(),
        name: header_value(&headers, "devname.bambu.com").map(ToOwned::to_owned),
        model,
        source: "ssdp",
    })
}

pub fn deduplicate_printers(printers: Vec<DiscoveredPrinter>) -> Vec<DiscoveredPrinter> {
    let mut seen_serials = HashSet::new();
    let mut seen_hosts = HashSet::new();
    let mut deduped = Vec::new();

    for printer in printers {
        if let Some(serial) = &printer.serial_number {
            if !seen_serials.insert(serial.clone()) {
                continue;
            }
        } else if !seen_hosts.insert(printer.host.clone()) {
            continue;
        }
        seen_hosts.insert(printer.host.clone());
        deduped.push(printer);
    }

    deduped
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name == name)
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

fn serial_from_usn(usn: &str) -> Option<String> {
    let serial = usn
        .split("::")
        .next()
        .unwrap_or(usn)
        .trim()
        .trim_start_matches("uuid:")
        .trim();
    (!serial.is_empty()).then(|| serial.to_owned())
}

fn model_from_nt(nt: &str) -> Option<String> {
    nt.split(':')
        .next_back()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("1"))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_extracts_bambuddy_fields_and_source_host() {
        let packet = b"HTTP/1.1 200 OK\r\nUSN: SERIAL123::urn:bambulab-com:device:3dprinter:1\r\nDevName.bambu.com: Office X1C\r\nDevModel.bambu.com: X1 Carbon\r\nST: urn:bambulab-com:device:3dprinter:1\r\n\r\n";
        let source = "192.0.2.55:2021".parse().unwrap();

        assert_eq!(
            parse_ssdp_response(packet, source),
            Some(DiscoveredPrinter {
                serial_number: Some("SERIAL123".to_owned()),
                host: "192.0.2.55".to_owned(),
                name: Some("Office X1C".to_owned()),
                model: Some("X1 Carbon".to_owned()),
                source: "ssdp",
            })
        );
    }

    #[test]
    fn parser_ignores_unrelated_packets() {
        let source = "192.0.2.55:2021".parse().unwrap();

        assert_eq!(
            parse_ssdp_response(b"HTTP/1.1 200 OK\r\nST: upnp:rootdevice\r\n\r\n", source),
            None
        );
        assert_eq!(
            parse_ssdp_response(b"HTTP/1.1 200 OK\r\nUSN: uuid:SERIAL123\r\n\r\n", source),
            None
        );
    }

    #[test]
    fn deduplication_prefers_first_serial_or_host() {
        let printers = vec![
            printer(Some("SERIAL1"), "192.0.2.1"),
            printer(Some("SERIAL1"), "192.0.2.2"),
            printer(None, "192.0.2.3"),
            printer(None, "192.0.2.3"),
        ];

        assert_eq!(
            deduplicate_printers(printers),
            vec![
                printer(Some("SERIAL1"), "192.0.2.1"),
                printer(None, "192.0.2.3")
            ]
        );
    }

    fn printer(serial_number: Option<&str>, host: &str) -> DiscoveredPrinter {
        DiscoveredPrinter {
            serial_number: serial_number.map(str::to_owned),
            host: host.to_owned(),
            name: None,
            model: None,
            source: "ssdp",
        }
    }
}
