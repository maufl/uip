use std::net::{SocketAddr};
use std::collections::HashMap;
use std::sync::{Arc,RwLock,Mutex,RwLockReadGuard,RwLockWriteGuard};
use interfaces::{Interface,Kind};
use futures::{Future,Poll,Async,future};
use tokio_core::net::TcpStream;
use rustls::{ClientConfig,Certificate,ProtocolVersion};
use tokio_rustls::{ClientConfigExt};
use tokio_core::reactor::{Handle};
use std::io;

use transport::{Transport};
use peer_information_base::{Peer,PeerInformationBase};

struct LocalAddress {
    interface: String,
    internal_address: SocketAddr,
    external_address: Option<SocketAddr>,
}

impl LocalAddress {
    fn new<S: Into<String>>(interface: S, internal_address: SocketAddr, external_address: Option<SocketAddr>) -> LocalAddress {
        LocalAddress {
            interface: interface.into(),
            internal_address: internal_address,
            external_address: external_address,
        }
    }
}

pub struct InnerState {
    pub id: String,
    pub pib: PeerInformationBase,
    connections: HashMap<String, Vec<Transport>>,
    pub relays: Vec<String>,
    addresses: Vec<LocalAddress>,
    handle: Handle,
}

#[derive(Clone)]
pub struct State(pub Arc<RwLock<InnerState>>);

impl State {
    pub fn new(id: String, handle: Handle) -> State {
        State(Arc::new(RwLock::new(InnerState {
            id: id,
            pib: PeerInformationBase::new(),
            connections: HashMap::new(),
            relays: Vec::new(),
            addresses: Vec::new(),
            handle: handle,
        })))
    }

    fn read(&self) -> RwLockReadGuard<InnerState> {
        self.0.read().expect("Unable to acquire read lock on state")
    }

    fn write(&self) -> RwLockWriteGuard<InnerState> {
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
            for address in &interface.addresses {
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

    fn lookup_peer(&self, id: &str) -> Option<(SocketAddr, Certificate)> {
        self.read().pib
            .get_peer(id)
            .and_then(|peer| {
                if !peer.addresses.is_empty() {
                    Some( (peer.addresses[0], peer.user_certificate.clone()) )
                } else {
                    None
                }
            })
    }

    fn connect_to_relays(&self) {
        for relay in &self.read().relays {
            let (addr, cert) = match self.lookup_peer(relay) {
                Some(info) => info,
                None => continue
            };
            let relay = relay.clone();
            println!("Connecting to relay {}", relay);
            let future = self.connect(relay.clone(), addr, cert)
                .and_then(|_| future::ok(()) )
                .map_err(move |err| println!("Unable to connect to peer {}: {}", relay, err) );
            self.read().handle.spawn(future);
        }
    }

    fn add_connection(&self, id: String, conn: Transport) {
        self.write()
            .connections.entry(id).or_insert_with(Vec::new)
            .push(conn);
    }

    fn connect(&self, id: String, addr: SocketAddr, cert: Certificate) -> impl Future<Item=Transport, Error=io::Error> {
        let handle = self.read().handle.clone();
        let config = {
            let mut config = ClientConfig::new();
            config.versions = vec![ProtocolVersion::TLSv1_2];
            let _ = config.root_store.add(&cert);
            Arc::new(config)
        };
        let state = self.clone();
        let id2 = id.clone();
        TcpStream::connect(&addr, &handle)
            .and_then(move |stream| config.connect_async(id.as_ref(), stream) )
            .and_then(move |stream| {
                let conn = Transport::from_tls_client(stream);
                state.add_connection(id2, conn.clone());
                Ok(conn)
            })
    }

    pub fn add_relay_peer(&self,  name: String, address: SocketAddr, cert: Certificate) {
        self.write().pib
            .add_peer(name.clone(), Peer::new(name, vec![address], vec![], cert));
    }

    pub fn add_relay(&self, address: String) {
        self.write().relays.push(address)
    }

    pub fn send_to(&self, id: String, data: Vec<u8>) {
        if let Some(connections) =  self.read().connections.get(&id) {
            if let Some(connection) = connections.first() {
                connection.write_async(data)
            }
        }
    }
}

impl Future for State {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.discover_addresses();
        self.connect_to_relays();
        Ok(Async::NotReady)
    }
}
