use std::net::SocketAddr;
use std::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use futures::{Future, IntoFuture, Poll, Async, future, Stream, Sink};
use futures::stream::iter_ok;
use futures::sync::mpsc::Sender;
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use bytes::BytesMut;
use openssl::hash::{hash2, MessageDigest};
use openssl::x509::X509;
use openssl::ssl::{SslConnectorBuilder, SslAcceptorBuilder, SslMethod, SslVerifyMode,
                   SSL_VERIFY_PEER};
use openssl::stack::Stack;
use tokio_openssl::{SslStream, SslConnectorExt, SslAcceptorExt};
use std::io;
use std::error::Error;

use peer_information_base::{Peer, PeerInformationBase};
use id::Id;
use network::{Transport, LocalAddress, SharedSocket};
use network::discovery::{discover_addresses, request_external_address, AddressDiscoveryError};
use network::change::Listener;

pub struct InnerState {
    pub id: Id,
    pub pib: PeerInformationBase,
    connections: HashMap<String, Vec<Transport>>,
    pub relays: Vec<String>,
    pub port: u16,
    handle: Handle,
    upstream: Sender<(String, u16, BytesMut)>,
    sockets: HashMap<LocalAddress, SharedSocket>,
}

#[derive(Clone)]
pub struct NetworkState(Arc<RwLock<InnerState>>);

impl NetworkState {
    pub fn new(
        id: Id,
        pib: PeerInformationBase,
        relays: Vec<String>,
        port: u16,
        handle: Handle,
        upstream: Sender<(String, u16, BytesMut)>,
    ) -> NetworkState {
        NetworkState(Arc::new(RwLock::new(InnerState {
            id: id,
            pib: pib,
            connections: HashMap::new(),
            relays: relays,
            port: port,
            handle: handle,
            upstream: upstream,
            sockets: HashMap::new(),
        })))
    }
    pub fn read(&self) -> RwLockReadGuard<InnerState> {
        self.0.read().expect("Unable to acquire read lock on state")
    }

    fn write(&self) -> RwLockWriteGuard<InnerState> {
        self.0.write().expect(
            "Unable to acquire write lock on state",
        )
    }

    pub fn spawn<F: Future<Item = (), Error = ()> + 'static>(&self, f: F) {
        self.read().handle.spawn(f)
    }

    fn open_sockets(&self) {
        let state = self.clone();
        let state2 = self.clone();
        let state3 = self.clone();
        let state4 = self.clone();
        let port = self.read().port;
        let task = discover_addresses()
            .map_err(|err| warn!("Unable to enumerate local addresses: {}", err))
            .map(move |mut addr| {
                addr.internal_address.set_port(port);
                addr
            })
            .collect()
            .and_then(move |addresses| {
                let stale: Vec<LocalAddress> = state
                    .read()
                    .sockets
                    .keys()
                    .filter(|address| !addresses.contains(address))
                    .cloned()
                    .collect();
                for address in stale {
                    info!("Closing stale socket {}", address.internal_address);
                    if let Some(socket) = state.write().sockets.remove(&address) {
                        info!("Closed socket {}", address.internal_address);
                    } else {
                        info!("No socket found for {}", address.internal_address);
                    };
                }
                iter_ok(addresses)
                    .filter(move |address| !state.read().sockets.contains_key(&address))
                    .and_then(move |address| {
                        request_external_address(address.clone(), &state2.read().handle)
                            .or_else(|err| {
                                match err {
                                    AddressDiscoveryError::UnsupportedAddress(_) => {}
                                    _ => warn!("Error while requesting external address: {}", err),
                                };
                                Ok(address)
                            })
                    })
                    .for_each(move |address| {
                        state3.open_socket(address).map_err(|err| {
                            warn!("Error while opening socket: {}", err)
                        });
                        Ok(())
                    })
            });
        self.spawn(task);
    }

    fn open_socket(&self, address: LocalAddress) -> io::Result<()> {
        let mut addr: SocketAddr = address.internal_address;
        addr.set_port(self.read().port);
        debug!("Opening new socket on address {}", addr);
        let socket = SharedSocket::bind(&addr, &self.read().handle)?;
        self.write().sockets.insert(address, socket.clone());
        self.listen(socket);
        Ok(())
    }

    fn observe_network_changes(&self) {
        let state = self.clone();
        let listener = match Listener::new(&self.read().handle) {
            Ok(l) => l,
            Err(err) => return warn!("Unable to listen for network changes: {}", err),
        };
        let task = listener
            .debounce(Duration::from_millis(2000))
            .for_each(move |_| {
                info!("Network changed");
                state.open_sockets();
                Ok(())
            })
            .map_err(|err| {
                warn!("Error while listening for network changes: {}", err)
            });
        self.spawn(task);
    }

    fn lookup_peer_address(&self, id: &str) -> Option<SocketAddr> {
        self.read()
            .pib
            .get_peer(id)
            .and_then(|peer| peer.addresses.first())
            .cloned()
    }

    pub fn add_peer_address(&mut self, id: String, addr: SocketAddr) {
        self.write().pib.add_peer(
            id.clone(),
            Peer::new(id, vec![addr], vec![]),
        )
    }

    pub fn add_relay(&mut self, id: String) {
        self.write().relays.push(id);
    }

    pub fn connect_to_relays(&self) {
        for relay in &self.read().relays {
            let addr = match self.lookup_peer_address(relay) {
                Some(info) => info,
                None => continue,
            };
            let relay = relay.clone();
            println!("Connecting to relay {}", relay);
            let future = self.open_transport(relay.clone(), addr)
                .and_then(|_| future::ok(()))
                .map_err(move |err| {
                    println!("Unable to connect to peer {}: {}", relay, err)
                });
            self.spawn(future);
        }
    }

    fn listen(&self, listener: SharedSocket) {
        let state = self.clone();
        let acceptor = {
            let id = &self.read().id;
            let empty_chain: Stack<X509> = Stack::new().expect("unable to build empty cert chain");
            let mut builder = SslAcceptorBuilder::mozilla_modern(
                SslMethod::dtls(),
                &id.key,
                id.cert.as_ref(),
                empty_chain.as_ref(),
            ).expect("Unable to build new SSL acceptor");
            builder.set_verify_callback(SSL_VERIFY_PEER, |_valid, context| {
                context.current_cert().is_some()
            });
            builder.build()
        };
        let local_addr = listener.local_addr().expect("Not bound to a local address");
        self.write().port = local_addr.port();
        let task = listener
            .incoming()
            .for_each(move |stream| {
                debug!(
                    "Accepting new UDP connection from: {}",
                    stream.local_addr().expect("Impossible")
                );
                let state2 = state.clone();
                acceptor
                    .accept_async(stream)
                    .map_err(|err| err.description().to_string())
                    .and_then(move |connection| state2.accept_connection(connection))
                    .then(|result| {
                        if let Err(err) = result {
                            println!("Error while accepting a new TLS connection: {}", err)
                        }
                        Ok(())
                    })
            })
            .map(move |_| info!("UDP socket on {} was closed", local_addr))
            .map_err(move |_err| {
                info!("DTLS listener on {} died unexpectedly", local_addr)
            });
        self.spawn(task);
    }

    fn accept_connection<S>(&self, connection: SslStream<S>) -> Result<(), String>
    where
        S: AsyncRead + AsyncWrite + 'static,
    {
        let id = {
            let session = connection.get_ref().ssl();
            let x509 = session.peer_certificate().ok_or(
                "Client did not provide a certificate",
            )?;
            let pub_key = x509.public_key().map_err(|err| {
                format!("Unable to get public key from certificate: {}", err)
            })?;
            let pub_key_pem = pub_key.public_key_to_pem().map_err(|err| {
                format!("Error while serializing public key to pem: {}", err)
            })?;
            hash2(MessageDigest::sha256(), &pub_key_pem)
                .map_err(|err| err.description().to_string())?
                .iter()
                .map(|byte| format!("{:02X}", byte))
                .collect::<Vec<String>>()
                .join("")
        };
        let transport = Transport::from_tls_stream(self.clone(), connection, id.clone());
        self.add_connection(id, transport);
        Ok(())
    }

    fn add_connection(&self, id: String, conn: Transport) {
        self.write()
            .connections
            .entry(id)
            .or_insert_with(Vec::new)
            .push(conn);
    }

    fn connect(&self, remote_id: String) -> impl Future<Item = Transport, Error = io::Error> {
        let state = self.clone();
        self.lookup_peer_address(&remote_id)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "peer address not found")
            })
            .into_future()
            .and_then(move |addr| state.open_transport(remote_id, addr))
    }

    fn open_transport(
        &self,
        id: String,
        addr: SocketAddr,
    ) -> impl Future<Item = Transport, Error = io::Error> {
        let connector = {
            let mut builder = SslConnectorBuilder::new(SslMethod::dtls()).expect(
                "Unable to build new SSL connector",
            );
            builder.set_verify(SslVerifyMode::empty());
            builder.set_certificate(&self.read().id.cert).expect(
                "Unable to get reference to client certificate",
            );
            builder.set_private_key(&self.read().id.key).expect(
                "Unable to get a reference to the client key",
            );
            builder.build()
        };
        let state = self.clone();
        let id2 = id.clone();
        self.read()
            .sockets
            .values()
            .next()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotConnected, "Currently not connected")
            })
            .and_then(|socket| socket.connect(addr))
            .into_future()
            .and_then(move |stream| {
                connector.connect_async(id.as_ref(), stream).map_err(|err| {
                    io::Error::new(io::ErrorKind::Other, format!("Handshake error: {:?}", err))
                })
            })
            .and_then(move |stream| {
                let conn = Transport::from_tls_stream(state.clone(), stream, id2.clone());
                state.add_connection(id2, conn.clone());
                Ok(conn)
            })
    }

    pub fn deliver_frame(&self, host_id: String, channel_id: u16, data: BytesMut) {
        let task = self.read()
            .upstream
            .clone()
            .send((host_id, channel_id, data))
            .map(|_| ())
            .map_err(|err| warn!("Failed to pass message to upstream: {}", err));
        self.spawn(task);
    }

    pub fn send_frame(&self, host_id: String, channel_id: u16, data: BytesMut) {
        let task = self.get_connection(host_id)
            .and_then(move |connection| {
                connection.send_frame(channel_id, data).map_err(|_| {
                    io::Error::new(io::ErrorKind::BrokenPipe, "unable to forward frame")
                })
            })
            .map(|_| {})
            .map_err(|err| warn!("Error while sending frame: {}", err));
        self.spawn(task);
    }

    pub fn get_connection(
        &self,
        host_id: String,
    ) -> impl Future<Item = Transport, Error = io::Error> {
        let state = self.clone();
        self.read()
            .connections
            .get(&host_id)
            .and_then(|cs| cs.first())
            .cloned()
            .ok_or(())
            .into_future()
            .or_else(move |_| state.connect(host_id))
    }
}

impl Future for NetworkState {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.open_sockets();
        self.connect_to_relays();
        self.observe_network_changes();
        Ok(Async::NotReady)
    }
}
