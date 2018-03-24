use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use std::collections::HashMap;
use futures::{Async, Future, IntoFuture, Poll, Sink, Stream};
use futures::sync::mpsc::Sender;
use tokio_core::reactor::Handle;
use bytes::BytesMut;
use std::io;

use data::{Peer, PeerInformationBase};
use {Identifier, Identity, Shared};
use network::LocalAddress;
use network::transport::{Connection as TransportConnection, Socket};
use network::discovery::discover_addresses;
use network::change::Listener;
use network::protocol::Message;

#[derive(Clone)]
pub struct NetworkState {
    pub id: Identity,
    pub pib: PeerInformationBase,
    pub relays: Vec<Identifier>,
    pub port: u16,
    handle: Handle,
    upstream: Sender<(Identifier, u16, u16, BytesMut)>,
    sockets: HashMap<SocketAddr, Shared<Socket>>,
}

impl NetworkState {
    pub fn new(
        id: Identity,
        pib: PeerInformationBase,
        relays: Vec<Identifier>,
        port: u16,
        handle: Handle,
        upstream: Sender<(Identifier, u16, u16, BytesMut)>,
    ) -> NetworkState {
        NetworkState {
            id: id,
            pib: pib,
            relays: relays,
            port: port,
            handle: handle,
            upstream: upstream,
            sockets: HashMap::new(),
        }
    }

    pub fn shared(self) -> Shared<NetworkState> {
        Shared::new(self)
    }
}

impl Shared<NetworkState> {
    pub fn spawn<F: Future<Item = (), Error = ()> + 'static>(&self, f: F) {
        self.read().handle.spawn(f)
    }

    fn open_new_sockets(&self) {
        let mut addresses = match discover_addresses() {
            Ok(a) => a,
            Err(err) => return warn!("Error enumerating network interfaces: {}", err),
        };
        let port = self.read().port;
        addresses
            .iter_mut()
            .for_each(|address| address.internal.set_port(port));
        self.close_stale_sockets(&addresses);
        let new_addresses = addresses
            .iter()
            .filter(|address| !self.read().sockets.contains_key(&address.internal))
            .filter(|address| match address.internal.ip() {
                IpAddr::V4(_) => true,
                IpAddr::V6(v6) => v6.is_global(),
            });
        for address in new_addresses {
            let _ = self.open_socket(*address)
                .map_err(|err| warn!("Error while opening socket: {}", err));
        }
        self.publish_addresses();
    }

    fn close_stale_sockets(&self, current_addresses: &[LocalAddress]) {
        let stale: Vec<SocketAddr> = self.read()
            .sockets
            .keys()
            .filter(|address| {
                current_addresses
                    .iter()
                    .map(|addr| addr.internal)
                    .any(|addr| &addr == *address)
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
        let socket = Shared::<Socket>::open(
            address,
            &self.read().handle,
            self.read().id.clone(),
            self.clone(),
        )?;
        self.write()
            .sockets
            .insert(address.internal, socket.clone());
        self.connect_to_relays_on_socket(&socket);
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
            .map_err(|err| warn!("Error while listening for network changes: {}", err));
        self.spawn(task);
    }

    pub fn add_relay(&mut self, id: Identifier) {
        self.write().relays.push(id);
    }

    pub fn connect_to_relays(&self) {
        for socket in self.read().sockets.values() {
            self.connect_to_relays_on_socket(socket)
        }
    }

    pub fn connect_to_relays_on_socket(&self, socket: &Shared<Socket>) {
        for relay in &self.read().relays {
            if socket.get_connection(relay).is_some() {
                continue;
            };
            let addr = match self.read().pib.lookup_peer_address(relay) {
                Some(info) => info,
                None => continue,
            };
            info!("Connecting to relay {}", relay);
            let relay = *relay;
            let state = self.clone();
            let future = socket
                .open_connection(relay, addr)
                .and_then(move |conn| {
                    conn.send_peer_info(state.my_peer_information());
                    Ok(())
                })
                .map_err(move |err| warn!("Unable to connect to peer {}: {}", relay, err));
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
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "peer address not found"))
            .into_future()
            .and_then(move |addr| state.connect_with_address(remote_id, addr))
    }

    fn connect_with_address(
        &self,
        remote_id: Identifier,
        address: SocketAddr,
    ) -> impl Future<Item = TransportConnection, Error = io::Error> {
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

    pub fn deliver_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: BytesMut) {
        if src_port == 0 && dst_port == 0 {
            return self.process_control_message(host_id, data);
        }
        let task = self.read()
            .upstream
            .clone()
            .send((host_id, src_port, dst_port, data))
            .map(|_| ())
            .map_err(|err| warn!("Failed to pass message to upstream: {}", err));
        self.spawn(task);
    }

    pub fn process_control_message(&self, host_id: Identifier, data: BytesMut) {
        match Message::deserialize_from_msgpck(&data.freeze()) {
            Message::PeerInfo(peer_info) => {
                info!(
                    "Received new peer information from {}: {:?}",
                    host_id, peer_info
                );
                if host_id == peer_info.peer.id {
                    self.write().pib.add_peer(peer_info.peer.id, peer_info.peer)
                }
            }
            Message::PeerInfoRequest(identifier) => {
                debug!("Received peer information request for {}", identifier);
                let peer = match self.read().pib.get_peer(&identifier) {
                    Some(peer) => peer.clone(),
                    None => return,
                };
                let task = self.get_connection(host_id)
                    .and_then(move |conn| {
                        conn.send_peer_info(peer);
                        Ok(())
                    })
                    .map_err(|err| warn!("Unable to respond with peer information: {}", err));
                self.spawn(task);
            }
            Message::Invalid(_) => warn!("Received invalid control message from peer {}", host_id),
        }
    }

    pub fn request_peer_info(&self, id: Identifier) {
        debug!("Requesting peer information for {}", id);
        let relay = match self.read().pib.get_peer(&id).and_then(|p| p.relays.first()) {
            Some(id) => *id,
            None => return,
        };
        debug!("Requesting peer information from relay {}", relay);
        let task = self.get_connection(relay)
            .and_then(move |conn| {
                conn.send_peer_info_request(&id);
                Ok(())
            })
            .map_err(move |err| {
                warn!(
                    "Unable to request peer information for {} from relay {}: {}",
                    id, relay, err
                )
            });
        self.spawn(task);
    }

    pub fn send_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: BytesMut) {
        let task = self.get_connection(host_id)
            .and_then(move |connection| {
                connection
                    .send_data_frame(src_port, dst_port, data)
                    .map_err(|_| {
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
        connection
            .ok_or(())
            .into_future()
            .or_else(move |_| state.connect(host_id))
    }
}

impl Future for Shared<NetworkState> {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.open_new_sockets();
        self.observe_network_changes();
        Ok(Async::NotReady)
    }
}
