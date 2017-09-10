use tokio_rustls::{TlsStream};
use std::sync::{Arc,Mutex,RwLock};
use tokio_core::net::TcpStream;
use tokio_io::codec::{Encoder,Decoder};
use tokio_io::{AsyncRead,AsyncWrite};
use rustls::{ServerSession,ClientSession,Session};
use std::io::{Read,Write,Error,ErrorKind,Cursor};
use futures::{Poll,Stream,Sink,StartSend,Future,Async,AsyncSink,future};
use futures::sync::mpsc::{Sender,SendError,Receiver,channel};
use bytes::{BytesMut, BufMut, BigEndian as BytesBigEndian};
use bytes::buf::{FromBuf};
use byteorder::{BigEndian,ByteOrder};
use std::collections::{HashMap};
use state::State;

pub enum Frame {
    Ping,
    Pong,
    Data(u16, BytesMut)
}

pub struct Codec();

impl Encoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Error> {
        match item {
            Frame::Ping => dst.put_u8(1),
            Frame::Pong => dst.put_u8(2),
            Frame::Data(app_id, data) => {
                dst.put_u8(3);
                dst.put_u16::<BytesBigEndian>(app_id);
                dst.put_u16::<BytesBigEndian>(data.len() as u16);
                dst.put(data);
            }
        };
        Ok(())
    }
}

impl Decoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, Error> {
        let typ = match src.first() {
            Some(byte) => *byte,
            None => return Ok(None)
        };
        match typ {
            1 => return Ok(Some(Frame::Ping)),
            2 => return Ok(Some(Frame::Pong)),
            3 => {},
            _ => return Err(Error::new(ErrorKind::InvalidData, "invalid message type"))
        };
        if src.len() < 5 {
            return Ok(None);
        };
        let app_id = BigEndian::read_u16(&src[1..3]);
        let length = BigEndian::read_u16(&src[3..5]) as usize;
        if src.len() < 5 + length {
            return Ok(None);
        }
        src.split_to(5);
        Ok(Some(Frame::Data(app_id, src.split_off(length))))
    }
}

#[derive(Clone)]
pub struct Transport {
    state: State,
    sink: Sender<Frame>,
}

impl Transport {
    pub fn from_tls_stream<S: Session>(state: State, stream: TlsStream<TcpStream, S>, remote_id: String) -> Transport {
        let (sink, stream) = stream.framed(Codec()).split();
        let (sender, receiver) = channel::<Frame>(10);
        receiver.forward(sink.sink_map_err(|_|()));
        let transport = Transport {
            state: state,
            sink: sender,
        };
        let transport2 = transport.clone();
        stream.for_each(move |frame| {
            match frame {
                Frame::Ping => println!("Ping"),
                Frame::Pong => println!("Pong"),
                Frame::Data(channel_id, data) => {
                    transport2.state.deliver_frame(remote_id.clone(), channel_id, data)
                }
            };
            Ok(())
        });
        return transport;
    }

    pub fn send_frame(&self, channel_id: u16, data: BytesMut) -> impl Future<Item=Sender<Frame>,Error=SendError<Frame>>{
        self.sink.clone()
            .send(Frame::Data(channel_id, data))
    }

}
