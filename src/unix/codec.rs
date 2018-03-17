use std::io::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};
use rmp_serde::encode::Error as EncodeError;
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use bytes::buf::BufMut;
use tokio_io::codec::{Decoder, Encoder};
use Identifier;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    Connect(Identifier, u16),
    Data(Vec<u8>),
    //Accept(Identifier, u16),
}

pub struct ControlProtocolCodec;

impl Decoder for ControlProtocolCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Frame>> {
        debug!("Decoding UNIX message: {:?}", buf);
        if buf.len() < 3 {
            return Ok(None);
        }
        let typ = buf.as_ref()[0];
        match typ {
            1u8 => parse_connect(buf),
            2u8 => parse_data(buf),
            _ => Err(Error::new(ErrorKind::Other, "Invalid packet")),
        }
    }
}

impl Encoder for ControlProtocolCodec {
    type Item = Frame;
    type Error = Error;

    fn encode(&mut self, msg: Frame, buf: &mut BytesMut) -> Result<()> {
        match msg {
            Frame::Connect(host_id, channel_id) => encode_connect(&host_id, channel_id, buf),
            Frame::Data(data) => encode_data(&data, buf),
        }
    }
}

fn encode_connect(host_id: &Identifier, channel_id: u16, buf: &mut BytesMut) -> Result<()> {
    buf.reserve(5 + host_id.len());
    buf.put_u8(1);
    buf.put_u16::<BigEndian>(host_id.len() as u16);
    buf.put_slice(host_id.as_bytes());
    buf.put_u16::<BigEndian>(channel_id);
    Ok(())
}

fn encode_data(data: &Vec<u8>, buf: &mut BytesMut) -> Result<()> {
    buf.reserve(3 + data.len());
    buf.put_u8(2);
    buf.put_u16::<BigEndian>(data.len() as u16);
    buf.put_slice(&data);
    Ok(())
}

fn parse_connect(buf: &mut BytesMut) -> Result<Option<Frame>> {
    let len = BigEndian::read_u16(&buf.as_ref()[1..3]) as usize;
    if buf.len() < len + 5 {
        return Ok(None);
    };
    let host_id = Identifier::copy_from_slice(&buf.as_ref()[3..len + 3])
        .map_err(|_| Error::new(ErrorKind::Other, "Invalid host identifier"))?;
    let channel_id = BigEndian::read_u16(&buf.as_ref()[len + 3..len + 5]);
    let _ = buf.split_to(len + 5);
    Ok(Some(Frame::Connect(host_id, channel_id)))
}

fn parse_data(buf: &mut BytesMut) -> Result<Option<Frame>> {
    let len: usize = BigEndian::read_u16(&buf.as_ref()[1..3]) as usize;
    if buf.len() < len + 3 {
        return Ok(None);
    };
    let _ = buf.split_to(3);
    Ok(Some(Frame::Data(buf.split_to(len).as_ref().to_vec())))
}
