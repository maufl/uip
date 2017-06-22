use tokio_rustls::{TlsStream};
use std::sync::{Arc,Mutex};
use tokio_core::net::TcpStream;
use rustls::{ServerSession,ClientSession};
use tokio_io::{AsyncRead,AsyncWrite};
use std::io::{Read,Write};

trait Stream: Read + AsyncRead + Write + AsyncWrite {}

impl Stream for TlsStream<TcpStream, ClientSession> {}
impl Stream for TlsStream<TcpStream, ServerSession> {}

pub struct Connection {
    stream: Box<Stream>
}

impl Connection {
    pub fn from_tls_client(stream: TlsStream<TcpStream, ClientSession>) -> Connection {
        Connection {
            stream: Box::new(stream)
        }
    }

    pub fn from_tls_server(stream: TlsStream<TcpStream, ServerSession>) -> Connection {
        Connection {
            stream: Box::new(stream)
        }
    }
}

pub type SharedConnection = Arc<Mutex<Connection>>;
