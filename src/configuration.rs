use peer_information_base::PeerInformationBase;
use Identity;

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub id: Identity,
    pub pib: PeerInformationBase,
    pub relays: Vec<String>,
    pub port: u16,
    pub ctl_socket: String,
}
