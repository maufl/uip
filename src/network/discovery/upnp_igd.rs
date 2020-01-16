use std::net::{SocketAddrV4, Ipv4Addr};

use igd::{
    PortMappingProtocol,
    aio::search_gateway,
    aio::Gateway
};

use super::AddressDiscoveryError;

pub async fn request_external_address(
    internal: SocketAddrV4,
) -> Result<SocketAddrV4, AddressDiscoveryError> {
    let gateway = search_gateway(Default::default()).await?;
    if let Some(port) = check_for_existing_mapping(&gateway, &internal).await {
        match gateway.get_external_ip().await {
            Ok(ip) => return Ok(SocketAddrV4::new(ip, port)),
            Err(err) => return Err(AddressDiscoveryError::IgdError(format!("Unable to determine the external IP address: {}", err))),
        }
    }
    Ok(gateway.get_any_address(PortMappingProtocol::UDP, internal, 0, "UIP").await?)
}

async fn check_for_existing_mapping(gateway: &Gateway, internal: &SocketAddrV4) -> Option<u16> {
    let mut index = 0u32;
    loop {
        let mapping = match gateway.get_generic_port_mapping_entry(index).await {
            Ok(mapping) => mapping,
            Err(igd::GetGenericPortMappingEntryError::SpecifiedArrayIndexInvalid) => return None,
            Err(err) => {
                debug!("Received error while getting port mappings from IGD: {}", err);
                return None;
            }
        };
        index += 1;
        let client_ip: Ipv4Addr = match mapping.internal_client.parse() {
            Ok(ip) => ip,
            Err(_) => continue
        };
        if &client_ip != internal.ip() {
            continue;
        };
        if mapping.internal_port != internal.port() {
            continue;
        };
        if mapping.protocol != PortMappingProtocol::UDP {
            continue;
        };
        if !mapping.enabled {
            continue;
        }
        return Some(mapping.external_port);
    }
}