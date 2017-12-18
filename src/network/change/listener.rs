use std::io::{Result, Error};

use futures::{Stream, Poll};
use tokio_core::reactor::Handle;

use network::change::rtnetlink_socket::RTNetlinkSocket;

pub const RTMGRP_IPV4_ROUTE: u32 = 0x40;
pub const RTMGRP_IPV6_ROUTE: u32 = 0x400;

pub struct Listener {
    inner: RTNetlinkSocket,
}

impl Listener {
    pub fn new(handle: &Handle) -> Result<Listener> {
        RTNetlinkSocket::bind(RTMGRP_IPV6_ROUTE | RTMGRP_IPV4_ROUTE, handle)
            .map(|socket| Listener { inner: socket })
    }
}

impl Stream for Listener {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<()>, Error> {
        self.inner.poll().map(|a| a.map(|o| o.map(|_| ())))
    }
}
