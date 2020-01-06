use tokio;
use tokio::prelude::*;
use tokio::sync::mpsc::{channel, Sender, Receiver};
use tokio::sync::mpsc::error::SendError;
use tokio::io::split;
use tokio_util::codec::{FramedRead,FramedWrite};
use tokio_openssl::SslStream;
use futures_util::StreamExt;
use bytes::{BytesMut, Bytes};
use std::time::{Duration};

use super::codec::{Codec, Frame};
use crate::data::Peer;
use crate::network::protocol::{Message, PeerInfo};
use crate::Identifier;

#[derive(Clone)]
pub struct Connection {
    sink: Sender<Frame>,
}

impl Connection {
    pub fn from_tls_stream<S>(
        stream: SslStream<S>,
        remote_id: Identifier,
        data_sink: Sender<(Identifier, u16, u16, Bytes)>,
    ) -> Connection
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let (read_half, write_half) = split(stream);
        let read_half = FramedRead::new(read_half, Codec());
        let write_half = FramedWrite::new(write_half, Codec());
        let (sender, receiver) = channel::<Frame>(10);
        let conn = Connection { sink: sender };
        conn.spawn_read_task(remote_id, read_half, data_sink);
        conn.spawn_write_task(receiver, write_half);
        conn.spawn_ping_task();
        conn
    }

    fn spawn_read_task<S>(&self, remote_id: Identifier, mut read_half: FramedRead<tokio::io::ReadHalf<SslStream<S>>, Codec>, mut data_sink: Sender<(Identifier, u16, u16, Bytes)>) 
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static {
        let mut connection = self.clone();
        tokio::spawn(async move {
            loop {
                let frame = match read_half.next().await {
                    Some(Ok(f)) => f,
                    Some(Err(e)) => return warn!("Error while receiving frame: {}", e),
                    None => return
                };
                match frame {
                    Frame::Ping => {
                        debug!("Received ping from {}", remote_id);
                        connection.send_frame(Frame::Pong).await;
                    }
                    Frame::Pong => debug!("Received pong from {}", remote_id),
                    Frame::Data {
                        src_port,
                        dst_port,
                        data,
                    } => if data_sink.send((remote_id, src_port, dst_port, data)).await.is_err() {
                        return info!("Upstream closed the data sink");
                    }
                }
            }
        });
    }

    fn spawn_write_task<S>(&self, receiver: Receiver<Frame>, write_half: FramedWrite<tokio::io::WriteHalf<SslStream<S>>, Codec>)
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
     {
        tokio::spawn(async move {
            if let Err(err) = receiver.map(|f| Ok(f)).forward(write_half).await {
                warn!("Error forwarding frames: {}", err);
            }
        });
    }
    
    fn spawn_ping_task(&self) {
        let mut connection = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(15));
            loop {
                if interval.next().await.is_none() {
                    return info!("Stop ping task");
                };
                connection.send_frame(Frame::Ping).await;
            }
        });
    }

    pub async fn send_frame(&mut self, f: Frame) {
        if self.sink.send(f).await.is_err() {
            warn!("Error sending frame");
        };
    }

    pub async fn send_data_frame(
        &mut self,
        src_port: u16,
        dst_port: u16,
        data: Bytes,
    ) -> Result<(), SendError<Frame>> {
        debug!("Sending frame from port {} to port {}", src_port, dst_port);
        self.sink.send(Frame::Data {
            src_port: src_port,
            dst_port: dst_port,
            data: data,
        }).await
    }

    pub async fn send_control_message(&mut self, msg: Message) {
        let buf = BytesMut::with_capacity(1500);
        let buf = match msg.serialize_to_msgpck(buf) {
            Err(err) => {
                return warn!(
                    "Failed to send peer information because serialization failed: {}",
                    err
                )
            }
            Ok(buf) => buf,
        };
        if self.send_data_frame(0, 0, buf.freeze()).await.is_err() {
            warn!("Unable to send data frame, pipe broken?");
        }
    }

    pub async fn send_peer_info_request(&mut self, peer_id: &Identifier) {
        let peer_info_request = Message::PeerInfoRequest(*peer_id);
        self.send_control_message(peer_info_request).await;
    }

    pub async fn send_peer_info(&mut self, peer: Peer) {
        let peer_info = Message::PeerInfo(PeerInfo { peer: peer });
        self.send_control_message(peer_info).await;
    }
}
