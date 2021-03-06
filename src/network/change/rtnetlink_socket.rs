use std::io::{Error, ErrorKind, Result};
use std::result;
use nix;
use nix::sys::socket::{bind, socket, AddressFamily, MsgFlags, SockAddr, SockType, SOCK_CLOEXEC,
                       SOCK_NONBLOCK};
use nix::unistd::Pid;
use nix::pty::SessionId;
use libc::NETLINK_ROUTE;
use bytes::BytesMut;
use tokio::reactor::PollEvented2;
use mio::Ready;
use tokio::io;
use tokio::prelude::*;

use network::change::evented_socket::EventedSocket;

fn nix_error_to_io_error(err: nix::Error) -> Error {
    match err {
        nix::Error::Sys(err_no) => Error::from(err_no),
        _ => Error::new(ErrorKind::Other, err),
    }
}

pub struct RTNetlinkSocket {
    io: PollEvented2<EventedSocket>,
}

impl RTNetlinkSocket {
    /// Creates a netlink route socket that sends messages for all given groups
    pub fn bind(groups: u32) -> Result<RTNetlinkSocket> {
        let flags = SOCK_CLOEXEC | SOCK_NONBLOCK;
        let sock = socket(AddressFamily::Netlink, SockType::Raw, flags, NETLINK_ROUTE)
            .map_err(nix_error_to_io_error)?;
        let pid = SessionId::from(Pid::this()) as u32;
        let addr = SockAddr::new_netlink(pid, groups);
        bind(sock, &addr).map_err(nix_error_to_io_error)?;
        let io = PollEvented2::new(EventedSocket::from(sock));
        Ok(RTNetlinkSocket { io: io })
    }

    /// Receives data from the socket.
    ///
    /// On success, returns the number of bytes read.
    pub fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        if let Async::NotReady = self.io.poll_read_ready(Ready::readable())? {
            return Err(ErrorKind::WouldBlock.into());
        }
        match self.io.get_ref().recv(buf, MsgFlags::empty()) {
            Ok(n) => Ok(n),
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    self.io.clear_read_ready(Ready::readable())?;
                }
                Err(e)
            }
        }
    }

    pub fn poll_read_ready(&self, mask: Ready) -> result::Result<Async<Ready>, io::Error> {
        self.io.poll_read_ready(mask)
    }
}

impl Stream for RTNetlinkSocket {
    type Item = BytesMut;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<BytesMut>, Error> {
        if let Async::NotReady = self.poll_read_ready(Ready::readable())? {
            return Ok(Async::NotReady);
        }
        let mut buf = BytesMut::with_capacity(4096);
        match self.recv(&mut buf) {
            Ok(n) => Ok(Async::Ready(Some(buf.split_to(n)))),
            Err(err) => {
                if err.kind() == ErrorKind::WouldBlock {
                    Ok(Async::NotReady)
                } else {
                    Err(err)
                }
            }
        }
    }
}
