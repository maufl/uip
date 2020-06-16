use std::collections::HashMap;
use std::net::SocketAddrV6;

use super::Peer;
use crate::Identifier;

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct PeerInformationBase {
    peers: HashMap<Identifier, Peer>,
}

#[allow(dead_code)]
impl PeerInformationBase {
    pub fn new() -> PeerInformationBase {
        PeerInformationBase {
            peers: HashMap::new(),
        }
    }

    pub fn add_peer(&mut self, id: Identifier, peer: Peer) {
        self.peers.insert(id, peer);
    }

    pub fn get_peer(&self, id: &Identifier) -> Option<&Peer> {
        self.peers.get(id)
    }

    pub fn lookup_peer_address(&self, id: &Identifier) -> Option<SocketAddrV6> {
        self.get_peer(id)
            .and_then(|peer| peer.addresses.first())
            .cloned()
    }

    pub fn add_peer_address(&mut self, id: Identifier, addr: SocketAddrV6) {
        self.peers
            .entry(id)
            .or_insert_with(|| Peer::new(id, vec![], vec![]))
            .addresses
            .push(addr)
    }
}
