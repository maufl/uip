use std::net::SocketAddrV4;

use igd::{
    PortMappingProtocol,
    aio::search_gateway,
};

use super::AddressDiscoveryError;

pub async fn request_external_address(
    internal: SocketAddrV4,
) -> Result<SocketAddrV4, AddressDiscoveryError> {
    let gateway = search_gateway(Default::default()).await?;
    Ok(gateway.get_any_address(PortMappingProtocol::UDP, internal, 0, "UIP").await?)
}
