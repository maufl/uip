use std::os::unix::io::{RawFd, AsRawFd};
use std::io::{Result, Error, ErrorKind};

use nix;
use nix::sys::socket::{recv, MsgFlags};

use mio::event::Evented;
use mio::unix::EventedFd;
use mio::{Poll, Token, Ready, PollOpt};

fn nix_error_to_io_error(err: nix::Error) -> Error {
    match err {
        nix::Error::Sys(err_no) => Error::from(err_no),
        _ => Error::new(ErrorKind::Other, err),
    }
}

pub struct EventedSocket {
    inner: RawFd,
}

impl EventedSocket {
    pub fn recv(&self, buf: &mut [u8], flags: MsgFlags) -> Result<usize> {
        recv(self.inner, buf, flags).map_err(nix_error_to_io_error)
    }
}

impl AsRawFd for EventedSocket {
    fn as_raw_fd(&self) -> i32 {
        self.inner
    }
}

impl From<RawFd> for EventedSocket {
    fn from(fd: RawFd) -> EventedSocket {
        EventedSocket { inner: fd }
    }
}

impl Evented for EventedSocket {
    fn register(&self, poll: &Poll, token: Token, events: Ready, opts: PollOpt) -> Result<()> {
        EventedFd(&self.as_raw_fd()).register(poll, token, events, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, events: Ready, opts: PollOpt) -> Result<()> {
        EventedFd(&self.as_raw_fd()).reregister(poll, token, events, opts)
    }

    fn deregister(&self, poll: &Poll) -> Result<()> {
        EventedFd(&self.as_raw_fd()).deregister(poll)
    }
}
