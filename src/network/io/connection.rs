use async_trait::async_trait;
use bytes::{Buf, Bytes};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, Result};
use tokio::net::UdpSocket;
use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use webrtc_util::Conn;

#[derive(Debug)]
pub struct Connection {
    incoming: RwLock<Receiver<Bytes>>,
    socket: Arc<UdpSocket>,
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
}

impl Connection {
    pub fn new(
        socket: Arc<UdpSocket>,
        incoming: Receiver<Bytes>,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> Connection {
        Connection {
            socket,
            incoming: RwLock::new(incoming),
            local_addr,
            remote_addr,
        }
    }
}

impl AsyncRead for Connection {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context, buf: &mut ReadBuf) -> Poll<Result<()>> {
        let mut guard = match self.get_mut().incoming.try_write() {
            Ok(g) => g,
            Err(_) => return Poll::Pending,
        };
        match guard.poll_recv(cx) {
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

#[async_trait]
impl Conn for Connection {
    async fn connect(&self, addr: SocketAddr) -> webrtc_util::Result<()> {
        if self.remote_addr == addr {
            Ok(())
        } else {
            Err(webrtc_util::Error::ErrCantAssignRequestedAddr)
        }
    }

    async fn recv(&self, buf: &mut [u8]) -> webrtc_util::Result<usize> {
        let Some(mut bytes) = self.incoming.write().await.recv().await else {
            return Err(webrtc_util::Error::ErrClosedListener)
        };
        bytes.copy_to_slice(buf);
        Ok(bytes.len())
    }

    async fn recv_from(&self, buf: &mut [u8]) -> webrtc_util::Result<(usize, SocketAddr)> {
        let Some(mut bytes) = self.incoming.write().await.recv().await else {
            return Err(webrtc_util::Error::ErrClosedListener)
        };
        bytes.copy_to_slice(buf);
        Ok((bytes.len(), self.remote_addr.clone()))
    }

    async fn send(&self, buf: &[u8]) -> webrtc_util::Result<usize> {
        Ok(self.socket.send_to(buf, self.remote_addr).await?)
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> webrtc_util::Result<usize> {
        if target != self.remote_addr {
            return Err(webrtc_util::Error::ErrCantAssignRequestedAddr);
        }
        Ok(self.socket.send_to(buf, self.remote_addr).await?)
    }

    async fn close(&self) -> webrtc_util::Result<()> {
        Ok(())
    }

    fn local_addr(&self) -> webrtc_util::Result<SocketAddr> {
        Ok(self.local_addr)
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote_addr)
    }
}
