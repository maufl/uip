use std::net::SocketAddrV6;

use crate::Identifier;

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Peer {
    pub id: Identifier,
    pub addresses: Vec<SocketAddrV6>,
    pub relays: Vec<Identifier>,
}

#[allow(dead_code)]
impl Peer {
    pub fn new(id: Identifier, addresses: Vec<SocketAddrV6>, relays: Vec<Identifier>) -> Peer {
        Peer {
            id: id,
            addresses: addresses,
            relays: relays,
        }
    }
}
