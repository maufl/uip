use std::net::SocketAddrV4;

use futures::Future;
use tokio_core::reactor::Handle;
use igd::PortMappingProtocol;
use igd::tokio::search_gateway;

use super::AddressDiscoveryError;

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
