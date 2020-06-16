use std::net::SocketAddr;

use interfaces::flags::IFF_RUNNING;
use interfaces::Interface;

use super::AddressDiscoveryError;
use crate::network::LocalAddress;

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
                    Some(SocketAddr::V6(addr)) if addr.ip().is_global() => {
                        Some(LocalAddress::new(&interface.name, addr))
                    }
                    _ => None,
                })
        });
    Ok(iter.collect())
}
