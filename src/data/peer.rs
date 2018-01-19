use std::net::SocketAddr;

use Identifier;


#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Peer {
    pub id: Identifier,
    pub addresses: Vec<SocketAddr>,
    pub relays: Vec<Identifier>,
}

#[allow(dead_code)]
impl Peer {
    pub fn new(id: Identifier, addresses: Vec<SocketAddr>, relays: Vec<Identifier>) -> Peer {
        Peer {
            id: id,
            addresses: addresses,
            relays: relays,
        }
    }
}
