use tokio_rustls::{TlsStream};
use std::sync::{Arc,Mutex};
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

enum GenericTlsStream {
    Client(TlsStream<TcpStream, ClientSession>),
    Server(TlsStream<TcpStream, ServerSession>),
}

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
    sink: Sender<Frame>,

}

impl Transport {
    pub fn from_tls_stream<S: Session>(stream: TlsStream<TcpStream, S>) -> Transport {
        let (sink, stream) = stream.framed(Codec()).split();
        stream.for_each(|frame| {
            match frame {
                Frame::Ping => println!("Ping"),
                Frame::Pong => println!("Pong"),
                _ => {}
            };
            Ok(())
        });
        let (sender, receiver) = channel::<Frame>(10);
        receiver.forward(sink.sink_map_err(|_|()));
        Transport {
            sink: sender
        }
    }

    pub fn open_channel(&self, id: u16) -> impl Sink<SinkItem=BytesMut,SinkError=()> {
        self.sink
            .clone()
            .sink_map_err(|_| ())
            .with(move |buf| future::ok(Frame::Data(id, buf)))
    }

}
