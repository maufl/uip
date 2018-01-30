use std::net::{SocketAddr, SocketAddrV4};
use std::error::Error;
use std::fmt::{self, Display};

use futures::Future;
use futures::future::err;
use tokio_core::reactor::Handle;

use igd::{PortMappingProtocol, SearchError, AddAnyPortError};
use igd::tokio::search_gateway;
use interfaces::{Interface, Kind};
use interfaces::flags::IFF_RUNNING;

use network::LocalAddress;

#[derive(Clone, Debug)]
pub enum AddressDiscoveryError {
    IgdError(String),
    InterfacesError(String),
    UnsupportedAddress(String),
}

impl Display for AddressDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl Error for AddressDiscoveryError {
    fn description(&self) -> &str {
        match *self {
            AddressDiscoveryError::IgdError(ref string) |
            AddressDiscoveryError::InterfacesError(ref string) |
            AddressDiscoveryError::UnsupportedAddress(ref string) => string,
        }
    }
}

impl From<SearchError> for AddressDiscoveryError {
    fn from(err: SearchError) -> AddressDiscoveryError {
        AddressDiscoveryError::IgdError(format!(
            "Error while searching internet gateway device: {}",
            err
        ))
    }
}

impl From<AddAnyPortError> for AddressDiscoveryError {
    fn from(err: AddAnyPortError) -> AddressDiscoveryError {
        AddressDiscoveryError::IgdError(format!("Error while setting up port mapping: {}", err))
    }
}

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

pub fn request_external_address(
    internal: SocketAddrV4,
    handle: &Handle,
) -> impl Future<Item = SocketAddrV4, Error = AddressDiscoveryError> {
    search_gateway(handle)
        .from_err::<AddressDiscoveryError>()
        .and_then(move |gateway| {
            gateway
                .get_any_address(PortMappingProtocol::UDP, internal, 0, "UIP")
                .from_err()
        })
}
