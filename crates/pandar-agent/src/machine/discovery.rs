use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use anyhow::Context;
use serde::Serialize;
use tokio::{net::UdpSocket, time::Instant};

mod scan;

use scan::local_discovery_targets;

const SSDP_PORT: u16 = 2021;
const SSDP_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 255, 255, 250)), SSDP_PORT);
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
    let targets = std::iter::once(SSDP_ADDR).chain(local_discovery_targets()?);
    discover_printers_at_targets(timeout_seconds, targets).await
}

async fn discover_printers_at_targets(
    timeout_seconds: u32,
    targets: impl IntoIterator<Item = SocketAddr>,
) -> anyhow::Result<PrinterDiscoveryResult> {
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
    for target in targets {
        socket
            .send_to(request.as_bytes(), target)
            .await
            .with_context(|| format!("send Bambu SSDP discovery request to {target}"))?;
    }

    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.into());
    let mut buf = [0u8; 4096];
    let mut deduplicator = DiscoveredPrinterDedup::default();
    let mut printers = Vec::new();

    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, source))) => {
                if let Some(printer) = parse_ssdp_response(&buf[..len], source)
                    && let Some(printer) = deduplicator.admit(printer)
                {
                    printers.push(printer);
                }
            }
            Ok(Err(err)) => return Err(err).context("receive Bambu SSDP discovery response"),
            Err(_) => break,
        }
    }

    Ok(PrinterDiscoveryResult::new(printers))
}

pub async fn discover_printer_at_host(
    host: &str,
    timeout_seconds: u32,
) -> anyhow::Result<Option<DiscoveredPrinter>> {
    let addr = SocketAddr::new(
        host.parse()
            .with_context(|| format!("parse Bambu SSDP host {host}"))?,
        SSDP_PORT,
    );
    discover_printer_at_addr(addr, timeout_seconds).await
}

async fn discover_printer_at_addr(
    addr: SocketAddr,
    timeout_seconds: u32,
) -> anyhow::Result<Option<DiscoveredPrinter>> {
    let timeout_seconds = timeout_seconds.clamp(1, 15);
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .context("bind direct SSDP discovery UDP socket")?;
    let request = format!(
        "M-SEARCH * HTTP/1.1\r\nHOST: {SSDP_ADDR}\r\nMAN: \"ssdp:discover\"\r\nMX: 1\r\nST: {SSDP_ST}\r\n\r\n"
    );
    socket
        .send_to(request.as_bytes(), addr)
        .await
        .with_context(|| format!("send Bambu SSDP discovery request to {addr}"))?;

    let deadline = Instant::now() + Duration::from_secs(timeout_seconds.into());
    let mut buf = [0u8; 4096];
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, source))) => {
                if let Some(printer) = parse_ssdp_response(&buf[..len], source) {
                    return Ok(Some(printer));
                }
            }
            Ok(Err(err)) => return Err(err).context("receive direct Bambu SSDP response"),
            Err(_) => break,
        }
    }

    Ok(None)
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
    let mut deduplicator = DiscoveredPrinterDedup::default();
    printers
        .into_iter()
        .filter_map(|printer| deduplicator.admit(printer))
        .collect()
}

/// Deduplicates discovery responses as they arrive so repeated responses
/// within one scan window do not accumulate in memory. The admission rules
/// match `deduplicate_printers` exactly, so batching or streaming responses
/// through this type yields the same result.
#[derive(Default)]
struct DiscoveredPrinterDedup {
    seen_serials: HashSet<String>,
    seen_hosts: HashSet<String>,
}

impl DiscoveredPrinterDedup {
    fn admit(&mut self, printer: DiscoveredPrinter) -> Option<DiscoveredPrinter> {
        if let Some(serial) = &printer.serial_number {
            if !self.seen_serials.insert(serial.clone()) {
                return None;
            }
        } else if !self.seen_hosts.insert(printer.host.clone()) {
            return None;
        }
        self.seen_hosts.insert(printer.host.clone());
        Some(printer)
    }
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

    #[tokio::test]
    async fn direct_host_discovery_parses_unicast_response() {
        let server = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let addr = server.local_addr().unwrap();
        let server_task = tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            let (_len, peer) = server.recv_from(&mut buf).await.unwrap();
            let response = b"HTTP/1.1 200 OK\r\nUSN: SERIAL123\r\nDevName.bambu.com: Office X1C\r\nDevModel.bambu.com: X1 Carbon\r\nST: urn:bambulab-com:device:3dprinter:1\r\n\r\n";
            server.send_to(response, peer).await.unwrap();
        });

        let printer = discover_printer_at_addr(addr, 1).await.unwrap().unwrap();

        assert_eq!(printer.serial_number.as_deref(), Some("SERIAL123"));
        assert_eq!(printer.host, Ipv4Addr::LOCALHOST.to_string());
        assert_eq!(printer.name.as_deref(), Some("Office X1C"));
        assert_eq!(printer.model.as_deref(), Some("X1 Carbon"));
        server_task.await.unwrap();
    }

    #[test]
    fn local_subnet_targets_cover_private_interface_peers_only() {
        let targets = scan::local_subnet_targets(
            Ipv4Addr::new(10, 1, 61, 3),
            Ipv4Addr::new(255, 255, 255, 0),
        );

        assert_eq!(targets.len(), 253);
        assert!(targets.contains(&SocketAddr::from((Ipv4Addr::new(10, 1, 61, 84), 2021))));
        assert!(!targets.contains(&SocketAddr::from((Ipv4Addr::new(10, 1, 61, 0), 2021))));
        assert!(!targets.contains(&SocketAddr::from((Ipv4Addr::new(10, 1, 61, 3), 2021))));
        assert!(!targets.contains(&SocketAddr::from((Ipv4Addr::new(10, 1, 61, 255), 2021))));
    }

    #[test]
    fn local_subnet_targets_are_bounded_and_never_scan_public_networks() {
        let broad_private_targets =
            scan::local_subnet_targets(Ipv4Addr::new(10, 1, 61, 3), Ipv4Addr::new(255, 0, 0, 0));

        assert_eq!(broad_private_targets.len(), 1021);
        assert!(
            broad_private_targets.contains(&SocketAddr::from((Ipv4Addr::new(10, 1, 61, 84), 2021)))
        );
        assert!(
            scan::local_subnet_targets(
                Ipv4Addr::new(203, 0, 113, 3),
                Ipv4Addr::new(255, 255, 255, 0),
            )
            .is_empty()
        );
    }

    #[tokio::test]
    async fn discovery_finds_unicast_printer_when_another_target_does_not_respond() {
        let silent_target = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let printer = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let printer_addr = printer.local_addr().unwrap();
        let printer_task = tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            let (_len, peer) = printer.recv_from(&mut buf).await.unwrap();
            let response = b"HTTP/1.1 200 OK\r\nUSN: FALLBACK123\r\nDevName.bambu.com: Unicast Printer\r\nDevModel.bambu.com: P1S\r\nST: urn:bambulab-com:device:3dprinter:1\r\n\r\n";
            printer.send_to(response, peer).await.unwrap();
        });

        let result =
            discover_printers_at_targets(1, [silent_target.local_addr().unwrap(), printer_addr])
                .await
                .unwrap();

        assert_eq!(result.printers.len(), 1);
        assert_eq!(
            result.printers[0].serial_number.as_deref(),
            Some("FALLBACK123")
        );
        assert_eq!(result.printers[0].name.as_deref(), Some("Unicast Printer"));
        printer_task.await.unwrap();
    }
}
