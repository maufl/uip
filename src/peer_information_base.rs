use std::net::SocketAddr;
use std::collections::HashMap;

use Identifier;

#[allow(dead_code)]
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

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct PeerInformationBase {
    peers: HashMap<Identifier, Peer>,
}

#[allow(dead_code)]
impl PeerInformationBase {
    pub fn new() -> PeerInformationBase {
        PeerInformationBase { peers: HashMap::new() }
    }

    pub fn add_peer(&mut self, id: Identifier, peer: Peer) {
        self.peers.insert(id, peer);
    }

    pub fn get_peer(&self, id: &Identifier) -> Option<&Peer> {
        self.peers.get(id)
    }

    pub fn lookup_peer_address(&self, id: &Identifier) -> Option<SocketAddr> {
        self.get_peer(id)
            .and_then(|peer| peer.addresses.first())
            .cloned()
    }

    pub fn add_peer_address(&mut self, id: Identifier, addr: SocketAddr) {
        self.peers
            .entry(id)
            .or_insert_with(|| Peer::new(id, vec![], vec![]))
            .addresses
            .push(addr)
    }
}
