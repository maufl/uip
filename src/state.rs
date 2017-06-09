use std::net::{SocketAddr};
use std::collections::HashMap;
use std::sync::{Arc,RwLock,Mutex};
use igd::{search_gateway_from,PortMappingProtocol};
use interfaces::{Interface,Kind};
use futures::{Future,IntoFuture,future};
use tokio_core::net::TcpStream;
use rustls::{Session,ClientConfig,Certificate,ProtocolVersion,ClientSession};
use tokio_io::{AsyncRead,AsyncWrite};
use tokio_rustls::{ClientConfigExt,TlsStream};
use tokio_core::reactor::{Core,Handle};
use std::error::Error;
use std::io;

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

pub struct InnerState {
    pub id: String,
    pub pib: Arc<RwLock<PeerInformationBase>>,
    connections: Arc<RwLock<HashMap<String, Vec<SharedConnection>>>>,
    pub relays: Arc<RwLock<Vec<String>>>,
    addresses: Arc<RwLock<Vec<LocalAddress>>>,
    core: Core,
}

pub struct State(Arc<InnerState>);


impl State {
    pub fn new(id: String) -> State {
        State(Arc::new(InnerState {
            id: id,
            pib: Arc::new(RwLock::new(PeerInformationBase::new())),
            connections: Arc::new(RwLock::new(HashMap::new())),
            relays: Arc::new(RwLock::new(Vec::new())),
            addresses: Arc::new(RwLock::new(Vec::new())),
            core: Core::new().unwrap(),
        }))
    }

    fn discover_addresses(&self) -> () {
        let interfaces = match Interface::get_all()  {
            Ok(i) => i,
            Err(_) => return,
        };
        self.0.addresses
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
                    self.0.addresses
                        .write()
                        .expect("Unable to acquire write lock.")
                        .push(LocalAddress::new(interface.name.clone(), addr, None))
                }
            }
        };
    }

    fn lookup_peer(&self, id: &String) -> Option<(SocketAddr, Certificate)> {
        self.0.pib
            .read().expect("Failed to acquire read lock for peer information base")
            .get_peer(id)
            .and_then(|peer| {
                if peer.addresses.len() > 0 {
                    Some( (peer.addresses[0].clone(), peer.user_certificate.clone()) )
                } else {
                    None
                }
            })
    }

    fn connect_to_relays(&self) {
        for relay in self.0.relays.read().expect("Failed to acquire read lock for relay list").into_iter() {
            let (addr, cert) = match self.lookup_peer(&relay) {
                Some(info) => info,
                None => continue
            };
            let future = self.connect(relay.clone(), addr, cert)
                .and_then(move |conn| { let state = self.clone(); future::ok(state.add_connection(relay.clone(), conn)) } )
                .map_err(move |err| println!("Unable to connect to peer {}", relay.clone()) );
            self.0.core.handle().spawn(future);
        }
    }

    fn add_connection(&self, id: String, conn: SharedConnection) {
        self.0.connections
            .write().expect("Failed to acquire write lock for connections map")
            .entry(id).or_insert(vec![])
            .push(conn);
    }

    fn connect(&self, id: String, addr: SocketAddr, cert: Certificate) -> impl Future<Item=SharedConnection, Error=io::Error> {
        let handle = self.0.core.handle();
        let config = {
            let mut config = ClientConfig::new();
            config.versions = vec![ProtocolVersion::TLSv1_2];
            config.root_store.add(&cert);
            Arc::new(config)
        };
        TcpStream::connect(&addr, &handle)
            .and_then(move |stream| config.connect_async(id.as_ref(), stream) )
            .and_then(|stream| Ok(Arc::new(Mutex::new(Connection::from_tls_client(stream)))) )
    }
}
 
