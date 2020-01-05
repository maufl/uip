use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use std::collections::HashMap;
use tokio;
use tokio::prelude::*;
use tokio::stream::StreamExt;
use tokio::sync::mpsc::Sender;
use bytes::{BytesMut, Bytes};
use std::io;

use crate::data::{Peer, PeerInformationBase};
use crate::{Identifier, Identity, Shared};
use crate::network::LocalAddress;
use crate::network::transport::{Connection, Socket};
use crate::network::discovery::discover_addresses;
use crate::network::change::Listener;
use crate::network::protocol::Message;

#[derive(Clone)]
pub struct NetworkState {
    pub id: Identity,
    pub pib: PeerInformationBase,
    pub relays: Vec<Identifier>,
    pub port: u16,
    upstream: Sender<(Identifier, u16, u16, Bytes)>,
    sockets: HashMap<SocketAddr, Shared<Socket>>,
}

impl NetworkState {
    pub fn new(
        id: Identity,
        pib: PeerInformationBase,
        relays: Vec<Identifier>,
        port: u16,
        upstream: Sender<(Identifier, u16, u16, Bytes)>,
    ) -> NetworkState {
        NetworkState {
            id: id,
            pib: pib,
            relays: relays,
            port: port,
            upstream: upstream,
            sockets: HashMap::new(),
        }
    }

    pub fn shared(self) -> Shared<NetworkState> {
        Shared::new(self)
    }
}

impl Shared<NetworkState> {
    async fn open_new_sockets(&self) {
        let mut addresses = match discover_addresses() {
            Ok(a) => a,
            Err(err) => return warn!("Error enumerating network interfaces: {}", err),
        };
        let port = self.read().port;
        addresses
            .iter_mut()
            .for_each(|address| address.internal.set_port(port));
        self.close_stale_sockets(&addresses);
        let new_addresses: Vec<LocalAddress> = addresses
            .iter()
            .filter(|address| !self.read().sockets.contains_key(&address.internal))
            .filter(|address| match address.internal.ip() {
                IpAddr::V4(_) => true,
                IpAddr::V6(v6) => v6.is_global(),
            })
            .cloned()
            .collect();
        for address in new_addresses {
            if let Err(err) = self.open_socket(address).await {
                 warn!("Error while opening socket: {}", err);
            }
        }
        self.publish_addresses().await;
    }

    fn close_stale_sockets(&self, current_addresses: &[LocalAddress]) {
        let stale: Vec<SocketAddr> = self.read()
            .sockets
            .keys()
            .filter(|address| {
                !current_addresses
                    .iter()
                    .map(|addr| addr.internal)
                    .any(|addr| &addr == *address)
            })
            .cloned()
            .collect();
        for address in stale {
            info!("Closing stale socket {}", address);
            if let Some(socket) = self.write().sockets.remove(&address) {
                socket.close();
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

    async fn publish_addresses(&self) {
        let peer_information = self.my_peer_information();
        let sockets: Vec<Shared<Socket>> = self.read().sockets.values().cloned().collect();
        for socket in sockets {
            let connections: Vec<Connection> = socket.read().connections.values().cloned().collect();
            for mut connection in connections {
                connection.send_peer_info(peer_information.clone()).await;
            }
        }
    }

    async fn open_socket(&self, address: LocalAddress) -> io::Result<()> {
        debug!("Opening new socket on address {:?}", address);
        let id = self.read().id.clone();
        let (data_sender, mut data_receiver) = tokio::sync::mpsc::channel::<(Identifier, u16, u16, Bytes)>(10);
        let socket = Shared::<Socket>::open( address, id, data_sender).await?;
        self.write()
            .sockets
            .insert(address.internal, socket.clone());
        self.connect_to_relays_on_socket(&socket).await;
        let state = self.clone();
        tokio::spawn(async move {
            loop {
                let (id, src_port, dst_port, data) = match data_receiver.next().await {
                    Some(frame) => frame,
                    None => return info!("All frame senders for socket closed")
                };
                state.deliver_frame(id, src_port, dst_port, data).await
            }
        });
        Ok(())
    }

    async fn observe_network_changes(&self) -> Result<(),()> {
        let listener = match Listener::new() {
            Ok(l) => l,
            Err(err) => {
                warn!("Unable to listen for network changes: {}", err);
                return Err(());
            }
        };
        let mut change_stream = listener.debounce(Duration::from_millis(2000));
        while change_stream.next().await.is_some() {
            info!("Network changed");
            self.open_new_sockets().await;
        };
        Ok(())
    }

    pub fn add_relay(&mut self, id: Identifier) {
        self.write().relays.push(id);
    }

    pub async fn connect_to_relays(&self) {
        for socket in self.read().sockets.values() {
            self.connect_to_relays_on_socket(socket).await;
        }
    }

    pub async fn connect_to_relays_on_socket(&self, socket: &Shared<Socket>) {
        let relays: Vec<Identifier> = self.read().relays.clone();
        for relay in &relays {
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
            match socket.open_connection(relay, addr).await {
                Ok(mut conn) => conn.send_peer_info(state.my_peer_information()).await,
                Err(err) =>  warn!("Unable to connect to peer {}: {}", relay, err)
            };
        }
    }

    async fn connect(
        &self,
        remote_id: Identifier,
    ) -> Result<Connection, io::Error> {
        let addr = self.read().pib
            .lookup_peer_address(&remote_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "peer address not found"))?;
        self.connect_with_address(remote_id, addr).await
    }

    async fn connect_with_address(
        &self,
        remote_id: Identifier,
        address: SocketAddr,
    ) -> Result<Connection, io::Error> {
        let socket = self.read()
            .sockets
            .iter()
            .filter(|&(addr, _socket)| addr.is_ipv4() == address.is_ipv4())
            .map(|(_addr, socket)| socket)
            .next()
            .cloned()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotConnected, "No socket is currently open")
            })?;
        socket.open_connection(remote_id, address).await
    }

    pub async fn deliver_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: Bytes) {
        if src_port == 0 && dst_port == 0 {
            return self.process_control_message(host_id, data).await;
        }
        let mut upstream = self.write().upstream.clone();
        if let Err(err) = upstream.send((host_id, src_port, dst_port, data)).await {
            warn!("Failed to pass message to upstream: {}", err);
        }
    }

    pub async fn process_control_message(&self, host_id: Identifier, data: Bytes) {
        match Message::deserialize_from_msgpck(&data) {
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
                match self.get_connection(host_id).await {
                    Ok(mut conn) => conn.send_peer_info(peer).await,
                    Err(err) => warn!("Unable to respond with peer information: {}", err)
                }
            }
            Message::Invalid(_) => warn!("Received invalid control message from peer {}", host_id),
        }
    }

    pub async fn request_peer_info(&self, id: Identifier) {
        debug!("Requesting peer information for {}", id);
        let relay = match self.read().pib.get_peer(&id).and_then(|p| p.relays.first()) {
            Some(id) => *id,
            None => return,
        };
        debug!("Requesting peer information from relay {}", relay);
        match self.get_connection(relay).await {
            Ok(mut conn) => conn.send_peer_info_request(&id).await,
            Err(err) => warn!("Unable to request peer information for {} from relay {}: {}", id, relay, err)
        }
    }

    pub async fn send_frame(&self, host_id: Identifier, src_port: u16, dst_port: u16, data: Bytes) {
        match self.get_connection(host_id).await {
            Ok(mut connection) => if connection.send_data_frame(src_port, dst_port, data).await.is_err() {
                        // FIXME: At this point, the connection must be removed
                        warn!("unable to forward frame");
                    }
            Err(err) => warn!("Error while sending frame: {}", err)
        }
    }

    pub async fn get_connection(
        &self,
        host_id: Identifier,
    ) -> Result<Connection, io::Error> {
        let optional_connection = self.read()
            .sockets
            .values()
            .filter_map(|socket| socket.get_connection(&host_id))
            .next();
        match optional_connection {
            Some(connection) => Ok(connection),
            None => self.connect(host_id).await
        }
    }

    pub async fn run(&self) {
        self.open_new_sockets().await;
        self.observe_network_changes().await;
    }
}
