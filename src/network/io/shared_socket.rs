use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;
use futures::sync::mpsc::{channel, Sender, Receiver};
use futures::stream::poll_fn;
use std::rc::Rc;
use std::cell::{RefCell, Ref, RefMut};
use futures::{Future, Poll, Async, Stream, Sink};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::io::{Result, ErrorKind};

use network::{LocalAddress, Connection};


struct Socket {
    inner: UdpSocket,
    connections: HashMap<SocketAddr, Sender<Vec<u8>>>,
    handle: Handle,
    incoming: Receiver<Connection>,
    address: LocalAddress,
}

#[derive(Clone)]
pub struct SharedSocket(Rc<RefCell<Socket>>);

impl SharedSocket {
    pub fn bind(addr: LocalAddress, handle: Handle) -> Result<SharedSocket> {
        UdpSocket::bind(&addr.internal, &handle).and_then(move |socket| {
            SharedSocket::from_socket(socket, addr, handle)
        })
    }

    pub fn from_socket(
        sock: UdpSocket,
        address: LocalAddress,
        handle: Handle,
    ) -> Result<SharedSocket> {
        let (sender, receiver) = channel::<Connection>(10);
        let socket = SharedSocket(Rc::new(RefCell::new(Socket {
            inner: sock,
            connections: HashMap::new(),
            handle: handle.clone(),
            incoming: receiver,
            address: address,
        })));
        socket.listen(sender);
        Ok(socket)
    }

    fn read(&self) -> Ref<Socket> {
        self.0.borrow()
    }

    fn write(&self) -> RefMut<Socket> {
        self.0.borrow_mut()
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
                let task = sender.clone().send(connection).map(|_| ()).map_err(|err| {
                    warn!("Unable to forward new UDP connection: {}", err)
                });
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
