mod debounce;
mod rtnetlink_socket;

pub use debounce::debounce;

use std::io::Result;
use std::time::Duration;

use futures_util::StreamExt;
use tokio_stream::Stream;
use tokio_util::io::ReaderStream;

use crate::network::change::rtnetlink_socket::RTNetlinkSocket;

pub const RTMGRP_IPV4_ROUTE: u32 = 0x40;
pub const RTMGRP_IPV6_ROUTE: u32 = 0x400;

pub fn listen(debounce_duration: Duration) -> Result<impl Stream<Item = ()>> {
    let socket = RTNetlinkSocket::bind(RTMGRP_IPV6_ROUTE | RTMGRP_IPV4_ROUTE)?;
    let stream = ReaderStream::new(socket);
    Ok(debounce(stream.map(|_| ()), debounce_duration))
}
