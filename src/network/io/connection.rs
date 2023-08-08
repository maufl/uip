use bytes::{Bytes, BytesMut};
use futures_util::Sink;
use pin_project_lite::pin_project;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, Error, ErrorKind, ReadBuf, Result};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::sync::PollSender;

pin_project! {
    #[derive(Debug)]
    pub struct Connection {
        incoming: Receiver<Bytes>,
        outgoing: PollSender<(SocketAddr, Bytes)>,
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
            outgoing: PollSender::new(outgoing),
            remote_addr: remote_addr,
        }
    }

    pub fn close(&mut self) {
        self.incoming.close();
    }
}

impl AsyncRead for Connection {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<Result<()>> {
        match self.project().incoming.poll_recv(cx) {
            Poll::Ready(Some(b)) => {
                buf.clear();
                buf.put_slice(&b[..]);
                return Poll::Ready(Ok(()));
            }
            Poll::Ready(None) => {
                return Poll::Ready(Ok(()));
            }
            Poll::Pending => {
                return Poll::Pending;
            }
        }
    }
}

impl AsyncWrite for Connection {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<Result<usize>> {
        let conn = self.get_mut();
        let pinned_sender = Pin::new(&mut conn.outgoing);
        match pinned_sender.poll_ready(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(_)) => {
                return Poll::Ready(Err(Error::new(
                    ErrorKind::BrokenPipe,
                    "Channel to send bytes is closed.",
                )))
            }
            Poll::Ready(Ok(())) => {}
        };
        let mut bytes = BytesMut::new();
        bytes.extend_from_slice(buf);
        let pinned_sender = Pin::new(&mut conn.outgoing);
        match pinned_sender.start_send((conn.remote_addr.clone(), bytes.freeze())) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(_) => Poll::Ready(Err(Error::new(
                ErrorKind::BrokenPipe,
                "Channel to send bytes is closed",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
