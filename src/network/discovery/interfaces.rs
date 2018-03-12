use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

use interfaces::{Address, Interface, Kind};
use interfaces::flags::IFF_RUNNING;

use network::LocalAddress;
use super::AddressDiscoveryError;

pub fn discover_addresses() -> Result<Vec<LocalAddress>, AddressDiscoveryError> {
    let interfaces = Interface::get_all().map_err(|err| {
        AddressDiscoveryError::InterfacesError(
            format!("Error while fetching interfaces: {}", err).to_owned(),
        )
    })?;
    let iter = interfaces
        .into_iter()
        .filter(|interface| !interface.is_loopback() && interface.flags.contains(IFF_RUNNING))
        .flat_map(move |interface| {
            interface
                .addresses
                .clone()
                .into_iter()
                .filter_map(move |address| match address.addr {
                    Some(addr) if address.kind == Kind::Ipv4 || address.kind == Kind::Ipv6 => {
                        Some(LocalAddress::new(
                            &interface.name,
                            addr,
                            guess_gateway(&address),
                            None,
                        ))
                    }
                    _ => None,
                })
        });
    Ok(iter.collect())
}

fn guess_gateway(addr: &Address) -> Option<SocketAddr> {
    match (addr.addr, addr.mask) {
        (Some(SocketAddr::V4(addr)), Some(SocketAddr::V4(mask))) => {
            Some(first_ipv4(addr, mask).into())
        }
        (Some(SocketAddr::V6(addr)), Some(SocketAddr::V6(mask))) => {
            Some(first_ipv6(addr, mask).into())
        }
        _ => None,
    }
}

fn first_ipv4(addr: SocketAddrV4, mask: SocketAddrV4) -> SocketAddrV4 {
    let mut masked: Vec<u8> = addr.ip()
        .octets()
        .iter()
        .zip(mask.ip().octets().iter())
        .map(|(a, b)| a & b)
        .collect();
    let i = masked.len() - 1;
    masked[i] = masked[i] | 1u8;
    let mut ip = [0u8; 4];
    ip.clone_from_slice(&masked);
    SocketAddrV4::new(Ipv4Addr::from(ip), addr.port())
}

fn first_ipv6(addr: SocketAddrV6, mask: SocketAddrV6) -> SocketAddrV6 {
    let mut masked: Vec<u8> = addr.ip()
        .octets()
        .iter()
        .zip(mask.ip().octets().iter())
        .map(|(a, b)| a & b)
        .collect();
    let i = masked.len() - 1;
    masked[i] = masked[i] | 1u8;
    let mut ip = [0u8; 16];
    ip.clone_from_slice(&masked);
    SocketAddrV6::new(
        Ipv6Addr::from(ip),
        addr.port(),
        addr.flowinfo(),
        addr.scope_id(),
    )
}
