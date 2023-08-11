use bytes::Bytes;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, Result};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::Receiver;

#[derive(Debug)]
pub struct Connection {
    incoming: Receiver<Bytes>,
    socket: Arc<UdpSocket>,
    remote_addr: SocketAddr,
}

impl Connection {
    pub fn new(
        socket: Arc<UdpSocket>,
        incoming: Receiver<Bytes>,
        remote_addr: SocketAddr,
    ) -> Connection {
        Connection {
            socket,
            incoming,
            remote_addr,
        }
    }

    pub fn close(&mut self) {
        self.incoming.close();
    }
}

impl AsyncRead for Connection {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<Result<()>> {
        match self.get_mut().incoming.poll_recv(cx) {
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
        let addr = conn.remote_addr;
        return conn.socket.poll_send_to(cx, buf, addr);
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
}
