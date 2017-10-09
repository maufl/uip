use std::net::SocketAddr;
use peer_information_base::{PeerInformationBase};
use id::Id;

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub id: Id,
    pub pib: PeerInformationBase,
    pub relays: Vec<String>,
    pub certificate: String,
    pub key: String,
    pub listen: SocketAddr,
    pub ctl_socket: String
}
