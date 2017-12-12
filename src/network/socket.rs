use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use futures::sync::mpsc::{channel, Sender, Receiver};
use futures::stream::poll_fn;
use futures::{Future, Poll, Async, Stream, Sink};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::io::{Read, Write, Result, Error, ErrorKind};

pub struct Connection {
    incoming: Receiver<Vec<u8>>,
    addr: SocketAddr,
    socket: SharedSocket,
}

impl Connection {
    fn new(incoming: Receiver<Vec<u8>>, addr: SocketAddr, socket: SharedSocket) -> Connection {
        Connection {
            incoming: incoming,
            addr: addr,
            socket: socket,
        }
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.addr)
    }
}

impl Read for Connection {
    fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let async = self.incoming.poll().expect(
            "Error while polling futures Receiver!",
        );
        match async {
            Async::NotReady => Err(Error::new(ErrorKind::WouldBlock, "no bytes ready")),
            Async::Ready(None) => Err(Error::new(ErrorKind::UnexpectedEof, "end of file")),
            Async::Ready(Some(recv)) => {
                buf[..recv.len()].clone_from_slice(recv.as_slice());
                Ok(recv.len())
            }
        }
    }
}

impl AsyncRead for Connection {}

impl Write for Connection {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.socket.send_to(buf, &self.addr)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl AsyncWrite for Connection {
    fn shutdown(&mut self) -> Poll<(), Error> {
        Ok(Async::Ready(()))
    }
}

struct Socket {
    inner: UdpSocket,
    connections: HashMap<SocketAddr, Sender<Vec<u8>>>,
    handle: Handle,
    incoming: Receiver<Connection>,
}

#[derive(Clone)]
pub struct SharedSocket(Arc<RwLock<Socket>>);

impl SharedSocket {
    pub fn bind(addr: &SocketAddr, handle: &Handle) -> Result<SharedSocket> {
        UdpSocket::bind(addr, handle).map(|s| SharedSocket::from_socket(s, handle.clone()))
    }

    pub fn from_socket(sock: UdpSocket, handle: Handle) -> SharedSocket {
        let (sender, receiver) = channel::<Connection>(10);
        let socket = SharedSocket(Arc::new(RwLock::new(Socket {
            inner: sock,
            connections: HashMap::new(),
            handle: handle.clone(),
            incoming: receiver,
        })));
        let socket_clone = socket.clone();
        let handle_clone = handle.clone();
        let task = poll_fn(move || {
            let mut datagram = [0u8; 1500];
            let (read, addr) = match socket_clone.recv_from(&mut datagram) {
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
            if let Some(connection) = socket_clone.forward_or_new_connection(
                &datagram[0..read],
                addr,
            )
            {
                let task = sender.clone().send(connection).map(|_| ()).map_err(|err| {
                    warn!("Unable to forward new UDP connection: {}", err)
                });
                handle_clone.spawn(task);
            }
            Ok(Async::Ready(Some(())))
        }).collect()
            .map(|_| ());
        handle.spawn(task);
        socket
    }

    fn read(&self) -> RwLockReadGuard<Socket> {
        self.0.read().expect("Unable to acquire read lock on state")
    }

    fn write(&self) -> RwLockWriteGuard<Socket> {
        self.0.write().expect(
            "Unable to acquire write lock on state",
        )
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
        IncomingUdpConnections { socket: self.clone() }
    }

    pub fn connect(&self, addr: SocketAddr) -> Result<Connection> {
        let (destination, source) = channel::<Vec<u8>>(10);
        self.write().connections.insert(addr, destination);
        Ok(Connection::new(source, addr, self.clone()))
    }

    pub fn forward_or_new_connection(&self, buf: &[u8], remote: SocketAddr) -> Option<Connection> {
        let mut socket = self.write();
        if let Some(destination) = socket.connections.get(&remote) {
            let task = destination.clone().send(buf.into()).map(|_| ()).map_err(
                |_| {
                    println!("Error when forwarding datagram")
                },
            );
            socket.handle.spawn(task);
            return None;
        }
        let (destination, source) = channel::<Vec<u8>>(10);
        let task = destination.clone().send(buf.into()).map(|_| ()).map_err(
            |_| {
                println!("Error when forwarding datagram")
            },
        );
        socket.handle.spawn(task);
        socket.connections.insert(remote, destination);
        Some(Connection::new(source, remote, self.clone()))
    }
}

struct IncomingUdpConnections {
    socket: SharedSocket,
}

impl Stream for IncomingUdpConnections {
    type Item = Connection;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Connection>, ()> {
        self.socket.write().incoming.poll()
    }
}
