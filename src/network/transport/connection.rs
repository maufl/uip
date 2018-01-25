use tokio_io::{AsyncRead, AsyncWrite};
use tokio_core::reactor::Handle;
use futures::{Stream, Sink, Future};
use futures::sync::mpsc::{Sender, SendError, channel};
use tokio_openssl::SslStream;
use bytes::BytesMut;

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
        F: Fn(Identifier, u16, BytesMut) + 'static,
    {
        let (sink, stream) = stream.framed(Codec()).split();
        let (sender, receiver) = channel::<Frame>(10);
        let task = receiver
            .forward(sink.sink_map_err(
                |err| println!("Unexpected sink error: {}", err),
            ))
            .map(|_| ());
        handle.spawn(task);
        let task = stream
            .for_each(move |frame| {
                match frame {
                    Frame::Ping => println!("Ping"),
                    Frame::Pong => println!("Pong"),
                    Frame::Data(channel_id, data) => callback(remote_id, channel_id, data),
                };
                Ok(())
            })
            .map_err(|err| println!("Error while receiving frame: {}", err));
        handle.spawn(task);
        Connection {
            handle: handle,
            sink: sender,
        }
    }

    pub fn send_frame(&self, f: Frame) {
        let task = self.sink.clone().send(f).map(|_| ()).map_err(|err| {
            warn!("Error sending frame: {}", err)
        });
        self.handle.spawn(task);
    }

    pub fn send_data_frame(
        &self,
        channel_id: u16,
        data: BytesMut,
    ) -> impl Future<Item = Sender<Frame>, Error = SendError<Frame>> {
        println!("Sending frame to {}", channel_id);
        self.sink.clone().send(Frame::Data(channel_id, data))
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
        let task = self.send_data_frame(0, buf).map(|_| {}).map_err(|err| {
            warn!("Unable to send peer information: {}", err)
        });
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
