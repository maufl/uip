use tokio_uds::{UnixDatagramCodec};
use std::io;
use std::str;
use std::path::PathBuf;
use std::os;
use bytes::BytesMut;
use bytes::buf::FromBuf;

pub struct ControlProtocolCodec;

impl UnixDatagramCodec for ControlProtocolCodec {
    type In = (String, String, u16);
    type Out = ();

    fn decode(&mut self, _: &os::unix::net::SocketAddr, buf: &[u8]) -> io::Result<(String, String, u16)> {
        let path = str::from_utf8(buf)
            .map(|s| s.to_owned())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non utf8 string"))?;
        let (channel_id, host_id) = {
            let mut iter = path.split('/');
            (
                iter.next_back()
                    .ok_or(io::Error::new(io::ErrorKind::InvalidData, "not enough path segements"))?
                    .trim()
                    .parse::<u16>().map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("invalid path segement: {}", e)))?
                ,
                iter.next_back().ok_or(io::Error::new(io::ErrorKind::InvalidData, "not enough path segements"))?.to_string()
            )
        };
        Ok((path, host_id, channel_id))
    }

    fn encode(&mut self, _: (), _: &mut Vec<u8>) -> io::Result<PathBuf> {
        Err(io::Error::new(io::ErrorKind::Other, "no data expected"))
    }
}

pub struct Raw;

impl UnixDatagramCodec for Raw {
    type In = BytesMut;
    type Out = BytesMut;

    fn decode(&mut self, _: &os::unix::net::SocketAddr, buf: &[u8]) -> io::Result<BytesMut> {
        Ok(BytesMut::from_buf(buf))
    }

    fn encode(&mut self, msg: BytesMut, buf: &mut Vec<u8>) -> io::Result<PathBuf> {
        buf.copy_from_slice(&msg);
        Ok(PathBuf::new())
    }
}
