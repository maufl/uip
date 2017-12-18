use std::net::{SocketAddr, SocketAddrV4};
use std::error::Error;
use std::fmt::{self, Display};

use futures::{Stream, IntoFuture, Future};
use futures::future::ok;
use futures::stream::iter_ok;
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
            AddressDiscoveryError::InterfacesError(ref string) => string,
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

pub fn discover_addresses(
    handle: Handle,
) -> impl Stream<Item = LocalAddress, Error = AddressDiscoveryError> + 'static {
    Interface::get_all()
        .map_err(|err| {
            AddressDiscoveryError::InterfacesError(
                format!("Error while fetching interfaces: {}", err).to_owned(),
            )
        })
        .into_future()
        .into_stream()
        .map(|interfaces| {
            iter_ok::<_, AddressDiscoveryError>(interfaces)
                .filter_map(|interface| {
                    if interface.is_loopback() || !interface.flags.contains(IFF_RUNNING) {
                        return None;
                    }
                    let stream = iter_ok(interface.addresses.clone()).filter_map(
                        move |address| {
                            match address.addr {
                                Some(addr)
                                    if address.kind == Kind::Ipv4 || address.kind == Kind::Ipv6 => {
                                    Some(LocalAddress::new(interface.name.clone(), addr, None))
                                }
                                _ => None,
                            }
                        },
                    );
                    Some(stream)
                })
                .flatten()
        })
        .flatten()
        .and_then(move |address| match address.internal_address {
            SocketAddr::V4(addr) => request_external_address(address, addr, &handle),
            _ => Box::new(ok(address)),
        })
}

fn request_external_address(
    address: LocalAddress,
    internal_address: SocketAddrV4,
    handle: &Handle,
) -> Box<Future<Item = LocalAddress, Error = AddressDiscoveryError> + 'static> {
    let default_address = address.clone();
    let future = search_gateway(handle)
        .from_err::<AddressDiscoveryError>()
        .and_then(move |gateway| {
            gateway
                .get_any_address(PortMappingProtocol::UDP, internal_address, 0, "UIP")
                .from_err()
        })
        .map(move |external_address| {
            LocalAddress::new(
                address.interface,
                SocketAddr::V4(internal_address),
                Some(SocketAddr::V4(external_address)),
            )
        })
        .or_else(move |err| {
            info!(
                "Unable to request external address on interface {}: {}",
                default_address.interface,
                err
            );
            Ok(default_address)
        });
    Box::new(future)
}
