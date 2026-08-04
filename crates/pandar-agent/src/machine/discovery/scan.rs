use std::{
    collections::BTreeSet,
    net::{Ipv4Addr, SocketAddr},
};

use anyhow::Context;
use if_addrs::IfAddr;

use super::SSDP_PORT;

const MIN_SCAN_PREFIX: u32 = 22;

pub(super) fn local_discovery_targets() -> anyhow::Result<Vec<SocketAddr>> {
    let interfaces =
        if_addrs::get_if_addrs().context("enumerate local interfaces for discovery")?;
    let mut targets = BTreeSet::new();

    for interface in interfaces {
        if !interface.is_oper_up() || interface.is_loopback() || interface.is_p2p() {
            continue;
        }
        let IfAddr::V4(address) = interface.addr else {
            continue;
        };
        targets.extend(local_subnet_targets(address.ip, address.netmask));
    }

    Ok(targets.into_iter().collect())
}

pub(super) fn local_subnet_targets(ip: Ipv4Addr, netmask: Ipv4Addr) -> Vec<SocketAddr> {
    if !ip.is_private() {
        return Vec::new();
    }

    let ip = u32::from(ip);
    let netmask = u32::from(netmask);
    // Keep discovery traffic bounded when an interface has a broad subnet mask.
    let scan_mask = if netmask.count_ones() < MIN_SCAN_PREFIX {
        u32::MAX << (32 - MIN_SCAN_PREFIX)
    } else {
        netmask
    };
    let network = ip & scan_mask;
    let broadcast = network | !scan_mask;

    (network.saturating_add(1)..broadcast)
        .filter(|candidate| *candidate != ip)
        .map(|candidate| SocketAddr::from((Ipv4Addr::from(candidate), SSDP_PORT)))
        .collect()
}
