use crate::Identifier;
use bytes::{Buf, BytesMut};
use rmp_serde::{from_slice, Serializer};
use serde::Serialize;
use std::io::{Error, ErrorKind, Result};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ErrorCode {
    NetworkUnreachable,
    NotBound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Frame {
    Bind(u16),
    Data(Identifier, u16, Vec<u8>),
    Error(ErrorCode),
}

pub struct ControlProtocolCodec;

impl Decoder for ControlProtocolCodec {
    type Item = Frame;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Frame>> {
        if buf.len() < 1 {
            return Ok(None);
        }
        debug!("Decoding UNIX message: {:?}", buf);
        let frame: Frame = from_slice(&buf).map_err(|err| {
            Error::new(
                ErrorKind::Other,
                format!("Error while decoding message: {}", err),
            )
        })?;
        // FIXME: This is an ugly and slow workaround
        let mut tmp = Vec::new();
        frame
            .serialize(&mut Serializer::new(&mut tmp))
            .expect("Error reserializing parsed frame");
        buf.advance(tmp.len());
        Ok(Some(frame))
    }
}

impl Encoder<Frame> for ControlProtocolCodec {
    type Error = Error;

    fn encode(&mut self, msg: Frame, buf: &mut BytesMut) -> Result<()> {
        let mut tmp = Vec::new();
        if let Err(err) = msg.serialize(&mut Serializer::new(&mut tmp)) {
            return Err(Error::new(
                ErrorKind::Other,
                format!("Error while serializing frame: {}", err),
            ));
        }
        buf.extend_from_slice(&tmp);
        Ok(())
    }
}
