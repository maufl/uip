#![feature(conservative_impl_trait)]

extern crate interfaces;
extern crate igd;
extern crate bytes;
extern crate rustls;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;
extern crate tokio_rustls;
extern crate async_readline;

mod transport;
mod peer_information_base;
mod state;
use state::{State};

use rustls::{Certificate};
use rustls::internal::pemfile::{ certs };
use std::net::ToSocketAddrs;
use std::io::{ BufReader };
use tokio_core::reactor::{Core};
use futures::{Stream};

fn load_certs(path: &str) -> Vec<Certificate> {
    certs(&mut BufReader::new(std::fs::File::open(path).unwrap())).unwrap()
}

fn main() {
    let mut core = Core::new().unwrap();

    let addr = "127.0.0.1:4433".to_socket_addrs().unwrap().next().unwrap();
    let cert = load_certs("rsa/ca.cert").pop().unwrap();
    let state = State::new("test".to_string(), core.handle());
    state.add_relay("testserver.com".to_string());
    state.add_relay_peer("testserver.com".to_string(), addr, cert.clone());

    let state2 = state.clone();
    core.handle().spawn(state);

    let stdio = async_readline::RawStdio::new(&core.handle()).unwrap();
    let (stdin, stdout, _) = stdio.split();
    let (commands, rl_writer) = async_readline::init(stdin, stdout);

    let done = commands.map(move |line| {
        state2.send_to("testserver.com".to_string(), line.line.clone());
    }).collect();
    core.run(done).unwrap();
}

