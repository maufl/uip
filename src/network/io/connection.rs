use tokio_io::{AsyncRead, AsyncWrite};
use futures::sync::mpsc::Receiver;
use futures::{Async, Poll, Stream};
use std::net::SocketAddr;
use std::io::{Error, ErrorKind, Read, Result, Write};

use network::io::Socket;
use Shared;

pub struct Connection {
    incoming: Receiver<Vec<u8>>,
    remote_addr: SocketAddr,
    socket: Shared<Socket>,
}

impl Connection {
    pub fn new(
        incoming: Receiver<Vec<u8>>,
        remote_addr: SocketAddr,
        socket: Shared<Socket>,
    ) -> Connection {
        Connection {
            incoming: incoming,
            remote_addr: remote_addr,
            socket: socket,
        }
    }

    pub fn remote_addr(&self) -> Result<SocketAddr> {
        Ok(self.remote_addr)
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn close(&mut self) {
        self.incoming.close()
    }
}

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let async = self.incoming
            .poll()
            .expect("Error while polling futures Receiver!");
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
        self.socket.send_to(buf, &self.remote_addr)
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
