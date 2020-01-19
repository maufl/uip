use std::io::{Error, ErrorKind, Result};
use std::task::{Context, Poll};
use std::pin::Pin;

use nix;
use nix::sys::socket::{bind, socket, AddressFamily, MsgFlags, SockAddr, SockType, SockFlag};
use nix::unistd::Pid;
use nix::pty::SessionId;
use bytes::BytesMut;
use tokio::io::PollEvented;
use tokio::stream::Stream;
use mio::Ready;

use crate::network::change::evented_socket::EventedSocket;

fn nix_error_to_io_error(err: nix::Error) -> Error {
    match err {
        nix::Error::Sys(err_no) => Error::from(err_no),
        _ => Error::new(ErrorKind::Other, err),
    }
}

pub struct RTNetlinkSocket {
    io: PollEvented<EventedSocket>,
}

impl RTNetlinkSocket {
    /// Creates a netlink route socket that sends messages for all given groups
    pub fn bind(groups: u32) -> Result<RTNetlinkSocket> {
        let flags = SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK;
        let sock = socket(AddressFamily::Netlink, SockType::Raw, flags, None)
            .map_err(nix_error_to_io_error)?;
        let pid = SessionId::from(Pid::this()) as u32;
        let addr = SockAddr::new_netlink(pid, groups);
        bind(sock, &addr).map_err(nix_error_to_io_error)?;
        let io = PollEvented::new(EventedSocket::from(sock))?;
        Ok(RTNetlinkSocket { io: io })
    }

}

impl Stream for RTNetlinkSocket {
    type Item = Result<BytesMut>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<BytesMut>>> {
        if let Poll::Pending = self.io.poll_read_ready(cx, Ready::readable()) {
            return Poll::Pending;
        }
        let mut buf = BytesMut::new();
        buf.resize(4096, 0u8);
        match self.io.get_ref().recv(&mut buf, MsgFlags::empty()) {
            Ok(n) => Poll::Ready(Some(Ok(buf.split_to(n)))),
            Err(err) => {
                if err.kind() == ErrorKind::WouldBlock {
                    self.io.clear_read_ready(cx, mio::Ready::readable())?;
                    Poll::Pending
                } else {
                    Poll::Ready(Some(Err(err)))
                }
            }
        }
    }
}
