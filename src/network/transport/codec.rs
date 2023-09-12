use byteorder::{BigEndian, ByteOrder};
use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io::{Error, ErrorKind};
use tokio_util::codec::{Decoder, Encoder};

pub enum Frame {
    Ping,
    Pong,
    Data {
        src_port: u16,
        dst_port: u16,
        data: Bytes,
    },
}

fn encode(item: Frame, dst: &mut BytesMut) -> Result<(), Error> {
    match item {
        Frame::Ping => dst.put_u8(1),
        Frame::Pong => dst.put_u8(2),
        Frame::Data {
            src_port,
            dst_port,
            data,
        } => {
            dst.put_u8(3);
            dst.put_u16(src_port);
            dst.put_u16(dst_port);
            dst.put_u16(data.len() as u16);
            dst.put(data);
        }
    };
    Ok(())
}

fn decode(src: &mut BytesMut) -> Result<Option<Frame>, Error> {
    let typ = match src.first() {
        Some(byte) => *byte,
        None => return Ok(None),
    };
    match typ {
        1 => {
            src.advance(1);
            return Ok(Some(Frame::Ping));
        }
        2 => {
            src.advance(1);
            return Ok(Some(Frame::Pong));
        }
        3 => {}
        _ => return Err(Error::new(ErrorKind::InvalidData, "invalid message type")),
    };
    if src.len() < 7 {
        return Ok(None);
    };
    let src_port = BigEndian::read_u16(&src[1..3]);
    let dst_port = BigEndian::read_u16(&src[3..5]);
    let length = BigEndian::read_u16(&src[5..7]) as usize;
    if src.len() < 7 + length {
        return Ok(None);
    }
    src.advance(7);
    Ok(Some(Frame::Data {
        src_port: src_port,
        dst_port: dst_port,
        data: src.split_to(length).freeze(),
    }))
}

pub struct Codec();

impl Encoder<Frame> for Codec {
    type Error = Error;

    fn encode(&mut self, item: Frame, dst: &mut BytesMut) -> Result<(), Error> {
        encode(item, dst)
    }
}

impl Decoder for Codec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Frame>, Error> {
        decode(src)
    }
}
