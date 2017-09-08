use tokio_rustls::{TlsStream};
use std::sync::{Arc,Mutex};
use tokio_core::net::TcpStream;
use tokio_io::codec::{Encoder,Decoder};
use tokio_io::{AsyncRead,AsyncWrite};
use rustls::{ServerSession,ClientSession};
use std::io::{Read,Write,Error,ErrorKind,Cursor};
use futures::{Poll,Stream,Sink,StartSend,Future,Async,AsyncSink};
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
pub struct Transport(Arc<Mutex<Sink<SinkItem=Frame,SinkError=Error>>>);

impl Transport {
    pub fn from_tls_client(stream: TlsStream<TcpStream, ClientSession>) -> Transport {
        let (sink, stream) = stream.framed(Codec()).split();
        stream.for_each(|frame| {
            match frame {
                Frame::Ping => println!("Ping"),
                Frame::Pong => println!("Pong"),
                _ => {}
            };
            Ok(())
        });
        Transport(Arc::new(Mutex::new(sink)))
    }

    pub fn from_tls_server(stream: TlsStream<TcpStream, ServerSession>) -> Transport {
        let (sink, stream) = stream.framed(Codec()).split();
        stream.for_each(|frame| {
            match frame {
                Frame::Ping => println!("Ping"),
                Frame::Pong => println!("Pong"),
                _ => {}
            };
            Ok(())
        });
        Transport(Arc::new(Mutex::new(sink)))
    }

    pub fn write_async(&self, data: Vec<u8>) -> FutureWrite {
        FutureWrite {
            data: data,
            transport: self.clone()
        }
    }
}

pub struct FutureWrite {
    data: Vec<u8>,
    transport: Transport,
}

impl Future for FutureWrite {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<(), Error> {
        let mut sink = self.transport.0.lock().expect("Unable to acquire lock on connection");
        let res = sink.start_send(Frame::Data(0, BytesMut::from_buf(&self.data)))?;
        match res {
            AsyncSink::Ready => {
                sink.poll_complete();
                Ok(Async::Ready(()))
            }
            AsyncSink::NotReady(_) => Ok(Async::NotReady)
        }
    }
}
