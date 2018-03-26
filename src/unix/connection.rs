use futures::{Future, Sink, Stream};
use futures::future;
use futures::sync::mpsc::{channel, SendError, Sender};
use bytes::BytesMut;
use tokio_uds::UnixStream;
use tokio_io::codec::Framed;

use unix::{ControlProtocolCodec, Frame, UnixState};
use {Identifier, Shared};

#[derive(Clone)]
pub struct Connection {
    state: Shared<UnixState>,
    sink: Sender<Frame>,
}

impl Connection {
    pub fn from_unix_socket(
        state: Shared<UnixState>,
        socket: Framed<UnixStream, ControlProtocolCodec>,
        host_id: Identifier,
        src_port: u16,
        dst_port: u16,
    ) -> Connection {
        let (sink, stream) = socket.split();
        let (sender, receiver) = channel::<Frame>(10);
        state.spawn(
            receiver
                .forward(sink.sink_map_err(|err| warn!("Sink error: {}", err)))
                .map(|_| ())
                .map_err(|err| warn!("Forwarding error: {:?}", err)),
        );
        let state2 = state.clone();
        let done = stream
            .for_each(move |frame| {
                match frame {
                    Frame::Data(buf) => {
                        state2.send_frame(host_id, src_port, dst_port, BytesMut::from(buf))
                    }
                    _ => warn!("Unexpected Unix message {:?}", frame),
                };
                future::ok(())
            })
            .map_err(|err| warn!("Unix stream error: {}", err));
        state.spawn(done);
        Connection {
            state: state,
            sink: sender,
        }
    }

    pub fn send_frame(
        &self,
        data: BytesMut,
    ) -> impl Future<Item = Sender<Frame>, Error = SendError<Frame>> {
        println!("Sending frame");
        self.sink.clone().send(Frame::Data(data.to_vec()))
    }
}
