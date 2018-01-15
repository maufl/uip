use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use rmp_serde::{Deserializer, Serializer};
use rmp_serde::encode::Error as EncodeError;
use rmp_serde::decode::Error as DecodeError;

use peer_information_base::Peer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    PeerInfo(PeerInfo),
    Invalid(Vec<u8>),
}

impl Message {
    pub fn serialize_to_msgpck<W: Write>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.serialize(&mut Serializer::new(writer))
    }

    pub fn deserialize_from_msgpck<R: Read>(reader: &R) -> Result<Message, DecodeError> {
        Deserialize::deserialize(&mut Deserializer::new(reader))
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer: Peer,
}
