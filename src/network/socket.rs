use tokio_core::net::{UdpSocket};
use tokio_core::reactor::{Handle};
use tokio_io::{AsyncRead,AsyncWrite};
use futures::sync::mpsc::{channel,Sender,Receiver};
use futures::{Future,Poll,Async,Stream,Sink};
use std::sync::{Arc,RwLock,RwLockReadGuard,RwLockWriteGuard};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::io::{Read,Write,Result,Error,ErrorKind};

pub struct Connection {
    incoming: Receiver<Vec<u8>>,
    addr: SocketAddr,
    socket: SharedSocket,
}

impl Connection {

    fn new(incoming: Receiver<Vec<u8>>, addr: SocketAddr, socket: SharedSocket) -> Connection {
        Connection{
            incoming: incoming,
            addr: addr,
            socket: socket
        }
    }
}

impl Read for Connection {
    fn read(&mut self, mut buf: &mut [u8]) -> Result<usize> {
        let async = self.incoming.poll().expect("Error while polling futures Receiver!");
        match async {
            Async::NotReady => Err(Error::new(ErrorKind::WouldBlock,"no bytes ready")),
            Async::Ready(None) => Err(Error::new(ErrorKind::UnexpectedEof,"end of file")),
            Async::Ready(Some(recv)) => buf.write(recv.as_slice())
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
    fn shutdown(&mut self) -> Poll<(),Error> {
        Ok(Async::Ready(()))
    }
}

struct Socket {
    inner: UdpSocket,
    connections: HashMap<SocketAddr, Sender<Vec<u8>>>,
    handle: Handle,
}

#[derive(Clone)]
pub struct SharedSocket(Arc<RwLock<Socket>>);

impl SharedSocket {

    pub fn bind(addr: &SocketAddr, handle: &Handle) -> Result<SharedSocket> {
        UdpSocket::bind(addr, handle)
            .map(|s| SharedSocket::from_socket(s, handle.clone()))
    }

    pub fn from_socket(sock: UdpSocket, handle: Handle) -> SharedSocket {
        SharedSocket(Arc::new(RwLock::new(Socket{
            inner: sock,
            connections: HashMap::new(),
            handle: handle
        })))
    }

    fn read(&self) -> RwLockReadGuard<Socket> {
        self.0.read().expect("Unable to acquire read lock on state")
    }

    fn write(&self) -> RwLockWriteGuard<Socket> {
        self.0.write().expect("Unable to acquire write lock on state")
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.read().inner.local_addr()
    }

    pub fn send_to(&self, buf: &[u8], remote: &SocketAddr) -> Result<usize> {
        self.write().inner.send_to(buf,remote)
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.write().inner.recv_from(buf)
    }

    pub fn incoming(&self) -> impl Stream<Item=Connection,Error=Error> {
        IncomingUdpConnections{ socket: self.clone() }
    }

    pub fn connect(&self, addr: SocketAddr) -> Result<Connection> {
        let (destination, source) = channel::<Vec<u8>>(10);
        self.write().connections.insert(addr, destination);
        Ok(Connection::new(source, addr, self.clone()))
    }

    pub fn forward_or_new_connection(&self, buf: &[u8], remote: SocketAddr) -> Option<Connection> {
        let mut socket = self.write();
        if let Some(destination) = socket.connections.get(&remote) {
            let task = destination.clone().send(buf.into())
                .map(|_| ()).map_err(|_| println!("Error when forwarding datagram") );
            socket.handle.spawn(task);
            return None
        }
        let (destination, source) = channel::<Vec<u8>>(10);
        let task = destination.clone().send(buf.into())
            .map(|_| ()).map_err(|_| println!("Error when forwarding datagram") );
        socket.handle.spawn(task);
        socket.connections.insert(remote, destination);
        Some(Connection::new(source, remote, self.clone()))
    }
}

struct IncomingUdpConnections {
    socket: SharedSocket
}

impl Stream for IncomingUdpConnections {
    type Item = Connection;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Connection>, Error> {
        let mut datagram = [0u8; 1500];
        let (read, addr) = match self.socket.recv_from(&mut datagram) {
            Err(e) => return match e.kind() {
                ErrorKind::WouldBlock => Ok(Async::NotReady),
                ErrorKind::UnexpectedEof => Ok(Async::Ready(None)),
                _ => Err(e)
            },
            Ok(d) => d
        };
        if let Some(connection) = self.socket.forward_or_new_connection(&datagram[0..read], addr) {
            Ok(Async::Ready(Some(connection)))
        } else {
            Ok(Async::NotReady)
        }
    }
}
