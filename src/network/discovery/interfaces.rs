use interfaces::{Interface, Kind};
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
        .filter(|interface| {
            !interface.is_loopback() && interface.flags.contains(IFF_RUNNING)
        })
        .flat_map(move |interface| {
            interface.addresses.clone().into_iter().filter_map(
                move |address| {
                    match address.addr {
                        Some(addr) if address.kind == Kind::Ipv4 || address.kind == Kind::Ipv6 => {
                            Some(LocalAddress::new(&interface.name, addr, None, None))
                        }
                        _ => None,
                    }
                },
            )
        });
    Ok(iter.collect())
}
