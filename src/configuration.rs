use data::PeerInformationBase;
use {Identity, Identifier};

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub id: Identity,
    pub pib: PeerInformationBase,
    pub relays: Vec<Identifier>,
    pub port: u16,
    pub ctl_socket: String,
}
