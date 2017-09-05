use tokio_rustls::{TlsStream};
use std::sync::{Arc,Mutex};
use tokio_core::net::TcpStream;
use tokio_io::AsyncWrite;
use rustls::{ServerSession,ClientSession};
use std::io::{Read,Write,Error,Cursor};
use futures::{Poll};

pub enum Stream {
    Client(TlsStream<TcpStream, ClientSession>),
    Server(TlsStream<TcpStream, ServerSession>),
}

pub struct Connection {
    pub stream: Stream
}


impl Connection {
    pub fn from_tls_client(stream: TlsStream<TcpStream, ClientSession>) -> Connection {
        Connection {
            stream: Stream::Client(stream)
        }
    }

    pub fn from_tls_server(stream: TlsStream<TcpStream, ServerSession>) -> Connection {
        Connection {
            stream: Stream::Server(stream)
        }
    }
}

#[derive(Clone)]
pub struct SharedConnection(Arc<Mutex<Connection>>);

impl SharedConnection {
    pub fn from_tls_client(stream: TlsStream<TcpStream, ClientSession>) -> SharedConnection {
        SharedConnection(Arc::new(Mutex::new(Connection::from_tls_client(stream))))
    }

    pub fn from_tls_server(stream: TlsStream<TcpStream, ServerSession>) -> SharedConnection {
        SharedConnection(Arc::new(Mutex::new(Connection::from_tls_server(stream))))
    }

    pub fn write_async(&self, data: Vec<u8>) {
        match self.0.lock().expect("Unable to acquire lock on connection").stream {
            Stream::Client(ref mut stream) => stream.write_buf(&mut Cursor::new(data)),
            Stream::Server(ref mut stream) => stream.write_buf(&mut Cursor::new(data))
        };
    }
}
