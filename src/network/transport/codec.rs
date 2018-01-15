use tokio_io::codec::{Encoder, Decoder};
use std::io::{Error, ErrorKind};
use bytes::{BytesMut, BufMut, BigEndian as BytesBigEndian};
use byteorder::{BigEndian, ByteOrder};

pub enum Frame {
    Ping,
    Pong,
    Data(u16, BytesMut),
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
            None => return Ok(None),
        };
        match typ {
            1 => return Ok(Some(Frame::Ping)),
            2 => return Ok(Some(Frame::Pong)),
            3 => {}
            _ => return Err(Error::new(ErrorKind::InvalidData, "invalid message type")),
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
        Ok(Some(Frame::Data(app_id, src.split_to(length))))
    }
}
