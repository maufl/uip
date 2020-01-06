use crate::data::PeerInformationBase;
use crate::{Identity, Identifier};

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub port: u16,
    pub ctl_socket: String,
    pub relays: Vec<Identifier>,
    pub id: Identity,
    pub pib: PeerInformationBase,
}
