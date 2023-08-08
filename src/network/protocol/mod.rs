use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};
use rmp_serde::encode::Error as EncodeError;
use bytes::{BytesMut, Bytes, BufMut};

use crate::data::{Peer, Identifier};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    PeerInfo(PeerInfo),
    PeerInfoRequest(Identifier),
    Invalid(Vec<u8>),
}

impl Message {
    pub fn serialize_to_msgpck(&self, buffer: BytesMut) -> Result<BytesMut, EncodeError> {
        self.serialize(&mut Serializer::new(&mut buffer.writer()))?;
        Ok(buffer)
    }

    pub fn deserialize_from_msgpck(buffer: &Bytes) -> Message {
        Deserialize::deserialize(&mut Deserializer::new(buffer.as_ref()))
            .unwrap_or_else(|_| Message::Invalid(buffer.to_vec()))
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer: Peer,
}
