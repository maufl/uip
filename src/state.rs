use std::net::{SocketAddr};
use std::collections::HashMap;
use std::sync::{Arc,RwLock,RwLockReadGuard,RwLockWriteGuard};
use interfaces::{Interface,Kind};
use futures::{Future,Poll,Async,future,Stream,Sink};
use futures::sync::mpsc::{channel,Sender};
use tokio_core::net::{TcpListener,TcpStream};
use tokio_core::reactor::{Handle};
use tokio_uds::{UnixListener};
use tokio_io::{AsyncRead};
use bytes::BytesMut;
use openssl::hash::{hash2,MessageDigest};
use openssl::x509::{X509};
use openssl::ssl::{SslConnectorBuilder,SslAcceptorBuilder, SslMethod, SslVerifyMode,SSL_VERIFY_PEER};
use openssl::stack::Stack;
use tokio_openssl::{SslStream,SslConnectorExt,SslAcceptorExt};
use std::io;
use std::path::Path;
use std::fs;
use std::error::Error;

use transport::{Transport};
use peer_information_base::{Peer,PeerInformationBase};
use configuration::{Configuration};
use unix_socket::{ControlProtocolCodec,Frame};
use id::Id;

#[allow(dead_code)]
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
    pub id: Id,
    pub pib: PeerInformationBase,
    connections: HashMap<String, Vec<Transport>>,
    pub relays: Vec<String>,
    addresses: Vec<LocalAddress>,
    sockets: HashMap<(String, u16), Sender<BytesMut>>,
    handle: Handle,
    port: u16,
    ctl_socket: String,
}



#[derive(Clone)]
pub struct State(pub Arc<RwLock<InnerState>>);

impl State {
    pub fn from_configuration(config: Configuration, handle: Handle) -> Result<State, String> {
        Ok(State(Arc::new(RwLock::new(InnerState {
            id: config.id,
            pib: config.pib,
            connections: HashMap::new(),
            relays: config.relays,
            addresses: Vec::new(),
            sockets: HashMap::new(),
            handle: handle,
            port: config.port,
            ctl_socket: config.ctl_socket
        }))))
    }

    pub fn from_id(id: Id, handle: Handle) -> Result<State, String> {
        let hash = id.hash.clone();
        Ok(State(Arc::new(RwLock::new(InnerState {
            id: id,
            pib: PeerInformationBase::new(),
            connections: HashMap::new(),
            relays: Vec::new(),
            addresses: Vec::new(),
            sockets: HashMap::new(),
            handle: handle,
            port: 0,
            ctl_socket: format!("/run/user/1000/uip/{}.ctl", hash)
        }))))
    }

    pub fn to_configuration(&self) -> Configuration {
        let state = self.read();
        Configuration {
            id: state.id.clone(),
            pib: state.pib.clone(),
            relays: state.relays.clone(),
            port: state.port.clone(),
            ctl_socket: state.ctl_socket.clone()
        }
    }

    pub fn read(&self) -> RwLockReadGuard<InnerState> {
        self.0.read().expect("Unable to acquire read lock on state")
    }

    pub fn handle(&self) -> Handle {
        self.read().handle.clone()
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

    fn lookup_peer_address(&self, id: &str) -> Option<SocketAddr> {
        self.read().pib
            .get_peer(id)
            .and_then(|peer| {
                if !peer.addresses.is_empty() {
                    Some(peer.addresses[0])
                } else {
                    None
                }
            })
    }

    pub fn add_peer_address(&self, id: String, addr: SocketAddr) {
        self.write().pib
            .add_peer(id.clone(), Peer::new(id, vec![addr], vec![]) )
    }

    pub fn add_relay(&self, id: String) {
        self.write().relays.push(id);
    }

    pub fn connect_to_relays(&self) {
        for relay in &self.read().relays {
            let addr = match self.lookup_peer_address(relay) {
                Some(info) => info,
                None => continue
            };
            let relay = relay.clone();
            println!("Connecting to relay {}", relay);
            let future = self.connect(relay.clone(), addr)
                .and_then(|_| future::ok(()) )
                .map_err(move |err| println!("Unable to connect to peer {}: {}", relay, err) );
            self.read().handle.spawn(future);
        }
    }

    fn open_ctl_socket(&self) {
        let state = self.clone();
        let done = UnixListener::bind(&self.read().ctl_socket, &self.read().handle)
            .expect("Unable to open unix control socket")
            .incoming().for_each(move |(stream, _addr)| {
                let state = state.clone();
                let socket = stream.framed(ControlProtocolCodec);
                socket.into_future().and_then(move |(frame,socket)| {
                    let (host_id, channel_id) = match frame {
                        Some(Frame::Connect(host_id, channel_id)) => (host_id, channel_id),
                        _ => return Err((io::Error::new(io::ErrorKind::Other, "Unexpected message"), socket))
                    };
                    let (sink, stream) = socket.split();
                    let (sender, receiver) = channel::<BytesMut>(10);
                    state.read().handle.spawn(receiver.forward(sink.sink_map_err(|_|()).with(|data| Ok(Frame::Data(data)) )).map(|_| ()).map_err(|_| ()));
                    state.write().sockets.insert( (host_id.clone(), channel_id), sender);
                    let state2 = state.clone();
                    let done = stream.for_each(move |buf| {
                        match buf {
                            Frame::Data(buf) => state2.send_frame(host_id.clone(), channel_id, buf),
                            Frame::Connect(_,_) => {}
                        };
                        future::ok(())
                    }).map_err(|_| ());
                    state.read().handle.spawn(done);
                    Ok(())
                }).map_err(|e| e.0 )
            }).map_err(|e| println!("Control socket was closed: {}", e) );
        self.read().handle.spawn(done);
    }

    fn listen(&self) {
        let state = self.clone();
        let acceptor = {
            let id = &self.read().id;
            let empty_chain: Stack<X509> = Stack::new().expect("unable to build empty cert chain");
            let mut builder = SslAcceptorBuilder::mozilla_modern(SslMethod::tls(), &id.key, id.cert.as_ref(), empty_chain.as_ref()).expect("Unable to build new SSL acceptor");
            builder.builder_mut().set_verify_callback(SSL_VERIFY_PEER, |_valid, context| context.current_cert().is_some() );
            builder.build()
        };
        let addr: SocketAddr = format!("0.0.0.0:{}", &self.read().port).parse().expect("Unable to parse local address");
        let listener = TcpListener::bind(&addr, &self.read().handle).expect("Unable to bind to local address");
        let local_addr = listener.local_addr().expect("Not bound to a local address");
        println!("Listening on address {}", local_addr);
        self.write().port = local_addr.port();
        let task = listener.incoming()
            .for_each(move |(stream, _address)| {
                let state2 = state.clone();
                acceptor.accept_async(stream)
                    .map_err(|err| err.description().to_string() )
                    .and_then(move |connection| state2.accept_connection(connection) )
                    .then(|result| {
                        if let Err(err) = result {
                            println!("Error while accepting a new TLS connection: {}", err)
                        }
                        Ok(())
                    })
            }).map_err(|err| println!("TLS listener died: {}", err) );
        self.read().handle.spawn(task);
    }

    fn accept_connection(&self, connection: SslStream<TcpStream>) -> Result<(), String> {
        let id = {
            let session = connection.get_ref().ssl();
            let x509 = session.peer_certificate()
                .ok_or("Client did not provide a certificate")?;
            let pub_key = x509.public_key()
                .map_err(|err| format!("Unable to get public key from certificate: {}", err) )?;
            let pub_key_pem = pub_key.public_key_to_pem()
                .map_err(|err| format!("Error while serializing public key to pem: {}", err) )?;
            hash2(MessageDigest::sha256(), &pub_key_pem).map_err(|err| err.description().to_string() )?
            .iter().map(|byte| format!("{:02X}", byte) ).collect::<Vec<String>>().join("")
        };
        let transport = Transport::from_tls_stream(self.clone(), connection, id.clone());
        self.add_connection(id, transport);
        Ok(())
    }

    fn add_connection(&self, id: String, conn: Transport) {
        self.write()
            .connections.entry(id).or_insert_with(Vec::new)
            .push(conn);
    }

    fn connect(&self, id: String, addr: SocketAddr) -> impl Future<Item=Transport, Error=io::Error> {
        let handle = self.read().handle.clone();
        let connector = {
            let mut builder = SslConnectorBuilder::new(SslMethod::tls()).expect("Unable to build new SSL connector");
            builder.builder_mut().set_verify(SslVerifyMode::empty());
            builder.builder_mut().set_certificate(&self.read().id.cert).expect("Unable to get reference to client certificate");
            builder.builder_mut().set_private_key(&self.read().id.key).expect("Unable to get a reference to the client key");
            builder.build()
        };
        let state = self.clone();
        let id2 = id.clone();
        TcpStream::connect(&addr, &handle)
            .and_then(move |stream| {
                connector.connect_async(id.as_ref(), stream)
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("Handshake error: {:?}", err)) )
            })
            .and_then(move |stream| {
                let conn = Transport::from_tls_stream(state.clone(), stream, id2.clone());
                state.add_connection(id2, conn.clone());
                Ok(conn)
            })
    }

    pub fn send_frame(&self, host_id: String, channel_id: u16, data: BytesMut) {
        if let Some(connections) = self.read().connections.get(&host_id) {
            if let Some(connection) = connections.first() {
                let task = connection.send_frame(channel_id, data)
                    .then(|result| {
                        match result {
                            Ok(_) => println!("Successfully sent frame"),
                            Err(ref err) => println!("Error while sending frame: {}", err)
                        };
                        Ok(())
                    });
                self.handle().spawn(task);
            } else {
                println!("No connection for host id {}", host_id)
            }
        } else {
            println!("No connection for host id {}", host_id)
        }
    }

    pub fn deliver_frame(&self, host_id: String, channel_id: u16, data: BytesMut) {
        println!("Received new data from {} in channel {}: {:?}", host_id, channel_id, data);
        if let Some(socket) = self.read().sockets.get( &(host_id, channel_id) ) {
            self.read().handle.spawn(socket.clone().send(data).map(|_| ()).map_err(|_| ()));
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        let path = &self.read().ctl_socket;
        if Path::new(path).exists() {
            let _ = fs::remove_file(path);
        };
    }
}

impl Future for State {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.discover_addresses();
        self.connect_to_relays();
        self.open_ctl_socket();
        self.listen();
        Ok(Async::NotReady)
    }
}
