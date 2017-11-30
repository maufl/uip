use interfaces::{Interface,Kind,Result};
use interfaces::flags::{IFF_RUNNING};

use network::{LocalAddress};

pub fn discover_addresses() -> Result<Vec<LocalAddress>> {
    let interfaces = Interface::get_all()?;
    let mut addresses = Vec::new();
    for interface in interfaces {
        if interface.is_loopback() || !interface.flags.contains(IFF_RUNNING) {
            continue
        }
        for address in &interface.addresses {
            let addr = match address.addr {
                Some(addr) => addr,
                None => continue,
            };
            if address.kind == Kind::Ipv4 || address.kind == Kind::Ipv6 {
                addresses.push(LocalAddress::new(interface.name.clone(), addr, None))
            }
        }
    };
    Ok(addresses)
}
