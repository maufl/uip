use tokio::prelude::*;
use tokio::sync::mpsc::{Receiver,Sender};
use std::task::{Poll, Context};
use std::pin::Pin;
use std::net::SocketAddr;
use tokio::io::{Error, ErrorKind, Result};
use bytes::{Bytes,BytesMut};
use pin_project_lite::pin_project;

pin_project!{
    #[derive(Debug)]
    pub struct Connection {
        incoming: Receiver<Bytes>,
        outgoing: Sender<(SocketAddr, Bytes)>,
        remote_addr: SocketAddr,
    }
}

impl Connection {
    pub fn new(
        incoming: Receiver<Bytes>,
        outgoing: Sender<(SocketAddr, Bytes)>,
        remote_addr: SocketAddr,
    ) -> Connection {
        Connection {
            incoming: incoming,
            outgoing: outgoing,
            remote_addr: remote_addr
        }
    }

    pub fn close(&mut self) {
        self.incoming.close();
    }
}

impl AsyncRead for Connection {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8]
    ) -> Poll<Result<usize>>
    {
        match self.project().incoming.poll_recv(cx) {
            Poll::Ready(Some(b)) => {
                buf[..b.len()].copy_from_slice(&b[..]);
                return Poll::Ready(Ok(b.len()));
            },
            Poll::Ready(None) => {
                return Poll::Ready(Ok(0));
            },
            Poll::Pending => {
                return Poll::Pending;
            }
        }
    }
}


impl AsyncWrite for Connection {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &[u8]
    ) -> Poll<Result<usize>> {
        let projected_self = self.project();
        match projected_self.outgoing.poll_ready(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(_)) => return Poll::Ready(Err(Error::new(ErrorKind::BrokenPipe, "Channel to send bytes is closed."))),
            Poll::Ready(Ok(()))=> {}
        };
        let mut bytes = BytesMut::new();
        bytes.extend_from_slice(buf);
        match projected_self.outgoing.try_send((projected_self.remote_addr.clone(), bytes.freeze())) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(_) => Poll::Ready(Err(Error::new(ErrorKind::BrokenPipe, "Channel to send bytes is closed")))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
