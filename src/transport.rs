use tokio_rustls::{TlsStream};
use std::sync::{Arc,Mutex};
use tokio_core::net::TcpStream;
use tokio_io::AsyncWrite;
use rustls::{ServerSession,ClientSession};
use std::io::{Read,Write,Error,Cursor};
use futures::{Poll};

enum Stream {
    Client(TlsStream<TcpStream, ClientSession>),
    Server(TlsStream<TcpStream, ServerSession>),
}

#[derive(Clone)]
pub struct Transport(Arc<Mutex<Stream>>);

impl Transport {
    pub fn from_tls_client(stream: TlsStream<TcpStream, ClientSession>) -> Transport {
        Transport(Arc::new(Mutex::new(Stream::Client(stream))))
    }

    pub fn from_tls_server(stream: TlsStream<TcpStream, ServerSession>) -> Transport {
        Transport(Arc::new(Mutex::new(Stream::Server(stream))))
    }

    pub fn write_async(&self, data: Vec<u8>) {
        match *self.0.lock().expect("Unable to acquire lock on connection") {
            Stream::Client(ref mut stream) => stream.write_buf(&mut Cursor::new(data)),
            Stream::Server(ref mut stream) => stream.write_buf(&mut Cursor::new(data))
        };
    }
}
