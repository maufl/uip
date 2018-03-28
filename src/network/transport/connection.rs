use tokio_io::{AsyncRead, AsyncWrite};
use tokio_core::reactor::Handle;
use tokio_timer::Timer;
use futures::{Future, Sink, Stream};
use futures::sync::mpsc::{channel, SendError, Sender};
use tokio_openssl::SslStream;
use bytes::BytesMut;
use std::time::Duration;

use super::codec::{Codec, Frame};
use data::Peer;
use network::protocol::{Message, PeerInfo};
use Identifier;

#[derive(Clone)]
pub struct Connection {
    handle: Handle,
    sink: Sender<Frame>,
}

impl Connection {
    pub fn from_tls_stream<S, F>(
        stream: SslStream<S>,
        remote_id: Identifier,
        handle: Handle,
        callback: F,
    ) -> Connection
    where
        S: AsyncRead + AsyncWrite + 'static,
        F: Fn(Identifier, u16, u16, BytesMut) + 'static,
    {
        let (sink, stream) = stream.framed(Codec()).split();
        let (sender, receiver) = channel::<Frame>(10);
        let task = receiver
            .forward(sink.sink_map_err(|err| println!("Unexpected sink error: {}", err)))
            .map(|_| ());
        handle.spawn(task);
        let conn = Connection {
            handle: handle.clone(),
            sink: sender,
        };
        let conn2 = conn.clone();
        let task = stream
            .for_each(move |frame| {
                match frame {
                    Frame::Ping => {
                        debug!("Received ping from {}", remote_id);
                        conn2.send_frame(Frame::Pong);
                    }
                    Frame::Pong => debug!("Received pong from {}", remote_id),
                    Frame::Data {
                        src_port,
                        dst_port,
                        data,
                    } => callback(remote_id, src_port, dst_port, data),
                };
                Ok(())
            })
            .map_err(|err| warn!("Error while receiving frame: {}", err));
        handle.spawn(task);
        let conn2 = conn.clone();
        let task = Timer::default()
            .interval(Duration::from_secs(15))
            .map_err(|err| {
                warn!(
                    "Heartbeat timer of connection encountered an error: {}",
                    err
                )
            })
            .for_each(move |_| {
                conn2
                    .sink
                    .clone()
                    .send(Frame::Ping)
                    .map(|_| ())
                    .map_err(|_| debug!("Heartbeat ended because connection went away"))
            });
        handle.spawn(task);
        conn
    }

    pub fn send_frame(&self, f: Frame) {
        let task = self.sink
            .clone()
            .send(f)
            .map(|_| ())
            .map_err(|err| warn!("Error sending frame: {}", err));
        self.handle.spawn(task);
    }

    pub fn send_data_frame(
        &self,
        src_port: u16,
        dst_port: u16,
        data: BytesMut,
    ) -> impl Future<Item = Sender<Frame>, Error = SendError<Frame>> {
        debug!("Sending frame from port {} to port {}", src_port, dst_port);
        self.sink.clone().send(Frame::Data {
            src_port: src_port,
            dst_port: dst_port,
            data: data,
        })
    }

    pub fn send_control_message(&self, msg: Message) {
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
        let task = self.send_data_frame(0, 0, buf)
            .map(|_| {})
            .map_err(|err| warn!("Unable to send peer information: {}", err));
        self.handle.spawn(task);
    }

    pub fn send_peer_info_request(&self, peer_id: &Identifier) {
        let peer_info_request = Message::PeerInfoRequest(*peer_id);
        self.send_control_message(peer_info_request);
    }

    pub fn send_peer_info(&self, peer: Peer) {
        let peer_info = Message::PeerInfo(PeerInfo { peer: peer });
        self.send_control_message(peer_info);
    }
}
