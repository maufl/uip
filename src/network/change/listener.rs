use std::io::{Result};
use std::time::Duration;
use std::pin::Pin;
use std::task::{Context,Poll};

use tokio::stream::Stream;
use pin_project_lite::pin_project;

use crate::network::change::rtnetlink_socket::RTNetlinkSocket;
use crate::network::change::debounce;

pub const RTMGRP_IPV4_ROUTE: u32 = 0x40;
pub const RTMGRP_IPV6_ROUTE: u32 = 0x400;

pin_project!{
    pub struct Listener {
        #[pin]
        inner: RTNetlinkSocket,
    }
}

impl Listener {
    pub fn new() -> Result<Listener> {
        RTNetlinkSocket::bind(RTMGRP_IPV6_ROUTE | RTMGRP_IPV4_ROUTE)
            .map(|socket| Listener { inner: socket })
    }

    pub fn debounce(self, duration: Duration) -> impl Stream<Item=()> {
        debounce(self, duration)
    }
}

impl Stream for Listener {
    type Item = ();

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<()>> {
        self.project().inner.poll_next(cx).map(|opt| opt.map(|_| ()))
    }
}
