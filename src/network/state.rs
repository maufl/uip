use std::net::{SocketAddr, IpAddr};
use std::time::Duration;
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::{RefCell, Ref, RefMut};
use futures::{Future, IntoFuture, Poll, Async, future, Stream, Sink};
use futures::sync::mpsc::Sender;
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use bytes::BytesMut;
use openssl::x509::X509;
use openssl::ssl::{SslConnectorBuilder, SslConnector, SslAcceptorBuilder, SslMethod,
                   SslVerifyMode, SSL_VERIFY_PEER};
use openssl::stack::Stack;
use tokio_openssl::{SslStream, SslConnectorExt, SslAcceptorExt};
use std::io;
use std::error::Error;

use data::{Peer, PeerInformationBase};
use {Identity, Identifier};
use network::LocalAddress;
use network::transport::{Connection as TransportConnection, Socket};
use network::discovery::discover_addresses;
use network::change::Listener;
use network::protocol::Message;

pub struct InnerState {
    pub id: Identity,
    pub pib: PeerInformationBase,
    pub relays: Vec<Identifier>,
    pub port: u16,
    handle: Handle,
    upstream: Sender<(Identifier, u16, BytesMut)>,
    sockets: HashMap<SocketAddr, Socket>,
}

#[derive(Clone)]
pub struct NetworkState(Rc<RefCell<InnerState>>);

impl NetworkState {
    pub fn new(
        id: Identity,
        pib: PeerInformationBase,
        relays: Vec<Identifier>,
        port: u16,
        handle: Handle,
        upstream: Sender<(Identifier, u16, BytesMut)>,
    ) -> NetworkState {
        NetworkState(Rc::new(RefCell::new(InnerState {
            id: id,
            pib: pib,
            relays: relays,
            port: port,
            handle: handle,
            upstream: upstream,
            sockets: HashMap::new(),
        })))
    }
    pub fn read(&self) -> Ref<InnerState> {
        self.0.borrow()
    }

    pub fn write(&self) -> RefMut<InnerState> {
        self.0.borrow_mut()
    }

    pub fn spawn<F: Future<Item = (), Error = ()> + 'static>(&self, f: F) {
        self.read().handle.spawn(f)
    }

    fn open_new_sockets(&self) {
        let mut addresses = match discover_addresses() {
            Ok(a) => a,
            Err(err) => return warn!("Error enumerating network interfaces: {}", err),
        };
        let port = self.read().port;
        addresses.iter_mut().for_each(|address| {
            address.internal.set_port(port)
        });
        self.close_stale_sockets(&addresses);
        let new_addresses = addresses
            .iter()
            .filter(|address| {
                !self.read().sockets.contains_key(&address.internal)
            })
            .filter(|address| match address.internal.ip() {
                IpAddr::V4(_) => true,
                IpAddr::V6(v6) => v6.is_global(),
            });
        for address in new_addresses {
            let _ = self.open_socket(*address).map_err(|err| {
                warn!("Error while opening socket: {}", err)
            });
        }
        self.publish_addresses();
    }

    fn close_stale_sockets(&self, current_addresses: &Vec<LocalAddress>) {
        let stale: Vec<SocketAddr> = self.read()
            .sockets
            .keys()
            .filter(|address| {
                current_addresses.iter().map(|addr| addr.internal).any(
                    |addr| {
                        &addr == *address
                    },
                )
            })
            .cloned()
            .collect();
        for address in stale {
            info!("Closing stale socket {}", address);
            if let Some(_socket) = self.write().sockets.remove(&address) {
                info!("Closed socket {}", address);
            } else {
                info!("No socket found for {}", address);
            };
        }
    }

    fn external_addresses(&self) -> Vec<SocketAddr> {
        self.read()
            .sockets
            .iter()
            .filter_map(|(_addr, socket)| socket.public_address())
            .collect()
    }

    fn my_peer_information(&self) -> Peer {
        Peer::new(
            self.read().id.identifier,
            self.external_addresses(),
            self.read().relays.clone(),
        )
    }

    fn publish_addresses(&self) {
        let peer_information = self.my_peer_information();
        for socket in self.read().sockets.values() {
            for connection in socket.read().connections.values() {
                connection.send_peer_info(peer_information.clone());
            }
        }
    }

    fn open_socket(&self, address: LocalAddress) -> io::Result<()> {
        debug!("Opening new socket on address {:?}", address);
        let socket = Socket::open(
            address,
            self.read().handle.clone(),
            self.read().id.clone(),
            self.clone(),
        )?;
        self.write().sockets.insert(
            address.internal,
            socket.clone(),
        );
        self.connect_to_relays(&socket);
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
                state.open_new_sockets();
                Ok(())
            })
            .map_err(|err| {
                warn!("Error while listening for network changes: {}", err)
            });
        self.spawn(task);
    }

    pub fn add_relay(&mut self, id: Identifier) {
        self.write().relays.push(id);
    }

    pub fn connect_to_relays(&self, socket: &Socket) {
        for relay in &self.read().relays {
            let addr = match self.read().pib.lookup_peer_address(relay) {
                Some(info) => info,
                None => continue,
            };
            println!("Connecting to relay {}", relay);
            let relay = *relay;
            let future = socket
                .open_connection(relay, addr)
                .and_then(|_| future::ok(()))
                .map_err(move |err| {
                    println!("Unable to connect to peer {}: {}", relay, err)
                });
            self.spawn(future);
        }
    }

    fn connect(
        &self,
        remote_id: Identifier,
    ) -> impl Future<Item = TransportConnection, Error = io::Error> {
        let state = self.clone();
        self.read()
            .pib
            .lookup_peer_address(&remote_id)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "peer address not found")
            })
            .into_future()
            .and_then(move |addr| state.connect_with_address(remote_id, addr))
    }

    fn connect_with_address(
        &self,
        remote_id: Identifier,
        address: SocketAddr,
    ) -> impl Future<Item = TransportConnection, Error = io::Error> {
        let state = self.clone();
        self.read()
            .sockets
            .values()
            .next()
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotConnected, "No socket is currently open")
            })
            .into_future()
            .and_then(move |socket| socket.open_connection(remote_id, address))
    }

    pub fn deliver_frame(&self, host_id: Identifier, channel_id: u16, data: BytesMut) {
        if channel_id == 0 {
            return self.process_control_message(host_id, data);
        }
        let task = self.read()
            .upstream
            .clone()
            .send((host_id, channel_id, data))
            .map(|_| ())
            .map_err(|err| warn!("Failed to pass message to upstream: {}", err));
        self.spawn(task);
    }

    pub fn process_control_message(&self, host_id: Identifier, data: BytesMut) {
        match Message::deserialize_from_msgpck(&data.freeze()) {
            Message::PeerInfo(peer_info) => {
                info!(
                    "Received new peer information from {}: {:?}",
                    host_id,
                    peer_info
                )
            }
            Message::Invalid(_) => warn!("Received invalid control message from peer {}", host_id),
        }
    }

    pub fn send_frame(&self, host_id: Identifier, channel_id: u16, data: BytesMut) {
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
        host_id: Identifier,
    ) -> impl Future<Item = TransportConnection, Error = io::Error> {
        let state = self.clone();
        let connection = self.read()
            .sockets
            .values()
            .filter_map(|socket| socket.get_connection(&host_id))
            .next();
        connection.ok_or(()).into_future().or_else(move |_| {
            state.connect(host_id)
        })
    }
}

impl Future for NetworkState {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.open_new_sockets();
        self.observe_network_changes();
        Ok(Async::NotReady)
    }
}
