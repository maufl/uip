use std::io::{self, Error, ErrorKind, Result};
use std::os::fd::RawFd;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::ready;
use nix;
use nix::pty::SessionId;
use nix::sys::socket::{bind, recv, socket, AddressFamily, MsgFlags, SockAddr, SockFlag, SockType};
use nix::unistd::Pid;
use tokio::io::unix::AsyncFd;
use tokio::io::{AsyncRead, ReadBuf};

fn nix_error_to_io_error(err: nix::Error) -> Error {
    match err {
        nix::Error::Sys(err_no) => Error::from(err_no),
        _ => Error::new(ErrorKind::Other, err),
    }
}

pub struct RTNetlinkSocket {
    async_fd: AsyncFd<RawFd>,
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

        Ok(RTNetlinkSocket {
            async_fd: AsyncFd::new(sock)?,
        })
    }
}

impl AsyncRead for RTNetlinkSocket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.async_fd.poll_read_ready(cx))?;

            let unfilled = buf.initialize_unfilled();
            match guard.try_io(|inner| {
                recv(*inner.get_ref(), unfilled, MsgFlags::empty()).map_err(nix_error_to_io_error)
            }) {
                Ok(Ok(len)) => {
                    buf.advance(len);
                    return Poll::Ready(Ok(()));
                }
                Ok(Err(err)) => return Poll::Ready(Err(err)),
                Err(_would_block) => continue,
            }
        }
    }
}
