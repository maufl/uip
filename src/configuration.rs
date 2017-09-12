use peer_information_base::{PeerInformationBase};

#[derive(Serialize, Deserialize)]
pub struct Configuration {
    pub id: String,
    pub pib: PeerInformationBase,
    pub relays: Vec<String>
}
