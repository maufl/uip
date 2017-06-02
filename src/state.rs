use std::net::{SocketAddr};
use std::collections::HashMap;
use std::sync::{Arc,RwLock,Mutex};
use igd::{search_gateway_from,PortMappingProtocol};
use interfaces::{Interface,Kind};
use futures::{Future,IntoFuture};
use tokio_core::net::TcpStream;
use rustls::{Session,ClientConfig,Certificate,ProtocolVersion,ClientSession};
use tokio_io::{AsyncRead,AsyncWrite};
use tokio_rustls::{ClientConfigExt,TlsStream};
use tokio_core::reactor::{Core,Handle};
use std::error::Error;

use connection::{Connection,SharedConnection};

pub struct Peer {
    id: String,
    addresses: Vec<SocketAddr>,
    relays: Vec<String>,
    user_certificate: Certificate,
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

pub struct PeerInformationBase {
    peers: HashMap<String, Peer>
}

struct LocalAddress {
    interface: String,
    internalAddress: SocketAddr,
    externalAddress: Option<SocketAddr>,
}

impl LocalAddress {
    fn new<S: Into<String>>(interface: S, internalAddress: SocketAddr, externalAddress: Option<SocketAddr>) -> LocalAddress {
        LocalAddress {
            interface: interface.into(),
            internalAddress: internalAddress,
            externalAddress: externalAddress,
        }
    }
}
impl PeerInformationBase {
    fn new() -> PeerInformationBase {
        PeerInformationBase {
            peers: HashMap::new()
        }
    }

    pub fn add_peer(&mut self, id: String, peer: Peer) {
        self.peers.insert(id, peer);
    }

    pub fn get_peer(&self, id: &String) -> Option<&Peer> {
        self.peers.get(id)
    }
}

trait AsyncStream: AsyncRead + AsyncWrite {
}

pub struct State {
    pub id: String,
    pub pib: Arc<RwLock<PeerInformationBase>>,
    connections: Arc<RwLock<HashMap<String, Vec<SharedConnection>>>>,
    pub relays: Arc<RwLock<Vec<String>>>,
    addresses: Arc<RwLock<Vec<LocalAddress>>>,
    core: Core,
}


impl State {
    pub fn new(id: String) -> State {
        State {
            id: id,
            pib: Arc::new(RwLock::new(PeerInformationBase::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            relays: Arc::new(RwLock::new(Vec::new())),
            addresses: Arc::new(RwLock::new(Vec::new())),
            core: Core::new().unwrap(),
        }
    }

    fn discover_addresses(&self) -> () {
        let interfaces = match Interface::get_all()  {
            Ok(i) => i,
            Err(_) => return,
        };
        self.addresses
            .write()
            .expect("Unable to acquire write lock")
            .clear();
        for interface in interfaces {
            if interface.is_loopback() || !interface.is_up() {
                continue
            }
            for address in interface.addresses.iter() {
                let addr = match address.addr {
                    Some(addr) => addr,
                    None => continue,
                };
                if address.kind == Kind::Ipv4 || address.kind == Kind::Ipv6 {
                    self.addresses
                        .write()
                        .expect("Unable to acquire write lock.")
                        .push(LocalAddress::new(interface.name.clone(), addr, None))
                }
            }
        };
    }

    fn lookup_peer(&self, id: &String) -> Option<(SocketAddr, Certificate)> {
        match self.pib.read() {
            Ok(pib) => match pib.get_peer(id) {
                Some(peer) => if peer.addresses.len() > 0 {
                    Some( (peer.addresses[0].clone(), peer.user_certificate.clone()) )
                } else {
                    None
                },
                None => None,
            },
            Err(_) => None,
        }
    }

    fn add_connection(&self, id: &String, conn: SharedConnection) -> Result<SharedConnection, String> {
        let connections = match self.connections.write() {
            Ok(conns) => conns,
            Err(_) => return Err("Failed to acquire lock for connections".to_owned())
        };
        match connections.get_mut(&id) {
            Some(vec) => { vec.push(conn); }
            None => { connections.insert(id.clone(), vec![conn]); }
        };
        Ok(conn)
    }

    fn connect(&self, id: String) -> impl Future<Item=SharedConnection, Error=String> {
        self.lookup_peer(&id).ok_or("Failed to find peer information".to_owned())
            .and_then(|(addr, cert)| {
                Ok((addr, cert, self.core.handle()))
            })
            .into_future()
            .and_then(move |(addr, cert, handle)| {
                let config = {
                    let mut config = ClientConfig::new();
                    config.versions = vec![ProtocolVersion::TLSv1_2];
                    config.root_store.add(&cert);
                    Arc::new(config)
                };
                TcpStream::connect(&addr, &handle)
                    .and_then(move |stream| config.connect_async(id.as_ref(), stream) )
                    .and_then(move |stream| Arc::new(Mutex::new(Connection::from_tls_client(stream))) )
                    .map_err(|err| err.description().to_owned() )
            })
            .and_then(|conn| {
                self.add_connection(&id, conn)
            })
    }
}
