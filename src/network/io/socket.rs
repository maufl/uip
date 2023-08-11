use bytes::{Bytes, BytesMut};
use futures::future::FutureExt;
use std::collections::HashMap;
use std::io::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio;
use tokio::net::UdpSocket;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use super::Connection;
use crate::network::LocalAddress;
use crate::Shared;

#[derive(Debug)]
pub struct Socket {
    inner: Arc<UdpSocket>,
    connections: HashMap<SocketAddr, Sender<Bytes>>,
    address: LocalAddress,
    upd_address: SocketAddr,
    close_sender: broadcast::Sender<()>,
}

impl Shared<Socket> {
    pub async fn bind(addr: LocalAddress) -> Result<(Shared<Socket>, Receiver<Connection>)> {
        let upd_socket = Arc::new(UdpSocket::bind(&addr.address).await?);
        let upd_address = upd_socket.local_addr()?;
        let (connection_sender, connection_receiver) = channel::<Connection>(10);
        let (close_sender, close_receiver) = broadcast::channel::<()>(1);
        let socket = Shared::new(Socket {
            inner: upd_socket.clone(),
            connections: HashMap::new(),
            address: addr,
            upd_address: upd_address,
            close_sender: close_sender,
        });
        socket.spawn_read_task(connection_sender, close_receiver);
        Ok((socket, connection_receiver))
    }

    fn spawn_read_task(
        &self,
        connection_sender: Sender<Connection>,
        mut close_receiver: broadcast::Receiver<()>,
    ) {
        let socket = self.clone();
        let udp_socket = socket.read().inner.clone();
        tokio::spawn(async move {
            loop {
                let mut bytes = BytesMut::new();
                bytes.resize(1_500_000, 0);
                let res = futures_util::select! {
                    res = udp_socket.recv_from(&mut bytes).fuse() => res,
                    _ = close_receiver.recv().fuse() => return info!("Shared UDP socket closed"),
                };
                let (size, remote_addr) = match res {
                    Ok(r) => r,
                    Err(err) => {
                        return warn!("Receiver task of UDP socket terminated with error {}", err)
                    }
                };
                bytes.truncate(size);
                if let Some(connection) = socket
                    .forward_or_new_connection(bytes.freeze(), remote_addr)
                    .await
                {
                    if connection_sender.send(connection).await.is_err() {
                        return warn!("Unable to forward new connections");
                    }
                }
            }
        });
    }

    pub fn close(&self) {
        let _ = self.write().close_sender.send(());
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.read().upd_address)
    }

    pub fn connect(&self, addr: SocketAddr) -> Result<Connection> {
        let (destination, source) = channel::<Bytes>(10);
        self.write().connections.insert(addr, destination);
        Ok(Connection::new(self.read().inner.clone(), source, addr))
    }

    async fn forward_or_new_connection(
        &self,
        buf: Bytes,
        remote: SocketAddr,
    ) -> Option<Connection> {
        let (destination, connection) =
            if let Some(dest) = self.read().connections.get(&remote).cloned() {
                (dest, None)
            } else {
                let (destination, source) = channel::<Bytes>(10);
                let conn = Connection::new(self.read().inner.clone(), source, remote);
                (destination, Some(conn))
            };
        if destination.send(buf.into()).await.is_err() {
            warn!("Error forwarding datagram, receiver half in connection is closed");
            return None;
        };
        self.write().connections.insert(remote, destination);
        connection
    }
}
