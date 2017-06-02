#![feature(ip)]
#![feature(conservative_impl_trait)]

extern crate interfaces;
extern crate igd;
extern crate rustls;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate tokio_rustls;
extern crate tokio_file_unix;
extern crate webpki_roots;

use std::collections::HashMap;
use std::sync::{Arc,RwLock};
use std::net::{SocketAddr,SocketAddrV4,Ipv4Addr};
use std::str::FromStr;

mod connection;
mod state;
use state::{State,Peer};

use futures::Future;
use tokio_core::net::TcpStream;
use tokio_core::reactor::{Core,Handle};
use tokio_io::io;
use tokio_io::{AsyncRead,AsyncWrite};
use rustls::{Session,ClientConfig,Certificate,ProtocolVersion,ClientSession};
use rustls::internal::pemfile::{ certs };
use tokio_rustls::{ClientConfigExt,TlsStream};
use std::net::ToSocketAddrs;
use std::io::{ BufReader, stdout, stdin, Error };
use tokio_file_unix::{ StdFile, File };

fn load_certs(path: &str) -> Vec<Certificate> {
    certs(&mut BufReader::new(std::fs::File::open(path).unwrap())).unwrap()
}

fn main() {
    let addr = "127.0.0.1:4433".to_socket_addrs().unwrap().next().unwrap();
    let cert = load_certs("rsa/ca.cert").pop().unwrap();
    let state = State::new("test".to_string());
    state.relays.write().expect("Unable to acquire lock").push("testserver.com".to_string());
    state.pib
        .write().expect("Unable to acquire lock")
        .add_peer("testserver.com".to_string(), Peer::new("testserver.com".to_string(), vec![addr], vec![], cert.clone()));
    let mut core = Core::new().unwrap();
    let handle = core.handle();
    let stdout = stdout();
    let stdout = File::new_nb(StdFile(stdout.lock())).unwrap()
        .into_io(&handle).unwrap();
    let request = "GET / HTTP/1.0\r\nHost: google.de\r\n\r\n";
    let resp = connect(addr, cert, "testserver.com".to_string(), &handle)
        .and_then(|stream| io::write_all(stream, request.as_bytes()))
        .and_then(|(stream, _)| {
            let (r, w) = stream.split();
            io::copy(r, stdout)
        });
    core.run(resp).unwrap();
}


fn connect(addr: SocketAddr, cert: Certificate, id: String, handle: &Handle) -> impl Future<Item=TlsStream<TcpStream,ClientSession>, Error=Error> {
    let config = {
        let mut config = ClientConfig::new();
        config.versions = vec![ProtocolVersion::TLSv1_2];
        config.root_store.add(&cert);
        Arc::new(config)
    };
    let socket = TcpStream::connect(&addr, handle);
    socket
        .and_then(move |stream| config.connect_async(id.as_ref(), stream) )
}
