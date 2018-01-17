use std::net::SocketAddr;
use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Peer {
    pub id: String,
    pub addresses: Vec<SocketAddr>,
    pub relays: Vec<String>,
}

#[allow(dead_code)]
impl Peer {
    pub fn new(id: String, addresses: Vec<SocketAddr>, relays: Vec<String>) -> Peer {
        Peer {
            id: id,
            addresses: addresses,
            relays: relays,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct PeerInformationBase {
    peers: HashMap<String, Peer>,
}

#[allow(dead_code)]
impl PeerInformationBase {
    pub fn new() -> PeerInformationBase {
        PeerInformationBase { peers: HashMap::new() }
    }

    pub fn add_peer(&mut self, id: String, peer: Peer) {
        self.peers.insert(id, peer);
    }

    pub fn get_peer(&self, id: &str) -> Option<&Peer> {
        self.peers.get(id)
    }

    pub fn lookup_peer_address(&self, id: &str) -> Option<SocketAddr> {
        self.get_peer(id)
            .and_then(|peer| peer.addresses.first())
            .cloned()
    }

    pub fn add_peer_address(&mut self, id: String, addr: SocketAddr) {
        self.peers
            .entry(id.clone())
            .or_insert_with(|| Peer::new(id, vec![], vec![]))
            .addresses
            .push(addr)
    }
}
