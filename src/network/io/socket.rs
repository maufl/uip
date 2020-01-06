use tokio;
use tokio::net::{UdpSocket, udp};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::broadcast;
use futures::future::FutureExt;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::io::{Result};
use bytes::{Bytes,BytesMut};

use crate::network::LocalAddress;
use crate::Shared;
use super::Connection;

#[derive(Debug)]
pub struct Socket {
    connections: HashMap<SocketAddr, Sender<Bytes>>,
    out: Sender<(SocketAddr, Bytes)>,
    address: LocalAddress,
    upd_address: SocketAddr,
    close_sender: broadcast::Sender<()>
}

impl Shared<Socket> {
    pub async fn bind(addr: LocalAddress) -> Result<(Shared<Socket>, Receiver<Connection>)> {
        let socket = UdpSocket::bind(&addr.internal).await?;
        let upd_address = socket.local_addr()?;
        let (udp_receive_half, udp_send_half) = socket.split();     
        let (connection_sender, connection_receiver) = channel::<Connection>(10);
        let (data_sender, data_receiver) = channel::<(SocketAddr, Bytes)>(10);
        let (close_sender, close_receiver) = broadcast::channel::<()>(1);
        let second_close_receiver = close_sender.subscribe();
        let socket = Shared::new(Socket {
            connections: HashMap::new(),
            out: data_sender,
            address: addr,
            upd_address: upd_address,
            close_sender: close_sender
        });
        socket.spawn_read_task(udp_receive_half, connection_sender, close_receiver);
        socket.spawn_write_task(udp_send_half, data_receiver, second_close_receiver);
        Ok((socket, connection_receiver))
    }

    fn spawn_read_task(&self, mut udp_receive_half: udp::RecvHalf, mut connection_sender: Sender<Connection>, mut close_receiver: broadcast::Receiver<()>) {
        let socket = self.clone();
        tokio::spawn(async move {
            loop {
                let mut bytes = BytesMut::new();
                bytes.resize(1_500_000, 0);
                let res = futures_util::select! {
                    res = udp_receive_half.recv_from(&mut bytes).fuse() => res,
                    _ = close_receiver.recv().fuse() => return info!("Shared UDP socket closed"),
                };
                let (size, remote_addr) = match res {
                    Ok(r) => r,
                    Err(err) => return warn!("Receiver task of UDP socket terminated with error {}", err)
                };
                bytes.truncate(size);
                if let Some(connection) = socket.forward_or_new_connection(bytes.freeze(), remote_addr).await {
                    if connection_sender.send(connection).await.is_err() {
                        return warn!("Unable to forward new connections");
                    }
                }
            }
        });
    }

    fn spawn_write_task(&self, mut udp_send_half: udp::SendHalf, mut data_receiver: Receiver<(SocketAddr, Bytes)>, mut close_receiver: broadcast::Receiver<()>) {
        tokio::spawn(async move {
            loop {
                let res = futures_util::select! {
                    res = data_receiver.recv().fuse() => res,
                    _ = close_receiver.recv().fuse() => return info!("Shared UDP socket closed"),
                };
                let (addr, data) = match res {
                    Some(m) => m,
                    None => return info!("Sender task of UDP socket terminaed")
                };
                if let Err(err) = udp_send_half.send_to(&data, &addr).await {
                    return warn!("Unable to send data via UDP: {}", err);
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
        Ok(Connection::new(source, self.read().out.clone(), addr))
    }

    async fn forward_or_new_connection(&self, buf: Bytes, remote: SocketAddr) -> Option<Connection> {
        let (mut destination, connection) = if let Some(dest) = self.read().connections.get(&remote).cloned() {
            (dest, None)
        } else {
            let (destination, source) = channel::<Bytes>(10);
            let conn = Connection::new(source, self.read().out.clone(), remote);
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