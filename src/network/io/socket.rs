use tokio;
use tokio::net::UdpSocket;
use futures::sync::mpsc::{channel, Receiver, Sender};
use futures::stream::poll_fn;
use futures::{Async, Future, Poll, Sink, Stream};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::io::{Error, ErrorKind, Result};
use nix;
use std::os::unix::io::AsRawFd;

use network::LocalAddress;
use Shared;
use super::Connection;

#[derive(Debug)]
pub struct Socket {
    inner: UdpSocket,
    connections: HashMap<SocketAddr, Sender<Vec<u8>>>,
    incoming: Receiver<Connection>,
    address: LocalAddress,
    closed: bool,
}

impl Socket {
    pub fn shared(self) -> Shared<Socket> {
        Shared::new(self)
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        debug!("IO socket dropped")
    }
}

fn nix_error_to_io_error(err: nix::Error) -> Error {
    match err {
        nix::Error::Sys(err_no) => Error::from(err_no),
        _ => Error::new(ErrorKind::Other, err),
    }
}

impl Shared<Socket> {
    pub fn bind(addr: LocalAddress) -> Result<Shared<Socket>> {
        let socket = UdpSocket::bind(&addr.internal)?;
        if cfg!(unix) {
            nix::sys::socket::setsockopt(
                socket.as_raw_fd(),
                nix::sys::socket::sockopt::ReusePort,
                &true,
            ).map_err(nix_error_to_io_error)?;
        }
        Shared::<Socket>::from_socket(socket, addr)
    }

    pub fn from_socket(sock: UdpSocket, address: LocalAddress) -> Result<Shared<Socket>> {
        let (sender, receiver) = channel::<Connection>(10);
        let socket = Socket {
            inner: sock,
            connections: HashMap::new(),
            incoming: receiver,
            address: address,
            closed: false,
        }.shared();
        socket.listen(sender);
        Ok(socket)
    }

    pub fn close(&self) {
        self.write().closed = true;
        for (_, sender) in self.write().connections.iter_mut() {
            sender.close();
        }
        self.write().incoming.close()
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
                tokio::spawn(task);
            }
            Ok(Async::Ready(Some(())))
        }).collect()
            .map(move |_| info!("UDP socket {} was closed", local_address));
        tokio::spawn(task);
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.read().inner.local_addr()
    }

    pub fn send_to(&self, buf: &[u8], remote: &SocketAddr) -> Result<usize> {
        if self.read().closed {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Socket was closed"));
        }
        match self.write().inner.poll_send_to(buf, remote) {
            Ok(Async::Ready(r)) => Ok(r),
            Ok(Async::NotReady) => Err(Error::new(ErrorKind::WouldBlock, "Operation would block")),
            Err(e) => Err(e),
        }
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        if self.read().closed {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Socket was closed"));
        }
        match self.write().inner.poll_recv_from(buf) {
            Ok(Async::Ready(r)) => Ok(r),
            Ok(Async::NotReady) => Err(Error::new(ErrorKind::WouldBlock, "Operation would block")),
            Err(e) => Err(e),
        }
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
            tokio::spawn(task);
            return None;
        }
        let (destination, source) = channel::<Vec<u8>>(10);
        let task = destination
            .clone()
            .send(buf.into())
            .map(|_| ())
            .map_err(|_| println!("Error when forwarding datagram"));
        tokio::spawn(task);
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
