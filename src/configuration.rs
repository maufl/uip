use peer_information_base::PeerInformationBase;
use id::Id;

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub id: Id,
    pub pib: PeerInformationBase,
    pub relays: Vec<String>,
    pub port: u16,
    pub ctl_socket: String,
}
