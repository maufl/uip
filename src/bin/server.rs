extern crate rustls;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate webpki_roots;
extern crate tokio_rustls;

use std::sync::Arc;
use std::net::ToSocketAddrs;
use std::io::BufReader;
use std::fs::File;
use futures::{ Future, Stream };
use rustls::{ Certificate, PrivateKey, ServerConfig };
use rustls::internal::pemfile::{ certs, pkcs8_private_keys };
use tokio_io::{ io, AsyncRead };
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use tokio_rustls::ServerConfigExt;


fn load_certs(path: &str) -> Vec<Certificate> {
    certs(&mut BufReader::new(File::open(path).unwrap())).unwrap()
}

fn load_keys(path: &str) -> Vec<PrivateKey> {
    pkcs8_private_keys(&mut BufReader::new(File::open(path).unwrap())).unwrap()
}


fn main() {

    let addr = "0.0.0.0:4433".to_socket_addrs().unwrap().next().unwrap();
    let cert_file = "rsa/end.fullchain";
    let key_file = "rsa/end.key";

    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let mut config = ServerConfig::new();
    let mut keys = load_keys(key_file);
    config.set_single_cert(load_certs(cert_file), keys.remove(0));
    let arc_config = Arc::new(config);

    let socket = TcpListener::bind(&addr, &handle).unwrap();
    println!("Opened socket on {}", addr);
    let done = socket.incoming()
        .for_each(|(stream, addr)| {
            println!("Received new connection");
            let done = arc_config.accept_async(stream)
                .and_then(|stream| {
                    println!("Completed TLS handshake");
                    let (reader, writer) = stream.split();
                    io::copy(reader, writer)
                })
                .map(move |(n, ..)| println!("Echo: {} - {}", n, addr))
                .map_err(move |err| println!("Error: {:?} - {}", err, addr));
            handle.spawn(done);

            Ok(())
        });

    core.run(done).unwrap();
}

