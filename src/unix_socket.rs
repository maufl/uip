use futures::{Stream,Sink,Future};
use futures::future;
use futures::sync::mpsc::{Sender,SendError,channel};
use bytes::{BytesMut};
use tokio_uds::{UnixStream};
use tokio_io::codec::Framed;

use state::State;
use unix_codec::{Frame,ControlProtocolCodec};


#[derive(Clone)]
pub struct UnixSocket {
    state: State,
    sink: Sender<Frame>,
}

impl UnixSocket {
    pub fn from_unix_socket(state: State, socket: Framed<UnixStream, ControlProtocolCodec>, host_id: String, channel_id: u16) -> UnixSocket {
        let (sink, stream) = socket.split();
        let (sender, receiver) = channel::<Frame>(10);
        state.spawn(receiver.forward(sink.sink_map_err(|err| warn!("Sink error: {}", err))).map(|_| ()).map_err(|err| warn!("Forwarding error: {:?}", err)));
        let state2 = state.clone();
        let done = stream.for_each(move |frame| {
            match frame {
                Frame::Data(buf) => state2.send_frame(host_id.clone(), channel_id, buf),
                Frame::Connect(_,_) => { warn!("Unexpected UNIX message CONNECT") }
            };
            future::ok(())
        }).map_err(|err| warn!("Unix stream error: {}", err));
        state.spawn(done);
        let unix_socket = UnixSocket {
            state: state.clone(),
            sink: sender,
        };
        return unix_socket;
    }

    pub fn send_frame(&self, data: BytesMut) -> impl Future<Item=Sender<Frame>,Error=SendError<Frame>>{
        println!("Sending frame");
        self.sink.clone()
            .send(Frame::Data(data))
    }

}
