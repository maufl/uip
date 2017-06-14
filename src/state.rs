use std::net::{SocketAddr};
use std::collections::HashMap;
use std::sync::{Arc,RwLock,Mutex,RwLockReadGuard,RwLockWriteGuard};
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
    pub pib: PeerInformationBase,
    connections: HashMap<String, Vec<SharedConnection>>,
    pub relays: Vec<String>,
    addresses: Vec<LocalAddress>,
    core: Core,
}

#[derive(Clone)]
pub struct State(pub Arc<RwLock<InnerState>>);

impl State {
    pub fn new(id: String) -> State {
        State(Arc::new(RwLock::new(InnerState {
            id: id,
            pib: PeerInformationBase::new(),
            connections: HashMap::new(),
            relays: Vec::new(),
            addresses: Vec::new(),
            core: Core::new().unwrap(),
        })))
    }

    pub fn read(&self) -> RwLockReadGuard<InnerState> {
        self.0.read().expect("Unable to acquire read lock on state")
    }

    pub fn write(&self) -> RwLockWriteGuard<InnerState> {
        self.0.write().expect("Unable to acquire write lock on state")
    }

    fn discover_addresses(&self) -> () {
        let mut state = self.write();
        let interfaces = match Interface::get_all()  {
            Ok(i) => i,
            Err(_) => return,
        };
        state.addresses.clear();
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
                    state.addresses
                        .push(LocalAddress::new(interface.name.clone(), addr, None))
                }
            }
        };
    }

    fn lookup_peer(&self, id: &String) -> Option<(SocketAddr, Certificate)> {
        self.read().pib
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
        for relay in self.read().relays.iter() {
            let (addr, cert) = match self.lookup_peer(&relay) {
                Some(info) => info,
                None => continue
            };
            let relay = relay.clone();
            let relay2 = relay.clone();
            let state = self.clone();
            let future = self.connect(relay.clone(), addr, cert)
                .and_then(move |conn| future::ok(state.add_connection(relay, conn)) )
                .map_err(move |err| println!("Unable to connect to peer {}", relay2) );
            self.0.read().expect("Unable to acquire read lock").core.handle().spawn(future);
        }
    }

    fn add_connection(&self, id: String, conn: SharedConnection) {
        self.write()
            .connections.entry(id).or_insert(vec![])
            .push(conn);
    }

    fn connect(&self, id: String, addr: SocketAddr, cert: Certificate) -> impl Future<Item=SharedConnection, Error=io::Error> {
        let handle = self.read().core.handle();
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
 
