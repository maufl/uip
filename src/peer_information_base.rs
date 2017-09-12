use std::net::{SocketAddr};
use std::collections::HashMap;
use rustls::{Certificate};
use serde::{Serialize,Deserialize,Serializer,Deserializer};
use openssl::x509::{X509};

#[derive(Serialize,Deserialize)]
pub struct Peer {
    pub id: String,
    pub addresses: Vec<SocketAddr>,
    pub relays: Vec<String>,
    #[serde(deserialize_with="deserialize_certificate",serialize_with="serialize_certificate")]
    pub user_certificate: Certificate,
}

fn serialize_certificate<S>(cert: &Certificate, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
    let bytes = cert.0.as_slice();
    let x509 = X509::from_der(bytes).unwrap();
    let pem = x509.to_pem().unwrap();
    let string = String::from_utf8(pem).unwrap();
    string.serialize(serializer)
}

fn deserialize_certificate<'de, D>(deserializer: D) -> Result<Certificate, D::Error> where D: Deserializer< 'de> {
    let string = String::deserialize(deserializer)?;
    let x509 = X509::from_pem(string.as_bytes()).unwrap();
    let der = x509.to_der().unwrap();
    Ok(Certificate(der))
}

impl Peer {
    pub fn new(id: String, addresses: Vec<SocketAddr>, relays: Vec<String>, cert: Certificate) -> Peer {
        Peer {
            id: id,
            addresses: addresses,
            relays: relays,
            user_certificate: cert,
        }
    }
}

#[derive(Serialize,Deserialize)]
pub struct PeerInformationBase {
    peers: HashMap<String, Peer>
}

impl PeerInformationBase {
    pub fn new() -> PeerInformationBase {
        PeerInformationBase {
            peers: HashMap::new()
        }
    }

    pub fn add_peer(&mut self, id: String, peer: Peer) {
        self.peers.insert(id, peer);
    }

    pub fn get_peer(&self, id: &str) -> Option<&Peer> {
        self.peers.get(id)
    }
}
