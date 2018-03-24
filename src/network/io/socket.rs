use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;
use futures::sync::mpsc::{channel, Receiver, Sender};
use futures::stream::poll_fn;
use futures::{Async, Future, Poll, Sink, Stream};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::io::{ErrorKind, Result};

use network::LocalAddress;
use Shared;
use super::Connection;

pub struct Socket {
    inner: UdpSocket,
    connections: HashMap<SocketAddr, Sender<Vec<u8>>>,
    handle: Handle,
    incoming: Receiver<Connection>,
    address: LocalAddress,
}

impl Socket {
    pub fn shared(self) -> Shared<Socket> {
        Shared::new(self)
    }
}

impl Shared<Socket> {
    pub fn bind(addr: LocalAddress, handle: Handle) -> Result<Shared<Socket>> {
        UdpSocket::bind(&addr.internal, &handle)
            .and_then(move |socket| Shared::<Socket>::from_socket(socket, addr, handle))
    }

    pub fn from_socket(
        sock: UdpSocket,
        address: LocalAddress,
        handle: Handle,
    ) -> Result<Shared<Socket>> {
        let (sender, receiver) = channel::<Connection>(10);
        let socket = Socket {
            inner: sock,
            connections: HashMap::new(),
            handle: handle,
            incoming: receiver,
            address: address,
        }.shared();
        socket.listen(sender);
        Ok(socket)
    }

    fn listen(&self, sender: Sender<Connection>) {
        let socket = self.clone();
        let local_address = socket.read().address.internal;
        let task = poll_fn(move || {
            let mut datagram = [0u8; 1500];
            let (read, addr) = match socket.recv_from(&mut datagram) {
                Err(e) => {
                    return match e.kind() {
                        ErrorKind::WouldBlock => Ok(Async::NotReady),
                        ErrorKind::UnexpectedEof => Ok(Async::Ready(None)),
                        _ => {
                            warn!("Error while reading from socket: {}", e);
                            Err(())
                        }
                    }
                }
                Ok(d) => d,
            };
            if let Some(connection) = socket.forward_or_new_connection(&datagram[0..read], addr) {
                let task = sender
                    .clone()
                    .send(connection)
                    .map(|_| ())
                    .map_err(|err| warn!("Unable to forward new UDP connection: {}", err));
                socket.read().handle.spawn(task);
            }
            Ok(Async::Ready(Some(())))
        }).collect()
            .map(move |_| info!("UDP socket {} was closed", local_address));
        self.read().handle.spawn(task);
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.read().inner.local_addr()
    }

    pub fn send_to(&self, buf: &[u8], remote: &SocketAddr) -> Result<usize> {
        self.write().inner.send_to(buf, remote)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.write().inner.recv_from(buf)
    }

    pub fn incoming(&self) -> impl Stream<Item = Connection, Error = ()> {
        IncomingUdpConnections {
            socket: self.clone(),
        }
    }

    pub fn connect(&self, addr: SocketAddr) -> Result<Connection> {
        let (destination, source) = channel::<Vec<u8>>(10);
        self.write().connections.insert(addr, destination);
        Ok(Connection::new(source, addr, self.clone()))
    }

    fn forward_or_new_connection(&self, buf: &[u8], remote: SocketAddr) -> Option<Connection> {
        let mut socket = self.write();
        if let Some(destination) = socket.connections.get(&remote) {
            let task = destination
                .clone()
                .send(buf.into())
                .map(|_| ())
                .map_err(|_| println!("Error when forwarding datagram"));
            socket.handle.spawn(task);
            return None;
        }
        let (destination, source) = channel::<Vec<u8>>(10);
        let task = destination
            .clone()
            .send(buf.into())
            .map(|_| ())
            .map_err(|_| println!("Error when forwarding datagram"));
        socket.handle.spawn(task);
        socket.connections.insert(remote, destination);
        Some(Connection::new(source, remote, self.clone()))
    }
}

struct IncomingUdpConnections {
    socket: Shared<Socket>,
}

impl Stream for IncomingUdpConnections {
    type Item = Connection;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Connection>, ()> {
        self.socket.write().incoming.poll()
    }
}
